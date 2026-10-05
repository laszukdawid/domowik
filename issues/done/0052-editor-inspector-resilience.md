# 0052: Keep inspector metrics and expansion usable

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0039

## Problem

Native-backed selection tests reveal a 37 px overflow in the 248 px-wide
diagnostics metric row. Adding/removing selection controls also shifts the
unkeyed diagnostics tile in the lazy list and loses its expanded state.

## Acceptance criteria

- [x] Metric labels and values remain readable without horizontal overflow
      in narrow and enlarged-text layouts, including long values and RTL.
- [x] Expanded diagnostics remain expanded across selection changes.
- [x] Focused widget tests and actual native editor tests prove the fixes;
      full repository verification passes with no new dependencies.

## Coordination

Claude Sonnet owns an isolated inspector metric widget and its tests. Codex
owns main editor integration, persistent expansion identity and native tests;
Claude Opus independently reviews the result.

## Closure

- **Implementation/spec commits:** `ff1214d`.
- **Verification:** Eleven metric tests inspect visible text-run bounds in
  production-like scrolling constraints, including narrow/enlarged/RTL and
  unbroken-token cases. The actual-native save/refusal/distant-edit selection
  test proved the old overflow and lost expansion, then passed after repair.
  All 96 Flutter tests pass. `./scripts/verify.sh` exited 0 on `ff1214d` with
  `=== VERIFICATION PASSED === all stages`; see
  [shared evidence](../../docs/evidence/0049-selection/VERIFIED.md).
