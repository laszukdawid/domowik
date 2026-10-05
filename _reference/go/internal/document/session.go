package document

import (
	crand "crypto/rand"
	"encoding/json"
	"fmt"
)

// TombOrigin records WHOSE operation minted a tombstone deletingKey this
// device created. It is the explicit provenance required to separate the
// two authorized-restoration classes of BEHAVIOR-SPEC §8 from sync
// tombstones, which must never authorize anything (R8.1.3):
//
//	OriginLocalCommand — delete ops inside accepted local commands; these
//	                     authorize their own UNDO's recreation.
//	OriginUndoChain    — delete ops executed by this session's undo/redo
//	                     compensations; these additionally authorize REDO
//	                     restoration (R8.1.3 "this device's own undo").
//
// Keys absent from the map are external/sync-origin and are never
// restorable.
type TombOrigin uint8

const (
	OriginLocalCommand TombOrigin = iota + 1
	OriginUndoChain
)

// CommandRejectedError is the user-input validation class at the command
// seam (FORMAT-SPEC §4.3 row 1 / O4 authoring replica): the command
// rejects BEFORE any effect — projection state unchanged, no batch
// delivered, no undo entry, both stacks untouched (BEHAVIOR-SPEC R8.2.2 /
// §8.5 matrix rows).
type CommandRejectedError struct {
	Code   string
	Detail string
}

func (e *CommandRejectedError) Error() string {
	return fmt.Sprintf("%s: %s", e.Code, e.Detail)
}

// Diagnostic codes for command-time rejections. `entity-id-collision`
// follows the BEHAVIOR-SPEC vocabulary; the level-deletion gates implement
// BEHAVIOR-SPEC §2.2–§2.3 normative text ("at least one level",
// containment, surviving references).
const (
	codeUnknownTarget     = "unknown-target"
	codeEmptyCommand      = "empty-command"
	codeLastLevel         = "last-level"
	codeLevelNotEmpty     = "level-not-empty"
	codeLevelReferenced   = "level-referenced"
	codeInvalidUndoTarget = "undo-target-unrestorable"
)

// Session drives LOCAL command execution for one open document projection
// on one device (BEHAVIOR-SPEC §8): it mints batches with fresh identities
// under this actor's monotonic counter stream (B2/B3), validates
// authoring-side invariants at command time, delivers everything through
// the unchanged Projection apply seam, and maintains the session-local
// undo/redo stacks. Stacks are never persisted (R8.4.2); a fresh Session
// over a replayed or snapshotted projection IS the reopened document, with
// empty stacks by construction.
//
// A Session is single-goroutine state, matching its device-local ownership.
type Session struct {
	proj        *Projection
	actor       UUID // this device's actor epoch (B2)
	nextCounter int64

	undoStack []undoEntry
	redoStack []undoEntry
	undoCap   int
	redoCap   int

	dirty bool // R8.3.3 seam; pure flag, save IO belongs to §9.1's owner

	provenance map[position]TombOrigin
	notices    []SessionNotice
}

// NewSession opens a command session over proj for the given actor epoch.
// The counter resumes after the projection's watermark for this actor so
// reopen-then-edit histories chain contiguously (B3).
func NewSession(proj *Projection, actor UUID) (*Session, error) {
	if proj == nil {
		return nil, fmt.Errorf("document: session requires a projection")
	}
	if actor == (UUID{}) {
		return nil, ErrInvalidUUID
	}
	return &Session{
		proj:        proj,
		actor:       actor,
		nextCounter: proj.frontier[actor] + 1,
		undoCap:     UndoStackCapacityDefault,
		redoCap:     RedoStackCapacityDefault,
		provenance:  map[position]TombOrigin{},
	}, nil
}

// SetStackCapacity overrides the device policy bounds of R8.4.4 (both
// bounds independent, both finite). Values ≤ 0 reject.
func (s *Session) SetStackCapacity(undo, redo int) error {
	if undo <= 0 || redo <= 0 {
		return fmt.Errorf("document: stack capacities must be positive")
	}
	s.undoCap, s.redoCap = undo, redo
	return nil
}

// mintBatch constructs the batch for ops at the session's current counter
// position WITHOUT committing the reservation; callers advance the counter
// only after successful delivery, so a rejected command never tears the
// actor stream (B3 forbids gaps).
func (s *Session) mintBatch(ops []*Operation) (*Batch, int64, error) {
	base := s.nextCounter
	frontier := make(map[UUID]int64, len(s.proj.frontier)+1)
	for a, n := range s.proj.frontier {
		frontier[a] = n
	}
	if base > 1 {
		frontier[s.actor] = base - 1 // honest observation of our own past (F1)
	}
	b, err := NewBatch(mustMint(), s.actor, base, frontier, ops...)
	if err != nil {
		return nil, 0, err
	}
	return b, base, nil
}

func (s *Session) commitCounter(n int) { s.nextCounter += int64(n) }

// Execute delivers ONE accepted local command as ONE atomic batch
// (R8.2.1). Command-time validation (§5 of the design record) rejects
// user-fixable problems before any effect; a successful delivery pushes
// exactly one undo entry (finite capacity, drop-oldest), clears the redo
// stack (R8.4.1), and marks the document dirty.
func (s *Session) Execute(ops ...*Operation) (*Batch, error) {
	if len(ops) == 0 {
		return nil, &CommandRejectedError{Code: codeEmptyCommand,
			Detail: "confirmed-but-unchanged edits emit nothing and never reach the command seam"}
	}
	st, err := s.stageAuthoring(ops)
	if err != nil {
		return nil, err
	}
	b, base, err := s.mintBatch(ops)
	if err != nil {
		return nil, err
	}
	if err := s.proj.Deliver(b); err != nil {
		return nil, err // rejected before any effect; stacks/dirty untouched
	}
	s.commitCounter(len(ops))
	for i, op := range ops {
		if op.Kind == OpDeleteEntity {
			s.provenance[position{Ref: batchRef{Base: base, Actor: s.actor}, OpIndex: int64(i)}] = OriginLocalCommand
		}
	}
	s.pushUndo(undoEntry{batch: b, plan: st.inversePlan()})
	s.redoStack = s.redoStack[:0] // accepted local command outside redo execution (R8.4.1)
	s.dirty = true
	return b, nil
}

// DeliverExternal is the fence entry point for accepted sync-delivered,
// recovery-restored, or CLI-applied batches (R8.3.1). Acceptance of ANY
// batch in the call clears BOTH local stacks and dirties the document
// (R8.3.3); external batches never enter the local stacks (R8.3.2). A full
// rejection leaves the stacks untouched; a partial acceptance (earlier
// batches committed, a later one rejected) fences too — the accepted part
// is external history the local stacks may not cross.
func (s *Session) DeliverExternal(batches ...*Batch) error {
	before := len(s.proj.batches)
	err := s.proj.Deliver(batches...)
	if len(s.proj.batches) > before {
		s.undoStack = nil
		s.redoStack = nil
		s.dirty = true
	}
	return err
}

// MarkSaved clears the dirty flag. It performs NO persistence itself: the
// §9.1 save-contract owner invokes it after its own successful write
// (0029+). Stacks are never touched by saves (matrix save rows).
func (s *Session) MarkSaved() { s.dirty = false }

// Dirty reports whether accepted deliveries (local or external) occurred
// since the last MarkSaved.
func (s *Session) Dirty() bool { return s.dirty }

// CanUndo / CanRedo / UndoDepth / RedoDepth observe the session-local
// stacks.
func (s *Session) CanUndo() bool  { return len(s.undoStack) > 0 }
func (s *Session) CanRedo() bool  { return len(s.redoStack) > 0 }
func (s *Session) UndoDepth() int { return len(s.undoStack) }
func (s *Session) RedoDepth() int { return len(s.redoStack) }

// stagedCommand is the sequential overlay simulation of one candidate
// command. It mirrors applyBatch's decision points (existence, tombstones,
// sequential visibility) so locally authored batches can never go stale at
// replay, and collects everything inverse-effect construction needs.
type stagedCommand struct {
	liveFinal   map[UUID]bool
	createdNew  map[UUID]Entity // created by THIS command (pre-batch absent)
	deletedIDs  []UUID          // pre-batch-live ids the command deletes
	wroteFields map[UUID]map[string]struct{}
	preImages   map[UUID]json.RawMessage
	preKinds    map[UUID]Kind // pre-batch live kinds
}

func (s *Session) stageAuthoring(ops []*Operation) (*stagedCommand, error) {
	st := &stagedCommand{
		liveFinal:   map[UUID]bool{},
		createdNew:  map[UUID]Entity{},
		wroteFields: map[UUID]map[string]struct{}{},
		preImages:   map[UUID]json.RawMessage{},
		preKinds:    map[UUID]Kind{},
	}
	for id, e := range s.proj.entities {
		st.liveFinal[id] = true
		st.preKinds[id] = e.Kind()
	}

	reject := func(code, format string, args ...any) error {
		return &CommandRejectedError{Code: code, Detail: fmt.Sprintf(format, args...)}
	}

	for _, op := range ops {
		if op == nil {
			return nil, reject(codeEmptyCommand, "command carries an absent operation")
		}
		switch op.Kind {
		case OpCreateEntity:
			ent, err := decodeEntity(op.Payload)
			if err != nil {
				var fe *FieldError
				if asFieldError(err, &fe) {
					return nil, &CommandRejectedError{Code: fe.Code, Detail: fe.Detail}
				}
				return nil, err
			}
			if ent.EntityID() != op.EntityID {
				return nil, reject(codeInvalidFieldSyntax,
					"operation entityId %s does not match payload id %s",
					op.EntityID.String(), ent.EntityID().String())
			}
			if _, gone := s.proj.tombstones[op.EntityID]; gone {
				return nil, reject(codeDuplicateElement,
					"id %s is tombstoned; resurrection is blocked (I3/E6)",
					op.EntityID.String())
			}
			if st.liveFinal[op.EntityID] {
				return nil, reject(codeDuplicateElement,
					"id %s already names a live entity of some kind", op.EntityID.String())
			}
			st.liveFinal[op.EntityID] = true
			st.createdNew[op.EntityID] = ent
		case OpSetFields:
			if _, isNew := st.createdNew[op.EntityID]; !isNew {
				// Target created earlier in THIS command needs no pre-image;
				// anything else must exist pre-command.
				if _, live := s.livePreimage(op.EntityID, st); !live {
					return nil, reject(codeUnknownTarget,
						"setFields targets absent or tombstoned entity %s", op.EntityID.String())
				}
			}
			var fields map[string]json.RawMessage
			if err := json.Unmarshal(op.Payload, &fields); err != nil || len(fields) == 0 {
				return nil, reject(codeInvalidFieldSyntax,
					"setFields payload must be an object mapping field names to values")
			}
			names := st.wroteFields[op.EntityID]
			if names == nil {
				names = map[string]struct{}{}
				st.wroteFields[op.EntityID] = names
			}
			for n := range fields {
				names[n] = struct{}{}
			}
		case OpDeleteEntity:
			if _, live := s.livePreimage(op.EntityID, st); !live {
				return nil, reject(codeUnknownTarget,
					"deleteEntity targets absent or tombstoned entity %s", op.EntityID.String())
			}
			delete(st.liveFinal, op.EntityID)
			st.deletedIDs = append(st.deletedIDs, op.EntityID)
			delete(st.createdNew, op.EntityID)
		default:
			return nil, reject(codeInvalidFieldSyntax, "unknown operation kind %q", string(op.Kind))
		}
	}
	if err := s.checkLevelGates(st); err != nil {
		return nil, err
	}
	return st, nil
}

// livePreimage reports whether eid is live in the staged overlay AND was
// live pre-command, capturing its complete pre-image once (compound
// same-entity sequences share one pre-image). Ids created by the command
// itself have no pre-image.
func (s *Session) livePreimage(eid UUID, st *stagedCommand) (Entity, bool) {
	if _, isNew := st.createdNew[eid]; isNew {
		return nil, false // created within this command: no pre-state
	}
	if !st.liveFinal[eid] {
		return nil, false // already deleted earlier in this command
	}
	e, ok := s.proj.entities[eid]
	if !ok {
		return nil, false // never existed, or tombstoned
	}
	if _, captured := st.preImages[eid]; !captured {
		raw, err := encodeEntity(e)
		if err != nil {
			panic(fmt.Sprintf("document: pre-image encoding failed for %s: %v", eid.String(), err))
		}
		st.preImages[eid] = raw
	}
	return e, true
}

// checkLevelGates enforces the BEHAVIOR-SPEC §2.2–§2.3 document invariants
// at command time (the invariant family issue 0010 explicitly deferred to
// this layer):
//
//	LAST-LEVEL: the post-command projection keeps at least one live level.
//	CONTAINS-ENTITIES: deleting a level hosting ANY live entity rejects,
//	    listing blocking kinds and counts — even when the same atomic
//	    command also deletes those entities (v0.1 never cascades).
//	SURVIVING-REFERENCE: no post-command-surviving entity may reference a
//	    level deleted by the command. In the v0.1 schema the only
//	    level-reference channel is levelId (openings reference walls;
//	    background images are fields ON the level and cannot outlive it),
//	    so this scan is exhaustive today and stays generic for future
//	    reference channels.
func (s *Session) checkLevelGates(st *stagedCommand) error {
	reject := func(code, format string, args ...any) error {
		return &CommandRejectedError{Code: code, Detail: fmt.Sprintf(format, args...)}
	}
	kindOf := func(id UUID) (Kind, bool) {
		if e, ok := st.createdNew[id]; ok {
			return e.Kind(), true
		}
		k, ok := st.preKinds[id]
		return k, ok // pre-batch kinds count even if deleted by this command
	}

	deletedLevels := make([]UUID, 0, 2)
	for _, id := range st.deletedIDs {
		if k, ok := kindOf(id); ok && k == KindLevel {
			deletedLevels = append(deletedLevels, id)
		}
	}

	// Lookup universe for effective-level resolution: pre-batch entities
	// for the containment gate; staged-final survivors for the reference
	// gate. An opening's effective level resolves through its host wall
	// (R4.0.2); a wallJoin's through its member walls. Cycles cannot occur
	// in the v0.1 schema (openings reference walls, joins reference walls,
	// walls reference levels), but the visited guard keeps malformed data
	// from recursing forever.
	effectiveRefs := func(e Entity, entOf func(UUID) (Entity, bool)) []UUID {
		var walk func(ent Entity, seen map[UUID]struct{}) []UUID
		walk = func(ent Entity, seen map[UUID]struct{}) []UUID {
			if _, loop := seen[ent.EntityID()]; loop {
				return nil
			}
			seen[ent.EntityID()] = struct{}{}
			switch t := ent.(type) {
			case *Wall:
				return []UUID{t.LevelID}
			case *Opening:
				if host, ok := entOf(t.HostWallID); ok {
					return walk(host, seen)
				}
				return nil
			case *WallJoin:
				var out []UUID
				for _, m := range t.Members {
					if w, ok := entOf(m.WallID); ok {
						out = append(out, walk(w, seen)...)
					}
				}
				return out
			case *RoomRecord:
				return []UUID{t.LevelID}
			case *Label:
				return []UUID{t.LevelID}
			case *Dimension:
				return []UUID{t.LevelID}
			case *AssetInstance:
				return []UUID{t.LevelID}
			default:
				return nil
			}
		}
		return walk(e, map[UUID]struct{}{})
	}

	levelsLeft := 0
	for id, live := range st.liveFinal {
		if !live {
			continue
		}
		if k, ok := kindOf(id); ok && k == KindLevel {
			levelsLeft++
		}
	}
	if levelsLeft == 0 {
		return reject(codeLastLevel, "a document always contains at least one level (§2.2)")
	}

	if len(deletedLevels) > 0 {
		// CONTAINS-ENTITIES over PRE-batch live entities, resolving
		// effective levels so openings (host wall) and wallJoins (member
		// walls) block their effective level too (§2.3 "any entity").
		blocked := map[Kind]int{}
		preOf := func(id UUID) (Entity, bool) {
			e, ok := s.proj.entities[id]
			return e, ok
		}
		for id, e := range s.proj.entities {
			for _, l := range deletedLevels {
				if id == l {
					continue
				}
				for _, ref := range effectiveRefs(e, preOf) {
					if ref == l {
						blocked[e.Kind()]++
						break
					}
				}
			}
		}
		if len(blocked) > 0 {
			detail := ""
			for _, k := range []Kind{KindWall, KindOpening, KindWallJoin, KindRoomRecord,
				KindLabel, KindDimension, KindAssetInstance} {
				if n := blocked[k]; n > 0 {
					detail += fmt.Sprintf(" %s×%d", string(k), n)
				}
			}
			return reject(codeLevelNotEmpty,
				"deleting a level that contains entities is rejected:%s", detail)
		}
		for _, l := range deletedLevels {
			// SURVIVING-REFERENCE: staged-final survivors must not occupy
			// the deleted level through any reference channel.
			survivingOf := func(id UUID) (Entity, bool) {
				if !st.liveFinal[id] {
					return nil, false
				}
				if e, ok := st.createdNew[id]; ok {
					return e, true
				}
				e, ok := s.proj.entities[id]
				return e, ok
			}
			for id := range st.liveFinal {
				if !st.liveFinal[id] || id == l {
					continue
				}
				e, ok := survivingOf(id)
				if !ok {
					continue
				}
				for _, ref := range effectiveRefs(e, survivingOf) {
					if ref == l {
						return reject(codeLevelReferenced,
							"surviving %s occupies deleted level %s", id.String(), l.String())
					}
				}
			}
		}
	}
	return nil
}

// asFieldError is errors.As specialized to *FieldError.
func asFieldError(err error, target **FieldError) bool {
	for err != nil {
		if fe, ok := err.(*FieldError); ok {
			*target = fe
			return true
		}
		u, ok := err.(interface{ Unwrap() error })
		if !ok {
			return false
		}
		err = u.Unwrap()
	}
	return false
}

// mustMint yields fresh RFC 9562 identities for command and compensation
// batches (R8.1.2: every executed undo/redo mints new batch AND operation
// identities; local commands carry caller-supplied operation ids). The
// indirection exists for deterministic tests, which install a counter-
// backed source; production uses crypto/rand.
var mintUUID = cryptoRandUUID

func cryptoRandUUID() UUID {
	var u UUID
	if err := crandRead(u[:]); err != nil {
		panic(fmt.Sprintf("document: identity entropy unavailable: %v", err))
	}
	u[6] = (u[6] & 0x0F) | 0x40 // RFC 9562 version nibble (fixed format bits)
	u[8] = (u[8] & 0x3F) | 0x80 // RFC 9562 variant bits
	return u
}

func crandRead(b []byte) error {
	_, err := crand.Read(b)
	return err
}

// mustMint resolves the swappable identity source for batch construction.
func mustMint() UUID { return mintUUID() }
