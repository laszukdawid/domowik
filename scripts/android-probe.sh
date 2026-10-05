#!/usr/bin/env bash
#
# Builds the issue-0024 spike for Android, installs it on a named AVD, and runs
# every probe inside the installed release APK.
#
# WHY THIS IS A SCRIPT AND NOT A VERIFY.SH STAGE. Android needs a JDK, an SDK,
# an NDK, an emulator and a booted virtual device, none of which exist on a
# machine that has not been set up for it, and a full run takes minutes rather
# than seconds. `scripts/verify.sh` stays runnable on a plain checkout; this is
# invoked deliberately, and its output is what the evidence document records.
#
# Nothing here is distributed. The APK is signed with the auto-generated debug
# key, for the reason ED-0099 gives about macOS: this is a development target.
#
# Usage:
#   scripts/android-probe.sh                 # build, install, functional probes
#   scripts/android-probe.sh --benchmark     # also measure; use a quiet machine
#   scripts/android-probe.sh --mutate-abi    # prove the APK cannot go stale
#
# Expects a booted device (see --help) and these in the environment:
#   ANDROID_HOME, JAVA_HOME, and flutter/cargo on PATH.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SPIKE="$REPO_ROOT/frontend"
APP="$SPIKE"
PKG="invalid.domowik_spike.domowik_spike_app"
PROBE_LOG="/sdcard/Android/data/$PKG/files/domowik-probe.log"
APK="$APP/build/app/outputs/flutter-apk/app-release.apk"

# ONE ABI. See frontend/android/app/build.gradle.kts for why this is passed
# here rather than set as an abiFilter.
TARGET_PLATFORM="android-arm64"
ABI="arm64-v8a"

WANT_BENCHMARK=0
MUTATE_ABI=0
for argument in "$@"; do
    case "$argument" in
        --benchmark) WANT_BENCHMARK=1 ;;
        --mutate-abi) MUTATE_ABI=1 ;;
        --help) ;;
        *) printf 'unknown option: %s\n' "$argument" >&2; exit 2 ;;
    esac
done

if [ "${1:-}" = "--help" ]; then
    sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'
    exit 0
fi

fail() { printf '\n=== ANDROID PROBE FAILED === %s\n' "$1" >&2; exit 1; }

# A mutation left behind by a run that was KILLED cannot be undone by any trap,
# so it is caught here instead. Without this, the next run would quietly build
# and measure a core whose ABI_VERSION is 99, and the only symptom would be one
# self-test line that a reader could easily take for the known provenance
# failure. Refuses rather than repairs: this is a tracked source file, and a
# script that silently rewrites one is worse than a script that stops.
if git -C "$REPO_ROOT" diff --quiet -- "$SPIKE/native/src/lib.rs"; then
    :
else
    printf 'frontend/native/src/lib.rs has uncommitted changes.\n' >&2
    printf 'If a --mutate-abi run was killed, restore it with:\n' >&2
    printf '  git checkout -- frontend/native/src/lib.rs\n' >&2
    fail "refusing to build from a modified bridge source"
fi

step() { printf '\n--- %s\n' "$1"; }

for tool in flutter cargo adb; do
    command -v "$tool" >/dev/null 2>&1 || fail "$tool is not on PATH"
done
[ -n "${ANDROID_HOME:-}" ] || fail "ANDROID_HOME is not set"
[ -n "${JAVA_HOME:-}" ] || fail "JAVA_HOME is not set"

# A device must be present BEFORE anything is built, so a missing emulator
# costs a second rather than a full build.
adb get-state >/dev/null 2>&1 || fail "no device; start the AVD first"
DEVICE_ABI="$(adb shell getprop ro.product.cpu.abi | tr -d '\r')"
[ "$DEVICE_ABI" = "$ABI" ] || fail "device is $DEVICE_ABI, this spike builds $ABI only"

DEVICE_DESC="Android $(adb shell getprop ro.build.version.release | tr -d '\r')"
DEVICE_DESC="$DEVICE_DESC / API $(adb shell getprop ro.build.version.sdk | tr -d '\r')"
DEVICE_DESC="$DEVICE_DESC, $DEVICE_ABI"
printf 'device: %s\n' "$DEVICE_DESC"

# Reading a probe's output needs root on the emulator: from Android 11 the
# shell cannot read another app's external files directory, and a release APK
# is not debuggable so `run-as` is unavailable. This is a property of the
# HARNESS, not of the app -- the app writes to its own storage either way.
adb root >/dev/null 2>&1 || true
adb wait-for-device
[ "$(adb shell whoami | tr -d '\r')" = "root" ] || \
    fail "adb is not root; probe output cannot be read back from this device"

step "build (release, $TARGET_PLATFORM)"
BUILD_START=$(date +%s)
(cd "$APP" && flutter build apk --release --target-platform "$TARGET_PLATFORM" >/dev/null)
BUILD_SECONDS=$(( $(date +%s) - BUILD_START ))
[ -f "$APK" ] || fail "no APK at $APK"

step "packaging assertions"
# The listings are taken ONCE into variables rather than re-piped into `grep
# -q` per assertion. Under `pipefail` a `grep -q` that matches exits at once,
# `unzip` is killed by SIGPIPE, and the pipeline reports failure for a check
# that actually PASSED -- which is exactly how this script failed the first
# time it was run, on an APK that was correct.
LISTING="$(unzip -l "$APK")"
VERBOSE_LISTING="$(unzip -v "$APK")"

# EXACTLY ONE ABI. `abiFilters` did not deliver this and its replacement must
# be checked rather than trusted -- Flutter packages its own engine libraries
# from the tool's target-platform list, so an APK carrying a second ABI would
# install on a device the Rust core was never built for and fail at
# DynamicLibrary.open instead of here.
ABIS="$(printf '%s\n' "$LISTING" | grep -oE 'lib/[^/]+/' | sort -u | tr -d ' ')"
[ "$ABIS" = "lib/$ABI/" ] || fail "expected only lib/$ABI/, found: $ABIS"

# The core must be IN the APK.
case "$LISTING" in
    *"lib/$ABI/libdomowik_bridge.so"*) ;;
    *) fail "the Rust core is not in the APK" ;;
esac

# STORED, not deflated. With extractNativeLibs=false the platform maps the
# library straight out of the APK, which is why the loaded image path reported
# by the probes contains "base.apk!/". A compressed entry would force an
# extraction step and change that claim.
printf '%s\n' "$VERBOSE_LISTING" | grep "lib/$ABI/libdomowik_bridge.so" | grep Stored >/dev/null || \
    fail "the core is not stored uncompressed; it cannot be mapped from the APK"

APK_BYTES=$(stat -f%z "$APK" 2>/dev/null || stat -c%s "$APK")
SO_PACKAGED=$(printf '%s\n' "$LISTING" | awk "/lib\/$ABI\/libdomowik_bridge.so/ {print \$1}")
SO_BUILT=$(find "$APP/build" -name 'libdomowik_bridge.so' -path '*release*' 2>/dev/null | head -1)
SO_BUILT_BYTES=$([ -n "$SO_BUILT" ] && (stat -f%z "$SO_BUILT" 2>/dev/null || stat -c%s "$SO_BUILT") || echo "?")

step "install"
adb install -r "$APK" >/dev/null

# Probes RECORD their verdict instead of aborting the run.
#
# Stopping at the first failure would mean one unfinished dependency hides
# every other probe's result, and the evidence this script exists to produce is
# the whole picture. The run still exits non-zero if anything failed.
FAILURES=""

run_probe() {
    local probe="$1" banner="$2"
    step "probe: $probe"
    adb shell rm -f "$PROBE_LOG" >/dev/null 2>&1 || true
    adb shell am force-stop "$PKG"
    adb shell am start -n "$PKG/.MainActivity" -e domowik_probe "$probe" >/dev/null 2>&1
    # A probe that hangs must fail the run rather than stall it.
    local waited=0 output=""
    while [ "$waited" -lt 120 ]; do
        output="$(adb shell cat "$PROBE_LOG" 2>/dev/null || true)"
        if printf '%s\n' "$output" | grep -E "$banner (PASSED|FAILED)" >/dev/null; then
            break
        fi
        sleep 2
        waited=$(( waited + 2 ))
    done
    # The emulator's software GL writes a great deal of noise to stderr, which
    # shares the redirected descriptor with the probe's own output.
    printf '%s\n' "$output" | grep -vE '^s_gl|^eglCodecCommon' || true
    if printf '%s\n' "$output" | grep "$banner PASSED" >/dev/null; then
        printf 'probe %s: PASSED\n' "$probe"
    else
        printf 'probe %s: FAILED\n' "$probe"
        FAILURES="$FAILURES $probe"
    fi
}

if [ "$MUTATE_ABI" = "1" ]; then
    # THE STALENESS TEST. The macOS slice once shipped a bundle containing a
    # Rust archive from an earlier build, and BOTH its probes passed on it. The
    # only convincing proof that this build graph cannot do the same is to
    # change the Rust source and require the installed app to notice.
    step "mutation: ABI_VERSION -> 99"
    ORIGINAL="$(grep -n '^const ABI_VERSION: i32 = ' "$SPIKE/native/src/lib.rs")"
    BEFORE="$(git -C "$REPO_ROOT" hash-object "$SPIKE/native/src/lib.rs")"
    # EXIT, INT and TERM. The EXIT trap alone was TESTED and does already run
    # on SIGINT in this bash, so naming the signals is hardening for other
    # shells rather than a fix for an observed failure -- said plainly because
    # a comment claiming to fix a bug nobody saw is how a file fills up with
    # cargo. Nothing traps SIGKILL, which is what the startup guard is for.
    trap 'git -C "$REPO_ROOT" checkout -- "$SPIKE/native/src/lib.rs"; echo "mutation reverted"' EXIT INT TERM
    printf 'was: %s\n' "$ORIGINAL"
    # perl, not `sed -i`. BSD and GNU sed disagree about how `-i` takes its
    # backup suffix, and the BSD reading of `-i'' -e` left a `src/lib.rs-e`
    # file behind in the source tree -- which the repository's clean-tree
    # verification stage would fail on, from a script whose whole purpose is to
    # leave the tree exactly as it found it. Observed, then fixed.
    /usr/bin/perl -pi -e 's/^const ABI_VERSION: i32 = \d+;$/const ABI_VERSION: i32 = 99;/' "$SPIKE/native/src/lib.rs"
    (cd "$APP" && flutter build apk --release --target-platform "$TARGET_PLATFORM" >/dev/null)
    adb install -r "$APK" >/dev/null
    adb shell rm -f "$PROBE_LOG" >/dev/null 2>&1 || true
    adb shell am force-stop "$PKG"
    adb shell am start -n "$PKG/.MainActivity" -e domowik_probe selftest >/dev/null 2>&1
    sleep 25
    MUTATED_OUTPUT="$(adb shell cat "$PROBE_LOG" 2>/dev/null || true)"
    REPORTED="$(printf '%s\n' "$MUTATED_OUTPUT" | grep -oE 'abi version \(got [0-9]+' | grep -oE '[0-9]+$' || true)"
    [ "$REPORTED" = "99" ] || \
        fail "the installed app reported ABI $REPORTED, not 99: the APK carried a STALE core"
    printf 'the installed app reported ABI %s: the build graph cannot serve a stale core\n' "$REPORTED"
    git -C "$REPO_ROOT" checkout -- "$SPIKE/native/src/lib.rs"
    trap - EXIT
    AFTER="$(git -C "$REPO_ROOT" hash-object "$SPIKE/native/src/lib.rs")"
    [ "$BEFORE" = "$AFTER" ] || fail "src/lib.rs was not restored ($BEFORE -> $AFTER)"
    printf 'src/lib.rs restored, hash %s\n' "$AFTER"
    printf '\n=== ANDROID MUTATION TEST PASSED ===\n'
    exit 0
fi

run_probe selftest   "BUNDLE SELFTEST"
run_probe firstframe "FIRST FRAME PROBE"

# The measurement probe is OPT-IN, because a timing run on a shared machine is
# only worth taking when nothing else is competing for the CPU. The functional
# probes establish correctness and can run whenever; the benchmark produces
# numbers that go into an evidence document, and a number taken while three
# other builds are running is a number nobody should quote. Pass --benchmark
# when the machine is quiet.
if [ "$WANT_BENCHMARK" = "1" ]; then
    run_probe benchmark "BENCHMARK PROBE"
else
    printf '\nprobe benchmark: SKIPPED - pass --benchmark on a quiet machine\n'
fi

# The lifecycle probe needs transitions driven from outside, so it is not a
# plain run_probe. It ends itself once it has seen a pause and a return.
step "probe: lifecycle"
adb shell rm -f "$PROBE_LOG" >/dev/null 2>&1 || true
adb shell am force-stop "$PKG"
adb shell settings put system accelerometer_rotation 0
adb shell settings put system user_rotation 0
adb shell am start -n "$PKG/.MainActivity" -e domowik_probe lifecycle >/dev/null 2>&1
sleep 8
PID_START="$(adb shell pidof "$PKG" | tr -d '\r')"
adb shell settings put system user_rotation 1   # landscape
sleep 4
adb shell settings put system user_rotation 0   # back to portrait
sleep 4
adb shell input keyevent KEYCODE_HOME           # pause
sleep 5
PID_BACKGROUND="$(adb shell pidof "$PKG" | tr -d '\r')"
adb shell am start -n "$PKG/.MainActivity" >/dev/null 2>&1   # resume
sleep 6
LIFECYCLE_OUTPUT="$(adb shell cat "$PROBE_LOG" 2>/dev/null || true)"
printf '%s\n' "$LIFECYCLE_OUTPUT" | grep -E '^LIFECYCLE' || true
if printf '%s\n' "$LIFECYCLE_OUTPUT" | grep 'LIFECYCLE PROBE PASSED' >/dev/null; then
    printf 'probe lifecycle: PASSED\n'
else
    printf 'probe lifecycle: FAILED\n'
    FAILURES="$FAILURES lifecycle"
fi
# Whether the process SURVIVED is part of what the run establishes: a probe
# whose process was replaced would be reporting about a different session.
printf 'pid at start %s, pid while backgrounded %s\n' "$PID_START" "$PID_BACKGROUND"
[ "$PID_START" = "$PID_BACKGROUND" ] || \
    printf 'NOTE: the process was replaced while backgrounded\n'

printf '\n'
printf 'device:            %s\n' "$DEVICE_DESC"
printf 'build seconds:     %s\n' "$BUILD_SECONDS"
printf 'apk bytes:         %s\n' "$APK_BYTES"
printf 'core, as built:    %s\n' "$SO_BUILT_BYTES"
printf 'core, as packaged: %s\n' "$SO_PACKAGED"

if [ -n "$FAILURES" ]; then
    printf '\n=== ANDROID PROBES FAILED ===%s\n' "$FAILURES"
    exit 1
fi
printf '\n=== ANDROID PROBES PASSED ===\n'
