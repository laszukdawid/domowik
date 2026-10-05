# ADR 0005: Defer external-reference schema from v0.1

- **Status:** Accepted
- **Date:** 2026-08-22

## Context

An earlier proposal placed one opaque `{system, ref}` object on every entity to
prepare for HomeHero. That model cannot represent multiple providers,
instances, tenants, object kinds, or schema versions, and changing it to a list
would itself be a format change. Domowik is standalone-first and has no current
integration requirements.

## Decision

Do not reserve a HomeHero-specific or generic external-reference field in
format v0.1. Design integration links when a real adapter exists.

Candidate approaches include an adapter-owned sidecar/database, a namespaced
extension map, or a typed list of external links. The selected approach must
address privacy, portability, offline behavior, merge rules, unknown-data
preservation, and multi-instance identity.

## Consequences

- The v0.1 format stays smaller and avoids speculative public API.
- A future adapter may initially keep mappings outside `.domo` files.
- Adding links to the public format requires a normal additive schema proposal
  and migration behavior; deferral does not block API-level integration.
