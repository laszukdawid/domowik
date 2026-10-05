# 0057: Repair the macOS build from a clean checkout

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024

## Original failure

The 2026-09-06 reproduction failed on the first verifier run in a fresh clone
and passed after the archive existed. The failing stage was
`spike 0024 Flutter macOS application`:

```text
error: Build input file cannot be found:
  '.../Flutter/ephemeral/.symlinks/plugins/domowik_bridge_plugin/macos/rust/libdomowik_bridge.a'.
  Did you forget to declare this file as an output of a script phase or
  custom build rule which produces it? (in target 'domowik_bridge_plugin'
  from project 'Pods')
```

Reproduced on 2026-09-06 in a detached worktree at `0fa2b57`, then isolated
across four builds on the same tree and toolchain:

| State | Result |
|---|---|
| `flutter clean`, archive absent | **BUILD FAILED** |
| `flutter clean`, archive present (even a stale one) | Built |
| no clean, archive absent | Built |
| no clean, archive present | Built |

Xcode's build-input existence check for the `-force_load` archive runs BEFORE
the pod's `Build Domowik Rust core` script phase that produces it. Only
`flutter clean` — which `scripts/verify.sh` runs deliberately, so the
`codesign --verify --deep --strict` assertion is meaningful — regenerates the
Pods project into the state where that check precedes the phase.

`flutter_plugin/macos/.gitignore` ignores `rust/`, so a fresh clone has no
archive. The failure is therefore invisible to anyone whose working directory
already holds one from an earlier build, which is every machine that has built
this app once.

**This is not issue 0056.** That failure is an intermittent duplicate Swift
SDK module map (`_SwiftConcurrencyShims`, `SwiftOverlayShims` and others) with
a different diagnostic. This one is deterministic and reproduces on demand.

## Why it matters

AGENTS.md makes a passing `./scripts/verify.sh` the project's gate and forbids
reporting a verification result without one of its two banners. That gate is
currently not reproducible from a clean checkout on the only platform where
the GUI stages run at all. A first-time contributor's first verification run
fails for a reason unrelated to their change.

## Scope

Belongs here: the ordering relationship between the pod's script phase and
Xcode's input validation, and whatever repair makes a clean checkout build on
the first attempt.

Explicitly excluded: issue 0056's intermittent SDK module redefinition; the
`always_out_of_date` decision, whose recorded rationale — that declaring
`:output_files` lets Xcode skip the phase and link a stale archive — must be
preserved by any fix; Linux and Android packaging (issue 0024).

## Acceptance criteria

- [x] The root cause is identified from the generated Pods project and build
      invocations, distinguishing CocoaPods' emitted target settings from
      Flutter's symlink layout and from Xcode's dependency analysis.
- [x] A clean checkout with no pre-existing archive completes
      `flutter clean && flutter build macos --release` on the FIRST attempt.
      Do not obtain a pass by committing a build artifact, by removing
      `flutter clean` from the verifier, or by re-running the build on
      failure.
- [x] Any fix that declares build inputs or outputs demonstrates, with a
      changed `ABI_VERSION` observed in a bundle probe, that a stale archive
      can still never be linked — the defect the `always_out_of_date` comment
      records.
- [x] The repair is exercised from a genuinely fresh clone or worktree, with
      each command, exit status and toolchain recorded, alongside the original
      failure evidence.
- [x] `./scripts/verify.sh` passes twice in succession from that fresh tree.

## Repair

`s.script_phase` now declares BOTH `:always_out_of_date => '1'` and
`:output_files => ['$(PODS_TARGET_SRCROOT)/rust/libdomowik_bridge.a']`. Each
line answers a different failure and neither is sufficient alone:

- `:output_files` gives the archive a declared PRODUCER, which is the edge
  Xcode's dependency analysis needed. `OTHER_LDFLAGS` hands the same path to
  `-force_load`, and Xcode validates that operand as a build INPUT before any
  script phase runs; with no producer the graph had no ordering constraint, so
  a checkout with no archive failed input validation.
- `:always_out_of_date => '1'` (emitted as `alwaysOutOfDate = 1`) keeps the
  phase running on every build. Declaring an output alone would let Xcode
  treat the phase as up to date whenever the archive merely exists and link a
  stale one, the defect the original comment recorded and the reason that
  comment must be preserved.

Confirmed in the generated `Pods/Pods.xcodeproj/project.pbxproj`: the
`Build Domowik Rust core` phase carries both `outputPaths` and
`alwaysOutOfDate = 1`.

Adding `:output_files` changes the podspec's CocoaPods checksum, so
`flutter_app/macos/Podfile.lock` moves from
`0b27cb573e31f954aa2b3262d08ce4bf860210b6` to
`56b2190ab1cb1f16ae267954764bc5ffb1a671c5`. The lockfile is tracked, so that
correction is part of the fix rather than an artifact to discard; without it
the `clean tree` stage fails.

## Verification evidence

Toolchain: macOS 26.6.2 (25G83), Xcode 26.6 (17F113), CocoaPods 1.17.0,
Flutter 3.41.6 stable (framework `db50e20168`), rustc/cargo 1.98.0.

A genuinely fresh clone was made into `mktemp -d` with no symlink or copy of
any root build directory; only the shared Cargo registry cache was reused.
`flutter_plugin/macos/rust/libdomowik_bridge.a` was confirmed ABSENT before
and after `flutter clean`.

First build, clone at `e4dbfbe`:

| Step | Command | Exit | Observed |
|---|---|---:|---|
| clean | `flutter clean` | 0 | archive still absent |
| build | `flutter build macos --release` | 0 | ZERO `Build input file cannot be found`; `domowik_spike_app.app` produced |

Stale-archive regression, same clone, `ABI_VERSION` raised to 11 with the
Flutter app, self-test and harness expectations updated to match, rebuilt
INCREMENTALLY with **no** `flutter clean`:

| Step | Command | Exit | Observed |
|---|---|---:|---|
| rebuild | `flutter build macos --release` | 0 | phase re-ran despite the declared output existing |
| bundle probe | `DOMOWIK_SPIKE_SELFTEST=1 …/domowik_spike_app` | 0 | `ok abi version (got 11, want 11)`, `BUNDLE SELFTEST PASSED` |
| framework symbol | `dlopen`/`dlsym` reader on the embedded framework | 0 | exported `domowik_abi_version` = 11 |

The ABI bump was a temporary probe in the clone only; it was reverted to 10
and the tree returned to a clean state before verification. The clone was then
fast-forwarded to `dc872d3`, which carries the lockfile-only correction, with
a clean tree and the plugin checksum `56b2190ab1cb1f16ae267954764bc5ffb1a671c5`.

Two sequential full verification runs used the clean clone at `dc872d3`,
with Rust and Go on PATH and Flutter made explicit:

```sh
PATH="/Users/dawidlaszuk/develop/flutter/bin:$PATH" ./scripts/verify.sh
```

| Run | Exit | Banner |
|---|---:|---|
| 1 | 0 | `=== VERIFICATION PASSED === all stages` |
| 2 | 0 | `=== VERIFICATION PASSED === all stages` |

Neither log contains a `SKIPPED` line or any occurrence of
`Build input file cannot be found`. Both runs repeat `flutter clean` followed
by `flutter build macos --release`; the archive already exists by then.
The separate first build above is the evidence for the absent-archive case. One
`will be run during every build because it does not specify any outputs`
warning remains in each log and belongs to Flutter's own generated
`Flutter Assemble` target in the `Runner` project, not to this pod.

The clone is `/private/tmp/domowik-0057-FS4MQ5/repo`. Its parent retains the
raw logs outside the repository:

- `01-flutter-clean.log` and `02-first-build.log`: first absent-archive build.
- `03-abi11-incremental-build.log` and `04-bundle-selftest.log`: changed-ABI test.
- `05-fullverify-run1.log` and `06-fullverify-run2.log`: full verifier runs.

The parent also contains `read_abi.c` and its executable, which load the
embedded framework and call `domowik_abi_version` directly. The probe reported
11 after the temporary change. A root-agent repeat after restoration and both
verifier runs reported `FRAMEWORK_EXPORTED_ABI=10`, exit 0. No build artifact
or probe source is included in the product.

## Provenance

Original local reproduction and build-log inspection during a repository
review on 2026-09-06. No third-party source or patch has been consulted or
adapted.

## Closure

- **Implementation/spec commits:** `e4dbfbe` (podspec `:output_files`
  producer edge, comment), `dc872d3` (`Podfile.lock` checksum correction)
- **Follow-up issues:** 0056 (intermittent macOS SDK module redefinition,
  a separate and non-deterministic failure); 0061 (CI never runs the macOS
  or GUI stages, so this regression can still only be caught locally)
