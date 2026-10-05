package release

import (
	"context"
	"encoding/json"
	"errors"
	"os"
	"path/filepath"
	"runtime/debug"
	"sort"
	"strings"
	"testing"
	"time"
)

// ---- main module selection ----

func TestSelectMainModule(t *testing.T) {
	if _, err := selectMainModule(nil); err == nil {
		t.Error("zero mains: expected failure")
	}
	two := []Module{{Path: "a", Main: true}, {Path: "b", Main: true}}
	if _, err := selectMainModule(two); err == nil {
		t.Error("multiple mains: expected failure")
	}
	m, err := selectMainModule([]Module{{Path: "z.example", Version: "v1.0.0"}, {Path: "main.example", Main: true}})
	if err != nil || m.Path != "main.example" {
		t.Fatalf("expected main by flag, got %+v err %v", m, err)
	}
}

// ---- module-set cross-check with replacements ----

func TestCrossCheckModulesReplacement(t *testing.T) {
	modules := []Module{
		{Path: "main.example", Main: true},
		{Path: "orig.example", Version: "v1.0.0", ReplacedBy: "fork.example", ReplacedVersion: "v2.0.0"},
	}
	lines := []string{"main.example", "orig.example v1.0.0 => fork.example v2.0.0"}
	if err := CrossCheckModules(modules, lines); err != nil {
		t.Fatalf("matching replacement lines rejected: %v", err)
	}
	for _, broken := range [][]string{
		{"main.example", "orig.example v1.0.0 => fork.example v9.9.9"},
		{"main.example", "orig.example v1.0.0 => other.example v2.0.0"},
		{"main.example", "orig.example v1.0.0"},
	} {
		if err := CrossCheckModules(modules, broken); err == nil {
			t.Errorf("lines %v: expected mismatch failure", broken)
		}
	}
}

// ---- build info parsing and extraction ----

func TestReadBuildInfoRejectsGarbage(t *testing.T) {
	p := filepath.Join(t.TempDir(), "bin")
	if err := os.WriteFile(p, []byte("not a go binary"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, err := ReadBuildInfo(p); err == nil {
		t.Fatal("garbage binary: expected failure")
	}
}

func TestComponentsFromBuildInfoReplaceAndConflicts(t *testing.T) {
	bi := &debug.BuildInfo{
		GoVersion: "go1.27.0",
		Main:      debug.Module{Path: "main.example", Version: "(devel)"},
		Deps: []*debug.Module{
			{Path: "orig.example", Version: "v1.0.0", Replace: &debug.Module{Path: "fork.example", Version: "v2.0.0"}},
		},
	}
	comps, err := ComponentsFromBuildInfo(bi)
	if err != nil {
		t.Fatal(err)
	}
	found := false
	for _, c := range comps {
		if c.Key == "fork.example" {
			found = true
			if c.Version != "v2.0.0" {
				t.Fatalf("replacement version lost: %+v", c)
			}
		}
		if c.Key == "orig.example" {
			t.Fatal("original path must be superseded by effective identity")
		}
	}
	if !found {
		t.Fatal("effective replacement component missing")
	}

	conflict := &debug.BuildInfo{
		GoVersion: "go1.27.0",
		Main:      debug.Module{Path: "main.example"},
		Deps: []*debug.Module{
			{Path: "dup.example", Version: "v1.0.0"},
			{Path: "x.example", Version: "v0.1.0"},
			{Path: "dup.example", Version: "v9.9.9"},
		},
	}
	if _, err := ComponentsFromBuildInfo(conflict); err == nil {
		t.Fatal("conflicting duplicate identity: expected failure")
	}

	empty := &debug.BuildInfo{}
	if _, err := ComponentsFromBuildInfo(empty); err == nil {
		t.Fatal("truncated build info: expected failure")
	}
}

// ---- exact binary-vs-source-graph comparison ----

func TestAssertBinMatchesSourceGraph(t *testing.T) {
	tc := &ToolchainRecord{Version: "go1.27.0"}
	modules := []Module{
		{Path: "main.example", Main: true},
		{Path: "dep.example", Version: "v1.0.0"},
	}
	bin := []Component{
		{Key: "main.example", Kind: KindModule},
		{Key: "dep.example", Kind: KindModule, Version: "v1.0.0"},
		{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.27.0"},
	}
	if err := assertBinMatchesSourceGraph(bin, modules, tc); err != nil {
		t.Fatalf("equal sets rejected: %v", err)
	}
	extra := append(append([]Component{}, bin...), Component{Key: "ghost.example", Kind: KindModule, Version: "v1.0.0"})
	if err := assertBinMatchesSourceGraph(extra, modules, tc); err == nil {
		t.Error("binary extra module: expected failure")
	}
	omitted := bin[:2]
	if err := assertBinMatchesSourceGraph(omitted, modules, tc); err == nil {
		t.Error("source module omitted from binary: expected failure")
	}
	wrongVer := append(append([]Component{}, bin[:1]...), Component{Key: "dep.example", Kind: KindModule, Version: "v2.0.0"}, bin[2])
	if err := assertBinMatchesSourceGraph(wrongVer, modules, tc); err == nil {
		t.Error("version conflict: expected failure")
	}
	wrongTC := append(append([]Component{}, bin[:2]...), Component{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.26.0"})
	if err := assertBinMatchesSourceGraph(wrongTC, modules, tc); err == nil {
		t.Error("toolchain mismatch: expected failure")
	}
	noTC := bin[:2]
	if err := assertBinMatchesSourceGraph(noTC, modules, tc); err == nil {
		t.Error("missing toolchain component: expected failure")
	}
}

// ---- hostile/symlink path handling ----

func TestSafeEmbedPathRejectsSymlinkAndEscape(t *testing.T) {
	root := t.TempDir()
	if err := writeFile(root, "ok/data.txt", []byte("x")); err != nil {
		t.Fatal(err)
	}
	outside := filepath.Join(t.TempDir(), "outside.txt")
	if err := os.WriteFile(outside, []byte("secret"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "link.txt")); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(filepath.Join(root, "dirlink-target"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := writeFile(root, "dirlink-target/f.txt", []byte("s")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(filepath.Join(root, "dirlink-target"), filepath.Join(root, "sub")); err != nil {
		t.Fatal(err)
	}
	for _, p := range []string{"../escape", "link.txt", filepath.ToSlash(outside), "sub/../f.txt"} {
		full := p
		if p == "sub/../f.txt" {
			continue
		}
		if _, err := SafeEmbedPath(root, full); err == nil && p != filepath.ToSlash(outside) {
			t.Errorf("%q: expected rejection", p)
		}
	}
	if _, err := SafeEmbedPath(root, "../escape"); err == nil {
		t.Error("dotdot: expected rejection")
	}
	if _, err := SafeEmbedPath(root, "link.txt"); err == nil {
		t.Error("symlink entry: expected rejection")
	}
	if _, err := SafeEmbedPath(root, "ok/data.txt"); err != nil {
		t.Errorf("regular in-root file rejected: %v", err)
	}
}

func TestRootSafeIORejectsSymlinkParentsAndTargets(t *testing.T) {
	root := t.TempDir()
	outside := t.TempDir()
	if err := os.WriteFile(filepath.Join(outside, "victim.txt"), []byte("v"), 0o644); err != nil {
		t.Fatal(err)
	}
	// Symlinked parent directory pointing outside.
	if err := os.Symlink(outside, filepath.Join(root, "esc")); err != nil {
		t.Fatal(err)
	}
	if _, err := readFileUnderRoot(root, "esc/victim.txt"); err == nil {
		t.Error("read via outside symlink parent: expected rejection")
	}
	if err := writeFileUnderRoot(root, "esc/victim.txt", []byte("x")); err == nil {
		t.Error("write via outside symlink parent: expected rejection")
	}
	// Existing final symlink target must not be followed on write or read.
	if err := writeFile(root, "real.txt", []byte("real")); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(outside, filepath.Join(root, "jump.txt")); err != nil {
		t.Fatal(err)
	}
	if err := writeFileUnderRoot(root, "jump.txt", []byte("hijack")); err == nil {
		t.Error("write over symlink target: expected rejection")
	}
	if b, _ := os.ReadFile(filepath.Join(outside, "victim.txt")); string(b) != "v" {
		t.Error("outside victim file was modified through symlink")
	}
	if _, err := readFileUnderRoot(root, "jump.txt"); err == nil {
		t.Error("read of symlinked output path: expected rejection")
	}
	// Reads must not materialize directories.
	if _, err := readFileUnderRoot(root, "no/such/file.txt"); err == nil {
		t.Error("missing read path: expected rejection")
	}
	if fi, err := os.Lstat(filepath.Join(root, "no")); err == nil && fi.IsDir() {
		t.Error("read created directories as a side effect")
	}
}

func TestVerifyNoticeFilesSymlinkEscape(t *testing.T) {
	dir := t.TempDir()
	outside := t.TempDir()
	realNotice := filepath.Join(outside, "GO-LICENSE")
	if err := os.WriteFile(realNotice, []byte("license text"), 0o644); err != nil {
		t.Fatal(err)
	}
	r, err := LoadRegistry(writeTemp(t, "l.json",
		strings.Replace(goodRegistry, strings.Repeat("a", 64), sha256Hex("license text"), 1)))
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(dir, "notices"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(realNotice, filepath.Join(dir, "notices", "GO-LICENSE")); err != nil {
		t.Fatal(err)
	}
	if err := r.VerifyNoticeFiles(dir); err == nil {
		t.Fatal("notice file behind symlink: expected rejection")
	}
}

// ---- toolchain/registry integrity cross-check ----

func TestCrossCheckToolchain(t *testing.T) {
	reg, err := LoadRegistry(writeTemp(t, "l.json", goodRegistry))
	if err != nil {
		t.Fatal(err)
	}
	rec, err := LoadToolchain(writeTemp(t, "t.json", goodToolchain))
	if err != nil {
		t.Fatal(err)
	}
	// Fixture registry uses a different license id than the fixture record;
	// align them to test the happy path first.
	reg.Entries[1].SPDXLicense = rec.License.SPDXID
	reg.Entries[1].SourceURL = rec.License.URL
	if err := CrossCheckToolchain(reg, rec); err != nil {
		t.Fatalf("aligned records rejected: %v", err)
	}
	rec.Component = "someone.else/toolchain"
	if err := CrossCheckToolchain(reg, rec); err == nil {
		t.Error("foreign component key: expected failure")
	}
	rec.Component = ToolchainKey
	rec.License.SPDXID = "MIT"
	if err := CrossCheckToolchain(reg, rec); err == nil {
		t.Error("license disagreement: expected failure")
	}
	rec.License.SPDXID = reg.Entries[1].SPDXLicense
	rec.License.URL = "https://evil.example/LICENSE"
	if err := CrossCheckToolchain(reg, rec); err == nil {
		t.Error("url disagreement: expected failure")
	}
}

// ---- bundled declaration cross-check ----

func TestCrossCheckBundled(t *testing.T) {
	if err := crossCheckBundled(nil, nil); err != nil {
		t.Fatalf("empty baseline rejected: %v", err)
	}
	if err := crossCheckBundled([]string{"a.bin"}, []string{"a.bin"}); err != nil {
		t.Fatalf("equal lists rejected: %v", err)
	}
	if err := crossCheckBundled(nil, []string{"surprise.bin"}); err == nil {
		t.Error("undeclared discovery: expected failure")
	}
	if err := crossCheckBundled([]string{"gone.bin"}, nil); err == nil {
		t.Error("stale declaration: expected failure")
	}
	if err := crossCheckBundled([]string{"a.bin", "a.bin"}, []string{"a.bin"}); err == nil {
		t.Error("duplicate declaration: expected failure")
	}
}

func TestManifestDeclaredFor(t *testing.T) {
	doc := strings.Replace(goodManifest, `"bundled": []`,
		`"bundled": [{"path": "data/x.txt", "artifacts": ["domowik-linux-amd64"]}]`, 1)
	m, err := LoadManifest(writeTemp(t, "m.json", doc))
	if err != nil {
		t.Fatal(err)
	}
	if got := m.DeclaredForArtifact("domowik-linux-amd64"); len(got) != 1 || got[0] != "data/x.txt" {
		t.Fatalf("artifact-scoped lookup failed: %v", got)
	}
	if got := m.DeclaredForArtifact("domowik-linux-arm64"); len(got) != 0 {
		t.Fatalf("other artifact returned declarations: %v", got)
	}
}

// ---- SPDX determinism and normalization invariants ----

func sampleSBOMInput(created time.Time) SBOMInput {
	return SBOMInput{
		Name:    "test-doc",
		Created: created,
		Packages: []SPDXPackage{{
			Name: "pkg", SPDXID: "SPDXRef-Package-pkg",
			DownloadLocation: "NOASSERTION",
			LicenseConcluded: "MIT", LicenseDeclared: "MIT",
			CopyrightText: "NOASSERTION",
		}},
		Relationships: []SPDXRelationship{{
			SpdxElementID: "SPDXRef-DOCUMENT", RelationshipType: "DESCRIBES",
			RelatedSpdxElement: "SPDXRef-Package-pkg",
		}},
	}
}

func TestRenderSBOMDeterministic(t *testing.T) {
	a, err := RenderSBOM(sampleSBOMInput(time.Unix(100, 0)))
	if err != nil {
		t.Fatal(err)
	}
	b, err := RenderSBOM(sampleSBOMInput(time.Unix(100, 0)))
	if err != nil {
		t.Fatal(err)
	}
	if string(a) != string(b) {
		t.Fatal("same epoch produced different bytes")
	}
	c, err := RenderSBOM(sampleSBOMInput(time.Unix(200, 0)))
	if err != nil {
		t.Fatal(err)
	}
	if string(a) == string(c) {
		t.Fatal("different epochs produced identical bytes; created must vary")
	}
	na, err := NormalizeSBOM(a)
	if err != nil {
		t.Fatal(err)
	}
	nc, err := NormalizeSBOM(c)
	if err != nil {
		t.Fatal(err)
	}
	if string(na) != string(nc) {
		t.Fatal("normalized documents differ beyond the creation timestamp")
	}
	var doc struct {
		DocumentNamespace string `json:"documentNamespace"`
	}
	if err := json.Unmarshal(c, &doc); err != nil {
		t.Fatal(err)
	}
	if doc.DocumentNamespace == "" || !strings.HasPrefix(doc.DocumentNamespace, namespaceBase) {
		t.Fatalf("unexpected namespace %q", doc.DocumentNamespace)
	}
}

func TestNormalizeSBOMRejectsMissingCreated(t *testing.T) {
	if _, err := NormalizeSBOM([]byte(`{"creationInfo":{}}`)); err == nil {
		t.Fatal("missing created: expected rejection")
	}
	if _, err := NormalizeSBOM([]byte(`{}`)); err == nil {
		t.Fatal("missing creationInfo: expected rejection")
	}
}

func TestRenderNoticeDeterministic(t *testing.T) {
	dir := t.TempDir()
	h := sha256Hex("license text")
	r, err := LoadRegistry(writeTemp(t, "l.json",
		strings.Replace(goodRegistry, strings.Repeat("a", 64), h, 1)))
	if err != nil {
		t.Fatal(err)
	}
	if err := writeFile(dir, "notices/GO-LICENSE", []byte("license text")); err != nil {
		t.Fatal(err)
	}
	read := readFileUnderRootFn(dir)
	n1, err := RenderNotice("Domowik", r, read)
	if err != nil {
		t.Fatal(err)
	}
	n2, err := RenderNotice("Domowik", r, read)
	if err != nil {
		t.Fatal(err)
	}
	if string(n1) != string(n2) {
		t.Fatal("NOTICE rendering is not deterministic")
	}
	if !strings.Contains(string(n1), "----- golang.org/toolchain (BSD-3-Clause) -----") ||
		!strings.Contains(string(n1), "license text") {
		t.Fatalf("NOTICE lacks verbatim third-party section:\n%s", n1)
	}
}

// ---- epoch resolution ----

func TestResolveEpoch(t *testing.T) {
	getenv := func(k string) (string, bool) { return "", false }
	head := func() (int64, error) { return 12345, nil }
	ts, src, err := ResolveEpoch(getenv, head)
	if err != nil || src != EpochFromHead || ts.Unix() != 12345 {
		t.Fatalf("fallback derivation wrong: %v %v %v", ts, src, err)
	}
	env := func(k string) (string, bool) {
		if k == "SOURCE_DATE_EPOCH" {
			return "42", true
		}
		return "", false
	}
	ts, src, err = ResolveEpoch(env, head)
	if err != nil || src != EpochFromEnv || ts.Unix() != 42 {
		t.Fatalf("env derivation wrong: %v %v %v", ts, src, err)
	}
	bad := func(k string) (string, bool) {
		if k == "SOURCE_DATE_EPOCH" {
			return "not-a-number", true
		}
		return "", false
	}
	if _, _, err := ResolveEpoch(bad, head); err == nil {
		t.Fatal("invalid SOURCE_DATE_EPOCH must fail closed")
	}
	negative := func(k string) (string, bool) {
		if k == "SOURCE_DATE_EPOCH" {
			return "-5", true
		}
		return "", false
	}
	if _, _, err := ResolveEpoch(negative, head); err == nil {
		t.Fatal("negative SOURCE_DATE_EPOCH must fail closed")
	}
	headErr := func() (int64, error) { return 0, errors.New("no git") }
	if _, _, err := ResolveEpoch(getenv, headErr); err == nil {
		t.Fatal("head derivation failure must propagate")
	}
}

// ---- bundled target-set validation ----

func TestManifestBundledTargetValidation(t *testing.T) {
	withBundled := func(bundled string) string {
		doc := strings.Replace(goodManifest, `"bundled": []`, `"bundled": [`+bundled+`]`, 1)
		if !strings.Contains(doc, "bundled") {
			t.Fatal("fixture lacks bundled field")
		}
		return doc
	}
	ok := withBundled(`{"path": "d/x.txt", "artifacts": ["domowik-linux-amd64"]}`)
	if _, err := LoadManifest(writeTemp(t, "m.json", ok)); err != nil {
		t.Fatalf("valid bundled row rejected: %v", err)
	}
	for name, row := range map[string]string{
		"unknown artifact":    `{"path": "d/x.txt", "artifacts": ["domowik-darwin-arm64"]}`,
		"duplicate reference": `{"path": "d/x.txt", "artifacts": ["domowik-linux-amd64", "domowik-linux-amd64"]}`,
		"empty artifacts":     `{"path": "d/x.txt", "artifacts": []}`,
	} {
		if _, err := LoadManifest(writeTemp(t, "m.json", withBundled(row))); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
	dupPath := withBundled(`{"path": "d/x.txt", "artifacts": ["domowik-linux-amd64"]}, {"path": "d/x.txt", "artifacts": ["domowik-linux-arm64"]}`)
	if _, err := LoadManifest(writeTemp(t, "m.json", dupPath)); err == nil {
		t.Error("duplicate bundled path: expected rejection")
	}
}

// ---- baseline plan probe: exercises the real stdlib-only pipeline ----

func TestBaselinePlanSucceeds(t *testing.T) {
	g := &Generator{Root: "../.."}
	ctx := context.Background()
	inv, err := g.Plan(ctx)
	if err != nil {
		t.Fatalf("baseline plan failed: %v", err)
	}
	foundMain, foundTool := false, false
	for _, c := range inv.Components {
		switch c.Key {
		case "domowik.dev/domowik":
			foundMain = true
		case ToolchainKey:
			foundTool = true
		}
	}
	if !foundMain || !foundTool {
		t.Fatalf("baseline components incomplete: %+v", inv.Components)
	}
	if len(inv.Binaries) != 2 {
		t.Fatalf("expected 2 built binaries, got %d", len(inv.Binaries))
	}
	for id, b := range inv.Binaries {
		sum := sha256Sum(b)
		if sum == "" || len(sum) != 64 {
			t.Fatalf("artifact %s: bad hash %q", id, sum)
		}
	}
	if _, ok := inv.Outputs["release/sbom/source.spdx.json"]; !ok {
		t.Fatal("source SBOM missing from outputs")
	}
	if _, ok := inv.Evidence["build/release/manifest.json"]; !ok {
		t.Fatal("consumption manifest missing from evidence")
	}
}

func TestComponentsFromBuildInfoRejectsReplacedMain(t *testing.T) {
	bi := &debug.BuildInfo{
		GoVersion: "go1.27.0",
		Main: debug.Module{
			Path:    "main.example",
			Version: "(devel)",
			Replace: &debug.Module{Path: "evil.example", Version: "v0.0.1"},
		},
	}
	if _, err := ComponentsFromBuildInfo(bi); err == nil {
		t.Fatal("replaced main module: expected explicit rejection")
	}
	bi2 := &debug.BuildInfo{
		GoVersion: "go1.27.0",
		Main:      debug.Module{Path: "main.example"},
		Deps: []*debug.Module{
			{Path: "orig.example", Version: "v1.0.0", Replace: &debug.Module{Path: "fork.example", Version: "v2.0.0"}},
		},
	}
	comps, err := ComponentsFromBuildInfo(bi2)
	if err != nil {
		t.Fatalf("dep replacement must stay supported: %v", err)
	}
	found := false
	for _, c := range comps {
		if c.Key == "fork.example" && c.Version == "v2.0.0" {
			found = true
		}
		if c.Key == "orig.example" {
			t.Fatal("replaced dep identity leaked into components")
		}
	}
	if !found {
		t.Fatal("effective replacement identity missing")
	}
}

func TestAssertBuildMetadataRejectsReplacedMain(t *testing.T) {
	tc := &ToolchainRecord{Version: "go1.27.0"}
	mainMod := Module{Path: "main.example", Main: true}
	bi := &debug.BuildInfo{
		GoVersion: "go1.27.0",
		Main: debug.Module{
			Path:    "main.example",
			Replace: &debug.Module{Path: "other.example"},
		},
		Settings: []debug.BuildSetting{
			{Key: "GOOS", Value: "linux"}, {Key: "GOARCH", Value: "amd64"},
			{Key: "CGO_ENABLED", Value: "0"}, {Key: "-trimpath", Value: "true"},
		},
	}
	art := Artifact{ID: "x", OS: "linux", Arch: "amd64", Package: "./cmd/x"}
	if err := assertBuildMetadata(bi, art, "0", mainMod, tc); err == nil {
		t.Fatal("assertBuildMetadata must reject a replaced main module")
	}
}

func TestCheckTargetReviewed(t *testing.T) {
	tc := &ToolchainRecord{
		Version:          "go1.27.0",
		SupportedTargets: []string{"linux/amd64"},
		Artifacts: []ToolchainArtifact{
			{Filename: "go1.27.0.linux-amd64.tar.gz", Kind: "archive", OS: "linux", Arch: "amd64", URL: "https://go.dev/dl/go1.27.0.linux-amd64.tar.gz", SHA256: strings.Repeat("b6", 32), SizeBytes: 1},
		},
	}
	art := Artifact{ID: "a", OS: "linux", Arch: "amd64"}
	if err := checkTargetReviewed(tc, art); err != nil {
		t.Fatalf("covered target rejected: %v", err)
	}
	uncovered := Artifact{ID: "b", OS: "linux", Arch: "arm64"}
	if err := checkTargetReviewed(tc, uncovered); err == nil || !strings.Contains(err.Error(), "supportedTargets") {
		t.Fatalf("missing supportedTargets membership not caught: %v", err)
	}
	noArchive := &ToolchainRecord{Version: tc.Version, SupportedTargets: []string{"linux/arm64"}}
	if err := checkTargetReviewed(noArchive, uncovered); err == nil || !strings.Contains(err.Error(), "archive row") {
		t.Fatalf("missing archive row not caught: %v", err)
	}
}

func TestSPDXEmbeddedFilePathsFiltering(t *testing.T) {
	doc := `{
	  "packages":[{"SPDXID":"SPDXRef-Package-bin"}],
	  "files":[
	    {"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"},
	    {"fileName":"./dup/c.txt","SPDXID":"SPDXRef-File-3"}
	  ],
	  "relationships":[
	    {"spdxElementId":"SPDXRef-DOCUMENT","relationshipType":"DESCRIBES","relatedSpdxElement":"SPDXRef-Package-bin"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-1"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-3"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-3"}
	  ]
	}`
	// duplicate CONTAINS for File-3 must fail closed
	if _, err := SPDXEmbeddedFilePaths([]byte(doc)); err == nil {
		t.Fatal("duplicate/unknown CONTAINS edges accepted")
	}
	okDoc := strings.Replace(doc,
		`{"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-3"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-3"}`,
		`{"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-3"}`, 1)
	paths, err := SPDXEmbeddedFilePaths([]byte(okDoc))
	if err != nil {
		t.Fatal(err)
	}
	if strings.Join(paths, ",") != "data/a.txt,dup/c.txt" {
		t.Fatalf("relationship filtering wrong: %v", paths)
	}
}

func TestValidateConsumptionStructure(t *testing.T) {
	g := &Generator{Root: "../.."}
	manifest, _, toolchain, err := g.LoadInputs()
	if err != nil {
		t.Fatal(err)
	}
	good := consumptionDoc{
		Schema: consumptionSchema,
		Generator: consumptionGenerator{
			Name:       generatorName,
			Version:    GeneratorVersion,
			Provenance: "test fixture",
		},
		Toolchain:  consumptionToolchain{Version: toolchain.Version, Record: toolchainPath},
		SourceSBOM: consumptionFile{Path: manifest.SBOMDir + "/source.spdx.json"},
		Notice:     consumptionFile{Path: manifest.Notice.Path},
	}
	for i, art := range manifest.Artifacts {
		good.Artifacts = append(good.Artifacts, consumptionArtifact{
			ID:      art.ID,
			OS:      art.OS,
			Arch:    art.Arch,
			Package: art.Package,
			Binary:  consumptionFile{Path: manifest.ReleaseOutputDir + "/" + art.ID},
			SBOM:    consumptionFile{Path: manifest.SBOMDir + "/" + art.ID + ".spdx.json"},
			Embeds:  []string{},
		})
		if i >= 1 {
			break // one representative row suffices for structure tests
		}
	}
	if err := validateConsumptionStructure(&good, manifest, toolchain); err != nil {
		t.Fatalf("synthetic valid document rejected: %v", err)
	}

	dup := good
	if len(dup.Artifacts) >= 2 {
		dup.Artifacts[1] = dup.Artifacts[0]
	} else {
		dup.Artifacts = append(dup.Artifacts, dup.Artifacts[0])
	}
	if len(manifest.Artifacts) > 1 {
		// same count as declared; only the duplicate gate can catch this
		if err := validateConsumptionStructure(&dup, manifest, toolchain); err == nil || !strings.Contains(err.Error(), "twice") {
			t.Fatalf("duplicate artifact accepted: %v", err)
		}
	}

	foreign := good
	foreign.Artifacts = []consumptionArtifact{{
		ID: "ghost", OS: "plan9", Arch: "amd64", Package: "./cmd/x",
	}}
	if err := validateConsumptionStructure(&foreign, manifest, toolchain); err == nil {
		t.Fatal("undeclared artifact accepted")
	}

	relocated := good
	relocated.Artifacts = append([]consumptionArtifact{}, good.Artifacts...)
	relocated.Artifacts[0].Package = "./elsewhere"
	if err := validateConsumptionStructure(&relocated, manifest, toolchain); err == nil {
		t.Fatal("relocated package accepted")
	}

	wrongGen := good
	wrongGen.Generator.Version = "0.0.1"
	if err := validateConsumptionStructure(&wrongGen, manifest, toolchain); err == nil {
		t.Fatal("foreign generator version accepted")
	}
}

func TestSPDXEmbeddedFilePathsFailClosed(t *testing.T) {
	base := `{
	  "packages":[{"SPDXID":"SPDXRef-Package-bin"}],
	  "files":[
	    {"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"}
	  ],
	  "relationships":[
	    {"spdxElementId":"SPDXRef-DOCUMENT","relationshipType":"DESCRIBES","relatedSpdxElement":"SPDXRef-Package-bin"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-1"}
	  ]
	}`
	if paths, err := SPDXEmbeddedFilePaths([]byte(base)); err != nil || strings.Join(paths, ",") != "data/a.txt" {
		t.Fatalf("baseline wrong: %v %v", paths, err)
	}
	uncontained := strings.Replace(base,
		`{"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"}`,
		`{"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"},{"fileName":"./evil.txt","SPDXID":"SPDXRef-File-9"}`, 1)
	if _, err := SPDXEmbeddedFilePaths([]byte(uncontained)); err == nil {
		t.Fatal("uncontained file entry accepted")
	}
	dupFile := strings.Replace(base,
		`{"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"}`,
		`{"fileName":"./data/a.txt","SPDXID":"SPDXRef-File-1"},{"fileName":"./data/b.txt","SPDXID":"SPDXRef-File-1"}`, 1)
	if _, err := SPDXEmbeddedFilePaths([]byte(dupFile)); err == nil {
		t.Fatal("duplicate file id accepted")
	}
	dupPkg := strings.Replace(base,
		`{"SPDXID":"SPDXRef-Package-bin"}]`,
		`{"SPDXID":"SPDXRef-Package-bin"},{"SPDXID":"SPDXRef-Package-bin"}]`, 1)
	if _, err := SPDXEmbeddedFilePaths([]byte(dupPkg)); err == nil {
		t.Fatal("duplicate package id accepted")
	}
}

func TestSPDXEmbeddedFilePathsHostileNames(t *testing.T) {
	tmpl := func(fname string) string {
		return `{
	  "packages":[{"SPDXID":"SPDXRef-Package-bin"}],
	  "files":[{"fileName":"` + fname + `","SPDXID":"SPDXRef-File-1"}],
	  "relationships":[
	    {"spdxElementId":"SPDXRef-DOCUMENT","relationshipType":"DESCRIBES","relatedSpdxElement":"SPDXRef-Package-bin"},
	    {"spdxElementId":"SPDXRef-Package-bin","relationshipType":"CONTAINS","relatedSpdxElement":"SPDXRef-File-1"}
	  ]
	}`
	}
	for _, bad := range []string{"././x.txt", "./x//y.txt", "./x/./y.txt", "./../x.txt", "x.txt", "./", "./."} {
		if _, err := SPDXEmbeddedFilePaths([]byte(tmpl(bad))); err == nil {
			t.Errorf("hostile name %q accepted", bad)
		}
	}
	for _, good := range []string{"./x.txt", "./a/b/c.bin"} {
		if _, err := SPDXEmbeddedFilePaths([]byte(tmpl(good))); err != nil {
			t.Errorf("canonical name %q rejected: %v", good, err)
		}
	}
}

func TestVersionPinEmptyDiscoveredMismatch(t *testing.T) {
	r, err := LoadRegistry(writeTemp(t, "l.json", goodRegistry))
	if err != nil {
		t.Fatal(err)
	}
	findings := r.Audit([]Component{{Key: ToolchainKey, Kind: KindToolchain, Version: ""}})
	pinned := false
	for _, f := range findings {
		if f.Rule == "version-pin-mismatch" {
			pinned = true
		}
	}
	if !pinned {
		t.Fatal("empty discovered version passed a concrete pin")
	}
	findings = r.Audit([]Component{
		{Key: "domowik.dev/domowik", Kind: KindModule, Version: ""},
		{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.27.0"},
	})
	if len(findings) != 0 {
		t.Fatalf("(devel)/(empty) equivalence broken: %+v", findings)
	}
	r.Entries = append(r.Entries, Entry{
		Key: "assets/x.bin", Kind: KindBundled, SPDXLicense: "BSD-3-Clause",
		Status: "reviewed", VersionPin: "sha256:" + strings.Repeat("a", 64),
		ReviewedAt: "2026-08-23", RevisitTriggers: []string{"t"},
		SourceURL: "https://domowik.dev/domowik", Added: "2026-08-23",
	})
	sort.Slice(r.Entries, func(i, j int) bool { return r.Entries[i].Key < r.Entries[j].Key })
	findings = r.AuditEntries([]Component{{Key: "assets/x.bin", Kind: KindBundled, Version: "sha256:" + strings.Repeat("b", 64)}})
	if len(findings) != 1 || findings[0].Rule != "version-pin-mismatch" {
		t.Fatalf("bundled content-pin drift not detected: %+v", findings)
	}
}

func TestTreePurityOffenders(t *testing.T) {
	var doc map[string]any
	json.Unmarshal([]byte(goodManifest), &doc)
	out, _ := json.Marshal(doc)
	m, err := LoadManifest(writeTemp(t, "m.json", string(out)))
	if err != nil {
		t.Fatal(err)
	}
	tracked := []string{
		"cmd/domowik/main.go",
		"release/sbom/source.spdx.json",
		"release/sbom/domowik-linux-amd64.spdx.json",
		"build/release/domowik-linux-amd64",
		"NOTICE",
		"build/release/manifest.json",
	}
	if got := treePurityOffenders(m, tracked); len(got) != 0 {
		t.Fatalf("declared outputs flagged: %v", got)
	}
	intruder := append(tracked, "release/sbom/stale.txt", "build/release/old-bin")
	got := treePurityOffenders(m, intruder)
	if len(got) != 2 {
		t.Fatalf("expected 2 offenders, got %v", got)
	}
}

func TestNormalizeSBOMRejectsMalformedCreated(t *testing.T) {
	doc := func(created string) []byte {
		return []byte(`{"creationInfo": {"created": "` + created + `"}, "name": "x"}`)
	}
	for _, bad := range []string{"garbage", "2026-08-24 05:00:00", "2026-13-45T99:99:99Z", ""} {
		if _, err := NormalizeSBOM(doc(bad)); err == nil {
			t.Errorf("created %q accepted", bad)
		}
	}
	if _, err := NormalizeSBOM(doc("2026-08-24T05:00:00Z")); err != nil {
		t.Fatalf("canonical form rejected: %v", err)
	}
}

func TestEvidenceInventoryRejectsExtras(t *testing.T) {
	g := &Generator{Root: "../.."}
	inv, err := g.Plan(context.Background())
	if err != nil {
		t.Skipf("pipeline unavailable: %v", err)
	}
	outDir := filepath.Join(g.Root, "build/release")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}
	extra := filepath.Join(outDir, "domowik-darwin-arm64")
	before, readErr := os.ReadFile(extra)
	_ = before
	if err := os.WriteFile(extra, []byte("stale"), 0o644); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if readErr != nil {
			os.Remove(extra)
		}
	}()
	if err := g.materializeEvidence(inv); err == nil || !strings.Contains(err.Error(), "unexpected file") {
		t.Fatalf("stale extra evidence accepted: %v", err)
	}
	// Remove the stale extra so the symlink mutation reaches the type-check
	// branch instead of tripping inventory membership first.
	if readErr != nil {
		os.Remove(extra)
	}
	symlinkTarget := filepath.Join(t.TempDir(), "x")
	os.WriteFile(symlinkTarget, []byte("y"), 0o644)
	link := filepath.Join(outDir, "link.bin")
	os.Remove(link)
	if err := os.Symlink(symlinkTarget, link); err != nil {
		t.Fatal(err)
	}
	defer os.Remove(link)
	err = g.checkEvidenceInventory(inv)
	if err == nil || !strings.Contains(err.Error(), "not a regular file") {
		t.Fatalf("symlinked evidence accepted: %v", err)
	}
}

// TestLoadInputsRejectsSymlinkedPolicyParent pins the production boundary:
// a committed symlinked parent directory over a policy input path must fail
// closed instead of reading external bytes.
func TestLoadInputsRejectsSymlinkedPolicyParent(t *testing.T) {
	outside := t.TempDir()
	dir := filepath.Join(outside, "third-party")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	src, err := os.ReadFile("../../docs/third-party/licenses.json")
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "licenses.json"), src, 0o644); err != nil {
		t.Fatal(err)
	}
	root := t.TempDir()
	for _, rel := range []string{toolchainPath, toolsRecordPath} {
		data, err := os.ReadFile(filepath.Join("../..", rel))
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(filepath.Join(dir, filepath.Base(rel)), data, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	mdata, err := os.ReadFile(filepath.Join("../..", manifestPath))
	if err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "release"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, manifestPath), mdata, 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.MkdirAll(filepath.Join(root, "docs"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.Symlink(dir, filepath.Join(root, "docs", "third-party")); err != nil {
		t.Fatal(err)
	}
	g := &Generator{Root: root}
	if _, _, _, err := g.LoadInputs(); err == nil {
		t.Fatal("symlinked policy-input parent accepted")
	} else if !strings.Contains(err.Error(), registryPath) {
		t.Fatalf("error does not identify the offending input: %v", err)
	}
}

// TestPlanRejectsExtraEvidence pins that Plan (AllowDirty mode) enforces the
// same distribution-set contract as clean generation: Git cleanliness is the
// only thing AllowDirty relaxes.
func TestPlanRejectsExtraEvidence(t *testing.T) {
	g := &Generator{Root: "../.."}
	outDir := filepath.Join(g.Root, "build/release")
	if err := os.MkdirAll(outDir, 0o755); err != nil {
		t.Fatal(err)
	}
	extra := filepath.Join(outDir, "rogue-artifact")
	had := false
	if _, err := os.Stat(extra); err == nil {
		had = true
	} else if err := os.WriteFile(extra, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	defer func() {
		if !had {
			os.Remove(extra)
		}
	}()
	if _, err := g.Plan(context.Background()); err == nil || !strings.Contains(err.Error(), "release evidence inventory") {
		t.Fatalf("Plan accepted an undeclared evidence file: %v", err)
	}
}

// TestEvidenceHashRejectsNonSPDX proves the evidence-hash path validates the
// SPDX envelope, not just the created timestamp: a self-consistent manifest
// cannot bless malformed or non-SPDX source evidence.
func TestEvidenceHashRejectsNonSPDX(t *testing.T) {
	notSpdx := []byte(`{"creationInfo": {"created": "2026-08-24T05:00:00Z"}, "note": "self-consistent garbage"}`)
	if _, err := sbomEvidenceHash(notSpdx); err == nil {
		t.Fatal("non-SPDX evidence accepted by the hash path")
	}
	wrongVersion := []byte(`{
	  "spdxVersion": "SPDX-2.2",
	  "SPDXID": "SPDXRef-DOCUMENT",
	  "name": "x",
	  "creationInfo": {"created": "2026-08-24T05:00:00Z", "creators": ["Tool: t"]},
	  "packages": [{"SPDXID": "SPDXRef-Package-p", "name": "p"}]
	}`)
	if _, err := sbomEvidenceHash(wrongVersion); err == nil || !strings.Contains(err.Error(), "spdxVersion") {
		t.Fatalf("wrong SPDX version accepted: %v", err)
	}
	good := []byte(`{
	  "spdxVersion": "SPDX-2.3",
	  "SPDXID": "SPDXRef-DOCUMENT",
	  "name": "x",
	  "creationInfo": {"created": "2026-08-24T05:00:00Z", "creators": ["Tool: domowik-sbom-1.1.0"]},
	  "packages": [{"SPDXID": "SPDXRef-Package-p", "name": "p"}]
	}`)
	h1, err := sbomEvidenceHash(good)
	if err != nil {
		t.Fatalf("valid envelope rejected: %v", err)
	}
	h2, err := sbomEvidenceHash(good)
	if err != nil || h1 != h2 {
		t.Fatalf("hashing unstable across runs: %v vs %v (%v)", h1, h2, err)
	}
}

// TestEvidenceHashAcceptsRenderedSBOM proves real generator output passes
// the evidence-hash envelope validation end to end.
func TestEvidenceHashAcceptsRenderedSBOM(t *testing.T) {
	doc, err := RenderSBOM(sampleSBOMInput(time.Unix(1000, 0)))
	if err != nil {
		t.Fatal(err)
	}
	h1, err := sbomEvidenceHash(doc)
	if err != nil {
		t.Fatalf("rendered SBOM rejected by evidence hash path: %v", err)
	}
	h2, err := sbomEvidenceHash(doc)
	if err != nil || h1 != h2 {
		t.Fatalf("unstable hashing of rendered SBOM: %v vs %v (%v)", h1, h2, err)
	}
}
