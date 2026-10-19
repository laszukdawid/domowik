# Contributor and agent guidelines

Domowik is an experimental, standalone Apache-2.0 floor-plan editor.

## Start here

- [Architecture](docs/ARCHITECTURE.md): component ownership and platform boundaries.
- [Roadmap](docs/ROADMAP.md): current capabilities and remaining work.
- [Contributing](CONTRIBUTING.md): source map, build commands and verification.
- [Glossary](docs/GLOSSARY.md): shared terminology.
- [Architecture decisions](docs/decisions/README.md): current Accepted decisions
  and Proposed directions. Proposed does not mean implemented or binding.

Read the [clean-room policy](docs/CLEAN-ROOM-POLICY.md) and
[third-party policy](docs/THIRD-PARTY-POLICY.md) before non-trivial work.
Use reviewed normative sections of the behavior and format specifications;
draft questions are not implementation instructions.

## Clean-room and licensing

- Study comparable applications through public behavior and documentation,
  never their GPL source, private schemas, internal tests, catalogs or artwork.
- Do not copy or translate another application's source, identifiers, layouts,
  test vectors or data format. Stop for review if provenance is uncertain.
- Record exact publications, consulted artifacts, adaptation type, test-data
  origins, licences and notice obligations. Keep current records in
  [Provenance](docs/PROVENANCE.md) and `docs/third-party/`.
- New dependencies, fonts, models and other distributed content require review,
  including transitive dependencies. Keep inventories and notices current.
- Imported user content retains its original rights; it is not Apache-2.0
  merely because Domowik can read it. Do not commit personal plans, credentials
  or unreviewed assets.

## Implementation conventions

- Core and CLI use Rust, in the root Cargo workspace under `crates/`.
  The Flutter editor and native/rendering workspace live in `frontend/`.
- Rust owns document semantics and geometry. Do not duplicate authoritative
  joining, splitting, room topology or persistence logic in Dart.
- Geometry APIs document units, coordinate bounds, tolerances or exactness,
  deterministic output and failure behavior.
- Performance is a design requirement. New geometry includes benchmarks;
  predicates try a proven floating-point filter before exact arithmetic.
- Exported APIs document ownership, concurrency, errors and numerical guarantees.
  FFI contracts also cover lifetimes, allocation, callbacks, cancellation,
  threading, panic transport and platform support.
- An operation log is not automatically a CRDT or collaboration protocol.
  Ordering, identity, deletion, conflict and compaction rules must be explicit.
- Format changes require tests and a compatibility assessment. Do not silently
  weaken normative requirements or call a proposal an established guarantee.

## Changes and verification

Agree a bounded scope and acceptance criteria before implementing. Keep the
roadmap, specifications and relevant architecture decisions aligned with the
current product. Public documentation describes the current state, not internal
issue administration or agent-session history.

Run `./scripts/verify.sh` from the repository root. Rust, Python and applicable
Flutter/native tools must be on PATH. It checks formatting, builds, clippy,
tests, benchmark compilation, corpus integrity, inventories, notices, release
validation and applicable editor/platform probes. Benchmark timings are
host-dependent and are not the pass/fail gate.

Only `=== VERIFICATION PASSED ===` establishes full success. Failure prints
`=== VERIFICATION FAILED ===` and its stage. Report skips and failures honestly;
a dirty tree fails the final clean-tree check. Do not erase someone else's work
to make verification pass.

Use Conventional Commits: lowercase imperative subjects under 72 characters,
without a trailing period. Preserve existing edits. Do not push, tag, publish,
change repository visibility or rewrite history without explicit authorization.

For drafting a user's plan rather than changing code, follow
[the agent guide](docs/AGENT-GUIDE.md).
