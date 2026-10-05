# 0092: Promote the actively developed frontend

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024, 0091

## Scope

Promote the usable Flutter interface from the issue-0024 spike to root-level
`frontend/`, named Domowik (`domowik_editor` package/executable). Keep the
shared core authoritative, native ABI ownership and private development
application IDs stable, and retain existing edits and tests. This changes
organization and development status, not release/platform acceptance.

Assess issue 0025's wgpu work without adopting an unimplemented renderer or
adding a new runtime dependency. No publication, commit or push is implied.

## Acceptance criteria

- [x] Flutter runs/builds from `frontend/`; native bridge and plugin have clear
      homes and all existing source/test files are preserved.
- [x] Build scripts, Cargo/Flutter manifests, release inventory, CI, task
      commands and current documentation use the new paths and names.
- [x] Local Linux native-backed tests, release build and both runtime probes
      pass; full verification passes in a clean workspace snapshot.
- [x] wgpu assessment identifies exactly what can be reused and what remains
      unimplemented; the current working renderer is preserved.
- [x] Historical experiment records remain dated, and relocation does not
      imply a stable format, released product or completed platform gates.

## Closure — 2026-10-05

Moved the application to `frontend/`, with the native adapter in `native/`,
platform packaging in `plugin/`, and shared bindings in `dart_harness/`.
Displayed name is Domowik; Dart package/executable is `domowik_editor` and the
Rust adapter package is `domowik-frontend-bridge`. ABI symbols, development
application IDs and probe environment names are preserved. All 128 previously
tracked experiment files have corresponding files in the new layout; existing
uncommitted application work is also retained.

Updated workspace/package manifests, Linux/Android/macOS build paths, Docker
helpers, task commands, CI bundle paths, release inventory and current guides.
The plugin's stale-artifact check now considers Rust sources/manifests rather
than documentation or test/build output. Cargo normalized the renamed package's
lockfile ordering without changing Rust dependency versions. Flutter resolution
uses the CI-pinned SDK (3.41.6), including its SDK-pinned dependency versions.

The issue-0025 directory has no renderer implementation to merge. Its proposed
interop/lifecycle design may inform a future backend using the current neutral
scene payloads and UI controls. The working Linux GL backend remains unchanged;
no wgpu runtime dependency or platform-acceptance claim is added.

Local Linux evidence:

- Native adapter tests, frontend analysis and all 305 widget tests pass.
- Linux release builds from the actual working directory; both bundled
  self-test and first-frame probes pass under Xvfb/software rendering.
- Full `DOMOWIK_REQUIRE_LINUX_EDITOR=1 ./scripts/verify.sh` exits 0 with
  `=== VERIFICATION PASSED === all stages` in an isolated source snapshot.
  Rust checks, FFI harnesses, all three frontend CPU harnesses, Linux file
  chooser/prerequisite checks and both Linux bundle probes ran. macOS runtime
  stages are explicitly skipped on Linux; Android runtime is not claimed.
- Release-inventory regression: 27 checks; CI summary regression: 27 cases;
  shell declaration check: eight files; current-guide local links: no missing
  targets across eleven guides. Whitespace and tracker checks pass.

The initial implementation was left uncommitted. The owner subsequently
authorized committing the verified pending application, frontend and CI work
together. No push is authorized by that follow-up. Disposable verification
snapshot commits only satisfy the canonical clean-tree check.
