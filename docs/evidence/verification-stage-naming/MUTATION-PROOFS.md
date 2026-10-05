# Verification stage naming — mutation proofs

**Artifact under test:** `scripts/verify.sh` at `ab659bf`
**Date:** 2026-08-31
**Decision context:** ED-0077 (captured exit status), ED-0087 R3 (a check that
cannot fail), ED-0041 family.

## The defect this fixes

`verify.sh` runs under `set -euo pipefail`. Before this change a failing stage
aborted the script with **no message of its own**. The output therefore ended
with the last stage that SUCCEEDED, and a failed run was indistinguishable
from a clean one to any reader who did not capture `$?`. On 2026-08-31 two
agent pairs reported "verify.sh exit 0" for branches that could not have
passed, both in good faith, both reading the tail.

The remedy is NOT to echo a stage name before running it. A pre-announcement
records that a stage STARTED; it never records that it PASSED, so the output
of a silent failure would look identical with names added. The name must be
printed on the FAILURE path, carrying the captured exit status.

## Mechanism

```sh
CURRENT_STAGE="startup"
stage() { CURRENT_STAGE="$1"; }
trap 'rc=$?; printf "\n=== VERIFICATION FAILED ===\nSTAGE: %s\nline:  %s\nexit:  %s\n" \
  "$CURRENT_STAGE" "$LINENO" "$rc" >&2; exit "$rc"' ERR
```

A success banner is printed at the end. Both banners matter: the ABSENCE of a
failure banner is weaker evidence than the PRESENCE of a success banner,
because absence is also what a killed process produces.

The ERR trap does not fire on the script's guarded expected-failure probes
(`cmd || rc=$?`), because `set -e` does not treat those as errors.

## Proofs

Each mutation was applied, the full script run, the exit status captured with
`$?` (never through a pipe -- ED-0077), and the mutation reverted.

| # | Mutation | Captured exit | Stage named |
|---|----------|---------------|-------------|
| 1 | appended `func   badlyFormatted( ) {  }` to `internal/document/projection.go` | 1 | `gofmt` |
| 2 | appended a reference to an undefined symbol to the same file | 1 | `go build` |
| 3 | added `internal/document/mutant_test.go` with `t.Fatal` | 1 | `go test` |
| 4 | set issue 0004's file-side status to `Done` while `issues/README.md` still said `In Progress` | 1 | `tracker consistency` |

Mutation 4's diagnostic, in full:

```
domowik-tracker-check: tracker inconsistencies:
status mismatch for 0004:
  file issues/0004-format-spec-entities-ops.md: "Done"
  index issues/README.md:43: "In Progress"
exit status 1

=== VERIFICATION FAILED ===
STAGE: tracker consistency
line:  29
exit:  1
```

Clean run at `ab659bf`: exit **0**, ending `=== VERIFICATION PASSED === all stages`.

## Honest limits of proof 4

Mutations 1-3 were run through `scripts/verify.sh` itself. **Mutation 4 was
not, and could not be.** The `sbom verify` stage refuses to run against a
dirty working tree, and committing the mutation changes `HEAD`, which changes
`SOURCE_DATE_EPOCH`, which makes the committed SPDX documents mismatch
regeneration. Either way the run terminates at `sbom verify` before reaching
the tracker stage. Both routes were attempted and both failed there; that is
itself recorded below.

Proof 4 therefore used a harness whose two parts were EXTRACTED
PROGRAMMATICALLY from `scripts/verify.sh` -- the preamble through the ERR
trap, and the `tracker consistency` block verbatim -- so the harness cannot
drift from the script. What this establishes: the trap names that stage, and
the tracker tool detects a status mismatch. What it does NOT establish: the
stage's ordering inside the full script. Ordering is established separately by
reading the script and by the clean run, in which the stage executes.

## Reachability note — the clean-tree assertion (P13)

`test -z "$(git status --porcelain)"` is **not** mutation-proven, for a
structural reason worth recording rather than working around.

Any working-tree dirt introduced BEFORE the run is caught by `sbom verify`,
which demands a clean checkout and runs much earlier. P13 is therefore
unreachable by pre-run mutation, and it is not redundant: it catches dirt
created BY the run itself, between `sbom verify` and the end -- stray build
output, a tool writing outside `build/ci`. That is precisely its stated
purpose. Proving it would require a mutation injected mid-run, which means
editing the script under test.

Recorded as a known gap rather than closed, per ED-0087 R3: a stage whose
proof is impractical is declared, not left silently unexamined.
