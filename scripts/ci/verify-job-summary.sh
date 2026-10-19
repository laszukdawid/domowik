#!/usr/bin/env bash
# Render the GitHub job summary for a ./scripts/verify.sh run.
#
# Writes markdown to STDOUT; the caller appends it to $GITHUB_STEP_SUMMARY.
# Kept out of the workflow YAML so it can be exercised against synthetic logs
# by scripts/ci/verify-job-summary-test.sh, and so the Linux and macOS jobs
# cannot drift apart -- they were maintaining two copies of this logic.
#
# Usage: verify-job-summary.sh <verify log path> [<exit status file>]
#
# THE CONTRACT: report exactly one of these, and never let two of them be
# confused. A green step alone cannot tell them apart.
#
#   nolog                a setup step failed before verify.sh ran
#   failed               verify.sh named the stage it died in
#   unreported-failure   verify.sh exited non-zero and named NOTHING
#   passed               the pass banner and a validated exit 0 agree
#   unconfirmed          the pass banner, but no usable exit status to confirm it
#   contradiction        the pass banner is present but the exit status is not 0
#   incomplete           neither banner, and no exit status to judge by
#
# `unreported-failure` exists because of GitHub Actions run 34672441495: bash
# 3.2 on the macOS runner swallowed the failure banner, and the old summary
# called a genuine stage failure "did not complete ... may have been
# interrupted, e.g. by a timeout". scripts/verify.sh no longer does that, but
# a summary that can only describe the failures it expects is how the first
# one went unnoticed, so the case stays reportable.
set -euo pipefail

LOG="${1:?usage: $0 <verify log path> [<exit status file>]}"
EXIT_FILE="${2:-}"

# THE EXIT STATUS MUST BE EXACTLY ONE LINE OF DIGITS, or it is not a status.
#
# The first version of this stripped non-digits with `tr -cd '0-9'`, which
# turns the text "failed0" into the number 0 and reports a verified pass on a
# file that says the opposite. Anything absent, empty, multiline, or carrying a
# single non-digit character is `unknown`, and `unknown` is never evidence of
# success.
#
# THE FILE IS READ BYTE-EXACTLY, and that is the whole reason for the `od`
# below. `raw=$(cat "$EXIT_FILE")` strips EVERY trailing newline, so a file
# holding "0\n\n\n" was accepted as a single line of digits and reported a
# verified pass -- while the paragraph above claimed multiline content could
# not. The accepted shape is now stated exactly: an optional run of digits
# followed by at most ONE final newline, which is what `echo $? > file` and
# `printf %s` both produce and nothing else is.
#
# This never produced a WRONG verdict, and the fix is not a correctness repair.
# Command substitution can only hide newlines, so the digit run was always the
# file's sole non-empty line: "0\n1\n" and "\n\n0\n" were both already
# `unknown`. What was wrong was the CLAIM -- a strictness this script asserted
# and did not have -- and a check whose documented contract is not its real one
# is how the next reader reasons from the wrong rule.
#
# The read is guarded because this script runs in an `if: always()` step. A
# crash here would replace the result it exists to report with nothing at all,
# which is the failure mode it was written to prevent.
rc=unknown
if [ -n "$EXIT_FILE" ] && [ -f "$EXIT_FILE" ] && [ -r "$EXIT_FILE" ]; then
  # Octal bytes, space-separated: `od` survives NULs, CRs and a missing final
  # newline identically, none of which a shell read does.
  bytes=$(od -An -v -to1 "$EXIT_FILE" 2>/dev/null | tr -s ' \n' ' ' || true)
  # Strip at most one trailing newline byte (012), then require digits only.
  bytes=${bytes% }
  bytes=${bytes# }
  case "$bytes" in
    *" 012") bytes=${bytes%" 012"} ;;
    012) bytes="" ;;
  esac
  raw=""
  ok=yes
  for byte in $bytes; do
    case "$byte" in
      06[0-9]) raw="$raw$((8#$byte - 8#060))" ;;
      *) ok=no; break ;;
    esac
  done
  if [ "$ok" = yes ] && [ -n "$raw" ]; then
    rc="$raw"
  fi
fi

# Compared NUMERICALLY below. `[ "$rc" = 0 ]` calls the string "00" non-zero
# and reports a contradiction against a run that succeeded. Long strings of
# digits are rejected rather than fed to arithmetic.
rc_is_zero=no
case "$rc" in
  unknown) ;;
  ????????*) rc=unknown ;;
  *) [ "$((10#$rc))" -eq 0 ] && rc_is_zero=yes ;;
esac

echo "### Verification summary"

# THE LAST BANNER IN THE FILE IS THE RESULT.
#
# Matched anchored and full-line, so log text that merely contains these words
# cannot read as a result. Taking the LAST one rather than preferring any
# failure banner anywhere matters for an append-mode log: a stale failure block
# from an earlier run would otherwise outrank the current run's pass.
if [ -f "$LOG" ]; then
  last_banner=$(grep -nxF -e '=== VERIFICATION FAILED ===' \
                           -e '=== VERIFICATION PASSED === all stages' \
                "$LOG" | tail -1 || true)
else
  last_banner=""
fi

if [ ! -f "$LOG" ]; then
  STATUS=nolog
  echo "**No verification log found.** A setup step before \`verify.sh\` failed, or the job stopped before verification ran."
elif [ "${last_banner#*:}" = '=== VERIFICATION FAILED ===' ]; then
  STATUS=failed
  banner_line=${last_banner%%:*}
  stage=$(sed -n "$((banner_line + 1)),$((banner_line + 4))p" "$LOG" | grep '^STAGE:' | sed 's/^STAGE: *//' | head -1 || true)
  echo "**Verification FAILED** at stage: ${stage:-unknown (log had no STAGE line)}. Exit status: \`$rc\`."
elif [ "${last_banner#*:}" = '=== VERIFICATION PASSED === all stages' ]; then
  # A pass is claimed only when the banner and a VALIDATED exit status of 0
  # agree. The banner alone is one half of the evidence.
  if [ "$rc_is_zero" = yes ]; then
    STATUS=passed
    echo "**Verification PASSED.**"
  elif [ "$rc" = unknown ]; then
    STATUS=unconfirmed
    echo "**Verification is UNCONFIRMED:** the log carries the PASSED banner, but no usable exit status was recorded (the file is missing, empty, or not a single number). A banner without its exit status is half the evidence ED-0077 asks for; treat this as unverified rather than as a pass."
  else
    STATUS=contradiction
    echo "**Verification is self-contradictory:** the log carries the PASSED banner but \`verify.sh\` exited \`$rc\`. Treat this as a failure and investigate the discrepancy — one of the two is lying."
  fi
elif [ "$rc" != unknown ] && [ "$rc" != 0 ]; then
  STATUS=unreported-failure
  echo "**Verification FAILED with no banner** (exit \`$rc\`, and the log names no stage). \`verify.sh\` aborted without reporting where, which is itself a defect — see \`scripts/ci/verify-banner-regression.sh\`."
else
  STATUS=incomplete
  echo "**Verification did not complete** (no pass or fail banner in the log — the run may have been interrupted, e.g. by a timeout)."
fi

# The skip inventory. "No stages were skipped" is claimed ONLY on a confirmed
# pass: on any other outcome, the absence of a SKIPPED marker means stages were
# never reached, not that they ran.
if [ "$STATUS" = passed ]; then
  if grep -q 'SKIPPED' "$LOG"; then
    echo
    echo "Stages skipped:"
    echo
    grep 'SKIPPED' "$LOG" | sed 's/^/- /' || true
    echo
    echo "An all-green run with skips above did NOT check those stages."
  else
    echo
    echo "No stages were skipped."
  fi
else
  if grep -q 'SKIPPED' "$LOG" 2>/dev/null; then
    echo
    echo "Stages skipped before the run stopped:"
    echo
    grep 'SKIPPED' "$LOG" | sed 's/^/- /' || true
    echo
    echo "Stages after that point were never reached, not confirmed skip-free."
  else
    echo
    echo "No skip markers recorded — unreached stages remain unverified."
  fi
fi
