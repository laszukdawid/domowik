# Tracker consistency checker — failure proofs

Evidence that `cmd/domowik-tracker-check` detects all four claimed defect
classes, measured by constructing each drift and observing exit status and
diagnostic output.

## Environment

Branch: `chore/tracker-consistency-check`  
Commit: f545679 (checker implementation), 989ec17 (SBOM refresh)  
Executor: wtrk  
Date: 2026-08-30

## Method

For each defect class:
1. Start from a clean tracker (checker exits 0)
2. Construct the specific drift
3. Run `go run ./cmd/domowik-tracker-check`
4. Capture exit status and stderr
5. Restore the tracker

## Proof 1: Status mismatch (TODAY'S FIRST REAL DEFECT)

Issue 0007 was listed as "In Progress" in README.md but had "In Progress
(partial scope)" in the file.

```
$ # Baseline: clean tracker
$ go run ./cmd/domowik-tracker-check
$ echo $?
0

$ # Construct the drift: modify README.md to mismatch 0007 status
$ cp issues/README.md issues/README.md.bak
$ sed -i.tmp 's/\(| \[0007\].*\) In Progress (partial scope) |/\1 In Progress |/' issues/README.md

$ # Run checker
$ go run ./cmd/domowik-tracker-check
domowik-tracker-check: tracker inconsistencies:
status mismatch for 0007:
  file issues/0007-geometry-spike-booleans.md: "In Progress (partial scope)"
  index issues/README.md:46: "In Progress"

$ echo $?
1

$ # Restore
$ mv issues/README.md.bak issues/README.md
$ rm -f issues/README.md.tmp
```

**Result:** Exit 1, diagnostic names the issue ID, file path, index line, and
both status values.

## Proof 2: Missing Status line (TODAY'S SECOND DEFECT, part 1)

The new `0010-lwwhorizon-fixation-analysis.md` file had no Status line.

```
$ # Construct the drift: create issue file without Status line
$ cat > issues/0010-lwwhorizon-fixation-analysis.md << 'EOF'
# 0010: LWW-Horizon Fixation Analysis

- **Phase:** 1
- **Priority:** High
EOF

$ # Run checker
$ go run ./cmd/domowik-tracker-check
domowik-tracker-check: issues/0010-lwwhorizon-fixation-analysis.md: missing Status line

$ echo $?
1

$ # Restore
$ rm issues/0010-lwwhorizon-fixation-analysis.md
```

**Result:** Exit 1, diagnostic names the specific file lacking a Status line.

## Proof 3: Duplicate prefix (TODAY'S SECOND DEFECT, part 2)

Two files with `0010-` prefix existed.

```
$ # Construct the drift: create duplicate 0010- file WITH Status line
$ cat > issues/0010-lwwhorizon-fixation-analysis.md << 'EOF'
# 0010: LWW-Horizon Fixation Analysis

- **Phase:** 1
- **Priority:** High
- **Status:** Open
EOF

$ # Run checker
$ go run ./cmd/domowik-tracker-check
domowik-tracker-check: duplicate issue IDs:
  0010: issues/0010-core-document-model.md, issues/0010-lwwhorizon-fixation-analysis.md

$ echo $?
1

$ # Restore
$ rm issues/0010-lwwhorizon-fixation-analysis.md
```

**Result:** Exit 1, diagnostic lists the duplicate ID and both file paths.

## Proof 4: Orphan index row

README.md references a file that does not exist.

```
$ # Construct the drift: add phantom issue to README.md
$ cp issues/README.md issues/README.md.bak
$ sed -i.tmp '46a\
| [9999](9999-phantom.md) | Phantom issue | Open |' issues/README.md

$ # Run checker
$ go run ./cmd/domowik-tracker-check
domowik-tracker-check: tracker inconsistencies:
issues/README.md:47: orphan index row for 9999 (9999-phantom.md) — no corresponding file

$ echo $?
1

$ # Restore
$ mv issues/README.md.bak issues/README.md
$ rm -f issues/README.md.tmp
```

**Result:** Exit 1, diagnostic names the index file, line number, issue ID, and
referenced path.

## Proof 5: Clean tracker passes

```
$ go run ./cmd/domowik-tracker-check
$ echo $?
0
```

**Result:** Exit 0, no output.

## Summary

All four defect classes produce exit status 1 with actionable diagnostics
naming specific files, line numbers, and mismatched values. The clean tracker
exits 0 with no output.
