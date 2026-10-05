package document

import (
	"encoding/json"
	"fmt"
)

// Session-local undo/redo machinery (BEHAVIOR-SPEC §8): LIFO stacks of
// entries referencing authoritative local batches, executed as compensating
// batches under fresh identities (R8.1.2), restoring equivalent projections
// — never identical historical metadata (§8 preamble).

// inverseOp is one net-effect inversion instruction computed at Execute
// time against the captured pre-state. Payload holds the complete prior
// entity object for OpCreateEntity and a field→value object for
// OpSetFields (absent-in-pre-image optional names carry JSON null).
type inverseOp struct {
	kind    OpKind
	entity  UUID
	payload json.RawMessage
}

// undoEntry references one accepted local command batch together with its
// precomputed inversion plan. The same entry type serves both stacks: it
// moves between them on executed undos/redos and is itself never re-pushed
// by the compensating deliveries (R8.2.1 exemption (a)).
type undoEntry struct {
	batch *Batch
	plan  []inverseOp
}

// SessionNotice records one non-blocking stack event: capacity drops
// (R8.4.4, optional) and retention-driven truncations (R8.4.3, mandatory).
// Notices are never operations and never block correctness.
type SessionNotice struct {
	Code   string
	Detail string
}

const (
	noticeUndoDrop       = "undo-capacity-drop"
	noticeRedoDrop       = "redo-capacity-drop"
	noticeUndoTruncate   = "undo-truncation"
	noticeRedoTruncate   = "redo-truncation"
	noticeCompactionName = "undo entry"
)

// inversePlan derives the deterministic net-effect inversion of the
// staged command: one inverse operation per touched entity, emitted as
// delete-of-created, create-of-deleted, then revert-of-modified, each
// class in ascending entityId order. Net-effect inversion realizes
// R3.1.5's reverse-dependency restore as a projection-equivalent result;
// atomicity makes per-entity net classification well defined regardless
// of intra-batch operation order.
func (st *stagedCommand) inversePlan() []inverseOp {
	plan := make([]inverseOp, 0, len(st.createdNew)+len(st.deletedIDs)+len(st.wroteFields))
	created := make([]UUID, 0, len(st.createdNew))
	for id := range st.createdNew {
		created = append(created, id)
	}
	SortUUIDs(created)
	for _, id := range created {
		plan = append(plan, inverseOp{kind: OpDeleteEntity, entity: id})
	}
	deleted := make([]UUID, 0, len(st.deletedIDs))
	for _, id := range st.deletedIDs {
		if _, ok := st.preImages[id]; ok { // created-and-deleted ids are net no-ops
			deleted = append(deleted, id)
		}
	}
	SortUUIDs(deleted)
	for _, id := range deleted {
		plan = append(plan, inverseOp{kind: OpCreateEntity, entity: id, payload: st.preImages[id]})
	}
	modified := make([]UUID, 0, len(st.wroteFields))
	for id := range st.wroteFields {
		if _, gone := st.preImages[id]; gone {
			modified = append(modified, id)
		}
	}
	SortUUIDs(modified)
	for _, id := range modified {
		var prior map[string]json.RawMessage
		if err := json.Unmarshal(st.preImages[id], &prior); err != nil {
			panic(fmt.Sprintf("document: pre-image of %s is not an object: %v", id.String(), err))
		}
		fields := make(map[string]json.RawMessage, len(st.wroteFields[id]))
		for name := range st.wroteFields[id] {
			if v, ok := prior[name]; ok {
				fields[name] = v // exact prior bytes
			} else {
				fields[name] = json.RawMessage("null") // absent optional restored to absent
			}
		}
		raw, err := json.Marshal(fields)
		if err != nil {
			panic(fmt.Sprintf("document: inversion payload marshal failed: %v", err))
		}
		plan = append(plan, inverseOp{kind: OpSetFields, entity: id, payload: raw})
	}
	return plan
}

// plannedOp is one operation of a compensating or restoring batch before
// identity minting: kind, target entity, and payload (complete entity
// object for creates; field object for setFields).
type plannedOp struct {
	kind    OpKind
	entity  UUID
	payload json.RawMessage
}

// vetRestorable computes the authorized-restoration map for a batch whose
// operations will occupy counter slots [base, base+len(planned)). A create
// may proceed over a tombstone in exactly two cases (R8.1.3):
//
//   - the tombstone is written by an EARLIER OPERATION OF THE SAME BATCH
//     (a delete→recreate pair inside one atomic compensation/restoration —
//     the tombstone is this device's own effect by construction); or
//   - the tombstone pre-exists and carries THIS device's provenance
//     (OriginUndoChain for executed redos; any own origin for undo
//     compensations). Sync-origin tombstones never pass.
func (s *Session) vetRestorable(planned []plannedOp, base int64, redo bool) (map[UUID]position, error) {
	restorable := map[UUID]position{}
	pending := map[UUID]position{}
	reject := func(format string, args ...any) error {
		return &CommandRejectedError{Code: codeInvalidUndoTarget,
			Detail: fmt.Sprintf(format, args...)}
	}
	for i, po := range planned {
		slot := position{Ref: batchRef{Base: base, Actor: s.actor}, OpIndex: int64(i)}
		switch po.kind {
		case OpDeleteEntity:
			pending[po.entity] = slot
		case OpCreateEntity:
			if pk, ok := pending[po.entity]; ok {
				restorable[po.entity] = pk // this batch's own delete→create pair
				delete(pending, po.entity)
				continue
			}
			key, gone := s.proj.tombstones[po.entity]
			if !gone {
				continue // plain create onto unoccupied space
			}
			origin := s.provenance[key]
			if redo && origin != OriginUndoChain {
				return nil, reject("redo cannot restore %s: tombstone is not from this device's undo chain",
					po.entity.String())
			}
			if !redo && origin == 0 {
				return nil, reject("cannot recreate %s: tombstone is external (sync origin)",
					po.entity.String())
			}
			restorable[po.entity] = key
		}
	}
	return restorable, nil
}

// Undo executes one undo: pops the most recent entry, delivers ONE
// compensating batch with freshly minted identities and the session's next
// counter advance (R8.1.2), and moves the popped entry to the redo stack.
// The compensation pushes NO new entry (R8.2.1 exemption (a)) and does NOT
// clear redo (R8.4.1). An empty stack is a no-op (nil, nil).
//
// Restoration authorization: recreation of a deleted id requires its live
// tombstone to carry THIS device's provenance (OriginLocalCommand or
// OriginUndoChain). External/sync tombstones refuse fail-closed with
// stacks unchanged.
func (s *Session) Undo() (*Batch, error) {
	if len(s.undoStack) == 0 {
		return nil, nil
	}
	e := s.undoStack[len(s.undoStack)-1]
	planned := make([]plannedOp, 0, len(e.plan))
	for _, inv := range e.plan {
		planned = append(planned, plannedOp{kind: inv.kind, entity: inv.entity, payload: inv.payload})
	}
	b, err := s.deliverPlanned(planned, false)
	if err != nil {
		return nil, err // entry remains on the undo stack
	}
	for i, op := range b.Operations {
		if op.Kind == OpDeleteEntity {
			s.provenance[position{Ref: batchRef{Base: b.CounterBase, Actor: s.actor}, OpIndex: int64(i)}] = OriginUndoChain
		}
	}
	s.undoStack = s.undoStack[:len(s.undoStack)-1]
	s.pushRedo(e)
	s.dirty = true
	return b, nil
}

// Redo executes one redo: pops the redo stack and re-applies the ORIGINAL
// command's logical content (same kinds, SAME entity IDs, identical
// payloads) as a fresh batch under fresh op/batch identities (R8.1.2/U1),
// pushing the entry back onto the undo stack. Authorized restoration of
// tombstoned ids requires OriginUndoChain provenance exclusively
// (R8.1.3); local-command tombstones authorize undo, never redo. An empty
// stack is a no-op (nil, nil) (U6).
func (s *Session) Redo() (*Batch, error) {
	if len(s.redoStack) == 0 {
		return nil, nil
	}
	e := s.redoStack[len(s.redoStack)-1]
	planned := make([]plannedOp, 0, len(e.batch.Operations))
	for _, orig := range e.batch.Operations {
		planned = append(planned, plannedOp{kind: orig.Kind, entity: orig.EntityID, payload: orig.Payload})
	}
	b, err := s.deliverPlanned(planned, true)
	if err != nil {
		return nil, err // entry remains on the redo stack
	}
	for i, op := range b.Operations {
		if op.Kind == OpDeleteEntity {
			s.provenance[position{Ref: batchRef{Base: b.CounterBase, Actor: s.actor}, OpIndex: int64(i)}] = OriginLocalCommand
		}
	}
	s.redoStack = s.redoStack[:len(s.redoStack)-1]
	s.pushUndo(e)
	s.dirty = true
	return b, nil
}

// deliverPlanned materializes planned operations as ONE fresh batch
// (freshly minted operation and batch identities, R8.1.2) and delivers it
// through the authorized-restoration seam. The counter reservation commits
// only after successful delivery so a refusal never tears B3 contiguity.
func (s *Session) deliverPlanned(planned []plannedOp, redo bool) (*Batch, error) {
	base := s.nextCounter
	restorable, err := s.vetRestorable(planned, base, redo)
	if err != nil {
		return nil, err
	}
	ops := make([]*Operation, 0, len(planned))
	for _, po := range planned {
		var op *Operation
		var err error
		switch po.kind {
		case OpDeleteEntity:
			op, err = NewDeleteOp(mustMint(), po.entity)
		case OpCreateEntity:
			op, err = NewCreateOp(mustMint(), po.payload)
		case OpSetFields:
			var fields map[string]json.RawMessage
			if err := json.Unmarshal(po.payload, &fields); err != nil {
				return nil, fmt.Errorf("document: planned payload corrupt: %w", err)
			}
			op, err = NewSetFieldsOp(mustMint(), po.entity, fields)
		}
		if err != nil {
			return nil, err
		}
		ops = append(ops, op)
	}
	frontier := make(map[UUID]int64, len(s.proj.frontier)+1)
	for a, n := range s.proj.frontier {
		frontier[a] = n
	}
	if base > 1 {
		frontier[s.actor] = base - 1
	}
	b, err := NewBatch(mustMint(), s.actor, base, frontier, ops...)
	if err != nil {
		return nil, err
	}
	if err := s.proj.deliverRestoring(b, restorable); err != nil {
		return nil, err
	}
	s.commitCounter(len(ops))
	return b, nil
}

func (s *Session) pushUndo(e undoEntry) {
	s.undoStack = append(s.undoStack, e)
	if len(s.undoStack) > s.undoCap {
		dropped := s.undoStack[0]
		s.undoStack = s.undoStack[1:]
		s.notices = append(s.notices, SessionNotice{Code: noticeUndoDrop,
			Detail: fmt.Sprintf("undo capacity exhausted; oldest entry (%s) dropped",
				dropped.batch.BatchID.String())})
	}
}

func (s *Session) pushRedo(e undoEntry) {
	s.redoStack = append(s.redoStack, e)
	if len(s.redoStack) > s.redoCap {
		dropped := s.redoStack[0]
		s.redoStack = s.redoStack[1:]
		s.notices = append(s.notices, SessionNotice{Code: noticeRedoDrop,
			Detail: fmt.Sprintf("redo capacity exhausted; oldest entry (%s) dropped",
				dropped.batch.BatchID.String())})
	}
}

// Notices returns a copy of all non-blocking stack events recorded so far.
func (s *Session) Notices() []SessionNotice {
	out := make([]SessionNotice, len(s.notices))
	copy(out, s.notices)
	return out
}

// TruncateToRetained implements the R8.4.3 seam: given the snapshot
// coverage frontier of a completed compaction, every stack entry whose
// source batch intersects the covered region can no longer resolve against
// retained metadata and truncates off its stack. Affected stacks truncate
// independently; a NON-BLOCKING diagnostic names each truncation (U10).
// This computes over stack entries only — it performs no compaction
// itself (0027 owns schedules and writers).
func (s *Session) TruncateToRetained(frontier map[UUID]int64) []SessionNotice {
	var out []SessionNotice
	truncate := func(stack *[]undoEntry, code, label string) {
		kept := (*stack)[:0]
		dropped := 0
		var boundary int64
		for _, e := range *stack {
			w := frontier[e.batch.ActorID]
			if w >= e.batch.CounterBase { // any coverage defeats resolution
				dropped++
				if w > boundary {
					boundary = w
				}
				continue
			}
			kept = append(kept, e)
		}
		if dropped > 0 {
			out = append(out, SessionNotice{Code: code,
				Detail: fmt.Sprintf("snapshot frontier covers through counter %d; %d %s entries truncated",
					boundary, dropped, label)})
		}
		*stack = kept
	}
	truncate(&s.undoStack, noticeUndoTruncate, noticeCompactionName)
	truncate(&s.redoStack, noticeRedoTruncate, "redo entry")
	return out
}
