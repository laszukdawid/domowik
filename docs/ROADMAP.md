# Roadmap

Domowik is actively developing live 3D while continuing to close correctness,
platform and whole-application performance gates for the 2D editor. Work in a
later phase does not imply that earlier exit criteria are complete.

## Current product

- **Architectural foundations:** rectangular/sloped construction bodies, hosted
  body/floor cuts and explicit level connections. Straight stair assemblies share
  one identity across both levels, with direct **Stairs** click/drag drawing,
  Floor-panel name/count/elevation editing, CLI creation/conversion, undo,
  2D/export symbols and derived 3D treads/guards. Other assembly
  tools and general-profile construction remain future work; see
  [Architectural primitives](ARCHITECTURAL-PRIMITIVES.md).

- **Core and CLI:** a Rust document engine and headless `domowik` CLI create,
  edit, validate, inspect, pack and export plans. The CLI is usable by people,
  scripts and AI agents.
- **2D editor:** the Flutter editor runs on Linux and macOS desktop and in a
  browser. It supports walls, openings, levels, room labels, selection,
  furniture with R-activated 2D rotation, Shift-held 15° snapping and click-away
  acceptance, bulk wall height/thickness editing, undo/redo and plan file workflows.
  Plans open on the first listed floor by default and remember the device's last
  active-floor choice, including for older files without document identity.
  The asset library offers list/grid browsing with on-demand model thumbnails.
- **3D view:** experimental views derive scene data from the same plan and
  share its selection: selected items are tinted, clicking in 3D selects, and
  navigation zooms toward the cursor, orbits about the picked surface and
  frames the selection. Dragging, rotating or numerically placing furniture,
  including furniture-only group moves, previews live in 3D before release;
  wall and opening edits update 3D after they are committed. Linux
  uses a native OpenGL presenter; macOS and the browser use the shared `wgpu`
  renderer via Metal and WebGPU respectively.
- **Performance:** the Rust core uses incremental geometry, spatial indexes,
  caches and filtered exact predicates. Benchmarks cover core workloads, and
  `task benchmark` measures whole-editor frame time, interaction latency and
  memory on a desktop host; the budgets those measurements are judged against
  are still open.

See [Architecture](ARCHITECTURE.md) for how these pieces fit together,
[Getting started](GETTING-STARTED.md) to run them, and
[Compatibility](COMPATIBILITY.md) for platform and format limits.

## Phase status

| Phase | State | Remaining outcome |
| --- | --- | --- |
| 0 — behavior and risk specification | Exited for the current implementation, with named numerical and format obligations still open | Keep implementation aligned with normative specifications and close the remaining conformance gaps. |
| 1 — core and headless CLI | Delivered for its defined scope | Release, migration and additional format obligations remain separate work. |
| 2 — interactive 2D editor | In use; formal exit remains open | Finish relevant behavior/platform validation, measure end-to-end interaction budgets on reference devices, and establish GUI/CLI semantic equivalence. |
| **3 — live 3D plan view** | **Active** | Complete portable presenter and lifecycle validation; measure whole-application performance on named devices. |
| 4 — optional synchronization | Future | Define explicit operation identity, snapshots, conflict handling and retention before offering synchronization. |
| 5 — Gaussian Splat viewing | Future | Choose a licensed, compact representation and demonstrate useful progressive viewing and plan alignment. |
| 6 — capture and reconstruction | Future | Design guided capture and an optional reconstruction workflow with privacy and deployment safeguards. |

Remaining work includes format/container conformance and explicit development
schema conversions; GUI bundle licensing and release coverage; portable renderer
lifecycle validation; whole-editor performance measurements; and reducing
cross-bridge synchronization work after small edits. These are separate
obligations, not implied complete by the current feature list.

## Design priorities

1. **One authoritative model.** CLI, editor and future tools use the same
   Rust document and geometry rules.
2. **Performance in the design.** Keep work local to the edit and data it can
   affect; measure representative workloads instead of relying on visual
   impressions or microbenchmarks alone.
3. **Usable by people and agents.** Keep interactive workflows direct, and
   expose explicit CLI commands, stable references, structured output and
   reviewable results for automation.
4. **Correctness before compatibility promises.** The format and API remain
   experimental until their publication and compatibility gates are met.
5. **Separate optional services.** Synchronization and reconstruction do not
   become dependencies of the local-first core.

## What is not promised

The roadmap is not a release schedule. It does not promise support for every
desktop/mobile platform, stable format compatibility, continuous collaboration,
or whole-editor performance targets that have not been measured. Keep backups
of important plans and consult [Compatibility and limitations](COMPATIBILITY.md)
before relying on a particular feature or platform.
