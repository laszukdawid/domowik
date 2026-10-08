# Current backlog — 2026-10-08

Current triage of the Rust application and root-level `frontend/`. Read each
issue's Current disposition before its historical handoffs. This review checks
named artifacts and targeted regressions; it is not a new adversarial
certification of every original criterion or a platform audit.

The 2026-10-05 triage is retained below with 2026-10-08 updates for issue
0059's placeholder integration and the related 0011/0012/0026 dispositions.
The [complete issue audit](AUDIT-2026-10-08.md) additionally reviews all 94
preceding records, including terminal history, under issue 0095. It corrects
0032's stale room-record gate, clarifies the furnished-corpus activation gate,
and makes later residual acceptance work explicit.
See [the implementation evidence](../docs/evidence/0059-marquee/2026-10-08.md).

## Disposition

- **0015 is Done:** L11 enforcement and depth-127/128 admission / depth-129
  refusal are implemented and tested. Its last recorded defect is resolved.
- **0064 is Done:** empty asset universes are portable; current writer and CLI
  tests prove both requested profiles and the non-empty control agree.
- **0055, 0058 and 0061 are partial scope**, not active assignments. They have
  delivered implementations and named residuals. **0060 is Blocked** on its
  explicitly recorded specification/owner decisions, not missing room naming.
- **0059's frontend placeholder integration is delivered:** point-hit and
  marquee queries, declared bounds, PH1/PH2 deletion/undo and required-degraded
  inspection are covered. The issue remains partial for numerical/performance
  and asset-policy gates and its unchecked full-verification criterion.
- Other nonterminal issues now have a current disposition and next action.
  Original criteria and dated evidence remain; an old all-ticked checklist
  does not erase a subsequently discovered defect.

After the earlier review (0094), there were **94 issues: 61 Done, 24 partial
scope, 4 Blocked and 5 Open**. After closing the complete audit (0095), the
current count is **95 issues: 62 Done, 24 partial scope, 4 Blocked and 5 Open**.
None is actively In Progress.

## Accepted format direction — 2026-10-08

The owner selected a defined core with namespaced feature extensions and the
existing manifest `formatVersion` in
[ADR 0011](../docs/decisions/0011-extensible-versioned-document-format.md). Incompatible
development revisions use distinct labels without a promise to migrate every
unpublished artifact. 0004/0005 own the extension/capability wire contract and
0028 its fixtures; those remain unfinished. Current files still emit `"0.1"`.
Snapshot retention and post-v1 compatibility remain separate decisions.

## Work that remains

### Application correctness and performance

| Issues | Remaining outcome / next action |
| --- | --- |
| 0012, 0030, 0058 | Constructed geometry/distance guarantees, exact locality links and certified fast/fallback benchmarks. Snapping and hit classes already exist. |
| 0013, 0032 | Actual scoped recomputation proved against the locality oracle; room-record churn measurements. The record entity exists. |
| 0055 | Wall-contact crossing/join-policy and raw/external/compensation admission boundaries, atomic refusal and benchmark evidence. |
| 0059, 0026 | Frontend placeholder marquee/point-hit integration and native-backed PH1 selection/deletion/read-only tests delivered 2026-10-08. Remaining numerical/frame gates stay with 0012/0033, and wider asset-policy/cache obligations with 0026. |
| 0014 | Strict/compatibility validation, asset-digest reporting and six-way exit taxonomy. Warnings exist; exit codes remain 0/1/2. |
| 0016, 0017 | Shared semantic 2D paint, tolerant visual comparisons and reviewed shaping/bidi. SVG/PNG and limited-repertoire labels work. |

### Format, assets and persistence

| Issues | Remaining outcome / next action |
| --- | --- |
| 0004, 0005, 0010 | Outstanding logical schema, snapshot/retention and container decisions, reconciled with Rust model/property tests. Room/asset entities and opening fields exist. |
| 0011, 0027, 0029 | Full snapshot metadata/concurrent replay, history truncation and persistence/recovery matrix. Local undo/compaction is not full retention conformance. |
| 0021, 0026 | External-cache/thin-package and asset-policy obligations. Embedded import works; thin packing and remote retrieval do not. |
| 0028 | Reconcile stale expectation dispositions and prove remaining cases reject for their correct diagnostic. L11 twins and the domowik.json reader exist. |
| 0035 | Furnished/history corpus and complete encoding benchmark records. Preserve original named-host measurements. |
| 0031 | Build the Rust public-migration framework now; positive migration coverage waits for a real predecessor release. Developer conversion is not that promise. |

### Platforms and release gates

| Issues | Remaining outcome / next action |
| --- | --- |
| 0024 | Native bridge/platform validation at the current ABI and feature slice. Linux build/runtime evidence exists; earlier mobile results are bounded. |
| 0025, 0078 | Portable renderer and hardware/resource evidence. Linux GL 3D is delivered; deleting scaffolding did not close the portable gate. |
| 0033 | Named-device whole-app frame/input latency and memory budgets/results, not CPU-only measurements. |
| 0056 | macOS module-redefinition investigation and three consecutive clean build/probe runs on the relevant toolchain. Linux cannot supply this evidence. |
| 0061 | Reconcile successful hosted macOS verification, negative packaging proof and real summaries against unchecked criteria. Both jobs exist; Linux editor checks are mandatory. No remote run was caused here. |
| 0088 | Required but owner-deferred browser core/file/storage/render adapters, browser workflow tests and mandatory web CI. Native FFI/scaffolding is insufficient. |
| 0036 | Actual GUI engine/runtime/plugins/fonts/native inventory; reviewed notices in an authorized binary release. Rust CLI inventory/checks exist. |
| 0034, 0018 | Publication identity/application IDs and authorized format/binary release after its gates. Sharing labelled source is not a format/binary release. |

### Decisions, not missing features

- **0060:** disposition R5.1.7's manual-room-geometry/opaque-field boundary
  and recorded diagnostic/projection/interior-point findings. Naming works.
- **0072:** low-priority future curved-wall inclusion/exclusion decision with
  compatibility/specification work. Straight-only v0.1 does not settle that
  future question. No curved-wall implementation is authorized here.

## Removed from the live work plan

Superseded claims, not reasons to waive unfinished parent criteria:

- Recreate Go/archive code or a frontend under `spikes/`: retirement/removal
  are complete under 0062/0091/0093.
- Implement room records, opening fields, asset instances, PlaceholderBox,
  core PH1 queries, warnings or snapping from scratch: they exist. Remaining
  integration/conformance obligations are listed above.
- Add Linux/macOS jobs or a Rust CLI SBOM pipeline as if none existed.
- Decide empty-package profiles or add the missing apply depth guard:
  resolved and closed under 0064/0015.
- Publish a standalone tracker with its own license and replace relative
  links: this tracker ships with the root source tree.
- Treat September worker panes/branches, external logs or old commit IDs as
  current assignments or guaranteed reproducible evidence in a fresh clone.

**No whole remaining issue was shown obsolete.** Do not close genuine
platform, numerical, release or future product work as Not needed because
scaffolding disappeared or a subset became usable. Historical issue files
and the September audit remain records, not active work plans.

## Verification scope

Original 2026-10-05 closure regressions: `cargo test --locked --offline
-p domowik-cli -p domowik-container` on Linux, including
`apply_admits_127_and_128_and_rejects_129`,
`an_asset_free_package_is_portable_not_thin`, both empty `pack` request tests
and the non-empty control. Issue 0094 records the final tracker/link checks
and canonical verification result.

The 2026-10-08 integration run passed functional checks, 307 Flutter tests and
Linux bundle probes, then ended with `=== VERIFICATION FAILED ===` at
`P13 clean tree` because existing tracker edits and the fix were uncommitted.
The evidence records platform skips and the restored Dart lockfile after SDK
resolution drift; this is not an exit-0 verification claim.

The original 0094 tracker cleanup caused no visibility change, push, workflow
dispatch, registry reservation, dependency or product implementation. The
2026-10-08 update records the separate authorized implementation under 0059.
