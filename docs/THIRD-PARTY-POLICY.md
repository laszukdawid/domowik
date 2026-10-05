# Third-Party Policy

Status: **Active project policy.** This is an engineering and distribution
policy, not legal advice.

## Goals

- Keep Domowik source reusable under Apache-2.0.
- Know what is shipped in every application and service artifact.
- Keep code licenses separate from user-created or imported content licenses.
- Avoid discovering incompatible obligations at release time.

## Dependency categories

### Code distributed with Domowik

Libraries, generated bindings, shaders copied from another project, native
binaries, fonts, and bundled tools must have their exact source, version,
license, copyright notice, and distribution obligations recorded.

Apache-2.0, MIT, BSD, ISC, Zlib, and similar permissive licenses are preferred.
Copyleft, source-available, or proprietary components require an ADR and
explicit compatibility review before adoption. Transitive dependencies count.

### Development-only tools

Tools not distributed with Domowik may use other licenses, but their output
must not introduce incompatible code or assets. Record important generators
and toolchain versions for reproducibility.

### System components and optional services

GPU drivers, CUDA, operating-system frameworks, and hosted reconstruction
stacks are not automatically governed like linked application libraries.
They still require a deployment-specific license review, notices where
required, and a documented fallback or support boundary. Do not describe
proprietary system software as permissively licensed.

### User and catalog assets

Plans, photographs, textures, models, fonts, and splats retain their original
rights. The `.domo` manifest should eventually record asset provenance and an
optional SPDX license identifier or license URI. Domowik must not imply that
importing an asset makes it Apache-2.0.

Bundled catalogs require an asset-by-asset manifest. Unknown-license content
must not be included in releases.

## Required release artifacts

Before the first binary release:

- Automate source dependency license scanning.
- Generate a software bill of materials (SBOM).
- Maintain `NOTICE`/third-party notices for distributed components.
- Record native binary provenance and reproducible build instructions where
  practical.
- Document licenses of bundled assets separately from source dependencies.

## Adding a dependency

Record the exact package and version, why it is needed, alternatives
considered for material dependencies, direct and transitive licenses, whether
it is linked or bundled, and required notices. Uncertain cases block adoption
until reviewed.
