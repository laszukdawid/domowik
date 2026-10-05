# 0038: Session commit cost is cubic in history length

- **Phase:** 1
- **Priority:** High
- **Status:** Done
- **Depends on:** 0011

## Problem

Committing one command to a `Session` costs O(n²) in the number of operations
already in the log, so building a document of n operations costs O(n³). The
Go spike raised this as finding **F1** on 2026-08-25
(`spikes/0024-flutter-go-bridge/README.md`), attributed it to "retained-history
replay/baseline cost in Projection/Session", and left it undiagnosed. The Rust
rewrite carried it across unchanged: it bought a constant factor of roughly
2.4x and did not change the asymptotics.

Measured on this host (macOS arm64, release profile), one `Session::commit`
per wall onto one session:

| ops | Rust cumulative | Rust per-op | Go spike (F1) |
|---:|---:|---:|---:|
| 100 | 9.27 ms | 92.7 µs | ≈24 ms |
| 250 | 94.04 ms | 376.2 µs | — |
| 500 | 608.59 ms | 1.22 ms | — |
| 1000 | 4.31 s | 4.31 ms | ≈10.3 s |
| 2000 | 32.88 s | 16.44 ms | — |

Doubling n multiplies cumulative cost by ~7, which is n^2.8 — cubic within
measurement noise, so per-commit cost is quadratic.

The Go column is **shape evidence, not a comparison**: it was measured on
different hardware through a different boundary, and that spike disclaimed
comparative use. What it establishes is that the same superlinear growth was
present before the rewrite and survived it.

## Cause (diagnosed, unlike F1)

Three compounding costs, all in the local-commit path:

1. **`replay::apply_batch` clones the whole document AND the whole interval
   map for every batch.** The clone exists for atomicity — B1 requires a
   discarded batch to leave no trace — but it makes a single full replay
   O(n²) on its own.
2. **`Session::apply_local` replays the ENTIRE log to build its candidate**,
   then `reproject()` replays it again after the real delivery. Two full
   replays per commit.
3. **`self.log.clone()` per commit**, a deep clone of every retained batch.

## Why this blocks Phase 2

The Phase 2 exit is "a real apartment can be edited on reference
desktop/mobile devices". At 2000 operations a commit costs 16 ms — one
interactive frame, for one keystroke — and the cost is still climbing
quadratically. No amount of GUI work fixes that.

## Acceptance criteria

- [x] A committed benchmark measures per-commit cost against history length,
      so the asymptotics are visible and cannot silently regress.
- [x] Per-commit cost is independent of history length, or its dependence is
      measured, stated, and justified.
- [x] `apply_batch` no longer copies the document per batch, and atomicity
      (B1: a discarded batch leaves no trace) is proven preserved by test,
      including the partial-application case where a batch's later operation
      is stale after earlier ones applied.
- [x] A local commit does not replay history it has already projected. The
      justification — that a local batch's canonical position is strictly
      last, so it can never reorder against existing batches — is stated and
      tested.
- [x] Full replay from a stored log (the open path) is measured too; opening a
      large plan is the other user-visible instance of the same cost.
- [x] Every existing replay semantic is preserved: staleness classification at
      each operation's own write position (I5), LWW resolution (§4.6),
      authorized restoration (E6), tombstone retention (S4), and batch
      atomicity (B1).

## Notes

Issue 0011 owns replay correctness and is `In Progress (partial scope)`; this
issue owns its COST. They are separated because the correctness criteria are
met and the cost criteria were never written down — recording the work here
rather than widening 0011's criteria after the fact.

## Delivered — 2026-09-01

Measurements and the correctness argument:
[docs/evidence/0038-session-commit-cost/MEASURED.md](../../docs/evidence/0038-session-commit-cost/MEASURED.md).

Two of the three causes went: the pair of full replays per commit, and the
per-commit `log.clone()`. The marginal cost of one commit at 2000 operations
fell from 48.61 ms to 197.0 µs, and per-commit cost stopped being quadratic in
history length. The third — the per-batch document clone — was left in place
and recorded as a residual, with the reasoning for not spending the risk there
at the time.

## Delivered — 2026-09-12

The third cause removed. Same evidence document, "The fix, part two".

`replay::apply_batch` no longer copies the document. It mutates the projection
in place under a `BatchJournal` that records each displaced value — and, for a
deletion, the index it came from — and `rollback` undoes the entries in reverse,
restoring contents and collection order exactly. The journal is bounded by the
batch rather than the document.

`Session::commit_appended` then stopped copying the projection per command:
it applies the batch to the session's real projection and rolls back if any
check refuses, which is R8.2.2's "a rejected command changes nothing" at the
price of the command instead of the price of the plan. The wall-axis guard
reads its affected-wall set off the journal through a new
`wall_contact_refusals_among`, rather than diffing a retained copy of the
pre-batch document.

| Criterion | State |
|---|---|
| A committed benchmark measures per-commit cost against history length | **Met** — `crates/domowik-document/benches/session.rs`, compiled on every verification run |
| Per-commit cost is independent of history length, or measured and justified | **Met by measurement** — it is LINEAR in document size, not independent: 1,000 to 2,000 operations costs 1.95x. At 2,000 operations a commit is 202 µs, about 1.2% of an interactive frame. What remains is `Document::validate`'s duplicate-id pass and the `Vec` position scans, both stated below |
| `apply_batch` no longer copies the document per batch, B1 proven by test | **Met** — the journal, with the partial-application case covered by four tests across both discard paths and both admission refusals, and five mutations proven caught |
| A local commit does not replay history it has already projected | **Met**, with the canonical-position argument stated on `Session::apply_local` and both its cases mutation-proven |
| Full replay from a stored log is measured | **Met** — `open/2000` is 5.21 ms against a 200 ms budget, down from 29.14 ms |
| Every existing replay semantic is preserved | **Met** — 775 workspace tests green; the incremental-versus-replay invariant now also compares live intervals and commits multi-operation batches |

### What this did not make linear, and who owns it

`open` is still **quadratic**: 1,000 to 2,000 operations costs 3.99x. The
constant fell 5.6x, and that is all removing the copy could do. `Document`
stores entities in `Vec`s, so `insert_entity`, `remove_entity` and `entity` are
linear scans and a replay of n batches performs n of them. Per-commit cost is
linear for the same reason, plus `Document::validate`'s whole-document pass.

Indexing those collections by id belongs to the document model (issue 0010),
not to replay, which cannot change `Document`'s storage from outside. Both
paths are inside their budgets at twice the corpus target — 5.21 ms against
200 ms for `open`, 202 µs against a 16 ms frame for a commit — so this is
recorded as a known shape rather than raised as a new blocker. It is stated
here because the issue title says "cubic" and half of that is still true of
the open path's asymptotics.

### The advisory round produced no adversary

Codex `gpt-5.6-sol` at high reasoning effort was run once, narrowly scoped, and
returned `ERROR: Your workspace is out of credits` with no findings. Not
retried, per INSTRUCTIONS.md. The scrutiny was done here instead by writing the
tests it was asked for, which is how the affected-axis defect — judging a wall
against the previous operation of its own batch instead of against the
pre-batch state — was found and fixed before it shipped.
