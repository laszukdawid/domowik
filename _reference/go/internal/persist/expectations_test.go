package persist_test

import (
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"testing"
)

// expectations.go: machine-readable oracle loader for EXPECTATIONS.md
// (issue 0028). The fenced ```json arrays are parsed on every run; the
// checker recomputes artifact sha256 values so tampering is detected.

type Expectation struct {
	ID                     string         `json:"id"`
	Class                  string         `json:"class"`
	Artifact               string         `json:"artifact"`
	SHA256                 string         `json:"sha256"`
	Outcome                string         `json:"outcome"`
	Category               *string        `json:"category"`
	Clauses                []string       `json:"clauses"`
	ImplementationDeferred bool           `json:"implementationDeferred"`
	RawByDesign            bool           `json:"rawByDesign"`
	Assert                 map[string]any `json:"assert"`
	Recipe                 map[string]any `json:"recipe"`
	Diagnostic             *string        `json:"diagnostic"`
	Notes                  string         `json:"notes"`

	// ExecutionBlocked marks named assertions of an otherwise EXECUTING
	// record as not yet provable, with the route that owns them.
	// EXPECTATIONS.md is the sole authority: this harness carries no
	// parallel map of blocked records, so the two cannot drift.
	ExecutionBlocked *ExecutionBlocked `json:"executionBlocked"`
}

// ExecutionBlocked mirrors the manifest marker validated by
// internal/corpusgen. A record carrying it still runs; what is withheld is
// a named assertion, never the whole record.
type ExecutionBlocked struct {
	State      string   `json:"state"`
	Assertions []string `json:"assertions"`
	RoutedTo   string   `json:"routedTo"`
	Clauses    []string `json:"clauses"`
	Citations  []string `json:"citations"`
	Reason     string   `json:"reason"`

	// Observed records the current non-conformant value of each blocked
	// assertion, so the harness pins the specific divergence rather than
	// merely asserting inequality with the oracle.
	Observed map[string]any `json:"observed"`
}

// Blocks reports whether the named assertion of this record is blocked.
func (e Expectation) Blocks(assertion string) bool {
	if e.ExecutionBlocked == nil {
		return false
	}
	for _, a := range e.ExecutionBlocked.Assertions {
		if a == assertion {
			return true
		}
	}
	return false
}

var fenceRe = regexp.MustCompile("(?s)```json\n(.*?)```")

func loadExpectations(t *testing.T) []Expectation {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(corpusDir, "EXPECTATIONS.md"))
	if err != nil {
		t.Fatalf("read EXPECTATIONS.md: %v", err)
	}
	var out []Expectation
	for _, m := range fenceRe.FindAllStringSubmatch(string(raw), -1) {
		var arr []Expectation
		if jerr := json.Unmarshal([]byte(m[1]), &arr); jerr != nil {
			var one Expectation
			if oerr := json.Unmarshal([]byte(m[1]), &one); oerr != nil {
				t.Fatalf("EXPECTATIONS.md block unparsable: %v / %v", jerr, oerr)
			}
			arr = []Expectation{one}
		}
		out = append(out, arr...)
	}
	if len(out) == 0 {
		t.Fatal("no expectation records found")
	}
	return out
}

func expectationIndex(t *testing.T) map[string]Expectation {
	t.Helper()
	idx := map[string]Expectation{}
	for _, e := range loadExpectations(t) {
		idx[e.ID] = e
	}
	return idx
}
