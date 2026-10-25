# ADR 0013: Architectural primitives and assemblies

- **Status:** Accepted (initial straight-stair slice; other assembly tools deferred)
- **Date:** 2026-10-19

## Decision

Separate elevation references, physical construction bodies, hosted openings and
connectivity. Architectural assemblies coordinate those primitives and derive
dependent geometry. A staircase is not a furniture asset or two independent
symbols on different storeys. Preserve the existing authoritative wall/opening
schemas and room enclosure rules rather than maintaining duplicate bodies for
them or replacing their semantics with generic mesh grouping.

The first implementation supports rectangular longitudinally sloped bodies,
through-cuts in those bodies and level floors, two-endpoint connections and
straight-flight stairs. Rust owns creation/conversion consequences, validation,
treads, guards, cuts and 2D symbols; CLI/editor use the same builders. Landings,
columns and beams can be represented as bodies. Full ramp, railing, roof and
lift assemblies, arbitrary profiles and multi-flight stairs remain future work.

The editor authors stairs directly from a toolbar tool, upper end to lower end,
with a read-only Rust preview and automatic nearest-lower-level selection. The
Floor panel edits names, counts and endpoint elevations; Rust stores elevation
offsets against the existing level datums. Placeholder-room conversion remains a
CLI convenience, not a prerequisite or alternate drawing engine in the editor.

Use the explicit experimental core schema `0.2` for the new entity kinds, with
ordinary `0.1` files remaining readable and unmodified by unrelated saves. This
is a core evolution permitted by ADR 0011, not a shortcut to claim its still-
pending namespaced extension wire contract is implemented. Unsupported older
readers must reject essential construction rather than preserve it opaquely while
editing its dependencies unsafely.

## Contract and verification

[Architectural primitives](../ARCHITECTURAL-PRIMITIVES.md) is normative for these
entities, shape limits, operation/undo behavior and compatibility. Synthetic
tests cover references and bounds, admission rollback, undo/replay, width/cut
coordination, shared-room preservation, file round trips, schema gating, exported
symbols, both endpoint levels and derived 3D elevations. Geometry benchmarks
cover stair derivation, both-level graphics and hosted floor cuts. This does not
establish general solid modelling, accessibility or building-code compliance.

Provenance: original owner-requested product design, Domowik source/specification
and existing reviewed geometry/triangulation implementations. No third-party
application source, data format, artwork or fixtures were consulted or copied;
no new dependencies or distributed content were introduced.
