# 0004: Specify logical entities and operation metadata

- **Phase:** 0
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0001, 0002, 0003, 0019, 0020, 0021, 0030

Activated 2026-08-24 under ED-0014 on branch `issue/0004-format-entities-ops`.
Consumes the Done behavior specifications as normative input, only material
labeled "Provisional handoff — 0021 partial scope" for asset fields, and
NUMERICAL-MODEL values strictly as Candidate parameters marked provisional.
Numeric-bound fixation listed NOT APPROVED in `docs/evidence/0030-numerical-
model/RATIFICATION.md` §6 (including FORMAT-SPEC L14 and BEHAVIOR-SPEC R3.0.8)
stays explicitly deferred; this issue cannot reach Done while any such marker
remains absent or unfixed, except by explicit re-scope.

## Partial handoff accepted (2026-08-24)

Branch `issue/0004-format-entities-ops` merged to main by `6041867` with SBOM
refresh `04a8304`; full main `./scripts/verify.sh` exits 0 (now including the
syntax-only `cmd/domowik-jsoncheck` docs step). All seven acceptance criteria
were reviewed as satisfied by the adversarial reviewer and confirmed by the
orchestrator diff/provenance review: typed v0.1 entity schemas with Tier B
provisional envelopes, keyed/canonical-set collection rules, named provisional
coordinate parameters (`coordBoundMm`, `dimensionCapMm`, `derivedCoordBoundMm`,
`lwwHorizon`), batch/counter/frontier/idempotency/tombstone/LWW semantics with
deterministic vectors, snapshot winner-key metadata, four-way retention
separation, explicit no-canonical-JSON decision. Per the partial-scope guard
and ED-0014 the issue remains `In Progress`: no Done status until the 0030-
dependent parameter fixation gates resolve or an explicit re-scope records
otherwise. Dependents may consume only this delivered material as a labeled
provisional handoff under the tracker's partial-scope semantics.

## Problem

Turn `../docs/FORMAT-SPEC.md` sections 2–5 into normative logical schemas.
Define stable entity IDs, room reconciliation representation, atomic batches,
operation IDs, actor counters, frontiers, idempotency, tombstones, conflicts,
snapshots, and local undo/history separation.

## Acceptance criteria

- [ ] Every v0.1 entity has typed required/optional fields, units, bounds,
      reference lifecycle, and valid examples. Splats are excluded.
- [ ] Ordered tuples/rings/batches are permitted and specified; entity
      collections do not acquire accidental array order.
- [ ] Batch-boundary invariants are distinguished from per-op checks,
      warnings, and derived-geometry errors.
- [ ] Duplicate, reordered, concurrent, delete-versus-set, and snapshot replay
      vectors have deterministic expected projections.
- [ ] Snapshots retain per-field winner/version, tombstone, batch, and
      deduplication metadata needed for late-concurrent operations.
- [ ] Undo, sync retention, audit retention, and compaction are not conflated.
- [ ] JSON examples are valid; canonical JSON is either explicitly selected or
      byte identity is not required.

Open format question routed from issue 0011 review (2026-08-25): snapshot
state carries `CreatedKeys` (creation op-key per UUID) needed for
snapshot-seeded replicas to resolve late-concurrent create collisions
identically to never-compacted replicas (LWW4/V5), but FORMAT-SPEC §5.1 S1–S5
does not enumerate it. Resolution options: extend S3/S5 winner-metadata
wording to cover creation keys, or specify an alternative mechanism. Owned by
this issue; must be resolved before its closure gates can complete.

Open format questions routed from issue 0028 review (2026-08-25): (1) no
deterministic trigger exists for the ambiguous-manifest rejection category
inside the normative v0.1 closed-namespace profile (FORMAT-SPEC 7.3/7.4/9.4) —
decide whether the category stays reserved-with-no-trigger or the profile
changes; (2) tolerance for repeated physical occurrences of identical blobs
appears unreachable given injective blob mapping plus duplicate-logical-name
rejection — decide whether the tolerance rule is removed, reworded, or made
reachable. Both owned by this issue's closure work.

## Continuation activated 2026-08-26 (ED-0035)

Branch `issue/0004-lww-wording-and-i2-scope`. Two tasks, both routed here:

1. **ED-0032:** tighten §4.6 LWW1's fourth sentence ("Across actors, K
   totally orders writes"), which read in isolation licensed the delivered
   issue-0011 implementation to apply the key comparison across causally
   ordered writes. The normative outcome is settled by ED-0032 — §4.4 F3 plus
   LWW1's "concurrent candidates" scoping plus §4.7 V2's "equals in-order
   delivery exactly" — and is written into the text, not relitigated.
2. **ED-0033:** answer the §4.5 I2 half of the open question — is §7.6's
   structural B3 check unconditional over every retained entry, or scoped to
   batches surviving watermark classification? Issue 0005 owns the §7.6 half;
   issue 0029's fix direction depends on both.

This issue stays `In Progress` under ED-0014.

## Continuation outcome 2026-08-26 (ED-0035 → ED-0041)

Branch `issue/0004-lww-wording-and-i2-scope`. Both routed tasks are answered,
and the first grew from a wording fix into the canonical-order construction
after ED-0036 found §4.4 F3 underdetermined. Eighteen approved passages in
`docs/FORMAT-SPEC.md` §§4.4–4.9, §5.1 and §5.2 changed, plus one contradiction
repair (I3) found while applying them. Every §4.7 vector's required outcome is
unchanged; only V5's terminology and two added explanatory paragraphs moved.
No Go file, no §7 text, no corpus fixture, no evidence record.

### Task 1 — the canonical order (ED-0032 → ED-0036 → ED-0037)

ED-0032 asked only that §4.6 LWW1's fourth sentence stop reading as an
override of F3. Issue 0011's reviewer then showed, and ED-0036 confirmed, that
F3 defined no canonical order at all: "any linear extension of the causality
relation with ties broken by ascending key K" states pairwise constraints that
cycle. Exhaustively, 6 of 114 keyed 3-write and 888 of 5256 keyed 4-write
shapes cycle under that reading.

This pair then proved an impossibility that eliminated a whole family of
repairs: no construction holds all three of LWW1's greatest-K among causal
maxima, LWW2's discarding of losing values, and V5/V6's snapshot equivalence.
The witness is the reviewer's own attack — `Q(K3)` concurrent with `D(K2)` so Q
wins; a later `W(K1)` observing only Q supersedes Q, at which point the correct
winner becomes `D`, a value already discarded. 768 of 5256 keyed 4-write shapes
exhibit that dethroning.

ED-0037 (owner) adopted the ordering position, refined by comment (b) to
`(causalPastSize, counterBase, actorId, operationIndex)`. It is a SORT KEY
rather than a set of pairwise constraints, which is why cycles are
structurally impossible rather than merely avoided; it is causally monotone, so
the sort is always a linear extension; and it is intrinsic to each write, which
is what lets one retained position per field replace history. The accepted
consequence is that K no longer separates concurrent writes of unequal
causal-past size — measured deviation from LWW1 as written: 15.8% of keyed
3-write and 24.7% of keyed 4-write shapes.

Supporting rulings written into the text: F4 makes frontier transitive closure
a normative authoring requirement, including the own-actor entry, and records
that closure is unprovable once ancestry is compacted (ED-0037 (e)); S1, S3 and
S4 retain creation positions, winner positions and deleting positions with
per-entity live intervals (ED-0037 (f), (l)); I5 places a late batch in three
branches per operation with mixed-batch any-stale semantics, `createEntity`
establishing an interval and `deleteEntity` closing it; ED-0038's legacy
convention is stated with V5 exactness explicitly disclaimed; and ED-0039
tightens §5.2 S6's drop condition.

### Task 2 — the §4.5 I2 half of ED-0033's watermark question

**Ruling: not watermark-scoped.** I2 suppresses effects only and is not a
structural exemption; watermark classification never precedes, gates, or
replaces the pre-replay structural checks §4.3 row 4 applies to the retained
log. Grounds: I2's "already applied" inference depends on B3, so it cannot be
B3's gate; §4.3 row 4 and §9.5 already order framing before replay; and
watermark-scoping would make identical bytes conforming for one reader and
malformed for another, which is the divergent-reader risk §9.4's Rule T1
rejects and the parity premise ED-0023 rests on.

**Consequence for issue 0029:** the `Load` path is the defective side and
`Inspect` is NOT over-rejecting; ED-0023's parity premise needs no amendment on
account of ED-0033 finding 3. `frameAndOrder` building its non-skipped set by
watermark classification before per-actor range chaining violates this ruling,
and that consequence is robust to whatever issue 0005 rules, because the defect
is that classification PRECEDES the check, not which entries it covers.

**Not decided here — issue 0005's §7.6 half:** which entries §7.6 must
enumerate, and whether a discontinuity left by compaction is a B3 violation or
a separate coverage category. One constraint from this half: if 0005 rules such
a discontinuity legitimate it must be a named compaction-coverage rule, not an
I2 watermark exemption.

### What this issue also resolved

The `CreatedKeys` question routed here from issue 0011's review is answered:
§5.1 S1 enumerates `createdPositions`, retained for tombstoned entities as well
as live ones, and the retained datum is a position rather than a bare key.

### Method note

Three of this pair's findings came from the reviewer catching a divergence
between an artifact that looked like evidence and the thing that did the work:
an aggregate that could not discriminate the sound rule from a broken one,
expectation records inspected instead of the checkers that consume them, and
specification prose that contradicted the verified model it described. ED-0041
records the class; §4.7 now carries the normative half of the remedy.

### Status

`In Progress`, unchanged, under ED-0014.

### Verification claim, stated accurately

What was actually done before committing: every decision cited in the edits was
checked against `git show main:EXECUTIVE-DECISIONS.md` to confirm it exists and
is Active (twelve of them); the do-not-cite list was honoured (the
`operationCounter - 1` identity appears only as ED-0038's *convention*, never as
a derivation; the four-component write position rather than the three-component
`P`; no claim that single-actor legacy packages are unaffected; the own-actor
frontier entry required rather than benign; ED-0039 cited rather than the
withdrawn comment (o)); and the text itself was derived from the amendment
chain — ED-0037 comments (a)–(t), ED-0038, ED-0039 — rather than from the
summary block.

The earlier handoff claim to have confirmed the in-force table "line by line"
cannot stand, and not only because it was broader than the check performed: the
table was itself STALE on the one row this text turns on. Its retention row
still carried comment (m)'s unqualified `O(entities + fields)` after comment (s)
and ED-0039 had narrowed it to *number bounded, bytes not*. Issue 0004's
reviewer found it during a citation spot-check; it is corrected on main at
`e9a28d5`, which also adds the standing requirement that anyone amending
ED-0037 must update that block in the same commit.

So the accurate statement is the stronger one: this text follows the latest
amendments, which is why it is right — not because a summary confirmed it. Two
things are recorded here deliberately. A claim of having checked must not
outlive the discovery that the thing checked was defective. And the
same-commit requirement is the mechanised form of both failures at once: it
would have forced the stale row to be rewritten, and it would have made
checking that row mean something.

### Open before closure — exhaustive, feeding ED-0040's standing queue

Owned by this issue:

- ED-0014's numeric-bound fixation gates that RATIFICATION §6 lists NOT
  APPROVED, and `lwwHorizon` fixation, now also carrying the snapshot-size
  consequence of the widened position maps.
- Exact wire bounds for `causalPastSize`: the arithmetic is fixed here
  (exact, non-overflowing, no serialized sum); any §9.3 limit row is issue
  0005's placement (ED-0037 (g)).
- The two format questions routed from issue 0028 review on 2026-08-25: the
  ambiguous-manifest trigger, and the repeated-identical-blob tolerance.
- Terminology and phrasing disclosed rather than edited: §5.1's
  "not interchangeable" paragraph still says "pre-horizon winner keys", in a
  passage the reviewer confirmed unchanged; and I3's pre-existing absolute
  "block resurrection" phrasing, which does not cite E6 as its authority and
  so was left alone rather than widened. Both remain open.

Issue 0005 (§7 and §8):

- The §7.6 half of ED-0033's watermark question, which this issue's §4.5 half
  is written to compose with.
- ED-0033's §7.6 name/content-binding defect (`batchId` absent from the entry
  name).
- §7.5's snapshot member placement: the full new member set
  (`batchPositions`, `winnerPositions`, `createdPositions`, positional
  tombstones), not merely the stale "S3 winner keys" terminology.
- The `assetRoot` `null` acceptance category from ED-0034 — reject under §9.6
  or warning under an absent-table reading.
- Recording in §8 that v0.1 is unreleased (ED-0037 item 3).
- Placement of any §9.3 limit row for the `causalPastSize` bound.

Issue 0011 (implementation):

- Implement the write position, the widened position maps, the three-branch
  per-operation lifecycle, and F4 closure validation.
- Retire or rewrite `TestLWWWinnerKeyKeepsGreatestKAcrossCausalWrites`, which
  pins the ED-0032 defect and cites V3 while violating its precondition.
- Remove or reclassify issue 0028's V2 `executionBlocked` marker and make the
  coverage audit count V2 as real coverage, per ED-0036's replacement
  condition for ED-0035's void one.
- Implement ED-0038's legacy read, rewrite-on-save, and fail-closed paths.

Issue 0028 (conformance corpus):

- `checkV5` is tautological — it never reads the projection it is handed;
  `checkV3` ignores `actorId` and `loserValueDiscarded`; `checkV4` ignores
  `concurrentWriteApplied` (ED-0037 (r)).
- Every vector check must be mutation-proven: executed against a deliberately
  wrong projection and required to fail.
- Re-derive the position-bearing assertions in `EXPECTATIONS.md` at lines 448,
  452, 685, 708 and 1510, plus the two multi-actor snapshot fixtures
  (`golden/snapshots/snapshot-full-s1-s5.json`,
  `adversarial/vectors/v5-late-concurrent-after-compaction.json`).
- The V3 fixture must use equal-past-size writes or have its expected value
  re-derived.

Issues 0027 / 0029 / 0031 (persistence, compaction, migration):

- The `Load` path builds its non-skipped set before per-actor reserved-range
  chaining, which this issue's §4.5 I2 half-ruling rejects; `Inspect` is not
  over-rejecting and ED-0023's parity premise stands.
- `internal/persist/reader.go:468` discards `checkReservedRanges`' `*Error`.
- `internal/persist/wire.go`'s `null` tolerance for the required `assetRoot`
  array.
- The retained-log write-path disagreement: `internal/cli/apply.go:415`
  rebuilds from `proj.Log()` while `internal/persist/smoke_test.go:62`
  rebuilds from `view.Retained`.
- The save-path question behind it: a save drops all PRIOR covered retained
  entries (ED-0037 (h) corrects the broader "one save discards the entire
  history" claim).
- Inheritance of ED-0039's S6 drop precondition and attestation, and of the
  synced/local compaction observables added here (S6 and S7).

Issues 0035 / 0018:

- Re-measure Part A budgets after the position widening; never re-derive them.
  Issue 0018 may not publish the pre-ED-0037 numbers as v0.1 budgets.

## Continuation handoff accepted 2026-08-26 (ED-0035, ED-0037/38/39)

Branch `issue/0004-lww-wording-and-i2-scope` (`3e841d5`..`3dcc015`, seven
commits) merged to main by `fa7e847` with SBOM refresh `12ea7c0`. Full main
`./scripts/verify.sh` **exits 0** with a clean tree.

Delivered: FORMAT-SPEC §§4.4–5.2 define the canonical order as ascending write
position `(causalPastSize, counterBase, actorId, operationIndex)` per ED-0037,
resolving ED-0036's underdetermined F3; frontier transitive closure becomes a
normative **authoring** requirement including the own-actor entry, with
validation obligations and an explicit statement that snapshot-only validation
cannot prove it; snapshot positions widen for winners, creations and
deletions, with per-entity live intervals and three-branch lifecycle
placement; ED-0038's legacy reading convention is stated as a convention with
V5 exactness disclaimed; ED-0039's tightened S6 drop condition is a
precondition with the acknowledgement branch narrowed to its three protocol
attestations and both synced and local blocked-compaction observables; and the
§4.5 I2 half of ED-0033's watermark question is answered — the watermark never
gates retained-log structural checks, so the `Load` path is defective,
`Inspect` is not over-rejecting, and ED-0023's parity premise stands. An
unplanned I3 tombstone-shape repair was disclosed and accepted as the minimal
consistency fix the approved S4 change required.

**No §4.7 vector outcome changed.** V1, V2, V3, V4, V4b, V6 and V7 are
byte-identical; V5 differs only `winner key` → `winner position`, with
"identical outcome to the uncompacted replica" untouched. Confirmed by three
independent runs — worker, reviewer, and orchestrator.

Adversarial review (r0004, `openai/gpt-5.6-sol` at max effort): **ACCEPT**
after twelve proposals, with nothing written until the twelfth. It withheld
approval nine times, and its findings included the F3 pairwise cycles, the
causal-max loser resurrection, operation/batch interleaving, the `>int64`
`causalPastSize` bound, the legacy ambiguity, own-actor frontier omission, the
creation and tombstone live intervals, the unbounded residual I5 class that
became ED-0039, peer-acknowledgement insufficiency, the create/delete
endpoints, E6's authorized-restoration exception, and the stale retention row
in ED-0037's own summary. Its acceptance was amended to cover `3dcc015` after
the orchestrator merged past the originally accepted range; it reviewed every
byte of that delta rather than letting the earlier range stand in.

This issue remains `In Progress` under ED-0014, whose numeric-fixation and
`lwwHorizon` gates are untouched, and under ED-0040. Its exhaustive
owner-grouped routed list is the record ED-0040's queue resumes from.

## Post-acceptance finding 2026-08-27 (ED-0040 queue item 8)

Issue 0011's worker, reconciling its evidence document against this issue's
newly merged text, found a scoping mismatch between two accepted sentences;
the orchestrator verified it against main.

§4.5 states that "a collision with another create on the same id resolves
under LWW4", unqualified. §4.6's LWW4 is scoped to "two **concurrent**
`createEntity` operations for the SAME id from different actors (each locally
valid under O4)". A causally ordered same-id collision is therefore sent to a
rule whose own text does not govern it.

The case may be unreachable: the later actor observed the create, so O4 local
validation should have rejected it, and LWW4 already states the
within-one-actor case is impossible. That is not a reason to leave it — it is
the same "probably unreachable" reasoning that produced several of this
session's defects, and two accepted normative sentences disagreeing is a
wording question for §4.6's owner.

Not fixed: ED-0040 forbids dispatching further work. Recorded here and in the
ED-0040 queue so whoever resumes finds it.

## Second post-acceptance finding 2026-08-27 (ED-0040 queue item 9)

Issue 0011's reviewer found, and the orchestrator verified against main, that
§5.1 lines 739–747 select the legacy reading mode solely by the frontier's
actor count — one actor takes the documented convention, two or more is
fail-closed. `FromSnapshot` however accepts winner and creation key actors
absent from the frontier, so a snapshot may carry a single-actor frontier
while its keys span several actors. Such a snapshot would be routed to the
single-actor convention while its winners are in fact unplaceable.

The predicate likely needs to be the actor UNION over the frontier and the
retained keys, or §5.1 must state why the frontier alone is sufficient. Not
fixed: ED-0040 forbids dispatching further work.

## Third post-acceptance finding 2026-08-27 (ED-0040 queue item 10)

§5.1 line 738 defines a legacy snapshot as one carrying the pre-ED-0037
`winnerKeys`, `createdKeys`, or `deletingKey` members "instead". The wording
implies the two encodings are mutually exclusive but rules nothing for a
snapshot carrying **both** — new `winnerPositions` alongside an old
`createdKeys`, for instance. A reader implementing §5.1 has no rule to apply.

The natural candidate is the fail-closed posture used everywhere else here: a
snapshot new in one position-bearing member and old in another is neither
new-shape nor eligible for the convention, so it is rejected rather than
partly converted. That is this project's reasoning rather than a ruling, and
issue 0011's evidence document labels it as that document's posture rather
than asserting it as a rule. Not fixed: ED-0040 forbids dispatching further
work.

## Zero-versus-absence ruling integrated 2026-08-31 (ED-0069)

Branch `issue/0004-zero-absence-semantics` merged by `77968d2` with SBOM
refresh `4fe0018`; full main `./scripts/verify.sh` exits 0 with a clean tree.

**Ruling: a frontier member whose counter is ZERO is malformed input and is
FORBIDDEN.** FORMAT-SPEC §4.4 now carries **F1 Zero prohibition**: absence —
the actor omitted from the frontier map — is the sole representation of
"nothing observed"; zero is not a permitted alternative spelling. Readers MUST
reject before constructing watermark state, performing closure validation, or
storing the frontier in any position table (§5.1 S3). The justification argues
from format semantics rather than implementation convenience: F1 defines
absence as nothing observed, and B3 starts an actor's counter stream at 1, so
zero cannot be a valid watermark for any stream state.

**Why this issue ruled, and why now.** In one day, two independent pairs and
the orchestrator hit the same ambiguity in FOUR distinct places — the snapshot
S2 watermark (which PANICKED a reader on the public `ReplayFromSnapshot` path),
§5.1 S3 position authentication (where `sameFrontier` compares map
representations so `{}` and `{actor:0}` contradict each other), F4 ancestry
reporting (which reads an absent member as zero), and the closure oracle itself
(`closure.go` skips `n <= 0` in BOTH validators, which is structurally why the
differential oracle could never have caught the first site). Four sites, one
question, owned here because no implementation may settle it in a test.

**Consequences, recorded so they are not re-litigated:**
- Issue 0011's `3011d83` snapshot normalization becomes DEFENCE IN DEPTH rather
  than the fix.
- The literal `sameFrontier` comparison at §5.1 S3 is CORRECT, not a divergence.
  No canonicalization is needed and none should be added.
- The `n <= 0` skip in both closure validators is sound by design.

**Diagnostic, tightened after orchestrator review.** The first draft named two
conforming diagnostics with no primary, which contradicted §9.4:1642 (each class
states a PRIMARY) and §9.5:1704 (outermost failing stage wins) and would have
left issue 0028 unable to author a fixture carrying one expected token per
§9.6 — reproducing, one layer down, the divergent-reader risk the ruling itself
invokes. The rule now names `invalid-field-syntax` as primary at schema
validation, with `malformed-framing` where schema does not constrain counter
minimums, and routes only the schema-design question to issue 0005.

**Corpus impact: ZERO fixtures affected, verified twice and by different
methods.** The pair scanned 77 loose JSON files. That search was incomplete —
`testdata/corpus` also holds 38 `.domo` packages, zip containers with JSON
inside, a third of the corpus. The orchestrator opened all 38, parsed 135 JSON
entries, and ran a raw-byte pass catching 74 uncompressed frontier objects,
including the three deliberately corrupt hostile packages that cannot be opened
as archives. **No zero-valued frontier member exists anywhere in the corpus**,
so the prohibition requires no fixture change.

**`CreatedKeys` (routed here from issue 0011's review, 2026-08-25) is
DISCHARGED**, and the discharge was verified against the text rather than
accepted on report: §5.1 S1 enumerates `createdPositions` and states in terms
that it resolves the routed question, with the retained datum a POSITION rather
than a bare `(operationCounter, actorId)` pair.

**Routed onward:** the schema-constrains-minimum question to issue 0005 (§7.6);
the L33 reader-arithmetic obligation to issue 0011; a `batchPositions`
comparison check to issue 0029 (likely no action).

Status remains `In Progress`: this issue's other acceptance criteria and its
upstream gates are untouched.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** FORMAT-SPEC §§2-5 are normative for v0.1 within their stated scope and the Rust core implements them. What remains is parameter FIXATION — the semantic batch caps of B7 and the lwwHorizon of §5.1 S3 — which is why implementation issues no longer gate on this one (ED-0095).

## Parameter fixation delivered 2026-09-12 (ED-0105)

Branch `finalize/0004`. This closes the residual ED-0095 named — "what remains
is parameter FIXATION — the semantic batch caps of B7 and the lwwHorizon of
§5.1 S3" — and three more residuals found by reading the current normative text
rather than the checkboxes.

**`lwwHorizon` is FIXED at complete coverage.** The horizon equals the
snapshot's own S2 coverage, with no count, age or byte window inside it.
`docs/LWWHORIZON-FIXATION-ANALYSIS.md` assembled the inputs on 2026-08-31 and
declined to recommend, on the grounds that the horizon decides what users lose.
Working the options through, there was nothing to trade: any window smaller
than coverage breaks §4.7's V5 for inputs outside it and V5 is a fixed vector,
tombstones cannot be aged out at all without §5.2 S6's precondition, and cost
is already bounded by L23 with an observable refusal. The measured baseline is
NOT an affordability claim: an earlier draft of this record cited issue 0035's
"116 KB compressed, 0.7% of L23's 16 MiB" as measured headroom, and root
corrected it. That figure is Go-era Part A WHOLE-SNAPSHOT compressed bytes, not
S1/S3 position metadata; the Rust implementation retains no S1/S3 positions, so
this horizon's cost has never been measured in the system that would pay it;
and the comparison crosses byte classes §9.2 calls separate quantities. The
metadata cost is UNMEASURED. One derived economy comes with it — a field still holding the value its
`createEntity` gave it takes its winner from the S1 creation position rather
than a restated `winnerPositions` entry — and one reader obligation keeps the
decision reversible: retaining more than the horizon requires is never an
error, so a future narrowing is writer-side only.

**B7's semantic caps are fixed as: there are none beyond §2.3 and
L17/L18/L23.** The two named deferrals close as DERIVED. `wallJoin` membership
is bounded by `w` for `w` walls, because §2.3 prohibits both endpoints of one
wall in one join. The frontier actor-count cap EQUALS L18, which **discharges
row L33's revisit trigger** — L33 asked what this issue fixes that cap to, and
the answer leaves its derived maximum standing rather than stale.

**Row L11 gained the counting contract it never had, and an implementation.**
This is the one that was not merely a wording residual. L11 ratified 128 and a
boundary but never defined a level, and nothing in the Rust workspace enforced
it: both untrusted parse sites relied on `serde_json`'s incidental recursion
limit, and the pinned 1.0.151 **accepts 127 levels and rejects 128** — a
document L11 calls conforming, refused at exactly the ratified boundary, with
an unmapped error instead of `limit-nesting-depth`. §9.3 now states the
counting rule in full, DERIVED from the corpus twins issue 0028 already
authored; `domowik-container` counts depth over the bytes before any parse and
reports the token; and the `apply` path calls the same check, which is what
issue 0015's "depth is enforced" criterion had been resting on nothing for.
Four mutants were run and each was killed.

**Two rejection classes are RESERVED — UNREACHABLE, and neither was removed.**
`ambiguous-manifest` and row L29 keep their tokens with a normative disposition
naming the diagnostic that fires first, and conformance coverage for such a
class is discharged by a fixture asserting the PRECEDING token. This closes
`gaps/ambiguous-manifest-deterministic-trigger`, open since 2026-08-25.

Two stale passages disclosed in the 2026-08-26 handoff are repaired: §5.1's
"pre-horizon winner keys" (ED-0037 renamed them positions) and §9.4's
negative-zero sentence, which still deferred canonicalization to "issue 0004's
schema … once ratified" after §3.2 had ratified it. The 2026-08-27 findings 8,
9 and 10 were checked against the current text and are ALREADY FIXED — §4.5
now scopes the causally ordered same-id create out of LWW4, §5.1 defines the
actor UNION rather than the frontier alone, and a snapshot carrying both
encodings rejects.

### The adversarial round did not happen, and what was done instead

Codex started with the right banner — `model: gpt-5.6-sol`, `reasoning effort:
high` — and returned **no findings**, exiting on "Your workspace is out of
credits". Its exit code was 0, so a round that produced no adversary would have
looked clean to anyone not reading the output. Nothing was written for it and
no review is claimed.

The scrutiny was done directly instead, by putting the prompt's own questions
to the work. **It found a defect in this issue's own text**: B7's first draft
derived the `wallJoin` bound as `2w` from the two-valued role and missed the
endpoint prohibition that halves it — an error in the permissive direction.
Two more holes were closed the same way. A CLEARED optional field was eligible
for the S3 creation-position economy, which could have restored a value the
clear had already beaten; §5.1 now says clearing is writing. And
`parse_depth_checked` was public and took a plain slice, so the guard that
makes standing `serde_json`'s limit down safe could be skipped by any caller;
it now takes a `DepthChecked` token only the guard can construct.

### Evidence

- `cargo test -p domowik-container -p domowik-cli`: all green, 8 new container
  tests and 1 new CLI test.
- Mutation proofs, four, each killed: depth guard removed; boundary made
  exclusive (the serde_json off-by-one); string awareness removed; the CLI call
  site deleted.
- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets -D
  warnings` clean; `domowik-jsoncheck docs` and `… testdata/corpus` both exit 0.
- `scripts/rust-deps-inventory.py` diffs EMPTY against
  `docs/RUST-DEPENDENCIES.md`: the `serde_json` `unbounded_depth` feature adds
  no crate and changes no licence.
- **Full `./scripts/verify.sh` is NOT yet run** — the shared verification slot
  was requested from the orchestrator and not yet granted. No banner, so no
  result is claimed (ED-0077).

### Status

`In Progress (partial scope)`, unchanged **in this commit**. Every acceptance
criterion and every gate this issue owns is now discharged, but the tracker
records a verification claim only against a run that produced a banner. The
status moves to `Done` in the commit that carries that run.

Residuals that were this issue's and are now elsewhere, named so the move is
auditable rather than implied: BEHAVIOR-SPEC R3.0.8's precision/tolerance
deferral and the remaining RATIFICATION §6 items belong to the numerical model
(ED-0100, ED-0103); implementing S3's positions and lifting
`check_dominates_snapshot` belongs to issues 0011/0027; the typed Tier B
structural models and `roomRecord` belong to issue 0010. A separate conformance
divergence found in passing and deliberately not touched: the Rust container
reads the manifest entry as `manifest.json` where §7.3 and the corpus name it
`domowik.json`.

## Boundary coverage completed 2026-09-12 (root direction)

Row L11 is now proven end-to-end at BOTH untrusted entry points, across the
whole boundary rather than only its rejecting side.

| Depth | Package reader (entry point A) | `apply` path (entry point B) |
|---|---|---|
| 127 | admits, reads back, unknown field preserved | admits, applies, unknown field preserved |
| 128 | admits, reads back, unknown field preserved | admits, applies, unknown field preserved |
| 129 | writer refuses (`CandidateInvalid`); reader refuses (`NestingDepthExceeded`) | refuses, `limit-nesting-depth` |

Test names: `nesting_depth_l11::the_package_reader_admits_127_and_128_and_rejects_129`
and `tests::apply_admits_127_and_128_and_rejects_129`, alongside the earlier
`the_boundary_is_inclusive_below_and_exclusive_above`,
`a_conforming_document_at_the_boundary_actually_parses`,
`the_corpus_twins_are_the_executable_statement_of_the_rule`,
`an_over_deep_entry_rejects_through_the_package_reader`,
`an_external_batch_nested_past_row_l11_is_refused_before_it_is_parsed`,
`brackets_inside_strings_are_not_containers`,
`an_escaped_quote_does_not_end_the_string`,
`depth_is_the_maximum_reached_not_the_running_total` and
`stray_closers_cannot_buy_depth_back`.

**Depth admission is separated from later semantics by construction.** The
admitted payloads are semantically VALID — a level carrying an unknown optional
field nested to suit, which §8 U2 requires a reader to preserve — so 127 and 128
do not merely "pass the guard and fail later"; they succeed completely, and the
preserved field is asserted on the far side. A payload that passed the guard and
then failed on schema would not distinguish the two stages, which is the thing
root asked to see separated.

**The depths are read off the corpus, not restated.** Both tests derive the
accepting and rejecting depths by measuring
`golden/parser-twin-nesting-depth-at-L11.json` and
`hostile/json/nesting-depth-over-L11.json` at run time, and compute 127 as one
below. No third artifact was needed. The corpus twins are bare arrays that no
schema admits as a document or a batch, so they supply the DEPTHS while the
end-to-end payloads are valid structures built to match them; that limit is
stated rather than worked around.

**Found by writing the test:** `write_package` REFUSES to emit an over-deep
package. §9.7 S2 validates the candidate with reader-equivalent checks, so the
depth guard fires on the save path too and the existing target is left
untouched. The 129 case was drafted expecting the write to succeed and the read
to reject; the assertion now pins both halves, including that no target file is
left behind (§9.7 F3).

**Fifth mutant, the sharpest one.** Removing `de.disable_recursion_limit()` —
which is precisely the original defect, a reader inheriting serde_json's limit —
fails BOTH new tests, the CLI one reporting `depth 128 conforms and must apply:
… recursion limit exceeded at line 1 column 594`. That is the bug reproduced
verbatim and caught at both entry points. Earlier mutants: guard removed,
boundary made exclusive, string awareness removed, CLI call site deleted.

Green at this commit: `cargo fmt --check`, `cargo clippy --workspace
--all-targets -- -D warnings`, `cargo test --workspace` (all suites, 0 failures).
No full `verify.sh` — ops0027 holds the slot.

## `lwwHorizon` — owner question, and why it is not mine to close

Root direction 2026-09-12: do not call this ratified owner policy on an agent
argument while the fixation analysis reserves it. **The ED-0105 horizon clause
and its FORMAT-SPEC §5.1 text are marked Proposed.** The other three ED-0105
decisions are unaffected and stand.

**The reserving text, quoted exactly** (`docs/LWWHORIZON-FIXATION-ANALYSIS.md`,
closing block):

> **Decision authority:** Product owner. The horizon determines what users lose
> when late deliveries exceed it, which is a product trade-off, not a technical
> one.

**What is settled without the owner, and is implemented.** Complete retention
is the safe minimum: it loses no data, and no smaller value is available
without changing §4.7 V5, because a window breaks V5 for inputs outside it and
tombstones cannot be aged out at all without §5.2 S6's precondition, which v0.1
supplies no protocol to attest. A writer implements it today; a reader may rely
on it. That is a no-data-loss implementation decision, not a product fixation.

**The one question for the owner.** *Should `lwwHorizon` be FIXED at complete
coverage as v0.1 policy, or left Proposed until a bounded horizon can be costed
against a real implementation?*

Concrete consequences either way:

- **Fix it now.** v0.1 ships with one retention rule, V5 holds unconditionally,
  and issues 0011/0027 implement S1/S3 against a settled target. The cost is
  accepted unmeasured, and if it later proves too high the narrowing is still
  available without a format break — §5.1's reader obligation ("retaining more
  than the horizon requires is never an error") was written to keep that door
  open.
- **Leave it Proposed.** Implementation proceeds identically, because complete
  retention is the safe minimum either way and is what the code does. The cost
  is that the parameter stays open through v0.1, every dependent issue keeps
  citing a Proposed value, and the tracker carries an unfixed parameter into
  release — which is the state ED-0014 was written to prevent.

**The byte domain the question turns on, and a defect it exposed.** §9.2
requires that "every limit row below names which quantity it binds". Row L23's
`snapshot metadata section ≤ 16 MiB` names NO byte class — logical, physical or
decoded — so no measurement can be compared against it without first choosing
one, and on this corpus logical and compressed differ by roughly an order of
magnitude. Any costed answer to the question above needs that domain named
first. Recorded here as an open residual of this issue, unfixed, because
choosing the domain changes which packages are accepted and is not a repair to
make silently.

What a real measurement would require, so nobody mistakes the historical figure
for one: an implementation that actually retains S1/S3 positions (issues
0011/0027), a multi-actor corpus rather than the single-actor generator, an
exclusive measurement slot, and the L23 byte domain settled.

## Row L23's byte domain decided 2026-09-12 (root delegation, bounded engineering authority)

Root delegated this rather than escalating it: **the 16 MiB snapshot-metadata
cap is UNCOMPRESSED SERIALIZED bytes.** The numeric value is unchanged. The
reason is that a reader-resource cap bounds what a reader must HOLD, and
compressed length cannot bound that — snapshot metadata is repetitive by
construction, the same actor UUIDs and batch stems recurring in every retained
frontier, so it compresses hard and a stored-byte cap would admit a section
that expands far past any bound a reader can honour. L15/L16 bound the ratio;
L23 bounds the result.

Counted: `frontier`, `batchPositions`, `winnerPositions`, `createdPositions`,
`tombstones`, S5 dedup metadata, and any unknown member — keys, punctuation and
the whitespace actually present, because §3.3 does not require canonical JSON
and the cap binds the bytes PRESENT rather than a normalized form. Unknown
members count because §8 U2 makes a reader preserve them, so they occupy memory
like anything else; excluding them would leave the least-understood part of the
section as the only unbounded thing in it. NOT counted: `entities`, S1's
projected state — it is document content bounded by L3/L4, and folding it in
would silently convert a metadata cap into a maximum document size, which is a
product decision this row does not make.

**Enforcement stage, stated because it differs from the pre-parse rows.**
Member boundaries are not knowable before parsing, so this cap is evaluated
during structural traversal, before the metadata is RETAINED. Allocation ahead
of that is bounded by L3/L4 and L15/L16, so nothing asks a reader to trust an
unbounded input first.

### A normative conflict found while defining it

§7.5 line 1346 says "Row L23's snapshot-metadata ceiling binds **this
section**" — the whole `snapshot/state.json` entry, S1 included — while §5.1
and §5.2 S7 use it as a bound on the metadata alone. Those are different
quantities and cannot both be right; under the whole-entry reading a large
document would become uncompactable for reasons having nothing to do with
metadata. The metadata reading governs, per root's delegation. §7.5's sentence
is a drafting error, named in §9.3 rather than silently overridden, and
repaired under issue 0005.

### The structural blocker root asked me to report rather than work around

**No member this cap counts exists in the wire.** `Package.snapshot` is a bare
`Document`; there is no `winnerPositions`, `createdPositions` or
`batchPositions` anywhere in `crates/`; S4's tombstones are a bare `Vec<Uuid>`
in the manifest rather than positions, which is the same misplacement class as
the frontier that persist0029 found. So the cap cannot be wired into the reader
yet — there is nothing to enforce it against — and `snapshot_metadata_bytes` is
deliberately unwired, saying so in its own doc comment. Issues 0011/0027 land
those members; this is the accounting they enforce, specified and tested ahead
of them rather than invented alongside them.

### Tests, including the one that justifies the domain

- `entity_state_is_not_counted_but_every_other_member_is` — 4 KiB of entity
  state must not move the count.
- `an_unknown_member_is_counted`.
- `the_boundary_is_inclusive_at_the_cap_and_rejects_above_it` — equal succeeds,
  above rejects.
- `a_highly_compressible_oversized_section_is_still_oversized` — the witness for
  the domain itself: realistic repetitive metadata whose STORED form fits well
  under 16 MiB while its uncompressed form exceeds it. The test asserts both
  halves, so it fails if the witness stops discriminating.

Mutants six and seven, each killed by the test that guards it: counting
`entities` (the maximum-document-size error), and skipping an unknown member.

### Correction carried into this record

An earlier draft said complete retention "is the current implemented
behaviour". That was wrong and root corrected it: nothing implements S1/S3
retention, so the phrase describes no reader that exists. ED-0105's horizon
item and FORMAT-SPEC §5.1 now say Proposed CONTRACT and say plainly that no
implementation retains those positions.

## L11's budget belongs to the entry — question from issue 0029 answered 2026-09-12

Issue 0029 integrated this issue's L11 and L23 work with the §7 wire
(`e3cf983` on `finalize/0029`) and routed one question back: row L11's budget
costs a level to the §7.5 section wrapper, so does ED-0105 mean the budget to
apply to document CONTENT rather than to the entry? If content, that is a
wording change here rather than a code change there.

**Answer: the budget belongs to the ENTRY. No change to the rule; the
consequence was missing from the text and is now stated (§9.3).**

Their finding is correct and I confirmed it against §5.1 rather than taking it
on report: the snapshot example carries `entities` as a MEMBER of the snapshot
object, so entity content sits at depth 3 inside `snapshot/state.json` where the
same content alone would sit at 1. They are also right that this survives their
interim body — §5.1 makes the projected state a member rather than the whole, so
every conforming package has it.

Why the entry and not the content. L11 is a parser-resource bound, and the thing
a parser recurses through is the entry, so counting from the entry root is what
makes the number mean anything. Granting an entry extra levels to offset its
wrapper would make the limit differ per entry, and a reader would have to know
which entry it was parsing before it knew the limit — the divergent-reader risk
the counting contract exists to remove. The cost of the rule is real and is now
written down instead of discovered: content gets 128 minus its entry's wrapper
depth, and a boundary fixture must name the entry it targets, because "a
document at depth 128" is ambiguous until it does.

**Their change to this issue's two package-level tests is accepted in
principle, and NOT yet verified by execution.** The §7.4 inventory pass now runs
before any entry is parsed, so tests that build archives by hand were rejecting
at the integrity layer and no longer reached the depth guard — a real way for a
passing test to stop proving anything, caught by them rather than by me. Routing
them through an inventory-correct helper is the right repair. I have read the
commit and the reasoning holds; I have not run it, because release0036 is
mid-verify and a build from here adds contention. Verify by execution when it
lands rather than recording it as proven now.

Also confirmed from their side: S1 is excluded from the L23 count under both its
names (`entities` per §5.1, `document` in the current wire), and their mutation
evidence shows the exclusion is the load-bearing half — without it the guard
passes just as happily and the first large plan anyone saves stops opening.

## Outbound-routing audit after the `herdr agent send` truncation bug — 2026-09-12

`herdr agent send` silently truncates past roughly 1,730 characters, returns
`{"result":{"type":"ok"}}`, and delivers only the TAIL. A cut message therefore
does not look cut — it reads as a short, complete, different message. Audited
every message this issue sent, by measuring the files they were sent from:

| Message | chars | outcome |
|---|---:|---|
| residual map → root | 6,095 | truncated |
| advisory-failure report → root | 3,456 | truncated |
| HEAD + boundary gap → root | 5,433 | truncated |
| namespace rules → 0029 | 3,696 | truncated |
| L23 accounting → 0029 | 2,352 | truncated |
| manifest discrepancy → 0029 | 1,927 | truncated |
| unreachable ruling → 0028 | 2,783 | truncated |
| Cargo coordination → 0010 | 1,640 | intact |
| L23 result → root | 1,606 | intact |
| 0005 request → root | 940 | intact |
| L23 + 0005 start → root | 1,230 | intact |

Seven of eleven were cut. **Re-sending was impossible for every affected
recipient**: 0028, 0029 and 0010's workers have all been shut down, and root is
blocked on credits.

**One confirmed casualty, re-homed rather than lost.** The unreachable-class
ruling reached 0028 without its ruling statement, and
`gaps/ambiguous-manifest-deterministic-trigger` is still open in
`EXPECTATIONS.md` asserting a blocker that no longer exists. Recorded in issue
0028's file with the one disposition that remains.

**Two that demonstrably did NOT cost anything, checked rather than assumed.**
Issue 0029's worker implemented the L23 accounting correctly — S1 excluded under
both its names, mutation-proven — and repaired the §7.5 wrapper-depth
interaction. They did not get that from my messages; they got it from
FORMAT-SPEC on `main`. The spec was the channel that worked, which is the
lesson: a normative decision written into the specification survives a lost
message, and one that lives only in a routing does not.

**Practice changed for anything this issue sends from here:** full content to a
file, a short pointer with a one-line gist, and delivery verified by grepping
the recipient for a marker from the HEAD of the message — a tail marker proves
nothing.
