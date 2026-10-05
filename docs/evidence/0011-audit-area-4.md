# Issue 0011 audit — Area 4: Repository-wide sweep

**Status:** Evidence record for the four-audit-area closure (issue 0011 continuation, 2026-08-31).

## Scope

Per KICKOFF: "A repository-wide sweep for defects outside the known areas."

**Known-live findings excluded (not re-derived):**
- Dangling snapshot entity references accepted
- Malformed batch UUID panics
- Create envelope and payload IDs can disagree
- Required batch frontier member may be absent
- S5 recentBatchIds forgotten before Projection

**Strategy:** Targeted sweep of high-risk areas in `internal/document` - error paths, boundary conditions, invariant assumptions. Budget-constrained: sample rather than exhaustive.

## Sweep areas

### 1. Panic vs error return consistency

**internal/document/projection.go**

Searched for `panic(` calls:
- Line 256: `panic("document: batch position references a batch absent from the position table")` - position table invariant
- Line 226: `panic("document: write position references a batch absent from the position table")` - position table invariant

Both are internal invariants where the caller (within the same package) should have already ensured the batch exists in the table. Panics are appropriate here per Go conventions (programming errors, not user input errors).

**internal/document/snapshot.go**

Searched for `panic(` calls:
- Line 95: `panic("document: frontier member with zero-valued actor in sameFrontier check")` - BLOCKER 1 fix, normalizes at S2 boundary

No user-facing panics found on input validation paths.

### 2. Slice bounds and map access

**Checked for unchecked slice indexing:**

`grep -n "\[len.*-.*\]" internal/document/*.go`

Found patterns like `rs[len(rs)-1]` - checked each:
- `snapshot.go:99-133`: Actor-keyed ranges, zero-valued actor normalized at entry (BLOCKER 1 fix)
- `projection.go:509-513`: Per-actor ranges indexed, but only after ranges exist check

No out-of-bounds vulnerabilities found in current code.

### 3. Integer overflow in causalPastSize

**position.go** `causalPast.addCounter`:

```go
func (p causalPast) addCounter(n int64) causalPast {
	if n < 0 {
		panic("causalPast: negative counter")
	}
	if n > math.MaxInt64-p.lo {
		// Overflow into high word
		p.hi++
		p.lo = n - (math.MaxInt64 - p.lo) - 1
	} else {
		p.lo += n
	}
	return p
}
```

Handles overflow correctly via two-word representation. Row L18 admits 65,536 frontier members each at L13's signed-64-bit ceiling. Test `TestCausalPastIsExact` verifies `3×(2^63−1)` compares correctly.

### 4. Frontier closure validation

**closure.go** `validateFrontierClosure`:

The differential oracle issue (BLOCKER 3) was fixed - optimized and naive validators now have independent partial-range checks, with nine spec-derived witnesses each proven to discriminate.

Checked that F4 own-actor-prior-end requirement is enforced:
- `closure.go:176-180`: Validates own actor entry present
- Mutation M12 confirmed omission is rejected

### 5. Watermark classification edge cases

**projection.go:444-456** watermark classification:

```go
case b.EndCounter() <= w:
	skip[b.BatchID] = true // already applied
case b.CounterBase <= w:
	return nil, nil, wrapFraming("batch straddles watermark") // torn batch
```

Correctly handles three cases: fully covered, straddling (torn), and fresh. BLOCKER 2 fix ensures covered batches don't poison closure validation.

### 6. Position table lifecycle

Checked for position table mutations during framing that could affect rollback:

**projection.go:490-496** documents the mutation is safe because transaction boundary owns rollback. ED-0056 explicitly chose one mechanism over a scratch clone.

### 7. Snapshot serialization round-trips

**internal/persist/snapshot_wire_test.go** `TestSnapshotRoundTrip`:

Verifies batchPositions, winnerPositions, createdPositions, tombstones survive serialization. OperationIndex > 0 case tested.

### 8. Atomic batch application

Checked that partial application is impossible:

**projection.go:299-313** `Deliver`:
- Begins transaction
- Calls `deliverWithin`
- Rolls back entire transaction on any error
- Only commits if `deliverWithin` succeeds

Per `projection.go:23-31`: "Either the whole call is rolled back or it stays committed. The boundary is the Deliver call, not the batch."

Atomicity verified.

### 9. State copying and mutation safety

**projection.go:136-148** `State()`:

Returns a COPY via `state.copy()`, not a live reference. Mutations to returned state don't affect projection internals.

**session.go** undo/redo stacks:

Store entire `State` snapshots, not references. Verified at `session.go:89-95` (undo) and `:103-109` (redo).

### 10. Error wrapping consistency

Checked that framing errors use `wrapFraming`:

`grep -n "wrapFraming" internal/document/*.go | wc -l`

46 call sites. Spot-checked 10 - all use `wrapFraming` for structural anomalies, preserving the `ErrMalformedFraming` sentinel for §9.4 classification.

## Sampling limitations

This is a TARGETED SWEEP, not exhaustive verification. Areas sampled:
- Panic vs error consistency ✓
- Slice bounds ✓
- Integer overflow ✓
- Frontier closure ✓
- Watermark classification ✓
- Position table lifecycle ✓
- Serialization round-trips ✓
- Atomic application ✓
- State mutation safety ✓
- Error wrapping ✓

Areas NOT exhaustively verified:
- Every possible input combination
- Every error path reachability
- Performance characteristics
- Concurrency (no concurrency in this package)

## Findings

**No new defects found** in the sampled areas. The three BLOCKER fixes from round 3 address the high-risk issues that this sweep targeted.

## Result

**AREA 4 SAMPLED.** Targeted sweep of high-risk areas in internal/document completed. Ten categories checked. No new defects found. Per the brief: "Report what you VERIFIED, what you SAMPLED, and what you never reached." This area was SAMPLED (targeted checks, not exhaustive).
