# 0075: Pick and edit plan-local asset library directories

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0074

## Problem

The owner expects Add directory to open the system folder chooser. Manual path
entry should remain optional, and already attached paths should be editable.

## Acceptance criteria

- [x] Add directory opens a native folder picker on Linux/macOS and attaches the
      selected directory; cancellation changes no plan state.
- [x] A separate manual path option remains available, and attached directories
      can be edited by typing a replacement or choosing another folder.
- [x] Replacement validates before changing persisted metadata, does not leave
      the previous library attached, and preserves placed/embedded models.
- [x] Duplicate picks/replacements do not create duplicate library entries;
      picker errors, pending requests and unmount are handled.
- [x] Native persistence and Flutter interaction regressions, desktop build and
      canonical verification are run, with platform limits recorded.

## Provenance and compatibility

Original owner-requested UI behavior. Existing platform SDKs only; no new
package, catalog, model content or model format. Android document-provider URI
libraries are outside this filesystem-directory increment. Directory metadata
keeps the issue-0074 optional manifest shape; ABI 15 gains a new
`replaceDirectory` request kind. Existing callers are unchanged; an older ABI
15 implementation rejects the new kind instead of silently appending a path.

## Closure — 2026-10-03

Implemented in the working tree, no commit requested. GTK/AppKit directory
pickers use the existing method channel; manual additions and edit/browse
replacement controls share native validation and plan-local persistence.
See [usage/evidence](../../docs/evidence/0075-asset-directory-picker.md).

Linux release build, analysis, native regressions and the complete Flutter suite
(268 tests) passed. macOS picker source is updated but was not executed on this
Linux host. Canonical verification printed `=== VERIFICATION FAILED ===`,
`STAGE: P13 clean tree`, exit 1 after all preceding executed checks passed:
existing work and this implementation remain uncommitted. No full verification
pass is claimed.
