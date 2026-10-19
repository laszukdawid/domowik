# ADR 0007: Experimental and post-v1 format compatibility

- **Status:** Proposed

## Current experimental boundary

The current implementation emits and supports schema `0.1`. It is experimental,
not a stability freeze or a claim of a public format release. No predecessor
schema is claimed as publicly released; no migration path may be inferred from
a fixture or development conversion helper.

[FORMAT-SPEC §8](../FORMAT-SPEC.md#8-versioning-and-unknown-data) governs current
version dispatch, unsupported required data, optional-data preservation and
future released-schema migration obligations. Unsupported labels and kinds are
refused before effects. Unknown optional data is preserved with identical
meaning or write-back is refused; it must not be stripped silently.

A tested conversion, where provided, writes a separate validated package and
does not make opening a document an implicit migration. Save/recovery claims
remain limited to their documented platform and filesystem outcomes.

## Proposed post-v1 contract

Before a file claiming format v1.0.0 is released, ratify an explicit contract:

1. **Version form:** `MAJOR.MINOR.PATCH` with
   [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). The format's
   public API is its logical schemas, operations, snapshots, container and
   reader/writer obligations, not just a version string.
2. **MAJOR:** incompatible field/semantic/container changes or tightened
   acceptance boundaries, with tested migrations for supported predecessors.
3. **MINOR:** additions older same-major readers can handle through the
   preserve-or-refuse optional-data contract. Each compatibility claim requires
   positive and negative conformance fixtures.
4. **PATCH:** clarifications or corrections that do not change the specified
   accepted/rejected boundary. A changed boundary is at least MINOR.
5. **Preservation/security floor:** later versions do not retroactively weaken
   the duties for data written under earlier same-major schemas.

## Acceptance and consequences

This ADR remains Proposed and creates no post-v1 guarantee today. Ratification
requires conformance tests and agreement with FORMAT-SPEC §8.3. Experimental
development schemas remain governed by §8.2 and
[ADR 0011](0011-extensible-versioned-document-format.md); post-v1 semantics do
not apply retroactively. If this direction changes, update both this ADR and
the format specification together.
