# ADR 0004: Identified operation batches and snapshots

- **Status:** Proposed
- **Date:** 2026-08-22

## Context

Domowik needs deterministic edits, CLI scripting, local undo, recovery, and a
credible path toward multi-device synchronization. Saving only the latest
state makes change exchange and replay difficult. Conversely, calling any
append-only log "CRDT-ready" overstates what it provides.

## Proposed decision

Represent durable changes as atomic, identified operation batches over stable
UUID-keyed entities. Store periodic snapshots with a causal coverage frontier
and the per-field version/tombstone/deduplication metadata required to merge
late operations after compaction. Require operation identity, single-writer
replica actors and counters, idempotency, tombstone/delete rules, batch
ordering, and explicit conflict semantics in the v0.1 schema.

Entity collections are keyed rather than implicitly ordered. Ordered values
remain allowed where their order is intrinsic and specified (rings, tuples,
batches). Wall endpoint relationships are modeled explicitly enough that
connected edits and concurrent changes are not hidden pointer mutation.

## What this does not decide

- A CRDT or real-time collaboration algorithm.
- Whether initial sync uses per-field LWW, locks, or explicit conflicts.
- Permanent audit-history retention.
- Persistence of the remaining undo stack across sessions. An executed undo
  that changes saved state still appends a compensating batch.
- The physical operation-batch encoding.

## Validation gates

- Conformance vectors for duplicate, reordered, concurrent, delete-versus-set,
  late-concurrent, and post-snapshot operations.
- Defined snapshot frontier and safe local/synchronized retention behavior.
- Room split/merge reconciliation and wall-join batch examples.
- Property tests showing deterministic projection under all permitted delivery
  orders.

## Consequences

The metadata costs more than a simple single-user change list, but avoids
throwing away identities needed for sync. It reduces future migration risk; it
does not guarantee migration-free CRDT collaboration.
