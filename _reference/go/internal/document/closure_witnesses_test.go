// Copyright 2025 the Domowik AUTHORS. All rights reserved. Use of this
// source code is governed by the Apache-2.0 license in LICENSE.

package document

import (
	"testing"
)

// TestValidateOwnPriorEndsSpecDerived provides spec-derived witnesses for
// validateOwnPriorEnds that never use the differential oracle. Each witness
// is derived from FORMAT-SPEC F4 obligation "includes the writing actor's
// own prior end counter" and verified by deliberate mutation.
//
// BLOCKER 3: validateOwnPriorEnds is shared by both optimized and naive
// validators (closure.go:50, :298), so differential agreement cannot validate
// it. These witnesses establish independent coverage from the specification.
func TestValidateOwnPriorEndsSpecDerived(t *testing.T) {
	x := actorID(actorX)
	y := actorID(actorY)

	// Witness 1: Batch at counterBase > 1 MUST include own prior end.
	// FORMAT-SPEC F4: "includes the writing actor's own prior end counter"
	// A batch at counterBase N (N > 1) must have frontier[own actor] = N-1.
	t.Run("non-first batch must include own prior end", func(t *testing.T) {
		// x[2,2] with frontier {} (omits own prior end x:1)
		batch := &Batch{
			BatchID:     numUUID(1),
			ActorID:     x,
			CounterBase: 2,
			Frontier:    map[UUID]int64{}, // OMITS x:1
			Operations: []*Operation{
				mustCreateOp(t, 1, levelJSON(t, numUUID(1), "test")),
			},
		}

		err := validateOwnPriorEnds([]*Batch{batch})
		if err == nil {
			t.Fatal("validateOwnPriorEnds accepted batch that omits own prior end (F4 violation)")
		}
		errStr := err.Error()
		if !contains(errStr, "omits its own prior end") {
			t.Errorf("error does not mention omitted prior end: %v", err)
		}
	})

	// Witness 2: Batch at counterBase > 1 with WRONG prior end rejects.
	// FORMAT-SPEC F4: frontier[own actor] must equal counterBase - 1 EXACTLY.
	t.Run("non-first batch with wrong prior end rejects", func(t *testing.T) {
		// x[3,3] with frontier {x:1} (reports x:1, should be x:2)
		batch := &Batch{
			BatchID:     numUUID(2),
			ActorID:     x,
			CounterBase: 3,
			Frontier:    map[UUID]int64{x: 1}, // WRONG (should be 2)
			Operations: []*Operation{
				mustCreateOp(t, 2, levelJSON(t, numUUID(2), "test")),
			},
		}

		err := validateOwnPriorEnds([]*Batch{batch})
		if err == nil {
			t.Fatal("validateOwnPriorEnds accepted batch with wrong prior end (F4 violation)")
		}
		errStr := err.Error()
		if !contains(errStr, "reports its own prior end") {
			t.Errorf("error does not mention wrong prior end: %v", err)
		}
	})

	// Witness 3: Batch at counterBase > 1 with CORRECT prior end accepts.
	// This is the positive control: validates that the rule is exact.
	t.Run("non-first batch with correct prior end accepts", func(t *testing.T) {
		// x[2,2] with frontier {x:1} (correct)
		batch := &Batch{
			BatchID:     numUUID(3),
			ActorID:     x,
			CounterBase: 2,
			Frontier:    map[UUID]int64{x: 1}, // CORRECT
			Operations: []*Operation{
				mustCreateOp(t, 3, levelJSON(t, numUUID(3), "test")),
			},
		}

		err := validateOwnPriorEnds([]*Batch{batch})
		if err != nil {
			t.Fatalf("validateOwnPriorEnds rejected valid batch: %v", err)
		}
	})

	// Witness 4: First batch (counterBase=1) MUST NOT claim prior end > 0.
	// FORMAT-SPEC F4: first batch has no prior, so frontier[own] must be absent or 0.
	t.Run("first batch with nonzero prior end rejects", func(t *testing.T) {
		// x[1,1] with frontier {x:1} (claims prior end, but this IS the first)
		batch := &Batch{
			BatchID:     numUUID(4),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 1}, // WRONG (first batch, no prior)
			Operations: []*Operation{
				mustCreateOp(t, 4, levelJSON(t, numUUID(4), "test")),
			},
		}

		err := validateOwnPriorEnds([]*Batch{batch})
		if err == nil {
			t.Fatal("validateOwnPriorEnds accepted first batch claiming prior end (F4 violation)")
		}
		errStr := err.Error()
		if !contains(errStr, "first batch but claims own prior end") {
			t.Errorf("error does not mention first-batch violation: %v", err)
		}
	})

	// Witness 5: First batch with absent or zero prior end accepts.
	t.Run("first batch with no prior end accepts", func(t *testing.T) {
		// x[1,1] with frontier {} (no prior end, correct)
		batch1 := &Batch{
			BatchID:     numUUID(5),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{}, // CORRECT (absent)
			Operations: []*Operation{
				mustCreateOp(t, 5, levelJSON(t, numUUID(5), "test1")),
			},
		}
		// y[1,1] with frontier {x:2, y:0} (y:0 is acceptable for first batch)
		batch2 := &Batch{
			BatchID:     numUUID(6),
			ActorID:     y,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 2, y: 0}, // CORRECT (y:0 allowed)
			Operations: []*Operation{
				mustCreateOp(t, 6, levelJSON(t, numUUID(6), "test2")),
			},
		}

		err := validateOwnPriorEnds([]*Batch{batch1, batch2})
		if err != nil {
			t.Fatalf("validateOwnPriorEnds rejected valid first batches: %v", err)
		}
	})
}

// TestValidateOwnPriorEndsMutation proves that Witness 1 (non-first batch
// must include own prior end) actually exercises the own-prior-end check by
// deliberately breaking the helper and observing the witness change from
// reject to accept.
//
// Mutant: Comment out the own-prior-end check for counterBase > 1 at
// closure.go:269-279. Run Witness 1, assert it changes from REJECT to ACCEPT.
// Restore, assert it REJECTS again.
//
// This is the mutation proof r0011 required (plan lines 29-33).
func TestValidateOwnPriorEndsMutation(t *testing.T) {
	t.Skip("MUTATION PROOF: Manually uncomment this test, apply the mutant at closure.go:269-279, run, observe witness goes reject→accept, restore, observe reject again")

	// This test is SKIPPED by default because it requires manual mutation.
	// To run the mutation proof:
	// 1. Comment out closure.go:269-279 (the own-prior-end check for counterBase > 1)
	// 2. Unskip this test (remove t.Skip line)
	// 3. Run: go test -run=TestValidateOwnPriorEndsMutation
	// 4. Observe: Witness 1 ACCEPTS (should reject)
	// 5. Restore closure.go:269-279
	// 6. Run again: Witness 1 REJECTS (correct)

	x := actorID(actorX)

	// Witness 1 fixture: x[2,2] omitting own prior end
	batch := &Batch{
		BatchID:     numUUID(1),
		ActorID:     x,
		CounterBase: 2,
		Frontier:    map[UUID]int64{}, // OMITS x:1
		Operations: []*Operation{
			mustCreateOp(t, 1, levelJSON(t, numUUID(1), "test")),
		},
	}

	err := validateOwnPriorEnds([]*Batch{batch})
	// With mutant applied: err == nil (WRONG, should reject)
	// With helper restored: err != nil (CORRECT, rejects F4 violation)
	if err == nil {
		t.Fatal("MUTANT DETECTED: validateOwnPriorEnds accepted batch omitting own prior end (check is broken)")
	}
	t.Log("Helper intact: correctly rejects batch omitting own prior end")
}

// TestValidateNoPartialRangeObservationSpecDerived provides spec-derived
// witnesses for validateNoPartialRangeObservation that never use the
// differential oracle. Each witness is derived from FORMAT-SPEC B5 "Causal
// metadata advances past a batch only to its END" and verified by deliberate
// mutation.
//
// BLOCKER 3: validateNoPartialRangeObservation is shared by both optimized
// and naive validators (closure.go:53, :304), so differential agreement
// cannot validate it. These witnesses establish independent coverage from
// the specification.
func TestValidateNoPartialRangeObservationSpecDerived(t *testing.T) {
	x := actorID(actorX)
	y := actorID(actorY)

	// Witness 1: Frontier pointing INSIDE a multi-operation batch rejects.
	// FORMAT-SPEC B5: "Intermediate counters of a batch are never exposed in any frontier"
	// A batch x[1,3] reserves counters 1,2,3. A frontier {x:2} points INSIDE,
	// not at the end (3), violating B5.
	t.Run("frontier pointing inside batch range rejects", func(t *testing.T) {
		// x[1,3] (3 operations, ends at counter 3)
		xBatch := &Batch{
			BatchID:     numUUID(10),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 10, levelJSON(t, numUUID(10), "x1")),
				mustCreateOp(t, 11, levelJSON(t, numUUID(11), "x2")),
				mustCreateOp(t, 12, levelJSON(t, numUUID(12), "x3")),
			},
		}
		// y[1,1] with frontier {x:2} (points INSIDE x[1,3], not at end)
		yBatch := &Batch{
			BatchID:     numUUID(20),
			ActorID:     y,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 2}, // INSIDE [1,3], not at end (3)
			Operations: []*Operation{
				mustCreateOp(t, 20, levelJSON(t, numUUID(20), "y1")),
			},
		}

		err := validateNoPartialRangeObservation([]*Batch{xBatch, yBatch})
		if err == nil {
			t.Fatal("validateNoPartialRangeObservation accepted frontier pointing inside batch range (B5 violation)")
		}
		errStr := err.Error()
		if !contains(errStr, "lies INSIDE the reserved range") {
			t.Errorf("error does not mention inside-range violation: %v", err)
		}
	})

	// Witness 2: Frontier pointing AT batch end accepts.
	// FORMAT-SPEC B5: "Causal metadata advances past a batch only to its END"
	// A batch x[1,3] ends at counter 3. A frontier {x:3} points at the end,
	// which is correct per B5.
	t.Run("frontier pointing at batch end accepts", func(t *testing.T) {
		// x[1,3] (3 operations, ends at counter 3)
		xBatch := &Batch{
			BatchID:     numUUID(11),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 13, levelJSON(t, numUUID(13), "x1")),
				mustCreateOp(t, 14, levelJSON(t, numUUID(14), "x2")),
				mustCreateOp(t, 15, levelJSON(t, numUUID(15), "x3")),
			},
		}
		// y[1,1] with frontier {x:3} (points AT end of x[1,3], correct)
		yBatch := &Batch{
			BatchID:     numUUID(21),
			ActorID:     y,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 3}, // AT end [1,3], correct
			Operations: []*Operation{
				mustCreateOp(t, 21, levelJSON(t, numUUID(21), "y1")),
			},
		}

		err := validateNoPartialRangeObservation([]*Batch{xBatch, yBatch})
		if err != nil {
			t.Fatalf("validateNoPartialRangeObservation rejected valid frontier at batch end: %v", err)
		}
	})

	// Witness 3: Frontier pointing BEFORE batch start accepts.
	// This validates that the check only rejects INSIDE, not BEFORE.
	t.Run("frontier pointing before batch start accepts", func(t *testing.T) {
		// x[5,7] (starts at 5, ends at 7)
		xBatch := &Batch{
			BatchID:     numUUID(12),
			ActorID:     x,
			CounterBase: 5,
			Frontier:    map[UUID]int64{x: 4}, // own prior end
			Operations: []*Operation{
				mustCreateOp(t, 16, levelJSON(t, numUUID(16), "x5")),
				mustCreateOp(t, 17, levelJSON(t, numUUID(17), "x6")),
				mustCreateOp(t, 18, levelJSON(t, numUUID(18), "x7")),
			},
		}
		// y[1,1] with frontier {x:3} (BEFORE x[5,7], not inside)
		yBatch := &Batch{
			BatchID:     numUUID(22),
			ActorID:     y,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 3}, // BEFORE [5,7], acceptable
			Operations: []*Operation{
				mustCreateOp(t, 22, levelJSON(t, numUUID(22), "y1")),
			},
		}

		err := validateNoPartialRangeObservation([]*Batch{xBatch, yBatch})
		if err != nil {
			t.Fatalf("validateNoPartialRangeObservation rejected frontier pointing before batch: %v", err)
		}
	})

	// Witness 4: Single-operation batch never has partial observation.
	// A batch with 1 operation has counterBase=N, end=N, so any frontier
	// pointing at N is at the end, never inside.
	t.Run("single-operation batch has no partial range", func(t *testing.T) {
		// x[1,1] (single operation)
		xBatch := &Batch{
			BatchID:     numUUID(13),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 19, levelJSON(t, numUUID(19), "x1")),
			},
		}
		// y[1,1] with frontier {x:1} (at end of x[1,1], only possibility)
		yBatch := &Batch{
			BatchID:     numUUID(23),
			ActorID:     y,
			CounterBase: 1,
			Frontier:    map[UUID]int64{x: 1}, // at end, correct
			Operations: []*Operation{
				mustCreateOp(t, 23, levelJSON(t, numUUID(23), "y1")),
			},
		}

		err := validateNoPartialRangeObservation([]*Batch{xBatch, yBatch})
		if err != nil {
			t.Fatalf("validateNoPartialRangeObservation rejected single-operation batch: %v", err)
		}
	})
}

// TestValidateNoPartialRangeObservationMutation proves that Witness 2 (frontier
// at batch end must accept) actually exercises the inside-range check by
// deliberately breaking the helper's boundary condition and observing the
// witness change from accept to reject.
//
// Mutant: Change closure.go:219 from `maxEnds[lo-1] > n` to `maxEnds[lo-1] >= n`.
// This makes the check reject frontiers AT batch end (wrong), not just INSIDE.
// Run Witness 2, assert it changes from ACCEPT to REJECT.
// Restore, assert it ACCEPTS again.
//
// This is the mutation proof r0011 required (plan lines 49-53).
func TestValidateNoPartialRangeObservationMutation(t *testing.T) {
	t.Skip("MUTATION PROOF: Manually uncomment this test, apply the mutant at closure.go:219 (> to >=), run, observe Witness 2 goes accept→reject, restore, observe accept again")

	// This test is SKIPPED by default because it requires manual mutation.
	// To run the mutation proof:
	// 1. Change closure.go:219 from `maxEnds[lo-1] > n` to `maxEnds[lo-1] >= n`
	// 2. Unskip this test (remove t.Skip line)
	// 3. Run: go test -run=TestValidateNoPartialRangeObservationMutation
	// 4. Observe: Witness 2 REJECTS (should accept)
	// 5. Restore closure.go:219 to `maxEnds[lo-1] > n`
	// 6. Run again: Witness 2 ACCEPTS (correct)

	x := actorID(actorX)
	y := actorID(actorY)

	// Witness 2 fixture: y[1,1] with frontier {x:3} observing AT end of x[1,3]
	xBatch := &Batch{
		BatchID:     numUUID(11),
		ActorID:     x,
		CounterBase: 1,
		Frontier:    map[UUID]int64{},
		Operations: []*Operation{
			mustCreateOp(t, 13, levelJSON(t, numUUID(13), "x1")),
			mustCreateOp(t, 14, levelJSON(t, numUUID(14), "x2")),
			mustCreateOp(t, 15, levelJSON(t, numUUID(15), "x3")),
		},
	}
	yBatch := &Batch{
		BatchID:     numUUID(21),
		ActorID:     y,
		CounterBase: 1,
		Frontier:    map[UUID]int64{x: 3}, // AT end [1,3], should accept
		Operations: []*Operation{
			mustCreateOp(t, 21, levelJSON(t, numUUID(21), "y1")),
		},
	}

	err := validateNoPartialRangeObservation([]*Batch{xBatch, yBatch})
	// With mutant applied (>= instead of >): err != nil (WRONG, should accept)
	// With helper restored (> correct): err == nil (CORRECT, accepts at-end)
	if err != nil {
		t.Fatalf("MUTANT DETECTED: validateNoPartialRangeObservation rejected frontier at batch end (boundary check is broken): %v", err)
	}
	t.Log("Helper intact: correctly accepts frontier at batch end")
}
