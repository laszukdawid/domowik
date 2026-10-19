//! The operation log: framing validation, causal holding, and canonical order.
//!
//! Implements FORMAT-SPEC §4.3 row 4 (framing rejects, BEFORE replay), §4.4
//! (frontier, delivery-order independence) and §4.5 I2 (duplicate suppression).
//!
//! **This is not a CRDT** (§4 preamble). The log provides replay and
//! synchronization metadata only; §4.6's conflict rule is deterministic
//! per-field last-writer-wins and is LOSSY. Nothing here provides convergence.

use crate::operation::{Batch, OperationKind};
use std::collections::{BTreeMap, HashMap, HashSet};
use thiserror::Error;
use uuid::Uuid;

/// Why a batch could not enter the log.
///
/// Every variant is a §4.3 row 4 framing anomaly: rejected BEFORE replay
/// begins, never partially applied.
#[derive(Debug, Error, PartialEq)]
pub enum LogError {
    #[error("batch {batch_id} has no operations; an empty batch defines an empty counter range and no frontier position (FORMAT-SPEC B3)")]
    EmptyBatch { batch_id: Uuid },

    #[error("batch {batch_id} declares counterBase {counter_base}; an actor's counter starts at 1 (FORMAT-SPEC B3)")]
    CounterBaseBelowOne { batch_id: Uuid, counter_base: u64 },

    #[error("batch {batch_id}: frontier member {actor} has counter zero; absence is the sole spelling of nothing observed (FORMAT-SPEC F1 zero prohibition)")]
    ZeroFrontierMember { batch_id: Uuid, actor: Uuid },

    #[error("batch {batch_id} is already in the log with different content; batch ids are unique (FORMAT-SPEC E1)")]
    DuplicateBatchId { batch_id: Uuid },

    #[error("batch {batch_id} reserves counters {from}..={to} for actor {actor}, overlapping a range already reserved")]
    OverlappingRange {
        batch_id: Uuid,
        actor: Uuid,
        from: u64,
        to: u64,
    },

    #[error("batch {batch_id} starts actor {actor} at counter {from}, leaving a gap after {expected}; an actor's ranges are contiguous with no gaps (FORMAT-SPEC B3)")]
    CounterGap {
        batch_id: Uuid,
        actor: Uuid,
        from: u64,
        expected: u64,
    },

    #[error("batch {batch_id}: frontier entry for actor {actor} is {counter}, which is not the end of any of that actor's batches; frontiers advance only at batch ends (FORMAT-SPEC B5)")]
    FrontierMidBatch {
        batch_id: Uuid,
        actor: Uuid,
        counter: u64,
    },

    #[error("batch {batch_id}: frontier is not transitively closed — it observes a batch whose own frontier it does not dominate for actor {actor} ({observed} > {declared}); FORMAT-SPEC F4 rejects this before replay")]
    FrontierNotClosed {
        batch_id: Uuid,
        actor: Uuid,
        declared: u64,
        observed: u64,
    },

    #[error("batch {batch_id}: operation {operation_id} sets `{field}`, which is not an ordinary field — entity identity and type are immutable (FORMAT-SPEC E6, §2.3)")]
    ImmutableFieldWrite {
        batch_id: Uuid,
        operation_id: Uuid,
        field: &'static str,
    },

    #[error("batch {batch_id} is concurrent with this plan's snapshot: it does not observe actor {actor} through counter {covered}. Its position relative to compacted operations cannot be decided, so it is refused rather than guessed at (FORMAT-SPEC §5.1 S3)")]
    ConcurrentWithSnapshot {
        batch_id: Uuid,
        actor: Uuid,
        covered: u64,
    },
}

/// What happened to a delivered batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Accepted and now part of the log.
    Accepted,
    /// Its whole counter range lies at or below the watermark: already
    /// applied, ignored silently and deterministically (§4.5 I2).
    Duplicate,
    /// Causal prerequisites are not yet present; held until they arrive (F3).
    Held,
}

/// A durable log of operation batches.
///
/// Batches may arrive in any order (F3). The log holds a batch whose causal
/// prerequisites are absent and promotes it when they arrive, so the projected
/// result is a function of the delivered SET and never of arrival sequence.
#[derive(Debug, Clone, Default)]
pub struct OperationLog {
    accepted: Vec<Batch>,
    held: Vec<Batch>,
    ids: HashSet<Uuid>,
    /// actor → highest counter accepted, the per-actor watermark.
    watermark: HashMap<Uuid, u64>,
    /// actor → counter covered by the baseline snapshot, if any.
    baseline: HashMap<Uuid, u64>,
    /// Batches accepted by the LAST `deliver` that were not its argument.
    ///
    /// A held batch promoted by someone else's delivery is still an accepted
    /// batch, and a caller that has to react to acceptances — a session firing
    /// BEHAVIOR-SPEC R8.3.1's external fence, for instance — cannot see it from
    /// the return value of the call it made.
    promoted: Vec<Uuid>,
}

impl OperationLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// A log that continues from a snapshot's frontier (FORMAT-SPEC §5.1 S2).
    ///
    /// The frontier states which counters the snapshot already covers, so the
    /// log can tell a batch that is ALREADY APPLIED (I2, at or below the
    /// watermark) from one whose predecessors have not arrived (F3, held).
    /// Without it a snapshot-seeded log holds every subsequent batch forever,
    /// because their prerequisites were compacted away.
    ///
    /// A zero-valued member is rejected here as it is anywhere else (F1); an
    /// invalid seed would silently become watermark state, which F1 forbids
    /// specifically.
    pub fn with_baseline_frontier(frontier: &BTreeMap<Uuid, u64>) -> Result<Self, LogError> {
        let mut log = Self::default();
        for (&actor, &counter) in frontier {
            if counter == 0 {
                return Err(LogError::ZeroFrontierMember {
                    batch_id: Uuid::nil(),
                    actor,
                });
            }
            log.watermark.insert(actor, counter);
            log.baseline.insert(actor, counter);
        }
        Ok(log)
    }

    /// The frontier a snapshot of the current state should record (S2).
    pub fn baseline_frontier(&self) -> BTreeMap<Uuid, u64> {
        self.frontier()
    }

    /// The baseline frontier this log was SEEDED with, for rebuilding it.
    pub fn baseline_frontier_seed(&self) -> BTreeMap<Uuid, u64> {
        self.baseline.iter().map(|(&a, &c)| (a, c)).collect()
    }

    /// Batches accepted so far, in arrival order.
    pub fn batches(&self) -> &[Batch] {
        &self.accepted
    }

    /// Batches held for missing causal prerequisites (F3).
    pub fn held(&self) -> &[Batch] {
        &self.held
    }

    /// Batches OTHER than the last `deliver`'s argument that it accepted.
    ///
    /// Delivering one batch can promote any number of held ones (F3), and each
    /// promotion is an acceptance with the same consequences. Without this a
    /// caller sees only what it handed in.
    pub fn newly_promoted(&self) -> &[Uuid] {
        &self.promoted
    }

    /// The per-actor watermark: highest counter accepted for each actor.
    pub fn watermark(&self) -> &HashMap<Uuid, u64> {
        &self.watermark
    }

    /// The frontier a new local batch should declare: everything observed.
    pub fn frontier(&self) -> BTreeMap<Uuid, u64> {
        self.watermark
            .iter()
            .filter(|(_, &c)| c > 0)
            .map(|(&a, &c)| (a, c))
            .collect()
    }

    /// The next unused counter for `actor`.
    pub fn next_counter(&self, actor: Uuid) -> u64 {
        self.watermark.get(&actor).copied().unwrap_or(0) + 1
    }

    /// Delivers a batch.
    ///
    /// Framing is checked BEFORE anything else, because §4.3 row 4 rejects
    /// framing anomalies before replay begins and I2 explicitly does not
    /// exempt a duplicate from them: "watermark classification never precedes,
    /// gates, or replaces the pre-replay structural checks".
    pub fn deliver(&mut self, batch: Batch) -> Result<Delivery, LogError> {
        self.promoted.clear();
        self.check_framing(&batch)?;

        // I2: a range wholly at or below the watermark is already applied.
        let mark = self.watermark.get(&batch.actor_id).copied().unwrap_or(0);
        if batch.range_end() <= mark {
            return Ok(Delivery::Duplicate);
        }

        // AFTER the duplicate check, never before: a re-delivered batch from
        // before a compaction legitimately does not observe the snapshot, and
        // I2 already says to ignore it silently.
        self.check_dominates_snapshot(&batch)?;

        if !self.prerequisites_present(&batch) {
            self.held.push(batch);
            return Ok(Delivery::Held);
        }

        self.accept(batch);
        self.promote_held();
        Ok(Delivery::Accepted)
    }

    fn accept(&mut self, batch: Batch) {
        self.ids.insert(batch.batch_id);
        let end = batch.range_end();
        let mark = self.watermark.entry(batch.actor_id).or_insert(0);
        *mark = (*mark).max(end);
        self.accepted.push(batch);
    }

    /// Re-checks held batches after an acceptance, repeatedly, because
    /// promoting one can satisfy another's prerequisites.
    fn promote_held(&mut self) {
        loop {
            let Some(i) = self.held.iter().position(|b| self.prerequisites_present(b)) else {
                return;
            };
            let batch = self.held.remove(i);
            // Framing was validated against accepted AND held when this batch
            // arrived, and everything since has been validated against it, so
            // promotion cannot introduce a conflict. Re-checking anyway costs
            // one pass and turns a future invariant break into a rejection
            // rather than a corrupt log.
            if self.check_framing(&batch).is_err() {
                continue;
            }
            let mark = self.watermark.get(&batch.actor_id).copied().unwrap_or(0);
            if batch.range_end() > mark {
                let id = batch.batch_id;
                self.accept(batch);
                self.promoted.push(id);
            }
        }
    }

    /// F3: a batch whose frontier points at batches not yet present is held.
    ///
    /// Because B3 makes each actor's ranges contiguous from 1, "every batch
    /// its frontier points to is present" reduces to "the watermark for each
    /// frontier actor is at least the counter the frontier names" — the
    /// contiguity is what makes the watermark a complete answer rather than a
    /// high-water mark with holes under it.
    ///
    /// A batch's own actor is exempt for its own prior counters: those are
    /// checked by the gap rule in [`Self::check_framing`], which reports a
    /// missing predecessor as the framing anomaly it is rather than holding
    /// forever.
    fn prerequisites_present(&self, batch: &Batch) -> bool {
        let observed = batch
            .frontier
            .iter()
            .all(|(&actor, &counter)| self.watermark.get(&actor).copied().unwrap_or(0) >= counter);
        // B3 also makes an actor's OWN prior counters a prerequisite: a batch
        // at counterBase n needs 1..n-1 of that actor present, or the
        // watermark would advance over a hole.
        let mark = self.watermark.get(&batch.actor_id).copied().unwrap_or(0);
        observed && batch.counter_base <= mark + 1
    }

    /// Whether `batch` would be accepted, WITHOUT delivering it.
    ///
    /// [`Session`](crate::session::Session) checks framing before touching the
    /// projection, so a command rejected on framing grounds leaves the log
    /// and the document exactly as they were. `deliver` cannot be undone.
    pub fn would_accept(&self, batch: &Batch) -> Result<(), LogError> {
        self.check_framing(batch)?;
        self.check_dominates_snapshot(batch)
    }

    fn check_framing(&self, batch: &Batch) -> Result<(), LogError> {
        if batch.operations.is_empty() {
            return Err(LogError::EmptyBatch {
                batch_id: batch.batch_id,
            });
        }
        if batch.counter_base < 1 {
            return Err(LogError::CounterBaseBelowOne {
                batch_id: batch.batch_id,
                counter_base: batch.counter_base,
            });
        }
        if let Some((&actor, _)) = batch.frontier.iter().find(|(_, &c)| c == 0) {
            return Err(LogError::ZeroFrontierMember {
                batch_id: batch.batch_id,
                actor,
            });
        }

        // FORMAT-SPEC E6 and §2.3: entity identity and type are immutable.
        // `setFields` addresses ORDINARY top-level fields; naming `id` or
        // `type` does not edit an entity, it substitutes a different one —
        // and replay would insert the substitute while leaving the original,
        // because the operation names the original as its target. Rejected
        // here rather than in the command layer, because a hostile external
        // batch never passes through the command layer.
        for op in &batch.operations {
            if let OperationKind::SetFields { fields } = &op.kind {
                for field in ["id", "type"] {
                    if fields.contains_key(field) {
                        return Err(LogError::ImmutableFieldWrite {
                            batch_id: batch.batch_id,
                            operation_id: op.operation_id,
                            field,
                        });
                    }
                }
            }
        }

        // Identity and range conflicts are decided against every batch the log
        // KNOWS ABOUT — accepted and held alike.
        //
        // Checking only the accepted ones let two held batches share a batch id
        // or a counter range and both be promoted later, and made the outcome
        // depend on arrival order: of two held batches claiming one range, the
        // one promoted first won and the other was silently mistaken for an I2
        // duplicate because its range had fallen below the watermark. That is
        // exactly the arrival-order dependence F3 forbids.
        let known = self.accepted.iter().chain(self.held.iter());
        let (from, to) = batch.counter_range();
        for other in known {
            if other.batch_id == batch.batch_id {
                // A re-delivery of the SAME batch is a duplicate, not a
                // collision; a DIFFERENT batch reusing the id is an anomaly.
                if other != batch {
                    return Err(LogError::DuplicateBatchId {
                        batch_id: batch.batch_id,
                    });
                }
                continue;
            }
            if other.actor_id != batch.actor_id {
                continue;
            }
            let (o_from, o_to) = other.counter_range();
            if from <= o_to && o_from <= to {
                return Err(LogError::OverlappingRange {
                    batch_id: batch.batch_id,
                    actor: batch.actor_id,
                    from,
                    to,
                });
            }
        }

        // The BASELINE reserves [1, watermark] for its actors. A range wholly
        // inside it is an I2 duplicate, handled by the caller; a range that
        // STRADDLES it would replay operations the snapshot already applied.
        // No accepted batch records those counters — the snapshot replaced
        // them — so an accepted-only overlap check cannot see this.
        if let Some(&covered) = self.baseline.get(&batch.actor_id) {
            if from <= covered && to > covered {
                return Err(LogError::OverlappingRange {
                    batch_id: batch.batch_id,
                    actor: batch.actor_id,
                    from,
                    to,
                });
            }
        }

        self.check_frontier_closure(batch)?;
        Ok(())
    }

    /// A batch delivered after compaction must be causally AFTER everything
    /// the snapshot absorbed.
    ///
    /// # Why this guard exists, and what it buys
    ///
    /// FORMAT-SPEC §5.1 S3 requires a snapshot to retain per-field WINNER
    /// POSITIONS so that a late CONCURRENT write can be re-decided against the
    /// value the snapshot already holds — vector V5. Those positions are
    /// bounded by the named parameter `lwwHorizon`, which ED-0105 has since
    /// FIXED at the snapshot's complete coverage — so a conforming S3 is now
    /// writable, and this guard is a conservative stand-in rather than the
    /// only available answer. Implementing S3 and lifting the guard belongs
    /// to issues 0011/0027, not here; what follows is why the stand-in is
    /// sound in the meantime.
    ///
    /// This guard makes the case S3 exists for UNREACHABLE instead. A batch
    /// whose frontier dominates the snapshot's observed everything the
    /// snapshot did, so its write position is greater than every position the
    /// snapshot absorbed: it can never be the late concurrent write that needs
    /// re-deciding. A batch that does NOT dominate is refused, loudly, rather
    /// than applied on a guess.
    ///
    /// The same argument retires S1's `createdPositions`: a batch that follows
    /// everything compacted is after every retained creation, so I5 always
    /// lands in branch 2 for a live target. Tombstones still have to be
    /// carried — branch 3 is reachable, and without them a compacted delete
    /// would let a later create resurrect the id.
    ///
    /// **This is a narrowing, and it is the honest one.** For a single writer
    /// editing one plan — the whole of v0.1's local-first case — the guard
    /// never fires. It fires exactly when someone applies a batch authored
    /// without seeing the compacted history, and there the alternative is not
    /// "work anyway", it is "silently produce a different document than a
    /// replica that never compacted".
    fn check_dominates_snapshot(&self, batch: &Batch) -> Result<(), LogError> {
        for (&actor, &covered) in &self.baseline {
            if batch.frontier.get(&actor).copied().unwrap_or(0) < covered {
                return Err(LogError::ConcurrentWithSnapshot {
                    batch_id: batch.batch_id,
                    actor,
                    covered,
                });
            }
        }
        Ok(())
    }

    /// FORMAT-SPEC F4: frontiers must be TRANSITIVELY CLOSED.
    ///
    /// "A batch's frontier dominates (F2) the frontier of every batch it
    /// observes, includes that batch's end counter, AND includes the writing
    /// actor's own prior end counter."
    ///
    /// Closure is what makes F3's canonical order sound. An omitted entry
    /// under-reports `causalPastSize`, and F4 spells out the consequence: "a
    /// batch whose closed past is 45 reports 5 with its own entry dropped and
    /// loses to a concurrent batch at 30 it should beat." Without this check a
    /// batch could sort BEFORE one it causally follows, and replay would then
    /// discard it as targeting an entity that does not exist yet.
    ///
    /// Obligation 3 is honoured by omission: closure is NOT provable for a
    /// batch whose observed ancestors are no longer retained, so an entry at or
    /// below the baseline is skipped rather than guessed at.
    fn check_frontier_closure(&self, batch: &Batch) -> Result<(), LogError> {
        // The writing actor's own prior end counter must be included.
        //
        // B3 makes an actor's ranges contiguous from 1, so that counter is
        // exactly `counterBase - 1` — not the highest counter the actor has
        // reached. Taking the highest breaks re-delivery: an old batch being
        // delivered again legitimately does not observe counters written after
        // it, and would be rejected for a closure violation it does not have.
        let declared_own = batch.frontier.get(&batch.actor_id).copied().unwrap_or(0);
        if batch.counter_base > 1 {
            let prior = batch.counter_base - 1;
            if declared_own < prior {
                return Err(LogError::FrontierNotClosed {
                    batch_id: batch.batch_id,
                    actor: batch.actor_id,
                    declared: declared_own,
                    observed: prior,
                });
            }
        }
        // ...and no FURTHER than its own prior end. A frontier is a batch's
        // causal PAST (F1/F3), so it cannot contain the batch itself or
        // anything after it — an entry at or above `counterBase` claims to
        // have observed operations this batch is only now reserving, and B5
        // forbids a frontier reaching inside a batch at all.
        //
        // Found by the hostile corpus: `inconsistent-frontier.json` reserves
        // counter 85 and declares 86 for its own actor. The closure check
        // enforced only the LOWER bound, so a frontier that ran ahead of its
        // own batch passed.
        if declared_own >= batch.counter_base {
            return Err(LogError::FrontierMidBatch {
                batch_id: batch.batch_id,
                actor: batch.actor_id,
                counter: declared_own,
            });
        }

        for (&actor, &counter) in &batch.frontier {
            // Below the baseline the ancestor is compacted and closure is
            // unprovable (F4 obligation 3).
            if self.baseline.get(&actor).copied().unwrap_or(0) >= counter {
                continue;
            }
            let Some(observed) = self
                .accepted
                .iter()
                .chain(self.held.iter())
                .find(|b| b.actor_id == actor && b.range_end() == counter)
            else {
                // Either the ancestor has not arrived — the batch will be held
                // and re-checked — or the counter is not a batch END, which
                // B5 forbids. The two are distinguished by whether that actor
                // has any batch covering the counter at all.
                let covered = self.accepted.iter().chain(self.held.iter()).any(|b| {
                    b.actor_id == actor && b.counter_base <= counter && counter <= b.range_end()
                });
                if covered {
                    return Err(LogError::FrontierMidBatch {
                        batch_id: batch.batch_id,
                        actor,
                        counter,
                    });
                }
                continue;
            };
            for (&a, &c) in &observed.frontier {
                if batch.frontier.get(&a).copied().unwrap_or(0) < c {
                    return Err(LogError::FrontierNotClosed {
                        batch_id: batch.batch_id,
                        actor: a,
                        declared: batch.frontier.get(&a).copied().unwrap_or(0),
                        observed: c,
                    });
                }
            }
        }
        Ok(())
    }

    /// B3's no-gap rule, checked over the log as a whole.
    ///
    /// This is NOT part of [`Self::deliver`], and the distinction is the point.
    /// A batch that starts after a gap in its own actor's stream may simply
    /// have overtaken its predecessor in transit — F3 says batches may arrive
    /// in any order — so delivery HOLDS it rather than rejecting it. The gap
    /// is a framing anomaly only once the log is final: a retained log with a
    /// permanent hole in an actor's stream is malformed under §4.3 row 4,
    /// because the watermark stops being a complete answer to what has been
    /// seen.
    ///
    /// Call this when reading a package, not when receiving a batch.
    pub fn validate_retained(&self) -> Result<(), LogError> {
        let mut ends: HashMap<Uuid, Vec<(u64, u64, Uuid)>> = HashMap::new();
        for b in &self.accepted {
            let (from, to) = b.counter_range();
            ends.entry(b.actor_id)
                .or_default()
                .push((from, to, b.batch_id));
        }
        // Sorted, so which actor's gap is reported does not depend on hash
        // iteration order.
        let mut actors: Vec<Uuid> = ends.keys().copied().collect();
        actors.sort();
        for actor in actors {
            let mut ranges = ends.remove(&actor).expect("present");
            ranges.sort();
            // A baseline-seeded log starts above 1 for the actors its snapshot
            // covers: those counters were compacted, not lost.
            let mut expected = ranges
                .first()
                .map(|&(from, _, _)| from.min(self.baseline.get(&actor).copied().unwrap_or(1) + 1))
                .unwrap_or(1);
            for (from, to, batch_id) in ranges {
                if from != expected {
                    return Err(LogError::CounterGap {
                        batch_id,
                        actor,
                        from,
                        expected: expected.saturating_sub(1),
                    });
                }
                expected = to + 1;
            }
        }
        if let Some(b) = self.held.first() {
            return Err(LogError::CounterGap {
                batch_id: b.batch_id,
                actor: b.actor_id,
                from: b.counter_base,
                expected: self.watermark.get(&b.actor_id).copied().unwrap_or(0),
            });
        }
        Ok(())
    }

    /// The accepted batches in CANONICAL ORDER: ascending by ordering position
    /// `P = (causalPastSize, counterBase, actorId)` (F3).
    ///
    /// P is unique per batch because B3 makes `(counterBase, actorId)` unique,
    /// so exactly one canonical order exists. A batch's causal past strictly
    /// contains the past of every batch it observes, so `causalPastSize`
    /// strictly increases along causality and this sort is always a linear
    /// extension of it.
    pub fn canonical_order(&self) -> Vec<&Batch> {
        let mut out: Vec<&Batch> = self.accepted.iter().collect();
        out.sort_by_key(|b| b.position());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operation::Operation;
    use domowik_format::{Entity, Level};

    fn actor() -> Uuid {
        Uuid::now_v7()
    }

    fn ops(n: usize) -> Vec<Operation> {
        (0..n)
            .map(|_| Operation::create(Entity::Level(Level::new("L"))))
            .collect()
    }

    /// A conforming batch: F4 requires the writing actor's own prior end
    /// counter in the frontier, which for a contiguous stream is
    /// `counterBase - 1`.
    fn batch(a: Uuid, base: u64, n: usize) -> Batch {
        let mut frontier = BTreeMap::new();
        if base > 1 {
            frontier.insert(a, base - 1);
        }
        Batch::new(a, base, frontier, ops(n))
    }

    fn observing(a: Uuid, base: u64, n: usize, seen: &[(Uuid, u64)]) -> Batch {
        let mut frontier: BTreeMap<Uuid, u64> = seen.iter().copied().collect();
        if base > 1 {
            let own = frontier.entry(a).or_insert(0);
            *own = (*own).max(base - 1);
        }
        Batch::new(a, base, frontier, ops(n))
    }

    #[test]
    fn an_accepted_batch_advances_the_watermark_to_its_range_end() {
        // B5: causal metadata advances past a batch only to its END.
        let a = actor();
        let mut log = OperationLog::new();
        assert_eq!(log.deliver(batch(a, 1, 3)), Ok(Delivery::Accepted));
        assert_eq!(log.watermark().get(&a), Some(&3));
        assert_eq!(log.next_counter(a), 4);
    }

    #[test]
    fn an_empty_batch_is_a_framing_reject() {
        let mut log = OperationLog::new();
        let b = Batch::new(actor(), 1, BTreeMap::new(), vec![]);
        assert!(matches!(log.deliver(b), Err(LogError::EmptyBatch { .. })));
    }

    #[test]
    fn a_zero_valued_frontier_member_is_rejected_before_anything_else() {
        // F1 zero prohibition: absence is the SOLE spelling of nothing
        // observed. Zero is not a permitted alternative, and readers must
        // reject it before constructing watermark state.
        let mut log = OperationLog::new();
        let b = observing(actor(), 1, 1, &[(actor(), 0)]);
        assert!(matches!(
            log.deliver(b),
            Err(LogError::ZeroFrontierMember { .. })
        ));
        assert!(log.watermark().is_empty(), "no state may have been built");
    }

    #[test]
    fn a_counter_base_below_one_is_rejected() {
        let mut log = OperationLog::new();
        let b = batch(actor(), 0, 1);
        assert!(matches!(
            log.deliver(b),
            Err(LogError::CounterBaseBelowOne { .. })
        ));
    }

    #[test]
    fn vector_v1_a_duplicate_delivery_is_ignored_silently() {
        // V1: second occurrence ignored (I2); state and frontier identical to
        // a single delivery.
        let a = actor();
        let mut log = OperationLog::new();
        let b = batch(a, 1, 2);
        assert_eq!(log.deliver(b.clone()), Ok(Delivery::Accepted));
        let after_one = log.watermark().clone();
        assert_eq!(log.deliver(b), Ok(Delivery::Duplicate));
        assert_eq!(log.batches().len(), 1, "no second copy in the log");
        assert_eq!(log.watermark(), &after_one);
    }

    #[test]
    fn a_range_straddling_an_accepted_one_is_an_overlap_not_a_duplicate() {
        // I2 covers a re-delivery of the SAME batch. A different batch whose
        // counters overlap an accepted range would apply some counters twice.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 3)).expect("first");
        assert!(matches!(
            log.deliver(batch(a, 2, 3)),
            Err(LogError::OverlappingRange { .. })
        ));
    }

    #[test]
    fn a_different_batch_reserving_an_accepted_range_is_rejected_not_ignored() {
        // §7.6 enumerates "duplicate reserved ranges" as a framing anomaly.
        // A watermark test alone reports this as an I2 duplicate, because the
        // range does lie at or below the mark — and silently drops a batch
        // that is evidence of a corrupt log.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 2)).expect("first");
        let clash = batch(a, 1, 2); // same range, fresh batch id
        assert!(matches!(
            log.deliver(clash),
            Err(LogError::OverlappingRange { .. })
        ));
    }

    #[test]
    fn a_gap_in_an_actors_own_stream_holds_the_batch_rather_than_rejecting_it() {
        // F3: batches may arrive in any order, so a batch that overtook its
        // own predecessor is early, not malformed.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 2)).expect("first");
        assert_eq!(log.deliver(batch(a, 5, 1)), Ok(Delivery::Held));
        assert_eq!(log.batches().len(), 1);
    }

    #[test]
    fn a_gap_that_never_closes_makes_the_retained_log_malformed() {
        // §4.3 row 4: once the log is final, a hole in an actor's stream means
        // the watermark is no longer a complete answer to what has been seen.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 2)).expect("first");
        assert!(log.validate_retained().is_ok());
        log.deliver(batch(a, 5, 1)).expect("held");
        assert!(matches!(
            log.validate_retained(),
            Err(LogError::CounterGap { .. })
        ));
    }

    #[test]
    fn a_gap_closes_when_the_missing_predecessor_arrives() {
        let a = actor();
        let mut log = OperationLog::new();
        assert_eq!(log.deliver(batch(a, 3, 1)), Ok(Delivery::Held));
        assert_eq!(log.deliver(batch(a, 1, 2)), Ok(Delivery::Accepted));
        assert_eq!(log.batches().len(), 2, "the held batch must promote");
        assert!(log.validate_retained().is_ok());
        assert_eq!(log.watermark().get(&a), Some(&3));
    }

    #[test]
    fn a_different_batch_reusing_an_accepted_id_is_rejected() {
        let a = actor();
        let mut log = OperationLog::new();
        let first = batch(a, 1, 1);
        log.deliver(first.clone()).expect("first");
        let mut impostor = batch(a, 2, 1);
        impostor.batch_id = first.batch_id;
        assert!(matches!(
            log.deliver(impostor),
            Err(LogError::DuplicateBatchId { .. })
        ));
    }

    #[test]
    fn vector_v2_a_causally_later_batch_is_held_until_its_prerequisite_arrives() {
        // V2: B causally after A arrives first; B is held (F3) and the final
        // projection equals in-order delivery exactly.
        let (x, y) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();

        let a = batch(x, 1, 2);
        let b = observing(y, 1, 1, &[(x, 2)]);

        assert_eq!(log.deliver(b.clone()), Ok(Delivery::Held));
        assert_eq!(log.batches().len(), 0);
        assert_eq!(log.held().len(), 1);

        assert_eq!(log.deliver(a.clone()), Ok(Delivery::Accepted));
        assert_eq!(log.batches().len(), 2, "B must be promoted with A");
        assert!(log.held().is_empty());

        // In-order delivery of the same set must give the same canonical order.
        let mut in_order = OperationLog::new();
        in_order.deliver(a).expect("a");
        in_order.deliver(b).expect("b");
        let ids = |l: &OperationLog| -> Vec<Uuid> {
            l.canonical_order().iter().map(|b| b.batch_id).collect()
        };
        assert_eq!(ids(&log), ids(&in_order));
    }

    #[test]
    fn holding_cascades_when_one_promotion_satisfies_another() {
        let (x, y, z) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let mut log = OperationLog::new();

        let a = batch(x, 1, 1);
        let b = observing(y, 1, 1, &[(x, 1)]);
        let c = observing(z, 1, 1, &[(x, 1), (y, 1)]);

        assert_eq!(log.deliver(c), Ok(Delivery::Held));
        assert_eq!(log.deliver(b), Ok(Delivery::Held));
        assert_eq!(log.deliver(a), Ok(Delivery::Accepted));
        assert_eq!(log.batches().len(), 3, "both held batches must promote");
        assert!(log.held().is_empty());
    }

    #[test]
    fn canonical_order_is_a_function_of_the_set_not_the_arrival_sequence() {
        // F3: the canonical order is a function of the delivered SET, never of
        // arrival sequence.
        let (x, y) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let a = batch(x, 1, 1);
        let b = batch(y, 1, 1);
        let c = observing(x, 2, 1, &[(x, 1), (y, 1)]);

        let order = |batches: Vec<Batch>| -> Vec<Uuid> {
            let mut log = OperationLog::new();
            for b in batches {
                log.deliver(b).expect("delivers");
            }
            log.canonical_order().iter().map(|b| b.batch_id).collect()
        };

        let forward = order(vec![a.clone(), b.clone(), c.clone()]);
        let backward = order(vec![c.clone(), b.clone(), a.clone()]);
        let shuffled = order(vec![b, a, c]);
        assert_eq!(forward, backward);
        assert_eq!(forward, shuffled);
    }

    #[test]
    fn canonical_order_puts_a_causally_later_batch_after_a_higher_counter_one() {
        // LWW1: CAUSALITY GOVERNS. A batch observing another has a strictly
        // greater causalPastSize and sorts after it regardless of counters.
        let (x, y) = (Uuid::from_u128(9), Uuid::from_u128(1));
        let mut log = OperationLog::new();
        let early = batch(x, 1, 5); // counters 1..5, past 0
        let later = observing(y, 1, 1, &[(x, 5)]); // counter 1, past 5
        log.deliver(early.clone()).expect("early");
        log.deliver(later.clone()).expect("later");
        let order: Vec<Uuid> = log.canonical_order().iter().map(|b| b.batch_id).collect();
        assert_eq!(order, vec![early.batch_id, later.batch_id]);
    }

    #[test]
    fn a_baseline_frontier_lets_the_log_continue_from_a_snapshot() {
        // §5.1 S2. Without the seed, a batch at counterBase 3 whose
        // predecessors were compacted would be held forever.
        let a = actor();
        let mut seed = BTreeMap::new();
        seed.insert(a, 2);
        let mut log = OperationLog::with_baseline_frontier(&seed).expect("seed");
        assert_eq!(log.next_counter(a), 3);
        assert_eq!(log.deliver(batch(a, 3, 1)), Ok(Delivery::Accepted));
        assert!(log.validate_retained().is_ok());
    }

    #[test]
    fn a_baseline_frontier_still_suppresses_a_covered_batch() {
        // I2: a range wholly at or below the watermark is already applied.
        let a = actor();
        let mut seed = BTreeMap::new();
        seed.insert(a, 5);
        let mut log = OperationLog::with_baseline_frontier(&seed).expect("seed");
        assert_eq!(log.deliver(batch(a, 1, 3)), Ok(Delivery::Duplicate));
    }

    #[test]
    fn a_zero_valued_baseline_frontier_member_is_rejected() {
        let mut seed = BTreeMap::new();
        seed.insert(actor(), 0);
        assert!(matches!(
            OperationLog::with_baseline_frontier(&seed),
            Err(LogError::ZeroFrontierMember { .. })
        ));
    }

    #[test]
    fn the_frontier_a_new_batch_should_declare_omits_unobserved_actors() {
        // F1: absent actor means nothing observed; zero is never written.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 2)).expect("first");
        let f = log.frontier();
        assert_eq!(f.get(&a), Some(&2));
        assert_eq!(f.len(), 1);
        assert!(f.values().all(|&c| c > 0));
    }

    // ---- regressions from adversarial review (Codex, gpt-5.6-sol, 2026-08-31)

    #[test]
    fn two_held_batches_may_not_share_a_batch_id() {
        // Finding 1. `check_framing` compared only against ACCEPTED batches,
        // so two held batches with one id were both promoted later and the
        // log ended up containing the same batch id twice.
        let (a, x) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        let h1 = observing(a, 1, 1, &[(x, 1)]);
        let mut h2 = observing(a, 2, 1, &[(x, 1)]);
        h2.batch_id = h1.batch_id;

        assert_eq!(log.deliver(h1), Ok(Delivery::Held));
        assert!(matches!(
            log.deliver(h2),
            Err(LogError::DuplicateBatchId { .. })
        ));
    }

    #[test]
    fn two_held_batches_may_not_reserve_overlapping_ranges() {
        // Finding 1, the range half: held ranges were invisible, so both were
        // promoted and one counter was reserved twice.
        let (a, x) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        assert_eq!(
            log.deliver(observing(a, 1, 2, &[(x, 1)])),
            Ok(Delivery::Held)
        );
        assert!(matches!(
            log.deliver(observing(a, 2, 2, &[(x, 1)])),
            Err(LogError::OverlappingRange { .. })
        ));
    }

    #[test]
    fn two_held_batches_claiming_one_range_resolve_the_same_way_in_either_order() {
        // Finding 1's arrival-order dependence, which is what F3 forbids
        // outright: whichever of R and S was promoted first won, and the other
        // was silently mistaken for an I2 duplicate because its range had
        // fallen below the watermark.
        let (a, x) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let r = observing(a, 1, 1, &[(x, 1)]);
        let sb = observing(a, 1, 1, &[(x, 1)]);

        let run = |first: Batch, second: Batch| {
            let mut log = OperationLog::new();
            log.deliver(first).expect("first is held");
            let verdict = log.deliver(second);
            log.deliver(batch(x, 1, 1)).expect("unblocks");
            (verdict, log.batches().len())
        };
        let (v1, n1) = run(r.clone(), sb.clone());
        let (v2, n2) = run(sb, r);
        assert!(matches!(v1, Err(LogError::OverlappingRange { .. })));
        assert!(matches!(v2, Err(LogError::OverlappingRange { .. })));
        assert_eq!(n1, n2, "the same set must give the same log");
        assert_eq!(n1, 2, "the loser is REJECTED, not silently dropped");
    }

    #[test]
    fn a_range_straddling_the_baseline_watermark_is_rejected() {
        // Finding 2. Counters 4 and 5 are covered by the snapshot; no accepted
        // batch records them, so an accepted-only overlap check saw nothing,
        // and the whole batch replayed on top of the snapshot.
        let a = actor();
        let mut seed = BTreeMap::new();
        seed.insert(a, 5);
        let mut log = OperationLog::with_baseline_frontier(&seed).expect("seed");
        let straddling = observing(a, 4, 3, &[(a, 5)]);
        assert!(matches!(
            log.deliver(straddling),
            Err(LogError::OverlappingRange { .. })
        ));
    }

    #[test]
    fn a_batch_omitting_its_own_prior_counter_is_rejected() {
        // Finding 3. F4: a frontier must include "the writing actor's own
        // prior end counter". Omitting it under-reports causalPastSize, and
        // canonical order then places the batch BEFORE one it causally
        // follows — so replay discards it as targeting an entity that does not
        // exist yet.
        let (y, a) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        log.deliver(batch(y, 1, 2)).expect("y");
        log.deliver(observing(a, 1, 1, &[(y, 2)])).expect("a1");

        // A2 declares only {A:1}, dropping the inherited {Y:2}.
        let mut frontier = BTreeMap::new();
        frontier.insert(a, 1);
        let a2 = Batch::new(a, 2, frontier, ops(1));
        assert!(matches!(
            log.deliver(a2),
            Err(LogError::FrontierNotClosed { .. })
        ));

        // And the own-prior rule alone, with nothing for the transitive rule
        // to catch: one actor, no observed batches, second batch declaring an
        // EMPTY frontier. Without this the case above proves only the
        // transitive half, because A1's own frontier already fails it.
        let solo = Uuid::from_u128(9);
        let mut alone = OperationLog::new();
        alone.deliver(batch(solo, 1, 1)).expect("first");
        let second = Batch::new(solo, 2, BTreeMap::new(), ops(1));
        match alone.deliver(second) {
            Err(LogError::FrontierNotClosed {
                actor,
                declared,
                observed,
                ..
            }) => {
                assert_eq!(actor, solo);
                assert_eq!((declared, observed), (0, 1));
            }
            other => panic!("expected FrontierNotClosed, got {other:?}"),
        }
    }

    #[test]
    fn a_batch_omitting_an_inherited_frontier_entry_is_rejected() {
        // The transitive half of F4: A2 includes its own prior counter but
        // drops {Y:2}, which A1 observed. Its causal past is under-reported
        // just the same.
        let (y, a) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        log.deliver(batch(y, 1, 2)).expect("y");
        log.deliver(observing(a, 1, 1, &[(y, 2)])).expect("a1");

        let mut frontier = BTreeMap::new();
        frontier.insert(a, 1);
        // Observing A's own batch 1 requires dominating ITS frontier, {Y:2}.
        let a2 = Batch::new(a, 2, frontier, ops(1));
        match log.deliver(a2) {
            Err(LogError::FrontierNotClosed {
                actor, observed, ..
            }) => {
                assert_eq!(actor, y);
                assert_eq!(observed, 2);
            }
            other => panic!("expected FrontierNotClosed, got {other:?}"),
        }
    }

    #[test]
    fn a_batch_may_not_observe_its_own_counters_or_beyond() {
        // A frontier is a batch's causal PAST, so an entry at or above its own
        // counterBase claims to have observed operations the batch is only now
        // reserving.
        let a = actor();
        let mut log = OperationLog::new();
        log.deliver(batch(a, 1, 1)).expect("first");

        for claim in [2, 3, 9] {
            let mut frontier = BTreeMap::new();
            frontier.insert(a, claim);
            let b = Batch::new(a, 2, frontier, ops(1));
            assert!(
                matches!(log.deliver(b), Err(LogError::FrontierMidBatch { .. })),
                "a frontier of {claim} against counterBase 2 must be refused"
            );
        }
        // Exactly counterBase - 1 is the conforming value.
        assert_eq!(log.deliver(batch(a, 2, 1)), Ok(Delivery::Accepted));
    }

    #[test]
    fn a_frontier_pointing_inside_a_batch_is_rejected() {
        // B5: "Intermediate counters of a batch are never exposed in any
        // frontier." A frontier at 1 for an actor whose only batch spans 1..=3
        // claims to have observed half of an atomic batch.
        let (a, x) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        log.deliver(batch(x, 1, 3)).expect("x");
        assert!(matches!(
            log.deliver(observing(a, 1, 1, &[(x, 1)])),
            Err(LogError::FrontierMidBatch { .. })
        ));
        // The batch END is fine.
        assert_eq!(
            log.deliver(observing(a, 1, 1, &[(x, 3)])),
            Ok(Delivery::Accepted)
        );
    }

    #[test]
    fn a_set_fields_operation_may_not_write_id_or_type() {
        // Finding 5. E6 makes an entity id immutable and §2.3 makes the type
        // tag structural. A `setFields` naming either does not edit an entity,
        // it substitutes a different one — and replay inserts the substitute
        // while LEAVING the original, because the operation names the original
        // as its target.
        for field in ["id", "type"] {
            let mut fields = serde_json::Map::new();
            fields.insert(field.to_owned(), serde_json::json!("x"));
            let b = Batch::new(
                actor(),
                1,
                BTreeMap::new(),
                vec![crate::operation::Operation::set_fields(
                    Uuid::now_v7(),
                    fields,
                )],
            );
            let mut log = OperationLog::new();
            assert!(
                matches!(log.deliver(b), Err(LogError::ImmutableFieldWrite { .. })),
                "field {field} must be refused"
            );
        }
    }

    #[test]
    fn a_promoted_batch_is_reported_to_the_caller() {
        // Finding 8's mechanism: delivering one batch can accept others, and a
        // caller reacting to acceptances cannot see that from the return value
        // of the call it made.
        let (a, x) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let mut log = OperationLog::new();
        let held = observing(a, 1, 1, &[(x, 1)]);
        let held_id = held.batch_id;
        assert_eq!(log.deliver(held), Ok(Delivery::Held));
        assert!(log.newly_promoted().is_empty());

        assert_eq!(log.deliver(batch(x, 1, 1)), Ok(Delivery::Accepted));
        assert_eq!(log.newly_promoted(), &[held_id]);
    }
}
