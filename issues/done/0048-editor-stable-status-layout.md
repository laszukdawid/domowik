# 0048: Keep status messages from moving the plan canvas

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0039, 0041

## Problem

The status bar allows one or two text lines and sizes itself to the current
message. A message change resizes the canvas, which correctly recentres its
camera for the new dimensions but moves geometry beneath an unchanged pointer.
Issue 0045's real-native targeting tests exposed an 8.5 logical-pixel shift
after a message change, enough to make a repeated target miss.

## Acceptance criteria

- [x] Status message length and error state do not change canvas bounds at a
      fixed window size and text scale.
- [x] Long messages remain available accessibly without overflow; larger text
      scales and compact/desktop layouts remain usable.
- [x] Regression tests cover short/long/error messages and text scaling;
      real-native 0045 targeting tests pass without stale-coordinate workarounds.
- [x] Full verification passes.

## Coordination

Claude Sonnet 5 owns `flutter_app/lib/editor_shell.dart` and
`flutter_app/test/editor_shell_test.dart` only. Codex owns tracker updates,
integration and verification; Opus owns the separate native hit-test bridge.

## Closure

- **Implementation/spec commits:** `8fad110`.
- **Follow-up issues:** None identified.

Scoped checks pass: 11 shell tests, including actual two-line render bounds,
Unicode, accessibility scaling and icon-height preservation at tiny scale.
The combined Flutter suite passes all 57 tests. Full `./scripts/verify.sh`
passed against `8fad110`, including native macOS bundle probes.
