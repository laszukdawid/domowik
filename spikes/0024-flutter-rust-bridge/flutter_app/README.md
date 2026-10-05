# Domowik experimental editor

Experimental application for issue 0024 and subsequent editor increments.
**Not a released product.** The directory remains under `spikes/` while
platform/rendering and measured whole-app performance gates are open.

Flutter calls the shared Rust core through `dart:ffi`. The current editor
supports walls/openings, levels, room naming, selection, snapping controls,
undo/redo, native open/save, imported furniture, resizing and preferences.
Linux additionally has an experimental native-texture 3D view. macOS has 2D
editor/bundle evidence; earlier Android execution does not establish current
full-editor support. Web editor support is planned but deferred in
[issue 0088](../../../issues/0088-web-editor-support.md); it does not work yet.

Start with the repository guides:

- [Getting started](../../../docs/GETTING-STARTED.md): Linux/macOS prerequisites,
  CLI, native build, tests and troubleshooting.
- [User guide](../../../docs/USER-GUIDE.md): editing, exact platform shortcuts,
  assets, resize previews, 3D and split preferences.
- [Compatibility](../../../docs/COMPATIBILITY.md): experimental files and
  platform/feature limits.
- [Contributing](../../../CONTRIBUTING.md): source ownership and verification.

From the repository root, after installing the required toolchains:

```sh
cd spikes/0024-flutter-rust-bridge/flutter_app
flutter pub get
flutter run -d linux
```

On macOS use `flutter run -d macos`. Platform builds compile and bundle the
Rust core automatically. Start drawing with **Walls** on an untitled plan.
Do not depend on local private `assets/` being present: attach your own licensed
models through **Assets**.

## Native-backed checks

From the repository root:

```sh
cd spikes/0024-flutter-rust-bridge
cargo build --release --locked
cd flutter_app
flutter pub get
flutter analyze
flutter test
```

Widget lifecycle tests load the real native library. Picker responses can be
injected; native handle/buffer leak checks still run. The
[parent spike README](../README.md) documents ABI ownership, cancellation,
platform probes and historical evidence.

From `flutter_app`, the macOS bundle has two in-process probes:

```sh
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_spike_app.app/Contents/MacOS/domowik_spike_app

DOMOWIK_SPIKE_SELFTEST=1   "$APP"   # core, persistence and ownership checks
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"   # drives the real UI, reports from inside paint
```

Both are exercised by the macOS-conditional stages in `scripts/verify.sh`.
Linux runs explicitly skip those stages; a skip is not macOS validation.

Viewport microbenchmark (separate from correctness tests):

```sh
flutter test benchmark/viewport_benchmark.dart
flutter test benchmark/level_render_benchmark.dart
flutter test benchmark/camera_3d_benchmark.dart
```

This measures JIT CPU camera work and grid/ruler display-list recording, not
presented frame time or input latency. Issue 0033 still owns those product gates.
