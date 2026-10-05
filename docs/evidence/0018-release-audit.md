# Issue 0018 Release-Candidate Audit

Status: **Partial handoff verdict.** This is audit evidence under ED-0026, not
a release, tag, publication, compatibility approval, or Done claim.

## Review gate

The following matrix is machine-checkable in the sense that every row has an
explicit status, evidence reference, and typed assertion. `SKIP` is not a
status. A blocked or mismatching record remains a blocker and is routed to its
owner; fixture counts and prose do not substitute for semantic assertions.

```json
[
  {"row":"E","scope":"8 entity kinds","status":"BLOCKED","assertions":["kind","required-fields","optional-fields","semantic round-trip"],"evidence":"testdata/corpus/EXPECTATIONS.md; internal/persist/corpus_semantic_test.go","blocker":"Tier-B behavior is structural only; golden checks do not assert every field semantic"},
  {"row":"O","scope":"createEntity,setFields,deleteEntity","status":"BLOCKED","assertions":["effect","whole-batch atomicity","tombstone","unknown-kind refusal"],"evidence":"testdata/corpus/EXPECTATIONS.md; internal/document/*_test.go","blocker":"corpus vector harness contains skipped/incomplete semantic paths"},
  {"row":"A","scope":"thin,portable,duplicate,corrupt,missing,orphan","status":"BLOCKED","assertions":["U(P)","embedding","complete omissions","digest","GC safety"],"evidence":"internal/cli/asset_pack_test.go; internal/assets/*_test.go","blocker":"duplicate physical blob and orphan crash-consistency coverage is not a complete release matrix"},
  {"row":"U","scope":"unknown optional/required data and newer schema","status":"BLOCKED","assertions":["preserve or safe-write-refusal","pre-effect unsupported-version","foreign namespace refusal","unknown entity/op/descriptor paths"],"evidence":"internal/persist/unknown_data_test.go; internal/migration/migration_test.go","blocker":"existing checks prove selected optional/entity and schema-label paths, not the complete entity/operation/descriptor unknown-data matrix"},
  {"row":"M","scope":"evidenced predecessor set and migration","status":"BLOCKED","assertions":["public-history evidence","no fabricated positive migration","backup/source preservation"],"evidence":"internal/migration/migration.go; internal/migration/migration_test.go","blocker":"0031 remains In Progress and the predecessor/public-history record is not an independent release artifact"},
  {"row":"R","scope":"backup,validation-before-replace,fault phases","status":"BLOCKED","assertions":["source/target bytes","leftover policy","F1-F9 outcomes"],"evidence":"internal/migration/migration_test.go; docs/FORMAT-SPEC.md §9.7","blocker":"tested hooks cover only part of the normative platform/recovery matrix"},
  {"row":"S","scope":"0035 Part A measured thin packages","status":"PASS","assertions":["direct record values and strict budgets","preview-inclusive accounting","raw-output provenance"],"evidence":"docs/evidence/0035-size-corpus/budget-empty-v1.json; docs/evidence/0035-size-corpus/budget-unfurnished-1000-v1.json"},
  {"row":"H","scope":"hostile container, JSON, framing, assets","status":"BLOCKED","assertions":["exact category","zero effect","corruption/missing/orphan behavior"],"evidence":"internal/persist/hostile_container_test.go; internal/persist/corpus_framing_test.go","blocker":"three hostile-container records and duplicate-key/framing cases are skipped or mismatch"},
  {"row":"P","scope":"v0.1 wording and ADR 0007","status":"BLOCKED","assertions":["exact promise","ADR Proposed","no release/publication claim"],"evidence":"docs/decisions/0007-format-compatibility-promise.md","blocker":"ADR approval and all dependency closure gates remain open"}
]
```

## v0.1 kind inventory

The complete entity set is `level`, `wall`, `wallJoin`, `opening`,
`roomRecord`, `label`, `dimension`, and `assetInstance`. Gaussian Splats are
excluded. The complete operation set is `createEntity`, `setFields`, and
`deleteEntity`. `setFields` replaces set/array fields wholesale; delete keeps
tombstones; unknown operation kinds reject the containing batch.

The acceptance-category set from FORMAT-SPEC §9.6 is: `reject`, `warning`,
`degraded read-only inspection`, `missing-asset equivalence`, and `safe write
refusal`. Ordinary success is additionally needed for valid golden records.
Each category needs an outcome assertion, not just an artifact count.

## Findings and routing

- `v2-reordered-delivery` is skipped for an unresolved semantic conflict; it is
  not coverage.
- V7 records both intended orderings in metadata, but the test executes only
  one effective projection; it is not proof of both outcomes.
- V6 can skip on oracle shape and then checks only non-empty state, not equality
  with full replay.
- Hostile-container `missing-manifest`, `spanned-disk-number`, and
  `ratio-under-L15-success-twin` remain explicit corpus-defect skips. The JSON
  duplicate-key fixture is also listed as an open question while its bytes do
  not contain duplicate keys.
- The migration implementation correctly has an empty production chain. No
  positive migration is claimed. The only predecessor is `0.1`, and the
  repository supplies no independent public-release history for an earlier
  schema, so the predecessor set is empty for this audit.
- Asset tests cover thin/portable, missing, and corrupt omission behavior, but
  do not establish the complete duplicate/orphan/crash-consistency matrix.

## 0035 measured evidence

The audit directly consumes the measured records, rather than restating a
fixture count: `empty-v1` is **3,597 B < 16,384 B** and
`unfurnished-1000-v1` is **119,394 B < 262,144 B**, both preview-inclusive
strict passes on the named environment. History is explicitly zero by Part A
construction. `encoding-candidates.json` keeps WebP, AVIF, KTX2/Basis,
glTF/GLB + meshoptimizer, and font subsetting `not-measured`; no candidate is
promoted. `furnished-cached-v1` remains dormant and is not covered.

## Verdict

**ED-0026 partial handoff: NOT RELEASE-QUALIFYING.** The measured Part A size
handoff and the migration framework are consumable as provisional evidence.
Corpus semantic gaps, skipped/mismatching hostile records, incomplete asset and
recovery matrices, unresolved upstream dependencies, and ADR approval remain
open. Route these gaps to issues 0026, 0028, 0029, and 0031 as applicable;
do not weaken gates, fabricate a predecessor, add a positive migration, tag,
publish, or claim v0.1 released/Done.
