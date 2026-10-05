# 0071: Edit wall thickness, height and centreline length

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0024, 0049, 0052

## Problem

A wall's thickness and height are authoritative per wall (R3.0.6) and the
bridge already carries the `setWall` gesture, but the editor creates every
wall at a hardcoded 200 mm, never surfaces height, and has no numeric editing
at all. A person cannot give an existing wall its real plan thickness, storey
height, or an exact centreline length: dragging endpoints is the only way to
change geometry. This was recorded as the first missing basic in
`docs/FEATURE-REQUESTS.md` on 2026-10-01.

## Scope

Expose thickness, height, and exact centreline length in the editor's wall
inspector as one numeric form submitted as one atomic gesture. A length edit
anchors `startMm` and moves `endMm` along the wall's existing Start→End
direction; a joined end co-moves its node members under R3.4.2. Curved walls
are explicitly NOT in scope: R3.0.1 excludes them from v0.1 and issue 0072
owns the decision to revisit that. Endpoint coordinate entry remains the
existing drag and `moveWall` behavior.

## Acceptance criteria

- [x] Selecting a wall shows length, thickness, and height, and offers an
      Edit dimensions form.
- [x] The form submits every changed measurement as ONE atomic `setWall`
      batch: one undo entry restores all changed values.
- [x] A requested length keeps Start fixed and moves End along the existing
      direction, including through a join (R3.4.2 co-movement).
- [x] Nonpositive or nonfinite input is refused at the form; a core refusal
      (contact, zero length, opening constraints) leaves the document,
      history, and selection unchanged and surfaces the reason.
- [x] Bridge wire contract, Rust tests, Dart form tests, and an end-to-end
      editor test cover apply, cancel, bounds, and undo.
- [x] BEHAVIOR-SPEC records the interaction rule; FORMAT-SPEC needs no change.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
  Bridge ABI 14; `wall_properties_form.dart`; R7.1.6. Evidence: Rust
  `edit_set_wall_length_and_dimensions_are_one_batch`, Dart
  `wall_properties_form_test.dart`, `wall_dimensions_test.dart`; full
  `./scripts/verify.sh` passes every stage except the expected dirty-tree
  check.
- **Follow-up issues:** 0072 owns curved-wall representation.
