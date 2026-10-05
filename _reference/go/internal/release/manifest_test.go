package release

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeTemp(t *testing.T, name, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

const goodManifest = `{
  "manifestSchema": 1,
  "sourceDateEpoch": {"policy": "require-env-or-git-commit"},
  "artifacts": [
    {"id": "domowik-linux-amd64", "os": "linux", "arch": "amd64", "package": "./cmd/domowik"},
    {"id": "domowik-linux-arm64", "os": "linux", "arch": "arm64", "package": "./cmd/domowik"}
  ],
  "notice": {"path": "NOTICE"},
  "sbomDir": "release/sbom",
  "releaseOutputDir": "build/release",
  "consumptionManifest": {"path": "build/release/manifest.json"},
  "bundled": []
}`

func TestLoadManifestOK(t *testing.T) {
	m, err := LoadManifest(writeTemp(t, "manifest.json", goodManifest))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(m.Artifacts) != 2 || m.SBOMDir != "release/sbom" || m.Notice.Path != "NOTICE" {
		t.Fatalf("unexpected manifest: %+v", m)
	}
}

func TestLoadManifestRejections(t *testing.T) {
	cases := map[string]string{
		"unknown field":    strings.Replace(goodManifest, `"sbomDir"`, `"sbomDirX"`, 1),
		"trailing json":    goodManifest + "\n{}",
		"trailing garbage": goodManifest + "\nnot-json",
		"bad schema":       strings.Replace(goodManifest, `"manifestSchema": 1`, `"manifestSchema": 2`, 1),
		"bad epoch policy": strings.Replace(goodManifest, `"policy": "require-env-or-git-commit"`, `"policy": "wall-clock"`, 1),
		"no artifacts":     `{"manifestSchema":1,"sourceDateEpoch":{"policy":"require-env-or-git-commit"},"artifacts":[],"notice":{"path":"NOTICE"},"sbomDir":"s","releaseOutputDir":"b","consumptionManifest":{"path":"b/m.json"}}`,
	}
	for name, doc := range map[string]string{
		"missing releaseOutputDir":    strings.Replace(goodManifest, `"releaseOutputDir": "build/release",`, ``, 1),
		"missing consumptionManifest": strings.Replace(goodManifest, `"consumptionManifest": {"path": "build/release/manifest.json"}`, ``, 1),
		"cgo field rejected":          strings.Replace(goodManifest, `"package": "./cmd/domowik"`, `"package": "./cmd/domowik", "cgo": false`, 1),
	} {
		if _, err := LoadManifest(writeTemp(t, "m.json", doc)); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
	for name, doc := range cases {
		if _, err := LoadManifest(writeTemp(t, "m.json", doc)); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}

	traversal := []string{"../out", "/abs/out", ".", "a/../b"}
	for _, p := range traversal {
		doc := strings.Replace(goodManifest, `"sbomDir": "release/sbom"`, `"sbomDir": "`+p+`"`, 1)
		if _, err := LoadManifest(writeTemp(t, "m.json", doc)); err == nil {
			t.Errorf("sbomDir %q: expected rejection", p)
		}
	}
	dupID := strings.Replace(goodManifest, `"id": "domowik-linux-arm64"`, `"id": "domowik-linux-amd64"`, 1)
	if _, err := LoadManifest(writeTemp(t, "m.json", dupID)); err == nil {
		t.Error("duplicate id: expected rejection")
	}
	dupTarget := strings.Replace(goodManifest,
		`{"id": "domowik-linux-arm64", "os": "linux", "arch": "arm64", "package": "./cmd/domowik"}`,
		`{"id": "other-amd64", "os": "linux", "arch": "amd64", "package": "./cmd/domowik"}`, 1)
	if _, err := LoadManifest(writeTemp(t, "m.json", dupTarget)); err == nil {
		t.Error("duplicate target: expected rejection")
	}
	for _, id := range []string{"../evil", "a/b", ".hidden-ok", "", "has space"} {
		doc := strings.Replace(goodManifest, `"id": "domowik-linux-amd64"`, `"id": "`+id+`"`, 1)
		if _, err := LoadManifest(writeTemp(t, "m.json", doc)); err == nil && id != ".hidden-ok" {
			t.Errorf("artifact id %q: expected rejection", id)
		}
	}
	for _, pkg := range []string{"cmd/domowik", "../cmd", "/abs"} {
		doc := strings.ReplaceAll(goodManifest, `"package": "./cmd/domowik"`, `"package": "`+pkg+`"`)
		if _, err := LoadManifest(writeTemp(t, "m.json", doc)); err == nil {
			t.Errorf("package %q: expected rejection", pkg)
		}
	}
}

func writeFile(dir, rel string, data []byte) error {
	path := filepath.Join(dir, filepath.FromSlash(rel))
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	return os.WriteFile(path, data, 0o644)
}

func sha256Hex(s string) string {
	sum := sha256.Sum256([]byte(s))
	return hex.EncodeToString(sum[:])
}

func TestManifestOutputCollisions(t *testing.T) {
	mutate := func(f func(m *map[string]any)) string {
		var doc map[string]any
		if err := json.Unmarshal([]byte(goodManifest), &doc); err != nil {
			t.Fatal(err)
		}
		f(&doc)
		out, err := json.Marshal(doc)
		if err != nil {
			t.Fatal(err)
		}
		return string(out)
	}
	cases := map[string]func(m *map[string]any){
		"notice inside sbomDir": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "release/sbom/NOTICE"}
		},
		"notice equals sbomDir": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "release/sbom"}
		},
		"consumption manifest collides with binary": func(m *map[string]any) {
			(*m)["consumptionManifest"] = map[string]any{"path": "build/release/domowik-linux-amd64"}
		},
		"sbomDir nested in notice": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "release"}
		},
		"notice inside releaseOutputDir": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "build/release/NOTICE"}
		},
		"disposable trees nest": func(m *map[string]any) {
			(*m)["sbomDir"] = "build/release/sbom"
		},
		"output over policy inputs": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "docs/third-party/licenses.json"}
		},
		"output containing policy inputs": func(m *map[string]any) {
			(*m)["sbomDir"] = "docs"
		},
		"release tree over docs": func(m *map[string]any) {
			(*m)["releaseOutputDir"] = "docs"
		},
		"notice under docs/": func(m *map[string]any) {
			(*m)["notice"] = map[string]any{"path": "docs/NOTICE"}
		},
		"output under docs/third-party": func(m *map[string]any) {
			(*m)["consumptionManifest"] = map[string]any{"path": "docs/third-party/tools-copy.json"}
		},
	}
	for name, f := range cases {
		if _, err := LoadManifest(writeTemp(t, "m.json", mutate(f))); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
	ok := mutate(func(m *map[string]any) {})
	if _, err := LoadManifest(writeTemp(t, "m.json", ok)); err != nil {
		t.Fatalf("baseline rejected unexpectedly: %v", err)
	}
}

func TestManifestBundledRequired(t *testing.T) {
	var doc map[string]any
	if err := json.Unmarshal([]byte(goodManifest), &doc); err != nil {
		t.Fatal(err)
	}
	delete(doc, "bundled")
	out, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := LoadManifest(writeTemp(t, "m.json", string(out))); err == nil {
		t.Error("omitted bundled: expected rejection")
	}
	// explicit empty array still passes
	doc["bundled"] = []any{}
	out, _ = json.Marshal(doc)
	if _, err := LoadManifest(writeTemp(t, "m.json", string(out))); err != nil {
		t.Fatalf("explicit empty bundled rejected: %v", err)
	}
}
