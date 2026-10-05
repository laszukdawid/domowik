package document

import (
	"encoding/json"
	"fmt"
	"sort"
)

// FieldError names the user-fixable cause of a schema rejection. Codes use
// the diagnostic vocabulary of FORMAT-SPEC §9.3 and BEHAVIOR-SPEC where
// ratified; proposed tokens (`coordinate-out-of-range`,
// `dimension-out-of-range`) are recorded as proposals per FORMAT-SPEC
// §3.1 and are not commitments.
type FieldError struct {
	Field  string
	Code   string
	Detail string
}

func (e *FieldError) Error() string {
	if e.Field == "" {
		return fmt.Sprintf("%s: %s", e.Code, e.Detail)
	}
	return fmt.Sprintf("%s (%s): %s", e.Code, e.Field, e.Detail)
}

func fieldErr(field, code, format string, args ...any) *FieldError {
	return &FieldError{Field: field, Code: code, Detail: fmt.Sprintf(format, args...)}
}

// Diagnostic codes.
const (
	codeInvalidFieldSyntax = "invalid-field-syntax"
	codeNonfiniteValue     = "nonfinite-value"
	codeNumericOverflow    = "numeric-overflow"
	codeCoordOutOfRange    = "coordinate-out-of-range"
	codeDimOutOfRange      = "dimension-out-of-range"
	codeZeroLengthWall     = "zero-length-wall"
	codeInvalidOpeningKind = "invalid-opening-kind"
	codeInvalidSide        = "invalid-side"
	codeInvalidHinge       = "invalid-hinge"
	codeInvalidOpensToward = "invalid-opens-toward"
	codeMissingRequired    = "missing-required-field"
	codeUnknownField       = "unknown-field"
	codeUnknownEntityKind  = "unknown-entity-kind"
	codeUnknownEntityType  = "unknown-entity-type"
	codeInvalidDigest      = "invalid-digest-syntax"
	codeDuplicateElement   = "duplicate-set-element"
	codeInvalidMembership  = "invalid-membership"
	codeEmptyStyleToken    = "invalid-style-token"
)

// decodeEntity validates one complete entity object per FORMAT-SPEC §2.3
// and returns its typed form. Unknown entity kinds fail closed: an
// authoritative create request whose required data is unrecognized refuses
// safely (§2.3/§8); opaque preservation applies only to unknown OPTIONAL
// data on known kinds. Unknown top-level fields on known kinds are
// preserved opaquely and are read-only thereafter.
func decodeEntity(raw json.RawMessage) (Entity, error) {
	var obj map[string]json.RawMessage
	if err := json.Unmarshal(raw, &obj); err != nil {
		return nil, fieldErr("", codeInvalidFieldSyntax, "entity object malformed: %v", err)
	}
	idRaw, ok := obj[fieldID]
	if !ok {
		return nil, fieldErr(fieldID, codeMissingRequired, "entity id absent")
	}
	var idText string
	if err := json.Unmarshal(idRaw, &idText); err != nil {
		return nil, fieldErr(fieldID, codeInvalidFieldSyntax, "entity id must be a string")
	}
	id, err := ParseUUID(idText)
	if err != nil {
		return nil, fieldErr(fieldID, codeInvalidFieldSyntax, "%q: %v", idText, err)
	}
	typeRaw, ok := obj[fieldType]
	if !ok {
		return nil, fieldErr(fieldType, codeMissingRequired, "entity type absent")
	}
	var kindText string
	if err := json.Unmarshal(typeRaw, &kindText); err != nil {
		return nil, fieldErr(fieldType, codeInvalidFieldSyntax, "entity type must be a string")
	}
	kind := Kind(kindText)

	base := entityBase{id: id}
	delete(obj, fieldID)
	delete(obj, fieldType)

	var ent Entity
	switch kind {
	case KindLevel:
		e := &Level{entityBase: base}
		err = decodeLevel(e, obj)
		ent = e
	case KindWall:
		e := &Wall{entityBase: base}
		err = decodeWall(e, obj)
		ent = e
	case KindWallJoin:
		e := &WallJoin{entityBase: base}
		err = decodeWallJoin(e, obj)
		ent = e
	case KindOpening:
		e := &Opening{entityBase: base}
		err = decodeOpening(e, obj)
		ent = e
	case KindRoomRecord:
		e := &RoomRecord{entityBase: base}
		err = decodeRoomRecord(e, obj)
		ent = e
	case KindLabel:
		e := &Label{entityBase: base}
		err = decodeLabel(e, obj)
		ent = e
	case KindDimension:
		e := &Dimension{entityBase: base}
		err = decodeDimension(e, obj)
		ent = e
	case KindAssetInstance:
		e := &AssetInstance{entityBase: base}
		err = decodeAssetInstance(e, obj)
		ent = e
	default:
		return nil, fieldErr(fieldType, codeUnknownEntityKind, "kind %q is unrecognized; refusing safely", kindText)
	}
	if err != nil {
		return nil, err
	}
	// Only fields outside the kind's schema are unknown data (§8); every
	// schema field consumed above must not leak into the opaque bag.
	for _, name := range schemaFieldNames(kind) {
		delete(obj, name)
	}
	for name, value := range obj {
		base.unknown = append(base.unknown, UnknownField{Name: name, Value: value})
	}
	sort.Slice(base.unknown, func(i, j int) bool { return base.unknown[i].Name < base.unknown[j].Name })
	setUnknown(ent, base.unknown)
	return ent, nil
}

func setUnknown(e Entity, unknown []UnknownField) {
	switch t := e.(type) {
	case *Level:
		t.unknown = unknown
	case *Wall:
		t.unknown = unknown
	case *WallJoin:
		t.unknown = unknown
	case *Opening:
		t.unknown = unknown
	case *RoomRecord:
		t.unknown = unknown
	case *Label:
		t.unknown = unknown
	case *Dimension:
		t.unknown = unknown
	case *AssetInstance:
		t.unknown = unknown
	}
}

// schemaFieldNames lists every serialized field name of the kind's §2.3
// schema, required and optional alike.
func schemaFieldNames(k Kind) []string {
	switch k {
	case KindLevel:
		return []string{FieldName, FieldElevationMm, FieldFloorThickness, FieldDefaultWallH, FieldDefaultCeilH, FieldBackgroundImgs}
	case KindWall:
		return []string{fieldLevelID, FieldStartMm, FieldEndMm, FieldThicknessMm, FieldHeightMm, FieldStyleRefs}
	case KindWallJoin:
		return []string{FieldMembers}
	case KindOpening:
		return []string{FieldKind, fieldHostWall, FieldU, FieldWidthMm, FieldHeightMm, FieldSillMm, FieldDepthMm,
			FieldSide, FieldHinge, FieldOpensToward, FieldSwingDeg, FieldStyleRefs}
	case KindRoomRecord:
		return []string{fieldLevelID, FieldSeedMm, FieldName, FieldFloorStyleRef, FieldCeilingStyleRef}
	case KindLabel:
		return []string{fieldLevelID, FieldAnchorMm, FieldName}
	case KindDimension:
		return []string{fieldLevelID, FieldStartMm, FieldEndMm, FieldLabelOverride}
	case KindAssetInstance:
		return []string{fieldLevelID, FieldDigest, FieldTransform, FieldDisplayName}
	default:
		return nil
	}
}

// The per-kind decoders share a uniform contract: every known field is
// validated at decode time (per-op invariant boundary); leftover map keys
// remain for opaque preservation.

func decodeLevel(e *Level, obj map[string]json.RawMessage) error {
	var err error
	if e.Name, err = reqDisplayString(obj, FieldName); err != nil {
		return err
	}
	if e.ElevationMm, err = reqCoord(obj, FieldElevationMm); err != nil {
		return err
	}
	if e.FloorThicknessMm, err = reqDimNonNeg(obj, FieldFloorThickness); err != nil {
		return err
	}
	if e.DefaultWallHeightMm, err = reqDimPositive(obj, FieldDefaultWallH); err != nil {
		return err
	}
	if e.DefaultCeilingHeightMm, err = reqDimPositive(obj, FieldDefaultCeilH); err != nil {
		return err
	}
	if _, ok := obj[FieldBackgroundImgs]; ok {
		imgs, err := decodeBackgroundImages(obj[FieldBackgroundImgs])
		if err != nil {
			return wrapField(FieldBackgroundImgs, err)
		}
		e.BackgroundImages = imgs
	}
	return nil
}

func decodeWall(e *Wall, obj map[string]json.RawMessage) error {
	var err error
	if e.LevelID, err = reqUUIDRef(obj, fieldLevelID); err != nil {
		return err
	}
	if e.StartMm, err = reqCoordPair(obj, FieldStartMm); err != nil {
		return err
	}
	if e.EndMm, err = reqCoordPair(obj, FieldEndMm); err != nil {
		return err
	}
	if e.StartMm == e.EndMm {
		return fieldErr(FieldStartMm, codeZeroLengthWall, "startMm and endMm must differ")
	}
	if e.ThicknessMm, err = reqDimPositive(obj, FieldThicknessMm); err != nil {
		return err
	}
	if e.HeightMm, err = reqDimPositive(obj, FieldHeightMm); err != nil {
		return err
	}
	if _, ok := obj[FieldStyleRefs]; ok {
		refs, err := decodeStyleRefs(obj[FieldStyleRefs])
		if err != nil {
			return wrapField(FieldStyleRefs, err)
		}
		e.StyleRefs = refs
	}
	return nil
}

func decodeWallJoin(e *WallJoin, obj map[string]json.RawMessage) error {
	members, err := reqMembers(obj, FieldMembers)
	if err != nil {
		return err
	}
	e.Members = members
	return nil
}

func decodeOpening(e *Opening, obj map[string]json.RawMessage) error {
	var err error
	if e.KindField, err = reqOpeningKind(obj, FieldKind); err != nil {
		return err
	}
	if e.HostWallID, err = reqUUIDRef(obj, fieldHostWall); err != nil {
		return err
	}
	if e.U, err = reqUnitInterval(obj, FieldU); err != nil {
		return err
	}
	if e.WidthMm, err = reqDimPositive(obj, FieldWidthMm); err != nil {
		return err
	}
	if e.HeightMm, err = reqDimPositive(obj, FieldHeightMm); err != nil {
		return err
	}
	if e.SillMm, err = reqDimNonNeg(obj, FieldSillMm); err != nil {
		return err
	}
	if e.DepthMm, err = reqDimPositive(obj, FieldDepthMm); err != nil {
		return err
	}
	if e.Side, err = reqSide(obj, FieldSide); err != nil {
		return err
	}
	if e.Hinge, err = reqHinge(obj, FieldHinge); err != nil {
		return err
	}
	if e.OpensToward, err = reqSide(obj, FieldOpensToward); err != nil {
		if fe, ok := err.(*FieldError); ok {
			fe.Code = codeInvalidOpensToward
		}
		return err
	}
	if e.SwingDeg, err = reqSwingDeg(obj, FieldSwingDeg); err != nil {
		return err
	}
	if _, ok := obj[FieldStyleRefs]; ok {
		refs, err := decodeStyleRefs(obj[FieldStyleRefs])
		if err != nil {
			return wrapField(FieldStyleRefs, err)
		}
		e.StyleRefs = refs
	}
	return nil
}

func decodeRoomRecord(e *RoomRecord, obj map[string]json.RawMessage) error {
	var err error
	if e.LevelID, err = reqUUIDRef(obj, fieldLevelID); err != nil {
		return err
	}
	if e.SeedMm, err = reqCoordPair(obj, FieldSeedMm); err != nil {
		return err
	}
	if s, ok, err := optDisplayString(obj, FieldName); err != nil {
		return err
	} else if ok {
		e.Name = &s
	}
	if s, ok, err := optStyleToken(obj, FieldFloorStyleRef); err != nil {
		return err
	} else if ok {
		e.FloorStyleRef = &s
	}
	if s, ok, err := optStyleToken(obj, FieldCeilingStyleRef); err != nil {
		return err
	} else if ok {
		e.CeilingStyleRef = &s
	}
	return nil
}

func decodeLabel(e *Label, obj map[string]json.RawMessage) error {
	var err error
	if e.LevelID, err = reqUUIDRef(obj, fieldLevelID); err != nil {
		return err
	}
	if e.AnchorMm, err = reqCoordPair(obj, FieldAnchorMm); err != nil {
		return err
	}
	if e.Name, err = reqDisplayString(obj, FieldName); err != nil {
		return err
	}
	return nil
}

func decodeDimension(e *Dimension, obj map[string]json.RawMessage) error {
	var err error
	if e.LevelID, err = reqUUIDRef(obj, fieldLevelID); err != nil {
		return err
	}
	if e.StartMm, err = reqCoordPair(obj, FieldStartMm); err != nil {
		return err
	}
	if e.EndMm, err = reqCoordPair(obj, FieldEndMm); err != nil {
		return err
	}
	if s, ok, err := optDisplayString(obj, FieldLabelOverride); err != nil {
		return err
	} else if ok {
		e.LabelOverride = &s
	}
	return nil
}

func decodeAssetInstance(e *AssetInstance, obj map[string]json.RawMessage) error {
	var err error
	if e.LevelID, err = reqUUIDRef(obj, fieldLevelID); err != nil {
		return err
	}
	if e.Digest, err = reqDigest(obj, FieldDigest); err != nil {
		return err
	}
	tr, ok := obj[FieldTransform]
	if !ok {
		return fieldErr(FieldTransform, codeMissingRequired, "required field absent")
	}
	tf, err := decodeTransform(tr)
	if err != nil {
		return wrapField(FieldTransform, err)
	}
	e.Transform = tf
	if s, ok, err := optDisplayString(obj, FieldDisplayName); err != nil {
		return err
	} else if ok {
		e.DisplayName = &s
	}
	return nil
}
