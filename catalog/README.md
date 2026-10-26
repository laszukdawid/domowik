# Original hosted furniture

100 original, low-poly generic household models: seating, sofas, tables, beds,
storage, office, kitchen/bathroom and outdoor furniture. Designs and dimensions
are invented, illustrative planning aids, not manufacturer products or measured
ergonomic/safety specifications. No external models, catalogs, photographs,
textures or brand designs were consulted or copied.

The recipes, generator, model geometry and derived thumbnails are original
**Apache-2.0 Domowik content**. GLB writing reuses the project's existing
Apache-2.0 [Welcome Home box builder](../examples/welcome-home/generate.py).
The already reviewed Rust decoder/rasterizer produces the thumbnails. No new
runtime or development package is needed. GLB encoding follows the existing
Khronos glTF reference recorded in [Provenance](../docs/PROVENANCE.md).

## Generate and verify

Build the native bridge first, with the pinned Rust toolchain:

```sh
cargo build --release --manifest-path frontend/Cargo.toml
python3 catalog/generate.py --output build/catalog/v1
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s catalog -p 'test_*.py'
```

The output must be a **new directory**; generation never overwrites an existing
distribution. A different output path is useful when comparing releases. The
generator is deterministic and adds no timestamps or host paths to published
files. Each index entry records its stable ID, display name, category, measured
width/depth/height in mm, Apache-2.0 license, model/thumbnail byte counts and
SHA-256 hashes. File names include those hashes. Each GLB also carries copyright,
license and original-catalog provenance in `asset` metadata. Each GLB includes
the full license text and its original-content notice so saved plans retain
those notices offline. `LICENSE.txt` and
`NOTICE.txt` travel with the catalog. Tests regenerate every byte twice, verify
the inventory, import all 100 models through Rust, and compare decoded physical
dimensions. Generator/decoder verification is part of `scripts/verify.sh`.

The initial catalog has a 52,963-byte index, 2,135,796 bytes of models and 262,742
bytes of thumbnails. All images are 256 by 256 pixels. No GLB exceeds 44 KiB and
no thumbnail exceeds 5 KiB. These are measured uncompressed file sizes for this
recipe revision, not general limits or a network-latency guarantee.

## Hosting and discovery contract

Generated output is ignored build output. It is **not** a Flutter asset, a
pubspec resource, or part of the web application image. An optional independent
static server exposes it at the deployed page's same-origin `/catalog/v1/`.
The application starts without requesting the index, previews or models.
Opening Furniture discovers `index.json`; search is local, and only visible
entries request thumbnail PNGs. Selecting an entry alone downloads no GLB.
The explicit **Place in plan** action downloads just the chosen model, verifies
its exact size and SHA-256, and imports it through the existing Rust API before
starting placement. As with a manual upload, imported library bytes remain in
the plan even if the subsequent canvas placement is cancelled.

Successful index discovery is cached for the page lifetime; failed discovery
can be retried. Model bytes are not held in a second application catalog cache.
Thumbnails use the existing serialized, bounded 64-entry dialog cache, discarded
on close. An active bounded thumbnail/index request may finish after closing the
dialog, but cannot update the closed dialog; queued previews are dropped.
Leaving the editor aborts active requests. Requests omit credentials, reject
redirects, use same-origin mode and have a 15-second deadline. Indexes are capped
at 128 KiB and 500 entries; models at 256 KiB; PNGs at 64 KiB and 256 pixels per
axis. Only fixed hash-addressed model/thumbnail paths are accepted. HTTPS is
provided by the site's normal deployment. SHA-256 checks integrity against the
index, not authorship or trust in a compromised origin.

Catalog failure leaves local uploads and embedded models usable. Missing assets
in a `.domo` file do not trigger discovery or arbitrary URL retrieval. Imported
bytes are embedded, so placed catalog models reopen offline without this server.
This is browser catalog browsing, not a general remote-asset resolver or a
change to the `.domo` format. Native filesystem libraries remain unchanged.

Operators should compress the small mutable index, cache hash-addressed files
immutably, retain published hash paths when updating the index, and deploy the
catalog independently of the editor. The Firefox split-view/modal crash tracked
in [#3](https://github.com/laszukdawid/domowik/issues/3) is not fixed by this feature;
use 2D-only mode for Furniture on affected systems.
