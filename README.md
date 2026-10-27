# Domowik

**A local-first home layout editor: plan in 2D, explore in 3D.**

Domowik (from the Slavic household spirit and guardian of the home) is a
project for creating floor plans, placing openings and furnishings, and
exploring the result in 3D. A Rust core powers both a headless command-line
interface (CLI, mostly for AI agents) and Flutter frontend allows for webapp and desktop GUI.

- **Performance first.** An optimized Rust core, incremental updates and cached
  rendering keep work focused on what changed. [Benchmarks](docs/BENCHMARK-INTERACTION.md) guide
  development.
- **For people and AI agents.** Use the visual editor or automate through rich
  Rust APIs, a native C ABI and a scriptable CLI with machine-readable output.
  Both workflows use the same document and geometry core.
- **Linux, web and macOS.** Run the desktop editor on Linux or macOS, or work
  in a browser. Native Windows support remains unverified.
- **A work in progress, with much more to come.** Explore what works today,
  follow the [roadmap](docs/ROADMAP.md), and help shape what comes next.

![Domowik showing the furnished Welcome Home example in 2D and 3D](docs/images/welcome-home-editor.png)

## Make yourself at home

Start with [Welcome Home](examples/welcome-home/README.md), a small furnished
house you can open, explore and change. [Download the example plan](examples/welcome-home/welcome-home.domo?raw=true),
launch the [desktop or browser editor](docs/GETTING-STARTED.md), then choose
**Open** and **2D + 3D**. All mock furniture is included in the file.
Try moving the sofa, changing a wall, or adding a window, then undo your edits.
The example and its original models are Apache-2.0, with no asset downloads.

For a multi-storey plan, try the [Four-level house](examples/four-level-house/README.md):
ground-floor living spaces, three upstairs bedrooms, a bathroom on each living
floor, a small crawlspace and an attic.

## Status

**Experimental development software; no public release is claimed.** Plans
are self-contained `.domo` files. You can create one, draw and edit walls with
real join and split behaviour, add doors and windows, place a scan to trace
over, get true floor areas, undo, apply external operation batches, compact the
history, and export to SVG or PNG — all from the command line. The CLI is
built to be driven by an AI agent as well as a person: walls and openings
can be named by a point or a short ID, `check` flags likely drafting
mistakes, and `export-png --review` labels walls and dimensions. An agent
drafting a plan from photos follows [docs/AGENT-GUIDE.md](docs/AGENT-GUIDE.md).

The editor supports wall/opening properties, levels, room naming, selection,
undo/redo, native open/save, imported furniture with textured GLB previews, resizing and name/path search,
and device preferences. It has an experimental 3D view with orbit/walk
navigation and a 2D/3D split, left/right by default: on macOS through a
shared Rust `wgpu` renderer (Metal), on Linux through a native GL texture, and
in browsers through the same `wgpu` renderer on WebGPU.

Linux and macOS desktop editor work have evidence. Android has earlier bridge/application execution evidence, not a
claim of current full-editor support. iOS is not a supported editor target.
Native Windows builds remain unverified and are not currently provided.
The editor also runs in a web browser
([ADR 0012](docs/decisions/0012-browser-editor.md)): the same Rust core as
WebAssembly, plans opened from and saved to your files, and 3D on WebGPU;
`task run:gui:web` builds and serves it. Remaining platform-portability and
measured whole-app performance gates are open under
[ADR 0003](docs/decisions/0003-flutter-gui-wgpu-native.md) and
[ADR 0010](docs/decisions/0010-native-3d-texture-experiment.md).
There is no comparative performance claim against other applications.

Use copies of important plans: the file format, bridge and editor are still
experimental. See [compatibility and limitations](docs/COMPATIBILITY.md).

## Run locally

Start from a checkout of this repository. Rust is enough to build the CLI:

```sh
# Run from the repository root. rustup reads rust-toolchain.toml.
cargo build --release --locked -p domowik-cli
./target/release/domowik --help
```

For the desktop editor, install Flutter and the platform's native build tools;
follow [Getting started](docs/GETTING-STARTED.md) for prerequisites. On Linux:

```sh
cd frontend
flutter pub get
flutter run -d linux
```

On macOS, use `flutter run -d macos` instead. The native build compiles the
Rust bridge automatically. Choose **Walls** and drag on the empty canvas to
start; see the [user guide](docs/USER-GUIDE.md) for the rest.

For tests, verification and contribution rules, read
[CONTRIBUTING.md](CONTRIBUTING.md). A local source build is not an authorized
binary release.

Unsigned developer builds (CLI for Linux and macOS, editor for Linux and
macOS) come from the `binaries` GitHub workflow: push a `v*` tag, which also
publishes a prerelease, or start it by hand from the Actions tab. Nothing
runs product CI on ordinary pushes. GitHub Pages separately
publishes `docs/` from `main`.

## Try it

Requires a Rust toolchain; the pinned version installs automatically from
`rust-toolchain.toml`.

```sh
cargo build --release -p domowik-cli
alias domowik=./target/release/domowik

# A 6 m x 4 m flat with a dividing wall. Millimetres throughout.
domowik new flat.domo
domowik add-wall flat.domo 0    0    6000 0    --thickness 200
domowik add-wall flat.domo 6000 0    6000 4000 --thickness 200
domowik add-wall flat.domo 6000 4000 0    4000 --thickness 200
domowik add-wall flat.domo 0    4000 0    0    --thickness 200

# Dropping this across the room SPLITS the walls it crosses and joins the
# children — one atomic batch, so one undo takes it all back.
domowik add-wall flat.domo 3500 0    3500 4000 --thickness 100

# Openings are placed by a POINT on the wall, centred there. Wall numbers
# shift when walls split, points do not.
domowik add-opening flat.domo 1050,0    door   --width 900
domowik add-opening flat.domo 4750,4000 window --width 1500 --height 1200 --sill 900

# Name the two rooms the dividing wall made. A room record is a NAME and a
# SEED POINT; the room itself stays derived from the walls, so moving a wall
# renames nothing and splitting a room needs no bookkeeping.
domowik room add flat.domo 1500 2000 --name "Salon"
domowik room add flat.domo 4500 2000 --name "Kuchnia"
domowik room list flat.domo      # names, status, area, seed

domowik info flat.domo          # rooms, names, floor areas, extent
domowik validate flat.domo
domowik check flat.domo         # likely drafting mistakes, with suggestions
domowik ops flat.domo           # the operation log, in canonical order
domowik ops flat.domo --json    # the same, for a script
domowik undo flat.domo          # appends a compensating batch

domowik export-svg flat.domo flat.svg
domowik export-usdz flat.domo flat.usdz            # every level as a 3D USD scene
domowik export-png flat.domo flat.png --scale 0.3
domowik export-png flat.domo review.png --review   # labels, lengths, scale bar
```

An AI agent drafting a plan from photos should follow
[docs/AGENT-GUIDE.md](docs/AGENT-GUIDE.md).

Floor areas are measured to the **inner faces** of the walls, not their centre
lines, so a 6 m x 4 m shell of 200 mm walls reports the 22.04 m² you can
furnish rather than the 24 m² its centre lines enclose.

A room record does not own its room. It holds a name and a point, and the room
is whatever the walls enclose around that point — so a record whose walls are
knocked through reports `seed-in-exterior` and waits, rather than being deleted
or silently reattached to a room nobody meant. `room list` shows which records
currently name a room and which do not.

Room names render in both exports for Latin, Greek and Cyrillic scripts. A name
that needs text shaping — CJK, Arabic, Hebrew, or a name written with combining
accents — is left out of the drawing with a warning naming the room and the
character, rather than drawn wrongly; the stored name is untouched.

Trace over a survey, and pack a copy with everything embedded:

```sh
domowik asset add flat.domo scan.png --scale 8   # millimetres per pixel
domowik export-png flat.domo traced.png          # the scan renders underneath
domowik pack flat.domo share.domo --portable     # every asset embedded, verified
domowik compact flat.domo --dry-run              # what folding would discard
domowik compact flat.domo --assert-local-only    # fold the log into the snapshot
```

`info` reports levels, wall counts, total wall length and the plan's extent.
`validate` lists every problem it finds, not just the first.

`compact` is irreversible, so it asks before it acts. `--dry-run` prints what
would be discarded and what would be kept; the assertion is required because
nothing in a plan file records whether a copy was ever given to someone else,
and folding the log is what would leave them stranded. It also names the writer epochs whose
operations it folded, which `ops` lists.

Exit codes: **0** success, **1** the command failed, **2** you used it wrong.
Results go to stdout and diagnostics to stderr, so piping is safe.

## Priorities

- **Useful standalone.** Domowik is an independent Apache-2.0 project with a
  service-independent core.
- **Core before GUI.** Document and geometry behavior belongs in a reusable
  core. The CLI proves that the core does not depend on a user interface.
- **Headless document workflows.** Creating, editing, validating, inspecting,
  and exporting 2D plans should work without a GUI. Interactive 3D and mobile
  capture are not promised as headless features.
- **Small files and assets.** Plans store procedural geometry, deduplicate
  assets by content, and support self-contained portable packages. Thin
  external-cache packages are specified but not implemented. See the
  [asset strategy](docs/ASSET-STRATEGY.md).
- **Offline-first editing.** Local files are authoritative. Network services
  such as sync and photogrammetry are optional.
- **Collaboration-aware design.** Stable entity and operation identities will
  make later synchronization easier, but real-time collaboration and CRDT
  behavior are not claimed yet.
- **Clean implementation.** The project follows a documented black-box
  behavior-study policy and audits third-party licenses.

## Architecture

```text
                    ┌──────────────────────────┐
                    │ domowik-cli              │
                    │ 26 commands · SVG · PNG  │
                    └────────────┬─────────────┘
                                 │
        ┌────────────────┬───────┴────────┬─────────────────┐
        ▼                ▼                ▼                 │
┌───────────────┐ ┌─────────────┐ ┌───────────────┐         │
│ domowik-      │ │ domowik-    │ │ domowik-      │         │
│ container     │ │ document    │ │ geometry      │         │
│ ZIP package,  │ │ operation   │ │ predicates,   │         │
│ save machine, │ │ log, replay,│ │ union, rooms, │         │
│ blob store    │ │ undo/redo   │ │ spatial index │         │
└───────┬───────┘ └──────┬──────┘ └───────┬───────┘         │
        └────────────────┴────────────────┴─────────────────┘
                                 ▼
                    ┌──────────────────────────┐
                    │ domowik-format           │
                    │ entities, validation     │
                    └──────────────────────────┘

   Flutter editor → native bridge → domowik-edit + core
   3D: domowik-scene + domowik-models → shared wgpu renderer
       (frontend/renderer; macOS Metal and browser WebGPU) or Linux GL
   Mobile rendering and hardware/performance gates remain open.
```

The Rust workspace has no dependency cycles. `format` holds the entity types everything else
speaks in; `document` owns the operation log that every change flows through;
`container` owns the file; `geometry` owns the numerics and the plan topology;
`edit` owns shared editing gestures, consumed by both the CLI and the Flutter
frontend's native bridge.

The GUI borrows native document handles; Rust owns editing semantics and
derived scene geometry. The platform runner owns GPU resources. See
[Architecture](docs/ARCHITECTURE.md) and the
[decision records](docs/decisions/README.md) for accepted boundaries and open
technology gates.

## Documentation

Read the [documentation website](https://laszukdawid.github.io/domowik/) or
browse the source guides below.

| Document | Purpose |
|---|---|
| [Documentation index](docs/README.md) | Choose a user, contributor or design path |
| [Getting started](docs/GETTING-STARTED.md) | Build/run the CLI and desktop editor; troubleshoot setup |
| [User guide](docs/USER-GUIDE.md) | Current editor workflows and platform-specific shortcuts |
| [Contributing](CONTRIBUTING.md) | Source map, issue workflow and verification |
| [Agent guide](docs/AGENT-GUIDE.md) | How an AI agent drafts a plan from photos with the CLI |
| [Compatibility](docs/COMPATIBILITY.md) | Experimental formats, platform limits and safe file handling |
| [Naming registry](docs/NAMING.md) | Canonical identity values and the procedure for changing them |
| [Glossary](docs/GLOSSARY.md) | Plain-language definitions of project terms |
| [Roadmap](docs/ROADMAP.md) | Phases and validation gates |
| [Architecture](docs/ARCHITECTURE.md) | Components, boundaries, and provisional choices |
| [Format requirements](docs/FORMAT-SPEC.md) | Logical model and compatibility requirements |
| [Asset strategy](docs/ASSET-STRATEGY.md) | Size budgets, deduplication, and packaging profiles |
| [Behavior specification](docs/BEHAVIOR-SPEC.md) | Normative behavior rules and separately marked draft questions |
| [Clean-room policy](docs/CLEAN-ROOM-POLICY.md) | Black-box study and provenance rules |
| [Third-party policy](docs/THIRD-PARTY-POLICY.md) | Code, tools, services, and user asset licensing |
| [Decision records](docs/decisions/README.md) | Accepted and proposed architecture decisions |

