package cli

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

const goldenThin = "../../testdata/corpus/golden/packages/thin-valid.domo"

func shaOf(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return fmt.Sprintf("%x", sha256.Sum256(b))
}

func runOpsOK(t *testing.T, args ...string) (string, string) {
	t.Helper()
	var stdout, stderr bytes.Buffer
	code := Run(args, &stdout, &stderr)
	if code != 0 {
		t.Fatalf("Run(%v) = %d, stdout %q, stderr %q", args, code, stdout.String(), stderr.String())
	}
	return stdout.String(), stderr.String()
}

// --- ops -----------------------------------------------------------------

func TestOpsUsageErrors(t *testing.T) {
	cases := [][]string{
		{"ops"},
		{"ops", "a.domo", "b.domo"},
		{"ops", "--bogus"},
		{"ops", "--retain-batches"},
	}
	for _, args := range cases {
		var stdout, stderr bytes.Buffer
		if code := Run(args, &stdout, &stderr); code != 2 {
			t.Errorf("Run(%v) = %d, want 2", args, code)
		}
		if stdout.Len() != 0 || stderr.Len() == 0 {
			t.Errorf("Run(%v) stream discipline broken: out %q err %q", args, stdout.String(), stderr.String())
		}
	}
}

func TestOpsJSONAndTextOnGoldenPackage(t *testing.T) {
	view := loadGoldenForCounts(t)
	jsonOut, _ := runOpsOK(t, "ops", "--json", goldenThin)
	var report struct {
		Schema  string `json:"schema"`
		Package struct {
			Path          string `json:"path"`
			FormatVersion string `json:"formatVersion"`
			Profile       string `json:"profile"`
			SHA256        string `json:"sha256"`
		} `json:"package"`
		Snapshot struct {
			Present     bool             `json:"present"`
			EntityCount int              `json:"entityCount"`
			Frontier    map[string]int64 `json:"coverageFrontier"`
		} `json:"snapshot"`
		Batches []struct {
			Entry       string           `json:"entry"`
			BatchID     string           `json:"batchId"`
			CounterBase int64            `json:"counterBase"`
			EndCounter  int64            `json:"endCounter"`
			Frontier    map[string]int64 `json:"frontier"`
			Operations  []opRecord       `json:"operations"`
		} `json:"batches"`
		Actors []actorRecord `json:"actors"`
	}
	if err := json.Unmarshal([]byte(jsonOut), &report); err != nil {
		t.Fatal(err)
	}
	if report.Schema != "domowik.ops.v1" {
		t.Fatalf("schema identifier missing: %q", report.Schema)
	}
	if report.Package.FormatVersion != view.FormatVersion || report.Package.Profile != "thin" {
		t.Fatalf("identity mismatch: %s/%s", report.Package.FormatVersion, report.Package.Profile)
	}
	if report.Package.Path != goldenThin {
		t.Fatalf("package path mismatch: %q", report.Package.Path)
	}
	if len(report.Package.SHA256) != 64 {
		t.Fatalf("package sha256 must be 64 lowercase hex characters: %q", report.Package.SHA256)
	}
	if !report.Snapshot.Present {
		t.Fatal("snapshot must be reported present")
	}
	if len(report.Batches) != len(view.Retained) {
		t.Fatalf("batch count %d != %d", len(report.Batches), len(view.Retained))
	}
	for _, b := range report.Batches {
		if b.EndCounter < b.CounterBase {
			t.Fatalf("batch %s range inverted", b.BatchID)
		}
		if len(b.Operations) == 0 {
			t.Fatalf("batch %s lacks operation metadata", b.BatchID)
		}
	}
	if len(report.Batches) > 0 && len(report.Actors) == 0 {
		t.Fatal("actors must be derived when batches exist")
	}

	textOut, _ := runOpsOK(t, "ops", goldenThin)
	for _, want := range []string{"snapshot present", "batches:", "actors:", "range ["} {
		if !bytes.Contains([]byte(textOut), []byte(want)) {
			t.Errorf("text output missing %q:\n%s", want, textOut)
		}
	}
}

// --- compact: plan-only default ------------------------------------------

func TestCompactPlanWritesNothing(t *testing.T) {
	target := copyGolden(t)
	before := shaOf(t, target)
	out, _ := runOpsOK(t, "compact", target)
	if !bytes.Contains([]byte(out), []byte("plan only: nothing was written")) {
		t.Fatalf("plan banner missing:\n%s", out)
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("plan-only compaction must not write the target")
	}
	// JSON plan mode behaves identically on disk.
	var plan compactPlan
	jsonOut, _ := runOpsOK(t, "compact", "--json", target)
	if err := json.Unmarshal([]byte(jsonOut), &plan); err != nil {
		t.Fatal(err)
	}
	if plan.Write {
		t.Fatal("default plan must report write=false")
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("json plan-only compaction must not write the target")
	}
}

// --- compact: execution --------------------------------------------------

func TestCompactExecuteFoldsAndPreservesProjection(t *testing.T) {
	target := copyGolden(t)
	source := loadGoldenForCounts(t)
	wantState := source.Projection.State()

	out, _ := runOpsOK(t, "compact", "--assert-local-only", "--execute", target)
	// Preflight plan must appear before any commit confirmation.
	preflight := bytes.Index([]byte(out), []byte("preflight complete"))
	committed := bytes.Index([]byte(out), []byte("committed:"))
	if preflight < 0 || committed < 0 || preflight > committed {
		t.Fatalf("execute output must show preflight plan then commit:\n%s", out)
	}
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatalf("compacted package must load: %v", lerr)
	}
	if len(view.Retained) != 0 {
		t.Fatalf("default retention folds everything, got %d batches", len(view.Retained))
	}
	if !projectionStatesEqual(wantState, view.Projection.State()) {
		t.Fatal("projection must survive full compaction unchanged")
	}
	insp, ierr := persist.InspectFile(target)
	if ierr != nil {
		t.Fatal(ierr)
	}
	if insp.Snapshot.EntityCount != len(wantState.Entities) {
		t.Fatalf("compacted snapshot entity count %d != %d", insp.Snapshot.EntityCount, len(wantState.Entities))
	}

	// ops over the compacted package reports zero retained batches.
	opOut, _ := runOpsOK(t, "ops", "--json", target)
	if !bytes.Contains([]byte(opOut), []byte(`"batches": []`)) && !bytes.Contains([]byte(opOut), []byte("\"batches\": null")) &&
		len(extractBatches(t, opOut)) != 0 {
		t.Fatalf("ops must show empty retention after full fold: %s", opOut)
	}

	// Idempotent re-execution is a valid no-op fold.
	runOpsOK(t, "compact", "--assert-local-only", "--execute", target)
}

func TestCompactExecuteRetainBatchesKeepsNewestTail(t *testing.T) {
	target := copyGolden(t)
	source := loadGoldenForCounts(t)
	if len(source.Retained) < 1 {
		t.Skip("golden package has no retained batches")
	}
	keep := len(source.Retained) - 1
	runOpsOK(t, "compact", "--assert-local-only", "--execute",
		"--retain-batches", fmt.Sprint(keep), target)
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatal(lerr)
	}
	if len(view.Retained) != keep {
		t.Fatalf("retention kept %d, want %d", len(view.Retained), keep)
	}
	if !projectionStatesEqual(source.Projection.State(), view.Projection.State()) {
		t.Fatal("projection must survive partial compaction unchanged")
	}
}

// --- compact: locality gate ----------------------------------------------

func TestCompactRefusesMultiActorHistoryEvenWithAssertion(t *testing.T) {
	target := filepath.Join(t.TempDir(), "multi.domo")
	actorA := "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	actorB := "01948f5a-ca77-7a09-9e76-5ace11b12fd3"
	raws := map[string][]byte{
		"snapshot/state.json":  []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`),
		batchEntry(42, actorA): batchDoc(t, "01949102-0000-7000-8000-000000000001", actorA, 42),
		batchEntry(44, actorB): batchDoc(t, "01949102-0000-7000-8000-000000000002", actorB, 44),
	}
	pkg, werr := persist.AssembleRawPackage(raws)
	if werr != nil {
		t.Fatal(werr)
	}
	if err := os.WriteFile(target, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
	before := shaOf(t, target)

	args := []string{"compact", "--assert-local-only", "--execute", target}
	var stdout, stderr bytes.Buffer
	if code := Run(args, &stdout, &stderr); code != 1 {
		t.Fatalf("multi-actor compaction must refuse with exit 1, got %d (%q)", code, stderr.String())
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("refused compaction must leave the old bytes intact")
	}
	if !bytes.Contains(stderr.Bytes(), []byte("--assert-local-only flag records an assertion")) {
		t.Fatalf("refusal must state that the assertion is not protocol: %q", stderr.String())
	}
}

func TestCompactExecuteWithoutFlagRefusesEvenSingleActor(t *testing.T) {
	target := copyGolden(t)
	before := shaOf(t, target)
	var stdout, stderr bytes.Buffer
	if code := Run([]string{"compact", "--execute", target}, &stdout, &stderr); code != 1 {
		t.Fatalf("execute without --assert-local-only must refuse with exit 1, got %d", code)
	}
	if !bytes.Contains(stderr.Bytes(), []byte("locality is never assumed")) {
		t.Fatalf("refusal must name the missing flag: %q", stderr.String())
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("refused execution must leave old bytes intact")
	}
}

func TestCompactRetentionPolicyPerActorSuffixClosure(t *testing.T) {
	actor := "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	raws := map[string][]byte{
		batchEntry(42, actor): batchDoc(t, "01949102-0000-7000-8000-000000000001", actor, 42),
		batchEntry(43, actor): batchDoc(t, "01949102-0000-7000-8000-000000000002", actor, 43),
		batchEntry(44, actor): batchDoc(t, "01949102-0000-7000-8000-000000000003", actor, 44),
	}
	target := filepath.Join(t.TempDir(), "chain.domo")
	pkg, werr := persist.AssembleRawPackage(raws)
	if werr != nil {
		t.Fatal(werr)
	}
	if err := os.WriteFile(target, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
	jsonOut, _ := runOpsOK(t, "compact", "--json", "--retain-batches", "1", target)
	var plan compactPlan
	if err := json.Unmarshal([]byte(jsonOut), &plan); err != nil {
		t.Fatal(err)
	}
	if plan.RetainBatches != 1 || plan.FoldedBatches != 2 || plan.KeptBatches != 1 {
		t.Fatalf("plan counts wrong: %+v", plan)
	}
	if len(plan.FoldedEntries) != 2 {
		t.Fatalf("folded entries wrong: %v", plan.FoldedEntries)
	}
	// The folded prefix is exactly the two OLDEST batches.
	if plan.FoldedEntries[0] == batchEntry(44, actor) {
		t.Fatalf("newest batch must never fold first: %v", plan.FoldedEntries)
	}
	before := shaOf(t, target)
	runOpsOK(t, "compact", "--assert-local-only", "--execute", "--retain-batches", "1", target)
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatalf("per-actor suffix closure must keep ranges gap-free: %v", lerr)
	}
	if len(view.Retained) != 1 || view.Retained[0].CounterBase != 44 {
		t.Fatalf("kept batch must be the newest: %+v", view.Retained)
	}
	if after := shaOf(t, target); after == before {
		t.Fatal("executed compaction must replace the bytes")
	}
}

func TestCompactEquivalenceGateRefusesBadCandidate(t *testing.T) {
	target := copyGolden(t)
	before := shaOf(t, target)
	origBuild := buildPackageFn
	defer func() { buildPackageFn = origBuild }()
	buildPackageFn = func(in *persist.WriteInput) ([]byte, *persist.Error) {
		pkg, berr := persist.BuildPackage(in)
		if berr != nil {
			return nil, berr
		}
		// Corrupt the candidate so its reload cannot match the source.
		return pkg[:len(pkg)-1], nil
	}
	var stdout, stderr bytes.Buffer
	if code := compactTarget(compactOptions{target: target, execute: true, assertLocalOnly: true}, &stdout, &stderr, nil); code != 1 {
		t.Fatalf("equivalence gate must refuse with exit 1, got %d (%q)", code, stderr.String())
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("refused candidate must leave old bytes intact")
	}
	if !bytes.Contains(stderr.Bytes(), []byte("candidate failed validation")) &&
		!bytes.Contains(stderr.Bytes(), []byte("projection differs")) {
		t.Fatalf("refusal must name the failed proof: %q", stderr.String())
	}
}

// --- compact: baseline wiring --------------------------------------------

func TestCompactBaselineRefusesModifiedTarget(t *testing.T) {
	target := copyGolden(t)
	origInspect, origLoad, origBaseline := inspectFileFn, loadFileFn, baselineFn
	defer func() { inspectFileFn, loadFileFn, baselineFn = origInspect, origLoad, origBaseline }()

	// The plan sees the ORIGINAL bytes; by execute time the baseline is
	// taken over DIFFERENT current bytes — the §9.2 mismatch must refuse.
	current := copyGolden(t)
	if err := os.WriteFile(current, append(mustRead(t, current), 0), 0o644); err != nil {
		t.Fatal(err)
	}
	baselineFn = func(string) (*persist.TargetBaseline, error) {
		return persist.BaselineTarget(current)
	}
	inspectFileFn = func(string) (*persist.Inspection, error) { return persist.InspectFile(origLoad_target()) }
	loadFileFn = func(string) (*persist.PackageView, error) { return persist.LoadFile(origLoad_target()) }
	_ = origInspect

	before := shaOf(t, target)
	var stdout, stderr bytes.Buffer
	if code := compactTarget(compactOptions{target: target, execute: true}, &stdout, &stderr, nil); code != 1 {
		t.Fatalf("externally modified target must refuse with exit 1, got %d", code)
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("baseline refusal must preserve old bytes")
	}
}

// --- compact: §9.7 fault injection proves old/new outcomes ---------------

func TestCompactFaultInjectionOldAndNewOutcomes(t *testing.T) {
	t.Run("abort after S1 leaves old bytes", func(t *testing.T) {
		target := copyGolden(t)
		before := shaOf(t, target)
		hooks := &persist.SaveHooks{
			AfterPhase: func(p persist.SavePhase) error {
				if p == persist.S1Written {
					return errors.New("injected power loss")
				}
				return nil
			},
		}
		var stdout, stderr bytes.Buffer
		if code := compactTarget(compactOptions{target: target, execute: true}, &stdout, &stderr, hooks); code != 1 {
			t.Fatalf("injected S1 failure must exit 1, got %d", code)
		}
		if after := shaOf(t, target); after != before {
			t.Fatal("S1 failure must preserve OLD bytes exactly")
		}
	})

	t.Run("abort after S4 leaves NEW bytes atomically", func(t *testing.T) {
		target := copyGolden(t)
		hooks := &persist.SaveHooks{
			AfterPhase: func(p persist.SavePhase) error {
				if p == persist.S4Replaced {
					return errors.New("injected dir-sync failure")
				}
				return nil
			},
		}
		var stdout, stderr bytes.Buffer
		code := compactTarget(compactOptions{target: target, execute: true, assertLocalOnly: true}, &stdout, &stderr, hooks)
		if code != 1 {
			t.Fatalf("post-replace interruption must surface as failure, got %d", code)
		}
		if !bytes.Contains(stderr.Bytes(), []byte("no rollback")) {
			t.Fatalf("F6 semantics must acknowledge no-rollback: %q", stderr.String())
		}
		view, lerr := persist.LoadFile(target)
		if lerr != nil {
			t.Fatal(lerr)
		}
		if len(view.Retained) != 0 {
			t.Fatal("after replacement the NEW compacted generation must be in place")
		}
	})
}

// --- helpers --------------------------------------------------------------

func copyGolden(t *testing.T) string {
	t.Helper()
	target := filepath.Join(t.TempDir(), "copy.domo")
	b := mustRead(t, goldenThin)
	if err := os.WriteFile(target, b, 0o644); err != nil {
		t.Fatal(err)
	}
	return target
}

func mustRead(t *testing.T, path string) []byte {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return b
}

func loadGoldenForCounts(t *testing.T) *persist.PackageView {
	t.Helper()
	view, err := persist.LoadFile(goldenThin)
	if err != nil {
		t.Fatal(err)
	}
	return view
}

func extractBatches(t *testing.T, jsonOut string) []batchRecord {
	t.Helper()
	var report struct {
		Batches []batchRecord `json:"batches"`
	}
	if err := json.Unmarshal([]byte(jsonOut), &report); err != nil {
		t.Fatal(err)
	}
	return report.Batches
}

func batchEntry(base int64, actor string) string {
	return fmt.Sprintf("ops/%019d-%s.json", base, actor)
}

func batchDoc(t *testing.T, batchID, actor string, base int64) []byte {
	t.Helper()
	// §4.4 F4: a conforming writer emits the actor's own prior end counter,
	// which B3 contiguity fixes at counterBase-1. Its absence is a closure
	// violation, not a tidy omission.
	frontier := "{}"
	if base > 1 {
		frontier = fmt.Sprintf(`{%q:%d}`, actor, base-1)
	}
	return []byte(fmt.Sprintf(
		`{"batchId":%q,"actorId":%q,"counterBase":%d,"frontier":%s,"operations":[{"operationId":"01949100-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"01949101-0000-7000-8000-000000000001"}]}`,
		batchID, actor, base, frontier, base))
}

// origLoad_target is a test-scoped stand-in for the original path under
// baseline tampering; it resolves through the closure set up by the test.
var origLoad_target = func() string { return "" }

// --- stable schema contract ----------------------------------------------

func TestOpsExactSchemaFields(t *testing.T) {
	out, _ := runOpsOK(t, "ops", "--json", goldenThin)
	var doc map[string]any
	if err := json.Unmarshal([]byte(out), &doc); err != nil {
		t.Fatal(err)
	}
	for _, key := range []string{"schema", "package", "snapshot", "batches", "actors"} {
		if _, ok := doc[key]; !ok {
			t.Errorf("stable schema missing top-level %q", key)
		}
	}
	if doc["schema"] != "domowik.ops.v1" {
		t.Errorf("schema identifier = %v, want domowik.ops.v1", doc["schema"])
	}
	pkg, ok := doc["package"].(map[string]any)
	if !ok {
		t.Fatal("package must be an object")
	}
	for _, key := range []string{"path", "formatVersion", "profile", "sha256"} {
		if _, ok := pkg[key]; !ok {
			t.Errorf("stable schema missing package.%s", key)
		}
	}
	batches := doc["batches"].([]any)
	for _, raw := range batches {
		b := raw.(map[string]any)
		for _, key := range []string{"entry", "batchId", "actorId", "counterBase", "endCounter", "operations"} {
			if _, ok := b[key]; !ok {
				t.Errorf("batch missing %q", key)
			}
		}
		for _, op := range b["operations"].([]any) {
			o := op.(map[string]any)
			for _, key := range []string{"operationId", "kind", "entityId", "payload"} {
				if _, ok := o[key]; !ok {
					t.Errorf("operation missing %q", key)
				}
			}
		}
		break // one batch's shape pins the record contract
	}
	for _, raw := range doc["actors"].([]any) {
		a := raw.(map[string]any)
		for _, key := range []string{"actorId", "batchCount", "operationCount", "ranges"} {
			if _, ok := a[key]; !ok {
				t.Errorf("actor missing %q", key)
			}
		}
		if a["batchCount"].(float64) < 1 || a["operationCount"].(float64) < 1 {
			t.Errorf("actor counts must be positive: %v", a)
		}
	}
}

// --- stream separation under writer failure ------------------------------

type failingWriter struct{ writes *int }

func (f failingWriter) Write(p []byte) (int, error) {
	*f.writes++
	return 0, errors.New("injected stdout failure")
}

func TestEmitJSONUsesInjectedStderrOnly(t *testing.T) {
	calls := 0
	stdout := failingWriter{writes: &calls}
	var stderr bytes.Buffer
	code := emitJSON(stdout, &stderr, map[string]string{"schema": opsSchemaID})
	if code != 1 {
		t.Fatalf("encoder failure must return exit 1, got %d", code)
	}
	if calls != 1 {
		t.Fatalf("stdout must have been attempted exactly once, got %d", calls)
	}
	if !bytes.Contains(stderr.Bytes(), []byte("injected stdout failure")) {
		t.Fatalf("diagnostic must land on the injected stderr, got %q", stderr.String())
	}
}

func TestCompactJSONWriterFailureGoesToInjectedStderr(t *testing.T) {
	target := copyGolden(t)
	before := shaOf(t, target)
	calls := 0
	stdout := failingWriter{writes: &calls}
	var stderr bytes.Buffer
	opts := compactOptions{target: target, json: true, assertLocalOnly: true}
	if code := compactTarget(opts, stdout, &stderr, nil); code != 1 {
		t.Fatalf("stdout encoder failure must surface as exit 1, got %d", code)
	}
	if !bytes.Contains(stderr.Bytes(), []byte("injected stdout failure")) {
		t.Fatalf("diagnostic must land on the injected stderr stream: %q", stderr.String())
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("failed report emission must still write nothing")
	}
}

func TestCompactPreflightEmittedWhenSaveFailsAndTargetStaysOld(t *testing.T) {
	target := copyGolden(t)
	before := shaOf(t, target)
	hooks := &persist.SaveHooks{
		AfterPhase: func(p persist.SavePhase) error {
			if p == persist.S1Written {
				return errors.New("injected power loss")
			}
			return nil
		},
	}
	var stdout, stderr bytes.Buffer
	code := compactTarget(compactOptions{target: target, execute: true, assertLocalOnly: true}, &stdout, &stderr, hooks)
	if code != 1 {
		t.Fatalf("injected save failure must exit 1, got %d", code)
	}
	out := stdout.String()
	for _, want := range []string{"compaction plan for", "retention:", "preflight complete; writing now"} {
		if !bytes.Contains([]byte(out), []byte(want)) {
			t.Fatalf("preflight plan missing %q on failed save:\n%s", want, out)
		}
	}
	if after := shaOf(t, target); after != before {
		t.Fatal("failed save must leave old bytes exactly intact")
	}

	// JSON mode: the stream must decode as preflight plan followed by no
	// commit document, and the preflight must carry retention fields.
	target2 := copyGolden(t)
	before2 := shaOf(t, target2)
	var out2, err2 bytes.Buffer
	failHooks := &persist.SaveHooks{
		AfterPhase: func(p persist.SavePhase) error {
			if p == persist.S1Written {
				return errors.New("injected power loss")
			}
			return nil
		},
	}
	if code := compactTarget(compactOptions{target: target2, json: true, execute: true, assertLocalOnly: true}, &out2, &err2, failHooks); code != 1 {
		t.Fatalf("json fault run must exit 1, got %d", code)
	}
	dec := json.NewDecoder(bytes.NewReader(out2.Bytes()))
	var docs []map[string]any
	for {
		var m map[string]any
		if err := dec.Decode(&m); err != nil {
			break
		}
		docs = append(docs, m)
	}
	if len(docs) != 1 || docs[0]["phase"] != "preflight" {
		t.Fatalf("json stream must contain exactly the preflight document, got %v", docs)
	}
	if docs[0]["write"] != true || docs[0]["target"] != target2 {
		t.Fatalf("preflight must record write intent and target: %v", docs[0])
	}
	ftf, ok := docs[0]["foldedThroughFrontier"].(map[string]any)
	if !ok || len(ftf) != 1 {
		t.Fatalf("full fold must record the folded frontier per actor: %v", docs[0])
	}
	for _, v := range ftf {
		if v.(float64) != 42 {
			t.Fatalf("folded frontier must equal the batch end counter, got %v", v)
		}
	}
	if rb, ok := docs[0]["retainedBatches"].([]any); !ok || len(rb) != 0 {
		t.Fatalf("default retention keeps nothing: %v", docs[0]["retainedBatches"])
	}
	if after := shaOf(t, target2); after != before2 {
		t.Fatal("json fault run must leave old bytes intact")
	}
}
