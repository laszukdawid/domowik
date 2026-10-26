# Domowik editor

The actively developed Flutter interface for Domowik. **Not a released
product:** platform/rendering and measured whole-app performance gates remain
open.

Flutter calls the shared Rust core through `dart:ffi` natively and through a
WebAssembly build of the same core in a browser; `lib/core/` holds the one
`DomowikCore` interface both implement. The current editor supports
walls/openings, levels, room naming, selection, snapping controls, undo/redo,
open/save, imported furniture, resizing and preferences. 3D runs on the shared
`wgpu` renderer on macOS (Metal) and in browsers (WebGPU); Linux has an
experimental native-texture 3D view. Earlier Android execution does not
establish current full-editor support. The browser editor's design, limits and
dependencies are [ADR 0012](../docs/decisions/0012-browser-editor.md).

Start with the repository guides:

- [Getting started](../docs/GETTING-STARTED.md): Linux/macOS prerequisites,
  CLI, native build, tests and troubleshooting.
- [User guide](../docs/USER-GUIDE.md): editing, exact platform shortcuts,
  assets, resize previews, 3D and split preferences.
- [Compatibility](../docs/COMPATIBILITY.md): experimental files and
  platform/feature limits.
- [Contributing](../CONTRIBUTING.md): source ownership and verification.

## Signing in to a server (optional)

The editor is anonymous and local by default and makes no network request
for accounts. A native build can sign in to a Domowik server when
`DOMOWIK_SERVER_URL` names one, as an environment variable at launch or a
`--dart-define` at build or run time (the environment wins):

```sh
DOMOWIK_SERVER_URL=https://planner.example flutter run -d macos
```

The URL is an HTTPS origin, or plain HTTP on loopback for local development.
A **Sign in** button then appears in the header. It opens the system browser
at the server's `/api/v1/auth/desktop`, receives a one-time code on a
loopback port (RFC 8252) and exchanges it with an S256 PKCE verifier at
`/api/v1/auth/desktop/token` for an API token. The editor knows only those
routes, not how the server signs people in. The token lives in memory until
sign-out, which revokes it, or until the app closes; it is never written to
disk. The browser build has no such button.

Degraded model instances participate in point-hit and marquee selection.
Optional instances can be deleted and restored by undo; required degradation
preserves inspection while refusing editing and normal saving. Declared
placeholders use transformed width/depth bounds; the fallback remains a fixed
1000 mm square. See [Compatibility](../docs/COMPATIBILITY.md) for current
selection and degraded-content limits.

## Layout and naming

The displayed name is **Domowik**; the Dart package and desktop executable
are `domowik_editor`. Development application IDs and native ABI symbols stay
unchanged so this move does not reset platform identity or break callers.

- `lib/`, `test/`, `benchmark/`: interface, regression tests and measurements.
  `lib/core/` is the core interface and its FFI and WebAssembly hosts;
  `lib/platform/` holds each host-specific service behind a conditional
  import (files, preferences, probes, 3D presenter). `test/web/` runs in a
  browser (`scripts/test-web.sh`).
- `web/`, `web-renderer/`: the browser page and the 3D renderer module
  (`wasm-bindgen` over `renderer/`); `scripts/build-web.sh` builds both
  WebAssembly modules and the Flutter web bundle.
- `native/`: Rust C ABI adapter and its C/Rust tests.
- `plugin/`: platform packaging and native build integration.
- `dart_harness/`: shared FFI declarations and standalone boundary tests.
- `Cargo.toml`: separate native workspace; artifacts go into `target/`.

## Rendering and the proposed portable backend

The shared `wgpu` renderer serves macOS and browser presenters. Linux uses a
native GL texture backend. Renderer-neutral scene data lives in
`crates/domowik-scene`. Remaining portable surface/lifecycle and performance
gates are described in [ADR 0003](../docs/decisions/0003-flutter-gui-wgpu-native.md).

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
platform probes.

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
presented frame time or input latency. Those whole-application budgets remain
open in the [roadmap](../docs/ROADMAP.md).
