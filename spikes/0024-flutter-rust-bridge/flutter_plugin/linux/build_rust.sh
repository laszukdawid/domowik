#!/usr/bin/env bash
# Builds the Domowik Rust core for Linux and puts it where CMake asked.
#
# Split out of CMakeLists.txt because the checks below are the useful part and
# they are unreadable as escaped CMake COMMAND arguments. It takes one
# argument: the destination path for the `.so`.
set -euo pipefail

dest=${1:?usage: build_rust.sh <destination .so path>}

# Flutter SYMLINKS the plugin into the app's ephemeral directory, so this
# script is reached through `linux/flutter/ephemeral/.plugin_symlinks/`. `..`
# is resolved LOGICALLY by the shell, so the obvious `$here/../..` lands in the
# symlink farm rather than in the spike. `cd -P` resolves the link first, which
# is the only form that works from either side of it. The macOS podspec learned
# this the same way.
here=$(cd -P "$(dirname "$0")" && pwd -P)
spike=$(cd -P "$here/../.." && pwd -P)

if [ ! -f "$spike/Cargo.toml" ]; then
  echo "error: no Cargo.toml at $spike; the plugin is not inside the spike tree" >&2
  exit 1
fi

# A build launched from an IDE or from the Flutter tool does not necessarily
# inherit a login shell's PATH, so a rustup-installed toolchain can be
# invisible. Failing loudly beats producing a bundle with no core in it, which
# links fine and dies at DynamicLibrary.open.
export PATH="$HOME/.cargo/bin:/usr/local/cargo/bin:/usr/local/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo is not on PATH. A Flutter build does not inherit a login" >&2
  echo "       shell's PATH; if rustup lives outside \$HOME/.cargo, set" >&2
  echo "       CARGO_HOME and add \$CARGO_HOME/bin to PATH for this build." >&2
  exit 1
fi

# PIN THE OUTPUT LOCATION. This is not tidiness, it is the fix for a
# demonstrated defect: this script used to run plain `cargo build` and then
# copy `$spike/target/release/libdomowik_bridge.so` unconditionally. With
# CARGO_TARGET_DIR set in the environment -- which several of this spike's own
# scripts do set -- cargo writes the FRESH library somewhere else, returns
# zero, and the copy picks up whatever stale file the default path still holds.
# Reproduced directly: with ABI_VERSION changed to 77 and CARGO_TARGET_DIR
# pointing elsewhere, cargo produced a 77 core and the bundle shipped and
# reported 12, with the self-test passing. That is the macOS stale-archive
# defect wearing different clothes, and an adversarial review found it.
#
# CARGO_BUILD_TARGET is unset for the same reason: a triple in the environment
# inserts a `<triple>/` component and moves the artefact out from under us.
export CARGO_TARGET_DIR="$spike/target"
unset CARGO_BUILD_TARGET

# `--release` regardless of the Flutter build mode, deliberately. This spike
# exists partly to measure the boundary, and a debug-profile core would make
# every number meaningless while still looking like a valid measurement.
#
# `cd` into the spike rather than relying on --manifest-path: rustup resolves a
# directory-override toolchain from the PROCESS's working directory, not from
# the manifest's, so building from elsewhere would silently use the machine's
# default toolchain instead of the one rust-toolchain.toml pins.
( cd "$spike" && cargo build --release --locked )

built="$CARGO_TARGET_DIR/release/libdomowik_bridge.so"
if [ ! -f "$built" ]; then
  echo "error: cargo reported success but $built does not exist" >&2
  exit 1
fi

# BELT AND BRACES, and it guards the whole class rather than the one mechanism
# above. If the artefact is older than any source file it is built from, it is
# stale no matter how it got that way, and shipping it would repeat the defect
# through some route nobody has thought of yet.
stale=$(find "$spike/src" "$spike/Cargo.toml" "$spike/Cargo.lock" \
          -type f -newer "$built" -print -quit 2>/dev/null || true)
if [ -n "$stale" ]; then
  echo "error: $built is older than $stale, so it is stale." >&2
  echo "       cargo returned success, which means it wrote its output" >&2
  echo "       somewhere other than $CARGO_TARGET_DIR." >&2
  exit 1
fi

# COPY into the CMake binary directory rather than bundling straight out of
# the source tree. Every configuration then owns its own file, so two
# concurrent builds cannot race on one path -- which is a known limitation of
# the macOS podspec, recorded there and not repeated here.
mkdir -p "$(dirname "$dest")"
cp -f "$built" "$dest"
