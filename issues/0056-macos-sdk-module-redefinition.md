# 0056: Diagnose intermittent macOS SDK module redefinition

- **Phase:** 2
- **Priority:** Medium
- **Status:** Open
- **Depends on:** 0037

## Problem

On 2026-09-06, the full verifier passed at `f12380f`, then failed at the
macOS release-build stage on the documentation-only checkpoint `f0f70dc`.
Swift reported duplicate `_SwiftConcurrencyShims`, `SwiftOverlayShims`,
`_SynchronizationShims`, `_SwiftOSOverlayShims` and `_SwiftXPCOverlayShims`
modules. Its invocation included both the physical `MacOSX.sdk` path and
the `MacOSX26.5.sdk` symlink to that directory. The host used Xcode 26.6
(17F113), macOS 26.6.2 (25G83), and Flutter's Dart 3.11.4 toolchain.

Running the same `flutter clean` followed by `flutter build macos --release`
again succeeded without SDK, cache or project-setting changes. This establishes
an intermittent build failure, not its cause or a fix. The first passing
verification does not establish repeated-build reliability.

Local evidence:

- `/private/tmp/domowik-0055-verify.log`: exit 0, full passing banner.
- `/private/tmp/domowik-0055-checkpoint-verify.log`: exit 1, failing banner
  names `spike 0024 Flutter macOS application`.
- `/private/tmp/domowik-0055-macos-repro.log`: unchanged clean release retry,
  exit 0. This isolated retry did not run the full verifier's bundle probes.

## Acceptance criteria

- [ ] Identify the source of the duplicate module-map paths using build
      invocations and generated metadata; distinguish an upstream SDK/cache
      defect from project settings or concurrent build activity.
- [ ] Implement a scoped remedy if the project owns the cause, or document
      a verified upstream issue and an explicit supported-toolchain boundary.
      Do not silently retry failures, disable module validation, modify the
      system SDK or weaken bundle/signature checks to obtain a pass.
- [ ] Run at least three consecutive clean macOS release builds with the
      existing bundled-core, universal architecture, signature and runtime
      probes. Record each command, exit status and toolchain. Three passes
      are regression evidence, not proof that an intermittent defect is gone.
- [ ] Run the full repository verifier and preserve the original failure
      evidence alongside the remedy's results.

## Provenance

Original local build logs and filesystem inspection. No third-party source
or patch has been consulted or adapted.
