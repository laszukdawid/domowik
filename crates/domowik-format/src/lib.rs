//! Domowik document model: levels, walls, joins and openings.
//!
//! Entity shapes follow `docs/FORMAT-SPEC.md` §3 (the entity table at lines
//! 142-145) and `docs/BEHAVIOR-SPEC.md`. Units are **millimetres** throughout;
//! the `Mm` suffix marks millimetre-valued fields, and the plan plane is a
//! right-handed X/Y system (BEHAVIOR-SPEC §1.1).
//!
//! # Conformance scope
//!
//! This implements the level/wall/opening subset needed for a working editor.
//! It does **not** yet implement the operation log, snapshots, causal position,
//! or the container framing rules — those are separate and larger.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Ceiling on any coordinate magnitude, in millimetres.
///
/// **RATIFIED** at 10^7 mm (±10 km, a 20 km span) by ED-0100, closing the
/// FORMAT-SPEC row L14 deferral. It is no longer provisional.
///
/// The value is a PRODUCT decision, not a numerical one. The numerical
/// argument that previously favoured a smaller domain — that a distant
/// polygon's area is computed as a difference of huge nearly-equal products —
/// was a defect in the area routine, not a property of the domain: since
/// `signed_double_area` translates to a local origin, the measured worst-case
/// area error for a room-sized rectangle is 3.7e-9 mm² and is the SAME at
/// 10^6, 10^7 and 10^9. What the bound must actually do is cover the
/// documents people draw, and ±10 km covers a campus, an airport or a site
/// plan while ±1 km does not.
///
/// Coordinates are PLAN-LOCAL. No coordinate reference system or
/// georeferencing transform is defined for v0.1, so a survey import must
/// rebase into local coordinates and carry its datum separately; national-grid
/// coordinates in millimetres run to 10^8–10^9 and are deliberately outside
/// this domain.
///
/// Margin at the bound: `ulp(10^7 mm)` is 1.86e-9 mm, so the 10^-3 mm topology
/// quantum ([`JOIN_COINCIDENCE_MM`]) is about 537,000 ulps wide there.
pub const COORD_BOUND_MM: f64 = 10_000_000.0;

/// Ceiling on any stored dimension (thickness, height, width, depth, sill), in
/// millimetres.
///
/// **RATIFIED** at 10^6 mm by ED-0100. This is a domain ceiling that keeps
/// arithmetic in range, NOT a plausibility check: a tighter cap such as 10 m
/// would reject atria, hangars, auditoria and tall curtain-wall openings that
/// are entirely real. Implausible-but-legal values are the UI's business to
/// warn about, not the format's to refuse.
pub const DIMENSION_CAP_MM: f64 = 1_000_000.0;

/// The largest integer JavaScript reads exactly from JSON (2⁵³ − 1).
///
/// Count-like ordinal fields that editors read through a pool of JSON
/// engines (the browser editor included) are bounded by this so no reader can
/// silently round them. Level `displayOrder` is the first such field
/// (issue 0115); it is unitless and unrelated to the millimetre caps above.
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Ceiling on a DERIVED coordinate, in millimetres.
///
/// **RATIFIED** at `COORD_BOUND_MM + DIMENSION_CAP_MM` by ED-0100. Derived
/// geometry leaves the stored domain: a wall's rectangle extends half its/// thickness either side of its centre line, and a join patch is a hull of
/// such offsets, so a wall lying exactly on the bound produces polygon
/// vertices outside it. Ratifying the stored bound without this one would make
/// every boundary-hugging plan's own derived output out of domain.
///
/// Conservative rather than tight: half the dimension cap would do for the
/// offset itself, and the full cap is used so no future construction that
/// offsets by a whole dimension has to revisit it.
pub const DERIVED_COORD_BOUND_MM: f64 = COORD_BOUND_MM + DIMENSION_CAP_MM;

/// Which range a level field must lie in (FORMAT-SPEC §3).
///
/// Three different ranges, spelled out because collapsing them is exactly the
/// mistake that rejected a legal zero slab.
#[derive(Debug, Clone, Copy)]
enum LevelRange {
    /// A coordinate: finite, `|v| <= coordBoundMm`, sign free.
    Coordinate,
    /// Finite, in `[0, dimensionCapMm]` — zero is legal.
    NonNegative,
    /// Finite, in `(0, dimensionCapMm]`.
    Positive,
}

/// A point on the plan plane, in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Euclidean distance to `other`, in millimetres.
    pub fn distance_to(self, other: Self) -> f64 {
        (other.x - self.x).hypot(other.y - self.y)
    }

    /// Whether both components are finite and inside [`COORD_BOUND_MM`].
    ///
    /// The one predicate every authoritative coordinate is judged by, so a
    /// command builder refusing a seed and the validator refusing the stored
    /// record cannot disagree about what a coordinate is.
    pub fn is_within_bounds(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= COORD_BOUND_MM
            && self.y.abs() <= COORD_BOUND_MM
    }
}

/// Serialised as the spec's `[x, y]` coordinate pair rather than an object.
impl From<Point> for [f64; 2] {
    fn from(p: Point) -> Self {
        [p.x, p.y]
    }
}

impl From<[f64; 2]> for Point {
    fn from(a: [f64; 2]) -> Self {
        Self { x: a[0], y: a[1] }
    }
}

/// A storey. FORMAT-SPEC §3 `level`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Level {
    pub id: Uuid,
    pub name: String,
    pub elevation_mm: f64,
    pub floor_thickness_mm: f64,
    pub default_wall_height_mm: f64,
    pub default_ceiling_height_mm: f64,
    /// Background images to trace over, in stack order.
    ///
    /// Typed rather than left in `unknown`: this version DOES interpret them,
    /// and a field carried opaquely cannot be rendered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub background_images: Vec<BackgroundImage>,
    /// The level's optional place in the level-list presentation.
    ///
    /// BEHAVIOR-SPEC §2.2: a whole-list authoring device (the editor's drag or
    /// the CLI `order-levels`) assigns ascending ordinals 0..n−1 to every
    /// level of the document. Absent keeps the elevation-derived order, and
    /// mixed documents sort customized levels before non-customized ones, so
    /// any two documents present the same way everywhere. Nothing else reads
    /// it: render tiers, the 3-D scene, diagnostics and default targeting all
    /// stay elevation-based.
    ///
    /// The bound is the JSON/JS-exact integer range (u53), validated in
    /// [`Document::validate`], so the browser editor never reads an ordinal
    /// the JavaScript engine would silently round.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_order: Option<u64>,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// FORMAT-SPEC §8 U2: a writer that cannot interpret optional
    /// well-formed data MUST carry it into the successor package or refuse
    /// write-back entirely. **Silent stripping is forbidden everywhere.**
    /// Without this map `serde` drops what it does not recognise, so an
    /// older Domowik opening and saving a newer file would quietly delete
    /// the newer version's data.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Level {
    /// A ground floor with the spec example's dimensions.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            background_images: Vec::new(),
            display_order: None,
            unknown: Default::default(),
            id: Uuid::now_v7(),
            name: name.into(),
            elevation_mm: 0.0,
            floor_thickness_mm: 200.0,
            default_wall_height_mm: 2500.0,
            default_ceiling_height_mm: 2500.0,
        }
    }
}

/// A wall segment. FORMAT-SPEC §3 `wall`.
///
/// Carries NO derived geometry: the spec is explicit that wall polygons and
/// join patches are derived, never stored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wall {
    pub id: Uuid,
    pub level_id: Uuid,
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    pub thickness_mm: f64,
    pub height_mm: f64,
    /// Opaque style tokens, an unordered SET (FORMAT-SPEC §2.3, C3).
    ///
    /// Nothing resolves these in v0.1 — R3.0.6's explicit-resolution
    /// discipline means a token is never inherited from a level, a default or
    /// a neighbour, and an absent set means unspecified rather than "the usual
    /// one". They are carried so a document that declares them survives a
    /// round trip with its meaning intact.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_refs: Vec<String>,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// FORMAT-SPEC §8 U2: a writer that cannot interpret optional
    /// well-formed data MUST carry it into the successor package or refuse
    /// write-back entirely. **Silent stripping is forbidden everywhere.**
    /// Without this map `serde` drops what it does not recognise, so an
    /// older Domowik opening and saving a newer file would quietly delete
    /// the newer version's data.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Wall {
    pub fn new(
        level_id: Uuid,
        start: Point,
        end: Point,
        thickness_mm: f64,
        height_mm: f64,
    ) -> Self {
        Self {
            unknown: Default::default(),
            id: Uuid::now_v7(),
            level_id,
            start_mm: start.into(),
            end_mm: end.into(),
            thickness_mm,
            height_mm,
            style_refs: Vec::new(),
        }
    }

    pub fn start(&self) -> Point {
        self.start_mm.into()
    }

    pub fn end(&self) -> Point {
        self.end_mm.into()
    }

    /// Centre-line length in millimetres.
    pub fn length_mm(&self) -> f64 {
        self.start().distance_to(self.end())
    }
}

/// Door or window. FORMAT-SPEC §3 `opening`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpeningKind {
    Door,
    Window,
}

/// Operable construction, separate from whether the aperture is a door or a
/// window. Absent on older plans: [Opening::mechanism] resolves that case to
/// the original single-hinged representation without rewriting old files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpeningMechanism {
    DoorHinged,
    DoorSliding,
    WindowSideHung,
    WindowDoubleSwing,
    WindowLift,
}

impl OpeningMechanism {
    pub fn accepts(self, kind: OpeningKind) -> bool {
        matches!(
            (self, kind),
            (Self::DoorHinged | Self::DoorSliding, OpeningKind::Door)
                | (
                    Self::WindowSideHung | Self::WindowDoubleSwing | Self::WindowLift,
                    OpeningKind::Window
                )
        )
    }
}

/// Which host face a value refers to (BEHAVIOR-SPEC R4.0.4).
///
/// `Left` names the `+n̂` host face and `Right` the `−n̂` face, where `n̂` is
/// the wall's left normal. The mapping is used consistently everywhere,
/// including the R4.2.1 cut spans; nothing may reinterpret it locally.
///
/// Serialises as `"Left"` / `"Right"`, matching FORMAT-SPEC §3 and the golden
/// entity fixtures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    /// The `+n̂` face.
    Left,
    /// The `−n̂` face.
    Right,
}

/// Which end of the host axis a leaf is hinged at (BEHAVIOR-SPEC R4.0.4).
///
/// Named in AXIS terms, not screen terms: `NearStart` is the end nearer the
/// host's `startMm`. Reversing a host therefore has to remap it (R4.3.1), and
/// that is exactly the field map a naive reversal forgets.
///
/// Serialises as `"nearStart"` / `"nearEnd"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Hinge {
    /// Hinged at the end nearer `startMm`.
    NearStart,
    /// Hinged at the end nearer `endMm`.
    NearEnd,
}

/// An opening hosted by a wall. Stores no `levelId`: the effective level
/// resolves through `hostWallId` (BEHAVIOR-SPEC R4.0.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opening {
    pub id: Uuid,
    pub kind: OpeningKind,
    /// Optional for compatibility with plans written before mechanism choices
    /// existed. The absence has the same semantics as a single-hinged opening.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mechanism: Option<OpeningMechanism>,
    pub host_wall_id: Uuid,
    /// Position of the opening's CENTRE along the host wall's centre line,
    /// in `[0, 1]`.
    pub u: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Height of the opening's bottom edge above the HOST LEVEL's finished
    /// floor (`elevationMm`). Doors sit at 0.
    pub sill_mm: f64,
    /// How far the opening cuts into the host, in millimetres.
    ///
    /// R4.0.4: `> 0`, finite, and `≤ host.thicknessMm`. A door normally cuts
    /// the full thickness; a niche does not.
    pub depth_mm: f64,
    /// Which host face the opening is measured from (R4.0.4).
    pub side: Side,
    /// Which end of the host axis the leaf is hinged at (R4.0.4).
    pub hinge: Hinge,
    /// The half-plane the leaf extends into when open (R4.0.4).
    ///
    /// Geometrically inert when `swing_deg` is zero — a closed leaf is
    /// coplanar with the wall — but still authoritative stored data.
    pub opens_toward: Side,
    /// The leaf's excursion about its hinge, in degrees.
    ///
    /// R4.0.4: an UNSIGNED finite angle in `[0, 180]`. Zero means closed.
    pub swing_deg: f64,
    /// Opaque style tokens, an unordered SET (FORMAT-SPEC §2.3, C3). Same
    /// discipline as [`Wall::style_refs`]: nothing resolves them in v0.1 and
    /// nothing is inherited.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub style_refs: Vec<String>,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// FORMAT-SPEC §8 U2: a writer that cannot interpret optional
    /// well-formed data MUST carry it into the successor package or refuse
    /// write-back entirely. **Silent stripping is forbidden everywhere.**
    /// Without this map `serde` drops what it does not recognise, so an
    /// older Domowik opening and saving a newer file would quietly delete
    /// the newer version's data.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Opening {
    pub fn effective_mechanism(&self) -> OpeningMechanism {
        self.mechanism.unwrap_or(match self.kind {
            OpeningKind::Door => OpeningMechanism::DoorHinged,
            OpeningKind::Window => OpeningMechanism::WindowSideHung,
        })
    }
    /// A door of standard proportions, centred at `u` on `host_wall_id`.
    ///
    /// `host_thickness_mm` sets `depthMm`: a door cuts the whole wall, and
    /// R4.0.4 requires `depthMm ≤ host.thicknessMm`, so defaulting it to a
    /// fixed number would produce openings that fail validation on thin
    /// walls. The caller knows the host; it passes the thickness.
    pub fn door(
        host_wall_id: Uuid,
        u: f64,
        width_mm: f64,
        height_mm: f64,
        host_thickness_mm: f64,
    ) -> Self {
        Self {
            unknown: Default::default(),
            id: Uuid::now_v7(),
            kind: OpeningKind::Door,
            mechanism: None,
            host_wall_id,
            u,
            width_mm,
            height_mm,
            sill_mm: 0.0,
            depth_mm: host_thickness_mm,
            side: Side::Left,
            hinge: Hinge::NearStart,
            opens_toward: Side::Left,
            swing_deg: 90.0,
            style_refs: Vec::new(),
        }
    }

    /// A window centred at `u`, with its sill `sill_mm` above the floor.
    ///
    /// `swingDeg` is 0 — a window leaf is stored closed — which makes
    /// `opensToward` geometrically inert while remaining authoritative
    /// (R4.0.4).
    pub fn window(
        host_wall_id: Uuid,
        u: f64,
        width_mm: f64,
        height_mm: f64,
        sill_mm: f64,
        host_thickness_mm: f64,
    ) -> Self {
        Self {
            unknown: Default::default(),
            id: Uuid::now_v7(),
            kind: OpeningKind::Window,
            mechanism: None,
            host_wall_id,
            u,
            width_mm,
            height_mm,
            sill_mm,
            depth_mm: host_thickness_mm,
            side: Side::Left,
            hinge: Hinge::NearStart,
            opens_toward: Side::Left,
            swing_deg: 0.0,
            style_refs: Vec::new(),
        }
    }

    /// The span `[u_start, u_end]` the opening occupies along its host wall,
    /// given the host's length. `None` when the wall has no length.
    pub fn span(&self, host_length_mm: f64) -> Option<(f64, f64)> {
        if !host_length_mm.is_finite() || host_length_mm <= 0.0 {
            return None;
        }
        let half = (self.width_mm / 2.0) / host_length_mm;
        Some((self.u - half, self.u + half))
    }
}

/// Which end of a wall a join member refers to. FORMAT-SPEC §3 `wallJoin.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Role {
    /// The wall's `startMm`.
    Start,
    /// The wall's `endMm`.
    End,
}

/// One endpoint participating in a join.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinMember {
    pub wall_id: Uuid,
    pub role: Role,
}

impl JoinMember {
    pub fn new(wall_id: Uuid, role: Role) -> Self {
        Self { wall_id, role }
    }
}

/// A join: at least two wall endpoints meeting at one point.
/// FORMAT-SPEC §3 `wallJoin`, BEHAVIOR-SPEC R3.3.1-R3.3.7.
///
/// Stores NO coordinates and NO `levelId`: the node position is whatever the
/// members agree on (R3.3.4) and the level derives from the members. Storing
/// either would create a second source of truth that edits could desynchronise
/// from the walls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallJoin {
    pub id: Uuid,
    /// An unordered set of at least two distinct wall endpoints.
    pub members: Vec<JoinMember>,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// FORMAT-SPEC §8 U2: a writer that cannot interpret optional
    /// well-formed data MUST carry it into the successor package or refuse
    /// write-back entirely. **Silent stripping is forbidden everywhere.**
    /// Without this map `serde` drops what it does not recognise, so an
    /// older Domowik opening and saving a newer file would quietly delete
    /// the newer version's data.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl WallJoin {
    pub fn new(members: Vec<JoinMember>) -> Self {
        Self {
            unknown: Default::default(),
            id: Uuid::now_v7(),
            members,
        }
    }

    /// True when this join already contains `member`.
    pub fn contains(&self, member: JoinMember) -> bool {
        self.members.contains(&member)
    }
}

/// Collects the §2.3/C3 violations of one `styleRefs` set.
///
/// Shared by wall and opening so the two cannot end up with different ideas of
/// what a token set is.
fn check_style_refs(
    errors: &mut Vec<ValidationError>,
    id: Uuid,
    kind: &'static str,
    refs: &[String],
) {
    // A set, not a `Vec` + `contains`: the token list has no length bound, and
    // a linear membership test per token makes validation quadratic in it.
    // `BTreeSet` rather than `HashSet` keeps the lookup bounded without adding
    // a hasher whose iteration order would be one more thing to reason about;
    // nothing here iterates it, but the choice costs nothing either way.
    //
    // The reported errors are unchanged, including their order and their
    // multiplicity: `insert` returns false exactly when `contains` did, an
    // empty token still short-circuits before the set is touched, and a token
    // repeated three times still reports twice.
    let mut seen: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for token in refs {
        if token.is_empty() {
            errors.push(ValidationError::EmptyStyleRef { id, kind });
            continue;
        }
        if !seen.insert(token.as_str()) {
            errors.push(ValidationError::DuplicateStyleRef {
                id,
                kind,
                token: token.clone(),
            });
        }
    }
}

/// Applies the L19 byte cap to one optional display string.
fn check_display_string(
    errors: &mut Vec<ValidationError>,
    id: Uuid,
    kind: &'static str,
    field: &'static str,
    value: &Option<String>,
) {
    if let Some(text) = value {
        if text.len() > DISPLAY_STRING_CAP_BYTES {
            errors.push(ValidationError::DisplayStringTooLong {
                id,
                kind,
                field,
                bytes: text.len(),
                cap: DISPLAY_STRING_CAP_BYTES,
            });
        }
    }
}

/// Replaces the first matching element in place, or appends when none match.
fn replace_or_push<T>(items: &mut Vec<T>, value: T, matches: impl Fn(&T) -> bool) {
    match items.iter().position(matches) {
        Some(i) => items[i] = value,
        None => items.push(value),
    }
}

/// The topology grid quantum: the cell size that decides whether two points
/// are the SAME POINT.
///
/// **RATIFIED** at 10⁻³ mm by ED-0100, and given one meaning by ED-0103 N-2.
/// One micrometre is far below any building dimension and far above
/// millimetre-scale floating-point noise.
///
/// This is the same quantum `domowik-geometry`'s arrangement welds vertices
/// on, and it must stay that way: the two crates cannot share a constant
/// because geometry depends on format and not the reverse, so
/// `domowik_geometry::locality` carries a test that fails if they drift.
///
/// # It is a grid, not a radius
///
/// Points are the same point when they round to the same cell — see
/// [`topology_cell`]. That relation is transitive and independent of the order
/// members are listed in; a distance from a chosen member is neither. Two
/// points inside one cell may be up to √2 quanta apart, and two points either
/// side of a cell boundary are different however close they are. That is
/// snap-rounding, and it is what the arrangement actually does, so validation
/// answers the same way rather than a more comfortable way that would pass
/// documents whose joins do not exist topologically.
///
/// The quantum is 2⁻¹⁰ mm (ED-0110, issue 0096): a power of two, so scaling by
/// its inverse is exact and every cell boundary is an exact binary64 value.
pub const JOIN_COINCIDENCE_MM: f64 = 1.0 / 1024.0;

/// The topology-grid cell a point falls in (ED-0103 N-2).
///
/// Two points are the SAME POINT exactly when this is equal for both. Callers
/// must pass finite coordinates; a non-finite value has no cell and the
/// saturating cast would put wildly different coordinates in the same one.
pub fn topology_cell(p: Point) -> (i64, i64) {
    debug_assert!(
        p.x.is_finite() && p.y.is_finite(),
        "non-finite point has no cell"
    );
    (cell_index(p.x), cell_index(p.y))
}

/// `floor(v · 1024 + ½)`, exactly: cells are half-open `[k − ½, k + ½)`.
/// Scaling by a power of two is exact, and the fractional part of the scaled
/// value is always representable, so no step rounds.
fn cell_index(v: f64) -> i64 {
    let s = v / JOIN_COINCIDENCE_MM;
    let f = s.floor();
    if s - f >= 0.5 {
        f as i64 + 1
    } else {
        f as i64
    }
}

/// How a background image is placed on a level.
///
/// FORMAT-SPEC §2.3's `anchor`: a translation in plan millimetres, a uniform
/// scale, and a rotation in degrees. Uniform scale only — a non-uniform one
/// would let a user distort a scan into agreeing with a wrong measurement,
/// which is the opposite of what tracing over a survey is for.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Anchor {
    pub translation_mm: [f64; 2],
    pub scale_uniform: f64,
    pub rotation_deg: f64,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// §8 U2 names "unknown DESCRIPTOR fields" explicitly, and an anchor is a
    /// descriptor. Without this map an older Domowik opening and saving a
    /// newer file would silently drop, say, a skew or a per-axis scale the
    /// newer version wrote — which U2 forbids "everywhere", not only on
    /// entities.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Default for Anchor {
    fn default() -> Self {
        Self {
            translation_mm: [0.0, 0.0],
            scale_uniform: 1.0,
            rotation_deg: 0.0,
            unknown: Default::default(),
        }
    }
}

/// A background image placed on a level, referenced by content digest.
///
/// The image itself lives in the package's blob store; this is the reference
/// and its placement. `stackOrdinal` orders several images on one level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImage {
    /// `sha256:<lowercase hex>` (ASSET-STRATEGY §10.1).
    pub digest: String,
    #[serde(default)]
    pub anchor: Anchor,
    #[serde(default)]
    pub stack_ordinal: i64,
    /// Optional fields this version does not interpret, kept verbatim (§8 U2).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// A room's persistent metadata. FORMAT-SPEC §2.3 `roomRecord`.
///
/// # The whole authoritative field set, and nothing else
///
/// BEHAVIOR-SPEC R5.2.1 fixes it exactly: a command-supplied stable UUID,
/// `levelId`, `seedMm`, an optional display name, an optional floor-style
/// reference, an optional ceiling-style reference. Everything a user thinks of
/// as "the room" — the polygon, its area, its label anchor, whether it is the
/// face's effective record at all — is DERIVED from the walls plus this record
/// and is rebuildable (R3.0.7). A record is metadata layered on detection,
/// never a precondition for a face to exist (R5.1.7), and faces carry no
/// identity for it to point at (R5.1.8), which is why the association runs
/// through a seed POINT rather than a face id.
///
/// # Absent and empty names are different things
///
/// `name: None` and `name: Some("")` are distinct authoritative states that
/// display identically — both fall back to the token `Room` (R5.3.3). The
/// distinction is preserved rather than normalized away because F37d asserts
/// it: `{name: ""}` and `{name: null}` are two different edits with two
/// different undos.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRecord {
    pub id: Uuid,
    pub level_id: Uuid,
    /// The seed point, in plan millimetres. Editable, and its location NEVER
    /// rejects or warns (R5.2.2, R5.2.4): a seed in a wall or in the exterior
    /// is a legal record with a non-associating status.
    pub seed_mm: [f64; 2],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// An opaque, non-empty style token. No registry resolves it in v0.1 and
    /// nothing inherits it from a wall, a level or a default (R5.3.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub floor_style_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_style_ref: Option<String>,
    /// Optional fields this version does not interpret, kept verbatim (§8 U2).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl RoomRecord {
    /// A record with the supplied identity, and no name or styles.
    ///
    /// The id is a PARAMETER, unlike every other entity constructor here,
    /// because R5.2.2 forbids automatic id assignment for rooms: the command
    /// supplies the identifier and a collision rejects. A constructor that
    /// minted one would make that rule unreachable.
    pub fn new(id: Uuid, level_id: Uuid, seed: Point) -> Self {
        Self {
            id,
            level_id,
            seed_mm: seed.into(),
            name: None,
            floor_style_ref: None,
            ceiling_style_ref: None,
            unknown: Default::default(),
        }
    }

    /// The seed point.
    pub fn seed(&self) -> Point {
        self.seed_mm.into()
    }

    /// The text a label shows: the name when present and non-empty, else the
    /// R5.3.3 fallback token.
    pub fn display_name(&self) -> &str {
        match self.name.as_deref() {
            Some(n) if !n.is_empty() => n,
            _ => ROOM_LABEL_FALLBACK,
        }
    }
}

/// What a room label reads when its record has no usable name (R5.3.3).
///
/// Absent and empty-string names are authoritatively distinct and display
/// identically, so both land here.
pub const ROOM_LABEL_FALLBACK: &str = "Room";

/// Ceiling on a display string, in UTF-8 BYTES (FORMAT-SPEC §9.3 row L19).
///
/// A PARSER bound, not a semantic one: it stops a hostile package carrying a
/// megabyte of text in a field meant for a few words. It says nothing about
/// what the text means, and nothing here trims or normalises a string that
/// fits.
pub const DISPLAY_STRING_CAP_BYTES: usize = 1_024;

/// Whether `digest` is a well-formed `sha256:<64 lowercase hex>`
/// (ASSET-STRATEGY §10.1, FORMAT-SPEC §2.3).
///
/// Lowercase is part of the FORM, not a courtesy: the digest is an identity,
/// two spellings of one hash would be two identities, and the blob entry name
/// is derived from these characters directly.
///
/// `domowik-container` applies the same rule inside `blob_entry_name`. That is
/// a straightforward duplication with NO structural obstacle: the dependency
/// runs container -> format, so `blob_entry_name` could call this helper
/// today. An earlier version of this comment claimed the dependency ran the
/// other way and therefore blocked it; it does not, and the invented blocker
/// is corrected here rather than left to discourage the fix. Unifying them is
/// worth doing and is not this issue's to do.
pub fn is_well_formed_digest(digest: &str) -> bool {
    let Some(hex) = digest.strip_prefix("sha256:") else {
        return false;
    };
    hex.len() == 64
        && hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A free text annotation placed on a level. FORMAT-SPEC §2.3 `label`.
///
/// **Tier B: a structural envelope, not a behaviour.** No normative section
/// defines creating, editing or anchoring a label, and none is invented here.
/// What this delivers is the wire shape and its structural checks, so a
/// document carrying labels round-trips with its meaning intact instead of
/// being refused or silently stripped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub id: Uuid,
    pub level_id: Uuid,
    pub anchor_mm: [f64; 2],
    /// The text. Absent and empty-string are DISTINCT authoritative states
    /// that display identically, following the R5.3.3/F37d pattern the §2.3
    /// row cites.
    ///
    /// **The §2.3 row is self-contradictory here, and this type does not
    /// settle it.** The row lists `name` under REQUIRED fields while its own
    /// parenthetical distinguishes absent from empty-string, which presupposes
    /// absence is legal. Both halves cannot hold.
    ///
    /// Typed `Option` PROVISIONALLY, matching `roomRecord.name` and the half
    /// that carries semantics. That is an implementation choice made to keep
    /// the structural envelope usable, not a ruling: the question is routed to
    /// issue 0005, and if it rules `name` required, this becomes
    /// `String` and validation gains a presence check. No claim is made here
    /// that both readings hold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Optional fields this version does not interpret, kept verbatim (§8 U2).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Label {
    pub fn anchor(&self) -> Point {
        self.anchor_mm.into()
    }
}

/// A measured extent drawn on a level. FORMAT-SPEC §2.3 `dimension`.
///
/// **Tier B: a structural envelope, not a behaviour.** `startMm`/`endMm` are
/// the STORED measured extent; attaching a dimension to entities that move,
/// and what its display says, are open questions the §2.3 row records and
/// this does not answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dimension {
    pub id: Uuid,
    pub level_id: Uuid,
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    /// Text shown instead of the measured value. Bounded by L19 only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_override: Option<String>,
    /// Optional fields this version does not interpret, kept verbatim (§8 U2).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Dimension {
    pub fn start(&self) -> Point {
        self.start_mm.into()
    }

    pub fn end(&self) -> Point {
        self.end_mm.into()
    }
}

/// Where an asset instance sits. FORMAT-SPEC §2.3 `assetInstance.transform`.
///
/// # Why this is not `Anchor`, and must not become it
///
/// The §2.3 row says this shape is "structurally analogous to, but NOT derived
/// from or owned by" the §2.5 background-anchor pattern. Two types with the
/// same three fields invite exactly the merge that sentence forbids — and the
/// merge would be wrong, because a background anchor places an IMAGE under the
/// plan while this places an INSTANCE in it, and the two are free to diverge.
/// The name carries that distinction where a bare `Transform` would lose it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceTransform {
    pub translation_mm: [f64; 2],
    pub scale_uniform: f64,
    /// Degrees, counter-clockwise. NOT range-normalised: R4.0.4's unsigned
    /// `[0, 180]` is an OPENINGS rule about a hinged leaf, and borrowing it
    /// here would be inventing a constraint §2.3 does not state. Finiteness is
    /// the whole of the check.
    pub rotation_deg: f64,
    /// Optional fields this version does not interpret, kept verbatim.
    ///
    /// §8 U2 forbids silent stripping "everywhere", not only on entities, so a
    /// descriptor carries one exactly as [`Anchor`] does.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Default for InstanceTransform {
    fn default() -> Self {
        Self {
            translation_mm: [0.0, 0.0],
            scale_uniform: 1.0,
            rotation_deg: 0.0,
            unknown: Default::default(),
        }
    }
}

/// A placed instance of a content asset. FORMAT-SPEC §2.3 `assetInstance`.
///
/// **Tier B: a structural envelope, not a behaviour.** Furniture and catalogue
/// scope belong to BEHAVIOR-SPEC §10, which is still "To define", and no
/// placement semantics are implied here. What the structure DOES enable is
/// BEHAVIOR-SPEC §9.6: an instance whose blob is missing still has an
/// identity, a level and an origin, which is what a deterministic placeholder
/// and a degraded-but-selectable entity need.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetInstance {
    pub id: Uuid,
    pub level_id: Uuid,
    /// Content identity: `sha256:<64 lowercase hex>` (ASSET-STRATEGY §10.1).
    pub digest: String,
    pub transform: InstanceTransform,
    /// A human name for the instance. Bounded by L19 only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional fields this version does not interpret, kept verbatim (§8 U2).
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl AssetInstance {
    /// The instance origin, as a point.
    ///
    /// Mirrors [`Wall::start`]: stored coordinates are `[f64; 2]` throughout
    /// the format layer and callers reach a [`Point`] through an accessor, so
    /// a hit path never hand-converts.
    pub fn origin(&self) -> Point {
        self.transform.translation_mm.into()
    }
}

/// Any authoritative entity, tagged by its FORMAT-SPEC §2.3 type name.
///
/// The tag is what a `createEntity` operation carries as its payload (§4.2
/// O2), and what lets replay reconstruct an entity without knowing which
/// collection it belongs to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Entity {
    Level(Level),
    Wall(Wall),
    Opening(Opening),
    WallJoin(WallJoin),
    RoomRecord(RoomRecord),
    Label(Label),
    Dimension(Dimension),
    AssetInstance(AssetInstance),
}

impl Entity {
    /// The entity's own identifier.
    pub fn id(&self) -> Uuid {
        match self {
            Entity::Level(l) => l.id,
            Entity::Wall(w) => w.id,
            Entity::Opening(o) => o.id,
            Entity::WallJoin(j) => j.id,
            Entity::RoomRecord(r) => r.id,
            Entity::Label(l) => l.id,
            Entity::Dimension(d) => d.id,
            Entity::AssetInstance(a) => a.id,
        }
    }

    /// The §2.3 type name, as it appears in the serialized `type` field.
    pub fn type_name(&self) -> &'static str {
        match self {
            Entity::Level(_) => "level",
            Entity::Wall(_) => "wall",
            Entity::Opening(_) => "opening",
            Entity::WallJoin(_) => "wallJoin",
            Entity::RoomRecord(_) => "roomRecord",
            Entity::Label(_) => "label",
            Entity::Dimension(_) => "dimension",
            Entity::AssetInstance(_) => "assetInstance",
        }
    }
}

/// A non-blocking level advisory (BEHAVIOR-SPEC §2.1).
///
/// Distinct from [`ValidationError`] in kind, not merely in severity: §2.1
/// says these fire a warning "not an error", so a document carrying them is
/// VALID. Nothing here may ever reach a validity decision.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "kebab-case")]
pub enum ValidationWarning {
    /// The level's slab reaches below the finished-floor plane of the level
    /// beneath it.
    ///
    /// Attributed to the UPPER level: its `elevationMm` or
    /// `floorThicknessMm` is what a user changes to clear the advisory.
    #[error("level {level}: its floor slab reaches below the finished floor of level {below}")]
    #[serde(rename_all = "camelCase")]
    SlabIntrudesBelowLevel { level: Uuid, below: Uuid },
    /// §2.1: wall height and ceiling height are independent fields, and a
    /// ceiling below the walls it caps is legal but worth saying.
    #[error("level {level}: ceiling height is below the default wall height")]
    #[serde(rename_all = "camelCase")]
    CeilingBelowWallHeight { level: Uuid },
    /// Two levels at the same elevation. Reported separately from an
    /// intrusion because it is an ORDERING concern, not slab geometry: §2.2's
    /// stable-UUID tie-break is what keeps such a pair totally ordered, and
    /// merging the two advisories would report one situation under a message
    /// describing the other.
    #[error("level {level}: shares elevation with level {other}")]
    #[serde(rename_all = "camelCase")]
    DuplicateElevation { level: Uuid, other: Uuid },
}

/// Exact difference: returns `(fl(a-b), err)` with `a - b == fl(a-b) + err`.
///
/// Knuth's TwoSum applied to `a + (-b)`, error-free for all finite operands
/// that do not overflow, including subnormal results. Reproduced here rather
/// than shared because `domowik-geometry` depends on THIS crate and not the
/// other way round; the implementation and its provenance are the same as
/// `domowik-geometry`'s `two_diff` (Shewchuk 1997, "Adaptive Precision
/// Floating-Point Arithmetic and Fast Robust Geometric Predicates", §2.3).
#[inline]
fn two_diff(a: f64, b: f64) -> (f64, f64) {
    let s = a - b;
    let bv = a - s;
    (s, (a - (s + bv)) + (bv - b))
}

/// §2.1's slab-intrusion test: `elevation − thickness < below`, exactly.
///
/// # Why a filter and a residual, and where each is needed
///
/// IEEE-754 subtraction is correctly rounded and therefore monotonic, and
/// `below` is a representable value. So rounding cannot carry the difference
/// ACROSS `below`: if the exact difference were greater than `below`, the
/// nearest representable value to it cannot be smaller than `below`, and
/// symmetrically. That makes the rounded comparison sound whenever it is
/// strict —
///
/// - `fl(elevation − thickness) < below` implies the exact difference is too;
/// - `fl(elevation − thickness) > below` implies the exact difference is too.
///
/// The one undecided case is EQUALITY. When the rounded difference lands
/// exactly on `below`, the exact difference may be strictly under it — by
/// at most half the spacing to its representable predecessor (including a
/// midpoint rounded ties-to-even). Answering "no intrusion" there is a
/// false NEGATIVE: a real intrusion within the rounding interval reported as clean. The
/// residual settles it: with `elevation − thickness == s + err` exactly and
/// `s == below`, the exact difference is less than `below` precisely when
/// `err < 0`.
///
/// # Bounds and cost
///
/// Operands are bounded by `COORD_BOUND_MM` and `DIMENSION_CAP_MM`, so the
/// subtraction cannot overflow and the error-free transformation holds. No
/// dependency, allocation or arbitrary-precision arithmetic is needed. The
/// common path is one subtraction and two comparisons; only the tie calls [`two_diff`],
/// whose six floating-point operations include recomputing the difference.
fn slab_intrudes(elevation: f64, thickness: f64, below: f64) -> bool {
    let s = elevation - thickness;
    if s != below {
        return s < below;
    }
    // The tie, and only the tie: recover the residual the rounding discarded.
    let (_, err) = two_diff(elevation, thickness);
    err < 0.0
}

/// Why a document failed validation.
///
/// A BEHAVIOR-SPEC §4.4 opening diagnostic code. Variants are declared in
/// R4.4's precedence order, so the derived [`Ord`] IS the precedence: when a
/// command violates several conditions, the least is the one primary
/// diagnostic reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpeningCode {
    NonfiniteValue,
    EntityIdCollision,
    UnknownHost,
    WrongHost,
    OpeningDirectRehost,
    OpeningLevelImmutable,
    WallLevelImmutable,
    InvalidOpeningKind,
    MissingStyle,
    InvalidOpeningCenter,
    InvalidSide,
    InvalidHinge,
    InvalidOpensToward,
    InvalidSwingAngle,
    NonpositiveWidth,
    NonpositiveHeight,
    NonpositiveDepth,
    NegativeSill,
    DepthExceedsHostThickness,
    ApertureNotStrictFit,
    ApertureExceedsHostHeight,
    OpeningOverlap,
    OpeningSplitAmbiguous,
}

impl OpeningCode {
    /// The code as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NonfiniteValue => "nonfinite-value",
            Self::EntityIdCollision => "entity-id-collision",
            Self::UnknownHost => "unknown-host",
            Self::WrongHost => "wrong-host",
            Self::OpeningDirectRehost => "opening-direct-rehost",
            Self::OpeningLevelImmutable => "opening-level-immutable",
            Self::WallLevelImmutable => "wall-level-immutable",
            Self::InvalidOpeningKind => "invalid-opening-kind",
            Self::MissingStyle => "missing-style",
            Self::InvalidOpeningCenter => "invalid-opening-center",
            Self::InvalidSide => "invalid-side",
            Self::InvalidHinge => "invalid-hinge",
            Self::InvalidOpensToward => "invalid-opens-toward",
            Self::InvalidSwingAngle => "invalid-swing-angle",
            Self::NonpositiveWidth => "nonpositive-width",
            Self::NonpositiveHeight => "nonpositive-height",
            Self::NonpositiveDepth => "nonpositive-depth",
            Self::NegativeSill => "negative-sill",
            Self::DepthExceedsHostThickness => "depth-exceeds-host-thickness",
            Self::ApertureNotStrictFit => "aperture-not-strict-fit",
            Self::ApertureExceedsHostHeight => "aperture-exceeds-host-height",
            Self::OpeningOverlap => "opening-overlap",
            Self::OpeningSplitAmbiguous => "opening-split-ambiguous",
        }
    }

    /// The 1-based rank in R4.4's precedence list.
    pub fn rank(self) -> u8 {
        self as u8 + 1
    }
}

impl std::fmt::Display for OpeningCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The along-axis parameter of `p` on the line `a → b` (0 at `a`, 1 at
/// `b`), by projection; 0 for a degenerate line.
pub fn param_along(a: Point, b: Point, p: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return 0.0;
    }
    ((p.x - a.x) * dx + (p.y - a.y) * dy) / len2
}

/// R4.3.3's remap of an opening's `u` from a host of length `length` onto
/// the child spanning `[s, e]` mm along it. The ONE formula the split
/// command uses and admission verifies a host change against, so the two
/// agree bit for bit.
pub fn remap_u(u: f64, length: f64, s: f64, e: f64) -> f64 {
    (u * length - s) / (e - s)
}

/// Each variant names the constraint and the entity, because a validator that
/// says only "invalid" makes the user hunt for what it already knew.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum ValidationError {
    /// A materialized user plan, unlike an internal builder, needs a level.
    #[error("a plan must contain at least one level; create a level before saving")]
    NoLevels,
    #[error("wall {id}: start and end are the same point ({x}, {y}); FORMAT-SPEC §3 requires startMm ≠ endMm")]
    ZeroLengthWall { id: Uuid, x: f64, y: f64 },

    #[error("wall {id}: {field} is {value} mm; must be finite and in (0, {cap}]")]
    DimensionOutOfRange {
        id: Uuid,
        field: &'static str,
        value: f64,
        cap: f64,
    },

    #[error("wall {id}: {endpoint} coordinate is out of bounds or not finite; each component must satisfy |v| ≤ {bound} mm")]
    CoordinateOutOfBounds {
        id: Uuid,
        endpoint: &'static str,
        bound: f64,
    },

    #[error("wall {wall_id} references level {level_id}, which is not in the document")]
    DanglingLevelRef { wall_id: Uuid, level_id: Uuid },

    #[error("opening {opening_id} is hosted by wall {wall_id}, which is not in the document")]
    DanglingHostWall { opening_id: Uuid, wall_id: Uuid },

    #[error("opening {id}: u is {u}; must be in [0, 1]")]
    OpeningParamOutOfRange { id: Uuid, u: f64 },

    #[error(
        "opening {opening_id}: sill + height reaches {top:.0} mm, but host wall \
         {wall_id} is only {host_height:.0} mm high"
    )]
    ApertureExceedsHostHeight {
        /// The offending opening.
        opening_id: Uuid,
        /// Its host.
        wall_id: Uuid,
        /// `sillMm + heightMm`.
        top: f64,
        /// The host's height.
        host_height: f64,
    },

    #[error("opening {id}: swingDeg is {value}; must be finite and in [0, 180]")]
    InvalidSwingAngle {
        /// The offending opening.
        id: Uuid,
        /// The value found.
        value: f64,
    },

    #[error("opening {id}: mechanism {mechanism:?} is incompatible with {kind:?}")]
    InvalidOpeningMechanism {
        id: Uuid,
        kind: OpeningKind,
        mechanism: OpeningMechanism,
    },

    #[error(
        "opening {opening_id}: depthMm is {depth} mm but host wall {wall_id} is \
         only {thickness} mm thick"
    )]
    DepthExceedsHostThickness {
        /// The offending opening.
        opening_id: Uuid,
        /// Its host.
        wall_id: Uuid,
        /// The opening's depth.
        depth: f64,
        /// The host's thickness.
        thickness: f64,
    },

    #[error("level {id}: {field} is {value} mm; must be finite and within {cap} mm")]
    LevelDimensionOutOfRange {
        /// The offending level.
        id: Uuid,
        /// Serialized field name.
        field: &'static str,
        /// The value found.
        value: f64,
        /// The ratified cap (ED-0100).
        cap: f64,
    },

    /// The level-list ordinal (issue 0115) is a count, not a dimension: its
    /// bound is the JSON/JS-exact integer range, in whole units.
    #[error("level {id}: displayOrder is {value}; must be an integer in 0..={bound}")]
    LevelDisplayOrderOutOfRange {
        /// The offending level.
        id: Uuid,
        value: u64,
        bound: u64,
    },

    #[error("opening {id}: {field} is {value} mm; must be finite and in (0, {cap}]")]
    OpeningDimensionOutOfRange {
        id: Uuid,
        field: &'static str,
        value: f64,
        cap: f64,
    },

    #[error(
        "opening {id} is {width:.0} mm wide at u={u:.1} on a {host_length:.0} mm wall, so it \
         runs from {start:.0} to {end:.0} mm and does not fit within the wall"
    )]
    OpeningDoesNotFitHost {
        id: Uuid,
        width: f64,
        u: f64,
        host_length: f64,
        start: f64,
        end: f64,
    },

    #[error("openings {a} and {b} overlap on wall {wall_id}")]
    OverlappingOpenings { a: Uuid, b: Uuid, wall_id: Uuid },

    #[error("duplicate entity id {id}")]
    DuplicateId { id: Uuid },

    #[error("join {id} has {count} member(s); BEHAVIOR-SPEC R3.3.1 requires at least two distinct wall endpoints")]
    JoinTooFewMembers { id: Uuid, count: usize },

    #[error("join {join_id} references wall {wall_id}, which is not in the document")]
    JoinDanglingWall { join_id: Uuid, wall_id: Uuid },

    #[error("join {join_id} contains both endpoints of wall {wall_id}; a wall cannot join to itself (R3.3.3, degenerate-join-membership)")]
    JoinSelfMembership { join_id: Uuid, wall_id: Uuid },

    #[error("wall {wall_id} {role:?} belongs to more than one join; R3.3.2 allows at most one")]
    JoinMemberInTwoJoins { wall_id: Uuid, role: Role },

    #[error("join {join_id}: members do not meet at one point; R3.3.4 requires every member endpoint to fall in the same {tolerance} mm topology cell")]
    JoinCoordinateDisagreement { join_id: Uuid, tolerance: f64 },

    #[error("join {join_id} spans levels; all members must be on one level because levels never interact (R3.0.6)")]
    JoinCrossesLevels { join_id: Uuid },

    /// R5.2.2's `nonfinite-coordinate`: the seed is not a number.
    ///
    /// Seed LOCATION never rejects (R5.2.4) — in a wall, on a boundary or out
    /// in the exterior are all legal and merely change the derived status.
    /// What rejects is a seed that is not a coordinate at all.
    #[error("room record {id}: seedMm ({x}, {y}) is not finite (nonfinite-coordinate)")]
    RoomSeedNotFinite { id: Uuid, x: f64, y: f64 },

    /// FORMAT-SPEC §2.3's magnitude bound on `seedMm`, which is a DIFFERENT
    /// rejection from the one above and now says so.
    ///
    /// It used to share `nonfinite-coordinate`, which told a reader their
    /// perfectly finite 2 x 10⁷ mm seed was "not finite" — a diagnostic that
    /// describes the wrong defect sends the reader to the wrong fix.
    #[error("room record {id}: seedMm ({x}, {y}) is outside the coordinate domain; each component must satisfy |v| ≤ {bound} mm (coordinate-out-of-range)")]
    RoomSeedOutOfBounds {
        id: Uuid,
        x: f64,
        y: f64,
        bound: f64,
    },

    #[error("room record {room_id} references level {level_id}, which is not in the document")]
    RoomDanglingLevelRef { room_id: Uuid, level_id: Uuid },

    /// R5.2.2/R5.3.4's `invalid-style-reference`.
    ///
    /// Absent means unspecified and is legal; the empty string is not a token.
    /// Clearing a style is `null`, never `""`.
    #[error("room record {id}: {field} is the empty string; a style reference is an optional NON-EMPTY opaque token, and clearing one uses null (invalid-style-reference)")]
    InvalidStyleReference { id: Uuid, field: &'static str },

    /// FORMAT-SPEC §2.3: `styleRefs` is a set of NON-EMPTY opaque tokens.
    #[error("{kind} {id}: styleRefs contains an empty token; tokens are non-empty opaque strings (invalid-style-reference)")]
    EmptyStyleRef { id: Uuid, kind: &'static str },

    /// C3: a set serialized as an array rejects duplicates deterministically.
    /// Order is not semantic, so two copies of one token are not two members —
    /// they are a malformed set.
    #[error("{kind} {id}: styleRefs contains {token:?} more than once; it is a SET, and permutations are equivalent while duplicates are invalid (C3)")]
    DuplicateStyleRef {
        id: Uuid,
        kind: &'static str,
        token: String,
    },

    #[error("{kind} {id} references level {level_id}, which is not in the document")]
    TierBDanglingLevelRef {
        id: Uuid,
        kind: &'static str,
        level_id: Uuid,
    },

    #[error("{kind} {id}: {field} coordinate is out of bounds or not finite; each component must satisfy |v| ≤ {bound} mm")]
    TierBCoordinateOutOfBounds {
        id: Uuid,
        kind: &'static str,
        field: &'static str,
        bound: f64,
    },

    /// L19: a display string is bounded in BYTES, not characters — the cap
    /// exists to stop a hostile package, and a character count would let one
    /// through four times over.
    #[error("{kind} {id}: {field} is {bytes} UTF-8 bytes; the limit is {cap} (limit-string-length, §9.3 L19)")]
    DisplayStringTooLong {
        id: Uuid,
        kind: &'static str,
        field: &'static str,
        bytes: usize,
        cap: usize,
    },

    #[error("asset instance {id}: digest {digest:?} is not a well-formed sha256:<64 lowercase hex> (ASSET-STRATEGY §10.1)")]
    MalformedDigest { id: Uuid, digest: String },

    #[error("asset instance {id}: transform.{field} is {value}; must be {requirement}")]
    InvalidInstanceTransform {
        id: Uuid,
        field: &'static str,
        value: f64,
        requirement: &'static str,
    },
}

/// A complete floor plan.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    /// Stable document identity (issue 0116).
    ///
    /// Minted by the new-plan entry points, carried by every save, compaction
    /// and package conversion, and never written by any user command. Absent
    /// in documents created before issue 0116; readers of those simply find
    /// nothing here, and per-device storage keyed by it (issue 0117) falls
    /// back to the elevation default. UUID v7, so identities sort by time.
    ///
    /// Typed rather than left in `unknown` for the same reason the entity
    /// kinds are: this version interprets it, and an opaque field could not
    /// key per-device state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<Uuid>,
    #[serde(default)]
    pub levels: Vec<Level>,
    #[serde(default)]
    pub walls: Vec<Wall>,
    #[serde(default)]
    pub openings: Vec<Opening>,
    #[serde(default)]
    pub joins: Vec<WallJoin>,
    /// Free text annotations (FORMAT-SPEC §2.3 Tier B).
    #[serde(default)]
    pub labels: Vec<Label>,
    /// Measured extents (FORMAT-SPEC §2.3 Tier B).
    #[serde(default)]
    pub dimensions: Vec<Dimension>,
    /// Placed content instances (FORMAT-SPEC §2.3 Tier B).
    #[serde(default)]
    pub asset_instances: Vec<AssetInstance>,
    /// Room metadata records (BEHAVIOR-SPEC §5.2).
    ///
    /// Authoritative entities like the rest, and deliberately NOT the rooms
    /// themselves: a face exists whether or not a record seeds it, and a
    /// record persists whether or not any face contains its seed.
    #[serde(default)]
    pub room_records: Vec<RoomRecord>,
    /// Unrecognised top-level document fields, carried verbatim.
    ///
    /// FORMAT-SPEC §8.4 U2, at the level the entity types already honour. The
    /// nested types have carried their own `unknown` maps since v0.1; the
    /// document around them did not, so a field a later version adds beside
    /// `walls` was read, discarded, and written back absent. Silent stripping
    /// is forbidden everywhere, and "everywhere" includes the envelope.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// A document with one ground-floor level, which is what "a new plan" means
    /// to a user: somewhere to put a wall.
    ///
    /// This is the new-plan entry point the CLI `new` command and the editor's
    /// `domowik_plan_create_default` baseline share, so both mint a document
    /// identity here (issue 0116). Documents built for tests or by direct
    /// struct construction keep `documentId` absent unless calling code
    /// supplies one; that is the pre-0116 shape and is what the absent branch
    /// of every consumer describes.
    pub fn with_ground_floor() -> Self {
        let mut doc = Self::new();
        doc.document_id = Some(Uuid::now_v7());
        doc.levels.push(Level::new("Level 1"));
        doc
    }

    /// The number of entities the document holds, across every type.
    ///
    /// Counts what the v0.1 logical model defines as an entity: levels, walls,
    /// openings, joins and room records. Room FACES are derived geometry and
    /// are not entities (R5.1.7/R5.1.8); the records that name and style them
    /// are (R5.2.1), and the two counts are unrelated — a level may hold four
    /// faces and no records, or a record whose seed sits in the exterior.
    pub fn entity_count(&self) -> usize {
        self.levels.len()
            + self.walls.len()
            + self.openings.len()
            + self.joins.len()
            + self.room_records.len()
            + self.labels.len()
            + self.dimensions.len()
            + self.asset_instances.len()
    }

    pub fn level(&self, id: Uuid) -> Option<&Level> {
        self.levels.iter().find(|l| l.id == id)
    }

    pub fn wall(&self, id: Uuid) -> Option<&Wall> {
        self.walls.iter().find(|w| w.id == id)
    }

    /// Walls belonging to `level_id`, in document order.
    /// The room record with `id`, if the document holds one.
    pub fn room_record(&self, id: Uuid) -> Option<&RoomRecord> {
        self.room_records.iter().find(|r| r.id == id)
    }

    /// The asset instance with `id`, if the document holds one.
    pub fn asset_instance(&self, id: Uuid) -> Option<&AssetInstance> {
        self.asset_instances.iter().find(|a| a.id == id)
    }

    /// Asset instances living on `level_id`, in document order.
    pub fn asset_instances_on_level(&self, level_id: Uuid) -> impl Iterator<Item = &AssetInstance> {
        self.asset_instances
            .iter()
            .filter(move |a| a.level_id == level_id)
    }

    /// Room records living on `level_id`, in document order.
    pub fn room_records_on_level(&self, level_id: Uuid) -> impl Iterator<Item = &RoomRecord> {
        self.room_records
            .iter()
            .filter(move |r| r.level_id == level_id)
    }

    pub fn walls_on_level(&self, level_id: Uuid) -> impl Iterator<Item = &Wall> {
        self.walls.iter().filter(move |w| w.level_id == level_id)
    }

    /// Non-blocking level advisories (BEHAVIOR-SPEC §2.1).
    ///
    /// WARNINGS, never errors: §2.1 says each of these "fires a validation
    /// warning (not an error)", so a document carrying them is valid and must
    /// still save, open and accept edits. They are returned from their own
    /// accessor for exactly that reason — folding them into
    /// [`Self::validate`] would turn a legal plan into a rejected one and
    /// break the admission checks built on it.
    ///
    /// # Order
    ///
    /// Levels are considered in §2.2's canonical order — ascending elevation,
    /// ties broken by ascending stable UUID — because §2.1's `n` / `n−1`
    /// wording presumes it, and without a total order "the level below" is
    /// undefined for two levels at the same height. Per level the order is:
    /// ceiling advisory, then duplicate elevation against the immediate
    /// predecessor, then slab intrusion against that same predecessor.
    ///
    /// # Skipping
    ///
    /// A warning whose operands are invalid for its own comparison is not
    /// emitted: a non-finite elevation or a negative thickness is a
    /// [`ValidationError`], and reporting an advisory computed from it would
    /// describe geometry the document does not have. Each advisory checks
    /// only the fields it reads, so an invalid ceiling height does not
    /// suppress an intrusion warning.
    pub fn warnings(&self) -> Vec<ValidationWarning> {
        // Levels whose elevation is not a legal coordinate are PARTITIONED
        // out, not merely tolerated by the comparator. A `partial_cmp`
        // falling back to `Equal` on NaN is NON-TRANSITIVE — `a == NaN` and
        // `NaN == b` while `a < b` — which makes the sort's output depend on
        // insertion order and, with a debug-checked sort, can abort. An
        // invalid document is exactly when a diagnostic is most wanted, so it
        // must not be the case that misbehaves.
        //
        // Diagnostic-only ordering policy: valid levels first in ascending
        // elevation with a stable-UUID tie-break (signed zeros tie, since
        // −0 == +0), then the invalid group by UUID alone. Keeping the valid
        // levels CONTIGUOUS matters — an invalid elevation must never sit
        // between two valid neighbours and break the predecessor adjacency
        // §2.1's comparisons are defined over.
        let legal_elevation =
            |l: &Level| l.elevation_mm.is_finite() && l.elevation_mm.abs() <= COORD_BOUND_MM;
        let (mut ordered, mut invalid): (Vec<&Level>, Vec<&Level>) =
            self.levels.iter().partition(|l| legal_elevation(l));
        ordered.sort_by(|a, b| {
            // Both finite here, so the fallback is unreachable and the order
            // is a total one.
            a.elevation_mm
                .partial_cmp(&b.elevation_mm)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });
        invalid.sort_by_key(|l| l.id);
        ordered.append(&mut invalid);

        // §2.1: "Evaluate each condition only when its operands satisfy their
        // FIELD CONSTRAINTS" — not merely when they are finite. An infinite
        // height is greater than zero and would otherwise pass a finiteness
        // test while being exactly what a validation error already reports.
        let positive = |v: f64| v.is_finite() && v > 0.0 && v <= DIMENSION_CAP_MM;
        let non_negative = |v: f64| v.is_finite() && (0.0..=DIMENSION_CAP_MM).contains(&v);
        let coordinate = |v: f64| v.is_finite() && v.abs() <= COORD_BOUND_MM;

        let mut out = Vec::new();
        for (index, level) in ordered.iter().enumerate() {
            if positive(level.default_ceiling_height_mm)
                && positive(level.default_wall_height_mm)
                && level.default_ceiling_height_mm < level.default_wall_height_mm
            {
                out.push(ValidationWarning::CeilingBelowWallHeight { level: level.id });
            }
            let Some(below) = index.checked_sub(1).map(|i| ordered[i]) else {
                continue;
            };
            if !coordinate(level.elevation_mm) || !coordinate(below.elevation_mm) {
                continue;
            }
            if level.elevation_mm == below.elevation_mm {
                out.push(ValidationWarning::DuplicateElevation {
                    level: level.id,
                    other: below.id,
                });
            }
            // The LOWER level's thickness is deliberately unread: §2.1 tests
            // intrusion below a PLANE, so a zero-thickness slab beneath does
            // not suppress the warning.
            if non_negative(level.floor_thickness_mm)
                && slab_intrudes(
                    level.elevation_mm,
                    level.floor_thickness_mm,
                    below.elevation_mm,
                )
            {
                out.push(ValidationWarning::SlabIntrudesBelowLevel {
                    level: level.id,
                    below: below.id,
                });
            }
        }
        out
    }

    /// Validates a materialized user plan, including its required level.
    ///
    /// Use this for user-facing validation. Empty replay snapshots and
    /// incremental builders instead use [Self::validate]; their validity is
    /// assessed after replay/materialization, not from an intermediate state.
    pub fn validate_plan(&self) -> Vec<ValidationError> {
        let mut errors = self.validate();
        if self.levels.is_empty() {
            errors.insert(0, ValidationError::NoLevels);
        }
        errors
    }

    /// Validates entity-local constraints, allowing an empty internal builder.
    ///
    /// Returns ALL violations rather than the first, so a user fixing a file
    /// sees the whole list instead of rediscovering it one run at a time.
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for id in self
            .levels
            .iter()
            .map(|l| l.id)
            .chain(self.walls.iter().map(|w| w.id))
            .chain(self.openings.iter().map(|o| o.id))
            .chain(self.joins.iter().map(|j| j.id))
            .chain(self.room_records.iter().map(|r| r.id))
            .chain(self.labels.iter().map(|l| l.id))
            .chain(self.dimensions.iter().map(|d| d.id))
            .chain(self.asset_instances.iter().map(|a| a.id))
        {
            if !seen.insert(id) {
                errors.push(ValidationError::DuplicateId { id });
            }
        }

        // Level numerics were unchecked entirely until ED-0100: a level could
        // carry a NaN elevation or a negative slab thickness and validate
        // clean, then poison every derived elevation that read it.
        for level in &self.levels {
            // The display ordinal (issue 0115) is a count, not a millimetre
            // measure, so it is bounded by ITS OWN constant, not a cap above.
            if let Some(order) = level.display_order {
                if order > MAX_SAFE_INTEGER {
                    errors.push(ValidationError::LevelDisplayOrderOutOfRange {
                        id: level.id,
                        value: order,
                        bound: MAX_SAFE_INTEGER,
                    });
                }
            }

            // The three ranges are NOT the same, and FORMAT-SPEC §3 is
            // precise about it: `elevationMm` is a COORDINATE, so it is
            // bounded by `coordBoundMm` and may be negative for a basement;
            // `floorThicknessMm` admits ZERO, because a level may
            // legitimately have no slab of its own; only the two defaults are
            // strictly positive. An earlier version of this loop applied the
            // dimension cap to elevation and rejected a zero slab, both of
            // which contradicted the specification.
            for (field, value, range) in [
                ("elevationMm", level.elevation_mm, LevelRange::Coordinate),
                (
                    "floorThicknessMm",
                    level.floor_thickness_mm,
                    LevelRange::NonNegative,
                ),
                (
                    "defaultWallHeightMm",
                    level.default_wall_height_mm,
                    LevelRange::Positive,
                ),
                (
                    "defaultCeilingHeightMm",
                    level.default_ceiling_height_mm,
                    LevelRange::Positive,
                ),
            ] {
                let out_of_range = match range {
                    LevelRange::Coordinate => !value.is_finite() || value.abs() > COORD_BOUND_MM,
                    LevelRange::NonNegative => {
                        !value.is_finite() || value < 0.0 || value > DIMENSION_CAP_MM
                    }
                    LevelRange::Positive => {
                        !value.is_finite() || value <= 0.0 || value > DIMENSION_CAP_MM
                    }
                };
                if out_of_range {
                    errors.push(ValidationError::LevelDimensionOutOfRange {
                        id: level.id,
                        field,
                        value,
                        cap: match range {
                            LevelRange::Coordinate => COORD_BOUND_MM,
                            _ => DIMENSION_CAP_MM,
                        },
                    });
                }
            }
        }

        for wall in &self.walls {
            let (start, end) = (wall.start(), wall.end());

            if !start.is_within_bounds() {
                errors.push(ValidationError::CoordinateOutOfBounds {
                    id: wall.id,
                    endpoint: "start",
                    bound: COORD_BOUND_MM,
                });
            }
            if !end.is_within_bounds() {
                errors.push(ValidationError::CoordinateOutOfBounds {
                    id: wall.id,
                    endpoint: "end",
                    bound: COORD_BOUND_MM,
                });
            }
            if start == end {
                errors.push(ValidationError::ZeroLengthWall {
                    id: wall.id,
                    x: start.x,
                    y: start.y,
                });
            }
            for (field, value) in [
                ("thicknessMm", wall.thickness_mm),
                ("heightMm", wall.height_mm),
            ] {
                if !value.is_finite() || value <= 0.0 || value > DIMENSION_CAP_MM {
                    errors.push(ValidationError::DimensionOutOfRange {
                        id: wall.id,
                        field,
                        value,
                        cap: DIMENSION_CAP_MM,
                    });
                }
            }
            if self.level(wall.level_id).is_none() {
                errors.push(ValidationError::DanglingLevelRef {
                    wall_id: wall.id,
                    level_id: wall.level_id,
                });
            }
        }

        // Style references on the two kinds that carry them. One helper, so
        // a wall and an opening cannot end up with different ideas of what a
        // token set is.
        for wall in &self.walls {
            check_style_refs(&mut errors, wall.id, "wall", &wall.style_refs);
        }
        for opening in &self.openings {
            check_style_refs(&mut errors, opening.id, "opening", &opening.style_refs);
        }

        // Tier B kinds: STRUCTURAL checks only. FORMAT-SPEC is explicit that
        // these carry "structural bounds only: finiteness, L19 string caps,
        // §3.1 parameter references", because no normative section defines
        // what they MEAN yet. Validating more would be inventing semantics.
        for label in &self.labels {
            if !label.anchor().is_within_bounds() {
                errors.push(ValidationError::TierBCoordinateOutOfBounds {
                    id: label.id,
                    kind: "label",
                    field: "anchorMm",
                    bound: COORD_BOUND_MM,
                });
            }
            check_display_string(&mut errors, label.id, "label", "name", &label.name);
            if self.level(label.level_id).is_none() {
                errors.push(ValidationError::TierBDanglingLevelRef {
                    id: label.id,
                    kind: "label",
                    level_id: label.level_id,
                });
            }
        }
        for dimension in &self.dimensions {
            for (field, point) in [("startMm", dimension.start()), ("endMm", dimension.end())] {
                if !point.is_within_bounds() {
                    errors.push(ValidationError::TierBCoordinateOutOfBounds {
                        id: dimension.id,
                        kind: "dimension",
                        field,
                        bound: COORD_BOUND_MM,
                    });
                }
            }
            check_display_string(
                &mut errors,
                dimension.id,
                "dimension",
                "labelOverride",
                &dimension.label_override,
            );
            if self.level(dimension.level_id).is_none() {
                errors.push(ValidationError::TierBDanglingLevelRef {
                    id: dimension.id,
                    kind: "dimension",
                    level_id: dimension.level_id,
                });
            }
        }
        for instance in &self.asset_instances {
            if !is_well_formed_digest(&instance.digest) {
                errors.push(ValidationError::MalformedDigest {
                    id: instance.id,
                    digest: instance.digest.clone(),
                });
            }
            if !instance.origin().is_within_bounds() {
                errors.push(ValidationError::TierBCoordinateOutOfBounds {
                    id: instance.id,
                    kind: "assetInstance",
                    field: "transform.translationMm",
                    bound: COORD_BOUND_MM,
                });
            }
            let scale = instance.transform.scale_uniform;
            if !scale.is_finite() || scale <= 0.0 {
                errors.push(ValidationError::InvalidInstanceTransform {
                    id: instance.id,
                    field: "scaleUniform",
                    value: scale,
                    requirement: "finite and greater than zero",
                });
            }
            // Finiteness only. The range is deliberately unconstrained: see
            // `InstanceTransform::rotation_deg`.
            let rotation = instance.transform.rotation_deg;
            if !rotation.is_finite() {
                errors.push(ValidationError::InvalidInstanceTransform {
                    id: instance.id,
                    field: "rotationDeg",
                    value: rotation,
                    requirement: "finite",
                });
            }
            check_display_string(
                &mut errors,
                instance.id,
                "assetInstance",
                "displayName",
                &instance.display_name,
            );
            if self.level(instance.level_id).is_none() {
                errors.push(ValidationError::TierBDanglingLevelRef {
                    id: instance.id,
                    kind: "assetInstance",
                    level_id: instance.level_id,
                });
            }
        }

        for record in &self.room_records {
            let seed = record.seed();
            // Two distinct defects, two distinct diagnostics: "not a number"
            // and "outside the domain" send a reader to different fixes.
            if !seed.x.is_finite() || !seed.y.is_finite() {
                errors.push(ValidationError::RoomSeedNotFinite {
                    id: record.id,
                    x: seed.x,
                    y: seed.y,
                });
            } else if !seed.is_within_bounds() {
                errors.push(ValidationError::RoomSeedOutOfBounds {
                    id: record.id,
                    x: seed.x,
                    y: seed.y,
                    bound: COORD_BOUND_MM,
                });
            }
            for (field, value) in [
                ("floorStyleRef", record.floor_style_ref.as_deref()),
                ("ceilingStyleRef", record.ceiling_style_ref.as_deref()),
            ] {
                if value == Some("") {
                    errors.push(ValidationError::InvalidStyleReference {
                        id: record.id,
                        field,
                    });
                }
            }
            if self.level(record.level_id).is_none() {
                errors.push(ValidationError::RoomDanglingLevelRef {
                    room_id: record.id,
                    level_id: record.level_id,
                });
            }
        }

        for opening in &self.openings {
            if let Some(mechanism) = opening.mechanism {
                if !mechanism.accepts(opening.kind) {
                    errors.push(ValidationError::InvalidOpeningMechanism {
                        id: opening.id,
                        kind: opening.kind,
                        mechanism,
                    });
                }
            }
            if !(0.0..=1.0).contains(&opening.u) || !opening.u.is_finite() {
                errors.push(ValidationError::OpeningParamOutOfRange {
                    id: opening.id,
                    u: opening.u,
                });
            }
            for (field, value) in [
                ("widthMm", opening.width_mm),
                ("heightMm", opening.height_mm),
            ] {
                if !value.is_finite() || value <= 0.0 || value > DIMENSION_CAP_MM {
                    errors.push(ValidationError::OpeningDimensionOutOfRange {
                        id: opening.id,
                        field,
                        value,
                        cap: DIMENSION_CAP_MM,
                    });
                }
            }
            if !opening.sill_mm.is_finite()
                || opening.sill_mm < 0.0
                || opening.sill_mm > DIMENSION_CAP_MM
            {
                errors.push(ValidationError::OpeningDimensionOutOfRange {
                    id: opening.id,
                    field: "sillMm",
                    value: opening.sill_mm,
                    cap: DIMENSION_CAP_MM,
                });
            }

            // R4.0.4 swing: unsigned, finite, in [0, 180].
            if !opening.swing_deg.is_finite()
                || opening.swing_deg < 0.0
                || opening.swing_deg > 180.0
            {
                errors.push(ValidationError::InvalidSwingAngle {
                    id: opening.id,
                    value: opening.swing_deg,
                });
            }

            // R4.0.4 depth: strictly positive, finite, within the cap. The
            // `depthMm <= host.thicknessMm` half is checked below, where the
            // host is resolved.
            if !opening.depth_mm.is_finite()
                || opening.depth_mm <= 0.0
                || opening.depth_mm > DIMENSION_CAP_MM
            {
                errors.push(ValidationError::OpeningDimensionOutOfRange {
                    id: opening.id,
                    field: "depthMm",
                    value: opening.depth_mm,
                    cap: DIMENSION_CAP_MM,
                });
            }

            match self.wall(opening.host_wall_id) {
                None => errors.push(ValidationError::DanglingHostWall {
                    opening_id: opening.id,
                    wall_id: opening.host_wall_id,
                }),
                Some(host) => {
                    // R4.0.4: the cut cannot be deeper than the wall it cuts.
                    if opening.depth_mm.is_finite()
                        && host.thickness_mm.is_finite()
                        && opening.depth_mm > host.thickness_mm
                    {
                        errors.push(ValidationError::DepthExceedsHostThickness {
                            opening_id: opening.id,
                            wall_id: host.id,
                            depth: opening.depth_mm,
                            thickness: host.thickness_mm,
                        });
                    }

                    // An opening wider than its wall, or hanging off the end,
                    // cannot be cut. Catching it here means the renderer and
                    // any future 3D pass can assume the cut is well-formed.
                    // R4.1.3 top check, INCLUSIVE: equality is legal, so a
                    // 400 mm transom on a 2100 sill fits a 2500 wall exactly.
                    if opening.sill_mm.is_finite()
                        && opening.height_mm.is_finite()
                        && host.height_mm.is_finite()
                        && opening.sill_mm + opening.height_mm > host.height_mm
                    {
                        errors.push(ValidationError::ApertureExceedsHostHeight {
                            opening_id: opening.id,
                            wall_id: host.id,
                            top: opening.sill_mm + opening.height_mm,
                            host_height: host.height_mm,
                        });
                    }

                    let len = host.length_mm();
                    if let Some((s, e)) = opening.span(len) {
                        // R4.1.2 is STRICT: positive clearance from BOTH host
                        // endpoints is mandatory, so zero clearance rejects.
                        // A 900 mm opening centred on a 900 mm wall touches
                        // both ends and is not a fit, however much it looks
                        // like one.
                        if s <= 0.0 || e >= 1.0 {
                            errors.push(ValidationError::OpeningDoesNotFitHost {
                                id: opening.id,
                                width: opening.width_mm,
                                u: opening.u,
                                host_length: len,
                                start: s * len,
                                end: e * len,
                            });
                        }
                    }
                }
            }
        }

        // R4.1.4 overlap matrix. Two openings on one host conflict IFF BOTH
        // their along-axis AND their vertical intervals intersect with
        // POSITIVE length.
        //
        // The vertical half is not optional and its absence was a real bug: a
        // transom sitting exactly on a door's head shares the whole horizontal
        // span, and checking only that axis rejected it. R4.1.4's own example
        // is precisely this — a transom at sill 2100 above a 2100-high door
        // commits, and the same transom at sill 2099 rejects.
        //
        // Touching in EITHER axis is legal regardless of the other, which is
        // why both tests are strict.
        for (i, a) in self.openings.iter().enumerate() {
            for b in self.openings.iter().skip(i + 1) {
                if a.host_wall_id != b.host_wall_id {
                    continue;
                }
                let Some(host) = self.wall(a.host_wall_id) else {
                    continue;
                };
                let len = host.length_mm();
                let (Some((a0, a1)), Some((b0, b1))) = (a.span(len), b.span(len)) else {
                    continue;
                };
                let horizontal = a0 < b1 && b0 < a1;
                let (a_low, a_high) = (a.sill_mm, a.sill_mm + a.height_mm);
                let (b_low, b_high) = (b.sill_mm, b.sill_mm + b.height_mm);
                let vertical = a_low < b_high && b_low < a_high;
                if horizontal && vertical {
                    errors.push(ValidationError::OverlappingOpenings {
                        a: a.id,
                        b: b.id,
                        wall_id: a.host_wall_id,
                    });
                }
            }
        }

        errors.extend(self.validate_joins());
        errors
    }

    /// BEHAVIOR-SPEC R3.3: join membership, self-join, single membership,
    /// coordinate agreement, and the level rule.
    fn validate_joins(&self) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        let mut owner: std::collections::HashMap<JoinMember, Uuid> =
            std::collections::HashMap::new();

        for join in &self.joins {
            let mut unique: Vec<JoinMember> = Vec::new();
            for m in &join.members {
                if !unique.contains(m) {
                    unique.push(*m);
                }
            }
            if unique.len() < 2 {
                errors.push(ValidationError::JoinTooFewMembers {
                    id: join.id,
                    count: unique.len(),
                });
            }

            // R3.3.3: both endpoints of one wall in one join is degenerate.
            for (i, a) in unique.iter().enumerate() {
                for b in unique.iter().skip(i + 1) {
                    if a.wall_id == b.wall_id {
                        errors.push(ValidationError::JoinSelfMembership {
                            join_id: join.id,
                            wall_id: a.wall_id,
                        });
                    }
                }
            }

            // R3.3.2: an endpoint belongs to at most one join.
            for m in &unique {
                match owner.get(m) {
                    Some(other) if *other != join.id => {
                        errors.push(ValidationError::JoinMemberInTwoJoins {
                            wall_id: m.wall_id,
                            role: m.role,
                        });
                    }
                    _ => {
                        owner.insert(*m, join.id);
                    }
                }
            }

            let mut points: Vec<Point> = Vec::with_capacity(unique.len());
            let mut levels: Vec<Uuid> = Vec::with_capacity(unique.len());
            let mut dangling = false;
            for m in &unique {
                match self.wall(m.wall_id) {
                    None => {
                        dangling = true;
                        errors.push(ValidationError::JoinDanglingWall {
                            join_id: join.id,
                            wall_id: m.wall_id,
                        });
                    }
                    Some(w) => {
                        points.push(match m.role {
                            Role::Start => w.start(),
                            Role::End => w.end(),
                        });
                        if !levels.contains(&w.level_id) {
                            levels.push(w.level_id);
                        }
                    }
                }
            }

            if dangling {
                // Coordinate agreement is unanswerable with a member missing;
                // reporting it as well would be noise on top of the real fault.
                continue;
            }
            if levels.len() > 1 {
                errors.push(ValidationError::JoinCrossesLevels { join_id: join.id });
            }
            // R3.3.4 under ED-0103 N-2: members agree when they are the SAME
            // POINT, and "same point" is the topology grid, not a distance.
            // A non-finite coordinate has no cell; the finiteness diagnostics
            // report it and piling a second complaint on one fault helps no one.
            if points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
                let first = points.first().map(|p| topology_cell(*p));
                if points.iter().any(|p| Some(topology_cell(*p)) != first) {
                    errors.push(ValidationError::JoinCoordinateDisagreement {
                        join_id: join.id,
                        tolerance: JOIN_COINCIDENCE_MM,
                    });
                }
            }
        }

        errors
    }

    /// Inserts `entity` into the collection its type names.
    ///
    /// Replay uses this so a `createEntity` operation need not know which
    /// collection its payload belongs to. An id already present is REPLACED,
    /// which is what FORMAT-SPEC LWW4 requires of a create that wins against
    /// another create on the same id.
    ///
    /// A replacement happens IN PLACE. FORMAT-SPEC C1 says collection order
    /// does not exist, so this is not a semantic requirement — it is a
    /// stability one. The CLI numbers walls by their position for `add-opening`
    /// and `set-wall`, and a `setFields` that moved its target to the end of
    /// the collection would silently renumber every wall after it, so the same
    /// command run twice would edit two different walls.
    pub fn insert_entity(&mut self, entity: Entity) {
        let id = entity.id();
        match entity {
            Entity::Level(l) => replace_or_push(&mut self.levels, l, |x| x.id == id),
            Entity::Wall(w) => replace_or_push(&mut self.walls, w, |x| x.id == id),
            Entity::Opening(o) => replace_or_push(&mut self.openings, o, |x| x.id == id),
            Entity::WallJoin(j) => replace_or_push(&mut self.joins, j, |x| x.id == id),
            Entity::RoomRecord(r) => replace_or_push(&mut self.room_records, r, |x| x.id == id),
            Entity::Label(l) => replace_or_push(&mut self.labels, l, |x| x.id == id),
            Entity::Dimension(d) => replace_or_push(&mut self.dimensions, d, |x| x.id == id),
            Entity::AssetInstance(a) => {
                replace_or_push(&mut self.asset_instances, a, |x| x.id == id)
            }
        }
    }

    /// Removes the entity with `id` from whichever collection holds it, and
    /// returns it. `None` when no entity has that id.
    ///
    /// This does NOT cascade: dependents of a removed entity are the caller's
    /// concern, because the operation log records each effect explicitly and a
    /// hidden cascade would produce state no operation describes.
    pub fn remove_entity(&mut self, id: Uuid) -> Option<Entity> {
        if let Some(i) = self.levels.iter().position(|l| l.id == id) {
            return Some(Entity::Level(self.levels.remove(i)));
        }
        if let Some(i) = self.walls.iter().position(|w| w.id == id) {
            return Some(Entity::Wall(self.walls.remove(i)));
        }
        if let Some(i) = self.openings.iter().position(|o| o.id == id) {
            return Some(Entity::Opening(self.openings.remove(i)));
        }
        if let Some(i) = self.joins.iter().position(|j| j.id == id) {
            return Some(Entity::WallJoin(self.joins.remove(i)));
        }
        if let Some(i) = self.room_records.iter().position(|r| r.id == id) {
            return Some(Entity::RoomRecord(self.room_records.remove(i)));
        }
        if let Some(i) = self.labels.iter().position(|l| l.id == id) {
            return Some(Entity::Label(self.labels.remove(i)));
        }
        if let Some(i) = self.dimensions.iter().position(|d| d.id == id) {
            return Some(Entity::Dimension(self.dimensions.remove(i)));
        }
        if let Some(i) = self.asset_instances.iter().position(|a| a.id == id) {
            return Some(Entity::AssetInstance(self.asset_instances.remove(i)));
        }
        None
    }

    /// The entity with `id`, cloned out of whichever collection holds it.
    pub fn entity(&self, id: Uuid) -> Option<Entity> {
        if let Some(l) = self.levels.iter().find(|l| l.id == id) {
            return Some(Entity::Level(l.clone()));
        }
        if let Some(w) = self.walls.iter().find(|w| w.id == id) {
            return Some(Entity::Wall(w.clone()));
        }
        if let Some(o) = self.openings.iter().find(|o| o.id == id) {
            return Some(Entity::Opening(o.clone()));
        }
        if let Some(j) = self.joins.iter().find(|j| j.id == id) {
            return Some(Entity::WallJoin(j.clone()));
        }
        if let Some(r) = self.room_records.iter().find(|r| r.id == id) {
            return Some(Entity::RoomRecord(r.clone()));
        }
        if let Some(l) = self.labels.iter().find(|l| l.id == id) {
            return Some(Entity::Label(l.clone()));
        }
        if let Some(d) = self.dimensions.iter().find(|d| d.id == id) {
            return Some(Entity::Dimension(d.clone()));
        }
        if let Some(a) = self.asset_instances.iter().find(|a| a.id == id) {
            return Some(Entity::AssetInstance(a.clone()));
        }
        None
    }

    /// Joins whose members all lie on `level_id`, in document order.
    ///
    /// A join stores no level of its own; it belongs to whichever level its
    /// walls do. A join whose members straddle levels is invalid (validation
    /// reports `JoinCrossesLevels`) and is reported for neither level here,
    /// because rendering it under one of them would silently pick a side.
    ///
    /// `O(walls + joins)`: wall levels are looked up through one map built
    /// per call. Resolving each member with [`Self::wall`], a linear scan,
    /// made this `O(joins × walls)` — quadratic on an ordinary plan, where
    /// almost every wall end is joined (issue 0012).
    pub fn joins_on_level(&self, level_id: Uuid) -> Vec<&WallJoin> {
        if self.joins.is_empty() {
            return Vec::new();
        }
        let level_of: std::collections::HashMap<Uuid, Uuid> =
            self.walls.iter().map(|w| (w.id, w.level_id)).collect();
        self.joins
            .iter()
            .filter(|j| {
                !j.members.is_empty()
                    && j.members
                        .iter()
                        .all(|m| level_of.get(&m.wall_id) == Some(&level_id))
            })
            .collect()
    }

    /// The join, if any, whose members meet at `point` on `level_id`.
    ///
    /// R3.3.5: a new contact landing on an occupied join point merges into the
    /// existing join rather than creating a second one there.
    pub fn join_at(&self, level_id: Uuid, point: Point) -> Option<&WallJoin> {
        self.joins.iter().find(|j| {
            j.members.iter().any(|m| {
                self.wall(m.wall_id).is_some_and(|w| {
                    if w.level_id != level_id {
                        return false;
                    }
                    let p = match m.role {
                        Role::Start => w.start(),
                        Role::End => w.end(),
                    };
                    (p.x - point.x).hypot(p.y - point.y) <= JOIN_COINCIDENCE_MM
                })
            })
        })
    }

    /// Openings hosted by `wall_id`, ordered by position along the wall.
    pub fn openings_on_wall(&self, wall_id: Uuid) -> Vec<&Opening> {
        let mut v: Vec<&Opening> = self
            .openings
            .iter()
            .filter(|o| o.host_wall_id == wall_id)
            .collect();
        v.sort_by(|a, b| a.u.partial_cmp(&b.u).unwrap_or(std::cmp::Ordering::Equal));
        v
    }

    /// Axis-aligned bounding box of every wall centre line, expanded by half
    /// thickness. `None` when the document has no walls.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut it = self.walls.iter();
        let first = it.next()?;
        let half = first.thickness_mm / 2.0;
        let (mut min_x, mut min_y) = (
            first.start().x.min(first.end().x) - half,
            first.start().y.min(first.end().y) - half,
        );
        let (mut max_x, mut max_y) = (
            first.start().x.max(first.end().x) + half,
            first.start().y.max(first.end().y) + half,
        );
        for w in it {
            let half = w.thickness_mm / 2.0;
            min_x = min_x.min(w.start().x.min(w.end().x) - half);
            min_y = min_y.min(w.start().y.min(w.end().y) - half);
            max_x = max_x.max(w.start().x.max(w.end().x) + half);
            max_y = max_y.max(w.start().y.max(w.end().y) + half);
        }
        Some((Point::new(min_x, min_y), Point::new(max_x, max_y)))
    }
}

impl ValidationError {
    /// The §4.4 code of an opening-related violation, or `None` for a
    /// violation the opening table does not name. Mapped from the exact
    /// condition each variant records, never from its message.
    pub fn opening_code(&self) -> Option<OpeningCode> {
        use OpeningCode as C;
        Some(match self {
            Self::DanglingHostWall { .. } => C::UnknownHost,
            Self::DuplicateId { .. } => C::EntityIdCollision,
            Self::OpeningParamOutOfRange { u, .. } if !u.is_finite() => C::NonfiniteValue,
            Self::OpeningParamOutOfRange { .. } => C::InvalidOpeningCenter,
            Self::InvalidSwingAngle { value, .. } if !value.is_finite() => C::NonfiniteValue,
            Self::InvalidSwingAngle { .. } => C::InvalidSwingAngle,
            Self::OpeningDimensionOutOfRange { value, .. } if !value.is_finite() => {
                C::NonfiniteValue
            }
            Self::OpeningDimensionOutOfRange { field, value, .. } => match (*field, *value) {
                ("widthMm", v) if v <= 0.0 => C::NonpositiveWidth,
                ("heightMm", v) if v <= 0.0 => C::NonpositiveHeight,
                ("depthMm", v) if v <= 0.0 => C::NonpositiveDepth,
                ("sillMm", v) if v < 0.0 => C::NegativeSill,
                // Above the format cap: a format bound, not an R4.4 code.
                _ => return None,
            },
            Self::DepthExceedsHostThickness { .. } => C::DepthExceedsHostThickness,
            Self::OpeningDoesNotFitHost { .. } => C::ApertureNotStrictFit,
            Self::ApertureExceedsHostHeight { .. } => C::ApertureExceedsHostHeight,
            Self::OverlappingOpenings { .. } => C::OpeningOverlap,
            // An explicitly supplied empty token on an opening. Tokens are
            // otherwise opaque: nothing resolves them (ED-0114).
            Self::EmptyStyleRef {
                kind: "opening", ..
            } => C::MissingStyle,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_with_wall(start: Point, end: Point) -> Document {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(level, start, end, 120.0, 2500.0));
        doc
    }

    #[test]
    fn opening_mechanisms_preserve_legacy_wire_and_reject_wrong_kinds() {
        let mut doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(4000.0, 0.0));
        let host = doc.walls[0].id;
        let door = Opening::door(host, 0.5, 900.0, 2100.0, 120.0);
        let json = serde_json::to_value(&door).unwrap();
        assert!(json.get("mechanism").is_none());
        let mut decoded: Opening = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.effective_mechanism(), OpeningMechanism::DoorHinged);
        decoded.mechanism = Some(OpeningMechanism::DoorSliding);
        assert_eq!(
            serde_json::to_value(&decoded).unwrap()["mechanism"],
            "doorSliding"
        );
        doc.openings.push(decoded.clone());
        assert!(doc.validate().is_empty());
        doc.openings[0].mechanism = Some(OpeningMechanism::WindowLift);
        assert!(doc
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidOpeningMechanism { .. })));
    }

    #[test]
    fn opening_fit_error_rounds_display_values_to_human_scale() {
        let error = ValidationError::OpeningDoesNotFitHost {
            id: Uuid::nil(),
            width: 1200.0000000001,
            u: -0.3577315571290243,
            host_length: 1870.0158345082205,
            start: -1268.9636763345577,
            end: -68.9636763345577,
        };

        let message = error.to_string();
        assert!(message.contains("1200 mm wide at u=-0.4"), "{message}");
        assert!(message.contains("1870 mm wall"), "{message}");
        assert!(message.contains("runs from -1269 to -69 mm"), "{message}");
        assert!(!message.contains("357731557"), "{message}");
    }

    #[test]
    fn opening_host_height_error_rounds_display_values_to_whole_millimetres() {
        let error = ValidationError::ApertureExceedsHostHeight {
            opening_id: Uuid::nil(),
            wall_id: Uuid::nil(),
            top: 2300.0000000001,
            host_height: 250.0000000001,
        };

        let message = error.to_string();
        assert!(
            message.contains("sill + height reaches 2300 mm"),
            "{message}"
        );
        assert!(message.contains("only 250 mm high"), "{message}");
        assert!(!message.contains("0000000001"), "{message}");
    }

    fn level_at(elevation: f64, thickness: f64) -> Level {
        let mut l = Level::new("L");
        l.elevation_mm = elevation;
        l.floor_thickness_mm = thickness;
        l
    }

    fn warned(levels: Vec<Level>) -> Vec<ValidationWarning> {
        let mut doc = Document::new();
        doc.levels = levels;
        doc.warnings()
    }

    #[test]
    fn a_slab_touching_the_floor_below_does_not_warn_but_one_ulp_deeper_does() {
        // §2.1's relation is STRICT: merely touching is not intrusion.
        let touching = warned(vec![level_at(0.0, 200.0), level_at(3000.0, 3000.0)]);
        assert_eq!(touching, [], "3000 - 3000 == 0 exactly: touching");

        // One ulp deeper is a real intrusion, and no epsilon may hide it.
        let deeper = warned(vec![
            level_at(0.0, 200.0),
            level_at(3000.0, f64::from_bits(3000f64.to_bits() + 1)),
        ]);
        assert_eq!(deeper.len(), 1, "{deeper:?}");
        assert!(matches!(
            deeper[0],
            ValidationWarning::SlabIntrudesBelowLevel { .. }
        ));
    }

    #[test]
    fn the_rounded_difference_landing_on_the_plane_is_settled_exactly() {
        // The ONE case the correctly-rounded subtraction cannot decide: the
        // rounded difference equals the plane below, while the exact
        // difference is under it. A test that only compared `f64` results
        // would call this clean — the false NEGATIVE this guard exists for.
        let elevation = 1.0e7_f64;
        // A thickness whose exact subtraction falls just below the ulp
        // boundary, so `fl(elevation - thickness)` rounds back up to it.
        let ulp = f64::from_bits(elevation.to_bits() + 1) - elevation;
        let thickness = ulp / 4.0;
        let plane = elevation;
        assert_eq!(
            elevation - thickness,
            plane,
            "the fixture needs the ROUNDED difference to land on the plane"
        );
        assert!(
            super::slab_intrudes(elevation, thickness, plane),
            "the exact difference is below the plane, so it intrudes"
        );
        // And a genuinely exact touch, same shape, must NOT warn.
        assert!(!super::slab_intrudes(elevation, 0.0, plane));
    }

    #[test]
    fn two_diff_is_error_free_including_subnormal_residuals() {
        let tiny = f64::from_bits(1);
        for (a, b, expected_s, expected_err) in [
            (1.0e7, f64::MIN_POSITIVE, 1.0e7, -f64::MIN_POSITIVE),
            (1.0, tiny, 1.0, -tiny),
            (-1.0, tiny, -1.0, -tiny),
            (COORD_BOUND_MM, DIMENSION_CAP_MM, 9_000_000.0, 0.0),
            (
                f64::MIN_POSITIVE,
                tiny,
                f64::from_bits(f64::MIN_POSITIVE.to_bits() - 1),
                0.0,
            ),
            // The exact midpoint below 1 rounds ties-to-even back to 1.
            (1.0, 2.0f64.powi(-54), 1.0, -2.0f64.powi(-54)),
        ] {
            let (s, err) = super::two_diff(a, b);
            // Assert the residual itself: recomputing s+err in binary64
            // would round it away and would also pass with a broken err=0.
            assert_eq!(s, expected_s, "a={a}, b={b}");
            assert_eq!(err, expected_err, "a={a}, b={b}");
            if expected_err < 0.0 {
                assert!(super::slab_intrudes(a, b, s));
            }
        }
    }

    #[test]
    fn a_zero_thickness_slab_cannot_intrude_and_a_zero_one_below_does_not_suppress() {
        // §2.1 explicitly: a zero-thickness UPPER slab cannot intrude, and a
        // zero-thickness LOWER slab does not suppress the warning — the test
        // is against a plane, not a volume.
        assert_eq!(warned(vec![level_at(0.0, 0.0), level_at(3000.0, 0.0)]), []);
        let intruding = warned(vec![level_at(0.0, 0.0), level_at(3000.0, 4000.0)]);
        assert_eq!(intruding.len(), 1, "a zero slab below does not suppress it");
    }

    #[test]
    fn equal_elevations_warn_once_per_adjacent_pair_and_can_also_intrude() {
        let a = level_at(0.0, 0.0);
        let mut b = level_at(0.0, 200.0);
        let mut c = level_at(0.0, 0.0);
        // Fixed ids so the elevation tie resolves by UUID, deterministically.
        let (ida, idb, idc) = (Uuid::from_u128(1), Uuid::from_u128(2), Uuid::from_u128(3));
        let mut a = a;
        a.id = ida;
        b.id = idb;
        c.id = idc;

        let out = warned(vec![c.clone(), a.clone(), b.clone()]);
        // Three equal elevations give TWO duplicate warnings, not three pairs,
        // and `b` also intrudes because its slab has thickness.
        assert_eq!(
            out,
            [
                ValidationWarning::DuplicateElevation {
                    level: idb,
                    other: ida
                },
                ValidationWarning::SlabIntrudesBelowLevel {
                    level: idb,
                    below: ida
                },
                ValidationWarning::DuplicateElevation {
                    level: idc,
                    other: idb
                },
            ],
            "ordered by elevation then UUID, ceiling/duplicate/intrusion per level"
        );
    }

    #[test]
    fn one_level_emitting_all_three_warnings_reports_them_in_section_order() {
        // §2.1 fixes the per-level order: ceiling below wall height, then
        // duplicate elevation against the immediate predecessor, then slab
        // intrusion against that same predecessor. Emission of each variant
        // is covered end to end elsewhere; what is asserted here is the
        // ORDER, which only a level firing all three can show.
        let mut lower = level_at(0.0, 0.0);
        let mut upper = level_at(0.0, 200.0);
        lower.id = Uuid::from_u128(1);
        upper.id = Uuid::from_u128(2);
        upper.default_wall_height_mm = 2500.0;
        upper.default_ceiling_height_mm = 2200.0;

        assert_eq!(
            warned(vec![upper, lower]),
            [
                ValidationWarning::CeilingBelowWallHeight {
                    level: Uuid::from_u128(2)
                },
                ValidationWarning::DuplicateElevation {
                    level: Uuid::from_u128(2),
                    other: Uuid::from_u128(1),
                },
                ValidationWarning::SlabIntrudesBelowLevel {
                    level: Uuid::from_u128(2),
                    below: Uuid::from_u128(1),
                },
            ]
        );
    }

    #[test]
    fn changing_an_elevation_reorders_the_pair_without_any_stored_order() {
        // The enumeration order is DERIVED from elevation and UUID on every
        // call. Nothing persists a level order, so raising the lower level
        // above the upper one must swap which id is the intruder and which is
        // the level below — with no reordering of `doc.levels` itself.
        let mut a = level_at(0.0, 200.0);
        let mut b = level_at(3000.0, 4000.0);
        a.id = Uuid::from_u128(1);
        b.id = Uuid::from_u128(2);
        let mut doc = Document::new();
        doc.levels = vec![a, b];

        assert_eq!(
            doc.warnings(),
            [ValidationWarning::SlabIntrudesBelowLevel {
                level: Uuid::from_u128(2),
                below: Uuid::from_u128(1),
            }]
        );

        // Raise `a` above `b`, exactly as an accepted `setFields` would, and
        // give it a slab that reaches below `b`.
        doc.levels[0].elevation_mm = 6000.0;
        doc.levels[0].floor_thickness_mm = 4000.0;
        doc.levels[1].floor_thickness_mm = 200.0;

        assert_eq!(
            doc.warnings(),
            [ValidationWarning::SlabIntrudesBelowLevel {
                level: Uuid::from_u128(1),
                below: Uuid::from_u128(2),
            }],
            "attribution follows elevation, not document position"
        );
        assert_eq!(
            doc.levels.iter().map(|l| l.id).collect::<Vec<_>>(),
            [Uuid::from_u128(1), Uuid::from_u128(2)],
            "the document's own level order is untouched"
        );
    }

    #[test]
    fn an_invalid_elevation_never_splits_two_valid_neighbours() {
        // The ordering policy is diagnostic-only, and its point is that an
        // invalid document still produces a stable, useful answer: NaN cannot
        // sit between two valid levels and destroy the adjacency §2.1's
        // comparisons are defined over, and the result does not depend on
        // insertion order.
        let mut lower = level_at(0.0, 200.0);
        let mut upper = level_at(3000.0, 4000.0);
        let mut broken = level_at(f64::NAN, 200.0);
        lower.id = Uuid::from_u128(1);
        upper.id = Uuid::from_u128(2);
        broken.id = Uuid::from_u128(3);
        let expected = [ValidationWarning::SlabIntrudesBelowLevel {
            level: upper.id,
            below: lower.id,
        }];
        for order in [
            vec![lower.clone(), broken.clone(), upper.clone()],
            vec![broken.clone(), upper.clone(), lower.clone()],
            vec![upper.clone(), lower.clone(), broken.clone()],
        ] {
            assert_eq!(
                warned(order),
                expected,
                "the invalid level is sorted last and never breaks adjacency"
            );
        }
    }

    #[test]
    fn signed_zero_elevations_tie_and_break_by_uuid() {
        // FORMAT-SPEC maps −0 to +0, so the two are the SAME elevation: they
        // must tie and resolve by UUID, and report a duplicate.
        let mut a = level_at(-0.0, 0.0);
        let mut b = level_at(0.0, 0.0);
        a.id = Uuid::from_u128(2);
        b.id = Uuid::from_u128(1);
        assert_eq!(
            warned(vec![a, b]),
            [ValidationWarning::DuplicateElevation {
                level: Uuid::from_u128(2),
                other: Uuid::from_u128(1),
            }]
        );
    }

    #[test]
    fn warnings_skip_operands_their_own_field_constraints_reject() {
        let mut bad = level_at(0.0, 200.0);
        bad.default_ceiling_height_mm = f64::INFINITY;
        bad.default_wall_height_mm = 2500.0;
        // An infinite ceiling is greater than the wall height, so a naive
        // check would emit nothing; but it is also not a legal operand, and a
        // validation ERROR already reports it.
        assert!(!bad.default_ceiling_height_mm.is_finite());
        let mut upper = level_at(f64::NAN, 200.0);
        upper.id = Uuid::from_u128(9);
        let out = warned(vec![bad, upper]);
        assert_eq!(out, [], "no advisory computed from an invalid operand");
    }

    #[test]
    fn a_warning_serializes_with_its_code_and_camel_case_ids() {
        let w = ValidationWarning::SlabIntrudesBelowLevel {
            level: Uuid::from_u128(1),
            below: Uuid::from_u128(2),
        };
        let v: serde_json::Value = serde_json::to_value(&w).unwrap();
        assert_eq!(v["code"], "slab-intrudes-below-level");
        assert!(v["level"].is_string() && v["below"].is_string());
        assert_eq!(serde_json::from_value::<ValidationWarning>(v).unwrap(), w);
        assert!(w.to_string().contains("reaches below"));
    }

    #[test]
    fn warnings_are_never_validation_errors() {
        let doc = {
            let mut d = Document::new();
            d.levels = vec![level_at(0.0, 200.0), level_at(3000.0, 4000.0)];
            d
        };
        assert_eq!(doc.validate(), [], "a warned document is still VALID");
        assert_eq!(doc.validate_plan(), []);
        assert_eq!(doc.warnings().len(), 1);
    }

    #[test]
    fn a_new_plan_has_a_level_to_build_on() {
        let doc = Document::with_ground_floor();
        assert_eq!(doc.levels.len(), 1);
        assert_eq!(doc.levels[0].name, "Level 1");
        assert_eq!(doc.levels[0].elevation_mm, 0.0);
        assert!(doc.validate_plan().is_empty());
    }

    #[test]
    fn empty_builder_is_not_a_valid_materialized_plan() {
        let doc = Document::new();
        assert!(doc.validate().is_empty());
        assert_eq!(doc.validate_plan(), [ValidationError::NoLevels]);
    }

    #[test]
    fn wall_length_is_the_centre_line_distance() {
        let doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(3000.0, 4000.0));
        assert_eq!(doc.walls[0].length_mm(), 5000.0);
    }

    #[test]
    fn zero_length_wall_is_rejected() {
        let p = Point::new(100.0, 100.0);
        let errors = doc_with_wall(p, p).validate();
        assert!(
            matches!(errors.as_slice(), [ValidationError::ZeroLengthWall { .. }]),
            "expected a zero-length rejection, got {errors:?}"
        );
    }

    #[test]
    fn non_finite_and_out_of_bounds_coordinates_are_rejected() {
        for bad in [f64::NAN, f64::INFINITY, COORD_BOUND_MM * 2.0] {
            let errors = doc_with_wall(Point::new(0.0, 0.0), Point::new(bad, 0.0)).validate();
            assert!(
                errors
                    .iter()
                    .any(|e| matches!(e, ValidationError::CoordinateOutOfBounds { .. })),
                "coordinate {bad} should be rejected, got {errors:?}"
            );
        }
    }

    #[test]
    fn non_positive_thickness_is_rejected() {
        let mut doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(1000.0, 0.0));
        doc.walls[0].thickness_mm = 0.0;
        let errors = doc.validate();
        assert!(
            errors.iter().any(|e| matches!(
                e,
                ValidationError::DimensionOutOfRange {
                    field: "thicknessMm",
                    ..
                }
            )),
            "zero thickness should be rejected, got {errors:?}"
        );
    }

    #[test]
    fn a_wall_pointing_at_a_missing_level_is_rejected() {
        let mut doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(1000.0, 0.0));
        doc.walls[0].level_id = Uuid::now_v7();
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::DanglingLevelRef { .. })),
            "dangling level reference should be rejected, got {errors:?}"
        );
    }

    #[test]
    fn validate_reports_every_violation_not_just_the_first() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let p = Point::new(0.0, 0.0);
        doc.walls.push(Wall::new(level, p, p, 0.0, 2500.0));
        let errors = doc.validate();
        assert!(
            errors.len() >= 2,
            "a wall that is both zero-length and zero-thickness should report \
             both, got {errors:?}"
        );
    }

    #[test]
    fn document_round_trips_through_json_with_spec_field_names() {
        let doc = doc_with_wall(Point::new(0.0, 2500.0), Point::new(4000.0, 2500.0));
        let json = serde_json::to_string(&doc).expect("serialize");

        // The spec's example wall uses these names; camelCase is not cosmetic.
        assert!(json.contains("\"startMm\":[0.0,2500.0]"), "got {json}");
        assert!(json.contains("\"thicknessMm\":120.0"), "got {json}");

        let back: Document = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(doc, back);
    }

    /// Issue 0115: the level-list ordinal serializes under its spec name, is
    /// absent by default, and survives a round trip including an odd-but-legal
    /// gap.
    #[test]
    fn level_display_order_serializes_absent_by_default_and_round_trips() {
        let mut level = Level::new("List head");
        let plain = serde_json::to_value(&level).unwrap();
        assert!(plain.get("displayOrder").is_none(), "got {plain:?}");

        level.display_order = Some(2);
        let json = serde_json::to_value(&level).unwrap();
        assert_eq!(json["displayOrder"], 2, "got {json:?}");
        let back: Level = serde_json::from_value(json).unwrap();
        assert_eq!(back, level);
    }

    /// Issue 0115: the ordinal is a count, not a millimetre measure, so its
    /// bound is the u53 JSON-exact range and not `DIMENSION_CAP_MM`.
    #[test]
    fn display_order_beyond_u53_is_a_validation_error() {
        let mut doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(4000.0, 0.0));
        doc.levels[0].display_order = Some(MAX_SAFE_INTEGER);
        assert!(
            doc.validate().is_empty(),
            "the u53 limit itself is legal: {:?}",
            doc.validate()
        );
        doc.levels[0].display_order = Some(MAX_SAFE_INTEGER + 1);
        let errors = doc.validate();
        assert!(
            errors.iter().any(|error| matches!(
                error,
                ValidationError::LevelDisplayOrderOutOfRange {
                    value: 9_007_199_254_740_992,
                    ..
                }
            )),
            "got {errors:?}"
        );
    }

    /// Issue 0116: new-plan entry points mint a v7 document identity, which
    /// rides every round trip; documents built directly keep the pre-issue
    /// shape (absent) and are NOT rewritten behind a caller's back.
    #[test]
    fn document_identity_is_minted_by_new_plans_and_carried() {
        let fresh = Document::with_ground_floor();
        let minted = fresh.document_id.expect("a new plan has an identity");
        assert_eq!(minted.get_version_num(), 7, "{minted}");

        let plain = Document::new();
        assert!(plain.document_id.is_none(), "not everything is a new plan");

        let doc = doc_with_wall(Point::new(0.0, 2500.0), Point::new(4000.0, 2500.0));
        let json = serde_json::to_string(&doc).expect("serialize");
        assert!(json.contains("\"documentId\":\""), "got {json}");
        let back: Document = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.document_id, doc.document_id, "identity survives");
        let foreign = format!("{{\"documentId\":\"{minted}\",\"levels\":[]}}");
        let adopted: Document = serde_json::from_str(&foreign).expect("deserialize");
        assert_eq!(adopted.document_id, Some(minted));
    }

    #[test]
    fn bounds_expand_by_half_wall_thickness() {
        let doc = doc_with_wall(Point::new(0.0, 0.0), Point::new(1000.0, 0.0));
        let (min, max) = doc.bounds().expect("walls present");
        assert_eq!(min, Point::new(-60.0, -60.0));
        assert_eq!(max, Point::new(1060.0, 60.0));
    }

    fn doc_with_wall_and(len: f64) -> (Document, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(len, 0.0),
            120.0,
            2500.0,
        );
        let id = wall.id;
        doc.walls.push(wall);
        (doc, id)
    }

    /// The five R4.0.4 fields must be TYPED, not carried opaquely. Until
    /// 2026-09-04 they landed in `unknown`, which round-tripped them but left
    /// `swingDeg` unvalidatable and a door's swing undrawable.
    #[test]
    fn the_golden_door_fixture_parses_into_typed_fields() {
        let text =
            std::fs::read_to_string("../../testdata/corpus/golden/entities/opening-door.json")
                .expect("golden fixture is present");
        let entity: Entity = serde_json::from_str(&text).expect("fixture parses");
        let Entity::Opening(o) = entity else {
            panic!("fixture is an opening")
        };
        assert_eq!(o.depth_mm, 120.0);
        assert_eq!(o.side, Side::Left);
        assert_eq!(o.hinge, Hinge::NearStart);
        assert_eq!(o.opens_toward, Side::Left);
        assert_eq!(o.swing_deg, 90.0);
        assert!(
            !o.unknown.contains_key("depthMm") && !o.unknown.contains_key("swingDeg"),
            "R4.0.4 fields must not be carried as unknown data: {:?}",
            o.unknown.keys().collect::<Vec<_>>()
        );
    }

    /// The enum spellings are the specification's, not Rust's. `side` is
    /// PascalCase and `hinge` is camelCase in FORMAT-SPEC §3; they differ, and
    /// a tidy-looking unification would break every existing file.
    #[test]
    fn the_enum_spellings_match_the_specification() {
        assert_eq!(serde_json::to_string(&Side::Left).unwrap(), "\"Left\"");
        assert_eq!(serde_json::to_string(&Side::Right).unwrap(), "\"Right\"");
        assert_eq!(
            serde_json::to_string(&Hinge::NearStart).unwrap(),
            "\"nearStart\""
        );
        assert_eq!(
            serde_json::to_string(&Hinge::NearEnd).unwrap(),
            "\"nearEnd\""
        );
    }

    #[test]
    fn an_unknown_side_or_hinge_is_refused_at_parse_time() {
        let bad = r#"{"id":"019534c7-0000-7000-8000-000000000005","type":"opening",
            "kind":"door","hostWallId":"019534c7-0000-7000-8000-000000000002",
            "u":0.5,"widthMm":900,"heightMm":2100,"sillMm":0,"depthMm":120,
            "side":"Middle","hinge":"nearStart","opensToward":"Left","swingDeg":90}"#;
        assert!(serde_json::from_str::<Entity>(bad).is_err());
    }

    #[test]
    fn a_swing_outside_zero_to_one_eighty_is_rejected() {
        for bad in [-1.0, 180.5, f64::NAN, f64::INFINITY] {
            let (mut doc, wall) = doc_with_wall_and(4000.0);
            let mut o = Opening::door(wall, 0.5, 900.0, 2100.0, 120.0);
            o.swing_deg = bad;
            doc.openings.push(o);
            assert!(
                doc.validate()
                    .iter()
                    .any(|e| matches!(e, ValidationError::InvalidSwingAngle { .. })),
                "swingDeg {bad} should reject"
            );
        }
    }

    /// A closed leaf is legal: R4.0.4 gives zero an explicit meaning.
    #[test]
    fn a_zero_swing_is_legal_and_means_closed() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        let mut o = Opening::door(wall, 0.5, 900.0, 2100.0, 120.0);
        o.swing_deg = 0.0;
        doc.openings.push(o);
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    #[test]
    fn a_cut_deeper_than_its_host_is_rejected() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        // The fixture wall is 120 mm thick.
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 121.0));
        assert!(
            doc.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::DepthExceedsHostThickness { .. })),
            "{:?}",
            doc.validate()
        );
    }

    #[test]
    fn a_level_with_a_nonfinite_elevation_is_rejected() {
        let mut doc = Document::with_ground_floor();
        doc.levels[0].elevation_mm = f64::NAN;
        assert!(doc
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::LevelDimensionOutOfRange { .. })));
    }

    /// FORMAT-SPEC §3 puts `floorThicknessMm` in `[0, dimensionCapMm]`, so a
    /// level with NO slab of its own is legal. An earlier version of this
    /// validation rejected it.
    #[test]
    fn a_level_with_no_slab_is_legal() {
        let mut doc = Document::with_ground_floor();
        doc.levels[0].floor_thickness_mm = 0.0;
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    #[test]
    fn a_level_with_a_negative_slab_is_rejected() {
        let mut doc = Document::with_ground_floor();
        doc.levels[0].floor_thickness_mm = -1.0;
        assert!(doc
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::LevelDimensionOutOfRange { .. })));
    }

    /// Elevation is a COORDINATE, bounded by `coordBoundMm`, not by the
    /// dimension cap — and a basement's is negative. Applying the dimension
    /// cap here rejected legal levels ten times over.
    #[test]
    fn a_basement_elevation_is_legal_up_to_the_coordinate_bound() {
        let mut doc = Document::with_ground_floor();
        doc.levels[0].elevation_mm = -DIMENSION_CAP_MM * 5.0;
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());

        doc.levels[0].elevation_mm = COORD_BOUND_MM * 1.5;
        assert!(doc
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::LevelDimensionOutOfRange { .. })));
    }

    /// R4.1.2 is STRICT. The specification's own example: a 900 mm opening
    /// centred on a 900 mm wall has zero clearance at both ends and is not a
    /// fit, however much it looks like one.
    #[test]
    fn an_aperture_with_zero_clearance_is_not_a_fit() {
        let (mut doc, wall) = doc_with_wall_and(900.0);
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        assert!(
            doc.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::OpeningDoesNotFitHost { .. })),
            "{:?}",
            doc.validate()
        );
    }

    /// R4.1.3 is INCLUSIVE: sill 2100 + height 400 exactly meets a 2500 host.
    #[test]
    fn an_aperture_reaching_exactly_the_host_height_is_legal() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::window(wall, 0.5, 900.0, 400.0, 2100.0, 120.0));
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    #[test]
    fn an_aperture_taller_than_its_host_is_rejected() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::window(wall, 0.5, 900.0, 401.0, 2100.0, 120.0));
        assert!(
            doc.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::ApertureExceedsHostHeight { .. })),
            "{:?}",
            doc.validate()
        );
    }

    /// R4.1.4's own example, and the case the horizontal-only check got
    /// wrong: a transom sitting exactly on a door's head shares the whole
    /// horizontal span and is still legal, because the vertical intervals only
    /// touch.
    #[test]
    fn a_transom_resting_on_a_door_head_is_legal() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        doc.openings
            .push(Opening::window(wall, 0.5, 900.0, 400.0, 2100.0, 120.0));
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    /// Drop the same transom by one millimetre and both intervals intersect
    /// with positive length, which R4.1.4 rejects.
    #[test]
    fn a_transom_overlapping_a_door_by_one_millimetre_is_rejected() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        doc.openings
            .push(Opening::window(wall, 0.5, 900.0, 400.0, 2099.0, 120.0));
        assert!(
            doc.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::OverlappingOpenings { .. })),
            "{:?}",
            doc.validate()
        );
    }

    #[test]
    fn a_door_that_fits_its_wall_validates() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        // The fixture's wall is 120 mm thick, and R4.0.4 caps the cut at the
        // host's thickness.
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    #[test]
    fn an_opening_span_is_centred_on_u() {
        let door = Opening::door(Uuid::now_v7(), 0.5, 1000.0, 2100.0, 200.0);
        let (s, e) = door.span(4000.0).expect("span");
        assert!((s - 0.375).abs() < 1e-12, "got {s}");
        assert!((e - 0.625).abs() < 1e-12, "got {e}");
    }

    #[test]
    fn a_door_wider_than_its_wall_is_rejected() {
        let (mut doc, wall) = doc_with_wall_and(800.0);
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::OpeningDoesNotFitHost { .. })),
            "a 900 mm door cannot fit an 800 mm wall; got {errors:?}"
        );
    }

    #[test]
    fn a_door_hanging_off_the_end_of_its_wall_is_rejected() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        // Centred at the very end: half the door is past the wall.
        doc.openings
            .push(Opening::door(wall, 1.0, 900.0, 2100.0, 200.0));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::OpeningDoesNotFitHost { .. })),
            "a door centred at the wall end overhangs it; got {errors:?}"
        );
    }

    #[test]
    fn two_overlapping_openings_on_one_wall_are_rejected() {
        let (mut doc, wall) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::door(wall, 0.5, 1000.0, 2100.0, 200.0));
        doc.openings
            .push(Opening::window(wall, 0.55, 1000.0, 1200.0, 900.0, 200.0));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::OverlappingOpenings { .. })),
            "overlapping openings must be rejected; got {errors:?}"
        );
    }

    #[test]
    fn two_openings_that_do_not_overlap_are_accepted() {
        let (mut doc, wall) = doc_with_wall_and(6000.0);
        doc.openings
            .push(Opening::door(wall, 0.2, 900.0, 2100.0, 120.0));
        doc.openings
            .push(Opening::window(wall, 0.7, 1200.0, 1200.0, 900.0, 120.0));
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    #[test]
    fn openings_on_a_wall_come_back_in_position_order() {
        let (mut doc, wall) = doc_with_wall_and(9000.0);
        doc.openings
            .push(Opening::door(wall, 0.8, 900.0, 2100.0, 120.0));
        doc.openings
            .push(Opening::door(wall, 0.2, 900.0, 2100.0, 120.0));
        doc.openings
            .push(Opening::door(wall, 0.5, 900.0, 2100.0, 120.0));
        let ordered: Vec<f64> = doc.openings_on_wall(wall).iter().map(|o| o.u).collect();
        assert_eq!(ordered, vec![0.2, 0.5, 0.8]);
    }

    #[test]
    fn an_opening_on_a_missing_wall_is_rejected() {
        let (mut doc, _) = doc_with_wall_and(4000.0);
        doc.openings
            .push(Opening::door(Uuid::now_v7(), 0.5, 900.0, 2100.0, 200.0));
        assert!(doc
            .validate()
            .iter()
            .any(|e| matches!(e, ValidationError::DanglingHostWall { .. })));
    }

    #[test]
    fn bounds_is_none_without_walls() {
        assert!(Document::with_ground_floor().bounds().is_none());
    }

    // ---- joins (BEHAVIOR-SPEC R3.3, FORMAT-SPEC §3 `wallJoin`) ----

    fn two_walls_meeting_at_a_corner() -> (Document, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        ));
        doc.walls.push(Wall::new(
            level,
            Point::new(4000.0, 0.0),
            Point::new(4000.0, 3000.0),
            200.0,
            2500.0,
        ));
        (doc, level)
    }

    fn corner_join(doc: &Document) -> WallJoin {
        WallJoin::new(vec![
            JoinMember::new(doc.walls[0].id, Role::End),
            JoinMember::new(doc.walls[1].id, Role::Start),
        ])
    }

    #[test]
    fn a_well_formed_join_validates() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        let join = corner_join(&doc);
        doc.joins.push(join);
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn a_join_needs_at_least_two_distinct_members() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        let member = JoinMember::new(doc.walls[0].id, Role::End);
        // Listed twice: a set of one, not a join.
        doc.joins.push(WallJoin::new(vec![member, member]));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinTooFewMembers { count: 1, .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn a_wall_cannot_join_to_itself() {
        // R3.3.3 degenerate-join-membership.
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        let w = doc.walls[0].id;
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(w, Role::Start),
            JoinMember::new(w, Role::End),
        ]));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinSelfMembership { .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn an_endpoint_cannot_belong_to_two_joins() {
        // R3.3.2 single membership.
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        doc.joins.push(corner_join(&doc));
        doc.joins.push(corner_join(&doc));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinMemberInTwoJoins { .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn join_members_must_meet_at_one_point() {
        // R3.3.4 coordinate agreement: the walls do not touch.
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        doc.walls[1].start_mm = [4001.0, 0.0];
        doc.joins.push(corner_join(&doc));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinCoordinateDisagreement { .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn join_coordinate_agreement_tolerates_sub_micrometre_drift() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        doc.walls[1].start_mm = [4000.0 + JOIN_COINCIDENCE_MM / 4.0, 0.0];
        doc.joins.push(corner_join(&doc));
        assert_eq!(doc.validate(), vec![]);
    }

    /// ED-0103 N-2. The retired rule measured distance from whichever member
    /// happened to be listed first, so the SAME three members validated or
    /// failed depending on their order — though FORMAT-SPEC §2.3 (C3) makes
    /// membership an unordered set. Cells are a function of the set.
    #[test]
    fn join_agreement_does_not_depend_on_which_member_is_listed_first() {
        let drift = JOIN_COINCIDENCE_MM * 0.9;
        let mut verdicts = Vec::new();
        for order in [[0, 1, 2], [1, 0, 2], [2, 1, 0]] {
            let mut doc = Document::with_ground_floor();
            let level = doc.levels[0].id;
            // Three walls whose joined endpoints sit at -0.9, 0 and +0.9 µm.
            for offset in [0.0, drift, -drift] {
                doc.walls.push(Wall::new(
                    level,
                    Point::new(4000.0 + offset, 0.0),
                    Point::new(4000.0 + offset, 3000.0),
                    200.0,
                    2500.0,
                ));
            }
            let members = order
                .iter()
                .map(|&i| JoinMember::new(doc.walls[i].id, Role::Start))
                .collect();
            doc.joins.push(WallJoin::new(members));
            verdicts.push(
                doc.validate()
                    .iter()
                    .any(|e| matches!(e, ValidationError::JoinCoordinateDisagreement { .. })),
            );
        }
        assert!(
            verdicts.iter().all(|v| *v == verdicts[0]),
            "member order changed the verdict: {verdicts:?}"
        );
    }

    /// Two points inside ONE cell are one vertex to the arrangement, so
    /// validation must not call the join broken — even though they are further
    /// apart than the quantum, which the retired distance rule tripped over.
    #[test]
    fn members_sharing_a_cell_agree_even_when_further_apart_than_the_quantum() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        let offset = JOIN_COINCIDENCE_MM * 0.4;
        doc.walls[0].end_mm = [4000.0 + offset, offset];
        doc.walls[1].start_mm = [4000.0 - offset, -offset];
        assert_eq!(
            topology_cell(Point::new(doc.walls[0].end_mm[0], doc.walls[0].end_mm[1])),
            topology_cell(Point::new(
                doc.walls[1].start_mm[0],
                doc.walls[1].start_mm[1]
            )),
            "the fixture must put both in one cell"
        );
        let separation = (2.0 * offset).hypot(2.0 * offset);
        assert!(
            separation > JOIN_COINCIDENCE_MM,
            "and further apart than the quantum"
        );

        doc.joins.push(corner_join(&doc));
        assert_eq!(doc.validate(), vec![]);
    }

    /// The other direction, which is the one that used to pass silently: two
    /// points a quantum apart across a cell boundary are DIFFERENT vertices to
    /// the arrangement, so a join claiming they coincide is not realizable and
    /// validation now says so.
    #[test]
    fn members_in_different_cells_disagree_however_close_they_are() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        doc.walls[0].end_mm = [4000.0 + JOIN_COINCIDENCE_MM * 0.4, 0.0];
        doc.walls[1].start_mm = [4000.0 + JOIN_COINCIDENCE_MM * 1.4, 0.0];
        assert_ne!(
            topology_cell(Point::new(doc.walls[0].end_mm[0], doc.walls[0].end_mm[1])),
            topology_cell(Point::new(
                doc.walls[1].start_mm[0],
                doc.walls[1].start_mm[1]
            )),
            "the fixture must straddle a cell boundary"
        );

        doc.joins.push(corner_join(&doc));
        assert!(
            doc.validate()
                .iter()
                .any(|e| matches!(e, ValidationError::JoinCoordinateDisagreement { .. })),
            "a join across a cell boundary is not one node"
        );
    }

    #[test]
    fn a_join_to_a_missing_wall_is_reported_once() {
        // THREE members, one missing and the two survivors in disagreement.
        // With only two members the coordinate check is vacuous and would pass
        // whether or not the dangling case short-circuits, so this test could
        // not tell the two apart.
        let (mut doc, level) = two_walls_meeting_at_a_corner();
        doc.walls.push(Wall::new(
            level,
            Point::new(9999.0, 9999.0),
            Point::new(9999.0, 8000.0),
            200.0,
            2500.0,
        ));
        let ghost = Uuid::now_v7();
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(doc.walls[0].id, Role::End),
            JoinMember::new(doc.walls[2].id, Role::Start),
            JoinMember::new(ghost, Role::Start),
        ]));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinDanglingWall { .. })),
            "got {errors:?}"
        );
        // Coordinate agreement is unanswerable with a member missing, so it
        // must not pile a second complaint on the same fault.
        assert!(
            !errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinCoordinateDisagreement { .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn a_join_cannot_span_levels() {
        // R3.0.6: walls on different levels never interact.
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        let upper = Level::new("Level 2");
        doc.walls[1].level_id = upper.id;
        doc.levels.push(upper);
        doc.joins.push(corner_join(&doc));
        let errors = doc.validate();
        assert!(
            errors
                .iter()
                .any(|e| matches!(e, ValidationError::JoinCrossesLevels { .. })),
            "got {errors:?}"
        );
    }

    #[test]
    fn joins_on_level_excludes_a_join_that_straddles_levels() {
        let (mut doc, level) = two_walls_meeting_at_a_corner();
        doc.joins.push(corner_join(&doc));
        assert_eq!(doc.joins_on_level(level).len(), 1);

        let upper = Level::new("Level 2");
        let upper_id = upper.id;
        doc.walls[1].level_id = upper_id;
        doc.levels.push(upper);
        assert!(doc.joins_on_level(level).is_empty());
        assert!(doc.joins_on_level(upper_id).is_empty());
    }

    #[test]
    fn join_at_finds_the_node_occupying_a_point() {
        // R3.3.5: a new contact at an occupied point merges into that join.
        let (mut doc, level) = two_walls_meeting_at_a_corner();
        doc.joins.push(corner_join(&doc));
        let found = doc.join_at(level, Point::new(4000.0, 0.0));
        assert_eq!(found.map(|j| j.id), Some(doc.joins[0].id));
        assert!(doc.join_at(level, Point::new(0.0, 0.0)).is_none());
    }

    #[test]
    fn a_join_round_trips_through_json_with_the_specified_field_names() {
        let (mut doc, _) = two_walls_meeting_at_a_corner();
        doc.joins.push(corner_join(&doc));
        let json = serde_json::to_string(&doc).expect("serializes");
        // FORMAT-SPEC §3 spells these `members`, `wallId`, `role`.
        assert!(json.contains(r#""members":[{"wallId":"#), "got {json}");
        assert!(json.contains(r#""role":"End""#), "got {json}");
        let back: Document = serde_json::from_str(&json).expect("round trips");
        assert_eq!(back, doc);
    }

    #[test]
    fn a_document_without_a_joins_field_still_loads() {
        // Documents written before joins existed must not fail to open.
        let json = r#"{"levels":[],"walls":[],"openings":[]}"#;
        let doc: Document = serde_json::from_str(json).expect("loads");
        assert!(doc.joins.is_empty());
    }

    #[test]
    fn replacing_an_entity_keeps_its_position_in_the_collection() {
        // FORMAT-SPEC C1 makes collection order non-semantic, so this is a
        // STABILITY property, not a correctness one — but the CLI numbers
        // walls by position, and a replacement that appended would renumber
        // every wall after the edited one.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        for i in 0..3 {
            doc.walls.push(Wall::new(
                level,
                Point::new(0.0, i as f64 * 1000.0),
                Point::new(1000.0, i as f64 * 1000.0),
                200.0,
                2500.0,
            ));
        }
        let ids: Vec<Uuid> = doc.walls.iter().map(|w| w.id).collect();

        let mut edited = doc.walls[0].clone();
        edited.thickness_mm = 150.0;
        doc.insert_entity(Entity::Wall(edited));

        assert_eq!(doc.walls.iter().map(|w| w.id).collect::<Vec<_>>(), ids);
        assert_eq!(doc.walls[0].thickness_mm, 150.0);
        assert_eq!(doc.walls.len(), 3, "a replacement is not an addition");
    }

    // ---- FORMAT-SPEC §8 U2: unknown optional data is preserved ----

    #[test]
    fn an_unknown_field_survives_a_round_trip() {
        // U2: "Silent stripping is forbidden everywhere." Without the
        // `unknown` map, serde drops what it does not recognise, so an older
        // Domowik opening and saving a newer file would quietly delete the
        // newer version's data.
        let json = r#"{
            "id":"01948f55-6b18-7d3a-a153-1ee2f33cc402",
            "levelId":"01948f55-6b18-7d3a-a153-1ee2f33cc403",
            "startMm":[0.0,0.0],
            "endMm":[4000.0,0.0],
            "thicknessMm":200.0,
            "heightMm":2500.0,
            "styleRef":"brick",
            "futureThing":{"nested":[1,2,3]}
        }"#;
        let wall: Wall = serde_json::from_str(json).expect("loads");
        assert_eq!(wall.unknown.len(), 2, "got {:?}", wall.unknown);
        assert_eq!(
            wall.unknown.get("styleRef"),
            Some(&serde_json::json!("brick"))
        );

        let back = serde_json::to_value(&wall).expect("serialises");
        assert_eq!(back.get("styleRef"), Some(&serde_json::json!("brick")));
        assert_eq!(
            back.get("futureThing"),
            Some(&serde_json::json!({"nested":[1,2,3]})),
            "a nested unknown value must round-trip by VALUE, not by shape"
        );
        // And the known fields are untouched.
        assert_eq!(back.get("thicknessMm"), Some(&serde_json::json!(200.0)));
    }

    #[test]
    fn unknown_descriptor_fields_survive_on_an_anchor_and_a_background_image() {
        // §8 U2 names "unknown DESCRIPTOR fields" first among the things a
        // writer must carry forward, and forbids silent stripping
        // "EVERYWHERE" — not only on entities. An anchor is a descriptor, and
        // it had no `unknown` map: a newer version's per-axis scale or skew
        // was dropped on the first save by an older build. Found by project
        // review, 2026-09-01.
        let json = r#"{
            "digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000",
            "anchor":{
                "translationMm":[10.0,20.0],
                "scaleUniform":2.0,
                "rotationDeg":45.0,
                "skewDeg":3.5
            },
            "stackOrdinal":2,
            "opacity":0.5
        }"#;
        let image: BackgroundImage = serde_json::from_str(json).expect("loads");
        assert_eq!(
            image.anchor.unknown.get("skewDeg"),
            Some(&serde_json::json!(3.5))
        );
        assert_eq!(image.unknown.get("opacity"), Some(&serde_json::json!(0.5)));

        let back = serde_json::to_value(&image).expect("serialises");
        assert_eq!(back.get("opacity"), Some(&serde_json::json!(0.5)));
        assert_eq!(
            back.get("anchor").and_then(|a| a.get("skewDeg")),
            Some(&serde_json::json!(3.5)),
            "the anchor's unknown field must survive at its own level"
        );
        // The known fields are untouched, and the placement still reads.
        assert_eq!(image.anchor.scale_uniform, 2.0);
        assert_eq!(image.stack_ordinal, 2);

        // And a level carrying the image round-trips it through the document.
        let mut level = Level::new("Ground");
        level.background_images.push(image);
        let mut doc = Document::new();
        doc.levels.push(level);
        let text = serde_json::to_string(&doc).expect("serialises");
        let round: Document = serde_json::from_str(&text).expect("loads");
        assert_eq!(round, doc);
        assert!(text.contains("skewDeg"), "got {text}");
    }

    #[test]
    fn a_document_with_no_unknown_fields_serialises_without_extra_keys() {
        // The map is skipped when empty, so preservation costs nothing for
        // documents this version fully understands.
        let doc = Document::with_ground_floor();
        let json = serde_json::to_string(&doc).expect("serialises");
        assert!(!json.contains("unknown"), "got {json}");
    }

    #[test]
    fn unknown_fields_survive_a_whole_document_round_trip() {
        let json = r#"{
            "levels":[{
                "id":"01948f55-6b18-7d3a-a153-1ee2f33cc403",
                "name":"Level 1","elevationMm":0.0,"floorThicknessMm":200.0,
                "defaultWallHeightMm":2500.0,"defaultCeilingHeightMm":2500.0,
                "backgroundImages":[{"digest":"sha256:abc"}]
            }],
            "walls":[],"openings":[],"joins":[]
        }"#;
        let doc: Document = serde_json::from_str(json).expect("loads");
        let out = serde_json::to_value(&doc).expect("serialises");
        assert_eq!(
            out["levels"][0]["backgroundImages"][0]["digest"],
            serde_json::json!("sha256:abc")
        );
    }

    #[test]
    fn an_unknown_field_does_not_make_an_entity_unequal_to_itself() {
        // The map participates in PartialEq, which is what makes a
        // round-trip test meaningful: two entities differing only in data
        // this version cannot read are NOT the same entity.
        let mut a = Wall::new(
            Uuid::now_v7(),
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            200.0,
            2500.0,
        );
        let b = a.clone();
        assert_eq!(a, b);
        a.unknown
            .insert("futureThing".to_owned(), serde_json::json!(1));
        assert_ne!(a, b, "dropping unknown data must be observable");
    }

    /// FORMAT-SPEC §2.3's own `roomRecord` example, byte for byte.
    ///
    /// A schema that matches the specification in prose and not in fact is the
    /// failure this catches. The literal is copied from the document rather
    /// than reconstructed, so editing one without the other fails here.
    #[test]
    fn the_format_spec_room_record_example_round_trips() {
        let literal = r#"{"id":"019534c7-0000-7000-8000-000000000006","type":"roomRecord","levelId":"019534c7-0000-7000-8000-000000000001","seedMm":[6000,4500],"floorStyleRef":"F0"}"#;
        let entity: Entity = serde_json::from_str(literal).expect("the spec example parses");
        let Entity::RoomRecord(record) = &entity else {
            panic!("the `roomRecord` tag must select the room record variant");
        };
        assert_eq!(record.seed(), Point::new(6000.0, 4500.0));
        assert_eq!(record.floor_style_ref.as_deref(), Some("F0"));
        assert_eq!(record.name, None, "the example carries no name");
        assert_eq!(record.ceiling_style_ref, None);
        assert!(
            record.unknown.is_empty(),
            "every field of the example must be TYPED, not swept into the \
             unknown map: {:?}",
            record.unknown
        );
        assert_eq!(entity.type_name(), "roomRecord");
        // And back out unchanged, key order aside.
        //
        // Compared after normalising every number to binary64, because that is
        // what §3.2 says an authoritative number IS: the example writes
        // `6000` and this writes `6000.0`, and treating those as different
        // values would be asserting a JSON spelling the specification does not
        // fix (§3.3 — whitespace and key order are insignificant).
        fn as_binary64(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Number(n) => {
                    if let Some(v) = n.as_f64() {
                        *value = serde_json::json!(v);
                    }
                }
                serde_json::Value::Array(items) => items.iter_mut().for_each(as_binary64),
                serde_json::Value::Object(fields) => {
                    fields.values_mut().for_each(as_binary64);
                }
                _ => {}
            }
        }
        let mut back: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&entity).expect("serialises"))
                .expect("valid json");
        let mut expected: serde_json::Value = serde_json::from_str(literal).expect("valid json");
        as_binary64(&mut back);
        as_binary64(&mut expected);
        assert_eq!(back, expected);
    }

    /// A room record carries the R5.2.1 field set and NOTHING ELSE.
    ///
    /// Asserted over the serialized form rather than the struct, because the
    /// wire is where "no more" is observable and where a stray field would
    /// reach another reader.
    #[test]
    fn a_room_record_serializes_exactly_the_r5_2_1_fields() {
        let mut record =
            RoomRecord::new(Uuid::from_u128(1), Uuid::from_u128(2), Point::new(1.0, 2.0));
        record.name = Some("Salon".to_owned());
        record.floor_style_ref = Some("F0".to_owned());
        record.ceiling_style_ref = Some("C0".to_owned());
        let value = serde_json::to_value(Entity::RoomRecord(record)).expect("serialises");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "ceilingStyleRef",
                "floorStyleRef",
                "id",
                "levelId",
                "name",
                "seedMm",
                "type",
            ],
            "R5.2.1 fixes the ENTIRE authoritative field set; `id` and `type` \
             are §2.3's two reserved keys"
        );
    }

    /// The optional fields are omitted when absent, not written as null.
    #[test]
    fn an_absent_optional_field_is_absent_on_the_wire() {
        let record = RoomRecord::new(Uuid::from_u128(1), Uuid::from_u128(2), Point::new(0.0, 0.0));
        let json = serde_json::to_string(&Entity::RoomRecord(record)).expect("serialises");
        assert!(!json.contains("name"), "got {json}");
        assert!(!json.contains("StyleRef"), "got {json}");
        // And the empty string is NOT absent: F37d's distinction has to
        // survive the wire, or the two edits become one.
        let mut named =
            RoomRecord::new(Uuid::from_u128(1), Uuid::from_u128(2), Point::new(0.0, 0.0));
        named.name = Some(String::new());
        let json = serde_json::to_string(&Entity::RoomRecord(named)).expect("serialises");
        assert!(json.contains(r#""name":"""#), "got {json}");
    }

    // ---- `float_roundtrip`, asserted in THIS graph (issue 0010) ----
    //
    // The root workspace enables the feature and, until these tests, nothing
    // in the root workspace failed without it. The spike has its own test,
    // but Cargo unifies features across one build graph, so the spike passes
    // while EITHER Cargo.toml carries the line. Deleting the root line was
    // therefore a green-build change: the reason was documented and
    // unenforced. These two tests are what make it fail.
    //
    // Split in two deliberately, the way the spike's pair is, so a failure
    // says which half broke: the parser configuration, or the document path
    // around it.

    /// The parser in this graph is correctly rounded.
    #[test]
    fn this_workspace_parses_floats_with_full_precision() {
        // A full-mantissa value: its shortest decimal form does not re-parse
        // exactly under serde_json's default parser.
        let witness = f64::from_bits(0x3ff23d70a3d70a3e);
        let text = serde_json::to_string(&witness).expect("serialises");
        let back: f64 = serde_json::from_str(&text).expect("parses");
        assert_eq!(
            back.to_bits(),
            witness.to_bits(),
            "this graph is parsing without `float_roundtrip`; {text} came \
             back as {:#x}. The feature is declared in the ROOT Cargo.toml.",
            back.to_bits()
        );
    }

    /// And a stored coordinate survives the document path bit-exactly, which
    /// is the property FORMAT-SPEC §3.2 actually requires of a binary64 field
    /// and §9.6 leans on when it promises a deterministic reopen.
    #[test]
    fn a_full_mantissa_coordinate_survives_a_document_round_trip() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        // Arithmetic-produced coordinates are the exposure, not grid values:
        // a snapped or split point carries a full mantissa. This witness is
        // 1244.8621756078435 mm — an ordinary plan magnitude — and the default
        // parser returns it one ULP high, as 0x40937372de299c64.
        let awkward = f64::from_bits(0x40937372de299c63);
        doc.walls.push(Wall::new(
            level,
            Point::new(awkward, 0.0),
            Point::new(awkward + 1000.0, 0.0),
            200.0,
            2500.0,
        ));
        let json = serde_json::to_string(&doc).expect("serialises");
        let back: Document = serde_json::from_str(&json).expect("parses");
        assert_eq!(
            back.walls[0].start_mm[0].to_bits(),
            awkward.to_bits(),
            "writing and reading moved the coordinate: wrote {:#x}, read \
             {:#x}. Save and reopen is not the identity without \
             `float_roundtrip`.",
            awkward.to_bits(),
            back.walls[0].start_mm[0].to_bits()
        );
    }
}
