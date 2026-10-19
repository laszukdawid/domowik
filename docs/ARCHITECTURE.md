# Architecture

This page describes the current implementation and the boundaries that keep it
consistent across interactive, command-line and automated workflows. Proposed
features are labeled as future work; see the [roadmap](ROADMAP.md) for status.

## At a glance

```text
 Person ─────────────── Flutter editor (desktop or browser) ─────┐
                                                                  │
 Script / AI agent ──── domowik CLI ──────────────────────────────┤
                                                                  ▼
                 Shared Rust document and geometry core
          format · operations · editing · assets · persistence
                           │                 │
                           │                 └── SVG / PNG export
                           ▼
                  Renderer-neutral 3D scene
                           │
             macOS / browser / Linux presenters
```

The core is authoritative: the user interface chooses and presents actions,
while Rust validates and applies edits, derives geometry and reads or writes
the plan. The core does not depend on Flutter, a window system, a GPU or a
network service.

## Components and ownership

| Component | Responsibility |
| --- | --- |
| `domowik-format` | Document types, identifiers, schema validation and diagnostics |
| `domowik-document` | Operation batches, current-state projection and document history |
| `domowik-edit` | User-facing edits and their consequences, including wall joins and splits |
| `domowik-geometry` | Wall shapes, room topology, robust predicates, hit/snap queries and spatial indexes |
| `domowik-container` / `domowik-assets` | `.domo` persistence, asset integrity, local cache and package profiles |
| `domowik-models` / `domowik-scene` | Bounded model decoding and renderer-neutral 3D scene construction |
| `domowik-cli` | Headless commands, validation, drafting checks and SVG/PNG export |
| Flutter editor | Interactive 2D editing, application workflows and platform presentation |
| `frontend/renderer` and platform presenters | 3D rendering and GPU/surface resource ownership |

The Rust crates live in the root Cargo workspace. The Flutter application and
its native adapter have a separate workspace under `frontend/`; this keeps
platform packaging separate from reusable core code. The workspace APIs are
used by the repository's CLI and editor. They are not a promise of a stable,
published third-party SDK.

## Plan data and edits

A `.domo` file is the user's plan and the shared interchange point between the
CLI and editor. Stable entity IDs identify walls, openings, levels and other
document objects. An edit is applied as an atomic operation batch and projected
into the current document state. Derived values such as room outlines, areas
and render geometry come from the authoritative entities; they are not a second
editable copy of the plan.

The current format is experimental. Its manifest carries a `formatVersion`,
but the current version is not a public compatibility promise. Keep backups
before opening important plans in a newer development build; see
[Compatibility](COMPATIBILITY.md) and the [format specification](FORMAT-SPEC.md).

The operation history supports replay and local undo semantics, but it does not
by itself provide synchronization or collaboration. There is no current sync
service or CRDT contract. A future synchronization design needs explicit
identity, ordering, conflict, deletion and retention rules.

Assets can be embedded in a plan or managed through the local content-cache
workflow. Placing an imported model can embed its bytes so the plan remains
self-contained. The editor does not configure external or remote asset
retrieval. Imported content keeps its original rights; see the
[third-party policy](THIRD-PARTY-POLICY.md).

## Performance and numerical design

Performance is a design constraint, not a later cleanup task. The implementation
uses several complementary techniques:

- **Optimized Rust release builds.** The release profile enables full
  link-time optimization and a single code-generation unit so hot calls can be
  inlined across crate boundaries.
- **Incremental geometry.** Per-level derived geometry retains intermediate
  state and updates affected dependency stages after an edit, instead of
  rebuilding geometry that the edit cannot affect.
- **Spatial indexes and caches.** Hit/snap queries use maintained indexes;
  decoded models and derived scene data are reused where their content or
  revision remains valid.
- **Filtered exact predicates.** Geometry first uses a fast floating-point
  calculation with a proven error bound. Ambiguous or unsafe cases escalate
  to exact arithmetic, preserving correctness for difficult inputs without
  always paying the exact-path cost.
- **Measured workloads.** Core and editor harnesses exercise representative
  plans, allocations and interaction work. See
  [benchmarks](BENCHMARK-INTERACTION.md).

The predicate and incremental-geometry guarantees are specified in the
[numerical model](NUMERICAL-MODEL.md). Benchmark timings depend on hardware and
workload. Core measurements do not establish end-to-end frame time or input
latency; reference-device application budgets remain part of the roadmap.

## Human and automated interfaces

People can use the Flutter editor for drawing, selection, properties, undo and
visual review. The `domowik` CLI exposes the same plan behavior for scripts and
AI agents. Commands accept points and stable short IDs for many wall/opening
references, avoiding fragile positional numbers after edits. `info --json` and
`check --json` provide structured state and diagnostics; `check` reports
possible drafting mistakes, and `export-png --review` creates a labeled image
for inspection.

This design lets an agent plan through explicit commands and inspect the
resulting document without editing the package bytes or duplicating geometry
rules. The [CLI walkthrough](../README.md#try-it) and
[agent guide](AGENT-GUIDE.md) show the intended workflow. Always verify the
measurements and intent with the plan owner; a geometrically plausible result
is not evidence that it matches a real home.

## Editor and rendering boundaries

The editor calls Rust through a shared `DomowikCore` interface. Desktop builds
use the native C ABI; the browser build compiles the core to WebAssembly and
uses the same interface. Flutter owns interaction and 2D presentation. Rust
owns editing semantics and returns derived data; the UI must not implement a
second wall-joining or room-topology engine.

The 3D pipeline is separate from the document core:

1. Rust decodes supported models and derives a renderer-neutral scene from the
   plan.
2. A presenter uploads scene data and owns GPU buffers and presentation
   surfaces.
3. Camera movement updates the view without changing plan geometry.

The shared `wgpu` renderer is used on macOS through Metal and in browsers
through WebGPU. Linux currently uses a separate native OpenGL presenter; it is
not the shared `wgpu` backend. Android 3D is unvalidated, and native Windows
and iOS editor workflows are not established. The 3D view is experimental,
not a photorealistic renderer. See
[Compatibility](COMPATIBILITY.md) for platform limits.

The CLI's SVG/PNG exporters and the editor's 2D canvas are separate rendering
paths; a shared semantic paint API is not currently provided. The editor and
CLI do share document and geometry behavior.

## Boundaries and future work

- **Core:** document semantics, geometry, local editing, persistence and
  validation. It remains independent of UI frameworks and network services.
- **Editor:** interaction, dialogs, preferences and presentation. It does not
  own authoritative plan geometry or file-format rules.
- **3D presenters:** platform surface and GPU lifecycle. They do not own the
  plan or determine room geometry.
- **Future synchronization:** requires a separately specified protocol and
  conflict model; the operation log alone is insufficient.
- **Future reconstruction:** an optional capture/reconstruction service would
  need its own privacy, deployment and dependency decisions.

The [behavior specification](BEHAVIOR-SPEC.md) defines product semantics. The
[format specification](FORMAT-SPEC.md) defines the document contract, and
[accepted ADRs](decisions/README.md) record binding architectural choices.
