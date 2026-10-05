package cli

import (
	"bytes"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

func runMigrateOutput(t *testing.T, args ...string) (int, string, string) {
	t.Helper()
	var stdout, stderr bytes.Buffer
	code := Run(append([]string{"migrate"}, args...), &stdout, &stderr)
	return code, stdout.String(), stderr.String()
}

func goldenPackage(t *testing.T) string {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join("..", "..", "testdata", "corpus", "golden", "packages", "thin-valid.domo"))
	if err != nil {
		t.Fatal(err)
	}
	p := filepath.Join(t.TempDir(), "doc.domo")
	if err := os.WriteFile(p, raw, 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestMigrateUsageAndArgErrors(t *testing.T) {
	cases := []struct {
		name    string
		args    []string
		want    int
		wantOut string
	}{
		{"help flag", []string{"--help"}, 0, migrateUsage},
		{"no source", nil, 2, migrateUsage},
		{"unknown flag", []string{"--bogus"}, 2, migrateUsage},
		{"dangling --to", []string{"--to"}, 2, migrateUsage},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			code, out, errOut := runMigrateOutput(t, tc.args...)
			if code != tc.want {
				t.Fatalf("exit = %d, want %d (stderr: %s)", code, tc.want, errOut)
			}
			if tc.want == 0 && out != tc.wantOut {
				t.Fatalf("stdout = %q, want %q", out, tc.wantOut)
			}
			if tc.want == 2 && !strings.Contains(errOut, tc.wantOut) {
				t.Fatalf("stderr missing usage: %q", errOut)
			}
		})
	}
}

// The migrate subcommand must not leak into unrelated invocations: the
// scaffold contract for unknown commands stays byte-identical.
func TestMigrateRegistrationIsolated(t *testing.T) {
	if _, ok := runSubcommand([]string{"frobnicate"}, nil, nil); ok {
		t.Fatal("unknown command must not be claimed by the registry")
	}
	var stdout, stderr bytes.Buffer
	code := Run([]string{"frobnicate"}, &stdout, &stderr)
	if code != 2 || stderr.Len() == 0 || stdout.Len() != 0 {
		t.Fatalf("scaffold unknown-command behavior changed: %d %q %q", code, stdout.String(), stderr.String())
	}
}

func TestMigrateDryRunReportsWithoutWriting(t *testing.T) {
	src := goldenPackage(t)
	before, err := os.ReadFile(src)
	if err != nil {
		t.Fatal(err)
	}
	code, out, errOut := runMigrateOutput(t, src, "--dry-run")
	if code != 0 {
		t.Fatalf("exit = %d, stderr: %s", code, errOut)
	}
	for _, want := range []string{
		"source: " + src,
		"schema: 0.1 -> 0.1",
		"validation: ok",
		"steps: 0",
		"preservation: clean",
		"action: dry-run (no write performed)",
	} {
		if !strings.Contains(out, want) {
			t.Fatalf("dry-run output missing %q:\n%s", want, out)
		}
	}
	after, rerr := os.ReadFile(src)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if string(before) != string(after) {
		t.Fatal("dry run modified the source package")
	}
	if strings.Contains(out, "tmp-") {
		t.Fatal("dry run must not create candidates")
	}
}

func TestMigrateInPlaceAlreadyAtTarget(t *testing.T) {
	src := goldenPackage(t)
	before, err := os.ReadFile(src)
	if err != nil {
		t.Fatal(err)
	}
	code, out, errOut := runMigrateOutput(t, src)
	if code != 0 {
		t.Fatalf("exit = %d, stderr: %s", code, errOut)
	}
	if !strings.Contains(out, "action: no change (already at target schema)") {
		t.Fatalf("missing no-change action line:\n%s", out)
	}
	after, rerr := os.ReadFile(src)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if string(before) != string(after) {
		t.Fatal("already-at-target migration rewrote the package")
	}
}

func TestMigrateRejectsUnsupportedSourceWithDiagnostic(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "newer.domo")
	manifest, err := os.ReadFile(filepath.Join("..", "..", "testdata", "corpus", "migration", "newer-minor-reject.json"))
	if err != nil {
		t.Fatal(err)
	}
	pkg, werr := persist.AssembleRawPackage(map[string][]byte{"domowik.json": manifest})
	if werr != nil {
		t.Fatal(werr)
	}
	if err := os.WriteFile(src, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
	code, out, errOut := runMigrateOutput(t, src)
	_ = out
	if code != 1 {
		t.Fatalf("exit = %d, want 1 (stdout: %s)", code, out)
	}
	if !strings.Contains(errOut, "unsupported-version") || !strings.Contains(errOut, "0.2") {
		t.Fatalf("diagnostic must name category and both versions: %q", errOut)
	}
}

func TestMigrateStreamsNeverMixedOnFailure(t *testing.T) {
	src := filepath.Join(t.TempDir(), "absent.domo")
	var stdout, stderr bytes.Buffer
	code := Run([]string{"migrate", src}, &stdout, &stderr)
	if code != 1 {
		t.Fatalf("exit = %d", code)
	}
	if stdout.Len() > 0 && stderr.Len() > 0 {
		t.Fatalf("mixed streams on failure: stdout %q stderr %q", stdout.String(), stderr.String())
	}
}
