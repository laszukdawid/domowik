# Domowik experimental editor

Spike application for issue 0024. **Not a product.**

A Flutter macOS editor using the Domowik Rust core through `dart:ffi` from
inside its bundle. Draw walls, move corners, add doors/windows, inspect rooms
and floor areas, switch levels, undo, and open/save `.domo` plans. It is the
current GUI prototype, not a released cross-platform application: there is no
3D view, and Linux and Android have not been validated.

From a repository checkout, use macOS with Flutter (Dart 3.11.4 or later),
Xcode, CocoaPods, and the Rust toolchain selected by the repository. Run
`flutter doctor` first if Flutter has not built a macOS app on this machine.
Xcode invokes the plugin's build phase outside a login shell, so `cargo` and
`rustup` must be available in `~/.cargo/bin`.

```sh
cd spikes/0024-flutter-rust-bridge/flutter_app
flutter pub get
flutter run -d macos
```

The first run builds the Rust archive through CocoaPods and Xcode. It opens an
untitled plan. Pick **Walls**, then drag empty canvas to make the first wall.
Use **Open plan** to load `.domo` files created by the CLI. See the repository
[README](../../../README.md#run-locally) for a CLI example and the full local
verification command.

Choose **Walls**, then drag empty canvas to draw; click a wall to select it.
Click repeatedly at the same spot to cycle overlapping objects. Shift, Cmd
or Ctrl-click toggles the top object into or out of the selection. Adding an
object makes it primary; removing the primary leaves no primary until another
is explicitly selected. Edits and Delete act on the primary, not the group.
Escape during a gesture restores the pre-press selection; Escape while idle
clears it. Hold Shift, Cmd or Ctrl and drag from empty canvas to add walls
inside or touching the rectangle to the selection. The last newly added wall
becomes primary; a rectangle that adds nothing leaves the primary unchanged.
The rectangle appears during the drag, and selection updates on release.
Drag an endpoint to move its joined corner, or drag the wall body to move the
wall. Choose **Door** or **Window** and click the host wall to place an opening.
Scroll to pan; Cmd/Ctrl-scroll zooms about the cursor. Middle-drag or
Space-drag pans, and Escape cancels a pan. Use **Fit plan** to frame the
content; editing never silently refits it. The **+ / −** controls (or keyboard
keys) zoom, arrow keys pan by one grid step, and the grid/ruler toggle changes
only the view. Plan +Y points upward. The inspector shows the active level's
counts, floor area and selected-wall dimensions. On a small window, open
**Plan details** in the toolbar to reach the inspector.

**Save** opens the native file dialog; **Open plan** reads files also editable
with the CLI. Unsaved edits are marked beside the filename. New/Open, window
close and application quit offer Save, Discard or Cancel before leaving a
dirty plan. Cancelled dialogs and failed saves keep the current work available.
Boundary tests and the demo rectangle action are
available under the collapsed **Developer diagnostics** section.

This remains a prototype with known interaction gaps; it does not yet claim
full behavior-spec conformance or measured frame/input performance. Linux and
Android validation remain open under ADR 0003. No 3D view exists yet.

Two headless probes let the built bundle verify itself, because "the author
looked at a window" is not evidence that survives review:

```sh
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_spike_app.app/Contents/MacOS/domowik_spike_app

DOMOWIK_SPIKE_SELFTEST=1   "$APP"   # core, persistence and ownership checks
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"   # drives the real UI, reports from inside paint
```

Both run in `scripts/verify.sh`, along with `flutter analyze` and `flutter test`
for the workspace controls, responsive layout and canvas-state preservation.
The lifecycle widget tests load the real Rust library (build it first with
`cargo build --release` from the parent directory); only the native file
pickers are replaced with deterministic responses. They exercise failed and
cancelled saves/opens, save/discard/cancel on close, no-op undo, and input
exclusion during pending dialogs, with native handle/buffer leak checks.
See the spike README for results and limits.

Viewport microbenchmark (separate from correctness tests):

```sh
flutter test benchmark/viewport_benchmark.dart
```

This measures JIT CPU camera work and grid/ruler display-list recording, not
presented frame time or input latency. Issue 0033 still owns those product gates.
