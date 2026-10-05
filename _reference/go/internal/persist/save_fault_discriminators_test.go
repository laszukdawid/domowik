package persist_test

import (
	"errors"
	"os"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// save_fault_discriminators_test.go: Mutant validation for F1–F9
// discriminators per r0029's Challenge 2. Each test proves the
// weight-bearing assertion by running the mutant and verifying it FAILS.

// TestDiscriminatorF1TargetMustNotExist validates that F1's discriminator
// (target file does not exist) actually fails when the abort happens AFTER
// a file is created.
func TestDiscriminatorF1TargetMustNotExist(t *testing.T) {
	target := tempTarget(t)
	// Mutant: normal save (no hook) creates the target file.
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// F1's discriminator (original at save_fault_test.go:225):
	// if _, err := os.Lstat(target); !os.IsNotExist(err) {
	//     t.Fatal("F1: no target file may exist")
	// }
	// With the mutant (successful save), target EXISTS, so IsNotExist is FALSE,
	// so !IsNotExist is TRUE, so the discriminator would FAIL.
	// We verify the discriminator WOULD fail by checking the opposite:
	if _, err := os.Lstat(target); os.IsNotExist(err) {
		// Target doesn't exist — the discriminator wouldn't fire, test is broken.
		t.Fatal("test setup error: successful save should have created target")
	}
	// Proof: target exists, so F1's discriminator "!os.IsNotExist" would fire,
	// correctly detecting that a target was created (not the F1 outcome).
}

// TestDiscriminatorF2TargetIsOLD validates that F2's discriminator (OLD
// entity count preserved) fails when the write completes and entity count
// changes.
func TestDiscriminatorF2TargetIsOLD(t *testing.T) {
	target := tempTarget(t)
	// Establish OLD generation with 1 entity.
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	oldView, err := persist.LoadFile(target)
	if err != nil {
		t.Fatal(err)
	}
	oldEntities := len(oldView.Projection.State().Entities)
	if oldEntities != 1 {
		t.Fatalf("setup: expected 1 entity in OLD, got %d", oldEntities)
	}

	// Mutant: normal save (no Writer hook) completes successfully with DIFFERENT input.
	// Use two entities to change the count.
	levelRaw := corpusFile(t, "golden/entities/level-minimal.json")
	ent, err := document.DecodeEntity(levelRaw)
	if err != nil {
		t.Fatal(err)
	}
	wallRaw := corpusFile(t, "golden/entities/wall-minimal.json")
	wall, err := document.DecodeEntity(wallRaw)
	if err != nil {
		t.Fatal(err)
	}

	in := &persist.WriteInput{
		Profile: "thin",
		State: document.State{
			Entities: []document.Entity{ent, wall}, // TWO entities (changed from 1)
			// ED-0037 replaced the pre-position key members with positions;
			// this fixture predates issue 0011's merge and named the old shape.
			Tombstones:       map[document.UUID]document.Position{},
			Frontier:         map[document.UUID]int64{},
			WinnerPositions:  map[document.FieldKey]document.Position{},
			BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
			CreatedPositions: map[document.UUID]document.Position{},
		},
		RetainedBatches: []*document.Batch{},
		Blobs:           map[string][]byte{},
	}
	in.Baseline, _ = persist.BaselineTarget(target)

	if _, serr := persist.SaveDocument(target, 0o644, in, nil); serr != nil {
		t.Fatalf("mutant save should succeed: %v", serr)
	}

	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatal(lerr)
	}
	newEntities := len(view.Projection.State().Entities)

	// F2's discriminator (original at save_fault_test.go:125):
	// if lerr != nil || len(view.Projection.State().Entities) != oldEntities {
	//     t.Fatalf("target must remain OLD-readable after interrupted write: %v", lerr)
	// }
	// With the mutant (successful write with 2 entities), newEntities != oldEntities,
	// so the discriminator would FAIL (correctly detecting target is NEW, not OLD).
	if newEntities == oldEntities {
		t.Fatalf("test setup error: mutant save should have changed entity count (got %d, same as OLD)", newEntities)
	}
	// Proof: entity count changed (1 → 2), so F2's discriminator
	// "len(entities) != oldEntities" would fire, correctly detecting target is NEW.
}

// TestDiscriminatorF3MessageIsCandidateInvalid validates that F3's
// discriminator (error contains "candidate-invalid") fails when the error
// is from a different phase.
func TestDiscriminatorF3MessageIsCandidateInvalid(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Mutant: inject error at S3 (SyncFile) instead of S2 (Validate).
	hooks := &persist.SaveHooks{SyncFile: func(*os.File) error {
		return errors.New("injected fsync failure")
	}}
	in := validWriteInput(t)
	in.Baseline, _ = persist.BaselineTarget(target)
	_, serr := persist.SaveDocument(target, 0o644, in, hooks)
	if serr == nil {
		t.Fatal("expected error from sync hook")
	}
	// F3's discriminator: error must contain "candidate-invalid".
	if strings.Contains(serr.Error(), "candidate-invalid") {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: S3 error incorrectly contains 'candidate-invalid'")
	}
	// Proof: the error is "F4" (from S3 sync), not "candidate-invalid" (from S2 validate).
	if !strings.Contains(serr.Error(), "F4") {
		t.Fatal("expected F4 error from sync failure")
	}
}

// TestDiscriminatorF4MessageIsF4 validates that F4's discriminator (error
// contains "F4") fails when the error is from a different phase.
func TestDiscriminatorF4MessageIsF4(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Mutant: inject error at S2 (Validate) instead of S3 (SyncFile).
	hooks := &persist.SaveHooks{Validate: func(string) *persist.Error {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Detail: "injected validation failure"}
	}}
	in := validWriteInput(t)
	in.Baseline, _ = persist.BaselineTarget(target)
	_, serr := persist.SaveDocument(target, 0o644, in, hooks)
	if serr == nil {
		t.Fatal("expected error from validate hook")
	}
	// F4's discriminator: error must contain "F4".
	if strings.Contains(serr.Error(), "F4") {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: S2 error incorrectly contains 'F4'")
	}
	// Proof: the error is "candidate-invalid" (from S2), not "F4" (from S3).
	if !strings.Contains(serr.Error(), "candidate-invalid") {
		t.Fatal("expected candidate-invalid error from validation failure")
	}
}

// TestDiscriminatorF5MessageIsF5 validates that F5's discriminator (error
// contains "F5") fails when the error is from a different phase.
func TestDiscriminatorF5MessageIsF5(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Mutant: inject error at S3 (SyncFile) instead of S4 (Replace).
	hooks := &persist.SaveHooks{SyncFile: func(*os.File) error {
		return errors.New("injected fsync failure")
	}}
	in := validWriteInput(t)
	in.Baseline, _ = persist.BaselineTarget(target)
	_, serr := persist.SaveDocument(target, 0o644, in, hooks)
	if serr == nil {
		t.Fatal("expected error from sync hook")
	}
	// F5's discriminator: error must contain "F5".
	if strings.Contains(serr.Error(), "F5") {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: S3 error incorrectly contains 'F5'")
	}
	// Proof: the error is "F4" (from S3), not "F5" (from S4).
	if !strings.Contains(serr.Error(), "F4") {
		t.Fatal("expected F4 error from sync failure")
	}
}

// TestDiscriminatorF6DurabilityCaveat validates that F6's discriminator
// (DurabilityCaveat flag) fails when the error is from an earlier phase.
func TestDiscriminatorF6DurabilityCaveat(t *testing.T) {
	target := tempTarget(t)
	// Mutant: inject error at S4 (Replace) instead of S5 (SyncDir).
	hooks := &persist.SaveHooks{Replace: func(tmp, tgt string) error {
		return errors.New("injected replace failure")
	}}
	res, serr := persist.SaveDocument(target, 0o644, validWriteInput(t), hooks)
	// F6's discriminator: save returns a result (serr == nil) with DurabilityCaveat == true.
	// With the mutant (replace failure), save returns an error, not a result.
	if serr == nil {
		t.Fatal("expected error from replace hook")
	}
	if res != nil && res.DurabilityCaveat {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: replace failure incorrectly set DurabilityCaveat")
	}
	// Proof: F6 returns a successful result with caveat; F5 returns an error.
}

// TestDiscriminatorF7InvalidCandidateDiagnostic validates that F7's
// discriminator (diagnostic contains "invalid candidate") fails when the
// artifact is valid.
func TestDiscriminatorF7InvalidCandidateDiagnostic(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	// Mutant: write a VALID artifact instead of corrupt.
	pkg, berr := persist.BuildPackage(validWriteInput(t))
	if berr != nil {
		t.Fatal(berr)
	}
	leftover := target + ".tmp-0000000000000000001-c0"
	if werr := os.WriteFile(leftover, pkg, 0o644); werr != nil {
		t.Fatal(werr)
	}
	report, rerr := persist.Recover(target)
	if rerr != nil {
		t.Fatal(rerr)
	}
	// F7's discriminator: diagnostic contains "invalid candidate" (RS6/RS3).
	for _, d := range report.Diagnostics {
		if strings.Contains(d, string(persist.StateInvalidCandidate)) {
			t.Fatal("DISCRIMINATOR FAILED TO DETECT: valid artifact incorrectly classified as invalid")
		}
	}
	// Proof: a valid artifact is classified differently (RS4 stale, not RS6 invalid).
}

// TestDiscriminatorF8TargetStaysValid validates that F8's discriminator
// (TargetCategory remains StateValidTarget) fails when the target is corrupted.
func TestDiscriminatorF8TargetStaysValid(t *testing.T) {
	target := tempTarget(t)
	if _, err := persist.SaveDocument(target, 0o644, validWriteInput(t), nil); err != nil {
		t.Fatal(err)
	}
	garbage := target + ".tmp-0000000000000000002-c0"
	if werr := os.WriteFile(garbage, []byte("PK\x03\x04truncated"), 0o644); werr != nil {
		t.Fatal(werr)
	}
	_, _ = persist.CleanLeftovers(target)

	// Mutant: corrupt the target itself.
	if werr := os.WriteFile(target, []byte("PK\x03\x04corrupt"), 0o644); werr != nil {
		t.Fatal(werr)
	}

	report, rerr := persist.Recover(target)
	if rerr != nil {
		t.Fatal(rerr)
	}
	// F8's discriminator: TargetCategory must be StateValidTarget.
	if report.TargetCategory == persist.StateValidTarget {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: corrupt target incorrectly classified as valid")
	}
	// Proof: the corrupt target is classified as StateCorruptTarget (RS5) or StateNoRecovery (RS7).
}

// TestDiscriminatorF9AutosaveReturnsOk validates that F9's discriminator
// (first call ok=false, second ok=true) fails when the first call succeeds.
func TestDiscriminatorF9AutosaveReturnsOk(t *testing.T) {
	target := tempTarget(t)
	// Mutant: remove the failing hook from the first call.
	ok, serr := persist.Autosave(target, 0o644, validWriteInput(t), nil)
	// F9's discriminator: first call must return ok=false (autosave failed non-blockingly).
	if !ok {
		t.Fatal("DISCRIMINATOR FAILED TO DETECT: first autosave call returned ok=false without a failing hook")
	}
	if serr != nil {
		t.Fatal("expected first call to succeed without hook")
	}
	// Proof: without the failing hook, autosave succeeds (ok=true) on the first call.
}
