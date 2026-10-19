# Embedded model texture decoding

Decision/review, 2026-10-17: use pinned `image` 0.25.9 with default features
disabled and only `png`, `jpeg`, `webp`. This is a pure-Rust decoder usable in
native and both WebAssembly targets, with pre-allocation image limits. Native
platform codecs would duplicate behavior in browsers; implementing image
codecs internally would add unnecessary security and maintenance risk.

Reviewed exact crates.io package manifests and packaged LICENSE files (not a
legal opinion). Newly resolved distributed packages:

| Package | Version | Declared licence | Selected branch |
|---|---|---|---|
| image | 0.25.9 | MIT OR Apache-2.0 | Apache-2.0 |
| image-webp | 0.2.4 | MIT OR Apache-2.0 | Apache-2.0 |
| moxcms | 0.7.11 | BSD-3-Clause OR Apache-2.0 | Apache-2.0 |
| pxfm | 0.1.30 | BSD-3-Clause OR Apache-2.0 | Apache-2.0 |
| byteorder-lite | 0.1.0 | Unlicense OR MIT | MIT |
| quick-error | 2.0.1 | MIT/Apache-2.0 | Apache-2.0 |
| zune-core | 0.5.3 | MIT OR Apache-2.0 OR Zlib | Apache-2.0 |
| zune-jpeg | 0.5.15 | MIT OR Apache-2.0 OR Zlib | Apache-2.0 |

Exact source packages: `https://crates.io/api/v1/crates/<package>/<version>/download`.
`image` also reuses already-reviewed `bytemuck`, `num-traits` and `png`.
The root inventory and generated `NOTICE` retain packaged copyright, licence
and notice texts; redistributions must retain them. No copyleft or proprietary
code is adopted. `moxcms`/`pxfm` BSD alternatives bear Radzivon Bartoshyk's
copyright. `byteorder-lite`'s MIT notice names Andrew Gallant. `quick-error`
and zune packages carry their developers' notices. No code from these packages
is copied into Domowik; they are linked dependencies.

## Public-format provenance

Independent implementation from Khronos' public specifications:

- Khronos glTF 2.0.1, 2021-10-11, rendered specification commit
  `8e798b02d254cea97659a333cfcb20875b62bdd4`:
  https://registry.khronos.org/glTF/specs/2.0/glTF-2.0.html
  (base-colour sRGB images, linear factors, UV origin, accessors and samplers).
- `KHR_texture_transform`:
  https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_texture_transform
- `EXT_texture_webp`:
  https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Vendor/EXT_texture_webp
- `KHR_draco_mesh_compression`:
  https://github.com/KhronosGroup/glTF/tree/main/extensions/2.0/Khronos/KHR_draco_mesh_compression

No reference implementation is copied. Bilinear sampling and texture array
packing are original implementations of standard sampling arithmetic. The
existing Apache-2.0 `draco-core` 2.2.1 encoder is enabled only for synthetic
tests/benchmarks; its packaged `examples/encode_decode.rs` was consulted for
API usage, not copied. Test geometry and quadrant images are original.
Its packaged `geometry_attribute.rs` documentation was also consulted to
confirm that `read_f32s` follows point mappings but does not normalize integer
components; glTF accessor normalization is applied independently by Domowik.

## Imported content

User-imported models retain their original rights. Public availability does not
grant redistribution permission or relicense them under Apache-2.0. Private
imports are not bundled and are not the source of tracked fixtures; tests use
original synthetic content. No GPL application source, schemas or catalogs
were consulted for this implementation.
