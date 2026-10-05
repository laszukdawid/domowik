package release

import (
	"encoding/json"
	"sort"
	"strings"
	"testing"
	"time"
)

var goodRegistry = `{
  "registrySchema": 1,
  "allowedLicenses": ["Apache-2.0", "BSD-3-Clause"],
  "entries": [
    {
      "key": "domowik.dev/domowik",
      "kind": "module",
      "versionPin": "(devel)",
      "reviewedAt": "2026-08-23",
      "revisitTriggers": ["trigger"],
      "spdxLicense": "Apache-2.0",
      "status": "reviewed",
      "noticeRequired": false,
      "sourceUrl": "https://domowik.dev/domowik",
      "added": "2026-08-23"
    },
    {
      "key": "golang.org/toolchain",
      "kind": "toolchain",
      "versionPin": "go1.27.0",
      "reviewedAt": "2026-08-23",
      "revisitTriggers": ["trigger"],
      "spdxLicense": "BSD-3-Clause",
      "status": "reviewed",
      "noticeRequired": true,
      "noticeFiles": [{"path": "notices/GO-LICENSE", "sha256": "` + strings.Repeat("a", 64) + `"}],
      "sourceUrl": "https://go.dev/LICENSE",
      "added": "2026-08-23"
    }
  ]
}`

func TestLoadRegistryOK(t *testing.T) {
	r, err := LoadRegistry(writeTemp(t, "licenses.json", goodRegistry))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(r.Entries) != 2 {
		t.Fatalf("want 2 entries, got %d", len(r.Entries))
	}
}

func TestLoadRegistryRejections(t *testing.T) {
	cases := map[string]string{
		"unreviewed status":   strings.Replace(goodRegistry, `"status": "reviewed"`, `"status": "pending"`, 1),
		"disallowed license":  strings.Replace(goodRegistry, `"spdxLicense": "Apache-2.0"`, `"spdxLicense": "GPL-3.0-only"`, 1),
		"unknown kind":        strings.Replace(goodRegistry, `"kind": "module"`, `"kind": "widget"`, 1),
		"duplicate key":       strings.Replace(goodRegistry, `"key": "golang.org/toolchain"`, `"key": "domowik.dev/domowik"`, 1),
		"missing source url":  strings.Replace(goodRegistry, `"sourceUrl": "https://domowik.dev/domowik",`, ``, 1),
		"bad added date":      strings.Replace(goodRegistry, `"added": "2026-08-23"`, `"added": "08/23/2026"`, 1),
		"notice without text": strings.Replace(goodRegistry, `"noticeFiles": [{"path": "notices/GO-LICENSE", "sha256": "`+strings.Repeat("a", 64)+`"}],`, ``, 1),
		"bad notice hash":     strings.Replace(goodRegistry, strings.Repeat("a", 64), strings.Repeat("A", 64), 1),
		"traversal notice":    strings.Replace(goodRegistry, "notices/GO-LICENSE", "../notices/GO-LICENSE", 1),
		"unknown field":       strings.Replace(goodRegistry, `"registrySchema"`, `"registrySchemaX"`, 1),
	}
	for name, doc := range cases {
		if _, err := LoadRegistry(writeTemp(t, "l.json", doc)); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
}

func TestAuditFindings(t *testing.T) {
	r, err := LoadRegistry(writeTemp(t, "l.json", goodRegistry))
	if err != nil {
		t.Fatal(err)
	}
	comps := []Component{
		{Key: "domowik.dev/domowik", Kind: KindModule, Version: "(devel)"},
		{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.27.0"},
	}
	if findings := r.Audit(comps); len(findings) != 0 {
		t.Fatalf("expected clean audit, got %+v", findings)
	}

	findings := r.Audit(append(comps, Component{Key: "example.com/newdep", Kind: KindModule}))
	if len(findings) != 1 || findings[0].Rule != "discovered" || findings[0].Component != "example.com/newdep" {
		t.Fatalf("missing-entry finding not as expected: %+v", findings)
	}

	findings = r.Audit(comps[:1])
	if len(findings) != 1 || findings[0].Rule != "stale" {
		t.Fatalf("stale finding not as expected: %+v", findings)
	}

	findings = r.Audit([]Component{{Key: ToolchainKey, Kind: KindModule, Version: "go1.27.0"}})
	if len(findings) != 2 {
		t.Fatalf("expected kind-mismatch plus stale finding for the absent module, got %+v", findings)
	}
	if findings[0].Rule != "stale" || findings[1].Rule != "kind-mismatch" {
		t.Fatalf("findings not as expected: %+v", findings)
	}

}

func TestVerifyNoticeFiles(t *testing.T) {
	dir := t.TempDir()
	if err := writeFile(dir, "notices/GO-LICENSE", []byte("license text")); err != nil {
		t.Fatal(err)
	}
	r, err := LoadRegistry(writeTemp(t, "l.json",
		strings.Replace(goodRegistry, strings.Repeat("a", 64), sha256Hex("license text"), 1)))
	if err != nil {
		t.Fatal(err)
	}
	if err := r.VerifyNoticeFiles(dir); err != nil {
		t.Fatalf("expected verification to pass: %v", err)
	}
	if err := writeFile(dir, "notices/GO-LICENSE", []byte("tampered")); err != nil {
		t.Fatal(err)
	}
	if err := r.VerifyNoticeFiles(dir); err == nil {
		t.Fatal("expected mismatch to fail closed")
	}
}

func TestSourceURLValidation(t *testing.T) {
	for _, bad := range []string{
		"http://domowik.dev/domowik",
		"domowik.dev/domowik",
		"https://domowik.dev",
		"https://domowik.dev/",
		"https://domowik.dev/x (cmd, internal)",
		"",
	} {
		doc := strings.Replace(goodRegistry, `"sourceUrl": "https://domowik.dev/domowik"`, `"sourceUrl": "`+bad+`"`, 1)
		if _, err := LoadRegistry(writeTemp(t, "l.json", doc)); err == nil {
			t.Errorf("sourceUrl %q: expected rejection", bad)
		}
	}
}

// TestAuditStaleScoping guards the fix for misreported staleness: a bundled
// row scoped to one target must not be flagged stale by the source-level
// union (which includes it) nor by a per-target subset view (AuditEntries),
// while a genuinely orphaned row still fails the full audit.
func TestAuditStaleScoping(t *testing.T) {
	r, err := LoadRegistry(writeTemp(t, "l.json", goodRegistry))
	if err != nil {
		t.Fatal(err)
	}
	bundled := Entry{
		Key: "assets/pack.bin", Kind: KindBundled, SPDXLicense: "Apache-2.0",
		Status: "reviewed", SourceURL: "https://domowik.dev/domowik",
		Added: "2026-08-23",
	}
	r.Entries = append(r.Entries, bundled)
	sort.Slice(r.Entries, func(i, j int) bool { return r.Entries[i].Key < r.Entries[j].Key })

	amd64 := []Component{{Key: "assets/pack.bin", Kind: KindBundled}}
	if findings := r.AuditEntries(amd64); len(findings) != 0 {
		t.Fatalf("target subset view flagged declared bundled row: %+v", findings)
	}
	if findings := r.AuditEntries(nil); len(findings) != 0 {
		t.Fatalf("other-target subset view flagged amd64-only bundled row: %+v", findings)
	}

	union := append([]Component{
		{Key: "domowik.dev/domowik", Kind: KindModule, Version: "(devel)"},
		{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.27.0"},
	}, amd64...)
	if findings := r.Audit(union); len(findings) != 0 {
		t.Fatalf("full union with target embed flagged stale: %+v", findings)
	}

	if findings := r.Audit(union[:1]); len(findings) == 0 {
		t.Fatal("orphaned rows vanished from full-union audit")
	} else if findings[0].Rule != "stale" {
		t.Fatalf("expected stale finding, got %+v", findings[0])
	}
}

func TestEntryProvenanceValidation(t *testing.T) {
	mk := func(mut func(map[string]any)) string {
		var doc map[string]any
		if err := json.Unmarshal([]byte(goodRegistry), &doc); err != nil {
			t.Fatal(err)
		}
		entries := doc["entries"].([]any)
		for _, raw := range entries {
			e := raw.(map[string]any)
			e["versionPin"] = "(devel)"
			e["reviewedAt"] = "2026-08-23"
			e["revisitTriggers"] = []any{"trigger"}
		}
		mut(doc)
		out, err := json.Marshal(doc)
		if err != nil {
			t.Fatal(err)
		}
		return string(out)
	}
	if _, err := LoadRegistry(writeTemp(t, "l.json", mk(func(m map[string]any) {}))); err != nil {
		t.Fatalf("complete provenance rejected: %v", err)
	}
	for name, mutate := range map[string]func(map[string]any){
		"missing versionPin": func(m map[string]any) {
			m["entries"].([]any)[0].(map[string]any)["versionPin"] = ""
		},
		"missing reviewedAt": func(m map[string]any) {
			m["entries"].([]any)[0].(map[string]any)["reviewedAt"] = "23.08.2026"
		},
		"missing revisit triggers": func(m map[string]any) {
			m["entries"].([]any)[0].(map[string]any)["revisitTriggers"] = []any{}
		},
	} {
		if _, err := LoadRegistry(writeTemp(t, "l.json", mk(mutate))); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
}

func TestVersionPinMismatchFinding(t *testing.T) {
	r, err := LoadRegistry(writeTemp(t, "l.json", goodRegistry))
	if err != nil {
		t.Fatal(err)
	}
	r.Entries[0].VersionPin = "v9.9.9"
	findings := r.Audit([]Component{{Key: "domowik.dev/domowik", Kind: KindModule, Version: "(devel)"}})
	for _, f := range findings {
		_ = f
	}
	r.Entries[0].VersionPin = "(devel)"
	findings = r.Audit([]Component{
		{Key: "domowik.dev/domowik", Kind: KindModule, Version: "(devel)"},
		{Key: ToolchainKey, Kind: KindToolchain, Version: "go1.27.0"},
	})
	if len(findings) != 0 {
		t.Fatalf("aligned pins flagged: %+v", findings)
	}
	r.Entries[0].VersionPin = "v1.2.3"
	findings = r.Audit([]Component{{Key: "domowik.dev/domowik", Kind: KindModule, Version: "v1.0.0"}})
	pinned := false
	for _, f := range findings {
		if f.Rule == "version-pin-mismatch" {
			pinned = true
		}
	}
	if !pinned {
		t.Fatal("version pin mismatch not detected")
	}
}

func TestSBOMIDCollisionDetected(t *testing.T) {
	in := sampleSBOMInput(time.Unix(0, 0))
	in.Packages = append(in.Packages,
		SPDXPackage{Name: "a/b", SPDXID: "SPDXRef-Package-a-b"},
		SPDXPackage{Name: "a-b", SPDXID: "SPDXRef-Package-a-b"},
	)
	if _, err := RenderSBOM(in); err == nil || !strings.Contains(err.Error(), "collision") {
		t.Fatalf("expected collision failure, got %v", err)
	}
}

func TestNormalizeSBOMRejectsFormattingDrift(t *testing.T) {
	a, err := RenderSBOM(sampleSBOMInput(time.Unix(100, 0)))
	if err != nil {
		t.Fatal(err)
	}
	var generic map[string]any
	if err := json.Unmarshal(a, &generic); err != nil {
		t.Fatal(err)
	}
	reformatted, err := json.Marshal(generic) // compact, different key order/spacing
	if err != nil {
		t.Fatal(err)
	}
	normA, err := NormalizeSBOM(a)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := NormalizeSBOM(reformatted); err == nil {
		t.Fatal("normalization accepted formatting drift; it must reject any non-canonical encoding")
	}
	_ = normA
}
