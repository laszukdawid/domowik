#!/bin/bash
# Fast macOS developer checks (issue 0097).
#
#   scripts/macos-check.sh          quick: Rust core + bridge + Flutter analyze/tests
#   scripts/macos-check.sh app      clean release build of the macOS app, bundle
#                                   packaging assertions, and both in-bundle probes
#   scripts/macos-check.sh all      quick, then app
#
# `quick` is the inner loop: everything that can fail on a code change, without
# the CLI probes, corpus gates, sanitizer harness and benchmark compiles that
# ./scripts/verify.sh adds. `app` runs exactly the commands of verify.sh's
# "frontend Flutter macOS application" stage. Neither replaces verify.sh, which
# remains the gate.
#
# Like verify.sh, every failure names its stage, and a run ends with one of two
# banners. Written for the bash 3.2 that macOS ships.
set -euo pipefail

REPO_ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$REPO_ROOT"
MODE=${1:-quick}
CURRENT_STAGE="setup"
START=$(date +%s)

stage() {
  CURRENT_STAGE="$1"
  echo "--- $1"
}

on_exit() {
  status=$?
  elapsed=$(( $(date +%s) - START ))
  if [ "$status" -eq 0 ]; then
    echo "=== MACOS CHECK PASSED === $MODE (${elapsed}s)"
  else
    echo "=== MACOS CHECK FAILED ==="
    echo "STAGE: $CURRENT_STAGE"
    echo "exit:  $status"
  fi
}
trap on_exit EXIT

case "$MODE" in
  quick | app | all) ;;
  *)
    echo "usage: $0 [quick|app|all]" >&2
    exit 2
    ;;
esac

stage "toolchain"
[ "$(uname -s)" = "Darwin" ] || { echo "macOS only" >&2; exit 1; }
for tool in cargo flutter xcodebuild pod; do
  command -v "$tool" >/dev/null 2>&1 || { echo "$tool is not on PATH" >&2; exit 1; }
done
xcodebuild -version | head -1
flutter --version 2>/dev/null | head -1

quick() {
  stage "cargo fmt"
  cargo fmt --all --check
  stage "cargo clippy"
  cargo clippy -q --workspace --all-targets -- -D warnings
  stage "cargo test"
  cargo test -q --workspace
  stage "bridge fmt, clippy, tests"
  (
    cd frontend
    cargo fmt --check
    cargo clippy -q --all-targets -- -D warnings
    cargo test -q
  )
  stage "flutter pub get"
  (cd frontend && flutter pub get >/dev/null)
  stage "flutter analyze"
  (cd frontend && flutter analyze)
  stage "flutter widget tests"
  (cd frontend && flutter test)
}

app() {
  stage "macOS release build (clean)"
  (
    cd frontend
    # Clean on purpose: an incremental release build rewrites App.framework
    # without re-sealing the outer bundle, and the signature check below then
    # fails on a build that is otherwise fine (see verify.sh).
    flutter clean >/dev/null
    flutter build macos --release
  )
  APP=frontend/build/macos/Build/Products/Release/domowik_editor.app
  BIN="$APP/Contents/MacOS/domowik_editor"
  stage "bundle packaging assertions"
  ./scripts/check-macos-bundle.sh "$APP"
  run_probe() {
    /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 180 "$@"
  }
  stage "bundle self-test probe"
  DOMOWIK_SPIKE_SELFTEST=1 run_probe "$BIN" 2>/dev/null | grep -q 'BUNDLE SELFTEST PASSED'
  stage "first frame probe"
  DOMOWIK_SPIKE_FIRSTFRAME=1 run_probe "$BIN" 2>/dev/null | grep -q 'FIRST FRAME PROBE PASSED'
  echo "app: $APP"
}

case "$MODE" in
  quick) quick ;;
  app) app ;;
  all) quick; app ;;
esac
