package sizecorpus

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

// TestGatedRecordGeneration is the documented reproduction command for the
// committed evidence tree. It never runs during normal verification: it
// requires DOMOWIK_WRITE_0035_RECORDS=1 and fails hard when the reference
// encoder is unavailable (no silent acceptance, no stale records).
func TestGatedRecordGeneration(t *testing.T) {
	if os.Getenv("DOMOWIK_WRITE_0035_RECORDS") != "1" {
		t.Skip("gated writer disabled: set DOMOWIK_WRITE_0035_RECORDS=1 to regenerate docs/evidence/0035-size-corpus")
	}
	gen, err := GenerateAll()
	if err != nil {
		t.Fatalf("generation failed: %v", err)
	}
	// Refuse to write failing-budget records without explicit intervention:
	// ED-0024 demands a recorded budget revision rather than a silent pass.
	for _, plan := range Plans() {
		recBytes := gen.Records[plan.BudgetsRel]
		var rec BudgetRecord
		if err := json.Unmarshal(recBytes, &rec); err != nil {
			t.Fatal(err)
		}
		if !rec.Budget.Pass {
			t.Fatalf("scenario %s measures at or above its target (%d >= %d bytes); revise the recorded budget deliberately before regenerating records",
				plan.Name, rec.Budget.HeadlineBytes, rec.Budget.TargetBytes)
		}
	}
	root := filepath.Join(repoRoot(t), EvidenceRoot)
	if err := WriteTree(root, gen); err != nil {
		t.Fatal(err)
	}
	// Determinism proof inside the same process: a second generation must
	// be byte-identical.
	gen2, err := GenerateAll()
	if err != nil {
		t.Fatal(err)
	}
	for rel, data := range gen.Packages {
		if string(gen2.Packages[rel]) != string(data) {
			t.Fatalf("package %s not byte-stable across generations", rel)
		}
	}
	for rel, data := range gen.Records {
		if string(gen2.Records[rel]) != string(data) {
			t.Fatalf("record %s not byte-stable across generations", rel)
		}
	}
}
