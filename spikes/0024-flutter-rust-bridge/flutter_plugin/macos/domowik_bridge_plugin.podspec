# CocoaPods integration for the Domowik bridge spike (issue 0024).
#
# WHAT THIS ANSWERS. The spike previously proved the FFI boundary and left
# "bundle packaging" recorded as assessed rather than answered. This is the
# answer for macOS: the Rust core is built as a STATIC library and linked into
# this pod's framework, which Xcode embeds in `Contents/Frameworks` and signs
# with the app.
#
# WHY A STATICLIB AND NOT THE CDYLIB. A cargo `cdylib` carries an `LC_ID_DYLIB`
# install name that is the ABSOLUTE PATH of the developer's build directory:
#
#   /Users/<someone>/.../target/release/deps/libdomowik_bridge.dylib
#
# Copying that into a bundle produces an app that works on the machine that
# built it and is broken everywhere else. Linking a staticlib into a framework
# makes Xcode set an `@rpath` id, sign it, and embed it — no `install_name_tool`
# surgery, and the same shape iOS requires.
#
# WHY -force_load. Nothing in this pod's own sources references the Rust
# symbols, so the linker would pull no object files out of the archive at all
# and the framework would export nothing. `-force_load` keeps the whole
# archive. It is the reason the framework is large; see the spike README.
Pod::Spec.new do |s|
  s.name             = 'domowik_bridge_plugin'
  s.version          = '0.0.0'
  s.summary          = 'Links the Domowik Rust core into a Flutter macOS app.'
  s.description      = <<-DESC
Spike plugin for issue 0024. Builds the Rust bridge crate as a static library
and links it into this pod's framework so a Flutter macOS application can load
it through dart:ffi from inside its own bundle. Not a product component.
                       DESC
  # No project URL is asserted. docs/NAMING.md records that no namespace is
  # reserved and that reservation is forbidden pending separately established
  # authority; `domowik.dev` is registered by a third party (dossier O4). The
  # RFC 2606 `.invalid` TLD is used wherever a value is structurally required,
  # because it can never be delegated to anyone.
  s.homepage         = 'https://domowik-spike.invalid/'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Domowik Contributors' => 'nobody@domowik-spike.invalid' }

  s.source           = { :path => '.' }
  s.source_files     = 'Classes/**/*'
  s.dependency 'FlutterMacOS'
  s.platform = :osx, '10.15'

  # Builds the Rust core for exactly the architectures Xcode asked for.
  #
  # `--release` regardless of the Flutter build mode, deliberately: this spike
  # exists partly to measure the boundary, and a debug-profile Rust core would
  # make every number meaningless while looking like a valid measurement.
  s.script_phase = {
    :name => 'Build Domowik Rust core',
    :execution_position => :before_compile,
    # BOTH lines are required; each answers a different failure.
    #
    # `:always_out_of_date` forces the phase to run every build. Without it,
    # Xcode treats the phase as up to date whenever its declared output
    # exists, skips the script, and the framework links last build's archive
    # — a stale ABI that both bundle probes accepted.
    #
    # `:output_files` declares who PRODUCES the archive. `OTHER_LDFLAGS`
    # below hands it to `-force_load`, and Xcode validates that operand as a
    # build INPUT during dependency analysis, before any script phase runs.
    # With no declared producer there is no edge between the two, so a
    # checkout without an archive failed with "Build input file cannot be
    # found … Did you forget to declare this file as an output of a script
    # phase". `rust/` is gitignored, so that reproduced only on a fresh clone
    # (issue 0057).
    #
    # REQUIRED REGRESSION, issue 0057: a clean-checkout build with no archive,
    # and a bumped-ABI rebuild WITHOUT `flutter clean` whose new value is read
    # both from the bundle self-test and from the built framework's exported
    # symbol. Both observed in a fresh clone at `e4dbfbe`; issue 0057 records
    # the commands, exit statuses and logs.
    :always_out_of_date => '1',
    :output_files => ['$(PODS_TARGET_SRCROOT)/rust/libdomowik_bridge.a'],
    :script => <<-SCRIPT
set -euo pipefail

# Xcode does not inherit a login shell's PATH, so a toolchain installed by
# rustup is invisible here unless it is named. Failing loudly beats emitting a
# framework with no Rust symbols in it, which links cleanly and crashes at
# `DynamicLibrary.open`.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
for tool in cargo rustup; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: $tool is not on PATH. Xcode does not inherit a login shell's" >&2
    echo "       PATH; if rustup lives outside \$HOME/.cargo, set CARGO_HOME and" >&2
    echo "       add \$CARGO_HOME/bin to PATH for this build." >&2
    exit 1
  fi
done

# Flutter SYMLINKS the plugin into the app's ephemeral directory, so
# `$PODS_TARGET_SRCROOT` is a path through
# `Flutter/ephemeral/.symlinks/plugins/`. `..` is resolved LOGICALLY by the
# shell, so the obvious `$PODS_TARGET_SRCROOT/../..` lands in the symlink
# farm rather than in the spike, and cargo reports a manifest that does not
# exist. `cd -P` resolves the link first, which is the only form that works
# from either side of it.
PLUGIN_MACOS=$(cd -P "$PODS_TARGET_SRCROOT" && pwd -P)
SPIKE=$(cd -P "$PLUGIN_MACOS/../.." && pwd -P)
OUT="$PLUGIN_MACOS/rust"
mkdir -p "$OUT"

if [ ! -f "$SPIKE/Cargo.toml" ]; then
  echo "error: no Cargo.toml at $SPIKE; the plugin is not inside the spike tree" >&2
  exit 1
fi

slices=""
for arch in $ARCHS; do
  case "$arch" in
    arm64)  triple=aarch64-apple-darwin ;;
    x86_64) triple=x86_64-apple-darwin ;;
    *) echo "error: no Rust target mapping for architecture '$arch'" >&2; exit 1 ;;
  esac
  if ! ( cd "$SPIKE" && rustup target list --installed ) | grep -qx "$triple"; then
    echo "error: Rust target $triple is not installed (needed for arch $arch)." >&2
    echo "       run: rustup target add $triple" >&2
    exit 1
  fi
  # `cd` into the spike rather than passing --manifest-path alone. Rustup
  # picks a directory-override toolchain from the PROCESS's current
  # directory, not from the manifest's, so building from Xcode's working
  # directory would silently use the machine's default toolchain instead of
  # the one pinned in rust-toolchain.toml.
  ( cd "$SPIKE" && cargo build --release --target "$triple" )
  slices="$slices $SPIKE/target/$triple/release/libdomowik_bridge.a"
done

# `lipo -create` with one slice is a copy; with several it is a real universal
# archive. Doing it unconditionally keeps one code path.
#
# KNOWN LIMITATION: every configuration writes this one path in the plugin
# source tree, so two concurrent Xcode builds with different ARCHS can race
# and leave the wrong slice set behind. Acceptable for a spike driven by one
# `flutter build` at a time; a product build would write per-configuration
# into DERIVED_FILE_DIR instead.
lipo -create $slices -output "$OUT/libdomowik_bridge.a"
    SCRIPT
  }

  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'OTHER_LDFLAGS' => '-force_load ${PODS_TARGET_SRCROOT}/rust/libdomowik_bridge.a',
  }
end
