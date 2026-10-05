# Adversarial review — operation log, replay, and session (issue 0011)

- **Date.** 2026-08-31
- **Advisor.** Codex, `gpt-5.6-sol`, `model_reasoning_effort="high"`, read-only
  sandbox. The banner in the transcript echoes `model: gpt-5.6-sol` and
  `reasoning effort: high`, so the configuration is verified rather than
  assumed (ED-0061).
- **Scope.** Three files — `log.rs`, `replay.rs`, `session.rs` — with the five
  claims under test stated in the prompt and an explicit demand for
  counter-examples as data.
- **Transcript.** `adversarial-round-1-transcript.txt`, unedited.

**Eight findings. All eight are real, all eight are accepted, all eight are
fixed.** Fourteen mutation proofs cover the fixes; two of the tests written for
them proved nothing until they were rewritten.

This was the most productive round so far, and the reason is worth naming: two
of the eight were defects in code whose own comments claimed the opposite.

## 1 — Held batches bypassed identity and range validation

`check_framing` compared only against ACCEPTED batches. Two HELD batches could
share a batch id, or reserve overlapping counter ranges, and both were promoted
later. Worse, the outcome depended on arrival order: of two held batches
claiming one range, whichever was promoted first won and the other was silently
mistaken for an I2 duplicate because its range had fallen below the watermark.

That is exactly the arrival-order dependence F3 forbids, so claims 1 and 2 were
both false for the same delivered set. **Fixed**: conflicts are decided against
accepted AND held, and `promote_held` re-checks before accepting.

## 2 — A range straddling the baseline watermark replayed twice

With a snapshot covering counters 1–5, a batch reserving 4–6 was accepted whole
and all three of its operations replayed on top of the snapshot. No accepted
batch records baseline counters — the snapshot replaced them — so the
accepted-only overlap check saw nothing, and the whole-range I2 test did not
fire because the range extends past the watermark.

**Fixed**: the baseline's covered range is an overlap source in its own right.

## 3 — A non-closed frontier reversed one actor's own causal order

The most consequential finding. FORMAT-SPEC F4 requires TRANSITIVELY CLOSED
frontiers, and obligation 2 makes validating that a READER's duty: "A violation
is an inconsistent frontier and rejects `malformed-framing` before replay."
None of it was implemented.

Codex's case: `A2` declares `{A:1}`, dropping the `{Y:2}` its own predecessor
observed. Its `causalPastSize` is 1 against `A1`'s 2, so canonical order places
`A2` BEFORE `A1` — and replay then discards `A2` as targeting an entity that
does not exist yet. F4 anticipates exactly this: "a batch whose closed past is
45 reports 5 with its own entry dropped and loses to a concurrent batch at 30
it should beat."

**Fixed**: F4 closure is validated on delivery — the actor's own prior end
counter, and dominance over every observed batch's frontier — with obligation 3
honoured by skipping entries at or below the baseline, where closure is not
provable. B5 is checked alongside it: a frontier entry that is not a batch END
claims to have observed half of an atomic batch.

## 4 — Any same-actor create resurrected that actor's tombstone

`authorized_restore` reads E6 authorization from provenance: same actor as the
tombstone. The doc comment said the enforcement lived in `Session`, "which
mints a restoring create only from its own redo stack".

**It did not.** `Session::commit` was public and accepted any create, so an
ordinary command could reopen a tombstone. **Fixed**: `commit` refuses to create
a tombstoned id, `redo` takes the restoring path, and a `compensate` method
gives a stateless front end — a CLI process — the same authorization for
inverting a batch. The comment now names the check instead of assuming it.

## 5 — Malformed and identity-changing `setFields` were accepted

Two defects in one place.

A payload with a wrong-typed value made `merge_fields` return `None` and the
operation silently do nothing — so the batch was accepted, the document marked
dirty, and an undo entry pushed, for a command that changed nothing.

A payload naming `id` or `type` was worse: it does not edit an entity, it
substitutes a different one. Replay inserted the substitute and left the
original, because the operation named the original as its target, and the undo
entry could not remove what it had never heard of.

**Fixed**: `id` and `type` are refused at the log's framing check, where a
hostile external batch also meets them; a payload that does not produce a valid
entity discards its batch with a `MalformedPayload` reason instead of no-oping.

## 6 — Inverting a create ignored LWW4 collisions

A create whose id already existed REPLACES rather than adds, so its inverse is
to restore the old entity. Inverting it as a delete left the id absent and
tombstoned. And a batch creating one id twice inverted to two deletes, the
second stale against the first — discarding the whole compensating batch while
leaving it in the durable log.

**Fixed**: a colliding create inverts to a restore, and a repeat within one
batch contributes no second inverse.

## 7 — A rejected command still grew durable history

`would_be` replayed a scratch batch and returned only the resulting document.
That document validated, so the REAL batch was delivered, and only then did
`apply_local` notice replay had discarded it. The caller got an error and no
undo entry — and the rejected batch stayed in the log.

R8.2.2 and §4.3 row 1 both require rejection before any effect, and a batch
existing in durable history is an effect. **Fixed**: everything is decided on
the scratch log, including whether replay would discard the batch; the real log
changes only afterwards.

## 8 — A promoted external batch did not fire the fence

Delivering a local batch can promote held EXTERNAL ones. Each promotion is an
acceptance with all of R8.3.1's consequences, but `deliver` reported only the
verdict for the batch it was handed, so the fence never fired and local undo
crossed an accepted external batch.

**Fixed**: the log reports what a delivery promoted; a local commit that
promotes an external batch clears both stacks AND does not push its own entry,
which would otherwise land on the far side of the fence that just cleared them.

## What the round found SOUND

Recorded because a negative result from a competent adversary is evidence:

- `promote_held` cannot loop forever — every successful iteration removes one
  element from a finite vector.
- B1 rollback is complete: document and intervals are cloned and assigned back
  only after every operation succeeds; the only mutation on failure is the
  discard record, which is the intended output.
- The three-branch I5 classifier is sound for a correctly ordered history.
  Finding 3 broke the ORDERING supplied to it, not the classifier.
- For schema-valid, identity-preserving `setFields`, canonical-order replay
  implements per-field LWW correctly.
- Fresh undo/redo operation and batch ids, preserved entity ids, and
  independently bounded stacks all hold.
- Hash iteration does not reach canonical output. One exception was found —
  `validate_retained` reported whichever actor its `HashMap` visited first —
  reachable only by first exploiting finding 1. Fixed anyway by sorting.

## What this says about the previous review

`docs/evidence/wall-joins/ADVERSARIAL-REVIEW.md` ended with the observation
that naming the algorithm's rules in the prompt was worth more than anything
else, because it let the advisor simulate the code rather than read it. That
held again. The prompt for this round stated the five claims and quoted the
spec rules; every one of the eight findings is a counter-example to a stated
claim rather than a general observation about the code.
