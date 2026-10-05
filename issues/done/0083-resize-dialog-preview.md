# 0083: Make the resize dialog compact and preview size changes

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0082

## Acceptance criteria

- [x] Dialog width is capped; object names are limited to 50 characters including
  an ellipsis, with the complete name available on hover.
- [x] Initial and computed dimension displays use one decimal place in mm, without
  rounding untouched authoritative dimensions just by opening/applying.
- [x] A live top-view preview uses cached object art where available, compares the
  proposed footprint with its original outline, and shows height changes.
- [x] Invalid input retains the last valid preview and disables Apply. Cancel is
  non-mutating; Apply remains one undoable edit. Narrow-window layout is tested.

## Provenance

Original user screenshot and direction on 2026-10-04. The preview is an original
UI visualization using existing cached user artwork, with no new dependencies.

## Closure — 2026-10-04

Implemented a 420-pixel capped dialog, 50-character hoverable object names,
one-decimal input/display formatting and cached-art footprint/height previews.
Preview changes are local display state; underlying untouched dimensions retain
their precision. Invalid input keeps the last valid preview. Rendered screenshot
and narrow-window/hover/pixel-change/Apply/Cancel tests verify the UI.

Flutter analysis, all 280 Flutter tests and the Linux release build pass.
Canonical verification passed all executed checks through tracker consistency,
then printed `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1
because the workspace contains uncommitted work. macOS checks were explicitly
skipped on Linux. The first run caught an unnecessary test import, removed
before the complete rerun; no unresolved analysis failure remains.

See [preview evidence](../../docs/evidence/0083-resize-dialog-preview.md).
