# 0011: Implement operation replay, commands, and undo/redo

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0003, 0004, 0010

Activated 2026-08-24 under ED-0018 on branch `issue/0011-oplog-undo`.
Consumes FORMAT-SPEC §2–§5 normative semantics and conformance vectors, the
Done undo behavior specification, and issue 0010's delivered material as a
labeled provisional handoff (including the command-layer invariants 0010
deferred). Numeric values only via named provisional parameters; no Done while
0004 or 0010 remain unclosed under their recorded gates, except by explicit
re-scope.

## Partial handoff accepted (2026-08-24/25)

Branch `issue/0011-oplog-undo` (8 commits, `8a9bace`..`12a1f96`) merged to
main by `3451ba4` with SBOM refresh; full main `./scripts/verify.sh` exits 0.
Deliverables: batch validation with canonical-position stale-discard over the
total delivery set; idempotent replay from snapshots with watermark
classification; command session with local undo/redo (compensating batches
with fresh identities, redo restoring same entity IDs, external-batch fence,
finite stacks, reopen clearing); V1–V7 vector coverage including a new
V7-separate-calls reconciliation fix (`3c7cfe7`) plus regression tests;
deterministic property streams. Adversarial reviewer verdict: APPROVE after
resolved findings (same-call LWW/V7 arrival divergence, effective-level
deletion gates); orchestrator review confirmed parameter discipline and no
public mutation surface. Open format question routed to the format owner
(issue 0004): `State.CreatedKeys` is required by LWW4/V5 snapshot-seeded
reconciliation but is not enumerated in FORMAT-SPEC §5.1 S1–S5 — extend S3/S5
wording or specify an alternative before ratification. Per ED-0018 this issue
remains `In Progress` until 0004/0010 close under their gates or an explicit
re-scope records otherwise; dependents consume this delivered material as a
labeled provisional handoff under partial-scope semantics.

## Position implementation in progress; FROZEN at `dd17124` (state at 2026-08-30 shutdown)

**This work is NOT integrated.** Branch
`issue/0011-causal-position-implementation`, range `d25c141..dd17124`, sixteen
commits, clean tree, `./scripts/verify.sh` exit 0 verified independently by the
orchestrator. The branch is frozen and its worker idle by instruction; a
post-fix review was outstanding when the session ended.

Delivered on the branch: ED-0037's causal write position end to end — ordering,
conflict resolution, the three-branch lifecycle, F4 closure validation, the
widened snapshot encoding, ED-0038's legacy read, and the ED-0043
re-derivations. The ED-0032 defect is fixed and V2 holds at 150. Forty-five
mutation proofs across four waves, each with a stated alternate cause per
ED-0053.

**Review history on this range, because it explains the SHA:** an
eleven-defect wave was found and fixed, then a further round found four
blockers. Three were verified by the orchestrator directly against the frozen
tree and fixed — the missing semantic binding in `authenticateAgainstBatches`,
ED-0038 tombstone recovery returning the FIRST retained create, and the
`p.restorable` capability leak across rollback. **The fourth was WITHDRAWN as
stale**: it described a guardrail defect that was real when ED-0054 recorded it
and had been fixed before the freeze. Building the rollback fix surfaced
ED-0056's own shape inside it — capturing the position table BY POINTER
restored nothing, because that map is mutated in place.

**Post-fix verdict, 2026-08-30: BLOCKED, three NEW blockers, review
explicitly INCOMPLETE.** Four of five delegated audit areas returned no result
(subagent resource exhaustion), and the reviewer disclosed that rather than
presenting uniform confidence. Found: (1) an index-out-of-range reachable from
a `{actor: 0}` frontier entry the wire parser accepts — `wire.go:507-529`
stores it, `snapshot.go:99-133` keeps it, and `projection.go:509-513`
unconditionally indexes `rs[len(rs)-1]`; (2) call-granularity dependence
violating I2's silent-ignore rule, where the same batches in different
groupings yield different verdicts; and (3) the closure differential oracle is
STRUCTURALLY unable to validate either implementation, because the optimized
and naive validators call the same own-prior and partial-range helpers — the
ED-0053 amendment (a) shared-blind-spot case, now shown to be structural
rather than incidental. The closure-oracle audit that DID complete confirms
linear extension holds for fully retained well-formed histories and that fully
retained cycles cannot be accepted; it also confirms the stronger claim is
false by the documented compaction limit, which FORMAT-SPEC already admits as
unprovable. Full verdict preserved at
`~/projects/domowik-observatory/session-2026-08-29/verdicts/0011-r-postfix-verdict.md`.

**What the next reviewer owes**, and it is recorded because a fresh reviewer
inherits context and never agreement: the three fixes, AND the five leads the
previous verdict explicitly listed as unverified together with its
non-exhaustive position spot-checks.

**Criteria that may NOT be marked met** (ED-0055): duplicate/idempotent replay,
because S5's `recentBatchIds` are parsed then discarded; and snapshot replay,
because `FromSnapshot` accepts dangling authoritative references. Both are
issue 0011's but NOT this continuation's.

Open gates this branch inherited or opened: ED-0045's audit dimension to issue
0028, ED-0049's §5.1 S4 member to issues 0004/0005, ED-0050's regeneration
exemption dying with ED-0040 queue item 6, and ED-0055's three routings.

Session material a fresh orchestrator needs — reviewer verdicts, preserved
probe files, the pair contract, and the send/monitor/probe helpers — is at
`~/projects/domowik-observatory/session-2026-08-29/`, with `RECOVERY-STATE.md`
as its index. It is outside the repository because it is operational, and
outside `/tmp` because that does not survive a reboot.

## Problem

Implement operation/batch validation, idempotent replay, command grouping, and
local undo/redo. Keep undo state separate from durable retention and restore an
equivalent projection rather than identical historical metadata.

## Acceptance criteria

- [ ] Duplicate and out-of-order batches behave according to conformance
      vectors; atomic batches fully apply or do not apply.
- [ ] No public API mutates projected entities directly.
- [ ] Undo/redo property tests cover random command streams, compound edits,
      failed commands, cancellation, and redo invalidation.
- [ ] Replay from a snapshot/frontier produces the expected projection for all
      permitted delivery orders.
- [ ] Session persistence and compaction interaction match behavior/spec rules.

## Conformance defect routed here 2026-08-26 (ED-0032)

Issue 0028's reproduction pass found that the delivered projection applies the
LWW1 winner-key comparison to causally ordered writes, not only to concurrent
ones. `internal/document/projection.go:721` (`winnerSet.dominates`) compares K
unconditionally, so a causally-later write with a smaller K loses its value.

FORMAT-SPEC §4 is normative for v0.1: §4.4 F3 makes canonical order a linear
extension of causality with ties broken by K; LWW1 scopes K to "the winning
write among concurrent candidates"; and §4.7 V2 requires the final projection
to equal in-order delivery exactly. A causally-later write is not a tie, so
the delivered behavior is non-conforming.

`internal/document/projection_test.go:245`
(`TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`) pins the defect and cites
vector V3, whose precondition is "neither observes the other"; its own
`smallWrite` carries frontier `{main:1, y:1}`, so X observed Y. This issue
owns both the fix and the retirement or rewrite of that test.

Blocks issue 0028's `v2-reordered-delivery` vector and issue 0018's
release-audit row O. Issue 0004 is asked separately to tighten LWW1's fourth
sentence, which reads loosely in isolation.

## Continuation activated 2026-08-26 (ED-0035)

Branch `issue/0011-lww-causal-ordering`. Fix the conformance defect routed
here by ED-0032 and prove V2/V3/V7 against the normative §4.7 vectors. The
normative outcome is settled by ED-0032 and is not reopened: causality
governs, and LWW1's key comparison breaks ties among concurrent writes only.

Issue 0028's `v2-reordered-delivery` witness is written to FAIL loudly once
this lands. Removing that marker and its `executionBlocked` entry in
`testdata/corpus/EXPECTATIONS.md`, so the coverage audit counts V2 as real
coverage, is part of this work. This issue stays `In Progress` under ED-0018.

## Continuation handoff accepted 2026-08-27 (ED-0031 halt, ED-0040 queue)

Branch `issue/0011-lww-causal-ordering` (`3e841d5`..`9c6c64c`, fifteen commits)
merged to main by `fe18097` with SBOM refresh `4706192`. Full main
`./scripts/verify.sh` **exits 0** with a clean tree.

**The semantic fix stayed halted throughout.** ED-0036 halted this issue's
implementation under ED-0004, and `git diff --name-only -- '*.go'` is empty
across all eight amendments — no product code, no test, no FORMAT-SPEC byte.
What landed is the repurposed deliverable: `docs/evidence/0011-causal-position-compatibility.md`,
which documents rather than recommends.

It establishes the derivability-versus-provability separation that kept the
ED-0037 ruling from returning to the owner — `causalPastSize` is computable
from any retained batch entry, while frontier closure is not provable after
compaction; the measured retention behaviour, corrected to "no prior covered
payload survives a writer-produced apply cycle"; ED-0038's convention recorded
as a convention rather than a derivation, with the information-theoretic
ambiguity proof that withdrew the single-actor-lossless claim; the fail-closed
multi-actor posture; and the detection rule rebuilt against the accepted
member-name encoding after issue 0004's text landed.

**Three defects in that freshly accepted text were found from here** and are
routed as ED-0040 queue items 8, 9 and 10 rather than fixed: the §4.6 LWW4
scoping mismatch, the legacy-mode actor-universe conflict, and §5.1's silence
on mixed members. None was found by that text's own twelve-round adversarial
review or by the orchestrator.

Adversarial review (r0011, `openai/gpt-5.6-sol` at max effort): **APPROVE of
the evidence handoff only, explicitly not of the queued implementation.** It
blocked the original causal fold pre-edit with the R/P/D counterexample that
produced ED-0036; found the empty-map vacuity recorded as ED-0041 instance
three; caught two staleness classes against the decision record and then the
cross-branch staleness after issue 0004 integrated; and mutation-proved its
own retention probe by making it fail before trusting it. It did the later
half of that while rebuilding from a context loss that wiped its working
history mid-verdict.

This issue remains `In Progress`. ED-0018's gates are open, and the P
implementation — widened position maps, three-branch lifecycle, per-target
mixed-batch semantics, retiring
`TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`, and removing issue 0028's
V2 marker — is **queued and unstarted under ED-0040**, not merely blocked.
V2's 150 oracle and its blocked marker are unchanged on main.

## Round 3 dispatched (2026-08-31)

Re-dispatched under ED-0065 after a planned reboot. The branch was re-checked
out into worktree `.herdr/worktrees/domowik/issue-0011-r3` and lands on
`dd17124`, the frozen SHA, confirmed rather than assumed. Scope is the three
blockers from the post-fix verdict: the zero-valued snapshot frontier panic,
the I2-suppressed redelivery poisoning closure validation, and the differential
closure oracle's structural inability to validate either implementation
through shared helpers. The reviewer additionally carries the backlog the
previous round disclosed as unverified — the three pre-freeze blocker FIXES
(the defects were orchestrator-verified; the fixes were not) and the four
audit areas whose subagents returned nothing.

## Round 3 INTEGRATED 2026-08-31 (ED-0069)

Branch `issue/0011-causal-position-implementation` (`dd17124`..`9cebd00`)
merged by `22aa322`. Full main `./scripts/verify.sh` exits 0 with a clean tree
after two integration fixes recorded below. **This closes the round that began
frozen at `dd17124` with three blockers outstanding.**

**BLOCKER 1 — zero-valued snapshot frontier panic. FIXED (`3011d83`).** A
frontier entry `{actor: 0}` produced an empty range that reached an
unconditional `rs[len(rs)-1]` and panicked the reader on the fully public
`ReplayFromSnapshot` path. The orchestrator reproduced it independently with a
discriminating control: `{x: 0}` panicked while the actor omitted returned
cleanly, proving the two spellings were not equivalent. Normalized at the S2
boundary. **Issue 0004 has since ruled the zero member FORBIDDEN outright, so
this normalization is now DEFENCE IN DEPTH rather than the fix, and the literal
`sameFrontier` comparison at §5.1 S3 is CORRECT — do not canonicalize it.**

**BLOCKER 2 — I2 call-granularity dependence. FIXED (`8e31aaf`).** The same
batches produced different verdicts depending on how they were grouped into
`Deliver` calls. The remedy is a DOMAIN SPLIT, not a reordering: ordinary
`Deliver` classifies covered redeliveries before building the ancestry set for
fresh ones, while retained-log construction keeps validating every entry.
**The brief that dispatched this round told the executor the opposite** — that
watermark classification must precede closure validation — which is the literal
inverse of FORMAT-SPEC I2 (`:490-493`). The reviewer caught it within twenty
minutes; see ED-0067.

**BLOCKER 3 — the differential oracle's overstated coverage. FIXED
(`5d89613`).** The optimized and naive closure validators call the SAME
`validateOwnPriorEnds` and `validateNoPartialRangeObservation`, so differential
agreement could never validate either helper. **The sharing is deliberate and
must remain** — `closure.go:301-303` records that independent partial-range
implementations historically reproduced the same straddling-range misconception.
The defect was the coverage CLAIM, not the architecture. Nine spec-derived
witnesses now carry that burden, each with an executed mutant, and the reviewer
confirmed each mutant reddened the exact witness it targeted rather than some
other one.

**The `ReplayFromSnapshot` gates, and an orchestrator error.** Mutation testing
showed removing `validateFrontierClosure(log)` reddens the retained-log test
while removing `authenticateAgainstBatches(log)` leaves the package GREEN. The
orchestrator demanded a witness for the second; the executor built one, found it
did not discriminate, diagnosed why, and asked rather than faking it.
**The gate is REDUNDANT, not untested**: ordinary `Deliver` authenticates ALL
unique batches including covered ones (`projection.go:431`), while closure
excludes covered (`:478`). That asymmetry explains both results. The demand for
a witness was unexecutable — the ED-0047 item 1 shape from the orchestrator
again — and the gate is now retained with a comment stating it is currently
redundant, when it would become load-bearing, and that no test can witness it
today (ED-0072's second branch).

**Two integration fixes, needed only on the combined tree:**
1. `testdata/corpus/EXPECTATIONS.md` conflicted with issue 0028's
   just-integrated partial-history labels. Resolved by taking this branch's
   RE-DERIVED facts (ED-0043 payload shape, expansion 704:1, digest
   `99b84f2f…`) and re-attaching issue 0028's gating disclosure. The recorded
   digest was then checked against the committed artifact bytes and MATCHES —
   taking issue 0028's side would have left a digest describing bytes that no
   longer exist.
2. `cb4e075` — issue 0029's reserved-range fixture built batches at
   `counterBase 42` with an empty frontier, which this branch's F4 own-prior-end
   enforcement correctly rejects. The FIXTURE was non-conforming, not the
   enforcement; it now declares its own prior end.

**A contract breach, recorded because the claim did not hold.** The branch was
NOT gofmt-clean at handoff — four files including the gate comment carried a
trailing tab — so `./scripts/verify.sh` could not have exited 0, despite the
handoff reporting a clean verification run. Fixed at integration.

Status remains `In Progress`: the audit areas the round-4 verdict left
unverified are still unverified, and the upstream gates on 0004 and 0010 remain
open under ED-0018.

## Deferred-audit continuation integrated 2026-08-31 (ED-0069)

Branch `issue/0011-deferred-audits` merged by `882e916` with SBOM refresh
`1947738`; full main `./scripts/verify.sh` exits 0 with a clean tree. Evidence
in TRACKED files at `docs/evidence/0011-audit-areas-1-2.md`,
`0011-audit-area-3.md`, `0011-audit-area-4.md`.

These are the four areas an earlier round delegated to five parallel subagents,
four of which died on usage limits having produced nothing. They have been
carried as UNVERIFIED in every verdict since. **Two are now verified and two
are sampled. That distinction is the point of this entry.**

**Area 1 — ED-0043 re-derived expectations: VERIFIED.** All five
`EXPECTATIONS.md` entries checked against FORMAT-SPEC §5.1, §4.4 and §4.7.
Outcomes unchanged; the reasoning matches normative semantics.

**Area 2 — ED-0045 V2 coverage discharge: VERIFIED.** V2's 150-oracle check is
executed and DISCRIMINATES UNDER MUTATION rather than merely existing, which is
ED-0045's observable standard rather than a mechanism claim.

**Area 3 — V5/V6 compaction paths and I5 placement: SAMPLED.** Coverage
enumerated across seven sub-areas. Well covered: LWW4 concurrent creates,
multi-operation `operationIndex`, V5 seeded-versus-full replay, V6 snapshot
replay equality. **Two gaps recorded and still OPEN:** I5 branch 1 (a late
batch positioned before creation) and multiple independent create/delete
cycles. Both are test-coverage gaps, not demonstrated implementation defects,
and are recorded as unverified rather than as clean.

**Area 4 — repository-wide sweep: SAMPLED.** Ten high-risk categories checked
in `internal/document` — panic-versus-error consistency, slice bounds and map
access, `causalPastSize` overflow, frontier closure, watermark classification
edges. No new defects found. Targeted, not exhaustive.

**A correction the orchestrator required before integration, recorded because
the shape recurs.** The handoff's headline read "ALL FOUR AREAS CLOSED" while
its own body three lines later said two verified and two sampled, and the
evidence documents said SAMPLED in their own Result lines. **Closed means the
obligation is discharged**; two of these have not discharged it, and a tracker
entry saying otherwise would have retired an obligation and given the next
orchestrator no reason to look again. The evidence was honest throughout — only
the summary outran it. This is the same shape as the ED-0032 commit message
earlier the same day: accurate body, overstated headline. **A summary is a
claim and is held to the standard of the thing it summarises.**

Issue 0011 remains `In Progress`: the two Area 3 gaps are open, Area 4 is
targeted rather than exhaustive, and the ED-0018 upstream gates on issues 0004
and 0010 are unchanged.

## Rust implementation delivered 2026-08-31

**Everything above this line describes the FROZEN GO TREE.** That work is at
`_reference/go/` and is reference, not product (ADR 0008). The Rust core
implements FORMAT-SPEC §4 and BEHAVIOR-SPEC §8 afresh against the
specifications, so the Go tree's open blockers, audit gaps and ED queue items
do not transfer as defects — they transfer as things to get right, and this
entry records which were and which were not.

Delivered in `crates/domowik-document`:

- `operation` — operations and batches; F3's ordering position
  `P = (causalPastSize, counterBase, actorId)`; LWW1's write position refined
  by operation index.
- `log` — §4.3 row 4 framing validation before replay; F3 delivery-order
  independence with causal holding and cascading promotion; canonical order as
  a sort by `P`; I2 duplicate suppression.
- `replay` — I5's THREE-branch staleness test against each target's live
  interval at the operation's own write position; I3 tombstones; §4.6 LWW by
  application in canonical order; E6 authorized restoration by provenance.
- `session` — R8.1 through R8.4.4 in full.

Persisted per §7.6 in `crates/domowik-container`: one JSON document per batch
at `ops/<19-digit counterBase>-<actorId>.json`, name/content binding, L23
envelope caps, and the §5.1 S2 snapshot frontier in the manifest.

CLI: every mutating command commits a batch; `undo`, `apply` and `ops` exist;
probes P6-P8 exercise them against the shipped binary.

### Acceptance criteria

- [x] Duplicate and out-of-order batches behave according to conformance
      vectors; atomic batches fully apply or do not apply. V1, V2, V3, V4, V4b,
      V6 and V7 are covered by name; B1 atomicity is enforced by classifying a
      batch against a scratch copy so a discard leaves no trace.
- [x] No public API mutates projected entities directly. `Session` exposes
      `&Document` and `&Projection`; the only paths to state are commit, undo,
      redo, and accepting a batch.
- [x] Undo/redo property tests cover random command streams, compound edits,
      failed commands, cancellation, and redo invalidation.
- [ ] Replay from a snapshot/frontier produces the expected projection for all
      permitted delivery orders. Snapshot-SEEDED replay works and V6 holds, but
      §5.1 S4 tombstone retention is not implemented, so a baseline cannot
      block resurrection of something deleted before it was taken. Stated in
      `project_from`'s own documentation rather than left for a reader to find.
- [ ] Session persistence and compaction interaction match behavior/spec
      rules. Persistence is delivered; COMPACTION IS NOT (issue 0027), so
      R8.4.3's truncation diagnostic has nothing to fire on.

### Not delivered, and why

- **§5 snapshots and compaction.** Replay is always from the full log, so a
  long-lived plan grows without bound. Issue 0027 owns it.
- **LWW5 / vector V5.** Retained winner positions in a snapshot; needs the
  above.
- **A CLI redo.** R8.4.2 makes the stacks session-local and never persisted, so
  a CLI process — a session that lives for one command — has no stack to pop.
  `undo` compensates the last batch, and undoing an undo redoes it. The full R8
  stack semantics are implemented and tested in `Session`; an interactive front
  end holding one across commands gets them with no change to the core.
- **The Go tree's ED-0037 causal-position machinery.** Not ported. FORMAT-SPEC
  §4's own preamble puts multi-user merge out of v0.1 scope, and §4.4-§4.6 as
  written are satisfied by canonical-order application. If a future issue needs
  the widened position tables, it starts from the specification, not from the
  frozen tree.

Status stays In Progress: two criteria are open above, and ED-0018's upstream
gates on issues 0004 and 0010 are unchanged.
## Adversarial review — 2026-08-31

Codex (`gpt-5.6-sol`, high effort) reviewed `log.rs`, `replay.rs` and
`session.rs` against five stated claims. **Eight findings, all real, all
fixed**, with fourteen mutation proofs. Recorded with its transcript at
`docs/evidence/oplog/ADVERSARIAL-REVIEW.md`.

The two worth repeating here, because both were defects in code whose own
comments claimed the opposite:

- **FORMAT-SPEC F4 frontier closure was not implemented.** Obligation 2 makes
  validating it a READER's duty, and a non-closed frontier under-reports
  `causalPastSize` so a batch sorts BEFORE one it causally follows. Now
  validated on delivery, with obligation 3 honoured by skipping entries at or
  below the baseline where closure is unprovable.
- **`Session::commit` did not enforce E6.** `replay::authorized_restore` reads
  authorization from provenance, and its comment said the command layer stopped
  ordinary creates from resurrecting a tombstone. It did not; `commit` accepted
  anything. It now refuses, and `redo`/`compensate` are the authorized paths.

Held batches bypassing framing validation made the delivered result depend on
ARRIVAL ORDER, so criteria 1 and 2 above — the two this issue's acceptance
rests on — were false until this round. They are met now, and the tests that
show it are named in the evidence.

## Status corrected by ED-0096 — 2026-09-01

**In Progress (partial scope).** Vectors V1–V6 covered, no public mutation surface on the session, replay from a baseline proven identical under ALL 24 delivery orders of four batches (three concurrent, one causally dependent, so most orders force a hold and a later promotion), and `causalPastSize` widened to `u128` on 2026-09-01 because §9.3 row L33 says in terms that a 64-bit accumulator cannot represent the range and that wrapping or saturating arithmetic is FORBIDDEN. NOT delivered, and the reason this is not Done: the compaction interaction does NOT match §5.1 — S1 and S3 are omitted and every batch concurrent with the snapshot is refused, which is a narrowing of the contract rather than an implementation of it — and R8.4.3's live-stack truncation diagnostic does not exist.
