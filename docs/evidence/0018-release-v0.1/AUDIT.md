# Issue 0018: v0.1 release-candidate audit record

Status: **Evidence** recorded under ED-0026 on branch
`issue/0018-release-v0.1-audit`. This is a gated release-candidate audit.
It is not a release: format v0.1 is not tagged, published, or called
released anywhere in this repository, and no acceptance checkbox of issue
0018 may be ticked while any dependency remains unclosed. Reviewer
agreement for scope was obtained before editing (rev0018release, this
session); the final partial-handoff verdict is recorded separately by the
reviewer.

## 1. Scope and method

Every claim below cites either a committed corpus artifact with its
machine-readable expectation record (`testdata/corpus/EXPECTATIONS.md`)
or an executable Go test that ran green in this session's verification.
Coverage claims are separated from known-blocked records: blocked items
are listed in §8 with their owning gap records and are never counted as
coverage. Method limits are stated where they exist: corpus `.domo`
artifacts are byte-reproducible generator outputs; other fixtures are
integrity-checked; `implementationDeferred` records encode obligations,
not behavior observed.

## 2. Predecessor schemas: honest empty set

Repository evidence records **no publicly released schema** of the
experimental series at all. Exactly one label, `"0.1"`, is EMITTED
(FORMAT-SPEC §8.1 V1) and SUPPORTED (§8.2 V3); the RELEASED set is empty
(§8.2 V4/V5/V6). The three are distinct concepts and this section needs
the third:

- `internal/migration.releasedSchemas` is **empty**, pinned by
  `migration.TestShippedRegistryHonesty` (subtest "released set is empty
  per 8.2 V6", which also requires `IsReleasedSchema` to be false for
  `"0.1"` itself). The production migration chain is therefore not merely
  empty but **unfillable**: §8.2 V4 lets a step originate only at a
  released schema, so no step can be added before a release is recorded
  (`TestOnlyReleasedSchemasMayOriginateSteps`). Separately and by a
  different mechanism, a package under a label this build does not
  SUPPORT refuses with an actionable `unsupported-version` diagnostic
  naming both labels — the §8.2 V3 reader gate at
  `internal/persist/reader.go:259-265` — and a migration plan may end
  only at the EMITTED schema (`TestReportPlanMustEndAtEmittedSchema`).
  Release status is consulted on neither path.
- The corpus contains no predecessor fixture and none could honestly
  exist. `migration/older-minor-unknown-label.json` pins that `"0.0"` is
  **not among the minors this reader supports**: its expectation record
  is `unsupported-version` under clause §8.2 V3, which is purely the
  support gate, and it pins nothing whatever about release status. The
  predecessor fact is not evidenced by that fixture; it comes from §8.2
  V6 and the empty `releasedSchemas`. `migration/v4-tested-migration-path`
  plus `gaps/v4-positive-migration-path` record §8.2 V4 as a
  machine-readable blocked obligation behind **two** gates, neither of
  which alone is the whole blocker: no `0.N` schema is publicly released,
  so V4 presently binds nothing (§8.2 V6 consequence 1); and a positive
  path would additionally need a specified later target schema that
  **that later release** emits and supports, plus a passing migration
  corpus (issue 0031 handoff).
- Git history contains no earlier public schema label, manifest form, or
  wire shape.

Therefore issue 0018's criterion "current application migrates every
earlier public schema fixture it claims to support" is discharged
**vacuously and only together with this statement**: the predecessor set
is empty, and *a fortiori* so: with no released schema at all there is
none earlier than the label this build emits. No positive migration
exists, is fabricated, or is claimed; the empty-chain honesty is
executable (`TestShippedRegistryHonesty`,
`TestOnlyReleasedSchemasMayOriginateSteps` — the chain cannot be filled
without first recording a release, `TestRunIdempotentAlreadyAtTarget` —
a source already at target writes nothing). Backups/rollback machinery
is exercised independently through the §9.7 save-machine fault matrix
(`TestMigrationSaveFaultMatrix`, F2/F3/F4/F5 injection points) so the
V4 duty has verified mechanics ready for its first real step.

## 3. Entity and operation coverage (v0.1 surface)

All eight entity kinds and all three operation kinds have committed
golden artifacts consumed by executable readers:

| Surface | Corpus records | Executable harness |
|---|---|---|
| `level` minimal + `backgroundImages` unordered set | golden/entities/level-minimal, level-background-images | `persist.TestGoldenEntityFixtures` |
| `wall` minimal + `styleRefs` | wall-minimal, wall-style-refs | same |
| `wallJoin` two members + C3 permutation equivalence | walljoin-two-members, c3-walljoin-perm-a/b | same |
| `opening` door/window incl. swingDeg range end | opening-door, opening-window | same |
| `roomRecord` seed-only / named (absent ≠ empty-string) | roomrecord-seed-only, roomrecord-named | same |
| Tier B `label`, `dimension`, `assetInstance` structural validity | label-minimal, dimension-measured, asset-instance | same |
| Coherent referential bundle (O4) | bundle-coherent | same |
| C1 key-order insignificance | c1-key-order-permutation-a/b | same |
| E2 UUID version opacity (v7/v4/v8 equally valid) | e2-uuid-version-variants | same |
| Batches B1–B7: single-create, multi-op atomic batch, two-actor frontier | golden/batches/single-create, multi-operation, frontier-two-actors | `TestGoldenEntityFixtures`, framing harness |
| Snapshots S1–S5 incl. empty projection | golden/snapshots/empty-projection, full-s1-s5 | snapshot round-trip tests |
| Ops createEntity / setFields / deleteEntity | batch records above | `persist.TestProjectionVectors` over the delivered vectors against the document engine |
| Deterministic projection vectors — executed with outcome assertions | v1-duplicate-delivery, v3-concurrent-set-set (winner), v4-delete-vs-set (tombstone), v4b-stale-set-after-delete (discard + diagnostics) | `TestProjectionVectors` subtests pass |
| Deterministic projection vectors — partial or skipped (NOT counted as coverage) | v2-reordered-delivery **SKIP** (unresolved semantic conflict between the fixture oracle and the delivered greatest-K resolution; escalated to issues 0004/0011); v5 partial (asserts lateBatchApplied=false only); v6 partial (pins non-empty projection; equality-with-full-replay deferred to the package round-trip suite); v7 **metadata-only** (records both orderings, executes one projection, asserts neither outcome) | same harness; skip/partial status machine-visible |
| Warning-class commits | adversarial/warnings/duplicate-level-elevations, ceiling-below-wall-height | **expectation records only — no executable harness consumes them yet** (routed, §8) |

Parser-boundary success twins exist at L11 (depth 128 parses), L12
(64-char token), L13 (i64 max): golden/parser-twin-*.

## 4. Required category coverage (issue 0018 AC1)

| Category | Corpus evidence | Executable evidence |
|---|---|---|
| Thin packages | golden/packages/thin-valid.domo | load/round-trip in persist suite |
| Portable packages | golden/packages/portable-valid.domo (every U(P) member digest-verified) | same |
| Duplicates — covered | V1 duplicate delivery (I2 silent ignore, executed); duplicate logical name; casefold/normalization collision | `TestProjectionVectors` v1; hostile-container exact-token passes |
| Duplicates — skipped corpus defects (NOT counted) | hostile/framing/duplicate-batch-ids recipe cannot materialize without a different reject firing first; hostile/json/duplicate-object-keys bytes contain no duplicate keys | both SKIP with escalated diagnostics (`framingOpenQuestions`, `jsonOpenQuestions`) |
| Corruption — covered | checksum-error container; digest-mismatch-portable (portability claim broken); torn batch document; truncated header/EOCD; trailing-data T1; concatenated packages; overlapping-regions | exact-token passes in `TestHostileContainerFixtures` / framing harness |
| Corruption — skipped corpus defects (NOT counted) | missing-manifest (fixture still contains domowik.json), spanned-disk-number (patched byte lands elsewhere; actual class malformed-length), ratio-under-L15-success-twin (stale inventory digest forces checksum-error vs pinned success) | three closed-set SKIPs in `TestHostileContainerFixtures`; unexpected divergence fails, so the set is consciously maintained |
| Missing blobs | thin-missing-optional-asset, dangling-well-formed-digest, descriptorless-reference, digest-mismatch-thin ≡ missing — **expectation records only** | **no executable harness consumes the degraded/* records yet** (routed, §8); synthetic stream-level coverage exists via `TestStreamEntryRejectsDigestMismatch` and degraded paths inside load round-trips |
| Unknown optional data | adversarial/unknown-data/unknown-optional-entity-field-preserve (U2 preserve variant); write-refusal-unpreservable (U4/I2 refusal variant) | §5 below — verified executable |
| Unsupported required data | adversarial/unknown-data/unknown-required-entity-field-refuse; migration/newer-minor-reject (V3 names BOTH versions); migration/older-minor-unknown-label; migration/foreign-namespace-entry (U3 closed namespaces) | §5 below — verified executable |

Known-blocked categories are NOT counted as covered; see §8.

## 5. Unknown-data and version-gate behavior (executable verification)

Requested verifications and their passing harnesses:

- **Unknown optional opaque preservation.**
  `persist.TestUnknownOptionalFieldPreservedAcrossRoundTrip` decodes the
  corpus probe entity carrying `futureNote`, saves through the full
  writer, reloads, and asserts the unknown member survives with
  value-identical JSON — silent stripping fails the test (U2/L32).
- **Safe write refusal when preservation is impossible.**
  `persist.TestUnpreservableConstructsForceSafeWriteRefusal` and
  `migration.TestUnknownOptionalPreserveOrRefuse`: unknown
  manifest/snapshot/batch members produce named findings; write-back is
  refused with `safe-write-refusal`, no target file is created, and no
  temporary candidates remain (partial-knowledge selective rewrite is
  structurally unreachable).
- **Newer-schema refusal.** The reader gate
  (`internal/persist/reader.go`, V3) rejects any `formatVersion` other
  than the sole accepted `"0.1"` with a diagnostic naming both versions;
  `persist.TestMigrationGateFixtures` executes the committed
  newer/older-label fixtures; guessing and best-effort parsing across
  minors do not exist in the read path.
- **Required-data refusal under a newer label.** Covered by the same
  gate plus `TestUnknownRequiredRefusesViaVersionGate`.
- **Migration pre-effect gates.**
  `migration.TestPreEffectGatesLeaveTargetUntouched`: unsupported
  labels and foreign namespaces reject before any effect; an existing
  target stays byte-identical and no leftovers appear.

Residual (honest gap, routed — see §8): the two U1 classes "unknown
entity kind under an accepted `0.1` label" and "unknown operation kind
under an accepted `0.1` label" have executable safe-refusal coverage
(`internal/document/schema_test.go` `codeUnknownEntityKind`;
persist non-execution mutation probe rejecting `teleportEntity` as
`malformed-framing` before replay) but no dedicated committed corpus
artifact. This audit does not claim AC1 complete for those two classes'
fixture set; authorship belongs to the issue 0028 corpus owner.

## 6. Compatibility and migration language

The exact experimental-series compatibility statement lives in ADR 0007
(Proposed). It states three facts distinctly: `"0.1"` is the sole schema
this build EMITS (§8.1 V1); `"0.1"` is the sole schema its reader
SUPPORTS (§8.2 V3); and no `0.N` schema has been publicly RELEASED
(§8.2 V6), so there is no earlier public schema and the predecessor set
is empty. Cross-label requests refuse with actionable diagnostics through
the §8.2 V3 support gate, and the V4 tested-migration duty attaches
prospectively to each future released minor. Nothing retroactive is
promised for the experimental series; the post-v1 contract and ADR 0007's
ratification gate (a v1.0 release item) are unchanged and still bind
nobody until ratified. FORMAT-SPEC §8.2 remains the sole governing text
pre-1.0.

## 7. Size corpus and measured budgets (0035 publication handoff)

Consumed strictly as the labeled provisional handoff of issue 0035
(Partial handoff accepted 2026-08-25):

| Scenario | Headline (preview-inclusive) | Strict target | Verdict |
|---|---:|---:|---|
| `empty-v1` | 3,597 B | < 16 KiB | strict pass |
| `unfurnished-1000-v1` | 119,394 B | < 256 KiB | strict pass |

Records: docs/evidence/0035-size-corpus/budget-empty-v1.json and
budget-unfurnished-1000-v1.json (per-entry-class breakdown, external
totals excluded, environment of record, raw outputs, reproduction
command). Deterministic tests re-validate digests and byte-equality
whenever the reference encoder is available, degrading to loud skips
otherwise. Scope caveats carried forward without dilution:

- All five encoding candidates (WebP, AVIF, KTX2/Basis Universal,
  glTF/GLB + meshoptimizer, font subsetting) are recorded
  **not-measured** in encoding-candidates.json; AVIF tooling existed but
  was deliberately not exercised. The per-candidate metrics criterion of
  issue 0035 remains OPEN; nothing here claims it satisfied.
- The `furnished-cached-v1` scenario remains dormant behind
  BEHAVIOR-SPEC §10; ED-0025 interpretations await the issue 0021
  amendment; no compatibility claim attaches to the size corpus.

## 8. Coverage versus blocked register, and upstream routings

Blocked records already machine-readable in EXPECTATIONS.md are carried
here as blockers, never as coverage:

| Blocked item | Gap record | Owner / routing |
|---|---|---|
| ambiguous-manifest deterministic trigger | gaps/ambiguous-manifest-deterministic-trigger | FS §7.4/§9.4 owners |
| Provisional-parameter bound probes (L14) | gaps/provisional-parameter-bound-probes | issues 0022/0030 |
| L29 duplicate-physical-occurrence tolerance appears unreachable | gaps/l29-duplicate-physical-occurrence-semantics | FS owners |
| L30 recovery-candidate quantities | gaps/l30-recovery-candidate-file-count | issue 0029 |
| L31 writer thresholds | gaps/l31-retained-package-size-writer-thresholds | issues 0005/0029 |
| Physical-ordering row | gaps/physical-ordering-row | issue 0005 (writer-normative only) |
| Preview render fixtures | gaps/preview-render-fixtures | AS §10.9 owners |
| V4 positive migration path | gaps/v4-positive-migration-path | issue 0031 upon first successor schema |

Findings raised by this audit (routed upstream, not absorbed here):

1. **Unknown-kind corpus artifacts** (entity kind, operation kind under
   an accepted label): executable coverage exists; fixture authorship
   routes to issue 0028.
2. **Orphan embedded blob semantics**: a blob entry present in the
   package but unlisted in the manifest inventory currently degrades as
   a missing asset (`internal/persist/reader.go` treats unlisted blob
   entries via the missing-asset path). No normative clause pins this
   outcome and no corpus case exists. Routed to the FS §7.4/§9.4 owners
   for a ruling, with issue 0029 owning any resulting implementation
   change; this audit neither claims nor weakens the category.
3. **Unresolved corpus-defect SKIP set** (all escalated by the harnesses
   themselves, closed sets, machine-visible): adversarial/vectors/
   v2-reordered-delivery (oracle vs delivered greatest-K resolution;
   issues 0004/0011); hostile/container/missing-manifest,
   spanned-disk-number, ratio-under-L15-success-twin, hostile/json/
   duplicate-object-keys, hostile/framing/duplicate-batch-ids (fixture
   generation defects; issues 0028/0023/0004). None of these records is
   counted as coverage anywhere in this document.
4. **Harness gaps**: no executable harness yet consumes the
   adversarial/degraded/* or adversarial/warnings/* expectation records;
   v6 equality-with-full-replay and both v7 orderings lack outcome
   assertions in the vector harness. Routed to issue 0028 (harness) and
   issue 0011 (vector semantics).
5. **Remaining release gates**: every dependency of issue 0018 (0005,
   0011, 0026, 0027, 0028, 0029, 0031, 0035, 0036) is In Progress per
   the tracker; identity finalization is held at the ED-0013 gate; ADR
   0007 is Proposed. None may be closed from this audit.

## Reconciliation with the reviewer verdict

The independent reviewer verdict at
[docs/evidence/0018-release-audit.md](../0018-release-audit.md)
("ED-0026 partial handoff: NOT RELEASE-QUALIFYING") was verified claim
by claim against repository evidence during this session before this
document was finalized: every SKIP it names reproduces exactly, the V6/V7
partial-assertion descriptions match `checkV6`/`checkV7`, and the
duplicate-key fixture bytes indeed contain no duplicate keys. This audit
was corrected accordingly and adopts the same conclusion: the measured
0035 Part A budgets and the migration framework are consumable as
provisional evidence, while the corpus semantic gaps, skipped/mismatching
records, harness gaps, unclosed dependencies, and ADR approval gates
keep issue 0018 below release qualification. Nothing was weakened to
reconcile; the reviewer file is preserved verbatim.

## 9. Reproducibility and release-inventory evidence

`GOTOOLCHAIN=local ./scripts/verify.sh` exits 0 on this branch: build,
full test suite, gofmt, vet, jsoncheck over docs and corpus, corpusgen
regeneration/integrity gates, dependency-drift guard, single-module
license guard, release-inventory verify (fail-closed registry audit,
committed SPDX/NOTICE comparison, consumption-manifest checks), CLI
stream/exit-code probes, dual-architecture cross-builds, and the
clean-tree assertion. Local release evidence under gitignored
`build/release/` (linux-amd64/arm64 binaries plus manifest.json) is
materialized and hash-checked for issue 0018 consumption; it is
candidate evidence, not a publication.

## 10. Non-release statement

Nothing in this branch tags, pushes, publishes, uploads, announces, or
describes format v0.1 as released. This document uses FORMAT-SPEC §8's
three concepts distinctly and never as synonyms: **emitted** (§8.1 V1,
the single label this build writes), **supported** (§8.2 V3, what its
reader accepts), and **released** (§8.2 V4/V5/V6, publicly released —
currently EMPTY; v0.1 is not in it).

An earlier revision of this section carried a local definition — that
"released schema" above meant "accepted by this codebase's reader as its
sole supported label" — and it is removed rather than repaired. It was
load-bearing in the wrong direction: a private glossary placed *below*
the claims it licensed made every use of "released" above it readable as
true, which is exactly how the fused model survived review. Each use is
now correct against §8 on its own, so no glossary is required.

Issue 0018 cannot reach Done while §8 finding 5 holds.

## Machine-readable summary

```json
{
  "audit": "0018-release-candidate",
  "decisionContext": "ED-0026 gated RC audit",
  "isRelease": false,
  "adr0007Status": "Proposed",
  "emittedSchemaLabel": "0.1",
  "supportedSchemaLabels": ["0.1"],
  "releasedSchemaLabels": [],
  "predecessorSchemaSet": "empty (no earlier public schema; stated honestly)",
  "entityKindsCovered": ["level", "wall", "wallJoin", "opening", "roomRecord", "label", "dimension", "assetInstance"],
  "operationKindsCovered": ["createEntity", "setFields", "deleteEntity"],
  "projectionVectorsExecuted": ["V1", "V3", "V4", "V4b"],
  "projectionVectorsPartial": {
    "V5": "asserts lateBatchApplied=false only",
    "V6": "non-empty projection pinned; equality-with-full-replay deferred",
    "V7": "both orderings recorded, neither outcome asserted"
  },
  "projectionVectorsSkipped": [
    "V2 (oracle vs greatest-K conflict; escalated to issues 0004/0011)"
  ],
  "corpusDefectSkips": [
    "hostile/container/missing-manifest",
    "hostile/container/spanned-disk-number",
    "hostile/container/ratio-under-L15-success-twin",
    "hostile/json/duplicate-object-keys",
    "hostile/framing/duplicate-batch-ids"
  ],
  "harnessGaps": ["adversarial/degraded/* records unconsumed", "adversarial/warnings/* records unconsumed"],
  "categoryCoverage": {
    "thin": "covered",
    "portable": "covered",
    "duplicates": "partially-covered-named-skips-routed",
    "corruption": "covered-except-three-skipped-container-defects-routed",
    "missingBlobs": "expectation-records-only-no-harness-yet",
    "orphanBlobs": "residual-routed-upstream",
    "unknownOptionalData": "covered-executable",
    "unsupportedRequiredData": "covered-executable",
    "unknownKindCorpusArtifacts": "residual-routed-to-issue-0028"
  },
  "measuredBudgets": {
    "empty-v1": {"headlineBytes": 3597, "targetBytes": 16384, "strictPass": true},
    "unfurnished-1000-v1": {"headlineBytes": 119394, "targetBytes": 262144, "strictPass": true},
    "source": "docs/evidence/0035-size-corpus (labeled provisional publication handoff)"
  },
  "encodingCandidatesComplete": false,
  "encodingCandidatesMeasuredCount": 0,
  "blockedOpenQuestionsCarried": [
    "gaps/v4-positive-migration-path",
    "gaps/ambiguous-manifest-deterministic-trigger",
    "gaps/provisional-parameter-bound-probes",
    "gaps/l29-duplicate-physical-occurrence-semantics",
    "gaps/l30-recovery-candidate-file-count",
    "gaps/l31-retained-package-size-writer-thresholds",
    "gaps/physical-ordering-row",
    "gaps/preview-render-fixtures"
  ],
  "remainingReleaseBlockers": [
    "dependencies 0005/0011/0026/0027/0028/0029/0031/0035/0036 all In Progress",
    "ED-0013 identity finalization gate",
    "furnished-cached-v1 dormant behind BEHAVIOR-SPEC §10",
    "ADR 0007 ratification reserved to v1.0"
  ]
}
```
