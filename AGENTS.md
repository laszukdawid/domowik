# AGENTS.md

Guidelines for contributors and AI coding agents working on Domowik.

## Before working

Read these documents for non-trivial work:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/GLOSSARY.md](docs/GLOSSARY.md)
- [docs/CLEAN-ROOM-POLICY.md](docs/CLEAN-ROOM-POLICY.md)
- [docs/THIRD-PARTY-POLICY.md](docs/THIRD-PARTY-POLICY.md)

The behavior specification defines product semantics once sections are marked
normative. Draft TODOs are questions, not implementation instructions.

## Clean-room policy (mandatory)

Domowik is legally and technically independent of GPL-licensed floor-plan
applications. Follow the full policy in
[docs/CLEAN-ROOM-POLICY.md](docs/CLEAN-ROOM-POLICY.md):

- Study comparable applications through their public behavior, user
  documentation, and independently published references, not their source.
- Never copy or translate source, identifiers, file layouts, tests, or data
  formats from another application.
- Implementation work uses Domowik's reviewed behavior specification.
- Record exact provenance for algorithms and adapted permissive code.
- Stop and request review if the provenance or license is uncertain.

This policy reduces risk; it is not legal advice or a guarantee that any
particular work is non-infringing.

## Licensing and dependencies

Domowik is Apache-2.0. Follow
[docs/THIRD-PARTY-POLICY.md](docs/THIRD-PARTY-POLICY.md) before adding code,
tools, native libraries, models, textures, fonts, datasets, or service
components. In particular:

- Dependencies distributed with Domowik must be license-compatible and
  recorded with their notices.
- Permissive licenses are strongly preferred. Copyleft or proprietary
  distributed components require an explicit decision record and review.
- System drivers and optional hosted reconstruction tools are assessed
  separately from linked application dependencies; their licenses still
  need documentation.
- User content is not relicensed under Apache-2.0.

## Decisions and specifications

- Architecture Decision Records (ADRs) live in `docs/decisions/`.
- `Accepted` means a decision is in force. `Proposed` means it still has a
  validation gate. Do not present proposed technology as settled.
- Format behavior must distinguish normative requirements from examples and
  candidates. Format changes require tests and a compatibility assessment.
- Do not claim that an operation log automatically provides collaboration or
  CRDT semantics. Ordering, identity, conflict, deletion, and compaction rules
  must be explicit.

## Work tracking

`issues/` holds the work tracker as regular tracked files in this repository.
It WAS a standalone nested Git repository during early design; commit `00893a5`
("track issues as regular files instead of gitlink") changed that, so the
issues are present in every clone and in CI. The `.gitignore` rule for it is
retained commented-out as the record of that change.

- Start from an issue with explicit dependencies and acceptance criteria.
- Update its status and index when work begins or ends. `verify.sh` enforces
  their agreement via `domowik-tracker-check`: a status line that
  disagrees with `issues/README.md`, a duplicate id, a missing status line, or
  an orphan index row fails the run at the `tracker consistency` stage.
- Add a new issue when discovered work cannot honestly fit existing criteria.

## Git commits

Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```text
<type>[optional scope]: <imperative description>
```

Use lowercase imperative subjects under 72 characters with no trailing
period. Typical types: `feat`, `fix`, `docs`, `refactor`, `perf`, `test`,
`build`, `ci`, and `chore`. Use `BREAKING CHANGE:` when a released public
contract is broken.

## Code conventions (when code exists)

- **Core and CLI: Rust** ([ADR 0008](docs/decisions/0008-rust-core-implementation.md),
  superseding ADR 0002). Cargo workspace at the repository root, crates under
  `crates/`, toolchain pinned in `rust-toolchain.toml`. Formatted with
  `cargo fmt`; `cargo clippy -- -D warnings` is a verification stage.
- **Performance is a first-class requirement, not a later optimisation.** The
  rewrite exists because two geometry predicates missed the 10,000 ns/op bound.
  New geometry code lands WITH its benchmark, and predicates must try a
  floating-point filter with a proven error bound before escalating to exact
  arithmetic — the measured gap between those paths was ~2,000x.
- **The Go implementation is FROZEN reference at `_reference/go/`.** Read it;
  do not port it mechanically. The specifications are normative and the Go code
  is not — where they disagree, the spec wins. The leading underscore keeps the
  tree out of the Go toolchain's default patterns; do not rename it.
- **Repo-hygiene tooling is Rust** (`crates/domowik-devtools`:
  `domowik-jsoncheck`, `domowik-tracker-check`). It is not product code and is
  not distributed; it lives in the workspace so `cargo fmt`, `cargo clippy`
  and `cargo test` cover it.
- **One Go package survives**: `internal/corpusgen`, the issue-0028 fixture
  generator, still formatted with `gofmt`. Its hand-rolled ZIP writer emits
  deliberately malformed archives, which the `zip` crate cannot do; whether to
  port or freeze it is issue 0062's open question. The Go module path remains
  `domowik.dev/domowik` per
  [ADR 0006](docs/decisions/0006-naming-and-identity.md) and the
  [naming registry](docs/NAMING.md).
- Proposed GUI: Dart/Flutter, subject to the FFI feasibility gate.
- Document exported APIs, ownership, concurrency, errors, numerical
  guarantees, and FFI safety. Avoid comments that merely restate code.
- Geometry APIs must state units, coordinate bounds, tolerance or exactness,
  deterministic-output rules, and failure behavior.
- FFI boundaries must state allocation ownership, lifetimes, threading,
  callback rules, cancellation, panic/error transport, and platform support.

## Verification

Run `./scripts/verify.sh` (equivalently, each command inside it) from the
repository root; it must exit 0. Both toolchains must be on PATH — the script
deliberately does not source `~/.cargo/env`, because a verification run that
depends on a developer's home directory cannot be trusted in CI.

Rust stages: `cargo fmt --check`, `cargo build --workspace --all-targets`,
`cargo clippy -- -D warnings`, `cargo test --workspace`, and
`cargo bench --no-run`. **Benchmarks are compiled on every run** so the
measurement harness cannot rot; timings are not a pass/fail gate because they
are host-dependent.

A `rust dependency inventory` stage regenerates `docs/RUST-DEPENDENCIES.md`
from `cargo metadata` and diffs it, so **adding a dependency fails the build**
until it is recorded and its licence reviewed
([THIRD-PARTY-POLICY](docs/THIRD-PARTY-POLICY.md)). It reports declared
licences only; it is not a licence audit and does not build `NOTICE`.

Go stages cover `internal/corpusgen` only: `go build`, `go test`, `gofmt -l`,
`go vet`, the corpus gates, dependency-drift and module-graph guards. The JSON
fence checks and the `tracker consistency` stage are Rust binaries from
`domowik-devtools` and therefore run AFTER `cargo test`; tracker consistency
still runs just before the clean-tree check, so a tracker repair stays visible
as uncommitted work rather than masked by it.

**Withdrawn, and owed back:** the Go release-inventory verifier froze with the
Go product it describes, and CLI stream/exit-code probes P1-P3 plus
cross-compilation coverage went with `./cmd/domowik`. Reinstating the probes
against the Rust binary is an acceptance criterion of the CLI port, and a Rust
release/SBOM process is owed. See
[ADR 0008](docs/decisions/0008-rust-core-implementation.md).

**Every stage names itself on failure.** A failing run prints
`=== VERIFICATION FAILED ===` with the STAGE, line, and captured exit status;
a passing run ends with `=== VERIFICATION PASSED ===`. This exists because the
script previously aborted under `set -e` with no message, so the output tail
showed the last stage that SUCCEEDED and a failed run was indistinguishable
from a clean one to anyone not capturing `$?` (ED-0077). Do not report a
verification result without one of those two banners in the output.
