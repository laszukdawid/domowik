# 0010: Implement entity projection

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0004, 0037

## Current disposition — 2026-10-05

Keep partial for complete schema/property conformance, not for missing room or
opening features.

`crates/domowik-format/src/lib.rs` now includes RoomRecord, AssetInstance, and
Opening's depth/side/hinge/opensToward/swing fields;
`tests/entity_properties.rs` supplies property coverage. Reconcile the
original checklist with these artifacts and remaining Tier B/unknown-data
rules under 0004 before closure. Earlier missing-entity claims are historical.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

Activated 2026-08-24 under ED-0016 on branch `issue/0010-entity-projection`.
Consumes FORMAT-SPEC §2–§5 as integrated on main (normative, testable,
accepted) plus the Done behavior specifications. Numeric values are consumed
only via the named provisional parameters; no independent literals; RATIFICATION
§6 deferrals untouched. This issue cannot reach Done while 0004 remains
unclosed under ED-0014 gates, except by explicit re-scope.

## Remaining acceptance work — 2026-10-08

The original checked criteria below describe delivered scope and dated
evidence. These later residuals explain the current nonterminal status.

- [ ] Reconcile complete schema/property coverage, provisional Tier B promotion
      and unknown-data preservation with issue 0004 before full closure.

## Original problem

Implement immutable/current entity projection and type schemas without a
public direct-mutation API. An internal apply primitive may support 0011; file
persistence belongs to 0029.

## Acceptance criteria

- [x] Typed schemas and reference validation match the normative v0.1 logical
      model.
- [x] Projection changes only through the internal operation/batch apply seam.
- [x] Invariants are checked at their specified per-op or atomic-batch boundary.
- [x] Property tests cover random valid/invalid entity sets and deterministic
      projection.
- [x] Unknown optional data follows the preservation/read-only rules.

## Partial handoff accepted (2026-08-24)

Implemented on branch `issue/0010-entity-projection` (commits `812262b` ..
`754d0c9`), merged to main with an SBOM refresh; full main
`./scripts/verify.sh` exits 0. Adversarial reviewer APPROVED after two fix
rounds (same-actor duplicate-create atomic rejection with regression
coverage); orchestrator diff/provenance review confirmed named-parameter
discipline (`params.go` sole definition point; `lwwHorizon` deliberately
unfixed with documented unbounded retention) and zero dependency drift.
Open items: lwwHorizon fixation owned here; at-least-one-level invariant
deferred to issue 0011's command layer. Per ED-0016 this issue remains
`In Progress` until issue 0004 closes under ED-0014 gates or an explicit
re-scope records otherwise; dependents consume this delivered material as a
labeled provisional handoff under the tracker's partial-scope semantics.

## lwwHorizon fixation analysis (2026-08-31)

[docs/LWWHORIZON-FIXATION-ANALYSIS.md](../docs/LWWHORIZON-FIXATION-ANALYSIS.md)
assembles the technical inputs needed to decide the `lwwHorizon` value: what
the horizon governs (with FORMAT-SPEC clauses), cost curves from issue 0035
measurements, failure modes at each extreme, and candidate values with
trade-offs. The analysis does NOT recommend a value — that decision remains
owner-owned. `LwwHorizonRetention` in `internal/document/params.go` remains
`"unbounded-pending-lwwHorizon-fixation"` and no value is asserted in
FORMAT-SPEC.

## lwwHorizon decision inputs integrated 2026-08-31 (ED-0069)

Branch `issue/0010-lwwhorizon-analysis` merged; full main
`./scripts/verify.sh` exits 0 with a clean tree.

**The value is NOT fixed and this round did not fix it.**
`internal/document/params.go:50` still reads
`LwwHorizonRetention = "unbounded-pending-lwwHorizon-fixation"`, and
FORMAT-SPEC §5.1 S3 still records the parameter as Provisional with no value
asserted. That was the constraint on the dispatch: a retention horizon decides
how long a document can resolve a late concurrent write, and therefore what a
user loses when it lapses — a product decision belonging to the owner.

**What was delivered is the decision itself, made takeable.** Before this
round nobody could have fixed the value if they had wanted to, because the
inputs had never been assembled. `docs/LWWHORIZON-FIXATION-ANALYSIS.md` now
records what the horizon governs with its FORMAT-SPEC clauses, the retained-byte
cost curve drawn from issue 0035's committed host-independent measurements, the
failure mode at each end — unbounded retention is the CURRENT behaviour, so its
cost is measurable rather than hypothetical — and candidate values presented as
trade-offs rather than as a ranking.

**A convention fix during review.** The analysis first landed at
`issues/0010-lwwhorizon-fixation-analysis.md`, which put two `0010-` files in
the tracker against `issues/README.md:19` ("One issue per `NNNN-slug.md`; IDs
are unique"). It carried no `Status` line, so a glob over `issues/0*.md` would
have returned it as a malformed issue. Relocated to `docs/` with a pointer from
this file. Same content, correct home.

Status remains `In Progress`: the `lwwHorizon` fixation is open and now
owner-ready, and the ED-0016 gate on issue 0004 is unchanged.

## Rust implementation — 2026-08-31

Delivered by `crates/domowik-format` and `crates/domowik-document`.

- Typed schemas for every v0.1 entity (`level`, `wall`, `opening`, `wallJoin`)
  with the §2.3 `type` discriminator, and `Document::validate` reporting EVERY
  violation rather than the first.
- **No public direct-mutation API.** `Session` hands out `&Document` and
  `&Projection`; the only paths to state are commit, undo, redo, and accepting
  a batch. A test pins it.
- Projection changes only through the operation/batch apply seam
  (`replay::project_from`).
- Invariants are checked at the atomic-batch boundary: a command validates the
  document it WOULD produce before its batch enters the log, because replay has
  no way to decline a batch afterwards.

### Still open

- ~~**Unknown optional data preservation.**~~ **Delivered 2026-08-31.** Every
  entity carries a flattened `unknown` map, so optional fields this version
  cannot interpret survive a load-and-save round trip by VALUE, which is the
  first branch of FORMAT-SPEC §8 U2. The map participates in `PartialEq`, so
  dropping unknown data is observable rather than silent. Not delivered: U2's
  SECOND branch — a `unknown-data-preservation` write-back refusal — is
  unreachable because preservation always succeeds; and L32's cumulative
  retention bound is not enforced, so a hostile package could carry arbitrary
  unknown data into memory.
- The `lwwHorizon` fixation and the at-least-one-level invariant recorded above
  remain open.

## Status corrected by ED-0096 — 2026-09-01

**In Progress (partial scope).** Projection, the apply seam, unknown-optional preservation and the atomic batch boundary are delivered — and the boundary was only made real on 2026-09-01, when `accept_external` started validating like `commit` already did. Unknown-descriptor preservation on `Anchor` and `BackgroundImage` was added the same day; §8 U2 forbids silent stripping "everywhere", not only on entities. NOT delivered: the typed schema is a SUBSET of the normative v0.1 logical model — no `roomRecord`, and `Opening` lacks `depthMm`, `side`, `hinge`, `opensToward` and `swingDeg` — and the property coverage over random valid/invalid entity sets is example-based plus one pseudo-random command stream, which is not the same thing.

## Bit preservation: serde_json `float_roundtrip` — 2026-09-12

Found by this issue's new property coverage on its first run, against the
fourth acceptance criterion. Recorded here rather than in messages.

**The defect.** serde_json's parser is not correctly rounded unless the
`float_roundtrip` feature is on; it is off by default because it is slower.
A value whose shortest decimal form does not re-parse exactly returns one ULP
away, so write-then-read is not the identity — which FORMAT-SPEC §3.2
requires of a binary64 field, and which §9.6 leans on when it promises a
deterministic reopen. Rust's own `str::parse::<f64>` returns 0 failures on
the identical text, so the writer was never at fault.

Witness: `0x3ff23d70a3d70a3e` writes as `1.1400000000000001` and reads back
as `0x3ff23d70a3d70a3d`.

**What it costs, and what it does not.** The first version of this record
said "roughly one coordinate in six moved on every save-and-reopen". That was
wrong twice over, and the correction came from measuring rather than
rewording:

| sample | mismatches |
|---|---|
| plan-like values — grid and half-millimetre, across the domain | **0 of 7,241** |
| arbitrary bit patterns | 31,179 of 200,000 |

The second figure is a property of that SAMPLE, not a frequency over real
documents: a bit-pattern walk produces full-mantissa values, and short
decimals re-parse exactly under either parser. The real exposure is values
that ARITHMETIC produced and then stored — a snapped point, a split point —
not anything a person typed.

And the hop is **one-time, not cumulative**: of the 31,179 that moved, all
31,179 settled on the second round trip and none moved again. This is a
broken identity, not a value that walks away with each save.

**Not a version change.** Both Cargo.lock files are byte-identical before and
after, `docs/RUST-DEPENDENCIES.md` regenerates with no diff, and `NOTICE`
regenerates with no diff. Features add no crates.

**Which workspace governs, measured rather than assumed.** The spike is a
separate Cargo workspace, so the expectation was that the root workspace's
selection could not reach it. That is false — the spike depends on the core
crates by path, they declare serde_json with `workspace = true`, and Cargo
unifies features across one build graph:

| root | spike | result |
|---|---|---|
| on | on | pass |
| on | off | pass |
| off | on | pass |
| off | off | **fail** |

Either line suffices today. Both are set anyway: the spike is the build
behind the FFI save-and-open path a user's file travels, and it should state
its own requirement rather than inherit one silently from a graph whose
existence is easy to miss.

**Regressions, unignored.** `entity_properties::every_document_round_trips_`
`through_json_unchanged` in the root workspace, and two in the spike — an
end-to-end save/reopen asserted on BITS, and a direct parser witness so a
failure says which half broke. Verified failing with the feature off in both
workspaces.

`unbounded_depth` in the root workspace belongs to issue 0004 and is kept
alongside so neither feature is lost in a merge resolution. It only EXPOSES
`Deserializer::disable_recursion_limit()`; enabling it changes no behaviour
on its own, and the guarded call that uses it to enforce the §9.3 row L11
nesting bound is **not on this branch** — it lands with 0004. The feature is
declared here, the guard is pending there.
