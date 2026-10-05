# Domowik editor

The actively developed Flutter interface for Domowik. **Not a released
product:** platform/rendering and measured whole-app performance gates remain
open. Promotion out of `spikes/` does not close those gates.

Flutter calls the shared Rust core through `dart:ffi`. The current editor
supports walls/openings, levels, room naming, selection, snapping controls,
undo/redo, native open/save, imported furniture, resizing and preferences.
Linux additionally has an experimental native-texture 3D view. macOS has 2D
editor/bundle evidence; earlier Android execution does not establish current
full-editor support. Web editor support is planned but deferred in
[issue 0088](../issues/0088-web-editor-support.md); it does not work yet.

Start with the repository guides:

- [Getting started](../docs/GETTING-STARTED.md): Linux/macOS prerequisites,
  CLI, native build, tests and troubleshooting.
- [User guide](../docs/USER-GUIDE.md): editing, exact platform shortcuts,
  assets, resize previews, 3D and split preferences.
- [Compatibility](../docs/COMPATIBILITY.md): experimental files and
  platform/feature limits.
- [Contributing](../CONTRIBUTING.md): source ownership and verification.

## Layout and naming

The displayed name is **Domowik**; the Dart package and desktop executable
are `domowik_editor`. Development application IDs and native ABI symbols stay
unchanged so this move does not reset platform identity or break callers.

- `lib/`, `test/`, `benchmark/`: interface, regression tests and measurements.
- `native/`: Rust C ABI adapter and its C/Rust tests.
- `plugin/`: platform packaging and native build integration.
- `dart_harness/`: shared FFI declarations and standalone boundary tests.
- `Cargo.toml`: separate native workspace; artifacts go into `target/`.

Historical issue-0024 records use `spikes/0024-flutter-rust-bridge`.
That root is now `frontend/`; its `flutter_app/` became this directory,
`flutter_plugin/` became `plugin/`, and Rust sources/tests/headers moved into
`native/`. Dated measurements retain their original paths and environments.

## Rendering and the wgpu experiment

`spikes/0025-flutter-wgpu-surface` contains an isolated dependency manifest,
lockfile and placeholder Rust file, not a renderer or Flutter texture bridge.
There is no implementation to merge. Its proposed texture/lifecycle work can
guide a future backend, but needs implementation, license review and platform
validation first. The working Linux native GL texture backend stays in place;
renderer-neutral scene data remains in `crates/domowik-scene`.

From the repository root, after installing the required toolchains:

```sh
cd frontend
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
cd frontend
cargo build --release --locked
flutter pub get
flutter analyze
flutter test
```

Widget lifecycle tests load the real native library. Picker responses can be
injected; native handle/buffer leak checks still run. The
[native bridge README](native/README.md) documents ABI ownership, cancellation,
platform probes and historical evidence.

From `frontend/`, the macOS bundle has two in-process probes (the legacy
`DOMOWIK_SPIKE_*` environment names remain supported):

```sh
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_editor.app/Contents/MacOS/domowik_editor

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
