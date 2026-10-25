#!/usr/bin/env bash
# Builds the Flutter Linux application, then checks the packaging claims
# directly rather than trusting that a successful build implies them.
#
# This mirrors the macOS stage in scripts/verify.sh check for check, because
# the point of the Linux slice is that the same questions get the same
# scrutiny on the platform ADR 0003's gate actually names.
#
# RUNS ON ANY LINUX HOST, not only inside this spike's container. It locates
# the tree relative to ITSELF and hard-codes no path, so a CI job on a runner
# that is already Linux invokes it directly with no Docker involved. That is
# the point: putting a Linux container on a Linux runner is a slower way to
# learn less. `docker/run-linux-app.sh` exists for developers whose own
# machine is not Linux.
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
spike=$(cd -P "$here/.." && pwd -P)
app="$spike"

missing=""
for tool in flutter cargo file readelf xvfb-run xauth timeout; do
  command -v "$tool" >/dev/null 2>&1 || missing="$missing $tool"
done
if [ -n "$missing" ]; then
  echo "error: not on PATH:$missing" >&2
  exit 1
fi
test -f "$app/pubspec.yaml" || {
  echo "error: no Flutter application at $app" >&2; exit 1; }

cd "$app"

echo "--- host ---"
uname -srm
flutter --version | head -2

echo "--- flutter build linux --release ---"
# CLEAN, for the same reason the macOS stage builds clean: an incremental
# build is not the artefact anyone ships, and a packaging assertion made
# against one is weaker than it looks.
flutter clean >/dev/null
flutter build linux --release

arch=$(uname -m)
case "$arch" in
  x86_64)  bundle=build/linux/x64/release/bundle ;;
  aarch64) bundle=build/linux/arm64/release/bundle ;;
  *) echo "error: no bundle path known for architecture $arch" >&2; exit 1 ;;
esac
bin="$bundle/domowik_editor"
core="$bundle/lib/libdomowik_bridge.so"

echo "--- scene packet preparation regressions ---"
bash "$spike/../scripts/check-linux-scene-packets.sh"

echo "--- packaging assertions ---"

# The core must be INSIDE the bundle. On macOS this needed a staticlib in a
# framework; here the cdylib goes in as-is, and this is the assertion that
# would notice if the plugin CMake stopped installing it.
test -f "$core" || { echo "the core is not inside the bundle" >&2; exit 1; }
file "$core"
file "$core" | grep -q "ELF 64-bit LSB shared object" || {
  echo "the bundled core is not an ELF shared object" >&2; exit 1; }

# The runner must look for its libraries NEXT TO ITSELF and nowhere else.
# An empty or absolute RUNPATH is how a bundle ends up working only on the
# machine that built it -- the ELF equivalent of the LC_ID_DYLIB defect the
# macOS slice hit. EXACT comparison: a substring match would accept a RUNPATH
# that merely contains $ORIGIN/lib alongside a build directory.
runpath=$(readelf -d "$bin" | sed -n 's/.*(RUNPATH).*\[\(.*\)\]/\1/p')
[ -z "$runpath" ] && runpath=$(readelf -d "$bin" | sed -n 's/.*(RPATH).*\[\(.*\)\]/\1/p')
echo "runner RUNPATH: [$runpath]"
test "$runpath" = '$ORIGIN/lib' || {
  echo "the runner RUNPATH is not exactly \$ORIGIN/lib" >&2; exit 1; }

# Nothing in the bundle may reference a path from the build tree. This is the
# check that would catch a core copied in with an absolute reference baked in.
#
# The path is DERIVED, not written as a literal. An earlier version of this
# grepped for the string "/build/spikes", which is this container's build
# directory and nobody else's: on any other machine the check would have found
# nothing, passed, and meant nothing. A check that can only succeed is the
# exact failure this spike keeps finding in its own evidence.
# The root is the SPIKE, not the app directory. An adversarial review caught
# this: cargo builds from the spike root and writes under
# <spike>/target/release, so an embedded path like
# /build/frontend/target/release/libdep.so does not
# contain the app directory string this used to search for. Searching the
# spike root covers the app directory too, since the app is inside it.
buildroot=$(cd -P "$spike" && pwd -P)

# The WHOLE bundle, not just lib/. The comment claimed "nothing in the bundle"
# while the code scanned one subdirectory, so the executable and the asset
# tree went unexamined. `grep -r` does not follow symlinks it meets while
# walking, and `cp -a` preserves symlinks, so -R is used to follow them.
refs_buildroot() { grep -RlF "$buildroot" "$1" >/dev/null 2>&1; }

if refs_buildroot "$bundle"; then
  echo "something in the bundle references the build directory $buildroot" >&2
  grep -RlF "$buildroot" "$bundle" >&2 || true
  exit 1
fi

# POSITIVE CONTROL, every run. The check above passes by finding nothing, and
# "found nothing" is also what a broken detector reports. So plant a file that
# does contain the build path, confirm the detector fires on it, and remove it.
# Without this the assertion above is unfalsifiable in place.
printf '%s\n' "$buildroot" > "$bundle/lib/.detector-control"
if refs_buildroot "$bundle"; then
  rm -f "$bundle/lib/.detector-control"
else
  rm -f "$bundle/lib/.detector-control"
  echo "the build-path detector did not fire on a planted control" >&2
  exit 1
fi
echo "build-path detector: no hits, and it fired on its control"

echo "--- probes, from inside the bundle ---"
# Xvfb because the GTK runner creates a real window and a GL context before
# the Dart entry point runs, even for the headless probes. Mesa software
# rendering because there is no GPU here.
export LIBGL_ALWAYS_SOFTWARE=1

run_probe() {
  local var=$1 want=$2 where=$3
  local out
  out=$(cd "$where" && timeout 300 xvfb-run -a env "$var=1" ./domowik_editor 2>/dev/null) || {
    echo "probe $var exited non-zero" >&2
    printf '%s\n' "$out" >&2
    exit 1
  }
  # WHOLE-LINE match. `grep -q "BUNDLE SELFTEST PASSED"` also accepts
  # "NOT BUNDLE SELFTEST PASSED" and "expected BUNDLE SELFTEST PASSED but
  # failed", so a probe that reported its own failure in prose could satisfy
  # the oracle. Found by adversarial review.
  printf '%s\n' "$out" | grep -Fxq "$want" || {
    echo "probe $var did not report: $want" >&2
    printf '%s\n' "$out" >&2
    exit 1
  }
  printf '%s\n' "$out" | grep -E "image:|checks|PASSED" || true
}

run_probe DOMOWIK_SPIKE_SELFTEST   'BUNDLE SELFTEST PASSED'   "$app/$bundle"
run_probe DOMOWIK_SPIKE_FIRSTFRAME 'FIRST FRAME PROBE PASSED' "$app/$bundle"

echo "--- negative control: an external core must NOT satisfy the probe ---"
# The exact attack an adversarial review described. Linux opens the BARE
# soname, so the ELF loader searches, and LD_LIBRARY_PATH is consulted BEFORE
# the executable's DT_RUNPATH. A working copy planted there is loaded in
# preference to the bundle's own, which means every behavioural check in the
# self-test would pass while proving nothing about the bundle.
#
# The defence is the dladdr provenance assertion inside the self-test, and
# this is what shows it actually holds instead of asserting that it does. The
# decoy is a byte-identical copy of the real core, so the ONLY thing that can
# distinguish it is where it was loaded from.
decoy=/tmp/decoy-core
rm -rf "$decoy"; mkdir -p "$decoy"
cp "$bundle/lib/libdomowik_bridge.so" "$decoy/"
decoy_out=$(cd "$app/$bundle" && LD_LIBRARY_PATH="$decoy" timeout 300 \
  xvfb-run -a env DOMOWIK_SPIKE_SELFTEST=1 ./domowik_editor 2>&1 || true)
rm -rf "$decoy"
if printf '%s\n' "$decoy_out" | grep -Fxq 'BUNDLE SELFTEST PASSED'; then
  echo "the self-test PASSED while loading a core from outside the bundle." >&2
  echo "the provenance assertion is not doing its job." >&2
  printf '%s\n' "$decoy_out" | grep -E "image:|checks" >&2
  exit 1
fi
printf '%s\n' "$decoy_out" | grep -E "image:|the core was loaded" | head -3 || true
echo "an external core is correctly refused"

echo "--- relocated copy ---"
# The macOS slice ran its bundle from the build directory and from /tmp. Same
# question here: does anything in the bundle depend on where it was built?
relocated=/tmp/domowik-relocated
rm -rf "$relocated"
cp -a "$app/$bundle" "$relocated"
# BOTH probes, not just the self-test. The self-test answers "can it still find
# and drive its core from here"; the first-frame probe answers "does it still
# render", and a bundle that relocated its library but not its assets would
# pass the first and fail the second.
run_probe DOMOWIK_SPIKE_SELFTEST   'BUNDLE SELFTEST PASSED'   "$relocated"
run_probe DOMOWIK_SPIKE_FIRSTFRAME 'FIRST FRAME PROBE PASSED' "$relocated"

# Hand the finished bundle to the caller so it can be run on a machine with no
# toolchain on it. Nothing is rebuilt there; this is the very same directory.
if [ -n "${DOMOWIK_EXPORT_BUNDLE:-}" ]; then
  echo "--- exporting the bundle for the clean-machine run ---"
  rm -rf "${DOMOWIK_EXPORT_BUNDLE:?}/bundle"
  mkdir -p "$DOMOWIK_EXPORT_BUNDLE"
  cp -a "$app/$bundle" "$DOMOWIK_EXPORT_BUNDLE/bundle"
  echo "exported to $DOMOWIK_EXPORT_BUNDLE/bundle"
fi

echo
echo "=== LINUX APPLICATION SLICE PASSED ==="
