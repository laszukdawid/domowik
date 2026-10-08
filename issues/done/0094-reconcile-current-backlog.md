# 0094: Reconcile the tracker with the current application

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0091, 0092, 0093

## Scope

Review nonterminal issues against current Rust/frontend artifacts. Separate
delivered work, genuine remaining criteria, owner decisions and historical
records. Do not implement product changes or publish anything in this task.

## Acceptance criteria

- [x] Every nonterminal issue has a dated current disposition and next action.
- [x] Obsolete implementation claims and tracker-publication instructions are
      explicitly superseded; historical criteria and evidence remain readable.
- [x] Completed work is closed only with current artifact/test evidence;
      unresolved product, platform and release obligations remain assigned.
- [x] Index/status/location consistency, local links and relevant closure
      regressions pass locally; verification scope is recorded accurately.

## Provenance

Owner-requested repository review, 2026-10-05. Only Domowik source,
specifications, tests and checked-in evidence are used.

## Review outcome

All 35 previously nonterminal issues were triaged. Issues 0015 and 0064 close
against actual implementations and current local CLI/container regressions;
33 remain nonterminal. Issues 0055/0058/0061 become partial scope; 0060 becomes
Blocked on its named decisions. No whole remaining issue was shown obsolete.

The current backlog and per-issue dispositions supersede outdated missing-
feature, branch/worker and Go/spike-path claims without erasing original
criteria or historical evidence. Four titles now describe remaining work
rather than an absent application or missing CI jobs. Issue filenames remain
stable for existing references except the two completed files moved to done/.
Active scaffold dependencies now point to Rust foundation 0037 rather than
historical Go scaffold 0008; 0018 explicitly names its identity gate 0034.

## Verification

- `cargo test --locked --offline -p domowik-cli -p domowik-container` passed
  locally on Linux. In particular, L11 admission/refusal and all empty/non-
  empty profile/reporting regressions ran and passed.
- `domowik-tracker-check`, relative file-link checks, dated-disposition
  coverage and `git diff --check` passed on the working tree.
- Initial canonical run: `=== VERIFICATION FAILED ===`, stage `frontend C
  harness (asan+ubsan)`, exit 1. Direct reproduction identified LeakSanitizer's
  fatal ptrace-confinement restriction, not a reported assertion or memory
  defect. Logs: `/tmp/domowik-tracker-verify.log` and
  `/tmp/domowik-tracker-charness.log`.
- Canonical verification reran outside that restriction in a clean disposable
  local snapshot at `/tmp/domowik-tracker-check.48j6Fd`: exit 0,
  `=== VERIFICATION PASSED === all stages`. All sanitizer checks remained
  enabled, with `DOMOWIK_REQUIRE_LINUX_EDITOR=1`, Flutter 3.41.6 and four Cargo
  build jobs. Log: `/tmp/domowik-tracker-verify-unsandboxed.log`.
  Rust/CLI/native tests, 305 Flutter widget tests, CPU harnesses and mandatory
  Linux release build/runtime probes ran. macOS application checks were
  explicitly skipped on Linux; no macOS or hardware-frame claim is made.
- Final edits after snapshot capture affect tracker metadata, titles and this
  closure record only. No application/build/test source changed. Tracker,
  links, disposition coverage and whitespace checks were repeated on the
  final working tree. The clean snapshot avoids committing the user's
  workspace merely to pass the clean-tree gate.

No application source changed. Nothing was committed or pushed in the user's
repository, and no GitHub run or publication action was caused.

## Closure

- **Implementation:** working-tree documentation/tracker changes requested
  by the owner; no commit requested. Original application baseline `a3cc8ee`.
- **Follow-up issues:** existing nonterminal issues retain their obligations;
  see `issues/CURRENT-BACKLOG.md` from the repository root.
