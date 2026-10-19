# Format conformance and security corpus

Synthetic fixtures for Domowik readers, writers and validators, authored from
FORMAT-SPEC §2–§9.6 and the explicitly scoped ASSET-STRATEGY §10 requirements.
No user plans or third-party application fixtures are included.

The corpus establishes expectations; executable tests establish only the
behavior they actually exercise. Records marked `"implementationDeferred": true`
remain declarative vectors and must not be presented as proof that an
implementation satisfies them. Parameter metadata references the normative
definitions without duplicating bound values in this provenance table.

## Layout

```text
testdata/corpus/
  README.md             this file: scope, provenance, safety, numeric policy
  EXPECTATIONS.md       machine-readable expectation manifest (fenced JSON)
  parameters.json       symbolic parameter table (names + provenance; NO values)
  golden/               valid inputs with expected success outcomes
    entities/           one logical entity per §2.3 kind plus shape variants
    batches/            §4 batch shapes, counters, frontiers
    snapshots/          §5.1 snapshot shapes incl. empty projection
    assets/             descriptors, content-addressed blobs
    packages/           thin-valid.domo / portable-valid.domo (.domo ZIPs)
  adversarial/          well-formed inputs with deterministic non-reject or
                        safely-rejected outcomes: projection vectors V1–V7,
                        warning-class commits, degraded/missing-asset classes,
                        unknown-data preservation/refusal pairs
  migration/            schema-identifier gate fixtures (V3/V5); V4 recorded
                        machine-readably as blocked (no predecessor schema)
  hostile/              inert hostile inputs: container defects, name/path
                        attacks, malformed JSON/numbers, bombs-as-recipes,
                        framing anomalies, Zip64 defects
```

`.domo` packages are real ZIP-family containers built deterministically by the
dev-only generator in `crates/domowik-devtools/src/corpusgen` (a hand-rolled
byte-level ZIP writer plus targeted patches for defect variants). Only these generated artifacts —
the `.domo` files and the ratio-twin sidecar `meta.json` — carry a
byte-reproducibility guarantee: the test suite regenerates them on every run
and asserts byte equality (`cargo run --locked -p domowik-devtools --bin
domowik-corpusgen -- --update` rewrites them). All OTHER committed fixtures are integrity-checked instead: every
artifact record in `EXPECTATIONS.md` carries a `sha256` that the checker
recomputes from the referenced file bytes on every run and never trusts from
a sidecar.

## Machine-readable expectations

`EXPECTATIONS.md` carries fenced ```json blocks, one array of expectation
records. Every fixture artifact is indexed exactly once; every record points
at an existing artifact or a bounded recipe. Record fields:

| Field | Meaning |
|---|---|
| `id` | unique stable identifier (`<class>/<area>/<name>`) |
| `class` | `golden` \| `adversarial` \| `migration` \| `hostile` \| `declarative` |
| `artifact` | repo-relative path under `testdata/corpus/`, or omitted |
| `sha256` | hex digest of the artifact bytes; RECOMPUTED and compared on every test run |
| `recipe` | bounded generator recipe object, or omitted; `recipe.inputs` lists member files owned (and structurally validated) by the record, e.g. the duplicate-batch-id pair |
| `outcome` | `success` \| `reject` \| `warning` \| `degraded-read-only-inspection` \| `missing-asset-equivalence` \| `safe-write-refusal` \| `blocked-open-question` |
| `category` | exact ratified §9.3/§9.4 token, or null where no ratified class exists |
| `diagnostic` | non-taxonomy diagnostic tokens (e.g. recovery-state categories) — deliberately NOT in `category` |
| `clauses` | normative clause references owning the expectation |
| `implementationDeferred` | true when no executable harness can exist pre-0029 |
| `rawByDesign` | true for fixtures that are intentionally not strict JSON objects (truncated documents, NaN/Infinity literals, bare depth arrays) |
| `notes` | free text; never load-bearing |

Outcome vocabulary realizes FORMAT-SPEC §9.6's five categories plus plain
success; `blocked-open-question` is reserved for obligations that cannot be
authored honestly yet (recorded per reviewer amendment 5).

## Structured assertions inside fixtures

Structured normative assertions live ONLY in EXPECTATIONS.md records
(BLOCKER-6: wire fixture bytes are never altered by assertion metadata).
Every authored JSON fixture's record carries an `"assert"` object whose
`"outcome"` string must agree with the record outcome and whose typed fields
the checker shape-validates whenever present: `canonicalOutcome` (closed
enum; `discard-stale` requires non-empty `discardedBatchIds`), `winner`
(field/batch/operationIndex/actorId, where `batch` is a FS §7.6 entry stem
— the pre-ED-0037 `operationCounter` pair cannot place a write against a
later concurrent one and is no longer a valid shape), `tombstoneRetained`,
`deletingPositionBatch`, `winnerPositionsPresent`,
`createdPositionsPresent`, `batchPositionsPresent`, `discardedBatchIds`,
`warnings` (forces warning outcome), `commits`, `preservedFields`,
`writeBackContract`.
Selected assert fields are CROSS-CHECKED against the artifact bytes:
`entityKind` vs the document's `type`, `counterBase` vs batch documents,
`descriptorCount` vs descriptor object members, `entityCount` vs snapshot
entities. Conditional-on-presence validation is deliberate: construct-
specific requirements are enforced structurally where they exist
(`canonicalOutcome=discard-stale` ⇒ `discardedBatchIds`; `warnings` ⇒
outcome warning; limit-probe ⇒ structured boundaries) — no prose substitutes
for a required field. Exemptions are narrow and machine-enforced:
generator-produced sidecars (`*.meta.json`) and explicit `rawByDesign`
records may not carry asserts at all. Explanatory keys in records
(`notes`, declaredLogicalSize prose) are never load-bearing.

Limit-probe recipes additionally carry STRUCTURED boundary semantics
(`boundaries: [{quantity, row, atLimitOutcome, oneOverOutcome}]`) validated
against ratified §9.3 row IDs and the outcome vocabulary, replacing prose-only
boundary claims. Recipe member inputs carry their own `sha256`, recomputed
from the referenced bytes on every run so input tampering is detected. The
ratio-twin sidecar is digest-checked AND cross-verified against the actual
package entry sizes in TestRatioTwinMeta.

## Numeric policy (ED-0019)

No independent numeric literals appear as expected limits anywhere.

- Ratified §9.3 quantities may be used directly in fixtures because the
  consolidated table is ratified for v0.1; every such use cites its row ID in
  the record's `clauses`/`notes`. In the Go generator every magnitude is a
  named constant citing its definition site (e.g. `l3EntryExpandedBytes`
  restates row L3; `l15SmallInputFloorBytes` restates L15's small-input
  floor; `corpusArtifactCapBytes` is the named corpus safety parameter, not
  a specification quantity).
- Provisional parameters (`coordBoundMm`, `dimensionCapMm`,
  `derivedCoordBoundMm`, `lwwHorizon`) are NEVER encoded as values.
  `parameters.json` is symbolic/provenance-only. Bound probes against these
  parameters are recipes or declarative records referencing the parameter
  NAME, marked implementation-deferred until issue 0030 ratifies values.
- Oversized-quantity obligations (L1/L2/L4/L5/L6/L7/L16/L18/L23 aggregate
  and count rows whose faithful bytes would exceed the artifact cap) are
  bounded generator recipes with declared logical-size expressions over §9.3
  row names; committed bytes stay tiny. Byte-level probes whose deterministic
  rejection class is `malformed-length` (FS §9.4/§9.5 physical stage) carry
  that token; their pure limit-category boundaries are additionally carried
  by clean declarative recipes.

## Safety

Every committed artifact is inert data ≤ 256 KiB. Bomb fixtures are either
declared-size header lies (small stored bytes contradicting huge declared
sizes) or genuine micro-streams whose actual expansion stays below 256 KiB;
no fixture expands to resource-exhausting sizes on any machine. Traversal
fixtures use synthetic entry names inside package metadata only; nothing is
extracted to host paths by this corpus, and no nested archive is ever
interpreted. The independent safety test in `domowik-devtools` asserts the
size cap over all artifacts and recipe bounds on every run.

## Provenance and clean-room statement

All fixtures were independently authored for Domowik from FORMAT-SPEC §2–§9.6,
ASSET-STRATEGY §10 labeled material, and the issue 0028 text during the
issue/0028-conformance-corpus branch session of 2026-08-24. No third-party
corpus, test suite, captured file, or external reference was consulted for
content or inspiration. Synthetic UUIDs and digests are computed locally from
self-chosen ASCII seeds via SHA-256 (deliberately distinct from the
ASSET-STRATEGY §10.9 size-corpus seeds). ZIP structural knowledge comes solely
from FORMAT-SPEC §7.2's own normative profile description; APPNOTE.TXT was not
downloaded. Hostile fixtures are synthetic constructions, not captures.
Recorded per docs/CLEAN-ROOM-POLICY.md; provenance uncertainty stops work for
review.

## Scope exclusions

No persistence implementation; no BEHAVIOR-SPEC command semantics beyond wire
shapes the format spec cites; no live-network retrieval execution (L24–L28 are
declarative hint/configuration validation vectors); L20/L21/L22 as declared-
metadata admission shapes without codec fixtures; preview rendering deferred
per ASSET-STRATEGY §10.9 (reference encoder is evidence-vector-only);
F1–F9/RS1–RS8 recovery duties as declarative expected-state records pending
the issue 0029 harness; §9.4 physical-ordering row excluded (writer-normative
only, owner-unspecified); ASSET-STRATEGY §10.9 Part B untouched; Gaussian
Splats excluded entirely.
