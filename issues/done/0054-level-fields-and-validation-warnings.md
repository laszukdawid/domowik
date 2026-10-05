# 0054: Complete authoritative level controls and validation warnings

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0020, 0053

## Problem

Level existence, deletion and view controls do not complete section 2.1.
The GUI still lacks editing of authoritative level names, elevations, slab
thickness and default wall/ceiling heights. Duplicate-elevation and slab-overlap
warnings must be observable without rejecting valid documents.

## Acceptance criteria

- [x] Expose the authoritative fields through validated core-owned commands
      and usable numeric/text controls, with specified units and bounds.
- [x] Field edits are atomic, undoable and preserved across save/reopen;
      changed elevation updates deterministic order without inventing a
      separately persisted level order or changing local flags implicitly.
- [x] Human/JSON validation and GUI diagnostics distinguish warnings from
      errors, including duplicate elevations and slab overlap per section 2.
- [x] Default-copy behavior follows each consumer's normative creation rule;
      no retroactive mutation of existing entities when a default changes.
- [x] Native-backed interaction and core/CLI tests cover valid/refused edits,
      ties, warnings, history and persistence; full verification passes.

## Closure

- **Implementation/spec commit:** `125734d`.
- **Verified:** 2026-09-05. `scripts/verify.sh` exited 0 with
  `=== VERIFICATION PASSED === all stages`, including 151 Flutter tests and
  the Rust/C/Dart/macOS bundle checks. No verification stage was skipped.
- **Evidence:** [review, compatibility and measurements](../../docs/evidence/0054-level-properties/VERIFIED.md).
- The slab warning formula and level-owned unspecified-wall-height resolution
  were reviewed and clarified in section 2.1 before acceptance. No historical
  file/replay migration or retroactive entity mutation was introduced.
- Separate wall-contact refusal/overlap-warning work is tracked in 0055;
  these level diagnostics do not claim complete wall validation or finish
  the platform, full-frame performance, room/asset or 3D gates.
