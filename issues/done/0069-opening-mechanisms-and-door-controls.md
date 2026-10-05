# 0069: Choose opening mechanisms and door orientation

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0010, 0024, 0067, 0068

## Problem

The editor creates only one hinged door and one undifferentiated window. It
does not expose the door's authoritative hinge or wall-relative swing side.

## Scope

Offer hinged and sliding doors; side-hung, double-swing and lift windows on
creation. Show distinct 2D derived glyphs. Expose the hinge, swing side and
mechanism for relevant openings in the inspector.

## Acceptance criteria

- [x] Adding each offered kind records a compatible mechanism and paints a
      recognisably different derived glyph; old openings render with their
      existing default mechanism and round-trip without a new field.
- [x] Door hinge and wall-relative swing side can be edited independently;
      invalid cross-kind mechanism requests are rejected.
- [x] Format/spec compatibility and provenance are documented; native and GUI
      tests cover serialization, validation, placement and edits.

## Closure

- **Implementation/spec commits:** Pending local changes (no commit requested).
- **Follow-up issues:** 0033 measures full editor performance. Animated
  opening fractions and multi-leaf actuator controls need their own contract.

Compatibility: `mechanism` is optional and omitted for legacy plans. Legacy
door/window values resolve as hinged/side-hung respectively. The physical
container and existing required opening fields do not change. Readers that
predate this optional field preserve its raw value as unknown data under
FORMAT-SPEC §8; they render the legacy depiction. Mechanism enum values are
closed within this version; adding another value requires a format review.
Input provenance: the owner's request for hinged/sliding doors and side-hung,
double-swing and lift windows; the plan glyphs are original symbolic 2D
choices, not copied third-party assets.

The opt-in native benchmark `opening_mechanism_glyph_benchmark` measured
1,338,562 ns per derived response over 200 calls on the local Linux host
with three window mechanisms. It includes the whole level projection and
FFI serialization, not only glyph math; this host-dependent timing is not
a release gate.
