package document

import (
	"encoding/json"
	"errors"
	"math"
	"reflect"
	"testing"
)

// setLevelName builds a one-field setFields payload.
func setLevelName(t testing.TB, v string) map[string]json.RawMessage {
	return map[string]json.RawMessage{FieldName: json.RawMessage(`"` + v + `"`)}
}

func TestProjectionCreateAndFrontierAdvance(t *testing.T) {
	p := New()
	lvl := numUUID(1)
	b := mustBatch(t, 1, actorID(actorMain), 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")))
	if err := p.Deliver(b); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	st := p.State()
	if len(st.Entities) != 1 || st.Entities[0].EntityID() != lvl {
		t.Fatalf("expected the created level, got %v", st.Entities)
	}
	if st.Frontier[actorID(actorMain)] != 1 {
		t.Fatalf("frontier must advance to the batch end (B5), got %d", st.Frontier[actorID(actorMain)])
	}
	if len(st.WinnerPositions) == 0 {
		t.Fatal("create must initialize winner positions for every present field (S3)")
	}
	want := pos(1, actorID(actorMain), 0)
	if got := st.WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}]; got != want {
		t.Fatalf("winner position %+v, want %+v", got, want)
	}
	// S3: the shared table carries EXACTLY the referenced batch, and its
	// frontier is what defines causalPastSize — here empty, so zero.
	if len(st.BatchPositions) != 1 {
		t.Fatalf("batchPositions must hold exactly the referenced batch, got %d", len(st.BatchPositions))
	}
	if fr, ok := st.BatchPositions[want.Ref]; !ok || len(fr) != 0 {
		t.Fatalf("referenced batch frontier %v ok=%v", fr, ok)
	}
}

func TestV1DuplicateDeliveryIgnoredSilently(t *testing.T) {
	p := New()
	b := mustBatch(t, 1, actorID(actorMain), 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "L1")))
	if err := p.Deliver(b); err != nil {
		t.Fatalf("first delivery: %v", err)
	}
	before := p.State()
	diags := len(p.Diagnostics())
	if err := p.Deliver(b); err != nil {
		t.Fatalf("redelivery must be silent (I2/V1): %v", err)
	}
	if !reflect.DeepEqual(before, p.State()) {
		t.Fatal("duplicate delivery changed state")
	}
	if len(p.Diagnostics()) != diags {
		t.Fatal("duplicate delivery added diagnostics")
	}
}

func TestV2ReorderedDeliveryHoldsForClosure(t *testing.T) {
	main := actorID(actorMain)
	other := actorID(actorX)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")))
	update := mustBatch(t, 2, other, 1, map[UUID]int64{main: 1},
		mustSetOp(t, 2000, lvl, setLevelName(t, "renamed")))

	p := New()
	err := p.Deliver(update)
	if !errors.Is(err, ErrCausalPrerequisiteMissing) {
		t.Fatalf("open set must refuse safely, got %v", err)
	}

	seq := New()
	if err := seq.Deliver(create); err != nil {
		t.Fatalf("sequential create: %v", err)
	}
	if err := seq.Deliver(update); err != nil {
		t.Fatalf("sequential update: %v", err)
	}
	set := New()
	if err := set.Deliver(update, create); err != nil {
		t.Fatalf("set delivery: %v", err)
	}
	if !reflect.DeepEqual(seq.State(), set.State()) {
		t.Fatal("V2: reordering arrival must not change the projection")
	}
}

func TestV3ConcurrentSetSetArrivalIndependence(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
	xw := mustBatch(t, 2, x, 1, nil, mustSetOp(t, 2000, lvl, setLevelName(t, "from-X")))
	yw := mustBatch(t, 3, y, 1, nil, mustSetOp(t, 3000, lvl, setLevelName(t, "from-Y")))

	var states []State
	for _, order := range [][]*Batch{{xw, yw, create}, {yw, create, xw}, {create, yw, xw}} {
		p := New()
		if err := p.Deliver(order...); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		states = append(states, p.State())
	}
	for i := 1; i < len(states); i++ {
		if !reflect.DeepEqual(states[0], states[i]) {
			t.Fatal("V3: outcome depends only on the delivered SET")
		}
	}
	ent := states[0].Entities[0].(*Level)
	// All three batches are an actor's first, so all three have causal past
	// size 0 and counterBase 1: the position's leading components tie and
	// actorId decides. actorY > actorX, so Y's write is last in canonical
	// order and wins (LWW1).
	if ent.Name != "from-Y" {
		t.Fatalf("winner should be the greatest-position writer, got %q", ent.Name)
	}
	if got, want := states[0].WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}], pos(1, y, 0); got != want {
		t.Fatalf("winner position must record Y's writing op, got %+v want %+v", got, want)
	}
}

func TestV4DeleteDominatesSet(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)

	for _, tc := range []struct {
		name       string
		deleter    UUID
		deleteBase int64
		batches    func() []*Batch
	}{
		{
			name:       "delete canonically precedes set",
			deleter:    x,
			deleteBase: 1,
			batches: func() []*Batch {
				return []*Batch{
					mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L"))),
					mustBatch(t, 2, x, 1, nil, mustDeleteOp(t, 4000, lvl)),
					mustBatch(t, 3, y, 1, nil, mustSetOp(t, 5000, lvl, setLevelName(t, "zombie"))),
				}
			},
		},
		{
			name:       "set canonically precedes delete",
			deleter:    y,
			deleteBase: 1,
			batches: func() []*Batch {
				return []*Batch{
					mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L"))),
					mustBatch(t, 2, x, 1, nil, mustSetOp(t, 4000, lvl, setLevelName(t, "zombie"))),
					mustBatch(t, 3, y, 1, nil, mustDeleteOp(t, 5000, lvl)),
				}
			},
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			bs := tc.batches()
			var states []State
			for _, order := range [][]*Batch{bs, {bs[2], bs[0], bs[1]}, {bs[1], bs[2], bs[0]}} {
				p := New()
				if err := p.Deliver(order...); err != nil {
					t.Fatalf("Deliver: %v", err)
				}
				states = append(states, p.State())
			}
			for i := 1; i < len(states); i++ {
				if !reflect.DeepEqual(states[0], states[i]) {
					t.Fatal("V4: arrival order changed the projection")
				}
			}
			if len(states[0].Entities) != 0 {
				t.Fatal("LWW3: deletes dominate; entity must stay deleted")
			}
			w, ok := states[0].Tombstones[lvl]
			if !ok {
				t.Fatal("tombstone must be retained (S4/I3)")
			}
			if want := pos(tc.deleteBase, tc.deleter, 0); w != want {
				t.Fatalf("deleting position mismatch: got %+v want %+v", w, want)
			}
			// S4: the creation position survives deletion so the live
			// interval [creation, deleting) can still be formed (I5).
			if _, ok := states[0].CreatedPositions[lvl]; !ok {
				t.Fatal("creation position must be retained for a tombstoned id (S1/S4)")
			}
		})
	}
}

func TestV4bStaleSetAfterObservedDeleteDiscardsWholeBatch(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L")))
	del := mustBatch(t, 2, x, 1, map[UUID]int64{main: 1}, mustDeleteOp(t, 2000, lvl))
	// Y observed X's delete (frontier claims it) yet still writes: stale.
	stale := mustBatch(t, 3, y, 1, map[UUID]int64{main: 1, x: 1},
		mustSetOp(t, 3000, lvl, setLevelName(t, "resurrected")),
		mustCreateOp(t, 3100, levelJSON(t, numUUID(9), "collateral")))

	p := New()
	if err := p.Deliver(create, del, stale); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	st := p.State()
	if len(st.Entities) != 0 {
		t.Fatal("I5: entire stale batch must be discarded — collateral create included")
	}
	if _, ok := st.Tombstones[lvl]; !ok {
		t.Fatal("no resurrection (R5.2.7 discipline)")
	}
	found := false
	for _, d := range p.Diagnostics() {
		if d.Code == DiagStaleBatchDiscard && d.BatchID == stale.BatchID {
			found = true
		}
	}
	if !found {
		t.Fatal("stale discard must be recorded in projection diagnostics (I5)")
	}
}

func TestV7ConcurrentCreateVersusModify(t *testing.T) {
	creator := actorID(actorY)  // later actor id loses ties
	modifier := actorID(actorX) // earlier actor id wins the canonical tie
	e := numUUID(1)
	create := mustBatch(t, 1, creator, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "created")))
	modify := mustBatch(t, 2, modifier, 1, nil, mustSetOp(t, 2000, e, setLevelName(t, "modified")))

	var states []State
	for _, order := range [][]*Batch{{create, modify}, {modify, create}} {
		p := New()
		if err := p.Deliver(order...); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		states = append(states, p.State())
	}
	for i := 1; i < len(states); i++ {
		if !reflect.DeepEqual(states[0], states[i]) {
			t.Fatal("V7: outcome is a function of the delivered set only")
		}
	}
	// The modify batch canonically precedes the create (equal counter, lower
	// actor id): it is discarded STALE in its entirety; the create stands.
	ent, ok := states[0].Entities[0].(*Level)
	if !ok || ent.Name != "created" {
		t.Fatalf("V7 stale path: create must win unmodified, got %+v", states[0].Entities)
	}
}

// TestCausalityGovernsOverTheCounterTieBreak replaces
// TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites, which pinned the
// ED-0032 conformance defect: it asserted that the greatest K won even
// where one write was causally LATER, and cited vector V3 while violating
// V3's own precondition ("neither observes the other") — its smallWrite
// carried frontier {main:1, y:1}, so X had observed Y.
//
// Under the write position the same three batches resolve the other way,
// and the derivation is the point:
//
//	create     main, base 1, frontier {}            -> past 0 -> (0,1,main,0)
//	bigWrite   Y,    base 1, frontier {main:1}      -> past 1 -> (1,1,Y,0)
//	smallWrite X,    base 1, frontier {main:1, Y:1} -> past 2 -> (2,1,X,0)
//
// X observed Y, so X's causal past is strictly larger and X wins REGARDLESS
// of counters — §4.6 LWW1's "CAUSALITY GOVERNS". The old rule compared
// K=(1,X) against K=(1,Y) and picked Y purely because actorY sorts higher,
// which is the tie-break applied to a pair that was never tied.
func TestCausalityGovernsOverTheCounterTieBreak(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
	bigWrite := mustBatch(t, 2, y, 1, map[UUID]int64{main: 1},
		mustSetOp(t, 2000, lvl, setLevelName(t, "observed-only-main")))
	laterWrite := mustBatch(t, 3, x, 1, map[UUID]int64{main: 1, y: 1},
		mustSetOp(t, 3000, lvl, setLevelName(t, "observed-main-and-y")))

	// The causal edge IS the test: if X stopped observing Y the two writes
	// would merely be concurrent, the tie-break would legitimately govern,
	// and this case would assert nothing about causality.
	if laterWrite.Frontier[y] < bigWrite.EndCounter() {
		t.Fatal("precondition: X's batch must observe Y's write")
	}
	if _, observes := bigWrite.Frontier[x]; observes {
		t.Fatal("precondition: Y must not observe X")
	}

	for _, order := range [][]*Batch{
		{create, bigWrite, laterWrite},
		{laterWrite, bigWrite, create},
		{bigWrite, create, laterWrite},
	} {
		p := New()
		if err := p.Deliver(order...); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		st := p.State()
		ent := st.Entities[0].(*Level)
		if ent.Name != "observed-main-and-y" {
			t.Fatalf("the causally later write must win, got %q", ent.Name)
		}
		if got, want := st.WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}], pos(1, x, 0); got != want {
			t.Fatalf("retained winner position %+v, want %+v (S3/LWW1)", got, want)
		}
	}
}

// TestWritePositionTieBreaksAmongEqualPast pins the components that decide
// only AFTER causalPastSize has tied. Each case isolates exactly one
// discriminator, so a comparator that dropped or reordered a component
// fails a single case rather than all three at once.
func TestWritePositionTieBreaksAmongEqualPast(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	other := numUUID(2)

	t.Run("equal past, greater counterBase wins", func(t *testing.T) {
		// X: a first batch at base 1, then base 2 with frontier {X:1} —
		// past 1, position (1,2,X,0).
		// Y: base 1 with frontier {main:1}          — past 1, position (1,1,Y,0).
		// Past sizes tie at 1; counterBase 2 beats 1, so X wins even though
		// actorX sorts BELOW actorY. An actor-first comparison picks Y.
		create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
		xSeed := mustBatch(t, 2, x, 1, nil, mustCreateOp(t, 1100, levelJSON(t, other, "unrelated")))
		xWrite := mustBatch(t, 3, x, 2, map[UUID]int64{x: 1},
			mustSetOp(t, 2000, lvl, setLevelName(t, "from-X")))
		yWrite := mustBatch(t, 4, y, 1, map[UUID]int64{main: 1},
			mustSetOp(t, 3000, lvl, setLevelName(t, "from-Y")))

		p := New()
		if err := p.Deliver(create, xSeed, xWrite, yWrite); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		st := p.State()
		if got, want := st.WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}], pos(2, x, 0); got != want {
			t.Fatalf("counterBase must decide among equal past sizes: got %+v want %+v", got, want)
		}
		if name := entityByIDForTest(t, st, lvl).(*Level).Name; name != "from-X" {
			t.Fatalf("value must follow the winning position, got %q", name)
		}
	})

	t.Run("equal past and counterBase, greater actorId wins", func(t *testing.T) {
		create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
		xWrite := mustBatch(t, 2, x, 1, map[UUID]int64{main: 1},
			mustSetOp(t, 2000, lvl, setLevelName(t, "from-X")))
		yWrite := mustBatch(t, 3, y, 1, map[UUID]int64{main: 1},
			mustSetOp(t, 3000, lvl, setLevelName(t, "from-Y")))

		p := New()
		if err := p.Deliver(create, xWrite, yWrite); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		st := p.State()
		if got, want := st.WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}], pos(1, y, 0); got != want {
			t.Fatalf("actorId must decide among equal past size and base: got %+v want %+v", got, want)
		}
		if name := entityByIDForTest(t, st, lvl).(*Level).Name; name != "from-Y" {
			t.Fatalf("value must follow the winning position, got %q", name)
		}
	})

	t.Run("within one batch, greater operationIndex wins", func(t *testing.T) {
		// Two writes in one batch share every leading component: they are
		// ORDERED rather than concurrent (B4), and only operationIndex
		// separates them (ED-0037 comment (s) corrected the decision body's
		// claim that equal past size implies concurrency).
		p := New()
		b := mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, lvl, "first")),
			mustSetOp(t, 1001, lvl, setLevelName(t, "second")),
			mustSetOp(t, 1002, lvl, setLevelName(t, "third")))
		if err := p.Deliver(b); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		st := p.State()
		if got, want := st.WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}], pos(1, main, 2); got != want {
			t.Fatalf("operationIndex must order same-batch writes: got %+v want %+v", got, want)
		}
		if name := entityByIDForTest(t, st, lvl).(*Level).Name; name != "third" {
			t.Fatalf("the last write in array order must win (B4), got %q", name)
		}
	})
}

func entityByIDForTest(t testing.TB, st State, id UUID) Entity {
	t.Helper()
	for _, e := range st.Entities {
		if e.EntityID() == id {
			return e
		}
	}
	t.Fatalf("entity %s absent from state", id.String())
	return nil
}

func TestSameActorDuplicateCreateRejectsBatch(t *testing.T) {
	actor := actorID(actorMain)
	e := numUUID(1)

	t.Run("two creates in one batch", func(t *testing.T) {
		p := New()
		dup := mustBatch(t, 20, actor, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, e, "first")),
			mustCreateOp(t, 1001, levelJSON(t, e, "second")))
		var bre *BatchRejectedError
		err := p.Deliver(dup)
		if !errors.As(err, &bre) || bre.BatchID != dup.BatchID {
			t.Fatalf("O4: same-batch duplicate ids must reject the batch atomically, got %v", err)
		}
		if len(p.State().Entities) != 0 {
			t.Fatal("rejection must leave zero effects")
		}
	})

	t.Run("same-actor create across batches", func(t *testing.T) {
		p := New()
		first := mustBatch(t, 21, actor, 1, nil, mustCreateOp(t, 2000, levelJSON(t, e, "first")))
		if err := p.Deliver(first); err != nil {
			t.Fatalf("seed: %v", err)
		}
		before := p.State()
		dup := mustBatch(t, 22, actor, 2, closedFrontier(actor, 2, nil), mustCreateOp(t, 3000, levelJSON(t, e, "again")))
		var bre *BatchRejectedError
		if !errors.As(p.Deliver(dup), &bre) {
			t.Fatal("O4/LWW4: duplicate id within one actor stream is impossible locally and must fail closed")
		}
		if !reflect.DeepEqual(before, p.State()) {
			t.Fatal("duplicate create had effects")
		}
	})
}

func TestLWW4ConcurrentCreateCollisionResolvesByGreaterK(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)
	fromX := levelJSON(t, e, "from-X")
	fromY := rawMap(t, levelJSON(t, e, "from-Y"))
	fromY["elevationMm"] = json.RawMessage("55")
	bx := mustBatch(t, 1, x, 1, nil, mustCreateOp(t, 1000, fromX))
	by := mustBatch(t, 2, y, 1, nil, mustCreateOp(t, 2000, rawJSON(t, fromY)))

	var states []State
	var anomalousCounts []int
	var lastP *Projection
	for _, order := range [][]*Batch{{bx, by}, {by, bx}} {
		p := New()
		if err := p.Deliver(order...); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		states = append(states, p.State())
		n := 0
		for _, d := range p.Diagnostics() {
			if d.Code == DiagAnomalousDiscardedCreate {
				n++
			}
		}
		anomalousCounts = append(anomalousCounts, n)
		lastP = p
	}
	if !reflect.DeepEqual(states[0], states[1]) {
		t.Fatal("LWW4: resolution must not depend on arrival order")
	}
	ent := states[0].Entities[0].(*Level)
	if ent.Name != "from-Y" || ent.ElevationMm != 55 {
		t.Fatalf("greater K (=Y) must be kept, got %+v", ent)
	}
	if anomalousCounts[0] == 0 || anomalousCounts[1] == 0 {
		t.Fatal("losing create must be recorded as anomalous-discarded-create")
	}
	for _, d := range lastP.Diagnostics() {
		if d.Code == DiagAnomalousDiscardedCreate && d.EntityID != e {
			t.Fatalf("anomalous create diag names wrong entity: %+v", d)
		}
	}
}

func TestFramingRejectsBeforeReplay(t *testing.T) {
	main := actorID(actorMain)
	lvl := numUUID(1)
	first := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")))

	t.Run("gap in counter ranges", func(t *testing.T) {
		p := New()
		if err := p.Deliver(first); err != nil {
			t.Fatalf("seed: %v", err)
		}
		before := p.State()
		gap := mustBatch(t, 2, main, 3, closedFrontier(main, 3, nil), mustSetOp(t, 1001, lvl, setLevelName(t, "n")))
		err := p.Deliver(gap)
		if !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("gap must reject as malformed-framing, got %v", err)
		}
		if !reflect.DeepEqual(before, p.State()) {
			t.Fatal("framing reject must leave zero effects")
		}
	})

	t.Run("duplicate batch id with differing content", func(t *testing.T) {
		p := New()
		if err := p.Deliver(first); err != nil {
			t.Fatalf("seed: %v", err)
		}
		before := p.State()
		forged := mustBatch(t, 1, main, 1, nil, mustSetOp(t, 1002, lvl, setLevelName(t, "forged")))
		err := p.Deliver(forged)
		if !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("forged duplicate id must reject, got %v", err)
		}
		if !reflect.DeepEqual(before, p.State()) {
			t.Fatal("forged delivery had effects")
		}
	})

	t.Run("self-frontier claim", func(t *testing.T) {
		if _, err := NewBatch(numUUID(7), main, 4, map[UUID]int64{main: 5},
			mustSetOp(t, 1003, lvl, setLevelName(t, "n"))); err == nil {
			t.Fatal("claiming observation of own reserved range must reject")
		}
	})

	t.Run("unknown operation kind refuses safely", func(t *testing.T) {
		rogue := &Operation{OperationID: numUUID(99), Kind: OpKind("purge"), EntityID: lvl}
		if _, err := NewBatch(numUUID(8), main, 2, nil, rogue); err == nil {
			t.Fatal("unknown kind must reject before replay (§4.3 row 4)")
		}
	})

	t.Run("counter overflow fail-closed", func(t *testing.T) {
		_, err := NewBatch(numUUID(9), main, math.MaxInt64-2, nil, make([]*Operation, 10)...)
		if err == nil {
			t.Fatal("overflowing reserved range must reject")
		}
	})

	t.Run("empty batch", func(t *testing.T) {
		if _, err := NewBatch(numUUID(10), main, 1, nil); err == nil {
			t.Fatal("empty batch is a malformed-framing reject (B3)")
		}
	})

	t.Run("zero operationId and entityId", func(t *testing.T) {
		if _, err := NewBatch(numUUID(13), main, 1, nil,
			&Operation{Kind: OpDeleteEntity, EntityID: lvl}); err == nil {
			t.Fatal("zero operationId must reject")
		}
		if _, err := NewBatch(numUUID(14), main, 1, nil,
			&Operation{OperationID: numUUID(15), Kind: OpDeleteEntity}); err == nil {
			t.Fatal("zero entityId must reject")
		}
	})

	t.Run("cross-actor causal cycle", func(t *testing.T) {
		x := actorID(actorX)
		a := mustBatch(t, 11, x, 1, map[UUID]int64{actorID(actorY): 1},
			mustSetOp(t, 1100, lvl, setLevelName(t, "a")))
		b := mustBatch(t, 12, actorID(actorY), 1, map[UUID]int64{x: 1},
			mustSetOp(t, 1200, lvl, setLevelName(t, "b")))
		p := New()
		if err := p.Deliver(a, b); !errors.Is(err, ErrMalformedFraming) && err != nil {
			t.Fatalf("cycle must surface as framing reject, got %v", err)
		} else if err == nil {
			t.Fatal("cycle accepted")
		}
	})
}

func TestUserInputRejectionIsBatchAtomic(t *testing.T) {
	main := actorID(actorMain)
	lvl := numUUID(1)
	wall := numUUID(2)
	p := New()
	seed := mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
		mustCreateOp(t, 1001, wallJSON(t, wall, lvl, 0, 0, 4000, 0)))
	if err := p.Deliver(seed); err != nil {
		t.Fatalf("seed: %v", err)
	}
	before := p.State()

	bad := mustBatch(t, 2, main, 3, closedFrontier(main, 3, nil),
		mustSetOp(t, 2000, lvl, setLevelName(t, "fine")),
		func() *Operation {
			op, err := NewSetFieldsOp(numUUID(2001), wall, map[string]json.RawMessage{
				FieldThicknessMm: json.RawMessage(`0`),
			})
			if err != nil {
				t.Fatalf("setup: %v", err)
			}
			return op
		}(),
	)
	err := p.Deliver(bad)
	var bre *BatchRejectedError
	if !errors.As(err, &bre) || bre.BatchID != bad.BatchID {
		t.Fatalf("user-input class must reject the whole batch, got %v", err)
	}
	if !reflect.DeepEqual(before, p.State()) {
		t.Fatal("atomicity (B1): zero effects allowed to escape the rejected batch")
	}
	if f := p.State().Frontier[main]; f != 2 {
		t.Fatalf("rejected batch must not advance the frontier, got %d", f)
	}
}

func TestCommitTimeReferenceValidation(t *testing.T) {
	main := actorID(actorMain)
	lvl := numUUID(1)
	ghost := numUUID(50)

	t.Run("dangling levelId rejects batch", func(t *testing.T) {
		p := New()
		err := p.Deliver(mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, wallJSON(t, numUUID(2), ghost, 0, 0, 1, 1))))
		var bre *BatchRejectedError
		if !errors.As(err, &bre) {
			t.Fatalf("O4 dangling reference must reject the batch, got %v", err)
		}
		if len(p.State().Entities) != 0 {
			t.Fatal("rejection must be batch-atomic")
		}
	})

	t.Run("same-batch creation satisfies references", func(t *testing.T) {
		p := New()
		if err := p.Deliver(mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
			mustCreateOp(t, 1001, wallJSON(t, numUUID(2), lvl, 0, 0, 4000, 2500)))); err != nil {
			t.Fatalf("cohesive bundle must commit: %v", err)
		}
	})

	t.Run("intra-batch deletion danglers reject", func(t *testing.T) {
		p := New()
		err := p.Deliver(mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
			mustCreateOp(t, 1001, wallJSON(t, numUUID(2), lvl, 0, 0, 4000, 2500)),
			mustDeleteOp(t, 1002, lvl)))
		var bre *BatchRejectedError
		if !errors.As(err, &bre) {
			t.Fatalf("post-batch referential integrity must hold, got %v", err)
		}
	})

	t.Run("wrong-kind target rejects", func(t *testing.T) {
		p := New()
		err := p.Deliver(mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
			func() *Operation {
				raw := rawMap(t, openingJSON(t, numUUID(3), lvl)) // hostWallId points at a LEVEL
				return mustCreateOp(t, 1001, rawJSON(t, raw))
			}()))
		var bre *BatchRejectedError
		if !errors.As(err, &bre) {
			t.Fatalf("opening hostWallId must reference a wall, got %v", err)
		}
	})

	t.Run("self-reference prohibited", func(t *testing.T) {
		p := New()
		self := rawMap(t, labelJSON(t, numUUID(7), numUUID(7)))
		self["levelId"] = numUUID(7).String()
		err := p.Deliver(mustBatch(t, 1, main, 1, nil,
			mustCreateOp(t, 1000, rawJSON(t, self))))
		var bre *BatchRejectedError
		if !errors.As(err, &bre) {
			t.Fatalf("self-reference must reject at commit, got %v", err)
		}
	})
}

func TestUnknownDataPreservedThroughUpdatesReadOnly(t *testing.T) {
	main := actorID(actorMain)
	rr := numUUID(6)
	lvl := numUUID(1)
	base := rawMap(t, roomRecordJSON(t, rr, lvl))
	base["vendorNote"] = json.RawMessage(`{"seq":7,"opaque":[1,2,3]}`)

	p := New()
	if err := p.Deliver(mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
		mustCreateOp(t, 1001, rawJSON(t, base)))); err != nil {
		t.Fatalf("create with unknown optional data: %v", err)
	}

	// Read-only: setFields cannot address preserved fields.
	err := p.Deliver(mustBatch(t, 2, main, 3, closedFrontier(main, 3, nil),
		mustSetOp(t, 2000, rr, map[string]json.RawMessage{
			"vendorNote": json.RawMessage(`{"seq":8}`),
		})))
	var bre *BatchRejectedError
	if !errors.As(err, &bre) {
		t.Fatalf("preserved unknown data must be read-only (§8/L32), got %v", err)
	}

	// Updates of schema fields leave preservation byte-exact.
	if err := p.Deliver(mustBatch(t, 3, main, 3, closedFrontier(main, 3, nil),
		mustSetOp(t, 3000, rr, map[string]json.RawMessage{
			FieldName: json.RawMessage(`"renamed-room"`),
		}))); err != nil {
		t.Fatalf("schema-field update: %v", err)
	}
	ent := p.Entity(rr).(*RoomRecord)
	if ent.Name == nil || *ent.Name != "renamed-room" {
		t.Fatalf("update lost: %v", ent.Name)
	}
	unk := ent.UnknownFields()
	if len(unk) != 1 || unk[0].Name != "vendorNote" || string(unk[0].Value) != `{"seq":7,"opaque":[1,2,3]}` {
		t.Fatalf("opaque preservation broken: %+v", unk)
	}
}

func TestStateIsImmutableDeepCopy(t *testing.T) {
	main := actorID(actorMain)
	lvl, rr := numUUID(1), numUUID(6)
	p := New()
	if err := p.Deliver(mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
		mustCreateOp(t, 1001, roomRecordJSON(t, rr, lvl)))); err != nil {
		t.Fatalf("seed: %v", err)
	}
	st := p.State()
	// Mutate everything reachable.
	st.Entities[0].(*Level).Name = "tampered"
	st.Frontier[main] = 999
	st.Tombstones[lvl] = pos(5, main, 0)
	for k := range st.WinnerPositions {
		st.WinnerPositions[k] = pos(77, main, 0)
	}
	for ref := range st.BatchPositions {
		st.BatchPositions[ref][main] = 12345
	}
	fresh := p.State()
	if fresh.Entities[0].(*Level).Name == "tampered" {
		t.Fatal("State leaked mutable references")
	}
	if fresh.Frontier[main] == 999 {
		t.Fatal("Frontier map is shared")
	}
	if _, ok := fresh.Tombstones[lvl]; ok {
		t.Fatal("Tombstones map is shared")
	}
	// Both loops are per-entry tests and would pass over an empty map
	// without examining anything, so the maps are required to be populated
	// before they are scanned.
	if len(fresh.WinnerPositions) == 0 || len(fresh.BatchPositions) == 0 {
		t.Fatalf("the sharing check needs populated maps to scan, got %d winner positions and %d batch entries",
			len(fresh.WinnerPositions), len(fresh.BatchPositions))
	}
	for k, v := range fresh.WinnerPositions {
		if v.Ref.Base == 77 {
			t.Fatalf("WinnerPositions map is shared at %v", k)
		}
	}
	for ref, fr := range fresh.BatchPositions {
		if fr[main] == 12345 {
			t.Fatalf("BatchPositions frontier is shared at %v", ref)
		}
	}
	clone := p.Entity(lvl).(*Level)
	clone.Name = "also-tampered"
	if p.Entity(lvl).(*Level).Name != "L1" {
		t.Fatal("Entity() must return an independent copy")
	}
}

func TestNullClearRecordsWinnerPositionWrite(t *testing.T) {
	main := actorID(actorMain)
	rr := numUUID(6)
	lvl := numUUID(1)
	base := rawMap(t, roomRecordJSON(t, rr, lvl))
	base[FieldFloorStyleRef] = "F0"

	p := New()
	if err := p.Deliver(mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")),
		mustCreateOp(t, 1001, rawJSON(t, base)))); err != nil {
		t.Fatalf("seed: %v", err)
	}
	// F4: the writer's own prior end counter is a required frontier entry,
	// not an optional one (ED-0037 comment (e)).
	clear := mustBatch(t, 2, main, 3, map[UUID]int64{main: 2},
		mustSetOp(t, 2000, rr, map[string]json.RawMessage{
			FieldFloorStyleRef: json.RawMessage(`null`),
		}))
	if err := p.Deliver(clear); err != nil {
		t.Fatalf("null clear: %v", err)
	}
	w, ok := p.State().WinnerPositions[fieldKey{Entity: rr, Field: FieldFloorStyleRef}]
	if !ok || w != pos(3, main, 0) {
		t.Fatalf("a null clear is a write and must record its position (S3), got %+v ok=%v", w, ok)
	}
	if e := p.Entity(rr).(*RoomRecord); e.FloorStyleRef != nil {
		t.Fatal("optional must be cleared to absent")
	}
}

func TestStaleDiscardConsumesRangeDeterministically(t *testing.T) {
	creator := actorID(actorY)
	modifier := actorID(actorX)
	e := numUUID(1)
	create := mustBatch(t, 1, creator, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "made")))
	stale := mustBatch(t, 2, modifier, 1, nil, mustSetOp(t, 2000, e, setLevelName(t, "too-early")))
	next := mustBatch(t, 3, modifier, 2, closedFrontier(modifier, 2, nil),
		mustSetOp(t, 2100, e, setLevelName(t, "after-stale")))

	p := New()
	if err := p.Deliver(stale, next, create); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	// Canonical order: stale(modifier@1) → create(creator@1? tie by actor:
	// creator id > modifier id, so stale first) … stale discards; create
	// applies; next applies onto the live entity.
	if got := p.Entity(e).(*Level).Name; got != "after-stale" {
		t.Fatalf("counters of a discarded batch are consumed once; later batch applies, got %q", got)
	}
	if err := p.Deliver(stale); err != nil {
		t.Fatalf("redelivery of a consumed stale batch must be silent: %v", err)
	}
}
