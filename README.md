# Domowik

**An open-source home layout editor: plan in 2D, explore in 3D.**

Domowik (from the Slavic household spirit and guardian of the home) is a
planned cross-platform application for creating floor plans, placing
openings and furnishings, and exploring the result in 3D. It will provide a
graphical editor and a headless command-line interface (CLI).

## Status

**Phase 1 is complete: a working headless floor-plan engine and CLI.** Plans
are self-contained `.domo` files. You can create one, draw and edit walls with
real join and split behaviour, add doors and windows, place a scan to trace
over, get true floor areas, undo, apply external operation batches, compact the
history, and export to SVG or PNG — all from the command line.

**Phase 2 has a working Flutter 2D editor prototype on macOS.** Draw and move
walls, place doors and windows, inspect floor areas, switch levels, undo, and
open/save the same `.domo` files used by the CLI. The workspace puts drawing
tools above the canvas and plan details in a sidebar (a drawer in small
windows). See [the editor instructions](spikes/0024-flutter-rust-bridge/flutter_app/README.md).

The editor remains experimental: Linux/Android bridge validation and measured
input/frame-time goals are still open under
[ADR 0003](docs/decisions/0003-flutter-gui-wgpu-native.md). There is no 3D yet,
and no comparative performance claim against other home-design applications.

The Go implementation that preceded this is frozen for reference at
`_reference/go/` (see
[ADR 0008](docs/decisions/0008-rust-core-implementation.md)).

## Run locally

The command-line application works on the Rust workspace's supported hosts.
The graphical editor currently runs on macOS only. It is useful for exercising
the same document and editing core as the CLI, but it is still a prototype: it
does not provide 3D, and its Linux, Android, input-latency, and frame-time
validation is unfinished.

Start with a normal clone and the toolchains required by the part you want to
run:

```sh
git clone git@github.com:laszukdawid/domowik.git
cd domowik

# The repository selects Rust 1.98.0 through rust-toolchain.toml.
cargo --version
```

For the CLI, Rust is enough:

```sh
cargo build --release -p domowik-cli
./target/release/domowik --help
```

For the macOS editor, install Flutter with Dart 3.11.4 or later, Xcode, and
CocoaPods. `flutter doctor` should report that the macOS desktop toolchain is
ready. Xcode's build phase looks for `cargo` and `rustup` in `~/.cargo/bin`, so
use the standard rustup installation or make those commands available there.
Then run:

```sh
cd spikes/0024-flutter-rust-bridge/flutter_app
flutter pub get
flutter run -d macos
```

The first GUI build compiles the Rust bridge as part of the CocoaPods/Xcode
build. It can take longer than later builds. The editor opens an untitled plan;
choose **Walls** and drag on empty canvas to draw. It reads and writes the same
`.domo` files as `domowik`, so a file created by the CLI can be opened with
**Open plan** in the editor. The full current interaction guide, bundle probes,
and known limits are in the [editor README](spikes/0024-flutter-rust-bridge/flutter_app/README.md).

To build the prototype without opening a window and exercise its bundle:

```sh
cd spikes/0024-flutter-rust-bridge/flutter_app
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_spike_app.app/Contents/MacOS/domowik_spike_app
DOMOWIK_SPIKE_SELFTEST=1 "$APP"
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"
```

The repository-wide check also needs Go and Python 3 for the remaining hygiene
tools, plus Flutter, Xcode, CocoaPods, and the pinned Rust toolchain for the
macOS bundle. Run it from the repository root:

```sh
PATH="/path/to/flutter/bin:$PATH" ./scripts/verify.sh
```

The authoritative success marker is `=== VERIFICATION PASSED === all stages`.

## Try it

Requires a Rust toolchain; the pinned version installs automatically from
`rust-toolchain.toml`.

```sh
cargo build --release -p domowik-cli
alias domowik=./target/release/domowik

# A 6 m x 4 m flat with a dividing wall. Millimetres throughout.
domowik new flat.domo
domowik add-wall flat.domo 0    0    6000 0
domowik add-wall flat.domo 6000 0    6000 4000
domowik add-wall flat.domo 6000 4000 0    4000
domowik add-wall flat.domo 0    4000 0    0

# Dropping this across the room SPLITS the walls it crosses and joins the
# children — one atomic batch, so one undo takes it all back.
domowik add-wall flat.domo 3500 0    3500 4000 --thickness 100

domowik add-opening flat.domo 1 door   --at 0.3 --width 900
domowik add-opening flat.domo 3 window --at 0.5 --width 1500 --sill 900

# Name the two rooms the dividing wall made. A room record is a NAME and a
# SEED POINT; the room itself stays derived from the walls, so moving a wall
# renames nothing and splitting a room needs no bookkeeping.
domowik room add flat.domo 1500 2000 --name "Salon"
domowik room add flat.domo 4500 2000 --name "Kuchnia"
domowik room list flat.domo      # names, status, area, seed

domowik info flat.domo          # rooms, names, floor areas, extent
domowik validate flat.domo
domowik ops flat.domo           # the operation log, in canonical order
domowik ops flat.domo --json    # the same, for a script
domowik undo flat.domo          # appends a compensating batch

domowik export-svg flat.domo flat.svg
domowik export-png flat.domo flat.png --scale 0.3
```

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

- **Useful standalone.** Domowik is an independent Apache-2.0 project.
  HomeHero integration is a possible future adapter, not a current dependency.
- **Core before GUI.** Document and geometry behavior belongs in a reusable
  core. The CLI proves that the core does not depend on a user interface.
- **Headless document workflows.** Creating, editing, validating, inspecting,
  and exporting 2D plans should work without a GUI. Interactive 3D and mobile
  capture are not promised as headless features.
- **Small files and assets.** Plans store procedural geometry, deduplicate
  assets by content, and support thin and self-contained packages. See the
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
                    │ 28 commands · SVG · PNG  │
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

   Flutter editor prototype → native bridge → domowik-edit + core
   NOT BUILT: a 3D renderer. See the roadmap for platform gates.
```

The Rust workspace has no dependency cycles. `format` holds the entity types everything else
speaks in; `document` owns the operation log that every change flows through;
`container` owns the file; `geometry` owns the numerics and the plan topology;
`edit` owns shared editing gestures, consumed by both the CLI and the Flutter
prototype's native bridge.

The GUI's Rust bridge has macOS evidence; Linux/Android and the 3D surface
still need platform spikes before they are accepted. See [Architecture](docs/ARCHITECTURE.md) and
the [decision records](docs/decisions/README.md).

## Documentation

| Document | Purpose |
|---|---|
| [Naming registry](docs/NAMING.md) | Canonical identity values and the procedure for changing them |
| [Glossary](docs/GLOSSARY.md) | Plain-language definitions of project terms |
| [Roadmap](docs/ROADMAP.md) | Phases and validation gates |
| [Architecture](docs/ARCHITECTURE.md) | Components, boundaries, and provisional choices |
| [Format requirements](docs/FORMAT-SPEC.md) | Logical model and compatibility requirements |
| [Asset strategy](docs/ASSET-STRATEGY.md) | Size budgets, deduplication, and packaging profiles |
| [Behavior specification](docs/BEHAVIOR-SPEC.md) | Testable editor behavior still to be defined |
| [Clean-room policy](docs/CLEAN-ROOM-POLICY.md) | Black-box study and provenance rules |
| [Third-party policy](docs/THIRD-PARTY-POLICY.md) | Code, tools, services, and user asset licensing |
| [HomeHero integration](docs/INTEGRATION-HOMEHERO.md) | Deferred, API-level integration goal |
| [Decision records](docs/decisions/README.md) | Accepted and proposed architecture decisions |

## Work tracking

`issues/` holds the work tracker as ordinary tracked files in this repository,
with dependencies and acceptance criteria. `./scripts/verify.sh` enforces that
each issue's status line agrees with the index. It will need its own published
location before external contributors can rely on it.

## License

Domowik source and project documentation are licensed under Apache-2.0; see
[LICENSE](LICENSE). Files created by users, imported models, photographs,
textures, and other third-party content retain their own copyrights and
licenses. Domowik does not relicense user content.
