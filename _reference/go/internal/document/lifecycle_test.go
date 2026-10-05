package document

import (
	"math"
	"reflect"
	"testing"
)

// lifecycle_test.go covers the parts of FORMAT-SPEC §4.5 I5 and §4.6 LWW1
// that only appear once a snapshot has folded a canonically LATER
// operation into the replay root. Every case compares a seeded replica
// against the never-compacted one, because that equality is what V6
// asserts and what a two-branch implementation silently breaks.

// TestCausalPastIsExact pins the arithmetic §4.4 F3 forbids implementations
// to get wrong: "implementations compare it EXACTLY, and wrapping or
// saturating arithmetic is FORBIDDEN because it would change the order."
func TestCausalPastIsExact(t *testing.T) {
	t.Run("sums that overflow int64 stay ordered", func(t *testing.T) {
		// Two counters just under the int64 ceiling. Their exact sum needs
		// a 65th bit, so a SIGNED 64-bit accumulator wraps to a negative
		// value — smaller than either input, which inverts the comparison
		// F3 says arithmetic may not change.
		two := sumFrontier(map[UUID]int64{
			actorID(actorX): math.MaxInt64,
			actorID(actorY): math.MaxInt64,
		})
		one := sumFrontier(map[UUID]int64{actorID(actorX): math.MaxInt64})
		if two.compare(one) <= 0 {
			t.Fatalf("2*MaxInt64 must exceed MaxInt64; got %s vs %s", two, one)
		}
	})

	t.Run("sums that overflow uint64 stay ordered", func(t *testing.T) {
		// Three counters cross the UNSIGNED boundary too: 3*(2^63-1) is
		// above 2^64-1, so an unsigned single-word accumulator wraps as
		// well. Only the two-word carry keeps the comparison exact, and
		// row L18 admits 65,536 members — four orders of magnitude more
		// than the three it takes to break a single word.
		three := sumFrontier(map[UUID]int64{
			actorID(actorX):     math.MaxInt64,
			actorID(actorY):     math.MaxInt64,
			actorID(actorLateC): math.MaxInt64,
		})
		two := sumFrontier(map[UUID]int64{
			actorID(actorX): math.MaxInt64,
			actorID(actorY): math.MaxInt64,
		})
		if three.hi == 0 {
			t.Fatal("3*MaxInt64 must carry into the high word")
		}
		if three.compare(two) <= 0 {
			t.Fatalf("3*MaxInt64 must exceed 2*MaxInt64; got %s vs %s", three, two)
		}
	})

	t.Run("the ratified maximum is representable and is not a round power of two", func(t *testing.T) {
		// Row L18 admits 65,536 frontier members and row L13 caps each at
		// the signed-64-bit ceiling, so the true maximum is
		// 65536 * (2^63 - 1) = 2^79 - 2^16 — NOT 2^79, which is not
		// attainable and would be a sloppier bound to assert.
		got := maxCausalPast()
		wantHi := uint64(1)<<15 - 1 // 2^79 - 2^16 = (2^15 - 1)·2^64 + (2^64 - 2^16)
		wantLo := ^uint64(0) - (uint64(1)<<16 - 1)
		if got.hi != wantHi || got.lo != wantLo {
			t.Fatalf("maximum causal past = {hi:%#x lo:%#x}, want {hi:%#x lo:%#x} (2^79 - 2^16)",
				got.hi, got.lo, wantHi, wantLo)
		}
	})

	t.Run("one more than the maximum still fits, so the bound is not the type's limit", func(t *testing.T) {
		if maxCausalPast().addCounter(1).compare(maxCausalPast()) <= 0 {
			t.Fatal("the accumulator must have headroom above the ratified maximum")
		}
	})
}

// TestOperationIndexIsBoundedBeforeNarrowing pins that a wire-supplied
// index is validated against row L23 as an int64, so persisted validity
// never depends on the host width of int.
func TestOperationIndexIsBoundedBeforeNarrowing(t *testing.T) {
	for _, tc := range []struct {
		name string
		idx  int64
		ok   bool
	}{
		{"zero", 0, true},
		{"last valid", MaxOperationsPerBatch - 1, true},
		{"at the ceiling", MaxOperationsPerBatch, false},
		{"negative", -1, false},
		{"beyond int32", math.MaxInt32 + 1, false},
	} {
		err := validateOperationIndex(tc.idx)
		if (err == nil) != tc.ok {
			t.Fatalf("%s: validateOperationIndex(%d) = %v, want ok=%v", tc.name, tc.idx, err, tc.ok)
		}
	}
}

// lateBatchWitness builds ED-0037 comment (n)'s falsified shape: entities E
// and F both live, a multi-operation late batch M writing to BOTH, and a
// delete D of E canonically AFTER M.
//
//	create  main, base 1, frontier {}        -> past 0 -> creates E and F
//	M       Y,    base 1, frontier {main:1}  -> past 1 -> sets E.name, F.name
//	D       Z,    base 1, frontier {main:1}  -> past 1 -> deletes E
//
// M and D share a causal past and a counterBase, so actorId separates them:
// actorLateC sorts above actorY, which puts D after M.
func lateBatchWitness(t *testing.T) (create, m, d *Batch, e, f UUID) {
	t.Helper()
	main, y, z := actorID(actorMain), actorID(actorY), actorID(actorLateC)
	e, f = numUUID(1), numUUID(2)
	create = mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "E-original")),
		mustCreateOp(t, 1001, levelJSON(t, f, "F-original")))
	m = mustBatch(t, 2, y, 1, map[UUID]int64{main: 2},
		mustSetOp(t, 2000, e, setLevelName(t, "E-from-M")),
		mustSetOp(t, 2001, f, setLevelName(t, "F-from-M")))
	d = mustBatch(t, 3, z, 1, map[UUID]int64{main: 2},
		mustDeleteOp(t, 3000, e))
	return create, m, d, e, f
}

// TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget is the case
// ED-0037 comment (n) recorded as FALSIFYING a fix that had already been
// verified at zero divergences over 40,342 splits: with creation and
// deleting positions retained but a TWO-BRANCH rule, a seeded replay
// produced {E.name: M, F.name: M} where full replay gives only {F.name: M}.
// It placed the batch correctly and then materialised M's write to E,
// resurrecting a field on a tombstoned entity.
func TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget(t *testing.T) {
	create, m, d, e, f := lateBatchWitness(t)

	full := New()
	if err := full.Deliver(create, m, d); err != nil {
		t.Fatalf("full replay: %v", err)
	}

	// The snapshot covers the create and the delete; M arrives afterwards,
	// so the seeded replica must place it INSIDE E's closed live interval.
	compacted := New()
	if err := compacted.Deliver(create, d); err != nil {
		t.Fatalf("compaction root: %v", err)
	}
	seeded, err := FromSnapshot(compacted.State(), nil)
	if err != nil {
		t.Fatalf("FromSnapshot: %v", err)
	}
	if err := seeded.Deliver(m); err != nil {
		t.Fatalf("late delivery: %v", err)
	}

	if !reflect.DeepEqual(seeded.State(), full.State()) {
		t.Fatalf("V6: seeded replay diverged from full replay\n seeded %+v\n full   %+v",
			seeded.State(), full.State())
	}

	// Stated explicitly as well as by the equality, because the equality
	// would also hold if BOTH paths were wrong in the same way.
	if seeded.Entity(e) != nil {
		t.Fatal("I3/LWW3: the canonically later delete supersedes M's write to E; E must stay tombstoned")
	}
	if _, gone := seeded.Tombstone(e); !gone {
		t.Fatal("E's tombstone must survive")
	}
	live := seeded.Entity(f)
	if live == nil {
		t.Fatal("F is not the delete's target and must remain live")
	}
	if got := live.(*Level).Name; got != "F-from-M" {
		t.Fatalf("M is not stale, so its write to F takes effect normally, got %q", got)
	}
	// The suppressed write still RECORDS its position: full replay records
	// it before the delete, and a delete does not erase winner positions,
	// so a seeded replica that skipped it would diverge on exactly the
	// metadata V6 compares.
	if _, ok := seeded.State().WinnerPositions[fieldKey{Entity: e, Field: FieldName}]; !ok {
		t.Fatal("the superseded write must still record its winner position")
	}
}

// TestLateCreateAgainstARetainedTombstone covers the four ways a
// createEntity can meet a tombstone. A create is never classified by the
// live interval — it ESTABLISHES existence at its own position — so
// "tombstoned therefore stale" is not the rule, and the blanket shortcut
// that shape suggests gets three of these wrong.
func TestLateCreateAgainstARetainedTombstone(t *testing.T) {
	main, y, z := actorID(actorMain), actorID(actorY), actorID(actorLateC)
	e := numUUID(1)

	// create  main, base 1, frontier {}       -> past 0
	// delete  Z,    base 1, frontier {main:1} -> past 1
	//
	// The colliding creates below carry an EMPTY frontier, which is what
	// makes them LWW4 collisions at all: §4.5 scopes LWW4 to "neither
	// create observing the other", and a create whose author had already
	// seen the first create is an O4 rejection instead. Past size 0 and
	// counterBase 1 then tie with the original, so actorId decides.
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "original")))
	del := mustBatch(t, 2, z, 1, map[UUID]int64{main: 1}, mustDeleteOp(t, 2000, e))

	root := func(t *testing.T) *Projection {
		t.Helper()
		compacted := New()
		if err := compacted.Deliver(create, del); err != nil {
			t.Fatalf("compaction root: %v", err)
		}
		p, err := FromSnapshot(compacted.State(), nil)
		if err != nil {
			t.Fatalf("FromSnapshot: %v", err)
		}
		return p
	}

	t.Run("pre-delete concurrent create that WINS moves the retained creation position", func(t *testing.T) {
		// Full history would have re-decided this LWW4 collision and then
		// tombstoned the winner, so the seeded replica must do the same
		// even though nothing materialises. Leaving the creation position
		// stale would misclassify every LATER late batch positioned between
		// the old and the new creation boundary — the retained position IS
		// the boundary between I5 branch 1 and branch 2.
		lateCreate := mustBatch(t, 3, y, 1, nil,
			mustCreateOp(t, 3000, levelJSON(t, e, "from-Y")))
		if _, observes := lateCreate.Frontier[main]; observes {
			t.Fatal("precondition: a LWW4 collision requires that neither create observes the other")
		}

		full := New()
		if err := full.Deliver(create, lateCreate, del); err != nil {
			t.Fatalf("full replay: %v", err)
		}
		seeded := root(t)
		if err := seeded.Deliver(lateCreate); err != nil {
			t.Fatalf("late create: %v", err)
		}
		if !reflect.DeepEqual(seeded.State(), full.State()) {
			t.Fatalf("V6: seeded diverged from full\n seeded %+v\n full   %+v", seeded.State(), full.State())
		}
		if got, want := seeded.State().CreatedPositions[e], pos(1, y, 0); got != want {
			t.Fatalf("the winning create must own the retained creation position: got %+v want %+v", got, want)
		}
		if seeded.Entity(e) != nil {
			t.Fatal("the canonically later delete still tombstones the winner; nothing may materialise")
		}
		if !hasDiagnostic(seeded, DiagAnomalousDiscardedCreate) {
			t.Fatal("LWW4 records the losing create as an anomalous discard")
		}
	})

	t.Run("pre-delete concurrent create that LOSES changes nothing", func(t *testing.T) {
		// Symmetric case: the retained creation position already belongs to
		// a greater write, so the arrival loses and must not move it. Its
		// batch is NOT stale, so any other operation it carries still runs.
		other := numUUID(9)
		greater := mustBatch(t, 4, z, 1, nil,
			mustCreateOp(t, 4000, levelJSON(t, e, "greater")))
		loser := mustBatch(t, 5, y, 1, nil,
			mustCreateOp(t, 5000, levelJSON(t, e, "loser")),
			mustCreateOp(t, 5001, levelJSON(t, other, "collateral")))
		delAfter := mustBatch(t, 6, main, 2, map[UUID]int64{main: 1},
			mustDeleteOp(t, 6000, e))

		full := New()
		if err := full.Deliver(create, greater, loser, delAfter); err != nil {
			t.Fatalf("full replay: %v", err)
		}
		if got, want := full.State().CreatedPositions[e], pos(1, z, 0); got != want {
			t.Fatalf("the greatest concurrent create owns the id: got %+v want %+v", got, want)
		}
		if full.Entity(other) == nil {
			t.Fatal("a losing create does not make its batch stale; its other operations still apply")
		}
		if full.Entity(e) != nil {
			t.Fatal("the delete still wins")
		}
	})

	t.Run("create positioned AFTER the delete is stale and resurrects nothing", func(t *testing.T) {
		// I3: a create positioned after a visible tombstone targets a
		// nonexistent identity and is resolved by I5's stale rule, without
		// recreating anything. The one exception E6 names — authorized
		// restoration by this device's own undo — is reachable only through
		// the unexported deliverRestoring entry point, never through
		// Deliver.
		// Observing the delete forces observing the create: F4 closure
		// requires this frontier to dominate the delete's {main:1}. So an
		// after-delete create has ALWAYS observed the original create —
		// which is why the after-delete branch is checked before the
		// collision branch, and why this case is a stale discard (I3/I5)
		// rather than the O4 rejection a causally ordered COLLISION gets.
		afterDelete := mustBatch(t, 7, y, 1, map[UUID]int64{main: 1, z: 1},
			mustCreateOp(t, 7000, levelJSON(t, e, "resurrection")))
		seeded := root(t)
		if err := seeded.Deliver(afterDelete); err != nil {
			t.Fatalf("delivery: %v", err)
		}
		if seeded.Entity(e) != nil {
			t.Fatal("no resurrection (I3)")
		}
		if !hasDiagnostic(seeded, DiagStaleBatchDiscard) {
			t.Fatal("a post-tombstone create is discarded stale and recorded in diagnostics")
		}
	})

	t.Run("a same-batch create still establishes existence for later operations", func(t *testing.T) {
		// B4/I5: "an entity created by an earlier operation of the same
		// batch is present for the later ones". Nothing above may weaken it.
		fresh := numUUID(21)
		p := New()
		b := mustBatch(t, 8, main, 1, nil,
			mustCreateOp(t, 8000, levelJSON(t, fresh, "created")),
			mustSetOp(t, 8001, fresh, setLevelName(t, "then-renamed")))
		if err := p.Deliver(b); err != nil {
			t.Fatalf("Deliver: %v", err)
		}
		ent := p.Entity(fresh)
		if ent == nil || ent.(*Level).Name != "then-renamed" {
			t.Fatalf("same-batch create-then-set must apply, got %+v", ent)
		}
	})
}

func hasDiagnostic(p *Projection, code DiagnosticCode) bool {
	for _, d := range p.Diagnostics() {
		if d.Code == code {
			return true
		}
	}
	return false
}

// TestSnapshotBatchTableIsExactlyTheReferencedSet pins §5.1 S3's "no
// unreferenced entries, and a reference to an absent stem is absent
// required data" in BOTH directions, and pins the empty case so a per-entry
// rule cannot pass vacuously over an empty collection (ED-0041 instance 3,
// which this issue produced).
func TestSnapshotBatchTableIsExactlyTheReferencedSet(t *testing.T) {
	main := actorID(actorMain)
	e := numUUID(1)
	p := New()
	if err := p.Deliver(mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, e, "L")))); err != nil {
		t.Fatalf("seed: %v", err)
	}
	base := p.State()
	if len(base.BatchPositions) != 1 {
		t.Fatalf("expected exactly the one referenced batch, got %d", len(base.BatchPositions))
	}

	t.Run("a reference to an absent batch rejects", func(t *testing.T) {
		s := base
		s.BatchPositions = map[batchRef]map[UUID]int64{}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("a position referencing a batch the table does not carry is absent required data")
		}
	})

	t.Run("an unreferenced batch entry rejects", func(t *testing.T) {
		s := base
		s.BatchPositions = map[batchRef]map[UUID]int64{}
		for ref, fr := range base.BatchPositions {
			s.BatchPositions[ref] = fr
		}
		s.BatchPositions[batchRef{Base: 99, Actor: actorID(actorY)}] = map[UUID]int64{}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("batchPositions must contain EXACTLY the referenced batches")
		}
	})

	t.Run("control: the exact table is accepted", func(t *testing.T) {
		if _, err := FromSnapshot(base, nil); err != nil {
			t.Fatalf("the exact table must load: %v", err)
		}
	})

	t.Run("empty maps are not vacuously satisfying", func(t *testing.T) {
		// The per-entry rules above iterate collections. Run them against
		// EMPTY ones and require the result to be the empty projection
		// rather than an accidental pass that says nothing.
		empty := State{
			Frontier:         map[UUID]int64{},
			BatchPositions:   map[batchRef]map[UUID]int64{},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{},
			Tombstones:       map[UUID]position{},
		}
		proj, err := FromSnapshot(empty, nil)
		if err != nil {
			t.Fatalf("the empty projection must load: %v", err)
		}
		if len(proj.State().Entities) != 0 || len(proj.State().BatchPositions) != 0 {
			t.Fatal("the empty snapshot must produce the empty projection")
		}
	})

	t.Run("a tombstone without its creation position rejects", func(t *testing.T) {
		// §5.1 S4 states this one in terms: without the creation position
		// the live interval cannot be formed and I5 branches 1 and 2
		// collapse. It is the one lifecycle datum that is never optional.
		s := base
		s.Tombstones = map[UUID]position{numUUID(7): {Ref: batchRef{Base: 1, Actor: main}, OpIndex: 0}}
		if _, err := FromSnapshot(s, nil); err == nil {
			t.Fatal("a tombstone with no creation position must reject")
		}
	})
}

// TestReadyKeyOrderDiffersFromCanonicalOrder is the witness for the
// ordering rule itself, built after this issue's first attempt at one
// failed to discriminate — recorded because the failure is instructive.
//
// The previous implementation walked the causal DAG topologically and broke
// ties by ascending (counterBase, actorId) among the batches that were
// READY. That always produced SOME linear extension of causality; the claim
// that it was ED-0036's unsatisfiable pairwise rule was wrong, and r0011
// corrected it. What it was not is THE canonical order.
//
// The first witness tried concurrent batches whose counter bases ran
// opposite to their causal past sizes and asserted a stale discard. It
// passed under BOTH orders, because I5 classifies a target against retained
// POSITIONS rather than against whatever happens to be live when a batch is
// reached — which is a property worth having, and which makes most outcomes
// order-independent by construction. The ordering is load-bearing exactly
// where a target's creation position is not yet staged at all:
//
//	seed  Y, base 1,  len 49, frontier {}      -> past 0
//	C     Y, base 50, len 1,  frontier {Y:49}  -> past 49 -> creates E
//	M     X, base 1,  len 1,  frontier {Y:50}  -> past 50 -> sets E.name
//
// M observed C — an ordinary causal edge — so under P it applies. Under the
// ready-set tie-break M's counterBase 1 sorts it before C, E has not been
// created yet, and M is discarded STALE. The two disagree on the field value
// AND on whether a stale diagnostic exists.
func TestReadyKeyOrderDiffersFromCanonicalOrder(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)

	fillers := make([]*Operation, 0, 49)
	for i := 0; i < 49; i++ {
		fillers = append(fillers, mustCreateOp(t, uint64(6000+i), levelJSON(t, numUUID(uint64(300+i)), "filler")))
	}
	seed := mustBatch(t, 1, y, 1, nil, fillers...)
	c := mustBatch(t, 2, y, 50, map[UUID]int64{y: 49},
		mustCreateOp(t, 7000, levelJSON(t, e, "created-by-C")))
	m := mustBatch(t, 3, x, 1, map[UUID]int64{y: 50},
		mustSetOp(t, 7100, e, setLevelName(t, "modified-by-M")))

	// The causal edge is the premise: M's frontier reaches C's end counter.
	if m.Frontier[y] < c.EndCounter() {
		t.Fatal("precondition: M must observe C, or the case is about concurrency instead")
	}

	p := New()
	if err := p.Deliver(seed, c, m); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	ent := p.Entity(e)
	if ent == nil {
		t.Fatal("C creates E under either order")
	}
	if got := ent.(*Level).Name; got != "modified-by-M" {
		t.Fatalf("M is causally after C and must apply; got %q — the ready-set tie-break would sort "+
			"M's counterBase 1 ahead of C's 50 and discard it stale", got)
	}
	if hasDiagnostic(p, DiagStaleBatchDiscard) {
		t.Fatal("nothing here is stale under the canonical order")
	}
}

// TestWritePositionUsesTheOperationIndexNotItsCounter pins ED-0037 comment
// (b)'s discriminator against the completion it rejected. The write position
// is F3's batch position refined by the operation's INDEX; using the
// operation COUNTER instead lets operation positions interleave batch
// ranges, which B1 forbids, and picks a different winner here.
//
//	A  actor Y, base 10, len 2, frontier {Y:9, main:1} -> past 10
//	B  actor X, base 11, len 1, frontier {X:10}        -> past 10
//
// Past sizes tie, so counterBase decides: 10 < 11, and B's write is last in
// canonical order. Under the counter reading, A's index-1 operation carries
// counter 11 and B's index-0 operation carries counter 11 too — they tie,
// actorId breaks it, and actorY sorts ABOVE actorX, so A would win instead.
func TestWritePositionUsesTheOperationIndexNotItsCounter(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	e := numUUID(1)

	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "base")))

	ySeedOps := make([]*Operation, 0, 9)
	for i := 0; i < 9; i++ {
		ySeedOps = append(ySeedOps, mustCreateOp(t, uint64(2000+i), levelJSON(t, numUUID(uint64(100+i)), "y-filler")))
	}
	ySeed := mustBatch(t, 2, y, 1, nil, ySeedOps...)

	xSeedOps := make([]*Operation, 0, 10)
	for i := 0; i < 10; i++ {
		xSeedOps = append(xSeedOps, mustCreateOp(t, uint64(3000+i), levelJSON(t, numUUID(uint64(200+i)), "x-filler")))
	}
	xSeed := mustBatch(t, 3, x, 1, nil, xSeedOps...)

	a := mustBatch(t, 4, y, 10, map[UUID]int64{y: 9, main: 1},
		mustSetOp(t, 4000, e, setLevelName(t, "A-index-0")),
		mustSetOp(t, 4001, e, setLevelName(t, "A-index-1")))
	b := mustBatch(t, 5, x, 11, map[UUID]int64{x: 10},
		mustSetOp(t, 5000, e, setLevelName(t, "B-index-0")))

	p := New()
	if err := p.Deliver(create, ySeed, xSeed, a, b); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	if got := p.Entity(e).(*Level).Name; got != "B-index-0" {
		t.Fatalf("counterBase must decide among equal past sizes, so B wins; got %q "+
			"(A wins only if the position is built from the operation counter)", got)
	}
	if got, want := p.State().WinnerPositions[fieldKey{Entity: e, Field: FieldName}], pos(11, x, 0); got != want {
		t.Fatalf("retained winner position %+v, want %+v", got, want)
	}
}
