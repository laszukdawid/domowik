# 0008: Scaffold Go module and CI

- **Phase:** 1
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0009

## Problem

Create the canonical Go module, CLI stub, package boundaries, formatting,
tests, static analysis, dependency/SBOM checks, and ordinary desktop CLI
cross-builds. Mobile FFI packaging belongs to 0024.

## Acceptance criteria

- [x] `go build ./...`, `go test ./...`, formatting, static analysis, and
      license checks run as required merge checks.
- [x] Linux amd64/arm64 CLI builds succeed; other supported CLI targets are
      listed explicitly.
- [x] Dependency inventory records direct/transitive licenses and notices.
- [x] Parent AGENTS verification section contains actual commands.
- [x] No Android executable build is presented as proof of Flutter FFI.

## Closure

- **Main repository commit:** `3791779`
  (`build(issue-0008): scaffold Go module and CI`).
- **Tracker start commit:** `f571fec`
  (`chore(issue-0008): mark scaffold in progress`).
- **Verification:** `./scripts/verify.sh` exited 0 under
  `go1.27.0 linux/amd64`; native tests, formatting, vet, dependency drift,
  inventory comparison, CLI probes, and Linux amd64/arm64 static builds all
  passed. The final observed artifacts were amd64 SHA-256
  `e49badaaa9475ebeaadd9f38b36885c0f3d2f0953b09ef958b63efb5fcb71942`
  (2,366,034 bytes, x86-64) and arm64 SHA-256
  `3f1aeb5df06a15422f2b2173d14e8fbe7f9df8a5a712dd170ba89ef1352fd1dc`
  (2,350,830 bytes, AArch64); both were statically linked.
- **Dependency and provenance evidence:** the generated inventory contains
  only the main module `domowik.dev/domowik`, the pinned Go toolchain/stdlib,
  and runtime build information. CI pins `actions/checkout` v5.0.0 at
  `08c6903cd8c0fde910a37f88322edcfb5dd907a8` and `actions/setup-go` v6.0.0
  at `44694675825211faa026b3c33043df3e48a5fa00`. No external Go module,
  bundled third-party code, or notice obligation is introduced. The code and
  contract were AI-authored from Domowik's reviewed requirements without
  adapting third-party source.
- **Review:** GLM-5.3 recorded contract agreement and explicit worker/reviewer
  COMPLETION AGREED on 2026-08-23. The accepted P6 erratum derives the
  canonical module value with `go list -m` to avoid a naming-sweep self-hit;
  it preserves the one-module guard.
- **Release follow-up:** issue 0036 owns full SPDX/CycloneDX generation,
  automated source-license scanning, and NOTICE assembly before the first
  public binary release; issue 0018 now depends on it.
