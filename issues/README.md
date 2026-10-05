# Domowik Local Issue Tracker

Status: **local pre-publication tracker.** These are ordinary tracked files in
the Domowik repository. They were a nested Git repository during early design;
commit `00893a5` ("track issues as regular files instead of gitlink") ended
that, so the issues are present in every clone and in CI. Relative
documentation links are intentionally local and must be replaced by stable
repository URLs before this tracker is published (issue 0034).

The issue text is licensed under Apache-2.0 with the rest of the repository.

A per-issue reading of all 63 issues against their acceptance criteria, with
residuals, ownership and sequencing, is in
[AUDIT-2026-09-12.md](AUDIT-2026-09-12.md).

## Required context

Read `../AGENTS.md`, `../docs/CLEAN-ROOM-POLICY.md`, and the relevant ADRs.
The clean-room rules apply fully to specification and spike issues.

## Workflow

- One issue per `NNNN-slug.md`; IDs are unique across the root, `done/`,
  `not-needed/` and `dropped/` directories.
- States: `Open`, `In Progress`, `In Progress (partial scope)`, `Blocked`,
  `Done`, `Not needed`. `In Progress` means someone is working on it NOW; it is
  not a record that an issue has been touched. Per
  [ED-0095](../EXECUTIVE-DECISIONS.md), an issue whose acceptance criteria are
  met closes as `Done` even when a future specification fixation would refine
  it, and that residual work is recorded against the specification issue that
  owns it.
- **Spellings are exact, and `domowik-tracker-check` enforces them.** `Not
  needed`, not `Not Needed` or `not needed`. A status outside this list fails
  the `tracker consistency` stage naming the file and the vocabulary.
- **`Not needed` is the second terminal status**, for scope that is obsolete or
  superseded ([ED-0102](../EXECUTIVE-DECISIONS.md#ed-0102-owner-direction--every-issue-reaches-done-or-not-needed-and-implementation-is-delegated)).
  It requires a written rationale in the issue saying what superseded the scope
  and where the obligation now lives, and it leaves the acceptance criteria
  standing. **Deleting criteria to reach a terminal status is forbidden**; an
  issue closed this way must still read as an accurate record of what was asked
  for and why it stopped being asked for. A criterion that is merely unfinished
  is not obsolete, and `Not needed` is not a synonym for `Done`.
- `Dropped` is HISTORICAL. The checker still accepts it so closed files keep
  the vocabulary they were closed under; nothing new takes it.
- Move terminal files into `done/` or `not-needed/` and update this index in
  the same commit.
- Every issue states dependencies and testable acceptance criteria.
- **Partial-scope dependency semantics:** an issue may deliver under
  `In Progress (partial scope)`. A `Deferred-closure gates:` line lists
  other issues whose future outputs prevent full closure of named criteria;
  it is NOT an upstream `Depends on` edge and must not create a cycle —
  when a gated issue itself declares `Depends on:` this issue, it consumes
  this issue's delivered material as provisional handoff and its later
  outputs flow back only through the gates. Closure of the partially scoped
  issue requires every gate resolved or explicitly re-scoped.
- Use tracker commits such as `chore(issue-0004): refine operation metadata`.
- Record implementation commits/PRs when closing an issue.
- Use [TEMPLATE.md](TEMPLATE.md) for new issues.

## Phase 0: specification and feasibility

| Issue | Title | Status |
|---|---|---|
| [0001](done/0001-behavior-spec-walls-joins.md) | Behavior: walls and joins | Done |
| [0002](done/0002-behavior-spec-room-detection.md) | Behavior: room topology and identity | Done |
| [0003](done/0003-behavior-spec-snapping-selection-undo.md) | Behavior: snapping, selection, undo | Done |
| [0004](0004-format-spec-entities-ops.md) | Format: logical entities and operations | In Progress (partial scope) |
| [0005](0005-format-spec-container-versioning.md) | Format: container and versioning | In Progress (partial scope) |
| [0006](done/0006-geometry-spike-predicates.md) | Geometry spike: required robust predicates | Done |
| [0007](done/0007-geometry-spike-booleans.md) | Geometry spike: clipping and planar topology | Done |
| [0009](done/0009-naming-glossary.md) | Ratify names, module path, and glossary | Done |
| [0019](done/0019-behavior-spec-openings.md) | Behavior: doors and windows | Done |
| [0020](done/0020-behavior-levels-view-persistence.md) | Behavior: levels, viewport, persistence | Done |
| [0021](0021-format-assets-size-budgets.md) | Format: assets, package profiles, size budgets | In Progress (partial scope) |
| [0022](done/0022-geometry-numerical-model.md) | Decide numerical geometry model | Done |
| [0023](done/0023-container-security-recovery.md) | Container threat model and recovery contract | Done |
| [0030](0030-ratify-numerical-model.md) | Ratify numerical geometry model after spikes | In Progress (partial scope) |
| [0034](0034-identity-prerelease-finalization.md) | Pre-publication identity finalization | Blocked |

## Phase 1: core and CLI

| Issue | Title | Status |
|---|---|---|
| [0008](done/0008-go-scaffold-ci.md) | Go module scaffold and CI | Done |
| [0010](0010-core-document-model.md) | Core entity projection | In Progress (partial scope) |
| [0011](0011-core-oplog-undo.md) | Operation replay, commands, undo/redo | In Progress (partial scope) |
| [0012](0012-core-spatial-index.md) | Spatial index and hit testing | In Progress (partial scope) |
| [0013](0013-core-geometry-engine.md) | Geometry engine and room reconciliation | In Progress (partial scope) |
| [0014](0014-cli-new-info-validate.md) | CLI `new`, `info`, `validate` | In Progress (partial scope) |
| [0015](0015-cli-apply.md) | CLI `apply` | In Progress (partial scope) |
| [0016](0016-cli-export-svg.md) | CLI SVG export | In Progress (partial scope) |
| [0017](0017-cli-export-png.md) | CLI PNG export | In Progress (partial scope) |
| [0018](0018-release-format-v0.1.md) | Release experimental format v0.1 | Blocked |
| [0026](0026-core-asset-store-cli.md) | Asset store and CLI commands | In Progress (partial scope) |
| [0027](0027-cli-ops-compact.md) | CLI operation inspection and compaction | In Progress (partial scope) |
| [0028](0028-format-conformance-security-corpus.md) | Format compatibility and security corpus | In Progress (partial scope) |
| [0029](0029-container-persistence-recovery.md) | Container persistence and recovery | In Progress (partial scope) |
| [0031](0031-format-migrations.md) | Implement format migrations and backups | Blocked |
| [0032](0032-geometry-interaction-benchmark-corpus.md) | Geometry interaction benchmark corpus | In Progress (partial scope) |
| [0035](0035-size-corpus-measurement-encoding-benchmarks.md) | Measure size corpus and record encoding benchmarks | In Progress (partial scope) |
| [0036](0036-release-sbom-license-notices.md) | Release SBOM, license scanning, and notices | In Progress (partial scope) |
| [0037](done/0037-rust-core-foundation.md) | Rust core foundation: supersede ADR 0002, scaffold, verification | Done |
| [0038](done/0038-session-commit-cost.md) | Session commit cost is cubic in history length | Done |

## Later feasibility gates

| Issue | Phase | Title | Status |
|---|---:|---|---|
| [0024](0024-flutter-rust-bridge-spike.md) | 2 | Flutter-to-Rust bridge spike | In Progress (partial scope) |
| [0025](0025-flutter-wgpu-surface-spike.md) | 3 | Flutter-to-wgpu surface spike | In Progress |
| [0033](0033-viewport-render-input-performance.md) | 2 | Viewport, render, and input performance criteria | Open |
| [0039](done/0039-editor-workspace.md) | 2 | Make the Flutter editor workspace clear and usable | Done |
| [0040](done/0040-editor-unsaved-changes.md) | 2 | Protect unsaved edits when replacing an editor session | Done |
| [0041](done/0041-editor-viewport.md) | 2 | Make the editor viewport stable and specification-correct | Done |
| [0042](done/0042-editor-undo-redo.md) | 2 | Complete editor undo/redo and expose truthful session state | Done |
| [0043](done/0043-editor-gesture-lifecycle.md) | 2 | Preserve gesture intent across navigation and cancellation | Done |
| [0044](done/0044-editor-pan-zoom-input.md) | 2 | Support continuous trackpad and two-touch navigation | Done |
| [0045](done/0045-editor-screen-space-targeting.md) | 2 | Use consistent screen-space targeting throughout the editor | Done |
| [0046](done/0046-editor-grid-extreme-origins.md) | 2 | Bound grid painting under extreme finite camera origins | Done |
| [0047](done/0047-hit-region-ordering.md) | 2 | Order equal-distance region hits by canonical geometry | Done |
| [0048](done/0048-editor-stable-status-layout.md) | 2 | Keep status messages from moving the plan canvas | Done |
| [0049](done/0049-editor-selection-cycling.md) | 2 | Complete click cycling and modifier selection | Done |
| [0050](done/0050-hit-feature-deduplication.md) | 2 | Preserve distinct native hit features during deduplication | Done |
| [0051](done/0051-editor-level-visibility.md) | 2 | Complete level visibility and ghosting controls | Done |
| [0052](done/0052-editor-inspector-resilience.md) | 2 | Keep inspector layout and expansion usable | Done |
| [0053](done/0053-authoritative-level-lifecycle.md) | 2 | Align authoritative level lifecycle with the specification | Done |
| [0054](done/0054-level-fields-and-validation-warnings.md) | 2 | Complete authoritative level controls and validation warnings | Done |
| [0055](0055-wall-contact-admission.md) | 2 | Enforce wall contact refusals and semantic overlap warnings | In Progress |
| [0056](0056-macos-sdk-module-redefinition.md) | 2 | Diagnose intermittent macOS SDK module redefinition | Open |
| [0057](done/0057-macos-clean-checkout-build.md) | 2 | Repair the macOS build from a clean checkout | Done |
| [0058](0058-snapping-magnetism.md) | 2 | Implement magnetism and snapping | In Progress |
| [0059](0059-editor-marquee-selection.md) | 2 | Supply the marquee selection gesture | In Progress (partial scope) |
| [0060](0060-room-records-and-naming.md) | 2 | Deliver room records and room naming | In Progress |
| [0061](0061-ci-macos-gui-coverage.md) | 2 | Give CI the macOS and GUI stages it currently never runs | In Progress |
| [0062](0062-retire-go-toolchain.md) | 2 | Retire the Go toolchain | In Progress (partial scope) |
| [0063](done/0063-native-close-test-synchronization.md) | 2 | Make native close quiescence tests deterministic | Done |
| [0064](0064-empty-universe-package-profile.md) | 1 | Decide the package profile of a plan with no assets | In Progress |
| [0090](0090-ci-main-repair.md) | 2 | Repair hosted main CI preflight failures | In Progress |
