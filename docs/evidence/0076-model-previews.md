# Imported model previews — issue 0076

## Provenance and dependency review

OBJ/MTL is implemented independently from public format documentation hosted by
Paul Bourke, https://paulbourke.net/dataformats/obj/ (consulted 2026-10-03).
Only format descriptions were used; no example geometry or reference code was
copied. GLB/accessors/static transforms use Khronos glTF 2.0.1 (2021-10-11),
https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html. The specification's
copyright terms govern that document; no specification text/code is bundled.
The top-down barycentric raster/depth buffer is original implementation over
model-local coordinates, not an exact document topology predicate.

**Reviewed dependency: draco-core 2.2.1**, decoder-only with valence and legacy
decode compatibility. Source: crates.io package, upstream
https://github.com/Filyus/draco-rust at
`c6244f35ec1a3c22a69948b04907244e9b215981`. Declared Apache-2.0, confirmed against
the pinned upstream LICENSE. The package's only normal dependency is bytemuck
(already reviewed), enabling bytemuck_derive 1.12.1 (Zlib OR Apache-2.0 OR MIT,
taken under Apache-2.0; packaged license texts reproduced in NOTICE). No C++ library,
build generator or decoder source is copied into Domowik. We call the public
decoder API with explicit point, face and decoded-byte ceilings. The package
omits its license text; the pinned upstream license URL and Domowik's shipped
Apache-2.0 license provide the recorded license provenance, and the generated
NOTICE records the package and this missing-text fact rather than fabricating
an upstream notice.

Alternatives: a C++ Draco bridge adds native build/platform tooling; rejecting
Draco would leave 31 of the owner's 32 GLBs as placeholders; implementing a
compression codec ourselves would add an unnecessary correctness burden.
Existing zip and tiny-skia dependencies provide archive decoding and PNG
encoding. No user assets, decoded meshes or their artwork are bundled as tests.

The owner-selected ZIP model files retain the attribution/license comments
inside their unchanged imported bytes. Public-format decoding does not derive
Domowik implementation from those meshes or from another application's source.

## Implemented display

Placed objects use top-down mesh previews, including holes and outlines, with
diffuse colors from MTL or glTF base-color factors. Textures/PBR/animation are
not rendered in this increment. Images are derived from embedded content on a
worker isolate, one at a time, and cached by digest per plan; rotation/movement
reuse the same image. A 128-image cache caps retained RGBA images at 32 MiB.
Errors retain a footprint and are shown in its inspector. The native preview
call releases the plan mutex during decode but retains its close/drain lease.

Local corpus smoke: all **66 owner-selected models rendered successfully**,
including the screenshot's desk2 and sandpit and 31 Draco-compressed GLBs.
No assets or derived artwork from this smoke are stored in the repository.
Original tests cover concave OBJ faces, negative indices/winding, ZIP MTL colors,
upper-surface depth, GLB node transforms, malformed/truncated input, and actual
editor pixels (a model hole is transparent rather than filled by a rectangle).

`cargo bench -p domowik-models --bench preview -- --sample-size 10
--measurement-time 1 --warm-up-time 1`: original quad → 256² PNG measured
**2.5857–2.6585 ms** locally. This is a host-specific cold generation cost,
not frame time; view/transform changes use the cached image.

## Verification record

Root workspace/native tests, formatting/clippy, dependency inventories/notices,
FFI probes and **271 Flutter tests** passed. Linux release bundle built.
`./scripts/verify.sh` ended with **`=== VERIFICATION FAILED ===`**,
**`STAGE: P13 clean tree`**, exit **1**, after every preceding executed stage
passed. The worktree contains pre-existing and new uncommitted work. No full
verification pass or macOS/Android bundle execution is claimed.
