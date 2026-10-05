package document

// Kind enumerates the eight v0.1 entity kinds of FORMAT-SPEC §2.3.
type Kind string

const (
	KindLevel         Kind = "level"
	KindWall          Kind = "wall"
	KindWallJoin      Kind = "wallJoin"
	KindOpening       Kind = "opening"
	KindRoomRecord    Kind = "roomRecord"
	KindLabel         Kind = "label"
	KindDimension     Kind = "dimension"
	KindAssetInstance Kind = "assetInstance"
)

// OpeningKind is the opening `kind` enum (FORMAT-SPEC §2.3; BEHAVIOR-SPEC
// §4 owns semantics).
type OpeningKind string

const (
	OpeningDoor   OpeningKind = "door"
	OpeningWindow OpeningKind = "window"
)

// Side names a wall face (BEHAVIOR-SPEC §4; `invalid-side` outside the
// enum).
type Side string

const (
	SideLeft  Side = "Left"
	SideRight Side = "Right"
)

// Hinge names the hinged end of an opening in axis terms
// (`invalid-hinge` outside the enum).
type Hinge string

const (
	HingeNearStart Hinge = "nearStart"
	HingeNearEnd   Hinge = "nearEnd"
)

// WallRole is the wallJoin member role.
type WallRole string

const (
	RoleStart WallRole = "Start"
	RoleEnd   WallRole = "End"
)

// Serialized field names per kind (FORMAT-SPEC §2.3). Field names never
// collide with the reserved keys `id` and `type`.
const (
	fieldID       = "id"
	fieldType     = "type"
	fieldLevelID  = "levelId"
	fieldHostWall = "hostWallId"

	FieldName            = "name"
	FieldElevationMm     = "elevationMm"
	FieldFloorThickness  = "floorThicknessMm"
	FieldDefaultWallH    = "defaultWallHeightMm"
	FieldDefaultCeilH    = "defaultCeilingHeightMm"
	FieldBackgroundImgs  = "backgroundImages"
	FieldStartMm         = "startMm"
	FieldEndMm           = "endMm"
	FieldThicknessMm     = "thicknessMm"
	FieldHeightMm        = "heightMm"
	FieldStyleRefs       = "styleRefs"
	FieldMembers         = "members"
	FieldKind            = "kind"
	FieldU               = "u"
	FieldWidthMm         = "widthMm"
	FieldDepthMm         = "depthMm"
	FieldSillMm          = "sillMm"
	FieldSide            = "side"
	FieldHinge           = "hinge"
	FieldOpensToward     = "opensToward"
	FieldSwingDeg        = "swingDeg"
	FieldSeedMm          = "seedMm"
	FieldFloorStyleRef   = "floorStyleRef"
	FieldCeilingStyleRef = "ceilingStyleRef"
	FieldAnchorMm        = "anchorMm"
	FieldLabelOverride   = "labelOverride"
	FieldDigest          = "digest"
	FieldTransform       = "transform"
	FieldDisplayName     = "displayName"
)
