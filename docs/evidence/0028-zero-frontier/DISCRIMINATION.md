# F1 Zero-Member Frontier Fixture — Discrimination Evidence

## Issue Context

- **Issue 0004** ruled zero-valued frontier member `{A:0}` is FORBIDDEN (FORMAT-SPEC §4.4 F1)
- **Issue 0005** ruled the diagnostic: `invalid-field-syntax` at `StageSchema`

## Fixture

**Location:** `testdata/corpus/hostile/framing/zero-frontier-member.json`

**Content:** Minimal snapshot with `frontier: {"01948f5a-ca77-7a09-9e76-5ace11b12fd2": 0}`

**Expected:** Reject with `invalid-field-syntax` per issue 0005

## Gap Identified

**Location:** `internal/persist/wire.go:523-526` in `parseSnapshotFrontier()`

**Current check (commit b4dc85d, before fix):**
```go
if !okn || n < 0 {
    return nil, fail(CatInvalidFieldSyntax, StageSchema,
        "frontier counter for %s must be a non-negative integer", actorText)
}
```

**Gap:** Check is `n < 0`, not `n <= 0`. Zero passes validation and reaches `internal/document/snapshot.go:106-113` where it is NORMALIZED to absent rather than rejected.

## Discrimination Proof (Executed Against Committed State)

### Instrument 1: Fixture Demonstrates Gap

**Execution:** Checked out commit `3e0cd6c^` (b4dc85d, before fix), applied fixture and test only.

**Command:**
```bash
git checkout 3e0cd6c^
git show 3e0cd6c:testdata/corpus/hostile/framing/zero-frontier-member.json > testdata/corpus/hostile/framing/zero-frontier-member.json
git show 3e0cd6c:internal/persist/zero_frontier_test.go > internal/persist/zero_frontier_test.go
go test ./internal/persist -v -run TestZeroFrontierMemberGap
```

**Output:**
```
=== RUN   TestZeroFrontierMemberGap
    zero_frontier_test.go:27: DEMONSTRATES GAP: zero-valued frontier member was ACCEPTED but F1 forbids it
    zero_frontier_test.go:28: Expected: reject with invalid-field-syntax at StageSchema (per issue 0005)
    zero_frontier_test.go:29: Actual: accepted and normalized at snapshot.go:106
    zero_frontier_test.go:30: Gap location: wire.go:523 checks n<0 but not n==0
--- FAIL: TestZeroFrontierMemberGap (0.00s)
FAIL
FAIL	domowik.dev/domowik/internal/persist	0.328s
FAIL
```

**Conclusion:** Zero is ACCEPTED when F1 forbids it. Gap confirmed on committed code before fix.

### Instrument 2: Adding Zero Check Causes Rejection

**Execution:** Checked out full commit 3e0cd6c with fix applied.

**Fix applied at wire.go:527-532:**
```go
// F1 zero prohibition (issue 0004): zero-valued frontier member is forbidden.
// Absence (actor omitted) is the sole representation of "nothing observed".
if n == 0 {
    return nil, fail(CatInvalidFieldSyntax, StageSchema,
        "frontier counter for %s is zero (forbidden by FS §4.4 F1)", actorText)
}
```

**Command:**
```bash
git checkout 3e0cd6c
go test ./internal/persist -v -run TestZeroFrontierMemberGap
```

**Output:**
```
=== RUN   TestZeroFrontierMemberGap
    zero_frontier_test.go:39: Correctly rejects with invalid-field-syntax
--- PASS: TestZeroFrontierMemberGap (0.00s)
PASS
ok  	domowik.dev/domowik/internal/persist	0.255s
```

**Conclusion:** Adding the zero-specific check makes the fixture reject with the correct category.

### Instrument 3: Removing Check Restores Gap

**Execution:** From commit 3e0cd6c, commented out `n == 0` check at wire.go:529-532 and inspect.go:327-331.

**Command:**
```bash
# Comment out the zero checks
go test ./internal/persist -v -run TestZeroFrontierMemberGap
```

**Output:**
```
=== RUN   TestZeroFrontierMemberGap
    zero_frontier_test.go:27: DEMONSTRATES GAP: zero-valued frontier member was ACCEPTED but F1 forbids it
    zero_frontier_test.go:28: Expected: reject with invalid-field-syntax at StageSchema (per issue 0005)
    zero_frontier_test.go:29: Actual: accepted and normalized at snapshot.go:106
    zero_frontier_test.go:30: Gap location: wire.go:523 checks n<0 but not n==0
--- FAIL: TestZeroFrontierMemberGap (0.00s)
FAIL
FAIL	domowik.dev/domowik/internal/persist	0.263s
FAIL
```

**Conclusion:** Removing the check returns the fixture to FAIL (gap state). The fixture reaches the zero check specifically and discriminates effectively.

## What Would Fire First If It Did Not Reach F1

**Candidate earlier checks:**

1. **Type validation (line 523):** `!okn` checks if the value is an integer
   - Zero IS an integer, so this check does not fire
   - Verified: Fixture reaches line 527

2. **Negative check (line 523):** `n < 0` checks for negative values
   - Zero is not negative, so this check does not fire
   - Verified: Fixture reaches line 527

3. **JSON parsing:** Could reject malformed JSON before reaching parseSnapshotFrontier
   - Fixture is well-formed JSON
   - Verified: JSON parses successfully, reaches frontier validation

**Conclusion:** No earlier check rejects a zero-valued frontier member. The fixture must reach the F1 zero prohibition check specifically, and the discrimination proof confirms it does.

## Scope

**Snapshot frontier only** — per reviewer verdict and issue 0005's ruling that StageSchema precedes StageFraming under §9.5.

Batch frontier validation (`internal/persist/wire.go:838-845` in `parseBatchDoc()`) uses StageFraming and is unreachable when snapshot rejects at StageSchema. Batch frontier coverage requires a separate fixture without a snapshot.

## Verification

All three instruments executed successfully against committed state:
1. ✓ Fixture demonstrates gap at b4dc85d (zero accepted)
2. ✓ Adding guard at 3e0cd6c makes it reject (invalid-field-syntax)
3. ✓ Removing guard restores gap (zero accepted again)

The fixture discriminates.
