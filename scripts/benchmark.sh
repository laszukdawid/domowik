#!/usr/bin/env bash
# Whole-application benchmark (`task benchmark`): builds the release desktop
# editor, runs its scripted benchmark (DOMOWIK_BENCHMARK, lib/app_benchmark.dart)
# on a realistic plan and the campus-xl stress corpus, and writes a
# CONFIDENTIAL report about this machine.
#
# Output: private/benchmarks/<UTC stamp>-<host>/ (gitignored, owner-only):
#   REPORT.md         the report to read or carry to another machine
#   results.json      raw measurements, for `task benchmark:compare`
#   environment.json  hardware, OS, power and toolchain details
#   app.log           the application's own log
#
# Options: --no-build reuses the last release build; --out DIR changes the
# parent directory. Timings depend on the host and its load, so close other
# work and plug in a laptop first. Nothing here is a pass/fail gate.
set -euo pipefail
umask 077

root=$(cd "$(dirname "$0")/.." && pwd)
build=1
parent="$root/private/benchmarks"
while [ $# -gt 0 ]; do
  case "$1" in
    --no-build) build=0 ;;
    --out) parent=$2; shift ;;
    *) echo "usage: $0 [--no-build] [--out DIR]" >&2; exit 2 ;;
  esac
  shift
done

host=$(hostname -s 2>/dev/null || hostname)
host=${host//[^A-Za-z0-9._-]/_}
run="$parent/$(date -u +%Y%m%dT%H%M%SZ)-$host"
mkdir -p "$run"
chmod 700 "$parent" "$run"
echo "benchmark: writing to $run"

cd "$root"
cargo run --quiet --release --locked -p domowik-bench -- \
  --export campus-xl "$run/campus-xl.domo"
cp examples/four-level-house/four-level-house.domo "$run/four-level-house.domo"

case "$(uname -s)" in
  Darwin)
    [ "$build" = 1 ] && (cd frontend && flutter build macos --release)
    bin=frontend/build/macos/Build/Products/Release/domowik_editor.app/Contents/MacOS/domowik_editor
    ;;
  Linux)
    [ "$build" = 1 ] && (cd frontend && flutter build linux --release)
    case "$(uname -m)" in
      aarch64) bin=frontend/build/linux/arm64/release/bundle/domowik_editor ;;
      *) bin=frontend/build/linux/x64/release/bundle/domowik_editor ;;
    esac
    # Match `task run:gui:linux` on NVIDIA PRIME laptops unless overridden.
    if command -v nvidia-smi >/dev/null 2>&1; then
      export GDK_BACKEND=${GDK_BACKEND:-x11}
      export __NV_PRIME_RENDER_OFFLOAD=${__NV_PRIME_RENDER_OFFLOAD:-1}
      export __GLX_VENDOR_LIBRARY_NAME=${__GLX_VENDOR_LIBRARY_NAME:-nvidia}
    fi
    ;;
  *) echo "benchmark: desktop macOS or Linux only" >&2; exit 2 ;;
esac
[ -x "$bin" ] || { echo "benchmark: no release build at $bin" >&2; exit 1; }

python3 scripts/benchmark_report.py environment > "$run/environment.json"

# The macOS release app is sandboxed: it may read and write only its own
# container, so its inputs and results are staged there and moved out after.
stage="$run/stage"
if [ "$(uname -s)" = Darwin ]; then
  bundle_id=$(sed -n 's/^PRODUCT_BUNDLE_IDENTIFIER = //p' \
    frontend/macos/Runner/Configs/AppInfo.xcconfig)
  container="$HOME/Library/Containers/$bundle_id/Data"
  if [ ! -d "$container/tmp" ]; then
    # The first launch creates the container; the self-test exits at once.
    DOMOWIK_SPIKE_SELFTEST=1 "$bin" >/dev/null 2>&1 || true
  fi
  [ -d "$container/tmp" ] || { echo "benchmark: no sandbox container at $container" >&2; exit 1; }
  stage="$container/tmp/domowik-benchmark-$(basename "$run")"
fi
trap '[ "$stage" != "$run/stage" ] && rm -rf "$stage"' EXIT
mkdir -p "$stage/models"
cp "$run/four-level-house.domo" "$run/campus-xl.domo" "$stage/"
cp examples/four-level-house/models/*.glb "$stage/models/"
files=$(cd "$stage/models" && ls -1 ./*.glb | sed 's|^\./||' | paste -sd ';' -)

echo "benchmark: running scenarios (several minutes; keep the window visible)"
status=0
DOMOWIK_BENCHMARK="$stage/results.json" \
DOMOWIK_BENCHMARK_PLANS="four-level-house=$stage/four-level-house.domo;campus-xl=$stage/campus-xl.domo=furnish" \
DOMOWIK_BENCHMARK_MODELS="$stage/models" \
DOMOWIK_BENCHMARK_MODEL_FILES="$files" \
  /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 1800 "$bin" \
  >"$run/app.log" 2>&1 || status=$?
grep '^benchmark:\|BENCHMARK' "$run/app.log" || true
if [ "$status" != 0 ] || [ ! -s "$stage/results.json" ]; then
  echo "benchmark: the application failed (exit $status); see $run/app.log" >&2
  exit 1
fi
mv "$stage/results.json" "$run/results.json"
rm -rf "$stage"

python3 scripts/benchmark_report.py report "$run" > "$run/REPORT.md"
chmod 600 "$run"/*
echo "benchmark: CONFIDENTIAL report at $run/REPORT.md"
