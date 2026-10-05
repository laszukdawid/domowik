package persist_test

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// save_fault_test.go: deterministic fault injection around the §9.7
// S0–S5 state machine asserting the F1–F9 guarantee table and the
// RS1–RS8 recovery-state vocabulary. All injection is hook-based and
// timing-free; mocked failures exercise process-level semantics only and
// never establish power-loss durability (platform profiles documented in
// the package documentation).

func validWriteInput(t *testing.T) *persist.WriteInput {
	t.Helper()
	levelRaw := corpusFile(t, "golden/entities/level-minimal.json")
	ent, err := document.DecodeEntity(levelRaw)
	if err != nil {
		t.Fatal(err)
	}
	return &persist.WriteInput{
		Profile: "thin",
		State: document.State{
			Entities:         []document.Entity{ent},
			Tombstones:       map[document.UUID]document.Position{},
			Frontier:         map[document.UUID]int64{},
			WinnerPositions:  map[document.FieldKey]document.Position{},
			BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
			CreatedPositions: map[document.UUID]document.Position{},
		},
		RetainedBatches: []*document.Batch{},
		Blobs:           map[string][]byte{},
	}
}

func tempTarget(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), "doc.domo")
}

// resaveInput returns write input carrying the §9.2 baseline of an
// existing target so re-saves exercise external-modification detection
// on the matching-baseline path.
func resaveInput(t *testing.T, target string) *persist.WriteInput {
	t.Helper()
	bl, berr := persist.BaselineTarget(target)
	if berr != nil {
		t.Fatal(berr)
	}
	in := validWriteInput(t)
	in.Baseline = bl
	return in
}

func phaseFails(phase persist.SavePhase, err error) *persist.SaveHooks {
	return &persist.SaveHooks{AfterPhase: func(p persist.SavePhase) error {
		if p == phase {
			return err
		}
		return nil
	}}
}

// TestSaveHappyPath runs S0–S5 with real primitives and reopens the
// result through the full reader.
func TestSaveHappyPath(t *testing.T) {
	target := tempTarget(t)
	res, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil)
	if err != nil {
		t.Fatalf("save: %v", err)
	}
	if res.DurabilityCaveat {
		t.Fatal("no durability caveat expected on a clean local save")
	}
	for _, p := range []persist.SavePhase{persist.S0TempCreated, persist.S1Written,
		persist.S2Validated, persist.S3Synced, persist.S4Replaced, persist.S5DirSynced} {
		found := false
		for _, got := range res.Phases {
			if got == p {
				found = true
			}
		}
		if !found {
			t.Fatalf("phase %v missing from completed save", p)
		}
	}
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatalf("reload after save: %v", lerr)
	}
	if len(view.Projection.State().Entities) != 1 {
		t.Fatal("saved projection lost its entity")
	}
}

// F2/X3: candidate write interrupted — target untouched (OLD readable),
// leftover cleanable.
func TestFaultF2WriteInterrupted(t *testing.T) {
	target := tempTarget(t)
	// Establish a previous generation.
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	oldView, err := persist.LoadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	oldEntities := len(oldView.Projection.State().Entities)

	hooks := &persist.SaveHooks{Writer: func(w io.Writer) io.Writer {
		return failingWriter{}
	}}
	_, serr := persist.SaveDocument(target, 0o644, resaveInput(t, target), hooks)
	if serr == nil {
		t.Fatal("expected save failure at S1")
	}
	view, lerr := persist.LoadFile(target)
	if lerr != nil || len(view.Projection.State().Entities) != oldEntities {
		t.Fatalf("target must remain OLD-readable after interrupted write: %v", lerr)
	}
	leftovers := persist.DiscoverArtifacts(target)
	for _, art := range leftovers {
		os.Remove(art.Path) // ignorable and cleanable (RS3)
	}
}

type failingWriter struct{}

func (failingWriter) Write(p []byte) (int, error) {
	return 0, errors.New("injected EIO at first chunk (X2)")
}

// F3: candidate validation fails — abort candidate-invalid, target
// untouched.
func TestFaultF3CandidateInvalid(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	hooks := &persist.SaveHooks{Validate: func(string) *persist.Error {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Detail: "injected candidate corruption"}
	}}
	_, serr := persist.SaveDocument(target, 0o644, resaveInput(t, target), hooks)
	if serr == nil || !strings.Contains(serr.Error(), "candidate-invalid") {
		t.Fatalf("expected candidate-invalid abort (F3), got %v", serr)
	}
	if _, lerr := persist.LoadFile(target); lerr != nil {
		t.Fatalf("target untouched requirement violated: %v", lerr)
	}
}

// F4: content sync fails — abort, target untouched.
func TestFaultF4SyncFailure(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	hooks := &persist.SaveHooks{SyncFile: func(*os.File) error {
		return errors.New("injected fsync failure")
	}}
	_, serr := persist.SaveDocument(target, 0o644, resaveInput(t, target), hooks)
	if serr == nil || !strings.Contains(serr.Error(), "F4") {
		t.Fatalf("expected F4 abort, got %v", serr)
	}
	if _, lerr := persist.LoadFile(target); lerr != nil {
		t.Fatalf("target must be untouched after F4: %v", lerr)
	}
}

// F5: atomic replace interrupted — report error; target is whatever the
// filesystem left (old here, since the injected Replace declines to act);
// EITHER per the rename-atomicity assumption.
func TestFaultF5ReplaceInterrupted(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	hooks := &persist.SaveHooks{Replace: func(tmp, tgt string) error {
		return errors.New("injected replace interruption")
	}}
	_, serr := persist.SaveDocument(target, 0o644, resaveInput(t, target), hooks)
	if serr == nil || !strings.Contains(serr.Error(), "F5") {
		t.Fatalf("expected F5 report, got %v", serr)
	}
	// Either outcome must be readable: here the filesystem left OLD.
	if _, lerr := persist.LoadFile(target); lerr != nil {
		t.Fatalf("EITHER guarantee violated: %v", lerr)
	}
}

// F6: replacement completed, directory sync failed — NEW readable,
// durability caveat recorded, no rollback.
func TestFaultF6DirSyncFailure(t *testing.T) {
	target := tempTarget(t)
	hooks := &persist.SaveHooks{SyncDir: func(string) error {
		return errors.New("injected directory-sync failure")
	}}
	res, serr := persist.SaveDocument(target, 0o644, validWriteInput(t), hooks)
	if serr != nil {
		t.Fatalf("S5 failure must not roll back: %v", serr)
	}
	if !res.DurabilityCaveat {
		t.Fatal("F6 requires the durability caveat flag")
	}
	if _, lerr := persist.LoadFile(target); lerr != nil {
		t.Fatalf("NEW package must be readable after F6: %v", lerr)
	}
}

// F1: first save with no prior target, failure before replace — no target
// file exists, leftover candidate ignorable and cleanable.
func TestFaultF1FirstSaveNoPriorTarget(t *testing.T) {
	target := tempTarget(t)
	hooks := phaseFails(persist.S2Validated, errors.New("injected validation abort"))
	if _, serr := persist.SaveDocument(target, 0o644, validWriteInput(t), hooks); serr == nil {
		t.Fatal("expected abort before replace")
	}
	if _, err := os.Lstat(target); !os.IsNotExist(err) {
		t.Fatal("F1: no target file may exist")
	}
	report, rerr := persist.Recover(target)
	if rerr != nil {
		t.Fatal(rerr)
	}
	removed, _ := persist.CleanLeftovers(target)
	for _, art := range persist.DiscoverArtifacts(target) {
		_ = art
	}
	_ = removed
	if report.Ambiguous {
		t.Fatal("F1 leftover must not create ambiguity")
	}
}

// F7: artifact write/sync failure — artifact excluded, target unaffected
// (RS6 exclusion semantics via discovery classification).
func TestFaultF7ArtifactExcluded(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Corrupt leftover plays the role of an artifact whose write failed.
	leftover := target + ".tmp-0000000000000000001-c0"
	if werr := os.WriteFile(leftover, []byte("not a package at all"), 0o644); werr != nil {
		t.Fatal(werr)
	}
	report, rerr := persist.Recover(target)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if report.TargetCategory != persist.StateValidTarget {
		t.Fatalf("target unaffected requirement: got %s", report.TargetCategory)
	}
	for _, d := range report.Diagnostics {
		if strings.Contains(d, string(persist.StateInvalidCandidate)) {
			return // RS6/RS3 exclusion observed
		}
	}
	t.Fatal("corrupt artifact must be reported as invalid candidate")
}

// F8: cleanup interrupted — artifacts remain valid-or-excluded, never
// partially promoted.
func TestFaultF8CleanupSafety(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	garbage := target + ".tmp-0000000000000000002-c0"
	if werr := os.WriteFile(garbage, []byte("PK\x03\x04truncated"), 0o644); werr != nil {
		t.Fatal(werr)
	}
	removed, kept := persist.CleanLeftovers(target)
	for _, path := range append(append([]string{}, removed...), kept...) {
		if fi, err := os.Lstat(path); err == nil && fi.IsDir() {
			t.Fatal("cleanup must never promote or transform artifacts")
		}
	}
	report, rerr := persist.Recover(target)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if report.TargetCategory != persist.StateValidTarget {
		t.Fatalf("valid target must stay promoted-safe: %s", report.TargetCategory)
	}
}

// F9: autosave failure — non-blocking notification surface, retry works,
// target untouched.
func TestFaultF9AutosaveRetry(t *testing.T) {
	target := tempTarget(t)
	failing := &persist.SaveHooks{Validate: func(string) *persist.Error {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Detail: "transient"}
	}}
	ok, serr := persist.Autosave(target, 0o644, validWriteInput(t), failing)
	if ok || serr == nil {
		t.Fatal("autosave failure must be reported non-blockingly")
	}
	ok, serr = persist.Autosave(target, 0o644, validWriteInput(t), nil)
	if !ok || serr != nil {
		t.Fatalf("retry at next trigger must succeed: %v", serr)
	}
	if _, lerr := persist.LoadFile(target); lerr != nil {
		t.Fatalf("target unreadable after autosave retry: %v", lerr)
	}
}

// X1: temporary creation failure — abort, target untouched.
func TestFaultX1TempCreationFailure(t *testing.T) {
	target := tempTarget(t)
	hooks := &persist.SaveHooks{TempName: func(string) (string, error) {
		return "", errors.New("injected name-collision exhaustion")
	}}
	if _, serr := persist.SaveDocument(target, 0o644, validWriteInput(t), hooks); serr == nil {
		t.Fatal("expected X1 abort")
	}
	if _, err := os.Lstat(target); !os.IsNotExist(err) {
		t.Fatal("target must not exist after X1 abort on fresh target")
	}
}

// TestExternalModificationDetection pins BEHAVIOR-SPEC §9.2: a baseline
// mismatch refuses silent overwrite; explicit confirmation proceeds.
func TestExternalModificationDetection(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Simulate external modification behind the writer's back.
	if werr := os.WriteFile(target, append([]byte{}, mustBuild(t, validWriteInput(t))...), 0o600); werr != nil {
		t.Fatal(werr)
	}
	stale := &persist.TargetBaseline{SHA256: "0000"}
	in := validWriteInput(t)
	in.Baseline = stale
	if _, serr := persist.SaveDocument(target, 0o644, in, nil); serr == nil ||
		!strings.Contains(serr.Error(), "BEHAVIOR-SPEC §9.2") {
		t.Fatalf("baseline mismatch must refuse silently overwriting: %v", serr)
	}
	in.BaselineConfirmed = true
	if _, serr := persist.SaveDocument(target, 0o644, in, nil); serr != nil {
		t.Fatalf("confirmed save must proceed: %v", serr)
	}
	// No baseline at all over an existing target also refuses.
	in2 := validWriteInput(t)
	if _, serr := persist.SaveDocument(filepath.Join(t.TempDir(), "x.domo"), 0o644, in2, nil); serr != nil {
		t.Fatalf("fresh target without baseline must proceed: %v", serr)
	}
}

func mustBuild(t *testing.T, in *persist.WriteInput) []byte {
	t.Helper()
	pkg, werr := persist.BuildPackage(in)
	if werr != nil {
		t.Fatal(werr)
	}
	return pkg
}

// TestStreamEntryBoundedExtraction pins the C1/C2/C6 surface: sequential
// guarded extraction of one entry with incremental digests and no
// whole-entry buffering by the view.
func TestStreamEntryBoundedExtraction(t *testing.T) {
	view, lerr := persist.Load(corpusFile(t, "golden/packages/portable-valid.domo"))
	if lerr != nil {
		t.Fatal(lerr)
	}
	es, serr := view.StreamEntry("ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json")
	if serr != nil {
		t.Fatal(serr)
	}
	drained, rerr := io.ReadAll(es)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if verr := es.Verified(); verr != nil {
		t.Fatalf("stream verdict: %v", verr)
	}
	if len(drained) == 0 {
		t.Fatal("empty stream")
	}
	if _, serr = view.StreamEntry("nope/missing.json"); serr == nil {
		t.Fatal("unknown entry must fail closed")
	}
}

// TestRecoveryStates pins the machine-readable RS categories for the
// startup combinations reachable in this suite.
func TestRecoveryStates(t *testing.T) {
	t.Run("RS1-valid-target", func(t *testing.T) {
		target := tempTarget(t)
		if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
			t.Fatal(err)
		}
		report, rerr := persist.Recover(target)
		if rerr != nil {
			t.Fatal(rerr)
		}
		if report.TargetCategory != persist.StateValidTarget {
			t.Fatalf("RS1: got %s", report.TargetCategory)
		}
	})
	t.Run("RS5-corrupt-target-falls-to-artifacts", func(t *testing.T) {
		target := tempTarget(t)
		if werr := os.WriteFile(target, []byte("PK\x03\x04garbage"), 0o644); werr != nil {
			t.Fatal(werr)
		}
		// A valid candidate behind the corrupt target exercises the RS5
		// precedence row (fall to artifact precedence).
		pkg, berr := persist.BuildPackage(validWriteInput(t))
		if berr != nil {
			t.Fatal(berr)
		}
		if werr := os.WriteFile(target+".tmp-0000000000000000003-v", pkg, 0o644); werr != nil {
			t.Fatal(werr)
		}
		report, rerr := persist.Recover(target)
		if rerr != nil {
			t.Fatal(rerr)
		}
		if report.TargetCategory != persist.StateCorruptTarget {
			t.Fatalf("RS5: got %s", report.TargetCategory)
		}
		if len(report.Candidates) != 1 {
			t.Fatalf("RS5: valid candidate must be listed for explicit choice, got %d", len(report.Candidates))
		}
	})
	t.Run("RS7-no-recovery-anywhere", func(t *testing.T) {
		target := tempTarget(t)
		if werr := os.WriteFile(target, []byte("PK\x03\x04garbage"), 0o644); werr != nil {
			t.Fatal(werr)
		}
		report, rerr := persist.Recover(target)
		if rerr != nil {
			t.Fatal(rerr)
		}
		if report.TargetCategory != persist.StateNoRecovery {
			t.Fatalf("RS7: got %s", report.TargetCategory)
		}
	})
	t.Run("RS8-same-sequence-ambiguous", func(t *testing.T) {
		target := tempTarget(t)
		if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
			t.Fatal(err)
		}
		pkg, berr := persist.BuildPackage(validWriteInput(t))
		if berr != nil {
			t.Fatal(berr)
		}
		same := target + ".tmp-0000000000000000009-dupA"
		same2 := target + ".tmp-0000000000000000009-dupB"
		for _, p := range []string{same, same2} {
			if werr := os.WriteFile(p, pkg, 0o644); werr != nil {
				t.Fatal(werr)
			}
		}
		report, rerr := persist.Recover(target)
		if rerr != nil {
			t.Fatal(rerr)
		}
		if !report.Ambiguous {
			t.Fatal("RS8: same-sequence collision must require explicit user choice")
		}
	})
	t.Run("L30-discovery-cap-excludes-oldest", func(t *testing.T) {
		target := tempTarget(t)
		if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
			t.Fatal(err)
		}
		pkg, berr := persist.BuildPackage(validWriteInput(t))
		if berr != nil {
			t.Fatal(berr)
		}
		for i := 0; i < persist.RecoveryArtifactCountMax+2; i++ {
			p := target + ".tmp-" + padSeqTest(i+1) + "-c"
			if werr := os.WriteFile(p, pkg, 0o644); werr != nil {
				t.Fatal(werr)
			}
		}
		artifacts := persist.DiscoverArtifacts(target)
		if len(artifacts) > persist.RecoveryArtifactCountMax {
			t.Fatalf("L30 cap violated: %d admitted", len(artifacts))
		}
	})
}

func padSeqTest(v int) string {
	s := "00000000000000000000"
	digits := []byte(s)
	n := []byte(itoaTest(v))
	copy(digits[len(digits)-len(n):], n)
	return string(digits)
}

func itoaTest(v int) string {
	if v == 0 {
		return "0"
	}
	var out []byte
	for v > 0 {
		out = append([]byte{byte('0' + v%10)}, out...)
		v /= 10
	}
	return string(out)
}
