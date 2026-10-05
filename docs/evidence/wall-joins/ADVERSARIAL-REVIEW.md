# Adversarial review — union, occupied region, and rooms

- **Date.** 2026-08-31
- **Advisor.** Codex, `gpt-5.6-sol`, `model_reasoning_effort="high"`, read-only
  sandbox. The startup banner in the transcript echoes
  `model: gpt-5.6-sol` and `reasoning effort: high`, so the configuration is
  verified rather than assumed (ED-0061, INSTRUCTIONS.md).
- **Scope given.** Three files only — `boolean.rs`, `walls.rs`,
  `arrangement.rs` — with the algorithm's five rules stated in the prompt and
  an explicit demand for counter-examples in coordinates rather than a
  taxonomy.
- **Transcript.** `adversarial-round-1-transcript.txt` (unedited, 3082 lines).

The round produced an argument, not a plan. It wrote and ran exact-rational
Python simulations of the algorithm, checked them against an independently
computed ground-truth area over every 3- and 4-rectangle combination from a
3x3 grid, then went after the tolerances directly. **Four findings, all
grounded in specific coordinates. All four are accepted.**

## Finding 1 — a fixed parameter epsilon is a length-dependent spatial one

`cut_params` collapsed cut parameters within `1e-12`. On an edge `1e13` mm
long that is 10 mm of real distance, so a 0.5 x 5 mm protrusion lost both its
cuts, its fragments formed an open chain instead of a ring, and `trace_rings`
discarded them. The protrusion vanished from the union.

**Accepted and fixed.** The tolerance is now `WELD_MM / len` — two cuts are the
same cut when the POINTS they name are closer than the tolerance that already
decides vertex identity.

Worth stating plainly: the demonstrated input is outside `COORD_BOUND_MM`
(1,000,000 mm), so the old form could not be reached by a valid document. It
was safe **by the accident of two unrelated constants being ordered correctly**,
and `COORD_BOUND_MM` is itself provisional pending issue 0030 (ED-0092). The
fix makes the relationship deliberate. Tests:
`cut_params_resolves_a_weld_sized_feature_on_a_full_length_edge`,
`cut_params_collapses_cuts_closer_together_than_one_weld`.

## Finding 2 — a sub-weld gap is closed, and the component count changes

Two rectangles 0.0004 mm apart merge into one component whose area includes the
gap, because their facing edges weld to the same vertex keys and cancel as a
reversed twin.

**Accepted as a documented consequence of the tolerance, not fixed.** `WELD_MM`
is 1 µm and ED-0092 states what it is for: below it there are no distinct
features. Codex is right that the mathematically correct answer is two
components; the implementation's answer is the one the tolerance defines, and
the error is bounded by the gap width times the contact length — under one
micrometre times a plan dimension.

What changed is that the bound is now PINNED rather than implied.
`a_gap_narrower_than_one_weld_is_closed_and_the_error_is_bounded` asserts both
that the merge happens and that the over-claimed area stays within one weld
across the contact, so a future change to `WELD_MM` surfaces here.

## Finding 3 — a room enclosed by several components was invisible

Four occupied triangles meeting cyclically at four points enclose a diamond.
The union was correct — four components, correctly split at each pinch per
R5.1.2 — but the enclosed diamond is a hole of no single component, and rooms
were derived by reading holes off each component in turn. `rooms` returned
nothing where R5.1.1 requires one face of area 50.

**Accepted and fixed, structurally.** Rooms are no longer derived from the
region; both are traced from the same boundary. Every surviving directed edge
has the occupied region on its left, so its REVERSE has free space on its left —
tracing the reversed edge set gives the free faces directly, and the unbounded
exterior face falls out by the same sign test that already discarded outer
faces. `union_and_complement` replaces the old two-step.

This was the most valuable finding: it was not a bug in a rule but a wrong
premise about where rooms come from, and no amount of testing the old shape
would have found it. Tests:
`a_cycle_of_point_touching_triangles_encloses_one_room` (Codex's exact input)
and `a_void_enclosed_by_components_that_only_touch_at_points_is_still_a_room`.

## Finding 4 — welded vertex coordinates depended on input order

Two points that weld to one key kept whichever was inserted first, so
`union([a, b])` and `union([b, a])` returned rings differing in a real
coordinate. Deterministic per run, but order-dependent — which R3.2.4 and
R5.1.3 forbid.

**Accepted and fixed.** The representative is now the lexicographically least
candidate: order-independent, and always one of the real input coordinates, so
exact input still produces exact output. Test:
`output_coordinates_do_not_depend_on_input_order_at_the_weld_tolerance`.

The existing `union_output_is_independent_of_input_order` could not have caught
this, because its inputs coincide exactly and never exercise the weld.

## What the round found SOUND

Recorded because a negative result from a competent adversary is evidence:

- **Nested overlap.** The midpoint rule could not be broken by nesting. Once
  splitting is complete, membership in any other ring makes a fragment internal
  to the union, and whether that ring is itself inside a third is irrelevant.
- **Three rings sharing one exact edge.** Correct in both directions: agreeing
  directions keep one copy, any opposing direction drops all.
- **Degree-four vertices.** The clockwise sweep separates an isolated
  point-touch into distinct occupied rings correctly. Finding 3 was about room
  DISCOVERY across such contacts, not about the sweep.
- **A hole touching its outer ring at one point.** No independent failure found.
- **HashMap iteration order.** No run-to-run leak: starts are sorted explicitly
  and maps are otherwise queried by key. (Order-dependence was real, but through
  Finding 4, not through iteration.)

## Round mechanics, for the next one

The round cost 82,711 tokens and about forty minutes. Three things made it
productive, all of them from INSTRUCTIONS.md: stdin redirected from
`/dev/null`, the reading restricted to three named files, and the instruction to
write findings first. Naming the algorithm's rules in the prompt was worth more
than any of them — it let the advisor simulate the code instead of reading it,
and every finding came out of a simulation rather than an inspection.
