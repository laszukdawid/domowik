# Getting started

Build from a checkout of this repository. There is no published installer or
public release implied by these instructions. Run commands from the repository
root unless a step explicitly changes directory. Back up important plans;
see [compatibility](COMPATIBILITY.md).

## Toolchains

| Work | Requirements |
| --- | --- |
| CLI / Rust core | rustup and the toolchain selected by `rust-toolchain.toml` (currently Rust 1.98.0) |
| Desktop editor | Flutter stable with Dart satisfying `^3.11.4`, Rust, and the native tools below |
| Full verification | Rust, Python 3, shell/native utilities used by `scripts/verify.sh`; Flutter for GUI checks |

macOS CI pins Flutter 3.41.6. The Dart constraint alone does not establish
compatibility with every Flutter release. Check which SDK your shell invokes:

```sh
cargo --version
rustc --version
flutter --version
flutter doctor -v
```

The CLI does not need Flutter or Python to build. Go is no longer required
for any active build or verification workflow; corpus generation is Rust.

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

The first CocoaPods/Xcode build compiles the Rust archive. macOS has the 2D
editor; the current Linux native 3D backend is not a macOS renderer.
For release compilation and in-bundle probes, from `frontend/`:

```sh
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_editor.app/Contents/MacOS/domowik_editor
DOMOWIK_SPIKE_SELFTEST=1 "$APP"
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"
```

See the [bridge evidence](../frontend/README.md) for
what these probes establish and their limits.

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
| Cargo/rustup missing during a GUI build | Check the build process's PATH, not just your terminal; macOS expects the standard rustup location. |
| GTK or epoxy not found | Install development packages and check the pkg-config command above. |
| Flutter invokes an unexpected SDK | Check `command -v flutter` and `flutter --version`; version-manager shims may select another SDK. |
| Tests cannot load `libdomowik_bridge` | Build the separate frontend workspace in release mode at the expected location. |
| GUI cannot start in a headless shell | Use widget tests or release compilation; launching requires a desktop/display. Native-dialog checks additionally need Xvfb. |
| 3D fails or is unavailable | Use the 2D plan view; 3D currently requires Linux and an OpenGL-capable environment. |
| Preferences cannot be saved | Inspect the reported config error; back up the config before repairing it. Corrupt/unsupported config is not silently overwritten. |

Web support is planned in [issue 0088](../issues/0088-web-editor-support.md),
but deferred. The current application uses native FFI and filesystem APIs and
has no browser backend. The Taskfile's web task reports this without generating
unsupported scaffolding.
