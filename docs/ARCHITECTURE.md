# Architecture

Status: **Proposed architecture.** Accepted project goals are separated below
from technology choices that still require feasibility spikes.

Current implementation (2026-10-04): the core and CLI are Rust (accepted ADR
0008). The experimental Flutter desktop editor uses that core through issue
0024's native bridge, with Linux and macOS evidence bounded by the increment
and ABI tested. Imported models are decoded by `domowik-models`; the
renderer-neutral `domowik-scene` derives 3D data from the document. Issue 0078
adds an experimental Linux native-OpenGL texture backend (ADR 0010), not a
portable renderer or acceptance of ADR 0003. Whole-app performance and
remaining platform/rendering gates remain open. See
[Getting started](GETTING-STARTED.md) and [compatibility](COMPATIBILITY.md).

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

The diagram's wgpu path is a proposed portable backend, not the current Linux
implementation. Today `domowik-scene` supplies renderer-neutral scene data to
the experimental native GL texture runner. Document/model code owns no GPU;
the runner owns texture/buffer lifetime.

The core does not call wgpu. The GUI owns rendering surfaces and submits
render-ready scene data derived from the document.

## Core responsibilities

These define ownership boundaries, not a completion checklist. In particular,
the shared semantic 2D paint API and external asset cache remain planned; the
current exporters and embedded-asset workflow do not establish those APIs.

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

These are architectural workflow names, not literal syntax for every current
command. The implementation uses `export-svg` and `export-png`; `pack --thin`
is refused because external-cache packaging is not implemented. Use the built
CLI's `--help` for current syntax. Headless 3D rendering may be added later,
but is not required by the initial architecture.

## GUI and native boundaries

### Rust-to-Flutter bridge (desktop prototype)

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
application loaded and drove the core from inside its own bundle on both, and
on Linux from a machine carrying no toolchain. Android was an emulator and one
ABI; that run does not validate the current ABI 15/editor. Later Linux editor
increments have local native-backed tests and release-build evidence. iOS is
deferred; macOS remains a desktop development/bundle target, not a current 3D
backend. ADR 0003 remains Proposed while measured editor-performance and
remaining validation obligations are open. Do not extrapolate an old platform
slice to every current feature.

### Flutter application services (issue 0085)

The experimental editor separates native file/session ownership from widgets:
`PlanSession` owns one native plan handle and file-operation allocations;
`FileWorkflowController` serializes file/close/property actions and unsaved
approval through borrowed callbacks; `FileDialogs` isolates native pickers and
close requests. The widget owns dialog presentation and UI reset decisions,
not native handle replacement. File operations remain synchronous at the C ABI
boundary; extracting services does not claim asynchronous file I/O.

`PlanGeometryCache` owns per-session/per-level content-key reuse, visibility
eviction and failed-read retry. The widget still assembles visible render
layers and owns preview images. `ThreeDView` still combines scene-upload and
surface scheduling with input/presentation; extracting a renderer controller
and preview-image owner remains future work. 2D paint and 3D GPU interfaces
remain separate; shared document geometry stays authoritative in Rust.

### 3D surface bridge (Linux experiment; portable gate open)

Issue 0078 now exercises the Linux embedder's GPU texture API with renderer-
neutral Rust scene data, instanced geometry and local orbit/walk cameras. This
is the bounded native-GL experiment in ADR 0010, not a completed wgpu/platform
gate. The renderer owns buffers and depth/color targets; view changes update
only the camera matrix. Model decode, LOD and scene construction are cached/off
the UI thread. The current feature is validated on Linux; other backend and
hardware-frame gates remain with issue 0025.

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

The numerical model is partly ratified: the coordinate domain and numeric
policy are in force, while conformance obligations remain open. Read
[NUMERICAL-MODEL.md](NUMERICAL-MODEL.md) §§3 and 12–14 for current policy;
its superseded Go-era machinery is historical, not an implementation mandate.
Issues 0012/0030 and related evidence retain the unmet guarantees. The original
comparison addressed:

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
