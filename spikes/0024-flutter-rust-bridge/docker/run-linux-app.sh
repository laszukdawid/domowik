#!/usr/bin/env bash
# Builds and runs the issue-0024 Flutter application ON LINUX.
#
# WHAT THIS ANSWERS. ADR 0003's gate is about target Phase 2 platforms, and it
# names Linux and Android. The macOS application answered bundle packaging for
# macOS and explicitly did not move that gate. This answers it for Linux.
#
# ARCHITECTURE, AND WHY IT IS NOT A DETAIL. Flutter publishes no stable Linux
# arm64 archive, so this runs linux/amd64, which on an Apple Silicon host is
# EMULATED. Everything it checks -- the ELF loader, RUNPATH resolution, the GTK
# runner, the bundle layout, the dladdr provenance of the loaded image -- is
# genuinely Linux behaviour and fails here exactly as it would on a Linux
# machine. TIMINGS UNDER EMULATION ARE MEANINGLESS and this script produces
# none; the measured numbers in the spike README are the macOS ones, and the
# native-arm64 comparison comes from run-linux-abi.sh.
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
spike=$(cd -P "$here/.." && pwd -P)
repo=$(cd -P "$spike/../.." && pwd -P)

image=domowik-0024-linux-app:amd64
platform=linux/amd64

if ! docker version >/dev/null 2>&1; then
  echo "linux application slice: SKIPPED - no reachable Docker daemon" >&2
  exit 0
fi

docker build --platform "$platform" -f "$here/Dockerfile.linux-app" \
  -t "$image" "$here" >/dev/null

# The repository is mounted READ-ONLY and copied to a writable layer: a Flutter
# build writes into the project tree, and a writable bind mount would leave
# those artefacts in the working tree for verify.sh's clean-tree stage to
# report as the developer's uncommitted work.
export_volume=domowik-0024-linux-bundle
docker volume create "$export_volume" >/dev/null

# A named volume is created root-owned, and both containers run unprivileged,
# so the export would fail with "Permission denied". The uid is READ FROM THE
# IMAGE rather than assumed to be 1000, because a hard-coded uid is wrong the
# first time the base image changes how it creates users.
build_uid=$(docker run --rm --platform "$platform" "$image" id -u)
build_gid=$(docker run --rm --platform "$platform" "$image" id -g)
docker run --rm --platform "$platform" --user 0:0 \
  -v "$export_volume":/export "$image" \
  chown -R "$build_uid:$build_gid" /export

docker run --rm --platform "$platform" \
  -v "$repo":/work:ro \
  -v "$export_volume":/export \
  -e DOMOWIK_EXPORT_BUNDLE=/export \
  --name domowik-0024-linux-app \
  "$image" bash -euo pipefail -c '
mkdir -p /build
cp -a /work/Cargo.toml /work/Cargo.lock /work/rust-toolchain.toml /build/
cp -a /work/crates /build/crates
cp -a /work/spikes /build/spikes
exec /build/spikes/0024-flutter-rust-bridge/scripts/linux-app-probe.sh
'

# ---- The clean machine -----------------------------------------------------
#
# The residual this answers is "no clean-machine install was tested". Running a
# bundle from a different directory on the SAME machine does not answer it: that
# machine still has Rust, Flutter, a Dart SDK and every GTK development package
# on it, so a bundle that quietly depended on one of them would still run.
#
# This image has runtime libraries and nothing else, and asserts that no
# toolchain is present. The bundle is not rebuilt; it is the same directory,
# carried over in a volume.
clean_image=domowik-0024-linux-clean:amd64
docker build --platform "$platform" -f "$here/Dockerfile.linux-clean" \
  -t "$clean_image" "$here" >/dev/null

docker run --rm --platform "$platform" \
  -v "$export_volume":/export:ro \
  --name domowik-0024-linux-clean \
  "$clean_image" bash -euo pipefail -c '
echo "--- clean machine ---"
for tool in cargo rustc flutter dart clang cmake pkg-config; do
  command -v "$tool" >/dev/null 2>&1 && { echo "error: $tool present" >&2; exit 1; }
done
echo "no cargo, rustc, flutter, dart, clang, cmake or pkg-config on this machine"

# The bundle is mounted read-only, and a GTK application wants somewhere to
# write. Copy it to the unprivileged users home and run it from there, which
# is also what installing it would look like.
cp -a /export/bundle /home/runner/app
cd /home/runner/app
export LIBGL_ALWAYS_SOFTWARE=1

# Each probe must report ITS OWN marker, matched as a whole line. The earlier
# form accepted either marker for either probe and matched substrings, so a
# probe printing the OTHER banner, or prose containing one, satisfied it.
# Adversarial review found the substring half; the interchangeable half was
# next to it. (No apostrophes in this block: it is a single-quoted argument
# to bash -c, so one would end the string.)
check_probe() {
  var=$1
  want=$2
  out=$(timeout 300 xvfb-run -a env "$var=1" ./domowik_spike_app 2>&1) || {
    echo "clean-machine probe $var exited non-zero" >&2
    printf "%s\n" "$out" >&2
    exit 1
  }
  printf "%s\n" "$out" | grep -Fxq "$want" || {
    echo "clean-machine probe $var did not report: $want" >&2
    printf "%s\n" "$out" >&2
    exit 1
  }
  printf "%s\n" "$out" | grep -E "image:|checks,|PASSED" || true
}
check_probe DOMOWIK_SPIKE_SELFTEST   "BUNDLE SELFTEST PASSED"
check_probe DOMOWIK_SPIKE_FIRSTFRAME "FIRST FRAME PROBE PASSED"

echo
echo "=== LINUX CLEAN-MACHINE RUN PASSED ==="
'
