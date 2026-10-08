# Domowik Work Tracker

These are ordinary tracked backlog files in the Domowik repository, present
in every clone and in CI. They were a nested repository during early design,
but are not one now. Relative documentation links work when this source tree
is published on GitHub; separate tracker hosting is not planned.

The issue text is licensed under Apache-2.0 with the rest of the repository.

Start with [the current backlog review](CURRENT-BACKLOG.md), updated 2026-10-08,
and each nonterminal issue's Current disposition. The
[September audit](AUDIT-2026-09-12.md) is a historical 63-issue snapshot,
not the current count, owner roster or work plan. Issue 0059's
[placeholder integration](../docs/evidence/0059-marquee/2026-10-08.md) is
delivered; its numerical/performance and asset-policy gates remain partial,
and full verification remains unchecked after the clean-tree failure.

The [complete October audit](AUDIT-2026-10-08.md) covers all 94 preceding
records, including terminal history, and records the named basis and next
action for each. With audit issue 0095 closed, the tracker has **95 issues:
62 Done, 24 partial scope, 4 Blocked and 5 Open**; none is actively In Progress.

The later owner-selected [format direction](../docs/decisions/0011-extensible-versioned-document-format.md)
keeps the existing file version key and adopts a defined core with namespaced
extensions. Issues 0004/0005 remain partial for its wire contract and gates.

Older progress notes retain original criteria and evidence. Removed Go/spike
paths, old branches, local transcript/log locations and pre-squash commit IDs
are historical references, not files or commits guaranteed in a fresh clone.
Do not infer current defects, support or pending work from them without
checking the dated current disposition and implementation.

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
| [0030](not-needed/0030-ratify-numerical-model.md) | Complete numerical-model implementation conformance | Not needed |
| [0034](0034-identity-prerelease-finalization.md) | Pre-publication identity finalization | Blocked |
| [0072](0072-curved-walls-decision.md) | Decide and specify curved walls | Open |

## Phase 1: core and CLI

| Issue | Title | Status |
|---|---|---|
| [0008](done/0008-go-scaffold-ci.md) | Go module scaffold and CI | Done |
| [0010](0010-core-document-model.md) | Core entity projection | In Progress (partial scope) |
| [0011](0011-core-oplog-undo.md) | Operation replay, commands, undo/redo | In Progress (partial scope) |
| [0012](0012-core-spatial-index.md) | Spatial index and hit testing | In Progress (partial scope) |
| [0013](0013-core-geometry-engine.md) | Geometry engine and room reconciliation | In Progress (partial scope) |
| [0014](0014-cli-new-info-validate.md) | CLI `new`, `info`, `validate` | In Progress (partial scope) |
| [0015](done/0015-cli-apply.md) | CLI `apply` | Done |
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
| [0096](0096-snap-rounded-arrangement.md) | Replace the union with a snap-rounded labelled arrangement | In Progress |

## Later feasibility gates

| Issue | Phase | Title | Status |
|---|---:|---|---|
| [0024](0024-flutter-rust-bridge-spike.md) | 2 | Validate frontend Rust bridge and platform packaging | In Progress (partial scope) |
| [0025](0025-flutter-wgpu-surface-spike.md) | 3 | Validate portable Flutter GPU surface integration | Open |
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
| [0055](0055-wall-contact-admission.md) | 2 | Enforce wall contact refusals and semantic overlap warnings | In Progress (partial scope) |
| [0056](0056-macos-sdk-module-redefinition.md) | 2 | Diagnose intermittent macOS SDK module redefinition | Open |
| [0057](done/0057-macos-clean-checkout-build.md) | 2 | Repair the macOS build from a clean checkout | Done |
| [0058](0058-snapping-magnetism.md) | 2 | Implement magnetism and snapping | In Progress (partial scope) |
| [0059](0059-editor-marquee-selection.md) | 2 | Supply the marquee selection gesture | In Progress (partial scope) |
| [0060](0060-room-records-and-naming.md) | 2 | Deliver room records and room naming | Blocked |
| [0061](0061-ci-macos-gui-coverage.md) | 2 | Verify hosted macOS packaging and GUI CI coverage | In Progress (partial scope) |
| [0062](done/0062-retire-go-toolchain.md) | 2 | Retire the Go toolchain | Done |
| [0063](done/0063-native-close-test-synchronization.md) | 2 | Make native close quiescence tests deterministic | Done |
| [0064](done/0064-empty-universe-package-profile.md) | 1 | Decide the package profile of a plan with no assets | Done |
| [0065](done/0065-live-wall-drag-preview-and-shift-wheel.md) | 2 | Preview wall movement and pan with Shift+wheel | Done |
| [0066](done/0066-select-openings-before-host-walls.md) | 2 | Select openings before their host walls | Done |
| [0067](done/0067-edit-opening-position-and-size.md) | 2 | Edit opening position and dimensions | Done |
| [0068](done/0068-opening-drag-handles-and-rehosting.md) | 2 | Rehost and directly manipulate openings | Done |
| [0069](done/0069-opening-mechanisms-and-door-controls.md) | 2 | Choose opening mechanisms and door orientation | Done |
| [0070](done/0070-editor-warning-and-selection-usability.md) | 2 | Improve selection, warnings and opening affordances | Done |
| [0071](done/0071-wall-measurements-editing.md) | 2 | Edit wall thickness, height and centreline length | Done |
| [0073](done/0073-copyable-editor-diagnostics.md) | 2 | Copy editor diagnostics and simplify numeric errors | Done |
| [0074](done/0074-plan-local-asset-library.md) | 2 | Use a directory as a plan-local furniture library | Done |
| [0075](done/0075-asset-directory-picker.md) | 2 | Pick and edit plan-local asset library directories | Done |
| [0076](done/0076-render-imported-models.md) | 2 | Render imported furniture instead of footprint placeholders | Done |
| [0077](done/0077-user-configuration.md) | 2 | Remember asset libraries in user-level configuration | Done |
| [0078](0078-live-3d-plan-view.md) | 3 | Live 3D plan view and navigation | In Progress (partial scope) |
| [0079](done/0079-upright-3d-texture.md) | 3 | Present the native 3D texture upright | Done |
| [0080](done/0080-editor-preferences.md) | 2 | Add editor preferences and split orientation | Done |
| [0081](done/0081-linux-plan-file-dialogs.md) | 2 | Implement Linux plan file dialogs | Done |
| [0082](done/0082-model-size-and-resize.md) | 2 | Preserve model dimensions and expose object resizing | Done |
| [0083](done/0083-resize-dialog-preview.md) | 2 | Make the resize dialog compact and preview size changes | Done |
| [0084](done/0084-platform-workflow-hints.md) | 2 | Share platform-aware editor workflow hints | Done |
| [0085](done/0085-editor-service-boundaries.md) | 2 | Separate editor file/session and render-cache responsibilities | Done |
| [0086](done/0086-public-preview-documentation.md) | 2 | Prepare documentation for a public source preview | Done |
| [0087](done/0087-public-source-preparation.md) | 2 | Require Linux editor CI and retain Flutter template notices | Done |
| [0088](0088-web-editor-support.md) | 2 | Support the editor in a web browser | Open |
| [0089](done/0089-single-commit-source-baseline.md) | 0 | Collapse committed history and prune branches | Done |
| [0090](done/0090-ci-main-repair.md) | 2 | Repair hosted main CI preflight failures | Done |
| [0091](done/0091-documentation-consolidation.md) | 0 | Consolidate documentation and retire archive references | Done |
| [0092](done/0092-promote-frontend.md) | 2 | Promote the actively developed frontend | Done |
| [0093](done/0093-remove-obsolete-repository-remnants.md) | 0 | Remove obsolete repository remnants | Done |
| [0094](done/0094-reconcile-current-backlog.md) | 0 | Reconcile the tracker with the current application | Done |
| [0095](done/0095-audit-issue-relevance-and-currency.md) | 0 | Audit issue relevance and currency | Done |
