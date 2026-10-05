# Contributing to Domowik

Domowik is experimental, Apache-2.0 software. Start with
[Getting started](docs/GETTING-STARTED.md) and
[compatibility and limitations](docs/COMPATIBILITY.md). These instructions do
not authorize publication, package registration or a binary release.

## Choose a scoped change

Read [AGENTS.md](AGENTS.md), the [architecture](docs/ARCHITECTURE.md),
[roadmap](docs/ROADMAP.md), [glossary](docs/GLOSSARY.md),
[clean-room policy](docs/CLEAN-ROOM-POLICY.md) and
[third-party policy](docs/THIRD-PARTY-POLICY.md) before non-trivial work.

Use [issues/README.md](issues/README.md) to find work and its dependencies.
Issues are regular files in this repository, not a separate Git repository.
For a new increment, copy [issues/TEMPLATE.md](issues/TEMPLATE.md), choose an
unused ID, state scope/dependencies/acceptance criteria, and add an index row.
Keep the status and index synchronized. Terminal issues move to `done/` or
`not-needed/` as appropriate. Do not mark partial criteria Done.

New product behavior belongs in reviewed normative specification sections,
with provenance recorded. Draft TODOs and Proposed ADRs are not authorization
to implement a speculative requirement. Discuss changes to semantics or
compatibility before treating them as settled.

## Source and ownership map

| Location | Responsibility |
| --- | --- |
| `crates/domowik-format` | Document types, schemas and validation |
| `crates/domowik-document` | Operations, projection and history |
| `crates/domowik-container` / `domowik-assets` | File/session persistence and asset integrity |
| `crates/domowik-geometry` / `domowik-edit` | Authoritative geometry and editing consequences |
| `crates/domowik-models` / `domowik-scene` | Bounded model decode and renderer-neutral 3D data |
| `crates/domowik-cli` | Headless command workflows and SVG/PNG export |
| `crates/domowik-devtools` / `domowik-bench` | Repository checks and measurement harnesses |
| `frontend/native/src` and `frontend/native/include/` | Separate Rust workspace and C ABI |
| `frontend/dart_harness` | Hand-written Dart bindings and boundary probes |
| `frontend/lib` | Editor interaction, UI and painting |
| `frontend/linux/runner` / `frontend/macos/Runner` | Native pickers, lifecycle and platform surfaces |
| `crates/domowik-devtools/src/corpusgen` | Rust malformed-file fixture generator and corpus gates |

The former `_reference/` archive was removed on 2026-10-05. Historical Go code
remains in earlier Git commits; it is not normative and is not a development
dependency. Use the reviewed specifications for implementation work.

Rust owns document semantics. Flutter must not implement a second geometry or
editing engine. `PlanSession` owns the native handle; clients borrow it.
`FileWorkflowController` coordinates approval and action exclusion;
`FileDialogs` isolates native pickers. `PlanGeometryCache` owns geometry reuse,
not preview images or GPU buffers. Platform renderers own GPU resources.
See [Architecture](docs/ARCHITECTURE.md) for the remaining mixed responsibilities.

## Provenance and dependencies

Do not consult or copy GPL application source, internal schemas, catalogs,
fixtures or artwork for this implementation. Use reviewed Domowik behavior
and approved public/permissive references; record exact provenance for
non-trivial algorithms and adapted code. Stop for review if rights are unclear.

New dependencies require license/provenance review, including transitives.
Inventory and notice generators fail on drift; update reviewed records rather
than relaxing their checks. Follow [Third-party policy](docs/THIRD-PARTY-POLICY.md).
Imported user models are not Apache-2.0 merely because the app can open them.
Do not commit your local `assets/`, personal plans, credentials or unreviewed
third-party screenshots. Use original synthetic fixtures where possible.

## Development checks

From the root, for Rust changes:

```sh
cargo fmt --check
cargo build --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo bench --no-run --locked
```

For bridge/editor changes, also build and test the separate frontend workspace:

```sh
cd frontend
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo build --release --locked
flutter pub get
flutter analyze
flutter test
```

The real native library is required by editor lifecycle tests. Run a relevant
test while iterating, then the full suite. Test failure/cancellation and resource
ownership as well as the successful path. For documentation changes, check
local links, command working directories and claims against code/evidence.

New geometry code must include its benchmark and documented numerical
guarantees. Editor CPU harnesses can be run from `frontend/`:

```sh
flutter test benchmark/viewport_benchmark.dart
flutter test benchmark/level_render_benchmark.dart
flutter test benchmark/camera_3d_benchmark.dart
```

CPU timings are not presented frame time or input latency. Record host,
toolchains, workload and measurement limits; do not promote them into a
whole-app or comparative performance claim.

## Full verification

Linux CI requires Flutter, GTK/epoxy development packages and Xvfb rather than
accepting skipped editor checks (`DOMOWIK_REQUIRE_LINUX_EDITOR=1`). The same
canonical script builds the Linux release bundle and runs bounded software-
rendered self-test/first-frame probes; these are not hardware-performance tests.

From the root, with Rust, Python and relevant native/Flutter tools on PATH:

```sh
./scripts/verify.sh
```

The script checks core, bridge, CLI probes, corpus, licenses/notices, release
inventory, Flutter tests when installed, native harnesses, tracker consistency
and the final clean tree. macOS bundle checks require macOS; Linux file-picker
checks need GTK development files and Xvfb. Conditional SKIPPED stages are not
evidence that their platform works. See the script and
[CI workflow](.github/workflows/ci.yml) for exact current coverage.

Only `=== VERIFICATION PASSED ===` establishes full success. A failure prints
`=== VERIFICATION FAILED ===` and its stage. A working tree with edits fails
the final `P13 clean tree` check even if preceding checks passed; report that
accurately, and rerun on the clean integrated tree before claiming full
verification. Do not erase unrelated work just to satisfy the check.

## Submit a change

Include the issue ID, intended behavior, tests/commands run, actual verification
result, provenance, compatibility impact and remaining limitations. Keep code,
tests and current docs together. Preserve dated evidence; correct a current
summary without rewriting what an older experiment established.

Use Conventional Commits with lowercase imperative subjects under 72
characters, for example `docs: explain desktop editor setup`. Do not publish
artifacts or change repository visibility as part of an ordinary contribution.
