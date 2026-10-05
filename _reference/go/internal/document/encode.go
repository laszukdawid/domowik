package document

import (
	"encoding/json"
	"fmt"
)

// encodeEntity renders a typed entity as the complete §2.3 entity object
// it would decode from: reserved `id`/`type`, every required schema field,
// populated optionals, and preserved unknown fields. json.Marshal emits
// map keys in ascending byte order (C1 recommends ascending emission), so
// output is deterministic for equal entities.
//
// The encoder exists for inverse-effect construction (BEHAVIOR-SPEC §8):
// an undo of deleteEntity must recreate the entity from its pre-image.
// It makes NO canonical-JSON claim (§3.3 is explicitly not adopted);
// round-tripping decodeEntity(encodeEntity(e)) must yield a semantically
// equal typed entity, which the property tests assert.
func encodeEntity(e Entity) (json.RawMessage, error) {
	obj := map[string]json.RawMessage{
		fieldID:   rawString(e.EntityID().String()),
		fieldType: rawString(string(e.Kind())),
	}
	var err error
	switch t := e.(type) {
	case *Level:
		if obj[FieldName], err = marshal(t.Name); err != nil {
			return nil, err
		}
		if obj[FieldElevationMm], err = marshal(t.ElevationMm); err != nil {
			return nil, err
		}
		if obj[FieldFloorThickness], err = marshal(t.FloorThicknessMm); err != nil {
			return nil, err
		}
		if obj[FieldDefaultWallH], err = marshal(t.DefaultWallHeightMm); err != nil {
			return nil, err
		}
		if obj[FieldDefaultCeilH], err = marshal(t.DefaultCeilingHeightMm); err != nil {
			return nil, err
		}
		if t.BackgroundImages != nil {
			imgs := make([]map[string]json.RawMessage, 0, len(t.BackgroundImages))
			for _, img := range t.BackgroundImages {
				anchor := map[string]json.RawMessage{
					"translationMm": rawJSONSlice(img.Anchor.TranslationMm),
					"scaleUniform":  rawMustMarshal(img.Anchor.ScaleUniform),
					"rotationDeg":   rawMustMarshal(img.Anchor.RotationDeg),
				}
				imgs = append(imgs, map[string]json.RawMessage{
					"digest":       rawMustMarshal(img.Digest),
					"anchor":       rawMustMarshal(anchor),
					"stackOrdinal": rawMustMarshal(img.StackOrdinal),
				})
			}
			if obj[FieldBackgroundImgs], err = marshal(imgs); err != nil {
				return nil, err
			}
		}
	case *Wall:
		obj[fieldLevelID] = rawString(t.LevelID.String())
		if obj[FieldStartMm], err = marshalCoordPair(t.StartMm); err != nil {
			return nil, err
		}
		if obj[FieldEndMm], err = marshalCoordPair(t.EndMm); err != nil {
			return nil, err
		}
		if obj[FieldThicknessMm], err = marshal(t.ThicknessMm); err != nil {
			return nil, err
		}
		if obj[FieldHeightMm], err = marshal(t.HeightMm); err != nil {
			return nil, err
		}
		if t.StyleRefs != nil {
			if obj[FieldStyleRefs], err = marshal(t.StyleRefs); err != nil {
				return nil, err
			}
		}
	case *WallJoin:
		members := make([]map[string]json.RawMessage, 0, len(t.Members))
		for _, m := range t.Members {
			members = append(members, map[string]json.RawMessage{
				"wallId": rawString(m.WallID.String()),
				"role":   rawString(string(m.Role)),
			})
		}
		if obj[FieldMembers], err = marshal(members); err != nil {
			return nil, err
		}
	case *Opening:
		if obj[FieldKind], err = marshal(t.KindField); err != nil {
			return nil, err
		}
		obj[fieldHostWall] = rawString(t.HostWallID.String())
		if obj[FieldU], err = marshal(t.U); err != nil {
			return nil, err
		}
		for _, fc := range []struct {
			name string
			v    float64
		}{{FieldWidthMm, t.WidthMm}, {FieldHeightMm, t.HeightMm}, {FieldSillMm, t.SillMm}, {FieldDepthMm, t.DepthMm}, {FieldSwingDeg, t.SwingDeg}} {
			if obj[fc.name], err = marshal(fc.v); err != nil {
				return nil, err
			}
		}
		if obj[FieldSide], err = marshal(t.Side); err != nil {
			return nil, err
		}
		if obj[FieldHinge], err = marshal(t.Hinge); err != nil {
			return nil, err
		}
		if obj[FieldOpensToward], err = marshal(t.OpensToward); err != nil {
			return nil, err
		}
		if t.StyleRefs != nil {
			if obj[FieldStyleRefs], err = marshal(t.StyleRefs); err != nil {
				return nil, err
			}
		}
	case *RoomRecord:
		obj[fieldLevelID] = rawString(t.LevelID.String())
		if obj[FieldSeedMm], err = marshalCoordPair(t.SeedMm); err != nil {
			return nil, err
		}
		if t.Name != nil {
			obj[FieldName] = rawString(*t.Name)
		}
		if t.FloorStyleRef != nil {
			obj[FieldFloorStyleRef] = rawString(*t.FloorStyleRef)
		}
		if t.CeilingStyleRef != nil {
			obj[FieldCeilingStyleRef] = rawString(*t.CeilingStyleRef)
		}
	case *Label:
		obj[fieldLevelID] = rawString(t.LevelID.String())
		if obj[FieldAnchorMm], err = marshalCoordPair(t.AnchorMm); err != nil {
			return nil, err
		}
		if obj[FieldName], err = marshal(t.Name); err != nil {
			return nil, err
		}
	case *Dimension:
		obj[fieldLevelID] = rawString(t.LevelID.String())
		if obj[FieldStartMm], err = marshalCoordPair(t.StartMm); err != nil {
			return nil, err
		}
		if obj[FieldEndMm], err = marshalCoordPair(t.EndMm); err != nil {
			return nil, err
		}
		if t.LabelOverride != nil {
			obj[FieldLabelOverride] = rawString(*t.LabelOverride)
		}
	case *AssetInstance:
		obj[fieldLevelID] = rawString(t.LevelID.String())
		if obj[FieldDigest], err = marshal(t.Digest); err != nil {
			return nil, err
		}
		tf := map[string]json.RawMessage{}
		if tf["translationMm"], err = marshalCoordPair(t.Transform.TranslationMm); err != nil {
			return nil, err
		}
		if tf["scaleUniform"], err = marshal(t.Transform.ScaleUniform); err != nil {
			return nil, err
		}
		if tf["rotationDeg"], err = marshal(t.Transform.RotationDeg); err != nil {
			return nil, err
		}
		if obj[FieldTransform], err = marshal(tf); err != nil {
			return nil, err
		}
		if t.DisplayName != nil {
			obj[FieldDisplayName] = rawString(*t.DisplayName)
		}
	default:
		return nil, fmt.Errorf("document: unencodable entity type %T", e)
	}
	for _, u := range e.UnknownFields() {
		obj[u.Name] = u.Value // opaque preservation, verbatim bytes
	}
	return marshal(obj)
}

// Level's Name assignment above must not be dead code; normalize by going
// through marshal uniformly.

func marshal(v any) (json.RawMessage, error) {
	b, err := json.Marshal(v)
	if err != nil {
		return nil, fmt.Errorf("document: encode %T: %w", v, err)
	}
	return json.RawMessage(b), nil
}

func marshalCoordPair(p [2]float64) (json.RawMessage, error) {
	return marshal([]float64{p[0], p[1]})
}

func rawJSONSlice(p [2]float64) json.RawMessage {
	b, _ := marshal([]float64{p[0], p[1]})
	return b
}

func rawMustMarshal(v any) json.RawMessage {
	b, _ := marshal(v)
	return b
}

func rawString(s string) json.RawMessage {
	b, _ := marshal(s) // string marshaling cannot fail
	return json.RawMessage(b)
}
