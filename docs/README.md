# Domowik documentation

Domowik is a local-first toolkit for making floor plans: a performance-
optimized Rust core, a scriptable command-line interface (CLI), and a Flutter
based UI which produces desktop, web and mobile applications.

The core is designed to do useful work quickly on real plans. It combines
incremental geometry updates, spatial queries, caching and robust numerical
predicates. These techniques are measured in the project's benchmark suite;
they are not a comparative speed claim, and whole-editor performance targets
are still being validated.

Domowik is designed for both hands-on use and automation. People can edit in
2D and explore in 3D. Scripts and AI agents can use the CLI's stable references,
checks, review exports and machine-readable output without implementing a
second geometry engine.

The software and `.domo` format are experimental. Keep backups of important
plans; see [compatibility and limitations](COMPATIBILITY.md).

![A furnished home open in Domowik](images/welcome-home-editor.png)

## Start here

- **Explore a plan:** open the [Welcome Home example](../examples/welcome-home/README.md)
  to see a furnished plan in 2D and 3D. Its original mock models are included.
- **Use the editor or CLI:** follow [Getting started](GETTING-STARTED.md) for
  build prerequisites and short first-run instructions.
- **Edit a plan:** the [user guide](USER-GUIDE.md) covers walls, openings,
  levels, furniture, saving and 3D navigation.
- **Automate a plan:** the [CLI walkthrough](../README.md#try-it) shows a small
  apartment from creation to export. For photo-based drafting, use the
  [agent guide](AGENT-GUIDE.md). The built CLI's `--help` is the command
  reference.
- **Check platform and file limits:** read
  [compatibility and limitations](COMPATIBILITY.md) before relying on the
  editor or format for important plans.

## How it fits together

The Rust core owns document validation, edits, geometry and persistence. The
CLI and Flutter editor are two front ends to that shared core. Desktop uses a
native bridge; the browser build uses WebAssembly. The 3D renderer consumes
scene data derived from the same document and is separate from model and
geometry code. See [Architecture](ARCHITECTURE.md) for component boundaries and
the current platform split.

## Build and contribute

- [Contributing](../CONTRIBUTING.md): source map, development checks and scope
  guidelines.
- [Contributor and agent rules](../AGENTS.md): repository conventions.
- [Clean-room policy](CLEAN-ROOM-POLICY.md) and
  [third-party policy](THIRD-PARTY-POLICY.md): provenance, dependency review
  and user-content rights.

## Design and specifications

- [Architecture](ARCHITECTURE.md): current components, data flow and performance
  design.
- [Roadmap](ROADMAP.md): current phase, remaining validation and later
  directions.
- [Glossary](GLOSSARY.md): shared vocabulary.
- [Behavior specification](BEHAVIOR-SPEC.md): product semantics; only sections
  marked normative are implementation requirements.
- [Format specification](FORMAT-SPEC.md): logical document, package and
  compatibility requirements, including provisional areas.
- [Numerical model](NUMERICAL-MODEL.md): geometry guarantees and open
  conformance obligations.
- [Decision records](decisions/README.md): accepted decisions and proposals
  with validation gates.
- [Asset strategy](ASSET-STRATEGY.md): asset handling and package profiles.
- [Release inventory](RELEASE-INVENTORY.md): dependency and artifact coverage
  and remaining release obligations.

## Reference material

[Provenance](PROVENANCE.md) records external mathematical references, public
format documentation and attributed test inputs used by the implementation.
The [decision records](decisions/) describe current architectural choices and
proposals, not internal review history.

The [publishing guide](PUBLISHING.md) describes the GitHub Pages site and local
preview.
