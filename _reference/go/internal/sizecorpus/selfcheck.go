package sizecorpus

import (
	"encoding/json"
	"fmt"

	"domowik.dev/domowik/internal/document"
)

// SelfCheck implements the ASSET-STRATEGY §10.9 normative generator
// self-check: every entity is validated against the R3.*, R4.*, R5.*/§2
// rule sets (wire shapes per FORMAT-SPEC §2.3), openings abort with
// `missing-opening-field:<field>` when any R4.0.5-authoritative field is
// absent, style tokens must resolve against the ED-0025 generator-local
// vocabulary, independent geometry properties are asserted from the emitted
// coordinates, totals must match the scenario definition exactly, and the
// delivered projection state must account for every operation. Any violation
// aborts naming the entity path.
func SelfCheck(d *Document) error {
	if err := SelfCheckRNGVector(); err != nil {
		return err
	}
	if err := checkTotals(d); err != nil {
		return err
	}
	ids := make(map[string]string, len(d.Objects)) // id string -> path
	for i := range d.Objects {
		o := &d.Objects[i]
		var obj map[string]json.RawMessage
		if err := json.Unmarshal(o.JSON, &obj); err != nil {
			return fmt.Errorf("entity %s: malformed object: %w", o.Path, err)
		}
		id, err := jsonString(obj, "id")
		if err != nil {
			return fmt.Errorf("entity %s: %v", o.Path, err)
		}
		if prev, dup := ids[id]; dup {
			return fmt.Errorf("entity %s: duplicate entity id also used by %s", o.Path, prev)
		}
		ids[id] = o.Path
		if err := checkStyleTokens(o.Path, obj); err != nil {
			return err
		}
		switch o.Kind {
		case document.KindLevel:
			if err := checkLevel(o.Path, obj); err != nil {
				return err
			}
		case document.KindWall:
			if err := checkWall(o.Path, obj); err != nil {
				return err
			}
		case document.KindWallJoin:
			if err := checkJoin(o.Path, obj); err != nil {
				return err
			}
		case document.KindOpening:
			if err := checkOpeningFields(o.Path, obj); err != nil {
				return err
			}
		case document.KindRoomRecord:
			if err := checkRoom(o.Path, obj); err != nil {
				return err
			}
		default:
			return fmt.Errorf("entity %s: unexpected kind %q", o.Path, o.Kind)
		}
	}
	if err := checkReferences(d); err != nil {
		return err
	}
	if err := checkIdentities(d); err != nil {
		return err
	}
	if err := checkGeometry(d); err != nil {
		return err
	}
	if err := checkOpeningPlacement(d); err != nil {
		return err
	}
	return checkDeliveredState(d)
}

// checkDeliveredState verifies the projection state accounts for every
// generated operation exactly once: one live entity per object, one creation
// key each, no tombstones, and the actor watermark at the operation count.
func checkDeliveredState(d *Document) error {
	total := d.Definition.Totals.TotalEntities()
	if len(d.State.Entities) != total {
		return fmt.Errorf("delivered entity count %d != definition total %d", len(d.State.Entities), total)
	}
	if len(d.State.CreatedPositions) != total {
		return fmt.Errorf("delivered creation positions %d != %d", len(d.State.CreatedPositions), total)
	}
	if len(d.State.Tombstones) != 0 {
		return fmt.Errorf("corpus carries no deletions but %d tombstones exist", len(d.State.Tombstones))
	}
	w, ok := d.State.Frontier[d.ActorID]
	if !ok || w != int64(d.OpCount) {
		return fmt.Errorf("actor watermark %d (present=%t) != operation count %d", w, ok, d.OpCount)
	}
	return nil
}

func checkTotals(d *Document) error {
	got := Totals{}
	for i := range d.Objects {
		switch d.Objects[i].Kind {
		case document.KindLevel:
			got.Levels++
		case document.KindWall:
			got.Walls++
		case document.KindWallJoin:
			got.Joins++
		case document.KindOpening:
			got.Openings++
		case document.KindRoomRecord:
			got.Rooms++
		}
	}
	want := d.Definition.Totals
	if got != want {
		return fmt.Errorf("totals mismatch: got %+v want %+v", got, want)
	}
	if got.TotalEntities() != len(d.Objects) {
		return fmt.Errorf("totals mismatch: %d kinds sum vs %d objects", got.TotalEntities(), len(d.Objects))
	}
	return nil
}

// R4.0.5 authoritative opening fields (BEHAVIOR-SPEC §4); all are mandatory
// literals in this corpus, so absence aborts generation.
var r405OpeningFields = []string{
	"kind", "hostWallId", "u", "widthMm", "heightMm", "sillMm", "depthMm",
	"side", "hinge", "opensToward", "swingDeg", "styleRefs",
}

func checkOpeningFields(path string, obj map[string]json.RawMessage) error {
	present := map[string]bool{}
	for _, f := range r405OpeningFields {
		raw, ok := obj[f]
		if !ok {
			return fmt.Errorf("missing-opening-field:%s (entity %s)", f, path)
		}
		present[f] = true
		_ = raw
	}
	kind, _ := jsonString(obj, "kind")
	if kind != "door" && kind != "window" {
		return fmt.Errorf("entity %s: invalid-opening-kind %q", path, kind)
	}
	u, err := jsonNumber(obj, "u")
	if err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	}
	if u < 0 || u > 1 {
		return fmt.Errorf("entity %s: u outside [0,1]", path)
	}
	side, _ := jsonString(obj, "side")
	toward, _ := jsonString(obj, "opensToward")
	hinge, _ := jsonString(obj, "hinge")
	if side != "Left" && side != "Right" {
		return fmt.Errorf("entity %s: invalid-side %q", path, side)
	}
	if toward != "Left" && toward != "Right" {
		return fmt.Errorf("entity %s: invalid-opens-toward %q", path, toward)
	}
	if hinge != "nearStart" && hinge != "nearEnd" {
		return fmt.Errorf("entity %s: invalid-hinge %q", path, hinge)
	}
	swing, err := jsonNumber(obj, "swingDeg")
	if err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	}
	if swing < 0 || swing > 180 {
		return fmt.Errorf("entity %s: invalid-swing-angle %v", path, swing)
	}
	for _, f := range []string{"widthMm", "heightMm", "depthMm"} {
		v, err := jsonNumber(obj, f)
		if err != nil {
			return fmt.Errorf("entity %s: %v", path, err)
		}
		if v <= 0 {
			return fmt.Errorf("entity %s: dimension-out-of-range %s=%v", path, f, v)
		}
	}
	if sill, err := jsonNumber(obj, "sillMm"); err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	} else if sill < 0 {
		return fmt.Errorf("entity %s: dimension-out-of-range sillMm=%v", path, sill)
	}
	_ = present
	return nil
}

func checkLevel(path string, obj map[string]json.RawMessage) error {
	name, err := jsonString(obj, "name")
	if err != nil {
		return fmt.Errorf("entity %s: missing-required-field name", path)
	}
	if name == "" {
		return fmt.Errorf("entity %s: empty level name", path)
	}
	if _, ok := obj["backgroundImages"]; ok {
		return fmt.Errorf("entity %s: background anchor must be absent in Part A corpus", path)
	}
	for _, f := range []string{"elevationMm", "floorThicknessMm", "defaultWallHeightMm", "defaultCeilingHeightMm"} {
		if _, ok := obj[f]; !ok {
			return fmt.Errorf("entity %s: missing-required-field %s", path, f)
		}
	}
	ft, err := jsonNumber(obj, "floorThicknessMm")
	if err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	}
	if ft < 0 {
		return fmt.Errorf("entity %s: dimension-out-of-range floorThicknessMm", path)
	}
	for _, f := range []string{"defaultWallHeightMm", "defaultCeilingHeightMm"} {
		if v, _ := jsonNumber(obj, f); v <= 0 {
			return fmt.Errorf("entity %s: dimension-out-of-range %s", path, f)
		}
	}
	return nil
}

func checkWall(path string, obj map[string]json.RawMessage) error {
	for _, f := range []string{"levelId", "startMm", "endMm", "thicknessMm", "heightMm"} {
		if _, ok := obj[f]; !ok {
			return fmt.Errorf("entity %s: missing-required-field %s", path, f)
		}
	}
	start, err := coordPair(obj, "startMm")
	if err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	}
	end, err := coordPair(obj, "endMm")
	if err != nil {
		return fmt.Errorf("entity %s: %v", path, err)
	}
	if start == end {
		return fmt.Errorf("entity %s: zero-length-wall", path)
	}
	if start.x != end.x && start.y != end.y {
		return fmt.Errorf("entity %s: corpus walls are axis-aligned", path)
	}
	return nil
}

func checkJoin(path string, obj map[string]json.RawMessage) error {
	raw, ok := obj["members"]
	if !ok {
		return fmt.Errorf("entity %s: missing-required-field members", path)
	}
	var members []struct {
		WallID string `json:"wallId"`
		Role   string `json:"role"`
	}
	if err := json.Unmarshal(raw, &members); err != nil {
		return fmt.Errorf("entity %s: malformed members: %w", path, err)
	}
	if len(members) < 2 {
		return fmt.Errorf("entity %s: wallJoin needs at least two members", path)
	}
	seen := map[string]bool{}
	for _, m := range members {
		if m.WallID == "" {
			return fmt.Errorf("entity %s: member without wallId", path)
		}
		if seen[m.WallID] {
			return fmt.Errorf("entity %s: both endpoints of one wall prohibited", path)
		}
		seen[m.WallID] = true
		if m.Role != "Start" && m.Role != "End" {
			return fmt.Errorf("entity %s: invalid member role %q", path, m.Role)
		}
	}
	return nil
}

func checkRoom(path string, obj map[string]json.RawMessage) error {
	if _, ok := obj["levelId"]; !ok {
		return fmt.Errorf("entity %s: missing-required-field levelId", path)
	}
	if _, ok := obj["seedMm"]; !ok {
		return fmt.Errorf("entity %s: missing-required-field seedMm", path)
	}
	// The corpus pins rooms without optional display/style fields (§10.9;
	// R5.3.4 keeps references unresolvable-in-v0.1 anyway).
	for _, f := range []string{"name", "floorStyleRef", "ceilingStyleRef"} {
		if _, ok := obj[f]; ok {
			return fmt.Errorf("entity %s: optional field %s must stay omitted in Part A corpus", path, f)
		}
	}
	return nil
}

// checkStyleTokens enforces the ED-0025 generator-local vocabulary exactly:
// every serialized styleRefs value must be the single non-empty resolvable
// token "S0", never an empty set or any other token.
func checkStyleTokens(path string, obj map[string]json.RawMessage) error {
	raw, ok := obj["styleRefs"]
	if !ok {
		return nil // kinds without optional styles (levels, joins, rooms omit them)
	}
	var toks []string
	if err := json.Unmarshal(raw, &toks); err != nil {
		return fmt.Errorf("entity %s: malformed styleRefs", path)
	}
	if len(toks) != 1 || toks[0] != styleVocabulary {
		return fmt.Errorf("entity %s: styleRefs %v outside the ED-0025 corpus vocabulary [\"%s\"]", path, toks, styleVocabulary)
	}
	return nil
}

// checkIdentities verifies every generated identifier against the
// independent reference oracle (identity_oracle.go): entity ids, operation
// ids, actor id, and batch id — the complete set, not samples. It never
// reuses the generator helper TagUUID, so a defect in that helper's hash
// domain, input encoding, or E4 bit layout is detectable. Malformed
// Documents produce named errors, never panics.
func checkIdentities(d *Document) error {
	seed, err := seedBytes(d.Definition.Name)
	if err != nil {
		return err
	}
	if len(d.OpIDs) != len(d.Objects) {
		return fmt.Errorf("operation identity count %d does not match object count %d",
			len(d.OpIDs), len(d.Objects))
	}
	refActor, err := deriveIdentityReference(seed, "actor/create")
	if err != nil {
		return fmt.Errorf("actor identity reference: %v", err)
	}
	if d.ActorID != refActor {
		return fmt.Errorf("actor identity mismatch: serialized %s but independent derivation gives %s", d.ActorID, refActor)
	}
	refBatch, err := deriveIdentityReference(seed, "batch/create")
	if err != nil {
		return fmt.Errorf("batch identity reference: %v", err)
	}
	if d.BatchID != refBatch {
		return fmt.Errorf("batch identity mismatch: serialized %s but independent derivation gives %s", d.BatchID, refBatch)
	}
	for i := range d.Objects {
		o := &d.Objects[i]
		var obj map[string]json.RawMessage
		if err := json.Unmarshal(o.JSON, &obj); err != nil {
			return err
		}
		idText, err := jsonString(obj, "id")
		if err != nil {
			return fmt.Errorf("entity %s: %v", o.Path, err)
		}
		id, err := document.ParseUUID(idText)
		if err != nil {
			return fmt.Errorf("entity %s: unparsable identity %q: %v", o.Path, idText, err)
		}
		refEntity, err := deriveIdentityReference(seed, o.Path)
		if err != nil {
			return fmt.Errorf("entity %s: reference derivation failed: %v", o.Path, err)
		}
		if id != refEntity {
			return fmt.Errorf("entity %s: identity mismatch: serialized %s but independent derivation gives %s", o.Path, id, refEntity)
		}
		refOp, err := deriveIdentityReference(seed, "op/create/"+o.Path)
		if err != nil {
			return fmt.Errorf("entity %s: operation reference derivation failed: %v", o.Path, err)
		}
		if d.OpIDs[i] != refOp {
			return fmt.Errorf("entity %s: operation identity mismatch: derived %s but independent derivation gives %s",
				o.Path, d.OpIDs[i], refOp)
		}
	}
	return nil
}

// checkReferences validates referential integrity across the object set.
func checkReferences(d *Document) error {
	levelIDs := map[string]string{}
	wallIDs := map[string]string{}
	for i := range d.Objects {
		o := &d.Objects[i]
		var obj map[string]json.RawMessage
		if err := json.Unmarshal(o.JSON, &obj); err != nil {
			return err
		}
		switch o.Kind {
		case document.KindLevel:
			id, _ := jsonString(obj, "id")
			levelIDs[id] = o.Path
		case document.KindWall:
			id, _ := jsonString(obj, "id")
			wallIDs[id] = o.Path
		}
	}
	ref := func(field string, known map[string]string) func(o *EntityObject, obj map[string]json.RawMessage) error {
		return func(o *EntityObject, obj map[string]json.RawMessage) error {
			v, err := jsonString(obj, field)
			if err != nil {
				return fmt.Errorf("entity %s: missing-required-field %s", o.Path, field)
			}
			if _, ok := known[v]; !ok {
				return fmt.Errorf("entity %s: %s names no known entity", o.Path, field)
			}
			return nil
		}
	}
	checks := map[document.Kind][]struct {
		field string
		known map[string]string
	}{}
	add := func(k document.Kind, field string, known map[string]string) {
		checks[k] = append(checks[k], struct {
			field string
			known map[string]string
		}{field, known})
	}
	add(document.KindWall, "levelId", levelIDs)
	add(document.KindRoomRecord, "levelId", levelIDs)
	add(document.KindOpening, "hostWallId", wallIDs)
	for i := range d.Objects {
		o := &d.Objects[i]
		for _, c := range checks[o.Kind] {
			var obj map[string]json.RawMessage
			if err := json.Unmarshal(o.JSON, &obj); err != nil {
				return err
			}
			if err := ref(c.field, c.known)(o, obj); err != nil {
				return err
			}
		}
	}
	// Join membership references walls.
	for i := range d.Objects {
		o := &d.Objects[i]
		if o.Kind != document.KindWallJoin {
			continue
		}
		var obj struct {
			Members []struct {
				WallID string `json:"wallId"`
			} `json:"members"`
		}
		if err := json.Unmarshal(o.JSON, &obj); err != nil {
			return err
		}
		for _, m := range obj.Members {
			if _, ok := wallIDs[m.WallID]; !ok {
				return fmt.Errorf("entity %s: member names no known wall", o.Path)
			}
		}
	}
	return nil
}

// pointValue decodes a coordinate pair.
type pointValue struct{ x, y float64 }

func coordPair(obj map[string]json.RawMessage, field string) (pointValue, error) {
	raw, ok := obj[field]
	if !ok {
		return pointValue{}, fmt.Errorf("missing-required-field %s", field)
	}
	var pair [2]float64
	if err := json.Unmarshal(raw, &pair); err != nil {
		return pointValue{}, fmt.Errorf("malformed %s", field)
	}
	return pointValue{x: pair[0], y: pair[1]}, nil
}

func jsonString(obj map[string]json.RawMessage, field string) (string, error) {
	raw, ok := obj[field]
	if !ok {
		return "", fmt.Errorf("missing-required-field %s", field)
	}
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return "", fmt.Errorf("%s is not a string", field)
	}
	return s, nil
}

func jsonNumber(obj map[string]json.RawMessage, field string) (float64, error) {
	raw, ok := obj[field]
	if !ok {
		return 0, fmt.Errorf("missing-required-field %s", field)
	}
	var n float64
	if err := json.Unmarshal(raw, &n); err != nil {
		return 0, fmt.Errorf("%s is not a number", field)
	}
	return n, nil
}
