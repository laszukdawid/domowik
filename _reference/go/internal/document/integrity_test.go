package document

// integrity_test.go: the eleven-defect wave r0011 found against the
// committed position implementation, adopted as tracked regression tests
// (ED-0054). Every case here was RED against the code as first submitted.
//
// The wave is worth reading as a set. Three of them — the rollback leak,
// the position-table leak, and the panic — are failures of what happens
// when something ELSE has already gone wrong, which is the region a happy
// path never visits. Four are snapshot shapes the type system permits and
// the specification does not. One is a non-conformance against §4.5 text
// accepted four days earlier. And the whole wave survived a green
// ./scripts/verify.sh on this branch, because the committed tests passed
// and these did not exist yet: "green therefore correct" is the inference
// that failed, not the run.

import (
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"reflect"
	"testing"
)

func emptyStateWithFrontier(frontier map[UUID]int64) State {
	return State{
		Frontier:         frontier,
		BatchPositions:   map[batchRef]map[UUID]int64{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		Tombstones:       map[UUID]position{},
	}
}

func TestRejectedCallLeavesNoBatchRegistered(t *testing.T) {
	actor := actorID(actorMain)
	goodID := numUUID(1)
	missingLevel := numUUID(99)
	good := mustBatch(t, 1, actor, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, goodID, "good")))
	bad := mustBatch(t, 2, actor, 2, map[UUID]int64{actor: 1},
		mustCreateOp(t, 1001, wallJSON(t, numUUID(2), missingLevel, 0, 0, 1, 0)))

	p := New()
	err := p.Deliver(good, bad)
	if err == nil {
		t.Fatal("bad batch must reject")
	}
	var rejected *BatchRejectedError
	if !errors.As(err, &rejected) {
		t.Fatalf("got %T, want BatchRejectedError", err)
	}
	if err := p.Deliver(good); err != nil {
		t.Fatalf("redelivering the good batch: %v", err)
	}
	if p.Entity(goodID) == nil {
		t.Fatal("the good batch was rolled back but retained as an applied duplicate")
	}
}

func TestRejectedCallLeavesNoPositionRefBehind(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	p, err := FromSnapshot(emptyStateWithFrontier(map[UUID]int64{y: 1}), nil)
	if err != nil {
		t.Fatal(err)
	}
	bad := mustBatch(t, 1, x, 1, nil,
		mustCreateOp(t, 1000, wallJSON(t, numUUID(1), numUUID(99), 0, 0, 1, 0)))
	if err := p.Deliver(bad); err == nil {
		t.Fatal("bad batch must reject")
	}
	corrected := mustBatch(t, 2, x, 1, map[UUID]int64{y: 1},
		mustCreateOp(t, 1001, levelJSON(t, numUUID(2), "corrected")))
	if err := p.Deliver(corrected); err != nil {
		t.Fatalf("a rejected batch leaked its ref/frontier into later replay: %v", err)
	}
}

func TestCollisionWithoutACreationPositionFailsClosed(t *testing.T) {
	e := numUUID(1)
	raw := levelJSON(t, e, "existing")
	ent, err := decodeEntity(raw)
	if err != nil {
		t.Fatal(err)
	}
	p, err := FromSnapshot(State{
		Entities:         []Entity{ent},
		Frontier:         map[UUID]int64{},
		BatchPositions:   map[batchRef]map[UUID]int64{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		Tombstones:       map[UUID]position{},
	}, nil)
	if err != nil {
		t.Fatal(err)
	}
	b := mustBatch(t, 1, actorID(actorX), 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "collision")))
	defer func() {
		if r := recover(); r != nil {
			t.Fatalf("snapshot-supported state caused a panic: %v", r)
		}
	}()
	if err := p.Deliver(b); err == nil {
		t.Fatal("collision without the retained position needed to decide it must fail closed")
	}
}

func TestCausallyOrderedDuplicateCreateIsAnO4Reject(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)
	p := New()
	first := mustBatch(t, 1, x, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "first")))
	if err := p.Deliver(first); err != nil {
		t.Fatal(err)
	}
	later := mustBatch(t, 2, y, 1, map[UUID]int64{x: 1},
		mustCreateOp(t, 1001, levelJSON(t, e, "later")))
	if err := p.Deliver(later); err == nil {
		t.Fatal("a causally ordered same-id create is an O4 reject, not an LWW4 collision")
	}
}

func TestSnapshotRejectsImpossiblePositions(t *testing.T) {
	x := actorID(actorX)
	e := numUUID(1)
	raw := levelJSON(t, e, "entity")
	ent, err := decodeEntity(raw)
	if err != nil {
		t.Fatal(err)
	}

	t.Run("self observation", func(t *testing.T) {
		ref := batchRef{Base: 1, Actor: x}
		s := State{
			Entities: []Entity{ent}, Frontier: map[UUID]int64{x: 1},
			BatchPositions:   map[batchRef]map[UUID]int64{ref: {x: 1}},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{e: {Ref: ref, OpIndex: 0}},
			Tombstones:       map[UUID]position{},
		}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("a referenced batch cannot observe its own reserved range")
		}
	})

	t.Run("position above snapshot frontier", func(t *testing.T) {
		ref := batchRef{Base: 2, Actor: x}
		s := State{
			Entities: []Entity{ent}, Frontier: map[UUID]int64{x: 1},
			BatchPositions:   map[batchRef]map[UUID]int64{ref: {x: 1}},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{e: {Ref: ref, OpIndex: 0}},
			Tombstones:       map[UUID]position{},
		}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("a snapshot cannot reference an operation above its own watermark")
		}
	})

	t.Run("operation counter overflow", func(t *testing.T) {
		ref := batchRef{Base: math.MaxInt64, Actor: x}
		s := State{
			Entities: []Entity{ent}, Frontier: map[UUID]int64{x: math.MaxInt64},
			BatchPositions:   map[batchRef]map[UUID]int64{ref: {}},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{e: {Ref: ref, OpIndex: 1}},
			Tombstones:       map[UUID]position{},
		}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("counterBase + operationIndex must remain within L13")
		}
	})
}

func TestSnapshotRejectsInvertedLiveInterval(t *testing.T) {
	actor := actorID(actorMain)
	e := numUUID(1)
	p := New()
	b := mustBatch(t, 1, actor, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "entity")),
		mustDeleteOp(t, 1001, e))
	if err := p.Deliver(b); err != nil {
		t.Fatal(err)
	}
	s := p.State()
	s.CreatedPositions[e], s.Tombstones[e] = s.Tombstones[e], s.CreatedPositions[e]
	if _, err := FromSnapshot(s, nil); err == nil {
		t.Fatal("creation must be strictly before deletion")
	}
}

func TestReplayAuthenticatesACoveredRetainedBatchFrontier(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)
	p := New()
	b := mustBatch(t, 1, x, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "entity")))
	if err := p.Deliver(b); err != nil {
		t.Fatal(err)
	}
	s := p.State()
	ref := batchRefOf(b)
	s.BatchPositions[ref] = map[UUID]int64{y: 1}
	s.Frontier[y] = 1
	if _, err := ReplayFromSnapshot(s, []*Batch{b}); err == nil {
		t.Fatal("a covered retained batch must authenticate the frontier stored under its ref")
	}
}

func TestReplayRejectsAnIndexOutsideItsRetainedBatch(t *testing.T) {
	x := actorID(actorX)
	e := numUUID(1)
	p := New()
	b := mustBatch(t, 1, x, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "entity")))
	if err := p.Deliver(b); err != nil {
		t.Fatal(err)
	}
	s := p.State()
	s.CreatedPositions[e] = position{Ref: batchRefOf(b), OpIndex: 1}
	if _, err := ReplayFromSnapshot(s, []*Batch{b}); err == nil {
		t.Fatal("a retained one-operation batch cannot be referenced at operation index 1")
	}
}

// TestReplayStageFailureIsAllOrNothing establishes, rather than argues, the
// fact r0011 and the orchestrator both identified as the one the session
// fence rewrite turns on.
//
// Deliver has two stages. A FRAMING rejection is provably pre-commit by
// inspection: Deliver returns frameAndOrder's error before replayAndCommit
// is ever called. What that says nothing about is a batch that fails during
// REPLAY — schema validation inside applyBatch, which is the path an
// out-of-range field value actually takes, and the path the session's
// external-fence case exercises.
//
// If replay could commit some batches before failing, partial acceptance
// would be reachable and the fence assertion that read it would be testing
// something real. It cannot: replayAndCommit rebuilds from the baseline and
// restores every committed map — including the applied-identity set, which
// was the leak — on the first error, and assigns p.log only on success. So
// the earlier assertion was pinned to the leak, not to a property.
func TestReplayStageFailureIsAllOrNothing(t *testing.T) {
	actor := actorID(actorMain)
	goodID, badID := numUUID(1), numUUID(2)

	good := mustBatch(t, 1, actor, 1, nil, mustCreateOp(t, 1000, levelJSON(t, goodID, "good")))
	badRaw := rawMap(t, levelJSON(t, badID, "bad"))
	badRaw["elevationMm"] = json.RawMessage(fmt.Sprint(-2 * CoordBoundMm))
	bad := mustBatch(t, 2, actor, 2, map[UUID]int64{actor: 1},
		&Operation{
			OperationID: numUUID(711),
			Kind:        OpCreateEntity,
			EntityID:    badID,
			Payload:     rawJSON(t, badRaw),
		})

	p := New()
	before := p.State()
	err := p.Deliver(good, bad)
	if err == nil {
		t.Fatal("the out-of-range value must reject")
	}

	// The stage matters: this must be a REPLAY-stage rejection, not a
	// framing one, or the case proves nothing about replayAndCommit.
	var rejected *BatchRejectedError
	if !errors.As(err, &rejected) {
		t.Fatalf("expected a user-input rejection from replay, got %T: %v", err, err)
	}
	if errors.Is(err, ErrMalformedFraming) {
		t.Fatal("this case must exercise the REPLAY stage; a framing reject never reaches replayAndCommit")
	}
	if rejected.BatchID != bad.BatchID {
		t.Fatalf("the named batch must be the invalid one, got %s", rejected.BatchID.String())
	}
	// Control: the SAME batch passes framing on its own, which is what makes
	// "it failed during replay" a fact rather than an inference.
	if ferr := validateFraming(bad); ferr != nil {
		t.Fatalf("the invalid batch must be structurally well-framed: %v", ferr)
	}

	// All-or-nothing: the good batch, which sorts FIRST in canonical order
	// and would have been applied before the invalid one was reached, left
	// nothing behind.
	if !reflect.DeepEqual(before, p.State()) {
		t.Fatalf("a replay-stage rejection must leave the projection untouched\n before %+v\n after  %+v",
			before, p.State())
	}
	if p.Entity(goodID) != nil {
		t.Fatal("the good batch must not survive a failed call")
	}
	if len(p.Log()) != 0 {
		t.Fatalf("no batch may join the delivered log on a failed call, got %d", len(p.Log()))
	}
	if err := p.Deliver(good); err != nil {
		t.Fatalf("the good batch must be redeliverable after the failed call: %v", err)
	}
	if p.Entity(goodID) == nil {
		t.Fatal("the redelivered good batch must apply")
	}
}

// TestRollbackEnumeratesEveryProjectionField is the guard ED-0056 requires,
// and it is the point of that ruling rather than an extra.
//
// Three separate leaks in this branch — the applied-identity set, the
// position table, and the restoration-authorization map — were one missing
// invariant: rollback must restore EVERY piece of state mutated before the
// failure point, including state that is not entity state. Fixing three
// fields by hand leaves the fourth for whoever next writes a pre-commit
// mutation, so the enumeration is derived from the STRUCT and asserted here.
// Adding a thirteenth field fails this test until someone classifies it.
func TestRollbackEnumeratesEveryProjectionField(t *testing.T) {
	// Every field of Projection, with what rollback owes it.
	const (
		rolledBack   = "restored by rollback"
		commitOnly   = "assigned only after every batch has applied"
		notDelivered = "never mutated by a delivery"
	)
	classified := map[string]string{
		"entities":         rolledBack,
		"createdPositions": rolledBack,
		"tombstones":       rolledBack,
		"winnerPositions":  rolledBack,
		"frontier":         rolledBack,
		"batches":          rolledBack,
		"ranges":           rolledBack,
		"diagnostics":      rolledBack,
		"positions":        rolledBack,
		"restorable":       rolledBack,
		"log":              commitOnly,
		"baseline":         notDelivered,
	}
	typ := reflect.TypeOf(Projection{})
	for i := 0; i < typ.NumField(); i++ {
		name := typ.Field(i).Name
		if _, ok := classified[name]; !ok {
			t.Fatalf("Projection.%s is not classified: every field must be declared as restored by "+
				"rollback, commit-only, or never mutated by a delivery (ED-0056)", name)
		}
	}
	if typ.NumField() != len(classified) {
		t.Fatalf("the classification lists %d fields but Projection has %d: the enumeration must be "+
			"derived from the struct, not from recall", len(classified), typ.NumField())
	}
}

// TestFailedRestorationTakesItsAuthorizationBack is r0011's reproducer for
// the third and worst leak. Entity and log state rolled back correctly and
// only CAPABILITY state survived: a failed private restoration left its
// authorization behind, and an ordinary PUBLIC delivery reusing that
// uncommitted batch id then consumed it to resurrect a tombstoned entity.
//
// A failed transaction conferring authority on a later unrelated batch is a
// different and worse class than a stale value surviving one, which is why
// I3's no-resurrection rule is what actually breaks here.
func TestFailedRestorationTakesItsAuthorizationBack(t *testing.T) {
	actor := actorID(actorMain)
	e := numUUID(1)
	p := New()
	create := mustBatch(t, 1, actor, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "entity")))
	remove := mustBatch(t, 2, actor, 2, map[UUID]int64{actor: 1}, mustDeleteOp(t, 1001, e))
	if err := p.Deliver(create, remove); err != nil {
		t.Fatalf("seed: %v", err)
	}
	deleting, gone := p.Tombstone(e)
	if !gone {
		t.Fatal("precondition: the entity must be tombstoned")
	}

	// A private restoring batch that WOULD be authorized, carrying a
	// dangling wall so it fails during replay after the tombstone is
	// cleared in the staging overlay.
	shared := numUUID(50)
	badRestore := mustBatch(t, 50, actor, 3, map[UUID]int64{actor: 2},
		mustCreateOp(t, 1010, levelJSON(t, e, "restored")),
		mustCreateOp(t, 1011, wallJSON(t, numUUID(2), numUUID(99), 0, 0, 1000, 0)))
	badRestore.BatchID = shared
	if err := p.deliverRestoring(badRestore, map[UUID]position{e: deleting}); err == nil {
		t.Fatal("the restoration batch's dangling reference must reject it")
	}
	if p.Entity(e) != nil {
		t.Fatal("a failed restoration must roll back the entity")
	}

	// An ORDINARY public delivery reusing the uncommitted batch id. Deliver
	// never registers a restorable entry, so this must resolve as an
	// ordinary post-tombstone create — stale, no resurrection (I3).
	public := mustBatch(t, 51, actor, 3, map[UUID]int64{actor: 2},
		mustCreateOp(t, 1020, levelJSON(t, e, "public")))
	public.BatchID = shared
	if err := p.Deliver(public); err != nil {
		t.Fatalf("an ordinary post-delete create resolves as stale, not as an error: %v", err)
	}
	if p.Entity(e) != nil {
		t.Fatal("authorization from a FAILED private restoration leaked to a later public batch " +
			"reusing its id, and resurrected a tombstoned entity (I3/E6)")
	}
	if _, still := p.Tombstone(e); !still {
		t.Fatal("the tombstone must survive")
	}
}

// TestRetainedPositionsAuthenticateTheNamedOperation is r0011's blocker 1.
// The index bound says an operation EXISTS at that index and nothing about
// whether it is the operation the key claims, so a snapshot could assert one
// entity's lifecycle from another entity's write.
func TestRetainedPositionsAuthenticateTheNamedOperation(t *testing.T) {
	actor := actorID(actorMain)
	e1, e2 := numUUID(1), numUUID(2)

	// One retained batch: index 0 creates e1, index 1 creates e2.
	retained := mustBatch(t, 1, actor, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e1, "one")),
		mustCreateOp(t, 1001, levelJSON(t, e2, "two")))
	p := New()
	if err := p.Deliver(retained); err != nil {
		t.Fatalf("seed: %v", err)
	}
	base := p.State()
	ref := batchRefOf(retained)
	wrong := position{Ref: ref, OpIndex: 1} // the operation for e2

	replay := func(t *testing.T, mutate func(*State)) error {
		t.Helper()
		s := base
		s.CreatedPositions = map[UUID]position{}
		for id, w := range base.CreatedPositions {
			s.CreatedPositions[id] = w
		}
		s.WinnerPositions = map[fieldKey]position{}
		for f, w := range base.WinnerPositions {
			s.WinnerPositions[f] = w
		}
		s.Tombstones = map[UUID]position{}
		mutate(&s)
		_, err := ReplayFromSnapshot(s, []*Batch{retained})
		return err
	}

	t.Run("a creation position naming another entity's create rejects", func(t *testing.T) {
		if err := replay(t, func(s *State) { s.CreatedPositions[e1] = wrong }); err == nil {
			t.Fatal("createdPositions[e1] must name a createEntity for e1 (S1)")
		}
	})

	t.Run("a winner position naming another entity's write rejects", func(t *testing.T) {
		if err := replay(t, func(s *State) {
			s.WinnerPositions[fieldKey{Entity: e1, Field: FieldName}] = wrong
		}); err == nil {
			t.Fatal("winnerPositions[e1.name] must name an operation writing e1.name (S3)")
		}
	})

	t.Run("a tombstone naming a create rejects", func(t *testing.T) {
		if err := replay(t, func(s *State) {
			s.Tombstones[e1] = position{Ref: ref, OpIndex: 0}
			delete(s.CreatedPositions, e1)
			s.CreatedPositions[e1] = position{Ref: ref, OpIndex: 0}
		}); err == nil {
			t.Fatal("tombstones[e1] must name a deleteEntity for e1 (S4)")
		}
	})

	t.Run("control: the correctly bound positions load", func(t *testing.T) {
		if err := replay(t, func(s *State) {}); err != nil {
			t.Fatalf("correctly bound positions must load: %v", err)
		}
	})
}
