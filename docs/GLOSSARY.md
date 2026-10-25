# Glossary

Plain-language definitions used throughout Domowik.

| Term | Meaning |
|---|---|
| Actor | A single-writer replica epoch that creates one monotonic operation-counter stream. User identity is separate; one user may have several actors/devices. |
| Asset | Non-geometric content such as a texture, image, font, furniture model, or Gaussian Splat dataset. |
| Assembly | A meaningful architectural object coordinating primitives and generating dependent geometry; a staircase is an assembly, not a furniture asset. |
| Construction body | Physical construction with a footprint and vertical extent, optionally sloping along its axis. |
| Hosted opening | An explicit cut through a named construction body or a level's floor surfaces; it does not imply traversal. |
| Level connection | Connectivity between positioned, elevation-referenced endpoints, independently of its physical assembly. |
| Asset-root table | A stable package-level list of every referenced content digest, including those used by data a reader does not understand, so old readers can preserve them without interpreting them. |
| Batch | A group of operations applied atomically: all succeed or none do. Order within a batch may be meaningful. |
| CLI | Command-line interface. It runs without a graphical window and supports scripts and automation. |
| Compaction | Creating a new snapshot and applying a retention policy to older operations. It is not automatically safe for synchronized peers. |
| Content addressing | Identifying an asset by a cryptographic digest of its bytes. Identical bytes have the same identity and can be deduplicated. |
| CRDT | Conflict-free Replicated Data Type: a data structure with formally defined merge rules that converge after concurrent edits. An operation log alone is not a CRDT. |
| Derived data | Data calculated from authoritative entities, such as wall polygons, room outlines, areas, or render meshes. It can normally be rebuilt. |
| Entity | A stable, UUID-identified document object such as a wall, level, opening, or label. |
| FFI | Foreign Function Interface: a boundary that lets one language call functions compiled from another language. |
| Frontier | Metadata describing which operations from each actor a snapshot or peer has already observed. Often represented as a version vector. |
| Gaussian Splat | A 3D representation made from many colored, oriented Gaussian primitives. It can provide photorealistic views but may require large datasets and GPU rendering. |
| Headless | Running without a graphical interface, normally through the CLI. |
| LWW | Last Writer Wins: a deterministic conflict rule where the operation with the greatest ordering key wins. It may intentionally discard a concurrent value. |
| Lease | A time-bound, advisory cache record permitting future collection of a blob after expiry. Durability-critical protection uses pins instead. |
| Magnetism | Editor behavior that snaps points or objects to nearby grid lines, angles, endpoints, or alignments. |
| Operation (op) | An identified document change, such as creating an entity or setting a field. |
| Operation log (op log) | An ordered collection of operations used to project document state. It supports replay and synchronization metadata but needs explicit merge and retention rules. |
| Pin | A durable cache record protecting a referenced blob from garbage collection until explicitly released or its document root set changes. |
| Projection | The current document state produced by applying operations to a snapshot. |
| R-tree | A spatial index used to quickly find objects near a point or inside a visible region. |
| Robust predicate | A geometry test, such as orientation, whose sign remains correct for difficult or nearly degenerate input. |
| Snapshot | A complete projected state plus metadata describing the operations it covers. |
| Thin package | A small plan package that references content-addressed assets outside the file. |
| Portable package | A self-contained plan package embedding every referenced asset, including assets otherwise considered optional. |
| Tombstone | Metadata recording a deletion so that older or concurrent edits cannot accidentally resurrect an entity. |
| Version vector | A map from actor identifiers to observed operation counters; one possible representation of a causal frontier. |
| WGSL | WebGPU Shading Language, used to write GPU shaders for WebGPU/wgpu. |
| wgpu-native | A native C API over the wgpu GPU abstraction. Embedding it in Flutter still requires a platform surface/plugin integration. |
