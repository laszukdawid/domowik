package document

import (
	"errors"
	"math/rand"
	"reflect"
	"testing"
)

// closure_test.go exercises FORMAT-SPEC §4.4 F4 obligation 2 and the §4.1
// B5 partial-range rule it depends on. Every positive case here is paired
// with a control that must be ACCEPTED, because a validator that rejects
// everything satisfies every rejection assertion.

func TestF4RejectsFrontierPointingInsideAnAtomicBatch(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)

	// A reserves [1,2] — two operations in ONE atomic batch.
	twoOp := func() *Batch {
		return mustBatch(t, 1, x, 1, nil,
			mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "a")),
			mustCreateOp(t, 1001, levelJSON(t, numUUID(2), "b")))
	}

	t.Run("intermediate counter rejects", func(t *testing.T) {
		// B5: "Intermediate counters of a batch are never exposed in any
		// frontier." Observing A through counter 1 claims half of an
		// atomic batch, which B1 forbids any observer from projecting.
		b := mustBatch(t, 2, y, 1, map[UUID]int64{x: 1},
			mustSetOp(t, 2000, numUUID(1), setLevelName(t, "half")))
		err := New().Deliver(twoOp(), b)
		if !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("a frontier naming an intermediate counter must reject malformed-framing, got %v", err)
		}
	})

	t.Run("control: the batch END is accepted", func(t *testing.T) {
		b := mustBatch(t, 3, y, 1, map[UUID]int64{x: 2},
			mustSetOp(t, 2000, numUUID(1), setLevelName(t, "whole")))
		if err := New().Deliver(twoOp(), b); err != nil {
			t.Fatalf("observing the batch end is exactly what B5 permits: %v", err)
		}
	})

	t.Run("control: compacted ancestry is unprovable, not violating", func(t *testing.T) {
		// F4 obligation 3: closure is NOT provable for a batch whose
		// observed ancestors are no longer retained, and no reader may
		// claim otherwise. With A compacted away, counter 1 could have
		// been any batch's end, so the reader must accept rather than
		// invent a violation it cannot see.
		seed := State{
			Frontier:         map[UUID]int64{x: 2},
			Tombstones:       map[UUID]position{},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{},
			BatchPositions:   map[batchRef]map[UUID]int64{},
		}
		p, err := FromSnapshot(seed, nil)
		if err != nil {
			t.Fatalf("FromSnapshot: %v", err)
		}
		b := mustBatch(t, 4, y, 1, map[UUID]int64{x: 1},
			mustCreateOp(t, 3000, levelJSON(t, numUUID(7), "fresh")))
		if err := p.Deliver(b); err != nil {
			t.Fatalf("an unprovable observation must not be reported as a violation: %v", err)
		}
	})
}

func TestF4RejectsOmittedAncestorEntryBehindACompactedTerminal(t *testing.T) {
	x, y, z := actorID(actorX), actorID(actorY), actorID(actorLateC)

	// X's watermark is 10 with NO retained X batches, so X's TERMINAL batch
	// is compacted. An EARLIER X batch [1,1] with frontier {z:7} exists.
	// B claims to observe X through 10 but omits Z.
	//
	// BLOCKER 2 domain split: the same batch bytes have DIFFERENT validation
	// depending on whether they're RETAINED LOG or ordinary redelivery:
	// - RETAINED LOG: must validate before suppression (FromSnapshot domain)
	// - Ordinary redelivery: silently ignored, NOT ancestry evidence (Deliver domain)
	seed := State{
		Frontier:         map[UUID]int64{x: 10, z: 7},
		Tombstones:       map[UUID]position{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		BatchPositions:   map[batchRef]map[UUID]int64{},
	}
	early := func() *Batch {
		return mustBatch(t, 1, x, 1, map[UUID]int64{z: 7},
			mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "early")))
	}

	t.Run("retained-log domain: omitting ancestor observation rejects", func(t *testing.T) {
		// early() as RETAINED LOG: FromSnapshot must validate it BEFORE suppression.
		// If another retained batch under-reports z, FromSnapshot must reject.
		underReported := mustBatch(t, 2, y, 1, map[UUID]int64{x: 10},
			mustCreateOp(t, 2000, levelJSON(t, numUUID(5), "under-reported")))

		_, err := FromSnapshot(seed, []*Batch{early(), underReported})
		if !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("retained-log domain: a frontier that observes a retained ancestor must dominate its frontier, got %v", err)
		}
	})

	t.Run("ordinary-delivery domain: covered redelivery does not poison fresh", func(t *testing.T) {
		// early() as ordinary REDELIVERY: covered by watermark, silently ignored, NOT ancestry.
		// Fresh batch b observes x:10 but omits z. This is VALID because early() is NOT evidence.
		p, err := FromSnapshot(seed, nil)
		if err != nil {
			t.Fatalf("FromSnapshot: %v", err)
		}
		b := mustBatch(t, 3, y, 1, map[UUID]int64{x: 10},
			mustCreateOp(t, 2000, levelJSON(t, numUUID(5), "fresh-without-z")))
		if err := p.Deliver(early(), b); err != nil {
			t.Fatalf("ordinary-delivery domain: covered redelivery must not poison fresh batch, got %v", err)
		}
	})

	t.Run("control: carrying the ancestor's observation is accepted", func(t *testing.T) {
		p, err := FromSnapshot(seed, nil)
		if err != nil {
			t.Fatalf("FromSnapshot: %v", err)
		}
		b := mustBatch(t, 4, y, 1, map[UUID]int64{x: 10, z: 7},
			mustCreateOp(t, 2000, levelJSON(t, numUUID(5), "closed")))
		if err := p.Deliver(early(), b); err != nil {
			t.Fatalf("a transitively closed frontier must be accepted: %v", err)
		}
	})
}

func TestF4RequiresTheWritingActorsOwnPriorEnd(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)

	// ED-0037 comment (e): omitting the OWN-actor entry preserves that
	// actor's own stream order, because counterBase is the position's
	// second component — and nothing else. Across actors it under-reports
	// the causal past by the actor's whole prior stream and can invert a
	// comparison a complete frontier would have won.
	seed := mustBatch(t, 1, x, 1, nil, mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "seed")))

	t.Run("omitted own prior end rejects", func(t *testing.T) {
		second := mustBatch(t, 2, x, 2, nil, mustSetOp(t, 2000, numUUID(1), setLevelName(t, "second")))
		if err := New().Deliver(seed, second); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("a batch at counterBase 2 must report its own prior end 1, got %v", err)
		}
	})

	t.Run("wrong own prior end rejects", func(t *testing.T) {
		second := mustBatch(t, 3, x, 2, map[UUID]int64{x: 0},
			mustSetOp(t, 2000, numUUID(1), setLevelName(t, "second")))
		if err := New().Deliver(seed, second); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("own prior end must be exactly counterBase-1, got %v", err)
		}
	})

	t.Run("control: the exact own prior end is accepted", func(t *testing.T) {
		second := mustBatch(t, 4, x, 2, map[UUID]int64{x: 1},
			mustSetOp(t, 2000, numUUID(1), setLevelName(t, "second")))
		if err := New().Deliver(seed, second); err != nil {
			t.Fatalf("the exact own prior end must be accepted: %v", err)
		}
	})

	t.Run("control: an actor's first batch needs no own entry", func(t *testing.T) {
		first := mustBatch(t, 5, y, 1, nil, mustCreateOp(t, 3000, levelJSON(t, numUUID(9), "first")))
		if err := New().Deliver(first); err != nil {
			t.Fatalf("counterBase 1 has no prior operation to report: %v", err)
		}
	})
}

// TestClosureCheckerAgreesWithItsOracle is the differential test r0011
// required: the swept implementation must agree with the direct
// all-observed-ancestor statement of F4 on both valid and deliberately
// invalid sets. The optimization does not get to certify itself.
func TestClosureCheckerAgreesWithItsOracle(t *testing.T) {
	rng := rand.New(rand.NewSource(20260829))
	actors := []UUID{actorID(actorMain), actorID(actorX), actorID(actorY), actorID(actorLateC)}

	for trial := 0; trial < 400; trial++ {
		// Build a small multi-actor history with honest, closed frontiers,
		// then corrupt it with probability — dropped entries, under-reported
		// counters, and intermediate-counter observations.
		next := map[UUID]int64{}
		observed := map[UUID]map[UUID]int64{}
		var batches []*Batch
		n := 2 + rng.Intn(5)
		for i := 0; i < n; i++ {
			a := actors[rng.Intn(len(actors))]
			base := next[a] + 1
			ops := 1 + rng.Intn(2)
			fr := map[UUID]int64{}
			for z, c := range observed[a] {
				fr[z] = c
			}
			if base > 1 {
				fr[a] = base - 1
			}
			// Observe an existing batch end, closing transitively.
			if len(batches) > 0 && rng.Intn(2) == 0 {
				peer := batches[rng.Intn(len(batches))]
				if peer.ActorID != a {
					if fr[peer.ActorID] < peer.EndCounter() {
						fr[peer.ActorID] = peer.EndCounter()
					}
					for z, c := range peer.Frontier {
						if fr[z] < c {
							fr[z] = c
						}
					}
				}
			}
			ops2 := make([]*Operation, 0, ops)
			for k := 0; k < ops; k++ {
				ops2 = append(ops2, mustCreateOp(t, uint64(10000+trial*100+i*10+k),
					levelJSON(t, numUUID(uint64(trial*1000+i*10+k+1)), "e")))
			}
			b, err := NewBatch(numUUID(uint64(500000+trial*100+i)), a, base, fr, ops2...)
			if err != nil {
				t.Fatalf("build: %v", err)
			}
			next[a] = b.EndCounter()
			cp := map[UUID]int64{}
			for z, c := range fr {
				cp[z] = c
			}
			cp[a] = b.EndCounter()
			observed[a] = cp
			batches = append(batches, b)
		}

		switch rng.Intn(4) {
		case 1: // drop one frontier entry
			b := batches[rng.Intn(len(batches))]
			for z := range b.Frontier {
				delete(b.Frontier, z)
				break
			}
		case 2: // under-report one counter
			b := batches[rng.Intn(len(batches))]
			for z, c := range b.Frontier {
				if c > 0 {
					b.Frontier[z] = c - 1
					break
				}
			}
		case 3: // point at an intermediate counter of a multi-operation batch
			for _, peer := range batches {
				if len(peer.Operations) < 2 {
					continue
				}
				for _, b := range batches {
					if b.ActorID == peer.ActorID {
						continue
					}
					b.Frontier[peer.ActorID] = peer.CounterBase
					break
				}
				break
			}
		}

		gotErr := validateFrontierClosure(batches) != nil
		wantErr := validateFrontierClosureNaive(batches) != nil
		if gotErr != wantErr {
			t.Fatalf("trial %d: swept checker rejects=%v, oracle rejects=%v", trial, gotErr, wantErr)
		}
	}
}

// TestRetainedRangeOverlapBelowTheWatermarkRejects is the reordering
// ED-0046 assigns to this issue, and the witness is the one issue 0005's
// pair executed against the live Load path: a snapshot watermark of 100
// with retained ranges [70,71] and [71,71] overlapping at 71, both wholly
// AT OR BELOW that watermark.
//
// The structural check that would catch it used to run over the NON-SKIPPED
// set, and watermark classification had already removed both ranges from
// that set — ED-0040 queue item 4 called it "the Load path building its
// non-skipped set before per-actor range chaining". §4.5 I2 says the
// opposite in terms: watermark classification "never precedes, gates, or
// replaces the pre-replay structural checks §4.3 row 4 applies to the
// retained log, and a retained range at or below the watermark is exempt
// from none of them".
//
// The control matters as much as the case: an ordinary REDELIVERY at or
// below the watermark necessarily reoccupies a range the replica has
// already seen, and I2 requires that to be ignored silently. So the check
// is scoped to the retained log and must not fire on a duplicate.
func TestRetainedRangeOverlapBelowTheWatermarkRejects(t *testing.T) {
	x := actorID(actorX)
	seed := State{
		Frontier:         map[UUID]int64{x: 100},
		BatchPositions:   map[batchRef]map[UUID]int64{},
		WinnerPositions:  map[fieldKey]position{},
		CreatedPositions: map[UUID]position{},
		Tombstones:       map[UUID]position{},
	}

	t.Run("the recorded issue-0005 witness rejects", func(t *testing.T) {
		// Ranges [70,71] and [71,71], overlapping at 71. Recorded in
		// ED-0046 as executed against the live Load path, where Inspect
		// rejected and Load accepted.
		//
		// Note honestly that TWO independent rules now catch this shape:
		// the overlap check, and F4's partial-range rule, because the
		// second batch's frontier {x:70} points INSIDE the first batch's
		// range [70,71]. The next subtest isolates the overlap check on a
		// shape the partial-range rule cannot see, so neither rule is
		// credited with the other's work.
		a := mustBatch(t, 1, x, 70, map[UUID]int64{x: 69},
			mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "a")),
			mustCreateOp(t, 1001, levelJSON(t, numUUID(2), "b")))
		b := mustBatch(t, 2, x, 71, map[UUID]int64{x: 70},
			mustCreateOp(t, 1002, levelJSON(t, numUUID(3), "c")))
		if a.EndCounter() != 71 || b.EndCounter() != 71 {
			t.Fatalf("precondition: the ranges must overlap at 71, got [%d,%d] and [%d,%d]",
				a.CounterBase, a.EndCounter(), b.CounterBase, b.EndCounter())
		}
		if _, err := ReplayFromSnapshot(seed, []*Batch{a, b}); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("overlapping retained ranges must reject even wholly below the watermark, got %v", err)
		}
	})

	t.Run("overlap alone rejects, with no frontier pointing inside a range", func(t *testing.T) {
		// Ranges [70,70] and [70,71] share their counterBase, so they
		// overlap; both frontiers report 69, which lies inside neither
		// range. F4's partial-range rule therefore sees nothing here and
		// only the overlap check can reject — which is what makes this the
		// witness that isolates it.
		a := mustBatch(t, 6, x, 70, map[UUID]int64{x: 69},
			mustCreateOp(t, 4000, levelJSON(t, numUUID(7), "a")))
		b := mustBatch(t, 7, x, 70, map[UUID]int64{x: 69},
			mustCreateOp(t, 4001, levelJSON(t, numUUID(8), "b")),
			mustCreateOp(t, 4002, levelJSON(t, numUUID(9), "c")))
		if err := validateFrontierClosure([]*Batch{a, b}); err != nil {
			t.Fatalf("precondition: no frontier here points inside a range, got %v", err)
		}
		if _, err := ReplayFromSnapshot(seed, []*Batch{a, b}); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("two retained batches reserving the same counter must reject (B3), got %v", err)
		}
	})

	t.Run("control: an ordinary redelivery below the watermark stays silent", func(t *testing.T) {
		// I2: "already applied: ignore it silently and deterministically —
		// no effects, no error." A duplicate reoccupies its own range by
		// definition, so a check that read that as an overlap would turn
		// V1 and V5 into framing rejects.
		p, err := FromSnapshot(seed, nil)
		if err != nil {
			t.Fatalf("FromSnapshot: %v", err)
		}
		dup := mustBatch(t, 3, x, 70, map[UUID]int64{x: 69},
			mustCreateOp(t, 2000, levelJSON(t, numUUID(4), "dup")))
		before := p.State()
		if err := p.Deliver(dup, dup); err != nil {
			t.Fatalf("a redelivered covered batch must be ignored silently: %v", err)
		}
		if !reflect.DeepEqual(before, p.State()) {
			t.Fatal("a covered redelivery must have no effects")
		}
	})

	t.Run("control: disjoint retained ranges below the watermark are accepted", func(t *testing.T) {
		a := mustBatch(t, 4, x, 70, map[UUID]int64{x: 69},
			mustCreateOp(t, 3000, levelJSON(t, numUUID(5), "a")))
		b := mustBatch(t, 5, x, 71, map[UUID]int64{x: 70},
			mustCreateOp(t, 3001, levelJSON(t, numUUID(6), "b")))
		if _, err := ReplayFromSnapshot(seed, []*Batch{a, b}); err != nil {
			t.Fatalf("disjoint retained ranges must load: %v", err)
		}
	})
}

// TestClosureSweepFoldsItsLastObservedAncestor is a hand-built witness for
// the one part of the sweep the random differential test does not reliably
// reach: the ancestor at the END of an actor's ascending list.
//
// The differential test caught a sweep that folded NOTHING, but not an
// off-by-one that folds all ancestors except the last, because its
// generator rarely places the deciding ancestor there for a given
// threshold. That is the aggregate-versus-witness distinction ED-0037
// comment (n) records, met head-on: a large random aggregate is soundness
// evidence for the rule adopted, and a hand-built witness is what
// discriminates.
//
//	A#1  [1,1] frontier {}
//	A#2  [2,2] frontier {A:1, Z:5}   <- last in A's list, and the decider
//	B    observes A through 2 but omits Z
//
// Only folding A#2 exposes the omission, so a loop stopping one short
// certifies B.
func TestClosureSweepFoldsItsLastObservedAncestor(t *testing.T) {
	a, b, z := actorID(actorX), actorID(actorY), actorID(actorLateC)

	first := mustBatch(t, 1, a, 1, nil, mustCreateOp(t, 1000, levelJSON(t, numUUID(1), "one")))
	last := mustBatch(t, 2, a, 2, map[UUID]int64{a: 1, z: 5},
		mustCreateOp(t, 1001, levelJSON(t, numUUID(2), "two")))

	t.Run("omitting the last ancestor's observation rejects", func(t *testing.T) {
		observer := mustBatch(t, 3, b, 1, map[UUID]int64{a: 2},
			mustCreateOp(t, 1002, levelJSON(t, numUUID(3), "three")))
		set := []*Batch{first, last, observer}
		if err := validateFrontierClosure(set); err == nil {
			t.Fatal("the observer must dominate the frontier of the LAST ancestor it observes")
		}
		if err := validateFrontierClosureNaive(set); err == nil {
			t.Fatal("the oracle must agree; if it does not, the two have diverged")
		}
	})

	t.Run("control: carrying the last ancestor's observation is accepted", func(t *testing.T) {
		observer := mustBatch(t, 4, b, 1, map[UUID]int64{a: 2, z: 5},
			mustCreateOp(t, 1003, levelJSON(t, numUUID(4), "four")))
		set := []*Batch{first, last, observer}
		if err := validateFrontierClosure(set); err != nil {
			t.Fatalf("a transitively closed frontier must be accepted: %v", err)
		}
		if err := validateFrontierClosureNaive(set); err != nil {
			t.Fatalf("the oracle must agree: %v", err)
		}
	})
}
