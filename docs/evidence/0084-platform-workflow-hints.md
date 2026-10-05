# Platform-aware workflow hints — issue 0084

## Scope and provenance

Original owner request and screenshot, 2026-10-04. Implemented from Domowik
BEHAVIOR-SPEC §11.2 and existing editor gesture contracts. No external source,
assets, dependencies, file-format changes or geometry algorithms introduced.

## Shared abstractions

`EditorWorkflows` supplies the runtime keyboard vocabulary and navigation,
selection, marquee and history guidance. `EditorWorkflowHints` owns their
inspector presentation. The platform is Flutter's runtime target platform,
not `ThemeData.platform`; it is resolved at build time. macOS shows Command
(⌘); Windows/Linux and other current keyboard targets show Ctrl.

2D and 3D call the same history shortcut resolver. Wheel zoom uses the shared
command-modifier predicate. Control and Meta remain compatible aliases;
Shift+command+Z remains Redo. Ctrl+Y is not advertised or newly introduced.
Selection's existing bitmask and native selection semantics stay unchanged.
No keyboard shortcuts were added for New/Open/Save; those remain button actions.

## Checks

Eight new tests cover all six platform labels, deliberate theme/runtime
disagreement, displayed navigation/selection/history hints, the modifier
truth table, rejected keys, and held-key history resolution. Existing native-
backed editor tests exercise history and navigation routing. Flutter analysis
and the Linux release build pass.

All 288 Flutter tests and the native checks pass. Full verification ended with
`=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1: the workspace
has uncommitted changes. macOS execution was explicitly skipped on Linux.
The initial sandbox run stopped at the sanitizer stage; rerunning outside the
sandbox passed that stage and all executed checks through tracker consistency.
