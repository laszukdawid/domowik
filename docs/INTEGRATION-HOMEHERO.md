# HomeHero Integration

Status: **Deferred goal, not part of the current architecture contract.**

Domowik is an independent Apache-2.0 project. HomeHero is a possible future
consumer. API-level separation is the preferred engineering boundary; it is
not, by itself, a legal conclusion about every possible deployment.

## Likely user value

The first useful bridge is mapping a Domowik room to a HomeHero inventory
location so users can inspect where items belong on a plan. Additional ideas
must follow actual product demand.

## Boundary principles

- No HomeHero authentication, billing, entitlement, tenancy, or inventory
  schema in Domowik's core.
- Domowik and HomeHero release and deploy independently.
- Adapters join data through stable public APIs and user-approved identifiers.
- Domowik does not copy HomeHero item data into plan files unless an explicit
  portable/export workflow requires it.
- Authentication/trust details are designed when service boundaries and
  threat models are known; no JWT mechanism is selected yet.

## Format implications today

None are mandatory for v0.1. The earlier proposal to reserve one `ext` object
on every entity was premature: it could not represent multiple systems,
instances, tenants, object kinds, or versions without a breaking change.

Options to evaluate when integration work starts:

- an adapter-owned mapping database or sidecar file;
- a namespaced extension map preserved opaquely by Domowik;
- a general list of typed external links with instance and schema identifiers.

The choice should follow concrete privacy, portability, merge, and offline
requirements and receive its own ADR. If HomeHero never integrates, Domowik
must remain complete and useful.
