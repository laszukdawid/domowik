package document

import (
	"encoding/json"
	"errors"
	"fmt"
	"reflect"
	"strings"
	"testing"
)

// installDeterministicMint replaces the identity source with a counter-
// backed one so every outcome is reproducible across runs (review
// condition: tests run twice with identical results).
func installDeterministicMint(t *testing.T) {
	t.Helper()
	prev := mintUUID
	var n uint64
	mintUUID = func() UUID {
		n++
		return numUUID(0xBEEF000000000000 | n)
	}
	t.Cleanup(func() { mintUUID = prev })
}

func mustSession(t *testing.T, p *Projection) *Session {
	t.Helper()
	s, err := NewSession(p, actorID(actorMain))
	if err != nil {
		t.Fatalf("NewSession: %v", err)
	}
	return s
}

var opCounter uint64 = 60000

func nextOpNum() uint64 { opCounter++; return opCounter }

func createLvlOp(t testing.TB, id UUID, name string) *Operation {
	return mustCreateOp(t, nextOpNum(), levelJSON(t, id, name))
}

func createWallOp(t testing.TB, id, lvl UUID) *Operation {
	return mustCreateOp(t, nextOpNum(),
		wallJSON(t, id, lvl, 0, 0, 4000, 2500))
}

func setThicknessOp(t testing.TB, wall UUID, mm int) *Operation {
	return mustSetOp(t, nextOpNum(), wall, map[string]json.RawMessage{
		FieldThicknessMm: json.RawMessage(fmt.Sprintf(`%d`, mm))})
}

// entitiesEqual compares the authoritative LIVE entity sets (S1) of two
// states: the equivalence-projection claim of BEHAVIOR-SPEC §8. Tombstones
// are NOT compared here: executed undos/redos append real compensating
// batches, so history-derived tombstones legitimately differ while the
// restored projection stays equivalent; tombstone behavior is asserted
// explicitly where it matters.
func entitiesEqual(a, b State) bool {
	if len(a.Entities) != len(b.Entities) {
		return false
	}
	for i := range a.Entities {
		if !reflect.DeepEqual(a.Entities[i], b.Entities[i]) {
			return false
		}
	}
	return true
}

func TestExecuteOneCommandOneEntryFreshIdentities(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)

	lvl := numUUID(11)
	b1, err := s.Execute(createLvlOp(t, lvl, "L1"))
	if err != nil || b1 == nil {
		t.Fatalf("execute L1: %v", err)
	}
	lvl2, wall := numUUID(12), numUUID(13)
	b2, err := s.Execute(createLvlOp(t, lvl2, "L2"), createWallOp(t, wall, lvl2))
	if err != nil {
		t.Fatalf("execute compound: %v", err)
	}

	if s.UndoDepth() != 2 || s.CanRedo() {
		t.Fatalf("stack bookkeeping wrong: undo=%d redo=%v", s.UndoDepth(), s.CanRedo())
	}
	if !s.Dirty() {
		t.Fatal("accepted local command must dirty the document")
	}
	if f := p.State().Frontier[actorID(actorMain)]; f != b2.EndCounter() {
		t.Fatalf("frontier must advance to batch end (B5): %d vs %d", f, b2.EndCounter())
	}
	afterCmds := p.State()

	seenBatches := map[UUID]struct{}{b1.BatchID: {}, b2.BatchID: {}}
	seenOps := map[UUID]struct{}{}
	for _, b := range []*Batch{b1, b2} {
		for _, op := range b.Operations {
			seenOps[op.OperationID] = struct{}{}
		}
	}
	freshBatch := func(b *Batch, what string) {
		t.Helper()
		if _, dup := seenBatches[b.BatchID]; dup {
			t.Fatalf("%s reused batch id %s (R8.1.2)", what, b.BatchID.String())
		}
		seenBatches[b.BatchID] = struct{}{}
		for _, op := range b.Operations {
			if _, dup := seenOps[op.OperationID]; dup {
				t.Fatalf("%s reused operation id %s (R8.1.2)", what, op.OperationID.String())
			}
			seenOps[op.OperationID] = struct{}{}
		}
	}

	for i := 0; i < 2; i++ {
		cb, err := s.Undo()
		if err != nil || cb == nil {
			t.Fatalf("undo %d: %v", i+1, err)
		}
		freshBatch(cb, fmt.Sprintf("undo %d", i+1))
	}
	if !entitiesEqual(New().State(), p.State()) {
		t.Fatal("undos must restore the empty projection")
	}
	for i := 0; i < 2; i++ {
		rb, err := s.Redo()
		if err != nil || rb == nil {
			t.Fatalf("redo %d: %v", i+1, err)
		}
		freshBatch(rb, fmt.Sprintf("redo %d", i+1))
	}
	if !entitiesEqual(afterCmds, p.State()) {
		t.Fatal("redos must restore an equivalent projection with same entity ids")
	}
	if s.UndoDepth() != 2 || s.RedoDepth() != 0 {
		t.Fatalf("final stacks: undo=%d redo=%d", s.UndoDepth(), s.RedoDepth())
	}
}

func TestExecuteRejectionsLeaveNoTrace(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl := numUUID(11)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1")); err != nil {
		t.Fatalf("seed level: %v", err)
	}
	wall := numUUID(12)
	if _, err := s.Execute(createWallOp(t, wall, lvl)); err != nil {
		t.Fatalf("seed wall: %v", err)
	}
	depth, dirty := s.UndoDepth(), s.Dirty()

	cases := []struct {
		name string
		exec func() error
	}{
		{"empty command", func() error { _, err := s.Execute(); return err }},
		{"setFields on absent target", func() error {
			_, err := s.Execute(mustSetOp(t, nextOpNum(), numUUID(77), setLevelName(t, "x")))
			return err
		}},
		{"delete on absent target", func() error {
			_, err := s.Execute(mustDeleteOp(t, nextOpNum(), numUUID(78)))
			return err
		}},
		{"create colliding with live id", func() error {
			_, err := s.Execute(createWallOp(t, wall, lvl))
			return err
		}},
		{"schema-invalid payload rejects at apply seam", func() error {
			_, err := s.Execute(setThicknessOp(t, wall, 0))
			return err
		}},
	}
	rejected := 0
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if err := tc.exec(); err == nil {
				t.Fatal("expected rejection")
			}
			rejected++
		})
	}

	if s.UndoDepth() != depth || s.RedoDepth() != 0 || s.Dirty() != dirty {
		t.Fatalf("rejections changed stack/dirty state: undo=%d dirty=%v", s.UndoDepth(), s.Dirty())
	}
	// The counter stream must be un-consumed: the next command chains.
	b, err := s.Execute(setThicknessOp(t, wall, 150))
	if err != nil {
		t.Fatalf("post-rejection command rejected (counter torn?): %v", err)
	}
	if b.CounterBase != 3 {
		t.Fatalf("rejected commands must not reserve counters (B3), got base %d", b.CounterBase)
	}
	_ = rejected
}

func TestLevelDeletionGatesAtCommandTime(t *testing.T) {
	installDeterministicMint(t)

	newDoc := func(t *testing.T) (*Session, UUID, UUID) {
		p := New()
		s := mustSession(t, p)
		l1, l2 := numUUID(21), numUUID(22)
		if _, err := s.Execute(createLvlOp(t, l1, "L1")); err != nil {
			t.Fatalf("L1: %v", err)
		}
		return s, l1, l2
	}

	t.Run("last remaining level rejects", func(t *testing.T) {
		s, l1, _ := newDoc(t)
		_, err := s.Execute(mustDeleteOp(t, nextOpNum(), l1))
		var cre *CommandRejectedError
		if !errors.As(err, &cre) || cre.Code != codeLastLevel {
			t.Fatalf("want last-level rejection, got %v", err)
		}
	})

	t.Run("level hosting entities rejects even when deleted together", func(t *testing.T) {
		s, l1, l2 := newDoc(t)
		wall := numUUID(23)
		if _, err := s.Execute(createLvlOp(t, l2, "L2"), createWallOp(t, wall, l1)); err != nil {
			t.Fatalf("seed: %v", err)
		}
		for _, ops := range [][]*Operation{
			{mustDeleteOp(t, nextOpNum(), l1)},
			{mustDeleteOp(t, nextOpNum(), wall), mustDeleteOp(t, nextOpNum(), l1)},
		} {
			_, err := s.Execute(ops...)
			var cre *CommandRejectedError
			if !errors.As(err, &cre) || cre.Code != codeLevelNotEmpty {
				t.Fatalf("containment gate must reject (never cascades), got %v", err)
			}
			if !strings.Contains(cre.Detail, "wall×1") {
				t.Fatalf("gate must list blocking kinds and counts, got %q", cre.Detail)
			}
		}
	})

	t.Run("surviving reference created in the same command rejects", func(t *testing.T) {
		s, l1, l2 := newDoc(t)
		if _, err := s.Execute(createLvlOp(t, l2, "L2")); err != nil {
			t.Fatalf("L2: %v", err)
		}
		wall := numUUID(24)
		_, err := s.Execute(createWallOp(t, wall, l1), mustDeleteOp(t, nextOpNum(), l1))
		var cre *CommandRejectedError
		if !errors.As(err, &cre) || cre.Code != codeLevelReferenced {
			t.Fatalf("surviving-reference gate must reject, got %v", err)
		}
	})

	t.Run("deleting an empty non-last level succeeds and undo restores it", func(t *testing.T) {
		s, _, l2 := newDoc(t)
		if _, err := s.Execute(createLvlOp(t, l2, "L2")); err != nil {
			t.Fatalf("L2: %v", err)
		}
		before := p_stateOf(s)
		if _, err := s.Execute(mustDeleteOp(t, nextOpNum(), l2)); err != nil {
			t.Fatalf("empty non-last level delete: %v", err)
		}
		if _, err := s.Undo(); err != nil {
			t.Fatalf("undo of level delete: %v", err)
		}
		if !entitiesEqual(before, p_stateOf(s)) {
			t.Fatal("undo must restore the deleted level with its own id")
		}
	})
}

func p_stateOf(s *Session) State { return s.proj.State() }

func TestUndoOfDeleteAndRedoTombstoneLifecycle(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl, wall := numUUID(31), numUUID(32)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1")); err != nil {
		t.Fatalf("L1: %v", err)
	}
	if _, err := s.Execute(createWallOp(t, wall, lvl)); err != nil {
		t.Fatalf("wall: %v", err)
	}

	if _, err := s.Execute(mustDeleteOp(t, nextOpNum(), wall)); err != nil {
		t.Fatalf("delete: %v", err)
	}
	afterDelete := p_stateOf(s)
	if _, ok := p.Tombstone(wall); !ok {
		t.Fatal("delete must tombstone (I3)")
	}

	ub, err := s.Undo()
	if err != nil {
		t.Fatalf("undo of delete: %v", err)
	}
	if _, ok := p.Tombstone(wall); ok {
		t.Fatal("authorized restoration must clear the own-command tombstone")
	}
	if got := p.Entity(wall); got == nil || got.EntityID() != wall {
		t.Fatal("undo must recreate the SAME entity id")
	}
	foundRestore := false
	for _, d := range p.Diagnostics() {
		if d.Code == DiagAuthorizedRestore && d.EntityID == wall && d.BatchID == ub.BatchID {
			foundRestore = true
		}
	}
	if !foundRestore {
		t.Fatal("authorized restoration must be recorded as a diagnostic")
	}

	rb, err := s.Redo()
	if err != nil || rb == nil {
		t.Fatalf("redo re-deletes: %v", err)
	}
	if ub.BatchID == rb.BatchID {
		t.Fatal("redo must mint fresh identities (R8.1.2)")
	}
	if _, ok := p.Tombstone(wall); !ok {
		t.Fatal("redo restores the original effect: deleted again")
	}
	if !entitiesEqual(afterDelete, p_stateOf(s)) {
		t.Fatal("full undo→redo cycle must return to the post-delete projection")
	}
}

func TestRedoInvalidationAndExternalFence(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	a, b := numUUID(41), numUUID(42)

	// U6: intervening local command clears the entire redo stack.
	if _, err := s.Execute(createLvlOp(t, a, "A")); err != nil {
		t.Fatalf("A: %v", err)
	}
	if _, err := s.Execute(createWallOp(t, numUUID(43), a)); err != nil {
		t.Fatalf("wall: %v", err)
	}
	if _, err := s.Undo(); err != nil {
		t.Fatalf("undo wall: %v", err)
	}
	if !s.CanRedo() {
		t.Fatal("setup: redo should hold one entry")
	}
	if _, err := s.Execute(createLvlOp(t, b, "B")); err != nil {
		t.Fatalf("B: %v", err)
	}
	if s.CanRedo() {
		t.Fatal("accepted local command must clear redo (R8.4.1)")
	}
	if rb, err := s.Redo(); rb != nil || err != nil {
		t.Fatalf("redo attempt after clearing must be a no-op, got %v %v", rb, err)
	}

	// MarkSaved clears dirty only; stacks untouched (save rows).
	s.MarkSaved()
	if s.Dirty() {
		t.Fatal("MarkSaved must clear dirty without touching stacks")
	}

	// U7: accepted external batch clears BOTH stacks and dirties.
	mainFrontier := p.State().Frontier[actorID(actorMain)]
	// The external write wins LWW legitimately: equal counter with A's
	// creation key, greater actor id (E5) breaks the tie toward X.
	ext := mustBatch(t, 700, actorID(actorX), 1,
		map[UUID]int64{actorID(actorMain): mainFrontier},
		mustSetOp(t, 701, a, setLevelName(t, "external")))
	if err := s.DeliverExternal(ext); err != nil {
		t.Fatalf("external delivery: %v", err)
	}
	if s.CanUndo() || s.CanRedo() {
		t.Fatal("fence must clear both stacks (R8.3.1)")
	}
	if !s.Dirty() {
		t.Fatal("external acceptance dirties the document though it makes no local entry (R8.3.3)")
	}
	if got := p.Entity(a).(*Level).Name; got != "external" {
		t.Fatalf("external effects missing: %q", got)
	}

	t.Run("a wholly rejected external call accepts nothing", func(t *testing.T) {
		wall := numUUID(46)
		if _, err := s.Execute(createWallOp(t, wall, b)); err != nil {
			t.Fatalf("local cmd: %v", err)
		}
		if !s.CanUndo() {
			t.Fatal("setup expects a local undo entry")
		}
		// One good batch and one invalid one in a single external call.
		// Deliver is a SET operation and a set with an invalid member
		// defines no projection, so the whole call rolls back: nothing is
		// accepted, so nothing fences and nothing dirties.
		//
		// This case previously asserted the opposite, and passed only
		// because a rejected call leaked its batch ids into the applied
		// set — DeliverExternal detected "acceptance" by that leak. Fixing
		// the leak removed the phantom acceptance the assertion was reading.
		good := mustBatch(t, 705, actorID(actorY), 1, nil,
			mustSetOp(t, 706, b, setLevelName(t, "external-2")))
		badRaw := rawMap(t, levelJSON(t, numUUID(47), "bad"))
		badRaw["elevationMm"] = json.RawMessage(fmt.Sprint(-2 * CoordBoundMm))
		badExt := &Operation{
			OperationID: numUUID(711),
			Kind:        OpCreateEntity,
			EntityID:    numUUID(47),
			Payload:     rawJSON(t, badRaw),
		}
		badExtB := mustBatch(t, 710, actorID(actorY), 2, closedFrontier(actorID(actorY), 2, nil), badExt)
		// Isolate the variable: the setup's local command already dirtied
		// the document, so without this the dirty assertion below would be
		// about that command rather than about the external call.
		s.MarkSaved()
		err := s.DeliverExternal(good, badExtB)
		if err == nil {
			t.Fatal("invalid external batch must report rejection")
		}
		if !s.CanUndo() {
			t.Fatal("a wholly rejected external call accepts nothing, so the local undo entry survives (R8.3.1)")
		}
		if s.Dirty() {
			t.Fatal("a wholly rejected external call has no effects to dirty the document with")
		}
		// The fence is not broken — it fires on real acceptance. Without
		// this half the pair would only prove that nothing accepts
		// nothing, which every broken fence also satisfies. The
		// transition is asserted in both directions: the stack is
		// non-empty going in and empty coming out.
		//
		// The stage this rests on is established directly by
		// TestReplayStageFailureIsAllOrNothing: the rejection above
		// happens during REPLAY, not framing, and replayAndCommit is
		// all-or-nothing there, so there was never a partially accepted
		// call for the old assertion to be reading.
		if !s.CanUndo() {
			t.Fatal("precondition: the local undo entry must still be present before the accepted delivery")
		}
		if err := s.DeliverExternal(good); err != nil {
			t.Fatalf("the good batch alone must apply: %v", err)
		}
		if s.CanUndo() {
			t.Fatal("an accepted external batch clears the local stacks (R8.3.1)")
		}
		if !s.Dirty() {
			t.Fatal("an accepted external batch dirties the document (R8.3.3)")
		}
	})
}

func TestReopenClearsStacksResumesCounters(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl := numUUID(51)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1")); err != nil {
		t.Fatalf("cmd: %v", err)
	}
	lastEnd := p.State().Frontier[actorID(actorMain)]

	reopened, err := NewSession(p, actorID(actorMain)) // R8.4.2/U9
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	if reopened.CanUndo() || reopened.CanRedo() {
		t.Fatal("a fresh session over a replayed projection has empty stacks")
	}
	nb, err := reopened.Execute(createWallOp(t, numUUID(52), lvl))
	if err != nil {
		t.Fatalf("first edit after reopen: %v", err)
	}
	if nb.CounterBase != lastEnd+1 {
		t.Fatalf("reopen-then-edit must chain contiguously (B3): base %d, want %d",
			nb.CounterBase, lastEnd+1)
	}
}

func TestTruncationToRetainedPrefixU10(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl, wall := numUUID(61), numUUID(62)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1"), createWallOp(t, wall, lvl)); err != nil {
		t.Fatalf("K1: %v", err)
	}
	if _, err := s.Execute(setThicknessOp(t, wall, 140)); err != nil {
		t.Fatalf("K2: %v", err)
	}
	k3, err := s.Execute(setThicknessOp(t, wall, 160))
	if err != nil {
		t.Fatalf("K3: %v", err)
	}
	if s.UndoDepth() != 3 {
		t.Fatalf("setup depth: %d", s.UndoDepth())
	}

	notices := s.TruncateToRetained(map[UUID]int64{actorID(actorMain): k3.CounterBase - 1})
	if len(notices) != 1 || notices[0].Code != noticeUndoTruncate {
		t.Fatalf("want one truncation diagnostic, got %+v", notices)
	}
	if !strings.Contains(notices[0].Detail, "2") {
		t.Fatalf("diagnostic must name the truncated count (U10): %q", notices[0].Detail)
	}
	if s.UndoDepth() != 1 {
		t.Fatalf("stack must truncate to the retained suffix, depth=%d", s.UndoDepth())
	}

	if _, err := s.Undo(); err != nil {
		t.Fatalf("undo of retained entry: %v", err)
	}
	if got := p.Entity(wall).(*Wall).ThicknessMm; got != 140 {
		t.Fatalf("undo must revert K3 only, got %v", got)
	}
	if b, err := s.Undo(); b != nil || err != nil {
		t.Fatalf("second undo must be a silent no-op, got %v %v", b, err)
	}
}

func TestCapacityExhaustionDropOldestU11(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	if err := s.SetStackCapacity(5, 3); err != nil {
		t.Fatalf("caps: %v", err)
	}
	lvl, wall := numUUID(71), numUUID(72)
	if _, err := s.Execute(createLvlOp(t, lvl, "L"), createWallOp(t, wall, lvl)); err != nil {
		t.Fatalf("seed: %v", err)
	}
	var cmds []*Batch
	for i := 0; i < 6; i++ {
		b, err := s.Execute(setThicknessOp(t, wall, 200+i))
		if err != nil {
			t.Fatalf("C%d: %v", i+1, err)
		}
		cmds = append(cmds, b)
	}
	if s.UndoDepth() != 5 {
		t.Fatalf("undo capacity must drop oldest: depth=%d", s.UndoDepth())
	}
	drops := 0
	for _, n := range s.Notices() {
		if n.Code == noticeUndoDrop && strings.Contains(n.Detail, cmds[0].BatchID.String()) {
			drops++
		}
	}
	if drops != 1 {
		t.Fatalf("oldest entry's drop must be named once, got %d", drops)
	}

	// Five undos overflow the redo capacity of 3 via undos ONLY (U11).
	for i := 0; i < 5; i++ {
		if _, err := s.Undo(); err != nil {
			t.Fatalf("undo %d: %v", i+1, err)
		}
	}
	redoDrops := 0
	for _, n := range s.Notices() {
		if n.Code == noticeRedoDrop {
			redoDrops++
		}
	}
	if redoDrops != 2 || s.RedoDepth() != 3 {
		t.Fatalf("redo overflow: drops=%d depth=%d", redoDrops, s.RedoDepth())
	}
	if got := p.Entity(wall).(*Wall).ThicknessMm; got != 200 {
		t.Fatalf("dropped C1 stays applied; five undos revert C2..C6 only, got %v", got)
	}

	// Three redos restore C2, C3, C4 (the surviving entries, newest first);
	// the fourth is a no-op even though C5/C6 restorations were dropped.
	for i := 0; i < 3; i++ {
		if _, err := s.Redo(); err != nil {
			t.Fatalf("redo %d: %v", i+1, err)
		}
	}
	if b, err := s.Redo(); b != nil || err != nil {
		t.Fatalf("fourth redo must be a no-op (U11 phase 2), got %v %v", b, err)
	}
	if got := p.Entity(wall).(*Wall).ThicknessMm; got != 203 {
		t.Fatalf("partial redo must land on C4's value 203, got %v", got)
	}
	if s.UndoDepth() != 3 || s.RedoDepth() != 0 {
		t.Fatalf("stacks after phase 2: undo=%d redo=%d", s.UndoDepth(), s.RedoDepth())
	}
}

func TestProvenanceClassesAuthorizeDistinctPaths(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl, wall := numUUID(81), numUUID(82)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1")); err != nil {
		t.Fatalf("L1: %v", err)
	}
	if _, err := s.Execute(createWallOp(t, wall, lvl)); err != nil {
		t.Fatalf("wall: %v", err)
	}
	// Undo tombstones the wall via the undo chain; redo is authorized.
	if _, err := s.Undo(); err != nil {
		t.Fatalf("undo: %v", err)
	}
	key, _ := p.Tombstone(wall)
	if s.provenance[key] != OriginUndoChain {
		t.Fatalf("undo-chain tombstone provenance wrong: %v", s.provenance[key])
	}

	// Defense-in-depth: lost provenance refuses fail-closed instead of
	// resurrecting across an unknown fence history.
	delete(s.provenance, key)
	before := p_stateOf(s)
	if _, err := s.Redo(); err == nil {
		t.Fatal("redo must refuse restoration without provenance")
	}
	if !entitiesEqual(before, p_stateOf(s)) || s.RedoDepth() != 1 {
		t.Fatal("refused redo must leave state and stacks untouched")
	}

	// A tombstone carrying ONLY local-command provenance authorizes undo,
	// never redo (R8.1.3). Unreachable through normal flows (any accepted
	// local command clears redo first); asserted at classification level.
	s.provenance[key] = OriginLocalCommand
	if _, err := s.Redo(); err == nil {
		t.Fatal("redo must refuse local-command-provenance tombstones (R8.1.3)")
	}
	s.provenance[key] = OriginUndoChain
	if _, err := s.Redo(); err != nil {
		t.Fatalf("restored provenance authorizes redo again: %v", err)
	}
}

func TestCompoundSameEntityInversionPreservesAbsenceFidelity(t *testing.T) {
	installDeterministicMint(t)
	p := New()
	s := mustSession(t, p)
	lvl, rr := numUUID(91), numUUID(92)
	if _, err := s.Execute(createLvlOp(t, lvl, "L1")); err != nil {
		t.Fatalf("L1: %v", err)
	}
	if _, err := s.Execute(mustCreateOp(t, nextOpNum(), roomRecordJSON(t, rr, lvl))); err != nil {
		t.Fatalf("room: %v", err)
	}
	before := p.Entity(rr).(*RoomRecord)
	if before.Name != nil || before.FloorStyleRef != nil {
		t.Fatal("setup expects both optionals absent")
	}

	// Compound: several writes to the SAME entity in one command, setting
	// optionals that were previously absent.
	if _, err := s.Execute(
		mustSetOp(t, nextOpNum(), rr, map[string]json.RawMessage{
			FieldName: json.RawMessage(`"kitchen"`)}),
		mustSetOp(t, nextOpNum(), rr, map[string]json.RawMessage{
			FieldFloorStyleRef: json.RawMessage(`"F1"`),
			FieldName:          json.RawMessage(`"kitchen-2"`)}),
	); err != nil {
		t.Fatalf("compound: %v", err)
	}
	modified := p.Entity(rr).(*RoomRecord)
	if modified.Name == nil || *modified.Name != "kitchen-2" || modified.FloorStyleRef == nil {
		t.Fatalf("compound writes missing: %+v", modified)
	}

	if _, err := s.Undo(); err != nil {
		t.Fatalf("undo: %v", err)
	}
	restored := p.Entity(rr).(*RoomRecord)
	if restored.Name != nil || restored.FloorStyleRef != nil {
		t.Fatalf("absence fidelity broken: name=%v floor=%v", restored.Name, restored.FloorStyleRef)
	}
	if restored.SeedMm != before.SeedMm || restored.LevelID != before.LevelID {
		t.Fatalf("typed fields drifted: %+v", restored)
	}
}

func TestLevelContainmentGateResolvesEffectiveLevelsF2(t *testing.T) {
	installDeterministicMint(t)

	t.Run("openings block through their host wall", func(t *testing.T) {
		p := New()
		s := mustSession(t, p)
		l1, l2 := numUUID(101), numUUID(102)
		if _, err := s.Execute(createLvlOp(t, l1, "L1"), createLvlOp(t, l2, "L2")); err != nil {
			t.Fatalf("levels: %v", err)
		}
		wall := numUUID(103)
		open1, open2 := numUUID(104), numUUID(105)
		if _, err := s.Execute(
			createWallOp(t, wall, l1),
			mustCreateOp(t, nextOpNum(), openingJSON(t, open1, wall)),
			mustCreateOp(t, nextOpNum(), openingJSON(t, open2, wall)),
		); err != nil {
			t.Fatalf("entities: %v", err)
		}
		_, err := s.Execute(mustDeleteOp(t, nextOpNum(), l1))
		var cre *CommandRejectedError
		if !errors.As(err, &cre) || cre.Code != codeLevelNotEmpty {
			t.Fatalf("opening-hosted level deletion must reject, got %v", err)
		}
		if !strings.Contains(cre.Detail, "wall×1") || !strings.Contains(cre.Detail, "opening×2") {
			t.Fatalf("gate must report openings among blocking kinds, got %q", cre.Detail)
		}
	})

	t.Run("wallJoin blocks through its member walls", func(t *testing.T) {
		p := New()
		s := mustSession(t, p)
		l1, l2 := numUUID(111), numUUID(112)
		if _, err := s.Execute(createLvlOp(t, l1, "L1"), createLvlOp(t, l2, "L2")); err != nil {
			t.Fatalf("levels: %v", err)
		}
		wa, wb, join := numUUID(113), numUUID(114), numUUID(115)
		if _, err := s.Execute(
			createWallOp(t, wa, l1),
			wallJSONOn(t, wb, l1),
			mustCreateOp(t, nextOpNum(), wallJoinJSON(t, join, wa, wb)),
		); err != nil {
			t.Fatalf("entities: %v", err)
		}
		_, err := s.Execute(mustDeleteOp(t, nextOpNum(), l1))
		var cre *CommandRejectedError
		if !errors.As(err, &cre) || cre.Code != codeLevelNotEmpty {
			t.Fatalf("join-membered level deletion must reject, got %v", err)
		}
		if !strings.Contains(cre.Detail, "wallJoin×1") {
			t.Fatalf("gate must report the wallJoin among blocking kinds, got %q", cre.Detail)
		}
	})
}

// wallJSONOn is a distinct-id helper so both walls differ.
func wallJSONOn(t testing.TB, id, lvl UUID) *Operation {
	return mustCreateOp(t, nextOpNum(), wallJSON(t, id, lvl, 4000, 0, 4000, 3000))
}
