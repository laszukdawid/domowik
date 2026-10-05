package release

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime/debug"
	"sort"
	"strings"
	"time"
)

// Generator runs the inventory pipeline against one repository checkout.
type Generator struct {
	// Root is the repository root; every input and output is resolved here,
	// never against the caller's working directory.
	Root string
}

// BuildOptions controls one generation pass.
type BuildOptions struct {
	// AllowDirty permits generation from a tree with uncommitted changes.
	// Verification never sets this: committed artifacts must correspond to a
	// clean checkout so the containing commit binds the VCS revision.
	AllowDirty bool

	getenv   Getenv
	headTime HeadTime
}

// Inventory is the complete in-memory generation result before writing.
type Inventory struct {
	Epoch       time.Time
	EpochSource EpochSource
	Components  []Component
	// Embeds maps artifact id to the embedded files discovered for its target.
	Embeds map[string][]EmbeddedFile
	// Binaries maps artifact id to the exact cross-built bytes whose hash is
	// recorded everywhere it is referenced.
	Binaries map[string][]byte
	// Outputs maps committed-path (slash) -> exact bytes for text artifacts
	// (SBOMs and NOTICE).
	Outputs map[string][]byte
	// Evidence maps local-evidence-path (slash) -> exact bytes (the
	// consumption manifest and hashed binaries, typically gitignored).
	Evidence map[string][]byte
}

// Third-party policy input paths. Provenance and license inputs are reviewed
// documents under docs/third-party/, deliberately separated from generated
// release outputs under release/ and build/.
const (
	manifestPath    = "release/manifest.json"
	registryPath    = "docs/third-party/licenses.json"
	toolchainPath   = "docs/third-party/toolchain-go.json"
	toolsRecordPath = "docs/third-party/tools.json"
)

// LoadInputs loads and validates every declarative input.
func (g *Generator) LoadInputs() (*Manifest, *Registry, *ToolchainRecord, error) {
	// Production boundary: every fixed policy input is read through the same
	// root-contained reader used everywhere else. A committed symlinked
	// parent directory (e.g. docs -> /outside) resolves outside the
	// repository root and fails closed here; final-entry symlinks are also
	// rejected.
	readPolicy := func(rel string) ([]byte, error) {
		data, err := readFileUnderRoot(g.Root, rel)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", rel, err)
		}
		return data, nil
	}
	manifestData, err := readPolicy(manifestPath)
	if err != nil {
		return nil, nil, nil, err
	}
	registryData, err := readPolicy(registryPath)
	if err != nil {
		return nil, nil, nil, err
	}
	toolchainData, err := readPolicy(toolchainPath)
	if err != nil {
		return nil, nil, nil, err
	}
	toolsData, err := readPolicy(toolsRecordPath)
	if err != nil {
		return nil, nil, nil, err
	}
	manifest, err := parseManifest(manifestData)
	if err != nil {
		return nil, nil, nil, err
	}
	registry, err := parseRegistry(registryData)
	if err != nil {
		return nil, nil, nil, err
	}
	if err := registry.VerifyDecisionRefs(g.Root); err != nil {
		return nil, nil, nil, err
	}
	toolchain, err := parseToolchain(toolchainData)
	if err != nil {
		return nil, nil, nil, err
	}
	tools, err := parseToolsRecord(toolsData)
	if err != nil {
		return nil, nil, nil, err
	}
	generatorRows := 0
	for _, t := range tools.Tools {
		switch {
		case t.Name == generatorName:
			generatorRows++
			if t.Version != GeneratorVersion {
				return nil, nil, nil, fmt.Errorf("tools record pins %s %q but the running generator is %s; bump the record together with GeneratorVersion", generatorName, t.Version, GeneratorVersion)
			}
			if t.SPDXLicense() != "Apache-2.0" || t.Status != ToolStatusDevOnly {
				return nil, nil, nil, fmt.Errorf("generator row must pin the first-party identity (Apache-2.0, dev-only), got license/status %q/%q", t.SPDXLicense(), t.Status)
			}
		case t.Status == ToolStatusDistributed:
			return nil, nil, nil, fmt.Errorf("tool %q is marked distributed; it requires full third-party review before any release proceeds", t.Name)
		}
	}
	if generatorRows != 1 {
		return nil, nil, nil, fmt.Errorf("tools record must contain exactly one %s row, found %d", generatorName, generatorRows)
	}
	return manifest, registry, toolchain, nil
}

// Build performs dynamic discovery, fails closed on any audit finding or
// assertion failure, and renders every declared output byte-exactly without
// touching the repository filesystem at all.
func (g *Generator) Build(ctx context.Context, opts BuildOptions) (*Inventory, error) {
	manifest, registry, toolchain, err := g.LoadInputs()
	if err != nil {
		return nil, err
	}
	if opts.getenv == nil {
		opts.getenv = EnvGetenv
	}
	if opts.headTime == nil {
		opts.headTime = func() (int64, error) { return gitHeadTime(g.Root) }
	}
	epoch, epochSource, err := ResolveEpoch(opts.getenv, opts.headTime)
	if err != nil {
		return nil, err
	}
	if err := registry.VerifyNoticeFiles(g.Root); err != nil {
		return nil, err
	}
	if err := requireCleanTree(ctx, g.Root, opts.AllowDirty, generatedOutputExclusions(manifest)); err != nil {
		return nil, err
	}

	if err := CrossCheckToolchain(registry, toolchain); err != nil {
		return nil, fmt.Errorf("toolchain/registry cross-check failed: %w", err)
	}

	modules, err := ListModules(ctx, g.Root)
	if err != nil {
		return nil, err
	}
	lines, err := ListModuleLines(ctx, g.Root)
	if err != nil {
		return nil, err
	}
	if err := CrossCheckModules(modules, lines); err != nil {
		return nil, err
	}

	embedsByArtifact, err := g.discoverAllEmbeds(ctx, toolchain, manifest)
	if err != nil {
		return nil, err
	}
	comps := modulesToComponents(modules)
	comps = append(comps, Component{Key: ToolchainKey, Kind: KindToolchain, Version: toolchain.Version})
	embedComps, err := unionEmbedComponents(embedsByArtifact, g.Root)
	if err != nil {
		return nil, err
	}
	comps = append(comps, embedComps...)
	sortComponents(comps)

	if findings := registry.Audit(comps); len(findings) > 0 {
		return nil, auditError("license audit failed closed", findings)
	}

	mainMod, err := selectMainModule(modules)
	if err != nil {
		return nil, err
	}
	mainEntry, ok := registry.Entry(mainMod.EffectivePath())
	if !ok {
		return nil, fmt.Errorf("main module %q missing from registry after audit", mainMod.EffectivePath())
	}

	inv := &Inventory{
		Epoch:       epoch,
		EpochSource: epochSource,
		Components:  comps,
		Embeds:      map[string][]EmbeddedFile{},
		Binaries:    map[string][]byte{},
		Outputs:     map[string][]byte{},
		Evidence:    map[string][]byte{},
	}

	sourceDoc, err := g.renderSourceSBOM(registry, modules, mainMod, epoch)
	if err != nil {
		return nil, err
	}
	inv.Outputs[manifest.SBOMDir+"/source.spdx.json"] = sourceDoc

	binHashes := map[string]string{}
	for _, art := range manifest.Artifacts {
		doc, binBytes, _, err := g.renderBinaryArtifact(ctx, manifest, registry, toolchain, modules, art, embedsByArtifact[art.ID], mainMod, mainEntry, epoch)
		if err != nil {
			return nil, fmt.Errorf("artifact %s: %w", art.ID, err)
		}
		sum := sha256Sum(binBytes)
		binHashes[art.ID] = sum
		inv.Embeds[art.ID] = embedsByArtifact[art.ID]
		inv.Binaries[art.ID] = binBytes
		inv.Outputs[manifest.SBOMDir+"/"+art.ID+".spdx.json"] = doc
		inv.Evidence[manifest.ReleaseOutputDir+"/"+art.ID] = binBytes
	}

	notice, err := RenderNotice("Domowik", registry, readFileUnderRootFn(g.Root))
	if err != nil {
		return nil, err
	}
	inv.Outputs[manifest.Notice.Path] = notice

	consumption, err := renderConsumptionManifest(manifest, toolchain, inv, binHashes)
	if err != nil {
		return nil, err
	}
	inv.Evidence[manifest.ConsumptionManifest.Path] = consumption

	// Fail generation on stale extras in EVERY mode: AllowDirty relaxes only
	// Git cleanliness, never the distribution-set contract. This keeps
	// Plan's documented success-implies-generate guarantee intact.
	if err := g.checkEvidenceInventory(inv); err != nil {
		return nil, fmt.Errorf("release evidence inventory: %w", err)
	}

	return inv, nil
}

func auditError(prefix string, findings []Finding) error {
	var b strings.Builder
	fmt.Fprintf(&b, "%s with %d finding(s):", prefix, len(findings))
	for _, f := range findings {
		b.WriteString("\n  - ")
		b.WriteString(f.Error())
	}
	return fmt.Errorf("%s", b.String())
}

func modulesToComponents(modules []Module) []Component {
	var comps []Component
	for _, m := range modules {
		comps = append(comps, Component{
			Key:     m.EffectivePath(),
			Kind:    KindModule,
			Version: m.EffectiveVersion(),
		})
	}
	return comps
}

// generatedOutputExclusions lists the pipeline's EXACT generated output
// paths. Whole trees are deliberately not excluded: a misplaced output
// directory must never hide unrelated tracked sources from the source SBOM
// (tree placement is instead rejected by treePurityOffenders).
func generatedOutputExclusions(manifest *Manifest) map[string]bool {
	excl := map[string]bool{}
	excl[manifest.SBOMDir+"/source.spdx.json"] = true
	for _, a := range manifest.Artifacts {
		excl[manifest.SBOMDir+"/"+a.ID+".spdx.json"] = true
		excl[manifest.ReleaseOutputDir+"/"+a.ID] = true
	}
	excl[manifest.Notice.Path] = true
	excl[manifest.ConsumptionManifest.Path] = true
	return excl
}

// treePurityOffenders reports tracked files that sit inside the disposable
// sbomDir or releaseOutputDir trees without being declared generated
// outputs themselves.
func treePurityOffenders(manifest *Manifest, tracked []string) []string {
	dirs := map[string]string{
		strings.TrimSuffix(manifest.SBOMDir, "/"):          "sbomDir",
		strings.TrimSuffix(manifest.ReleaseOutputDir, "/"): "releaseOutputDir",
	}
	outputs := generatedOutputExclusions(manifest)
	var out []string
	for _, f := range tracked {
		if outputs[f] {
			continue
		}
		for dir, kind := range dirs {
			if f == dir || strings.HasPrefix(f, dir+"/") {
				out = append(out, fmt.Sprintf("%s (%s)", f, kind))
			}
		}
	}
	sort.Strings(out)
	return out
}

func (g *Generator) renderSourceSBOM(registry *Registry, modules []Module, mainMod Module, epoch time.Time) ([]byte, error) {
	manifest, _, _, err := g.LoadInputs()
	if err != nil {
		return nil, err
	}
	excl := generatedOutputExclusions(manifest)
	tracked, err := listTrackedFiles(g.Root, excl)
	if err != nil {
		return nil, err
	}
	if offenders := treePurityOffenders(manifest, tracked); len(offenders) > 0 {
		return nil, fmt.Errorf("non-generated tracked files inside disposable release trees: %v; relocate them or fix the manifest", offenders)
	}

	entry, ok := registry.Entry(mainMod.EffectivePath())
	if !ok {
		return nil, fmt.Errorf("main module missing from registry after audit")
	}
	sourceID := "SPDXRef-Package-source"
	pkg := SPDXPackage{
		Name:                 mainMod.EffectivePath(),
		SPDXID:               sourceID,
		VersionInfo:          moduleDisplayVersion(mainMod.EffectiveVersion()),
		DownloadLocation:     entry.SourceURL,
		FilesAnalyzed:        true,
		LicenseConcluded:     entry.SPDXLicense,
		LicenseDeclared:      entry.SPDXLicense,
		CopyrightText:        "Copyright 2026 The Domowik Authors",
		PrimaryPackagePurpos: "APPLICATION",
		Comment:              "First-party source tree; the file list covers every git-tracked file except the release pipeline's own generated outputs.",
	}

	var pkgs []SPDXPackage
	pkgs = append(pkgs, pkg)
	var rels []SPDXRelationship
	rels = append(rels, SPDXRelationship{
		SpdxElementID:      "SPDXRef-DOCUMENT",
		RelationshipType:   "DESCRIBES",
		RelatedSpdxElement: sourceID,
	})
	for _, m := range modules {
		if m.EffectivePath() == mainMod.EffectivePath() {
			continue
		}
		e, ok := registry.Entry(m.EffectivePath())
		if !ok {
			continue
		}
		id := "SPDXRef-Package-" + sanitizeName(m.EffectivePath())
		pkgs = append(pkgs, SPDXPackage{
			Name:             m.EffectivePath(),
			SPDXID:           id,
			VersionInfo:      moduleDisplayVersion(m.EffectiveVersion()),
			DownloadLocation: e.SourceURL,
			FilesAnalyzed:    false,
			LicenseConcluded: e.SPDXLicense,
			LicenseDeclared:  e.SPDXLicense,
			CopyrightText:    "NOASSERTION",
		})
		rels = append(rels, SPDXRelationship{
			SpdxElementID: sourceID, RelationshipType: "DEPENDS_ON", RelatedSpdxElement: id,
		})
	}

	// Verbatim third-party license texts bundled for NOTICE generation keep
	// their upstream license identity; they are never classified first-party.
	noticeLicense := map[string]string{}
	bundledLicense := map[string]string{}
	for _, e := range registry.Entries {
		for _, nf := range e.NoticeFiles {
			noticeLicense[nf.Path] = e.SPDXLicense
		}
		if e.Kind == KindBundled {
			bundledLicense[e.Key] = e.SPDXLicense
		}
	}

	firstParty := entry.SPDXLicense
	var files []SPDXFile
	for i, rel := range tracked {
		sum, err := hashTrackedFile(g.Root, rel)
		if err != nil {
			return nil, err
		}
		f := SPDXFile{
			FileName:         "./" + rel,
			SPDXID:           fmt.Sprintf("SPDXRef-File-%06d", i),
			Checksums:        []SPDXChecksum{{Algorithm: "SHA256", Value: sum}},
			LicenseConcluded: firstParty,
			CopyrightText:    "NOASSERTION",
			Comment:          "First-party tracked source; the reviewed package-level license applies.",
		}
		switch {
		case noticeLicense[rel] != "":
			f.LicenseConcluded = noticeLicense[rel]
			f.Comment = "Verbatim third-party license text distributed as part of the notice bundle."
		case bundledLicense[rel] != "":
			f.LicenseConcluded = bundledLicense[rel]
			f.Comment = "Tracked bundled file; reviewed registry row governs its license."
		case bundledLicense[rel] == "" && isDeclaredBundled(manifest, rel):
			return nil, fmt.Errorf("tracked file %q is declared bundled but lacks a classified registry row; refusing to mislabel it first-party", rel)
		}
		files = append(files, f)
		rels = append(rels, SPDXRelationship{
			SpdxElementID: sourceID, RelationshipType: "CONTAINS", RelatedSpdxElement: f.SPDXID,
		})
	}
	return RenderSBOM(SBOMInput{Name: "domowik-source", Created: epoch, Packages: pkgs, Files: files, Relationships: rels})
}

// renderBinaryArtifact cross-builds the declared artifact statically,
// asserts its metadata, audits its components against the reviewed registry,
// renders its authoritative SBOM, and returns the document, the exact binary
// bytes, and the per-target embedded files.
func (g *Generator) renderBinaryArtifact(ctx context.Context, manifest *Manifest, registry *Registry, toolchain *ToolchainRecord, modules []Module, art Artifact, embeds []EmbeddedFile, mainMod Module, mainEntry Entry, epoch time.Time) ([]byte, []byte, []EmbeddedFile, error) {
	tmp, err := os.MkdirTemp("", "domowik-sbom-build")
	if err != nil {
		return nil, nil, nil, fmt.Errorf("create build dir: %w", err)
	}
	defer os.RemoveAll(tmp)
	binPath := filepath.Join(tmp, art.ID)

	const cgo = "0"
	targetEnv := []string{"CGO_ENABLED=" + cgo, "GOOS=" + art.OS, "GOARCH=" + art.Arch}
	if _, err := runGo(ctx, g.Root, targetEnv,
		"build", "-trimpath", "-buildvcs=false", "-o", binPath, art.Package); err != nil {
		return nil, nil, nil, fmt.Errorf("go build: %w", err)
	}

	bi, err := ReadBuildInfo(binPath)
	if err != nil {
		return nil, nil, nil, err
	}
	if err := assertBuildMetadata(bi, art, cgo, mainMod, toolchain); err != nil {
		return nil, nil, nil, err
	}

	binComps, err := ComponentsFromBuildInfo(bi)
	if err != nil {
		return nil, nil, nil, err
	}
	if err := assertBinMatchesSourceGraph(binComps, modules, toolchain); err != nil {
		return nil, nil, nil, err
	}

	// Declarative-vs-discovered cross-check runs BEFORE the registry audit:
	// any embedded/bundled file must be declared in release/manifest.json and
	// carry a reviewed registry row, or generation fails closed.
	discovered := make([]string, 0, len(embeds))
	for _, ef := range embeds {
		discovered = append(discovered, ef.Path)
	}
	sort.Strings(discovered)
	if err := crossCheckBundled(manifest.DeclaredForArtifact(art.ID), discovered); err != nil {
		return nil, nil, nil, fmt.Errorf("bundled declaration mismatch for artifact %s: %w", art.ID, err)
	}
	// Subset view: no stale checking here; the single top-level union audit
	// owns staleness so target-specific rows cannot be misreported by other
	// targets or by the source-level pass.
	bundledComps, err := embedComponents(embeds, g.Root)
	if err != nil {
		return nil, nil, nil, err
	}
	auditComps := append(append([]Component{}, binComps...), bundledComps...)
	if findings := registry.AuditEntries(auditComps); len(findings) > 0 {
		return nil, nil, nil, auditError("binary inventory audit failed closed", findings)
	}

	binBytes, err := os.ReadFile(binPath)
	if err != nil {
		return nil, nil, nil, fmt.Errorf("read built binary: %w", err)
	}
	binSum := sha256Sum(binBytes)

	binID := "SPDXRef-Package-" + sanitizeName(art.ID)
	comment := fmt.Sprintf(
		"Authoritative per-binary SBOM derived from this exact artifact's Go build metadata. " +
			"Built statically (CGO_ENABLED=0) with -trimpath -buildvcs=false from a clean checkout at the commit " +
			"containing this document; the absence of -buildvcs/vcs.* settings is asserted, so binary hashes are stable across commits.")

	binPkg := SPDXPackage{
		Name:                 art.ID,
		SPDXID:               binID,
		VersionInfo:          moduleDisplayVersion(bi.Main.Version),
		DownloadLocation:     "NOASSERTION",
		FilesAnalyzed:        false,
		LicenseConcluded:     mainEntry.SPDXLicense,
		LicenseDeclared:      mainEntry.SPDXLicense,
		CopyrightText:        "Copyright 2026 The Domowik Authors",
		PrimaryPackagePurpos: "APPLICATION",
		Checksums:            []SPDXChecksum{{Algorithm: "SHA256", Value: binSum}},
		Comment:              comment,
	}

	var pkgs []SPDXPackage
	pkgs = append(pkgs, binPkg)
	var rels []SPDXRelationship
	rels = append(rels, SPDXRelationship{
		SpdxElementID: "SPDXRef-DOCUMENT", RelationshipType: "DESCRIBES", RelatedSpdxElement: binID,
	})
	for _, c := range binComps {
		e, ok := registry.Entry(c.Key)
		if !ok {
			continue
		}
		id := "SPDXRef-Package-" + sanitizeName(c.Key)
		dl := e.SourceURL
		if c.Key == ToolchainKey {
			dl = "https://go.dev/dl/"
		}
		pkg := SPDXPackage{
			Name:             c.Key,
			SPDXID:           id,
			VersionInfo:      c.Version,
			DownloadLocation: dl,
			FilesAnalyzed:    false,
			LicenseConcluded: e.SPDXLicense,
			LicenseDeclared:  e.SPDXLicense,
			CopyrightText:    "NOASSERTION",
		}
		if c.Key == ToolchainKey {
			a, _ := toolchain.Artifact(art.OS, art.Arch)
			src, _ := toolchainSourceArtifact(toolchain)
			pkg.Comment = fmt.Sprintf(
				"Exact artifacts are pinned in docs/third-party/toolchain-go.json with SHA-256 hashes: archive %s (%s), source %s (%s).",
				a.Filename, a.SHA256, src.Filename, src.SHA256)
		}
		pkgs = append(pkgs, pkg)
		rels = append(rels, SPDXRelationship{
			SpdxElementID: binID, RelationshipType: "DEPENDS_ON", RelatedSpdxElement: id,
		})
	}

	var files []SPDXFile
	for i, ef := range embeds {
		sum, err := hashTrackedFile(g.Root, ef.Path)
		if err != nil {
			return nil, nil, nil, err
		}
		e, ok := registry.Entry(ef.Path)
		fileLicense := "NOASSERTION"
		fileComment := fmt.Sprintf("Embedded by %s via patterns %s.", ef.ImportPath, strings.Join(ef.Patterns, ", "))
		switch {
		case ok && e.Kind == KindBundled:
			fileLicense = e.SPDXLicense
		case sameModule(ef.Path, mainMod):
			fileLicense = mainEntry.SPDXLicense
			fileComment += " First-party embedded file."
		}
		fid := fmt.Sprintf("SPDXRef-File-%s-%06d", sanitizeName(art.ID), i)
		files = append(files, SPDXFile{
			FileName:         "./" + ef.Path,
			SPDXID:           fid,
			Checksums:        []SPDXChecksum{{Algorithm: "SHA256", Value: sum}},
			LicenseConcluded: fileLicense,
			CopyrightText:    "NOASSERTION",
			Comment:          fileComment,
		})
		rels = append(rels, SPDXRelationship{
			SpdxElementID: binID, RelationshipType: "CONTAINS", RelatedSpdxElement: fid,
		})
	}

	doc, err := RenderSBOM(SBOMInput{Name: "domowik-" + art.ID, Created: epoch, Packages: pkgs, Files: files, Relationships: rels})
	if err != nil {
		return nil, nil, nil, err
	}
	return doc, binBytes, embeds, nil
}

// crossCheckBundled requires exact equality between the declarative bundled
// list and go:embed discovery for a target. Missing declarations mean an
// undeclared distributed file; extra declarations mean stale manifest rows.
// Both directions require a deliberate manifest AND registry update.
func crossCheckBundled(declared, discovered []string) error {
	dset := map[string]bool{}
	for _, d := range declared {
		if dset[d] {
			return fmt.Errorf("declared bundled path %q is duplicated", d)
		}
		dset[d] = true
	}
	fset := map[string]bool{}
	for _, f := range discovered {
		if fset[f] {
			return fmt.Errorf("discovered bundled path %q is duplicated", f)
		}
		fset[f] = true
	}
	var missing, extra []string
	for _, d := range declared {
		if !fset[d] {
			missing = append(missing, d)
		}
	}
	for _, f := range discovered {
		if !dset[f] {
			extra = append(extra, f)
		}
	}
	sort.Strings(missing)
	sort.Strings(extra)
	if len(missing) > 0 || len(extra) > 0 {
		return fmt.Errorf("embedded files do not match the manifest declaration (missing declaration for: %v; undeclared extras: %v); update release/manifest.json and docs/third-party/licenses.json together",
			missing, extra)
	}
	return nil
}

// CrossCheckToolchain binds the pinned toolchain record to its reviewed
// registry entry: the component key must be the reserved toolchain key, the
// SPDX license ids must agree in both directions, and the record's license
// URL must match the reviewed source reference.
func CrossCheckToolchain(registry *Registry, toolchain *ToolchainRecord) error {
	if toolchain.Component != ToolchainKey {
		return fmt.Errorf("toolchain record component %q must be the reserved key %q", toolchain.Component, ToolchainKey)
	}
	e, ok := registry.Entry(ToolchainKey)
	if !ok {
		return fmt.Errorf("registry has no reviewed entry for %q", ToolchainKey)
	}
	if e.SPDXLicense != toolchain.License.SPDXID {
		return fmt.Errorf("toolchain record license %q disagrees with reviewed registry entry %q", toolchain.License.SPDXID, e.SPDXLicense)
	}
	if e.SourceURL != toolchain.License.URL {
		return fmt.Errorf("toolchain record license url %q disagrees with reviewed registry sourceUrl %q", toolchain.License.URL, e.SourceURL)
	}
	if e.VersionPin != toolchain.Version {
		return fmt.Errorf("registry pins toolchain %q but the record pins %q; review together", e.VersionPin, toolchain.Version)
	}
	return nil
}

func sameModule(path string, m Module) bool {
	return path == m.Path || path == m.ReplacedBy
}

// checkTargetReviewed requires BOTH a reviewed archive row for the exact
// os/arch and explicit membership in SupportedTargets before any build runs.
func checkTargetReviewed(toolchain *ToolchainRecord, art Artifact) error {
	tgt := art.OS + "/" + art.Arch
	member := false
	for _, t := range toolchain.SupportedTargets {
		if t == tgt {
			member = true
			break
		}
	}
	if !member {
		return fmt.Errorf("artifact %s: target %s is absent from toolchain supportedTargets; add it via the record's revisit process before building", art.ID, tgt)
	}
	if _, ok := toolchain.Artifact(art.OS, art.Arch); !ok {
		return fmt.Errorf("artifact %s: target %s has no reviewed archive row; add it via the record's revisit process before building", art.ID, tgt)
	}
	return nil
}

// discoverAllEmbeds collects embedded files for every declared artifact
// target up front (CGO_ENABLED=0), validating each against the declarative
// bundled list so the union used for the fail-closed audit is complete and
// consistent before any output is rendered.
func (g *Generator) discoverAllEmbeds(ctx context.Context, toolchain *ToolchainRecord, manifest *Manifest) (map[string][]EmbeddedFile, error) {
	out := make(map[string][]EmbeddedFile, len(manifest.Artifacts))
	for _, art := range manifest.Artifacts {
		if err := checkTargetReviewed(toolchain, art); err != nil {
			return nil, err
		}
		env := []string{"CGO_ENABLED=0", "GOOS=" + art.OS, "GOARCH=" + art.Arch}
		// Authoritative per-binary embeds come from the artifact's own
		// dependency closure, not from the whole repository.
		embeds, err := CollectEmbeds(ctx, g.Root, env, art.Package)
		if err != nil {
			return nil, fmt.Errorf("artifact %s: %w", art.ID, err)
		}
		discovered := make([]string, 0, len(embeds))
		for _, ef := range embeds {
			discovered = append(discovered, ef.Path)
		}
		sort.Strings(discovered)
		if err := crossCheckBundled(manifest.DeclaredForArtifact(art.ID), discovered); err != nil {
			return nil, fmt.Errorf("bundled declaration mismatch for %s/%s: %w", art.OS, art.Arch, err)
		}
		out[art.ID] = embeds
	}
	// Repo-wide hidden-embed guard: for EVERY distinct declared target,
	// every go:embed anywhere in the module must be declared in the
	// manifest, even if no shipped binary currently reaches it, so unused or
	// later-target-only embeds cannot silently enter future artifacts while
	// remaining undeclared.
	distinct := map[string]bool{}
	for _, art := range manifest.Artifacts {
		distinct[art.OS+"/"+art.Arch] = true
	}
	for _, tgt := range sortedStrings(distinct) {
		parts := strings.SplitN(tgt, "/", 2)
		env := []string{"CGO_ENABLED=0", "GOOS=" + parts[0], "GOARCH=" + parts[1]}
		all, err := CollectEmbeds(ctx, g.Root, env, "./...")
		if err != nil {
			return nil, fmt.Errorf("hidden-embed scan (%s): %w", tgt, err)
		}
		var paths []string
		for _, ef := range all {
			paths = append(paths, ef.Path)
		}
		if undeclared := undeclaredEmbeds(manifest, paths); len(undeclared) > 0 {
			return nil, fmt.Errorf("embedded files not declared under manifest.bundled (%s): %v", tgt, undeclared)
		}
	}
	return out, nil
}

// unionEmbedComponents deduplicates embed components across artifacts so the
// component inventory is independent of Go map iteration order when a file
// is embedded by multiple targets.
func unionEmbedComponents(embedsByArtifact map[string][]EmbeddedFile, root string) ([]Component, error) {
	ids := make([]string, 0, len(embedsByArtifact))
	for id := range embedsByArtifact {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	seen := map[string]bool{}
	var comps []Component
	for _, id := range ids {
		for _, ef := range embedsByArtifact[id] {
			key := ef.Path
			if seen[key] {
				continue
			}
			seen[key] = true
			sum, err := hashTrackedFile(root, key)
			if err != nil {
				return nil, fmt.Errorf("pin embedded file %s: %w", key, err)
			}
			comps = append(comps, Component{Key: key, Kind: KindBundled, Version: contentPin(sum)})
		}
	}
	sortComponents(comps)
	return comps, nil
}

// contentPin renders a file's SHA-256 as its auditable component version so
// changed distributed content can never pass a stale registry row.
func contentPin(sum string) string { return "sha256:" + sum }

// undeclaredEmbeds returns discovered paths absent from the manifest's
// bundled declaration for every target.
func undeclaredEmbeds(m *Manifest, discovered []string) []string {
	declared := map[string]bool{}
	if m.Bundled != nil {
		for _, b := range *m.Bundled {
			declared[b.Path] = true
		}
	}
	sort.Strings(discovered)
	var out []string
	for _, p := range discovered {
		if !declared[p] {
			out = append(out, p)
		}
	}
	return out
}

// isDeclaredBundled reports whether a tracked file appears under the
// manifest's bundled declaration for any artifact.
func isDeclaredBundled(m *Manifest, rel string) bool {
	if m.Bundled != nil {
		for _, b := range *m.Bundled {
			if b.Path == rel {
				return true
			}
		}
	}
	return false
}

func sortedStrings(set map[string]bool) []string {
	out := make([]string, 0, len(set))
	for k := range set {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// selectMainModule picks the main module by its Main flag, failing closed
// when the resolved graph has zero or multiple main entries.
func selectMainModule(modules []Module) (Module, error) {
	var mains []Module
	for _, m := range modules {
		if m.Main {
			mains = append(mains, m)
		}
	}
	switch len(mains) {
	case 1:
		return mains[0], nil
	case 0:
		return Module{}, fmt.Errorf("resolved module graph contains no main module")
	default:
		return Module{}, fmt.Errorf("resolved module graph contains %d main modules: %v", len(mains), modulePaths(mains))
	}
}

func modulePaths(ms []Module) []string {
	out := make([]string, 0, len(ms))
	for _, m := range ms {
		out = append(out, m.Path)
	}
	sort.Strings(out)
	return out
}

// embedComponents maps discovered embedded files to auditable components.
// Every embedded or otherwise bundled file must carry a reviewed registry row
// (kind "bundled") unless it belongs to the first-party module itself.
// embedComponents maps discovered embedded files to auditable components,
// pinned by content hash so any change to distributed bytes invalidates the
// reviewed registry row until it is deliberately re-reviewed.
func embedComponents(embeds []EmbeddedFile, root string) ([]Component, error) {
	comps := make([]Component, 0, len(embeds))
	for _, ef := range embeds {
		sum, err := hashTrackedFile(root, ef.Path)
		if err != nil {
			return nil, fmt.Errorf("pin embedded file %s: %w", ef.Path, err)
		}
		comps = append(comps, Component{Key: ef.Path, Kind: KindBundled, Version: contentPin(sum)})
	}
	sortComponents(comps)
	return comps, nil
}

// assertBinMatchesSourceGraph performs an EXACT comparison between the
// binary's build-info component set and the dynamically resolved source
// graph: identical module identity sets with identical effective versions,
// plus the reviewed toolchain version. Extras, omissions, duplicates, and
// version conflicts all fail closed (AC2).
func assertBinMatchesSourceGraph(binComps []Component, modules []Module, toolchain *ToolchainRecord) error {
	graph := map[string]string{}
	for _, m := range modules {
		key := m.EffectivePath()
		ver := comparableVersion(m.EffectiveVersion())
		if prev, dup := graph[key]; dup && prev != ver {
			return fmt.Errorf("resolved source graph contains conflicting identities for module %q (%q vs %q)", key, prev, ver)
		}
		graph[key] = ver
	}
	binMods := map[string]string{}
	binToolchain := ""
	for _, c := range binComps {
		switch c.Kind {
		case KindModule:
			if prev, dup := binMods[c.Key]; dup {
				return fmt.Errorf("build info contains duplicate identity for module %q (%q vs %q)", c.Key, prev, c.Version)
			}
			binMods[c.Key] = comparableVersion(c.Version)
		case KindToolchain:
			if binToolchain != "" {
				return fmt.Errorf("build info records multiple toolchains (%q, %q)", binToolchain, c.Version)
			}
			binToolchain = c.Version
		default:
			return fmt.Errorf("unexpected component kind %q from build info", c.Kind)
		}
	}
	if binToolchain == "" {
		return fmt.Errorf("build info records no toolchain")
	}
	if binToolchain != toolchain.Version {
		return fmt.Errorf("binary toolchain %q does not match the reviewed record %q; update docs/third-party/toolchain-go.json via its revisit triggers", binToolchain, toolchain.Version)
	}
	for key, ver := range binMods {
		want, ok := graph[key]
		if !ok {
			return fmt.Errorf("binary depends on module %q which is absent from the resolved source graph", key)
		}
		if want != ver {
			return fmt.Errorf("binary module %q version %q differs from resolved %q", key, binMods[key], want)
		}
	}
	for key := range graph {
		if _, ok := binMods[key]; !ok {
			return fmt.Errorf("resolved module %q is missing from the binary's build info", key)
		}
	}
	return nil
}

// comparableVersion normalizes the build-info placeholder version so the
// source-graph empty version and the binary "(devel)" marker compare equal.
func comparableVersion(v string) string {
	if v == "(devel)" || v == "" {
		return ""
	}
	return v
}

// assertBuildMetadata verifies the freshly built binary's recorded metadata
// against the declared artifact and the current checkout rather than trusting
// the build invocation.
func assertBuildMetadata(bi *debug.BuildInfo, art Artifact, wantCGO string, mainMod Module, toolchain *ToolchainRecord) error {
	settings := map[string]string{}
	for _, s := range bi.Settings {
		settings[s.Key] = s.Value
	}
	checkEq := func(key, want string) error {
		got, ok := settings[key]
		if !ok {
			return fmt.Errorf("build metadata missing %s", key)
		}
		if got != want {
			return fmt.Errorf("build metadata %s=%q does not match declared %q", key, got, want)
		}
		return nil
	}
	if err := checkEq("GOOS", art.OS); err != nil {
		return err
	}
	if err := checkEq("GOARCH", art.Arch); err != nil {
		return err
	}
	if err := checkEq("CGO_ENABLED", wantCGO); err != nil {
		return err
	}
	if wantCGO != "0" {
		return fmt.Errorf("distributed artifacts must be static (CGO_ENABLED=0), got %q", wantCGO)
	}
	if err := checkEq("-trimpath", "true"); err != nil {
		return err
	}
	// With -buildvcs=false the Go tool records no -buildvcs or vcs.* setting
	// at all; their presence would mean non-reproducible VCS stamping.
	for _, key := range []string{"-buildvcs", "vcs.revision", "vcs.modified", "vcs.time"} {
		if _, present := settings[key]; present {
			return fmt.Errorf("unexpected %s in build metadata; binaries must be built reproducibly without VCS stamping", key)
		}
	}
	goVersion := strings.TrimPrefix(bi.GoVersion, "go")
	if goVersion != strings.TrimPrefix(toolchain.Version, "go") {
		return fmt.Errorf("binary reports Go version %q but the reviewed record pins %q; update docs/third-party/toolchain-go.json via its revisit triggers", bi.GoVersion, toolchain.Version)
	}
	if bi.Main.Replace != nil {
		return fmt.Errorf("binary main module records a replacement to %q; replaced main modules are unsupported", bi.Main.Replace.Path)
	}
	if bi.Main.Path != mainMod.Path && bi.Main.Path != mainMod.ReplacedBy {
		return fmt.Errorf("binary main module %q does not match discovered main module %q", bi.Main.Path, mainMod.Path)
	}
	return nil
}

// WriteAll writes every generated output to its declared repository location,
// refusing any path that resolves outside the root or overwrites a symlink.
func (g *Generator) WriteAll(inv *Inventory) error {
	all := map[string][]byte{}
	for k, v := range inv.Outputs {
		all[k] = v
	}
	for k, v := range inv.Evidence {
		all[k] = v
	}
	paths := make([]string, 0, len(all))
	for p := range all {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	for _, p := range paths {
		if err := writeFileUnderRoot(g.Root, p, all[p]); err != nil {
			return err
		}
	}
	return nil
}

// materializeEvidence writes any missing local release evidence and fails on
// present-but-mismatched evidence, keeping CI scratch cleanup away from
// release outputs. All paths are resolved root-safely with final-target
// symlink rejection.
func (g *Generator) materializeEvidence(inv *Inventory) error {
	paths := make([]string, 0, len(inv.Evidence))
	for p := range inv.Evidence {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	if err := g.checkEvidenceInventory(inv); err != nil {
		return err
	}
	for _, p := range paths {
		_, realDir, err := resolveUnderRoot(g.Root, p, true)
		if err != nil {
			return err
		}
		base := filepath.Base(filepath.FromSlash(p))
		if err := rejectSymlinkTarget(realDir, base); err != nil {
			return fmt.Errorf("release evidence %s: %w", p, err)
		}
		existing, readErr := os.ReadFile(filepath.Join(realDir, base))
		if os.IsNotExist(readErr) {
			if err := writeFileUnderRoot(g.Root, p, inv.Evidence[p]); err != nil {
				return err
			}
			continue
		}
		if readErr != nil {
			return fmt.Errorf("read release evidence %s: %w", p, readErr)
		}
		if string(existing) != string(inv.Evidence[p]) {
			return fmt.Errorf("release evidence %s is stale or foreign; run `domowik-sbom generate` deliberately", p)
		}
	}
	return nil
}

// Plan runs the identical pipeline as Build (including target builds,
// per-target embed discovery, notice verification, and the full fail-closed
// audit) but never writes and tolerates a dirty tree for developer reporting.
// A successful plan implies generate can succeed.
func (g *Generator) Plan(ctx context.Context) (*Inventory, error) {
	return g.Build(ctx, BuildOptions{AllowDirty: true})
}

// checkEvidenceInventory validates the evidence tree for a freshly built
// inventory via checkEvidenceDir.
func (g *Generator) checkEvidenceInventory(inv *Inventory) error {
	manifest, _, _, err := g.LoadInputs()
	if err != nil {
		return err
	}
	ids := make([]string, 0, len(inv.Binaries))
	for id := range inv.Binaries {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	return g.checkEvidenceDir(manifest.ReleaseOutputDir, ids)
}

// checkEvidenceDir enforces an EXACT directory inventory for the release
// evidence tree: only the declared binaries and the consumption manifest may
// exist as regular files. Stale or future artifacts (gitignored and
// therefore invisible to the clean-tree gate) fail closed so they can never
// be consumed or published accidentally. Symlinks and non-regular files are
// rejected; a missing tree is tolerated so first-run generation can create
// it. Shared by Verify's materialization and standalone check-manifest.
func (g *Generator) checkEvidenceDir(releaseOutputDir string, binaryIDs []string) error {
	rootDir := filepath.Join(g.Root, filepath.FromSlash(releaseOutputDir))
	expected := map[string]bool{
		filepath.Join(rootDir, "manifest.json"): true,
	}
	for _, id := range binaryIDs {
		expected[filepath.Join(rootDir, id)] = true
	}
	entries, err := os.ReadDir(rootDir)
	if os.IsNotExist(err) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("read release output dir: %w", err)
	}
	for _, e := range entries {
		full := filepath.Join(rootDir, e.Name())
		fi, lstatErr := os.Lstat(full)
		if lstatErr != nil {
			return fmt.Errorf("stat release evidence %s: %w", e.Name(), lstatErr)
		}
		// Type checks precede inventory membership so a hostile or stale
		// symlink is reported as such, not merely as unexpected.
		if fi.Mode()&os.ModeSymlink != 0 || !fi.Mode().IsRegular() {
			return fmt.Errorf("release evidence entry %q is not a regular file", e.Name())
		}
		if !expected[full] {
			return fmt.Errorf("unexpected file %q under %s; it is absent from the distribution manifest and must be removed deliberately",
				e.Name(), releaseOutputDir)
		}
	}
	return nil
}

// VerifyResult summarizes comparison between regeneration and committed state.
type VerifyResult struct {
	CheckedPaths []string
}

// CheckConsumptionManifest validates build/release/manifest.json against the
// current on-disk release evidence and committed SBOM/NOTICE artifacts using
// the generation hashing contract (timestamp-stripped SBOM hashes, raw bytes
// otherwise), so external consumers such as CI share one implementation.
func (g *Generator) CheckConsumptionManifest(ctx context.Context) ([]string, error) {
	manifest, _, toolchain, err := g.LoadInputs()
	if err != nil {
		return nil, err
	}
	modules, err := ListModules(ctx, g.Root)
	if err != nil {
		return nil, err
	}
	mainMod, err := selectMainModule(modules)
	if err != nil {
		return nil, err
	}
	data, err := readFileUnderRoot(g.Root, manifest.ConsumptionManifest.Path)
	if err != nil {
		return nil, fmt.Errorf("read consumption manifest: %w", err)
	}
	var doc consumptionDoc
	if err := strictUnmarshal(data, &doc); err != nil {
		return nil, fmt.Errorf("parse consumption manifest: %w", err)
	}
	if doc.Schema != consumptionSchema {
		return nil, fmt.Errorf("unsupported consumption schema %d (want %d)", doc.Schema, consumptionSchema)
	}
	if doc.Generator.Name != generatorName || doc.Generator.Version != GeneratorVersion {
		return nil, fmt.Errorf("consumption manifest was produced by %s-%s; this tool is %s-%s",
			doc.Generator.Name, doc.Generator.Version, generatorName, GeneratorVersion)
	}
	if err := validateConsumptionStructure(&doc, manifest, toolchain); err != nil {
		return nil, err
	}
	var binaryIDs []string
	for _, a := range doc.Artifacts {
		binaryIDs = append(binaryIDs, a.ID)
	}
	if err := g.checkEvidenceDir(manifest.ReleaseOutputDir, binaryIDs); err != nil {
		return nil, fmt.Errorf("release evidence inventory: %w", err)
	}

	check := func(cf consumptionFile, sbom bool) (string, error) {
		onDisk, err := readFileUnderRoot(g.Root, cf.Path)
		if err != nil {
			return "", err
		}
		got := sha256Sum(onDisk)
		if sbom {
			got, err = sbomEvidenceHash(onDisk) // controlled error on malformed SBOM
			if err != nil {
				return "", err
			}
		}
		if got != cf.SHA256 {
			return got, fmt.Errorf("%s hash mismatch: recorded %s, computed %s", cf.Path, cf.SHA256, got)
		}
		return got, nil
	}
	if _, err := check(doc.SourceSBOM, true); err != nil {
		return nil, fmt.Errorf("source sbom: %w", err)
	}
	if _, err := check(doc.Notice, false); err != nil {
		return nil, fmt.Errorf("notice: %w", err)
	}
	var visited []string
	for _, a := range doc.Artifacts {
		binSum, err := check(a.Binary, false)
		if err != nil {
			return nil, fmt.Errorf("artifact %s binary: %w", a.ID, err)
		}
		if _, err := check(a.SBOM, true); err != nil {
			return nil, fmt.Errorf("artifact %s sbom: %w", a.ID, err)
		}
		// Bind the SBOM's application package checksum to the exact binary:
		// a validly hashed but mismatched per-binary SBOM must fail.
		onDiskSbom, err := readFileUnderRoot(g.Root, a.SBOM.Path)
		if err != nil {
			return nil, err
		}
		appSum, appID, err := spdxApplicationChecksum(onDiskSbom)
		if err != nil {
			return nil, fmt.Errorf("artifact %s: %w", a.ID, err)
		}
		wantApp := "SPDXRef-Package-" + sanitizeName(a.ID)
		if appID != wantApp {
			return nil, fmt.Errorf("artifact %s: sbom describes %q, expected the application package %q", a.ID, appID, wantApp)
		}
		// Independently confirm the on-disk binary's own build metadata
		// matches the declared target, so evidence swapped between targets
		// cannot pass even with consistent hashes.
		biPath := filepath.Join(g.Root, filepath.FromSlash(a.Binary.Path))
		bi, err := ReadBuildInfo(biPath)
		if err != nil {
			return nil, fmt.Errorf("artifact %s: %w", a.ID, err)
		}
		var declared Artifact
		for _, mArt := range manifest.Artifacts {
			if mArt.ID == a.ID {
				declared = mArt
				break
			}
		}
		if err := assertBuildMetadata(bi, declared, "0", mainMod, toolchain); err != nil {
			return nil, fmt.Errorf("artifact %s binary metadata: %w", a.ID, err)
		}
		if appSum != binSum {
			return nil, fmt.Errorf("artifact %s: sbom %s records application checksum %s for %q, which does not match binary hash %s", a.ID, a.SBOM.Path, appSum, appID, binSum)
		}
		// Bind the manifest's embeddedFiles claim to the authoritative SBOM
		// file entries: exact sorted set equality.
		sbomFiles, err := SPDXEmbeddedFilePaths(onDiskSbom)
		if err != nil {
			return nil, fmt.Errorf("artifact %s: %w", a.ID, err)
		}
		want := append([]string{}, a.Embeds...)
		sort.Strings(want)
		if strings.Join(sbomFiles, "\x00") != strings.Join(want, "\x00") {
			return nil, fmt.Errorf("artifact %s: embeddedFiles %v does not match sbom file entries %v", a.ID, want, sbomFiles)
		}
		visited = append(visited, a.Binary.Path, a.SBOM.Path)
	}
	return append([]string{doc.SourceSBOM.Path, doc.Notice.Path}, visited...), nil
}

// validateConsumptionStructure cross-checks a parsed consumption document
// against the declarative manifest and toolchain record: generator identity,
// exact artifact set (ids, targets, packages, paths), duplicate rejection,
// and declared output locations.
func validateConsumptionStructure(doc *consumptionDoc, manifest *Manifest, toolchain *ToolchainRecord) error {
	if doc.Schema != consumptionSchema {
		return fmt.Errorf("unsupported consumption schema %d (want %d)", doc.Schema, consumptionSchema)
	}
	if doc.Generator.Name != generatorName || doc.Generator.Version != GeneratorVersion {
		return fmt.Errorf("consumption manifest was produced by %s-%s; this tool is %s-%s",
			doc.Generator.Name, doc.Generator.Version, generatorName, GeneratorVersion)
	}
	if doc.Toolchain.Version != toolchain.Version || doc.Toolchain.Record != toolchainPath {
		return fmt.Errorf("consumption manifest toolchain block %+v disagrees with the declarative inputs", doc.Toolchain)
	}
	wantSource := manifest.SBOMDir + "/source.spdx.json"
	wantNotice := manifest.Notice.Path
	if doc.SourceSBOM.Path != wantSource {
		return fmt.Errorf("consumption manifest source sbom path %q does not match declarative %q", doc.SourceSBOM.Path, wantSource)
	}
	if doc.Notice.Path != wantNotice {
		return fmt.Errorf("consumption manifest notice path %q does not match declarative %q", doc.Notice.Path, wantNotice)
	}
	if len(doc.Artifacts) != len(manifest.Artifacts) {
		return fmt.Errorf("consumption manifest lists %d artifacts, manifest declares %d", len(doc.Artifacts), len(manifest.Artifacts))
	}
	byID := map[string]Artifact{}
	for _, a := range manifest.Artifacts {
		byID[a.ID] = a
	}
	seenDocID := map[string]bool{}
	seenDocPath := map[string]bool{}
	for _, a := range doc.Artifacts {
		if seenDocID[a.ID] {
			return fmt.Errorf("consumption manifest lists artifact %q twice", a.ID)
		}
		seenDocID[a.ID] = true
		for _, cf := range []consumptionFile{a.Binary, a.SBOM} {
			if seenDocPath[cf.Path] {
				return fmt.Errorf("consumption manifest references path %q more than once", cf.Path)
			}
			seenDocPath[cf.Path] = true
		}
		declared, ok := byID[a.ID]
		if !ok {
			return fmt.Errorf("consumption manifest artifact %q is not declared in release/manifest.json", a.ID)
		}
		delete(byID, a.ID)
		wantBinary := manifest.ReleaseOutputDir + "/" + a.ID
		wantSbom := manifest.SBOMDir + "/" + a.ID + ".spdx.json"
		switch {
		case a.OS != declared.OS || a.Arch != declared.Arch:
			return fmt.Errorf("artifact %q: consumption target %s/%s differs from declaration %s/%s", a.ID, a.OS, a.Arch, declared.OS, declared.Arch)
		case a.Package != declared.Package:
			return fmt.Errorf("artifact %q: consumption package %q differs from declaration %q", a.ID, a.Package, declared.Package)
		case a.Binary.Path != wantBinary:
			return fmt.Errorf("artifact %q: binary path %q does not match declared output location %q", a.ID, a.Binary.Path, wantBinary)
		case a.SBOM.Path != wantSbom:
			return fmt.Errorf("artifact %q: sbom path %q does not match declared location %q", a.ID, a.SBOM.Path, wantSbom)
		}
	}
	if len(byID) > 0 {
		var missing []string
		for id := range byID {
			missing = append(missing, id)
		}
		sort.Strings(missing)
		return fmt.Errorf("declared artifacts absent from consumption manifest: %v", missing)
	}

	return nil
}

// Verify regenerates the full inventory twice (determinism self-check),
// compares committed text artifacts byte-for-byte (normalizing only the SPDX
// creation timestamp), and materializes-or-validates the local release
// evidence (consumption manifest and hashed binaries). Scratch cleanup
// elsewhere in CI must never touch the declared release output directory.
func (g *Generator) Verify(ctx context.Context, getenv Getenv, headTime HeadTime) (*VerifyResult, error) {
	first, err := g.Build(ctx, BuildOptions{getenv: getenv, headTime: headTime})
	if err != nil {
		return nil, fmt.Errorf("regeneration failed: %w", err)
	}
	second, err := g.Build(ctx, BuildOptions{getenv: getenv, headTime: headTime})
	if err != nil {
		return nil, fmt.Errorf("second regeneration failed: %w", err)
	}
	for p := range first.Outputs {
		if string(first.Outputs[p]) != string(second.Outputs[p]) {
			return nil, fmt.Errorf("determinism failure: regeneration of %s is not byte-identical", p)
		}
	}
	for p := range first.Evidence {
		if string(first.Evidence[p]) != string(second.Evidence[p]) {
			return nil, fmt.Errorf("determinism failure: regeneration of evidence %s is not byte-identical", p)
		}
	}

	paths := make([]string, 0, len(first.Outputs))
	for p := range first.Outputs {
		paths = append(paths, p)
	}
	sort.Strings(paths)
	for _, p := range paths {
		committed, err := readFileUnderRoot(g.Root, p)
		if err != nil {
			return nil, fmt.Errorf("committed artifact %s unreadable: %w", p, err)
		}
		if isSBOMPath(p) {
			gotN, err := NormalizeSBOM(first.Outputs[p])
			if err != nil {
				return nil, fmt.Errorf("generated %s: %w", p, err)
			}
			wantN, err := NormalizeSBOM(committed)
			if err != nil {
				return nil, fmt.Errorf("committed %s: %w", p, err)
			}
			if string(gotN) != string(wantN) {
				return nil, fmt.Errorf("committed artifact %s differs from regeneration beyond the permitted creation timestamp; run `domowik-sbom generate` and commit the result", p)
			}
			if !hasCreated(committed) {
				return nil, fmt.Errorf("committed artifact %s lacks a recorded creation timestamp", p)
			}
			continue
		}
		if string(first.Outputs[p]) != string(committed) {
			return nil, fmt.Errorf("committed artifact %s differs byte-for-byte from regeneration; run `domowik-sbom generate` and commit the result", p)
		}
	}
	if err := g.materializeEvidence(first); err != nil {
		return nil, err
	}
	return &VerifyResult{CheckedPaths: paths}, nil
}

func isSBOMPath(p string) bool {
	return strings.HasSuffix(p, ".spdx.json")
}

func hasCreated(data []byte) bool {
	var doc struct {
		CreationInfo struct {
			Created string `json:"created"`
		} `json:"creationInfo"`
	}
	if err := json.Unmarshal(data, &doc); err != nil {
		return false
	}
	return doc.CreationInfo.Created != ""
}

// requireCleanTree fails on any tracked modification or unexpected untracked
// file. Untracked paths under the pipeline's own declared generated outputs
// are tolerated so first-time generation can create them; everything else,
// including renames and staged changes, is rejected.
func requireCleanTree(ctx context.Context, root string, allowDirty bool, excluded map[string]bool) error {
	cmd := exec.CommandContext(ctx, "git", "-C", root, "status", "--porcelain")
	out, err := cmd.Output()
	if err != nil {
		return fmt.Errorf("git status: %w", err)
	}
	var offenders []string
	for _, line := range strings.Split(string(out), "\n") {
		line = strings.TrimRight(line, "\r")
		if len(line) < 4 {
			continue
		}
		x, y := line[0], line[1]
		path := strings.TrimSpace(line[3:])
		if i := strings.Index(path, " -> "); i >= 0 {
			offenders = append(offenders, path)
			continue
		}
		if x == '?' && y == '?' && (excluded[path] || excludedUnder(excluded, path)) {
			continue
		}
		offenders = append(offenders, fmt.Sprintf("%c%c %s", x, y, path))
	}
	if len(offenders) > 0 && !allowDirty {
		return fmt.Errorf("working tree is not a clean checkout (%d offender(s): %s); the inventory must be generated from committed state",
			len(offenders), strings.Join(offenders, "; "))
	}
	return nil
}

func gitHeadTime(root string) (int64, error) {
	out, err := exec.Command("git", "-C", root, "log", "-1", "--format=%ct").Output()
	if err != nil {
		return 0, fmt.Errorf("git log: %w", err)
	}
	var sec int64
	if _, err := fmt.Sscanf(strings.TrimSpace(string(out)), "%d", &sec); err != nil {
		return 0, fmt.Errorf("parse HEAD time: %w", err)
	}
	return sec, nil
}

func toolchainSourceArtifact(t *ToolchainRecord) (ToolchainArtifact, bool) {
	for _, a := range t.Artifacts {
		if a.Kind == "source" {
			return a, true
		}
	}
	return ToolchainArtifact{}, false
}

func listTrackedFiles(root string, exclude map[string]bool) ([]string, error) {
	cmd := exec.Command("git", "-C", root, "ls-files", "-z")
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("git ls-files: %w", err)
	}
	var files []string
	for _, f := range strings.Split(string(out), "\x00") {
		if f == "" {
			continue
		}
		if exclude[f] || excludedUnder(exclude, f) {
			continue
		}
		files = append(files, f)
	}
	sort.Strings(files)
	return files, nil
}

func excludedUnder(exclude map[string]bool, f string) bool {
	for dir := filepath.Dir(f); dir != "." && dir != "/" && dir != ""; dir = filepath.Dir(dir) {
		if exclude[dir+"/"] {
			return true
		}
	}
	return false
}

// hashTrackedFile applies the same fail-closed root/symlink policy used for
// embeds to every tracked source file before reading it, so a hostile
// repository cannot make the source SBOM follow a symlink outside the root.
func hashTrackedFile(root, rel string) (string, error) {
	if _, err := SafeEmbedPath(root, rel); err != nil {
		return "", fmt.Errorf("tracked file %s: %w", rel, err)
	}
	return hashFile(filepath.Join(root, filepath.FromSlash(rel)))
}

func hashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", fmt.Errorf("open %s: %w", path, err)
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", fmt.Errorf("hash %s: %w", path, err)
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}

func sha256Sum(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}

func moduleDisplayVersion(v string) string {
	if v == "" {
		return "(devel)"
	}
	return v
}

// consumptionSchema pins the generated consumption-manifest format consumed
// by issue 0018's release checks.
const consumptionSchema = 1

type consumptionDoc struct {
	Schema     int                   `json:"schema"`
	Generator  consumptionGenerator  `json:"generator"`
	Toolchain  consumptionToolchain  `json:"toolchain"`
	SourceSBOM consumptionFile       `json:"sourceSbom"`
	Notice     consumptionFile       `json:"notice"`
	Artifacts  []consumptionArtifact `json:"artifacts"`
}

type consumptionGenerator struct {
	Name       string `json:"name"`
	Version    string `json:"version"`
	Provenance string `json:"provenance"`
}

type consumptionToolchain struct {
	Version string `json:"version"`
	Record  string `json:"record"`
}

type consumptionFile struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
}

type consumptionArtifact struct {
	ID      string          `json:"id"`
	OS      string          `json:"os"`
	Arch    string          `json:"arch"`
	Package string          `json:"package"`
	Binary  consumptionFile `json:"binary"`
	SBOM    consumptionFile `json:"sbom"`
	Embeds  []string        `json:"embeddedFiles"`
}

// sbomEvidenceHash hashes an SPDX document with its creation timestamp
// stripped, so the consumption manifest stays stable across commits while
// still binding every other byte of the document.
func sbomEvidenceHash(doc []byte) (string, error) {
	if err := validateSPDXEnvelope(doc); err != nil {
		return "", err
	}
	norm, err := NormalizeSBOM(doc)
	if err != nil {
		return "", fmt.Errorf("normalize sbom: %w", err)
	}
	return sha256Sum(norm), nil
}

// validateSPDXEnvelope performs lightweight structural checks on an emitted
// SPDX document so a self-consistent consumption manifest can never bless
// malformed or non-SPDX evidence. It is intentionally not a full schema
// validation: byte-level integrity is enforced by the hash comparison.
func validateSPDXEnvelope(doc []byte) error {
	var env struct {
		SPDXVersion  string `json:"spdxVersion"`
		SPDXID       string `json:"SPDXID"`
		Name         string `json:"name"`
		CreationInfo struct {
			Created  string   `json:"created"`
			Creators []string `json:"creators"`
		} `json:"creationInfo"`
		Packages []struct {
			SPDXID string `json:"SPDXID"`
			Name   string `json:"name"`
		} `json:"packages"`
	}
	// Unknown top-level fields (dataLicense, documentNamespace, files,
	// relationships, ...) are permitted: only the envelope identity is
	// asserted here, byte-level integrity stays with the hash comparison.
	if err := json.Unmarshal(doc, &env); err != nil {
		return fmt.Errorf("not a valid emitted SPDX document: %w", err)
	}
	switch {
	case env.SPDXVersion != spdxVersion:
		return fmt.Errorf("unexpected spdxVersion %q", env.SPDXVersion)
	case env.SPDXID != "SPDXRef-DOCUMENT":
		return fmt.Errorf("missing or wrong document SPDXID %q", env.SPDXID)
	case env.Name == "":
		return fmt.Errorf("document name is empty")
	case len(env.CreationInfo.Creators) == 0:
		return fmt.Errorf("creationInfo has no creators")
	case len(env.Packages) == 0:
		return fmt.Errorf("document has no packages")
	}
	for _, p := range env.Packages {
		if p.SPDXID == "" || p.Name == "" {
			return fmt.Errorf("package with empty SPDXID or name")
		}
	}
	return nil
}

func renderConsumptionManifest(manifest *Manifest, toolchain *ToolchainRecord, inv *Inventory, binHashes map[string]string) ([]byte, error) {
	srcHash, err := sbomEvidenceHash(inv.Outputs[manifest.SBOMDir+"/source.spdx.json"])
	if err != nil {
		return nil, fmt.Errorf("hash source sbom: %w", err)
	}
	doc := consumptionDoc{
		Schema: consumptionSchema,
		Generator: consumptionGenerator{
			Name:       generatorName,
			Version:    GeneratorVersion,
			Provenance: "First-party project-owned Apache-2.0 tool pinned in docs/RELEASE-INVENTORY.md; source at cmd/domowik-sbom and internal/release.",
		},
		Toolchain:  consumptionToolchain{Version: toolchain.Version, Record: toolchainPath},
		SourceSBOM: consumptionFile{Path: manifest.SBOMDir + "/source.spdx.json", SHA256: srcHash},
		Notice:     consumptionFile{Path: manifest.Notice.Path, SHA256: sha256Sum(inv.Outputs[manifest.Notice.Path])},
	}
	for _, art := range manifest.Artifacts {
		id := art.ID
		embedPaths := make([]string, 0, len(inv.Embeds[id]))
		for _, ef := range inv.Embeds[id] {
			embedPaths = append(embedPaths, ef.Path)
		}
		sort.Strings(embedPaths)
		sbomHash, err := sbomEvidenceHash(inv.Outputs[manifest.SBOMDir+"/"+id+".spdx.json"])
		if err != nil {
			return nil, fmt.Errorf("artifact %s: hash sbom: %w", id, err)
		}
		doc.Artifacts = append(doc.Artifacts, consumptionArtifact{
			ID:      id,
			OS:      art.OS,
			Arch:    art.Arch,
			Package: art.Package,
			Binary: consumptionFile{
				Path:   manifest.ReleaseOutputDir + "/" + id,
				SHA256: binHashes[id],
			},
			SBOM: consumptionFile{
				Path:   manifest.SBOMDir + "/" + id + ".spdx.json",
				SHA256: sbomHash,
			},
			Embeds: embedPaths,
		})
	}
	out, err := json.MarshalIndent(doc, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encode consumption manifest: %w", err)
	}
	return append(out, '\n'), nil
}
