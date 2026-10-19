#!/usr/bin/env bash
# Issue 0061 acceptance: prove the macOS packaging gate can REJECT.
#
# scripts/check-macos-bundle.sh has four assertions about the built bundle.
# Until this script existed, none of them had ever been watched failing, so an
# all-green macOS job was equally consistent with "packaging is correct" and
# "the assertions are vacuous" -- a `[ -f ]` against a path that moved, an
# `otool -D` whose output format changed, a `lipo -archs` typo. This runs the
# real checker against deliberately broken COPIES of the real bundle and
# requires each one to fail FOR THE REASON IT WAS BROKEN, matched on the
# checker's stable `BUNDLE CHECK FAILED: <tag>` line. A negative test that
# accepts any non-zero exit would pass on a checker that always crashed.
#
# Usage: macos-bundle-negative-proof.sh <path to domowik_editor.app>
#
# The mutated copies live under a temp directory outside the checkout, so
# verify.sh's clean-tree assertion is unaffected, and none of them is ever
# executed.
set -euo pipefail

# bash 3.2 -- still /bin/bash on every macOS host -- does not run a top-level
# ERR trap when a subshell fails. Nothing here relies on one; failures are
# reported explicitly.

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <path to .app bundle>" >&2
  exit 2
fi

APP="$1"
[ -d "$APP" ] || { echo "no bundle at $APP" >&2; exit 2; }

HERE=$(cd -P "$(dirname "$0")/.." && pwd -P)
CHECKER="$HERE/check-macos-bundle.sh"
[ -x "$CHECKER" ] || { echo "no checker at $CHECKER" >&2; exit 2; }

WORK=$(mktemp -d -t domowik-negative-proof)
trap 'rm -rf "$WORK"' EXIT

failures=0
summary=""

record() {
  summary="${summary}$1"$'\n'
  echo "$1"
}

# `ditto` rather than `cp -R`: it preserves extended attributes and resource
# forks, which a code signature depends on. A `cp -R` copy can fail codesign
# on its own, and a control case that fails for that reason would make every
# mutation below unattributable.
# DECLARED ON SEPARATE LINES, deliberately. In `local a="$1" b="$a"`, bash
# expands every right-hand side in the ENCLOSING scope before the locals come
# into existence, so `$a` there is not the value just assigned. Under `set -u`
# with no outer `name` that aborts the script; with an outer `name` -- which
# `expect_rejection` happens to provide -- it silently uses the caller's value
# instead. The second form is the dangerous one, because it works by accident
# for every case whose caller-local name matches.
# EVERY COMMAND CHECKED EXPLICITLY. This runs inside `$( )`, and a command
# substitution does not inherit errexit's abort: a failing `ditto` followed by
# a successful `printf` would return 0 and hand back a path to a bundle that
# was never fully copied.
fresh_copy() {
  local name="$1"
  local dest
  dest="$WORK/$name/$(basename "$APP")" || return 1
  mkdir -p "$WORK/$name" || return 1
  ditto "$APP" "$dest" || return 1
  printf '%s' "$dest"
}

# CONTROL. An untouched copy must still PASS. Without this, four failing
# mutations prove nothing: they would be equally consistent with "copying the
# bundle breaks it".
control=$(fresh_copy control)
if "$CHECKER" "$control" >/dev/null 2>&1; then
  record "PASS  control: an unmutated copy still passes the checker"
else
  record "FAIL  control: an unmutated copy does NOT pass the checker -- every"
  record "      mutation result below is unattributable. Checker output:"
  "$CHECKER" "$control" 2>&1 | sed 's/^/        /' || true
  failures=$((failures + 1))
fi

# Each mutation: break one property, require the checker to exit non-zero AND
# name that property's tag.
expect_rejection() {
  local name="$1" want_tag="$2" description="$3"
  shift 3
  local copy out rc
  copy=$(fresh_copy "$name")
  # The mutation command receives the copy's path as its last argument.
  if ! "$@" "$copy" >/dev/null 2>&1; then
    record "FAIL  $name: could not apply the mutation ($description)"
    failures=$((failures + 1))
    return
  fi
  set +e
  out=$("$CHECKER" "$copy" 2>&1)
  rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then
    record "FAIL  $name: the checker ACCEPTED a bundle with $description"
    failures=$((failures + 1))
  elif [ "$(printf '%s\n' "$out" | grep -c '^BUNDLE CHECK FAILED: ' || true)" -ne 1 ]; then
    record "FAIL  $name: the checker emitted more than one failure tag, so the"
    record "      rejection cannot be attributed to one assertion. Output:"
    printf '%s\n' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
  elif printf '%s\n' "$out" | grep -Fxq "BUNDLE CHECK FAILED: $want_tag"; then
    record "PASS  $name: rejected as '$want_tag' ($description)"
  else
    record "FAIL  $name: rejected, but not as '$want_tag' -- the mutation may"
    record "      have been caught by an unrelated assertion. Checker output:"
    printf '%s\n' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
  fi
}

FW_REL="Contents/Frameworks/domowik_bridge_plugin.framework/domowik_bridge_plugin"

# Each mutation propagates its own failures: these run as an `if !` condition,
# where errexit is suppressed and only the LAST command decides the status. A
# `lipo` that wrote nothing but returned non-zero would otherwise be followed
# by a successful `mv` and reported as a mutation that applied.
mutate_remove_core() { rm -f "$1/$FW_REL" || return 1; }

# An absolute install name is the exact defect the staticlib-in-a-framework
# packaging exists to prevent: it resolves on the machine that built it and
# nowhere else.
mutate_absolute_id() {
  chmod u+w "$1/$FW_REL" || return 1
  install_name_tool -id /Users/nobody/build/domowik_bridge_plugin "$1/$FW_REL" || return 1
}

# Drop the x86_64 slice, leaving a bundle that runs only on the builder's own
# architecture.
mutate_thin_arch() {
  chmod u+w "$1/$FW_REL" || return 1
  lipo -thin arm64 "$1/$FW_REL" -output "$1/$FW_REL.thin" || return 1
  mv "$1/$FW_REL.thin" "$1/$FW_REL" || return 1
}

# A STALE NESTED ARCHIVE. This is the real failure verify.sh's `flutter clean`
# comment describes: App.framework rewritten without the outer bundle being
# re-sealed. It leaves the bridge framework untouched, so the first three
# assertions still pass and only the signature check can catch it.
mutate_stale_nested_archive() {
  # The VERSIONED path, not `App.framework/App`: the latter is a symlink into
  # `Versions/A`, and appending through a symlink is fragile where a plain
  # `-type f` search does not even see it.
  local nested
  nested="$1/Contents/Frameworks/App.framework/Versions/A/App"
  [ -f "$nested" ] || return 1
  chmod u+w "$nested" || return 1
  printf 'stale' >> "$nested" || return 1
}

expect_rejection missing-core     core-not-in-bundle     "the core removed from the bundle"        mutate_remove_core
expect_rejection absolute-id      framework-id-not-rpath "a framework id that is not @rpath-relative" mutate_absolute_id
expect_rejection thin-arch        missing-arch-slice     "the x86_64 slice removed"                mutate_thin_arch
expect_rejection stale-archive    signature-invalid      "a stale nested archive"                  mutate_stale_nested_archive

echo
if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "### macOS packaging gate — negative proof"
    echo
    echo "Deliberately broken copies of the built bundle, each required to be"
    echo "rejected by \`scripts/check-macos-bundle.sh\` for its own reason."
    echo
    echo '```'
    printf '%s' "$summary"
    echo '```'
  } >> "$GITHUB_STEP_SUMMARY"
fi

if [ "$failures" -ne 0 ]; then
  echo "negative proof FAILED: $failures of 5 cases did not behave as required" >&2
  exit 1
fi
echo "negative proof passed: 1 control + 4 rejections, each for its own reason"
