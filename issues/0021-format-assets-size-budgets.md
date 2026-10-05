# 0021: Specify assets, package profiles, and size budgets

- **Phase:** 0
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0009
- **Deferred-closure gates** (exhaustive; not upstream dependencies;
  semantics in README "Partial-scope dependency semantics"): 0004 — logical
  operation/batch metadata (identity, counters, frontiers) gating Part-B
  history records/AC-6; 0005 — physical operation-batch naming/order/
  framing and container compression settings gating AC-6; BEHAVIOR-SPEC
  §10 acceptance + 0004 — furnished-corpus activation gating AC-5; 0028 —
  adversarial fixtures gating
  AC-4 conformance; 0035 — measurements, preview-record inventories, and
  encoding benchmark records gating AC-5 measurement/AC-6/AC-7. Publication
  path: 0035 results are published through 0018's existing acceptance
  criterion as an explicit non-dependency handoff (see Partial-scope guard).

## Problem

Turn `../docs/ASSET-STRATEGY.md` into normative v0.1 descriptors and measured
budgets. Keep asset identity, entity IDs, storage locations, and logical asset
instances distinct.

## Acceptance criteria

Disposition per the agreed outcome contract (Rev 9, 2026-08-23): AC-1–AC-4
(structure) and AC-8 delivered in `docs/ASSET-STRATEGY.md` §10 and
`docs/FORMAT-SPEC.md` §6; AC-4 values were resolved by issue 0023 and
conformance remains gated on issue 0028 fixtures; AC-5 partial (Part A only); AC-6 open; AC-7 deferred
to issue 0035. No criterion is checked at partial-scope start.

- [ ] AC-1 — Digest algorithm/encoding, exact hashed bytes, descriptor fields,
      blob path mapping, media type, byte length, required/optional,
      provenance, license metadata: **delivered** (ASSET-STRATEGY §10.1–10.3,
      §10.10); partial-scope handoff approved; full criterion remains open
      under the guard below.
- [ ] AC-2 — Thin/portable semantics, cache lookup, remote hints, missing
      assets, deduplication, dangling refs (incl. descriptorless rule),
      unknown-data preservation, fail-closed GC: **delivered**
      (ASSET-STRATEGY §10.4–10.7); partial-scope handoff approved; full
      criterion remains open under the guard below.
- [ ] AC-3 — Portable completeness over U(P) or fail; asset-root table
      invariants I1–I4 (wire shape deferred to issue 0005): **delivered**
      (ASSET-STRATEGY §10.4–10.5); partial-scope handoff approved; full
      criterion remains open under the guard below.
- [ ] AC-4 — Remote retrieval policy (consent, schemes, redirects, private
      addresses, ambient credentials, transfer/time limits, digest-before-
      decode/admission): **specification complete** (ASSET-STRATEGY §10.8);
      issue 0023's ratified L24–L28 values are incorporated; completion is
      gated only on issue 0028 fixtures.
- [ ] AC-5 — Representative corpus + revised targets: **partial — Part A
      only** (`empty-v1`, `unfurnished-1000-v1` definitions + preview-
      inclusive revised estimates, ASSET-STRATEGY §10.9); furnished corpus
      dormant (BEHAVIOR-SPEC §10 + 0004); measurement delegated to issue
      0035 → 0018.
- [ ] AC-6 — Corpus records (history/compression/previews; history cost
      separate): **Open** — history framing contingent on ADR 0004;
      compression fixation on issue 0005; preview-record inventories via
      issue 0035. Template and contingent-clause structure delivered.
- [ ] AC-7 — Encoding benchmarks before requirements: **Deferred to issue
      0035** with full successor record criteria; promotion gate delivered
      (ASSET-STRATEGY §10.9).
- [ ] AC-8 — Splats and raw reconstruction photos excluded from v0.1;
      future chunking/LOD documented non-normatively: **delivered**
      (ASSET-STRATEGY §10.9 exclusions).

## Scope

Delivered under partial scope: normative digest identity/hash domain,
descriptor schema, logical blob-path mapping constraints, thin/portable
profile semantics, referenced-blob universe U(P), asset-root table
invariants I1–I4, preview class, cache GC pins/leases (fail-closed), remote
retrieval policy structure, corpus Part A definitions with pinned literals,
encoding promotion gate, splat/photo exclusions, provenance records.

Explicitly out of scope (owned elsewhere): physical container/entry names
(0005); implementation (0008/0026); hostile
fixtures (0028); measurements and benchmark runs (0035 → 0018); splats;
reconstruction pipelines; asset-instance placement semantics (BEHAVIOR-SPEC
§10); MIME/app IDs (NAMING.md Deferred-by-rule). All delivered material is
labeled "Provisional handoff — 0021 partial scope"; dependents consume only
that labeled material.

## Partial-scope guard (binding; supersedes any external draft)

The working contract drafts live outside both repositories
(`/tmp/opencode/issue-0021-outcome-contract-v9.md`, reviewer-agreed
2026-08-23) and are NON-authoritative working records. The binding scope
and guard are the clauses below plus the delivered documents themselves:

- **Scope of record:** ASSET-STRATEGY §10 (digest identity/hash domain;
  descriptor schema; logical blob-path constraints; thin/portable
  profiles; U(P) and asset-root invariants I1–I4; preview class; GC
  pins/leases, fail-closed; retrieval policy structure; corpus Part A with
  pinned literals; encoding promotion gate; splat/photo exclusions;
  provenance records) and FORMAT-SPEC §6/§9 pointers.
- **G4 closure gates:** Part-B history framing ratified (0004 logical);
  physical batch framing + compression fixed (0005); furnished corpus
  activated (BEHAVIOR-SPEC §10 + 0004); measurements, preview-record
  inventories, and encoding benchmark records delivered (0035 → 0018);
  AC-4 conformance fixtures present and passing (0028).
- **Prohibitions:** no `Done` status; no criterion checkoff; no tracker
  `done/` move; no blanket closure — any future closure must list the
  resolving commit/issue per deferred criterion.
- **Handoff labeling:** all delivered material is labeled "Provisional
  handoff — 0021 partial scope"; dependents consume only that labeled
  material.

**Publication handoff (explicit non-dependency):** issue 0035's measured
budgets are published through issue 0018's existing acceptance criterion
("Size corpus and measured thin-package budgets are published"); 0018's
dependency list was deliberately NOT modified because that file is outside
this issue's ownership — flagged here for orchestrator attention as a
known tracker-graph follow-up.

## Closure

- **Implementation/spec commits:** main `ae0d7be`
  (`docs(issue-0021): specify asset and package strategy`); this tracker
  commit records the reviewed partial-scope handoff.
- **Review:** adversarial full-diff review and orchestrator verification
  approved the partial-scope handoff on 2026-08-23. This is not issue
  closure; every deferred gate and unchecked criterion remains open.
- **2026-08-24 continuation:** main commits `3f17a28` and `e8073b7`, merged
  with current main by `d007174` and refreshed by `ea05e96`, incorporate
  issue 0023's ratified retrieval limits into the AC-4 ledger. Paired review
  and final main-tree verification passed. The former 0023 gate is resolved;
  AC-4 now waits only on issue 0028 conformance fixtures.
- **Follow-up issues:** 0035 (size-corpus measurement and encoding
  benchmark records; owns AC-5 measurement, AC-6 preview/history records,
  AC-7 benchmark records).
- **2026-08-25 clarification debt:** ED-0025 provisionally resolves two
  internally inconsistent Part-A corpus statements so 0035 can measure: 39
  flattened ring instances each receive four edge-middle doors (156 total),
  and `S0` is a generator-local vocabulary rather than an invented wire
  registry. Issue 0021 must amend ASSET-STRATEGY §10.9 to state or replace
  those rules before closure; contrary owner text requires corpus regeneration.

## ED-0025 clarification debt DISCHARGED 2026-08-31 (ED-0069)

Branch `issue/0021-asset-strategy-amendment` (`028507b`, `b591411`) merged with
SBOM refresh; full main `./scripts/verify.sh` exits 0 with a clean tree.

ED-0025 had PROVISIONALLY resolved two internally inconsistent Part-A corpus
statements so that issue 0035 could measure at all, and this issue's tracker
required §10.9 to "state or replace those rules before closure". Both are now
stated normatively in `docs/ASSET-STRATEGY.md` §10.9 rather than living only in
a decision record:

- **Openings.** Ring instances are numbered globally in level-major/ring-major
  order, `g = 10·(ℓ-1) + r` for levels ℓ ∈ {1…4} and rings r ∈ {0…9}. Instances
  `g ∈ {0…38}` — the first 39 of 40 — each receive exactly four doors, one on
  the middle sub-wall of each edge in the order bottom, right, top, left, giving
  **156 non-overlapping openings**. Instance `g = 39` (ring 9 on Level 4)
  receives none. The amendment states the ENUMERATION, not just the total, so a
  regenerated corpus is reproducible rather than merely the same size.
- **Style vocabulary.** `S0` is exactly one generator-local token used for
  corpus self-check. Walls and openings serialize `styleRefs:["S0"]`; room style
  references stay absent; the generator validates every serialized token against
  the local vocabulary. **No wire-level registry field, manifest entry, or cache
  object is added**, and the amendment says why: FORMAT-SPEC §2.3–2.4 permit no
  authoritative document-level registry.

**Consequence for issue 0035.** Its committed evidence records carry
`"specBasis": {"provisionalPending0021Amendment": true}`. The provisional basis
those records name is now stated normatively. Whether that flag may be cleared
is issue 0035's to decide — ED-0031 forbids regenerating or rewriting those
records, so the flag stands until that issue rules on it. It is NOT cleared by
this merge.

Status remains `In Progress (partial scope)`: AC-1 through AC-8 remain open
under their recorded guards, AC-4 still waits on issue 0028 conformance
fixtures, and AC-5/AC-6/AC-7 remain with issue 0035.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** The normative corpus definitions were delivered here; their MEASUREMENTS landed under issue 0035 on 2026-09-01 and corrected this issue's compression assumption from 2.0-2.5x to a measured 9.0x.
