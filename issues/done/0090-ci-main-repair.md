# 0090: Repair hosted main CI preflight failures

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0061

## Scope

Repair failed run 37253702973 on committed main without publishing unrelated
local editor/docs/Linux-gate work. The owner explicitly authorized committing
and pushing the CI fixes on 2026-10-04. Preserve the dirty application worktree.

## Acceptance criteria

- [x] CI summary tests use a Linux/macOS-compatible temporary directory.
- [x] Inventory tests do not assume a clean runner has the pinned test-only
      dependency cached; failures remain diagnostic rather than suppressed.
- [x] Notice assertions isolate the fixture, and macOS artifact checks select
      their declared target on Linux.
- [x] Only scoped fixes/tracker records are committed and pushed.
- [x] Validate the committed snapshot and inspect the resulting hosted run;
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

The first repaired macOS run passed 225 widget tests, both CPU harnesses,
bundle probes, Bash 3.2 banner regression, and all packaging rejection tests,
then failed at P13 clean tree. Add file-status and lockfile diagnostics to that
gate without relaxing it; the existing assertion concealed the changed paths.

Diagnostic run 37258856090 identified exactly one changed file:
`flutter_app/macos/Podfile.lock`. CocoaPods 1.17.0 regenerated our local
`domowik_bridge_plugin` checksum as `e06a52bf53804cd29a39192a5ab98acd577afb39`,
replacing the stale `56b2190ab1cb1f16ae267954764bc5ffb1a671c5` recorded before
the current podspec. Record the runner-generated checksum; no dependencies,
versions, sources, or licence scope change. The clean-tree gate remains intact.

## Final validation and workflow correction

The owner required complete local Linux validation rather than hosted CI as a
debugging loop. The first push occurred before local canonical verification
finished; that was premature. Subsequent Linux-only verification also omitted
Flutter to mirror the existing hosted Linux job, which was insufficient.

Locally install the official Flutter 3.41.6 tag `db50e20168db8fee486b9abf32fc912de3bc5b6a`
in an isolated temporary SDK, leaving the installed 3.47.5 SDK untouched. The
isolated application checkout's tracked tree exactly matches pushed commit
`f966988bef4d77f240f62a99dfe27c8db34bba1a`:
`2ac31c27deebbf3dde427874b84f25b43d9cdf49`.

Local results:

- Canonical verification exited 0 with `=== VERIFICATION PASSED === all stages`,
  using pinned Go/Rust plus Flutter 3.41.6 and Dart 3.11.4 on PATH.
- Flutter analysis, all 225 widget tests, and both CPU harnesses passed.
- Linux release application built; Xvfb/software rendering passed the bundled
  core self-test (48 checks) and first-frame probe.
- CI preflight passed: 27 summary cases, seven shell checks, 27 inventory checks.
- The isolated checkout remained clean after all builds and tests. macOS
  application and Bash 3.2 cannot run on this Linux host; coverage is not claimed.

Logs: `/tmp/domowik-ci-full-linux-{verify,build,preflight,selftest,firstframe}.log`.
Existing hosted run 37259681164 subsequently completed with all four jobs green,
including macOS/Bash 3.2 and packaging rejection tests. No new hosted run was
triggered for this final local validation or tracker closure. Future changes
must complete applicable local checks before pushing; hosted CI confirms
platform coverage rather than replacing Linux testing.

Run: https://github.com/laszukdawid/domowik/actions/runs/37259681164
