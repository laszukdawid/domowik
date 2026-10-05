package document

import (
	"encoding/json"
	"fmt"
	"sort"
)

// applyField mutates ent (which callers must have cloned) by setting or
// clearing ONE top-level serialized field (FORMAT-SPEC §4.2 O2/O3).
// Set-valued and array-valued fields are replaced wholesale, never merged
// element-wise. JSON null clears an optional field to absent; null against
// a required field rejects. Unknown field names — including preserved
// opaque fields — reject: preserved data is read-only (§8/L32).
func applyField(ent Entity, name string, raw json.RawMessage) error {
	null := string(raw) == "null"
	single := map[string]json.RawMessage{name: raw}
	clearStr := func(dst **string) error {
		if null {
			*dst = nil
			return nil
		}
		s, ok, err := optDisplayString(single, name)
		if err != nil || !ok {
			if err != nil {
				return err
			}
			return fieldErr(name, codeInvalidFieldSyntax, "value must be a string or null")
		}
		*dst = &s
		return nil
	}
	clearToken := func(dst **string) error {
		if null {
			*dst = nil
			return nil
		}
		s, ok, err := optStyleToken(single, name)
		if err != nil || !ok {
			if err != nil {
				return err
			}
			return fieldErr(name, codeEmptyStyleToken, "value must be a non-empty token")
		}
		*dst = &s
		return nil
	}

	switch e := ent.(type) {
	case *Level:
		switch name {
		case FieldName:
			if null {
				return reqFieldNotNull(name)
			}
			s, err := reqDisplayString(single, name)
			if err != nil {
				return err
			}
			e.Name = s
		case FieldElevationMm:
			v, err := numOrNull(single, name, null, reqCoord)
			if err != nil {
				return err
			}
			e.ElevationMm = v.(float64)
		case FieldFloorThickness:
			v, err := numOrNull(single, name, null, reqDimNonNeg)
			if err != nil {
				return err
			}
			e.FloorThicknessMm = v.(float64)
		case FieldDefaultWallH:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.DefaultWallHeightMm = v.(float64)
		case FieldDefaultCeilH:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.DefaultCeilingHeightMm = v.(float64)
		case FieldBackgroundImgs:
			if null {
				e.BackgroundImages = nil
				return nil
			}
			imgs, err := decodeBackgroundImages(raw)
			if err != nil {
				return wrapField(name, err)
			}
			e.BackgroundImages = imgs
		default:
			return unknownFieldErr(name)
		}
	case *Wall:
		switch name {
		case fieldLevelID:
			id, err := refOrNull(single, name, null, reqUUIDRef)
			if err != nil {
				return err
			}
			if null {
				return reqFieldNotNull(name)
			}
			e.LevelID = id.(UUID)
		case FieldStartMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			if p == e.EndMm {
				return fieldErr(name, codeZeroLengthWall, "startMm and endMm must differ")
			}
			e.StartMm = p
		case FieldEndMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			if e.StartMm == p {
				return fieldErr(name, codeZeroLengthWall, "startMm and endMm must differ")
			}
			e.EndMm = p
		case FieldThicknessMm:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.ThicknessMm = v.(float64)
		case FieldHeightMm:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.HeightMm = v.(float64)
		case FieldStyleRefs:
			if null {
				e.StyleRefs = nil
				return nil
			}
			refs, err := decodeStyleRefs(raw)
			if err != nil {
				return wrapField(name, err)
			}
			e.StyleRefs = refs
		default:
			return unknownFieldErr(name)
		}
	case *WallJoin:
		switch name {
		case FieldMembers:
			if null {
				return reqFieldNotNull(name)
			}
			members, err := reqMembers(single, name)
			if err != nil {
				return err
			}
			e.Members = members
		default:
			return unknownFieldErr(name)
		}
	case *Opening:
		switch name {
		case FieldKind:
			if null {
				return reqFieldNotNull(name)
			}
			k, err := reqOpeningKind(single, name)
			if err != nil {
				return err
			}
			e.KindField = k
		case fieldHostWall:
			if null {
				return reqFieldNotNull(name)
			}
			id, err := reqUUIDRef(single, name)
			if err != nil {
				return err
			}
			e.HostWallID = id
		case FieldU:
			v, err := numOrNull(single, name, null, reqUnitInterval)
			if err != nil {
				return err
			}
			e.U = v.(float64)
		case FieldWidthMm:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.WidthMm = v.(float64)
		case FieldHeightMm:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.HeightMm = v.(float64)
		case FieldSillMm:
			v, err := numOrNull(single, name, null, reqDimNonNeg)
			if err != nil {
				return err
			}
			e.SillMm = v.(float64)
		case FieldDepthMm:
			v, err := numOrNull(single, name, null, reqDimPositive)
			if err != nil {
				return err
			}
			e.DepthMm = v.(float64)
		case FieldSide:
			if null {
				return reqFieldNotNull(name)
			}
			s, err := reqSide(single, name)
			if err != nil {
				return err
			}
			e.Side = s
		case FieldHinge:
			if null {
				return reqFieldNotNull(name)
			}
			h, err := reqHinge(single, name)
			if err != nil {
				return err
			}
			e.Hinge = h
		case FieldOpensToward:
			if null {
				return reqFieldNotNull(name)
			}
			s, err := reqSide(single, name)
			if err != nil {
				if fe, ok := err.(*FieldError); ok {
					fe.Code = codeInvalidOpensToward
				}
				return err
			}
			e.OpensToward = s
		case FieldSwingDeg:
			v, err := numOrNull(single, name, null, reqSwingDeg)
			if err != nil {
				return err
			}
			e.SwingDeg = v.(float64)
		case FieldStyleRefs:
			if null {
				e.StyleRefs = nil
				return nil
			}
			refs, err := decodeStyleRefs(raw)
			if err != nil {
				return wrapField(name, err)
			}
			e.StyleRefs = refs
		default:
			return unknownFieldErr(name)
		}
	case *RoomRecord:
		switch name {
		case fieldLevelID:
			if null {
				return reqFieldNotNull(name)
			}
			id, err := reqUUIDRef(single, name)
			if err != nil {
				return err
			}
			e.LevelID = id
		case FieldSeedMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			e.SeedMm = p
		case FieldName:
			return clearStr(&e.Name)
		case FieldFloorStyleRef:
			return clearToken(&e.FloorStyleRef)
		case FieldCeilingStyleRef:
			return clearToken(&e.CeilingStyleRef)
		default:
			return unknownFieldErr(name)
		}
	case *Label:
		switch name {
		case fieldLevelID:
			if null {
				return reqFieldNotNull(name)
			}
			id, err := reqUUIDRef(single, name)
			if err != nil {
				return err
			}
			e.LevelID = id
		case FieldAnchorMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			e.AnchorMm = p
		case FieldName:
			if null {
				return reqFieldNotNull(name)
			}
			s, err := reqDisplayString(single, name)
			if err != nil {
				return err
			}
			e.Name = s
		default:
			return unknownFieldErr(name)
		}
	case *Dimension:
		switch name {
		case fieldLevelID:
			if null {
				return reqFieldNotNull(name)
			}
			id, err := reqUUIDRef(single, name)
			if err != nil {
				return err
			}
			e.LevelID = id
		case FieldStartMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			e.StartMm = p
		case FieldEndMm:
			if null {
				return reqFieldNotNull(name)
			}
			p, err := reqCoordPair(single, name)
			if err != nil {
				return err
			}
			e.EndMm = p
		case FieldLabelOverride:
			return clearStr(&e.LabelOverride)
		default:
			return unknownFieldErr(name)
		}
	case *AssetInstance:
		switch name {
		case fieldLevelID:
			if null {
				return reqFieldNotNull(name)
			}
			id, err := reqUUIDRef(single, name)
			if err != nil {
				return err
			}
			e.LevelID = id
		case FieldDigest:
			if null {
				return reqFieldNotNull(name)
			}
			d, err := reqDigest(single, name)
			if err != nil {
				return err
			}
			e.Digest = d
		case FieldTransform:
			if null {
				return reqFieldNotNull(name)
			}
			tf, err := decodeTransform(raw)
			if err != nil {
				return wrapField(name, err)
			}
			e.Transform = tf
		case FieldDisplayName:
			return clearStr(&e.DisplayName)
		default:
			return unknownFieldErr(name)
		}
	default:
		return fmt.Errorf("unknown entity storage type %T", ent)
	}
	return nil
}

func reqFieldNotNull(name string) error {
	return fieldErr(name, codeMissingRequired, "required field cannot be cleared with null")
}

func unknownFieldErr(name string) error {
	return fieldErr(name, codeUnknownField,
		"field %q is not part of the entity schema; preserved unknown fields are read-only", name)
}

// numOrNull factors the null-reject-or-decode pattern for numeric fields;
// all such fields are required, so null always rejects.
func numOrNull(single map[string]json.RawMessage, name string, null bool, dec func(map[string]json.RawMessage, string) (float64, error)) (any, error) {
	if null {
		return nil, reqFieldNotNull(name)
	}
	v, err := dec(single, name)
	if err != nil {
		return nil, err
	}
	return any(v), nil
}

func refOrNull(single map[string]json.RawMessage, name string, null bool, dec func(map[string]json.RawMessage, string) (UUID, error)) (any, error) {
	if null {
		return nil, reqFieldNotNull(name)
	}
	v, err := dec(single, name)
	if err != nil {
		return nil, err
	}
	return any(v), nil
}

// applySetFields applies a setFields payload (top-level field name → new
// value) onto a cloned entity. Fields are processed in ascending name
// order so rejection behavior is deterministic regardless of payload map
// iteration order. Absent payload fields leave the entity unchanged (O2).
func applySetFields(ent Entity, fields map[string]json.RawMessage) error {
	names := make([]string, 0, len(fields))
	for n := range fields {
		names = append(names, n)
	}
	sort.Strings(names)
	for _, n := range names {
		if err := applyField(ent, n, fields[n]); err != nil {
			return err
		}
	}
	return nil
}

// mergeableFieldNames returns the serialized names of every top-level
// schema field PRESENT on the entity — required fields plus populated
// optionals — for winner-key recording (§5.1 S3). Preserved unknown fields
// are never mergeable: they are read-only and unaddressable.
func mergeableFieldNames(ent Entity) []string {
	var names []string
	add := func(ss ...string) { names = append(names, ss...) }
	switch e := ent.(type) {
	case *Level:
		add(FieldName, FieldElevationMm, FieldFloorThickness, FieldDefaultWallH, FieldDefaultCeilH)
		if e.BackgroundImages != nil {
			add(FieldBackgroundImgs)
		}
	case *Wall:
		add(fieldLevelID, FieldStartMm, FieldEndMm, FieldThicknessMm, FieldHeightMm)
		if e.StyleRefs != nil {
			add(FieldStyleRefs)
		}
	case *WallJoin:
		add(FieldMembers)
	case *Opening:
		add(FieldKind, fieldHostWall, FieldU, FieldWidthMm, FieldHeightMm, FieldSillMm, FieldDepthMm,
			FieldSide, FieldHinge, FieldOpensToward, FieldSwingDeg)
		if e.StyleRefs != nil {
			add(FieldStyleRefs)
		}
	case *RoomRecord:
		add(fieldLevelID, FieldSeedMm)
		if e.Name != nil {
			add(FieldName)
		}
		if e.FloorStyleRef != nil {
			add(FieldFloorStyleRef)
		}
		if e.CeilingStyleRef != nil {
			add(FieldCeilingStyleRef)
		}
	case *Label:
		add(fieldLevelID, FieldAnchorMm, FieldName)
	case *Dimension:
		add(fieldLevelID, FieldStartMm, FieldEndMm)
		if e.LabelOverride != nil {
			add(FieldLabelOverride)
		}
	case *AssetInstance:
		add(fieldLevelID, FieldDigest, FieldTransform)
		if e.DisplayName != nil {
			add(FieldDisplayName)
		}
	}
	sort.Strings(names)
	return names
}
