# 0035: Measure size corpus and record encoding benchmarks

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0004 (logical operation/batch metadata), 0005 (physical batch framing + container compression fixation), 0026 (writer/tooling)
- **Deferred-activation gate** (not a dependency edge; README "Partial-scope dependency semantics"): furnished-corpus activation additionally requires BEHAVIOR-SPEC §10 acceptance + 0004

## Current disposition — 2026-10-08

Keep partial for furnished/history scenarios and encoding measurements.

Part A measurements exist and the room-record increment remeasured the full
unfurnished corpus. Do not use earlier sizes as today's result or overwrite
named-host evidence. Activate furnished scenarios only against reviewed §10
semantics; produce Part B and candidate encoding/quality/decode/memory records
before promotion. Existing furniture import does not by itself satisfy the
corpus gate.

BEHAVIOR-SPEC §10 now defines normative experimental-editor import,
placement, preview and resizing. Its former "undefined" description is
historical. Furnished-corpus activation still needs 0004 to reconcile those
rules with FORMAT-SPEC's provisional Tier B envelopes and the corpus contract;
working import alone does not activate the corpus or waive its criteria.
Issue 0018 already declares its 0035 dependency; the historical request
to add that publication edge is discharged.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Issue 0021 delivered the normative corpus definitions, revised
preview-inclusive estimates, and the encoding promotion gate under partial
scope. Actual measurements and benchmark records do not exist yet and are
excluded from 0021's closure. This issue owns them.

## Scope

- Implement the `empty-v1` / `unfurnished-1000-v1` generator per
  ASSET-STRATEGY §10.9 (Part A) with its normative self-check.
- Produce measured thin-package sizes; activate the dormant furnished
  scenario only after §10 + 0004 acceptance.
- Produce encoding benchmark records behind the promotion gate.

Excluded: any change to normative ASSET-STRATEGY/FORMAT-SPEC semantics;
retrieval-limit values (issue 0023); conformance fixtures (issue 0028).

## Acceptance criteria

- [x] Measured-budget record per scenario containing: headline package size
      in bytes **including** preview bytes (strict pass ⇔ measured <
      target KiB × 1024), per-entry-class breakdown (manifest, descriptors,
      snapshot+history reported separately from Part A content, preview
      line item), external/remote byte totals excluded from the headline,
      writer/container/compression versions, generator version, OS/
      filesystem, committed raw machine-readable outputs, one documented
      reproduction command; measured ≥ target ⇒ mandatory recorded budget
      revision, never silent pass.
- [x] Preview-record inventories per scenario: thumbnail digests, byte
      counts, dimensions, encoder identity (completing AC-6 records).
- [ ] Encoding benchmark record per candidate containing: tool name/version/
      source URL, license identifier + notice obligations, adapted-vs-
      independent provenance, patent-risk review flag (mandatory for Basis
      Universal), named reference environment (CPU/RAM/OS/build mode),
      encoded and decoded sizes per fixed quality tier, decode-cost p50/p95
      and peak memory, corpus input digests, committed raw outputs,
      pre-registered failure thresholds. No candidate becomes a format
      requirement without a complete record.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**

## Publication handoff (explicit non-dependency)

Measured budgets are published through issue 0018's existing acceptance
criterion ("Size corpus and measured thin-package budgets are published").
0018's `Depends on:` list was deliberately not modified — that file is
outside this issue's ownership, and adding the edge here would silently
re-scope another issue. This is a documented publication handoff, not a
formal dependency edge; flagged for orchestrator attention as a known
tracker-graph follow-up (0018 may declare `Depends on: 0035` when its
scope is next touched by its own owner).

## Activation (2026-08-25)

Activated under ED-0024 on branch `issue/0035-size-benchmarks`, consuming the
accepted provisional handoffs from 0004, 0005, and 0026. Scope is Part A:
`empty-v1`, `unfurnished-1000-v1`, their normative self-checks, measured thin
packages, preview inventories, raw records, and policy-compliant encoding
candidate benchmarks. `furnished-cached-v1` remains dormant because
BEHAVIOR-SPEC §10 is still undefined; no furniture/instance semantics may be
invented. No Done while the upstream issues remain unclosed or the furnished
gate remains unresolved.

ED-0025 provisionally resolves the two Part-A specification ambiguities:
openings use 39 flattened ring instances with four edge-middle doors each,
and `S0` is a generator-local self-check vocabulary with no wire registry.
Issue 0021 retains the normative amendment debt; contrary owner text requires
regenerating all affected corpus and measurement records.

## Partial handoff accepted (2026-08-25)

Branch `issue/0035-size-benchmarks` (head `281ac5a`) merged to main by
`cf718cd` with SBOM refresh `f1fda52`; full main `./scripts/verify.sh` exits 0.
Delivered deterministic `empty-v1` and `unfurnished-1000-v1` generators,
independent identity/geometry/style self-checks, byte-identical writer
regeneration, committed thin packages and raw environment outputs, separate
preview inventories, and preview-inclusive measured budgets. Results on the
named environment are 3,597 B `<` 16 KiB and 119,394 B `<` 256 KiB.
Reviewer verdict: accepted Part-A size/preview handoff after resolving digest
schema, per-entry method reporting, preview rows, self-check depth/cardinality,
and the independent identity oracle.

This issue remains `In Progress`. All five encoding candidates are honestly
recorded `not-measured`, so the per-candidate metrics criterion remains open;
the furnished scenario remains dormant; ED-0025 awaits issue 0021's normative
amendment; and ED-0024's upstream closure gates remain. Issue 0018 may consume
the measured-budget records as a labeled provisional publication handoff and
must declare the documented 0035 dependency when activated.

## Continuation activated 2026-08-26 (ED-0031): host-portable evidence gate

The repository moved to a macOS/arm64 host. `TestCommittedArtifactsConsistent`
now false-fails there: its regeneration comparison is guarded only on
`GOTOOLCHAIN=local`, not on the Linux/amd64 + Python 3.14.7 + zlib 1.3.2
reference environment the committed records themselves name. Packages and
their digests still regenerate byte-identically, so the measured budgets are
intact; only the environment-provenance records diverge.

Continuation on branch `issue/0035-verification-host-portability`: scope the
gate to the reference environment its records name, so it passes on a
conforming host and degrades loudly with a named reason otherwise. Committed
evidence bytes must not change. See ED-0030 for the recorded baseline and
ED-0031 for the prohibitions and closure gate. This issue remains
`In Progress`; its Part-B, encoding-candidate, and upstream gates are
untouched.

## Continuation handoff accepted 2026-08-26 (ED-0031)

Branch `issue/0035-verification-host-portability` (`d07cbf6`..`fdb5ac2`, base
`b51dd50`) merged to main by `cdf887a` with SBOM refresh `4f29102`. **Full main
`./scripts/verify.sh` exits 0** — the first clean run on the macOS host and the
closure of ED-0030's recorded baseline exception.

Delivered: the committed-artifact gate is scoped to the reference environment
its own records name. The environment predicate is parsed out of the committed
records, and `TestRecordedIdentityParsesAndConforms` cross-checks that parse
against `recordedHostIdentity()`, a reviewed constant restating the Linux
fixture in full — so the gate cannot drift from the evidence, and an
evidence-only coordinated rewrite fails a test. A conforming host compares
exactly what it compared before; a non-conforming host omits byte comparison
for exactly the seven environment-bearing records while naming every divergent
fact and every omitted path, with no `t.Skip` anywhere. Unconditional and
unchanged: package digest and byte count, package byte-reproducibility, the
host-independent candidate record, seed literals, the pinned §10.9 thumbnail
vector, and the round-trip test. `GOTOOLCHAIN=local` remains a separate hard
prerequisite.

Diff is 6 files, test-only plus the SPDX documents;
`git diff b51dd50..fdb5ac2 -- docs/` is empty, so no evidence byte, budget,
digest, or pinned literal changed. Four genuine-defect demonstrations were
recorded and restored, including a corrupted preview-inventory field that
would previously have gone undetected on this host.

Adversarial review (r0035, `openai/gpt-5.6-sol` at max effort): **ACCEPT**. It
rejected the worker's first design as a fail-open masking surface, then raised
two blocking findings — permissive raw-field parsing, and misuse of
`dec.More` as a complete-document check, under which a record followed by a
second JSON document was silently accepted — both mutation-proven live
fail-opens, not style points. The worker additionally self-disclosed two more
defects in the same class before committing. No finding was dropped.

This issue remains `In Progress`. Untouched and open: Part B, all five
`not-measured` encoding candidates, the dormant furnished scenario,
ED-0025's pending issue 0021 amendment, and ED-0024's upstream gates.

## Review round integrated 2026-08-31 (ED-0069)

Branch `issue/0035-size-measurement` (`a090e56`..`da39a66`) merged by `6ce2979`
with SBOM refresh `7abd616`; full main `./scripts/verify.sh` exits 0 with a
clean tree.

**No measurement was redone and no evidence byte changed.** The round's whole
tracked delivery is the acceptance-criteria state: AC-1 and AC-2 marked met,
AC-3 left OPEN.

- **AC-1 and AC-2 met.** The orchestrator verified each required field against
  the artifacts rather than against the verdict: headline bytes including
  preview, per-entry-class breakdown, excluded external totals, the `versions`
  block (writer seam, container profile, compression library), generator
  version, environment, and committed raw outputs; the reproduction command the
  records do not carry in JSON is documented at
  `docs/evidence/0035-size-corpus/README.md:31-35`. Both committed packages were
  independently recomputed on this host and match their recorded size and digest
  exactly (3,597 B and 119,394 B).
- **AC-3 stays OPEN, and the first verdict had it wrong.** `encoding-candidates.json`
  records every candidate as `not-measured` with `patentRiskReview: not-performed`
  — which the criterion calls MANDATORY for Basis Universal — and carries a
  description of what a complete record would require rather than the required
  fields themselves. The record is honest and unchanged; the checkbox above it
  was not. **A file documenting the absence of the required content is not the
  required content.** AC-3 is gated on the Part-A corpus having zero assets to
  encode (ASSET-STRATEGY §10.9) and therefore on issues 0021 and 0026, plus the
  independent Basis Universal patent-risk obligation.
- **Two review failures recorded for reuse.** The reviewer certified "ED-0070
  compliance delivered" from the PRESENCE of populated `environment` fields
  without checking what they named; and AC-3 was certified from file presence
  rather than criterion content. Both are the ED-0041 shape occurring in a
  reviewer's assertions rather than in a test.
- **An orchestrator error, recorded as ED-0070 amendment (b).** ED-0070 declared
  the current session's host the measurement authority. That CONTRADICTED
  ED-0031's shipped design, which correctly scopes the gate to the environment
  the records themselves name, and it was written without reading ED-0031 or
  this file's own §102-129. The linux/amd64 provenance was never undocumented.
  What survives is the host-independent versus host-dependent classification.

Status remains `In Progress`: AC-3 and the upstream gates are open.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** Part A measured and published 2026-09-01: empty-v1 at 463 B and unfurnished-1000-v1 at 18.9 KiB, both inside target, with the estimate's compression assumption corrected from 2.0-2.5x to a measured 9.0x. NOT delivered: the dormant furnished scenario, Part B history framing, and the encoding benchmarks behind the promotion gate.
