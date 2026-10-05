package document

import (
	"encoding/json"
	"math"
	"strings"
	"testing"
)

func TestDecodeEntityAllKinds(t *testing.T) {
	lvl := numUUID(1)
	wall := numUUID(2)
	tests := []struct {
		name string
		raw  json.RawMessage
		want Kind
	}{
		{"level", levelJSON(t, lvl, "Level 1"), KindLevel},
		{"wall", wallJSON(t, wall, lvl, 0, 2500, 4000, 2500), KindWall},
		{"wallJoin", wallJoinJSON(t, numUUID(3), wall, numUUID(4)), KindWallJoin},
		{"opening", openingJSON(t, numUUID(5), wall), KindOpening},
		{"roomRecord", roomRecordJSON(t, numUUID(6), lvl), KindRoomRecord},
		{"label", labelJSON(t, numUUID(7), lvl), KindLabel},
		{"dimension", dimensionJSON(t, numUUID(8), lvl), KindDimension},
		{"assetInstance", assetInstanceJSON(t, numUUID(9), lvl), KindAssetInstance},
	}
	for _, tc := range tests {
		ent, err := decodeEntity(tc.raw)
		if err != nil {
			t.Fatalf("%s: decodeEntity: %v", tc.name, err)
		}
		if ent.Kind() != tc.want {
			t.Fatalf("%s: kind = %q want %q", tc.name, ent.Kind(), tc.want)
		}
	}
}

func TestNegativeZeroCanonicalization(t *testing.T) {
	raw := rawMap(t, levelJSON(t, numUUID(1), "L"))
	raw["elevationMm"] = json.RawMessage("-0")
	ent, err := decodeEntity(rawJSON(t, raw))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	lvl := ent.(*Level)
	if lvl.ElevationMm != 0 || math.Signbit(lvl.ElevationMm) {
		t.Fatal("§3.2: -0 must map to +0")
	}
}

func TestSchemaRejectsTable(t *testing.T) {
	lvl := numUUID(1)
	w1 := numUUID(2)
	w2 := numUUID(3)

	mutate := func(base json.RawMessage, key string, val any) json.RawMessage {
		m := rawMap(t, base)
		if val == nil {
			m[key] = nil
		} else {
			m[key] = val
		}
		return rawJSON(t, m)
	}

	tests := []struct {
		name     string
		raw      json.RawMessage
		wantCode string
	}{
		{"level missing name", mutate(levelJSON(t, lvl, "L"), "name", nil), codeInvalidFieldSyntax},
		{"level absent name", func() json.RawMessage {
			m := rawMap(t, levelJSON(t, lvl, "L"))
			delete(m, "name")
			return rawJSON(t, m)
		}(), codeMissingRequired},
		{"level name too long", levelJSON(t, lvl, strings.Repeat("x", DisplayStringMaxBytes+1)), codeInvalidFieldSyntax},
		{"level elevation out of range", mutate(levelJSON(t, lvl, "L"), "elevationMm", 2*CoordBoundMm), codeCoordOutOfRange},
		{"level floor thickness negative", mutate(levelJSON(t, lvl, "L"), "floorThicknessMm", -1.0), codeDimOutOfRange},
		{"level floor thickness above cap", mutate(levelJSON(t, lvl, "L"), "floorThicknessMm", DimensionCapMm*2), codeDimOutOfRange},
		{"level default wall height zero", mutate(levelJSON(t, lvl, "L"), "defaultWallHeightMm", 0), codeDimOutOfRange},
		{"level default ceiling height above cap", mutate(levelJSON(t, lvl, "L"), "defaultCeilingHeightMm", DimensionCapMm*2), codeDimOutOfRange},

		{"wall zero length", wallJSON(t, w1, lvl, 100, 100, 100, 100), codeZeroLengthWall},
		{"wall start out of range", wallJSON(t, w1, lvl, -(CoordBoundMm * 2), 0, 4000, 0), codeCoordOutOfRange},
		{"wall thickness zero", mutate(wallJSON(t, w1, lvl, 0, 0, 4000, 0), "thicknessMm", 0), codeDimOutOfRange},
		{"wall height above cap", mutate(wallJSON(t, w1, lvl, 0, 0, 4000, 0), "heightMm", DimensionCapMm*2), codeDimOutOfRange},
		{"wall bad levelId syntax", mutate(wallJSON(t, w1, lvl, 0, 0, 4000, 0), "levelId", "not-a-uuid"), codeInvalidFieldSyntax},
		{"wall duplicate styleRefs", mutate(wallJSON(t, w1, lvl, 0, 0, 4000, 0), "styleRefs", []string{"S0", "S0"}), codeDuplicateElement},
		{"wall empty style token", mutate(wallJSON(t, w1, lvl, 0, 0, 4000, 0), "styleRefs", []string{""}), codeEmptyStyleToken},

		{"join single member",
			rawJSON(t, map[string]any{
				"id": numUUID(9).String(), "type": "wallJoin",
				"members": []any{map[string]any{"wallId": w1.String(), "role": "Start"}},
			}), codeInvalidMembership},
		{"join both endpoints one wall",
			rawJSON(t, map[string]any{
				"id": numUUID(10).String(), "type": "wallJoin",
				"members": []any{
					map[string]any{"wallId": w1.String(), "role": "Start"},
					map[string]any{"wallId": w1.String(), "role": "End"},
				},
			}), codeInvalidMembership},
		{"join duplicate entry",
			rawJSON(t, map[string]any{
				"id": numUUID(11).String(), "type": "wallJoin",
				"members": []any{
					map[string]any{"wallId": w1.String(), "role": "Start"},
					map[string]any{"wallId": w1.String(), "role": "Start"},
				},
			}), codeDuplicateElement},
		{"join invalid role",
			rawJSON(t, map[string]any{
				"id": numUUID(12).String(), "type": "wallJoin",
				"members": []any{
					map[string]any{"wallId": w1.String(), "role": "Middle"},
					map[string]any{"wallId": w2.String(), "role": "Start"},
				},
			}), codeInvalidMembership},
		{"join unknown nested member",
			rawJSON(t, map[string]any{
				"id": numUUID(13).String(), "type": "wallJoin",
				"members": []any{
					map[string]any{"wallId": w1.String(), "role": "Start", "extra": 1},
					map[string]any{"wallId": w2.String(), "role": "Start"},
				},
			}), codeUnknownField},

		{"opening bad kind", mutate(openingJSON(t, numUUID(14), w1), "kind", "sliding"), codeInvalidOpeningKind},
		{"opening u out of range", mutate(openingJSON(t, numUUID(14), w1), "u", 1.5), codeInvalidFieldSyntax},
		{"opening width zero", mutate(openingJSON(t, numUUID(14), w1), "widthMm", 0), codeDimOutOfRange},
		{"opening sill above cap", mutate(openingJSON(t, numUUID(14), w1), "sillMm", DimensionCapMm*2), codeDimOutOfRange},
		{"opening swing beyond 180", mutate(openingJSON(t, numUUID(14), w1), "swingDeg", 180.5), codeInvalidFieldSyntax},
		{"opening bad side", mutate(openingJSON(t, numUUID(14), w1), "side", "Center"), codeInvalidSide},
		{"opening bad hinge", mutate(openingJSON(t, numUUID(14), w1), "hinge", "middle"), codeInvalidHinge},
		{"opening bad opensToward", mutate(openingJSON(t, numUUID(14), w1), "opensToward", "Up"), codeInvalidOpensToward},

		{"room seed out of range", mutate(roomRecordJSON(t, numUUID(15), lvl), "seedMm", []float64{CoordBoundMm * 2, 0}), codeCoordOutOfRange},
		{"room name too long", mutate(roomRecordJSON(t, numUUID(15), lvl), "name", strings.Repeat("y", DisplayStringMaxBytes+1)), codeInvalidFieldSyntax},
		{"room empty style ref", mutate(roomRecordJSON(t, numUUID(15), lvl), "floorStyleRef", ""), codeEmptyStyleToken},

		{"label anchor out of range", mutate(labelJSON(t, numUUID(16), lvl), "anchorMm", []float64{0, CoordBoundMm * 3}), codeCoordOutOfRange},
		{"dimension override too long", mutate(dimensionJSON(t, numUUID(17), lvl), "labelOverride", strings.Repeat("z", DisplayStringMaxBytes+1)), codeInvalidFieldSyntax},

		{"asset digest wrong prefix", mutate(assetInstanceJSON(t, numUUID(18), lvl), "digest", "md5:"+strings.Repeat("a", 64)), codeInvalidDigest},
		{"asset digest uppercase", mutate(assetInstanceJSON(t, numUUID(18), lvl), "digest", strings.ToUpper(fmtDigest(numUUID(18)))), codeInvalidDigest},
		{"asset transform missing rotationDeg",
			mutate(assetInstanceJSON(t, numUUID(18), lvl), "transform", map[string]any{
				"translationMm": []float64{1, 2}, "scaleUniform": 1,
			}), codeMissingRequired},
		{"asset transform scale zero", mutate(assetInstanceJSON(t, numUUID(18), lvl), "transform", map[string]any{
			"translationMm": []float64{1, 2}, "scaleUniform": 0, "rotationDeg": 0,
		}), codeDimOutOfRange},
		{"asset transform unknown member", mutate(assetInstanceJSON(t, numUUID(18), lvl), "transform", map[string]any{
			"translationMm": []float64{1, 2}, "scaleUniform": 1, "rotationDeg": 0, "skew": 3,
		}), codeUnknownField},

		{"unknown entity kind refuses safely",
			rawJSON(t, map[string]any{"id": numUUID(19).String(), "type": "gaussianSplat", "payload": 1}),
			codeUnknownEntityKind},
		{"missing id",
			rawJSON(t, map[string]any{"type": "level", "name": "L", "elevationMm": 0,
				"floorThicknessMm": 200, "defaultWallHeightMm": 2500, "defaultCeilingHeightMm": 2500}),
			codeMissingRequired},
		{"required field null", mutate(roomRecordJSON(t, numUUID(20), lvl), "seedMm", nil), codeInvalidFieldSyntax},
	}
	for _, tc := range tests {
		if tc.wantCode == "" {
			continue // covered by dedicated tests below
		}
		_, err := decodeEntity(tc.raw)
		if err == nil {
			t.Fatalf("%s: decode accepted invalid entity", tc.name)
		}
		fe, ok := err.(*FieldError)
		if !ok {
			t.Fatalf("%s: error is %T (%v), want *FieldError", tc.name, err, err)
		}
		if fe.Code != tc.wantCode {
			t.Fatalf("%s: code = %q want %q (detail: %s)", tc.name, fe.Code, tc.wantCode, fe.Detail)
		}
	}
}

func fmtDigest(u UUID) string {
	const hexDigits = "0123456789abcdef"
	out := make([]byte, 0, 71)
	out = append(out, "sha256:"...)
	v := ntoaID(u)
	for i := 0; i < 64; i++ {
		nibble := byte(v>>(uint(i%16)*4))&0xF ^ byte(i%7)
		out = append(out, hexDigits[nibble&0xF])
	}
	return string(out)
}

func TestStyleRefSetCanonicalization(t *testing.T) {
	base := wallJSON(t, numUUID(2), numUUID(1), 0, 0, 4000, 0)
	m := rawMap(t, base)
	m["styleRefs"] = []string{"Zed", "Alpha", "mid"}
	ent, err := decodeEntity(rawJSON(t, m))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	got := ent.(*Wall).StyleRefs
	want := []string{"Alpha", "Zed", "mid"}
	if len(got) != len(want) {
		t.Fatalf("len = %d", len(got))
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("canonical order: got %v want %v", got, want)
		}
	}
}

func TestBackgroundImagesCanonicalOrderAndStrictness(t *testing.T) {
	dig := func(n uint64) string { return fmtDigest(numUUID(n)) }
	img := func(digest string, ord int64, x, y, s, r float64) map[string]any {
		return map[string]any{
			"digest": digest,
			"anchor": map[string]any{
				"translationMm": []float64{x, y}, "scaleUniform": s, "rotationDeg": r,
			},
			"stackOrdinal": ord,
		}
	}
	base := levelJSON(t, numUUID(1), "L")
	m := rawMap(t, base)
	// Deliberately unsorted; canonical order is the C3 tuple.
	m["backgroundImages"] = []any{
		img(dig(2), 0, 0, 0, 1, 0),
		img(dig(1), 5, 9, 9, 1, 0),
		img(dig(1), 1, 0, 0, 1, 0),
	}
	ent, err := decodeEntity(rawJSON(t, m))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	imgs := ent.(*Level).BackgroundImages
	wantOrder := []string{dig(1), dig(1), dig(2)}
	for i, d := range wantOrder {
		if imgs[i].Digest != d {
			t.Fatalf("position %d: digest %q want %q", i, imgs[i].Digest, d)
		}
	}
	if imgs[0].StackOrdinal != 1 || imgs[1].StackOrdinal != 5 {
		t.Fatal("digest ties must break by stackOrdinal")
	}

	// Duplicate elements reject deterministically.
	m["backgroundImages"] = []any{img(dig(1), 0, 1, 2, 3, 4), img(dig(1), 0, 1, 2, 3, 4)}
	if _, err := decodeEntity(rawJSON(t, m)); err == nil {
		t.Fatal("duplicate backgroundImages entry accepted")
	}

	broken := img(dig(1), 0, 1, 2, 3, 4)
	delete(broken["anchor"].(map[string]any), "rotationDeg")
	m["backgroundImages"] = []any{broken}
	if _, err := decodeEntity(rawJSON(t, m)); err == nil {
		t.Fatal("missing anchor member accepted")
	} else {
		mustFieldErr(t, err, codeMissingRequired)
	}

	// Unknown anchor member refuses safely.
	broken = img(dig(1), 0, 1, 2, 3, 4)
	broken["anchor"].(map[string]any)["flip"] = true
	m["backgroundImages"] = []any{broken}
	if _, err := decodeEntity(rawJSON(t, m)); err == nil {
		t.Fatal("unknown anchor member accepted")
	} else {
		mustFieldErr(t, err, codeUnknownField)
	}
}

func TestUnknownTopLevelFieldsPreservedSortedReadOnly(t *testing.T) {
	base := roomRecordJSON(t, numUUID(6), numUUID(1))
	m := rawMap(t, base)
	m["zCustom"] = map[string]any{"b": 2}
	m["aCustom"] = json.RawMessage(`{"n":1}`)
	ent, err := decodeEntity(rawJSON(t, m))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	unk := ent.UnknownFields()
	if len(unk) != 2 || unk[0].Name != "aCustom" || unk[1].Name != "zCustom" {
		t.Fatalf("preserved fields not sorted/complete: %v", unk)
	}
	if string(unk[0].Value) != `{"n":1}` {
		t.Fatalf("opaque bytes altered: %s", unk[0].Value)
	}
	// Read-only: setFields cannot address preserved fields.
	clone := cloneEntity(ent)
	err = applySetFields(clone, map[string]json.RawMessage{
		"aCustom": json.RawMessage(`{"n":9}`),
	})
	mustFieldErr(t, err, codeUnknownField)
}

func TestApplySetFieldsNullSemantics(t *testing.T) {
	lvl := numUUID(1)
	base := roomRecordJSON(t, numUUID(6), lvl)
	m := rawMap(t, base)
	m["floorStyleRef"] = "F0"
	m["ceilingStyleRef"] = "C0"
	ent, err := decodeEntity(rawJSON(t, m))
	if err != nil {
		t.Fatalf("decode: %v", err)
	}
	clone := cloneEntity(ent)
	if err := applySetFields(clone, map[string]json.RawMessage{
		FieldName:          json.RawMessage(`null`),
		FieldFloorStyleRef: json.RawMessage(`null`),
	}); err != nil {
		t.Fatalf("apply null clears: %v", err)
	}
	rr := clone.(*RoomRecord)
	if rr.Name != nil {
		t.Fatal("optional absent-vs-empty: null must clear to absent")
	}
	if rr.FloorStyleRef != nil {
		t.Fatal("floorStyleRef should be cleared by null") //nolint:staticcheck // distinct optionals
	}
	// Absent-and-empty distinction survives a set to empty string.
	if err := applySetFields(clone, map[string]json.RawMessage{
		FieldName: json.RawMessage(`""`),
	}); err != nil {
		t.Fatalf("set empty name: %v", err)
	}
	if clone.(*RoomRecord).Name == nil || *clone.(*RoomRecord).Name != "" {
		t.Fatal("empty-string name must be present-but-empty")
	}
	// Null against required field rejects.
	err = applySetFields(clone, map[string]json.RawMessage{FieldSeedMm: json.RawMessage(`null`)})
	mustFieldErr(t, err, codeMissingRequired)
	// Wholesale replacement of a set-valued field.
	wallEnt, err := decodeEntity(wallJSON(t, numUUID(2), lvl, 0, 0, 4000, 0))
	if err != nil {
		t.Fatalf("decode wall: %v", err)
	}
	wc := cloneEntity(wallEnt)
	if err := applySetFields(wc, map[string]json.RawMessage{
		FieldStyleRefs: json.RawMessage(`["B","A"]`),
	}); err != nil {
		t.Fatalf("set styleRefs: %v", err)
	}
	if got := wc.(*Wall).StyleRefs; len(got) != 2 || got[0] != "A" || got[1] != "B" {
		t.Fatalf("wholesale set must store canonical order: %v", got)
	}
}

func mustFieldErr(t testing.TB, err error, code string) error {
	t.Helper()
	if err == nil {
		t.Fatalf("expected FieldError %q, got nil", code)
	}
	fe, ok := err.(*FieldError)
	if !ok {
		t.Fatalf("expected *FieldError, got %T: %v", err, err)
	}
	if fe.Code != code {
		t.Fatalf("code = %q want %q", fe.Code, code)
	}
	return fe
}
