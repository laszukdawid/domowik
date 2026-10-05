# CI macOS and GUI coverage — hosted run evidence

**Run:** [34672441495](https://github.com/laszukdawid/domowik/actions/runs/34672441495),
push of `5aab668` to `main`, 2026-09-12T04:13:33Z
**Read with:** `gh run view`, `gh api .../timing`, `gh api .../jobs`. Read-only;
nothing was dispatched or pushed to produce it.

## What the first hosted macOS run showed

The macOS job ran, and it failed before it checked any of the GUI coverage it
exists to provide. Its last output:

```
error: Rust target x86_64-apple-darwin is not installed (needed for arch x86_64).
       run: rustup target add x86_64-apple-darwin
** BUILD FAILED **
Build process failed
```

The plugin's podspec build script maps each Xcode `ARCHS` entry to a Rust
triple and refuses to continue on a triple that is not installed, because a
missing target would otherwise produce a quietly non-universal framework
(`spikes/0024-flutter-rust-bridge/flutter_plugin/macos/domowik_bridge_plugin.podspec:113-121`).
A hosted macOS runner ships only its native target. The maintainer's laptop has
both installed by hand — `rustup target list --installed` reports
`aarch64-apple-darwin` and `x86_64-apple-darwin` — which is why the stage has
always passed locally. That divergence between one laptop and everywhere else
is the thing this job exists to remove, so the job reproducing it on its first
run is the job working.

The fix is one line in the macOS job: `rustup target add aarch64-apple-darwin
x86_64-apple-darwin`.

## The second finding: the run reported nothing

Searching the job log for `VERIFICATION` returns three hits, all of them the
summary step echoing its own source. The run printed neither banner. Under
ED-0077 it was therefore not a reportable verification result at all, and the
job summary described a real stage failure as a run that *"did not complete ...
may have been interrupted, e.g. by a timeout"*.

The cause is `/bin/bash` 3.2 on the runner and a stage that runs inside
`( ... )`. Diagnosis, mutation proofs and the fix are in
[MUTATION-PROOFS.md](MUTATION-PROOFS.md).

## What the hosted run did establish

The Linux job passed in 12m10s and its skip announcements were exactly the
three the issue predicted:

```
Flutter editor checks: SKIPPED - flutter not on PATH
spike 0024 Dart FFI harness: SKIPPED - dart not on PATH
spike 0024 Flutter macOS application: SKIPPED - needs macOS and flutter on PATH
```

Both `cross-build` jobs passed and inspected their artifact, so the issue's
third acceptance criterion is settled by a hosted run rather than by argument:
`crates/domowik-cli` does declare a binary target, the jobs do build it, and
type, byte count and SHA-256 are recorded in the job summary without executing
it.

The macOS job also ran 191 Flutter tests and both measurement harnesses before
it failed, on `macos_arm64`, macOS 15.7.9 (24G830), 3 processors.

## Toolchain divergence, recorded rather than assumed

| | hosted `macos-15` | local |
|---|---|---|
| Xcode | 26.3 (17C529) | 26.6 (17F113) |
| macOS | 15.7.9 (24G830) | 26.6.2 (25G83) |
| processors | 3 | 16 |

Issue 0057 verified the build against Xcode 26.6, and issue 0056 records an
intermittent SDK module redefinition seen on 26.6. Neither has been exercised
on 26.3. The hosted log contains no occurrence of `SwiftConcurrencyShims`,
`SwiftOverlayShims` or `redefinition of module`, but the build never reached
the Swift compilation where issue 0056 appears, so that absence is not
evidence either way.

## Cost and wall clock

Measured from the run's own job timestamps:

| job | runner | elapsed | result |
|---|---|---|---|
| verify (rust core + go corpus generator) | `ubuntu-24.04` | 12m10s | success |
| cross-build x86_64-unknown-linux-gnu | `ubuntu-24.04` | 1m17s | success |
| cross-build aarch64-unknown-linux-gnu | `ubuntu-24.04` | 1m45s | success |
| verify (macos + flutter) | `macos-15` | 14m53s | failure |
| whole run | | 15m02s | |

The four jobs run in parallel, so the macOS job costs about **2m43s of extra
wall clock** on the critical path and about **15 minutes of macOS runner time**
per run.

No billed figure is available, and none is invented here.
`GET /repos/laszukdawid/domowik/actions/runs/34672441495/timing` reported
`billable.UBUNTU.total_ms` and `billable.MACOS.total_ms` as **0**, with
`run_duration_ms` 902000. What can be cited is GitHub's published list price
("About billing for GitHub Actions", docs.github.com, retrieved 2026-09-12):
$0.006 per minute for a Linux 2-core runner and $0.062 for a macOS 3-core or
4-core runner, a ratio of about 10.3 to 1. That page does not state how a
partial minute is rounded, so a minute count derived from elapsed time is an
upper bound, not a bill.

An earlier draft of this file and of the workflow's summary step said "10x" and
labelled a wall-clock figure "billed minutes". Both were claims this repository
had no evidence for: the ratio is 10.3 by the published rates, and nothing here
has ever read a billing figure. Every run now records its own elapsed time and
the published rate for its runner class, labelled as an estimate, so the choice
can be revisited against accumulated numbers rather than this single sample.

Against that cost: the macOS job is the only automation that runs 191 Flutter
tests, the FFI bundle probes and every packaging assertion. Issue 0057 is the
precedent: a deterministic clean-checkout build failure sat in `main` and CI
could not have caught it. Under ED-0099 macOS is a local-development target, so
what this job protects is the developer-facing half of the product, not a
shipped artifact.

## What still needs hosted execution

Everything in this change is verified locally, on macOS 26.6 with both darwin
targets installed and `/bin/bash` 3.2 exercised by the banner regression. The
following can only be closed by a hosted run, which needs authority to push:

- the macOS job passing end to end with both targets installed;
- the packaging negative proof running on a runner rather than a laptop;
- the job summary rendering against a real `$GITHUB_STEP_SUMMARY`;
- the cost recorder's numbers for a passing macOS job, which will be longer
  than the 14m53s of a run that failed partway.
