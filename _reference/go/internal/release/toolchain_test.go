package release

import (
	"strings"
	"testing"
)

var goodToolchain = func() string {
	return `{
  "recordSchema": 1,
  "component": "golang.org/toolchain",
  "version": "go1.27.0",
  "license": {"spdxId": "BSD-3-Clause", "name": "Go BSD", "url": "https://go.dev/LICENSE"},
  "artifacts": [
    {
      "filename": "go1.27.0.src.tar.gz",
      "kind": "source",
      "os": "",
      "arch": "",
      "url": "https://go.dev/dl/go1.27.0.src.tar.gz",
      "sha256": "` + strings.Repeat("a7", 32) + `",
      "sizeBytes": 35080395
    },
    {
      "filename": "go1.27.0.linux-amd64.tar.gz",
      "kind": "archive",
      "os": "linux",
      "arch": "amd64",
      "url": "https://go.dev/dl/go1.27.0.linux-amd64.tar.gz",
      "sha256": "` + strings.Repeat("b6", 32) + `",
      "sizeBytes": 70523269
    },
    {
      "filename": "go1.27.0.linux-arm64.tar.gz",
      "kind": "archive",
      "os": "linux",
      "arch": "arm64",
      "url": "https://go.dev/dl/go1.27.0.linux-arm64.tar.gz",
      "sha256": "` + strings.Repeat("c5", 32) + `",
      "sizeBytes": 66988980
    }
  ],
  "supportedTargets": ["linux/amd64", "linux/arm64"],
  "added": "2026-08-23",
  "reviewed": true,
  "revisitTriggers": ["toolchain change"]
}`
}()

func TestLoadToolchainOK(t *testing.T) {
	rec, err := LoadToolchain(writeTemp(t, "toolchain.json", goodToolchain))
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if a, ok := rec.Artifact("linux", "arm64"); !ok || a.SHA256 != strings.Repeat("c5", 32) {
		t.Fatalf("archive lookup failed: %+v %t", a, ok)
	}
	if _, ok := rec.Artifact("darwin", "amd64"); ok {
		t.Fatal("unexpected darwin artifact")
	}
}

func TestLoadToolchainRejections(t *testing.T) {
	cases := map[string]string{
		"uppercase hash":       strings.Replace(goodToolchain, strings.Repeat("b6", 32), strings.ToUpper(strings.Repeat("b6", 32)), 1),
		"short hash":           strings.Replace(goodToolchain, strings.Repeat("b6", 32), strings.Repeat("6", 63)+"z", 1),
		"filename version mix": strings.ReplaceAll(goodToolchain, "go1.27.0.linux-amd64.tar.gz", "go1.26.0.linux-amd64.tar.gz"),
		"url filename mismatch": strings.Replace(goodToolchain,
			"https://go.dev/dl/go1.27.0.linux-amd64.tar.gz",
			"https://go.dev/dl/go1.27.0.linux-other.tar.gz", 1),
		"non canonical host": strings.Replace(goodToolchain,
			"https://go.dev/dl/go1.27.0.linux-amd64.tar.gz",
			"https://mirror.example.net/go1.27.0.linux-amd64.tar.gz", 1),
		"non positive size":   strings.Replace(goodToolchain, `"sizeBytes": 70523269`, `"sizeBytes": 0`, 1),
		"uncovered target":    strings.Replace(goodToolchain, `["linux/amd64", "linux/arm64"]`, `["linux/amd64", "darwin/arm64"]`, 1),
		"missing source hash": strings.Replace(goodToolchain, `"kind": "source"`, `"kind": "unknown"`, 1),
		"unreviewed":          strings.Replace(goodToolchain, `"reviewed": true`, `"reviewed": false`, 1),
		"bad date":            strings.Replace(goodToolchain, `"added": "2026-08-23"`, `"added": "2026-8-23"`, 1),
		"unknown field":       strings.Replace(goodToolchain, `"reviewed"`, `"reviewedX"`, 1),
	}
	for name, doc := range cases {
		if _, err := LoadToolchain(writeTemp(t, "t.json", doc)); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
}

// TestCommittedReleaseInputsValidate pins the real declarative inputs: any
// edit to release/*.json must keep them valid or the pipeline fails.
func TestCommittedReleaseInputsValidate(t *testing.T) {
	g := &Generator{Root: "../.."}
	m, r, tc, err := g.LoadInputs()
	if err != nil {
		t.Fatalf("committed inputs invalid: %v", err)
	}
	if len(m.Artifacts) == 0 || len(r.Entries) < 2 || !tc.Reviewed {
		t.Fatalf("committed inputs look incomplete: %+v", tc)
	}
}

func TestToolchainLicenseURLValidation(t *testing.T) {
	for _, bad := range []string{"http://go.dev/LICENSE", "go.dev/LICENSE", "", "https://"} {
		doc := strings.Replace(goodToolchain, "https://go.dev/LICENSE", bad, 1)
		if _, err := LoadToolchain(writeTemp(t, "t.json", doc)); err == nil {
			t.Errorf("license url %q: expected rejection", bad)
		}
	}
}

func TestToolchainPlatformBinding(t *testing.T) {
	mutated := strings.Replace(goodToolchain,
		`"filename": "go1.27.0.linux-arm64.tar.gz",
      "kind": "archive",
      "os": "linux",
      "arch": "arm64"`,
		`"filename": "go1.27.0.linux-arm64.tar.gz",
      "kind": "archive",
      "os": "linux",
      "arch": "amd64"`, 1)
	if _, err := LoadToolchain(writeTemp(t, "t.json", mutated)); err == nil {
		t.Fatal("os/arch swapped against filename token: expected rejection")
	}
}

func TestToolchainCanonicalURL(t *testing.T) {
	for _, bad := range []string{
		"https://go.dev/dl/other/go1.27.0.linux-amd64.tar.gz",
		"https://go.dev/dl/go1.27.0.linux-amd64.tar.gz?x=1",
		"https://go.dev/dl/go1.27.0.linux-amd64.tar.gz#frag",
		"https://mirror.example.com/dl/go1.27.0.linux-amd64.tar.gz",
	} {
		doc := strings.Replace(goodToolchain,
			"https://go.dev/dl/go1.27.0.linux-amd64.tar.gz", bad, 1)
		if _, err := LoadToolchain(writeTemp(t, "t.json", doc)); err == nil {
			t.Errorf("non-canonical url %q accepted", bad)
		}
	}
}
