# Issue 0011 — causal-position compatibility: what a reader can derive, and what existing packages can become

Evidence record produced by issue 0011's continuation pair (worker `w0011`,
adversarial reviewer `r0011`) under ED-0035, ED-0036 and ED-0037.

**This document is not normative and proposes no specification text.** It
records what the delivered code and the packages it writes actually contain,
and it answers two questions routed to this pair: whether a reader can derive
the ordering position ED-0037 adopts, and what happens to packages already
written by the current code. Specification wording is issue 0004's; §7 wording
is issue 0005's; the product-direction decision was the project owner's and is
recorded in ED-0037.

Decision trail this document serves:

- **ED-0032** ruled that causality governs and that LWW1's key comparison
  breaks ties among concurrent writes only.
- **ED-0036** halted issue 0011's implementation after this pair's reviewer
  showed that a causal fold retaining the value-writer's key trades the V2
  repair for a V5/V6 divergence, and after the orchestrator found §4.4 F3's
  canonical order unsatisfiable as written.
- **ED-0037** records the owner's ruling: canonical order is ascending
  **P = (causalPastSize, counterBase, actorId)**, a field's winner is the
  greatest P among writes to it, and K separates only equal-past writers.
  ED-0037 comment (s) corrects that decision's own justification: equal-past
  writes are **not** necessarily concurrent, since two writes in one batch
  share a past size and are one actor's ordered writes, separated by
  `operationIndex`.
- **ED-0038** records the owner's ruling on legacy packages: one documented
  reading convention, V5 exactness explicitly not claimed, and rewrite on
  first save (§4.3).
- **ED-0039** records the owner's ruling tightening §5.2 S6's drop condition:
  a batch may be dropped only when no undelivered batch can still be
  positioned before it (§2.4). Its supporting inference from acknowledgement
  was narrowed by a later active amendment, which this document follows.
- **ED-0040** stops new issue dispatch after this wave, so the P
  implementation stays queued and unstarted; §5 is preparation for whoever
  resumes it, not a plan this pair executes.

Issue 0004's specification text was **accepted and integrated on main by
`fa7e847`** (tracker record `baca43e`) while this document was under review.
**That text is young, and two of its sentences were contradicted within hours
of integration — both by this pair, reading it fresh for a different purpose,
rather than by its own adversarial review or by the orchestrator.** §4.6's LWW4
scoping mismatch (ED-0040 queue item 8, found by this worker) and §5.1's
legacy-mode selection versus the actor universe (item 9, found by this
reviewer) are both queued as specification defects. A reader should treat the
citations below as accurate to the accepted text and not as evidence that the
accepted text is settled.
Where main's `docs/FORMAT-SPEC.md` now carries a rule normatively, that text is
the reference and this document cites it; the decision records remain the
authority only for what the specification does not yet state. Where a question
this document routed has since been answered, the row is marked resolved with
its citation rather than deleted, so a reader can see both that the question
existed and where it was settled.

Where ED-0037 has been amended by its own comments, this document cites the
**in-force summary** those comments maintain rather than the superseded
decision prose.
- The candidate enumeration this pair had been asked for was stopped by the
  orchestrator once the ruling landed. §6 records the eliminated families and
  cites the two decisions rather than reproducing a comparison matrix.

### Terminology used precisely below

- **declaredPastSize** — the arithmetic sum of the counters in a batch's
  declared `frontier`. It is mathematically derivable from bytes the reader
  already holds, **subject to the representation gap of §3.6**: the current
  types cannot represent every legal sum, so "derivable" is a statement about
  the bytes, not a claim that today's code can compute it.
- **causalPastSize** — the number of operations actually in the causal past,
  which is what ED-0037's P means. The two coincide **only** if the declared
  frontier is transitively closed and honest (§3.3). Every "exact" claim below
  says which of the two it is about.

### The four findings that carry the rest

1. **A reader can compute declaredPastSize for every batch entry it holds, and
   for no write that survives only as a snapshot key.** The sum is one pass
   over a frontier the reader already parses; a compacted write's frontier is
   not retained at all. So the ruling needs no revisiting on derivability
   grounds, and the retained position accepted §5.1 now specifies is the
   missing piece (§3.1, §3.2) —
   with the separate representation gap of §3.6 still to be closed before any
   implementation can compute every legal sum.
2. **An ordinary apply drops every batch the snapshot already covers.**
   Measured: a package with 2 retained entries reloads with
   `Projection.Log()` = 0; delivering one fresh batch and saving on the apply
   path yields a package retaining exactly that one fresh batch, with both
   prior entries gone (§2.3). So no **prior covered** payload survives a
   writer-produced apply cycle, and a value lost to the old rule cannot be
   recovered from that package afterwards. Batches above the snapshot frontier
   are a different case and are not dropped (§2.3).
3. **A legacy snapshot cannot place its own winners, and the reason is
   information-theoretic.** The adopted write position needs the batch
   decomposition, and two different histories — one multi-operation batch
   versus N one-operation batches — serialize to identical bytes once the logs
   drop. Legacy packages therefore preserve the current value and the winner
   identity but cannot reproduce V5's late-concurrent resolution, so being
   single-actor is not proof of conformance. ED-0038 rules that they are read
   under a documented convention with V5 exactness explicitly not claimed, and
   that the first save rewrites positions that are exact **under the
   convention** — historical exactness is impossible, which is what the
   identical-bytes proof establishes (§3.5, §4.3).
4. **P's monotonicity rests on an obligation no compacted-package reader can
   check**, and `declaredPastSize` can overflow int64 within ratified limits
   (§3.3, §3.6). This document's earlier claim that an omitted own-actor
   frontier entry is benign was wrong and is corrected in §3.3.

Everything below is derived from this repository's own specifications
(`docs/FORMAT-SPEC.md` §4, §5, §7, §8, §9.3; `docs/decisions/0007`) and its
code. No source, identifier, layout, test, or data format from any other
application was consulted (`docs/CLEAN-ROOM-POLICY.md`).

---

## 1. What today's code decides, exactly

`internal/document/projection.go` resolves a `setFields` write through
`winnerSet.dominates` (:721), which is `k.compare(cur) > 0` on
K = (operationCounter, actorId) and nothing else, and `winnerSet.record`
(:708) keeps the greatest K for the same field. The two predicates are
identical, so **value and key stay aligned**: a package written by this code
holds, per field, the value of the greatest-K write together with that same
write's key. It does not hold a lower-K value under a stale key.

What is therefore absent from such a package is the value of any write that
was *causally later* but carried a smaller K — precisely the write ED-0032
ruled should have won. That value exists only in the writing operation's
payload.

A create writes every mergeable field of its payload
(`recordWinners(op.EntityID, mergeableFieldNames(ent), k)`), so a created
entity's fields all carry the create's K until something overwrites them.

## 2. What a package physically carries

### 2.1 Snapshot (`snapshot/state.json`)

`internal/persist/wire.go` `buildSnapshotJSON` (:650-740) writes exactly:

| member | shape |
|---|---|
| `entities` | entityId → §2.3 entity object |
| `frontier` | actorId → int64 watermark (S2) |
| `winnerKeys` | `"<entityId>.<field>"` → `{operationCounter, actorId}` (S3) |
| `createdKeys` | `"<entityId>."` → `{operationCounter, actorId}` |
| `tombstones` | entityId → `{deletingKey:{operationCounter, actorId}}` (S4) |
| `batchMetadata.recentBatchIds` | array of batch ids (S5) |

There is **no per-key frontier, no ordering position, and no
writer-semantics marker**. The manifest's `formatVersion` is the string
`"0.1"` for every writer, old or new. §5.1 already states the consequence in
its own words: "pre-horizon winner keys, discarded loser values, and
historical states are recoverable from it alone by no reader."

So the only causal information a compacted package retains about a winning
write is its **identity** — the pair (operationCounter, actorId).

`FromSnapshot` (`internal/document/snapshot.go:121-135`) validates a winner or
created key only for a non-zero actor and a counter ≥ 1. It does **not**
require the key's actor to appear in the frontier, nor its counter to lie at or
below that actor's watermark. §4.2 depends on this.

### 2.2 Batch entries (`ops/<counterBase>-<actorId>.json`)

§7.6 serializes each batch as the §4 batch shape, and §4.9's normative example
carries `frontier` with the writing actor's own prior counter (41 for a base of
42). The reader rebuilds that frontier verbatim (`frontierOf(d)`, called at
`internal/persist/reader.go:459` and `reader_helpers.go:164`) before
constructing `document.Batch`.

`frontier` is nevertheless **optional in the wire parse**: `wire.go:456` reads
`fv, _ := get("frontier")` and leaves the map empty when the member is absent,
so a batch declaring no causal past at all is accepted today.
`internal/document/op.go` `validateFraming` requires neither closure nor an
own-actor entry either; it rejects only a zero actorId, a negative counter,
and (in `NewBatch`) a frontier claiming the batch's own reserved range. This is
the shape of reader laxity ED-0034 recorded for `assetRoot` — a parser
exemption standing in for a normative requirement — and it matters more under
P than under K, because P reads the frontier as a *quantity* rather than as an
ordering hint (§3.4).

§7.6 also already requires the check §3.3 shows cannot always be performed:
"inconsistent frontiers ... reject `malformed-framing`" before replay. Nothing
on the `Load` path performs a closure check today — `checkReservedRanges`
covers B3 ranges, and `Deliver`'s `ErrCausalPrerequisiteMissing` covers
*missing* prerequisites, not *under-declared* ones.

### 2.3 What an ordinary apply retains — measured

Probe run in this worktree on 2026-08-26 against `3e841d5`, as a transient
test in `internal/persist` deleted immediately afterwards (tree clean before
and after; it asserted nothing about desired semantics). A first version of
this probe called `BuildPackage` twice without delivering anything, which
`r0011` correctly identified as a no-op simulation: `internal/cli/apply.go`
returns without writing when the effective batch set is empty (:396-400). The
corrected probe delivers one fresh batch, which is what a real apply does.

History: actor X creates a level at counter 1 (batch `…dd01`); actor Y renames
it at counter 1 with frontier `{X:1}` (batch `…dd03`), i.e. causally later.
Then one fresh batch from actor Z (`…ee02`) is delivered to the reloaded
projection and the package is written the way `apply.go:415` writes it.

| step | observed |
|---|---|
| package written with `RetainedBatches: proj.Log()` | 2 entries: `…dd01`, `…dd03` |
| reload of that package | `view.Retained` = 2, **`view.Projection.Log()` = 0** |
| after delivering one fresh batch | `Log()` = 1, containing **only** `…ee02` |
| second package, written as `apply.go:415` does | **1 entry: `…ee02`** — both prior entries gone |

Mechanism, all in delivered code: `SaveDocument` writes `in.State`, whose
frontier already covers every retained batch; on reload
`document.ReplayFromSnapshot` calls `Deliver`, whose watermark classification
skips every batch at or below the per-actor watermark (I2) so none of them
enters `Projection.log`; `apply` then writes `proj.Log()`, which holds only
what that invocation delivered.

The accurate statement is therefore: **an ordinary apply drops every batch the
snapshot already covers and retains only the batches delivered in that
invocation.** No frequency claim is made about how often a given package has
been applied to. Two consequences decide §4:

1. No **prior covered** payload survives a writer-produced apply cycle, so
   reconstructing a value from the retained log is possible only for a package
   that has not been applied to since the operations were written. This says
   nothing about an arbitrary package: one may legitimately hold retained
   batches **above** its snapshot frontier, which replay does not skip and an
   apply does not drop.
2. A policy of "refuse to compact unresolved multi-actor history" does not
   protect covered history from an ordinary apply, because that path drops it
   without going through compaction at all.

### 2.4 What ED-0039 changes about that behaviour

The measurement above is a fact about the delivered code. What it is *permitted
to be* changed on 2026-08-26. ED-0039 exists because one I5 class is not
fixable by bounded metadata — a late lower-position batch can flip the I5
verdict of an already-compacted batch, and re-deciding it needs that batch's
operations — so instead of narrowing a vector it tightens §5.2 S6 so that **a batch may be
dropped only when no undelivered batch can still be positioned before it**. S6
previously allowed dropping on either recorded peer acknowledgement covering
the range OR transfer of the successor snapshot to peers, and the
snapshot-transfer branch is no longer sufficient on its own.

**The acknowledgement branch is not self-sufficient either.** ED-0039's active
amendment narrows its own supporting inference, and this document follows the
amendment rather than the decision's original flat sentence. Receipt does not
establish the precondition: an acknowledging peer may still hold a batch it
**created before acknowledging** and never delivered, and that batch has a
smaller causal past than the acknowledged range, so it sorts *before* it. An
acknowledgement establishes the drop condition only where the sync protocol
attests three things — **every capable peer has acknowledged**; **each
acknowledging peer has delivered everything it created before acknowledging**;
and **every later-created actor observes the range before producing batches**.

**v0.1 defines no sync protocol and therefore supplies no such attestation, so
a v0.1 writer cannot drop from a synced document on acknowledgement alone.**

A reader of §2.3 must not take today's drop behaviour as still permitted:

- for a **local-only** document, §5.2 S7 governs and nothing here changes;
- for anything **synced**, no v0.1 drop is available at all — neither branch of
  the old S6 supplies the ruled precondition.

The ruled precondition itself is unchanged and remains normative; what narrowed
is only the inference from acknowledgement to it. ED-0039 moves no §4.7
vector — V6 holds exactly as written, which is why the drop condition was
tightened instead — and it does not touch §4.4's multi-actor fail-closed rule.
The blocked-compaction observable is also now stated testably rather than as an
exhortation: a **writer or compaction diagnostic MUST be emitted when a
requested or policy-driven drop is blocked, naming the reason and the
retained-log consequence** (ED-0037 comment (s)).

Two compounding size consequences, kept distinct and deliberately **not
quantified**:

- retaining logs longer grows the **retained log, and so the package**;
- the new encoding grows the **snapshot**: the shared `batchPositions` table
  plus the three position maps and per-entity lifetime metadata (§5), which the
  record states makes the
  pre-ED-0037 Part A budgets further out of date without changing the
  conclusion that they must be re-measured rather than re-derived.

Both land on issue 0035's budgets and issue 0018's publication of them, and
measuring either is issue 0035's work to redo rather than this document's to
estimate. The active record also fixes the boundedness statement this document
must not overclaim: the **number** of retained positions is bounded by state
(O(entities + fields)), the **bytes** are not, L23 bounds the total, and a
document whose metadata will not fit L23 cannot compact — with that refusal
observable under S6.

Also noted and **routed rather than touched**: the two write paths disagree —
`internal/persist/smoke_test.go`'s round trip rebuilds from `view.Retained`
while `internal/cli/apply.go:415` rebuilds from `proj.Log()`. Whether an
ordinary save should preserve retained operations belongs to the retention and
compaction owners (issues 0027/0029/0031) with §5.2's four concerns, not to
this issue.

## 3. Can a reader derive P? Derivability and provability are different questions

ED-0037's P depends on a past size. `r0011` objected that frontier closure is
not reader-provable after compaction without retained or certified ancestry.
That objection is correct and it does **not** break the ruling, because P needs
the number to be *computable*, not closure to be *provable*. The two are
separated throughout.

### 3.1 declaredPastSize is computable for any batch present as an entry

It is the sum of the batch's own declared frontier counters, and B3's
contiguous, gapless per-actor ranges are what make a counter stand for a count.
The batch entry carries that frontier and the reader already parses it (§2.2).
So for **every retained batch, in a compacted package or not**, the leading
component of P is determined by data the reader holds — no ancestry, no
certification, no other entry consulted. There is no *derivability* gap here.

Two other gaps are real and live elsewhere: whether that number equals the true
`causalPastSize` is §3.3's question, and whether the current types can
represent it at all is §3.6's. Neither makes the number undeterminable from the
bytes; both must be closed before an implementation can compute it for every
legal frontier.

### 3.2 It is not computable for a write that survives only as a key

A compacted write's frontier is not retained (§2.1), so neither its declared
nor its causal past size can be recovered from the snapshot. A field's winner
therefore cannot be positioned against a newly delivered write unless the
**position itself is retained** — which accepted §5.1 now specifies as the
shared `batchPositions` table plus `{batch, operationIndex}` references (§5),
the shape ED-0037's consequence 1 directed, whose byte cost is routed to issues 0035 and 0018 (ED-0037 comment
(a): the pre-ED-0037 Part A measurements must be re-measured once the new
encoding lands).

One narrower route exists: if the retained log still contains the batch whose
reserved range covers a key's (counter, actor), that batch's own frontier gives
its declared past size exactly. §2.3 bounds how often that holds, and §4.3
shows it is not sufficient for migration even when it does.

What *does* survive in a legacy single-actor snapshot is weaker and worth
stating exactly, because it is the reason ED-0031's conclusion stands
(ED-0037 comment (a)): the winner **identity** is still correct, because within
one actor a greater operation counter always means a greater position — a later
batch has a strictly greater past size, and a later index within one batch has
a greater position — and that holds whether or not the own-actor frontier entry
is present. Those packages name the right winners; they cannot *place* them
against a future concurrent writer from another actor.

### 3.3 Monotonicity needs closure, and snapshot-only validation cannot establish it

P is strictly monotone along causal edges **when frontiers are transitively
closed**. Derivation: let w₂ observe w₁. If the actors differ, closure gives
frontier₂[actor₁] ≥ end₁ > frontier₁[actor₁], and every other entry of
frontier₂ is at least frontier₁'s, so past(w₂) > past(w₁). If the actor is the
same, B3 contiguity gives frontier₂[actor] ≥ base₂−1 ≥ end₁ > frontier₁[actor],
with the same conclusion.

Without closure, P can invert a genuine causal pair. Minimal case, accepted by
today's validation: `b1` = actor A, base 1, frontier `{B:5}` has P = (5,1,A);
`b2` = actor A, base 2, frontier `{}` has P = (0,2,A), so `b2` sorts before its
own predecessor.

**An omitted own-actor entry is not benign.** This document earlier claimed it
was, and that claim was wrong; ED-0037 comment (e) records the correction.
Omitting the own-actor entry under-reports the past size by the actor's whole
prior stream, which inverts *cross-actor* comparisons: a batch whose closed
past is 45 (own 40 plus 5 observed) reports 5 with the entry omitted, and loses
to a concurrent batch at 30 that it should beat. The only accurate residual
claim is that an actor's own-stream **order** survives the omission, because
`counterBase` is P's second component. The own-actor entry must therefore be
required like any other **wherever the actor has prior operations** — that is,
whenever `counterBase > 1` — and its omission there treated as a closure
violation. A first batch has no prior own stream and is not obliged to
serialize a zero entry.

Four validation levels, which the specification should not conflate:

| level | can it establish closure? |
|---|---|
| semantic requirement (F1's "observed") | states the obligation; proves nothing |
| author-time validation | yes — the author holds its own applied history |
| full-log validation (reader, uncompacted) | yes for batches whose observed ancestors are all retained: each observed batch's frontier must be pointwise ≤ the observer's |
| snapshot-only validation (reader, compacted) | **no** — the ancestors' frontiers are exactly what compaction removed |

§4.3 row 4 and §7.6 both already make an inconsistent frontier a pre-replay
`malformed-framing` reject. Under P the reader of a compacted package cannot
detect the inconsistency that matters, so that reject is unenforceable there.
The consequence is a deterministic but wrong order, never a crash.

### 3.4 P promotes the frontier from an ordering hint to a value-deciding field

Today the frontier decides holding and canonical order only; values are decided
by K, which an author cannot inflate (B3 fixes its own counters). Under P the
frontier decides which concurrent value survives, and an author chooses its own
frontier. Inflation is bounded — `chainAndOrderOver`'s `coveredThrough` check
refuses a batch claiming counters the receiving replica cannot cover
(`ErrCausalPrerequisiteMissing`) — but within that bound an actor that always
declares the largest legal frontier wins every concurrent conflict, and one
that under-declares loses conflicts it should win. That is the accepted
user-facing semantics of ED-0037 ("whoever had seen more of the document's
history wins"), stated here in its adversarial form because §9.1's trust
boundary treats the log as untrusted input and currently says nothing about the
frontier being value-deciding. Recorded for §9's owner; no change proposed.

### 3.5 The write position, as settled — and the batch decomposition that is not recoverable

ED-0037 defines P per **batch**: (causalPastSize, counterBase, actorId). A
write is an operation, so a per-write comparison needs one more component, and
ED-0037 comment (b) settles it:

> **write position = (causalPastSize, counterBase, actorId, operationIndex)**

The leading three components are the ruling verbatim; because they are constant
within a batch, all of a batch's writes sort contiguously and interleaving is
structurally impossible, which is what B1 requires. Same-batch ordering by
index is what LWW1 already demands of one actor's ordered writes.

Two alternatives were considered and both are closed:

- **Batch past then LWW1's own K** — `(past, operationCounter, actorId)`. Not a
  completion but an **edit**: it replaces the ruling's `counterBase` component,
  contradicts the batch canonical order, and lets operation positions
  interleave batch ranges (ED-0037 comment (b)).
- **Per-write past** — `past = sum(frontier) + operationIndex`. This pair
  recommended it, because it restores `causalPastSize = operationCounter − 1`
  for the single-actor case and makes the two histories of §3.5's ambiguity
  genuinely equivalent rather than merely indistinguishable. It is **rejected**
  (ED-0037 comment (c)) for breaking B1: batch A with frontier sum 0 and three
  operations yields per-write pasts 0, 1, 2 while a concurrent batch B with
  frontier sum 1 yields 1, so B's write sorts *between* A's —
  `(0,1,A,0), (1,1,A,1), (1,2,B,0), (2,1,A,2)`. Atomicity was preferred over
  recovering a derivation.

**What that costs, and it is information-theoretic.** Under the adopted
position, a legacy snapshot cannot place its own winners, because the batch
decomposition is not in the bytes. `internal/sizecorpus/scenario.go:326-346`
builds *every* object create of a scenario into ONE
`document.NewBatch(BatchID, ActorID, 1, nil, ops...)` — counterBase 1, **nil**
frontier, N operations — so `unfurnished-1000-v1` has a batch past size of 0
while its snapshot keys carry `operationCounter` 1..N, and it retains zero
batch entries. `empty-v1` is a single operation and is unaffected.

`r0011`'s construction generalises that: H1 is one base-1 multi-operation batch
creating N entities (what the generator actually does); H2 is the same creates
as N one-operation batches at counters 1..N. Once the logs drop (§2.3) the two
snapshots are **byte-identical** — same entities, frontier `{A:N}`, keys
`(c, A)`, no retained batches — yet the operation at counter *c* has position
`(0, 1, A, c−1)` in H1 and `(c−1, c, A, 0)` in H2. For N=5 with the winner at
counter 4, a later concurrent writer with past size 0, 1 or 2 beats H1 and
loses to H2. No reader can tell the two apart, so legacy bytes preserve the
current value and the winner identity but **cannot reproduce V5's
late-concurrent resolution**. Being single-actor is not itself proof of
conformance.

### 3.6 declaredPastSize has no safe representation in the current types

§9.3 row L18 caps object members at 65,536, so a `frontier` may name 65,536
actors, each with a non-negative int64 counter; the exact sum can approach 2⁷⁹.
Every counter on the wire and in `opKey` is `int64`. Wrapping or saturating
would change ordering, which is the one thing P may not do.

**Now settled on the semantics, still open on the limit table.** Issue 0004's
accepted §4.4 F3 states it directly: `causalPastSize` "is bounded by L18
members of L13 counters, so it may exceed int64: implementations compare it
EXACTLY, and wrapping or saturating arithmetic is FORBIDDEN because it would
change the order. No serialized member carries the sum." So the implementation
obligation is exact comparison over a value that does not fit the current
types, and nothing on the wire carries the sum to overflow. What remains routed
is issue 0005's half — placement of a §9.3 row for the bound, since the accepted
text fixes the arithmetic but not the limit table. The §9 dimension still
holds: frontier width is author-chosen up to L18, and inflating it raises the
author's own position (§3.4).

## 4. Existing packages: the ruling, and what it does not cover

### 4.1 What is lost, precisely

Per field, a legacy package carries the greatest-K value with its matching
greatest-K key (§1). Under P the winner may instead be **any** write with a
lower K and a greater position — a causally later one, but also a merely
concurrent one, since ED-0037 reorders concurrent writes of unequal past size
too. Either way that write's value lives only in its operation payload. So for the
affected fields a legacy package holds **the wrong value**, and the right value
is present only if that operation is still retained — which §2.3 bounds
sharply. Independently of that, §3.5 shows the *position* of even a correct
legacy winner is not recoverable.

### 4.2 What is detectable

`formatVersion` is `"0.1"` for old and new writers alike, so version is not the
discriminator. Issue 0004's accepted §5.1 settles what is, and it is **member
presence**, not per-entry inspection.

1. **Which members are present?** The accepted representation is exactly one
   shape: the snapshot carries `batchPositions`, mapping each referenced
   batch's §7.6 entry stem (`<counterBase padded to 19 digits>-<actorId>`) to
   that batch's `frontier`; and `winnerPositions` keyed
   `"<entityId>.<fieldName>"`, `createdPositions` keyed by entity, and S4's
   tombstones each carry only `{"batch": "<stem>", "operationIndex": n}`.
   `counterBase` and `actorId` are read from the stem and `causalPastSize` is
   the sum of the referenced frontier's counters. Sharing through that table is
   the **only permitted encoding**, `batchPositions` must contain exactly the
   batches those maps reference, and a reference to an absent stem is absent
   required data and rejects (§9.4). A snapshot carrying the pre-ED-0037
   `winnerKeys`, `createdKeys`, or `deletingKey` members **instead** is a
   legacy snapshot. So the classifier is which member names appear, and the
   marker is unambiguous without reading a single entry.

   **What this document got wrong, kept because the pattern outlived it.**
   Earlier revisions described new-shape as widened entries *inside* the old
   maps and defined the marker by inspecting every entry for a leading
   component. That is not the accepted shape, and the correction is above. The
   finding that killed the first version of that rule still stands on its own
   and is preserved deliberately: defining the marker through a `winnerKeys`
   entry alone missed a snapshot with **no** `winnerKeys` and old
   `createdKeys`, because **an empty map satisfies a per-entry test
   vacuously** — recorded as instance three of the vacuous-check pattern in
   **ED-0041**. The accepted classifier is not vulnerable to it, because member
   presence is not a per-entry predicate and an empty map is still a present
   member. The lesson transfers even though the classifier changed, and
   ED-0041's standing requirement — that any check relied on for conformance
   evidence be mutation-proven, with the empty-collection case given its own
   probe — applies to whatever check implements this one.

   **One case the accepted text does not settle: mixed members.** §5.1 says a
   snapshot carrying the old members "instead" is legacy, which reads as
   exclusive, but it does not say what a snapshot carrying BOTH new and old
   members is. This document's posture is fail-closed — such a snapshot is
   neither new-shape nor eligible for §4.3's convention, because that
   convention is total over what it covers or does not apply at all — and this
   is recorded as a **wording question routed to §5.1's owner**, not as a rule
   this pair is entitled to declare. It is queued as ED-0040 item 10
   (`5833d52`), which records the fail-closed posture as the natural candidate
   and, explicitly, as this project's reasoning rather than a ruling.

   **What rejects, stated as the accepted encoding actually works.** Earlier
   revisions of this document said a snapshot "omitting the leading component"
   is non-conforming. That is wrong: accepted S3 deliberately serializes NO
   leading scalar, deriving `causalPastSize` by summing the referenced
   `batchPositions` frontier. What rejects is incomplete position metadata — a
   position missing its required `{batch, operationIndex}`, a reference to a
   stem absent from `batchPositions`, or a `batchPositions` entry missing the
   frontier it must carry — each of which is absent required data under §9.4.
   A reader MUST NOT synthesise any part of a position to fill such a gap;
   only the explicit ED-0038 legacy mode of §4.3 may assign a synthetic
   position, by the documented convention and labelled as such, and the save
   that follows writes new-shape metadata.
2. **How many actors — and a normative conflict this document must not paper
   over.** Accepted §5.1 classifies legacy mode by the actors **in the
   frontier**: "with exactly one actor in its frontier" it is read under the
   convention, and "with two or more actors in its frontier" it is fail-closed.
   This document previously said the actor universe must be the **union** of
   the frontier's actors and the actors named by every winner, created, and
   tombstone key. Those two rules disagree, and the disagreement is reachable
   rather than theoretical: `FromSnapshot`
   (`internal/document/snapshot.go:121-135`) validates a winner or created key
   only for a non-zero actor and a counter ≥ 1, requiring neither that the
   key's actor appear in the frontier nor that its counter lie at or below that
   actor's watermark. A legacy snapshot with one frontier actor and key actors
   outside the frontier would therefore be read under the single-actor
   convention while demonstrably carrying more than one actor's writes.

   **This document does not override the accepted text.** The frontier-count
   rule is what §5.1 states and it governs; the conflict is **identified and
   routed** — either §5.1's legacy classification should read over the actor
   universe rather than the frontier alone, or `FromSnapshot`'s seam should
   reject keys naming actors absent from or uncovered by the frontier so the
   two coincide. Which of those is correct is for §5.1's owner and issue 0029's
   seam owner, not for this pair. It is queued as a **specification defect** —
   ED-0040 queue item 9, recorded at `eca7fcb` — which requires this document
   to state the conflict explicitly until an owner rules, and both sentences
   are cited above so neither is dropped. Note the conflict is specific to
   LEGACY classification: under the accepted shape the actor set is pinned by
   the stems in `batchPositions`, and an unreferenced entry or an absent stem
   already rejects.

### 4.3 Single-actor legacy packages: read under a documented convention (ED-0038)

The owner ruled that a legacy package is read under **one documented reading
convention**, that **V5 exactness is explicitly not claimed** for it, and that
the **first save rewrites positions that are exact under the convention** —
not historically exact, which §3.5 proves impossible — so the ambiguity cannot
propagate past the first write.

The convention, now accepted normative text rather than a proposal: a legacy
single-actor snapshot is read **as though each retained key were its own
one-operation batch**. Issue 0004's accepted §5.1 states all three components,
not only the first: `causalPastSize = operationCounter − 1`,
`counterBase = operationCounter`, and `operationIndex = 0`.

It has to cover **every** legacy member that the new shape replaces, not only
winner keys: `winnerKeys`, `createdKeys`, and `deletingKey` all become
positions — `winnerPositions`, `createdPositions`, and positional tombstones
carrying the live interval — so the convention supplies the four components
in memory for each of them and the first save then emits the new members plus
their `batchPositions` table (§5). The accepted text does not prescribe how a
reader materialises that table for synthesised positions, and this document
does not invent one. What it does require is totality: if any legacy member the
new shape replaces cannot be assigned a position under the selected
single-actor mode, package acceptance or the save must **fail**, rather than
converting some members and leaving others behind.

**The two gaps this document reported as open are now RESOLVED**, in issue
0004's accepted §5.1, and both took the posture this document argued for —
routed or fail-closed, never silently convention-assigned:

- **Empty legacy snapshot (the zero-actor case).** An empty legacy snapshot —
  empty entities, empty frontier, and no winner, creation, or tombstone
  entries, which is §7.5's brand-new document — "has nothing to place: it reads
  as the empty projection and no convention applies."
- **Legacy tombstones.** A legacy snapshot carrying ANY tombstone "is
  fail-closed unless the deleted id's creating batch is still retained",
  because the pre-ED-0037 shape records a `deletingKey` but, creation metadata
  being live-entity-only, no creation key for a deleted id, so I5's live
  interval cannot be reconstructed. The accepted text states the reason this
  document gave: no convention may invent it, because guessing the creation
  side would apply a late batch positioned before the true creation where full
  replay discards it whole — silently wrong, and worse than a refusal.

The live-entity-only treatment that causes it is the one this document observed
in the delivered code at `internal/document/snapshot.go:129` (§5).

Two things about that sentence should not be smoothed over:

- It adopts **as a convention** exactly the identity this document first
  asserted as **fact** and §3.5 shows cannot be derived. A future reader must
  not mistake the convention for a derivation; the honest statement is that the
  number is chosen, not computed.
- It is the conservative direction: it maximises a legacy winner's position, so
  an existing value is harder for a late concurrent write to displace — the
  safer default for a local-first application opening its own older file.

The two ED-0031-protected evidence packages (`empty-v1`,
`unfurnished-1000-v1`) remain **pre-ED-0037 development artifacts carrying no
conformance claim**. They are not re-labelled or regenerated; regenerating them
would invalidate issue 0035's measured budgets and would need the Linux
reference host ED-0030's open owner question still covers. The convention makes
them **readable only through this legacy mode** — not conforming, and not
readable as ordinary new-shape packages.

### 4.4 Multi-actor legacy packages: the convention does not extend

For a single actor the only unknown is how operations were batched. For several
actors the winner keys cannot be placed at all, and any convention would be
inventing cross-actor causal relationships that never existed. ED-0037 comment
(d) therefore governs unchanged:

- a field is **exactly migratable only** when the **full relevant write history
  for that field** is retained, or a certificate attests it;
- and the **package** is migratable only if, in addition, every required
  **creation position, delete position, and per-entity live interval** can be
  recovered or certified — accepted §4.5 and S1/S4 make all three load-bearing
  for the three-branch late-batch placement rule (§5), so a field-complete
  write history is not sufficient on its own. Where any of them cannot be
  established, the failure is **package-level** fail-closed, not per-field;
- otherwise the migration is **fail-closed**.

Retaining the old max-K winner's batch is *not* enough: it derives only that
candidate's new position, and cannot exclude a different write — lower K,
greater P — that lost under the old rule, was therefore never retained as a
winner, and whose value is gone. Under P that write may be the new winner.
Combined with §2.3's measurement, multi-actor legacy packages are in practice
**non-migratable rather than partially migratable**. Nothing in this document
calls per-field partial migration exact.

Reporting V5 as merely "unavailable" for such a field is **not** sufficient;
the migrate-with-full-history-or-fail-closed rule governs (ED-0037 comment
(i)). The rejection category, its `formatVersion` implications, and the
diagnostic token belong to issues 0005 and 0029; this pair proposes no category
name.

### 4.5 Why the arbitrariness is acceptable here

v0.1 is unreleased and the predecessor set is empty (issue 0018's audit,
ED-0026), so the convention's arbitrariness never reaches a user, and
rewrite-on-save bounds its lifetime to a single read. The alternative —
rejecting legacy outright — had cleaner semantics but broke
`internal/sizecorpus`'s `TestCommittedPackagesRoundTrip` and made its repair depend on a machine this
project no longer has.

## 5. What changes in `internal/document` under P (preparation only)

Issue 0004's text is now **accepted** (integrated on main by `fa7e847`), so
ED-0036's halt condition is discharged — but **ED-0040 leaves this
implementation queued and unstarted**, so nothing below is a plan this pair
executes. It is recorded so that whoever resumes the queue starts small and
reviewable, and every rule it summarises now has accepted normative text
behind it rather than a decision comment:

- a position type beside `opKey` (`projection.go:71`) carrying
  (causalPastSize, counterBase, actorId, operationIndex), under the exact
  arithmetic §3.6 records;
- the past size computed once per batch from `Batch.Frontier`; the fourth
  component is the operation's index, not its counter;
- `winnerSet`'s two predicates (`dominates` at :721 and `record` at :708)
  collapse into one comparison on position; they must move together, because
  the retained position has to be the position of the write that produced the
  value;
- the canonical order in `chainAndOrderOver` (:373; ready-set sort at :457)
  sorts **batches** by the leading three-component batch P
  (causalPastSize, counterBase, actorId) instead of by
  `(counterBase, actorId)`, while the causality edges themselves are
  unchanged. `operationIndex` is **not** part of the batch sort: it is the
  per-write refinement that lives in the winner metadata and orders writes
  within one batch;
- **The snapshot side is a NEW encoding, not widened old maps.** This document
  earlier described the change as `State.WinnerKeys` and `State.CreatedKeys`
  widening in place through the existing seam. That is not what issue 0004's
  accepted §5.1 specifies, and the outline is corrected here. The accepted
  representation is exactly one shape, and it is shared rather than repeated:
  - `batchPositions` maps each referenced batch's §7.6 entry stem
    (`<counterBase padded to 19 digits>-<actorId>`) to that batch's `frontier`,
    in the S2 shape with per-actor counters within L13;
  - `winnerPositions` (keyed `"<entityId>.<fieldName>"`), `createdPositions`
    (keyed by entity, S1) and S4's tombstones carry only
    `{"batch": "<stem>", "operationIndex": n}`;
  - `counterBase` and `actorId` are read from the stem, and `causalPastSize` is
    the sum of the referenced frontier's counters, **so no member carries an
    integer outside L13/§3.2** — which is how the accepted text keeps §3.6's
    overflow off the wire entirely, leaving exact comparison as an in-memory
    obligation only;
  - `batchPositions` contains EXACTLY the batches those maps reference: no
    unreferenced entries, and a reference to an absent stem is absent required
    data and rejects (§9.4). Sharing through this table is the ONLY permitted
    encoding, because a second one would fork readers (§9.4 Rule T1).

  So the implementation replaces the three `opKey` maps with position maps over
  a shared batch table; it does not widen `writeOpKeyMap` in place. The old
  member NAMES are what mark a legacy snapshot (§4.2), which is why the
  detection rule is a name distinction and not a per-entry shape test.
- **The `CreatedKeys` question this issue routed is RESOLVED by accepted S1**,
  which enumerates the member and states that "the retained datum is a
  POSITION, not a bare `(operationCounter, actorId)` pair" — citing this
  issue's review as the source of the question. The reason it is load-bearing
  is unchanged: a per-field max fold does not capture V7/I5 *whole-batch*
  staleness. A snapshot holds entity E (created by batch C) and field F; a late
  multi-operation batch M with a lower position canonically precedes C and sets
  both E and F. Full replay discards M entirely as stale, because E does not
  exist when M applies, so F is unchanged; a seeded replay sees E present,
  suppresses M's write to E by winner position, and may still apply M's write
  to F.
- **S4 tombstones carry the deleting position AND the creation position**, so
  the live interval `[creationPosition, deletingPosition)` survives deletion.
  The symmetric case is the reason: entities E and F live, `M = [set E, set F]`,
  `D = delete(E)`, with `create(E) < P(M) < P(D)`. Full replay applies M — E is
  live at M's position — so F changes and D then tombstones E; a snapshot
  covering D holds E tombstoned, so a late M is discarded whole as stale and F
  never changes. Structurally confirmed here against the delivered code: the
  old S4 wire record carries only `deletingKey`
  (`internal/persist/wire.go:304-306` parsing, `:722` writing) in the narrow
  `{operationCounter, actorId}` shape, so neither the delete position nor the
  live interval is reconstructable from it — and creation keys are treated as
  live-entity-only, a creation key for a non-live entity being rejected at
  `internal/document/snapshot.go:129`, which is exactly the assumption the live
  interval breaks and the reason accepted §5.1 makes a legacy snapshot carrying
  any tombstone fail-closed (§4.3).
- **Late-batch placement becomes three branches, not two:** *before create* →
  stale, discard whole; *within the live interval* → applies, with writes to
  the deleted entity's own fields subsequently superseded per LWW3; *after
  delete* → stale per I3, no resurrection.
- **Per-target and mixed-batch semantics — this document routed them as open;
  issue 0004's accepted §4.5 now SETTLES them, and the rules are these.**
  Presence is evaluated at the OPERATION's write position, not its batch's, so
  an entity created by an earlier operation of the same batch is present for
  the later ones (B4). After compaction the same test runs against the retained
  live interval `[creationPosition, deletingPosition)` from S1 and S4. The
  interval test governs only operations REQUIRING a pre-existing target
  (`setFields`, and any later kind carrying that requirement) and classifies
  each such target independently at that operation's own position, three ways:
  before creation, within the live interval, after deletion. The two kinds that
  DEFINE an interval are not classified by it — `createEntity` establishes
  existence at its own position and so is never stale for want of its own
  target, with same-id collisions resolving under LWW4 and no resurrection past
  a tombstone except E6's authorized restoration; `deleteEntity` closes the
  interval against a live target and is stale on branch 1 or 3 terms otherwise.
  The BATCH outcome then follows from its operations and never from a single
  classification of the batch: if ANY operation is stale the WHOLE batch is
  discarded, including operations whose own targets were live, because B1
  forbids partial application. Only when NO operation is stale does the batch
  apply, and then its writes to targets a canonically later delete has
  tombstoned are superseded by that delete (LWW3) and produce no field state,
  while its writes to other targets take effect normally. The accepted text
  also names the failure mode explicitly: the two-way present-or-absent reading
  makes branch 2 indistinguishable from branch 3 and silently changes an
  unrelated field's value, and is not conforming.
- **Retention size, stated as the active record states it and no stronger:**
  the **number** of retained positions is O(entities + fields) — two positions
  per entity and one per field — so the boundedness escalation trigger did not
  fire; the **bytes** are not bounded by that, L23 bounds the total, and a
  document whose metadata will not fit L23 cannot compact, observably under S6.
  The `batchPositions` table shape the active record discusses is
  O(positions × actors); this document records that figure as the record's and
  does not extend it into a design.
- `lwwHorizon` (§5.1 S3, provisional, fixation open) bounds which positions a
  snapshot must retain, exactly as it bounds winner keys today.

Two behaviours are **not covered by a rule that governs them**, and the first
matters because it shares a call site with one that is. The comparison at
`projection.go:553` (`prev.compare(k)`) handles both LWW4's concurrent create
collisions, which §4.6 does govern, and the *causally later* duplicate create,
which cannot be locally valid under O4 and so exists only in a malformed or
hostile log. The second is **referenced but not covered**: accepted §4.5 sends
any same-id create collision to LWW4, while LWW4's own wording is scoped to two
CONCURRENT creates each locally valid under O4, so the causally ordered case is
pointed at a rule whose text does not reach it. That gap is a wording question
for §4.6's owner, not an implementation licence, and it is queued as ED-0040
item 8 (`0c53b12`) with the orchestrator's note that "probably unreachable" is
the reasoning that produced this session's other defects. An implementation
converting the covered concurrent path to positions must therefore not silently
invent semantics for the uncovered case in the same edit; leaving the whole call
site untouched stops being available once create collisions move to positions.
The latter of the two is the absence of a diagnostic for a discarded concurrent
field write, which §4.6 does not require.

## 6. Families eliminated before the ruling

Recorded once, for the reader who finds this file after ED-0036:

- **Causal fold retaining the value-writer's key** — `r0011`'s counterexample:
  writes R, P, D on one field with K(P) < K(D) < K(R), P observing R, D
  concurrent with both; the snapshot retains P's key, so late D beats it, while
  the never-compacted replica orders D first and D loses. Any rule of the form
  "dominance first, else K" is non-transitive, which is the same
  non-transitivity ED-0036 turned into F3's cycle R → P → D → R.
- **Causal maxima (antichain) + greatest K** — eliminated by the impossibility
  ED-0037 records: it requires retained losing **values**, because a later
  causally ordered write can dethrone the winner and hand the field to a
  previously losing candidate. That contradicts LWW2's "the losing concurrent
  value is DISCARDED" and §4's lossy, not-a-CRDT positioning.
- **Greatest-K barrier with causal override** — survives the R/P/D shape and
  needs no new snapshot field, but it is still a pairwise rule layered over an
  order, and the ruling superseded it before this pair finished testing it
  against downward-closed coverage subsets. Recorded as superseded, not
  refuted.
- **"Any linear extension"** — non-viable independently of the rest: distinct
  extensions of one delivered set yield distinct projections, contradicting
  F3's own claim that the canonical order is a function of the delivered SET,
  and contradicting V6.
- **Explicit logical-clock or causal-height field** — rejected in ED-0037 for
  requiring a new batch field; P needs none.
- **Per-write past size** — this pair's own recommendation, rejected in
  ED-0037 comment (c) for breaking B1 (§3.5).

## 7. Items routed elsewhere by this document

| finding | owner |
|---|---|
| `declaredPastSize` overflow — **arithmetic RESOLVED** in issue 0004's accepted §4.4 F3 (compare exactly; wrapping or saturating forbidden; no serialized member carries the sum); the §9.3 row placing the bound is still owed (§3.6) | 0005 (limit-table placement) |
| Closure as a normative requirement, including the own-actor entry, and per-level reader duties (§3.3) | 0004 (§4.4/§4.5), 0005 (§7.6) |
| `frontier` optional in the wire parse where §4.9's shape carries it (§2.2) | 0005 / 0029 |
| Frontier becomes value-deciding; §9.1 trust boundary (§3.4) | 0005 / §9 owner |
| Ordinary apply drops prior covered entries; the two write paths disagree (§2.3) | 0027 / 0029 / 0031 |
| Winner/created key actors not required to appear in the frontier (§2.1) | 0029 (`FromSnapshot` seam), 0004 (§5.1 wording) |
| Legacy rejection category and diagnostic token (§4.4) | 0005 / 0029 |
| Position-map **logical shape** — **RESOLVED** in accepted §5.1: one shape only, `batchPositions` plus `winnerPositions` / `createdPositions` / positional tombstones carrying `{batch, operationIndex}` (§5) | §5.1 half closed (0004) |
| Physical **placement and terminology** for those members in §7.5, which says nothing about them — still open | 0005 (§7.5) |
| ~~`CreatedKeys` omitted from §5.1's S1–S5 enumeration~~ — **WORDING RESOLVED** in accepted S1, which enumerates the member, makes its datum a position, and cites this issue's review as the question's source; the implementation remains to be written (§5) | wording closed (0004); implementation 0011 (queued, ED-0040) |
| §4.6 LWW4 scoping mismatch: §4.5 sends any same-id create collision to LWW4 while LWW4 is scoped to concurrent, locally-valid creates (§5) | 0004 — ED-0040 queue item 8 (`0c53b12`) |
| Legacy-mode selection by frontier actor count versus the actor universe, reachable because `FromSnapshot` accepts uncovered key actors (§4.2) | 0004 — ED-0040 queue item 9 (`eca7fcb`); 0029 for the seam half |
| Mixed old and new position members in one snapshot: accepted §5.1 says "instead" without stating the both-present case (§4.2) | 0004 — ED-0040 queue item 10 (`5833d52`) |
| ~~Per-target and mixed-batch semantics for the three I5 lifecycle branches~~ — **RESOLVED** in issue 0004's accepted §4.5 (per-operation classification at the write position, create-establishes, delete-closes, whole-batch any-stale) (§5) | closed; was 0004 (ED-0037 comment (t)) |
| ~~ED-0038's convention derives neither tombstone live intervals nor the zero-actor case~~ — **RESOLVED** in issue 0004's accepted §5.1: an empty legacy snapshot reads as the empty projection with no convention applied, and a legacy snapshot carrying any tombstone is fail-closed unless the deleted id's creating batch is retained (§4.3) | closed; was 0004 (ED-0037 comment (t)) |
| ~~Creation keys treated as live-entity-only (`snapshot.go:129`), which the live interval breaks~~ — **WORDING RESOLVED** in accepted S1, which retains `createdPositions` for TOMBSTONED entities as well as live ones and names live-entity-only treatment as "exactly what collapses those branches"; the delivered seam still has to be changed (§5) | wording closed (0004); implementation 0011 — **queued** under ED-0040, not blocked |
| The new encoding adds snapshot bytes and ED-0039 adds retained-log bytes, so issue 0035's Part A budgets are pre-ED-0037 measurements | 0035 / 0018 (ED-0037 comment (a) and the ED-0039 amendment) |
| ED-0039's tightened S6 drop condition, its **narrowed** acknowledgement inference, and the blocked-compaction diagnostic — **§5.2 TEXT RESOLVED** in accepted §4/§5; the condition and the diagnostic still have to be implemented (§2.4) | text closed (0004); implementation 0027 / 0029 / 0031 |
| Mutation-proving the **accepted member-name classifier** of §4.2 — including a snapshot whose old member maps are present but EMPTY, and a snapshot carrying both old and new members — per ED-0041's standing requirement. The per-entry `winnerKeys` classifier this document originally proposed was rejected with the encoding and is not what needs proving (§4.2) | whoever implements the classifier (0011 implementation / 0029) |

## 8. Reproduction

Baseline for every claim: worktree `issue/0011-lww-causal-ordering` at
`3e841d5`, macOS/arm64, Go 1.27.0 with the module-cache toolchain first on
`PATH` per ED-0030. `./scripts/verify.sh` exits 0 on that tree.

Code claims are citations and need no run. The §2.3 measurement is reproducible
by building the history it names — actor X creates an entity at counter 1;
actor Y sets one of its fields at counter 1 with frontier `{X:1}` — then, in
sequence: `document.New().Deliver(create, later)`; `persist.BuildPackage` with
`State: proj.State()` and `RetainedBatches: proj.Log()`; `persist.Load` of
those bytes; `Deliver` of one fresh batch from a third actor to
`view.Projection`; and a second `BuildPackage` whose `RetainedBatches` is
`view.Projection.Log()`. The reloaded log is empty before the fresh delivery,
holds only the fresh batch after it, and the second package retains only that
batch. Tracking the old batch ids explicitly is what distinguishes "prior
entries dropped" from "history empty"; a probe that delivers nothing models a
path `apply.go:396-400` never takes.
