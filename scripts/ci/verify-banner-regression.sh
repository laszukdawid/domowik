#!/usr/bin/env bash
# Issue 0061 regression: verify.sh must NAME a failing stage on every bash it
# can be run under, including a failure inside a `( ... )` subshell.
#
# WHY THIS EXISTS. GitHub Actions run 34672441495 failed in verify.sh's macOS
# bundle stage and printed neither `=== VERIFICATION FAILED ===` nor
# `=== VERIFICATION PASSED ===`. The cause is not in the stage: bash 3.2, which
# is /bin/bash on every macOS host and on GitHub's macos-15 runners, does not
# run a top-level ERR trap when a subshell fails. Under ED-0077 a run with no
# banner is not a reportable verification result, so every macOS failure was
# unreportable and the CI job summary mislabelled it as an interrupted run.
#
# WHAT IT TESTS. The REAL preamble, extracted verbatim from scripts/verify.sh
# between its FAILURE-REPORTING PREAMBLE markers -- not a copy that can drift
# -- exercised under every bash on this machine. Three properties, because the
# obvious fix trades one defect for another: `set -E` unconditionally makes
# bash >= 4 print the banner TWICE for one failure.
#
#   1. a failure inside a subshell prints EXACTLY ONE banner, names the stage
#      that was running, and reports the failing command's exit status;
#   2. a failure at the top level does the same;
#   3. a clean run prints no banner at all.
set -euo pipefail

HERE=$(cd -P "$(dirname "$0")/../.." && pwd -P)
VERIFY="$HERE/scripts/verify.sh"
[ -f "$VERIFY" ] || { echo "no verify.sh at $VERIFY" >&2; exit 2; }

WORK=$(mktemp -d -t domowik-banner-regression)
trap 'rm -rf "$WORK"' EXIT

PREAMBLE="$WORK/preamble.sh"
awk '/^# BEGIN FAILURE-REPORTING PREAMBLE$/{f=1;next} /^# END FAILURE-REPORTING PREAMBLE$/{f=0} f' \
  "$VERIFY" > "$PREAMBLE"
if [ ! -s "$PREAMBLE" ]; then
  echo "could not extract the failure-reporting preamble from $VERIFY --" >&2
  echo "the BEGIN/END markers are missing or renamed" >&2
  exit 1
fi
# The preamble assumes the flags verify.sh sets on its own second line. If that
# line ever changes, this test would silently stop testing the real thing.
grep -qxF 'set -euo pipefail' "$VERIFY" || {
  echo "verify.sh no longer sets 'set -euo pipefail'; this test's synthetic" >&2
  echo "scripts no longer match it" >&2
  exit 1
}

# The shells to exercise. `bash` on PATH and /bin/bash are different binaries
# on macOS (5.x from a package manager, 3.2 from the system) and the same one
# on a typical Linux box; duplicates are collapsed by resolved path.
shells=""
for candidate in /bin/bash "$(command -v bash 2>/dev/null || true)" /opt/homebrew/bin/bash /usr/local/bin/bash; do
  [ -n "$candidate" ] && [ -x "$candidate" ] || continue
  resolved=$(cd -P "$(dirname "$candidate")" && pwd -P)/$(basename "$candidate")
  case " $shells " in *" $resolved "*) continue ;; esac
  shells="$shells $resolved"
done
[ -n "$shells" ] || { echo "no bash interpreter found" >&2; exit 2; }

failures=0
saw_bash3=no
report() { printf '%s\n' "$1"; }

# body is appended after the preamble; each case fails in a different place.
make_case() {
  local dest="$1" body="$2"
  {
    echo '#!/usr/bin/env bash'
    echo 'set -euo pipefail'
    cat "$PREAMBLE"
    printf '%s\n' "$body"
    echo 'printf "\n=== VERIFICATION PASSED === all stages\n"'
  } > "$dest"
}

# A real failing COMMAND, not a bare `exit`. `exit` is not an error, so it
# never triggers an ERR trap -- a fixture built on it would report "no banner"
# on a shell whose trap works perfectly. Status 7 rather than 1 so a banner
# that reports the exit status correctly can be told apart from one that
# hardcodes a plausible value.
FAILING_COMMAND="sh -c 'exit 7'"
SUBSHELL_BODY='stage "nested stage"
if true; then
  (
    cd /
    '"$FAILING_COMMAND"'
  )
fi'
TOPLEVEL_BODY='stage "top-level stage"
'"$FAILING_COMMAND"''
CLEAN_BODY='stage "a stage that works"
true'

make_case "$WORK/subshell.sh" "$SUBSHELL_BODY"
make_case "$WORK/toplevel.sh" "$TOPLEVEL_BODY"
make_case "$WORK/clean.sh" "$CLEAN_BODY"

check_failure_case() {
  local sh="$1" label="$2" script="$3" want_stage="$4" version="$5"
  local out rc banners stage_lines exit_lines
  set +e
  out=$("$sh" "$script" 2>&1)
  rc=$?
  set -e

  banners=$(printf '%s\n' "$out" | grep -cxF '=== VERIFICATION FAILED ===' || true)
  stage_lines=$(printf '%s\n' "$out" | sed -n 's/^STAGE: *//p' | sort -u)
  exit_lines=$(printf '%s\n' "$out" | sed -n 's/^exit: *//p' | sort -u)

  if [ "$banners" -ne 1 ]; then
    report "FAIL  $version $label: expected exactly 1 failure banner, saw $banners"
    printf '%s\n' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  fi
  if [ "$stage_lines" != "$want_stage" ]; then
    report "FAIL  $version $label: banner named stage '$stage_lines', expected '$want_stage'"
    failures=$((failures + 1))
    return
  fi
  if [ "$exit_lines" != "7" ]; then
    report "FAIL  $version $label: banner reported exit '$exit_lines', expected '7'"
    failures=$((failures + 1))
    return
  fi
  if [ "$rc" -ne 7 ]; then
    report "FAIL  $version $label: script exited $rc, expected 7"
    failures=$((failures + 1))
    return
  fi
  report "PASS  $version $label: one banner, stage '$want_stage', exit 7"
}

check_clean_case() {
  local sh="$1" version="$2" out rc
  set +e
  out=$("$sh" "$WORK/clean.sh" 2>&1)
  rc=$?
  set -e
  if [ "$rc" -ne 0 ]; then
    report "FAIL  $version clean run: exited $rc, expected 0"
    failures=$((failures + 1))
  elif printf '%s\n' "$out" | grep -qxF '=== VERIFICATION FAILED ==='; then
    report "FAIL  $version clean run: printed a failure banner on a clean run"
    failures=$((failures + 1))
  elif ! printf '%s\n' "$out" | grep -qxF '=== VERIFICATION PASSED === all stages'; then
    report "FAIL  $version clean run: no pass banner"
    failures=$((failures + 1))
  else
    report "PASS  $version clean run: pass banner only, exit 0"
  fi
}

for sh in $shells; do
  version=$("$sh" -c 'echo "${BASH_VERSINFO[0]}.${BASH_VERSINFO[1]}"')
  report "--- $sh (bash $version) ---"
  case "$version" in 3.*) saw_bash3=yes ;; esac
  check_failure_case "$sh" "nested (subshell) failure" "$WORK/subshell.sh" "nested stage" "bash $version"
  check_failure_case "$sh" "top-level failure" "$WORK/toplevel.sh" "top-level stage" "bash $version"
  check_clean_case "$sh" "bash $version"
done

echo
# ED-0087 R3: a conditional check that passes quietly cannot be told apart
# from one that ran. Linux hosts have no bash 3.2, so the platform this
# regression was FOUND on is not covered there -- say so rather than implying
# the coverage exists.
if [ "$saw_bash3" = no ]; then
  echo "NOTE: no bash 3.x on this host, so the macOS /bin/bash case this"
  echo "      regression exists for was NOT exercised here. It is covered on"
  echo "      the macOS CI runner, where /bin/bash is 3.2."
fi

if [ "$failures" -ne 0 ]; then
  echo "banner regression FAILED: $failures case(s)" >&2
  exit 1
fi
echo "banner regression passed on every bash present"
