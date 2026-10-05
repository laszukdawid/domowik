# ADR 0001: Standalone Apache-2.0 project

- **Status:** Accepted
- **Date:** 2026-08-22

## Context

Domowik should become useful as an independent layout application before any
HomeHero integration. A public, permissive codebase encourages reuse and
allows independently deployed proprietary or open adapters without building
HomeHero concepts into the core.

## Decision

Domowik is a standalone open-source project under Apache-2.0. It has a GUI and
a headless CLI. HomeHero integration is deferred and implemented through
public boundaries when concrete requirements exist.

The project uses a black-box behavior-study/provenance policy and audits all
distributed dependencies and assets. These are engineering risk controls, not
guarantees of legal status.

## Consequences

- Apache-2.0 provides a permissive grant including explicit patent terms.
- Domowik cannot depend on HomeHero auth, billing, inventory, or deployment.
- User and third-party content retains its own license; it is not relicensed by
  being placed in a `.domo` file.
- Copyleft or proprietary components are not categorically impossible, but
  distributing them requires explicit compatibility review and an ADR.
- Commercial use and hosting are permitted subject to all relevant component
  licenses; the reason for Apache-2.0 is project preference and integration
  flexibility, not a claim that GPL software cannot be commercial.
