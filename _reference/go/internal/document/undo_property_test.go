package document

import (
	"encoding/json"
	"fmt"
	"math/rand"
	"reflect"
	"strings"
	"testing"
)

// Deterministic property tests for the command/undo/redo layer: random
// command streams (fixed seeds) with compound edits, failed commands,
// redo invalidation, and external fences, verified against an independent
// linear reconstruction of the active history. Each subtest runs TWICE and
// must produce identical outcomes (review condition).

type streamModel struct {
	undo []*Batch // source batches currently on the undo stack (bottom→top)
	redo []*Batch
}

type streamHarness struct {
	log     []string
	t       *testing.T
	s       *Session
	p       *Projection
	rng     *rand.Rand
	model   streamModel
	liveIDs []UUID // cached live ids for target selection
	nextEnt uint64
	refOps  []refCommand // active history for the reference reconstruction
	ext     map[uint64]int64
	extSeq  uint64
}

type refCommand struct {
	batch *Batch // external batch, delivered verbatim
	ops   []*Operation
}

func newStreamHarness(t *testing.T, seed int64) *streamHarness {
	p := New()
	s := mustSession(t, p)
	h := &streamHarness{
		t: t, s: s, p: p,
		rng:     rand.New(rand.NewSource(seed)),
		nextEnt: 1000,
		ext:     map[uint64]int64{},
	}
	// Seed one initial level so child entities have a home.
	h.exec(h.createLevelCmd())
	return h
}

func (h *streamHarness) refreshLive() {
	st := h.p.State()
	h.liveIDs = h.liveIDs[:0]
	for _, e := range st.Entities {
		h.liveIDs = append(h.liveIDs, e.EntityID())
	}
}

func (h *streamHarness) pickLive() (UUID, Entity) {
	h.refreshLive()
	if len(h.liveIDs) == 0 {
		return UUID{}, nil
	}
	id := h.liveIDs[h.rng.Intn(len(h.liveIDs))]
	return id, h.p.Entity(id)
}

func (h *streamHarness) pickByKind(k Kind) UUID {
	h.refreshLive()
	var cands []UUID
	for _, id := range h.liveIDs {
		if h.p.Entity(id).Kind() == k {
			cands = append(cands, id)
		}
	}
	if len(cands) == 0 {
		return UUID{}
	}
	return cands[h.rng.Intn(len(cands))]
}

func (h *streamHarness) coord() float64 { return float64(h.rng.Intn(20001)) - 10000 }

// --- generators -----------------------------------------------------------

func (h *streamHarness) createLevelCmd() []*Operation {
	id := numUUID(0xC1E0 + h.nextEnt)
	h.nextEnt++
	op := mustCreateOp(h.t, nextOpNum(), levelJSON(h.t, id, fmt.Sprintf("PL%d", h.nextEnt)))
	return []*Operation{op}
}

func (h *streamHarness) createChildCmd() []*Operation {
	h.refreshLive()
	level := h.pickByKind(KindLevel)
	wallA := h.pickByKind(KindWall)
	var ops []*Operation
	switch {
	case level == (UUID{}):
		return h.createLevelCmd()
	case wallA != (UUID{}) && h.rng.Intn(4) == 0:
		id := numUUID(0xB0A0 + h.nextEnt)
		h.nextEnt++
		ops = append(ops, mustCreateOp(h.t, nextOpNum(), openingJSON(h.t, id, wallA)))
	default:
		id := numUUID(0xCA11 + h.nextEnt)
		h.nextEnt++
		switch h.rng.Intn(3) {
		case 0:
			ops = append(ops, mustCreateOp(h.t, nextOpNum(), wallJSON(h.t, id, level,
				h.coord(), h.coord(), h.coord()+float64(1+h.rng.Intn(2000)), h.coord())))
		case 1:
			ops = append(ops, mustCreateOp(h.t, nextOpNum(),
				roomRecordJSON(h.t, id, level)))
		default:
			ops = append(ops, mustCreateOp(h.t, nextOpNum(), labelJSON(h.t, id, level)))
		}
	}
	return ops
}

func (h *streamHarness) setCmd() []*Operation {
	id, ent := h.pickLive()
	if id == (UUID{}) {
		return nil
	}
	field := func(name string, v string) *Operation {
		return mustSetOp(h.t, nextOpNum(), id, map[string]json.RawMessage{
			name: json.RawMessage(v)})
	}
	switch e := ent.(type) {
	case *Level:
		if h.rng.Intn(2) == 0 {
			return []*Operation{field(FieldElevationMm, fmt.Sprint(-50*h.rng.Intn(5)))}
		}
		return []*Operation{field(FieldName, fmt.Sprintf(`"renamed-%d"`, h.rng.Intn(99)))}
	case *Wall:
		if h.rng.Intn(2) == 0 {
			return []*Operation{field(FieldThicknessMm, fmt.Sprint(50+h.rng.Intn(400)))}
		}
		return []*Operation{field(FieldHeightMm, fmt.Sprint(2000+h.rng.Intn(1500)))}
	case *Opening:
		return []*Operation{field(FieldSwingDeg, fmt.Sprint(h.rng.Intn(int(SwingDegMax)+1)))}
	case *RoomRecord:
		switch h.rng.Intn(3) {
		case 0:
			return []*Operation{field(FieldName, `"room-name"`)}
		case 1:
			return []*Operation{field(FieldName, `null`)}
		default:
			return []*Operation{field(FieldFloorStyleRef, fmt.Sprintf(`"F%d"`, h.rng.Intn(9)))}
		}
	case *Label:
		return []*Operation{field(FieldName, fmt.Sprintf(`"lbl-%d"`, h.rng.Intn(20)))}
	case *Dimension:
		return []*Operation{field(FieldLabelOverride, `"dim"`)}
	default:
		_ = e
		return nil // generator gap (assetInstance/wallJoin): nothing to set
	}
}

func (h *streamHarness) deleteCmd() []*Operation {
	id, _ := h.pickLive()
	if id == (UUID{}) {
		return nil
	}
	return []*Operation{mustDeleteOp(h.t, nextOpNum(), id)}
}

func (h *streamHarness) invalidCmd() []*Operation {
	switch h.rng.Intn(3) {
	case 0: // unknown target
		return []*Operation{mustSetOp(h.t, nextOpNum(), numUUID(0xDEAD), setLevelNameT("ghost"))}
	case 1: // schema-invalid value
		if w := h.pickByKind(KindWall); w != (UUID{}) {
			return []*Operation{mustSetOp(h.t, nextOpNum(), w, map[string]json.RawMessage{
				FieldThicknessMm: json.RawMessage(`0`)})}
		}
		fallthrough
	default: // duplicate live id
		id, _ := h.pickLive()
		if id == (UUID{}) {
			return []*Operation{mustSetOp(h.t, nextOpNum(), numUUID(0xDEAD), setLevelNameT("ghost"))}
		}
		lvl := h.pickByKind(KindLevel)
		if lvl == (UUID{}) {
			return []*Operation{mustSetOp(h.t, nextOpNum(), numUUID(0xDEAD), setLevelNameT("ghost"))}
		}
		return []*Operation{mustCreateOp(h.t, nextOpNum(), roomRecordJSON(h.t, id, lvl))}
	}
}

func setLevelNameT(v string) map[string]json.RawMessage {
	return map[string]json.RawMessage{FieldName: json.RawMessage(`"` + v + `"`)}
}

func (h *streamHarness) compoundCmd() []*Operation {
	var ops []*Operation
	for i := 0; i < 1+h.rng.Intn(2); i++ {
		switch h.rng.Intn(3) {
		case 0:
			ops = append(ops, h.createChildCmd()...)
		case 1:
			ops = append(ops, h.setCmd()...)
		default:
			ops = append(ops, h.deleteCmd()...)
		}
	}
	if len(ops) == 0 {
		ops = h.setCmd()
	}
	return ops
}

// external fence deliveries come from their OWN actor streams and create
// independent levels (no references into local data).
func (h *streamHarness) externalBatch() *Batch {
	actorNum := []uint64{actorX, actorY, actorLateC}[h.rng.Intn(3)]
	base := h.ext[actorNum] + 1
	h.ext[actorNum] = base
	h.extSeq++
	id := numUUID(0xEE00 + h.extSeq)
	op := mustCreateOp(h.t, uint64(90000)+h.extSeq, levelJSON(h.t, id, fmt.Sprintf("EXT%d", h.extSeq)))
	return mustBatch(h.t, uint64(80000)+h.extSeq, actorID(actorNum), base,
		closedFrontier(actorID(actorNum), base, nil), op)
}

// --- execution + verification ---------------------------------------------

func (h *streamHarness) execNamed(step int, name string, ops []*Operation) {
	var desc []string
	for _, op := range ops {
		desc = append(desc, fmt.Sprintf("%s:%s", op.Kind, op.EntityID.String()[0:8]))
	}
	h.log = append(h.log, fmt.Sprintf("step %d EXEC %s %v", step, name, desc))
	h.exec(ops)
}

func (h *streamHarness) exec(ops []*Operation) {
	h.t.Helper()
	if len(ops) == 0 {
		return
	}
	before := h.p.State()
	b, err := h.s.Execute(ops...)
	if err != nil {
		// Failed command (R8.2.2/matrix): zero effects, stacks unchanged.
		if !entitiesEqual(before, h.p.State()) || h.s.UndoDepth() != len(h.model.undo) ||
			h.s.RedoDepth() != len(h.model.redo) {
			h.t.Fatalf("rejected command had side effects: %v", err)
		}
		return
	}
	h.model.redo = h.model.redo[:0] // R8.4.1
	h.model.undo = append(h.model.undo, b)
	h.refOps = append(h.refOps, refCommand{ops: ops})
}

func (h *streamHarness) verify() {
	h.t.Helper()
	if h.s.UndoDepth() != len(h.model.undo) || h.s.RedoDepth() != len(h.model.redo) {
		h.t.Fatalf("stack model diverged: session undo=%d redo=%d, model %d/%d",
			h.s.UndoDepth(), h.s.RedoDepth(), len(h.model.undo), len(h.model.redo))
	}
	// Locally authored batches can never go stale at replay (command-time
	// staging mirrors applyBatch decision points).
	for _, d := range h.p.Diagnostics() {
		if d.Code == DiagStaleBatchDiscard && d.ActorID == actorID(actorMain) {
			b := h.s.proj.batches[d.BatchID]
			h.t.Logf("stale batch %s counterBase=%d", d.BatchID.String(), b.CounterBase)
			for i, op := range b.Operations {
				h.t.Logf("  op %d: %s %s payload=%s", i, op.Kind, op.EntityID.String(), op.Payload)
			}
			st := h.p.State()
			h.t.Logf("tombstones:")
			for id, w := range st.Tombstones {
				h.t.Logf("  %s <- %s#%d", id.String(), w.Ref.String(), w.OpIndex)
			}
			h.t.Logf("LOG:\n%s", strings.Join(h.log, "\n"))
			h.t.Fatalf("session-authored batch went stale")
		}
	}
	// Independent linear reconstruction of the ACTIVE history must project
	// to the same authoritative live state.
	ref := New()
	counter := int64(1)
	for _, rc := range h.refOps {
		if rc.batch != nil {
			if err := ref.Deliver(rc.batch); err != nil {
				h.t.Fatalf("reference external replay: %v", err)
			}
			continue
		}
		ops := make([]*Operation, len(rc.ops))
		copy(ops, rc.ops)
		rb, err := NewBatch(numUUID(0xFA11+uint64(counter)), actorMainActor(), counter,
			closedFrontier(actorMainActor(), counter, nil), ops...)
		if err != nil {
			h.t.Fatalf("reference batch build: %v", err)
		}
		if err := ref.Deliver(rb); err != nil {
			h.t.Fatalf("reference replay rejected active command at counter %d: %v", counter, err)
		}
		counter += int64(len(ops))
	}
	if !entitiesEqual(ref.State(), h.p.State()) {
		h.t.Fatal("active-history reconstruction diverged from the session projection")
	}
}

func actorMainActor() UUID { return actorID(actorMain) }

func runRandomStream(t *testing.T, seed int64) []byte {
	t.Helper()
	installDeterministicMint(t)
	h := newStreamHarness(t, seed)
	for step := 0; step < 48; step++ {
		switch roll := h.rng.Intn(100); {
		case roll < 34:
			h.execNamed(step, "compound", h.compoundCmd())
		case roll < 48:
			h.execNamed(step, "createChild", h.createChildCmd())
		case roll < 60:
			h.execNamed(step, "set", h.setCmd())
		case roll < 68:
			h.execNamed(step, "delete", h.deleteCmd())
		case roll < 74:
			h.execNamed(step, "invalid", h.invalidCmd())
		case roll < 88:
			h.log = append(h.log, fmt.Sprintf("step %d UNDO canUndo=%v", step, h.s.CanUndo()))
			if h.s.CanUndo() {
				b, err := h.s.Undo()
				if err != nil || b == nil {
					t.Fatalf("undo: %v", err)
				}
				e := h.model.undo[len(h.model.undo)-1]
				h.model.undo = h.model.undo[:len(h.model.undo)-1]
				h.model.redo = append(h.model.redo, e)
				h.refOps = h.refOps[:len(h.refOps)-1] // pop newest active command
			} else if b, err := h.s.Undo(); b != nil || err != nil {
				t.Fatalf("undo on empty stack must be a no-op, got %v %v", b, err)
			}
		case roll < 96:
			h.log = append(h.log, fmt.Sprintf("step %d REDO canRedo=%v", step, h.s.CanRedo()))
			if h.s.CanRedo() {
				b, err := h.s.Redo()
				if err != nil || b == nil {
					t.Fatalf("redo: %v", err)
				}
				e := h.model.redo[len(h.model.redo)-1]
				h.model.redo = h.model.redo[:len(h.model.redo)-1]
				h.model.undo = append(h.model.undo, e)
				h.refOps = append(h.refOps, refCommand{ops: redoRefOps(e)})
			} else if b, err := h.s.Redo(); b != nil || err != nil {
				t.Fatalf("redo on empty stack must be a no-op, got %v %v", b, err)
			}
		default:
			h.log = append(h.log, fmt.Sprintf("step %d EXTERNAL", step))
			ext := h.externalBatch()
			if err := h.s.DeliverExternal(ext); err != nil {
				t.Fatalf("external: %v", err)
			}
			h.model.undo = nil
			h.model.redo = nil
			h.refOps = append(h.refOps, refCommand{batch: ext})
		}
		h.verify()
	}
	sig, err := json.Marshal(map[string]any{
		"state": serializeLive(h.p.State()),
		"undo":  len(h.model.undo),
		"redo":  len(h.model.redo),
	})
	if err != nil {
		t.Fatalf("signature: %v", err)
	}
	return sig
}

// redoRefOps extracts the logical operation content a redo re-applies.
func redoRefOps(e *Batch) []*Operation {
	ops := make([]*Operation, len(e.Operations))
	for i, op := range e.Operations {
		cp := *op
		ops[i] = &cp
	}
	return ops
}

func serializeLive(st State) []string {
	out := make([]string, 0, len(st.Entities))
	for _, e := range st.Entities {
		raw, err := encodeEntity(e)
		if err != nil {
			panic(err)
		}
		out = append(out, string(raw))
	}
	return out
}

func TestPropertyRandomCommandStreamsMatchActiveHistory(t *testing.T) {
	for seed := int64(propertySeedBase + 100); seed < propertySeedBase+106; seed++ {
		seed := seed
		t.Run(fmt.Sprintf("seed-%d", seed), func(t *testing.T) {
			first := runRandomStream(t, seed)
			second := runRandomStream(t, seed)
			if string(first) != string(second) {
				t.Fatal("two runs of the same seed must produce identical outcomes")
			}
		})
	}
}

func TestPropertyEncodeRoundTripSemanticEquality(t *testing.T) {
	for seed := int64(propertySeedBase); seed < propertySeedBase+8; seed++ {
		world := newPropWorld(seed)
		world.generate()
		for _, e := range world.ents {
			d1, err := decodeEntity(e.raw)
			if err != nil {
				t.Fatalf("seed %d entity decode: %v", seed, err)
			}
			raw2, err := encodeEntity(d1)
			if err != nil {
				t.Fatalf("encode %s: %v", e.kind, err)
			}
			d2, err := decodeEntity(raw2)
			if err != nil {
				t.Fatalf("redecode %s: %v (%s)", e.kind, err, raw2)
			}
			if !reflect.DeepEqual(d1, d2) {
				t.Fatalf("round trip drifted for %s:\n want=%#v\n got =%#v", e.kind, d1, d2)
			}
			raw3, err := encodeEntity(d2)
			if err != nil || string(raw2) != string(raw3) {
				t.Fatalf("encoding not stable for %s", e.kind)
			}
		}
	}

	// Explicit absence/presence and preserved-unknown-field fidelity.
	base := map[string]any{
		"id": numUUID(5).String(), "type": "roomRecord", "levelId": numUUID(6).String(),
		"seedMm": []float64{6000, 4500}, "name": "room",
		"floorStyleRef": "F1", "ceilingStyleRef": "F2",
		"vendorNote": json.RawMessage(`{"opaque":[1]}`),
	}
	rawBase, err := json.Marshal(base)
	if err != nil {
		t.Fatal(err)
	}
	withUnknown, err := decodeEntity(rawBase)
	if err != nil {
		t.Fatal(err)
	}
	raw, err := encodeEntity(withUnknown)
	if err != nil {
		t.Fatal(err)
	}
	again, err := decodeEntity(raw)
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(withUnknown, again) {
		t.Fatal("unknown-field preservation broken through encoding")
	}
	var cleared map[string]json.RawMessage
	if err := json.Unmarshal(raw, &cleared); err != nil {
		t.Fatal(err)
	}
	for _, name := range []string{"name", "floorStyleRef", "ceilingStyleRef", "vendorNote"} {
		if _, ok := cleared[name]; !ok {
			t.Fatalf("field %q lost in encoding", name)
		}
	}
}
