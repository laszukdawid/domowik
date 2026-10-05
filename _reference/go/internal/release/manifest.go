package release

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
)

// ManifestSchema is the only manifest schema version this generator accepts.
const ManifestSchema = 1

// Artifact declares one distributed binary target. All distributed
// artifacts are built statically with CGO_ENABLED=0; there is deliberately
// no switch. A future non-static target requires a deliberate decision and
// a manifest schema change, not a flag flip.
type Artifact struct {
	ID      string `json:"id"`
	OS      string `json:"os"`
	Arch    string `json:"arch"`
	Package string `json:"package"`
}

// Manifest is the evolving declarative source of truth for what a release
// distributes and where generated inventory artifacts live.
type Manifest struct {
	ManifestSchema int               `json:"manifestSchema"`
	SourceEpoch    SourceEpochPolicy `json:"sourceDateEpoch"`
	Artifacts      []Artifact        `json:"artifacts"`
	Notice         NoticeConfig      `json:"notice"`
	SBOMDir        string            `json:"sbomDir"`
	// ReleaseOutputDir receives the exact hashed binaries described by the
	// consumption manifest (typically a gitignored build directory).
	ReleaseOutputDir string `json:"releaseOutputDir"`
	// ConsumptionManifest locates the generated stable manifest consumed by
	// issue 0018's release checks.
	ConsumptionManifest OutputFileConfig `json:"consumptionManifest"`
	// Bundled declaratively lists every non-first-party file expected to be
	// embedded or otherwise distributed inside artifacts, per target. The
	// pipeline cross-checks this declaration against go:embed discovery and
	// fails closed on any difference; adding a bundled file therefore
	// requires updating the manifest AND the reviewed registry together.
	// Bundled is a REQUIRED explicit declaration; omitting the field fails
	// closed so future embed checks can never rely on a silently absent list.
	Bundled *[]BundledFile `json:"bundled"`
}

// BundledFile declares one expected bundled/embedded file and the artifact
// IDs whose binaries must contain it. Declarations are keyed by artifact ID,
// not os/arch, because multiple artifacts may share a platform while having
// different dependency closures and therefore different embed sets.
type BundledFile struct {
	Path string `json:"path"`
	// Artifacts lists artifact IDs that must include this file.
	Artifacts []string `json:"artifacts"`
}

// OutputFileConfig locates one generated output file.
type OutputFileConfig struct {
	Path string `json:"path"`
}

// ExpectedSourceEpochPolicy is the only epoch policy the pipeline accepts;
// its semantics are implemented by ResolveEpoch.
const ExpectedSourceEpochPolicy = "require-env-or-git-commit"

// SourceEpochPolicy records, declaratively, how creation timestamps are
// resolved. The value is validated against ExpectedSourceEpochPolicy so a
// silent change of policy semantics cannot slip through the manifest.
type SourceEpochPolicy struct {
	Policy string `json:"policy"`
}

// NoticeConfig locates the generated top-level NOTICE file.
type NoticeConfig struct {
	Path string `json:"path"`
}

// LoadManifest reads and validates the distribution manifest from an
// arbitrary path (used by tests and tooling); production loading goes
// through Generator.LoadInputs, which enforces repository-root containment.
func LoadManifest(path string) (*Manifest, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read manifest: %w", err)
	}
	m, err := parseManifest(data)
	if err != nil {
		return nil, fmt.Errorf("manifest %s: %w", path, err)
	}
	return m, nil
}

// parseManifest validates raw manifest bytes.
func parseManifest(data []byte) (*Manifest, error) {
	var m Manifest
	if err := strictUnmarshal(data, &m); err != nil {
		return nil, fmt.Errorf("parse manifest: %w", err)
	}
	if err := m.validate(); err != nil {
		return nil, fmt.Errorf("invalid manifest: %w", err)
	}
	return &m, nil
}

// artifactIDRE constrains artifact identifiers to safe single path elements:
// no separators, traversal, or shell-hostile characters.
var artifactIDRE = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._-]*$`)

func (m *Manifest) validate() error {
	if m.ManifestSchema != ManifestSchema {
		return fmt.Errorf("unsupported manifestSchema %d (want %d)", m.ManifestSchema, ManifestSchema)
	}
	if m.SourceEpoch.Policy != ExpectedSourceEpochPolicy {
		return fmt.Errorf("sourceDateEpoch.policy must be %q, got %q", ExpectedSourceEpochPolicy, m.SourceEpoch.Policy)
	}
	if len(m.Artifacts) == 0 {
		return fmt.Errorf("artifacts must not be empty")
	}
	seenID := map[string]bool{}
	seenTarget := map[string]bool{}
	for i, a := range m.Artifacts {
		switch {
		case a.ID == "":
			return fmt.Errorf("artifact[%d]: id is required", i)
		case !artifactIDRE.MatchString(a.ID):
			return fmt.Errorf("artifact[%d]: id %q must match %s (single safe path element)", i, a.ID, artifactIDRE)
		case seenID[a.ID]:
			return fmt.Errorf("artifact[%d]: duplicate id %q", i, a.ID)
		case a.OS == "" || a.Arch == "":
			return fmt.Errorf("artifact %q: os and arch are required", a.ID)
		}
		target := fmt.Sprintf("%s/%s/%s", a.OS, a.Arch, a.Package)
		if seenTarget[target] {
			return fmt.Errorf("artifact %q: duplicate build target %s", a.ID, target)
		}
		seenTarget[target] = true
		if err := validatePackagePattern(a.Package); err != nil {
			return fmt.Errorf("artifact %q: package: %w", a.ID, err)
		}
		seenID[a.ID] = true
	}
	if m.SBOMDir == "" {
		return fmt.Errorf("sbomDir is required")
	}
	if err := safeRelPath(m.SBOMDir); err != nil {
		return fmt.Errorf("sbomDir: %w", err)
	}
	if m.Notice.Path == "" {
		return fmt.Errorf("notice.path is required")
	}
	if err := safeRelPath(m.Notice.Path); err != nil {
		return fmt.Errorf("notice.path: %w", err)
	}
	if m.ReleaseOutputDir == "" {
		return fmt.Errorf("releaseOutputDir is required")
	}
	if err := safeRelPath(m.ReleaseOutputDir); err != nil {
		return fmt.Errorf("releaseOutputDir: %w", err)
	}
	if m.ConsumptionManifest.Path == "" {
		return fmt.Errorf("consumptionManifest.path is required")
	}
	if err := safeRelPath(m.ConsumptionManifest.Path); err != nil {
		return fmt.Errorf("consumptionManifest.path: %w", err)
	}
	artifactIDs := map[string]bool{}
	for _, a := range m.Artifacts {
		artifactIDs[a.ID] = true
	}
	// Output-path integrity: enumerate EVERY concrete generated location,
	// then reject exact duplicates and ancestor/descendant overlaps (a file
	// output must never be nested inside another output or vice versa), and
	// never allow outputs over declarative policy inputs.
	outputs := map[string]string{} // path -> declaring output kind
	reserve := func(kind, p string) error {
		clean := filepath.ToSlash(filepath.Clean(p))
		if err := safeRelPath(clean); err != nil {
			return fmt.Errorf("%s path %q: %w", kind, p, err)
		}
		for prevPath, prevKind := range outputs {
			if prevPath == clean {
				return fmt.Errorf("%s path %q collides with the %s output", kind, p, prevKind)
			}
			if strings.HasPrefix(clean, prevPath+"/") || strings.HasPrefix(prevPath, clean+"/") {
				return fmt.Errorf("%s path %q overlaps the %s output %q", kind, p, prevKind, prevPath)
			}
		}
		for _, forbidden := range []string{manifestPath, registryPath, toolchainPath, toolsRecordPath, "docs/third-party/", "docs/"} {
			prot := strings.TrimSuffix(forbidden, "/")
			// Bidirectional: outputs may not live inside a protected path,
			// and no protected path may live inside an output tree either.
			if clean == prot || strings.HasPrefix(clean, prot+"/") ||
				strings.HasPrefix(prot, clean+"/") {
				return fmt.Errorf("%s path %q overlaps protected input %q; generated outputs must stay out of docs/ and declarative inputs in both directions", kind, p, forbidden)
			}
		}
		outputs[clean] = kind
		return nil
	}
	if err := reserve("source sbom", m.SBOMDir+"/source.spdx.json"); err != nil {
		return err
	}
	for _, a := range m.Artifacts {
		if err := reserve("sbom artifact output", m.SBOMDir+"/"+a.ID+".spdx.json"); err != nil {
			return err
		}
		if err := reserve("binary artifact output", m.ReleaseOutputDir+"/"+a.ID); err != nil {
			return err
		}
	}
	if err := reserve("notice", m.Notice.Path); err != nil {
		return err
	}
	if err := reserve("consumptionManifest", m.ConsumptionManifest.Path); err != nil {
		return err
	}

	// Directory-ownership rules beyond pairwise file overlaps: cleanup and
	// regeneration treat sbomDir and releaseOutputDir as disposable trees,
	// so unrelated outputs must never live inside them, and the two trees
	// must never nest within each other.
	inside := func(child, dir string) bool {
		dir = strings.TrimSuffix(dir, "/")
		return child == dir || strings.HasPrefix(child, dir+"/")
	}
	type located struct{ kind, path string }
	for _, d := range []located{
		{"sbomDir", m.SBOMDir},
		{"releaseOutputDir", m.ReleaseOutputDir},
	} {
		for _, o := range []located{
			{"notice", m.Notice.Path},
			{"consumptionManifest", m.ConsumptionManifest.Path},
		} {
			if o.kind == "consumptionManifest" && d.kind == "releaseOutputDir" {
				continue // deliberate default placement inside the evidence tree
			}
			if inside(o.path, d.path) || inside(d.path, o.path) {
				return fmt.Errorf("%s %q must not nest with the %s tree %q; disposable release trees may swallow unrelated outputs during cleanup",
					o.kind, o.path, d.kind, d.path)
			}
		}
	}
	if inside(m.SBOMDir, m.ReleaseOutputDir) || inside(m.ReleaseOutputDir, m.SBOMDir) {
		return fmt.Errorf("sbomDir %q and releaseOutputDir %q must not nest; disposable release trees may swallow each other during cleanup",
			m.SBOMDir, m.ReleaseOutputDir)
	}

	// The two disposable TREES must also stay clear of protected policy
	// inputs in both directions: a tree at or above docs/, or containing
	// docs/third-party, would put generated bytes beside (or over) reviewed
	// provenance documents.
	type dirLoc struct{ kind, path string }
	for _, d := range []dirLoc{{"sbomDir", m.SBOMDir}, {"releaseOutputDir", m.ReleaseOutputDir}} {
		dclean := strings.TrimSuffix(filepath.ToSlash(filepath.Clean(d.path)), "/")
		for _, forbidden := range []string{manifestPath, registryPath, toolchainPath, toolsRecordPath, "docs/third-party/", "docs/"} {
			prot := strings.TrimSuffix(forbidden, "/")
			if dclean == prot || strings.HasPrefix(dclean, prot+"/") || strings.HasPrefix(prot, dclean+"/") {
				return fmt.Errorf("%s %q overlaps protected policy path %q; disposable release trees must never contain or sit on declarative inputs",
					d.kind, d.path, forbidden)
			}
		}
	}

	if m.Bundled == nil {
		return fmt.Errorf("bundled must be present as an explicit declaration (use [] for none)")
	}
	seenBundled := map[string]bool{}
	for i, b := range *m.Bundled {
		switch {
		case b.Path == "":
			return fmt.Errorf("bundled[%d]: path is required", i)
		case seenBundled[b.Path]:
			return fmt.Errorf("bundled[%d]: duplicate path %q", i, b.Path)
		case len(b.Artifacts) == 0:
			return fmt.Errorf("bundled %q: artifacts must not be empty", b.Path)
		}
		if err := safeRelPath(b.Path); err != nil {
			return fmt.Errorf("bundled %q: %w", b.Path, err)
		}
		seenRef := map[string]bool{}
		for _, ref := range b.Artifacts {
			if !artifactIDs[ref] {
				return fmt.Errorf("bundled %q: references undeclared artifact %q", b.Path, ref)
			}
			if seenRef[ref] {
				return fmt.Errorf("bundled %q: duplicate artifact reference %q", b.Path, ref)
			}
			seenRef[ref] = true
		}
		seenBundled[b.Path] = true
	}
	return nil
}

// DeclaredForArtifact returns the declared bundled paths that must appear in
// the named artifact's binary.
func (m *Manifest) DeclaredForArtifact(id string) []string {
	var out []string
	if m.Bundled != nil {
		for _, b := range *m.Bundled {
			for _, ref := range b.Artifacts {
				if ref == id {
					out = append(out, b.Path)
					break
				}
			}
		}
	}
	sort.Strings(out)
	return out
}

// rejectUpwardElements inspects the ORIGINAL (unnormalized) path and rejects
// any ".." element, so traversal cannot be laundered by filepath.Clean.
func rejectUpwardElements(p string) error {
	for _, elem := range strings.Split(filepath.ToSlash(p), "/") {
		if elem == ".." {
			return fmt.Errorf("path escapes upward: %q", p)
		}
	}
	return nil
}

// safeRelPath rejects empty, dot, absolute, and escaping relative paths so
// that declared locations can never point outside the repository root.
//
// Symlink policy: lexical checks happen here at load time. Because declared
// paths are always interpreted against the live repository root, symlinked
// parent components are rejected at write time by resolveUnderRoot, which
// resolves each output's parent chain and requires it to stay under the
// real (symlink-resolved) repository root.
func safeRelPath(p string) error {
	if p == "" {
		return fmt.Errorf("path is empty")
	}
	if filepath.IsAbs(p) {
		return fmt.Errorf("path must be relative: %q", p)
	}
	if err := rejectUpwardElements(p); err != nil {
		return err
	}
	clean := filepath.Clean(filepath.ToSlash(p))
	if clean == "." {
		return fmt.Errorf("path normalizes to the repository root itself")
	}
	return nil
}

// validatePackagePattern constrains declared build targets to explicit rooted
// package-directory patterns ("./cmd/foo"). Wildcards ("./...", "*"), dot-dot
// anywhere in the original path, absolute paths, and empty values are
// rejected before the value is ever joined with a working directory.
func validatePackagePattern(p string) error {
	if p == "" {
		return fmt.Errorf("is empty")
	}
	if !strings.HasPrefix(p, "./") {
		return fmt.Errorf("must be a rooted ./ pattern, got %q", p)
	}
	if err := rejectUpwardElements(p); err != nil {
		return err
	}
	for _, elem := range strings.Split(strings.TrimPrefix(filepath.ToSlash(p), "./"), "/") {
		if strings.ContainsAny(elem, "*?[]") || elem == "..." || elem == ".." || elem == "." {
			return fmt.Errorf("wildcards and traversal are not allowed in package patterns: %q", p)
		}
	}
	return nil
}

func strictUnmarshal(data []byte, v any) error {
	dec := json.NewDecoder(bytes.NewReader(data))
	dec.DisallowUnknownFields()
	if err := dec.Decode(v); err != nil {
		return err
	}
	// Fail closed on trailing JSON values, garbage, or a second document.
	if err := dec.Decode(&struct{}{}); err != io.EOF {
		if err == nil {
			return fmt.Errorf("trailing JSON value after the document")
		}
		return fmt.Errorf("trailing data after the document: %w", err)
	}
	return nil
}
