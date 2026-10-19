# ADR 0005: Defer external-reference schema from v0.1

- **Status:** Accepted
- **Date:** 2026-08-22

## Context

An earlier proposal placed one opaque `{system, ref}` object on every entity
for external references. That model cannot represent multiple providers,
instances, tenants, object kinds, or schema versions, and changing it to a list
would itself be a format change. Domowik is standalone-first and has no current
external-reference requirements.

## Decision

Do not reserve a provider-specific or generic external-reference field in
format v0.1. Design external links only when concrete requirements exist.

Candidate approaches include an adapter-owned sidecar/database, a namespaced
extension map, or a typed list of external links. The selected approach must
address privacy, portability, offline behavior, merge rules, unknown-data
preservation, and multi-instance identity.

## Consequences

- The v0.1 format stays smaller and avoids speculative public API.
- A future adapter may initially keep mappings outside `.domo` files.
- Adding links to the public format requires a normal additive schema proposal
  and migration behavior.
