# 0045: Use consistent screen-space targeting throughout the editor

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0012, 0043

## Problem

The grab classifier uses logical-pixel reach divided by zoom, but click
selection still adds 150 mm and opening placement adds 400 mm. At low zoom,
a click can classify as a wall grab and then fail to select that same wall;
at high zoom, distant clicks can target it. Identified by the independent
Claude Opus 5 audit of commit c9efb80 and confirmed by source inspection.

## Acceptance criteria

- [x] Hit testing, selection and opening-host targeting use the applicable
      normative section 6/7 logical-pixel tolerances and geometry semantics.
- [x] The same pointer/modality/view produces consistent candidates between
      press and release; no fixed plan-mm interaction slack remains.
- [x] Tests cover zoom clamps, exact tolerance boundaries, touch/mouse and
      the low-zoom grab/selection disagreement without document side effects.
- [x] Full verification passes, with any geometry work benchmarked.

## Closure

- **Implementation/spec commits:** `8fad110`.
- **Follow-up issues:** Full selection cycling, marquee, placeholders and
  snapping remain separately tracked Phase 2 behavior work.

## Delivered scope

ABI 9 and Flutter now share native ordered candidates with screen-space
tolerance. Joined corners retain their own identity and co-move through the
existing native command; wall-only deletion never substitutes a member wall.
Opening-tool drags cannot accidentally move or create walls. Level changes
cancel a pending grab. Scoped checks are recorded in
`docs/evidence/0045-targeting/VERIFIED.md`. Full `./scripts/verify.sh` passed
against `8fad110`, including the native macOS bundle probes.

This issue owns frontend targeting consistency, not completion of issue 0012's
native exact-real distance/projection predicates. Those numerical guarantees
remain explicitly open; delegating to the core does not discharge them.
Issue 0048 fixes the status-driven camera shift exposed by these tests.

## Coordination

Claude Opus 5 owns the native bridge query, cache, C/Dart bindings and ABI
probes. Codex owns core ordering (0047), Flutter integration and verification.
A separate Claude Sonnet reviewer checks interaction semantics and test gaps.
Agents share a worktree with explicit file ownership; no concurrent full
verification or native builds.
