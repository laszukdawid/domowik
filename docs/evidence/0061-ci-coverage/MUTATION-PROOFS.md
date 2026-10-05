# CI macOS and GUI coverage — mutation proofs

**Artifacts under test:** `scripts/verify.sh`, `scripts/check-macos-bundle.sh`,
`scripts/ci/verify-job-summary.sh`, `scripts/ci/macos-bundle-negative-proof.sh`
**Date:** 2026-09-12
**Host:** macOS 26.6, arm64, Xcode 26.6 (17F113), Flutter 3.41.6,
rustc 1.98.0 (88d9e12ae), `/bin/bash` 3.2.57, `/opt/homebrew/bin/bash` 5.3.15
**Decision context:** ED-0077 (a verification claim carries the run's output),
ED-0087 R3 (an announced skip, never a silent one), ED-0099 (macOS is a
local-development target). Extends
[verification-stage-naming/MUTATION-PROOFS.md](../verification-stage-naming/MUTATION-PROOFS.md).

## The defect the hosted run exposed

`scripts/verify.sh` gained a failure banner on 2026-08-31 so that no stage
could fail silently. On macOS it still could.

GitHub Actions run
[34672441495](https://github.com/laszukdawid/domowik/actions/runs/34672441495),
job 103496232637, failed inside `spike 0024 Flutter macOS application` and
printed **neither** `=== VERIFICATION FAILED ===` nor
`=== VERIFICATION PASSED ===`. Grepping the downloaded job log for
`VERIFICATION` returns three hits, all of them the summary step echoing its own
source. Under ED-0077 that run was not a reportable verification result.

The cause is not in the stage. The macOS stage runs inside `( ... )`, and
bash 3.2 does not run a top-level `ERR` trap when a subshell fails. bash 3.2 is
`/bin/bash` on every macOS host and on GitHub's `macos-15` runners, so every
macOS verification failure was unreportable, on the one platform where the
macOS stages run at all.

Measured across both shells present, with a subshell that fails and a trap that
prints one banner:

| shell | without `set -E` | with `set -E` |
|---|---|---|
| bash 3.2.57 | no banner | one banner (from the subshell) |
| bash 5.3.15 | one banner (from the parent) | two banners |

Neither column is correct everywhere, which is why `verify.sh` enables errtrace
on bash 3.x only. Enabling it unconditionally trades a missing banner for a
doubled one.

## Proof 1 — the failure banner, under every bash

`scripts/ci/verify-banner-regression.sh` extracts the preamble from
`scripts/verify.sh` between its `BEGIN`/`END FAILURE-REPORTING PREAMBLE`
markers, so the harness cannot drift from the script, and appends a synthetic
stage. The same extraction technique as proof 4 of the 2026-08-31 record.

Three fixtures per shell: a failure inside a subshell, a failure at the top
level, and a clean run. Each failing fixture uses `sh -c 'exit 7'` rather than a
bare `exit 7`, because `exit` is not an error and never triggers an `ERR` trap.
A fixture built on it reports "no banner" on a shell whose trap works. Status 7
rather than 1 so that a banner reporting the status correctly can be told apart
from one printing a plausible constant.

Clean run, at `18ef975`:

```
--- /bin/bash (bash 3.2) ---
PASS  bash 3.2 nested (subshell) failure: one banner, stage 'nested stage', exit 7
PASS  bash 3.2 top-level failure: one banner, stage 'top-level stage', exit 7
PASS  bash 3.2 clean run: pass banner only, exit 0
--- /opt/homebrew/bin/bash (bash 5.3) ---
PASS  bash 5.3 nested (subshell) failure: one banner, stage 'nested stage', exit 7
PASS  bash 5.3 top-level failure: one banner, stage 'top-level stage', exit 7
PASS  bash 5.3 clean run: pass banner only, exit 0

banner regression passed on every bash present
```

| # | Mutation | Result |
|---|----------|--------|
| 1 | removed the `BASH_VERSINFO` guard and its `set -E` (the state the hosted run failed in) | `FAIL bash 3.2 nested (subshell) failure: expected exactly 1 failure banner, saw 0`, exit 1 |
| 2 | made `set -E` unconditional | `FAIL bash 5.3 nested (subshell) failure: expected exactly 1 failure banner, saw 2`, exit 1 |

Mutation 2's captured output, in full:

```
        === VERIFICATION FAILED ===
        STAGE: nested stage
        line:  32
        exit:  7

        === VERIFICATION FAILED ===
        STAGE: nested stage
        line:  33
        exit:  7
```

Both mutations were reverted and the suite returned to green.

The test names its own gap. A host with no bash 3.x prints:

```
NOTE: no bash 3.x on this host, so the macOS /bin/bash case this
      regression exists for was NOT exercised here. It is covered on
      the macOS CI runner, where /bin/bash is 3.2.
```

That is the Linux CI job's situation, and ED-0087 R3 requires it to be said
rather than implied by a green tick.

## Proof 2 — the job summary cannot call a failure an interruption

`scripts/ci/verify-job-summary.sh` renders the GitHub job summary. Before this
change the same logic was inlined twice, once per job, and the branch that
mattered was missing from both: a `verify.sh` that exits non-zero without a
banner was reported as *"did not complete ... the run may have been
interrupted, e.g. by a timeout"*. That is what a reader of run 34672441495
would have been told about a genuine stage failure.

`scripts/ci/verify-job-summary-test.sh` drives the renderer over ten synthetic
logs, including a log shaped exactly like the hosted failure. All ten pass at
`18ef975`.

| # | Mutation | Result |
|---|----------|--------|
| 3 | replaced the non-zero-exit branch condition with `false`, restoring the old behaviour | `FAIL a silent non-zero exit is a failure, not an interruption`, exit 1 |

The mutant's rendered summary, which is the text the old workflow would have
produced:

```
### Verification summary
**Verification did not complete** (no pass or fail banner in the log — the run may have been interrupted, e.g. by a timeout).

No skip markers recorded — unreached stages remain unverified.
```

Reverted; the suite returned to green.

## Proof 3 — the packaging gate rejects

`scripts/verify.sh` has asserted four things about the built macOS bundle since
2026-09-04: the core is inside the bundle, its framework id is exactly
`@rpath/domowik_bridge_plugin.framework/Versions/A/domowik_bridge_plugin`, both
architecture slices are present, and the bundle passes
`codesign --verify --deep --strict`. None had been seen to fail. An assertion
nobody has watched reject is equally consistent with correct packaging and with
a check that cannot fire: a path that moved, an `otool -D` output format that
changed, a typo in a `lipo` flag.

The four assertions moved verbatim into `scripts/check-macos-bundle.sh`, which
prints a stable `BUNDLE CHECK FAILED: <tag>` line before its human-readable
reason. `scripts/ci/macos-bundle-negative-proof.sh` copies the bundle
`verify.sh` just built, breaks one property per copy, and requires the checker
to reject **for that property's tag**. A negative test that accepted any
non-zero exit would pass against a checker that always crashed.

Copies are made with `ditto`, not `cp -R`, because a code signature depends on
extended attributes that `cp -R` drops; a control case failing for that reason
would make every mutation unattributable. Nothing mutated is executed, and the
copies live outside the checkout, so `verify.sh`'s clean-tree assertion is
unaffected.

Executed 2026-09-12 against the 47 MB bundle produced by that day's local
`verify.sh` run:

```
PASS  control: an unmutated copy still passes the checker
PASS  missing-core: rejected as 'core-not-in-bundle' (the core removed from the bundle)
PASS  absolute-id: rejected as 'framework-id-not-rpath' (a framework id that is not @rpath-relative)
PASS  thin-arch: rejected as 'missing-arch-slice' (the x86_64 slice removed)
PASS  stale-archive: rejected as 'signature-invalid' (a stale nested archive)

negative proof passed: 1 control + 4 rejections, each for its own reason
```

The real bundle still passed `scripts/check-macos-bundle.sh` afterwards, and
the temporary directory was removed.

## Proof 4 — two defects the proof script only revealed by being run

Writing the negative proof was not enough; the first execution against a real
bundle found two faults in the proof itself.

`fresh_copy` declared `local name="$1" dest="$WORK/$name/..."` in one
statement. bash expands every right-hand side in the ENCLOSING scope before the
locals exist, so `$name` there is not the value just assigned:

```
$ /bin/bash -c 'set -u; f() { local name="$1" dest="/w/$name/x"; }; f control'
localprobe.sh: line 3: name: unbound variable
```

and with an outer `name` in scope it silently used that instead:

```
name=STALE
f() { local name="$1" dest="$WORK/$name/x"; echo "name=$name dest=$dest"; }
f control        # -> name=control dest=/tmp/w/STALE/x
```

Both bash 3.2.57 and bash 5.3.15 behave this way. `expect_rejection` has its
own local called `name`, so the four mutation cases resolved to the caller's
value and were correct by accident, while the control case, called from the
top level where no `name` exists, aborted the script. A helper that works
everywhere it currently happens to be called from is the harder version of this
bug, not the easier one.

`scripts/ci/shell-local-decl-check.py` now rejects the construct. It fires on
the exact pre-fix line and passes `local sh="$1" version="$2" out rc`, so it
discriminates rather than banning multi-name declarations.

The stale-archive mutation searched `App.framework` with `find -type f -name
App`. That path is a symlink into `Versions/A`, so `-type f` never matched it
and the case reported `could not apply the mutation` rather than proving
anything. A negative test failing open. It targets the versioned path now.

Credit where due: the first defect was found by independent review before the
script had ever been executed, and the second by executing it.

## What none of this proves

The banner regression establishes that `verify.sh` names a failing stage on the
bash versions present on the machine running it. It does not establish that the
macOS CI job passes: that requires a hosted run, and the evidence for it is
recorded separately.

The packaging proof establishes that four assertions reject four specific
defects. It does not establish that the set of assertions is complete. Bundle
properties nobody has written an assertion for remain unchecked, ad-hoc signing
is all ED-0099 asks for, and the x86_64 slice is built but still never executed.

## Advisory review

Two rounds were run against Codex on `gpt-5.6-sol` at high reasoning effort,
per INSTRUCTIONS.md, each with a prompt scoped to three or four named files,
findings demanded first, stdin from `/dev/null`, in the background.

**Round 1 produced no adversary.** Codex started correctly, echoing
`model: gpt-5.6-sol` and `reasoning effort: high`, then returned
`ERROR: Your workspace is out of credits` and exited 1. No findings, and none
are written here on its behalf. INSTRUCTIONS.md requires the scrutiny to happen
anyway, by writing the tests the advisor would have demanded, so the five
questions put to it were answered against the code instead.

**Round 2 returned nine findings.** It also spent part of its budget trying to
delegate to OpenCode, which failed against the read-only sandbox; its own note
records that the findings are from direct analysis. Three of them —
`unknown` accepted as a pass, `tr -cd` fabricating a status, and an unreadable
status file aborting the script — independently reproduce what repository
review had found the same day, which is the most useful thing an advisory round
can do. Disposition:

| # | Finding | Disposition |
|---|---------|-------------|
| 1 | the expected tag is matched anywhere in combined output, so a second tag could satisfy the wrong case | fixed: exactly one `BUNDLE CHECK FAILED:` line is now required |
| 2 | `fresh_copy` runs in `$( )`, where errexit does not abort, so a failing `ditto` is masked by the trailing `printf` | fixed: every command propagates explicitly |
| 3 | mutation functions run as an `if !` condition, where only the last command's status survives | fixed: every command propagates explicitly |
| 4 | a pass reported with no exit status at all | fixed (also found by review) |
| 5 | `tr -cd` concatenates digits; `00` compared as a string reports a contradiction | fixed: single-line digits only, compared numerically |
| 6 | a stale failure banner earlier in an append-mode log outranks a later pass | fixed: the LAST banner is the result |
| 7 | an unreadable status file, or a log removed mid-render, aborts the script inside an `if: always()` step | fixed: guarded read and guarded pipelines |
| 8 | `otool -D` prints one id per architecture and the check matches any of them, so a bad x86_64 slice passes | recorded, not fixed: see "Known limits" below |
| 9 | a bundle path beginning with `-` is read as an option and the bundle is falsely rejected | fixed: relative paths are prefixed with `./` |

Finding 9, demonstrated:

```
$ otool -D "-V/Contents/Frameworks/.../domowik_bridge_plugin"
error: otool: unknown char `/' in flag -V/Contents/Frameworks/...

$ otool -D "./-V/Contents/Frameworks/.../domowik_bridge_plugin"
./-V/Contents/Frameworks/.../domowik_bridge_plugin (architecture x86_64):
@rpath/domowik_bridge_plugin.framework/Versions/A/domowik_bridge_plugin
```

That same output is the evidence for finding 8: `otool -D` really does print a
section per architecture, so matching the wanted id on any line cannot speak
for every slice.

After the fixes, the negative proof was re-run against the real bundle and all
five cases still pass, and a bundle placed at the path `-V` is now accepted
rather than falsely rejected.

One of the round-1 questions, answered by hand, found something. `scripts/ci/verify-banner-regression.sh` runs
as a `verify.sh` stage, and its fixtures print genuine
`=== VERIFICATION PASSED === all stages` and `=== VERIFICATION FAILED ===`
lines. Everything it prints lands in the same log the job summary reads. A
fixture banner reaching that log at column 0 would make a FAILING run carry a
passing banner, and the summary would report the fixture instead of the run.

It does not today, and the real logs confirm it: the passing run's log contains
exactly one pass banner and no fail banner, the failing run's exactly one fail
banner and no pass banner. The reason is that the regression captures fixture
output and re-emits it indented by one `sed 's/^/        /'`. That is too quiet
a thing to be load-bearing, so `scripts/ci/verify-job-summary-test.sh` now
asserts it. Making the regression echo fixture output at column 0 produces
`FAIL the banner regression leaked 2 pass and 0 fail banner line(s)`, and the
assertion was reverted to green.

The other four answers, each checked rather than reasoned about:

- A mutation function that does not exist is reported as `could not apply the
  mutation` and counted as a failure, not skipped. A mutation that leaves the
  bundle intact is reported as `the checker ACCEPTED a bundle with ...`.
  `ditto` and `mktemp` failures abort under `set -e` rather than passing.
- `1 control + 4 rejections` matches the call sites: one control block and four
  `expect_rejection` invocations.
- The `$rc` file is filtered with `tr -cd '0-9'`, so unexpected content becomes
  `unknown` and is reported as such rather than as a pass. Both banner greps are
  anchored full-line with `-qxF`, and the failure banner is tested before the
  pass banner.
- The architecture matcher is exact, as its comment claims. `arm64e` does not
  satisfy `arm64`, `x86_64h` does not satisfy `x86_64`, a single-architecture
  binary fails the check for the missing slice, and irregular spacing in
  `lipo -archs` output is handled by word splitting.

## Proof 5 — an exit status is a number or it is nothing

Independent review found two ways the job summary could claim a verified pass
it had no right to claim. Both were in the handling of the captured exit
status, and both were real.

The status file was read with `tr -cd '0-9'`, which deletes every non-digit
rather than rejecting the input. The text `failed0` therefore became the number
`0`. A status file that says the run failed was read as a run that succeeded.

Separately, `unknown` was accepted alongside `0` as grounds for reporting
**PASSED**, which contradicts this script's own documented contract that a pass
means "the pass banner and exit 0 agree". A missing or unreadable status file
produced a confident pass on the strength of the banner alone.

A pass now requires the banner and a validated exit status of `0`. The status
is accepted only if the file holds exactly one line of digits, so empty,
multiline, and digit-containing text are all `unknown`, and `unknown` under a
pass banner reports a new status, `unconfirmed`, which carries no
skip-freedom assurance. The read is also guarded against an unreadable file:
this script runs in an `if: always()` step, where a crash would replace the
result it exists to report with nothing at all.

Six cases were added: text containing a zero, an absent status, a multiline
status, an empty file, an unconfirmed pass not claiming skip-freedom, and a
genuine non-zero status under a pass banner.

| # | Mutation | Result |
|---|----------|--------|
| 4 | restored `tr -cd '0-9'` and re-accepted `unknown` as a pass | 5 of 17 cases fail, exit 1 |

Reverted; the suite returned to green at 17 cases.

## Known limits of the packaging assertions

Recorded rather than fixed, because these are properties of the assertions as
they already stood and changing them is not in this issue's scope.

`otool -D` on a universal binary prints one install name per architecture, and
the check matches the wanted id on any line. A framework whose arm64 slice
carries the correct `@rpath` id while its x86_64 slice carries an absolute
build path would pass. `codesign --verify --deep --strict` is likewise not run
with `--all-architectures`. Both would need a per-slice check to close.
