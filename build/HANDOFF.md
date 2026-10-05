# Tracker consistency checker — handoff

**Branch:** `chore/tracker-consistency-check`  
**Commits:** f545679, 989ec17  
**Executor:** wtrk (via opencode)

## Deliverable

`cmd/domowik-tracker-check` — a Go command that verifies tracker consistency
and fails loudly on:

1. **Status disagreement** between an issue file's `- **Status:**` line and its
   row in `issues/README.md`
2. **Duplicate NNNN prefixes** across `issues/`, `issues/done/`, and
   `issues/dropped/`
3. **Missing Status lines** in issue files
4. **Orphan index rows** in README.md with no corresponding file

Exit codes:
- 0: tracker is consistent
- 1: inconsistency detected (diagnostic written to stderr)
- 2: usage or IO error

## Proof of failure detection

All four defect types were constructed, run through the checker, and shown to
fail. Complete evidence with command transcripts is in
`docs/evidence/tracker-consistency/FAILURE-PROOFS.md`. Summary:

1. **Status mismatch** (TODAY'S FIRST REAL DEFECT: issue 0007) — Exit 1,
   diagnostic names issue ID, file path, index line, and both status values.
2. **Missing Status line** (TODAY'S SECOND DEFECT, part 1) — Exit 1, diagnostic
   names the specific file.
3. **Duplicate prefix** (TODAY'S SECOND DEFECT, part 2: two 0010- files) —
   Exit 1, diagnostic lists duplicate ID and both file paths.
4. **Orphan index row** — Exit 1, diagnostic names index file, line number,
   issue ID, and referenced path.
5. **Clean tracker** — Exit 0, no output.

## Tests

`cmd/domowik-tracker-check/main_test.go` — 7 tests covering:
- Each of the four defect types
- Consistent tracker (pass case)
- Usage error handling
- Fail-fast behavior when multiple errors are present

All tests pass:

```
$ go test ./cmd/domowik-tracker-check -v
=== RUN   TestCheckerCatchesStatusMismatch
--- PASS: TestCheckerCatchesStatusMismatch (0.00s)
=== RUN   TestCheckerCatchesDuplicatePrefix
--- PASS: TestCheckerCatchesDuplicatePrefix (0.00s)
=== RUN   TestCheckerCatchesMissingStatusLine
--- PASS: TestCheckerCatchesMissingStatusLine (0.00s)
=== RUN   TestCheckerCatchesOrphanIndexRow
--- PASS: TestCheckerCatchesOrphanIndexRow (0.00s)
=== RUN   TestCheckerPassesOnConsistentTracker
--- PASS: TestCheckerPassesOnConsistentTracker (0.00s)
=== RUN   TestCheckerRejectsArguments
--- PASS: TestCheckerRejectsArguments (0.00s)
=== RUN   TestCheckerDetectsMultipleErrors
--- PASS: TestCheckerDetectsMultipleErrors (0.00s)
PASS
ok  	domowik.dev/domowik/cmd/domowik-tracker-check	0.244s
```

## Verification

```
$ ./scripts/verify.sh > build/verify.log 2>&1; echo "EXIT $?"
EXIT 0
```

Clean tree, SBOM refreshed.

## Build artifact handling

The tool is tested via `go test ./cmd/domowik-tracker-check` and never built to
the repository root. The root `.gitignore` already ignores `/domowik` (the main
binary) but does not have a pattern for additional command binaries. Rather
than edit `.gitignore` for a build-artifact reason, I avoided creating the
artifact in the first place. All proofs use `go run ./cmd/domowik-tracker-check`
or test execution.

## Proposed verify.sh integration

**Do NOT apply yet** — two pairs are live and a new failing gate would block
them. Add this line to `scripts/verify.sh` after line 17 (after
`go run ./cmd/domowik-jsoncheck docs`), once live pairs land:

```bash
go run ./cmd/domowik-tracker-check
```

This places the tracker check early in the verification pipeline, immediately
after JSON syntax validation and before the conformance corpus checks. The
tracker is read-only infrastructure; if it's broken, later checks that depend
on issue state should not run.

## Notes

- The tool runs from the repository root and expects `issues/` to exist.
- No arguments are accepted; usage is simply `domowik-tracker-check`.
- The regex `^\|\s+\[(\d{4})\]\(([^)]+)\)\s+\|.*\|\s+([^|]+?)\s+\|$` matches
  README.md table rows with 3 columns: Issue, Title, Status.
- Follows the existing command pattern (`domowik-sbom`, `domowik-jsoncheck`).
- Language choice: Go, for consistency with the existing codebase and tooling.
