# Naming and identity

This is the current registry for project identifiers. It establishes no
trademark clearance, domain availability or rights in third-party names.

| Artifact | Current value | Status |
| --- | --- | --- |
| Product display name | `Domowik` | Provisional |
| Repository | `https://github.com/laszukdawid/domowik` | Current source location |
| Documentation | `https://laszukdawid.github.io/domowik/` | Configured Pages location; availability depends on deployment |
| CLI binary | `domowik` | Provisional |
| Editor package/executable | `domowik_editor` | Current development identifier |
| File extension | `.domo` | Provisional |
| Application IDs | Existing platform scaffolding IDs | Development-only; review before store publication |
| Media type | No project-specific registered MIME type | Deferred; identify files by extension and container signature |

The project does not claim control of a `domowik.*` domain. Similar names are
used by other parties; no affiliation or legal clearance is implied.

## Publication and changes

Before a public binary/store release, review the product name, relevant
trademark registries and package namespaces, select application IDs from a
controlled namespace, and decide whether a registered media type is needed.
These are risk-review obligations, not an assurance that checks confer rights.

A rename updates this registry, [ADR 0006](decisions/0006-naming-and-identity.md),
README/guides, Cargo and Dart package metadata, platform manifests, native
symbols where necessary, release manifests and build scripts. Assess
compatibility for executable names, file associations, persisted preferences
and external callers; record migration aliases when any public identifier
changes. No public migration aliases are currently promised.
