// Package document implements the v0.1 logical document model of
// FORMAT-SPEC §2–§5: typed entity schemas, reference validation, and the
// immutable/current entity projection. State changes flow only through
// batch delivery (Deliver); no exported type exposes direct mutation.
//
// All numeric magnitudes are consumed exclusively through the named
// provisional parameters of FORMAT-SPEC §3.1, mirrored in this package as
// its single definition site. No other file or test may inline a geometric
// bound literal (ED-0016). Parameter values remain PROVISIONAL until issue
// 0030 ratifies them; a 0030 outcome that changes a parameter's structure
// or units must be reflected here before issue 0010 closes.
package document

// CoordBoundMm is the named provisional parameter coordBoundMm
// (FORMAT-SPEC §3.1): maximum |x| and |y| for every authoritative plan
// coordinate, including room-record seeds and dimension/label anchors.
// Unit: millimetres. Value is PROVISIONAL (candidate 10^9 mm; NOT
// APPROVED pending issue 0030).
const CoordBoundMm = 1e9

// DimensionCapMm is the named provisional parameter dimensionCapMm
// (FORMAT-SPEC §3.1): inclusive upper cap for strictly positive dimension
// fields (wall thickness/height, opening width/height/depth, level
// defaults) and for the non-negative sillMm. Unit: millimetres. Value is
// PROVISIONAL (candidate 10^6 mm; NOT APPROVED pending issue 0030).
const DimensionCapMm = 1e6

// DerivedCoordBoundMm is the named provisional parameter
// derivedCoordBoundMm (FORMAT-SPEC §3.1): upper bound for ideal derived
// vertices. The formula mirrors the spec's single definition site
// ("coordBoundMm + 10^6 mm"); the addend is part of the parameter's
// definition, not an independent limit. A breach signals an implementation
// defect, never user-input rejection. Issue 0010 derives no geometry, so
// this parameter is defined but unused; it exists so later issues consume
// the name rather than re-deriving it.
const DerivedCoordBoundMm = CoordBoundMm + 1e6

// LwwHorizonRetention documents the named provisional parameter lwwHorizon
// (FORMAT-SPEC §5.1 S3). The specification asserts NO value for it — its
// fixation is explicitly open and owned by issue 0010's future fixation
// work. Until fixation, this implementation retains winner keys and
// tombstones without horizon truncation, which is conservative: every
// late-concurrent operation resolves against a retained key exactly as it
// would in the never-compacted replica (V5/LWW5). Truncation policy must
// not be added here before lwwHorizon has a ratified value and an owning
// compaction decision (issues 0005/0027/0029 own schedules, not values).
//
// Because retention is unbounded, no constant is defined; any future value
// MUST be introduced at this definition site only.
const LwwHorizonRetention = "unbounded-pending-lwwHorizon-fixation"

// DisplayStringMaxBytes is parser ceiling L19's display-string cap
// (FORMAT-SPEC §9.3, RATIFIED): display strings are at most 1024 bytes.
// This is a ratified reader envelope, not a provisional parameter.
const DisplayStringMaxBytes = 1024

// UndoStackCapacityDefault and RedoStackCapacityDefault are the named
// provisional parameters for the session-local undo/redo stack capacities
// (BEHAVIOR-SPEC R8.4.4: both bounds exist and are exhausted by dropping
// the oldest entry of THAT stack only; the two bounds are independent).
// Unit: stack entries. Values are PROVISIONAL (candidates 128 undo / 128
// redo entries; NOT APPROVED — fixation is implementation policy owned by
// later issues; R8.4.4 fixes only the exhaustion BEHAVIOR, never a value).
// Per-device configuration may override these defaults without new
// literals elsewhere; no other file may inline a capacity magnitude.
const (
	UndoStackCapacityDefault = 128
	RedoStackCapacityDefault = 128
)

// Normative field-domain constants fixed by FORMAT-SPEC §2.3's Tier A
// table (normative wire shape, issue 0004). ED-0016 note: these are NOT
// provisional magnitude parameters — they are the schema's own field
// domains (`u` ∈ [0,1]; `swingDeg` ∈ [0°,180°]), stated here as named
// constants so no comparison site carries an inline literal. They are not
// subject to issue 0030 ratification.
const (
	OpeningUMax     = 1.0
	SwingDegMax     = 180.0
	SwingDegMin     = 0.0
	OpeningUMin     = 0.0
	ScaleUniformMin = 0.0 // exclusive: scaleUniform must be > ScaleUniformMin
	DimensionMin    = 0.0 // exclusive for strictly-positive dimensions; inclusive for sillMm/floorThicknessMm lower bound
	CounterBaseMin  = 1   // B3: each actor's monotonic counter starts at 1
)
