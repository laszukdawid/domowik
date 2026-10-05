# Issue 0011 audit — Areas 1 & 2 verification

**Status:** Evidence record for the four-audit-area closure (issue 0011 continuation, 2026-08-31).

## Area 1: ED-0043 re-derived expectations

### Verified re-derivations

Commit `84f18b6` re-derived five EXPECTATIONS.md entries and their fixtures under the ED-0037 write position.

#### Line 448: golden/snapshots/full-s1-s5

**Note claim:** "winnerPositions and createdPositions reference the shared batchPositions table, which carries each referenced batch's own frontier; causalPastSize is the sum of that frontier and is never stored (FS 5.1 S3). createdPositions covers the tombstoned id as well as the live entities, so the live interval [creation, deleting) survives deletion (S1/S4)."

**Verification:**
- §5.1 S3 confirms positions reference `batchPositions` via `{"batch": "<stem>", "operationIndex": n}` and `causalPastSize` is the sum of the frontier
- §5.1 S1 explicitly states `createdPositions` is "retained for TOMBSTONED entities as well as live ones"
- §5.1 S4 confirms tombstones carry both creation and deleting positions for the live interval
- Fixture `snapshot-full-s1-s5.json` shows exact structure: `batchPositions` table, `winnerPositions`/`createdPositions`/`tombstones` referencing it
- Multi-actor frontier {X:45, Y:7} present

✓ CORRECT

#### Line 686: adversarial/vectors/v4-delete-vs-set

**Note claim:** "Y's frontier {Y:29} gives causalPastSize 29 against the delete's 79, so the set is placed before the delete and the delete then dominates (LWW3) - the same placement the pre-ED-0037 counters 30 < 80 gave."

**Verification:**
- X delete: counterBase 80, frontier {X:79} → causalPastSize = 79
- Y set: counterBase 30, frontier {Y:29} → causalPastSize = 29
- 29 < 79 → Y's set placed before X's delete → delete dominates per LWW3
- Pre-ED-0037: counters 30 < 80 gave same placement
- **Outcome unchanged per ED-0043's constraint**

✓ CORRECT

#### V3 (adversarial/vectors/v3-concurrent-set-set)

**Note claim:** "X's frontier {X:69} gives causalPastSize 69 against Y's 19, so the leading component already decides: 69 > 19 -> X wins. The pre-ED-0037 counter comparison 70 > 20 picked the same writer"

**Verification:**
- X: counterBase 70, frontier {X:69} → causalPastSize = 69
- Y: counterBase 20, frontier {Y:19} → causalPastSize = 19
- 69 > 19 → X wins (concurrent writes, past size decides first per ED-0037)
- Pre-ED-0037: counters 70 > 20 gave same winner
- **Outcome unchanged**

✓ CORRECT

#### V5 (adversarial/vectors/v5-late-concurrent-after-compaction)

**Note claim:** "the retained winner references batch 0000000000000000100-..., whose frontier {X:99} gives causalPastSize 99, against the late batch's 49, so the late write still loses"

**Verification:** Fixture structure updated to `batchPositions` with winner position. Retained winner's causalPastSize 99 > late batch's 49 → late write loses. Outcome unchanged.

✓ CORRECT

#### V6 (adversarial/vectors/v6-snapshot-replay)

**Note claim:** "Re-derived for FRAMING only: the fullHistory batches gained the writing actor's own prior end counter, which §4.4 F4 requires"

**Verification:** §4.4 F4 requires frontier to include own actor's prior end. Batches at counterBase > 1 now carry this. Framing fix, not semantic change.

✓ CORRECT

#### Multi-actor fixtures

**batch-frontier-two-actors.json:** Frontier {X:45, Y:6} with Y as writing actor. Matches §4.4 F1 multi-actor shape.

**snapshot-full-s1-s5.json:** Frontier {X:45, Y:7}, multi-actor batchPositions table, positions referencing it.

✓ CORRECT

### Constraint compliance

ED-0043: "A re-derivation is not a licence to move an outcome."

**All five vectors maintain their outcomes exactly:**
- V3: X wins (69 > 19 vs 70 > 20)
- V4: delete dominates (29 < 79 vs 30 < 80)
- V5: late write loses (99 > 49)
- V6: equality holds (framing-only change)
- Golden snapshot: structure widened, no outcome

✓ NO OUTCOMES MOVED

**Result:** All ED-0043 re-derivations verified correct against FORMAT-SPEC §5.1, §4.4, §4.7. Outcomes unchanged. Reasoning matches normative semantics.

---

## Area 2: ED-0045 V2 coverage discharge

### V2 claim verification

**Claim (EXPECTATIONS.md:640):** "the re-enabled check is mutation-proven per ED-0045"

**Evidence location:** `docs/evidence/0011-position-implementation.md` lines 130-165

### Two-part discharge per ED-0045

ED-0045 requires V2 to be "demonstrably executed and demonstrably able to fail."

#### Part 1: Marker removal was forced by green behavior

The `executionBlocked` marker was removed ONLY after the harness's own guard failed:

```
corpus_semantic_test.go:435: finalThicknessMm now MATCHES its oracle (150):
the issue-0011 conformance defect is resolved, so remove the executionBlocked
marker from EXPECTATIONS.md consciously
```

The marker was removed when it became UNTRUE, not before.

#### Part 2: Re-enabled check can fail (M10 mutation)

With the marker gone, the causalPastSize comparison in `position.go` `compare()` function was broken:

```
=== M10 causal-position winner comparison broken AFTER the V2 marker was removed:
    check went RED as required
--- FAIL: TestProjectionVectors
    --- FAIL: TestProjectionVectors/adversarial/vectors/v2-reordered-delivery
        --- FAIL: .../final-projection-equals-in-order-delivery
    (restored; checks green again)
```

### Current check structure (corpus_semantic_test.go:353-473)

**checkV2** function verifies three properties:

1. **Line 380:** `requireCausalEdge` asserts B-later observes A-earlier (the causal precondition)
2. **Lines 393-411:** Causal holding - B-later alone must not apply (F3)
3. **Lines 414-443:** Order independence - sequential vs reordered delivery must produce identical projections (F3)
4. **Lines 445-472:** Final projection equals in-order delivery with:
   - **Value assertion:** `finalThicknessMm == 150` (line 459-460)
   - **Position assertion:** Winner position must be owned by the causally-later writer (lines 465-471)

The position assertion prevents "coincidentally-correct value" false positives.

### Mutation target verified

The mutation broke `position.go:228-230`:
```go
if c := pa.compare(pb); c != 0 {
    return c
}
```

This is the causalPastSize comparison in the canonical ordering. With it disabled, the ED-0032 defect returns: a causally-later write with smaller counter loses.

### Efficacy confirmed

The check:
1. **EXISTS** in current code at the stated locations
2. **WAS MUTATED** as documented
3. **FAILED** when broken (transcript in evidence)
4. **PASSED** when restored

Per ED-0072: "Presence is not efficacy." The mutation transcript is efficacy evidence, not the check's presence.

**Result:** V2's 150-oracle check is executed, discriminates under mutation, and its efficacy is documented per ED-0045's observable standard.
