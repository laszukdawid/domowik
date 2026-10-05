# 0086: Prepare documentation for a public source preview

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0080, 0082, 0084, 0085

## Problem

The public entry points describe an older macOS-only editor without 3D.
Current capabilities, build requirements and contributor workflows are spread
across implementation evidence rather than accessible onboarding guides.

## Scope

Refresh current summaries and add getting-started, user, contributor
and compatibility guides. Preserve historical evidence and proposed decision
gates. No publication, visibility changes, package registration, dependencies,
application changes or third-party screenshots/assets. SECURITY.md is excluded
at the owner's request during this work.

## Acceptance criteria

- [x] README and editor instructions distinguish current capabilities from
      experimental features and platform evidence.
- [x] Getting-started instructions cover CLI, Linux/macOS GUI prerequisites,
      native-backed tests and troubleshooting with source-checked commands.
- [x] User documentation explains navigation, editing, file workflows, assets,
      resize previews, preferences and current limitations.
- [x] Contributor and compatibility guidance states ownership,
      provenance, verification and unresolved publication requirements honestly.
- [x] Documentation navigation and current architecture/roadmap/release
      summaries are reconciled without rewriting historical evidence.
- [x] Local documentation links and tracker consistency are checked; canonical
      verification is run and its actual result recorded.

## Provenance

Original owner request, 2026-10-04. Repository implementation, tests, policies
and existing evidence only; no external code, documentation quotations or art.

## Closure — 2026-10-04

Added the documentation index, getting-started guide, user guide, contributor
guide and compatibility guide. Refreshed root/editor/bridge entry points and
current architecture, roadmap, release-inventory and verification summaries.
Historical audit/experiment records remain intact; feature-request notes now
identify themselves as a dated snapshot. SECURITY.md was omitted at the
owner's explicit request. No publication, visibility or application changes.

Source checks cover native prerequisites, toolchain constraints, separate
bridge builds, user config paths, keyboard hints, model sizing, resize preview
semantics and Linux-only 3D. macOS instructions are source/CI checked, not
executed on this Linux host.

CLI smoke runs in temporary directories exercised creation, wall/opening and
room edits, validation, operations, undo, SVG/PNG export, background-image
import, portable packing and compaction. The old window example failed because
its implicit 2100 mm height plus 900 mm sill exceeded the host wall; it now
specifies 1200 mm height. Explicit 200 mm shell walls reproduce the documented
22.04 m² result instead of relying on the current 120 mm default.

Documentation link/anchor and whitespace checks passed; tracker consistency
passed. Canonical verification ran through all applicable checks, including
analysis and all 305 Flutter tests, then printed `=== VERIFICATION FAILED ===`,
`STAGE: P13 clean tree`, exit 1 for the uncommitted worktree. macOS bundle
checks were explicitly skipped. An approval-service timeout was retried once;
it was not a verification result. No commits were requested or made.

Remaining public-readiness work (outside this docs increment): history/secret
and asset-rights audit, publication-identity decisions, stronger Linux hosted
GUI coverage, complete binary license inventory and release authorization.

Start at the [documentation index](../../docs/README.md).
