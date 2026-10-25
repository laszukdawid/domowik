//! Replay: projecting an operation log into a document.
//!
//! Implements FORMAT-SPEC §4.5 (tombstones, the I5 three-branch staleness
//! test) and §4.6 (per-field last-writer-wins), by applying batches in the
//! canonical order §4.4 F3 defines.
//!
//! # Why applying in canonical order IS the conflict rule
//!
//! LWW1 says the winning write for a field is the one with the greatest write
//! position, and that this "is exactly the LAST write to that field in
//! canonical order". So there is no separate merge step: sort by position,
//! apply in order, and let a later write overwrite an earlier one. LWW3
//! (deletes dominate) falls out the same way — a canonically later delete
//! tombstones whatever a set wrote — and so does LWW4, where the surviving
//! create is simply the last one applied.
//!
//! What does NOT fall out, and is therefore explicit below, is I5: an
//! operation must be classified against its target's LIVE INTERVAL at its own
//! write position, not against the current projection. The two-way reading —
//! present-or-absent right now — makes I5's branch 2 indistinguishable from
//! branch 3 and silently changes an unrelated field's value.

use crate::log::OperationLog;
use crate::operation::{Batch, Operation, OperationKind, WritePosition};
use crate::wall_contact::{same_axis, wall_contact_refusals_among, WallContactBlock};
use domowik_format::{param_along, remap_u, Document, Entity, Opening, OpeningCode, Wall};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// An entity's live interval (FORMAT-SPEC §4.5 I3).
///
/// `[creation_position, deleting_position)`. A tombstone keeps the interval so
/// deletion does not erase the fact that the entity once existed at a
/// position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveInterval {
    /// Write position of the create that opened the interval.
    pub created_at: WritePosition,
    /// Write position of the delete that closed it, if any.
    pub deleted_at: Option<WritePosition>,
    /// The actor whose delete closed it. Used for E6's authorized restoration.
    pub deleted_by: Option<Uuid>,
}

/// Where an operation's target stands at that operation's write position.
///
/// I5 classifies in exactly THREE ways rather than two. The third is the one a
/// present-or-absent test cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetState {
    /// Branch 1: the target's creation is not earlier than this write.
    BeforeCreation,
    /// Branch 2: the target exists at this position.
    Live,
    /// Branch 3: the target's deletion is earlier than this write.
    AfterDeletion,
}

/// Why a batch was discarded during replay.
#[derive(Debug, Clone, PartialEq)]
pub struct Discard {
    pub batch_id: Uuid,
    pub operation_id: Uuid,
    pub entity_id: Uuid,
    pub reason: DiscardReason,
}

/// The reason replay refused a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiscardReason {
    /// An operation's target was not live at its own write position (I5).
    Stale(TargetState),
    /// A `setFields` payload does not produce a valid entity — a field named
    /// with a wrong-typed value, say.
    ///
    /// Previously this was a SILENT no-op: `merge_fields` returned `None` and
    /// the operation did nothing, so a batch could be accepted, marked dirty
    /// and pushed onto the undo stack while changing nothing at all. Found by
    /// adversarial review (Codex, gpt-5.6-sol, 2026-08-31).
    MalformedPayload,
}

/// The result of replaying a log.
#[derive(Debug, Clone, Default)]
pub struct Projection {
    /// The authoritative document.
    pub document: Document,
    /// Live intervals, including closed ones (tombstones).
    pub intervals: HashMap<Uuid, LiveInterval>,
    /// Batches discarded as stale, in canonical order. A projection outcome,
    /// not an error: silent to end users, recorded here (I5).
    pub discarded: Vec<Discard>,
}

impl Projection {
    /// True when `id` has been created and not since deleted.
    pub fn is_live(&self, id: Uuid) -> bool {
        self.intervals
            .get(&id)
            .is_some_and(|i| i.deleted_at.is_none())
    }

    /// True when `id` carries a tombstone.
    pub fn is_tombstoned(&self, id: Uuid) -> bool {
        self.intervals
            .get(&id)
            .is_some_and(|i| i.deleted_at.is_some())
    }
}

/// Replays `log` into a projection.
pub fn project(log: &OperationLog) -> Projection {
    project_from(&Document::default(), log)
}

/// A write position that strictly precedes every valid one.
///
/// Valid batches have `counterBase >= 1` (B3), so `(0, 0, 0)` is below every
/// real position at the same causal past size and below every larger one. This
/// is what lets a baseline document's entities be LIVE for operations that
/// follow it without inventing a fake batch to have created them.
pub const BASELINE: WritePosition = WritePosition {
    batch: crate::operation::Position {
        causal_past_size: 0,
        counter_base: 0,
        actor: 0,
    },
    operation_index: 0,
};

/// Replays `log` on top of `baseline`.
///
/// `baseline` is a previously projected state — FORMAT-SPEC §5's snapshot
/// role. Its entities are seeded as live from [`BASELINE`], so a later
/// `setFields` finds its target within a live interval (I5 branch 2) rather
/// than before creation. Replaying `log` on top of the state `log`'s own
/// prefix produced must give the same answer as replaying the whole log, which
/// is vector V6's requirement.
///
/// Tombstones are NOT carried by a baseline document, because a document is a
/// set of live entities. A baseline therefore cannot block resurrection of
/// something deleted before it was taken; §5.1 S4 exists to carry tombstones
/// alongside a snapshot and is not implemented (issue 0027). Until it is, do
/// not use a baseline across a compaction that dropped a delete.
pub fn project_from(baseline: &Document, log: &OperationLog) -> Projection {
    project_from_with_tombstones(baseline, &[], log)
}

/// [`project_from`], with the tombstones a snapshot retained (§5.1 S4).
///
/// A compacted delete leaves no operation behind, so without this a later
/// `createEntity` for that id would find nothing blocking it and RESURRECT
/// what the user deleted. I3 forbids exactly that.
///
/// The tombstones are seeded as intervals closed at [`BASELINE`] with no
/// deleting actor, so E6's authorized-restoration path — which reads
/// provenance — cannot match them. That is the conservative direction: after a
/// compaction nobody can prove whose undo wrote a tombstone, and refusing to
/// restore is the answer that cannot lose data.
pub fn project_from_with_tombstones(
    baseline: &Document,
    tombstones: &[Uuid],
    log: &OperationLog,
) -> Projection {
    let mut p = seeded(baseline, tombstones);
    for batch in log.canonical_order() {
        apply_batch(&mut p, batch);
    }
    p
}

/// [`project_from_with_tombstones`], guarding only the batches in `guarded`.
///
/// The slow admission path replays the WHOLE log to place a new or newly
/// promoted batch at its canonical position, and most of that log is history
/// that has already been accepted. Guarding it wholesale would refuse to open
/// or extend any plan written before R2.3 was enforced — the same "the tool
/// that made the mess refuses to let anyone edit their way out of it" failure
/// the introduced-errors filter avoids in `accept_external`.
///
/// So the guard applies to batch ids the caller names: the batch being
/// admitted, plus whatever its delivery newly promoted. Everything else
/// replays exactly as [`project_from_with_tombstones`] would.
pub fn project_from_with_tombstones_guarding(
    baseline: &Document,
    tombstones: &[Uuid],
    log: &OperationLog,
    guarded: &HashSet<Uuid>,
) -> Result<Projection, AdmissionFailure> {
    let mut p = seeded(baseline, tombstones);
    for batch in log.canonical_order() {
        apply_batch_inner(&mut p, batch, guarded.contains(&batch.batch_id))?;
    }
    Ok(p)
}

/// A projection seeded with `baseline`'s live entities and `tombstones`,
/// before any batch is applied. Shared by the guarded and unguarded replays so
/// the two cannot diverge in how they start.
fn seeded(baseline: &Document, tombstones: &[Uuid]) -> Projection {
    let mut p = Projection {
        document: baseline.clone(),
        ..Projection::default()
    };
    for entity in baseline_ids(baseline) {
        p.intervals.insert(
            entity,
            LiveInterval {
                created_at: BASELINE,
                deleted_at: None,
                deleted_by: None,
            },
        );
    }
    for &id in tombstones {
        p.intervals.insert(
            id,
            LiveInterval {
                created_at: BASELINE,
                deleted_at: Some(BASELINE),
                deleted_by: None,
            },
        );
    }
    p
}

fn baseline_ids(document: &Document) -> Vec<Uuid> {
    document
        .levels
        .iter()
        .map(|l| l.id)
        .chain(document.walls.iter().map(|w| w.id))
        .chain(document.openings.iter().map(|o| o.id))
        .chain(document.joins.iter().map(|j| j.id))
        .chain(document.room_records.iter().map(|r| r.id))
        .chain(document.labels.iter().map(|l| l.id))
        .chain(document.dimensions.iter().map(|d| d.id))
        .chain(document.asset_instances.iter().map(|a| a.id))
        .chain(document.construction_bodies.iter().map(|b| b.id))
        .chain(document.hosted_openings.iter().map(|o| o.id))
        .chain(document.level_connections.iter().map(|c| c.id))
        .chain(document.staircases.iter().map(|s| s.id))
        .collect()
}

/// Applies `batch` to a projection that already covers everything BEFORE it in
/// canonical order.
///
/// This is one step of [`project_from_with_tombstones`], exposed so a caller
/// holding a current projection can extend it instead of rebuilding it. The
/// precondition is the whole contract: `batch` must sort strictly LAST among
/// the batches `p` already reflects, because a batch that sorts earlier can
/// change how later ones resolve under LWW1 and only a replay can discover
/// that.
///
/// [`crate::session::Session`] is the caller, and a locally minted batch
/// always satisfies the precondition: its frontier observes every operation in
/// the log, so its `causalPastSize` is strictly greater than that of any batch
/// already there — each of those excludes at least its own operations from its
/// own past.
pub fn append_batch(p: &mut Projection, batch: &Batch) {
    apply_batch(p, batch)
}

/// Why a level deletion is refused (BEHAVIOR-SPEC R2.2/R2.3).
///
/// Public so the CLI and the bridge can ask BEFORE minting a batch and report
/// the same reasons the core would, rather than each restating the rule.
/// [`level_deletion_refusals`] is that entry point.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LevelDeletionBlock {
    /// No live level carries this id — a stale or mistyped target. Reported
    /// rather than silently ignored: a caller asking to delete something that
    /// is not there has a bug, and "nothing happened" hides it.
    #[error("level {0} does not exist")]
    UnknownLevel(Uuid),
    /// R2.2 requires at least one level at every step. Deleting the last one
    /// is refused even when a later operation in the same batch would create a
    /// replacement: the invariant is evaluated AFTER EACH operation, so the
    /// replacement must be created first.
    #[error("level {0} is the only level; create a replacement before deleting it")]
    LastLevel(Uuid),
    /// R2.3: deletion never cascades. The blocking kinds and counts are listed
    /// so the caller can say what to remove, and removing them EARLIER in the
    /// same batch is what makes the deletion admissible.
    #[error(
        "level {level} still holds {count} {kind}; delete them first (deletion never cascades)"
    )]
    Occupied {
        level: Uuid,
        kind: &'static str,
        count: usize,
    },
    /// R2.3's "otherwise referenced by surviving data" case.
    #[error("level {level} still references {count} background image(s); detach them first")]
    BackgroundImages { level: Uuid, count: usize },
}

/// Every reason `level_id` may not be deleted from `document`, or empty when
/// it may.
///
/// Evaluated against the state that exists BEFORE the deletion, which is the
/// only state that can report all blocking contents: deletion removes the
/// level's background descriptors and leaves its walls with dangling refs.
///
/// Order is fixed — existence, then last-level, then contents by kind, then
/// references — so two callers reporting the same refusal produce the same
/// list.
pub fn level_deletion_refusals(document: &Document, level_id: Uuid) -> Vec<LevelDeletionBlock> {
    let Some(level) = document.level(level_id) else {
        return vec![LevelDeletionBlock::UnknownLevel(level_id)];
    };
    let mut blocks = Vec::new();
    if document.levels.len() <= 1 {
        blocks.push(LevelDeletionBlock::LastLevel(level_id));
    }
    // One pass over the level's walls, reused for both counts: a `Vec` probed
    // once per opening is O(walls x openings) and this runs on every level
    // deletion a busy document attempts.
    let wall_ids: HashSet<Uuid> = document.walls_on_level(level_id).map(|w| w.id).collect();
    if !wall_ids.is_empty() {
        blocks.push(LevelDeletionBlock::Occupied {
            level: level_id,
            kind: "wall(s)",
            count: wall_ids.len(),
        });
    }
    // The current schema associates openings and joins through hosted/member
    // walls, and room records through their own `levelId`. Future entity/
    // reference kinds must extend this inventory.
    let openings = document
        .openings
        .iter()
        .filter(|o| wall_ids.contains(&o.host_wall_id))
        .count();
    if openings > 0 {
        blocks.push(LevelDeletionBlock::Occupied {
            level: level_id,
            kind: "opening(s)",
            count: openings,
        });
    }
    // Reuse the membership set rather than resolving each join member through
    // a linear scan of the whole document's wall list.
    let joins = document
        .joins
        .iter()
        .filter(|join| {
            !join.members.is_empty()
                && join
                    .members
                    .iter()
                    .all(|member| wall_ids.contains(&member.wall_id))
        })
        .count();
    if joins > 0 {
        blocks.push(LevelDeletionBlock::Occupied {
            level: level_id,
            kind: "join(s)",
            count: joins,
        });
    }
    // R5.2.8 and F38: records live on levels and count among the blocking
    // kinds, reported as `Room: 1` beside the walls and joins. Moving a record
    // off the level is the R5.2.2 `levelId` update, not a deletion — which is
    // exactly what this refusal tells the caller to do first.
    let rooms = document.room_records_on_level(level_id).count();
    if rooms > 0 {
        blocks.push(LevelDeletionBlock::Occupied {
            level: level_id,
            kind: "room record(s)",
            count: rooms,
        });
    }
    for (kind, count) in [
        (
            "construction body(s)",
            document
                .construction_bodies
                .iter()
                .filter(|b| b.level_id == level_id)
                .count(),
        ),
        (
            "hosted opening(s)",
            document
                .hosted_openings
                .iter()
                .filter(|o| document.opening_host_level(o.host) == Some(level_id))
                .count(),
        ),
        (
            "level connection(s)",
            document
                .level_connections
                .iter()
                .filter(|c| c.from.level_id == level_id || c.to.level_id == level_id)
                .count(),
        ),
    ] {
        if count > 0 {
            blocks.push(LevelDeletionBlock::Occupied {
                level: level_id,
                kind,
                count,
            });
        }
    }
    if !level.background_images.is_empty() {
        blocks.push(LevelDeletionBlock::BackgroundImages {
            level: level_id,
            count: level.background_images.len(),
        });
    }
    blocks
}

/// A newly effective batch's admission refusal, separate from replay discard.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionFailure {
    #[error("level deletion refused: {0:?}")]
    LevelDeletion(Vec<LevelDeletionBlock>),
    #[error("wall contact refused: {0:?}")]
    WallContact(Vec<WallContactBlock>),
    /// BEHAVIOR-SPEC §4.4 refusals decided at admission — raw writes the
    /// typed document cannot carry or must not take (`levelId` on an opening
    /// or wall, invalid enum values, an unverified host change) — together
    /// with the validation errors of the batch's candidate state, so the one
    /// primary diagnostic can be chosen across both by R4.4's precedence.
    #[error("opening refused: {diagnostics:?}")]
    Opening {
        diagnostics: Vec<(OpeningCode, Uuid)>,
        /// The candidate state's validation errors: their §4.4 code where
        /// they have one, and their message.
        validation: Vec<(Option<OpeningCode>, String)>,
    },
}

/// [`append_batch`], enforcing new-admission level and wall-contact rules.
///
/// The guard runs immediately before each EFFECTIVE level deletion — after
/// stale classification, so a discarded batch is never judged — against the
/// intermediate document that operation would see. Removing a level's walls
/// EARLIER in the same batch therefore makes deleting it admissible, and
/// creating a replacement level earlier makes deleting the last one
/// admissible; neither is a cascade, because the caller wrote both operations.
/// Wall-axis refusals instead compare the batch's final document to its
/// starting document: temporary intermediate contact is allowed (R3.1.2).
///
/// `p` is untouched on refusal: the batch is applied to a scratch copy that is
/// only installed once every operation has passed.
pub fn append_batch_guarded(p: &mut Projection, batch: &Batch) -> Result<(), AdmissionFailure> {
    append_batch_journaled(p, batch).map(|_| ())
}

/// [`append_batch_guarded`], handing the rollback journal back to the caller.
///
/// A caller with its own final checks — whether replay discarded the batch,
/// whether the document still validates, whether the levels emptied — can run
/// them against the REAL projection and [`rollback`] if any refuses, instead of
/// projecting onto a copy and keeping the copy only if every check passes.
///
/// That is the whole saving. [`crate::session::Session`] took a copy of the
/// projection per command for exactly this reason, so a rejected command and an
/// accepted one both cost a copy of the document; now neither does.
///
/// On `Err` the rollback has already happened and `p` is untouched. On `Ok` the
/// batch is applied and the journal will undo it exactly, collection ORDER
/// included, until it is dropped.
pub fn append_batch_journaled(
    p: &mut Projection,
    batch: &Batch,
) -> Result<BatchJournal, AdmissionFailure> {
    apply_batch_inner(p, batch, true)
}

/// Undoes exactly what `journal` recorded.
///
/// Edits are undone in reverse, which is what makes the restoration exact
/// rather than approximate: when an edit is undone every later one is already
/// gone, so the collection it touched is in the state that edit left it in, and
/// an entity goes back to the index it was taken from.
pub fn rollback(p: &mut Projection, journal: BatchJournal) {
    for edit in journal.document.into_iter().rev() {
        match edit {
            DocEdit::Added(id) => {
                p.document.remove_entity(id);
            }
            // `insert_entity` replaces in place, and the id is present because
            // every later edit is already undone.
            DocEdit::Replaced(prior) => p.document.insert_entity(prior),
            DocEdit::Removed { entity, index } => restore_at(&mut p.document, entity, index),
        }
    }
    for edit in journal.intervals.into_iter().rev() {
        match edit.prior {
            Some(interval) => p.intervals.insert(edit.id, interval),
            None => p.intervals.remove(&edit.id),
        };
    }
    p.discarded.truncate(journal.discarded);
}

/// Applies one batch atomically, or discards it as stale (B1, I5).
///
/// The batch is classified against a SCRATCH copy so a discard leaves no trace:
/// atomicity forbids partial application, and "check everything first" is not
/// enough on its own because an operation may legitimately depend on an
/// earlier operation of the same batch (B4).
fn apply_batch(p: &mut Projection, batch: &Batch) {
    // Unguarded: legacy replay and open must reproduce exactly what they
    // always did, including for documents written before R2.3 was enforced.
    let _ = apply_batch_inner(p, batch, false);
}

/// Everything one batch changed, and enough to undo it.
///
/// # Why a journal replaced a copy
///
/// B1 forbids partial application, and the previous shape bought that with a
/// clone of the whole document and the whole interval map per batch: apply to
/// the copy, install it only if every operation survived. Correct, and O(the
/// document) per batch whatever the batch does — so a full replay of n batches
/// was O(n²) and a single commit copied the plan the user is editing.
///
/// A journal is bounded by the BATCH instead: k operations record at most k
/// document edits and k interval edits, and undoing them in reverse restores
/// the projection exactly. What is stored is the displaced value, never the
/// surrounding collection, so the cost follows what the batch touched.
///
/// It is not a general undo facility and must not become one. R8.1 undo is a
/// compensating batch in the log (§4.2 O4); this never leaves `apply_batch`.
#[derive(Debug, Default)]
pub struct BatchJournal {
    document: Vec<DocEdit>,
    intervals: Vec<IntervalEdit>,
    /// `Projection::discarded` length before the batch. A discard SURVIVES a
    /// rollback inside replay — it is a projection outcome, not a mutation —
    /// but a caller rejecting the whole command has to drop it too.
    discarded: usize,
    /// Whether replay discarded this batch rather than applying it.
    discarded_batch: bool,
}

impl BatchJournal {
    /// True when replay DISCARDED the batch at its canonical position — stale
    /// (I5) or malformed — rather than applying it.
    ///
    /// The projection's document is unchanged either way; what differs is that
    /// a discard also recorded a [`Discard`]. A caller that treats a discarded
    /// local command as a rejection uses [`rollback`] to drop that record too,
    /// because from the user's side the command never happened.
    pub fn was_discarded(&self) -> bool {
        self.discarded_batch
    }
}

/// One reversible change to the document.
#[derive(Debug)]
enum DocEdit {
    /// `insert_entity` pushed an id the document did not hold.
    Added(Uuid),
    /// `insert_entity` replaced an existing entity in place; this is what it
    /// displaced.
    Replaced(Entity),
    /// `remove_entity` took an entity out of `index` of its own collection.
    ///
    /// The index is recorded because collection ORDER is a stability
    /// requirement even though FORMAT-SPEC C1 says it carries no meaning:
    /// `Document::insert_entity` documents that the CLI numbers walls by
    /// position, so returning a wall to the end would renumber every wall
    /// after it.
    Removed { entity: Entity, index: usize },
}

/// One reversible change to an entity's live interval.
#[derive(Debug)]
struct IntervalEdit {
    id: Uuid,
    /// `None` when the id had no interval at all before this edit.
    prior: Option<LiveInterval>,
}

fn apply_batch_inner(
    p: &mut Projection,
    batch: &Batch,
    guarded: bool,
) -> Result<BatchJournal, AdmissionFailure> {
    let mut journal = BatchJournal {
        discarded: p.discarded.len(),
        ..BatchJournal::default()
    };
    // COLLECTED, not returned early. A batch that is later discarded as stale
    // or malformed never takes effect at all, so it must not be refused for
    // what one of its operations would have done — returning here would turn a
    // silent discard into a rejection and change external delivery semantics
    // for batches this guard was never meant to judge. The blocks are raised
    // only if the batch survives to be published.
    let mut blocks: Vec<LevelDeletionBlock> = Vec::new();
    // §4.4 refusals of raw writes, likewise collected and raised only if the
    // batch survives; the offending fields are dropped from the applied
    // operation so the rest of the batch still yields a candidate state to
    // validate (R4.4 ranks across both).
    let mut opening_diags: Vec<(OpeningCode, Uuid)> = Vec::new();
    // Openings with an ATTEMPTED `hostWallId` write (even of the current
    // value, even later restored), and ids this batch deleted.
    let mut host_writes: HashSet<Uuid> = HashSet::new();
    let mut removed: HashSet<Uuid> = HashSet::new();

    for (index, op) in batch.operations.iter().enumerate() {
        let at = batch.write_position(index);
        let state = classify(&p.intervals, op.entity_id, at);

        let stale = match &op.kind {
            // A create ESTABLISHES existence at its own position: it is never
            // stale for want of its own target. It is refused only against a
            // tombstone it is not authorized to restore.
            OperationKind::CreateEntity { .. } => {
                state == TargetState::AfterDeletion && !authorized_restore(&p.intervals, op, batch)
            }
            // A delete against a live target closes the interval; against an
            // absent or already-tombstoned target it is stale on the same
            // terms as branch 1 or branch 3.
            OperationKind::DeleteEntity => state != TargetState::Live,
            // The interval test governs operations REQUIRING a pre-existing
            // target, classified at that operation's own write position.
            OperationKind::SetFields { .. } => state != TargetState::Live,
        };

        if stale {
            // If ANY operation is stale the WHOLE batch is discarded,
            // including operations whose own targets were live (B1). The ones
            // that already applied are undone here; the discard itself is the
            // projection's outcome and stays.
            rollback(p, journal);
            p.discarded.push(Discard {
                batch_id: batch.batch_id,
                operation_id: op.operation_id,
                entity_id: op.entity_id,
                reason: DiscardReason::Stale(state),
            });
            return Ok(BatchJournal {
                discarded: p.discarded.len() - 1,
                discarded_batch: true,
                ..BatchJournal::default()
            });
        }

        // R2.2/R2.3, at the only moment that can see the answer: this
        // operation is EFFECTIVE (not stale), and the document still holds
        // everything the deletion would take with it.
        if guarded
            && blocks.is_empty()
            && matches!(op.kind, OperationKind::DeleteEntity)
            && p.document.level(op.entity_id).is_some()
        {
            // First refusal only: the operations after a blocked deletion run
            // against a document that deletion already changed, so anything
            // they might add would describe a state the caller never asked
            // for. The pass continues so a later stale or malformed operation
            // can still discard the batch instead.
            blocks = level_deletion_refusals(&p.document, op.entity_id);
        }

        let sanitized;
        if guarded {
            match &op.kind {
                OperationKind::SetFields { fields }
                    if fields.contains_key("hostWallId")
                        && p.document.openings.iter().any(|o| o.id == op.entity_id) =>
                {
                    // On an OPENING only: elsewhere it is an unknown field.
                    host_writes.insert(op.entity_id);
                }
                OperationKind::DeleteEntity => {
                    removed.insert(op.entity_id);
                }
                _ => {}
            }
        }
        let applied = match guarded
            .then(|| {
                raw_write_diagnostics(
                    &p.document,
                    op,
                    state == TargetState::Live,
                    removed.contains(&op.entity_id),
                )
            })
            .flatten()
        {
            Some((diags, clean)) => {
                opening_diags.extend(diags);
                match clean {
                    Some(clean) => {
                        sanitized = clean;
                        &sanitized
                    }
                    // Not applied at all (a create on a live id): the batch
                    // is refused, and applying it would corrupt rollback.
                    None => continue,
                }
            }
            None => op,
        };
        if apply_operation(
            &mut p.document,
            &mut p.intervals,
            &mut journal,
            applied,
            at,
            batch.actor_id,
        )
        .is_err()
        {
            rollback(p, journal);
            p.discarded.push(Discard {
                batch_id: batch.batch_id,
                operation_id: op.operation_id,
                entity_id: op.entity_id,
                reason: DiscardReason::MalformedPayload,
            });
            return Ok(BatchJournal {
                discarded: p.discarded.len() - 1,
                discarded_batch: true,
                ..BatchJournal::default()
            });
        }
    }

    // The batch survived every discard path, so it is about to be published:
    // NOW the refusal is real.
    if !blocks.is_empty() {
        rollback(p, journal);
        return Err(AdmissionFailure::LevelDeletion(blocks));
    }
    if guarded {
        opening_diags.extend(unverified_rehosts(p, &journal, &host_writes));
        if !opening_diags.is_empty() {
            let validation = p
                .document
                .validate()
                .iter()
                .map(|e| (e.opening_code(), e.to_string()))
                .collect();
            rollback(p, journal);
            return Err(AdmissionFailure::Opening {
                diagnostics: opening_diags,
                validation,
            });
        }
    }
    if guarded {
        // The affected axes come from the journal rather than from a
        // comparison against a retained copy of the pre-batch document — which
        // is the copy this function no longer makes.
        let contacts = wall_contact_refusals_among(&p.document, &changed_axes(p, &journal));
        if !contacts.is_empty() {
            rollback(p, journal);
            return Err(AdmissionFailure::WallContact(contacts));
        }
    }

    Ok(journal)
}

/// The wall ids whose FINAL axis differs from the one they had before the
/// batch, read off the journal.
///
/// Exactly what comparing the pre-batch document to the post-batch one
/// computes, and the equivalence is the whole argument for reading it here:
/// a wall the batch never touched cannot have changed, and a wall it touched
/// repeatedly is judged against its state before the FIRST of those edits,
/// which is why the earliest journal entry per id wins. A wall the batch
/// removed is absent from the final document and is not affected — deletion
/// repairs a contact, it never creates one.
fn changed_axes(p: &Projection, journal: &BatchJournal) -> HashSet<Uuid> {
    let mut before: HashMap<Uuid, Option<&Entity>> = HashMap::new();
    for edit in &journal.document {
        let (id, prior) = match edit {
            DocEdit::Added(id) => (*id, None),
            DocEdit::Replaced(entity) => (entity.id(), Some(entity)),
            DocEdit::Removed { entity, .. } => (entity.id(), Some(entity)),
        };
        before.entry(id).or_insert(prior);
    }
    before
        .into_iter()
        .filter(|(id, prior)| {
            p.document.wall(*id).is_some_and(|now| match prior {
                Some(Entity::Wall(was)) => !same_axis(was, now),
                // Absent before, or present as something that was not a wall:
                // either way this axis is new to the document.
                _ => true,
            })
        })
        .map(|(id, _)| id)
        .collect()
}

/// Puts `entity` back at `index` of the collection its type names.
///
/// The mirror of `Document::remove_entity`, and coupled to it: a new entity
/// kind must appear in both. The exhaustive match is what says so — adding a
/// variant to [`Entity`] fails to compile here.
fn restore_at(document: &mut Document, entity: Entity, index: usize) {
    match entity {
        Entity::Level(l) => document.levels.insert(index, l),
        Entity::Wall(w) => document.walls.insert(index, w),
        Entity::Opening(o) => document.openings.insert(index, o),
        Entity::WallJoin(j) => document.joins.insert(index, j),
        Entity::RoomRecord(r) => document.room_records.insert(index, r),
        Entity::Label(l) => document.labels.insert(index, l),
        Entity::Dimension(d) => document.dimensions.insert(index, d),
        Entity::AssetInstance(a) => document.asset_instances.insert(index, a),
        Entity::ConstructionBody(b) => document.construction_bodies.insert(index, b),
        Entity::HostedOpening(o) => document.hosted_openings.insert(index, o),
        Entity::LevelConnection(c) => document.level_connections.insert(index, c),
        Entity::Staircase(s) => document.staircases.insert(index, s),
    }
}

/// `Document::remove_entity`, also reporting which index the entity came from.
///
/// The position is found before the removal because afterwards it is gone, and
/// only the index it occupied can put it back where it was.
fn remove_indexed(document: &mut Document, id: Uuid) -> Option<(Entity, usize)> {
    let index = position_of(document, id)?;
    let entity = document.remove_entity(id)?;
    Some((entity, index))
}

/// Where `id` sits within its own collection, if the document holds it.
///
/// Ids are unique across collections (E1, enforced by `Document::validate`),
/// so the first collection that carries it is the one that holds it.
fn position_of(document: &Document, id: Uuid) -> Option<usize> {
    document
        .levels
        .iter()
        .position(|x| x.id == id)
        .or_else(|| document.walls.iter().position(|x| x.id == id))
        .or_else(|| document.openings.iter().position(|x| x.id == id))
        .or_else(|| document.joins.iter().position(|x| x.id == id))
        .or_else(|| document.room_records.iter().position(|x| x.id == id))
        // The Tier B collections. `restore_at`'s exhaustive match forces a new
        // variant to be handled there; nothing forces it HERE, because this is
        // an `or_else` chain rather than a match.
        //
        // Omitting a collection is therefore silent, and the damage is NOT a
        // lost undo. `remove_indexed` calls this with `?` BEFORE
        // `remove_entity`, so a missing collection makes the whole removal
        // short-circuit: `deleteEntity` journals nothing and deletes nothing,
        // and the entity stays in the document while its interval is
        // tombstoned by the block that follows. A delete that reports success
        // and leaves the entity behind, not a delete that cannot be undone.
        .or_else(|| document.labels.iter().position(|x| x.id == id))
        .or_else(|| document.dimensions.iter().position(|x| x.id == id))
        .or_else(|| document.asset_instances.iter().position(|x| x.id == id))
        .or_else(|| document.construction_bodies.iter().position(|x| x.id == id))
        .or_else(|| document.hosted_openings.iter().position(|x| x.id == id))
        .or_else(|| document.level_connections.iter().position(|x| x.id == id))
        .or_else(|| document.staircases.iter().position(|x| x.id == id))
}

/// I5's three-branch classification, evaluated at `at` rather than "now".
fn classify(intervals: &HashMap<Uuid, LiveInterval>, id: Uuid, at: WritePosition) -> TargetState {
    let Some(interval) = intervals.get(&id) else {
        return TargetState::BeforeCreation;
    };
    if interval.created_at >= at {
        return TargetState::BeforeCreation;
    }
    match interval.deleted_at {
        Some(d) if d < at => TargetState::AfterDeletion,
        _ => TargetState::Live,
    }
}

/// E6's sole reuse exception: authorized restoration of a tombstoned identity
/// by the same device's own undo (BEHAVIOR-SPEC R8.1.3).
///
/// Authorization is read from PROVENANCE rather than from a flag on the
/// operation: the restoring create is authorized exactly when the tombstone it
/// would reopen was written by the same actor. "Tombstones created by sync
/// merge are never resurrected" is then automatic, because a merged tombstone
/// carries another actor's id.
///
/// # The limit, and where it is actually enforced
///
/// Within a single actor stream this ALSO permits a create of a tombstoned id
/// that is not a redo, because nothing downstream of the log can tell the two
/// apart — an undo's compensating batch is an ordinary batch. §4.2 O4 puts
/// that check on the AUTHORING replica at command time.
///
/// [`crate::session::Session::commit`] enforces it: an ordinary command may
/// not create an id this session's projection has tombstoned, and only
/// [`crate::session::Session::redo`] takes the restoring path. Adversarial
/// review (Codex, gpt-5.6-sol, 2026-08-31) found that this paragraph described
/// an enforcement that did not exist — `commit` was public and accepted any
/// create — so an ordinary command could resurrect a tombstone. The check is
/// now there, and this comment names it rather than assuming it.
fn authorized_restore(
    intervals: &HashMap<Uuid, LiveInterval>,
    op: &Operation,
    batch: &Batch,
) -> bool {
    intervals
        .get(&op.entity_id)
        .and_then(|i| i.deleted_by)
        .is_some_and(|by| by == batch.actor_id)
}

/// The payload could not be applied to its target.
struct MalformedPayload;

/// Applies one operation and records what it displaced.
///
/// Nothing is written without a journal entry naming its prior value, and an
/// operation that fails writes neither. That pairing is the invariant
/// [`rollback`] depends on.
fn apply_operation(
    document: &mut Document,
    intervals: &mut HashMap<Uuid, LiveInterval>,
    journal: &mut BatchJournal,
    op: &Operation,
    at: WritePosition,
    actor: Uuid,
) -> Result<(), MalformedPayload> {
    match &op.kind {
        OperationKind::CreateEntity { entity } => {
            // A create whose payload names a different id than the operation
            // targets would insert one entity and record an interval for
            // another. The two must agree.
            if entity.id() != op.entity_id {
                return Err(MalformedPayload);
            }
            // LWW4 lets a create REPLACE an existing entity, so what it
            // displaces has to be kept. `entity` costs one lookup that
            // `insert_entity`'s own scan repeats; both are linear in the
            // collection and neither copies the document.
            journal.document.push(match document.entity(op.entity_id) {
                Some(prior) => DocEdit::Replaced(prior),
                None => DocEdit::Added(op.entity_id),
            });
            document.insert_entity(entity.clone());
            journal.intervals.push(IntervalEdit {
                id: op.entity_id,
                prior: intervals.insert(
                    op.entity_id,
                    LiveInterval {
                        created_at: at,
                        deleted_at: None,
                        deleted_by: None,
                    },
                ),
            });
        }
        OperationKind::SetFields { fields } => {
            let Some(current) = document.entity(op.entity_id) else {
                return Err(MalformedPayload);
            };
            let Some(updated) = merge_fields(&current, fields) else {
                // The payload does not produce a valid entity. Doing nothing
                // and reporting success would let a batch be accepted, marked
                // dirty and pushed onto the undo stack while changing nothing.
                return Err(MalformedPayload);
            };
            // `id` and `type` are rejected at the log's framing check, so a
            // merge cannot change identity — but if that guard ever moved,
            // inserting under a different id would leave the target in place
            // and add a second entity. Cheap to assert, catastrophic to miss.
            if updated.id() != op.entity_id {
                return Err(MalformedPayload);
            }
            // `current` is the displaced value, already in hand.
            journal.document.push(DocEdit::Replaced(current));
            document.insert_entity(updated);
        }
        OperationKind::DeleteEntity => {
            if let Some((entity, index)) = remove_indexed(document, op.entity_id) {
                journal.document.push(DocEdit::Removed { entity, index });
            }
            if let Some(interval) = intervals.get_mut(&op.entity_id) {
                journal.intervals.push(IntervalEdit {
                    id: op.entity_id,
                    prior: Some(*interval),
                });
                interval.deleted_at = Some(at);
                interval.deleted_by = Some(actor);
            }
        }
    }
    Ok(())
}

/// Applies a `setFields` payload to an entity.
///
/// O3: fields are addressed by SERIALIZED NAME at top level, and set- or
/// array-valued fields are replaced WHOLESALE. Going through JSON is what
/// makes that literally true rather than approximately true — the payload
/// names the same strings the format writes, and no field is merged
/// element-wise because none is descended into.
///
/// Returns `None` when the result would not deserialize, which means the
/// payload named a field with an incompatible value. The operation then has no
/// effect; commit-time validation (O4) is what should have caught it, and
/// [`crate::session::Session`] runs it.
pub(crate) fn merge_fields(
    entity: &Entity,
    fields: &serde_json::Map<String, Value>,
) -> Option<Entity> {
    let mut value = serde_json::to_value(entity).ok()?;
    let object = value.as_object_mut()?;
    for (name, new) in fields {
        // `null` clears an optional field to absent (§4.2 O2). Every other
        // value replaces wholesale.
        if new.is_null() {
            object.remove(name);
        } else {
            object.insert(name.clone(), new.clone());
        }
    }
    serde_json::from_value(value).ok()
}

/// The `setFields` payload that would restore `entity`'s current values for
/// the field names `fields` names.
///
/// This is how an undo of a field edit is built: capture the prior values of
/// exactly the fields the forward operation touched. A field the forward
/// operation ADDED (absent before) is restored by `null`, which §4.2 O2
/// defines as clearing to absent.
pub fn prior_values(
    entity: &Entity,
    fields: &serde_json::Map<String, Value>,
) -> serde_json::Map<String, Value> {
    let before = serde_json::to_value(entity).unwrap_or(Value::Null);
    let mut out = serde_json::Map::new();
    for name in fields.keys() {
        let prior = before.get(name).cloned().unwrap_or(Value::Null);
        out.insert(name.clone(), prior);
    }
    out
}

/// §4.4 refusals a raw operation carries that the typed document cannot
/// express or must not take: a create on an id LIVE as an entity of another
/// kind (rollback cannot restore across collections; a same-kind live create
/// is LWW4 replacement and stays legal), a create of an id this batch
/// deleted (a host deleted and recreated would carry its openings across
/// levels, R4.3.2), a `levelId` on a created opening, and — in `SetFields`
/// — a `levelId` written on an opening (R4.0.2, R4.3.5) or an existing wall
/// (R4.3.2), ANY write, and an opening enum field naming no variant.
/// Returns the diagnostics and the operation to apply instead (`None`:
/// apply nothing), or `None` when there is nothing to refuse.
#[allow(clippy::type_complexity)]
fn raw_write_diagnostics(
    doc: &Document,
    op: &Operation,
    live: bool,
    removed_here: bool,
) -> Option<(Vec<(OpeningCode, Uuid)>, Option<Operation>)> {
    let collision = || Some((vec![(OpeningCode::EntityIdCollision, op.entity_id)], None));
    if let OperationKind::CreateEntity { entity } = &op.kind {
        // `live` is the operation's own interval classification (O(1)); the
        // document is consulted only for a live id.
        if live {
            let current = doc.entity(op.entity_id);
            let same_kind = current
                .as_ref()
                .is_some_and(|e| std::mem::discriminant(e) == std::mem::discriminant(entity));
            if !same_kind {
                return collision();
            }
            // A same-kind replacement (LWW4) is legal, but it must not do
            // what a SetFields may not: move a wall or opening to another
            // level, or rehost an opening.
            let code = match (current, entity) {
                (Some(Entity::Wall(was)), Entity::Wall(now)) if was.level_id != now.level_id => {
                    Some(OpeningCode::WallLevelImmutable)
                }
                (Some(Entity::Opening(was)), Entity::Opening(now))
                    if was.host_wall_id != now.host_wall_id =>
                {
                    Some(OpeningCode::OpeningDirectRehost)
                }
                (Some(Entity::Opening(_)), Entity::Opening(now))
                    if now.unknown.contains_key("levelId") =>
                {
                    Some(OpeningCode::OpeningLevelImmutable)
                }
                _ => None,
            };
            return code.map(|c| (vec![(c, op.entity_id)], None));
        }
        if removed_here {
            return collision();
        }
        if let Entity::Opening(o) = entity {
            if o.unknown.contains_key("levelId") {
                let mut clean = op.clone();
                if let OperationKind::CreateEntity {
                    entity: Entity::Opening(o),
                } = &mut clean.kind
                {
                    o.unknown.remove("levelId");
                }
                return Some((
                    vec![(OpeningCode::OpeningLevelImmutable, op.entity_id)],
                    Some(clean),
                ));
            }
        }
        return None;
    }
    let OperationKind::SetFields { fields } = &op.kind else {
        return None;
    };
    // Hot path: an ordinary edit writes none of the guarded fields, and the
    // document is not scanned for its kind at all.
    const GUARDED: [&str; 5] = ["levelId", "kind", "side", "hinge", "opensToward"];
    if !GUARDED.iter().any(|k| fields.contains_key(*k)) {
        return None;
    }
    let mut diags = Vec::new();
    let mut drop: Vec<&str> = Vec::new();
    if doc.openings.iter().any(|o| o.id == op.entity_id) {
        if fields.contains_key("levelId") {
            diags.push(OpeningCode::OpeningLevelImmutable);
            drop.push("levelId");
        }
        let invalid = |key: &str, ok: fn(&Value) -> bool| fields.get(key).is_some_and(|v| !ok(v));
        for (key, code, ok) in [
            (
                "kind",
                OpeningCode::InvalidOpeningKind,
                (|v: &Value| {
                    serde_json::from_value::<domowik_format::OpeningKind>(v.clone()).is_ok()
                }) as fn(&Value) -> bool,
            ),
            ("side", OpeningCode::InvalidSide, |v| {
                serde_json::from_value::<domowik_format::Side>(v.clone()).is_ok()
            }),
            ("hinge", OpeningCode::InvalidHinge, |v| {
                serde_json::from_value::<domowik_format::Hinge>(v.clone()).is_ok()
            }),
            ("opensToward", OpeningCode::InvalidOpensToward, |v| {
                serde_json::from_value::<domowik_format::Side>(v.clone()).is_ok()
            }),
        ] {
            if invalid(key, ok) {
                diags.push(code);
                drop.push(key);
            }
        }
    } else if doc.wall(op.entity_id).is_some() && fields.contains_key("levelId") {
        diags.push(OpeningCode::WallLevelImmutable);
        drop.push("levelId");
    }
    if diags.is_empty() {
        return None;
    }
    let mut clean = op.clone();
    if let OperationKind::SetFields { fields } = &mut clean.kind {
        for key in drop {
            fields.remove(key);
        }
    }
    Some((
        diags.into_iter().map(|c| (c, op.entity_id)).collect(),
        Some(clean),
    ))
}

/// Openings whose host this batch changed — or ATTEMPTED to write
/// (`host_writes`) — without a VERIFIED R4.3.3 split remap or its inverse
/// merge (`opening-direct-rehost`). Only the split may change a stored host
/// (R4.3.5). Verified from the batch's own before/after states, so undo,
/// redo and compensation need no trusted flag.
fn unverified_rehosts(
    p: &Projection,
    journal: &BatchJournal,
    host_writes: &HashSet<Uuid>,
) -> Vec<(OpeningCode, Uuid)> {
    let mut before: HashMap<Uuid, Option<&Entity>> = HashMap::new();
    for edit in &journal.document {
        let (id, prior) = match edit {
            DocEdit::Added(id) => (*id, None),
            DocEdit::Replaced(entity) => (entity.id(), Some(entity)),
            DocEdit::Removed { entity, .. } => (entity.id(), Some(entity)),
        };
        before.entry(id).or_insert(prior);
    }
    // Every end each wall held during the batch: before it, after each of
    // its edits (one edit's displaced state is the previous one's result),
    // and after it. A moved host's extent must be one of these.
    let mut held_ends: HashMap<Uuid, Vec<domowik_format::Point>> = HashMap::new();
    for edit in &journal.document {
        if let DocEdit::Replaced(Entity::Wall(w))
        | DocEdit::Removed {
            entity: Entity::Wall(w),
            ..
        } = edit
        {
            held_ends.entry(w.id).or_default().push(w.end());
        }
    }
    for wall in &p.document.walls {
        if let Some(ends) = held_ends.get_mut(&wall.id) {
            ends.push(wall.end());
        }
    }
    let held = |id: Uuid| held_ends.get(&id).map(Vec::as_slice).unwrap_or(&[]);
    let wall_before = |id: Uuid| -> Option<&Wall> {
        match before.get(&id) {
            Some(Some(Entity::Wall(w))) => Some(w),
            Some(_) => None,
            None => p.document.wall(id),
        }
    };
    // Walls this batch created (still live) and deleted (as they were).
    let created: Vec<&Wall> = before
        .iter()
        .filter(|(_, prior)| prior.is_none())
        .filter_map(|(id, _)| p.document.wall(*id))
        .collect();
    let deleted: Vec<&Wall> = before
        .iter()
        .filter_map(|(id, prior)| match prior {
            Some(Entity::Wall(w)) if p.document.wall(*id).is_none() => Some(w),
            _ => None,
        })
        .collect();

    let mut candidates: Vec<Uuid> = before
        .iter()
        .filter_map(|(id, prior)| match prior {
            Some(Entity::Opening(pre)) => p
                .document
                .openings
                .iter()
                .find(|o| o.id == *id)
                .is_some_and(|post| post.host_wall_id != pre.host_wall_id)
                .then_some(*id),
            _ => None,
        })
        .chain(host_writes.iter().copied())
        .collect();
    candidates.sort();
    candidates.dedup();

    let mut out = Vec::new();
    for id in candidates {
        let verified = (|| {
            let Some(Entity::Opening(pre)) = before.get(&id).copied().flatten() else {
                return false; // written but created here, or never an opening
            };
            let Some(post) = p.document.openings.iter().find(|o| o.id == id) else {
                return false; // host written, then deleted
            };
            if post.host_wall_id == pre.host_wall_id {
                // An explicit write of the unchanged host is legitimate only
                // as part of a split that leaves the opening on the RETAINED
                // parent (R4.3.3): verify that split.
                let (Some(parent_pre), Some(parent_post)) = (
                    wall_before(pre.host_wall_id),
                    p.document.wall(pre.host_wall_id),
                ) else {
                    return false;
                };
                // Either direction: a split shortening the parent, or its
                // inverse merge (undo) restoring it.
                return (split_chain(parent_pre, parent_post, &created)
                    && remap_holds(parent_pre, parent_post, pre, post))
                    || (split_chain(parent_post, parent_pre, &deleted)
                        && remap_holds(parent_post, parent_pre, post, pre));
            }
            let (Some(old_pre), Some(new_post)) = (
                wall_before(pre.host_wall_id),
                p.document.wall(post.host_wall_id),
            ) else {
                return false;
            };
            let new_pre = wall_before(post.host_wall_id);
            let old_post = p.document.wall(pre.host_wall_id);
            match (new_pre, old_post) {
                // Split: the child was created here; the parent keeps its id
                // and START (R3.1.3) and, with children this batch created,
                // chains exactly over its former span.
                (None, Some(parent_post)) => {
                    (split_chain(old_pre, parent_post, &created)
                        && chain_reaches(parent_post, old_pre.end(), &created, new_post.id)
                        && remap_holds(old_pre, new_post, pre, post))
                        || moved_split(
                            parent_post,
                            held(parent_post.id),
                            &created,
                            new_post.id,
                            |full| remap_holds(full, new_post, pre, post),
                        )
                }
                // Inverse merge (undo of a split): the child was deleted
                // here; the parent's former span and the deleted children
                // chain exactly over its new span.
                (Some(parent_pre), None) => {
                    (split_chain(new_post, parent_pre, &deleted)
                        && chain_reaches(parent_pre, new_post.end(), &deleted, old_pre.id)
                        && remap_holds(new_post, old_pre, post, pre))
                        || moved_split(
                            parent_pre,
                            held(parent_pre.id),
                            &deleted,
                            old_pre.id,
                            |full| remap_holds(full, old_pre, post, pre),
                        )
                }
                _ => false,
            }
        })();
        if !verified {
            out.push((OpeningCode::OpeningDirectRehost, id));
        }
    }
    out
}

/// Whether `w`'s endpoints lie on `full`'s axis: within the error of
/// constructing a split point in binary64 (a few ulps of the coordinates),
/// not an input tolerance — a child shifted by any visible amount is off
/// axis.
fn on_axis(full: &Wall, w: &Wall) -> bool {
    let (a, b) = (full.start(), full.end());
    let length = full.length_mm();
    if length == 0.0 {
        return false;
    }
    let on = |q: domowik_format::Point| {
        let cross = (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
        let scale = [a.x, a.y, b.x, b.y, q.x, q.y]
            .iter()
            .fold(1.0f64, |m, v| m.max(v.abs()));
        cross.abs() / length <= 8.0 * f64::EPSILON * scale
    };
    on(w.start()) && on(w.end())
}

/// Whether `parent` (its state after a split, or before a merge) is the
/// retained START child of `full` and, through children drawn from `pool`,
/// chains exactly to `full`'s end: same level, thickness, height and
/// styles, on `full`'s axis (within the binary64 construction error), each
/// child starting EXACTLY where the previous ends (R3.1.3's children share
/// their split points). Only children that continue the chain are used, so
/// unrelated collinear walls elsewhere — another split, another level —
/// neither join nor break it.
fn split_chain(full: &Wall, parent: &Wall, pool: &[&Wall]) -> bool {
    parent.start() == full.start()
        && parent.end() != full.end()
        && same_props(full, parent)
        && on_axis(full, parent)
        && chain_reaches(parent, full.end(), pool, parent.id)
}

/// Whether `parent` is the retained start piece of a host that this batch
/// MOVED and then split, with `remap` holding against that moved host.
///
/// A gesture may move a host's endpoints (a keep-parameters edit, R3.1.8)
/// and, at those final positions, split it (R3.1.2) in ONE batch. The
/// opening's `u` is then relative to the moved host, which the pre-batch
/// host does not show. The moved host is `parent`'s start and fields with an
/// end the host actually HELD during the batch (`held`: before it, after one
/// of its edits, or after it). Moving an end is a write the batch makes, so
/// the extent is certified by the batch, never inferred from where some new
/// wall happens to end (Codex GPT-6.1-Sol, 0055 round two: a raw batch that
/// created an adjacent wall and rehosted a door onto it, touching nothing
/// else, passed when any piece's end was accepted). The chain is checked
/// against the FULL moved axis, as [`split_chain`] does for an unmoved host:
/// the retained piece alone may be millimetres long, and an axis that short
/// cannot place a child metres away within construction error.
///
/// The inverse (undo of such a batch) is certified the same way: undo
/// inverts each operation against the state just before it, so it writes the
/// moved extent back before merging the pieces, and that extent is held.
///
/// Accepts exactly the composition: every piece created (or deleted) by the
/// batch, on the moved axis, chained by exact shared endpoints through the
/// opening's new host, the aperture strictly inside it and `u` bit for bit
/// from the shared formula. Costs O(e·p²) in the worst case for e held ends
/// and p pieces (each link of a chain searches the pool), and
/// runs only when the ordinary check has already failed.
fn moved_split(
    parent: &Wall,
    held: &[domowik_format::Point],
    pool: &[&Wall],
    through: Uuid,
    remap: impl Fn(&Wall) -> bool,
) -> bool {
    held.iter().any(|&end| {
        let mut full = parent.clone();
        full.end_mm = end.into();
        full.end() != parent.end()
            && full.length_mm() > 0.0
            && on_axis(&full, parent)
            && chain_along(&full, parent, pool, through)
            && remap(&full)
    })
}

/// [`chain_reaches`] from `from` to `full`'s end, judging every piece
/// against `full`'s axis rather than `from`'s.
fn chain_along(full: &Wall, from: &Wall, pool: &[&Wall], through: Uuid) -> bool {
    let to = full.end();
    let mut at = from.end();
    let mut seen = from.id == through;
    let mut steps = 0;
    while at != to {
        let Some(next) = pool.iter().find(|w| {
            w.start() == at && w.id != from.id && same_props(from, w) && on_axis(full, w)
        }) else {
            return false;
        };
        seen |= next.id == through;
        at = next.end();
        steps += 1;
        if steps > pool.len() {
            return false;
        }
    }
    seen
}

/// Whether, from `from`'s end, children from `pool` (matching `from`'s
/// properties and lying on its axis) chain by exact shared endpoints to
/// `to`, passing through the wall `through` (or `from` itself).
fn chain_reaches(from: &Wall, to: domowik_format::Point, pool: &[&Wall], through: Uuid) -> bool {
    let mut at = from.end();
    let mut seen = from.id == through;
    let mut steps = 0;
    while at != to {
        let Some(next) = pool.iter().find(|w| {
            w.start() == at && w.id != from.id && same_props(from, w) && on_axis(from, w)
        }) else {
            return false;
        };
        seen |= next.id == through;
        at = next.end();
        steps += 1;
        if steps > pool.len() {
            return false;
        }
    }
    seen
}

fn same_props(a: &Wall, b: &Wall) -> bool {
    (a.level_id, a.thickness_mm, a.height_mm, &a.style_refs)
        == (b.level_id, b.thickness_mm, b.height_mm, &b.style_refs)
}

/// Whether moving an opening from `big` (as `on_big`) onto its child
/// `small` (as `on_small`) is exactly R4.3.3's remap: the aperture strictly
/// inside the child, `u` from the ONE shared formula bit for bit, every
/// other field unchanged.
fn remap_holds(big: &Wall, small: &Wall, on_big: &Opening, on_small: &Opening) -> bool {
    let (a, b) = (big.start(), big.end());
    let length = big.length_mm();
    let (s, e) = (
        param_along(a, b, small.start()) * length,
        param_along(a, b, small.end()) * length,
    );
    let (lo, hi) = (
        on_big.u * length - on_big.width_mm / 2.0,
        on_big.u * length + on_big.width_mm / 2.0,
    );
    if !(s < lo && hi < e) || remap_u(on_big.u, length, s, e) != on_small.u {
        return false;
    }
    let mut same = on_small.clone();
    same.host_wall_id = on_big.host_wall_id;
    same.u = on_big.u;
    same == *on_big
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::OperationLog;
    use crate::operation::Batch;
    use domowik_format::{AssetInstance, Dimension, InstanceTransform, Label, Level, Point, Wall};
    use serde_json::json;
    use std::collections::BTreeMap;

    fn wall(level: Uuid) -> Wall {
        Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        )
    }

    fn set(id: Uuid, name: &str, v: Value) -> Operation {
        let mut m = serde_json::Map::new();
        m.insert(name.to_owned(), v);
        Operation::set_fields(id, m)
    }

    /// A log with one level and one wall, created by `actor`.
    fn seeded(actor: Uuid) -> (OperationLog, Uuid, Uuid) {
        let level = Level::new("Level 1");
        let (level_id, w) = (level.id, wall(level.id));
        let wall_id = w.id;
        let mut log = OperationLog::new();
        log.deliver(Batch::new(
            actor,
            1,
            BTreeMap::new(),
            vec![
                Operation::create(Entity::Level(level)),
                Operation::create(Entity::Wall(w)),
            ],
        ))
        .expect("seed");
        (log, level_id, wall_id)
    }

    #[test]
    fn replay_projects_created_entities() {
        let (log, level_id, wall_id) = seeded(Uuid::now_v7());
        let p = project(&log);
        assert_eq!(p.document.levels.len(), 1);
        assert_eq!(p.document.walls.len(), 1);
        assert!(p.is_live(level_id));
        assert!(p.is_live(wall_id));
        assert!(p.discarded.is_empty());
    }

    #[test]
    fn set_fields_replaces_a_top_level_field_by_its_serialized_name() {
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![set(wall_id, "endMm", json!([4000.0, 2500.0]))],
        ))
        .expect("edit");
        let p = project(&log);
        assert_eq!(p.document.walls[0].end_mm, [4000.0, 2500.0]);
        // Untouched fields survive.
        assert_eq!(p.document.walls[0].thickness_mm, 200.0);
    }

    #[test]
    fn set_fields_replaces_an_array_valued_field_wholesale() {
        // O3: set- and array-valued fields are replaced WHOLESALE, never
        // merged element-wise. Writing one component of startMm is not a
        // thing the format can express.
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![set(wall_id, "startMm", json!([100.0, 200.0]))],
        ))
        .expect("edit");
        assert_eq!(project(&log).document.walls[0].start_mm, [100.0, 200.0]);
    }

    #[test]
    fn a_delete_writes_a_tombstone_that_keeps_the_live_interval() {
        // I3: the entity's live interval survives deletion.
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("delete");
        let p = project(&log);
        assert!(p.document.walls.is_empty());
        assert!(p.is_tombstoned(wall_id));
        assert!(!p.is_live(wall_id));
        let interval = p.intervals.get(&wall_id).expect("interval retained");
        assert!(interval.deleted_at.is_some());
        assert!(interval.created_at < interval.deleted_at.unwrap());
    }

    #[test]
    fn a_stale_operation_discards_its_whole_batch() {
        // I5: if ANY operation is stale the whole batch is discarded,
        // INCLUDING operations whose own targets were live (B1).
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        let ghost = Uuid::now_v7();
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![
                set(wall_id, "thicknessMm", json!(999.0)),
                set(ghost, "thicknessMm", json!(1.0)),
            ],
        ))
        .expect("delivered");
        let p = project(&log);
        assert_eq!(
            p.document.walls[0].thickness_mm, 200.0,
            "the live operation must not have applied"
        );
        assert_eq!(p.discarded.len(), 1);
        assert_eq!(
            p.discarded[0].reason,
            DiscardReason::Stale(TargetState::BeforeCreation)
        );
    }

    #[test]
    fn vector_v4b_a_write_after_an_observed_delete_is_stale_and_resurrects_nothing() {
        // V4b: discarded as stale (I3); no resurrection.
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("delete");
        log.deliver(Batch::new(
            actor,
            4,
            log.frontier(),
            vec![set(wall_id, "thicknessMm", json!(999.0))],
        ))
        .expect("delivered");
        let p = project(&log);
        assert!(p.document.walls.is_empty(), "nothing may be resurrected");
        assert_eq!(p.discarded.len(), 1);
        assert_eq!(
            p.discarded[0].reason,
            DiscardReason::Stale(TargetState::AfterDeletion)
        );
    }

    #[test]
    fn a_target_created_earlier_in_the_same_batch_is_present_for_later_operations() {
        // I5: presence is evaluated at the OPERATION's write position, so an
        // entity created by an earlier operation of the same batch is present.
        let actor = Uuid::now_v7();
        let level = Level::new("Level 1");
        let w = wall(level.id);
        let wall_id = w.id;
        let mut log = OperationLog::new();
        log.deliver(Batch::new(
            actor,
            1,
            BTreeMap::new(),
            vec![
                Operation::create(Entity::Level(level)),
                Operation::create(Entity::Wall(w)),
                set(wall_id, "thicknessMm", json!(150.0)),
            ],
        ))
        .expect("one batch");
        let p = project(&log);
        assert!(p.discarded.is_empty(), "got {:?}", p.discarded);
        assert_eq!(p.document.walls[0].thickness_mm, 150.0);
    }

    #[test]
    fn a_delete_of_an_already_tombstoned_target_is_stale() {
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("delete");
        log.deliver(Batch::new(
            actor,
            4,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("delivered");
        let p = project(&log);
        assert_eq!(p.discarded.len(), 1);
        assert_eq!(
            p.discarded[0].reason,
            DiscardReason::Stale(TargetState::AfterDeletion)
        );
    }

    #[test]
    fn a_create_after_a_tombstone_by_another_actor_never_resurrects() {
        // I3/E6: tombstones created by sync merge are never resurrected.
        let (x, y) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let (mut log, _, wall_id) = seeded(x);
        let level_id = log.batches()[0].operations[0].entity_id;

        // Y observes everything and deletes the wall.
        log.deliver(Batch::new(
            y,
            1,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("y deletes");

        // X now tries to recreate it. X did not write the tombstone.
        let mut revived = wall(level_id);
        revived.id = wall_id;
        log.deliver(Batch::new(
            x,
            3,
            log.frontier(),
            vec![Operation::create(Entity::Wall(revived))],
        ))
        .expect("delivered");

        let p = project(&log);
        assert!(p.document.walls.is_empty(), "must stay tombstoned");
        assert_eq!(p.discarded.len(), 1);
        assert_eq!(
            p.discarded[0].reason,
            DiscardReason::Stale(TargetState::AfterDeletion)
        );
    }

    #[test]
    fn a_create_after_this_actors_own_tombstone_is_an_authorized_restoration() {
        // E6's sole reuse exception, the mechanism BEHAVIOR-SPEC R8.1.3 needs.
        let actor = Uuid::now_v7();
        let (mut log, level_id, wall_id) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![Operation::delete(wall_id)],
        ))
        .expect("delete");

        let mut restored = wall(level_id);
        restored.id = wall_id;
        log.deliver(Batch::new(
            actor,
            4,
            log.frontier(),
            vec![Operation::create(Entity::Wall(restored))],
        ))
        .expect("restore");

        let p = project(&log);
        assert_eq!(p.document.walls.len(), 1, "got {:?}", p.discarded);
        assert_eq!(p.document.walls[0].id, wall_id, "the SAME entity id");
        assert!(p.is_live(wall_id));
    }

    #[test]
    fn vector_v3_concurrent_writes_resolve_by_position_independent_of_arrival() {
        // V3: two actors write the same field, neither observing the other.
        // Winner per LWW1; loser discarded; outcome independent of arrival.
        let (x, y) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let (seed, _, wall_id) = seeded(x);
        let base = seed.frontier();

        let a = Batch::new(
            x,
            3,
            base.clone(),
            vec![set(wall_id, "thicknessMm", json!(111.0))],
        );
        let b = Batch::new(y, 1, base, vec![set(wall_id, "thicknessMm", json!(222.0))]);

        let replay = |order: Vec<Batch>| -> f64 {
            // The SAME seed for both runs, so only arrival order differs.
            let mut log = seed.clone();
            for batch in order {
                log.deliver(batch).expect("delivers");
            }
            project(&log).document.walls[0].thickness_mm
        };

        let forward = replay(vec![a.clone(), b.clone()]);
        let backward = replay(vec![b.clone(), a.clone()]);
        assert_eq!(forward, backward, "arrival order must not decide");

        // Equal causal pasts, so counterBase decides: x's base 3 beats y's 1.
        assert_eq!(a.causal_past_size(), b.causal_past_size());
        assert_eq!(forward, 111.0);
    }

    #[test]
    fn vector_v4_a_delete_dominates_a_concurrent_set() {
        // V4: deleted (LWW3); the concurrent write is discarded; tombstone
        // retained. Two actors that both observe the seed and neither the
        // other; equal causal pasts and equal counter bases, so actorId
        // decides and the DELETE sorts last.
        let seed_actor = Uuid::from_u128(1);
        let setter = Uuid::from_u128(2);
        let deleter = Uuid::from_u128(9);
        let (mut log, _, wall_id) = seeded(seed_actor);
        let base = log.frontier();

        log.deliver(Batch::new(
            setter,
            1,
            base.clone(),
            vec![set(wall_id, "thicknessMm", json!(999.0))],
        ))
        .expect("setter sets");
        log.deliver(Batch::new(
            deleter,
            1,
            base,
            vec![Operation::delete(wall_id)],
        ))
        .expect("deleter deletes");

        let p = project(&log);
        assert!(p.document.walls.is_empty(), "delete dominates");
        assert!(p.is_tombstoned(wall_id), "tombstone retained");
    }

    #[test]
    fn null_in_a_set_fields_payload_clears_a_field_to_absent() {
        // §4.2 O2. `name` is required on a level, so clearing it must FAIL to
        // deserialize and leave the entity untouched rather than corrupt it.
        let actor = Uuid::now_v7();
        let (mut log, level_id, _) = seeded(actor);
        log.deliver(Batch::new(
            actor,
            3,
            log.frontier(),
            vec![set(level_id, "name", Value::Null)],
        ))
        .expect("delivered");
        let p = project(&log);
        assert_eq!(p.document.levels[0].name, "Level 1");
    }

    #[test]
    fn vector_v6_replay_from_a_baseline_equals_full_history_replay() {
        // V6: rebuild from snapshot + subsequent log; the projection equals
        // full-history replay for all authoritative state.
        let actor = Uuid::now_v7();
        let (mut log, _, wall_id) = seeded(actor);
        let baseline = project(&log).document;

        let tail = Batch::new(
            actor,
            3,
            log.frontier(),
            vec![set(wall_id, "thicknessMm", json!(150.0))],
        );
        log.deliver(tail.clone()).expect("tail");
        let full = project(&log);

        // Now the same tail replayed on top of the baseline alone. The
        // baseline's frontier is what makes the tail applicable rather than
        // held: its predecessors are covered, not missing (§5.1 S2).
        let mut seed = BTreeMap::new();
        seed.insert(actor, 2);
        let mut tail_log = OperationLog::with_baseline_frontier(&seed).expect("seed");
        tail_log.deliver(tail).expect("tail");
        let seeded_replay = project_from(&baseline, &tail_log);

        assert_eq!(full.document, seeded_replay.document);
        assert_eq!(seeded_replay.document.walls[0].thickness_mm, 150.0);
        assert!(
            seeded_replay.discarded.is_empty(),
            "the target must be live"
        );
    }

    #[test]
    fn replay_is_identical_under_every_permitted_delivery_order() {
        // Issue 0011's fourth criterion: "replay from a snapshot/frontier
        // produces the expected projection for ALL permitted delivery orders".
        // V3 shows this for two batches, where "all orders" is two. This
        // exhausts the 24 orders of four batches, three of them mutually
        // concurrent and the fourth causally after one of those — so most
        // permutations force the log to HOLD a batch and promote it later,
        // which the two-batch case never exercises.
        let (x, y, z) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let (seed, level_id, wall_id) = seeded(x);
        let base = seed.frontier();

        // Three concurrent writers, none observing the others.
        let a = Batch::new(
            x,
            3,
            base.clone(),
            vec![set(wall_id, "thicknessMm", json!(111.0))],
        );
        let b = Batch::new(
            y,
            1,
            base.clone(),
            vec![set(wall_id, "thicknessMm", json!(222.0))],
        );
        let c = Batch::new(
            z,
            1,
            base,
            vec![Operation::create(Entity::Wall(wall(level_id)))],
        );

        // Causally AFTER b: it observes b's counter, so any order delivering
        // it before b must hold it.
        let mut after_b = a.frontier.clone();
        after_b.insert(y, 1);
        let d = Batch::new(y, 2, after_b, vec![set(wall_id, "heightMm", json!(2600.0))]);

        let replay = |order: &[&Batch]| -> Document {
            let mut log = seed.clone();
            for batch in order {
                // Held is a legitimate outcome mid-order; only an ERROR is not.
                log.deliver((*batch).clone()).expect("delivers or holds");
            }
            assert!(
                log.held().is_empty(),
                "every batch's prerequisites arrive within the order"
            );
            project(&log).document
        };

        let batches = [&a, &b, &c, &d];
        let mut orders = Vec::new();
        permutations(&batches, &mut Vec::new(), &mut orders);
        assert_eq!(orders.len(), 24, "four batches have 24 orders");

        let expected = replay(&orders[0]);
        for order in &orders {
            assert_eq!(
                replay(order),
                expected,
                "delivery order changed the projection"
            );
        }

        // And the projection is the one LWW1 dictates, not merely a stable
        // wrong answer: a and b are concurrent with equal causal pasts, so
        // counterBase decides and x's 3 beats y's 1.
        assert_eq!(expected.walls.len(), 2, "c created a second wall");
        let edited = expected
            .walls
            .iter()
            .find(|w| w.id == wall_id)
            .expect("the seeded wall survives");
        assert_eq!(edited.thickness_mm, 111.0);
        assert_eq!(edited.height_mm, 2600.0, "d is causally last, unopposed");
    }

    /// Every ordering of `items`, appended to `out`.
    fn permutations<'a>(
        items: &[&'a Batch],
        prefix: &mut Vec<&'a Batch>,
        out: &mut Vec<Vec<&'a Batch>>,
    ) {
        if items.is_empty() {
            out.push(prefix.clone());
            return;
        }
        for (i, item) in items.iter().enumerate() {
            let mut rest: Vec<&Batch> = items.to_vec();
            rest.remove(i);
            prefix.push(item);
            permutations(&rest, prefix, out);
            prefix.pop();
        }
    }

    #[test]
    fn a_baseline_entity_is_live_for_operations_that_follow_it() {
        // Without the BASELINE sentinel these would classify as branch 1
        // (before creation) and discard every edit made after an open.
        let actor = Uuid::now_v7();
        let (log, _, wall_id) = seeded(actor);
        let baseline = project(&log).document;
        let mut fresh = OperationLog::new();
        fresh
            .deliver(Batch::new(
                Uuid::now_v7(),
                1,
                BTreeMap::new(),
                vec![Operation::delete(wall_id)],
            ))
            .expect("delete");
        let p = project_from(&baseline, &fresh);
        assert!(p.discarded.is_empty(), "got {:?}", p.discarded);
        assert!(p.document.walls.is_empty());
        assert!(p.is_tombstoned(wall_id));
    }

    #[test]
    fn prior_values_captures_exactly_the_fields_the_forward_write_touched() {
        let w = wall(Uuid::now_v7());
        let entity = Entity::Wall(w);
        let mut fields = serde_json::Map::new();
        fields.insert("thicknessMm".to_owned(), json!(150.0));
        let prior = prior_values(&entity, &fields);
        assert_eq!(prior.len(), 1);
        assert_eq!(prior.get("thicknessMm"), Some(&json!(200.0)));
    }

    #[test]
    fn prior_values_uses_null_for_a_field_that_was_absent() {
        // Restoring a field the forward write ADDED means clearing it, and
        // O2 spells that as null.
        let entity = Entity::Wall(wall(Uuid::now_v7()));
        let mut fields = serde_json::Map::new();
        fields.insert("notAField".to_owned(), json!(1));
        assert_eq!(
            prior_values(&entity, &fields).get("notAField"),
            Some(&Value::Null)
        );
    }

    /// Three disjoint walls on one level, created by one batch.
    ///
    /// Three, and disjoint, because the rollback tests need a wall in the
    /// MIDDLE of `walls` to delete and no incidental axis contact to trip the
    /// admission guard.
    fn three_walls(actor: Uuid) -> (OperationLog, Uuid, [Uuid; 3]) {
        let level = Level::new("Level 1");
        let level_id = level.id;
        let mut ids = Vec::new();
        let mut ops = vec![Operation::create(Entity::Level(level))];
        for i in 0..3u32 {
            let w = disjoint_wall(level_id, i);
            ids.push(w.id);
            ops.push(Operation::create(Entity::Wall(w)));
        }
        let mut log = OperationLog::new();
        log.deliver(Batch::new(actor, 1, BTreeMap::new(), ops))
            .expect("seed");
        (log, level_id, [ids[0], ids[1], ids[2]])
    }

    /// Wall `i` on its own stretch of the x axis, touching nothing.
    fn disjoint_wall(level: Uuid, i: u32) -> Wall {
        let x = f64::from(i) * 10_000.0;
        Wall::new(
            level,
            Point::new(x, 0.0),
            Point::new(x + 4000.0, 0.0),
            200.0,
            2500.0,
        )
    }

    /// Everything one batch may mutate except `discarded`, which is a
    /// projection outcome and deliberately survives a discard.
    fn state(p: &Projection) -> (Document, HashMap<Uuid, LiveInterval>) {
        (p.document.clone(), p.intervals.clone())
    }

    /// The order of `walls`, which B1 rollback must restore exactly.
    ///
    /// `Document::insert_entity` documents why: the CLI numbers walls by their
    /// position, so a rollback that returned a deleted wall to the END of the
    /// collection would silently renumber every wall after it.
    fn wall_order(p: &Projection) -> Vec<Uuid> {
        p.document.walls.iter().map(|w| w.id).collect()
    }

    #[test]
    fn a_batch_that_goes_stale_after_partial_application_leaves_no_trace() {
        // B1 at the point atomicity is easiest to lose: the batch creates an
        // entity, edits a second, deletes a third from the MIDDLE of its
        // collection, and only its LAST operation is stale. Everything before
        // it has already taken effect on the working state.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);
        let discarded_before = p.discarded.len();
        let ghost = Uuid::now_v7();

        append_batch(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::create(Entity::Wall(disjoint_wall(level_id, 7))),
                    set(walls[0], "thicknessMm", json!(150.0)),
                    Operation::delete(walls[1]),
                    set(ghost, "thicknessMm", json!(1.0)),
                ],
            ),
        );

        assert_eq!(state(&p), before, "a discarded batch must leave no trace");
        assert_eq!(wall_order(&p), walls.to_vec(), "at its own index, not last");
        assert_eq!(p.discarded.len(), discarded_before + 1);
        assert_eq!(
            p.discarded[discarded_before].reason,
            DiscardReason::Stale(TargetState::BeforeCreation)
        );
    }

    #[test]
    fn a_batch_malformed_after_partial_application_leaves_no_trace() {
        // The other discard path. It runs one branch later than the stale one
        // — inside `apply_operation` rather than before it — so a rollback
        // that covered only the stale return would still corrupt here.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);
        let discarded_before = p.discarded.len();

        append_batch(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::create(Entity::Wall(disjoint_wall(level_id, 7))),
                    Operation::delete(walls[1]),
                    set(walls[2], "thicknessMm", json!("not a number")),
                ],
            ),
        );

        assert_eq!(state(&p), before, "a discarded batch must leave no trace");
        assert_eq!(wall_order(&p), walls.to_vec());
        assert_eq!(p.discarded.len(), discarded_before + 1);
        assert_eq!(
            p.discarded[discarded_before].reason,
            DiscardReason::MalformedPayload
        );
    }

    #[test]
    fn a_refused_level_deletion_after_partial_application_leaves_no_trace() {
        // The admission guard raises its refusal only once the WHOLE batch has
        // survived the discard paths, so by then the create AND the deletion
        // have both taken effect on the working state.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);
        let discarded_before = p.discarded.len();

        let refusal = append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::create(Entity::Wall(disjoint_wall(level_id, 7))),
                    Operation::delete(level_id),
                ],
            ),
        )
        .expect_err("an occupied level may not be deleted");

        assert!(matches!(refusal, AdmissionFailure::LevelDeletion(_)));
        assert_eq!(state(&p), before, "a refused batch must leave no trace");
        assert_eq!(wall_order(&p), walls.to_vec());
        assert_eq!(
            p.discarded.len(),
            discarded_before,
            "a refusal is not a discard"
        );
    }

    #[test]
    fn rollback_restores_deletes_replacements_and_within_batch_lifetimes() {
        // Every journal shape in one batch, so their undos have to compose:
        //   a delete from the MIDDLE of `walls`, followed by a create whose
        //   undo must not disturb the reinsertion of that delete;
        //   a create that REPLACES a live id (LWW4);
        //   a create and a delete of the same fresh id inside the batch.
        // Then a stale operation, so all of it is undone at once.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);

        let replacement = {
            let mut w = disjoint_wall(level_id, 0);
            w.id = walls[0];
            w.thickness_mm = 999.0;
            w
        };
        let ephemeral = disjoint_wall(level_id, 8);
        let ephemeral_id = ephemeral.id;

        append_batch(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::delete(walls[1]),
                    Operation::create(Entity::Wall(disjoint_wall(level_id, 7))),
                    Operation::create(Entity::Wall(replacement)),
                    Operation::create(Entity::Wall(ephemeral)),
                    Operation::delete(ephemeral_id),
                    set(Uuid::now_v7(), "thicknessMm", json!(1.0)),
                ],
            ),
        );

        assert_eq!(state(&p), before, "every shape must undo");
        assert_eq!(wall_order(&p), walls.to_vec());
    }

    #[test]
    fn a_wall_deleted_and_recreated_in_one_batch_is_refused() {
        // E6's restoration authorization belongs to undo/redo/compensation,
        // not to an ordinary batch that deletes and recreates one id: a host
        // recreated that way would carry its openings across levels (R4.3.2;
        // Codex GPT-6.1-Sol, issue 0013). Refused as `entity-id-collision`
        // before the axis guard, atomically.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);

        let moved = {
            let mut w = disjoint_wall(level_id, 0);
            w.id = walls[2];
            w
        };
        let refusal = append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::delete(walls[2]),
                    Operation::create(Entity::Wall(moved)),
                ],
            ),
        )
        .expect_err("a same-batch delete and recreate is refused");

        assert!(
            matches!(&refusal, AdmissionFailure::Opening { diagnostics, .. }
                if diagnostics == &[(OpeningCode::EntityIdCollision, walls[2])]),
            "{refusal:?}"
        );
        assert_eq!(state(&p), before);
    }

    #[test]
    fn a_wall_created_and_deleted_within_one_batch_occupies_no_axis() {
        // R3.1.2 judges the FINAL document. A wall that never survives its own
        // batch adds no axis, so the coincidence it held in the middle of the
        // batch refuses nothing.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let overlapping = {
            let mut w = disjoint_wall(level_id, 0);
            w.id = Uuid::now_v7();
            w
        };
        let doomed = overlapping.id;

        append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::create(Entity::Wall(overlapping)),
                    Operation::delete(doomed),
                ],
            ),
        )
        .expect("a wall the batch removes again occupies nothing");
        assert_eq!(wall_order(&p), walls.to_vec());
        assert!(p.is_tombstoned(doomed));
    }

    #[test]
    fn an_axis_is_affected_against_the_state_before_the_batch_not_its_last_edit() {
        // R3.1.2 compares the batch's FINAL walls to the ones it started with.
        // A batch that moves a wall onto an occupied axis and then edits a
        // non-geometric field of the SAME wall leaves its last intermediate
        // state identical to its final one, so a check that asked "did the
        // previous write change this axis?" would compare the wall to itself,
        // find nothing affected, and admit the overlap it just created.
        let actor = Uuid::now_v7();
        let (log, _, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);

        let refusal = append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    // Onto the axis walls[0] already occupies, in two writes...
                    set(walls[2], "startMm", json!([0.0, 0.0])),
                    set(walls[2], "endMm", json!([4000.0, 0.0])),
                    // ...and then a write that moves nothing.
                    set(walls[2], "thicknessMm", json!(300.0)),
                ],
            ),
        )
        .expect_err("the final axis is coincident with an existing wall");

        assert!(matches!(refusal, AdmissionFailure::WallContact(_)));
        assert_eq!(state(&p), before);
    }

    #[test]
    fn a_wall_returned_to_its_own_axis_within_one_batch_is_not_affected() {
        // The other direction, and R3.1.2's "transient intermediate overlap is
        // ignored": a wall that visits an occupied axis and leaves it again has
        // not changed, so nothing is refused and nothing is judged.
        let actor = Uuid::now_v7();
        let (log, _, walls) = three_walls(actor);
        let mut p = project(&log);
        let axis = p.document.wall(walls[2]).expect("wall").start_mm;

        append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    set(walls[2], "startMm", json!([0.0, 0.0])),
                    set(walls[2], "startMm", json!(axis)),
                ],
            ),
        )
        .expect("a wall that ends where it started occupies no new axis");
        assert_eq!(p.document.wall(walls[2]).expect("wall").start_mm, axis);
    }

    #[test]
    fn a_refused_wall_contact_after_partial_application_leaves_no_trace() {
        // The wall-axis refusal compares the batch's FINAL document to its
        // starting one, so it is raised last of all — after every operation,
        // including ones that are perfectly admissible on their own.
        let actor = Uuid::now_v7();
        let (log, level_id, walls) = three_walls(actor);
        let mut p = project(&log);
        let before = state(&p);

        let duplicate = {
            let mut w = disjoint_wall(level_id, 0);
            w.id = Uuid::now_v7();
            w
        };
        let refusal = append_batch_guarded(
            &mut p,
            &Batch::new(
                actor,
                5,
                log.frontier(),
                vec![
                    Operation::create(Entity::Wall(disjoint_wall(level_id, 7))),
                    set(walls[0], "heightMm", json!(3000.0)),
                    Operation::create(Entity::Wall(duplicate)),
                ],
            ),
        )
        .expect_err("a coincident axis is refused");

        assert!(matches!(refusal, AdmissionFailure::WallContact(_)));
        assert_eq!(state(&p), before, "a refused batch must leave no trace");
        assert_eq!(wall_order(&p), walls.to_vec());
    }

    #[test]
    fn snapshot_tier_b_entities_are_live_and_can_be_deleted() {
        let mut doc = Document::with_ground_floor();
        let level_id = doc.levels[0].id;
        let label = Label {
            id: Uuid::now_v7(),
            level_id,
            anchor_mm: [0.0, 0.0],
            name: Some("Original".into()),
            unknown: Default::default(),
        };
        let dimension = Dimension {
            id: Uuid::now_v7(),
            level_id,
            start_mm: [0.0, 0.0],
            end_mm: [100.0, 0.0],
            label_override: None,
            unknown: Default::default(),
        };
        let asset = AssetInstance {
            id: Uuid::now_v7(),
            level_id,
            digest: format!("sha256:{}", "9a".repeat(32)),
            transform: Default::default(),
            display_name: None,
            unknown: Default::default(),
        };
        let ids = [label.id, dimension.id, asset.id];
        doc.labels.push(label);
        doc.dimensions.push(dimension);
        doc.asset_instances.push(asset);
        let mut projection = project_from(&doc, &OperationLog::new());
        for id in ids {
            assert!(projection.is_live(id), "snapshot entity must be live");
        }
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            ids.into_iter().map(Operation::delete).collect(),
        );
        append_batch_guarded(&mut projection, &batch).unwrap();
        for id in ids {
            assert!(projection.is_tombstoned(id));
            assert!(projection.document.entity(id).is_none());
        }
    }

    /// A refused batch restores deleted Tier B entities to their own indices.
    ///
    /// Through the REAL path, not the helpers: `append_batch_guarded` applies
    /// every operation, then refuses late on the last-level block, and
    /// `rollback` has to undo the whole journal.
    ///
    /// Two defects are in range and they are different. If `position_of` were
    /// missing a Tier B collection, `remove_indexed` would short-circuit
    /// before `remove_entity` and the delete would silently do NOTHING — which
    /// the first half asserts against. If `restore_at` mishandled a variant,
    /// the delete would work and the rollback would not — which the second
    /// half asserts against, at the original index rather than merely present.
    #[test]
    fn a_refused_batch_restores_deleted_tier_b_entities_to_their_indices() {
        let actor = Uuid::now_v7();
        let level = Level::new("Level 1");
        let level_id = level.id;

        // Created through the LOG, not by pushing into a Document: a delete is
        // stale unless its target is Live in the interval map, and a document
        // assembled by hand has no intervals at all. Structural creation only
        // — Tier B has no gestures and this invents none.
        let mut ops = vec![Operation::create(Entity::Level(level))];
        let mut ids = Vec::new();
        for i in 0..2u128 {
            let at = i as f64 * 1000.0;
            ops.push(Operation::create(Entity::Label(Label {
                id: Uuid::from_u128(0x10 + i),
                level_id,
                anchor_mm: [at, 0.0],
                name: None,
                unknown: Default::default(),
            })));
            ops.push(Operation::create(Entity::Dimension(Dimension {
                id: Uuid::from_u128(0x20 + i),
                level_id,
                start_mm: [at, 0.0],
                end_mm: [at + 500.0, 0.0],
                label_override: None,
                unknown: Default::default(),
            })));
            ops.push(Operation::create(Entity::AssetInstance(AssetInstance {
                id: Uuid::from_u128(0x30 + i),
                level_id,
                digest: format!("sha256:{}", "9a".repeat(32)),
                transform: InstanceTransform {
                    translation_mm: [at, 0.0],
                    ..Default::default()
                },
                display_name: None,
                unknown: Default::default(),
            })));
            // Delete the FIRST of each kind, so a restore that appends rather
            // than inserts at the original index lands wrong.
            if i == 0 {
                ids.extend([
                    Uuid::from_u128(0x10),
                    Uuid::from_u128(0x20),
                    Uuid::from_u128(0x30),
                ]);
            }
        }
        let mut log = OperationLog::new();
        log.deliver(Batch::new(actor, 1, BTreeMap::new(), ops))
            .expect("seed");
        let before = project(&log).document;
        assert_eq!(before.labels.len(), 2, "the fixture seeded both labels");

        let deletions: Vec<Operation> = ids.iter().map(|id| Operation::delete(*id)).collect();

        // First: the deletions really do delete. Without every Tier B
        // collection in `position_of`, `remove_indexed` short-circuits before
        // `remove_entity` and this batch commits having changed nothing.
        let mut p = project(&log);
        append_batch(
            &mut p,
            &Batch::new(actor, 2, log.frontier(), deletions.clone()),
        );
        assert_eq!(p.document.labels.len(), 1, "the label was not deleted");
        assert_eq!(
            p.document.dimensions.len(),
            1,
            "the dimension was not deleted"
        );
        assert_eq!(
            p.document.asset_instances.len(),
            1,
            "the asset instance was not deleted"
        );

        // Then: the same deletions in a batch refused LATE. The trailing level
        // deletion is blocked because it is the last level, so every earlier
        // operation is rolled back.
        let mut p = project(&log);
        let mut ops = deletions;
        ops.push(Operation::delete(level_id));
        let refused = append_batch_guarded(&mut p, &Batch::new(actor, 2, log.frontier(), ops));
        assert!(
            matches!(refused, Err(AdmissionFailure::LevelDeletion(_))),
            "the batch must be refused LATE, or this proves nothing: {refused:?}"
        );
        assert_eq!(
            p.document.labels, before.labels,
            "labels did not come back in their original order"
        );
        assert_eq!(
            p.document.dimensions, before.dimensions,
            "dimensions did not come back in their original order"
        );
        assert_eq!(
            p.document.asset_instances, before.asset_instances,
            "asset instances did not come back in their original order"
        );
    }
}
