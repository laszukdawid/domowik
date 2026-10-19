# ADR 0006: Provisional naming and identity

- **Status:** Accepted

## Decision

Use the identifiers in [Naming and identity](../NAMING.md) as the current
registry. Product name, CLI name and file extension are provisional. The
repository and configured documentation URL identify this project; no control
of a similarly named third-party domain or affiliation is claimed.

Keep development platform identifiers stable until an intentional migration.
Before store/public binary publication, review naming and relevant namespaces,
select application IDs from a controlled namespace, and explicitly decide any
MIME registration. These checks reduce risk; they do not establish legal rights.

## Consequences

- A rename is coordinated across the registry, metadata, guides, platform
  manifests, native interfaces and release scripts.
- Public identifiers require a compatibility assessment and migration/alias
  policy when changed.
- Source publication is not a promise of stable file, executable or app IDs.
- Third-party licence and naming concerns require review rather than inferred
  clearance from a failed search or an unavailable registry.
