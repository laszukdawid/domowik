# 0090: Repair hosted main CI preflight failures

- **Phase:** 2
- **Priority:** High
- **Status:** In Progress
- **Depends on:** 0061

## Scope

Repair failed run 37253702973 on committed main without publishing unrelated
local editor/docs/Linux-gate work. The owner explicitly authorized committing
and pushing the CI fixes on 2026-10-04. Preserve the dirty application worktree.

## Acceptance criteria

- [ ] CI summary tests use a Linux/macOS-compatible temporary directory.
- [ ] Inventory tests do not assume a clean runner has the pinned test-only
      dependency cached; failures remain diagnostic rather than suppressed.
- [ ] Notice assertions isolate the fixture, and macOS artifact checks select
      their declared target on Linux.
- [ ] Only scoped fixes/tracker records are committed and pushed.
- [ ] Validate the committed snapshot and inspect the resulting hosted run;
      record actual results and any remaining platform gates.

## Provenance and diagnosis

Original owner request and GitHub job logs, read-only. Linux job 111586310369
failed with `mktemp: too few X's`; macOS job 111586310317 exited 101 in the
offline spike-fixture lockfile step. Both skipped canonical verification.
Cross-build jobs passed. Fixes were already tested locally under issue 0087,
but deliberately excluded from the history-only push. Existing test-only
ryu 1.0.23 fixture remains unbuilt/undistributed and is now exactly pinned.

Run: https://github.com/laszukdawid/domowik/actions/runs/37253702973

Follow-up run 37257759499 passed both cross-build jobs and macOS preflight.
Linux preflight exposed automatic Git maintenance racing with deletion of
scratch fixtures (`.git: Directory not empty`); disable maintenance and auto-GC
in those disposable repositories only. Canonical Linux snapshot verification
reached `failure banner regression`, exposing the same non-portable `mktemp`
usage in that harness; use an explicit Linux/macOS-compatible template there.

The isolated follow-up snapshot passed all 27 inventory checks and canonical
verification (`=== VERIFICATION PASSED === all stages`) on Linux. Dart,
Flutter, macOS application, and Bash 3.2 coverage were explicitly skipped on
that host; hosted macOS verification remains required.
