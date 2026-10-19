//! The local command session: commands, undo, and redo.
//!
//! Implements BEHAVIOR-SPEC §8 (normative) and FORMAT-SPEC §4.8's boundary.
//!
//! # The shape of undo here, and why it is not a stack of old states
//!
//! R8.1.1: executing undo appends a NEW compensating batch built from inverse
//! effects. History grows monotonically and is never mutated or reordered, so
//! replay and synchronization metadata stay correct. R8.1.2: every executed
//! undo AND redo mints fresh operation and batch identities and advances the
//! actor counter — entries are never popped and reused.
//!
//! R8.1.3 is the one rule that reaches back into identity: redo restores the
//! SAME ENTITY ids its undone command created, including reopening a tombstone
//! this device's own undo wrote. Operation and batch ids are always fresh; only
//! entity ids are reused, and only under that authorization.

use crate::log::{Delivery, LogError, OperationLog};
use crate::operation::{Batch, Operation, OperationKind};
use crate::replay::{
    append_batch_journaled, merge_fields, prior_values, project_from_with_tombstones,
    project_from_with_tombstones_guarding, rollback, AdmissionFailure, LevelDeletionBlock,
    Projection,
};
use crate::wall_contact::{wall_contact_refusals, WallContactBlock};
use domowik_format::{Document, Entity, ValidationError};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;
use uuid::Uuid;

/// Why a command did not commit.
#[derive(Debug, Error)]
pub enum CommandError {
    /// The resulting document would be invalid. Rejected atomically BEFORE any
    /// effect, with no undo entry (FORMAT-SPEC §4.3 row 1, R8.2.2).
    #[error("command rejected: {}", invalid_message(.0))]
    Invalid(Vec<ValidationError>),
    /// The batch could not enter the log.
    #[error(transparent)]
    Framing(#[from] LogError),
    /// The batch was discarded as stale at its canonical position, or held.
    #[error("batch was not applied: {0}")]
    NotApplied(&'static str),
    /// An ordinary command tried to create an entity id that is tombstoned.
    ///
    /// FORMAT-SPEC E6 permits exactly one reuse of a tombstoned identity —
    /// authorized restoration by the same device's own undo (R8.1.3) — and
    /// [`Session::redo`] is the only path that takes it.
    #[error("entity {0} is deleted; only redo may restore a deleted identity (FORMAT-SPEC E6)")]
    Resurrection(Uuid),
    /// A level deletion R2.2/R2.3 refuses, listing every reason.
    ///
    /// Reached from EVERY admission path — local commands, undo, redo,
    /// compensation, accepted external batches and held promotions — because
    /// the guard lives at the replay step they all share. Refusal is atomic:
    /// no batch enters the log, no stack moves, and `dirty` is unchanged.
    #[error("level deletion rejected: {}", .0.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("; "))]
    LevelDeletion(Vec<LevelDeletionBlock>),
    /// A new or changed final wall axis has an R3.1.2 refusing contact.
    #[error("command rejected: {}", .0.iter().map(|e| e.to_string()).collect::<Vec<_>>().join("; "))]
    WallContact(Vec<WallContactBlock>),
    /// BEHAVIOR-SPEC §4.4: a raw write admission refuses (`levelId` on an
    /// opening or wall, an invalid enum value, an unverified host change),
    /// with the candidate state's validation errors. The message leads with
    /// the ONE primary code R4.4's precedence selects across both.
    #[error("command rejected: {}", opening_message(diagnostics, validation))]
    OpeningRefused {
        diagnostics: Vec<(domowik_format::OpeningCode, Uuid)>,
        validation: Vec<(Option<domowik_format::OpeningCode>, String)>,
    },
}

impl CommandError {
    /// The primary §4.4 diagnostic (R4.4's precedence) of an opening-related
    /// refusal, or `None` when the refusal names no opening code.
    pub fn primary_opening_code(&self) -> Option<domowik_format::OpeningCode> {
        match self {
            Self::Invalid(errors) => errors.iter().filter_map(|e| e.opening_code()).min(),
            Self::OpeningRefused {
                diagnostics,
                validation,
            } => diagnostics
                .iter()
                .map(|(c, _)| *c)
                .chain(validation.iter().filter_map(|(c, _)| *c))
                .min(),
            _ => None,
        }
    }
}

/// `Invalid`'s message: the primary §4.4 code first when there is one.
fn invalid_message(errors: &[ValidationError]) -> String {
    let all = errors
        .iter()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join("; ");
    match errors.iter().filter_map(|e| e.opening_code()).min() {
        Some(code) => format!("{code}: {all}"),
        None => all,
    }
}

fn opening_message(
    diagnostics: &[(domowik_format::OpeningCode, Uuid)],
    validation: &[(Option<domowik_format::OpeningCode>, String)],
) -> String {
    let primary = diagnostics
        .iter()
        .map(|(c, _)| *c)
        .chain(validation.iter().filter_map(|(c, _)| *c))
        .min()
        .expect("an opening refusal carries a diagnostic");
    let mut parts: Vec<String> = diagnostics
        .iter()
        .map(|(c, id)| format!("{c} ({id})"))
        .collect();
    parts.extend(validation.iter().map(|(_, m)| m.clone()));
    format!("{primary}: {}", parts.join("; "))
}

impl From<AdmissionFailure> for CommandError {
    fn from(value: AdmissionFailure) -> Self {
        match value {
            AdmissionFailure::LevelDeletion(blocks) => Self::LevelDeletion(blocks),
            AdmissionFailure::WallContact(blocks) => Self::WallContact(blocks),
            AdmissionFailure::Opening {
                diagnostics,
                validation,
            } => Self::OpeningRefused {
                diagnostics,
                validation,
            },
        }
    }
}

fn guard_final_contacts(before: &Document, after: &Document) -> Result<(), CommandError> {
    let contacts = wall_contact_refusals(before, after);
    if contacts.is_empty() {
        Ok(())
    } else {
        Err(CommandError::WallContact(contacts))
    }
}

/// Whether admitting a candidate would take a document that HAS levels to one
/// with none.
///
/// The per-operation guard cannot see this case, and tracing canonical
/// reordering is how it was found rather than any test. A batch admitted at an
/// EARLIER canonical position than an already-accepted deletion is judged
/// against the state at ITS OWN position, where the level the older deletion
/// removes still exists — so its own deletion looks admissible. The older
/// batch is history and replays unguarded, and the two together land on zero
/// levels that neither one would have produced alone.
///
/// Only the FINAL state shows it, so this is a final-state check and is
/// deliberately not a document invariant: it fires only when the current
/// document has levels. An empty internal builder and a legacy empty file must
/// still be openable and repairable, which a `validate_plan` call on every
/// candidate would forbid.
fn empties_the_levels(before: &Document, after: &Document) -> bool {
    !before.levels.is_empty() && after.levels.is_empty()
}

/// The batch ids a delivery put at a canonical position for the FIRST time:
/// `admitted` itself, plus every batch its arrival promoted out of the held
/// set. Exactly the set R2.3 must judge — accepted history has already been
/// judged, or predates the rule and must still replay.
fn newly_admitted(scratch: &OperationLog, admitted: Uuid) -> HashSet<Uuid> {
    let mut ids: HashSet<Uuid> = scratch.newly_promoted().iter().copied().collect();
    ids.insert(admitted);
    ids
}

/// What a local delivery did.
struct Applied {
    /// The batch this command produced.
    batch: Uuid,
    /// Whether accepting it also promoted a held EXTERNAL batch, firing
    /// R8.3.1's fence. The caller must not then push a stack entry: it would
    /// sit on the far side of the fence that just cleared the stacks.
    fenced: bool,
}

/// One entry on the undo or redo stack.
///
/// Holds both directions, because R8.1.2 forbids reusing the original batch:
/// undo emits `inverse` under fresh identities and redo emits `forward` under
/// fresh identities, and the entry moves between the stacks unchanged.
#[derive(Debug, Clone)]
struct StackEntry {
    forward: Vec<Operation>,
    inverse: Vec<Operation>,
}

impl StackEntry {
    /// A copy with freshly minted operation ids, preserving ENTITY ids.
    ///
    /// R8.1.2 and R8.1.3 together: operation identity is always new, entity
    /// identity is deliberately reused.
    fn refreshed(ops: &[Operation]) -> Vec<Operation> {
        ops.iter()
            .map(|op| Operation {
                operation_id: Uuid::now_v7(),
                entity_id: op.entity_id,
                kind: op.kind.clone(),
            })
            .collect()
    }
}

/// Default capacities. R8.4.4 makes the EXISTENCE of a bound normative and the
/// values implementation policy; these are ours, and they are independent.
pub const DEFAULT_UNDO_CAPACITY: usize = 200;
pub const DEFAULT_REDO_CAPACITY: usize = 200;

/// An open document on one device.
///
/// Owns the durable log, the current projection, and the session-local undo
/// and redo stacks. The stacks are never persisted (R8.4.2).
#[derive(Debug, Clone)]
pub struct Session {
    actor: Uuid,
    /// The state the log replays on top of: FORMAT-SPEC §5's snapshot role.
    /// Empty for a session that starts from nothing.
    baseline: Document,
    /// The tombstoned ids the snapshot retained (§5.1 S4).
    ///
    /// A session opened over a COMPACTED plan must carry these, or it projects
    /// a different document than the package it was opened from: a compacted
    /// delete leaves no operation behind, so a later `createEntity` for that
    /// id resurrects what the user deleted (I3). `Package::project` always
    /// passed them; `Session::open_snapshot` did not, so the live document and
    /// the saved file disagreed. Found by project review, 2026-09-01.
    tombstones: Vec<Uuid>,
    log: OperationLog,
    projection: Projection,
    undo: VecDeque<StackEntry>,
    redo: VecDeque<StackEntry>,
    undo_capacity: usize,
    redo_capacity: usize,
    dirty: bool,
    /// Non-blocking diagnostics: capacity drops (R8.4.4), stale discards.
    diagnostics: Vec<String>,
}

impl Session {
    /// Opens an empty session for a fresh actor.
    pub fn new() -> Self {
        Self::with_actor(Uuid::now_v7())
    }

    /// Opens an empty session for a named actor.
    pub fn with_actor(actor: Uuid) -> Self {
        Self {
            actor,
            baseline: Document::default(),
            tombstones: Vec::new(),
            log: OperationLog::new(),
            projection: Projection::default(),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            undo_capacity: DEFAULT_UNDO_CAPACITY,
            redo_capacity: DEFAULT_REDO_CAPACITY,
            dirty: false,
            diagnostics: Vec::new(),
        }
    }

    /// Sets both stack capacities. R8.4.4: the two bounds are independent, and
    /// exhausting one leaves the other unchanged.
    pub fn with_capacities(mut self, undo: usize, redo: usize) -> Self {
        self.undo_capacity = undo;
        self.redo_capacity = redo;
        self
    }

    /// This session's actor id.
    pub fn actor(&self) -> Uuid {
        self.actor
    }

    /// The current projection's document.
    pub fn document(&self) -> &Document {
        &self.projection.document
    }

    /// The current projection, including tombstones and discards.
    pub fn projection(&self) -> &Projection {
        &self.projection
    }

    /// The durable operation log.
    pub fn log(&self) -> &OperationLog {
        &self.log
    }

    /// Whether the document has changed relative to the saved target.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clears dirty. §9.1's save contract is the only thing that may call this.
    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    /// Non-blocking diagnostics accumulated this session.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// Number of entries available to undo.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of entries available to redo.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Whether an ordinary command may create `id`.
    ///
    /// FORMAT-SPEC E6 allows exactly one reuse of a tombstoned identity:
    /// authorized restoration by the same device's own undo (R8.1.3). Replay
    /// cannot tell a redo from any other same-actor create — an undo's
    /// compensating batch is an ordinary batch — so the check lives here, at
    /// the authoring replica, exactly as §4.2 O4 requires.
    fn refuses_resurrection(&self, operations: &[Operation]) -> Option<Uuid> {
        operations.iter().find_map(|op| match &op.kind {
            OperationKind::CreateEntity { .. } if self.projection.is_tombstoned(op.entity_id) => {
                Some(op.entity_id)
            }
            _ => None,
        })
    }

    /// Commits a local command as one atomic batch and one undo entry.
    ///
    /// R8.2.1: exactly one undo entry per accepted operation batch. R8.4.1:
    /// an accepted local command outside redo execution clears the redo stack.
    ///
    /// The document is validated BEFORE the batch enters the log. A command
    /// that would produce an invalid document is rejected atomically with zero
    /// effects and no undo entry (R8.2.2, FORMAT-SPEC §4.3 row 1) — the log
    /// must never contain a batch whose projection is invalid, because replay
    /// has no way to decline it later.
    pub fn commit(&mut self, operations: Vec<Operation>) -> Result<Uuid, CommandError> {
        // §2.7 / R4.3.5: an ordinary command creating an id that is already
        // live — or that an EARLIER operation of this same command created —
        // is an `entity-id-collision`. LWW4 replacement is for CONCURRENT
        // creates (FORMAT-SPEC I5), which a local command never is; undo,
        // redo and compensation take their own paths and keep their
        // authorization. The rest of the command is still evaluated (on a
        // scratch copy, without the colliding creates) so R4.4 can rank the
        // collision against every other failure.
        let mut created: HashSet<Uuid> = HashSet::new();
        let mut collisions = Vec::new();
        let mut rest = Vec::with_capacity(operations.len());
        for op in &operations {
            if let OperationKind::CreateEntity { entity } = &op.kind {
                if !created.insert(op.entity_id)
                    || self.projection.document.entity(op.entity_id).is_some()
                {
                    collisions.push((domowik_format::OpeningCode::EntityIdCollision, op.entity_id));
                    // The excluded create is never validated, but R4.4
                    // ranks `nonfinite-value` above the collision, and
                    // external admission (LWW4 replacement) would report it.
                    if let domowik_format::Entity::Opening(o) = entity {
                        let values = [
                            o.u,
                            o.width_mm,
                            o.height_mm,
                            o.sill_mm,
                            o.depth_mm,
                            o.swing_deg,
                        ];
                        if values.iter().any(|v| !v.is_finite()) {
                            collisions
                                .push((domowik_format::OpeningCode::NonfiniteValue, op.entity_id));
                        }
                    }
                    continue;
                }
            }
            rest.push(op.clone());
        }
        if !collisions.is_empty() {
            let mut scratch = self.clone();
            let (mut diagnostics, validation) = match scratch.apply_local(rest) {
                Err(CommandError::OpeningRefused {
                    diagnostics,
                    validation,
                }) => (diagnostics, validation),
                Err(CommandError::Invalid(errors)) => (
                    Vec::new(),
                    errors
                        .iter()
                        .map(|e| (e.opening_code(), e.to_string()))
                        .collect(),
                ),
                _ => (Vec::new(), Vec::new()),
            };
            diagnostics.extend(collisions);
            return Err(CommandError::OpeningRefused {
                diagnostics,
                validation,
            });
        }
        if let Some(id) = self.refuses_resurrection(&operations) {
            return Err(CommandError::Resurrection(id));
        }
        let inverse = invert_against(&self.projection.document, &operations)?;
        let Applied { batch, fenced } = self.apply_local(operations.clone())?;
        // R8.3.1 is absolute: local undo never crosses an accepted external
        // batch. When this command's own delivery promoted one, pushing an
        // entry for the command would put an entry on the far side of the
        // fence that just cleared the stacks.
        if !fenced {
            self.push_undo(StackEntry {
                forward: operations,
                inverse,
            });
            self.redo.clear();
        }
        Ok(batch)
    }

    /// Undoes the most recent command, appending a compensating batch.
    ///
    /// Returns the compensating batch's id, or `None` when the undo stack is
    /// empty. R8.4.1: compensation batches generated by executed undos do NOT
    /// clear redo.
    pub fn undo(&mut self) -> Result<Option<Uuid>, CommandError> {
        let Some(entry) = self.undo.pop_back() else {
            return Ok(None);
        };
        let ops = StackEntry::refreshed(&entry.inverse);
        match self.apply_local(ops) {
            Ok(Applied { batch, fenced }) => {
                if !fenced {
                    self.push_redo(entry);
                }
                Ok(Some(batch))
            }
            Err(e) => {
                // Restore the entry: a failed undo must leave the stacks as
                // they were, exactly like a rejected command (R8.2.2).
                self.undo.push_back(entry);
                Err(e)
            }
        }
    }

    /// Redoes the most recently undone command.
    ///
    /// R8.1.3: this restores the SAME entity ids the undone command created,
    /// including reopening a tombstone written by this session's own undo.
    /// That authorization is exactly what [`crate::replay`] checks by
    /// provenance, and this is the only place that mints such a create.
    pub fn redo(&mut self) -> Result<Option<Uuid>, CommandError> {
        let Some(entry) = self.redo.pop_back() else {
            return Ok(None);
        };
        // The ONE path that may reopen a tombstone this session's own undo
        // wrote (R8.1.3, E6). `commit` refuses to; this does not, and that
        // asymmetry is the whole enforcement.
        let ops = StackEntry::refreshed(&entry.forward);
        match self.apply_local(ops) {
            Ok(Applied { batch, fenced }) => {
                if !fenced {
                    self.push_undo(entry);
                }
                Ok(Some(batch))
            }
            Err(e) => {
                self.redo.push_back(entry);
                Err(e)
            }
        }
    }

    /// Accepts an external or replayed batch: sync-delivered, recovery-restored
    /// or CLI-applied.
    ///
    /// R8.3.1: an accepted external batch clears BOTH stacks, so local undo
    /// never crosses one. R8.3.2: it never enters them. R8.3.3: it makes the
    /// document dirty relative to the saved target.
    pub fn accept_external(&mut self, batch: Batch) -> Result<Delivery, CommandError> {
        // The SAME scratch-log discipline as `apply_local`, for the same
        // reason. §4.3 row 1 puts `zero-length-wall` in the user-input class:
        // "whole batch rejects atomically BEFORE any effect; state unchanged".
        // It says nothing about the batch being local, and an external batch
        // that lands in the log cannot be declined later — replay has no way
        // to refuse it.
        //
        // Without this, `domowik apply` accepted a batch creating a
        // zero-length wall of negative thickness and SAVED it, producing a
        // file the same binary's `validate` then rejected. `apply_local` grew
        // this guard on 2026-08-31 after adversarial review; the external path
        // did not, and was found by the same reviewer on 2026-09-01.
        let mut scratch = self.log.clone();
        let delivery = scratch.deliver(batch.clone())?;
        if delivery == Delivery::Accepted {
            // R2.2/R2.3 is an ADMISSION guard, not a document invariant, so it
            // cannot ride on the introduced-errors filter below: `validate`
            // sees states, and a deletion is a transition. This path has its
            // own scratch log, so it is wired separately from `candidate_for`
            // — the two are not one seam however similar they read.
            let guarded = newly_admitted(&scratch, batch.batch_id);
            let candidate = project_from_with_tombstones_guarding(
                &self.baseline,
                &self.tombstones,
                &scratch,
                &guarded,
            )
            .map_err(CommandError::from)?;
            guard_final_contacts(&self.projection.document, &candidate.document)?;
            // Errors the batch INTRODUCES, not errors it inherits. A plan that
            // is already invalid — one written before this guard existed —
            // must still accept batches, or the tool that made the mess is the
            // one refusing to let anyone edit their way out of it.
            let existing = self.projection.document.validate();
            let introduced: Vec<ValidationError> = candidate
                .document
                .validate()
                .into_iter()
                .filter(|e| !existing.contains(e))
                .collect();
            if !introduced.is_empty() {
                return Err(CommandError::Invalid(introduced));
            }
            // Its own final-state check, on its own scratch path. Not part of
            // the introduced-errors filter above: `validate` describes a
            // STATE, and a document that legitimately has no levels — an
            // empty builder, a legacy file — must stay editable. This fires
            // only on the TRANSITION from levels to none, which a canonically
            // reordered deletion can reach without any single batch being
            // refusable on its own.
            if empties_the_levels(&self.projection.document, &candidate.document) {
                return Err(CommandError::Invalid(vec![ValidationError::NoLevels]));
            }
        }

        let delivery = self.log.deliver(batch)?;
        self.reproject();
        if delivery == Delivery::Accepted {
            self.undo.clear();
            self.redo.clear();
            self.dirty = true;
        }
        Ok(delivery)
    }

    /// Opens a session over a stored snapshot and the batches after it.
    ///
    /// This is [`Self::open_with`] for a package that has been compacted: the
    /// baseline is the snapshot's projection and `frontier` is what it covers
    /// (FORMAT-SPEC §5.1 S2). Like `open_with`, the batches are part of the
    /// open — they never fire the R8.3.1 fence and leave the document clean.
    pub fn open_snapshot(
        actor: Uuid,
        baseline: Document,
        frontier: &std::collections::BTreeMap<Uuid, u64>,
        tombstones: &[Uuid],
        batches: Vec<Batch>,
    ) -> Result<Self, CommandError> {
        let mut s = Self::with_actor(actor);
        s.log = OperationLog::with_baseline_frontier(frontier)?;
        s.baseline = baseline;
        s.tombstones = tombstones.to_vec();
        s.open_with(batches)?;
        Ok(s)
    }

    /// The snapshot this session replays on top of.
    pub fn baseline(&self) -> &Document {
        &self.baseline
    }

    /// Replays batches while CONSTRUCTING this session's projection.
    ///
    /// R8.4.2: batches replayed while constructing an open are part of that
    /// open. They establish the baseline, never fire the R8.3.1 fence, and
    /// leave the document NOT dirty relative to the opened source. This is a
    /// different operation from [`Self::accept_external`] for exactly that
    /// reason, and conflating the two is how a freshly opened document reports
    /// unsaved changes it does not have.
    pub fn open_with(&mut self, batches: Vec<Batch>) -> Result<(), CommandError> {
        for batch in batches {
            self.log.deliver(batch)?;
        }
        self.reproject();
        self.undo.clear();
        self.redo.clear();
        self.dirty = false;
        Ok(())
    }

    /// The projection as it stood BEFORE `batch` was applied.
    ///
    /// Replays every other accepted batch. That is the honest way to answer
    /// the question — the canonical order is a function of the delivered set,
    /// so removing one batch and replaying is exactly the state that set
    /// produced without it.
    pub fn projection_without(&self, batch: &Batch) -> Projection {
        let mut log = OperationLog::with_baseline_frontier(&self.log.baseline_frontier_seed())
            .unwrap_or_default();
        for other in self.log.batches() {
            if other.batch_id == batch.batch_id {
                continue;
            }
            let _ = log.deliver(other.clone());
        }
        project_from_with_tombstones(&self.baseline, &self.tombstones, &log)
    }

    /// Applies the inverse of `batch` as a compensating batch.
    ///
    /// This is the authorized-restoration path (E6, R8.1.3) for a front end
    /// that does NOT hold a session across commands — a CLI process, whose
    /// session lives for one invocation and has no stack to pop. Undoing a
    /// delete means recreating a tombstoned id, which `commit` refuses; that
    /// refusal is right for an ORDINARY command and wrong for a compensation,
    /// and the difference is exactly what E6 draws.
    ///
    /// Narrower than exposing a general escape hatch: the caller supplies a
    /// batch from the log and gets its inverse, so nothing can be restored
    /// that this actor did not delete in the batch being compensated.
    pub fn compensate(&mut self, batch: &Batch) -> Result<Uuid, CommandError> {
        let operations = self.invert_batch(batch)?;
        let Applied { batch, .. } = self.apply_local(operations)?;
        Ok(batch)
    }

    /// The inverse of `batch`, built against the state it was applied to.
    ///
    /// Exposed so a front end that does NOT hold a session across commands —
    /// a CLI process, for instance — can still compensate a batch. Inverting
    /// against the CURRENT projection would fail for a batch that deleted
    /// something, because the entity whose fields the inverse must restore is
    /// no longer there to read them from.
    pub fn invert_batch(&self, batch: &Batch) -> Result<Vec<Operation>, CommandError> {
        let prior = self.projection_without(batch);
        invert_against(&prior.document, &batch.operations)
    }

    /// The projection this session would have if `batch` were delivered,
    /// discovered by replaying the log with it in place.
    ///
    /// Only [`Self::commit_replayed`] needs this, and only because a delivery
    /// that PROMOTES held batches can put them anywhere in canonical order.
    /// The result is evidence that the command is admissible, never the state
    /// that follows from it: the real delivery may promote batches this scratch
    /// one did not, so the caller reprojects.
    fn candidate_by_replay(&self, batch: &Batch) -> Result<Projection, CommandError> {
        let mut scratch = self.log.clone();
        match scratch.deliver(batch.clone())? {
            Delivery::Accepted => {}
            Delivery::Duplicate => return Err(CommandError::NotApplied("already applied")),
            Delivery::Held => return Err(CommandError::NotApplied("causal prerequisites missing")),
        }
        // The batch being admitted AND anything its delivery promoted: both
        // are newly at a canonical position and neither has been guarded
        // before. Every other batch in the scratch log is accepted history and
        // replays unguarded.
        let guarded = newly_admitted(&scratch, batch.batch_id);
        let projection = project_from_with_tombstones_guarding(
            &self.baseline,
            &self.tombstones,
            &scratch,
            &guarded,
        )
        .map_err(CommandError::from)?;
        guard_final_contacts(&self.projection.document, &projection.document)?;
        // Keep the final materialized invariant explicit on the slow path as
        // well. F4 still constrains where promoted batches may sort; this is
        // defensive parity with external admission, not a bypass of causality.
        if empties_the_levels(&self.projection.document, &projection.document) {
            return Err(CommandError::Invalid(vec![ValidationError::NoLevels]));
        }
        Ok(projection)
    }

    /// Builds a batch from `operations`, admits it, and delivers it.
    ///
    /// # Why this is not simply a replay
    ///
    /// It was, and replaying the whole log per command made a commit cost
    /// O(n²) in history length and building an n-operation document cost
    /// O(n³). The Go spike recorded the symptom as finding F1 on 2026-08-25
    /// and left the cause undiagnosed; the rewrite inherited it. At 2000
    /// operations one commit cost 48 ms — three interactive frames, for one
    /// keystroke (issue 0038).
    ///
    /// # The appended path, and exactly when it is valid
    ///
    /// A locally minted batch observes the entire log, so its `causalPastSize`
    /// is strictly greater than every retained batch's — each of those excludes
    /// at least its own operations from its own past. Its canonical position is
    /// therefore strictly LAST, it cannot reorder against anything already
    /// projected, and extending the current projection gives the same answer a
    /// replay would.
    ///
    /// That argument fails in one case: if the log HOLDS batches, delivering
    /// this one can promote them, and a promoted batch may sort anywhere.
    /// [`Self::commit_replayed`] is kept for exactly that case rather than
    /// reasoned about.
    ///
    /// # Why the appended path builds no candidate at all
    ///
    /// It did, on a `self.projection.clone()`, and that copy was the residual
    /// cost issue 0038 recorded: every command copied the whole plan, whether
    /// it was accepted or rejected, purely so a rejection could throw the copy
    /// away. The batch is now applied to the REAL projection under a rollback
    /// journal bounded by the batch's own size, and a rejection undoes it. The
    /// guarantee is the same one — R8.2.2's "a rejected command changes
    /// nothing" — bought at the price of the command instead of the document.
    fn apply_local(&mut self, operations: Vec<Operation>) -> Result<Applied, CommandError> {
        // EVERYTHING is decided before the real log changes. R8.2.2 requires a
        // rejected command to leave the stacks and the state unchanged, and
        // FORMAT-SPEC §4.3 row 1 requires rejection BEFORE any effect — which
        // includes the effect of the batch existing in durable history.
        //
        // The earlier shape validated the projected DOCUMENT on a scratch log
        // but delivered the real batch before noticing that replay had
        // discarded it, so a rejected command still grew the log. Found by
        // adversarial review (Codex, gpt-5.6-sol, 2026-08-31).
        let batch = Batch::new(
            self.actor,
            self.log.next_counter(self.actor),
            self.log.frontier(),
            operations,
        );
        if self.log.held().is_empty() {
            self.commit_appended(batch)
        } else {
            self.commit_replayed(batch)
        }
    }

    /// The hot path: nothing is held, so the batch sorts strictly last and is
    /// applied to the session's own projection under a rollback journal.
    ///
    /// Every refusal below undoes the batch and returns. The projection, the
    /// log, the stacks and `dirty` are then exactly what they were, which is
    /// R8.2.2 — and it now costs the size of the command rather than a copy of
    /// the document.
    fn commit_appended(&mut self, batch: Batch) -> Result<Applied, CommandError> {
        // Framing is checked WITHOUT delivering, so a command rejected here
        // leaves the log untouched — `deliver` cannot be undone.
        self.log.would_accept(&batch)?;
        let had_levels = !self.projection.document.levels.is_empty();
        // The incremental hot path stays incremental: the guard adds one
        // `matches!` per operation and only looks at the document when that
        // operation is an effective level deletion.
        let journal =
            append_batch_journaled(&mut self.projection, &batch).map_err(CommandError::from)?;

        // The refusals that follow are in the order the two-stage shape used
        // — materialized level invariant, then discard, then validation — so a
        // command that trips more than one is still rejected for the same
        // reason it always was.
        let refusal = if had_levels && self.projection.document.levels.is_empty() {
            // Cannot reorder — this batch observes everything and sorts last —
            // but checked anyway, so the two commit paths cannot disagree
            // about what they admit.
            Some(CommandError::Invalid(vec![ValidationError::NoLevels]))
        } else if journal.was_discarded() {
            Some(CommandError::NotApplied(
                "the command's batch would be discarded at its canonical position",
            ))
        } else {
            let errors = self.projection.document.validate();
            (!errors.is_empty()).then_some(CommandError::Invalid(errors))
        };
        if let Some(error) = refusal {
            rollback(&mut self.projection, journal);
            return Err(error);
        }

        // Only now does the real log change. `would_accept` already said it
        // will, and the rollback is here because a projection that describes a
        // batch the log does not hold is worse than a slow commit.
        let id = batch.batch_id;
        if let Err(error) = self.log.deliver(batch) {
            rollback(&mut self.projection, journal);
            return Err(error.into());
        }
        debug_assert_eq!(
            self.log.newly_promoted().len(),
            0,
            "the appended path assumed nothing was held"
        );
        self.dirty = true;
        Ok(Applied {
            batch: id,
            fenced: false,
        })
    }

    /// The path taken when the log HOLDS batches: delivering this one may
    /// promote them, a promoted batch may sort anywhere, and only a replay can
    /// discover where.
    fn commit_replayed(&mut self, batch: Batch) -> Result<Applied, CommandError> {
        let id = batch.batch_id;
        let candidate = self.candidate_by_replay(&batch)?;
        if candidate.discarded.len() > self.projection.discarded.len() {
            return Err(CommandError::NotApplied(
                "the command's batch would be discarded at its canonical position",
            ));
        }
        let errors = candidate.document.validate();
        if !errors.is_empty() {
            return Err(CommandError::Invalid(errors));
        }

        // Only now does the real log change.
        self.log.deliver(batch)?;
        // R8.3.1: delivering a local batch can PROMOTE held external ones, and
        // each promotion is an acceptance with the same fence consequences. A
        // caller looking only at this call's own verdict would let local undo
        // cross an accepted external batch.
        //
        // The real delivery may promote batches the scratch delivery did not,
        // so `candidate` is evidence that the command is admissible and never
        // the state that results from it.
        let promoted = self.log.newly_promoted().len();
        self.reproject();
        let fenced = promoted > 0;
        if fenced {
            self.undo.clear();
            self.redo.clear();
            self.diagnostics.push(format!(
                "{promoted} external batch(es) became applicable and cleared the undo history"
            ));
        }

        self.dirty = true;
        Ok(Applied { batch: id, fenced })
    }
}

/// Builds the inverse of `operations` against the document they applied to.
///
/// R3.1.5/R8.1.1: inverse effects along reverse dependency edges. The inverses
/// are emitted in reverse order, so undoing a batch that creates A then B
/// deletes B then A.
///
/// Each operation is inverted against the entity as it was JUST BEFORE that
/// operation, not before the whole batch, so the inverse retraces the batch's
/// intermediate states in reverse. The net effect is the same; the difference
/// is evidence. A gesture that moves a door's host and then splits it writes
/// the moved extent in between, and the undo must write it back before it
/// merges the pieces: admission certifies the door's return against an
/// extent the batch actually held (ED-0119; Codex GPT-6.1-Sol, 0055 round
/// three: an undo whose parent ended on its original axis carried no trace
/// of the move and was refused). Only touched entities are tracked, so this
/// costs the batch's size, not a copy of the document.
fn invert_against(
    prior: &Document,
    operations: &[Operation],
) -> Result<Vec<Operation>, CommandError> {
    let mut out = Vec::with_capacity(operations.len());
    // Track what the batch itself creates, so inverting a batch that
    // creates an entity and then edits it does not look for the entity's
    // prior state in a projection that has never seen it.
    let mut created_here: Vec<Uuid> = Vec::new();
    // The state of each entity the batch has touched so far; `None` once
    // deleted.
    let mut current: HashMap<Uuid, Option<Entity>> = HashMap::new();
    // How many operations still name each entity. A state is carried
    // forward only for an entity a later operation touches again, so the
    // common batch (each entity written once) pays no extra merge.
    let mut remaining: HashMap<Uuid, usize> = HashMap::new();
    for op in operations {
        *remaining.entry(op.entity_id).or_default() += 1;
    }
    let state = |current: &HashMap<Uuid, Option<Entity>>, id: Uuid| match current.get(&id) {
        Some(state) => state.clone(),
        None => prior.entity(id),
    };

    for op in operations {
        let before = state(&current, op.entity_id);
        let later = {
            let left = remaining.get_mut(&op.entity_id).expect("counted above");
            *left -= 1;
            *left > 0
        };
        match &op.kind {
            // A create whose id ALREADY EXISTED is an LWW4 collision: it
            // replaced an entity rather than adding one, so its inverse is
            // to put the old one back, not to delete the id. Inverting it
            // as a delete leaves the entity absent AND tombstoned, which
            // undo has no business doing (adversarial review, Codex,
            // gpt-5.6-sol, 2026-08-31).
            //
            // Only a genuinely NEW id counts as created here. A replaced one
            // keeps the inverses of its later field writes: they retrace the
            // intermediate states admission certifies a rehost against
            // (Codex GPT-6.1-Sol, 0055 round four).
            OperationKind::CreateEntity { entity } if !created_here.contains(&entity.id()) => {
                match before {
                    Some(previous) => out.push(Operation::create(previous)),
                    None => {
                        created_here.push(entity.id());
                        out.push(Operation::delete(entity.id()));
                    }
                }
                current.insert(entity.id(), Some(entity.clone()));
            }
            // A SECOND create of an id this batch already created needs no
            // inverse of its own: the first one's inverse restores the
            // state before the whole batch. Emitting one would produce a
            // second delete, which is stale against the first and discards
            // the entire compensating batch.
            OperationKind::CreateEntity { entity } => {
                current.insert(entity.id(), Some(entity.clone()));
            }
            OperationKind::SetFields { fields } => match before {
                // The delete already queued for the create undoes this edit
                // too; a second inverse would be a write to a deleted entity.
                Some(entity) => {
                    if !created_here.contains(&op.entity_id) {
                        out.push(Operation::set_fields(
                            op.entity_id,
                            prior_values(&entity, fields),
                        ));
                    }
                    if later {
                        let next = merge_fields(&entity, fields).unwrap_or(entity);
                        current.insert(op.entity_id, Some(next));
                    }
                }
                None if created_here.contains(&op.entity_id) => {}
                None => {
                    return Err(CommandError::NotApplied(
                        "cannot invert an edit to an entity that is not present",
                    ))
                }
            },
            // A delete of an entity this batch created had no pre-batch
            // state to invert against; it stays refused as before.
            OperationKind::DeleteEntity if prior.entity(op.entity_id).is_none() => {
                return Err(CommandError::NotApplied(
                    "cannot invert a delete of an entity that is not present",
                ))
            }
            OperationKind::DeleteEntity => match before {
                Some(entity) => {
                    out.push(Operation::create(entity));
                    current.insert(op.entity_id, None);
                }
                None => {
                    return Err(CommandError::NotApplied(
                        "cannot invert a delete of an entity that is not present",
                    ))
                }
            },
        }
    }
    out.reverse();
    Ok(out)
}

impl Session {
    fn reproject(&mut self) {
        self.projection = project_from_with_tombstones(&self.baseline, &self.tombstones, &self.log);
    }

    /// R8.4.4: exceeding capacity drops the OLDEST entry of THAT stack and
    /// never blocks or corrupts anything.
    fn push_undo(&mut self, entry: StackEntry) {
        self.undo.push_back(entry);
        while self.undo.len() > self.undo_capacity {
            self.undo.pop_front();
            self.diagnostics
                .push("undo stack full: oldest entry dropped".to_owned());
        }
    }

    fn push_redo(&mut self, entry: StackEntry) {
        self.redo.push_back(entry);
        while self.redo.len() > self.redo_capacity {
            self.redo.pop_front();
            self.diagnostics
                .push("redo stack full: oldest entry dropped".to_owned());
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{
        Entity, Hinge, JoinMember, Level, Opening, OpeningKind, Point, Role, Side, Wall, WallJoin,
    };
    use serde_json::json;
    use std::collections::BTreeMap;

    fn set(id: Uuid, name: &str, v: serde_json::Value) -> Operation {
        let mut m = serde_json::Map::new();
        m.insert(name.to_owned(), v);
        Operation::set_fields(id, m)
    }

    /// A session with one level, committed as one command.
    fn with_level() -> (Session, Uuid) {
        let mut s = Session::new();
        let level = Level::new("Level 1");
        let id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("create level");
        (s, id)
    }

    /// A session whose level is part of the BASELINE rather than of a
    /// committed batch.
    ///
    /// R2.2 requires at least one level at every step, so a level created by a
    /// command cannot be undone away when it is the only one — which is
    /// correct, and which makes a committed level the wrong fixture for any
    /// test that undoes its whole history. A baseline level is what the editor
    /// opens with (`domowik_plan_create_default`) and what a plan saved after
    /// a compaction carries. NOT every stored baseline: an older snapshot may
    /// be empty, which is why the guard is an admission rule and not a
    /// document invariant.
    fn on_a_baseline_level() -> (Session, Uuid) {
        let baseline = Document::with_ground_floor();
        let id = baseline.levels[0].id;
        let s = Session::open_snapshot(Uuid::now_v7(), baseline, &Default::default(), &[], vec![])
            .expect("baseline opens");
        (s, id)
    }

    fn wall_on(level: Uuid) -> Wall {
        Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        )
    }

    // History tests need distinct legal walls, not repeated coincident axes.
    fn wall_at_y(level: Uuid, y: f64) -> Wall {
        let mut wall = wall_on(level);
        wall.start_mm[1] = y;
        wall.end_mm[1] = y;
        wall
    }

    // ---- R3.1.2: new-admission wall contacts, not historical replay ----

    fn baseline_walls(overlap: bool) -> (Session, Uuid, Uuid, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let a = wall_on(level);
        let mut b = wall_at_y(level, if overlap { 0.0 } else { 500.0 });
        b.start_mm[0] = 2000.0;
        b.end_mm[0] = 6000.0;
        let ids = (a.id, b.id);
        doc.walls = vec![a, b];
        let s = Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], vec![])
            .expect("historical baseline opens unchanged");
        (s, level, ids.0, ids.1)
    }

    #[test]
    fn raw_wall_contacts_refuse_atomically_in_global_precedence_order() {
        let (mut s, level, a, _) = baseline_walls(false);
        let overlap = Wall::new(
            level,
            Point::new(2000.0, 0.0),
            Point::new(6000.0, 0.0),
            200.0,
            2500.0,
        );
        let coincidence = Wall::new(
            level,
            Point::new(4000.0, -0.0),
            Point::new(0.0, 0.0),
            200.0,
            2500.0,
        );
        let before = admission_snapshot(&s);
        let err = s
            .commit(vec![
                set(a, "heightMm", json!(3000.0)),
                Operation::create(Entity::Wall(overlap)),
                Operation::create(Entity::Wall(coincidence)),
            ])
            .expect_err("coincidence before all overlap pairs");
        let CommandError::WallContact(blocks) = err else {
            panic!("wrong refusal: {err}")
        };
        assert!(matches!(
            blocks[0],
            WallContactBlock::WallCoincidence { .. }
        ));
        assert!(blocks
            .iter()
            .skip(1)
            .all(|b| matches!(b, WallContactBlock::CollinearOverlap { .. })));
        assert_eq!(admission_snapshot(&s), before);
    }

    // ---- ED-0119: no unresolved contacts from raw, external or history ----

    fn axis(level: Uuid, a: (f64, f64), b: (f64, f64)) -> Wall {
        Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        )
    }

    /// A session whose baseline holds `walls` — history, opened unguarded.
    fn legacy(walls: impl FnOnce(Uuid) -> Vec<Wall>) -> (Session, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls = walls(level);
        let s = Session::open_snapshot(Uuid::now_v7(), doc, &BTreeMap::new(), &[], vec![])
            .expect("historical baseline opens unchanged");
        (s, level)
    }

    #[test]
    fn raw_unresolved_crossings_and_unsplit_contacts_refuse_atomically() {
        let (mut s, level) = legacy(|l| vec![wall_on(l)]);
        let host = s.document().walls[0].id;
        let cases = [
            // Across the host's interior: an overpass.
            (axis(level, (2000.0, -1000.0), (2000.0, 1000.0)), true),
            // Ending on the host's interior without splitting it.
            (axis(level, (1000.0, 0.0), (1000.0, 1000.0)), false),
        ];
        for (wall, crossing) in cases {
            let id = wall.id;
            let before = admission_snapshot(&s);
            let err = s
                .commit(vec![Operation::create(Entity::Wall(wall))])
                .expect_err("an unresolved contact is refused");
            let CommandError::WallContact(blocks) = err else {
                panic!("wrong refusal: {err}")
            };
            let expected = if crossing {
                WallContactBlock::UnresolvedCrossing {
                    wall: host.min(id),
                    other: host.max(id),
                }
            } else {
                WallContactBlock::UnsplitContact {
                    wall: host,
                    other: id,
                }
            };
            assert_eq!(blocks, vec![expected]);
            assert!(!blocks[0].is_axis_refusal());
            assert_eq!(admission_snapshot(&s), before);
        }
        // An end a tenth of a micrometre inside the host's end is that end
        // (one topology cell): a corner, admitted raw.
        s.commit(vec![Operation::create(Entity::Wall(axis(
            level,
            (3999.9999, 0.0),
            (3999.9999, 3000.0),
        )))])
        .expect("an endpoint in the host end's cell is not unsplit");
    }

    #[test]
    fn external_batches_cannot_create_an_overpass() {
        let (mut s, level) = legacy(|l| vec![wall_on(l)]);
        let bad = Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Wall(axis(
                level,
                (2000.0, -1000.0),
                (2000.0, 1000.0),
            )))],
        );
        let before = admission_snapshot(&s);
        assert!(matches!(
            s.accept_external(bad),
            Err(CommandError::WallContact(ref b))
                if matches!(b.as_slice(), [WallContactBlock::UnresolvedCrossing { .. }])
        ));
        assert_eq!(admission_snapshot(&s), before);
    }

    #[test]
    fn legacy_overpass_survives_neutral_edits_and_its_repair_cannot_be_undone() {
        let (mut s, _) = legacy(|l| vec![wall_on(l), axis(l, (2000.0, -1000.0), (2000.0, 1000.0))]);
        let over = s.document().walls[1].id;
        s.commit(vec![set(over, "thicknessMm", json!(300.0))])
            .expect("a neutral edit of a legacy overpass is legal");
        s.commit(vec![
            set(over, "startMm", json!([5000.0, -1000.0])),
            set(over, "endMm", json!([5000.0, 1000.0])),
        ])
        .expect("moving it clear repairs it");
        let before = admission_snapshot(&s);
        assert!(matches!(
            s.undo(),
            Err(CommandError::WallContact(ref b))
                if matches!(b.as_slice(), [WallContactBlock::UnresolvedCrossing { .. }])
        ));
        assert_eq!(
            admission_snapshot(&s),
            before,
            "failed undo keeps its stacks"
        );
    }

    #[test]
    fn contact_guard_checks_final_not_intermediate_operations_and_history_round_trips() {
        let (mut s, _, a, b) = baseline_walls(false);
        let before = s.document().clone();
        s.commit(vec![
            set(b, "startMm", json!([2000.0, 0.0])),
            set(b, "endMm", json!([6000.0, 0.0])),
            set(a, "startMm", json!([0.0, -500.0])),
            set(a, "endMm", json!([4000.0, -500.0])),
        ])
        .expect("intermediate overlap is removed in the same batch");
        let after = s.document().clone();
        assert_eq!(s.undo_depth(), 1);
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.document(), &before);
        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document(), &after);
    }

    #[test]
    fn legacy_overlap_is_preserved_neutral_edits_work_and_repair_cannot_be_undone() {
        let (mut s, _, a, b) = baseline_walls(true);
        assert_eq!(s.document().walls.len(), 2);
        assert!(!s.is_dirty());
        s.commit(vec![set(a, "heightMm", json!(3000.0))])
            .expect("neutral legacy edit");
        let repair_id = s
            .commit(vec![
                set(b, "startMm", json!([2000.0, 500.0])),
                set(b, "endMm", json!([6000.0, 500.0])),
            ])
            .expect("repair");
        let repair = s
            .log()
            .batches()
            .iter()
            .find(|b| b.batch_id == repair_id)
            .unwrap()
            .clone();
        let before = admission_snapshot(&s);
        assert!(matches!(s.undo(), Err(CommandError::WallContact(_))));
        assert_eq!(
            admission_snapshot(&s),
            before,
            "failed undo restores its stack entry"
        );
        assert!(matches!(
            s.compensate(&repair),
            Err(CommandError::WallContact(_))
        ));
        assert_eq!(admission_snapshot(&s), before, "compensation has no bypass");
    }

    #[test]
    fn external_overlap_refuses_and_duplicate_delivery_preserves_history() {
        let (mut s, level, a, _) = baseline_walls(false);
        let other = Uuid::now_v7();
        let bad = Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Wall(wall_on(level)))],
        );
        let before = admission_snapshot(&s);
        assert!(matches!(
            s.accept_external(bad),
            Err(CommandError::WallContact(_))
        ));
        assert_eq!(admission_snapshot(&s), before);
        let good = Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![set(a, "heightMm", json!(3100.0))],
        );
        assert_eq!(s.accept_external(good.clone()).unwrap(), Delivery::Accepted);
        s.commit(vec![set(a, "heightMm", json!(3200.0))]).unwrap();
        let before = admission_snapshot(&s);
        assert_eq!(s.accept_external(good).unwrap(), Delivery::Duplicate);
        assert_eq!(admission_snapshot(&s), before);
    }

    #[test]
    fn wall_overlap_in_a_promoted_batch_refuses_local_and_external_prerequisites() {
        for local in [false, true] {
            let (mut s, level, _, _) = baseline_walls(false);
            let prerequisite_actor = if local { s.actor() } else { Uuid::now_v7() };
            let mut ahead = s.log().frontier();
            ahead.insert(prerequisite_actor, 1);
            let held = Batch::new(
                Uuid::now_v7(),
                1,
                ahead,
                vec![Operation::create(Entity::Wall(wall_on(level)))],
            );
            assert_eq!(s.accept_external(held).unwrap(), Delivery::Held);
            let before = admission_snapshot(&s);
            let ops = vec![set(level, "name", json!("Renamed"))];
            let result = if local {
                s.commit(ops).map(|_| ())
            } else {
                s.accept_external(Batch::new(prerequisite_actor, 1, s.log().frontier(), ops))
                    .map(|_| ())
            };
            assert!(matches!(result, Err(CommandError::WallContact(_))));
            assert_eq!(
                admission_snapshot(&s),
                before,
                "held batch remains held, prerequisite not admitted"
            );
        }
    }

    #[test]
    fn a_later_promoted_repair_does_not_excuse_an_invalid_newly_effective_batch() {
        let (mut s, level, _, _) = baseline_walls(false);
        let actor = Uuid::now_v7();
        let duplicate = wall_on(level);
        let mut after_first = s.log().frontier();
        after_first.insert(actor, 1);
        let bad = Batch::new(
            actor,
            2,
            after_first,
            vec![Operation::create(Entity::Wall(duplicate.clone()))],
        );
        let mut after_second = s.log().frontier();
        after_second.insert(actor, 2);
        let repair = Batch::new(
            actor,
            3,
            after_second,
            vec![Operation::delete(duplicate.id)],
        );
        assert_eq!(s.accept_external(repair).unwrap(), Delivery::Held);
        assert_eq!(s.accept_external(bad).unwrap(), Delivery::Held);
        let before = admission_snapshot(&s);
        let first = Batch::new(
            actor,
            1,
            s.log().frontier(),
            vec![set(level, "name", json!("Renamed"))],
        );
        assert!(matches!(
            s.accept_external(first),
            Err(CommandError::WallContact(_))
        ));
        assert_eq!(admission_snapshot(&s), before);
    }

    #[test]
    fn canonical_discard_can_change_walls_not_targeted_by_the_incoming_batch() {
        let (legacy, _, _, b) = baseline_walls(true);
        let mut baseline = legacy.document().clone();
        let marker = Level::new("Marker");
        let marker_id = marker.id;
        baseline.levels.push(marker);
        // Accepted history repaired B, but also requires this unrelated marker.
        let late = Batch::new(
            LATE,
            1,
            BTreeMap::new(),
            vec![
                set(marker_id, "name", json!("Observed")),
                set(b, "startMm", json!([2000.0, 500.0])),
                set(b, "endMm", json!([6000.0, 500.0])),
            ],
        );
        let mut s =
            Session::open_snapshot(Uuid::now_v7(), baseline, &BTreeMap::new(), &[], vec![late])
                .unwrap();
        assert!(wall_contact_refusals(&Document::new(), s.document()).is_empty());
        let before = admission_snapshot(&s);
        let early = Batch::new(
            EARLY,
            1,
            BTreeMap::new(),
            vec![Operation::delete(marker_id)],
        );
        // At its own position this deletion changes no wall. It makes the
        // historical repair stale, so ONLY the complete candidate check sees B.
        assert!(matches!(
            s.accept_external(early),
            Err(CommandError::WallContact(_))
        ));
        assert_eq!(admission_snapshot(&s), before);
    }

    #[test]
    fn discarded_external_batches_are_not_refused_for_contacts_they_never_commit() {
        for malformed in [false, true] {
            let (mut s, level, a, _) = baseline_walls(false);
            let impossible = if malformed {
                set(a, "heightMm", json!("not numeric"))
            } else {
                set(Uuid::now_v7(), "heightMm", json!(3000.0))
            };
            let batch = Batch::new(
                Uuid::now_v7(),
                1,
                s.log().frontier(),
                vec![Operation::create(Entity::Wall(wall_on(level))), impossible],
            );
            let before = s.document().clone();
            assert_eq!(s.accept_external(batch).unwrap(), Delivery::Accepted);
            assert_eq!(s.document(), &before);
            assert_eq!(s.projection().discarded.len(), 1);
        }
    }

    // ---- R8.2: boundaries ----

    #[test]
    fn each_accepted_command_pushes_exactly_one_undo_entry() {
        // R8.2.1: exactly one undo entry per accepted operation batch.
        let (mut s, level) = with_level();
        assert_eq!(s.undo_depth(), 1);
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        assert_eq!(s.undo_depth(), 2);
        assert_eq!(s.log().batches().len(), 2);
    }

    #[test]
    fn a_rejected_command_changes_nothing_and_pushes_no_entry() {
        // R8.2.2 and FORMAT-SPEC §4.3 row 1: rejected atomically BEFORE any
        // effect; state unchanged; no undo entry.
        let (mut s, level) = with_level();
        let before = s.document().clone();
        let batches_before = s.log().batches().len();

        // A zero-length wall is a user-input validation failure.
        let bad = Wall::new(
            level,
            Point::new(1.0, 1.0),
            Point::new(1.0, 1.0),
            200.0,
            2500.0,
        );
        let err = s
            .commit(vec![Operation::create(Entity::Wall(bad))])
            .expect_err("must reject");
        assert!(matches!(err, CommandError::Invalid(_)), "got {err:?}");

        assert_eq!(s.document(), &before, "no effect may reach the projection");
        assert_eq!(
            s.log().batches().len(),
            batches_before,
            "no batch in the log"
        );
        assert_eq!(s.undo_depth(), 1, "no undo entry");
        assert_eq!(s.redo_depth(), 0);
    }

    // ---- R8.1: architecture ----

    #[test]
    fn undo_appends_a_compensating_batch_rather_than_removing_history() {
        // R8.1.1: history grows monotonically.
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        assert_eq!(s.log().batches().len(), 2);

        s.undo().expect("undo").expect("an entry existed");
        assert_eq!(s.document().walls.len(), 0, "the wall is gone");
        assert_eq!(
            s.log().batches().len(),
            3,
            "history GREW: the original batches are still there"
        );
    }

    #[test]
    fn every_undo_and_redo_mints_fresh_batch_and_operation_ids() {
        // R8.1.2: entries are never popped-and-reused.
        let (mut s, level) = with_level();
        let created = s
            .commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        let undone = s.undo().expect("undo").expect("entry");
        let redone = s.redo().expect("redo").expect("entry");
        assert_ne!(created, undone);
        assert_ne!(undone, redone);
        assert_ne!(created, redone);

        let mut op_ids: Vec<Uuid> = s
            .log()
            .batches()
            .iter()
            .flat_map(|b| b.operations.iter().map(|o| o.operation_id))
            .collect();
        let total = op_ids.len();
        op_ids.sort();
        op_ids.dedup();
        assert_eq!(op_ids.len(), total, "every operation id must be distinct");
    }

    #[test]
    fn redo_restores_the_same_entity_id_through_its_own_tombstone() {
        // R8.1.3 with E6: authorized restoration of a tombstoned identity by
        // the same device's own undo. This is the rule that makes redo of a
        // creation possible at all.
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("wall");

        s.undo().expect("undo").expect("entry");
        assert!(
            s.projection().is_tombstoned(wall_id),
            "undo of a create writes a tombstone"
        );

        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().walls.len(), 1);
        assert_eq!(
            s.document().walls[0].id,
            wall_id,
            "redo must restore the SAME entity id"
        );
        assert!(s.projection().is_live(wall_id));
    }

    #[test]
    fn undo_and_redo_round_trip_a_field_edit_to_its_prior_value() {
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("wall");
        s.commit(vec![set(wall_id, "thicknessMm", json!(150.0))])
            .expect("edit");
        assert_eq!(s.document().walls[0].thickness_mm, 150.0);

        s.undo().expect("undo").expect("entry");
        assert_eq!(s.document().walls[0].thickness_mm, 200.0);

        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().walls[0].thickness_mm, 150.0);
    }

    #[test]
    fn undo_of_a_multi_operation_batch_reverses_it_in_dependency_order() {
        // R3.1.5/R8.1.1: inverse effects along REVERSE dependency edges. The
        // wall depends on the level, so undo must delete the wall first —
        // deleting the level first would leave a dangling reference.
        // On a BASELINE level, so undoing the created level is not also
        // deleting the last one (R2.2) — that is a separate rule with its own
        // tests, and this one is about dependency order.
        let (mut s, baseline_level) = on_a_baseline_level();
        let level = Level::new("Level 2");
        let level_id = level.id;
        let wall = wall_on(level_id);
        let wall_id = wall.id;
        s.commit(vec![
            Operation::create(Entity::Level(level)),
            Operation::create(Entity::Wall(wall)),
        ])
        .expect("both");

        s.undo().expect("undo").expect("entry");
        assert_eq!(
            s.document().levels.iter().map(|l| l.id).collect::<Vec<_>>(),
            [baseline_level],
            "the created level went; the baseline stayed"
        );
        assert!(s.document().walls.is_empty());
        assert!(s.projection().is_tombstoned(level_id));
        assert!(s.projection().is_tombstoned(wall_id));
        assert!(
            s.projection().discarded.is_empty(),
            "a dangling reference would have discarded the batch: {:?}",
            s.projection().discarded
        );
    }

    #[test]
    fn undo_of_a_batch_that_creates_then_edits_needs_only_the_delete() {
        // Inverting the edit would write to an entity the same inverse batch
        // deletes. The delete already undoes both.
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![
            Operation::create(Entity::Wall(wall)),
            set(wall_id, "thicknessMm", json!(150.0)),
        ])
        .expect("create and edit");
        assert_eq!(s.document().walls[0].thickness_mm, 150.0);

        s.undo().expect("undo").expect("entry");
        assert!(s.document().walls.is_empty());
        assert!(s.projection().discarded.is_empty());
    }

    #[test]
    fn undo_of_a_delete_restores_the_entity_with_its_fields() {
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("wall");
        s.commit(vec![set(wall_id, "thicknessMm", json!(150.0))])
            .expect("edit");
        s.commit(vec![Operation::delete(wall_id)]).expect("delete");
        assert!(s.document().walls.is_empty());

        s.undo().expect("undo").expect("entry");
        assert_eq!(s.document().walls.len(), 1);
        assert_eq!(s.document().walls[0].id, wall_id);
        assert_eq!(
            s.document().walls[0].thickness_mm,
            150.0,
            "the edited value, not the created one"
        );
    }

    // ---- R8.4.1: redo invalidation ----

    #[test]
    fn an_accepted_local_command_clears_the_redo_stack() {
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.redo_depth(), 1);

        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("another wall");
        assert_eq!(s.redo_depth(), 0, "R8.4.1");
    }

    #[test]
    fn a_compensation_batch_from_an_undo_does_not_clear_redo() {
        // R8.4.1 names this exemption explicitly: without it, undoing twice
        // would destroy the redo entry the first undo just created.
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("a");
        s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
            level, 500.0,
        )))])
        .expect("b");
        s.undo().expect("undo").expect("entry");
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.redo_depth(), 2, "both undos must be redoable");

        s.redo().expect("redo").expect("entry");
        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().walls.len(), 2);
        assert_eq!(s.redo_depth(), 0);
        assert_eq!(s.undo_depth(), 3);
    }

    #[test]
    fn undo_on_an_empty_stack_is_a_no_op_not_an_error() {
        let mut s = Session::new();
        assert_eq!(s.undo().expect("no error"), None);
        assert_eq!(s.redo().expect("no error"), None);
        assert!(s.log().batches().is_empty());
    }

    // ---- R2.2/R2.3: level deletion ----

    /// State that a refusal must leave exactly as it was.
    fn snapshot_of(s: &Session) -> (Document, usize, usize, usize, bool) {
        (
            s.document().clone(),
            s.log().batches().len(),
            s.undo_depth(),
            s.redo_depth(),
            s.is_dirty(),
        )
    }

    #[test]
    fn deleting_the_only_level_is_refused_atomically() {
        let (mut s, level) = on_a_baseline_level();
        let before = snapshot_of(&s);
        let err = s
            .commit(vec![Operation::delete(level)])
            .expect_err("R2.2 keeps at least one level");
        assert!(
            matches!(&err, CommandError::LevelDeletion(b)
                if b == &[LevelDeletionBlock::LastLevel(level)]),
            "got {err:?}"
        );
        assert_eq!(snapshot_of(&s), before, "no log, stack or dirty movement");
    }

    #[test]
    fn deleting_an_occupied_level_lists_the_blocking_kinds_and_counts() {
        let (mut s, level) = on_a_baseline_level();
        s.commit(vec![Operation::create(Entity::Level(Level::new(
            "Level 2",
        )))])
        .expect("a second level, so LastLevel cannot fire");
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("wall");

        let before = snapshot_of(&s);
        let err = s
            .commit(vec![Operation::delete(level)])
            .expect_err("R2.3 never cascades");
        assert!(
            matches!(&err, CommandError::LevelDeletion(b)
            if b == &[LevelDeletionBlock::Occupied {
                level,
                kind: "wall(s)",
                count: 1,
            }]),
            "got {err:?}"
        );
        assert_eq!(snapshot_of(&s), before);

        // The SAME batch may remove the wall first: that is explicit, not a
        // cascade, and the guard runs against the state each step sees.
        s.commit(vec![Operation::delete(wall_id), Operation::delete(level)])
            .expect("wall removed before its level");
        assert!(s.document().level(level).is_none());
        assert!(s.document().walls.is_empty());
    }

    #[test]
    fn every_blocking_kind_is_counted_not_just_walls() {
        // R2.3 requires the blocking KINDS and counts, so each kind has to be
        // reachable. Openings and joins hang off walls, so a wall-only fixture
        // would pass while silently never exercising either.
        let (mut s, level) = on_a_baseline_level();
        s.commit(vec![Operation::create(Entity::Level(Level::new(
            "Level 2",
        )))])
        .expect("a second level, so LastLevel cannot fire");

        // Two walls meeting at (4000, 0), joined there, with a door on the
        // first: one level holding a wall, an opening and a join.
        let first = wall_on(level);
        let first_id = first.id;
        let second = Wall::new(
            level,
            Point::new(4000.0, 0.0),
            Point::new(4000.0, 3000.0),
            200.0,
            2500.0,
        );
        let second_id = second.id;
        let door = Opening {
            id: Uuid::now_v7(),
            kind: OpeningKind::Door,
            mechanism: None,
            host_wall_id: first_id,
            u: 0.5,
            width_mm: 900.0,
            height_mm: 2100.0,
            sill_mm: 0.0,
            depth_mm: 200.0,
            side: Side::Left,
            hinge: Hinge::NearStart,
            opens_toward: Side::Left,
            swing_deg: 90.0,
            style_refs: Vec::new(),
            unknown: Default::default(),
        };
        let join = WallJoin::new(vec![
            JoinMember::new(first_id, Role::End),
            JoinMember::new(second_id, Role::Start),
        ]);
        s.commit(vec![
            Operation::create(Entity::Wall(first)),
            Operation::create(Entity::Wall(second)),
            Operation::create(Entity::Opening(door)),
            Operation::create(Entity::WallJoin(join)),
        ])
        .expect("a populated level");

        let before = snapshot_of(&s);
        let err = s
            .commit(vec![Operation::delete(level)])
            .expect_err("R2.3 lists every blocking kind");
        let CommandError::LevelDeletion(blocks) = &err else {
            panic!("got {err:?}");
        };
        assert_eq!(
            blocks,
            &[
                LevelDeletionBlock::Occupied {
                    level,
                    kind: "wall(s)",
                    count: 2,
                },
                LevelDeletionBlock::Occupied {
                    level,
                    kind: "opening(s)",
                    count: 1,
                },
                LevelDeletionBlock::Occupied {
                    level,
                    kind: "join(s)",
                    count: 1,
                },
            ],
            "walls, openings and joins each reported with their own count"
        );
        assert_eq!(snapshot_of(&s), before);
    }

    #[test]
    fn a_replacement_level_must_be_created_before_the_last_one_is_deleted() {
        // The asymmetry with the wall case above, and it falls out of the same
        // rule: the invariant is evaluated AFTER EACH operation, so at the
        // delete step there must already be another level.
        let (mut s, level) = on_a_baseline_level();
        let replacement = Level::new("Level 2");
        let replacement_id = replacement.id;

        let err = s
            .commit(vec![
                Operation::delete(level),
                Operation::create(Entity::Level(replacement.clone())),
            ])
            .expect_err("a later create does not repair an earlier deletion");
        assert!(matches!(err, CommandError::LevelDeletion(_)), "got {err:?}");

        s.commit(vec![
            Operation::create(Entity::Level(replacement)),
            Operation::delete(level),
        ])
        .expect("create the replacement first");
        assert_eq!(
            s.document().levels.iter().map(|l| l.id).collect::<Vec<_>>(),
            [replacement_id]
        );
    }

    #[test]
    fn an_unknown_level_target_is_reported_rather_than_ignored() {
        // Not reachable through `commit` — a delete of an absent id is stale
        // and discarded before the guard — so the shared helper is asked
        // directly. It is the entry point the CLI and bridge use to explain a
        // refusal before minting a batch.
        let doc = Document::with_ground_floor();
        let ghost = Uuid::now_v7();
        assert_eq!(
            crate::replay::level_deletion_refusals(&doc, ghost),
            [LevelDeletionBlock::UnknownLevel(ghost)]
        );
    }

    #[test]
    fn undo_redo_and_compensation_are_all_guarded() {
        let (mut s, level) = on_a_baseline_level();
        let second = Level::new("Level 2");
        let second_id = second.id;
        let batch = s
            .commit(vec![Operation::create(Entity::Level(second))])
            .expect("second level");

        // Undo of that create deletes Level 2, leaving one: allowed.
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.document().levels.len(), 1);
        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().levels.len(), 2);

        // Now delete the BASELINE level, then undo: the inverse recreates it,
        // which is a create and never blocked.
        s.commit(vec![Operation::delete(level)]).expect("allowed");
        assert_eq!(s.document().levels.len(), 1);
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.document().levels.len(), 2);

        // COMPENSATION of the batch that created Level 2 would delete it;
        // one level survives, so it is admissible.
        let batch = s
            .log()
            .batches()
            .iter()
            .find(|b| b.batch_id == batch)
            .expect("the batch")
            .clone();
        s.compensate(&batch)
            .expect("compensation is guarded, not blocked");
        assert!(s.document().level(second_id).is_none());
    }

    #[test]
    fn compensating_the_batch_that_created_the_only_level_is_refused() {
        // A SINGLE-operation batch, so there is no inverse-ordering question:
        // the compensation of one create is exactly one delete, and that
        // delete takes the last level.
        let mut s = Session::new();
        let level = Level::new("Level 1");
        let level_id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("the only level");
        let batch = s.log().batches()[0].clone();
        assert_eq!(batch.operations.len(), 1, "one operation, one inverse");

        let before = snapshot_of(&s);
        let err = s
            .compensate(&batch)
            .expect_err("its compensation deletes the only level");
        assert!(
            matches!(&err, CommandError::LevelDeletion(b)
                if b == &[LevelDeletionBlock::LastLevel(level_id)]),
            "got {err:?}"
        );
        assert_eq!(
            snapshot_of(&s),
            before,
            "no compensating batch entered the log; stacks and dirty unmoved"
        );
    }

    #[test]
    fn an_external_batch_deleting_the_only_level_is_refused_before_admission() {
        let (mut s, level) = on_a_baseline_level();
        let before = snapshot_of(&s);
        let external = Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![Operation::delete(level)],
        );
        let err = s
            .accept_external(external)
            .expect_err("admission guard, not a document invariant");
        assert!(matches!(err, CommandError::LevelDeletion(_)), "got {err:?}");
        assert_eq!(snapshot_of(&s), before, "nothing entered the log");
    }

    #[test]
    fn a_held_batch_is_guarded_when_it_is_promoted() {
        // A batch that arrives out of order sits HELD and is judged by nothing
        // until its prerequisites land. Promotion puts it at a canonical
        // position for the first time, which is exactly when R2.3 must see it.
        let (mut s, level) = on_a_baseline_level();
        let other = Uuid::now_v7();
        let first = Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        );
        // Counter 2 with counter 1 unseen: held, not applied, not refused.
        let mut ahead = s.log().frontier();
        ahead.insert(other, 1);
        let second = Batch::new(other, 2, ahead, vec![Operation::delete(level)]);
        assert_eq!(
            s.accept_external(second).expect("held, not judged"),
            Delivery::Held
        );
        assert_eq!(s.document().levels.len(), 1, "held batches do not apply");

        // Delivering its prerequisite promotes it. Both levels exist at that
        // point, so the deletion is admissible and BOTH land.
        assert_eq!(
            s.accept_external(first).expect("accepted"),
            Delivery::Accepted
        );
        assert!(s.document().level(level).is_none(), "the promotion applied");
        assert_eq!(s.document().levels.len(), 1);
    }

    /// Everything a refused admission must leave untouched, including the
    /// LOG's exact contents: accepted batches with their actors and counters,
    /// and the ids still held. A refusal that quietly accepted the
    /// prerequisite, or dropped the held batch, would pass a document-only
    /// comparison.
    #[allow(clippy::type_complexity)]
    fn admission_snapshot(
        s: &Session,
    ) -> (
        Vec<(Uuid, Uuid, u64)>,
        Vec<Uuid>,
        Document,
        usize,
        usize,
        bool,
    ) {
        (
            s.log()
                .batches()
                .iter()
                .map(|b| (b.batch_id, b.actor_id, b.counter_base))
                .collect(),
            s.log().held().iter().map(|b| b.batch_id).collect(),
            s.document().clone(),
            s.undo_depth(),
            s.redo_depth(),
            s.is_dirty(),
        )
    }

    #[test]
    fn an_external_prerequisite_promoting_a_blocked_delete_refuses_both() {
        // The REJECTION half of held promotion. The success case alone would
        // pass with the promotion guard removed entirely; only this can tell
        // that a promoted batch is judged at its canonical position.
        let (mut s, level) = on_a_baseline_level();
        let other = Uuid::now_v7();

        // Counter 2 arrives first and waits on counter 1: held, unjudged.
        let mut ahead = s.log().frontier();
        ahead.insert(other, 1);
        let blocked = Batch::new(other, 2, ahead, vec![Operation::delete(level)]);
        assert_eq!(
            s.accept_external(blocked).expect("held"),
            Delivery::Held,
            "a held batch is not judged on arrival"
        );

        // The prerequisite is innocuous ON ITS OWN — it renames the level —
        // but delivering it promotes the deletion that takes the last level.
        let before = admission_snapshot(&s);
        let prerequisite = Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![set(level, "name", json!("Renamed"))],
        );
        let err = s
            .accept_external(prerequisite)
            .expect_err("the promotion it causes is refused");
        assert!(
            matches!(&err, CommandError::LevelDeletion(b)
                if b == &[LevelDeletionBlock::LastLevel(level)]),
            "got {err:?}"
        );
        assert_eq!(
            admission_snapshot(&s),
            before,
            "BOTH refused atomically: the prerequisite never entered the log \
             and the blocked batch is still held"
        );
    }

    #[test]
    fn a_local_commit_promoting_a_blocked_delete_is_refused_atomically() {
        // The other slow admission path. The held batch waits on a LOCAL
        // counter, so it is a local commit — not another external delivery —
        // that promotes it, and `candidate_for`'s held branch is what judges
        // the promotion.
        let (mut s, level) = on_a_baseline_level();
        let other = Uuid::now_v7();
        let local = s.actor();

        // Waits on the local actor's first batch, which does not exist yet.
        let mut ahead = s.log().frontier();
        ahead.insert(local, s.log().next_counter(local));
        let blocked = Batch::new(other, 1, ahead, vec![Operation::delete(level)]);
        assert_eq!(s.accept_external(blocked).expect("held"), Delivery::Held);
        assert_eq!(s.document().levels.len(), 1, "held, so not applied");

        // A RENAME, not a wall: adding a wall to the level would make the
        // promoted deletion blocked as occupied too, and this test is about
        // the promotion being judged at all.
        let before = admission_snapshot(&s);
        let err = s
            .commit(vec![set(level, "name", json!("Renamed"))])
            .expect_err("this commit promotes a deletion of the last level");
        assert!(
            matches!(&err, CommandError::LevelDeletion(b)
                if b == &[LevelDeletionBlock::LastLevel(level)]),
            "got {err:?}"
        );
        assert_eq!(
            admission_snapshot(&s),
            before,
            "the local command is refused too: no batch, no stack entry, \
             no dirty change, and the blocked batch is still held"
        );
    }

    /// Two levels in the baseline, and two actors whose ids fix the canonical
    /// order: `EARLY` sorts before `LATE` at equal causal past and counter.
    const EARLY: Uuid = Uuid::from_u128(1);
    const LATE: Uuid = Uuid::from_u128(u128::MAX);

    fn two_baseline_levels() -> (Session, Uuid, Uuid) {
        let mut baseline = Document::with_ground_floor();
        let upper = Level::new("Level 2");
        let upper_id = upper.id;
        baseline.levels.push(upper);
        let lower_id = baseline.levels[0].id;
        let s = Session::open_snapshot(Uuid::now_v7(), baseline, &Default::default(), &[], vec![])
            .expect("baseline opens");
        (s, lower_id, upper_id)
    }

    #[test]
    fn a_batch_reordered_before_an_accepted_deletion_cannot_empty_the_levels() {
        // The per-operation guard is judged at each batch's OWN canonical
        // position, so it cannot see this: the incoming deletion sorts EARLIER
        // than an already-accepted one, is judged while both levels still
        // exist, and the accepted batch then replays unguarded after it. Only
        // the final state shows zero levels.
        let (mut s, lower, upper) = two_baseline_levels();

        // Accepted first, deletes the LOWER level. One level remains.
        let historical = Batch::new(LATE, 1, s.log().frontier(), vec![Operation::delete(lower)]);
        assert_eq!(
            s.accept_external(historical).expect("accepted"),
            Delivery::Accepted
        );
        assert_eq!(s.document().levels.len(), 1, "the upper level survives");

        // Concurrent, and does NOT observe the batch above, so it has the same
        // causal past size and counter base; the actor id breaks the tie and
        // places it EARLIER in canonical order.
        let before = admission_snapshot(&s);
        let concurrent = Batch::new(EARLY, 1, Default::default(), vec![Operation::delete(upper)]);
        // The trace this rests on, asserted rather than assumed: equal causal
        // past and counter, so the actor id decides, and EARLY precedes the
        // accepted deletion.
        let accepted = &s.log().batches()[0];
        assert_eq!(accepted.actor_id, LATE);
        assert_eq!(
            concurrent.position().causal_past_size,
            accepted.position().causal_past_size
        );
        assert_eq!(concurrent.position().counter_base, accepted.counter_base);
        assert!(
            concurrent.position() < accepted.position(),
            "the incoming batch must sort EARLIER, or this proves nothing"
        );
        let err = s
            .accept_external(concurrent)
            .expect_err("the pair would leave no level at all");
        assert!(
            matches!(&err, CommandError::Invalid(e)
                if e == &[ValidationError::NoLevels]),
            "got {err:?}"
        );
        assert_eq!(admission_snapshot(&s), before, "refused atomically");
        assert_eq!(s.document().levels.len(), 1, "still one level");
    }

    // No local-path counterpart to the test above, and the absence is a
    // RESULT rather than a gap. Canonical order is
    // `(causalPastSize, counterBase, actor)`, and a batch is promoted by a
    // local commit only when it waits on a local counter that commit supplies
    // — which puts its causal past at or above everything already accepted.
    // It therefore sorts AFTER accepted history and cannot reorder before a
    // deletion already in the log. Local promotion is judged by the
    // per-operation guard instead, which
    // `a_local_commit_promoting_a_blocked_delete_is_refused_atomically`
    // covers. The final-state check is wired on that path too, defensively,
    // so the two branches admit the same things.

    #[test]
    fn a_legacy_empty_document_can_still_be_repaired() {
        // The final-state check is CONDITIONAL on the current document having
        // levels, so an empty builder or a legacy empty file still accepts
        // edits — including the create that repairs it.
        let mut s = Session::new();
        assert!(s.document().levels.is_empty());
        s.commit(vec![Operation::create(Entity::Wall(wall_on(
            Uuid::now_v7(),
        )))])
        .expect_err("a wall on no level is invalid for other reasons");
        s.commit(vec![Operation::create(Entity::Level(Level::new(
            "Level 1",
        )))])
        .expect("repairing an empty document is allowed");
        assert_eq!(s.document().levels.len(), 1);
    }

    #[test]
    fn a_discarded_batch_is_not_refused_for_a_deletion_it_never_performs() {
        // A batch whose LATER operation is stale is discarded whole (B1), so
        // its earlier protected deletion never happens. Raising the deletion
        // refusal instead would turn a silent discard into a rejection and
        // change delivery semantics for a batch this guard never judges.
        let (mut s, level) = on_a_baseline_level();
        let gone = wall_on(level);
        let gone_id = gone.id;
        s.commit(vec![Operation::create(Entity::Wall(gone))])
            .expect("wall");
        s.commit(vec![Operation::delete(gone_id)]).expect("delete");

        let before = snapshot_of(&s);
        let external = Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![
                // Would be refused on its own: the only level.
                Operation::delete(level),
                // Stale: the wall is already tombstoned, so the WHOLE batch is
                // discarded and the deletion above never takes effect.
                set(gone_id, "thicknessMm", json!(150.0)),
            ],
        );
        assert_eq!(
            s.accept_external(external).expect("discarded, not refused"),
            Delivery::Accepted
        );
        assert_eq!(
            s.document(),
            &before.0,
            "the batch was discarded, so nothing changed"
        );
        assert!(
            !s.projection().discarded.is_empty(),
            "and it is recorded as a discard"
        );
    }

    #[test]
    fn a_malformed_later_operation_discards_the_batch_exactly_as_a_stale_one_does() {
        // The other whole-batch discard path. Both must reach the same answer,
        // or the guard would refuse for one kind of doomed batch and stay
        // silent for the other.
        let (mut s, level) = on_a_baseline_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("wall");

        let before = snapshot_of(&s);
        let external = Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![
                // Would be refused on its own: the level holds that wall, and
                // it is the only level.
                Operation::delete(level),
                // Malformed: a string where a number belongs, so the payload
                // cannot apply and the WHOLE batch is discarded.
                set(wall_id, "thicknessMm", json!("not a number")),
            ],
        );
        assert_eq!(
            s.accept_external(external).expect("discarded, not refused"),
            Delivery::Accepted
        );
        assert_eq!(s.document(), &before.0, "nothing changed");
        assert!(
            s.projection()
                .discarded
                .iter()
                .any(|d| d.reason == crate::replay::DiscardReason::MalformedPayload),
            "recorded as a malformed-payload discard: {:?}",
            s.projection().discarded
        );
    }

    // ---- R8.3: the external-batch fence ----

    #[test]
    fn an_accepted_external_batch_clears_both_stacks() {
        // R8.3.1: local undo never crosses an external batch.
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.undo_depth(), 1);
        assert_eq!(s.redo_depth(), 1);

        let other = Uuid::now_v7();
        let external = Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        );
        assert_eq!(
            s.accept_external(external).expect("accepted"),
            Delivery::Accepted
        );
        assert_eq!(s.undo_depth(), 0, "R8.3.1");
        assert_eq!(s.redo_depth(), 0, "R8.3.1");
        assert_eq!(s.document().levels.len(), 2, "but it APPLIED");
    }

    #[test]
    fn an_external_batch_never_becomes_a_local_undo_entry() {
        // R8.3.2: external batches appear only in durable history.
        let (mut s, _) = with_level();
        let other = Uuid::now_v7();
        s.accept_external(Batch::new(
            other,
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        ))
        .expect("accepted");
        assert_eq!(s.undo_depth(), 0);
        assert_eq!(s.undo().expect("no error"), None);
    }

    #[test]
    fn an_accepted_external_batch_makes_the_document_dirty() {
        // R8.3.3.
        let (mut s, _) = with_level();
        s.mark_saved();
        assert!(!s.is_dirty());
        s.accept_external(Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        ))
        .expect("accepted");
        assert!(s.is_dirty(), "R8.3.3");
    }

    #[test]
    fn an_external_batch_that_would_invalidate_the_document_is_refused_with_no_effect() {
        // FORMAT-SPEC §4.3 row 1: `zero-length-wall` is a user-input
        // validation failure — "whole batch rejects atomically BEFORE any
        // effect; state unchanged". Nothing in that row is about the batch
        // being LOCAL, and an external batch that reaches the log can never be
        // declined afterwards: replay has no way to refuse it.
        //
        // `domowik apply` used to accept this and SAVE it, producing a file
        // the same binary's `validate` then rejected.
        let (mut s, level) = with_level();
        let before = s.document().clone();
        let log_len = s.log().canonical_order().len();

        let mut bad = wall_on(level);
        bad.end_mm = bad.start_mm;
        bad.thickness_mm = -5.0;
        let err = s
            .accept_external(Batch::new(
                Uuid::now_v7(),
                1,
                s.log().frontier(),
                vec![Operation::create(Entity::Wall(bad))],
            ))
            .expect_err("must refuse");

        match err {
            CommandError::Invalid(errors) => {
                assert_eq!(errors.len(), 2, "both problems are named: {errors:?}");
            }
            other => panic!("wrong error: {other:?}"),
        }
        // "State unchanged" means the LOG too, not just the projection: a
        // rejected batch that still grew durable history has had an effect.
        assert_eq!(s.document(), &before);
        assert_eq!(s.log().canonical_order().len(), log_len);
        assert!(s.document().validate().is_empty());
    }

    #[test]
    fn an_already_invalid_document_still_accepts_external_batches() {
        // The guard rejects errors a batch INTRODUCES, not ones it inherits.
        // A plan written before the guard existed can be invalid on disk, and
        // a tool that refuses every subsequent edit to such a plan is the tool
        // that made the mess refusing to let anyone edit their way out of it.
        let (mut s, level) = with_level();
        let mut bad = wall_on(level);
        bad.thickness_mm = -5.0;
        // Reach past `commit`'s guard the way a stored log would: deliver
        // straight into the log, then reproject.
        s.log
            .deliver(Batch::new(
                Uuid::now_v7(),
                1,
                s.log.frontier(),
                vec![Operation::create(Entity::Wall(bad))],
            ))
            .expect("delivered");
        s.reproject();
        assert!(!s.document().validate().is_empty(), "the setup is invalid");

        // A well-formed batch still applies.
        s.accept_external(Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        ))
        .expect("accepted despite the pre-existing problem");
        assert_eq!(s.document().levels.len(), 2);
    }

    #[test]
    fn a_duplicate_external_batch_does_not_fire_the_fence() {
        // R8.3.1 fires on an ACCEPTED batch. A duplicate is ignored silently
        // (I2) and produces no effects, so clearing the stacks would destroy
        // a user's undo history for nothing.
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        let external = Batch::new(
            Uuid::now_v7(),
            1,
            s.log().frontier(),
            vec![Operation::create(Entity::Level(Level::new("Level 2")))],
        );
        s.accept_external(external.clone()).expect("first");
        let depth_after_fence = s.undo_depth();
        s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
            level, 500.0,
        )))])
        .expect("local");
        assert_eq!(s.undo_depth(), depth_after_fence + 1);

        assert_eq!(
            s.accept_external(external).expect("second"),
            Delivery::Duplicate
        );
        assert_eq!(
            s.undo_depth(),
            depth_after_fence + 1,
            "the fence must not fire"
        );
    }

    // ---- R8.4.2: session scope ----

    #[test]
    fn opening_with_replayed_batches_leaves_the_document_clean_and_the_stacks_empty() {
        // R8.4.2: batches replayed WHILE CONSTRUCTING the projection are part
        // of that open. They never fire the R8.3.1 fence and dirty is FALSE
        // relative to the opened source. Conflating this with a post-open
        // delivery is how a freshly opened document claims unsaved changes.
        let actor = Uuid::now_v7();
        let level = Level::new("Level 1");
        let level_id = level.id;
        let seed = Batch::new(
            actor,
            1,
            BTreeMap::new(),
            vec![Operation::create(Entity::Level(level))],
        );

        let mut s = Session::with_actor(Uuid::now_v7());
        s.open_with(vec![seed]).expect("open");
        assert_eq!(s.document().levels.len(), 1);
        assert!(s.projection().is_live(level_id));
        assert!(!s.is_dirty(), "R8.4.2: clean relative to the opened source");
        assert_eq!(s.undo_depth(), 0);
        assert_eq!(s.redo_depth(), 0);
    }

    #[test]
    fn the_first_edit_after_an_open_marks_the_document_dirty() {
        let actor = Uuid::now_v7();
        let level = Level::new("Level 1");
        let level_id = level.id;
        let mut s = Session::with_actor(Uuid::now_v7());
        s.open_with(vec![Batch::new(
            actor,
            1,
            BTreeMap::new(),
            vec![Operation::create(Entity::Level(level))],
        )])
        .expect("open");
        assert!(!s.is_dirty());
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level_id)))])
            .expect("wall");
        assert!(s.is_dirty());
    }

    // ---- R8.4.4: finite capacity on both stacks, independently ----

    #[test]
    fn exceeding_undo_capacity_drops_the_oldest_entry_and_blocks_nothing() {
        let mut s = Session::new().with_capacities(3, 10);
        let level = Level::new("Level 1");
        let level_id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("level");
        for i in 0..5 {
            s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
                level_id,
                i as f64 * 500.0,
            )))])
            .expect("wall");
        }
        assert_eq!(s.undo_depth(), 3, "capacity holds");
        assert_eq!(s.document().walls.len(), 5, "nothing was blocked");
        assert!(
            s.diagnostics()
                .iter()
                .any(|d| d.contains("undo stack full")),
            "a drop MAY surface a non-blocking diagnostic"
        );
    }

    #[test]
    fn redo_overflow_is_reachable_through_undos_alone() {
        // R8.4.4 says so explicitly (U11), and it is only true because the two
        // bounds are INDEPENDENT: a small redo bound overflows while the undo
        // stack is merely draining.
        let mut s = Session::new().with_capacities(10, 2);
        let level = Level::new("Level 1");
        let level_id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("level");
        for i in 0..4 {
            s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
                level_id,
                i as f64 * 500.0,
            )))])
            .expect("wall");
        }
        for _ in 0..4 {
            s.undo().expect("undo").expect("entry");
        }
        assert_eq!(s.redo_depth(), 2, "redo overflowed through undos alone");
        assert_eq!(s.undo_depth(), 1, "the undo stack is untouched by that");
    }

    #[test]
    fn exhausting_one_stack_leaves_the_other_unchanged() {
        let mut s = Session::new().with_capacities(2, 10);
        let level = Level::new("Level 1");
        let level_id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("level");
        for i in 0..4 {
            s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
                level_id,
                i as f64 * 500.0,
            )))])
            .expect("wall");
        }
        assert_eq!(s.undo_depth(), 2);
        assert_eq!(s.redo_depth(), 0);
        s.undo().expect("undo").expect("entry");
        assert_eq!(s.undo_depth(), 1);
        assert_eq!(s.redo_depth(), 1);
    }

    // ---- property-style coverage over random command streams (R8 criteria) ----

    #[test]
    fn undo_then_redo_returns_the_document_to_where_it_was() {
        // A deterministic pseudo-random command stream: create, edit, delete,
        // interleaved. After N undos and N redos the projection must equal the
        // one before the undos.
        // The level is BASELINE, not a committed batch: these streams undo
        // their whole history, and R2.2 forbids undoing away the last level.
        let (mut s, level_id) = on_a_baseline_level();

        let mut walls: Vec<Uuid> = Vec::new();
        let mut seed = 0x2545_F491_4F6C_DD1Du64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        for step in 0..40 {
            let choice = next() % 3;
            let committed = if choice == 0 || walls.is_empty() {
                let w = wall_at_y(level_id, step as f64 * 500.0);
                let id = w.id;
                s.commit(vec![Operation::create(Entity::Wall(w))])
                    .map(|_| walls.push(id))
                    .is_ok()
            } else if choice == 1 {
                let id = walls[(next() as usize) % walls.len()];
                s.commit(vec![set(id, "thicknessMm", json!(120.0))]).is_ok()
            } else {
                let i = (next() as usize) % walls.len();
                let id = walls[i];
                let ok = s.commit(vec![Operation::delete(id)]).is_ok();
                if ok {
                    walls.remove(i);
                }
                ok
            };
            assert!(committed, "every command in this stream is valid");
        }

        let before = s.document().clone();
        let depth = s.undo_depth();
        for _ in 0..depth {
            s.undo().expect("undo").expect("entry");
        }
        assert!(s.document().walls.is_empty(), "everything undone");
        for _ in 0..depth {
            s.redo().expect("redo").expect("entry");
        }
        assert_eq!(
            s.document(),
            &before,
            "undo-all then redo-all must restore the projection exactly"
        );
    }

    #[test]
    fn the_incremental_commit_path_agrees_with_a_full_replay_at_every_step() {
        // THE safety net for issue 0038. `commit` no longer replays the log to
        // learn what it produced; it extends the projection it already has,
        // which is valid only because a locally minted batch sorts strictly
        // last. This asserts the consequence directly, after EVERY command in
        // a mixed stream: the session's projection must equal what a full
        // replay of its own log produces.
        //
        // Without this, the optimisation is an argument. With it, any case
        // where the argument fails is a failing test.
        // The level is BASELINE, not a committed batch: these streams undo
        // their whole history, and R2.2 forbids undoing away the last level.
        let (mut s, level_id) = on_a_baseline_level();

        let mut walls: Vec<Uuid> = Vec::new();
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };

        let replay_of =
            |s: &Session| crate::replay::project_from_with_tombstones(s.baseline(), &[], s.log());

        let mut deleted: Vec<Uuid> = Vec::new();
        let mut rejections = 0usize;
        let mut multi = 0usize;
        for step in 0..120 {
            match next() % 6 {
                0 | 1 if !walls.is_empty() => {
                    let id = walls[(next() as usize) % walls.len()];
                    let _ = s.commit(vec![set(id, "thicknessMm", json!(120.0 + step as f64))]);
                }
                2 if !walls.is_empty() => {
                    let i = (next() as usize) % walls.len();
                    let id = walls.remove(i);
                    if s.commit(vec![Operation::delete(id)]).is_ok() {
                        deleted.push(id);
                    }
                }
                3 if s.undo_depth() > 0 => {
                    s.undo().expect("undo");
                }
                4 if s.redo_depth() > 0 => {
                    s.redo().expect("redo");
                }
                // A command that MUST be rejected: resurrecting a tombstone.
                // A rejected command must leave both the projection and the
                // log untouched, which this invariant also covers.
                5 if !deleted.is_empty() => {
                    let mut w = wall_at_y(level_id, step as f64 * 500.0);
                    let id = deleted[(next() as usize) % deleted.len()];
                    w.id = id;
                    let result = s.commit(vec![Operation::create(Entity::Wall(w))]);
                    // An undo earlier in the stream may have REOPENED this id,
                    // so the projection decides, not the bookkeeping: a create
                    // is refused exactly while the id is tombstoned.
                    if s.projection().is_tombstoned(id) {
                        assert!(result.is_err(), "resurrection must be refused");
                        rejections += 1;
                    }
                }
                _ => {
                    let w = wall_at_y(level_id, step as f64 * 500.0);
                    let id = w.id;
                    // Sometimes a MULTI-operation batch. One operation per
                    // batch never exercises a rollback that has to undo more
                    // than one edit, which is the shape B1 is about.
                    let ops = if next() % 3 == 0 {
                        multi += 1;
                        vec![
                            Operation::create(Entity::Wall(w)),
                            set(id, "thicknessMm", json!(180.0)),
                        ]
                    } else {
                        vec![Operation::create(Entity::Wall(w))]
                    };
                    if s.commit(ops).is_ok() {
                        walls.push(id);
                    }
                }
            }

            let replayed = replay_of(&s);
            assert_eq!(
                s.document(),
                &replayed.document,
                "step {step}: the incremental projection diverged from a replay"
            );
            assert_eq!(
                s.projection().discarded.len(),
                replayed.discarded.len(),
                "step {step}: the discard record diverged"
            );
            // Intervals, not just the document: a rejected command rolls back
            // tombstone provenance too, and a projection whose intervals drift
            // misclassifies the NEXT operation under I5 while still looking
            // correct from the outside.
            assert_eq!(
                s.projection().intervals,
                replayed.intervals,
                "step {step}: the live intervals diverged"
            );
        }
        // The stream has to actually exercise the interesting states, or the
        // invariant above is checked against nothing.
        assert!(!walls.is_empty() && !deleted.is_empty());
        assert!(rejections > 0, "no command was ever rejected");
        assert!(multi > 0, "no multi-operation batch was ever committed");
    }

    #[test]
    fn a_local_commit_that_promotes_a_held_batch_replays_instead_of_appending() {
        // The one case the fast path's argument does NOT cover, constructed so
        // that taking the fast path is actually WRONG rather than merely
        // unjustified.
        //
        // The held batch's prerequisite is this session's own NEXT counter, so
        // the local commit satisfies it and promotes it. The promoted batch
        // observed less than the local one did, so it sorts BEFORE it — a
        // position the local batch's projection never accounted for, and one
        // an append cannot discover.
        let (mut s, level) = with_level();
        let actor = s.actor();
        let other = Uuid::now_v7();

        let mut awaits_our_next = std::collections::BTreeMap::new();
        awaits_our_next.insert(actor, 2);
        let held = Batch::new(
            other,
            1,
            awaits_our_next,
            vec![Operation::create(Entity::Wall(wall_on(level)))],
        );
        let held_wall = match &held.operations[0].kind {
            OperationKind::CreateEntity { entity } => entity.id(),
            _ => unreachable!(),
        };
        assert_eq!(
            s.accept_external(held).expect("delivered"),
            Delivery::Held,
            "the fixture must HOLD, not accept"
        );
        assert_eq!(s.log().held().len(), 1);
        assert!(s.document().walls.is_empty(), "held means not projected");

        // This commit is counter 2, which is exactly what the held batch waits
        // for.
        s.commit(vec![Operation::create(Entity::Wall(wall_at_y(
            level, 500.0,
        )))])
        .expect("commit");

        assert!(s.log().held().is_empty(), "the held batch was promoted");
        assert_eq!(
            s.document().walls.len(),
            2,
            "both the promoted batch's wall and the local one must be present"
        );
        assert!(
            s.document().walls.iter().any(|w| w.id == held_wall),
            "an append would have missed the promoted batch entirely"
        );

        let replayed = crate::replay::project_from_with_tombstones(s.baseline(), &[], s.log());
        assert_eq!(s.document(), &replayed.document);
    }

    #[test]
    fn a_failed_command_in_a_stream_leaves_undo_and_redo_consistent() {
        let (mut s, level) = with_level();
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level)))])
            .expect("wall");
        s.undo().expect("undo").expect("entry");
        let (u, r) = (s.undo_depth(), s.redo_depth());

        let bad = Wall::new(
            level,
            Point::new(1.0, 1.0),
            Point::new(1.0, 1.0),
            200.0,
            2500.0,
        );
        s.commit(vec![Operation::create(Entity::Wall(bad))])
            .expect_err("rejected");
        assert_eq!((s.undo_depth(), s.redo_depth()), (u, r), "R8.2.2");

        // Redo must still work after the rejection.
        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().walls.len(), 1);
    }

    #[test]
    fn no_public_api_mutates_the_projection_directly() {
        // Issue 0011 acceptance criterion. `document()` and `projection()`
        // hand out shared references; the only way to change state is to
        // commit, undo, redo, or accept a batch. This test is a compile-time
        // assertion written as a runtime one: if either accessor ever returned
        // `&mut`, this would not build.
        fn takes_shared(_: &Document) {}
        let (s, _) = with_level();
        takes_shared(s.document());
        let _: &Projection = s.projection();
    }

    // ---- regressions from adversarial review (Codex, gpt-5.6-sol, 2026-08-31)

    #[test]
    fn an_ordinary_command_may_not_resurrect_a_tombstone() {
        // Finding 4. Replay permits a same-actor create after a same-actor
        // tombstone, because it cannot tell a redo from any other create. The
        // enforcement is E6's and belongs at the authoring replica — and the
        // source claimed `Session` did it while `commit` accepted anything.
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall.clone()))])
            .expect("create");
        s.commit(vec![Operation::delete(wall_id)]).expect("delete");
        assert!(s.projection().is_tombstoned(wall_id));

        let mut again = wall_on(level);
        again.id = wall_id;
        let err = s
            .commit(vec![Operation::create(Entity::Wall(again))])
            .expect_err("must refuse");
        assert!(matches!(err, CommandError::Resurrection(id) if id == wall_id));
        assert!(s.document().walls.is_empty(), "nothing may be resurrected");
    }

    #[test]
    fn redo_is_the_one_path_that_may_reopen_a_tombstone() {
        // The other half of finding 4: refusing resurrection must not break
        // R8.1.3, which REQUIRES redo to restore the same entity id.
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("create");
        s.undo().expect("undo").expect("entry");
        assert!(s.projection().is_tombstoned(wall_id));

        s.redo().expect("redo").expect("entry");
        assert_eq!(s.document().walls[0].id, wall_id);
    }

    #[test]
    fn a_set_fields_with_a_wrong_typed_value_is_rejected_not_silently_ignored() {
        // Finding 5. `merge_fields` returned None and the operation did
        // nothing, so the batch was accepted, the document marked dirty and an
        // undo entry pushed — for a command that changed nothing.
        let (mut s, level) = with_level();
        let batches = s.log().batches().len();
        let depth = s.undo_depth();

        let err = s
            .commit(vec![set(level, "name", json!(7))])
            .expect_err("must reject");
        assert!(matches!(err, CommandError::NotApplied(_)), "got {err:?}");
        assert_eq!(s.log().batches().len(), batches, "no batch in the log");
        assert_eq!(s.undo_depth(), depth, "no undo entry");
        assert_eq!(s.document().levels[0].name, "Level 1");
    }

    #[test]
    fn a_rejected_command_never_reaches_durable_history() {
        // Finding 7. R8.2.2 and §4.3 row 1 require rejection BEFORE any
        // effect, and a batch existing in the log IS an effect. The earlier
        // shape delivered the real batch and only then noticed replay had
        // discarded it.
        let (mut s, level) = with_level();
        let batches = s.log().batches().len();
        let depth = s.undo_depth();

        // A command whose INVERSE builds fine — the level is present, so
        // `invert_against` succeeds — but whose batch replay discards. An
        // edit to a MISSING entity would fail at inversion first and prove
        // nothing about the discard guard.
        let err = s
            .commit(vec![set(level, "name", json!(7))])
            .expect_err("must reject");
        assert!(matches!(err, CommandError::NotApplied(_)), "got {err:?}");
        assert_eq!(s.log().batches().len(), batches, "history must not grow");
        assert_eq!(s.undo_depth(), depth);
        assert!(
            s.projection().discarded.is_empty(),
            "no discard record either"
        );
    }

    #[test]
    fn an_uninvertible_command_is_refused_before_anything_changes() {
        // The other half: an edit to an entity that is not present cannot be
        // inverted, and `commit` refuses at that point — before a batch is
        // even built.
        let (mut s, _) = with_level();
        let batches = s.log().batches().len();
        let err = s
            .commit(vec![set(Uuid::now_v7(), "name", json!("x"))])
            .expect_err("must reject");
        assert!(matches!(err, CommandError::NotApplied(_)), "got {err:?}");
        assert_eq!(s.log().batches().len(), batches);
    }

    #[test]
    fn undo_of_a_create_that_replaced_an_entity_restores_the_old_one() {
        // Finding 6. An LWW4 collision REPLACES rather than adds, so its
        // inverse is to put the old entity back. Deleting the id leaves it
        // absent AND tombstoned, which undo has no business doing.
        //
        // Built through `accept_external` because `commit` now refuses a
        // same-id create, and a colliding create can only arrive from another
        // actor in the first place.
        let (mut s, level) = with_level();
        let wall = wall_on(level);
        let wall_id = wall.id;
        s.commit(vec![Operation::create(Entity::Wall(wall))])
            .expect("create");

        let mut replacement = wall_on(level);
        replacement.id = wall_id;
        replacement.thickness_mm = 400.0;
        let inverse = invert_against(
            s.document(),
            &[Operation::create(Entity::Wall(replacement))],
        )
        .expect("inverts");
        assert_eq!(inverse.len(), 1);
        assert!(
            matches!(inverse[0].kind, OperationKind::CreateEntity { .. }),
            "the inverse must RESTORE, not delete: {:?}",
            inverse[0].kind
        );
    }

    #[test]
    fn a_batch_creating_one_id_twice_inverts_to_a_single_delete() {
        // Finding 6's single-batch half. Two deletes would make the second
        // stale against the first, discarding the whole compensating batch —
        // and the discarded batch stayed in the durable log.
        let (s, level) = with_level();
        let wall = wall_on(level);
        let id = wall.id;
        let mut second = wall_on(level);
        second.id = id;

        let inverse = invert_against(
            s.document(),
            &[
                Operation::create(Entity::Wall(wall)),
                Operation::create(Entity::Wall(second)),
            ],
        )
        .expect("inverts");
        assert_eq!(inverse.len(), 1, "got {inverse:?}");
        assert!(matches!(inverse[0].kind, OperationKind::DeleteEntity));
    }

    #[test]
    fn an_external_batch_promoted_by_a_local_commit_still_fires_the_fence() {
        // Finding 8. A held external batch becomes applicable when a local
        // commit supplies its prerequisite. That is an ACCEPTED external batch
        // with all of R8.3.1's consequences, and `deliver` reported only the
        // verdict for the batch it was handed.
        let mut s = Session::with_actor(Uuid::from_u128(1));
        let level = Level::new("Level 1");
        let level_id = level.id;
        s.commit(vec![Operation::create(Entity::Level(level))])
            .expect("c1");
        assert_eq!(s.undo_depth(), 1);

        // An external batch that observes a counter this session has not
        // reached yet, so it is held.
        let other = Uuid::from_u128(2);
        let mut frontier = std::collections::BTreeMap::new();
        frontier.insert(s.actor(), 2);
        let held = Batch::new(
            other,
            1,
            frontier,
            vec![Operation::create(Entity::Level(Level::new("Loft")))],
        );
        assert_eq!(
            s.accept_external(held).expect("held"),
            Delivery::Held,
            "prerequisites are missing"
        );
        assert_eq!(s.undo_depth(), 1, "a held batch fires no fence");

        // The next local commit supplies counter 2 and promotes it.
        s.commit(vec![Operation::create(Entity::Wall(wall_on(level_id)))])
            .expect("c2");
        assert_eq!(s.document().levels.len(), 2, "the external batch applied");
        assert_eq!(s.undo_depth(), 0, "R8.3.1: both stacks cleared");
        assert_eq!(s.redo_depth(), 0);
    }
}
