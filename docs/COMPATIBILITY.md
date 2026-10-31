# Compatibility and limitations

Status: **Current development guidance, not a release announcement.** No public
Domowik binary or format release is claimed. `0.1.0` package metadata does not
establish a stable API or file-format promise.

## Platform evidence

| Component | Current boundary |
| --- | --- |
| CLI / Rust core | Linux and macOS development/CI work; Linux cross-build jobs are explicitly compile-only, not runtime proof for every target |
| 2D desktop editor | Linux execution/local checks and macOS prototype/bundle evidence; individual increments do not establish current full validation on every machine |
| 3D editor | Experimental: macOS through the shared `wgpu` renderer on Metal (issue 0101), browsers through the same renderer on WebGPU (issue 0088), Linux through the native OpenGL texture backend; not a mobile/Windows renderer |
| Android | Earlier bundled core/ABI execution on an emulator and one ABI; not current full-editor or 3D support |
| Windows native editor | Unverified; no native Windows editor build or packaging is currently provided. The web editor is a separate browser workflow, with its tested environments listed below |
| iOS editor | No supported editor workflow established |
| Web editor | Editor workflows and the release probe tested in Chrome for Testing 155 on macOS (WebGPU on the GPU). Linux headless browser tests and both release-bundle probes have also passed with software WebGPU (issues 0120/0121); this does not establish physical-GPU performance. The owner also tried it by hand in Firefox on 2026-10-13 and reported that it looked good (version and workflows not recorded, no automated Firefox run). Safari and Edge untested. 2D needs WebAssembly and CanvasKit; 3D needs WebGPU, with no WebGL fallback. Plans open and save through the user's files; no in-browser plan storage. CanvasKit, the core, the renderer and the text fonts (Roboto, and a Noto Sans Symbols 2 subset for ⌘) are served with the page, which contacts no other origin; characters Roboto lacks (CJK, Arabic, Hebrew, emoji, most symbols) draw as boxes. One page thread: large model imports pause the page, and a core panic ends the tab's session. See [ADR 0012](decisions/0012-browser-editor.md) |

**Firefox graphics boundary:** desktop Linux Firefox defaults to **Safe 2D**,
even when view preferences request 3D/split. WebGPU presentation can terminate
the whole browser on affected Intel/Mesa/Wayland stacks; a standalone WebGPU
canvas reproduces the failure without Flutter or the Domowik renderer. This
gate is deliberately broader than the known failing driver, since a page cannot
safely probe that driver by initializing WebGPU. The owner reports Firefox on
macOS works; that platform is not gated. Safe 2D does not initialize the 3D
module, adapter, device or canvas and does not rewrite saved view preferences.
The 2D Flutter interface still uses graphics acceleration; this is not a software
rendering mode or a guarantee against unrelated browser failures. The
[user guide](USER-GUIDE.md) describes explicit page-only safety/testing overrides.
Browsers that report missing WebGPU or adapter/device startup failures return to
Safe 2D with expandable diagnostics, without retrying until reload. This handles
ordinary reported failures, not native browser crashes, and adds no 3D backend.

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

The owner-selected [evolution direction](decisions/0011-extensible-versioned-document-format.md)
keeps `formatVersion` in the manifest and adopts a defined core with namespaced
extensions. Incompatible development schemas use distinct version labels;
there is no promise to migrate every unpublished revision. The extension wire
contract is pending. Current readers support `"0.1"` and the architectural
`"0.2"` supplement. Ordinary older plans retain `"0.1"`; adding bodies, hosted
cuts, connections or stairs selects `"0.2"`, which older builds reject before
editing. Updating a version string alone does not convert an incompatible document.

Keep untouched backups before opening important plans in a newer development
build. Do not assume an older build can read a newly saved file or interpret
every optional extension. A reader's preservation of unknown data is not proof
it displays that data correctly.

HomeHero property/room associations are hosted-service metadata keyed by plan,
room-record and location IDs. They are not stored in `.domo` packages and do
not alter document or geometry semantics. The property manager and inventory
controls are browser-only and require the configured same-origin hosted
account. Local/offline plans remain independent of HomeHero; no format
migration is involved.

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
autosave/recovery UI. See [Format specification](FORMAT-SPEC.md) for the contract
and its remaining implementation/conformance boundaries.

## Assets and rendering

Plan-attached library paths refer to local directories. Placed model bytes
are embedded and can survive source removal; unused directory contents are
not bundled merely by adding a library. `pack --portable` embeds referenced
assets and reports omissions; `pack --thin` is currently refused, not a
working external-cache profile.

Embedded GLB base-colour textures support PNG/JPEG/WebP, float or normalized
integer UVs (`TEXCOORD_0/1`, including Draco), `KHR_texture_transform`, and
repeat/clamp/mirrored wrapping. Filtering is bilinear without mipmaps; this is
diffuse textured rendering, not full glTF PBR or material-alpha fidelity.
External files/data URIs, OBJ texture maps, normal and roughness maps remain
unsupported. Referenced malformed/unsupported images produce diagnostics.
Source images are limited to 4096 pixels per edge, reduced to at most 2048 for
display, at most 16 base-colour images and 64 MiB of decoded display pixels per
model; the bridge's 128 MiB cache also counts textures and UVs. Limits affect
derived rendering only. Original embedded bytes and the plan format do not
change. Internal browser frames use DSW2 when textured (DSW1 otherwise); update
core and renderer modules together. Old scene readers reject DSW2 explicitly.

Supported model paths are GLB, OBJ, ZIP-OBJ and self-contained OpenUSD
(`.usda`, `.usdc`, `.usdz`), with static scene handling. GLB, OBJ, ZIP-OBJ and
text USD decoding is bounded; binary USD is bounded except as noted below.
USD support covers composed `Mesh` geometry, `xformOp` stacks, units and up
axis, `displayColor` and `UsdPreviewSurface` diffuse colour, opacity and
packaged base-colour textures; external references, nested packages,
subdivision, skinning and animation are not evaluated. A crafted binary
(`.usdc`) layer can demand far more memory than its size because the USD
reader allows each compressed block to expand up to 4 GiB; text layers and
package structure are checked before reading. `.usdz` export writes the derived 3D scene at the
editor's model display detail, not original model bytes. Support for a container format is not a claim to implement
all of its animation, material, texture or scene features. Unitless models use
an estimated proportional scale. 2D previews and 3D LOD are approximations;
missing, unsupported or undecodable content can produce diagnosed placeholders.
Degraded instances participate in point-hit and marquee selection using §9.6
bounds. Declared width/depth are scaled and rotated before taking axis-aligned
bounds; the absent-dimensions fallback is a fixed 1000 mm square. Required
degradation permits inspection while refusing mutations and normal saving;
optional instances remain deletable. Resolved model footprints have separate
point-hit/drag support and are not included in the placeholder marquee query.
The editor has no configured external-cache or remote-retrieval resolver.

Editor footprint and height metadata are optional extensions. Older readers
may preserve an independently resized height but render it incorrectly; legacy
instances without explicit height retain width-derived behavior until edited.

CLI room-label exports currently support the documented Latin/Greek/Cyrillic
repertoire, not general shaping or bidi. Unsupported labels are omitted from
the drawing with a warning; the stored name remains unchanged.

The editor still lacks general group commands, a complete snapping/conformance
gate and several standard file-workflow conveniences. See the
[user guide](USER-GUIDE.md) and [roadmap](ROADMAP.md).

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
