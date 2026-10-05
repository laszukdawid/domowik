# Issue 0038 — session commit cost, measured

Host: macOS 25.6.0, arm64, release profile (`lto = "fat"`, `codegen-units = 1`).
One host throughout, two rounds of work: 2026-09-01 and 2026-09-12. Timings are
host-dependent and are not a pass/fail gate (ED-0070). Reproduce with
`cargo bench -p domowik-document --bench session`.

**Load conditions.** The 2026-09-12 round ran under the shared verification
slot, announced before and released after, with `CARGO_BUILD_JOBS=2`. The
machine was NOT idle: other agent sessions held checkouts on it throughout,
though no other heavy verification run was known to be in progress. No load
average was captured at the time, so treat the absolute figures as indicative
and the before/after ratios as the result — both sides of every comparison in
"The fix, part two" were measured minutes apart in the same session, by
criterion `--save-baseline` across the change, which is what makes them
comparable to each other rather than to another host or another day. The
2026-09-01 figures were recorded before this was tracked and their load
conditions are unknown; they are not compared against the 2026-09-12 ones.

## The finding

The Go spike recorded this as **F1** on 2026-08-25
(`spikes/0024-flutter-go-bridge/README.md`): repeated `Session.Execute` slowed
"superlinearly with executed-command count (~quadratic in this workload)".
It named the suspect — "retained-history replay/baseline cost in
Projection/Session" — and did not diagnose it. The Rust rewrite inherited the
defect unchanged, buying a constant factor of roughly 2.4x.

## Cause

Three compounding costs in the local-commit path:

1. `Session::apply_local` replayed the ENTIRE log to build its candidate
   projection, then `reproject()` replayed it AGAIN after delivering. Two full
   replays per commit.
2. `self.log.clone()` per commit, a deep clone of every retained batch.
3. `replay::apply_batch` clones the whole document and the whole interval map
   per batch, so one full replay is O(n²) by itself.

(1) and (2) were fixed on 2026-09-01. (3) was fixed on 2026-09-12 — see "The
fix, part two".

## The fix, part one (2026-09-01): stop replaying what is already projected

A locally minted batch observes the entire log, so its `causalPastSize` is
strictly greater than that of every retained batch — each of those excludes at
least its own operations from its own past. Its canonical position is therefore
strictly **last**, it cannot reorder against anything already projected, and
extending the current projection gives the same answer a replay would.

The argument fails in exactly one case: if the log HOLDS batches, delivering a
local one can promote them, and a promoted batch may sort anywhere. That case
keeps the replay, and
`a_local_commit_that_promotes_a_held_batch_replays_instead_of_appending`
constructs it so that taking the fast path would be observably wrong — the
held batch's prerequisite is the committing session's own next counter.

### Marginal cost of one commit (criterion benchmark)

| history | before | after | factor |
|---:|---:|---:|---:|
| 100 | 164.5 µs | 14.5 µs | 11x |
| 500 | 3.58 ms | 55.3 µs | 65x |
| 1000 | 12.98 ms | 100.9 µs | 129x |
| 2000 | 48.61 ms | 197.0 µs | **247x** |

Before: 20x the history cost 296x as much — clean quadratic. After: 20x the
history costs 13.6x as much, which is sub-linear over this range.

### Cumulative build cost, in the shape F1 measured

| ops | Rust before | Rust after | factor |
|---:|---:|---:|---:|
| 100 | 9.27 ms | 1.14 ms | 8x |
| 250 | 94.04 ms | 6.44 ms | 15x |
| 500 | 608.59 ms | 16.09 ms | 38x |
| 1000 | 4.31 s | 39.19 ms | 110x |
| 2000 | 32.88 s | 112.45 ms | **292x** |

Before, 20x the operations cost 3,547x as much. After, 99x.

#### On comparing to the Go spike

The Go numbers (100 ops ≈24 ms, 1000 ops ≈10.3 s) are **not** in the table
above, and the ratio against them is not stated as a result. They were measured
on linux/amd64 through a cgo boundary from Dart; these are macOS/arm64 in-
process. That spike's own README says its numbers "support no comparative
performance claims", and it is right.

What the Go numbers DO establish, and all this work relies on them for, is the
SHAPE: F1 reported ~quadratic growth from 100 to 1000 operations, and the Rust
core reproduced quadratic growth over the same range on its own host. The
defect was the same; the fix is evidenced by the before/after on one host.

### Interactivity

The Phase 2 exit is "a real apartment can be edited on reference
desktop/mobile devices". An interactive frame is 16 ms. Before, a single
commit at 2000 operations cost 48.6 ms — three frames, for one keystroke, and
still climbing quadratically. After, it costs 197 µs: about 1.2% of a frame.

## The fix, part two (2026-09-12): remove the per-batch copy

`replay::apply_batch` bought batch atomicity with a copy. It cloned the whole
`Document` and the whole interval map, applied the batch to the copy, and
installed the copy only once every operation had survived. B1 got exactly what
it asks for — a discarded batch leaves no trace — at a price of O(the document)
per batch, however small the batch.

It now mutates the projection in place and records a `BatchJournal`: for each
write, the value it displaced, and for a deletion, the index it came from.
Undoing the entries in reverse restores the projection exactly, collection
order included. The journal is bounded by the BATCH — k operations produce at
most k document entries and k interval entries — so the cost follows what the
batch touched instead of what the document holds.

Two things fell out of that. The commit path had been cloning the whole
projection per command for the same reason, so that a rejected command could
throw the copy away; `Session::commit_appended` now applies the batch to the
session's real projection and rolls back if any check refuses, and
`candidate_by_replay` is what remains of the old shape for the one case that
still needs a replay. And the wall-axis guard, which
compared the pre-batch document to the post-batch one to find the affected
walls, reads them off the journal instead — `wall_contact_refusals_among` takes
the set rather than rediscovering it, and `wall_contact_refusals` still computes
it by comparison for every other caller.

### What one commit allocates

Deterministic, unlike a timing, so this one IS a test:
`crates/domowik-document/tests/commit_allocation.rs`. Two sessions hold 400
entities each and differ only in how much heap each entity carries in an
`unknown` payload the commit never reads. Scanning costs the same in both;
copying costs the bulky one a payload per entity.

| fixture | before | after |
|---|---:|---:|
| 400 entities, no payload | 395,179 B | 19,969 B |
| 400 entities, 4 KB payload each | 4,174,507 B | 19,969 B |

The two figures after the change are the same number, not a rounding: what a
commit allocates no longer depends on what the document's entities weigh. Before
it was 2.5 copies of the 1.6 MB the fixture carried.

What a commit still allocates in proportion to the document is 43 bytes per
entity (11,353 B at 200 entities, 37,385 B at 800). That is id-sized, not
entity-sized: the duplicate-id set `Document::validate` builds, and the linear
position scans in `insert_entity` and `entity`. The test holds it under 64.

### Timings

Same host, same session, criterion `--save-baseline` across the change rather
than a table quoted from an earlier run. Mean estimates.

| benchmark | before | after | change |
|---|---:|---:|---:|
| `commit/100` | 19.12 µs | 15.90 µs | −16.8% |
| `commit/500` | 75.97 µs | 55.95 µs | −26.4% |
| `commit/1000` | 142.93 µs | 103.81 µs | −27.4% |
| `commit/2000` | 271.69 µs | 202.19 µs | −25.6% |
| `open/100` | 95.05 µs | 36.18 µs | −61.9% |
| `open/500` | 2.059 ms | 391.0 µs | −81.0% |
| `open/1000` | 7.614 ms | 1.303 ms | −82.9% |
| `open/2000` | 29.139 ms | 5.205 ms | −82.1% |

**The commit figure understates the change, and the reason matters.** The
benchmark's walls carry no uninterpreted data, so the copy it removed was a
memcpy of a few hundred kilobytes of numbers — cheap per byte. The allocation
table above is the same commit on entities that carry content, and there the
copy was 4.17 MB. A real plan sits between the two.

**`open` is still quadratic.** Doubling the history from 1,000 to 2,000
operations costs 3.99x, which is n² within noise; it was 3.83x before. What
changed is the constant, by 5.6x at 2,000 operations. `Document` stores entities
in `Vec`s, so `insert_entity`, `remove_entity` and `entity` are linear scans and
a replay of n batches scans n times. Removing the copy could not change that and
the issue said so in advance.

**Per-commit cost is linear in document size**, before and after: 1,000 to 2,000
operations costs 1.95x, against 1.90x before. The quadratic part went on
2026-09-01; what remains is `Document::validate`'s whole-document duplicate-id
pass and the same linear scans. At 2,000 operations a commit is 202 µs, about
1.2% of a 16 ms frame. Making it sublinear means indexing `Document`'s
collections by id, which is the document model's own decision (issue 0010) and
not something replay can do from outside.

## What proves the optimisation correct

- `the_incremental_commit_path_agrees_with_a_full_replay_at_every_step` —
  after EVERY command in a 120-step mixed stream (creates, field edits,
  deletes, undo, redo, and rejected resurrections), the session's projection
  must equal a full replay of its own log, discards included. Mutation-proven:
  removing the append makes it fail at step 1.
- `a_local_commit_that_promotes_a_held_batch_replays_instead_of_appending` —
  mutation-proven: forcing the fast path unconditionally makes it fail.
- The existing 385-test suite, unchanged and green.

For the journal (2026-09-12), in `replay.rs`:

- `a_batch_that_goes_stale_after_partial_application_leaves_no_trace` and
  `a_batch_malformed_after_partial_application_leaves_no_trace` — a batch that
  creates one entity, edits a second and deletes a third from the MIDDLE of its
  collection before its last operation fails. Both discard paths, and the
  deleted entity has to come back at its own index.
- `a_refused_level_deletion_after_partial_application_leaves_no_trace` and
  `a_refused_wall_contact_after_partial_application_leaves_no_trace` — the two
  admission refusals, which are raised only once the whole batch has already
  applied.
- `rollback_restores_deletes_replacements_and_within_batch_lifetimes` — every
  journal shape in one batch: a mid-collection delete followed by a create, a
  create that replaces a live id (LWW4), and a create and delete of the same
  fresh id inside the batch.
- `an_axis_is_affected_against_the_state_before_the_batch_not_its_last_edit` —
  reading the affected set off the journal has to compare a wall to its
  PRE-BATCH state, not to whatever the previous operation of the same batch
  left. Found by mutation: judging against the latest entry passed every other
  test while admitting an overlap it had just created.
- `a_wall_deleted_and_recreated_elsewhere_in_one_batch_is_judged_on_its_final_axis`,
  `a_wall_created_and_deleted_within_one_batch_occupies_no_axis` and
  `a_wall_returned_to_its_own_axis_within_one_batch_is_not_affected` — the batch
  shapes where the journal and a document-to-document diff could disagree.
- `the_incremental_commit_path_agrees_with_a_full_replay_at_every_step` now also
  compares the live INTERVALS to the replay's, and its stream commits
  multi-operation batches. A projection whose intervals drift misclassifies the
  next operation under I5 while still looking correct from outside.

Five mutations were introduced and each was caught: restoring a deleted entity
at the end of its collection instead of its index, skipping the interval
rollback, skipping the discard truncation, judging an affected axis against the
latest journal entry, and treating every touched wall as affected.

All 775 workspace tests pass, with `cargo fmt --check` and
`cargo clippy --workspace --all-targets -- -D warnings` clean.

### The advisory round failed

Codex `gpt-5.6-sol` at high reasoning effort was run once, pointed at
`replay.rs`, `wall_contact.rs` and the three changed session functions and
nothing else, and asked five questions: whether `rollback` restores contents and
order exactly, whether every mutation has a journal entry and every entry a
mutation, whether the affected-axis set derived from the journal equals the one
a document-to-document diff computes, whether the discard bookkeeping still
matches a plain replay, and whether the refusal order in `commit_appended`
changed which error a command gets.

It printed its banner — model and reasoning effort as configured — and then
`ERROR: Your workspace is out of credits`. No findings. That round had no
adversary and is not reported as a review, and per INSTRUCTIONS.md it was not
retried. The scrutiny was done here instead, by writing the tests those five
questions asked for. That is how the affected-axis defect was found.
