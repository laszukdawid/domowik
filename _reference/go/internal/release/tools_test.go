package release

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestLoadToolsRecordOK(t *testing.T) {
	g := &Generator{Root: "../.."}
	if _, _, _, err := g.LoadInputs(); err != nil {
		t.Fatalf("committed inputs including tools record invalid: %v", err)
	}
}

func TestLoadToolsRecordRejections(t *testing.T) {
	base := func() string {
		data, err := os.ReadFile("../../docs/third-party/tools.json")
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
	cases := map[string]string{
		"unknown schema field": strings.Replace(base(), `"toolsSchema"`, `"recordSchema"`, 1),
		"bad schema version":   strings.Replace(base(), `"toolsSchema": 1`, `"toolsSchema": 2`, 1),
		"uppercase sha":        strings.Replace(base(), "08c6903cd8c0fde910a37f88322edcfb5dd907a8", strings.ToUpper("08c6903cd8c0fde910a37f88322edcfb5dd907a8"), 1),
		"short sha":            strings.Replace(base(), "08c6903cd8c0fde910a37f88322edcfb5dd907a8", "08c6903", 1),
		"unknown status":       strings.Replace(base(), `"status": "dev-only"`, `"status": "maybe"`, 1),
		"non-https url":        strings.Replace(base(), "https://github.com/actions/checkout", "http://github.com/actions/checkout", 1),
		"url with prose":       strings.Replace(base(), "https://github.com/actions/checkout", "https://github.com/actions/checkout (pinned)", 1),
		"distributed no provenance": func() string {
			s := strings.Replace(base(), `"name": "github-actions/checkout"`, `"provenance": "", "name": "github-actions/checkout"`, 1)
			return strings.Replace(s, `"usedIn": ".github/workflows/ci.yml",
      "added": "2026-08-23"
    },
    {
      "name": "github-actions/setup-go"`, `"usedIn": ".x",
      "added": "2026-08-23",
      "status": "distributed"
    },
    {
      "name": "github-actions/setup-go"`, 1)
		}(),
	}
	for name, doc := range cases {
		if _, err := LoadToolsRecord(writeTemp(t, "tools.json", doc)); err == nil {
			t.Errorf("%s: expected rejection", name)
		}
	}
	if _, err := LoadToolsRecord(filepath.Join(t.TempDir(), "missing.json")); err == nil {
		t.Error("missing tools record: expected failure")
	}
}

// TestPinnedActionsMatchCI fails closed if CI workflow pins drift from the
// reviewed docs/third-party/tools.json record.
func TestPinnedActionsMatchCI(t *testing.T) {
	ci, err := os.ReadFile("../../.github/workflows/ci.yml")
	if err != nil {
		t.Fatal(err)
	}
	tools, err := LoadToolsRecord("../../docs/third-party/tools.json")
	if err != nil {
		t.Fatal(err)
	}
	recorded := map[string]string{}
	for _, tool := range tools.Tools {
		if tool.CommitSHA != "" {
			recorded[tool.CommitSHA] = tool.Name
		}
	}
	seen := map[string]bool{}
	for _, line := range strings.Split(string(ci), "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "uses:") {
			continue
		}
		at := strings.LastIndex(line, "@")
		if at < 0 {
			t.Fatalf("unpinned action reference in CI: %s", line)
		}
		sha := strings.TrimSuffix(line[at+1:], " # v6.0.0")
		sha = strings.TrimSpace(strings.Split(sha, " #")[0])
		if len(sha) != 40 {
			t.Fatalf("action pin is not an exact SHA: %s", line)
		}
		if _, ok := recorded[sha]; !ok {
			t.Fatalf("CI uses action SHA %s which is not recorded in docs/third-party/tools.json", sha)
		}
		seen[sha] = true
	}
	for _, tool := range tools.Tools {
		if tool.CommitSHA != "" && !seen[tool.CommitSHA] {
			t.Errorf("recorded tool %q with SHA %s is not used by CI", tool.Name, tool.CommitSHA)
		}
	}
}

func TestToolsRecordRequiresExplicitLists(t *testing.T) {
	base := func() string {
		data, err := os.ReadFile("../../docs/third-party/tools.json")
		if err != nil {
			t.Fatal(err)
		}
		return string(data)
	}
	for _, field := range []string{"distributedTools", "packagingTools"} {
		var doc map[string]any
		if err := json.Unmarshal([]byte(base()), &doc); err != nil {
			t.Fatal(err)
		}
		delete(doc, field)
		out, err := json.Marshal(doc)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := LoadToolsRecord(writeTemp(t, "tools.json", string(out))); err == nil {
			t.Errorf("omitted %s: expected rejection", field)
		}
	}
	// present empty arrays pass
	var doc map[string]any
	json.Unmarshal([]byte(base()), &doc)
	out, _ := json.Marshal(doc)
	if _, err := LoadToolsRecord(writeTemp(t, "tools.json", string(out))); err != nil {
		t.Fatalf("committed record with explicit empties rejected: %v", err)
	}
}
