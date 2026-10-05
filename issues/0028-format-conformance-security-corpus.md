# 0028: Build format compatibility and security corpus

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0004, 0005, 0021, 0023

Activated 2026-08-24 under ED-0019 on branch `issue/0028-conformance-corpus`.
Fixtures are authored from FORMAT-SPEC §2–§9.6 as integrated on main plus
only labeled 0021 provisional material, before persistence implementation.
Numeric values only via named parameters or §9.3 quantities; no Done while
upstream issues remain unclosed under their recorded gates, except by explicit
re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0028-conformance-corpus` (commits `73004f8`..`0e809d6`) merged
to main by `8efdff5` with SBOM refresh; full main `./scripts/verify.sh` exits
0. Delivered: 118 inert bounded fixtures (golden/adversarial/migration/
hostile/declarative recipes) under `testdata/corpus/` with a machine-readable
manifest (recomputed sha256 per fixture, structured asserts, exact §9.3/§9.4
category tokens), seven corpus-gate tests in dev-only `internal/corpusgen`
(byte-reproducibility, safety cap, completeness/digest audit, coverage-set
equality with negative invariants), and additive verify.sh integration.
Adversarial reviewer verdict: ACCEPT after early scope rejection was satisfied
with the five required conditions. Machine-readable blocked/open residuals are
retained in EXPECTATIONS.md; two upstream spec findings routed to owners:
(1) ambiguous-manifest has no deterministic trigger inside the normative v0.1
closed-namespace profile (FS 7.3/7.4/9.4 owner decision needed);
(2) repeated-physical-blob tolerance appears unreachable given injective blob
mapping plus duplicate-logical-name rejection (FS 7.x owner decision).
Per ED-0019 this issue remains `In Progress`: implementation-deferred vectors
carry no conformance claims and upstream gates stay open. Dependents may
consume this delivered material as a labeled provisional handoff under
partial-scope semantics.

## Problem

Create reusable golden, adversarial, migration, and hostile fixtures for every
reader/writer and validator. Fixtures are authored from the normative specs
before persistence implementation; implementation-generated cases may be added
later.

## Acceptance criteria

- [ ] Corpus covers all entities/operations, snapshots/frontiers, thin and
      portable assets, unknown optional/required data, and migrations.
- [ ] Hostile fixtures cover traversal, duplicates, bombs, counts/sizes/depth,
      malformed JSON/numbers, digest mismatch, and corrupt/truncated packages.
- [ ] Expected success/error/warning category is machine-readable for each
      fixture.
- [ ] Fixtures are independently authored or have recorded compatible
      provenance and licenses.

Issue 0018's release audit (`ccfe953`) confirms the following remain blockers,
not coverage: skipped `v2-reordered-delivery`; partial V6/V7 semantic oracles;
hostile `missing-manifest`, `spanned-disk-number`, and
`ratio-under-L15-success-twin`; the duplicate-key record whose bytes contain no
duplicate key; and the duplicate-batch framing open question. Resolve these
without weakening expected categories before release qualification.

Continuation activated under ED-0027 on branch
`issue/0028-release-remediation`: make every actionable skipped/partial vector
executable with semantic assertions, while retaining genuine spec-owner
ambiguities as explicit blockers rather than manufacturing pass cases.

## Continuation re-dispatched 2026-08-26

The ED-0027 remediation wave delivered only issue 0026's branch; the 0028 and
0029 continuations were never dispatched and no branch or worktree survived.
Recorded as an orchestration incident. Issue 0028's ED-0027 continuation is
re-dispatched unchanged on branch `issue/0028-release-remediation` under the
ED-0030 verification baseline; issue 0029's follows once a slot frees, because
four of its deferred records name 0028 fixture defects that this work owns.

## Continuation handoff accepted 2026-08-26 (ED-0027)

Branch `issue/0028-release-remediation` (`a9e6b77`..`e32887c`, 14 commits,
base `b51dd50`) merged to main by `02373c5` with SBOM refresh `0209c3e`. Full
main `./scripts/verify.sh` **exits 0** on the integrated tree with a clean
tree afterwards.

All eight routed issue-0028 skips are gone; zero remain. Six were generator or
fixture defects rather than specification gaps: `missing-manifest` dropped the
wrong entry by slice position; `spanned-disk-number` patched EOCD+18 inside
`cdOffset`; the ratio twin swapped its payload after manifest construction and
shipped bytes that were not a parsable snapshot; the duplicate-key fixture had
eight distinct keys; and the two framing fixtures collapsed to a single §7.6
entry name so a different reject fired first. Neither framing case was
unmaterializable. V6's oracle asserted only non-emptiness and V7's `checkV7`
was dead code with a vacuous live assertion. V2 is settled by ED-0032 as an
issue-0011 conformance defect and executes as a divergence-pinned witness with
no `t.Skip`, contributing no coverage.

Diff is 53 files, +2129/−659; excluding the regenerated SPDX documents, 50
files of which 35 are hostile/container artifacts regenerated by the ED-0034
`assetRoot` fix. **Zero expected category or outcome lines changed** and the
170-record set is intact — both independently confirmed by the orchestrator.
`internal/document` and all `internal/persist` product code are untouched; the
only non-test source change is the dev-only `internal/corpusgen` generator,
confirmed absent from `go list -deps ./cmd/domowik`.

Adversarial review (r0028, `openai/gpt-5.6-sol` at max effort): **ACCEPT**
after fourteen findings across four rounds, every one resolved or explicitly
recorded. It rejected the first V2 witness design, which asserted only
divergence from the oracle and would have stayed green on any third value, and
required the causal premise to be asserted directly so a concurrent V7 case
could not masquerade as V2. Its coverage signoff confirms `executionBlocked`
is assertion-granular, `residualBlockers` matches bidirectionally, and
blocked-only evidence cannot satisfy category or row coverage.

Snyk Code: no high findings and none in modified files. Two unchanged
pre-existing findings recorded for the log — LOW `internal/cli/apply.go:110`
path traversal and MEDIUM
`spikes/0024-flutter-go-bridge/charness/main.c:100` env-to-`dlopen`. Both are
outside this diff and untouched.

This issue remains `In Progress` under ED-0019 with six named gates: upstream
0004/0005/0021 unclosed; ED-0032's V2 conformance defect routed to 0004/0011;
ED-0033's `frameAndOrder` finding routed to 0029; ED-0034's `assetRoot` null
tolerance routed to 0029 and 0005; the §7.6 `batchId` wording defect routed to
0005; and the two pre-existing routed gaps untouched.

## Tautological vector checks routed here 2026-08-26 (ED-0037 comment (r))

Issue 0004's reviewer found, while checking an unrelated clause, that
`internal/persist/corpus_semantic_test.go:467` `checkV5` takes
`proj *document.Projection` and never reads it. It asserts only that the
manifest's own `lateBatchApplied` literal is false, so V5 and
seeded-versus-uncompacted equivalence are not exercised at all. `checkV3`
ignores `actorId` and `loserValueDiscarded`; `checkV4` ignores
`concurrentWriteApplied`.

This is the same class the ED-0027 continuation was chartered to eliminate —
V6's non-emptiness oracle and V7's dead `checkV7` — and a third instance
survived that work, its adversarial review, and orchestrator acceptance. The
coverage audit cannot detect it by construction: it verifies that a record
exists, is categorised, and is not blocked, never that the assertion function
consumes the projection.

Required beyond repairing the three functions: **every vector check must be
mutation-proven** — executed against a deliberately wrong projection and
required to fail. This is ED-0033's probe discipline applied to the vector
harness rather than to the corpus gates, and it is what closes the class
rather than the instance.

Also queued for this issue from ED-0037: the two 2-actor fixtures
`golden/snapshots/snapshot-full-s1-s5.json` and
`adversarial/vectors/v5-late-concurrent-after-compaction.json` plus
`EXPECTATIONS.md:448` carry the old key shape, and the V3 fixture must use
equal-past-size writes or have its expected value re-derived.

## Round integrated 2026-08-31 (ED-0069)

Branch `issue/0028-l33-corpus-machinery` (`a090e56`..`e741010`, four commits)
merged by `754b12e` with SBOM refresh `331b2ab`; full main
`./scripts/verify.sh` exits 0 with a clean tree.

**Task 1 — the L33 machinery, delivered as one piece per ED-0048.** The
acceptance regex at `internal/corpusgen/corpus_test.go:307` widened to admit
L33, the completeness loop extended to the full §9.3 row set, and an L33
`rowAudit` entry added with status `blocked-upstream-spec`, gap record
`gaps/l33-reader-arithmetic-obligation`, and a `residualBlockers` entry.

The proof is EFFICACY, not presence: flipping the entry to `covered` makes the
audit fail at `corpus_test.go:640` with "row L33 covered without records", and
reverting restores green.

**Why L33 could never be marked covered, which corrects the brief that
dispatched this work.** The orchestrator's own Task 1 was written as if L33
were coverable by fixtures like other §9.3 rows. It is not, and the
specification says so: §9.3's L33 row carries "n/a (reader representation
obligation)" in its reject column, and `:1631-1632` states that "a frontier
exceeding L33 cannot be presented, because L18 or L13 rejects it first, so such
a clause could never fire". **No corpus fixture can violate L33 by
construction.** The executor stopped and escalated rather than widening a
literal to satisfy an impossible instruction — exactly what ED-0047 item 2
exists to produce. The obligation L33 actually states is the READER's (exact
comparison across a 64-bit boundary, wrapping and saturating forbidden) and is
routed to issue 0011 as an arithmetic test.

**Task 2 — three partial-history success oracles labelled in TRACKED content.**
`golden/packages/thin-valid`, `golden/packages/portable-valid` and
`hostile/container/ratio-under-L15-success-twin` each now carry, in
`testdata/corpus/EXPECTATIONS.md`, an explicit note that the SUCCESS OUTCOME IS
GATED ON AN UNRULED QUESTION, plus the new gap record
`gaps/partial-history-package-conformance` and its `residualBlockers` entry.

Two corrections were needed to reach that state, both found by orchestrator
verification against the artifacts:

1. **The third package was missed.** The first assessment covered two. Opening
   all 38 `.domo` archives and cross-referencing recorded outcomes shows
   EXACTLY THREE packages carry `"outcome": "success"`, and all three have the
   shape. The missed one lives under `hostile/container/` while being expected
   to SUCCEED, so a search reasoning by directory walks past it. The reliable
   predicate is the recorded outcome, not the path.
2. **The deliverable existed only in gitignored `build/`.** The assessment had
   been performed but its output would have been deleted when the pair was
   purged, leaving three gated oracles with nothing recording the fact. Work
   that lives only in the scratch directory has not been done.

**ED-0048's count is corrected.** It recorded "four of the 38 committed
packages share this shape". Measured against the artifacts, **34 do** — they
are generated from a common template — while the "three with outcome success"
is exactly right. For the other 31 the shape is irrelevant: they are hostile
container fixtures that reject before frontier semantics are reached.

**The labels deliberately distinguish two adjacent questions**, and this is
load-bearing: issue 0004 ruled today that an explicit zero-valued frontier
member `{A:0}` is FORBIDDEN, but an EMPTY frontier `{}` is absence and remains
legal. These three packages are gated on partial-history conformance, which is
a separate and still-unruled question.

Status remains `In Progress` per ED-0019: implementation-deferred vectors carry
no conformance claims and the upstream gates stay open.

## F1 zero-frontier hostile fixture integrated 2026-08-31

Branch `issue/0028-zero-member-fixture` merged by `71bb217`; full main
`./scripts/verify.sh` exits 0 and `domowik-tracker-check` passes.

**This fixture was unbuildable until the same day it was built.** §9.6 requires
each fixture to carry ONE machine-readable expected category. Issue 0004 ruled
the zero-valued frontier member FORBIDDEN, and issue 0005 ruled the detection
stage — `invalid-field-syntax` at `StageSchema`, per §9.5's outermost-failing-
stage rule. Only then could the expected token be stated.

**Delivered:** `testdata/corpus/hostile/framing/zero-frontier-member.json` with
its `EXPECTATIONS.md` record (category `invalid-field-syntax`, clauses citing
§4.4 F1 and §9.5), a discrimination proof at
`docs/evidence/0028-zero-frontier/DISCRIMINATION.md`, and
`internal/persist/zero_frontier_test.go`.

**The reviewer REJECTED the first handoff and was right to.** The initial
discrimination proof was not reproducible from committed state and the verify
evidence was asserted rather than captured. Both were corrected — instruments
re-executed against `b4dc85d`..`3e0cd6c`, `EXIT 0` quoted per ED-0077 — before
ACCEPT.

**An out-of-scope change was accepted deliberately.** The round also modified
`internal/persist/wire.go`, which the brief assigned to issue 0029. The change
adds the F1 enforcement itself: a zero-valued member now rejects with
`CatInvalidFieldSyntax` at `StageSchema`. It is correct, it implements two
owner-level rulings faithfully, and the orchestrator verified it discriminates —
removing the guard reddens `TestInspectCorpusFramingParity/hostile/framing/
zero-frontier-member` ("precondition broken: framing fixture must reject under
Load") and `TestZeroFrontierMemberGap`.

**The pair hit a real dependency the brief had missed: a hostile fixture cannot
reject if nothing enforces the rule it violates.** That discovery was sound; the
process was not — it should have been routed rather than implemented across an
ownership boundary while issue 0029 had a live pair in that package. The
orchestrator's brief was also at fault for commissioning a fixture against an
unenforced prohibition. Accepted with the breach recorded rather than reverted,
because undoing correct, discriminating work to punish a process error is the
worse outcome.

Status remains `In Progress` per ED-0019.

## The Rust reader now runs against the corpus — 2026-09-01

Two tracked harnesses run on every `cargo test --workspace`:
`domowik-container/tests/hostile_corpus.rs` over 37 hostile packages, and
`domowik-document/tests/hostile_framing.rs` over the framing documents.

Contract: **every hostile fixture is refused, and none panics.** That is §9's
threat model reduced to what is checkable over a whole corpus without
reproducing each fixture's diagnostic. The class, not the code — a fixture
rejected for the wrong reason passes, and the evidence says so.

**One defect found**: a batch could declare a frontier reaching PAST its own
reserved range (`inconsistent-frontier.json`, reserving 85 and declaring 86).
The F4 closure check enforced only the lower bound. Both bounds are enforced
now. See `docs/evidence/conformance/RUST-READER-VS-CORPUS.md`.

Not delivered: the per-fixture expected diagnostics in `EXPECTATIONS.md` are
not asserted by these harnesses, and the `golden`, `adversarial` and
`migration` trees are not exercised at all by the Rust reader.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** The Rust reader now runs against the hostile corpus on every test run, and found one defect. NOT delivered: the per-fixture expected diagnostics in EXPECTATIONS.md, and the golden, adversarial and migration trees.

## Fixture defect routed here 2026-09-12 (issue 0062, ED-0104)

Found by the adversarial review of the corpusgen port, confirmed against the
committed bytes. **Eight traversal/name fixtures plus `ambiguous-manifest`
carry a second defect on top of the one they isolate.**

The generator slices the sorted base entry list at index 1 to mean "everything
but the manifest". Sorted ascending, index 0 is `assets/descriptors.json`, not
`domowik.json` — so the slice drops the DESCRIPTORS and keeps the manifest,
which still inventories them:

```text
traversal-dotdot.domo entries:
  "domowik.json"
  "ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json"
  "snapshot/state.json"
  "../domowik.json"
  manifest inventories:
    "assets/descriptors.json"     <- absent from the archive
    "ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json"
    "snapshot/state.json"
```

Affected: `traversal-dotdot`, `absolute-path`, `drive-form`, `backslash-name`,
`nul-byte-name`, `dot-segment-name`, `overlong-component-L9`,
`overlong-name-L8` and `ambiguous-manifest`, whose alternate `DOMOWIK.JSON` is
built over the same short slice and therefore differs from the primary manifest
for the wrong reason.

**This is the same class as the `missing-manifest` defect this issue already
fixed** — "the entry is removed BY NAME; a positional drop removes the wrong
entry" — surviving one function away from the comment that records it. The
`withoutName` helper exists because of it and is not used here.

**Why a traversal fixture can still reject for its stated reason and this still
matters:** §9.5's outermost-failing-stage rule decides which class fires, and a
missing inventory member is a different stage from an invalid entry name. A
fixture whose oracle depends on stage ordering is not isolating what its record
says it isolates.

Not repaired by issue 0062. The port reproduces the defect EXACTLY, which is
what makes the 39 artifacts byte-identical and the port provable. Repairing it
changes nine artifacts, their recorded sha256 digests and the expected-outcome
reasoning behind them, which is this issue's call. Evidence:
[docs/evidence/0062-corpusgen-rust/PARITY.md](../docs/evidence/0062-corpusgen-rust/PARITY.md).

## base[1..] defect REPAIRED 2026-09-12 (branch `finalize/0028`)

Fixed properly rather than preserved as a known bug, per root's direction:
recipes, digests and diagnostics corrected together.

**The repair.** `traverse` and the `ambiguous-manifest` second manifest now
build over the FULL canonical entry set, with `domowik.json` rebuilt over it.
§7.4 requires `entries` to map "EVERY other logical entry name", so the hostile
entry is inventoried too. Each of the nine fixtures now carries EXACTLY ONE
defect — the name it is named for. Verified against the bytes: archive members
and manifest inventory now agree, e.g. `traversal-dotdot.domo` lists
`../domowik.json` in both.

**Nine artifacts regenerated; nine digests updated; ZERO expected category or
outcome lines changed.** The `EXPECTATIONS.md` diff is 9 changed lines, all
`sha256`. That is not luck and not a shortcut: §7.3 says logical names are
"validated before any use" and rejects "at name validation (outermost per
§9.5)", so an inventory defect one stage deeper could never have decided these
fixtures' class. The recorded `path-traversal`, `invalid-entry-name`,
`limit-name-component` and `limit-name-length` tokens were right for the wrong
bytes and are right for the corrected ones.

**Both generators were fixed together and agree.** `internal/corpusgen` and
`crates/domowik-devtools/src/corpusgen` were each corrected and their outputs
diffed directory-to-directory: identical across all 39 artifacts. `go test
./internal/corpusgen` and the Rust corpus gates both pass, as do the Rust
reader's hostile harnesses (35 container, 127 document).

### Residual routed to issue 0004 — the ambiguous-manifest inventory

`ambiguous-manifest.domo` ships `domowik.json` and `DOMOWIK.JSON`. §7.4 now
requires each manifest's `entries` to list every other logical entry name,
which would require one manifest to inventory the other — and `DOMOWIK.JSON`
is a name the §7.3 closed namespace forbids, so an inventory listing it is
itself irregular.

**Not resolved here, and deliberately not papered over.** §7.4 already states
the reject is `duplicate-logical-name` at name validation, outermost per §9.5,
which fires before either inventory is read — so the fixture's recorded class
is unaffected either way. The second manifest is built over the same
descriptors-included set as the first, so the two no longer disagree for a
reason the fixture does not name. What a conforming manifest should say about
a sibling manifest is a §7.4 question, and it joins the already-routed 2026-08-25
finding that `ambiguous-manifest` "has no deterministic trigger inside the
normative v0.1 closed-namespace profile".

## Two items settled with issue 0004 — 2026-09-12

**The ambiguous-manifest route is closed, and the answer is "the question is
moot".** Issue 0004 ruled that a reader is NOT required to be able to produce
either unreachable token and MUST NOT reorder its stages to make one
producible; the token is retained so that, if some fixture ever does produce
it, the unreachability analysis was merely wrong and the name is still there.

That settles this corpus's side without settling the §7.4 wording. Because a
reader may not reorder to reach the inventory, the inventory content of
`ambiguous-manifest.domo` is not observable: name validation rejects first, per
§7.3 and §9.5. The fixture keeps `duplicate-logical-name`, and the repair made
earlier today — building both manifests over the same entry set — stands as
fixture hygiene rather than as a conformance claim. **Nothing in this corpus
asks a reader to reach that inventory, and nothing here should be read as
licence to make it reachable.**

**No depth-127 fixture will be committed. Declined deliberately, not
overlooked.** Issue 0004 offered one and confirmed nothing depends on it. L11's
boundary is already pinned from both sides — `golden/parser-twin-nesting-depth-
at-L11.json` at depth 128 succeeds, `hostile/json/nesting-depth-over-L11.json`
at 129 rejects — and that is the whole of the at-limit/one-over contract the
structured `boundaries` block encodes.

A depth-127 artifact would assert that 127 also parses, which cannot fail
unless 128 fails too. A record that can never be the sole cause of a failure is
exactly the non-discriminating coverage this issue has had to remove three
times: V6's non-emptiness oracle, V7's dead `checkV7`, and `checkV5` ignoring
the projection it was handed. Adding one to feel complete would be the same
mistake wearing a fourth costume.

Issue 0004 deriving 127 from the golden twin's MEASURED depth is also strictly
better than a committed file: a derived value cannot drift away from the
artifact, whereas a third fixture and the twin could disagree silently. The
golden twin's record now says so, so the next author does not re-raise it.


## Unreachable-class ruling re-homed here 2026-09-12 (ED-0105 item 4)

Recorded in this file because the routing that carried it was lost. Issue 0004
sent the ruling to this issue's worker on 2026-09-12; `herdr agent send`
silently truncated it and delivered only the tail, so the ruling statement and
its reasoning never arrived, and the worker has since been shut down. The
tracker is the durable channel, so the ruling lives here.

**`ambiguous-manifest` and row L29 are RETAINED, marked RESERVED — UNREACHABLE
under the v0.1 profile.** FORMAT-SPEC §9.4 carries a third disposition defining
what that means. The preceding diagnostic — the one that actually fires — is
`duplicate-logical-name` in BOTH cases: §7.3 admits exactly one manifest name so
two manifests are two entries of that name, and ASSET-STRATEGY §10.3's blob
mapping is injective so identical blobs share a logical path. Name validation is
the outermost stage under §9.5, ahead of §7.4 manifest parsing and ahead of
enumeration.

**Coverage for such a class is discharged by a fixture asserting the PRECEDING
token plus the recorded unreachability rationale** — not by a gap record, and
not by a fixture expecting a token no conforming reader can emit.

### What that leaves undone here, stated precisely

`hostile/container/ambiguous-manifest` already carries
`"category": "duplicate-logical-name"`, so the fixture itself needs no change.
What remains is that **`gaps/ambiguous-manifest-deterministic-trigger` is still
open in `testdata/corpus/EXPECTATIONS.md`**, its notes still reading "the
category remains in the audit as blocked-pending-owner-decision (FS 7.4/9.4
owners)". That blocker is resolved: the §9.4 owner has ruled, above. The gap
record now asserts a constraint that no longer exists, which is worse than an
absent record because it reads as live.

Issue 0004 did NOT edit it: `EXPECTATIONS.md` is this issue's artifact, `main`
was frozen at the time, and silently editing another issue's corpus file is the
failure mode ED-0079 exists to prevent. It is one record's disposition, and it
needs an owner.

The same applies to L29's audit entry, on the same grounds.
## §7.2 archive-comment limb covered, and a harness defect found — 2026-09-12

**Delivered.** `hostile/container/nonzero-archive-comment.domo` closes the gap
issue 0029 pinned as `corpus-defect` (0d212af on `finalize/0029`). §7.2 reads
"Entry comments AND the archive comment MUST be zero-length"; only the entry
limb had a fixture. The new one writes both halves of the EOCD comment — 36
bytes and a matching `commentLength` — because writing only the bytes makes the
reject `trailing-data` under T1 and writing only the length truncates the
archive. Both generators produce all 40 artifacts byte-identically, and Go's
`archive/zip` confirms the package is otherwise well formed.

**Found while verifying it: the container harness above cannot fail.**
`domowik-container` looks for `manifest.json`; the format's manifest is
`domowik.json` (§7.3, §7.4). The reader therefore refuses BOTH golden success
packages, and refuses every hostile fixture for the same reason — so
`every_hostile_fixture_is_refused_without_panicking` is satisfied by a reader
that reads nothing at all.

The 2026-09-01 section above says "The Rust reader now runs against the corpus".
Against the container corpus it does not, and that claim is corrected in
`docs/evidence/conformance/RUST-READER-VS-CORPUS.md` with the probe output and
a reproduction. The `domowik-document` framing harness is unaffected and its
`inconsistent-frontier` find stands.

**Routed, not fixed.** `MANIFEST_ENTRY` is reader code. This issue has already
had one out-of-scope reader change accepted with the breach recorded; the lesson
was to route, so it is routed. The corpus gap is closed; the reader gap is not,
and no fixture here should be read as evidence that it is.

