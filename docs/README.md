# Documentation

Domowik is experimental development software. These guides describe the
current implementation; they do not announce a release or settle proposed
architecture decisions.

## Use Domowik

- [Getting started](GETTING-STARTED.md): prerequisites, CLI and desktop builds,
  native-backed tests and troubleshooting.
- [User guide](USER-GUIDE.md): edit a plan, import furniture, resize objects,
  navigate the Linux 3D view and change preferences.
- [Compatibility and limitations](COMPATIBILITY.md): platform evidence,
  experimental files and features, and precautions for important plans.
- [CLI walkthrough](../README.md#try-it): create and export a small apartment.
  The built CLI's `--help` is the current command reference.

## Contribute

- [Contributing](../CONTRIBUTING.md): source map, issue workflow and checks.
- [Issue index](../issues/README.md): dependencies and acceptance criteria.
- [Contributor/agent rules](../AGENTS.md): project conventions.
- [Clean-room policy](CLEAN-ROOM-POLICY.md) and
  [third-party policy](THIRD-PARTY-POLICY.md): provenance, dependencies and
  user-content rights.

## Understand the design

- [Architecture](ARCHITECTURE.md): authoritative core, UI services and native
  resource boundaries; accepted responsibilities versus experimental choices.
- [Roadmap](ROADMAP.md): phases, validation gates and dated closure records.
- [Glossary](GLOSSARY.md): shared vocabulary.
- [Behavior specification](BEHAVIOR-SPEC.md): product semantics. Only sections
  marked normative are implementation requirements; draft TODOs are questions.
- [Format specification](FORMAT-SPEC.md): logical document, package and
  compatibility requirements, including explicitly provisional sections.
- [Numerical model](NUMERICAL-MODEL.md): geometry guarantees and open decisions.
- [Decision records](decisions/README.md): Accepted decisions are in force;
  Proposed decisions still have gates.
- [Asset strategy](ASSET-STRATEGY.md): packaging and size goals, not a promise
  that every proposed storage profile is implemented.
- [Release inventory](RELEASE-INVENTORY.md): existing license/SBOM checks and
  remaining binary-release obligations.

## Evidence and historical material

Issue closures and `docs/evidence/` record tests, measurements and provenance
for a particular increment and host. They are not a continuously updated
platform-support matrix. The [bridge spike README](../spikes/0024-flutter-rust-bridge/README.md)
documents the native contract, probes and platform experiments.

The former `_reference/go/` archive was removed from the source tree on
2026-10-05. Its earlier committed contents remain in Git history, not as a
current product or development dependency.
Executive decisions and dated audit reports preserve development history.
Use the guides above for current onboarding, and the linked specifications
for authority; neither a historical measurement nor a draft establishes a new
product guarantee.
