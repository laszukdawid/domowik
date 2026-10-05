# Issue 0011 — causal position implementation: mutation-proof evidence

Status: **evidence record for the issue 0011 implementation handoff.** It
records what was deliberately broken to show each check could fail, not what
passed. A green run is not evidence on its own — ED-0041 exists because this
project counted a check that could not fail as coverage five separate times,
and one of those instances was this issue's.

Toolchain, per the orchestrator's binding correction of 2026-08-29:

```sh
export PATH="/Users/dawidlaszuk/.local/share/mise/installs/go/1.27.0/bin:$PATH"
GOTOOLCHAIN=local go version   # go version go1.27.0 darwin/arm64
```

`/opt/homebrew/bin/go` is **1.26.5** and only *reports* 1.27.0 because the
default `GOTOOLCHAIN=auto` silently downloads a newer toolchain; `verify.sh`
sets `GOTOOLCHAIN=local`, which forbids that switch. Every command below ran
with the mise 1.27.0 binary first on `PATH`.

## Method

Each proof applies one deliberate defect to committed source, runs the named
checks, **requires them to fail**, restores the file, and requires them to
pass again. A mutation that leaves its checks green is reported as proving
nothing and is a failure of the proof, not a pass.

Forty-five mutations were run across four waves. **Forty-five went red. None
proved nothing** — though three of them proved nothing on their FIRST witness,
and all three failures are recorded below rather than quietly replaced. The
harness and the full transcript are session artifacts; the tables below state
each defect, the check that caught it, and what the failure showed.

**The second wave exists because the first submission was wrong.** r0011
found eleven defects against the committed implementation and the
orchestrator found a twelfth in its own ruling. The single most important
fact in this document is that **`./scripts/verify.sh` exited 0 on this branch,
with a clean tree, while all eleven were present** — the orchestrator ran it
independently and got the same result. Nothing about that run was wrong. The
inference "green, therefore correct" was, and both the pair and the
orchestrator made it.

## The write position and canonical order

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M1 | `causalPastSize` dropped from the comparator, reverting to the pre-ED-0037 key | `TestCausalityGovernsOverTheCounterTieBreak`, `TestV5LateConcurrentResolvesIdenticallyAfterSnapshotSeed` | The ED-0032 defect returns exactly: a causally later write with a smaller counter loses its value, on the live path and the snapshot-seeded path alike |
| M2 | `causalPastSize` truncated to one 64-bit word | `TestCausalPastIsExact` | `3×(2^63−1)` compared as *less* than `2×(2^63−1)`; row L18 admits 65,536 members, four orders of magnitude past the three it takes to break a single word |
| M3 | Canonical order reverted to the ready-set `(counterBase, actorId)` walk | `TestReadyKeyOrderDiffersFromCanonicalOrder` | A write causally after its target's create is discarded STALE, differing on the field value *and* on the presence of a stale diagnostic |
| M11 | Write position built from the operation COUNTER instead of `(counterBase, operationIndex)` | `TestWritePositionUsesTheOperationIndexNotItsCounter` | Equal-past writes at base 10 index 1 and base 11 index 0 carry the same counter, so the wrong rule falls through to `actorId` and picks the opposite winner — ED-0037 comment (b)'s discriminator |

**M3 is recorded with its own first failure.** The first witness for it —
concurrent batches whose counter bases ran opposite to their causal past sizes
— passed under *both* orders and proved nothing. I5 classifies a target
against retained POSITIONS rather than against whatever is live when a batch
is reached, which makes most outcomes order-independent by construction. The
ordering is load-bearing only where a target's creation position is not staged
at all. The witness was rebuilt on that shape. This is the same failure mode
ED-0041 names: a check that looks correct and exercises nothing.

## The three-branch lifecycle (§4.5 I5)

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M4 | I5 collapsed to two branches (present-or-absent *now*) | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget`, `TestLateCreateAgainstARetainedTombstone` | The late batch is discarded whole, losing its writes to targets that were never deleted |
| M5 | Creation position dropped for tombstoned ids | `TestV4DeleteDominatesSet`, `TestSnapshotBatchTableIsExactlyTheReferencedSet` | The live interval cannot be formed, so I5 branches 1 and 2 collapse — §5.1 S4's stated consequence |
| M6 | Suppressed write materialised on a tombstoned target | `TestLateBatchInsideALiveIntervalDoesNotResurrectItsTarget` | ED-0037 comment (n)'s exact case: a field resurrected on a tombstoned entity, which that comment records as having FALSIFIED a fix already verified at zero divergences over 40,342 splits |
| M17 | Blanket "tombstoned therefore stale" shortcut restored for `createEntity` | `TestLateCreateAgainstARetainedTombstone` | A pre-delete concurrent create that wins LWW4 leaves the retained creation position stale, so the seeded state disagrees with full replay on the creation position *and* on every field winner |

## Frontier closure (§4.4 F4) and the B5 partial-range rule

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M9 | F4 dominance sweep skipped entirely | `TestF4RejectsOmittedAncestorEntryBehindACompactedTerminal` | An under-reported frontier is certified, which is the unsoundness ED-0037's soundness comment identified |
| M12 | Own-actor prior end omission accepted | `TestF4RequiresTheWritingActorsOwnPriorEnd` | Both the omitted and the *wrong* own prior end pass — ED-0037 comment (e)'s correction of the "benign" reading |
| M13 | Closure compares only the TERMINAL observed batch | `TestF4RejectsOmittedAncestorEntryBehindACompactedTerminal` | With the terminal batch compacted and an earlier ancestor retained, the omission becomes invisible; this is the shape r0011 supplied against my first version of the check |
| M18 | B5 partial-range observation check removed | `TestF4RejectsFrontierPointingInsideAnAtomicBatch` | A frontier naming counter 1 of a batch reserving [1,2] is accepted, claiming half of an atomic batch |

**M18 is r0011's blocker against the committed implementation, and the way it
was missed matters.** The dominance sweep folds only ancestors whose END lies
at or below the threshold, so a batch whose range STRADDLES the threshold was
skipped entirely and the observation silently certified. The naive oracle
written to check the optimization **shared the blind spot exactly**, so the
differential test agreed — two copies of one mistake agree. The fix is one
function both call, and the differential test alone would not have caught it:
only the hand-built witness did. That is §4.7's own evidence discipline —
"a hand-built adversarial witness is what DISCRIMINATES a correct rule from a
plausible one, while a large random aggregate is soundness evidence for the
rule actually adopted".

## The frameAndOrder reordering (ED-0046)

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M21 | Retained-log overlap check moved back behind watermark suppression | `TestRetainedRangeOverlapBelowTheWatermarkRejects` | Two retained batches reserving the same counter are accepted, because watermark classification had already removed both from the set the structural check sees |

**Which of issue 0029's symptoms this resolves, verified rather than
assumed** (ED-0046 item 3). The recorded issue-0005 witness — snapshot
watermark 100 with retained ranges `[70,71]` and `[71,71]` overlapping at 71,
both wholly at or below that watermark — now REJECTS at `Load`, where it
previously passed while `Inspect` rejected it.

Stated precisely, because the first version of this proof over-credited the
reordering: that particular witness is caught by **two** independent rules
now. The overlap check catches it, and so does F4's partial-range rule,
because the second batch's frontier `{x:70}` points inside the first batch's
range `[70,71]`. Removing the overlap check alone therefore left the witness
still rejecting, and the mutation proved nothing until a second witness was
built — ranges `[70,70]` and `[70,71]` sharing a counterBase, with both
frontiers reporting 69, which lies inside neither range. F4 sees nothing
there, so only the overlap check can reject it. Both are kept.

**What this does NOT resolve**: `internal/persist/reader.go:468`'s discarded
`checkReservedRanges` verdict remains issue 0029's, open and unfixed. That
function also treats GAPS as rejects, which post-compaction is a separate
question this issue deliberately did not take on: compaction drops batches, so
a gap in the retained set is not by itself a B3 violation, and the
implied-prefix machinery already models the dropped counters. Resolving a
symptom is not closing ED-0032.

## Snapshot encoding (§5.1 S3/S4) and ED-0038's legacy convention

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M7 | Mixed legacy/position snapshot accepted | `TestSnapshotEncodingsAreExclusive` | Ambiguous required data is converted partly, which §5.1 forbids in terms |
| M8 | Two-actor legacy snapshot admitted under the convention | `TestLegacySnapshotReadingConvention` | Winners that cannot be placed are placed anyway, inventing cross-actor causal relationships that never existed |
| M14 | Dangling and unreferenced `batchPositions` entries accepted | `TestSnapshotBatchTableIsExactlyTheReferencedSet` | Both directions of "EXACTLY the batches those maps reference" stop being enforced |
| M15 | Encoding selected by entry COUNT instead of member presence | `TestSnapshotEncodingsAreExclusive` | `"winnerPositions": {}` beside `"createdKeys": {}` reads as carrying neither family — **ED-0041 instance three, which this issue produced in its previous round**, reproduced with the collection changed and the mistake identical |
| M16 | `operationIndex` forced to zero on write | `TestSnapshotRoundTripsANonZeroOperationIndex` | A zero-only round trip would have passed with the index dropped entirely |

## V2's discharge (ED-0045)

ED-0045 restates ED-0036's replacement condition as an observable: **V2 must be
demonstrably executed and demonstrably able to fail.** Both halves are recorded
here because the first two formulations of that condition specified machinery
nobody had checked existed.

**First half — the marker fired on its own.** The moment the write position
landed, the harness's own guard, written by issue 0028 under ED-0033 to make
removal a conscious act, failed the run:

```
--- FAIL: TestProjectionVectors/adversarial/vectors/v2-reordered-delivery/blocked-implementation-nonconformance
    corpus_semantic_test.go:435: finalThicknessMm now MATCHES its oracle (150):
    the issue-0011 conformance defect is resolved, so remove the executionBlocked
    marker from EXPECTATIONS.md consciously
```

The `executionBlocked` marker and the `residualBlockers` entry were removed
only after that — when they had become untrue, not before.

**Second half — M10, the re-enabled check going red.** With the marker gone
and the oracle asserted unconditionally, the causal-position winner comparison
was broken deliberately:

```
=== M10 causal-position winner comparison broken AFTER the V2 marker was removed:
    check went RED as required
--- FAIL: TestProjectionVectors
    --- FAIL: TestProjectionVectors/adversarial/vectors/v2-reordered-delivery
        --- FAIL: .../final-projection-equals-in-order-delivery
    (restored; checks green again)
```

V2 is therefore executed evidence, and its check has been observed to fail.

## The ED-0050 regeneration-gate scoping

ED-0050 required the exemption itself to be mutation-proven, because this
issue scoped a gate that was blocking its own branch and "scoping is otherwise
indistinguishable from disabling".

| # | Deliberate defect | Check that failed | What the failure showed |
|---|---|---|---|
| M19 | The enumerated exemption emptied | `TestCommittedArtifactsConsistent` | The comparison is live: with the entry removed, the real committed artifact fails byte-equality immediately |
| M20 | The exemption widened from a list to an always-true predicate | `TestScopedRegenerationGateStillFails` | A non-exempt artifact whose bytes differ stops being reported, *and* one whose bytes match starts being reported — the gate disabled rather than scoped |

`TestScopedRegenerationGateStillFails` pins the boundary in four directions:
differing bytes off the list must fire; matching bytes off the list must not;
a listed artifact that starts matching must fire, so the exemption cannot
outlive its reason; and a listed artifact whose bytes differ must be reported
as SCOPED rather than skipped silently.

What is **not** scoped: T8a still checks every committed artifact's digest and
byte count against its budget record unconditionally, and
`TestCommittedPackagesRoundTrip` still passes — both ED-0031-protected
packages LOAD and round-trip under ED-0038's convention. No committed
`docs/evidence/` bytes were changed.

## The eleven-defect wave (ED-0054)

Found by r0011 against the committed implementation, after the first
submission. All eleven are now tracked regression tests —
`internal/document/integrity_test.go` and `internal/persist/integrity_test.go`
— each shown red before green.

| # | Defect found | Mutation that re-arms it | What it was |
|---|---|---|---|
| 1 | A rejected call rolled its effects back but left its batch ids REGISTERED, so a corrected resend skipped silently as a duplicate forever | M22 | The projection was already whole-call atomic; only the applied-identity set leaked |
| 2 | A rejected batch leaked its ref and frontier into the position table, so a corrected resend of the same reserved range collided with the dead batch's frontier | M23 | Framing ordered against the live table; it now orders against a clone |
| 3 | A create colliding with a live entity whose creation position was compacted **PANICKED** | M24 | The failure mode of the point-7 relaxation: absence must degrade, never crash. It now fails closed, and a reader that panics on a shape the spec permits is worse than the rejection it replaced |
| 4 | A causally ordered cross-actor duplicate create was resolved as a LWW4 collision | M25 | Non-conformance against §4.5 as narrowed by `3cca9ee` four days earlier: LWW4's scope is "neither create observing the other" |
| 5 | `FromSnapshot` accepted a batch position observing its own reserved range, a position above the S2 watermark, and a `counterBase + operationIndex` that overflows L13 | M26 | Three shapes the type system permits and the specification does not |
| 6 | An inverted or empty live interval `[creation, deletion)` was accepted | M27 | It would make I5 classify every late batch as branch 1 or 3 and silently discard writes full replay applies |
| 7 | `ReplayFromSnapshot` skipped a watermark-covered retained batch without authenticating its frontier against `batchPositions` | M28 | The suppression this issue moved structural checks ahead of, reappearing one layer down |
| 8 | A retained one-operation batch accepted a position referencing operation index 1 | M28 | A position naming an operation that does not exist |
| 9 | The acceptance parser treated every required S1–S5 member as OPTIONAL: `{}` parsed as a silently empty projection | M29 | **ED-0041 instance three recurring** — the empty-map vacuity, in new code written after that record existed. The remedy is a required-member assertion the parser structurally cannot pass without, with the complete-empty shape as the control that makes the nine rejection cases discriminating |
| 10 | `Inspect` hid a present legacy family behind an empty new-family member, and hid a `deletingKey` co-present with a `deletingPosition` | M30 | Structural misreporting, not reader acceptance: `Load` rejects mixed families correctly |
| 11 | The corpus generator's `emptyWriterSnapshot` still emitted `winnerKeys`/`createdKeys` and no `batchPositions` while its comment claimed to match `buildSnapshotJSON` | M31 | This issue's widening is what made the claim false. Fixed BEFORE the ED-0050 guardrail was rebuilt, because a generator that claims to match the writer and does not makes every regeneration proof non-discriminating by construction |

**The replay-stage atomicity fact, established rather than argued.** Both
r0011 and the orchestrator identified that the session-fence rewrite turns on
one checkable fact, and that only half of it was confirmable by inspection: a
FRAMING rejection is provably pre-commit, because `Deliver` returns
`frameAndOrder`'s error before `replayAndCommit` runs — but that says nothing
about a batch failing during REPLAY, which is the path the fence case's
out-of-range value actually takes. If replay could commit some batches before
failing, partial acceptance would be reachable and the old assertion was
testing something real.

`TestReplayStageFailureIsAllOrNothing` settles it directly. It delivers a good
batch and one that passes framing and fails during replay, asserts the
rejection is a `BatchRejectedError` and NOT `ErrMalformedFraming`, confirms the
invalid batch is structurally well-framed on its own so "it failed during
replay" is a fact rather than an inference, and then requires the projection,
the delivered log and the applied-identity set to be exactly as before — and
the good batch to be redeliverable. M32 breaks the rollback and the check goes
red. So the earlier assertion was pinned to the leak, not to a property, and
the rewrite removes no coverage. The positive half of the fence case asserts
the transition in both directions, because "nothing accepts nothing" is
satisfied by every broken fence too.

**Two contract corrections fell out of defect 1.** The `BatchRejectedError`
doc comment claimed batches committed earlier in the same call remain
committed; `replayAndCommit` never did that, so the comment was wrong before
this issue touched it. And the session's external-delivery fence test asserted
that a partially-rejected call still fences — it passed only because
`DeliverExternal` detected "acceptance" by reading the leaked batch ids.
Fixing the leak removed the phantom acceptance that assertion was reading, so
the test now states the real contract: a wholly rejected external call accepts
nothing, and a separate case proves the fence still fires on real acceptance.

## The ED-0050 guardrail, rebuilt

The first guardrail drove the gate with **arbitrary byte strings**. Arbitrary
bytes differ from a committed artifact for reasons that have nothing to do
with the exemption, so that transcript proved the comparison rejects garbage —
not that it notices a real artifact failing to regenerate. It did not satisfy
ED-0050, and the orchestrator recorded that it had read the summary claiming
four directions without opening the witness.

It now drives the same comparison over artifacts the CURRENT writer actually
produces, so the only variable is whether the path is on the enumerated
exemption list. It skips with a named reason if generation is unavailable, and
never converts that into acceptance.

## The fourth wave (ED-0055/ED-0056): three blockers

r0011's frozen-range review found three more blockers against the branch at
`8185691`. A fourth was withdrawn — the ED-0050 guardrail was already correct;
the reviewer searched for a symbol named `Generate` and missed the actual
helper, and the guardrail was left untouched.

### The rollback invariant, not three field fixes (ED-0056)

`p.restorable` leaked across a rollback: `deliverRestoring` stores
authorization before replay, and the restore did not put it back. The
reproducer resurrects a tombstoned entity — a private restoring batch fails,
and an ordinary PUBLIC delivery reusing that uncommitted `batchId` consumes
the leaked authorization.

With the applied-identity set and the position table already recorded, that is
**three leaks of one kind**, so ED-0056 required an invariant rather than three
fixes. What landed is one capture/restore pair at the transaction boundary —
`Deliver` and `deliverRestoring`, not inside `replayAndCommit`, because a
boundary drawn any later captures the leak as part of the "before" state —
plus `TestRollbackEnumeratesEveryProjectionField`, which asserts the exact set
of `Projection` field NAMES against a declared classification. A thirteenth
field fails until someone classifies it, which is the difference between an
enumeration and a memory.

**Two things the work itself taught, both worth more than the fix.** First,
capturing `positions` by POINTER restored nothing, because `note` mutates that
map in place — a rollback that looks complete and does nothing, and exactly
the class ED-0056 names. The existing leak test caught it immediately. Second,
an earlier scratch-clone in `frameAndOrder` was removed: it worked, but it left
two mechanisms for one property and a reader could not tell which was
load-bearing.

`TestReplayStageFailureIsAllOrNothing` now compares the full mutation set. The
orchestrator's own correction applies here and is the sharper lesson: it read
that test's enumerated claim as a general one, and the enumeration was chosen
by asking which state mattered rather than by reading the struct — which is
the method ED-0056 now forbids. **Any assertion of the form "X is unchanged"
has a hidden enumeration, and the enumeration is the claim.**

### Retained positions must authenticate the operation they name

`checkIndexWithin` validated only `OpIndex < len(Operations)`. That says the
operation EXISTS and nothing about whether it is the one the key claims, so
`createdPositions[E1]` could name a batch whose operation at that index
creates E2 — a snapshot asserting one entity's lifecycle from another
entity's write. Each retained position now binds to its §5.1 operation:
creation to a `createEntity` for that id, tombstone to a `deleteEntity` for
that id, winner to an operation writing that entity's field. Only where the
batch is RETAINED; a compacted batch has no payload to authenticate against.

### Legacy tombstone recovery chose the first create

`creationPositionFromRetained` returned the first syntactic create for the id.
In a create/delete/restore/delete history the final tombstone closes the
RESTORED interval, so the first match is a **different interval**, not a
conservative approximation of the right one — and I5 would then place every
late batch between the two creates on the wrong side of branch 1. It now
selects the greatest creation position strictly below the deleting position,
ordered through F3's arithmetic rather than a counter heuristic, and fails
closed when none qualifies. The sibling defect is fixed with it: the
`CreatedKeys` pass overwrote the position derived from RETAINED bytes, making
loop order the arbiter between evidence and convention; a disagreement now
fails closed.

## A self-sweep for the shape, not the instance

The eleven-defect wave surfaced one pattern twice in this branch: an
assertion accurate about what it checked and silent about whether the check
could distinguish anything. ED-0041 records that recognising a shape does not
stop it reproducing, and that the remedy is to sweep for the ASSUMPTION rather
than the reported line. So the new tests were swept for the same shape rather
than waiting for it to be reported a third time. Three instances were found in
this issue's own work:

- **Two vacuous loops.** `TestLegacySnapshotReadingConvention`'s counter-1 case
  and the `State()` deep-copy case both scanned a map for a forbidden value.
  Over an EMPTY map the loop runs zero times and the assertion passes without
  examining anything. Both now require the collection to be populated first.
- **A differential test with a generator-shaped blind spot.**
  `TestClosureCheckerAgreesWithItsOracle` catches a sweep that folds NOTHING
  (M37b) but not an off-by-one that folds every ancestor except the LAST (M37),
  because its generator rarely places the deciding ancestor there for a given
  threshold. That is the aggregate-versus-witness distinction ED-0037 comment
  (n) records, met head-on. `TestClosureSweepFoldsItsLastObservedAncestor` is
  the hand-built witness that discriminates, and M37 now goes red.

Five further mutations were added to cover checks that had none: M33, M34 and
M35 remove the `counterBase`, `actorId` and `operationIndex` components of the
comparator one at a time, and each fails exactly one case of the tie-break
table — which is why that table isolates the three rather than blending them
into one vector. M36 removes the operation-index bound.

## Empty-collection probes

Every per-entry rule added here is exercised against an EMPTY collection and
required to produce the empty projection rather than an accidental pass:
`TestSnapshotBatchTableIsExactlyTheReferencedSet/empty maps are not vacuously
satisfying`, and M15 above. This issue produced ED-0041's third instance — a
detection rule defined through a per-entry test that an empty map satisfied
vacuously — so the probe is not a formality here.

## What this evidence does NOT claim

- It says nothing about the issue 0028 checker repairs ED-0043 excludes.
  `checkV5` still never reads its projection, `checkWinner` still ignores the
  `actorId` and `loserValueDiscarded` it is handed, and V4 still ignores
  `concurrentWriteApplied`. Those functions were touched only where the
  widened member names forced a mechanical rename, and that rename is not a
  review.
- The differential test between `validateFrontierClosure` and its naive oracle
  is soundness evidence for the rule adopted, never proof that a rejected
  alternative fails. M18 is the demonstration: both agreed while both were
  wrong.
- No aggregate count here is offered as coverage. Per ED-0037 comment (n),
  large random aggregates are soundness evidence for the rule actually
  adopted, and a hand-built adversarial witness is the discriminating
  instrument.
