# 0087: Require Linux editor CI and retain Flutter template notices

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0061, 0086

## Scope

Prepare source publication without changing repository visibility. Preserve the
owner's assets ignore rule. Require Linux editor prerequisites in CI, run the
native picker checks, Flutter tests and release-bundle probes. Investigate and
retain the license for already committed Flutter template files and icons.
History rewriting is a separate explicitly scoped owner operation.
Repair existing CI harness portability/cache assumptions encountered while
checking the mandatory Linux path; preserve negative-proof assertions.

## Acceptance criteria

- [x] Linux CI installs pinned Flutter and native dependencies and refuses
      missing prerequisites rather than succeeding with skipped editor checks.
- [x] Canonical verification builds the Linux application and runs bounded
      bundle self-test/first-frame probes; no hardware-performance claim.
- [x] Missing-tool prerequisite checks have negative regression coverage.
- [x] Flutter template source/license provenance and copyright text are retained.
- [x] Verification and tracker state reflect actual results.

## Provenance

Owner request, 2026-10-04. Existing repository CI/probes and original shell
checks. Flutter BSD-3-Clause license consulted from the installed SDK and
https://github.com/flutter/flutter/blob/3.41.6/LICENSE. No new runtime dependency.

## Closure — 2026-10-04

Linux CI now installs the existing reviewed Flutter action/pin (3.41.6) and
GTK/epoxy/native/Xvfb prerequisites. `DOMOWIK_REQUIRE_LINUX_EDITOR=1` makes
missing tools/packages fail at startup instead of entering optional local
skip branches. The nine missing-tool cases and missing-package case are
negative-proven; the complete synthetic fixture passes. A real missing-Flutter
run printed `=== VERIFICATION FAILED ===` at mandatory Linux prerequisites.

Canonical verification resolves Flutter packages, analyzes/tests the editor,
runs three CPU harnesses and native file-picker tests, builds the Linux release
bundle and runs timeout-bounded software/Xvfb self-test and first-frame probes.
Local Flutter was 3.47.5/Dart 3.13.4; the pinned hosted 3.41.6 runner was not
executed by this work. No hosted-CI success or hardware-performance claim.

Retained the unchanged Flutter BSD-3-Clause copyright/license and a template
scope/provenance document, linked from README. Compiled GUI dependency/font
inventory remains a separate gate. Root `/assets/` stays ignored; anchoring the
owner's rule avoids ignoring reviewed nested assets and breaking CI fixtures.

CI preflight testing found and fixed three older harness assumptions: macOS-
only mktemp syntax, global ISC-text assertions hidden by another runtime crate's
identical notice, and the host target used for the macOS artifact negative proof.
The existing ryu 1.0.23 spike-only fixture is exactly pinned and may be fetched
if absent from cache; no fixture code is built or distributed. Notice assertions
now isolate the fixture asset section and retain mutation-proof omissions.

Validation: workflow YAML parsed with the mandatory flag and Flutter pin
asserted; shell syntax/declaration, whitespace and tracker checks passed.
All 27 summary tests and all 27 release-inventory checks passed. Canonical
verification analyzed the editor, passed all 305 widget tests, native checks,
three CPU harnesses, Linux release build and both bundle probes, then printed
`=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1. macOS checks were
explicitly skipped. Log: `/tmp/domowik-source-preparation-verify.log`.

Web is retained as a required but deferred target in issue 0088 and current
guides. Its task reports the missing backend without generating scaffolding.
No web implementation, visibility change or binary release. Per the owner's
history-scope choice, these current changes remain uncommitted and were not
included in the GitHub rewrite recorded by issue 0089.
