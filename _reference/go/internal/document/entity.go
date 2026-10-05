package document

import (
	"encoding/json"
)

// UnknownField is one unrecognized top-level field of a known entity kind,
// preserved opaquely (FORMAT-SPEC §2.3, §8). Preserved fields are
// read-only: they are never addressable by setFields and never rewritten.
// Name ordering of the slice is ascending byte order so preservation is
// deterministic.
type UnknownField struct {
	Name  string
	Value json.RawMessage
}

// BackgroundImage is one entry of the unordered `level.backgroundImages`
// set (FORMAT-SPEC §2.3 Tier A, §2.5 anchor transform p = t + s·R(θ)·q;
// semantics owned by BEHAVIOR-SPEC §2.1–§2.5).
type BackgroundImage struct {
	Digest string
	Anchor Anchor
	// StackOrdinal is an integer ≥ 0 that must fit signed 64-bit (L13).
	StackOrdinal int64
}

// Anchor is the background-image placement anchor. TranslationMm
// components are authoritative plan coordinates bounded by CoordBoundMm.
type Anchor struct {
	TranslationMm [2]float64
	// ScaleUniform is > 0 and finite.
	ScaleUniform float64
	// RotationDeg is finite, degrees CCW from +X.
	RotationDeg float64
}

// WallMember is one entry of a wallJoin membership set.
type WallMember struct {
	WallID UUID
	Role   WallRole
}

// Transform is the assetInstance placement transform. Structurally
// analogous to — but NOT derived from or owned by — the BEHAVIOR-SPEC §2.5
// background-anchor pattern.
type Transform struct {
	TranslationMm [2]float64
	ScaleUniform  float64 // > 0, finite
	RotationDeg   float64
}

// Entity is the read-only view every projected entity exposes. Values
// obtained from a Projection are deep copies; mutating them has no effect
// on projection state (no public direct-mutation API).
type Entity interface {
	// EntityID returns the stable identity (E1/E6): immutable for the
	// lifetime of the entity, never reassigned.
	EntityID() UUID
	// Kind returns the v0.1 entity kind.
	Kind() Kind
	// UnknownFields returns preserved opaque unknown top-level fields in
	// ascending name order; callers must treat the values as read-only.
	UnknownFields() []UnknownField
}

type entityBase struct {
	id      UUID
	unknown []UnknownField
}

func (e *entityBase) EntityID() UUID { return e.id }
func (e *entityBase) UnknownFields() []UnknownField {
	out := make([]UnknownField, len(e.unknown))
	for i, f := range e.unknown {
		f.Value = append(json.RawMessage(nil), f.Value...)
		out[i] = f
	}
	return out
}

// Level is the Tier A floor entity. Required fields are mandatory on
// decode; optional ones are nil/absent when not present.
type Level struct {
	entityBase
	Name                   string
	ElevationMm            float64
	FloorThicknessMm       float64
	DefaultWallHeightMm    float64
	DefaultCeilingHeightMm float64
	BackgroundImages       []BackgroundImage // optional unordered set; canonical order enforced
}

func (*Level) Kind() Kind { return KindLevel }

// Wall is the Tier A wall entity. No stored derived geometry (§2.4).
type Wall struct {
	entityBase
	LevelID     UUID
	StartMm     [2]float64
	EndMm       [2]float64
	ThicknessMm float64
	HeightMm    float64
	StyleRefs   []string // optional set of non-empty opaque tokens; canonical order enforced
}

func (*Wall) Kind() Kind { return KindWall }

// WallJoin stores NO coordinates and NO levelId; both derive from its
// members (FORMAT-SPEC §2.3). Membership is an unordered set (C3).
type WallJoin struct {
	entityBase
	Members []WallMember
}

func (*WallJoin) Kind() Kind { return KindWallJoin }

// Opening stores NO levelId: the effective level resolves through
// HostWallID at read time (BEHAVIOR-SPEC R4.0.2) and never changes during
// the opening's lifetime. Openings hosted by one wall form an unordered
// set; nothing introduces order (R4.0.6).
type Opening struct {
	entityBase
	KindField   OpeningKind
	HostWallID  UUID
	U           float64
	WidthMm     float64
	HeightMm    float64
	SillMm      float64
	DepthMm     float64
	Side        Side
	Hinge       Hinge
	OpensToward Side
	SwingDeg    float64
	StyleRefs   []string
}

func (*Opening) Kind() Kind { return KindOpening }

// RoomRecord carries only its authoritative fields (§2.5); candidacy,
// association, and status are derived projection, never authored by
// operations (R5.2.5).
type RoomRecord struct {
	entityBase
	LevelID         UUID
	SeedMm          [2]float64
	Name            *string // absent and empty-string are distinct authoritatively (R5.3.3/F37d pattern)
	FloorStyleRef   *string
	CeilingStyleRef *string
}

func (*RoomRecord) Kind() Kind { return KindRoomRecord }

// Label is a Tier B format-reserved provisional envelope (§2.3): only
// structural/type checks apply; all further semantics are deferred open
// questions.
type Label struct {
	entityBase
	LevelID  UUID
	AnchorMm [2]float64
	Name     string
}

func (*Label) Kind() Kind { return KindLabel }

// Dimension is a Tier B envelope. StartMm/EndMm are the STORED measured
// extent.
type Dimension struct {
	entityBase
	LevelID       UUID
	StartMm       [2]float64
	EndMm         [2]float64
	LabelOverride *string
}

func (*Dimension) Kind() Kind { return KindDimension }

// AssetInstance is a Tier B envelope. Digest identity follows
// ASSET-STRATEGY §10.1 as cited by §2.3; descriptor-level behavior is owned
// elsewhere (ASSET-STRATEGY §10.2/§10.6, BEHAVIOR-SPEC §9.6) and is cited,
// not restated, here.
type AssetInstance struct {
	entityBase
	LevelID     UUID
	Digest      string
	Transform   Transform
	DisplayName *string
}

func (*AssetInstance) Kind() Kind { return KindAssetInstance }
