# 0081: Implement Linux plan file dialogs

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024, 0075

## Acceptance criteria

- [x] Linux handles `openPlan` and `savePlan` on the existing file channel rather
  than returning MissingPluginException.
- [x] Open selects a local plan file; Save suggests the supplied filename and
  confirms overwrite. Both offer a `.domo` filter and return null on cancel.
- [x] Asset-directory selection continues to work.
- [x] Native chooser tests exercise all three actions and the Linux build passes.

## Provenance

Original bug reported by the user on 2026-10-04. Uses the existing GTK dependency
and Domowik's file-channel contract; no external application code consulted.

## Closure — 2026-10-04

The Linux file-channel handler previously recognized only `pickAssetDirectory`.
It now handles Open and Save using GTK native dialogs and the existing
asynchronous path/null response and method-call ownership. A shared chooser
factory configures local single selection, plan/all-file filters, suggested save
names and overwrite confirmation. Four native GTK checks cover action dispatch,
filters, default/supplied save names, directory selection and unknown methods.
The native test script is included in canonical verification on equipped Linux
hosts, with an explicit skip otherwise. Tests run with Xvfb; no new application
dependency was added. The Linux release build passes. Native response cancellation
uses the existing response handler; the chooser tests do not claim end-to-end
portal interaction or manually selecting a file in the running application.

Canonical verification passed all executed checks through tracker consistency,
including four native GTK checks, Flutter analysis and 278 Flutter tests, then
printed `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1 because
the workspace has uncommitted changes. macOS application checks were explicitly
skipped on Linux. GTK tests require a virtual display accessible outside the
sandbox; the initial sandboxed invocation could not connect to Xvfb.
