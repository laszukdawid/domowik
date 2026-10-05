# 0039: Make the Flutter editor workspace clear and usable

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024

## Problem

The working macOS editor still presents itself as a bridge test harness.
Diagnostic actions compete with drawing and file controls, and its fixed
sidebar takes most of a small window. The project overview also says no GUI
exists, hiding the prototype that contributors can actually run.

## Scope

Improve the existing experimental editor's presentation using issue 0024's
delivered macOS slice. Keep authoritative editing and derived geometry in
Rust. No new dependencies, format changes, platform promises or ADR 0003
gate closure are implied.

## Acceptance criteria

- [x] File actions and active drawing/opening tools are visible and labelled.
- [x] The inspector shows active-level counts, floor area and selected-wall
      dimensions, with a reachable delete action.
- [x] Boundary diagnostics remain available in a collapsed section.
- [x] Small windows retain a usable canvas and access to the inspector.
- [x] Widget checks cover responsive layout and action routing; the existing
      native bundle and raster probes still pass.
- [x] README and architecture/roadmap distinguish the working prototype from
      the remaining platform and performance gates.

## Closure

- **Implementation/spec commits:** `feat(editor): simplify the flutter workspace`
  (the commit containing this closure).
- **Follow-up issues:** 0024 (platform gate), 0033 (measured interaction budgets),
  0040 (protect unsaved edits when replacing a session).

Delivered 2026-09-05. Three widget tests cover file/tool callback routing,
390-pixel layout and drawer access, and canvas-state preservation while
resizing across the 900-pixel breakpoint. Flutter analysis and the native
bundle self-test and raster probe passed in the full verification run;
its only failure before committing was the required P13 clean-tree stage.

Provenance: original presentation work using the repository's existing Flutter
SDK widgets and Rust-backed editor actions. No external application source,
assets, additional dependencies or geometry algorithms were used. No document
format or authoritative editing semantics changed. Small-window support is
layout evidence on the development host, not mobile-platform validation.
