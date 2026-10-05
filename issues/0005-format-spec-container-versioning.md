# 0005: Specify physical container and versioning

- **Phase:** 0
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0004, 0009, 0021, 0023

Activated 2026-08-24 under ED-0017 on branch `issue/0005-container-versioning`.
Consumes Done 0009/0023 outputs, only "Provisional handoff — 0021 partial
scope" material for asset/package fields, and FORMAT-SPEC §2–§5 as integrated
on main via named provisional parameters. Operation-batch naming/order/framing
and compression fixation transfer here per 0021's contingent clauses; no Done
while any consumed-provisional or RATIFICATION §6 gate touching its criteria
remains open, except by explicit re-scope.

## Partial handoff accepted (2026-08-24)

Branch `issue/0005-container-versioning` (commits `2ba6cb2`..`87ecad5`) merged
to main by `c3acaf4` with SBOM refresh; full main `./scripts/verify.sh`
exits 0. Deliverables: normative FORMAT-SPEC §7 (strict ZIP-family Domowik
profile selected via same-criteria matrix against tar-pax/custom/SQLite;
manifest wire shape; closed entry namespaces; deterministic batch framing;
compression allowlist with non-observable writer settings; Zip64 schedule;
atomic replacement exclusively through §9.7 S0–S5; writer-threshold versus
retention separation) and §8 (experimental 0.N versioning, reader gate,
migration duty, unknown-data rules), plus Proposed ADR 0007 and evidence with
full provenance records E1–E6 (no GPL sources; APPNOTE consumed through 0023's
P7 record). Adversarial reviewer verdict ACCEPT after four findings were
addressed; orchestrator review confirmed §9 was consumed, never weakened.
Per ED-0017 this issue remains `In Progress`: no Done while consumed-
provisional or RATIFICATION §6 gates touching its criteria remain open.
Dependents may consume this delivered material as a labeled provisional
handoff under the tracker's partial-scope semantics.

## Problem

Select the physical `.domo` container and define manifests, entry names,
operation framing, thin/portable profiles, version identifiers, migration, and
unknown-data behavior. Do not assume ZIP until evaluated.

## Acceptance criteria

- [ ] Container decision covers streaming, large files, duplicate entries,
      checksums, compression methods, and atomic complete-package replacement.
- [ ] Thin and portable packages represent the same logical plan.
- [ ] Unknown required data is rejected; unknown optional data cannot be
      silently stripped during write-back.
- [ ] Pre-1.0 migration policy and the post-v1 compatibility promise are
      explicit; terminology does not misuse SemVer.
- [ ] Operation-batch naming/order/framing and snapshot frontier are normative.
- [ ] Writer thresholds are separated from format-observable retention rules.

## Routed-defect handoff accepted 2026-08-29 (ED-0042 queue item 1)

Branch `issue/0005-container-routed-defects` (`06bc662` content, `8476561`
SBOMs) merged to main by `4e77c12` with integration SBOM refresh `3de69cd`.
Full main `./scripts/verify.sh` exits 0 with a clean tree, run independently by
the executor, the reviewer, and the orchestrator.

All five ED-0040 queue-item-1 items landed as normative text; none absorbed,
none dropped. Five files, 138 insertions and 11 deletions: seven FORMAT-SPEC
hunks confined to §7.4, §7.6, §8.2 and §9.3, plus one line of
`testdata/corpus/parameters.json`. No §4.7 vector row moved — the eight rows
hash `392687d1d0` identically to the pre-edit baseline, verified separately by
the reviewer and the orchestrator, and no hunk falls in §4, §5 or §9.4.
`EXPECTATIONS.md` is provably untouched, which mattered because issue 0011 was
editing it in a parallel worktree.

Delivered: §7.6's name/content binding narrowed to what a reader can actually
compare, with malformed `ops/` names recorded as already rejected
`invalid-entry-name` at §7.3 name validation; the watermark question ruled
UNCONDITIONAL, so structural checks cover every retained entry before I2
classification, with the snapshot watermark permitted as a boundary datum but
never as a domain selector; a new §9.3 row L33 bounding `causalPastSize`,
derived from L18 and L13 with no quantity chosen and scoped to ONE frontier;
`assetRoot` `null` ruled `invalid-field-syntax` rather than an absent table;
and §8 recording that v0.1 is unreleased.

Two declarations are part of the acceptance. **L33 is knowingly uncovered**:
the completeness loop at `internal/corpusgen/corpus_test.go:650` stops at 32
and the acceptance regex at `:307` rejects `L33` outright, so the branch's
green run means nothing was broken, not that L33 is audited (ED-0048). And
`parameters.json` was the only corpus-tree file touched, confined to the
`ratifiedQuantitiesNote` string.

Adversarial review (r0005, `openai/gpt-5.6-sol` at max effort): **ACCEPT** of
this partial handoff, after seventeen findings across three rounds. Three
changed the outcome rather than the wording: it killed a proposed clause that
would have rejected a retained range ending above the snapshot watermark,
which contradicts fixed vector V6 and its executable fixture; it caught the
worker's own overflow witness being non-discriminating; and it caught agreed
exact text being silently dropped when the worker turned it into an apply
script. It also designed the discriminating mutation witness — overlapping
covered ranges under a watermark — and required the covered non-overlap
control that separates "rejects the overlap" from "rejects covered entries".

Status stays **In Progress** under ED-0017. Gates: the ED-0048 owner question
on partial-history packages, which is exactly why §7.6 is deliberately
incomplete for absent actors; issue 0021's partial-scope handoff; row L31
still Deferred to 0005/0029; row L14 Deferred to 0022/0030 with
RATIFICATION.md §6 holding the delegation NOT APPROVED; `lwwHorizon` still
Provisional in §5.1 S3; and L33's own revisit trigger depending on issue
0004's deferred frontier actor-count cap.

Routed onward: L33's audit entry, acceptance regex and derived-row-set fix to
issue 0028 as one piece of machinery, together with the four absent-actor
packages and `thin-valid.domo`'s fixture-integrity question; migration code
and tests to issue 0031; ADR 0007 and `AUDIT.md` to issue 0018.

## Wording defect and normative question routed here 2026-08-26 (ED-0033)

1. **§7.6 name/content binding is unsatisfiable as written.** FORMAT-SPEC
   line 950 requires a batch whose embedded `batchId` contradicts its entry
   name to reject as `malformed-framing`. The §7.6 entry name is
   `ops/<counterBase>-<actorId>.json` and contains no `batchId`, so
   `internal/persist/reader_helpers.go:47` structurally cannot perform that
   comparison. The `actorId` and `counterBase` clauses are satisfiable and are
   implemented. Found by the issue 0028 pair; no check was invented and no
   code was touched.
2. **Open normative question (jointly with issue 0004).** Is §7.6's
   structural B3 check unconditional over every retained entry, or scoped to
   batches surviving §4.5 I2 watermark classification? `Load` and `Inspect`
   currently answer differently — see the ED-0033 finding routed to issue
   0029, whose fix direction this ruling gates.

## Acceptance-category question routed here 2026-08-26 (ED-0034)

§7.4 line 852 makes `assetRoot` required with type "Ascending-by-digest
array", so no wording change is implied — but the acceptance CATEGORY for a
`null` value is an owner call: `reject` under §9.6, or `warning` read as an
absent table. `internal/persist/wire.go:85-88` currently tolerates it
silently. Issue 0029's fix direction depends on this ruling.

## §7.6 diagnostic placement and unreachable classes ruled — integrated 2026-08-31

Branch `issue/0005-zero-diagnostic-placement` merged by `7d2b5ad`; full main
`./scripts/verify.sh` exits 0 with a clean tree, and `domowik-tracker-check`
also passes. Rulings in `docs/decisions/ISSUE-0005-RULINGS.md` (187 lines) with
the normative text in `docs/FORMAT-SPEC.md`.

**Task 1 — the zero-member detection stage, routed here by issue 0004 hours
earlier.** Issue 0004 ruled a zero-valued frontier member FORBIDDEN and named
`invalid-field-syntax` as the primary diagnostic *"when JSON schema constraints
can detect the zero member"*, routing that condition here as a §7.6
schema-design question.

**Ruled: the primary diagnostic is `invalid-field-syntax` at `StageSchema`.**
The reasoning matters more than the token and is recorded with it: §9.5 speaks
in STAGES, not schema languages — "the outermost failing stage wins and
diagnostics name that stage" — so F1's phrase names this reader's
schema-validation stage rather than requiring a JSON Schema document. Reading it
as the latter would make the clause unsatisfiable by construction, which is the
ED-0041 shape in specification form. The stage is real and already doing this
work: `internal/persist/errors.go:76` defines `StageSchema`, and
`inspect.go:323` already rejects a malformed frontier counter there with
`CatInvalidFieldSyntax` — the same field, the same site.

**This unblocks issue 0028**, which can now author a hostile fixture for the
zero-member prohibition carrying ONE expected token as §9.6 requires.

**Task 2 — two classes ruled UNREACHABLE, and nothing deleted.**
`ambiguous-manifest` is unreachable under the v0.1 closed-namespace profile, and
L29 repeated-physical-blob tolerance is unreachable given injective blob mapping
plus duplicate-logical-name rejection. Both confirm findings issue 0028's review
routed here on 2026-08-25.

**A destructive change was stopped in the working tree before commit
(ED-0079).** The pair had begun deleting `CatAmbiguousManifest` from
`internal/persist/errors.go`. That would not have compiled —
`reader.go:229` uses it — and would have orphaned the `ambiguous-manifest.domo`
fixture, its generator, a coverage-set entry and an `EXPECTATIONS.md` gap
record, all owned by issue 0028. It is also a §9.4 taxonomy change, made in a
round scoped to ruling specification and routing implementation. **"This rule
cannot fire" and "this rule should not exist" are different claims with
different owners.** The finding stands; the token stands; the removal question
is routed to the §9.4 and issue 0028 owners with the dependent artifacts named.

Status remains `In Progress`: this issue's other criteria and its upstream gates
are unchanged.

## `label.name` required-vs-optional routed here 2026-09-12 (issue 0010)

FORMAT-SPEC §2.3's `label` row contradicts itself. It lists `name` under
**Req:**, while the same cell's parenthetical says "absent vs empty-string
distinct authoritatively, identical in display". A field that can be absent is
not required, so the two halves of one sentence disagree.

Issue 0010 implemented the Tier B envelope and needed a wire shape, so it typed
`name` as `Option<String>`, matching `roomRecord.name` and the parenthetical.
That is provisional and is marked as such in the source. It is NOT a ruling,
and issue 0010 makes no claim that both readings hold.

What resolving it costs either way, so the decision is cheap to take:

- **Optional** confirms the shipped shape; the row's `Req:` placement needs
  correcting so the table stops contradicting itself.
- **Required** makes `Option<String>` wrong: the field becomes `String`,
  validation gains a presence check, and any package written under the
  provisional reading with `name` absent becomes invalid, which is a
  compatibility question rather than a typo fix.

The second outcome is why this is worth deciding before the envelope is used,
rather than after.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** The §7 container profile, §7.6 batch framing and §9.7 save machine are implemented and exercised. What remains is fixation and the compression-method decisions §7.7 defers.
