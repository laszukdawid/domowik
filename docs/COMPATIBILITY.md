# Compatibility and limitations

Status: **Current development guidance, not a release announcement.** No public
Domowik binary or format release is claimed. `0.1.0` package metadata does not
establish a stable API or file-format promise.

## Platform evidence

| Component | Current boundary |
| --- | --- |
| CLI / Rust core | Linux and macOS development/CI work; Linux cross-build jobs are explicitly compile-only, not runtime proof for every target |
| 2D desktop editor | Linux execution/local checks and macOS prototype/bundle evidence; individual increments do not establish current full validation on every machine |
| 3D editor | Experimental Linux native OpenGL texture backend; not a macOS/mobile/Windows renderer |
| Android | Earlier bundled core/ABI execution on an emulator and one ABI; not current full-editor or 3D support |
| iOS, Windows editor | No supported editor workflow established |
| Web editor | Required future target, deferred in [0088](../issues/0088-web-editor-support.md); no browser backend yet |

Platform evidence is bounded by the tested ABI, host and increment. Read
[Architecture](ARCHITECTURE.md), the
[bridge evidence](../frontend/native/README.md),
[ADR 0003](decisions/0003-flutter-gui-wgpu-native.md) and
[ADR 0010](decisions/0010-native-3d-texture-experiment.md) for open gates.
Successful widget tests or CPU microbenchmarks do not prove physical-device
input latency, GPU presentation performance or all-platform functionality.

## Plans and history

The current CLI and editor share the Rust document/container implementation
and read/write `.domo` packages. The format specification describes an
experimental schema series, not a permanent freeze. There is no publicly
released predecessor set from which a migration guarantee can be inferred.
The stronger compatibility promise in
[ADR 0007](decisions/0007-format-compatibility-promise.md) remains Proposed.

Keep untouched backups before opening important plans in a newer development
build. Do not assume an older build can read a newly saved file or interpret
every optional extension. A reader's preservation of unknown data is not proof
it displays that data correctly.

Older unpublished package layouts have a bounded repair command:

```sh
./target/release/domowik convert-developer-package --help
```

It reads a source and writes a separate destination, leaving the source
untouched. It is not a general migration promise or a universal repair tool.

Local interactive undo/redo, persisted operations and future synchronization
history are different concepts. Reopening a saved plan does not restore an
identical GUI undo stack. An operation log is not a CRDT; there is no current
sync service or real-time collaboration guarantee.

Container recovery tests exercise specific filesystem states. They do not
establish universal power-loss safety, external-modification handling or an
autosave/recovery UI. See [Format specification](FORMAT-SPEC.md) and issue
[0029](../issues/0029-container-persistence-recovery.md) for the exact gaps.

## Assets and rendering

Plan-attached library paths refer to local directories. Placed model bytes
are embedded and can survive source removal; unused directory contents are
not bundled merely by adding a library. `pack --portable` embeds referenced
assets and reports omissions; `pack --thin` is currently refused, not a
working external-cache profile.

Supported model paths are GLB, OBJ and ZIP-OBJ, with bounded decode and static
scene handling. Support for a container format is not a claim to implement
all of its animation, material, texture or scene features. Unitless models use
an estimated proportional scale. 2D previews and 3D LOD are approximations;
missing/unsupported content can produce diagnosed placeholders.

Editor footprint and height metadata are optional extensions. Older readers
may preserve an independently resized height but render it incorrectly; legacy
instances without explicit height retain width-derived behavior until edited.
See [sizing evidence](evidence/0082-model-size-and-resize.md).

CLI room-label exports currently support the documented Latin/Greek/Cyrillic
repertoire, not general shaping or bidi. Unsupported labels are omitted from
the drawing with a warning; the stored name remains unchanged.

The editor still lacks general group commands, a complete snapping/conformance
gate, display-unit preferences and several standard file-workflow conveniences.
See the [user guide](USER-GUIDE.md) and [tracker](../issues/README.md).

## Source builds versus distributed releases

A successful build is not permission to publish an artifact. Rust dependency
inventory, notices and CLI release validation exist; GUI bundle license
coverage is staged and incomplete. Imported assets retain their own licenses.
Do not distribute private/unreviewed catalogs with the application or example
plans. See [Release inventory](RELEASE-INVENTORY.md) and
[Third-party policy](THIRD-PARTY-POLICY.md).

Application IDs, product/domain identity and format publication decisions have
their own recorded gates. This guide does not rename identifiers, reserve
namespaces, change repository visibility or ratify proposed decisions.
