# 0029: Implement container persistence and recovery

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0005, 0011, 0023, 0028

## Current disposition — 2026-10-05

Keep partial for complete persistence/recovery conformance.

`crates/domowik-container/` provides Plan, guarded parsing, descriptors and
degraded-mode admission. Current local round-trip/fault tests do not close
every §9.7 platform/fault, opaque-data and §5.1 retention criterion. Audit
that matrix and bridge inspection of degraded plans; do not claim power-loss
safety or complete recovery UI.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

Activated 2026-08-25 under ED-0021 on branch `issue/0029-persistence-recovery`.
Consumes normative FORMAT-SPEC §7/§8/§9, issue 0011's delivered replay/session
material as a labeled provisional handoff, and the issue 0028 fixture corpus
with machine-readable expectations. Numeric values only via named parameters
or §9.3 quantities; no Done while 0005/0011/0004 remain unclosed under their
recorded gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0029-persistence-recovery` merged to main by `e991b9d` with
SBOM refresh; full main `./scripts/verify.sh` exits 0. Delivered:
`internal/persist` read/write pipeline implementing the §7 profile (manifest,
closed namespaces, batch framing with pre-replay malformed-framing checks,
compression allowlist), §9.7 S0–S5 atomic replacement with S4 platform
dispatch and BEHAVIOR-SPEC §9.2 external-modification baseline detection,
fail-closed limits enforcement before allocation/extraction, snapshot/ops/
tombstone persistence per the retention model, benchmarks evidence recorded.
Review iterations fixed: S1 entry-by-entry streaming emission, a
StreamEntry security bypass (digest/L5 gating at EOF arbitration), error-
lifetime comparison, race-evidenced concurrency. Adversarial reviewer accepts
ALL code; independent verification included `-race` runs.

**Disposition: `In Progress` — AC2/AC4/AC5 satisfied; AC1 partially; AC3 via
executable probes plus declarative GiB-scale rows.** Six records sit in
CLOSED fail-safe skip sets pending upstream owners: four 0028 fixture defects
(missing-manifest generator bug, spanned-disk-number mispatch, ratio-twin
stale digest, duplicate-object-keys artifact without duplicates),
duplicate-batch-ids materialization impossibility (0028/0004), and V2's LWW
causality conflict versus delivered 0011 semantics (0004/0011 ruling). Skips
fail if divergence disappears or unexpected mismatches appear. Dependents may
consume this delivered material as a labeled provisional handoff under
partial-scope semantics.

## Original problem

Implement package read/write, manifests, operation framing, snapshots, and
old-or-new atomic replacement. This separates persistence from 0010's entity
projection and fixes the earlier dependency inversion.

## Acceptance criteria

- [ ] Save/load/replay round trips semantic projection and preserved opaque
      data across all golden fixtures.
- [ ] Fault injection around write, flush, rename/replace, and recovery leaves
      the previous or new complete package readable under documented platform
      assumptions.
- [ ] Reader enforces all threat-model limits before excessive allocation or
      extraction.
- [ ] Snapshot frontier, retained operations, and tombstones satisfy the
      normative retention model.
- [ ] Benchmarks report package size, load/save time, peak memory, and streaming
      behavior for the representative corpus.

Issue 0018's release audit (`ccfe953`) routes a remaining closure gap here:
current save/migration hooks prove selected old-or-new outcomes but do not cover
the complete normative platform/recovery fault matrix or every golden opaque-
data round trip. Existing proofs remain provisional and must not be generalized
beyond their documented platform assumptions.

Continuation activated under ED-0027 on branch
`issue/0029-release-remediation`: extend executable fault/opaque-data coverage
within documented platform capabilities; unavailable cross-platform or
power-loss outcomes remain assessed-and-deferred, never simulated.

## Product defect routed here 2026-08-26 (ED-0032)

Issue 0028's reproduction pass found that `internal/persist/reader.go:468`
calls `checkReservedRanges(rangesByActor)` on the `Load` path and discards the
returned `*Error`. `internal/persist/inspect.go:206` handles the same call
correctly.

FORMAT-SPEC §4/§7.6 require structural log checks to precede replay.
Overlapping or gapped actor ranges still reject, but only incidentally,
downstream in `document.ReplayFromSnapshot`, and with the wrong diagnostic.
`go vet` cannot detect a discarded non-error-typed return, so a regression
test is required alongside the fix.

This joins the ED-0027 remediation continuation already queued for this issue.

## Second routed defect 2026-08-26 (ED-0033, UNEXECUTED analysis)

`internal/document/projection.go` `frameAndOrder` classifies batches against
the per-actor watermark and builds its `nonSkipped` set BEFORE per-actor
reserved-range chaining. A B3 range violation lying entirely below the
snapshot watermark therefore has no structural check left on the `Load` path,
while `Inspect` — parse-only, with no watermark logic — still rejects it.
Ranges straddling the watermark are still caught as torn batches.

Combined with the discarded `checkReservedRanges` error above, this breaks
ED-0023's parity premise: `internal/persist/inspect.go:206`'s comment claims
those ranges "fail here with the same malformed-framing token the replaying
path produces", and that is now false twice over.

The issue 0028 pair produced a concrete witness but did not execute it,
because proving it needs a new `internal/persist` test outside that pair's
scope. Its written analysis is preserved at
`~/projects/domowik-observatory/routed/0029-reader-reserved-range-defect.md`.

Do not close this by restoring the discarded error alone. An open normative
question is routed to issues 0005 and 0004 and gates the fix direction: is
§7.6's structural B3 check unconditional over every retained entry, or scoped
to batches surviving §4.5 I2 watermark classification? If an owner rules it
watermark-scoped, this becomes an `Inspect` over-rejection and ED-0023's
parity language needs amending instead of the `Load` path.

## Third routed defect 2026-08-26 (ED-0034)

`internal/persist/wire.go:85-88` exempts `null` from the type-check for the
required `assetRoot` field, justified in-comment by "generators may serialize
an absent table as null". FORMAT-SPEC §7.4 line 852 makes `assetRoot`
required with type "Ascending-by-digest array"; `null` is not an array, and
generator behavior cannot justify reader laxity.

Issue 0028 fixed its dev-only generator to emit `[]` and left product code
untouched. This issue owns deciding and implementing whether the reader
rejects `null`, with a regression fixture. It must not be closed by loosening
the corpus back. The acceptance category is routed to issue 0005 and gates the
fix direction.

## Partial round integrated 2026-08-31 (ED-0069)

Branch `issue/0029-fault-opaque-coverage` (`a090e56`..`d09e651`, three commits)
merged by `5e47f48` with SBOM refresh; full main `./scripts/verify.sh` exits 0
with a clean tree. Evidence: `go test ./internal/persist/...` ok, and
`go test -race ./internal/persist/...` ok at 60.8s.

**Delivered — ED-0032 is CLOSED.** `internal/persist/reader.go:468` was calling
`checkReservedRanges(rangesByActor)` and discarding the returned `*Error`, so
overlapping and gapped actor ranges passed the structural gate and failed later
with the wrong diagnostic. Error handling is restored to parity with
`inspect.go:206`, with a regression test and a contiguous-ranges positive
control. The orchestrator verified the defect on main before the work started
and swept the tree: **this was the only bare-call site of that shape in any
non-test Go under `internal/` and `cmd/`**, so it is a single defect and not a
pattern. `go vet` does not flag it and `verify.sh` was green with it present,
because discarding a return value is legal Go — the regression test is the only
thing holding it closed.

**What the regression test actually discriminates on, verified by mutation
rather than accepted.** The orchestrator reverted the fix in a detached
worktree and ran the test twice. Both times ONLY the diagnostic-substring
assertions failed. Specifically:

- `Category == CatMalformedFraming` and `Stage == StageFraming` **PASS on the
  broken code** — the malformed input already rejected at framing, from a
  different check, with a different message. That is exactly the "rejects
  incidentally" behaviour ED-0032 describes, and it means those two assertions
  have no discriminating power here.
- The `actorID` assertion added by `07f2c0b` to strengthen the discriminator
  **also passes on the broken code**, because the other check's message names
  the actor too. It was added in good faith to fix an overstated claim and it
  does not narrow anything.
- The whole discrimination therefore rests on the words `overlapping` and
  `gap` appearing in the diagnostic. That is real but brittle in one direction:
  if the other check's wording ever comes to contain either word, this test
  goes green while the defect is back.

The commit message on `fe44c77` claims the test proves rejection "at
StageFraming with CatMalformedFraming, not incidentally downstream". **That
claim is false and is corrected here rather than in a rewritten commit.**

**Delivered — the fail-safe skip audit (Task 3).** Of the six records the issue
text describes as sitting in "CLOSED fail-safe skip sets": three are DISCHARGED
(missing-manifest, spanned-disk-number, ratio-twin — the generator defects were
fixed and the suppressions removed), and three remain LIVE and were each
demonstrated still capable of failing (duplicate-batch-ids,
duplicate-object-keys, V2 LWW causality). No suppression has silently lost the
ability to fail. Issue 0028's concurrent pair has not yet resolved any of the
three live ones.

**Tracker imprecision, recorded so the next pair does not pay for it.** "Six
records in CLOSED fail-safe skip sets" implies one uniform mechanism. There are
three: removed-and-superseded, per-fixture case branches, and
`executionBlocked` records. Only one is a `t.Skip`, so searching by skip syntax
finds almost none of them. Search by fixture ID and defect name instead.

**NOT started, and deferred deliberately.** Task 1 (the §9.7 F1–F9 fault and
recovery matrix) and Task 2 (opaque-data round trips across all golden
fixtures). The worker judged its remaining budget insufficient and asked rather
than starting; the orchestrator directed the handoff. The platform assessment
is complete and carried forward — darwin process-clash only, with unavailable
outcomes assessed and deferred, never simulated — and the fixture snapshot is
at `fa4d741`.

Status remains `In Progress`: AC1 and AC2 are open, and the upstream gates
remain open.

## Second continuation integrated 2026-08-31 (ED-0069)

Branch `issue/0029-fault-matrix-part2` (`5267b56`..`53c4877`, 7 commits) merged
by orchestrator with SBOM refresh; full main `./scripts/verify.sh` exits 0 with
a clean tree.

**Delivered — AC1 (opaque-data round trips) is SATISFIED.** Extended existing
`TestRoundTripGoldenPackages` (`internal/persist/smoke_test.go`) to assert
`ManifestUnknown`, `SnapshotUnknown`, and retained-batch preservation across
save/load/replay for all golden packages in EXPECTATIONS.md (thin-valid.domo,
portable-valid.domo). Both golden fixtures are conforming v0.1 packages with no
unknown fields, as expected. Round-trip assertions will catch any future golden
fixture carrying unknown optional fields.

**Delivered — AC2 (fault matrix) process-crash coverage ONLY; AC2 remains
OPEN.** Added discriminator validation for all F1–F9 tests
(`save_fault_discriminators_test.go`), bounded X3 injection at 3 representative
write boundaries (`save_fault_x3_test.go`), and platform disposition below.
Review found F1 had inverted logic and F2 checked nothing discriminating; both
fixed and re-verified.

### §9.7 Fault Matrix Disposition (AC2 partial, darwin/apfs only)

**Executable on this host (darwin, macOS 14.x, apfs, arm64):** Process-crash
semantics only, via hook-based deterministic injection. No timing races, no real
crashes. These tests prove what happens when a syscall returns an error or a
hook aborts mid-phase.

| Outcome | Coverage | Test/Evidence |
|---------|----------|---------------|
| F1: first save, no prior target, abort before replace | Process-crash | `TestFaultF1FirstSaveNoPriorTarget` + discriminator validation |
| F2: candidate write interrupted | Process-crash | `TestFaultF2WriteInterrupted` + discriminator + bounded X3 |
| F3: candidate validation fails | Process-crash | `TestFaultF3CandidateInvalid` + discriminator |
| F4: file-content sync fails | Process-crash | `TestFaultF4SyncFailure` + discriminator |
| F5: atomic replacement interrupted | Process-crash | `TestFaultF5ReplaceInterrupted` + discriminator |
| F6: replacement completed, directory sync fails | Process-crash | `TestFaultF6DirSyncFailure` + discriminator |
| F7: recovery-artifact write/sync fails | Process-crash | `TestFaultF7ArtifactExcluded` + discriminator |
| F8: cleanup interrupted | Process-crash | `TestFaultF8CleanupSafety` + discriminator |
| F9: autosave failure | Process-crash | `TestFaultF9AutosaveRetry` + discriminator |
| X1: temporary creation failure | Process-crash | `TestFaultX1TempCreationFailure` |
| X2: short write, ENOSPC, EIO | Process-crash | Covered by F2's `failingWriter` |
| X3: interruption at write boundaries | Process-crash (3 boundaries) | `TestFaultX3WriteBoundaries` (header, mid-manifest, first-entry) |
| X4: sync success/failure at S3 | Process-crash | Covered by F4 |
| X5: validation failure post-write | Process-crash | Covered by F3 |

**Deferred (cannot be produced on this host):** The following outcomes require
capabilities this host does not have and are ASSESSED but NOT TESTED. Per
ED-0027, unavailable outcomes are assessed and deferred, never simulated. A
simulated power-loss test claiming a real guarantee is worse than an absent one.

| Outcome | Reason Deferred | Provenance |
|---------|-----------------|------------|
| **Power-loss injection** (true crash mid-fsync, mid-replace, mid-dir-sync) | Apple documents `F_FULLFSYNC` as best-effort: "data can still be lost in the case of sudden power loss". No official Apple document establishes an atomic-replace durability profile. | P6: fcntl(2) F_FULLFSYNC, "Reducing disk writes" article |
| **Kernel-crash vs process-crash distinction** | Darwin rename(2) atomicity is documented for process crashes but not kernel panics. | P6: no official Apple rename durability document found |
| **Cross-device EXDEV** | S0 mandates same-filesystem temporary; EXDEV is unreachable by construction. | FORMAT-SPEC §9.7 S0/S4 |
| **Concurrent-writer outcomes** | BEHAVIOR-SPEC §9.1 non-guarantee; last-replace-wins; not a failure mode. | BEHAVIOR-SPEC §9.1 |
| **X6 sharing-violation enumeration** | Microsoft does not enumerate codes publicly (P3). | FORMAT-SPEC §9.7 X6 |
| **X9 cache-GC anomaly** | Requires cache implementation + deliberate corruption; deferred to integration. | FORMAT-SPEC §9.7 X9 |

**What these tests DO prove:**
- Process-crash-class error handling (syscall error returns)
- S0–S5 state-machine phase discrimination via error-message substrings
- RS1/RS5/RS6/RS7/RS8 recovery-state classification
- BEHAVIOR-SPEC §9.2 baseline detection
- Leftover cleanup safety (artifacts valid-or-excluded, never partially promoted)

**What these tests DO NOT and CANNOT prove:**
- Power-loss durability (S3 fsync success does NOT guarantee data survives power
  loss on macOS per P6)
- Kernel-panic atomicity (rename atomicity claim is process-crash only)
- Cross-platform behaviors (Linux ext4, Windows NTFS profiles)

**AC2 status:** OPEN. Process-crash coverage is complete for darwin. Power-loss
durability and cross-platform profiles remain blocked on:
1. A host with a validated durability profile (Linux ext4 or Windows NTFS per
   FORMAT-SPEC §9.7 table)
2. BEHAVIOR-SPEC §9.5 fault-injection suites passing on that host

**Disposition: `In Progress`** — AC1 satisfied, AC2 open (process-crash only),
upstream gates (0005/0011/0004) remain open.

## Second continuation handoff 2026-08-31 (ED-0069 dispatch, part 2)

Branch `issue/0029-fault-matrix-part2` (`5267b56`..`53c4877`, 7 commits) continues
the ED-0027 remediation. Worker w0029 (OpenCode litellm/gemini-3.1-pro-preview),
reviewer r0029 (OpenCode litellm/gemini-3.1-pro-preview after OpenAI quota
exhausted). Base: main at `5267b56`.

**Delivered — Task 1 (AC2): F1–F9 fault/recovery matrix extension.** Nine
discriminator-validation tests (`save_fault_discriminators_test.go`) prove each
F1–F9 test's weight-bearing assertion by running a named mutant and verifying
the discriminator fails. Bounded X3 injection (`save_fault_x3_test.go`) covers
3 representative write boundaries (header, mid-manifest, first-entry) with
documented coverage rationale. All tests hook-based, deterministic,
timing-free, process-crash semantics only.

**Delivered — Task 2 (AC1): opaque-data round trips.** Extended
`TestRoundTripGoldenPackages` (`smoke_test.go`) to assert `ManifestUnknown`,
`SnapshotUnknown`, and retained batches (count, ActorID, CounterBase, operation
count) are preserved across save/load cycles. Covers all golden packages in
EXPECTATIONS.md (`thin-valid.domo`, `portable-valid.domo`). No fixtures failed
to round-trip; both golden packages are conforming v0.1 with no unknown fields
(expected).

**Platform assessment (carried forward, unchanged).** darwin/macOS 14.x,
arm64, apfs. Process-crash fault injection only. Power-loss and kernel-crash
outcomes are DEFERRED, not simulated, per ED-0027.

### §9.7 Fault Matrix Disposition: Executable vs. Deferred

**Executable on this host (darwin process-crash):**

All F1–F9 injection points are covered with hook-based deterministic tests
proving process-level error-handling semantics (what happens when a syscall
returns an error or a hook aborts mid-phase):

| Point | Test | Discriminator Proven | Outcome |
|-------|------|---------------------|---------|
| F1 | TestFaultF1FirstSaveNoPriorTarget + TestDiscriminatorF1TargetMustNotExist | Target must not exist | No target file, leftover cleanable |
| F2 | TestFaultF2WriteInterrupted + TestDiscriminatorF2TargetIsOLD | OLD entity count preserved | Target untouched (OLD), leftover cleanable |
| F3 | TestFaultF3CandidateInvalid + TestDiscriminatorF3MessageIsCandidateInvalid | Error contains "candidate-invalid" | Abort S2, target untouched |
| F4 | TestFaultF4SyncFailure + TestDiscriminatorF4MessageIsF4 | Error contains "F4" | Abort S3, target untouched |
| F5 | TestFaultF5ReplaceInterrupted + TestDiscriminatorF5MessageIsF5 | Error contains "F5" | Report error, EITHER readable (OLD) |
| F6 | TestFaultF6DirSyncFailure + TestDiscriminatorF6DurabilityCaveat | DurabilityCaveat=true | NEW readable, no rollback, caveat |
| F7 | TestFaultF7ArtifactExcluded + TestDiscriminatorF7InvalidCandidateDiagnostic | Diagnostic "invalid candidate" | Target unaffected, artifact excluded (RS6) |
| F8 | TestFaultF8CleanupSafety + TestDiscriminatorF8TargetStaysValid | TargetCategory=ValidTarget | Artifacts valid-or-excluded, never promoted |
| F9 | TestFaultF9AutosaveRetry + TestDiscriminatorF9AutosaveReturnsOk | ok=false, then ok=true | Non-blocking, retry succeeds |

Extended coverage: X1 (temp creation failure), X2 (covered by F2 EIO injection),
bounded X3 (TestFaultX3WriteBoundaries: 3 representative write boundaries), X4
(covered by F4 sync failure), X5 (covered by F3 validation failure post-write).

**What these tests DO prove:**
- Process-crash-class failures (error returns from syscalls, aborted hooks)
- S0–S5 state-machine phase discrimination
- RS1/RS3/RS4/RS5/RS6/RS7/RS8 recovery-state classification
- §9.2 baseline detection (external-modification detection on matching-baseline path)
- Leftover cleanup safety (F8 valid-or-excluded invariant)

**What these tests DO NOT and CANNOT prove:**
- **Power-loss durability.** FORMAT-SPEC §9.7 P6 states: Apple documents
  F_FULLFSYNC as best-effort; "data can still be lost in the case of sudden
  power loss" (Apple article "Reducing disk writes"). No official Apple
  document establishes an atomic-replace durability profile for rename(2). S3
  fsync success does NOT guarantee data survives power loss on macOS. The macOS
  platform row in §9.7 is explicit: "NONE VALIDATED — process-crash-only".
- **Kernel-panic atomicity.** Darwin rename(2) atomicity is documented for
  process crashes but not kernel panics (P6: no official Apple rename durability
  doc found).
- **Cross-platform behaviors.** Linux ext4 and Windows NTFS profiles claim
  power-loss durability "after suite passes"; those suites do not exist and
  cannot run on darwin.

**Deferred outcomes (cannot be produced on this host):**

| Outcome | Reason Deferred | Provenance |
|---------|----------------|------------|
| True power-loss injection | Apple F_FULLFSYNC is best-effort; no atomic-replace durability profile | FORMAT-SPEC §9.7 P6 (fcntl, "Reducing disk writes") |
| Kernel-crash vs process-crash | Darwin rename(2) atomicity not documented for kernel panics | P6 (no official Apple durability doc found) |
| Cross-device EXDEV | S0 mandates same-filesystem temporary; EXDEV unreachable by construction | FORMAT-SPEC §9.7 S0/S4 |
| Concurrent-writer outcomes | BEHAVIOR-SPEC §9.1 non-guarantee; last-replace-wins | BEHAVIOR-SPEC §9.1 |
| Cache-GC anomaly (X9) | Requires ASSET-STRATEGY §10.7 fail-closed collector (not yet implemented) | FORMAT-SPEC §9.7 X9 |

**AC2 disposition: OPEN.** F1–F9 process-crash coverage is delivered and
executable. Power-loss and kernel-crash outcomes remain deferred pending a
validated platform profile (Linux ext4 or Windows NTFS with passing fault-injection
suite per §9.7). AC2 is satisfied for the darwin process-crash subset; the
full matrix stays open on the deferred cells.

**Review findings and fixes.** Reviewer r0029 (OpenCode
litellm/gemini-3.1-pro-preview) found two discriminator tests broken:
TestDiscriminatorF1 had inverted logic (accidentally passed because the
inversion cancelled out), TestDiscriminatorF2 checked nothing discriminating
(used same input for both generations, checked entities == 0). Both fixed in
commit `b8f8812`. All nine F1–F9 discriminators verified correct by mutation
inspection after fix. This is the ED-0041/ED-0053/ED-0071 class: a check that
appears to pass but observes the wrong thing.

**Verification:** `./scripts/verify.sh` exits 0, tree clean. Tests: 9
discriminator validations (F1–F9), 3 X3 boundary tests, extended round-trip
test with opaque-data assertions. Full verify: build, test, gofmt, vet, mod
tidy, SBOM verify, CLI probes.

Status remains `In Progress`: AC1 satisfied (opaque-data round trips), AC2
partially satisfied (darwin process-crash only; power-loss/kernel-crash
deferred), upstream gates (issues 0005/0011/0004) remain open.

## AC3 limits-enforcement audit integrated 2026-08-31 (ED-0069, ED-0076)

Branch `issue/0029-limits-enforcement` merged; full main
`./scripts/verify.sh` exits 0 with a clean tree. Evidence in TRACKED content at
`docs/evidence/0029-limits-enforcement/LIMITS-AUDIT.md`.

**AC3's wording may NOT be read as satisfied.** The criterion says "Reader
enforces all threat-model limits before excessive allocation or extraction",
and it had been recorded satisfied "via executable probes plus declarative
GiB-scale rows". A declarative row is a WRITTEN CLAIM that a limit is enforced.
This round separated the rows that are genuinely enforced from the rows that
merely say so.

**Of 33 §9.3 rows:**
- **6 EXECUTED with mutation proof** — L6-writer, L11, L15, L17, L18, L23. Each
  guard was removed and a NAMED test went red.
- **2 PROVEN NON-DISCRIMINATING — L3 and L6-reader. Their guards delete with a
  green suite.**
- ~11 with corpus fixtures whose discrimination is unverified (L4, L5, L7–L10,
  L12, L13, L16, L19), 2 unmaterialisable at GiB scale (L1, L2), 11 not
  applicable.

**The L6 writer/reader asymmetry is the finding.** Orchestrator-verified
independently: removing `if e.count > LimitEntryCount` at
`internal/persist/zipread.go:215` leaves the whole `internal/persist` package
GREEN (`ok, 7.781s`). The WRITER guard at `zipwrite.go:165` IS load-bearing —
`TestLimitL6EntryCountBoundary` reddens when it is removed — but that test
exercises only the writer path. **Hostile packages are READ, never written. The
tested path is the one an attacker does not use; the untested path is the one
they do.** L6 exists to refuse an entry-count explosion before allocation,
which is a reader-side obligation by construction.

**L3 fails the same way in a different disguise:** its corpus fixture rejects
for an EARLIER structural reason and never reaches the L3 guard, so the fixture
looks like coverage and is not — ED-0053's wrong-reason failure inside a
security limit.

**Deliberately NOT repaired in this round.** Finding these is the deliverable;
fixing a threat-model guard deserves its own dispatch and its own adversarial
review rather than a tail-end patch by the pair that found it. Recorded as
ED-0076 with a closure gate.

**A note on the evidence location.** The audit first landed at
`internal/persist/LIMITS-AUDIT.md`, inside a Go package directory. Asked
whether that had precedent, the pair checked rather than guessed and found the
established `docs/evidence/NNNN-slug/` pattern already used by
`0005-container-versioning/` and `0006-predicates/`, then relocated to match.

Status remains `In Progress`: AC3 is OPEN with two proven gaps and ~13
unverified rows, AC2 is open per the fault-matrix entry above, and the upstream
gates are unchanged.

## L6-reader guard closed; L3 ruled unreachable — integrated 2026-08-31 (ED-0076)

Branch `issue/0029-l3-l6-guards` merged by `46bc2c9`; full main
`./scripts/verify.sh` exits 0 with a clean tree. Evidence in
`docs/evidence/0029-limits-enforcement/MUTATION-EVIDENCE-ROUND3.md` and the
updated `LIMITS-AUDIT.md`.

**L6-reader now has EXECUTED coverage.** `TestLimitL6EntryCountReaderRefusal`
drives the READER path with a Zip64 EOCD declaring 65,536 entries — the archive
stays small because the reader must refuse before allocation, so the input need
only CLAIM the count. Orchestrator-verified by mutation: removing
`if e.count > LimitEntryCount` at `zipread.go:215` turns the test RED with
"expected limit-entry-count, got trailing-data". **It asserts the CATEGORY, so
it distinguishes rejection BY L6 from incidental rejection by a later check** —
the discrimination the earlier ED-0032 test lacked.

The test comment names what the guard protects: `e.count` is attacker-supplied
and reaches two proportional allocations at `zipread.go:219` (reserve) and
`:248` (entries slice). A future reader now sees why the guard exists rather
than inferring it is a range check.

**L3 is UNREACHABLE, and that is a finding rather than a gap.** The L5 guard at
`zipread.go:642` precedes it with an identical threshold, so no conforming input
can reach L3's check. Recorded with that precedence chain rather than satisfied
by a fixture that only appears to reach it — the failure mode the previous L3
fixture actually exhibited.

**Row counts, unrounded:** 7 of 33 EXECUTED with mutation proof (L6-writer,
L6-reader, L11, L15, L17, L18, L23); 1 structurally unreachable (L3); ~11 with
fixtures whose discrimination is unverified; 2 unmaterialisable at GiB scale;
11 not applicable.

**AC3 remains OPEN.** ED-0076's gate requires both halves and ~13 rows are still
unverified. Closing two guards does not satisfy "enforces ALL threat-model
limits".

**A self-contradiction corrected before integration, recorded as ED-0076
amendment (a).** After the working test landed, a later commit documented
L6-reader as BLOCKED — "hostile fixture unmaterializable within validation
ordering". That was false and the pair's own earlier commit disproved it: the
test passes on main and reddens when the guard is removed, so the fixture
demonstrably reaches `parseDirectory()`. The pair had re-attacked the problem
with a genuine 65,536-entry directory, hit a REAL T1 ordering wall on THAT
approach, and written the blocker up as a property of the ROW rather than of the
METHOD. The T1 finding is retained as a Fixture Technique Note — it is a genuine
navigational hint — and the row is EXECUTED. **A blocker discovered while
exploring one approach is not evidence about a row already closed by another.**

**One contract breach, recorded because the claim did not hold.** The branch was
NOT gofmt-clean at handoff (`internal/persist/limits_test.go` carried a
trailing-whitespace line), so the reported `verify.sh` exit 0 cannot have
happened. Fixed at integration. This is the second occurrence today after issue
0011's round 3; a pair reporting a verification it did not run is the ED-0068
family applied to the verify gate itself.

## Rust implementation — 2026-08-31

Everything above describes the frozen Go tree at `_reference/go/`. The Rust
container implements FORMAT-SPEC §7 and §9.7 afresh.

Delivered in `crates/domowik-container`:

- **§7 package structure**: manifest, snapshot, and `ops/` batch entries at the
  §7.6 name, with name/content binding, deterministic enumeration from names
  alone, and the L23 envelope on operations per batch and per package.
- **§9.7 save state machine S0-S5** in full: collision-safe temporary sibling,
  symlink and non-regular targets refused, complete candidate write,
  reader-equivalent validation of the candidate INCLUDING projecting its log,
  content `fsync`, atomic `rename`, parent-directory `fsync`.
- **Fail-closed limits before allocation**: L6 entry count checked from the
  central directory before enumeration, declared entry size checked before
  reserving, and the reader capped so a lying header cannot outrun the limit.
- A pre-flight log check, so a structurally malformed log never becomes a
  candidate at all.

### What is claimed, and what is not

Namespace atomicity and process-crash durability hold. **Power-loss durability
is NOT claimed on macOS**, and the code says so where a reader will meet it:
§9.7's platform profiles validate it for Linux/ext4 and Windows/NTFS after
fault-injection suites that do not exist here, and record that Apple documents
`F_FULLFSYNC` as best-effort. S3 and S5 are performed regardless — the claim is
withheld, not the work.

### Not delivered

- **Recovery artefacts and the RS1-RS5 state table.** No autosave, no recovery
  files, no artefact precedence by monotonic sequence number. The save path is
  crash-safe; there is nothing to recover FROM because nothing is written
  besides the target.
- **Fault-injection suites** (BEHAVIOR-SPEC §9.5). Without them no power-loss
  claim can be made for any platform, which is why none is.
- **§9.2 external-modification baseline detection.** `Session` tracks dirty
  against its own edits, not against the file changing underneath it.
- The six CLOSED fail-safe skip records above belong to the Go corpus work and
  do not transfer.

Status stays In Progress.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** The §7 package, the §9.7 S0-S5 save machine and fail-closed limits are delivered, and seven injected-failure tests cover the §9.7 guarantee table. NOT delivered: recovery artefacts and the RS1-RS5 states, fault-injection suites, and §9.2 external-modification detection.

## Wire contract reproduced as divergent — 2026-09-12 (finalize/0029)

The Rust container writes a format of its own. It is not the §7 package, and
the evidence is at `docs/evidence/0029-wire-contract/WIRE-DIVERGENCE.md`.

Seven normative divergences, W1–W7 in that record. The two the dispatch names
are the manifest at `manifest.json` rather than §7.3/§7.4's `domowik.json`
(`lib.rs:159`) and the snapshot at `document.json` rather than §7.5's
`snapshot/state.json` (`lib.rs:160`). The other five are the same defect seen
from further back: `formatVersion` is a `u32` where §8.1 V1 requires the
`MAJOR.MINOR` string, `profile` and the `entries` integrity inventory are
absent, `assetRoot` is a string array that disappears when empty, and the
snapshot frontier lives in the manifest. **That last one cannot be fixed by
renaming anything** — §7.5 says the frontier of record is S2 inside
`snapshot/state.json` and that "no other location may carry or shadow it", so
the location is what the spec forbids.

**The prior round's claim is corrected.** The 2026-08-31 Rust entry in this
issue says it delivered "§7 package structure: manifest, snapshot, and `ops/`
batch entries at the §7.6 name". Only `ops/` and `blob/` are at their spec
names. The manifest and snapshot are not, and the module's own doc comment
said so plainly the whole time ("a WORKING SUBSET, not the full container
specified in `docs/FORMAT-SPEC.md`"). The issue text generalized past it.

**Measured impact.** Over all 40 tracked `.domo` fixtures the reader accepts
none. Both golden conforming packages are refused for the missing manifest, so
AC1's "across all golden fixtures" has never been exercised against a
conforming package. 30 of the 36 hostile fixtures are refused the same way,
before reaching the check each exists to provoke.

**`tests/hostile_corpus.rs` is vacuous, proven by mutation rather than argued.**
The row-L6 reader guard (`lib.rs:857-864`) and blob digest verification
(`lib.rs:938-944`) were both deleted and the suite stayed green, 3 passed 0
failed. Its positive control passes for a separate reason: it writes its own
package with the writer, so it never meets a fixture either. Mutations reverted
and re-run green before any other work. This is ED-0089's shape with real code
and real fixtures behind it instead of a restated literal.

**Compatibility inventory, completed before proposing a change.** No released
schema (§8.2 V6, ADR 0007, ED-0051's unfillable chain), no git tag, no tracked
artifact carrying the runtime names, and the two entry-name constants are
private to `crates/domowik-container/src/lib.rs` with no other crate naming
either string. The whole population at risk is `.domo` files a developer wrote
with the current build.

**Not mutating the wire contract until §7's owner rules.** Refusing those files
outright and reading the old names for one release are both defensible; the
second is a documented deviation from §7.3's closed namespace rather than a
courtesy, because under that namespace `manifest.json` is `invalid-entry-name`.
Accepting both indefinitely is ruled out here. Question sent to root with the
inventory; issue 0005 owns §7.

## ED-0108 executed — 2026-09-12 (finalize/0029)

Seven commits, `287053b`..`18476c6`. `cargo test --workspace`, `cargo clippy
--workspace --all-targets -- -D warnings` and `cargo fmt --check` all exit 0.
Full `verify.sh` not yet run: it needs the shared slot.

**The wire contract is the §7 one.** The manifest is at `domowik.json` and the
snapshot at `snapshot/state.json`; `formatVersion` is the §8.1 V1
`MAJOR.MINOR` string with EMITTED, SUPPORTED and RELEASED kept apart from the
start; `profile` is required and derived at write time; `assetRoot` is the
ascending array of digest OBJECTS; and §7.4's `entries` inventory exists and is
verified before anything is parsed for meaning. The snapshot frontier moved out
of the manifest into S2, which §7.5 requires and which issue 0004 needs for its
ED-0105 horizon.

**Interop evidence comes from the tracked corpus, not from our own writer.**
Both golden manifests parse and their inventories verify against their real
entries, and `digest-mismatch-portable` is refused as `checksum-error` naming
both digests. Before this round all 40 fixtures died on the missing manifest.

**AC1 is NOT claimed.** The snapshot BODY is still this version's projection
rather than §5.1's `entities` map with winner positions and S5, so the golden
fixtures stop at the snapshot parse. The deviation is recorded on
`SnapshotSection` and asserted by a test that fails if it silently starts
working. Renaming entries does not make a package conforming and this round
does not pretend otherwise.

**§8.4 U2 was being violated by the code that existed.** `Manifest`,
`Document`'s top level and `Batch` all dropped unknown fields silently; only
the nested entity types preserved theirs. Each of the three maps is separately
load-bearing, proven by disabling one at a time. `assets/descriptors.json` was
dropped entirely and is now carried verbatim, agreed with issue 0026 after that
pair withdrew a patch that would have put descriptors in the manifest.

**Two security findings, both from reading code rather than from a failing
test.**

*§9.4 name validation did not exist.* Nothing rejected an absolute path, a `..`
segment, a drive or UNC form, a backslash, a control character, an overlong
name or component, or a case-folding collision. Nine fixtures aimed at those
forms were refused incidentally by the inventory noticing an unlisted entry.
Validation now runs as §9.5's outermost stage and all nine report their exact
normative token.

*Row L6 could not fire.* The guard read `ZipArchive::len()` under a comment
claiming it ran "BEFORE enumeration". `zip::ZipArchive::new` parses every
central-directory header first, so the check sat after the work it exists to
prevent and could never fire on a lying header at all. Deleting it left the
whole container suite green, because the only L6 test asserted a predicate
rather than the reader path. The guard now reads the end-of-central-directory
record itself, before the archive is built, and the hostile input is a few
hundred bytes that merely CLAIMS 65,536 entries.

**The corpus suite was vacuous and now discriminates.** `hostile_corpus.rs`
survives every mutation tried against it, including two security guards deleted
together. `corpus_diagnostics.rs` parses EXPECTATIONS.md at runtime and asserts
the §9.4 class per fixture. Agreement with the oracle went from 6 of 38 to 16 of
38; the other 22 are pinned with the class this reader produces and one of three
named causes, and a pin that goes stale fails the test.

**Mutation results, recorded including the ones that prove nothing.**

| mutant | `hostile_corpus` | `corpus_diagnostics` | whole suite |
|---|---|---|---|
| name validation removed | survives | 2 red | 2 red |
| L6 pre-enumeration guard removed | survives | survives | 1 red |
| blob digest verification removed | survives | survives | 1 red |
| S0 regular-file check removed | — | — | 1 red |
| S2 candidate validation removed | — | — | 2 red |

The corpus cannot cover L6 or the blob digest: no fixture declares a count over
the limit, and the digest fixture is caught by the inventory first. Both are
covered by dedicated non-corpus tests instead, and the L6 post-parse backstop
is documented in the code as unreachable rather than counted as coverage.

**Fault injection now injects faults.** Three of the old tests did not: one
claimed in its comment to make a directory read-only and performed an ordinary
successful save, one attempted no save at all, and one carried a dead
disjunction. Real `EACCES` failures at S0 are injected through a read-only
parent, with the target asserted byte-identical afterwards. `ENOSPC` was
attempted and deferred rather than simulated — it needs a fillable filesystem
and `newfs_hfs` on a ram disk refused without privileges. Power loss and kernel
panic remain unclaimable.

**The conversion path exists.** `domowik convert-developer-package` writes a
new file, leaves the source byte-identical with its mtime unchanged, refuses an
existing destination, refuses `source == destination`, and refuses a package
that is already canonical so it cannot become an undocumented `cp`. It shares
the batch, blob and limit checks with the canonical reader. It is labelled an
unpublished-layout repair, never a migration: §8.2 V4 lets a step originate only
at a released schema and §8.2 V6 records that none exists.

Status stays **In Progress**. AC1 is open on the §5.1 snapshot body, AC2 is
open on everything beyond process-crash, AC3 has 22 pinned §9.4 divergences of
which 3 are absent §7.2 profile checks, and the upstream gates are unchanged.
The `OpenMode` editable/read-only-degraded seam routed here is NOT started.

## §9.6 read-only degraded open — 2026-09-12 (routed seam)

`87ffae2`. BEHAVIOR-SPEC §9.6 blocks editable open and normal saving when a
required asset cannot be resolved, and nothing implemented it: a plan
referencing a blob nobody could supply opened, edited and saved like any other.

**Enforced at the core, not in a front end.** `Plan::session` — the one route
every mutation takes — returns a `Result` and refuses while degraded, so an
edit gesture, a delete and an accepted external batch are all covered without
each caller remembering. `save`, `save_as` and `commit_and_save` are gated
separately, because a plan can be degraded without ever being mutated. The
`save_as` gate is not redundant: without it "read-only" would mean "read-only
to this path" and the degraded package would propagate under a new name.

**Inspection is deliberately open**, which is what makes the mode degraded
rather than closed. `document`, `session_ref` and `read_package` are ungated,
so §9.6's "pan, zoom, viewing, diagnostics, `info` and `validate`" keep
working, and hit testing is untouched — §9.6 says placeholders stay selectable
and deletable, so what stops is committing the delete, not choosing it. Issue
0059 has been told directly, because a marquee that reached for the mutable
accessor to read something would fail to select on a degraded plan.

**The split with issue 0026.** This crate owns the §9.6 MATRIX; the caller
answers AVAILABILITY through a predicate. Embedded blobs always count before
the predicate is consulted, so a portable package is self-sufficient with no
cache, and a thin package's cache answers through `open_with_resolution`.
Exactly one field is read out of the otherwise opaque descriptors —
`requirement` — because the matrix is keyed on it and the matrix gates saving.

**A decision recorded rather than taken silently.** §9.6's matrix has no row
for an asset whose requirement level is undeclared. This fails closed, in
`Requirement::Undeclared`, and changes nothing unless the blob is also
unresolvable. **Routed to issue 0005 as a §9.6 matrix gap**; issue 0026 has
been asked to object if it thinks permissive is right, rather than this pair
defending its own guess.

Mutation evidence: the session gate reddens 1 test, the save gate 2, and
reading `"optional"` as required reddens 1 — the last being the control,
without which a check that ignored `requirement` entirely would pass every
refusal test in the file.

One existing CLI test changed as a consequence, and the change is a finding
rather than an accommodation: `remove_level_rejects_a_level_still_carrying_a_
background_image` built a level referencing an absent blob with no descriptor,
which is now correctly a §9.6 degraded open. Its descriptor now declares the
asset `optional`, matching §9.6's Optional/Thin/missing row, so the test
measures level removal again instead of asset resolution.

Also confirmed while here, since the dispatch requires it: the CLI does not
reimplement the parser. `crates/domowik-cli` has no `zip` dependency and no
archive parsing; every path goes through `domowik-container`.

## Required adversarial review — UNAVAILABLE, 2026-09-12

INSTRUCTIONS requires an independent adversarial review by Codex
`gpt-5.6-sol` at high effort. **One narrow attempt was made and it failed:**

```text
$ codex exec --model gpt-5.6-sol -c model_reasoning_effort=high --sandbox read-only
ERROR: Your workspace is out of credits. Ask your workspace owner to refill
```

No retry loop followed, per the standing instruction that review credits are
unreliable. The prompt was scoped to one file's diff and five named claims,
and is preserved in this session's scratchpad.

**The claims were therefore tested rather than asserted**, which is what
INSTRUCTIONS requires when the reviewer is unavailable. The five:

1. *The hand-rolled ZIP trailer parser is panic-free on hostile input.* Now
   covered by execution rather than inspection: `trailer_parsing_is_panic_free`
   drives roughly 3,000 inputs — every prefix of a real package, single-byte
   mutations across the trailer region, forged Zip64 locator offsets up to
   `u64::MAX`, planted signatures in fixed-seed noise, and every length from 0
   to 39 — through the public entry point with `catch_unwind`. None panics.
2. *§9.4 name rejection is complete.* It is NOT, and the gaps are named rather
   than claimed closed: Unicode-normalization collisions (the fold is ASCII,
   documented on `logical_name_key`) and symlink/special-file designations,
   which live in the external-attributes field and are not visible to a name
   check at all. Both are §7.2 profile checks this reader does not implement.
3. *The §7.4 inventory cannot be evaded.* Audited by reading: presence is
   checked over every archive entry and absence over every listed name, in
   both directions, and duplicate names are already refused upstream at name
   validation, so `by_name` cannot be made ambiguous.
4. *Every mutation and save is gated.* Audited and mutation-tested. One
   boundary worth stating plainly: `read_package` and `write_package` are the
   LOW-LEVEL API and are not gated, by design — §9.6 governs the editing
   session, and a tool that reads a degraded package and writes a copy is not
   editing it. `Plan` is the gated surface.
5. *The converter never touches the source and never overwrites.* Audited and
   tested, including the `source == destination` case, a textually different
   path naming the same file (caught by the existence check), and a
   destination that is a dangling symlink (caught by S0).

Finding 2 is the one a reviewer would most likely have pressed on, and it is
recorded as an open gap rather than resolved.

## §7.2 profile rules and rule T1 — 2026-09-12

`0d212af`. Four container-profile rules that FORMAT-SPEC specifies and this
reader did not implement: the encryption refusal, the §7.7 compression
allowlist, the empty-archive-comment rule, and §9.4's fail-closed rule T1.
Every fixture built to provoke one of them was previously refused for an
unrelated reason, so none had coverage.

**T1 needed two checks, not one.** Bytes after the end-of-central-directory
record are the obvious half. The half that matters is the other one: appending
a whole second package leaves the LAST trailer exactly at EOF, so nothing
trails it and the first package becomes leading data instead. A reader scanning
backwards sees the second package and one scanning forwards sees the first,
which is the ambiguity T1 exists for. It is detected by comparing the
directory's own offset and size against the record's absolute position — they
agree only when nothing precedes the archive.

The offset comparison fires only when the sum falls SHORT of the record. An
earlier version fired in both directions and reported `truncated-header.domo`
as trailing data, which named the wrong fault confidently; the other direction
is a truncated archive and belongs to the ZIP layer's diagnostic.

Corpus agreement: 16 of 38 to **20 of 38**, no fixture regressing. Mutation
evidence: removing the entry-profile check reddens 2 tests, removing either T1
half reddens 3.

**A corpus defect, recorded rather than worked around.**
`hostile/container/nonzero-comment.domo` does not contain a non-empty comment.
Byte inspection: its single EOCD declares `commentLength = 0`, its record ends
exactly at EOF, and no comment bytes exist. The §7.2 comment rule is
implemented and correct and that fixture cannot reach it, so **the rule has no
coverage** — pinned as `corpus-defect` and routed to issues 0028/0062. This is
the L12-on-the-boundary shape the Go audit found: a fixture that looks like
coverage and is not.

**Still absent from §7.2**, and pinned as such: the local-header/directory
structural scan (overlapping regions), the data-descriptor and
language-encoding flag checks, `versionNeeded`, foreign extra fields, and
symlink/special-file designations — which live in the external-attributes
field, where a name check cannot see them.

### Amendment — corpus agreement is 21 of 38, not 20

`ca16aaa`. A CRC-32 mismatch or a corrupt deflate stream was surfacing as
`io error on <path>`, which tells a user their disk is failing when their file
is corrupt. Classified on `io::ErrorKind::InvalidData` — what the `zip` crate
raises for both — rather than on the message text, so it survives the
dependency rewording itself.

The stale-pin mechanism then proved itself without being asked: the corpus test
failed with "`checksum-error.domo` now produces the normative `checksum-error`,
so the `earlier-guard` pin is stale and must be REMOVED rather than left to
rot". That is the behaviour the pin table was built for, observed rather than
asserted. The pin is gone and the fixture is asserted directly.

## Merged main and integrated issue 0004's work — 2026-09-12

`e3cf983`, merging main `99e7384` (47 commits) into `finalize/0029`. The merge
was textually clean and semantically was not; all three integration points were
found by something failing rather than by reading a diff.

**Row L11's depth guard had fallen off the tested path.** It still runs, but
its two package-level tests build archives by hand, and the §7.4 inventory pass
now precedes any parse — so both were rejecting at the integrity layer and no
longer reaching the guard. Routed through the inventory-correct helper, they
exercise it again. This is the ED-0072 shape and it would have been invisible:
the tests were green.

**The §7.5 section costs one level of the L11 budget**, caught by
`write_package` refusing a document at depth 128. L11 counts from the ENTRY
root, and since §7.5 the entry's root is the section with the projection as a
member, so a document reaching depth `d` reaches `d + 1` inside the entry that
carries it. The limit is untouched — L11 is ratified at 128 and depth 128 still
parses. What changed is how much of that budget document content can use. The
overhead is now asserted by its own test and the boundary derived from it,
rather than both being assumed. **Routed to issue 0004**, because a CONFORMING
package has the same property for the same reason — §5.1 makes the projected
state a member of the snapshot, not the snapshot itself — so it is not an
artefact of this version's body deviation and will not go away when the real
body lands. If ED-0105 intends the budget to apply to document content rather
than to the entry, that is a wording change in §9.3, not a code change here.

**Row L23's snapshot-metadata accounting is now WIRED.** Issue 0004 specified
and tested it ahead of the members it counts and left it deliberately
unenforced while `Package.snapshot` was a bare `Document` with nothing to
score. §7.5 landed those members, so `read_package_with_limits` enforces it —
on the decompressed bytes, before the section becomes owned structures, since
measuring afterwards would allocate exactly what the limit refuses.

S1 is excluded under BOTH of its current names: §5.1 calls it `entities` and
this version serialises it as `document`. Counting either turns a metadata cap
into a maximum document size. The list collapses to `entities` alone when the
§5.1 body lands. Mutation: removing the guard reddens one test, removing the
`document` exclusion reddens another — the second being the one that matters,
since without it the guard passes just as happily and the first large plan
anyone saves stops opening.

**A coordination note worth recording.** `herdr agent send` silently truncates
past roughly 1,700 characters, which cost several messages this round — the
snapshot-body ownership question to issue 0004 and the corpus-defect routing to
issue 0062 were both cut without any error. Long handoffs need splitting or a
file path.
