#!/usr/bin/env bash
# Static packaging assertions for the issue-0024 macOS application bundle.
#
# EXTRACTED from scripts/verify.sh (issue 0061) without changing a claim. The
# extraction exists so the same assertions can be pointed at a DELIBERATELY
# BROKEN bundle, which is what scripts/ci/macos-bundle-negative-proof.sh does.
# A packaging gate that has never been observed rejecting anything is not
# evidence that it can.
#
# Usage: check-macos-bundle.sh <path to domowik_spike_app.app>
#
# Scope is what is ON DISK. The two probes that execute the bundle stay in
# verify.sh: they assert what the running process does -- that the image it
# loaded came from inside this bundle -- and a mutated bundle must never be
# executed to find that out.
#
# Every failure prints `BUNDLE CHECK FAILED: <tag>` on stderr before its
# human-readable reason. The tag is the contract the negative proof asserts
# against, so that a mutation is confirmed to fail for the reason it was
# injected rather than for any reason at all.
set -euo pipefail

fail() {
  printf 'BUNDLE CHECK FAILED: %s\n%s\n' "$1" "$2" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <path to .app bundle>" >&2
  exit 2
fi

APP="$1"
# A relative path is prefixed with ./ so that a bundle named, say, `-V` cannot
# be read as an option by `otool`, `lipo` or `codesign`. Without this a
# perfectly good bundle is rejected as `framework-id-not-rpath`, which is a
# false accusation rather than a missed one, and none of these tools accepts a
# `--` terminator to say otherwise.
case "$APP" in
  /*) ;;
  *) APP="./$APP" ;;
esac
[ -d "$APP" ] || { echo "no bundle at $APP" >&2; exit 2; }

FW="$APP/Contents/Frameworks/domowik_bridge_plugin.framework/domowik_bridge_plugin"

# The core must be INSIDE the bundle, universal, and carry an @rpath id.
# A cargo cdylib's install name is the absolute path of the developer's
# build directory, which works on the machine that built it and nowhere
# else; linking a staticlib into the framework is what fixes that, and
# this is the assertion that would notice if it stopped being true.
[ -f "$FW" ] || fail core-not-in-bundle "the core is not inside the bundle"

# EXACT comparisons, not substring greps. `grep -q arm64` also accepts
# `arm64e`, and a partial install-name match would accept an id that
# merely starts the same way.
want_id='@rpath/domowik_bridge_plugin.framework/Versions/A/domowik_bridge_plugin'
otool -D "$FW" | grep -Fxq "$want_id" ||
  fail framework-id-not-rpath "the framework id is not exactly $want_id"

archs=$(lipo -archs "$FW")
for arch in arm64 x86_64; do
  printf '%s\n' $archs | grep -Fxq "$arch" ||
    fail missing-arch-slice "the framework is missing the $arch slice (has: $archs)"
done

codesign --verify --deep --strict "$APP" 2>/dev/null ||
  fail signature-invalid "the bundle does not pass its own signature check"

echo "bundle packaging assertions passed: $APP"
