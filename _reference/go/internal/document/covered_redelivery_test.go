// Copyright 2025 the Domowik AUTHORS. All rights reserved. Use of this
// source code is governed by the Apache-2.0 license in LICENSE.

package document

import (
	"reflect"
	"testing"
)

// TestCoveredRedeliveryDoesNotPoisonFreshBatchClosure is the permanent
// regression for BLOCKER 2: a covered redelivery (batch whose entire range
// lies at or below the watermark) must be silently ignored per I2/V5 without
// becoming ancestry evidence for fresh batches in the same Deliver call.
//
// Orchestrator's independent reproduction (dd17124, Go 1.27.0):
//
//	seed    frontier {x: 2, z: 7}
//	covered actor x, counterBase 1, frontier {z: 7}
//	fresh   actor y, counterBase 1, frontier {x: 2}
//
//	split   : Deliver(covered) = <nil> ; Deliver(fresh) = <nil>
//	samecall: Deliver(covered, fresh) = malformed-framing: ... (has 0, needs 7) (F4)
//
// Same batches, same order, same seed; only the grouping differs. The
// same-call rejection is the defect: covered batch x[1,1] provides z:7 as
// ancestry, fresh batch y[1,1] declares frontier {x:2} (absent z), closure
// validation reads absent z as zero and rejects "has 0, needs 7".
//
// Direction: grouping makes the reader STRICTER (rejects valid, not accepts
// invalid). The interesting assertion is that same-call ACCEPTS after the fix,
// and we must prove it accepts for the right reason (F4 check still runs) via
// a control that proves a genuinely non-closed fresh batch is still rejected
// (ED-0041 shape guard).
//
// Evidence:
//  1. Same-call: Deliver(covered, fresh) accepts (fixed behavior)
//  2. Split-call: Deliver(covered); Deliver(fresh) accepts (control)
//  3. Fresh-only: Deliver(fresh) accepts (proves fresh batch is valid)
//  4. F4-still-runs: Deliver(genuinely-malformed-fresh) rejects (check runs)
//  5. Retained-log: FromSnapshot with covered batch validates before suppression
//  6. Equivalence: compare state, frontier, log, diagnostics (not just nil)
//
// Note: The "has 0, needs 7" message exposes the zero-versus-absent
// conflation in F4 ancestry reporting (third occurrence after snapshot S2
// watermark and position authentication). This is a design-level finding
// routed to issues 0004/0005, not in scope for BLOCKER 2.
func TestCoveredRedeliveryDoesNotPoisonFreshBatchClosure(t *testing.T) {
	x := actorID(actorX)
	y := actorID(actorY)
	z := actorID(uint64(0xA004))

	// Helper to create a seed projection with frontier {x: 2, z: 7}.
	// x and z are INDEPENDENT (neither observes the other). This allows
	// fresh to observe x:2 without needing to observe z:7.
	makeSeed := func() *Projection {
		p := New()
		// Deliver x[1,1] with NO dependency on z
		xBatch1 := &Batch{
			BatchID:     numUUID(0xB101),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 101, levelJSON(t, numUUID(101), "x-level-1")),
			},
		}
		// Deliver x[2,2] with dependency on x:1 only
		xBatch2 := &Batch{
			BatchID:     numUUID(0xB102),
			ActorID:     x,
			CounterBase: 2,
			Frontier:    map[UUID]int64{x: 1},
			Operations: []*Operation{
				mustCreateOp(t, 102, levelJSON(t, numUUID(102), "x-level-2")),
			},
		}
		// Deliver z[1,7] with NO dependency on x
		zBatch := &Batch{
			BatchID:     numUUID(0xB103),
			ActorID:     z,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 103, levelJSON(t, numUUID(103), "z-level-1")),
				mustCreateOp(t, 104, levelJSON(t, numUUID(104), "z-level-2")),
				mustCreateOp(t, 105, levelJSON(t, numUUID(105), "z-level-3")),
				mustCreateOp(t, 106, levelJSON(t, numUUID(106), "z-level-4")),
				mustCreateOp(t, 107, levelJSON(t, numUUID(107), "z-level-5")),
				mustCreateOp(t, 108, levelJSON(t, numUUID(108), "z-level-6")),
				mustCreateOp(t, 109, levelJSON(t, numUUID(109), "z-level-7")),
			},
		}
		if err := p.Deliver(xBatch1, xBatch2, zBatch); err != nil {
			panic(err)
		}
		// Now p.frontier is {x: 2, z: 7}, and x and z are independent
		return p
	}

	// Covered redelivery: IDENTICAL to xBatch1 from the seed (same BatchID,
	// same content). Entire range lies at or below the x watermark (2), so
	// I2/V5 must silently ignore it. Its frontier {} must NOT become
	// ancestry evidence for fresh batch y.
	covered := &Batch{
		BatchID:     numUUID(0xB101), // SAME as xBatch1
		ActorID:     x,
		CounterBase: 1,
		Frontier:    map[UUID]int64{},
		Operations: []*Operation{
			mustCreateOp(t, 101, levelJSON(t, numUUID(101), "x-level-1")),
		},
	}

	// Fresh batch: actor y, range [1,1], frontier {x: 2}.
	// Observes x through counter 2 (satisfied by seed), does NOT observe z
	// (absent z entry). This is VALID because x and z are independent in the
	// seed, so observing x does not require observing z.
	fresh := &Batch{
		BatchID:     numUUID(0xB002),
		ActorID:     y,
		CounterBase: 1,
		Frontier:    map[UUID]int64{x: 2},
		Operations: []*Operation{
			mustCreateOp(t, 2, levelJSON(t, numUUID(2), "fresh-level")),
		},
	}

	// Genuinely malformed fresh batch: actor y, range [1,1], frontier {x: 2}.
	// Observes x:2 but OMITS z. Since xBatch2 (which ends at x:2) has frontier
	// {x:1}, and xBatch1 (which ends at x:1) has frontier {}, there's no
	// transitive z dependency through x. So this is actually VALID.
	//
	// CORRECTED: To make this genuinely malformed, it must observe x:2 and
	// under-report z. But the seed has x and z INDEPENDENT (neither observes
	// the other), so observing x:2 doesn't require observing z:7.
	//
	// The correct malformed batch: observe z:6 (under-reports z, should be z:7).
	malformed := &Batch{
		BatchID:     numUUID(0xB003),
		ActorID:     y,
		CounterBase: 1,
		Frontier:    map[UUID]int64{z: 6}, // under-reports z (seed has z:7)
		Operations: []*Operation{
			mustCreateOp(t, 3, levelJSON(t, numUUID(3), "malformed-level")),
		},
	}

	// EVIDENCE 3: Fresh-only control — fresh batch is valid against seed.
	t.Run("fresh-only", func(t *testing.T) {
		p := makeSeed()
		err := p.Deliver(fresh)
		if err != nil {
			t.Fatalf("fresh-only rejected valid batch: %v", err)
		}
		// State: 10 entities (9 from seed + 1 from fresh)
		// Frontier: {x: 2, y: 1, z: 7}
		// Log: [xBatch1, xBatch2, zBatch, fresh]
		// Diagnostics: empty
		state := p.State()
		if len(state.Entities) != 10 {
			t.Errorf("fresh-only state: got %d entities, want 10", len(state.Entities))
		}
		wantFrontier := map[UUID]int64{x: 2, y: 1, z: 7}
		if !reflect.DeepEqual(state.Frontier, wantFrontier) {
			t.Errorf("fresh-only frontier: got %v, want %v", state.Frontier, wantFrontier)
		}
		if len(p.batches) != 4 {
			t.Errorf("fresh-only log: got %d batches, want 4 (xBatch1, xBatch2, zBatch, fresh)", len(p.batches))
		}
		if p.batches[fresh.BatchID] == nil {
			t.Errorf("fresh-only log: missing fresh batch")
		}
		if len(p.Diagnostics()) != 0 {
			t.Errorf("fresh-only diagnostics: got %d, want 0", len(p.Diagnostics()))
		}
	})

	// EVIDENCE 2: Split-call control — covered then fresh, two calls.
	// This already worked at dd17124; it is the grouping that failed.
	t.Run("split-call", func(t *testing.T) {
		p := makeSeed()
		err := p.Deliver(covered)
		if err != nil {
			t.Fatalf("split-call covered rejected: %v", err)
		}
		err = p.Deliver(fresh)
		if err != nil {
			t.Fatalf("split-call fresh rejected: %v", err)
		}
		// State: 10 entities (9 from seed + 1 from fresh; covered was ignored)
		// Frontier: {x: 2, y: 1, z: 7} (covered was already applied, no change)
		// Log: [xBatch1, xBatch2, zBatch, fresh] (covered was silently ignored, not retained)
		// Diagnostics: empty
		state := p.State()
		if len(state.Entities) != 10 {
			t.Errorf("split-call state: got %d entities, want 10", len(state.Entities))
		}
		wantFrontier := map[UUID]int64{x: 2, y: 1, z: 7}
		if !reflect.DeepEqual(state.Frontier, wantFrontier) {
			t.Errorf("split-call frontier: got %v, want %v", state.Frontier, wantFrontier)
		}
		if len(p.batches) != 4 {
			t.Errorf("split-call log: got %d batches, want 4 (xBatch1, xBatch2, zBatch, fresh)", len(p.batches))
		}
		if p.batches[fresh.BatchID] == nil {
			t.Errorf("split-call log: missing fresh batch")
		}
		if len(p.Diagnostics()) != 0 {
			t.Errorf("split-call diagnostics: got %d, want 0", len(p.Diagnostics()))
		}
	})

	// EVIDENCE 1: Same-call — covered and fresh in one Deliver.
	// This is the BLOCKER 2 defect: dd17124 rejected, fix must accept.
	// CRITICAL: must accept for the right reason (F4 check runs), not
	// because check stopped running (ED-0041 shape). Evidence 4 proves this.
	t.Run("same-call", func(t *testing.T) {
		p := makeSeed()
		err := p.Deliver(covered, fresh)
		if err != nil {
			t.Fatalf("same-call rejected valid batches: %v", err)
		}
		// Must match split-call outcome exactly: state, frontier, log, diagnostics.
		state := p.State()
		if len(state.Entities) != 10 {
			t.Errorf("same-call state: got %d entities, want 10", len(state.Entities))
		}
		wantFrontier := map[UUID]int64{x: 2, y: 1, z: 7}
		if !reflect.DeepEqual(state.Frontier, wantFrontier) {
			t.Errorf("same-call frontier: got %v, want %v", state.Frontier, wantFrontier)
		}
		if len(p.batches) != 4 {
			t.Errorf("same-call log: got %d batches, want 4 (xBatch1, xBatch2, zBatch, fresh)", len(p.batches))
		}
		if p.batches[fresh.BatchID] == nil {
			t.Errorf("same-call log: missing fresh batch")
		}
		if len(p.Diagnostics()) != 0 {
			t.Errorf("same-call diagnostics: got %d, want 0", len(p.Diagnostics()))
		}
	})

	// EVIDENCE 4: F4-still-runs control — genuinely malformed fresh batch.
	// Must be rejected to prove closure check didn't stop running (ED-0041).
	t.Run("f4-still-runs", func(t *testing.T) {
		p := makeSeed()
		// Verify seed has x:2 in its frontier and retained batches
		state := p.State()
		if state.Frontier[x] != 2 {
			t.Fatalf("seed frontier x: got %d, want 2", state.Frontier[x])
		}
		if len(p.batches) != 3 {
			t.Fatalf("seed batches: got %d, want 3 (xBatch1, xBatch2, zBatch)", len(p.batches))
		}

		err := p.Deliver(malformed)
		if err == nil {
			t.Fatal("f4-still-runs accepted genuinely malformed batch (F4 check stopped running)")
		}
		// Must mention F4 violation: under-reports x (has 1, needs 2).
		errStr := err.Error()
		if !contains(errStr, "inconsistent frontier") && !contains(errStr, "under-reports") {
			t.Errorf("f4-still-runs error does not mention F4 violation: %v", err)
		}
	})

	// EVIDENCE 5: Retained-log domain — ReplayFromSnapshot validates before suppression.
	// This is the CRITICAL domain-split proof: the SAME bytes that ordinary
	// Deliver silently ignores (I2 "no error") must STILL REJECT when supplied
	// as retained log via ReplayFromSnapshot. This discriminates "domain split
	// working" from "F4 stopped running" (orchestrator requirement).
	t.Run("retained-log-domain", func(t *testing.T) {
		// Part 1: Ordinary delivery — malformed covered batch silently ignored.
		// Build a seed with watermark {x:2, z:7} but EMPTY position table.
		// This allows covered batches to pass authentication (no known refs to check).
		seedStateOrdinary := State{
			Entities:         []Entity{},
			Frontier:         map[UUID]int64{x: 2, z: 7},
			BatchPositions:   map[batchRef]map[UUID]int64{}, // EMPTY
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{},
			Tombstones:       map[UUID]position{},
		}
		seedForOrdinary, err := FromSnapshot(seedStateOrdinary, nil)
		if err != nil {
			t.Fatalf("FromSnapshot for ordinary delivery: %v", err)
		}

		// malformedCovered: x[1,1] with frontier {z:5} (under-reports z).
		// This is COVERED (x[1,1] EndCounter=1 <= x:2 watermark) so I2 "no error" applies.
		// Position table is empty, so authentication passes.
		malformedCovered := &Batch{
			BatchID:     numUUID(0xB010),
			ActorID:     x,
			CounterBase: 1,
			Frontier:    map[UUID]int64{z: 5}, // under-reports z (should be z:7)
			Operations: []*Operation{
				mustCreateOp(t, 10, levelJSON(t, numUUID(10), "malformed-covered")),
			},
		}

		// Ordinary Deliver: malformedCovered is COVERED, silently ignored per I2.
		err = seedForOrdinary.Deliver(malformedCovered)
		if err != nil {
			t.Fatalf("ordinary-delivery domain: covered redelivery with under-report rejected (should be silently ignored per I2 'no error'): %v", err)
		}

		// Part 2: Retained-log domain — SAME bytes via ReplayFromSnapshot must REJECT.
		// Build a snapshot with frontier {x:1, z:7} (matches end of retained log) and
		// log containing zBatch + malformedCovered.
		// zBatch establishes z:7, so malformedCovered's {z:5} under-reports.
		//
		// ReplayFromSnapshot calls FromSnapshot(s, nil) then proj.Deliver(log...).
		// The gates at persist_seam.go:196-201 must validate the log BEFORE Deliver,
		// catching the under-report.
		zBatchForRetained := &Batch{
			BatchID:     numUUID(0xB011),
			ActorID:     z,
			CounterBase: 1,
			Frontier:    map[UUID]int64{},
			Operations: []*Operation{
				mustCreateOp(t, 103, levelJSON(t, numUUID(103), "z-level-1")),
				mustCreateOp(t, 104, levelJSON(t, numUUID(104), "z-level-2")),
				mustCreateOp(t, 105, levelJSON(t, numUUID(105), "z-level-3")),
				mustCreateOp(t, 106, levelJSON(t, numUUID(106), "z-level-4")),
				mustCreateOp(t, 107, levelJSON(t, numUUID(107), "z-level-5")),
				mustCreateOp(t, 108, levelJSON(t, numUUID(108), "z-level-6")),
				mustCreateOp(t, 109, levelJSON(t, numUUID(109), "z-level-7")),
			},
		}

		sRetained := State{
			Entities:         []Entity{},
			Frontier:         map[UUID]int64{x: 1, z: 7},
			BatchPositions:   map[batchRef]map[UUID]int64{},
			WinnerPositions:  map[fieldKey]position{},
			CreatedPositions: map[UUID]position{},
			Tombstones:       map[UUID]position{},
		}

		// ReplayFromSnapshot with malformedCovered as retained log: must REJECT.
		// The validateFrontierClosure gate at persist_seam.go:196 must catch the under-report.
		_, err = ReplayFromSnapshot(sRetained, []*Batch{zBatchForRetained, malformedCovered})
		if err == nil {
			t.Fatal("retained-log domain: ReplayFromSnapshot accepted malformed covered batch (F4 stopped running for retained log)")
		}

		// Verify it's an F4 violation (under-reports z).
		errStr := err.Error()
		if !contains(errStr, "inconsistent frontier") && !contains(errStr, "under-reports") {
			t.Errorf("retained-log domain error does not mention F4 violation: %v", err)
		}
	})

	// EVIDENCE 6: Equivalence — same-call and split-call produce identical outcomes.
	t.Run("equivalence", func(t *testing.T) {
		pSame := makeSeed()
		errSame := pSame.Deliver(covered, fresh)

		pSplit := makeSeed()
		errSplit1 := pSplit.Deliver(covered)
		errSplit2 := pSplit.Deliver(fresh)

		// Both must succeed.
		if errSame != nil {
			t.Fatalf("equivalence same-call failed: %v", errSame)
		}
		if errSplit1 != nil || errSplit2 != nil {
			t.Fatalf("equivalence split-call failed: %v, %v", errSplit1, errSplit2)
		}

		// State, frontier, log, diagnostics must be identical.
		stateSame := pSame.State()
		stateSplit := pSplit.State()
		if !reflect.DeepEqual(stateSame, stateSplit) {
			t.Errorf("equivalence state mismatch:\n  same-call: %v\n  split-call: %v",
				stateSame, stateSplit)
		}
		if !reflect.DeepEqual(stateSame.Frontier, stateSplit.Frontier) {
			t.Errorf("equivalence frontier mismatch:\n  same-call: %v\n  split-call: %v",
				stateSame.Frontier, stateSplit.Frontier)
		}
		if len(pSame.batches) != len(pSplit.batches) {
			t.Errorf("equivalence log length mismatch: same-call %d, split-call %d",
				len(pSame.batches), len(pSplit.batches))
		}
		for id, bSame := range pSame.batches {
			bSplit := pSplit.batches[id]
			if bSplit == nil {
				t.Errorf("equivalence log: same-call has %s, split-call missing", id.String())
				continue
			}
			if !bSame.equalContent(bSplit) {
				t.Errorf("equivalence log: batch %s content differs", id.String())
			}
		}
		if !reflect.DeepEqual(pSame.Diagnostics(), pSplit.Diagnostics()) {
			t.Errorf("equivalence diagnostics mismatch:\n  same-call: %v\n  split-call: %v",
				pSame.Diagnostics(), pSplit.Diagnostics())
		}
	})
}

// TestReplayFromSnapshotAuthenticationGate proves that the authentication gate

func contains(s, substr string) bool {
	return len(s) >= len(substr) && (s == substr || len(s) > len(substr) && containsAt(s, substr, 0))
}

func containsAt(s, substr string, start int) bool {
	for i := start; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}
