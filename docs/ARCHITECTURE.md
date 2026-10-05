# Architecture

Status: **Proposed architecture.** Accepted project goals are separated below
from technology choices that still require feasibility spikes.

Current implementation (2026-09-05): the core and CLI are Rust (accepted ADR
0008). A Flutter macOS editor prototype uses that core through issue 0024's
native bridge. Flutter's target-platform and performance gates remain open
under ADR 0003; there is no 3D renderer yet.

## Accepted system boundaries

Domowik is a standalone local-first application with two first-class entry
points:

- A headless CLI for document creation, inspection, validation, scripted
  edits, asset management, and 2D export.
- A GUI for interactive 2D editing and, later, full 3D exploration.

Both must use one authoritative document and geometry implementation. Optional
sync, reconstruction, and HomeHero adapters remain separate services.

## Proposed component model

```text
┌────────────────────┐          ┌─────────────────────────┐
│ CLI                │          │ Flutter GUI             │
│ headless workflows │          │ interaction + 2D canvas │
└──────────┬─────────┘          └────────────┬────────────┘
           │                                │ native bridge
           └──────────────┬─────────────────┘
                          ▼
                 ┌──────────────────┐
                 │ Core library     │
                 │ Rust (accepted)  │
                 │ document + ops   │
                 │ geometry + index │
                 └──────────────────┘

Flutter GUI ── platform surface bridge ──▶ proposed wgpu-native renderer
```

The core does not call wgpu. The GUI owns rendering surfaces and submits
render-ready scene data derived from the document.

## Core responsibilities

- Project document, entity schemas, and validation.
- Applying atomic operation batches and projecting current state.
- Local command handling and undo/redo semantics.
- Robust 2D geometry: intersections, wall outlines and joins, room topology,
  opening cuts, spatial queries, and measurement.
- Spatial indexing for hit testing and visible-region queries.
- A semantic 2D paint model used by CLI exporters and mirrored by the GUI.
- Content-addressed asset manifests and integrity checks. Asset bytes may be
  embedded or stored in an external cache.

The core must not depend on Flutter, platform UI frameworks, network services,
or HomeHero.

## CLI scope

The planned CLI covers headless document workflows, not every GUI capability:

```text
domowik new
domowik info
domowik validate
domowik apply
domowik ops
domowik asset add|list|verify|remove
domowik compact
domowik pack --thin|--portable
domowik export svg|png
```

Headless 3D rendering may be added later, but is not required by the initial
architecture.

## GUI and native boundaries

### Rust-to-Flutter bridge (macOS prototype validated)

The editor calls a Rust C ABI through `dart:ffi`. The macOS plugin links a
Rust static archive into an embedded framework. Opaque plan handles, explicit
buffer ownership, error and panic containment, cancellation and close/drain
rules are exercised by Rust, C, Dart and built-bundle probes. See
[the spike evidence](../spikes/0024-flutter-rust-bridge/README.md).

`domowik-edit` owns gesture consequences (joins, splits and opening remaps),
while `domowik-container::Plan` owns document/file sessions. The CLI and
bridge share these APIs. Flutter chooses gestures and paints the geometry
returned by Rust; it must not duplicate core editing semantics (ED-0101).

Linux and Android execution were validated at ABI 12 (2026-09-12): a Flutter
application loads and drives the core from inside its own bundle on both, and
on Linux from a machine carrying no toolchain. Android was an emulator and one
ABI; neither has been re-run at the current ABI 13. iOS is deferred and macOS
is a local-development target. ADR 0003 remains Proposed until its measured
editor-performance gate passes; its platform gate no longer holds it.

### 3D surface bridge (unvalidated)

`wgpu-native` is a candidate renderer because it supports portable modern GPU
APIs and programmable shaders. Dart FFI alone is insufficient: Flutter needs a
platform plugin or texture/surface bridge, lifecycle handling, callback
scheduling, and native binary packaging for every supported ABI. Phase 3
starts with this spike. Prebuilt mobile binaries are not assumed.

## Document and synchronization architecture

The logical document uses stable entity IDs and identified operation batches.
This supports deterministic replay and provides useful synchronization
metadata. It does **not** make the document a CRDT.

The format must eventually define operation identity, single-writer replica
actors and counters, causal frontiers, idempotency, deletion/tombstone behavior,
batch atomicity, per-field version metadata, and deterministic conflict rules.
A future sync service may start with explicit conflict detection and per-field
LWW while preserving both operation streams. Real-time collaboration requires
a separate design and test corpus.

Undo history, synchronization history, and audit history are distinct:

- Undo/redo is initially local and session-scoped.
- The document operation history supports replay and sync retention.
- Compaction creates a snapshot plus coverage frontier; synchronized
  compaction additionally needs peer acknowledgement or snapshot transfer.

See [FORMAT-SPEC.md](FORMAT-SPEC.md) and the
[glossary](GLOSSARY.md).

## Geometry and performance

The numerical model is not yet decided. A candidate hybrid model — exact
predicates over float64 millimetre coordinates, with quantized evaluation
admissible only inside clipping under measured error budgets — is proposed in
[NUMERICAL-MODEL.md](NUMERICAL-MODEL.md) and remains unratified until issues
0006 and 0007 spike it and issue 0030 ratifies it. The comparison it
addresses:

- adaptive/exact predicates over floating-point coordinates; and
- integer quantization/snap rounding for polygon clipping.

Shewchuk-style predicates solve specific sign tests, not all topology
robustness. Room detection may require a planar arrangement/face traversal in
addition to polygon boolean operations.

Performance requirements will be benchmarked on named reference devices.
Targets will specify interaction, dataset, resolution, percentile frame time,
and allocation limits. Current strategies are hypotheses:

- query only visible/nearby entities using a spatial index;
- separate transient drag previews from committed operation batches;
- invalidate derived data by dependency or connected component, not always by
  individual entity and never blindly document-wide;
- reuse derived geometry and render resources by content/version keys;
- keep file loading bounded by a snapshot plus retained operations.

## Optional future services

- **Sync service:** self-hostable protocol for operation/snapshot exchange.
- **Reconstruction worker:** optional online or self-hosted GPU pipeline that
  converts photographs into a compact splat artifact. Its Python/CUDA or other
  stack requires a separate dependency and deployment review.
- **HomeHero adapter:** joins Domowik entities to HomeHero records through APIs
  without introducing HomeHero concepts into the core.
