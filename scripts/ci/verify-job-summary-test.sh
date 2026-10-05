#!/usr/bin/env bash
# Synthetic-log tests for scripts/ci/verify-job-summary.sh.
#
# The summary is the only thing most readers of a CI run will look at, so the
# ways it can be WRONG matter more than the way it is right: calling a real
# failure an interruption, claiming "no stages were skipped" on a run that
# never reached them, or reporting a pass whose exit status disagrees. Each
# case below is one of those.
set -euo pipefail

HERE=$(cd -P "$(dirname "$0")" && pwd -P)
SUMMARY="$HERE/verify-job-summary.sh"
[ -x "$SUMMARY" ] || { echo "no summary script at $SUMMARY" >&2; exit 2; }

WORK=$(mktemp -d -t domowik-summary-test)
trap 'rm -rf "$WORK"' EXIT

failures=0
n=0

# raw_case <name> <printf format for the status file> <must contain> [<must NOT contain>]
#
# `case_check` writes the status with `printf '%s\n'`, so it can only ever
# produce "<text>\n". The exact byte shapes below -- no final newline, extra
# blank lines, a CR, an embedded NUL -- are unreachable through it, which is
# why the trailing-blank-line hole went untested for as long as it did.
raw_case() {
  local name="$1" fmt="$2" want="$3" unwanted="${4:-}"
  n=$((n + 1))
  local log="$WORK/log.$n" exitf="$WORK/exit.$n" out
  printf '%s\n' "$PASS_LOG" > "$log"
  # shellcheck disable=SC2059
  printf "$fmt" > "$exitf"
  out=$("$SUMMARY" "$log" "$exitf" 2>&1) || {
    echo "FAIL  $name: summary script itself exited non-zero"
    echo "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  }
  if ! printf '%s' "$out" | grep -qF "$want"; then
    echo "FAIL  $name: summary did not contain '$want'"
    printf '%s' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  fi
  if [ -n "$unwanted" ] && printf '%s' "$out" | grep -qF "$unwanted"; then
    echo "FAIL  $name: summary contained '$unwanted'"
    printf '%s' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  fi
  echo "ok    $name"
}

# case <name> <log content|@missing> <exit status|@none> <must contain> [<must NOT contain>]
case_check() {
  local name="$1" log_content="$2" status="$3" want="$4" unwanted="${5:-}"
  n=$((n + 1))
  local log="$WORK/log.$n" exitf="$WORK/exit.$n" out
  if [ "$log_content" = "@missing" ]; then
    log="$WORK/definitely-not-here.$n"
  else
    printf '%s\n' "$log_content" > "$log"
  fi
  if [ "$status" = "@none" ]; then
    exitf=""
  else
    printf '%s\n' "$status" > "$exitf"
  fi

  out=$("$SUMMARY" "$log" $exitf 2>&1) || {
    echo "FAIL  $name: summary script itself exited non-zero"
    echo "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  }
  if ! printf '%s' "$out" | grep -qF "$want"; then
    echo "FAIL  $name: summary did not contain '$want'"
    printf '%s' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  fi
  if [ -n "$unwanted" ] && printf '%s' "$out" | grep -qF "$unwanted"; then
    echo "FAIL  $name: summary wrongly contained '$unwanted'"
    printf '%s' "$out" | sed 's/^/        /'
    failures=$((failures + 1))
    return
  fi
  echo "PASS  $name"
}

PASS_LOG='running stages
=== VERIFICATION PASSED === all stages'

PASS_WITH_SKIPS='running stages
Flutter editor checks: SKIPPED - flutter not on PATH
spike 0024 Flutter macOS application: SKIPPED - needs macOS and flutter on PATH
=== VERIFICATION PASSED === all stages'

FAIL_LOG='running stages

=== VERIFICATION FAILED ===
STAGE: cargo clippy
line:  131
exit:  101'

# The exact shape of GitHub run 34672441495: real failure, no banner.
SILENT_LOG='running stages
** BUILD FAILED **
Build process failed'

case_check "a clean pass" \
  "$PASS_LOG" 0 "**Verification PASSED.**" "did not complete"

case_check "a pass claims no skips only when there are none" \
  "$PASS_LOG" 0 "No stages were skipped."

case_check "a pass lists the stages it skipped" \
  "$PASS_WITH_SKIPS" 0 "Flutter editor checks: SKIPPED" "No stages were skipped."

case_check "a pass with skips says green did not mean checked" \
  "$PASS_WITH_SKIPS" 0 "did NOT check those stages"

case_check "a named failure reports its stage" \
  "$FAIL_LOG" 101 "at stage: cargo clippy" "did not complete"

case_check "a missing log is not a pass" \
  "@missing" 1 "No verification log found." "PASSED"

# The regression this whole issue turns on.
case_check "a silent non-zero exit is a failure, not an interruption" \
  "$SILENT_LOG" 1 "**Verification FAILED with no banner**" "may have been interrupted"

case_check "a silent non-zero exit does not claim skip-freedom" \
  "$SILENT_LOG" 1 "unreached stages remain unverified" "No stages were skipped."

# A banner and an exit status that disagree must not be resolved in favour of
# the banner: a tee/pipefail mistake could print a stale pass.
case_check "a pass banner with a non-zero exit is reported as a contradiction" \
  "$PASS_LOG" 3 "self-contradictory" "**Verification PASSED.**"

case_check "no banner and no exit status is still incomplete" \
  "$SILENT_LOG" "@none" "did not complete"

# AN EXIT STATUS IS A NUMBER OR IT IS NOTHING.
#
# The renderer used to strip non-digits out of the status file, so the text
# "failed0" became the number 0 and a run that said it failed was summarised as
# a verified pass. These four fix the shape of that hole: text that merely
# contains a zero, a status file that never appeared, a file with more than one
# value in it, and a real non-zero status under a pass banner.
case_check "text containing a zero is not an exit status" \
  "$PASS_LOG" "failed0" "UNCONFIRMED" "**Verification PASSED.**"

case_check "an absent exit status cannot confirm a pass banner" \
  "$PASS_LOG" "@none" "UNCONFIRMED" "**Verification PASSED.**"

case_check "a multi-line exit status is not a single value" \
  "$PASS_LOG" "0
1" "UNCONFIRMED" "**Verification PASSED.**"

case_check "an unconfirmed pass claims no skip-freedom" \
  "$PASS_LOG" "failed0" "unreached stages remain unverified" "No stages were skipped."

case_check "a real non-zero status under a pass banner is a contradiction" \
  "$PASS_LOG" 3 "self-contradictory" "**Verification PASSED.**"

# An empty status file is not a zero either.
case_check "an empty exit status file is not a zero" \
  "$PASS_LOG" "" "UNCONFIRMED" "**Verification PASSED.**"

# "00" is zero. A string comparison calls it non-zero and reports a
# contradiction against a run that in fact succeeded.
case_check "a zero written with a leading zero is still zero" \
  "$PASS_LOG" "00" "**Verification PASSED.**" "self-contradictory"

# THE EXACT BYTES OF THE STATUS FILE, which `case_check` cannot express.
#
# `raw=$(cat ...)` strips EVERY trailing newline, so "0\n\n\n" was accepted as
# a single line of digits and reported a verified pass, while the script's own
# comment claimed multiline content could not be. The accepted shape is now
# digits plus AT MOST ONE final newline. The first two cases are the shapes a
# real runner writes and must keep passing; the rest are the hole and its
# neighbours.
raw_case "a status written by echo is a zero" \
  '0\n' "**Verification PASSED.**" "UNCONFIRMED"

raw_case "a status written with no final newline is a zero" \
  '0' "**Verification PASSED.**" "UNCONFIRMED"

raw_case "trailing blank lines are not a single line of digits" \
  '0\n\n\n' "UNCONFIRMED" "**Verification PASSED.**"

raw_case "a CRLF status file is not a single line of digits" \
  '0\r\n' "UNCONFIRMED" "**Verification PASSED.**"

raw_case "an embedded NUL is not a single line of digits" \
  '0\000\n' "UNCONFIRMED" "**Verification PASSED.**"

raw_case "a file of newlines alone is not a zero" \
  '\n\n' "UNCONFIRMED" "**Verification PASSED.**"

raw_case "a real non-zero status with no final newline still contradicts" \
  '3' "self-contradictory" "**Verification PASSED.**"

# The LAST banner in the file is the result. An append-mode log carrying a
# stale failure block from an earlier run must not outrank the current pass.
STALE_THEN_PASS='=== VERIFICATION FAILED ===
STAGE: old-stage
line:  1
exit:  1
running again
=== VERIFICATION PASSED === all stages'
case_check "a stale failure banner does not outrank a later pass" \
  "$STALE_THEN_PASS" 0 "**Verification PASSED.**" "old-stage"

# And the converse, which matters more: a pass banner from an earlier run must
# not outrank the current run's failure.
STALE_PASS_THEN_FAIL='=== VERIFICATION PASSED === all stages
running again

=== VERIFICATION FAILED ===
STAGE: cargo test
line:  9
exit:  101'
case_check "a stale pass banner does not outrank a later failure" \
  "$STALE_PASS_THEN_FAIL" 101 "at stage: cargo test" "**Verification PASSED.**"

# THE SUMMARY'S INPUT MUST NOT BE POISONABLE BY ITS OWN SIBLING.
#
# scripts/ci/verify-banner-regression.sh runs as a verify.sh stage, and its
# fixtures print real `=== VERIFICATION PASSED === all stages` and
# `=== VERIFICATION FAILED ===` lines. Everything it emits goes into the same
# log this renderer reads. If one of those fixture banners ever reached the log
# unindented, a FAILING verify.sh run would carry a PASSED banner and be
# summarised as a pass.
#
# Today it cannot, because the regression captures fixture output and re-emits
# it indented. That is a property of one `sed 's/^/        /'`, which is far too
# quiet a thing to be load-bearing, so it is asserted here.
n=$((n + 1))
REGRESSION="$HERE/verify-banner-regression.sh"
if [ ! -x "$REGRESSION" ]; then
  echo "FAIL  the banner regression's output cannot poison this renderer: no script at $REGRESSION"
  failures=$((failures + 1))
else
  regression_out=$("$REGRESSION" 2>&1 || true)
  leaked=$(printf '%s\n' "$regression_out" |
    grep -cxF '=== VERIFICATION PASSED === all stages' || true)
  leaked_fail=$(printf '%s\n' "$regression_out" |
    grep -cxF '=== VERIFICATION FAILED ===' || true)
  if [ "$leaked" -ne 0 ] || [ "$leaked_fail" -ne 0 ]; then
    echo "FAIL  the banner regression leaked $leaked pass and $leaked_fail fail"
    echo "      banner line(s) at column 0; a verify.sh log containing its"
    echo "      output would be summarised from the fixture, not the run"
    failures=$((failures + 1))
  else
    echo "PASS  the banner regression's output cannot poison this renderer"
  fi
fi

echo
if [ "$failures" -ne 0 ]; then
  echo "job-summary tests FAILED: $failures of $n" >&2
  exit 1
fi
echo "job-summary tests passed: $n cases"
