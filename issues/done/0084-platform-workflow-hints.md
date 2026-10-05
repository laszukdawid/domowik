# 0084: Share platform-aware editor workflow hints

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0040, 0049, 0078

## Acceptance criteria

- [x] Navigation, selection and history hints share a platform-aware vocabulary.
- [x] macOS shows Command (⌘); Windows/Linux show Ctrl, not mixed alternatives.
- [x] 2D and 3D history handling share a tested shortcut resolver; existing
  Control/Meta compatibility and document semantics remain unchanged.
- [x] Tests cover platform detection, displayed hints and accepted/rejected input.
- [x] Behavior/provenance are recorded and repository verification is run.

## Provenance

Original owner request and screenshot, 2026-10-04. Existing Domowik behavior
is retained; no comparable-application source or new dependency is needed.

## Closure — 2026-10-04

Shared runtime-platform vocabulary, guidance widget, command-modifier predicate
and 2D/3D history resolver implemented. Eight new tests cover labels, theme
independence and input compatibility. Analysis, all 288 Flutter tests, native
checks and the Linux release build pass. Repository verification printed
`=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1, solely because
the workspace contains uncommitted changes. macOS execution was skipped on Linux.
An initial sandbox run stopped at the sanitizer stage; the unsandboxed rerun
passed it. Test setup/import issues were fixed before the full passing suite.

See [workflow evidence](../../docs/evidence/0084-platform-workflow-hints.md).
