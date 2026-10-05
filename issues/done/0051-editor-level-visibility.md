# 0051: Complete level visibility and ghosting controls

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0020, 0041, 0049

## Problem

The prototype switches the active level but does not expose the normative
section 2.4 visibility/ghost state. Inactive levels are not drawn at all.
Selection's visibility-transition reset can be tested in its state machine
but cannot yet be triggered through the application.

## Acceptance criteria

- [x] Implement section 2.4's normal/ghost/hidden level controls and active
      level transition rules, including empty documents and deleted levels.
- [x] Render visible inactive levels in their specified presentation without
      making them hittable, cycled, snapped to or marquee-selected.
- [x] Every real flag change invalidates a selection cycle immediately,
      including hide/show round trips before the next click.
- [x] View-state persistence/lifecycle follows the normative separation from
      document operations and undo; no invented document dirty state.
- [x] Native-backed widget tests and measured rendering costs cover multiple
      levels, transitions and active-level isolation; full verification passes.

## Coordination

Codex owns editor integration, native-backed tests, tracking and verification.
Claude Sonnet owns a pure level-view state controller and its tests; Claude
Opus owns an extracted multi-level painter and rendering tests/measurement.
The reviewed authority is BEHAVIOR-SPEC sections 2.2–2.6 and 8.7: no view
profiles persist anywhere in v0.1, and only the active level is interactive.
Existing per-level derived/hit calls and document revision need no ABI change.

Authoritative level baseline/validation discrepancies are tracked separately
in 0053, not silently resolved in a view controller. An empty-level list is a
defensive UI state, not a claim that such a document satisfies section 2.2.

## Closure

- **Implementation/spec commits:** `72b415e`.
- **Verification:** `./scripts/verify.sh` exited 0 on `72b415e` with
  `=== VERIFICATION PASSED === all stages`, including 129 Flutter tests,
  both measurement harnesses and the clean macOS bundle build/probes.
  [Evidence and scoped measurements](../../docs/evidence/0051-level-visibility/VERIFIED.md)
  record native-backed isolation, transitions and payload reuse. This closes
  0049's application visibility-reset gate. Marquee/full snapping remain
  unimplemented; passive levels are excluded from all available input paths.
- **Follow-up issues:** 0053 (authoritative level baseline and validation).
