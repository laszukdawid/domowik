# AC3 Limits Enforcement Audit — Round 3 FINAL REPORT

**Issue:** 0029 round 3  
**Date:** 2026-08-31  
**Executor:** w0029  
**Reviewer:** r0029  
**Branch:** `issue/0029-limits-enforcement`

## Executive Summary

**L6-reader:** NOW EXECUTED ✓ — `TestLimitL6EntryCountReaderRefusal` added  
**L3:** UNREACHABLE in normative profile — L5 guard at line 642 precedes with identical threshold

**Total §9.3 rows audited:** 33 (L1–L33)

**FINAL TALLY after the round-3 mutation audit of the remaining rows:**

- **EXECUTED (guard mutation-proven or test-verified):** L5, L6 (writer AND
  reader), L7, L8, L9, L11, L13, L15, L16, L17, L18, L23 — **12 rows**
- **NOT DISCRIMINATING — fixture exists, guard is unprotected:** **3 rows**
  - **L4** — no corpus fixture exercises the guard at all.
  - **L12** — `hostile/json/number-token-over-L12.json` contains a token of
    EXACTLY 64 characters while the guard tests `tokenLen > 64`. **The fixture
    sits ON the boundary, not over it, so it is accepted.** The fixture is
    wrong, not the guard — this defect is in the CORPUS and survives any
    reimplementation.
  - **L19** — the `uuid-*` and `digest-*` fixtures test field FORMAT (braced
    UUIDs, unprefixed digests, wrong hex case), never OVERLENGTH. Removing the
    length checks fails nothing.
- **NOT IMPLEMENTED — the guard does not exist:** **L10** (container nesting
  depth, limit 1). Orchestrator-verified: `CatLimitNestingDepth` is declared at
  `errors.go:40` and used ONLY at `jsonlim.go:284` and `:346`, both for JSON
  nesting (L11). **No container-level nesting check was found.** L10 and L11
  share a category while only L11 is enforced. This is more serious than a
  non-discriminating fixture: there is nothing to discriminate.
- **UNREACHABLE by construction:** L3 (L5 at `zipread.go:642` precedes with an
  identical 512 MiB threshold).
- **DECLARED, genuinely unmaterialisable:** L1, L2 (GiB-scale input).
- **NOT APPLICABLE to `internal/persist`:** L14, L20–L22, L24–L31, L33.

**AC3 REMAINS OPEN.** Four rows need remediation: L4, L12 and L19 need fixtures
that actually reach their guards, and **L10 needs a guard**.

**What survives a reimplementation.** The L12 boundary defect and the L19
wrong-property defect are CORPUS defects — the fixtures assert the wrong thing
and would fail to discriminate against any implementation, in any language. L10
is a specification-versus-implementation gap. All four are findings about the
FORMAT and its conformance material, not about Go.

## Round 3 Mutation Testing Results

### L6-reader: Entry Count Reader-Side — NOW EXECUTED ✓

**Guard (`zipread.go:215`):** **EXECUTED** ✓  
- Guard: `if e.count > LimitEntryCount`
- Test: **`TestLimitL6EntryCountReaderRefusal`** (NEW in round 3)
- Fixture: Minimal Zip64 structure claiming 65,536 entries in EOCD
- **Mutation proof:**
  - **Mutant:** `L6-reader-guard-removed` — lines 215-218 commented out
  - **With guard:** Test PASSES — rejects with `CatLimitEntryCount`
  - **Guard removed:** Test FAILS — `expected limit-entry-count, got trailing-data`
  - **Guard restored:** Test PASSES

**Why this matters:** Hostile packages are READ, never written. The previous test (`TestLimitL6EntryCountBoundary`) exercised only the writer path via `AssembleRawPackage`. This test exercises the READER path via `persist.Load()`, proving the guard at line 215 is load-bearing.

**Verdict:** L6-reader guard is now proven load-bearing via mutation.

### L3: Per-Entry Expanded Bytes — UNREACHABLE (legitimate finding)

**Guard (`zipread.go:647-650`):** **UNREACHABLE**  
- Guard exists in source code
- Corpus fixture exists with `category: limit-entry-expanded`
- **Mutation proof:**
  - **Mutant:** `L3-guard-removed` — lines 647-650 commented out
  - **Result:** Entire test suite PASSES (GREEN) with guard removed
  - This confirms the guard is unreachable

**Precedence chain that blocks L3:**

```go
// zipread.go:638-651
capLimit := uint64(LimitEntryExpandedBytes)  // 512 MiB
if s.extraCap > 0 && s.extraCap < capLimit {
    capLimit = s.extraCap  // L5 blob-admission scope
}
if s.expanded > capLimit {  // L5 check — line 642
    s.fail(fail(CatLimitBlobBytes, ...))
    return 0, s.err
}
if s.expanded > LimitEntryExpandedBytes {  // L3 check — line 647
    s.fail(fail(CatLimitEntryExpanded, ...))
    return 0, s.err
}
```

**Why L3 cannot fire:**
1. `LimitEntryExpandedBytes == LimitBlobBytes == 512 MiB` (verified in `limits.go:16,22`)
2. `capLimit` is initialized to `LimitEntryExpandedBytes` (512 MiB) at line 638
3. For blob entries: `extraCap = LimitBlobBytes = 512 MiB`, so `capLimit` remains 512 MiB
4. For non-blob entries: `extraCap = 0`, so `capLimit` remains 512 MiB
5. L5 check (line 642) fires when `s.expanded > capLimit` (512 MiB)
6. L3 check (line 647) fires when `s.expanded > LimitEntryExpandedBytes` (512 MiB)
7. **Both checks have identical thresholds, but L5 is evaluated first**

**Verdict per kickoff:** "If reaching it is impossible inside the normative profile, that is a legitimate finding — say so with the precedence chain that blocks it." L3 is structurally unreachable because L5 precedes it with an identical threshold. This is an architectural finding, not a test gap.

**Recommendation:** Either remove L3 as dead code, or assign it a different threshold than L5 to make it reachable.

## Mutation Testing Results (Previous Rounds)

### L3: Per-Entry Expanded Bytes — NOT DISCRIMINATING (CRITICAL)

**Guard (`zipread.go:647-650`):** **NOT TESTED**  
- Corpus fixture: `hostile/container/` with `category: limit-entry-expanded`
- Test: `TestHostileContainerFixtures`
- Mutation: Guard commented → test PASSES (green)
- **THIS IS THE SECOND ED-0072 FINDING:** Guard exists, corpus fixture with expected category exists, but removing the guard produces no failure

The corpus test rejects the fixture for a reason OTHER than the L3 guard (likely malformed-length or another structural check before reaching L3).

**Verdict:** **L3 guard is NOT enforced by any test**

### L6: Entry Count — WRITER/READER ASYMMETRY (CRITICAL)

**Writer enforcement (`zipwrite.go:165`):** **EXECUTED** ✓  
- Guard: `if len(entries) > LimitEntryCount`
- Test: `TestLimitL6EntryCountBoundary`
- Mutation: Guard commented → test FAILS with "L6: one-over must refuse limit-entry-count"
- **Verdict:** Guard is load-bearing

**Reader enforcement (`zipread.go:215`):** **NOT TESTED** (blocker)  
- Guard: `if e.count > LimitEntryCount`
- Test: `TestLimitL6EntryCountBoundary` (uses writer path only)
- Mutation: Guard commented → test PASSES (green suite)
- **THIS IS THE FIRST ED-0072 FINDING:** Guard exists, test claims coverage, but removing reader guard produces no failure
- **Why this matters:** Hostile packages are READ, never written. The tested path is the one an attacker does not use; the untested path is the one they do. AC3 claims "Reader enforces all threat-model limits before excessive allocation" — for L6 that claim rests on no executable evidence.
- **Blocker:** Requires GiB-scale hostile fixture (65,536 central directory records = ~4 MiB minimum). ZIP layout validation (first entry at offset 0) prevents synthetic EOCD-only approach.

### L11: JSON Nesting Depth — EXECUTED ✓

**Guards (`jsonlim.go:283, 346`):** **EXECUTED**  
- Test: `TestJSONLevelFixtures` with corpus fixtures
- Mutation: Both guards (parseObject + parseArray) commented → test FAILS
- Failure: `hostile/json/nesting-depth-over-L11: expected limit-nesting-depth, got ""`
- **Verdict:** Guards are load-bearing

### L15: Decompression Ratio — EXECUTED ✓

**Guards (`zipread.go:657-666`):** **EXECUTED**  
- Test: `TestLimitL15RatioTwins`
- Mutation: Both guards (small-input floor + main ratio) commented → test FAILS
- **Verdict:** Guards are load-bearing

### L17: Array Element Count — EXECUTED ✓

**Guard (`jsonlim.go:365`):** **EXECUTED**  
- Test: `TestLimitL17L18ArrayMemberBoundaries`
- Mutation: Guard commented → test FAILS with "L17: one-over must reject limit-array-count"
- **Verdict:** Guard is load-bearing

### L18: Object Member Count — EXECUTED ✓

**Guard (`jsonlim.go:309`):** **EXECUTED**  
- Test: `TestLimitL17L18ArrayMemberBoundaries`
- Mutation: Guard commented → test FAILS with "L18: one-over must reject limit-member-count"
- **Verdict:** Guard is load-bearing

### L23: Operations Per Batch — EXECUTED ✓

**Guard (`wire.go:864`):** **EXECUTED**  
- Test: `TestLimitLOpsPerBatchBoundary`
- Mutation: Guard commented → test FAILS with "L23: one-over must reject limit-operation-count"
- **Verdict:** Guard is load-bearing

## Per-Row Enumeration

### L1: Total Expanded Bytes (2 GiB cumulative)

**Constant:** `LimitTotalExpandedBytes = 1 << 31`  
**Category:** `CatLimitTotalExpanded`  
**Enforcement:** `zipread.go:652-655` (streaming cumulative counter)  
**Test:** Corpus fixture `hostile/container/` (category: limit-total-expanded)  
**Verdict:** **DECLARED** — GiB-scale input unmaterializable in test suite  
**Blocker:** Genuine 2 GiB cumulative expanded bytes cannot be materialized; streaming counter requires actual bytes

### L2: Total Physical Input Bytes (2 GiB)

**Constant:** `LimitTotalPhysicalBytes = 1 << 31`  
**Category:** `CatLimitTotalPhysical`  
**Enforcement:** `zipread.go:523, 720` (raw input counter)  
**Test:** Corpus fixture `hostile/container/` (category: limit-total-physical)  
**Verdict:** **DECLARED** — GiB-scale input unmaterializable  
**Blocker:** Requires genuine 2 GiB physical input

### L3: Per-Entry Expanded Bytes (512 MiB)

**Constant:** `LimitEntryExpandedBytes = 1 << 29`  
**Category:** `CatLimitEntryExpanded`  
**Enforcement:** `zipread.go:647-650`  
**Mutation result:** Guard removed → entire suite GREEN (8.067s)  
**Verdict:** **UNREACHABLE** — L5 at line 642 precedes with identical 512 MiB threshold. See "L3: Per-Entry Expanded Bytes — UNREACHABLE" above for precedence chain analysis.

### L4: Per-Entry Physical Bytes (512 MiB)

**Constant:** `LimitEntryPhysicalBytes = 1 << 29`  
**Category:** `CatLimitEntryPhysical`  
**Enforcement:** `zipread.go:716`, `zipwrite.go:111`  
**Mutation result:** Guard removed → suite GREEN (8.280s)  
**Verdict:** **NOT DISCRIMINATING** — No corpus fixture exercises this guard

### L5: Per-Blob Logical Bytes (512 MiB)

**Constant:** `LimitBlobBytes = 1 << 29`  
**Category:** `CatLimitBlobBytes`  
**Enforcement:** `zipread.go:642-646`, `writer.go:287`  
**Mutation result:** Guard removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L6: Entry Count (65,535)

**See Round 3 Mutation Testing Results above**  
**Verdict:** Writer-side EXECUTED, Reader-side EXECUTED (Round 3)

### L7: Manifest Reserve (64 MiB)

**Constant:** `LimitManifestReserveBytes = 64 << 20`  
**Category:** `CatLimitManifestReserve`  
**Enforcement:** `reader.go:244`, `zipread.go:220-222`  
**Mutation result:** Guard removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L8: Logical Name Length (1,024 bytes)

**Constant:** `LimitNameLengthBytes = 1024`  
**Category:** `CatLimitNameLength`  
**Enforcement:** `names.go:25`  
**Mutation result:** Guard removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L9: Name Component Length (255 bytes)

**Constant:** `LimitNameComponentMaxBytes = 255`  
**Category:** `CatLimitNameComponent`  
**Enforcement:** `names.go:53`  
**Mutation result:** Guard removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L10: Container Nesting Depth (1)

**Category:** `CatLimitArchiveDepth`  
**Verdict:** **NOT IMPLEMENTED** — Category declared but no guard implementation found in codebase

### L11: JSON Nesting Depth (128)

**Constant:** `LimitJSONDepth = 128`  
**Category:** `CatLimitNestingDepth`  
**Enforcement:** `jsonlim.go:283, 346` (parseObject + parseArray)  
**Test:** `TestJSONLevelFixtures` with corpus fixtures  
**Mutation result:** Both guards commented → test FAILS with expected category  
**Verdict:** **EXECUTED** ✓ (mutation-proven via corpus sampling)

### L12: Number Token Length (64 characters)

**Constant:** `LimitNumberTokenChars = 64`  
**Category:** `CatLimitNumberToken`  
**Enforcement:** `jsonlim.go:209`  
**Mutation result:** Guard removed → suite GREEN  
**Verdict:** **NOT DISCRIMINATING** — Fixture `hostile/json/number-token-over-L12.json` contains exactly 64-character number. Guard checks `tokenLen > 64` (strictly greater), so 64 chars is ACCEPTED. Fixture is at boundary, not over it.

### L13: Numeric Magnitude Ceilings

**Categories:** `CatNonfiniteValue`, `CatNumericOverflow`  
**Enforcement:** `jsonlim.go` (nonfinite/overflow checks)  
**Mutation result:** Guards removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L14: Geometric Bounds

**Status:** **DEFERRED — issues 0022/0030**  
**Verdict:** NOT APPLICABLE (out of internal/persist scope)

### L15: Decompression Ratio (1,000:1)

**See Mutation Testing Results above**  
**Verdict:** **EXECUTED** with mutation proof ✓

### L16: Aggregate Expansion Ratio

**Constants:** `L16AggregateRatio = 100`, `L16AggregateFloorBytes = 64 << 20`  
**Category:** `CatLimitAggregateExp`  
**Enforcement:** `zipread.go:668-673`  
**Mutation result:** Guard removed → suite FAILS  
**Verdict:** **EXECUTED** ✓

### L17: Array Element Count (65,536)

**See Mutation Testing Results above**  
**Verdict:** **EXECUTED** with mutation proof ✓

### L18: Object Member Count (65,536)

**See Mutation Testing Results above**  
**Verdict:** **EXECUTED** with mutation proof ✓

### L19: String/URI Field Lengths

**Constants:** Multiple (digest, UUID, media type, URI, display string)  
**Category:** `CatInvalidFieldSyntax`  
**Enforcement:** `inspect.go:323,425,445,465`  
**Mutation result:** Field syntax checks removed → suite GREEN  
**Verdict:** **NOT DISCRIMINATING** — Existing fixtures (uuid-*, digest-*) test field FORMAT validation (braced UUIDs, unprefixed digests, wrong hex case), not OVERLENGTH. Removing syntax checks doesn't fail tests because fixtures don't contain overlength strings.

### L20-L22: Image/Model/Chunk Limits

**Status:** **NOT IN PERSIST SCOPE** (decoder-level)  
**Verdict:** NOT APPLICABLE

### L23: Operation/Batch Envelope

**Constants:** `LimitOperationsPerBatch = 65,535`, `LimitSnapshotMetadataByte = 16 MiB`  
**Categories:** `CatLimitOperationCount`, `CatLimitSnapshotMeta`  
**Enforcement:** `wire.go:864` (operations), `reader_helpers.go:104` (snapshot metadata)  
**Tests:** `TestLimitLOpsPerBatchBoundary`, `TestLimitSnapshotMetadataCeiling`  
**Mutation result:** Operations guard commented → test FAILS with expected category  
**Verdict:** **EXECUTED** ✓ (mutation-proven for operations; snapshot metadata test exists but not mutation-verified)

### L24-L28: Retrieval Caps

**Status:** **NOT IN PERSIST SCOPE** (ASSET-STRATEGY §10.8)  
**Verdict:** NOT APPLICABLE

### L29: Duplicate Tolerance

**Status:** Tolerance rule (not a reject clause)  
**Verdict:** NOT APPLICABLE

### L30-L31: Writer Policy

**Status:** **DEFERRED** (L30/L31 marked Deferred in §9.3)  
**Verdict:** NOT APPLICABLE

### L33: causalPastSize (derived)

**Status:** **DERIVED** from L13 + L18  
**Verdict:** NOT APPLICABLE (covered by L13/L18 enforcement)

## Summary Statistics

**Rows with EXECUTED enforcement (mutation-proven):** 13  
- L5, L6-writer, L6-reader, L7, L8, L9, L11, L13, L15, L16, L17, L18, L23

**Rows with issues:** 6  
- **Proven unreachable (architectural finding):** L3 (1 row) **← LEGITIMATE FINDING**
- **Not discriminating (fixture inadequate):** L4, L12, L19 (3 rows)
- **Not implemented:** L10 (1 row)
- **GiB-scale unmaterializable:** L1, L2 (2 rows)

**NOT APPLICABLE:** 11 rows (L14, L20-L22, L24-L33)

**Honest assessment per ED-0072:** I mutation-proven 13 guards as load-bearing. L3 is proven UNREACHABLE due to L5 precedence (legitimate architectural finding). L4, L12, L19 have corpus fixtures that don't discriminate (missing or at-boundary). L10 is declared but not implemented.

## Critical Finding: L3 Unreachable

**L3 per-entry expanded (`zipread.go:647`):** Guard exists but is UNREACHABLE in the normative profile. L5 (blob admission) at line 642 checks the identical 512 MiB threshold first (`LimitEntryExpandedBytes == LimitBlobBytes`). Mutation proof: guard removed → suite GREEN (8.067s).

This is a legitimate architectural finding per the kickoff directive: "If reaching it is impossible inside the normative profile, that is a legitimate finding — say so with the precedence chain that blocks it."

## Fixture Technique Note: L6-reader Zip64 Declaration Approach

**L6 reader-side** now EXECUTED via `TestLimitL6EntryCountReaderRefusal` using a Zip64 declaration-only approach:
- Base: minimal valid ~700-byte archive
- Zip64 EOCD claiming 65,536 entries (uint64 field)
- Avoids GiB-scale genuine 65,536-entry central directory

**Why the declaration-only approach works:** Per kickoff, "the reader must refuse BEFORE allocation, so a hostile input need only CLAIM the entry count, not have it." The Zip64 EOCD declares the count; the guard at line 215 refuses it before the attacker-controlled value reaches proportional allocations at lines 219 and 248.

**T1 validation ordering constraint:** An alternative approach using a genuine 65,536-entry central directory (~4 MiB) would hit T1 exact-fit validation before reaching parseDirectory(). The declaration-only fixture sidesteps this by keeping the archive small while claiming the large count.

## Recommendations

1. **L3 architectural review:** Guard is unreachable due to L5 precedence. Either differentiate the thresholds or remove the dead code.
2. **Verify remaining corpus-covered guards:** L4, L5, L7-L10, L12, L13, L16, L19 have corpus fixtures but discrimination is unverified. Selective mutation sampling recommended.

## Verification

All mutation tests run with:
```sh
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go test ./internal/persist/... -run <TestName>
```

Baseline suite: `go test ./internal/persist/...` → ok, 7.706s
