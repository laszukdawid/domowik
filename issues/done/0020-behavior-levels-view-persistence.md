# 0020: Behavior specification for levels, viewport, and persistence

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** None

## Problem

Define `../../docs/BEHAVIOR-SPEC.md` sections 1, 2, and 9: coordinate/view behavior, level
states, and save/recovery/missing-assets UX. Decide shared document state versus
local per-device view state.

## Acceptance criteria

- [x] Level elevation/thickness/defaults and active/visible/ghosted/editable
      matrix are explicit, including cross-level hit testing.
- [x] Viewport, selection, active level, and cameras are classified as local,
      shared, or optionally persisted state.
- [x] Grid/ruler/zoom/pan display precision and input semantics are specified.
- [x] Atomic-save expectation is old-or-new readable state under defined
      platform assumptions and failure injection, not a universal power-loss
      guarantee.
- [x] Required/optional missing asset and thin-package offline UX is defined.
- [x] Normative rules have study-log entries and a provenance review.

## Closure

- **Main repository commit:** `69c40d3`
- **Review:** Ox Alpha worker and GPT-5.6 Luna adversarial reviewer agreed the
  issue was complete on 2026-08-22.
