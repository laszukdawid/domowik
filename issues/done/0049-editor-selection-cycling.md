# 0049: Complete click cycling and modifier selection

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0003, 0043, 0045, 0047

## Problem

The editor stores one selected ID and always chooses the first native hit.
It cannot cycle overlapping objects, toggle an entity into a selection set,
or represent a nonempty selection with no primary. R7.1/R7.2/R7.4 require
those states. Independent review also found an unresolved distinction between
raw feature candidates and the entity IDs that selection stores.

## Acceptance criteria

- [x] Review and record an original, implementation-independent decision for
      entity-versus-feature cycling, cycle position anchors and input-reset
      events, with normative rules and fixtures before implementation.
- [x] Plain clicks cycle the reviewed selection list deterministically;
      modifiers toggle membership, additions designate primary and removal
      never silently chooses another primary. Empty clicks clear both.
- [x] Every specified reset input is wired, including accepted document
      changes that leave the local candidate list unchanged and modifier/view
      transitions that return to their previous value between clicks.
- [x] Drag/cancel, deletion, undo/redo, level/file changes and primary-only
      controls preserve the specified selection/primary semantics. Selection
      is visible, transient and never enters document persistence or history.
- [x] Pure state-machine tests and actual native-backed widget tests cover
      cycling, toggles, primary-unset, resets, cancellation and history.
- [x] Full verification passes without new dependencies or format changes.

## Boundaries

Marquee gestures, placeholder instances and full snapping are separate work.
This increment must not pretend to implement modifier-drag marquee; its
interaction must remain non-destructive until that tool is supplied. Native
distance exactness remains issue 0012. Native feature deduplication is 0050.
Issue 0051 supplies application visibility/ghost transitions and closes the
previously deferred visibility-reset gate. Marquee/full snapping are not
implied by completion of this click-selection increment.

## Coordination

Codex owns specification decisions, Flutter integration and verification.
Claude Sonnet reviews the selection contract before owning its pure state
machine and tests. Claude Opus owns native revision exposure and independently
reviews integration. File ownership is explicit in the agent handoff.

## Reviewed behavior clarification — 2026-09-05

Original Domowik product choices, not observations of another application:

1. Raw hit features remain distinct under R6.3.2. Click selection derives a
   separate ordered list of unique entity IDs by retaining each ID's first
   occurrence in the native order. Each entity is one stop in the cycle;
   this does not merge native features or change drag-handle ranking.
2. The pointer anchor is the first click of a cycle. Subsequent in-radius
   clicks do not move it. Any observed finite pointer position beyond the
   applicable radius resets the cycle, even if the pointer returns before
   the next click (CR1). Boundary equality remains inside.
3. Modifier families Ctrl, Meta, Shift and Alt are observed as a bit set.
   Ctrl/Meta/Shift enable toggling; Alt alone does not. Any family-set change
   resets the cycle, even when it returns before the next click. Likewise,
   actual view/level/visibility changes reset when they happen, not merely
   when comparing the two eventual click contexts. Pointer type and accepted
   document revision changes reset too. A refused/no-op edit or save does not.
4. A plain press on the current primary preserves its whole selection set
   for a potential drag. Otherwise a plain press may tentatively select the
   hit entity alone as the potential drag primary. Click release applies its
   click/cycle rule to the pre-press selection, not that tentative selection.
   Cancel restores pre-press selection exactly; it does not undo cycle resets
   caused by observed input or document transitions.
5. A modifier press leaves selection untouched until release. If classified
   as a drag over an entity, only the existing primary may be the drag target;
   if the hit is not that primary, no edit starts. Empty modifier drags are
   reserved for additive marquee and must not create walls in the meantime.

Claude Sonnet 5 reviewed these original choices and the controller contract.
R7.2.2a/R7.2.3a/R7.2.6, the explicit R7.4 post-press baseline, and L9-L11
fixtures now record the decision in BEHAVIOR-SPEC with provenance. Review
required round-trip view/visibility fixtures and explicit modifier-click
semantics on non-primary entities; both are included. Empty click and idle
Escape end a cycle. Controller observations occur at event time, drag-target
selection on an existing primary is a no-op, and survivor pruning resets a
cycle only on actual removal.

An additional review found that modifier overlap targeting was still
ambiguous. R7.2.4a and L12 now explicitly choose the first projected entity
for every toggle-modifier click, ending any cycle; repeated held-modifier
clicks toggle that entity on/off. Plain clicks alone cycle (Alt alone counts
as plain). Claude Sonnet reviewed the exact proposal before ratification.
The initial implementation inferred this from singular wording; that was
insufficient authority and is recorded as a corrected process mistake.

## Closure

- **Implementation/spec commits:** `ff1214d`, `72b415e`.
- **Verification:** `./scripts/verify.sh` exited 0 on `ff1214d` with
  `=== VERIFICATION PASSED === all stages`, including all 96 Flutter tests
  and the clean macOS application build/probes. See
  [selection evidence](../../docs/evidence/0049-selection/VERIFIED.md).
- **Visibility gate resolved:** 0051 adds real controls and immediate context
  observations, with a native-backed hide/show round-trip test isolating the
  transition from pointer motion. `./scripts/verify.sh` exited 0 on `72b415e`
  with `=== VERIFICATION PASSED === all stages` and all 129 Flutter tests.
  See [level evidence](../../docs/evidence/0051-level-visibility/VERIFIED.md).
- **Follow-up issues:** Marquee and native numerical conformance (0012)
  remain separate work, not claimed as delivered here.
