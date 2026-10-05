# 0042: Complete editor undo/redo and expose truthful session state

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0011, 0024

## Problem

The Rust session supports redo but the native bridge and editor expose only
undo. The bridge's execute/undo summaries read the unused in-memory session
after a plan has been saved or opened, so their entity/history counts are
wrong. The GUI also creates its default level as an undoable user command,
allowing Undo on a new blank plan to remove its only drawing level.

## Scope

Expose core-owned history state and redo, initialise a normal empty editor
plan with a non-undoable baseline level, and add enabled/disabled Undo/Redo
controls and standard editor shortcuts. Follow BEHAVIOR-SPEC section 8 and
R9.1.1a, with no UI-authored compensation batches or document-format changes.

## Acceptance criteria

- [x] The native ABI exposes redo and read-only session state with correct
      entities, undo/redo counts and dirty state before/after save or open.
- [x] A new editor plan has one default baseline level and empty history.
- [x] Undo/Redo controls reflect actual core availability; Ctrl/Cmd+Z and
      Ctrl/Cmd+Shift+Z route through the same core commands.
- [x] Save and view changes preserve redo; an accepted edit invalidates it;
      reopen starts with empty local stacks; empty undo/redo emits no batch.
- [x] ABI declarations, revision checks, C/Dart callers and bundle probes are
      updated together; native and real-library widget tests cover the flows.
- [x] Full repository verification passes without new dependencies.

## Closure

- **Implementation/spec commits:** `feat(editor): complete undo and redo controls`
  (the commit containing this closure).
- **Follow-up issues:** None.

Delivered 2026-09-05. ABI 8 adds default-plan creation, redo and a read-only
session summary. Execute/undo/redo counts read the live container session,
not the unused in-memory fallback. Container saves clear dirty after success,
retain history, and restore the previous target on a failed Save As.

The frontend consumes core history availability and dirty state. Undo/redo
cancel active gestures, clear vanished selections, and adopt a surviving
level if history removes the active one. Canvas-focused Ctrl/Cmd+Z and
Ctrl/Cmd+Shift+Z use the same commands as the toolbar. The Ground baseline
has no local undo entry. No document format, dependency or ownership change;
the spike ABI revision intentionally changes and callers reject mismatches.

Regression coverage: 33 bridge tests, container save-state/target tests,
C and Dart ABI probes, built-bundle baseline/state/redo probes, and 23 Flutter
tests using the actual native library. Tests cover empty history, save between
undo/redo, invalidation by a new edit, clean reopen with empty stacks and both
keyboard modifier variants. Full verification is run against the closure
commit; CPU timings remain evidence-only, not a whole-editor performance gate.
