#!/usr/bin/env bash
# Runs the issue-0024 bridge against a REAL Linux kernel and reports.
#
# WHY THIS SCRIPT EXISTS. Until this ran, the spike's Linux coverage was a
# claim: "the source is platform-neutral and verify.sh selects `.so` when
# `.dylib` is absent". Both halves were true and neither had been executed.
# This executes them, and it is a script rather than a transcript so the result
# can be reproduced instead of believed (ED-0068).
#
# WHAT IT IS NOT. The guest is a Docker Desktop LinuxKit VM on an Apple Silicon
# host. The kernel, the libc, the ELF loader and the linker are genuinely
# Linux's; the CPU is shared with macOS and the filesystem is a bind mount.
# Functional results are therefore Linux results. TIMINGS ARE NOT bare-metal
# Linux timings and must not be quoted as though they were.
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
spike=$(cd -P "$here/.." && pwd -P)
repo=$(cd -P "$spike/.." && pwd -P)

image=domowik-0024-linux-abi:arm64
volume=domowik-0024-linux-target
platform=linux/arm64

if ! docker version >/dev/null 2>&1; then
  echo "linux abi slice: SKIPPED - no reachable Docker daemon" >&2
  exit 0
fi

docker build --platform "$platform" -f "$here/Dockerfile.linux-abi" \
  -t "$image" "$here" >/dev/null
docker volume create "$volume" >/dev/null

# The repository is mounted READ-ONLY and the sources are copied to a writable
# layer before anything runs. Two reasons, and the second is the load-bearing
# one: `dart pub get` writes `.dart_tool` next to the pubspec and fails on a
# read-only mount, and a writable bind mount would let the container leave
# artefacts in the working tree, which `verify.sh`'s clean-tree stage would
# then report as the developer's uncommitted work.
docker run --rm --platform "$platform" \
  -v "$repo":/work:ro \
  -v "$volume":/target \
  -e CARGO_TARGET_DIR=/target \
  -e PUB_CACHE=/target/pub-cache \
  -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" \
  "$image" bash -euo pipefail -c '
mkdir -p /build
# The ROOT manifest is required even though the spike is excluded from that
# workspace: the core crates inherit edition and rust-version from
# workspace.package, so without it cargo metadata fails to resolve the path
# dependencies with "failed to find a workspace root".
#
# NO APOSTROPHES ANYWHERE BELOW. This whole block is a single-quoted argument
# to bash -c, so one apostrophe in a comment ends the string and the rest of
# the script is reinterpreted by the outer shell.
cp -a /work/Cargo.toml /work/Cargo.lock /work/rust-toolchain.toml /build/
cp -a /work/crates /build/crates
cp -a /work/frontend /build/frontend
cd /build/frontend

echo "--- host ---"
uname -srm
rustc --version
dart --version

echo "--- cargo fmt --check ---"
cargo fmt --check

echo "--- cargo clippy -D warnings ---"
cargo clippy --all-targets --locked -- -D warnings

echo "--- cargo test ---"
cargo test --locked

echo "--- cargo build --release ---"
cargo build --release --locked
lib=/target/release/libdomowik_bridge.so
test -f "$lib"
file "$lib"

# The ELF must be a real Linux shared object for THIS architecture. A build
# that silently produced something else would still satisfy `test -f`.
file "$lib" | grep -q "ELF 64-bit LSB shared object"
file "$lib" | grep -q "ARM aarch64"

echo "--- C harness (asan+ubsan) ---"
mkdir -p /target/charness
clang -std=c11 -Wall -Wextra -Werror -g -fsanitize=address,undefined \
  -o /target/charness/charness native/charness/main.c
/target/charness/charness "$lib"

echo "--- Dart FFI harness ---"
cd dart_harness
dart pub get >/dev/null
dart run bin/harness.dart "$lib"
'

echo
echo "=== LINUX ABI SLICE PASSED === $image on $platform"
