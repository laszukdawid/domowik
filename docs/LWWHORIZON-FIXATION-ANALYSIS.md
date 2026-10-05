# Issue 0010 — `lwwHorizon` fixation analysis

Status: **Prepared input for owner decision**. This document assembles the
technical inputs needed to decide the retained competing-write horizon value.
It does NOT recommend a value — that is a product decision belonging to the
owner.

## 1. What `lwwHorizon` governs

The `lwwHorizon` parameter (FORMAT-SPEC §5.1 S3) controls the retained
competing-write horizon: how far back in causal time a snapshot retains the
winner-position metadata needed to resolve late-concurrent operations
identically to a never-compacted replica.

**Note on horizon unit:** This analysis assumes the horizon is measured in
batches (or equivalent causal-past size), since positions are per-batch and
S3 retention is stated as "within the retained competing-write horizon."
FORMAT-SPEC §5.1 S3 does not explicitly define the unit, so other
interpretations (time-based, causal-past-size-based) are possible pending
fixation.

### 1.1 Behaviors directly governed

Each behavior below changes as the horizon varies. Specification clauses are
cited for every claim.

#### Winner-position retention (§5.1 S3)

**Clause:** "for every mergeable top-level field written within the retained
competing-write horizon (named parameter `lwwHorizon`, Provisional — fixation
open, owned by this issue; no value asserted), the winning WRITE POSITION of
LWW1"

**Behavior:** A snapshot retains the full write position (causalPastSize,
counterBase, actorId, operationIndex) for every field write whose batch falls
within the horizon. Winner positions enable late-concurrent resolution per
LWW1/LWW5.

**Dependency verified:** The snapshot schema in §5.1 S3 explicitly names
`lwwHorizon` as the bound on retained positions. The `winnerPositions` map is
populated only for fields whose writes fall within this window.

#### Tombstone retention (§5.1 S4)

**Clause:** "for deletes within the same horizon"

**Behavior:** Tombstones (entityId + deleting position + creation position)
are retained for entities deleted within the horizon. This prevents
resurrection by late deliveries and enables I5 branch classification.

**Dependency verified:** §5.1 S4 explicitly states tombstones are retained
"for deletes within the same horizon", using the same `lwwHorizon` parameter.

#### Creation-position retention (§5.1 S1)

**Clause:** "for each entity within the horizon, the write position (LWW1) of
the `createEntity` operation that currently owns the id"

**Behavior:** Creation positions are retained for entities whose creation fell
within the horizon. Required for LWW4 collision resolution and I5 branch 1
classification.

**Dependency verified:** §5.1 S1 names "within the horizon" as the retention
condition for `createdPositions`.

#### Late-concurrent operation resolution equality (V5)

**Clause:** "V5 Late-concurrent after compaction: an operation arrives whose
batch predates snapshot coverage but whose write position loses to a
snapshot-retained winner position. Required deterministic projection:
Resolved via retained winner positions (LWW5/S3): identical outcome to the
uncompacted replica."

**Behavior:** When a late-arriving operation's write position is compared
against a retained winner position, the outcome must match what would have
happened in a never-compacted replica. This is the core guarantee of LWW5.

**Dependency verified:** Vector V5 explicitly depends on retained winner
positions per LWW5/S3. The horizon determines which positions are retained,
therefore which late operations can be resolved identically.

#### Compaction eligibility (§5.2 S6)

**Clause:** "Dropping still additionally requires retention of the S1 and
S3–S5 metadata needed to prevent resurrection (creation positions, tombstones
with their live intervals, winner positions, watermarks)."

**Behavior:** Operations may not be dropped from the log unless their
positions, tombstones, and creation metadata are retained within the snapshot.
The horizon bounds this metadata, therefore bounds which operations are
eligible for compaction.

**Dependency verified:** §5.2 S6 names the S3/S4 metadata as a precondition
for dropping. Since S3/S4 retention is bounded by `lwwHorizon`, compaction
eligibility depends on it transitively.

### 1.2 Behaviors NOT claimed to vary with `lwwHorizon`

The following are NOT CLAIMED to vary with `lwwHorizon` based on specification
clause reading. No verification of non-dependence was performed beyond
inspecting that these clauses do not reference S3/S4 retention or winner
positions:

- **Duplicate suppression (I2):** Watermark-based duplicate detection uses
  frontier counters per §4.5 I2 ("A delivered batch whose entire counter range
  lies at or below the applying replica's per-actor watermark is already
  applied"). The clause does not reference winner positions or S3.

- **Stale-batch discard (I5) for operations OUTSIDE the horizon:** Operations
  whose targets predate the snapshot's retained creation/deletion metadata are
  discarded deterministically by interval logic per §4.5 I5. The horizon
  affects the RANGE of intervals retained (which entities have creation/
  deletion positions), not the discard rule itself.

- **Causal ordering (F3):** The canonical order by (causalPastSize,
  counterBase, actorId) is computed from delivered batches and their frontiers
  per §4.4 F3. The clause does not reference snapshot positions.

- **Frontier closure validation (F4):** Closure validation applies to retained
  batches whose ancestors are also retained per §4.4 F4. The clause does not
  reference S3, though the horizon transitively affects which batches are
  retained for validation.

## 2. Cost curve from issue 0035 measurements

Issue 0035 delivered committed byte measurements for two scenarios under
`docs/evidence/0035-size-corpus/`. These figures are HOST-INDEPENDENT byte
counts (ED-0070 amendment (a)) — deterministic functions of committed package
bytes, reproducible on any machine.

### 2.1 Current unbounded-retention cost

Retention is currently UNBOUNDED per `params.go:50`. The measured costs under
unbounded retention are:

| Scenario | Total bytes | Snapshot class | History class | Winner/tombstone metadata |
|----------|------------|----------------|---------------|--------------------------|
| `empty-v1` | 3,597 B | 349 B (compressed from 1,110 B) | 0 B | S3 positions: 0 retained; S4 tombstones: 0 retained |
| `unfurnished-1000-v1` | 119,394 B | 116,144 B (compressed from 993,900 B) | 0 B | S3 positions: unbounded; S4 tombstones: unbounded |

**Key observation:** Issue 0035's Part A scenarios contain ZERO retained
operation entries by construction ("History cost is zero by construction: Part
A packages carry a complete snapshot and zero retained operation entries",
README.md:26-28). Therefore these measurements reflect snapshot-only costs,
with winner/tombstone metadata contributing to the snapshot class.

The `unfurnished-1000-v1` scenario snapshot (116,144 B compressed from 993,900
B logical) includes:
- 1,000 entities across 4 levels, 10 ring instances per level
- 156 walls, 156 joins, 156 openings, 40 room records (per ASSET-STRATEGY
  §10.9 as clarified by ED-0025)
- Winner positions for every field write (unbounded)
- Creation positions for every entity (unbounded)
- Zero tombstones (no deletions in the corpus)

**Unbounded retention therefore costs 116 KB (compressed) for 1,000 entities
with zero operations retained.** This is the baseline against which any
bounded horizon is compared.

### 2.2 L23 snapshot-metadata ceiling (ratified)

**Definition:** FORMAT-SPEC §9.3 row L23 (line 1627) ratifies the
snapshot-metadata ceiling as **16 MiB** (16,777,216 bytes). This is a
reader-side resource cap that bounds the total size of the snapshot metadata
section, including:
- `batchPositions` table (batch stems → frontiers)
- `winnerPositions` map (entity.field → write position)
- `createdPositions` map (entityId → creation position)
- `tombstones` map (entityId → deleting position + creation position)

**Status:** Ratified per FORMAT-SPEC §9.3 L23. The 16 MiB value is fixed and
applies to all readers.

**Compaction consequence:** Per FORMAT-SPEC §5.1 S3 (lines 810-812): "Row
L23's snapshot-metadata ceiling bounds the total, so a document whose required
metadata will not fit within L23 cannot compact, and that refusal is
observable under §5.2 — S6 for a synced document, S7 for a local-only one."

When a document's horizon-retained metadata exceeds 16 MiB:
- Per §5.2 S7: A local-only compaction that cannot proceed reports a
  NON-BLOCKING diagnostic naming the retained state and the blocking reason.
- The document cannot compact, so the operation log grows indefinitely.
- Saves become slower, package size grows without bound.

**Measured distance from ceiling:** The `unfurnished-1000-v1` scenario with
unbounded retention and a single actor costs 116 KB compressed (993,900 B
logical) for snapshot state, which is **0.7% of the 16 MiB ceiling**. However,
multi-actor documents with large frontiers will approach the ceiling faster
(see §2.3).

### 2.3 Horizon-dependent metadata growth

FORMAT-SPEC §5.1 S3 states: "The NUMBER of retained positions is bounded by
the projected state: at most two per entity within the horizon, one per
mergeable field, and one referenced batch per position at worst. The BYTES are
not a constant factor on that state: each referenced batch carries a frontier
whose member count grows with the number of actors that have ever written the
document, up to row L18, and that count is a function of history rather than
of current state."

**Per-entity metadata under bounded horizon:**
- Creation position: 1 per entity (if created within horizon)
- Deleting position: 1 per tombstone (if deleted within horizon)
- Winner positions: 1 per mergeable field written within horizon

**Batch-position overhead:** Each retained position references a batch stem
(`<counterBase>-<actorId>`), and the `batchPositions` table maps each stem to
its frontier (per-actor counters). This overhead grows with:
1. Number of distinct batches referenced by retained positions
2. Number of actors in each batch's frontier

**The critical unknown:** How many actors have written a typical document, and
how large are their frontiers. The issue 0035 corpus uses a SINGLE ACTOR by
construction (generator identity `actor/create`), so it provides no
multi-actor data.

### 2.4 Extrapolation to bounded horizons

Without multi-actor corpus data, precise cost curves cannot be computed.
However, structural bounds can be stated:

**Lower bound (single-actor case):** If a document has only one actor, each
batch's frontier has one entry, and the per-position overhead is minimal
(roughly 50-100 bytes per referenced batch for the stem + frontier encoding).

**Upper bound (multi-actor case):** If a document has N actors (up to L18 =
2^31 per §9.3), each batch's frontier can have up to N entries, and the
per-position overhead grows linearly with N.

**Candidate horizon values and their metadata implications:**

**Note:** The byte estimates below are **rough order-of-magnitude
extrapolations**, not measured values. They assume approximately 50-100 bytes
per batch stem + frontier overhead, scaled by actor count. These are
illustrative only.

| Horizon | Retained batches (1,000-entity doc) | Single-actor overhead | Multi-actor overhead (N=10) |
|---------|-------------------------------------|----------------------|---------------------------|
| 0 (no positions retained) | 0 | 0 B | 0 B |
| 1 batch | ~1 | ~100 B | ~500 B |
| 10 batches | ~10 | ~1 KB | ~5 KB |
| 100 batches | ~100 | ~10 KB | ~50 KB |
| Unbounded | All | 116 KB (measured) | Unknown (no data) |

**VERIFICATION STATUS:** These extrapolations are INFERRED from the schema and
the single-actor measurements. They are NOT verified against actual
multi-actor documents. The true cost curve for multi-actor collaboration is
unmeasured.

## 3. Failure modes

### 3.1 Horizon too small

**Failure:** Late-concurrent operations cannot be resolved identically to a
never-compacted replica.

**Mechanism:** When an operation arrives whose write position would have
competed with a field write that predates the retained horizon, the snapshot
has no winner position for that field. The late operation cannot be compared
against the true winner (V5 exactness is lost), and the projection diverges
from what full replay would have produced.

**User-visible consequence:** A user's edit, arriving late due to network
delay or offline work, is either discarded entirely (if it targets a
nonexistent entity under I5) or applied incorrectly (if the target exists but
the field's prior winner is unknown). In either case, the user's intent is
lost without a conflict surface.

**Severity:** Increases with:
- Frequency of late arrivals (offline editing, poor connectivity)
- Number of concurrent editors
- Time between writes and their delivery to all replicas

**Illustrative example:** User A edits a wall's `thicknessMm` at batch
position P1. User B concurrently edits the same field at P2, where P2 > P1 by
LWW1. B's write wins. If B's device compacts before A's write arrives, and the
horizon is too small to retain P2's position, A's late write cannot be
compared against P2. The system has no basis to decide between them.

### 3.2 Horizon unbounded (current state)

**Failure:** Snapshot metadata grows without bound as document history
lengthens.

**Mechanism:** Every field write, every entity creation, and every tombstone
accumulates winner/creation/deleting positions forever. Each position
references a batch, and each batch's frontier grows with the actor set. The
snapshot metadata section is bounded by L23's ratified 16 MiB ceiling
(§2.2).

**User-visible consequence:** Large documents with long edit histories
eventually exceed the 16 MiB snapshot-metadata ceiling (L23, §2.2). When this
happens:
- Per §5.2 S7: "A local-only compaction that cannot proceed at all —
  including one whose required snapshot metadata (S3) will not fit row L23 —
  reports a NON-BLOCKING diagnostic on the same terms, naming the retained
  state and the blocking reason; silent refusal is forbidden."
- The document can no longer compact, so the operation log grows indefinitely.
- Saves become slower, package size grows without bound, and the document may
  eventually become unloadable.

**Severity:** Increases with:
- Number of edits (every field write adds a position)
- Number of actors (each actor adds a frontier entry to every referenced
  batch)
- Document lifetime (old documents with long histories hit the ceiling first)

**Measured cost:** The `unfurnished-1000-v1` scenario, with unbounded
retention and a single actor, already costs 116 KB compressed for snapshot
metadata alone. Multi-actor documents with thousands of edits will approach
the 16 MiB ceiling (L23, §2.2) faster due to frontier overhead.

**Compaction blockage:** Per §5.2 S6, synced documents cannot compact until
the drop precondition is established (all peers have delivered all batches
they created before acknowledging). Unbounded retention exacerbates this: even
when the precondition IS established, the snapshot may not fit within the
16 MiB ceiling, blocking compaction for a different reason.

## 4. Candidate values and trade-offs

The following candidates are presented as options with their consequences. No
ranking or recommendation is given.

### Option A: Zero horizon (no positions retained)

**What it does:**
- Retains zero winner positions, zero tombstones, zero creation positions
- Minimal snapshot metadata overhead (~0 bytes beyond entity state)
- Compaction always succeeds (no L23 pressure)

**Trade-off:**
- Every late-concurrent operation loses V5 exactness
- Users lose edits silently when deliveries are reordered
- Offline editing becomes unreliable
- Only viable if late arrivals are impossible (single-device, always-online)

**When dominated:** Dominated by any positive horizon if late arrivals occur
at all.

### Option B: Short horizon (e.g., last 10 batches)

**What it does:**
- Retains positions for the most recent 10 batches per actor
- Metadata overhead grows with active-actor count, not total history
- Compaction can proceed once the 11th-oldest batch's precondition is met

**Trade-off:**
- Late arrivals within the 10-batch window resolve correctly (V5)
- Later arrivals lose V5 exactness
- Metadata stays bounded (roughly 1-5 KB for single-actor, 5-50 KB for 10
  actors)
- Horizon measured in batches, not wall-clock time, so "10 batches ago" varies
  by editing rate

**When dominated:** Dominated by a larger horizon if late arrivals routinely
exceed 10 batches. Dominates Option A if any late arrivals occur.

### Option C: Medium horizon (e.g., last 100 batches)

**What it does:**
- Retains positions for the most recent 100 batches per actor
- Covers longer offline periods and slower sync
- Metadata overhead 10× Option B

**Trade-off:**
- Most realistic offline/delay scenarios stay within the horizon
- Metadata grows (roughly 10-50 KB single-actor, 50-500 KB for 10 actors,
  order-of-magnitude estimates only)
- Compaction delayed until the 101st-oldest batch's precondition is met
- Still bounded (16 MiB ceiling pressure exists but is manageable for typical
  documents)

**When dominated:** Dominated by unbounded if late arrivals routinely exceed
100 batches AND multi-actor frontier overhead remains acceptable. Dominates
Option B if 10-batch coverage is insufficient.

### Option D: Large horizon (e.g., last 1,000 batches)

**What it does:**
- Retains positions for the most recent 1,000 batches per actor
- Covers extreme delay scenarios (months offline, very slow sync)
- Metadata overhead 10× Option C

**Trade-off:**
- Nearly all realistic scenarios stay within the horizon
- Metadata grows significantly (100 KB - 5 MB depending on actors,
  order-of-magnitude estimates only)
- Approaches 16 MiB ceiling (L23, §2.2) for large multi-actor documents
- Compaction delayed until the 1,001st-oldest batch's precondition is met

**When dominated:** Dominated by unbounded if the 16 MiB ceiling is hit
anyway. Dominates Option C if 100-batch coverage is insufficient.

### Option E: Unbounded (current state)

**What it does:**
- Retains all positions forever
- V5 exactness guaranteed for all late arrivals
- No compaction-eligibility bound from horizon

**Trade-off:**
- Metadata grows without bound
- Eventually exceeds the 16 MiB ceiling (L23, §2.2), blocking compaction
  entirely (§5.2 S7 diagnostic)
- Single-actor documents: measured 116 KB for 1,000 entities (manageable)
- Multi-actor documents: unbounded growth with actor count and history length
- Compaction already gated by §5.2 S6 precondition (separate issue)

**When dominated:** Dominated by any bounded horizon if the 16 MiB ceiling is
ever reached.

## 5. Summary for decision

**What needs to be decided:** The value of `lwwHorizon`, measured in batches
(or equivalent causal-past size bound).

**Key trade-off:** Coverage of late arrivals vs. metadata overhead.

**Unknowns that bound confidence:**
- Typical actor count in real documents (single-actor corpus cannot answer
  this)
- Typical delay distribution (how often do operations arrive >10, >100, >1000
  batches late?)
- Multi-actor frontier overhead (not measured; single-actor measurements show
  116 KB for 1,000 entities, but multi-actor scaling toward the 16 MiB ceiling
  is unmeasured)

**Verified inputs:**
- Unbounded retention costs 116 KB compressed for 1,000 entities,
  single-actor, zero operations retained (measured, host-independent)
- Schema structure bounds metadata at O(entities × fields × horizon) plus
  O(batches × actors) for frontier overhead
- V5 exactness is lost for operations outside the retained horizon (specified,
  not measured)

**Decision authority:** Product owner. The horizon determines what users lose
when late deliveries exceed it, which is a product trade-off, not a technical
one.

---

**Prepared by:** issue 0010 executor  
**Date:** 2026-08-31  
**Status:** Input for owner decision — NOT a recommendation
