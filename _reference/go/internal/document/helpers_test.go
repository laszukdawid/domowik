package document

import (
	"encoding/binary"
	"encoding/json"
	"fmt"
	"testing"
)

// numUUID derives a deterministic, syntactically valid RFC 9562 UUID
// (version 7 layout, RFC_9562 variant) from a counter. Tests never mint
// random identities: fixed inputs keep every assertion reproducible.
func numUUID(n uint64) UUID {
	var u UUID
	u[6] = 0x70 // version 7
	u[8] = 0x80 // variant 10
	binary.BigEndian.PutUint16(u[10:12], uint16(n>>48))
	binary.BigEndian.PutUint16(u[12:14], uint16(n>>16&0xFFFF))
	binary.BigEndian.PutUint16(u[14:16], uint16(n))
	return u
}

func numUUIDStr(n uint64) string { return numUUID(n).String() }

func rawJSON(t testing.TB, v any) json.RawMessage {
	t.Helper()
	b, err := json.Marshal(v)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	return b
}

func rawMap(t testing.TB, raw json.RawMessage) map[string]any {
	t.Helper()
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatalf("unmarshal entity: %v", err)
	}
	return m
}

// Entity builders produce schema-valid baseline objects; individual tests
// and the property generators mutate the returned maps.

func levelJSON(t testing.TB, id UUID, name string) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "level", "name": name,
		"elevationMm": 0, "floorThicknessMm": 200,
		"defaultWallHeightMm": 2500, "defaultCeilingHeightMm": 2500,
	})
}

func wallJSON(t testing.TB, id, levelID UUID, x1, y1, x2, y2 float64) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "wall", "levelId": levelID.String(),
		"startMm": []float64{x1, y1}, "endMm": []float64{x2, y2},
		"thicknessMm": 120, "heightMm": 2500,
	})
}

func wallJoinJSON(t testing.TB, id, w1, w2 UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "wallJoin",
		"members": []any{
			map[string]any{"wallId": w1.String(), "role": "End"},
			map[string]any{"wallId": w2.String(), "role": "Start"},
		},
	})
}

func openingJSON(t testing.TB, id, hostWall UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "opening", "kind": "door",
		"hostWallId": hostWall.String(), "u": 0.5,
		"widthMm": 900, "heightMm": 2100, "depthMm": 120, "sillMm": 0,
		"side": "Left", "hinge": "nearStart", "opensToward": "Left", "swingDeg": 90,
	})
}

func roomRecordJSON(t testing.TB, id, levelID UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "roomRecord", "levelId": levelID.String(),
		"seedMm": []float64{6000, 4500},
	})
}

func labelJSON(t testing.TB, id, levelID UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "label", "levelId": levelID.String(),
		"anchorMm": []float64{3000, 1250}, "name": "Kitchen",
	})
}

func dimensionJSON(t testing.TB, id, levelID UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "dimension", "levelId": levelID.String(),
		"startMm": []float64{0, 0}, "endMm": []float64{4000, 0},
	})
}

func assetInstanceJSON(t testing.TB, id, levelID UUID) json.RawMessage {
	return rawJSON(t, map[string]any{
		"id": id.String(), "type": "assetInstance", "levelId": levelID.String(),
		"digest": fmt.Sprintf("sha256:%064x", ntoaID(id)),
		"transform": map[string]any{
			"translationMm": []float64{5200, 3100}, "scaleUniform": 1, "rotationDeg": 0,
		},
	})
}

func ntoaID(u UUID) uint64 {
	return binary.BigEndian.Uint64(u[8:16])
}

func mustParseUUID(t testing.TB, s string) UUID {
	t.Helper()
	u, err := ParseUUID(s)
	if err != nil {
		t.Fatalf("ParseUUID(%q): %v", s, err)
	}
	return u
}

func mustCreateOp(t testing.TB, opIDNum uint64, entityRaw json.RawMessage) *Operation {
	t.Helper()
	op, err := NewCreateOp(numUUID(opIDNum), entityRaw)
	if err != nil {
		t.Fatalf("NewCreateOp: %v", err)
	}
	return op
}

func mustSetOp(t testing.TB, opIDNum uint64, entityID UUID, fields map[string]json.RawMessage) *Operation {
	t.Helper()
	op, err := NewSetFieldsOp(numUUID(opIDNum), entityID, fields)
	if err != nil {
		t.Fatalf("NewSetFieldsOp: %v", err)
	}
	return op
}

func mustDeleteOp(t testing.TB, opIDNum uint64, entityID UUID) *Operation {
	t.Helper()
	op, err := NewDeleteOp(numUUID(opIDNum), entityID)
	if err != nil {
		t.Fatalf("NewDeleteOp: %v", err)
	}
	return op
}

func mustBatch(t testing.TB, batchNum uint64, actorID UUID, counterBase int64, frontier map[UUID]int64, ops ...*Operation) *Batch {
	t.Helper()
	b, err := NewBatch(numUUID(batchNum), actorID, counterBase, frontier, ops...)
	if err != nil {
		t.Fatalf("NewBatch: %v", err)
	}
	return b
}

const (
	actorMain  = uint64(0xA001)
	actorX     = uint64(0xA002)
	actorY     = uint64(0xA003)
	actorLateC = uint64(0xA004)
)

func actorID(n uint64) UUID { return numUUID(0x9000000000000000 | n) }

// closedFrontier returns fr with the writing actor's own prior end counter
// added, which §4.4 F4 requires of every conforming writer at counterBase
// > 1. Test batches that are not themselves probing F4 use it so a fixture
// defect cannot masquerade as a semantic result.
func closedFrontier(actor UUID, counterBase int64, fr map[UUID]int64) map[UUID]int64 {
	out := make(map[UUID]int64, len(fr)+1)
	for a, n := range fr {
		out[a] = n
	}
	if counterBase > CounterBaseMin {
		out[actor] = counterBase - 1
	}
	return out
}

// pos builds a write position: a reference to the batch reserving
// counterBase for actor, plus the operation's index within it (§4.6 LWW1).
func pos(counterBase int64, actor UUID, opIndex int64) position {
	return position{Ref: batchRef{Base: counterBase, Actor: actor}, OpIndex: opIndex}
}
