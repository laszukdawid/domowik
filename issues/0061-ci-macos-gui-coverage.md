# 0061: Verify hosted macOS packaging and GUI CI coverage

- **Phase:** 2
- **Priority:** Medium
- **Status:** In Progress (partial scope)
- **Depends on:** 0024, 0057

## Current disposition — 2026-10-08

Keep partial for observed hosted acceptance, not adding CI jobs again.

`.github/workflows/ci.yml` already requires Linux editor checks and includes
macOS, summaries, cross-artifact inspection and packaging-negative proofs.
Reconcile the remaining unchecked boxes with a successful real hosted run and
its summary/mutation evidence when available; no workflow dispatch or push is
needed for this cleanup. Old 'GUI executes nowhere' and pending Go-removal
claims are superseded. The checked-in HOSTED-RUN.md records a failed first
macOS build and successful cross-build inspection, not complete macOS
acceptance. Keep that evidence dated; this audit does not refresh remote runs.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

`.github/workflows/ci.yml` runs `verify` on `ubuntu-24.04` only. Several
`scripts/verify.sh` stages test `[ "$(uname -s)" = "Darwin" ]` or the presence
of `flutter` and announce a skip otherwise:

- `Flutter editor analysis`
- `Flutter editor widget tests`
- `Flutter viewport measurement harness`
- `Flutter level-render measurement harness`
- `spike 0024 Flutter macOS application`, with the bundled-core, universal
  architecture, framework id, signature and both bundle probes

So the entire GUI half of verification — 152 Flutter tests, the FFI bundle
probes, and every packaging assertion — executes nowhere but a maintainer's
laptop. Issue 0057 is the demonstration: a deterministic clean-checkout build
failure sat in `main` and CI could not have caught it.

The skips are announced rather than silent, which is ED-0087 R3 working as
intended. The gap is that nothing automated ever takes the other branch.

A second, smaller inaccuracy lives in the same file. The `cross-build` job
comment states that "domowik-cli is a library scaffold with no binary target,
so these jobs prove the WORKSPACE cross-compiles, not that a shippable binary
does", and that artifact inspection "is NOT reproduced here because there is
no artifact to inspect yet". `crates/domowik-cli/Cargo.toml` has declared
`[[bin]] name = "domowik"` since the CLI port; the binary exists and the jobs
build it. The stated reason for omitting artifact inspection is now false even
though the omission itself is real.

## Scope

Belongs here: running the macOS and Flutter stages under automation; deciding
and recording what happens when a hosted macOS runner is unavailable or too
slow, so a skip is a recorded decision rather than an accident; correcting the
`cross-build` comment and deciding whether artifact inspection returns.

Explicitly excluded: Linux and Android bridge validation (issue 0024); the
measured interaction budgets (issue 0033); the release and SBOM process
(issue 0036); the build defect itself (issue 0057), which must be fixed first
or CI will simply reproduce it.

## Acceptance criteria

- [ ] The Flutter analysis, widget-test and macOS application stages run in
      CI on a macOS runner, including the bundle probes and the signature
      assertion, or a recorded decision states why not and what replaces them.
- [ ] A stage that is skipped in CI is visible as a skip in the job summary,
      not merely in log text, so an all-green run cannot mean "the GUI was
      never checked".
- [x] The `cross-build` job comment matches the workspace: the CLI binary
      target exists. Artifact inspection is either restored or its absence is
      justified by a reason that is currently true.
- [ ] A deliberately broken bundle — a stale archive, a missing architecture
      slice, or a framework id that is not `@rpath`-relative — fails the new
      CI job. A packaging gate that has never been observed failing is not
      evidence.
- [x] Runner cost and wall-clock impact are recorded so the choice can be
      revisited.

## Provenance

Original inspection of `.github/workflows/ci.yml`, `scripts/verify.sh` and
`crates/domowik-cli/Cargo.toml` during a repository review on 2026-09-06. No
third-party source or configuration has been consulted or adapted.

## Progress: 2026-09-07

The local workflow adds a `macos-15` job running the canonical `verify.sh`,
with Flutter 3.41.6 and a 45-minute timeout. Both verification jobs write
their logs outside the checkout and summarize pass, failure, missing or
incomplete logs separately from skip markers. The cross-build job inspects
the CLI artifact's type, byte count and SHA-256 without executing it.

The Flutter setup action is development-only, MIT licensed, pinned to
`subosito/flutter-action@1a449444c387b1966244ae4d4f8c696479add0b2` (v2.23.0).
Root independently checked the upstream tag and license. Caches remain off
because the action's optional cache steps reference a floating tag.

Local YAML parsing and synthetic summary checks are available, but no hosted
workflow has run for this change. The selected hosted Xcode lineup differs
from the locally verified 26.6 toolchain. Actual runner cost, elapsed time,
bundle failure injection and successful hosted packaging remain unverified.
Existing push and pull-request triggers are unchanged; no workflow was
dispatched or pushed during this work.

## Progress: 2026-09-12

The 2026-09-07 note above is stale in one respect that changed everything:
a hosted run exists. Pushing `5aab668` to `main` triggered
[34672441495](https://github.com/laszukdawid/domowik/actions/runs/34672441495),
and it found two defects that local work could not.

The macOS job failed in 14m53s at the bundle build:
`error: Rust target x86_64-apple-darwin is not installed (needed for arch
x86_64)`. The plugin's podspec maps each Xcode `ARCHS` entry to a Rust triple
and refuses to continue on a missing one; a hosted runner ships only its native
target, while the maintainer's laptop has both installed by hand. The macOS job
now installs both.

The same run printed NEITHER verification banner. `/bin/bash` on macOS is 3.2,
which does not run a top-level `ERR` trap when a subshell stage fails, so every
macOS failure was unreportable under ED-0077 and the job summary called a real
stage failure an interrupted run. `verify.sh` enables errtrace on bash 3.x only
(unconditionally would double the banner on bash 5), and
`scripts/ci/verify-banner-regression.sh` holds it there, checking banner count,
stage name and exit status on every bash present. Both mutants fail it.

The four bundle assertions moved verbatim into
`scripts/check-macos-bundle.sh`, and
`scripts/ci/macos-bundle-negative-proof.sh` breaks copies of the built bundle
to prove they reject: one unmutated control passes, and a removed core, an
absolute framework id, a stripped x86_64 slice and a stale nested archive are
each rejected for their own reason. Executed against a real 47 MB bundle.

Cost, measured: `ubuntu-24.04` verify 12m10s, cross-build 1m17s and 1m45s,
`macos-15` 14m53s, whole run 15m02s. The macOS job adds about 2m43s of critical
path and about 15 minutes of macOS runner time. The timing API reported zero
billable milliseconds for this run, so no billed figure is claimed. GitHub's
published list prices (retrieved 2026-09-12) are $0.006 per Linux 2-core minute
and $0.062 per macOS 3/4-core minute, a ratio of about 10.3 to 1; the rounding
of partial minutes is not documented, so every run records its own elapsed time
and the published rate for its runner class, labelled as an estimate.

Canonical verification at `2fa9333` and again at `20904f0`, macOS 26.6
arm64, clean tree both times:

    ./scripts/verify.sh > verify-final.log 2>&1; echo "EXIT $?"
    EXIT 0
    === VERIFICATION PASSED === all stages

Zero `SKIPPED` markers in that run, so every Darwin and Flutter stage actually
executed: 191 Flutter widget tests, both measurement harnesses, the macOS
release build, the extracted bundle checker against the real bundle, both
bundle probes, and the banner regression on bash 3.2 and bash 5.3.

Evidence: [HOSTED-RUN.md](../docs/evidence/0061-ci-coverage/HOSTED-RUN.md),
[MUTATION-PROOFS.md](../docs/evidence/0061-ci-coverage/MUTATION-PROOFS.md).

**What is not closed, and why.** Three criteria are implemented and proven
locally but not yet observed on a runner: the macOS job passing end to end, the
negative packaging proof running in CI, and the new job-summary renderer
writing to a real `$GITHUB_STEP_SUMMARY`. Closing them needs one hosted run,
which needs authority to push. No workflow was dispatched or pushed during this
work; the hosted run above was read with `gh`, not caused by it.

**Toolchain gap, unchanged and still real.** The hosted image selected Xcode
26.3 (17C529); issues 0056 and 0057 were both observed on 26.6. The hosted log
contains no issue-0056 module-redefinition signature, but the build never
reached Swift compilation, so that is not evidence either way.

## Closure

- **Implementation/spec commits:** `2750b9b` (bash 3.2 failure banner and the
  extracted bundle checker), `18ef975` (macOS darwin targets, negative
  packaging proof, shared job summary, cost recorder), `ec328a1` (negative
  proof defects found by review and by execution), `20904f0` (the banner
  regression cannot poison the job summary).
- **Follow-up issues:** none opened. Issue 0056's three-consecutive-clean-build
  criterion now has somewhere automated to run; issue 0062 owns the Go stages
  and `setup-go` in this workflow once its Rust corpusgen lands.
