package document

import (
	"reflect"
	"testing"
)

// TestZeroFrontierEquivalentToAbsence is the permanent regression test for
// BLOCKER 1. A zero-valued snapshot frontier entry must be observably
// equivalent to an omitted actor, per F1 ("an absent actor is nothing
// observed").
//
// On dd17124, a frontier entry {actor: X, counter: 0} was accepted by the
// wire parser, stored with an empty range slice by FromSnapshot, then reached
// chainAndOrderOver where it unconditionally indexed the final range element
// and panicked. After snapshot S2 frontier normalization, zero is treated as
// absence and no panic occurs.
//
// This test uses the public ReplayFromSnapshot path and asserts exact
// observable equivalence between zero and absence via State(), Log(), and
// Diagnostics() comparison.
func TestZeroFrontierEquivalentToAbsence(t *testing.T) {
	x := actorID(actorX)

	// Minimal pair: identical empty snapshots, one with {X: 0}, one with X absent.
	// Both replayed with nil log.

	// Test case: snapshot with frontier {X: 0}
	seedWithZero := State{
		Entities:         []Entity{},
		Frontier:         map[UUID]int64{x: 0},
		BatchPositions:   map[batchRef]map[UUID]int64{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		Tombstones:       map[UUID]position{},
	}

	// Control: snapshot with X absent (empty frontier)
	seedOmitted := State{
		Entities:         []Entity{},
		Frontier:         map[UUID]int64{},
		BatchPositions:   map[batchRef]map[UUID]int64{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		Tombstones:       map[UUID]position{},
	}

	// Assert fixture construction: seedWithZero actually contains X:0
	if v, present := seedWithZero.Frontier[x]; !present || v != 0 {
		t.Fatalf("fixture construction error: seedWithZero must contain X:0, got present=%v v=%d",
			present, v)
	}

	// Replay control FIRST so mutation proof establishes it succeeds before zero panics
	pOmitted, err := ReplayFromSnapshot(seedOmitted, nil)
	if err != nil {
		t.Fatalf("ReplayFromSnapshot with omitted actor: %v", err)
	}

	// Replay zero fixture (panics on dd17124 without normalization)
	pWithZero, err := ReplayFromSnapshot(seedWithZero, nil)
	if err != nil {
		t.Fatalf("ReplayFromSnapshot with zero frontier: %v", err)
	}

	// Assert exact observable equivalence: State, Log, Diagnostics
	stateWithZero := pWithZero.State()
	stateOmitted := pOmitted.State()

	if !reflect.DeepEqual(stateWithZero, stateOmitted) {
		t.Errorf("State() mismatch:\nwith zero: %+v\nomitted:   %+v",
			stateWithZero, stateOmitted)
	}

	logWithZero := pWithZero.Log()
	logOmitted := pOmitted.Log()

	if !reflect.DeepEqual(logWithZero, logOmitted) {
		t.Errorf("Log() mismatch:\nwith zero: %+v\nomitted:   %+v",
			logWithZero, logOmitted)
	}

	diagWithZero := pWithZero.Diagnostics()
	diagOmitted := pOmitted.Diagnostics()

	if !reflect.DeepEqual(diagWithZero, diagOmitted) {
		t.Errorf("Diagnostics() mismatch:\nwith zero: %+v\nomitted:   %+v",
			diagWithZero, diagOmitted)
	}

	// X should not appear in the zero-frontier result after normalization
	if _, hasX := stateWithZero.Frontier[x]; hasX {
		t.Errorf("zero-frontier case incorrectly retained actor X in frontier")
	}
}
