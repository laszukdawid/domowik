# Getting started

Domowik is an experimental, local-first plan editor with a shared Rust core,
CLI and Flutter editor. These instructions build it from a checkout; they do
not imply a published installer or stable file format. Run commands from the
repository root unless a step explicitly changes directory. Back up important
plans; see [compatibility](COMPATIBILITY.md).

## Choose a first step

- **Explore the editor:** build it for [Linux](#run-the-linux-desktop-editor),
  [macOS](#run-the-macos-desktop-editor) or the
  [browser](#run-the-browser-editor), then open the
  [Welcome Home example](../examples/welcome-home/README.md). The plan includes
  its mock furniture.
- **Try the CLI:** build the Rust command-line tool and create a plan:

  ```sh
  cargo build --release --locked -p domowik-cli
  ./target/release/domowik --help
  ./target/release/domowik new first-plan.domo
  ./target/release/domowik info first-plan.domo
  ```

  Commands use millimetres. The
  [CLI walkthrough](../README.md#try-it) demonstrates walls, openings, rooms
  and image export.
- **Automate or use an AI agent:** the CLI is designed for repeatable workflows
  with structured output and stable point/ID references. Start with the
  [agent guide](AGENT-GUIDE.md); use `domowik <command> --help` for the exact
  command syntax.

The editor and CLI use the same document and geometry core. See
[Architecture](ARCHITECTURE.md) for how the application is organized and
[User guide](USER-GUIDE.md) for editing instructions.

## Toolchains

| Work | Requirements |
| --- | --- |
| CLI / Rust core | rustup and the toolchain selected by `rust-toolchain.toml` (currently Rust 1.98.0) |
| Desktop editor | Flutter 3.47.7 or later stable (Dart 3.13.5+), Rust, and the native tools below |
| Browser editor | The above, plus Python 3 to serve the build; `rust-toolchain.toml` installs the two WebAssembly targets, and the scripts fetch the pinned `wasm-bindgen-cli` and test browser on first use |
| Full verification | Rust, Python 3, shell/native utilities used by `scripts/verify.sh`; Flutter for GUI checks |

CI pins Flutter 3.47.7, and `frontend/pubspec.yaml` refuses older SDKs at
`flutter pub get`. A newer stable release is not verified until CI moves to it. Check which SDK your shell invokes:

```sh
cargo --version
rustc --version
flutter --version
flutter doctor -v
```

The CLI does not need Flutter or Python to build; corpus generation is Rust.

## Build the CLI

```sh
cargo build --release --locked -p domowik-cli
./target/release/domowik --help
./target/release/domowik new example.domo
./target/release/domowik info example.domo
./target/release/domowik validate example.domo
```

`new` refuses an existing destination unless explicitly told to overwrite it.
The [root README walkthrough](../README.md#try-it) adds walls, openings and
room names, then exports SVG and PNG. Use a command's `--help` for its flags.
Coordinates and dimensions in CLI commands are millimetres.

## Run the Linux desktop editor

You need a graphical desktop session, Flutter's Linux desktop toolchain,
Rust, a C/C++ compiler, CMake, Ninja, pkg-config, GTK 3 development files and
libepoxy development files. The current 3D experiment also requires a working
OpenGL context. On Debian/Ubuntu, the native build packages can be installed
with:

```sh
sudo apt-get install clang cmake ninja-build pkg-config libgtk-3-dev libepoxy-dev
```

This installs native prerequisites, not Flutter or Rust. Other distributions
use different package names. Confirm the tools are visible:

```sh
pkg-config --modversion gtk+-3.0 epoxy
flutter doctor -v
```

Then, from the repository root:

```sh
cd frontend
flutter pub get
flutter run -d linux
```

The plugin build compiles and bundles the Rust bridge automatically. The editor
opens an untitled plan. Choose **Walls** and drag in empty canvas to start.
The [user guide](USER-GUIDE.md) covers assets, preferences and 3D navigation.

To compile without launching a window, from `frontend/`:

```sh
flutter build linux --release
```

The executable is `build/linux/x64/release/bundle/domowik_editor` on x64.
Keep the entire bundle together: its `lib/` and `data/` directories are required.
This is a local development artifact, not a release-grade distribution.

## Open the included examples

Download [Welcome Home](../examples/welcome-home/welcome-home.domo?raw=true),
or use `examples/welcome-home/welcome-home.domo` from your checkout. In the
editor choose **Open**, select it, then **2D + 3D**. Its original mock furniture
is embedded, with no asset setup. See [the example guide](../examples/welcome-home/README.md)
for things to try and regeneration instructions.

For multiple storeys, download the
[Four-level house](../examples/four-level-house/four-level-house.domo?raw=true).
It has a common room, dining space, kitchen and bathroom on the ground floor,
three bedrooms and another bathroom upstairs, plus a small crawlspace and an
attic. Open it and select **Ground floor** in **Floor**; switch levels to see
the rest. Furniture, rugs, plants and illustrative stair/hatch models are
embedded; see its [layout and limitations](../examples/four-level-house/README.md).

## Run the macOS desktop editor

Install Flutter, Xcode with its command-line tools, CocoaPods and Rust via
rustup. The Xcode build expects `cargo` and `rustup` in `~/.cargo/bin`.
The plugin builds for the architectures Xcode requests; install both targets
for the universal release build used by CI:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
flutter doctor -v
```

From the repository root:

```sh
cd frontend
flutter pub get
flutter run -d macos
```

The first CocoaPods/Xcode build compiles the Rust archive. macOS supports 2D
and experimental 3D through the shared Rust renderer on Metal.

Quick checks on macOS, from the repository root:

```sh
./scripts/macos-check.sh        # Rust core + bridge + Flutter analyze/tests
./scripts/macos-check.sh app    # clean release build, bundle checks, probes
./scripts/macos-check.sh all    # both (about 1.5 minutes with warm caches)
```

The app targets macOS 12.0, the oldest deployment target Xcode 27 builds for.
`./scripts/verify.sh` remains the full gate.
For release compilation and in-bundle probes, from `frontend/`:

```sh
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_editor.app/Contents/MacOS/domowik_editor
DOMOWIK_SPIKE_SELFTEST=1 "$APP"
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"
```

See the [bridge evidence](../frontend/README.md) for
what these probes establish and their limits.

## Run the browser editor

The browser build uses the same Rust core, compiled to WebAssembly, and the
same `wgpu` renderer on WebGPU ([ADR 0012](decisions/0012-browser-editor.md)).
From the repository root:

```sh
task run:gui:web            # builds frontend/build/web, serves http://127.0.0.1:8080
```

which runs `frontend/scripts/build-web.sh` and a local static server. Open the
address in a current browser; 3D needs WebGPU, and 2D works without it. Any
static web server can host `frontend/build/web`; the page loads everything,
its text fonts included, from the bundle and contacts no other origin.
Characters the bundled Roboto lacks, such as CJK or emoji in a room name,
draw as boxes (ADR 0012 §6). For development with hot reload, build the modules into
`web/` once and use Flutter's runner:

```sh
cd frontend
./scripts/build-web-modules.sh web
flutter run -d chrome
```

### Linux: start the server and Chromium together

For the Intel/NVIDIA Linux setup, run:

```sh
task run:gui:web-browser
```

This builds the web bundle, starts a **static file server** bound only to
`127.0.0.1:8080`, waits until it is bound, and opens Chromium. There is no
application backend service: document processing runs inside the browser's
WebAssembly core. Close that Chromium instance or press Ctrl+C in the terminal
to stop the server. Save/download your plan before closing the browser.

Requirements: Linux, Bash, Python 3, `chromium` on PATH, X11 or XWayland
(`DISPLAY` set), and the Flutter/Rust web build prerequisites above. The tested
configuration is Chromium 155 on the owner's Intel UHD 630 / NVIDIA GTX 1050 Ti
machine; it is a workaround for that setup, not a claim of all Linux GPU support.

The launcher uses these **process-local** settings:

- `VK_LOADER_DRIVERS_SELECT='*intel*'` selects Intel Vulkan drivers. Without
  this restriction, the NVIDIA adapter initialized but presented a black pane.
- `--enable-features=Vulkan` makes hardware WebGPU adapters available in the
  tested Chromium configuration; otherwise adapter requests returned `null`.
- `--ozone-platform=x11` uses X11/XWayland rather than the problematic
  Wayland/Vulkan combination.
- A dedicated persistent browser profile at
  `$HOME/snap/chromium/common/domowik-webgpu` avoids changing your normal profile
  and avoids forwarding the launch to an existing browser that ignores new
  command-line GPU settings. The path is accessible to Snap Chromium. Close
  this profile's browser before launching it again. Preferences persist there;
  plans still live in your files, not in browser storage.

Normal browser sandboxing and GPU blocklists remain enabled. Do not add
`--no-sandbox`, unsafe-WebGPU or unsafe-SwiftShader flags for normal browsing.
An isolated-profile hardware probe rendered a room and model visibly; the
application's probe reported `WEB PROBE PASSED`. This is separate from the
headless software-GPU CI checks.

If another server (including `task run:gui:web`) already uses port 8080, stop
it first, or choose another port. The launcher will not reuse or kill it:

```sh
DOMOWIK_WEB_PORT=8081 task run:gui:web-browser
# Or choose an available port automatically; the launcher prints its URL:
DOMOWIK_WEB_PORT=0 task run:gui:web-browser
```

Overrides are optional:

```sh
DOMOWIK_WEB_BROWSER=/path/to/chromium \
DOMOWIK_WEB_PROFILE="$HOME/snap/chromium/common/domowik-alternate" \
DOMOWIK_WEB_PORT=8081 task run:gui:web-browser
```

`DOMOWIK_WEB_BROWSER` names an executable, not a command with extra arguments.
For a different GPU, `VK_LOADER_DRIVERS_SELECT='*'` allows normal Vulkan driver
selection, but that is **not** the validated workaround on this hybrid machine.
The launcher does not install drivers, update browsers, or configure the system.
The separately installed Google Chrome 133 is older than the tested Chromium
155; do not assume their Linux WebGPU behavior is equivalent.

If 3D still fails, use **Copy 3D error** and collect `chrome://gpu` diagnostics.
An available adapter alone does not prove visible presentation: also check that
walls appear in the 3D pane, rather than accepting a black canvas as success.

Browser tests run the real editor widgets on the WebAssembly core and WebGPU
renderer in a pinned headless Chrome for Testing, then build the release
bundle and run its in-app probe:

```sh
task test:web
```

## Run editor tests

Some widget tests load the real native bridge; a root-workspace build alone
does not create it because the frontend is a separate Cargo workspace. From the
repository root:

```sh
cd frontend
cargo build --release --locked
flutter pub get
flutter analyze
flutter test
```

Tests expect the bridge in the frontend's `target/release/` directory. If you set
`CARGO_TARGET_DIR` or `CARGO_BUILD_TARGET`, use a shell without those overrides
for this recipe. Linux tests load a `.so`; macOS tests load a `.dylib`.
See [Contributing](../CONTRIBUTING.md) for full verification and CPU benchmarks.

## Troubleshooting

| Symptom | Check |
| --- | --- |
| `CoreSimulator is out of date` / `DVTCoreDeviceCore` noise | macOS is older than Xcode's device frameworks expect. Harmless for macOS builds; a macOS update clears it. |
| Cargo/rustup missing during a GUI build | Check the build process's PATH, not just your terminal; macOS expects the standard rustup location. |
| GTK or epoxy not found | Install development packages and check the pkg-config command above. |
| Flutter invokes an unexpected SDK | Check `command -v flutter` and `flutter --version`; version-manager shims may select another SDK. |
| Tests cannot load `libdomowik_bridge` | Build the separate frontend workspace in release mode at the expected location. |
| GUI cannot start in a headless shell | Use widget tests or release compilation; launching requires a desktop/display. Native-dialog checks additionally need Xvfb. |
| 3D fails or is unavailable | macOS uses the shared Metal renderer; Linux needs an OpenGL-capable environment. A rendering failure shows in the 3D pane; use the 2D plan view meanwhile. The browser editor uses WebGPU; mobile/Windows 3D support remains unvalidated. |
| Preferences cannot be saved | Inspect the reported config error; back up the config before repairing it. Corrupt/unsupported config is not silently overwritten. |

| Browser tests hang at start | An installed Chrome under a sign-in policy opens its profile picker and never loads the test page. The scripts use the pinned Chrome for Testing instead; if you set `CHROME_EXECUTABLE`, point it at such a build. |
| Browser 3D says WebGPU is unavailable | Use a browser and device with WebGPU. Headless Linux has no GPU; the test scripts enable Chrome's software adapter there. 2D keeps working either way. |
| `wasm-bindgen` version mismatch | The CLI must equal the crate (0.2.127); `frontend/scripts/wasm-bindgen.sh` installs that version into `frontend/.tools`. |
