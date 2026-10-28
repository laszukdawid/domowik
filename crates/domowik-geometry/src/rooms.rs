//! Room records against detected faces: candidacy, association, and status.
//!
//! BEHAVIOR-SPEC §5.2 and §5.3 in Rust. Everything here is DERIVED (R3.0.7):
//! it is recomputed from the walls plus the records, it is never authored by
//! an operation (R5.2.5), and persisting any of it is legal only in a
//! non-authoritative cache namespace.
//!
//! # Why this is not "which room is this record in"
//!
//! Faces carry no identity at all (R5.1.8) — none is minted by detection, none
//! appears in an operation, none is persistable. A record does not point at a
//! face; it carries a seed POINT, and the association is recomputed from
//! scratch every time the walls move. That is what makes split, merge,
//! disappearance and reappearance emit zero operations (R5.2.6): nothing was
//! ever stored that a wall edit could invalidate.
//!
//! The face indices this module hands out are exactly the ephemeral,
//! implementation-private keys R5.1.8 permits: they index one call's own
//! [`LevelRooms::faces`] and mean nothing across two calls.

use crate::aabb_tree::AabbTree;
use crate::arrangement::{classify_point, Containment};
use crate::incremental::LevelGeometry;
use crate::polygon::Polygon;
use crate::spatial::Aabb;
use crate::walls;
use domowik_format::{Document, Point, Side};
use uuid::Uuid;

/// A record's derived status. Exactly one holds at any time (R5.2.4).
///
/// Mutually exclusive BY CONSTRUCTION rather than by discipline: the seed is
/// classified once against the level's occupied region and free space, and
/// each branch of that classification produces one variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomStatus {
    /// The record is its face's effective winner: a candidate, and the lowest
    /// stable UUID among that face's candidates.
    Associated,
    /// A candidate of some face that lost that face's tie-break.
    Superseded,
    /// The seed lies on ∂O_L — the inner face of a wall band, or any other
    /// boundary point of the occupied region.
    SeedOnBoundary,
    /// The seed lies in the interior of the occupied region.
    SeedInWall,
    /// The seed lies in the unbounded component of the free space.
    SeedInExterior,
}

impl RoomStatus {
    /// The kebab-case token the specification names the status by.
    ///
    /// `associated` and `superseded` are R5.2.4's own words; the other three
    /// are quoted there verbatim as `seed-on-boundary`, `seed-in-wall` and
    /// `seed-in-exterior`. Used by the CLI report and the bridge so the two
    /// cannot invent different spellings.
    pub fn token(self) -> &'static str {
        match self {
            RoomStatus::Associated => "associated",
            RoomStatus::Superseded => "superseded",
            RoomStatus::SeedOnBoundary => "seed-on-boundary",
            RoomStatus::SeedInWall => "seed-in-wall",
            RoomStatus::SeedInExterior => "seed-in-exterior",
        }
    }

    /// Whether a record with this status is a CANDIDATE of some face (R5.2.3).
    ///
    /// Exactly the two statuses that come with a containing face. The other
    /// three are the non-candidate statuses R5.3.3 gives no deterministic
    /// label of any kind.
    pub fn is_candidate(self) -> bool {
        matches!(self, RoomStatus::Associated | RoomStatus::Superseded)
    }
}

/// One record's projection against the level's faces.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordProjection {
    /// The record's stable identifier.
    pub id: Uuid,
    /// Its single derived status.
    pub status: RoomStatus,
    /// The face strictly containing its seed, as an index into
    /// [`LevelRooms::faces`], or `None` for a non-candidate.
    ///
    /// Ephemeral and implementation-private per R5.1.8: valid only for the
    /// [`LevelRooms`] it came from.
    pub face: Option<usize>,
}

/// One level's derived room projection.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelRooms {
    /// The occupied region (R3.2.1), in canonical order.
    pub occupied: Vec<Polygon>,
    /// The bounded free-space components (R5.1.1/R5.1.2), in the R5.1.3
    /// canonical total order. This order is the iteration order for all
    /// derived room output.
    pub faces: Vec<Polygon>,
    /// Every record on the level, ordered by stable UUID.
    ///
    /// Stable-UUID order rather than document order: document order is not
    /// canonical (FORMAT-SPEC C1 says collection order does not exist), so a
    /// report built from it would differ between two documents that are the
    /// same document.
    pub records: Vec<RecordProjection>,
    /// Parallel to `faces`: the effective record of each face, or `None` when
    /// the face has no candidates.
    effective: Vec<Option<Uuid>>,
}

impl LevelRooms {
    /// The record effectively associated with `face`, or `None` when the face
    /// has no candidates and therefore shows no label (R5.3.3 absence).
    pub fn effective_record(&self, face: usize) -> Option<Uuid> {
        self.effective.get(face).copied().flatten()
    }

    /// The projection of `id`, if that record is on this level.
    pub fn record(&self, id: Uuid) -> Option<&RecordProjection> {
        self.records.iter().find(|r| r.id == id)
    }
}

/// Projects `level_id`'s room records against its detected faces.
///
/// Levels never interact (R5.1.1), so this reads exactly one level's walls and
/// exactly one level's records and nothing else in the document.
pub fn project_level(doc: &Document, level_id: Uuid) -> LevelRooms {
    let solids = walls::level_solids(doc, level_id);
    let (occupied, faces) = walls::plan(&solids.walls, &solids.joins);
    project_against(doc, level_id, occupied, faces)
}

/// [`project_level`] against geometry the caller already computed.
///
/// The exporters and the bridge already hold the occupied region and the faces
/// — they draw them — and recomputing a boolean union to answer "what is this
/// room called" would double the cost of every render.
///
/// # Precondition: `occupied` must be a canonical UNION
///
/// Its members must have disjoint interiors and share no boundary lying INSIDE
/// the union. [`walls::plan`] guarantees that — it is what taking a boolean
/// union means — and every caller in the tree gets its argument from there.
///
/// Hand-built input can break it, silently. Given two abutting rectangles
/// `[0,1]×[0,1]` and `[1,2]×[0,1]` passed as separate members, a seed at
/// `(1, 0.5)` is strictly inside their union and should be `seed-in-wall`; the
/// first rectangle reports it on ITS boundary and classification stops there
/// with `seed-on-boundary`. Found by adversarial review. The fix is to hand
/// this function a union rather than to re-derive one here, which would do the
/// work twice on every render to defend against an input no caller produces.
pub fn project_against(
    doc: &Document,
    level_id: Uuid,
    occupied: Vec<Polygon>,
    faces: Vec<Polygon>,
) -> LevelRooms {
    let records = reconcile(doc, level_id, faces.len(), |seed| {
        classify_seed(seed, &occupied, &faces)
    });
    LevelRooms {
        occupied,
        faces,
        records: records.0,
        effective: records.1,
    }
}

/// [`project_level`] against a level's retained incremental geometry, which
/// must be synced to `doc`'s level `level_id`.
///
/// Same result as [`project_against`] on `geometry.plan()`, but each seed is
/// located through the geometry's retained ring buckets
/// ([`LevelGeometry::locate_seed`]) instead of against every polygon, so a
/// record costs the rings near its seed rather than the whole level.
pub fn project_retained(doc: &Document, level_id: Uuid, geometry: &LevelGeometry) -> LevelRooms {
    let (occupied, faces) = geometry.plan();
    let records = reconcile(doc, level_id, faces.len(), |seed| {
        geometry.locate_seed(seed)
    });
    LevelRooms {
        occupied,
        faces,
        records: records.0,
        effective: records.1,
    }
}

/// R5.2.4's reconciliation of `level_id`'s records, given where each seed
/// lies: every record's status and face, and each of the `face_count` faces'
/// effective record. Emits no operations (R5.2.5/R5.2.6).
fn reconcile(
    doc: &Document,
    level_id: Uuid,
    face_count: usize,
    locate: impl Fn(Point) -> SeedPlacement,
) -> (Vec<RecordProjection>, Vec<Option<Uuid>>) {
    let mut records: Vec<RecordProjection> = doc
        .room_records_on_level(level_id)
        .map(|record| {
            let (status, face) = match locate(record.seed()) {
                SeedPlacement::InFace(index) => (RoomStatus::Superseded, Some(index)),
                SeedPlacement::OnBoundary => (RoomStatus::SeedOnBoundary, None),
                SeedPlacement::InWall => (RoomStatus::SeedInWall, None),
                SeedPlacement::InExterior => (RoomStatus::SeedInExterior, None),
            };
            RecordProjection {
                id: record.id,
                status,
                face,
            }
        })
        .collect();

    // Stable-UUID order, which is both the canonical report order and the
    // order the tie-break below needs: the first candidate encountered for a
    // face is then already the lowest-UUID one.
    //
    // `Uuid`'s own ordering IS the E5 unsigned-128-bit order the specification
    // means by "the existing semantic total order over stable UUIDs" — the
    // crate compares the sixteen bytes big-endian, which is the same relation.
    records.sort_by_key(|r| r.id);

    let mut effective: Vec<Option<Uuid>> = vec![None; face_count];
    for projection in &mut records {
        let Some(face) = projection.face else {
            continue;
        };
        // R5.2.4: exactly one record effectively associates with a face, and
        // it is the lowest stable UUID among its candidates. Records are
        // already in that order, so the FIRST candidate of a face wins and
        // every later one keeps the `superseded` status it arrived with.
        if effective[face].is_none() {
            effective[face] = Some(projection.id);
            projection.status = RoomStatus::Associated;
        }
    }
    (records, effective)
}

/// Where a seed lies, in the four mutually exclusive places R5.2.4 names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedPlacement {
    /// Strictly inside the bounded face at this index.
    InFace(usize),
    OnBoundary,
    InWall,
    InExterior,
}

/// Classifies one seed against a level's occupied region and faces.
///
/// The order of the tests is the specification's own case split, and it is not
/// interchangeable: the boundary case is settled FIRST, against the occupied
/// region, because ∂O_L is also the boundary of every face — a seed sitting on
/// a wall's inner edge is `seed-on-boundary`, never "just inside the room",
/// and R5.2.3 requires STRICT interiority for candidacy.
pub(crate) fn classify_seed(seed: Point, occupied: &[Polygon], faces: &[Polygon]) -> SeedPlacement {
    for polygon in occupied {
        match classify_against(seed, polygon) {
            Containment::Boundary => return SeedPlacement::OnBoundary,
            Containment::Inside => return SeedPlacement::InWall,
            Containment::Outside => {}
        }
    }
    for (index, face) in faces.iter().enumerate() {
        match classify_against(seed, face) {
            // Unreachable in a consistent arrangement — a face's boundary IS
            // part of ∂O_L and the loop above already returned — but the two
            // computations are separate polygons, so falling through to the
            // exterior rather than claiming interiority is the safe reading.
            Containment::Boundary => return SeedPlacement::OnBoundary,
            Containment::Inside => return SeedPlacement::InFace(index),
            Containment::Outside => {}
        }
    }
    SeedPlacement::InExterior
}

/// Classifies a point against a whole polygon, holes included.
///
/// A point inside a hole is OUTSIDE the polygon, and a point on a hole ring is
/// on the polygon's boundary — both of which a test against the outer ring
/// alone gets wrong, and both of which occur in the very first holed fixture
/// (F27's courtyard with its island).
fn classify_against(p: Point, polygon: &Polygon) -> Containment {
    match classify_point(p, &polygon.outer) {
        Containment::Outside => Containment::Outside,
        Containment::Boundary => Containment::Boundary,
        Containment::Inside => {
            for hole in &polygon.holes {
                match classify_point(p, hole) {
                    Containment::Boundary => return Containment::Boundary,
                    Containment::Inside => return Containment::Outside,
                    Containment::Outside => {}
                }
            }
            Containment::Inside
        }
    }
}

/// A point strictly inside `face`, or `None` for a degenerate one.
///
/// # Why the core computes this and a front end does not
///
/// A user naming a room points at it; a command creating a record needs a
/// SEED, and R5.2.3 makes candidacy a strict-interior test. Somewhere those
/// two meet, and ED-0101 says where: logic deciding what a document BECOMES
/// lives behind the core's boundary. A front end that picked its own interior
/// point would be reimplementing containment, and would put a seed in a wall
/// the first time it met an L-shaped room whose centroid lies outside it.
///
/// # How
///
/// The centroid of the outer ring first, because for the rectangles most rooms
/// are it is both correct and obvious. When that lands outside the face — a
/// non-convex room, or one whose hole swallows the centroid — the interior is
/// found by scanning: take a horizontal line strictly BETWEEN two adjacent
/// distinct vertex heights, so it passes through no vertex; cross it with every
/// edge of every ring; sort the crossings, which then alternate outside and
/// inside; and take the midpoint of the widest inside interval. The widest,
/// rather than the first, so the point sits away from the boundary where exact
/// arithmetic has the most room.
///
/// Deterministic: the scan order comes from the canonical ring order, and ties
/// between equally wide intervals resolve to the first in that order.
///
/// # Limit: a face can be too thin to hold a representable point
///
/// `None` means none was found, and for a face narrower than the spacing of
/// the doubles around it none exists: a rectangle one ulp wide has positive
/// real area, but its centroid and every scan midpoint round onto a boundary
/// vertex, so nothing classifies as strictly inside. Found by adversarial
/// review; about 2 x 10⁻¹⁶ mm² at unit scale, which no wall arrangement in
/// millimetres produces.
///
/// Callers must handle `None` rather than assume a face yields a seed. The
/// editor does: it says the room has no interior to seed, instead of appearing
/// to do nothing.
pub fn interior_point(face: &Polygon) -> Option<Point> {
    if face.outer.len() < 3 {
        return None;
    }
    let centroid = {
        let n = face.outer.len() as f64;
        let x = face.outer.iter().map(|p| p.x).sum::<f64>() / n;
        let y = face.outer.iter().map(|p| p.y).sum::<f64>() / n;
        Point::new(x, y)
    };
    if classify_against(centroid, face) == Containment::Inside {
        return Some(centroid);
    }

    let mut heights: Vec<f64> = std::iter::once(&face.outer)
        .chain(face.holes.iter())
        .flat_map(|ring| ring.iter().map(|p| p.y))
        .collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    heights.dedup();

    let mut best: Option<(f64, Point)> = None;
    for pair in heights.windows(2) {
        let y = (pair[0] + pair[1]) / 2.0;
        if !y.is_finite() {
            continue;
        }
        let mut crossings: Vec<f64> = Vec::new();
        for ring in std::iter::once(&face.outer).chain(face.holes.iter()) {
            for i in 0..ring.len() {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                // The half-open rule counts a vertex once. `y` sits strictly
                // between two distinct vertex heights, so it meets no vertex
                // and the rule is belt and braces rather than load-bearing.
                if (a.y > y) != (b.y > y) {
                    crossings.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                }
            }
        }
        crossings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        for span in crossings.as_chunks::<2>().0 {
            let width = span[1] - span[0];
            let candidate = Point::new((span[0] + span[1]) / 2.0, y);
            if width > best.map_or(0.0, |(w, _)| w)
                && classify_against(candidate, face) == Containment::Inside
            {
                best = Some((width, candidate));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// The displayed area of `area_mm2`, as R5.3.2 defines it.
///
/// Always square metres with two decimals, regardless of the §1.1 display-unit
/// preference — a deliberate v0.1 exception, because that preference governs
/// LENGTHS and an area shown in square feet beside a plan drawn in millimetres
/// helps nobody.
///
/// # APPROXIMATE: floating-area rounding, with a known numerical gap
///
/// This computes the rounded hundredths ITSELF, in floating point, from an
/// already rounded area. Product code displays areas with
/// [`display_polygon_area_m2`], which rounds the EXACT area
/// ([`crate::exact_area::display_hundredths_m2`]); this remains for callers
/// that hold only a number.
///
/// # Ties, and why the division is by 10,000 rather than 1,000,000
///
/// R5.3.2 resolves an EXACT half to the even last digit. Rounding a value
/// already divided into square metres cannot see an exact half reliably:
/// 30.245 is not representable in binary, so the rounding would be applied to
/// whichever neighbour the division happened to land on. Dividing the square
/// millimetres by 10,000 instead yields the value in HUNDREDTHS of a square
/// metre — 30,245,000 → 3024.5, exactly representable — so the worked examples
/// land on real ties that [`f64::round_ties_even`] resolves as specified.
///
/// # The gap, stated rather than implied
///
/// That is NOT a general equivalence, and the division is where it breaks: a
/// single f64 division can round an exact quotient that is merely NEAR a tie
/// ONTO the tie, and ties-to-even then walks to the even neighbour — which may
/// be the wrong one. Counterexample, found by adversarial review and verified
/// against exact rational arithmetic: 9.922544585489499e16 mm² yields
/// 9,922,544,585,490 here and 9,922,544,585,489 exactly.
///
/// That input is far outside anything a document can hold — derived geometry
/// is bounded by [`domowik_format::DERIVED_COORD_BOUND_MM`] rather than by the
/// stored centre-line bound, so the widest derived extent is 2.2 × 10⁷ mm and
/// the largest derived area about 4.84 × 10¹⁴ mm², some two hundred times
/// below the counterexample. Sampling every tie and its nearest neighbours at
/// every magnitude through and past that bound found no divergence.
///
/// **Sampling is not a proof, and no in-domain exactness is claimed here.**
/// The exact path is [`display_polygon_area_m2`], which needs the polygon
/// rather than a number; its witness test shows this function one hundredth
/// off on an in-domain room.
///
/// # Negative zero is forbidden
///
/// Face areas are measures of components of an OPEN set and are strictly
/// positive (R5.1.6), so no negative input is meaningful; anything at or below
/// zero, and anything at or below 0.005 m², renders `0.00`. `-0.00` is what a
/// naive `{:.2}` prints for a small negative, and R5.3.2 forbids it outright.
/// The R5.3.2 display area of `polygon`, exactly: its exact area rounded
/// half-to-even to hundredths of m², as `"12.54"`. This is the entry point
/// product code uses to SHOW an area (CLI reports and labels, the editor's
/// derived response); [`display_area_m2`] rounds an already rounded float
/// and can be one hundredth off near a tie.
///
/// Filtered ([`crate::exact_area::display_hundredths_m2`]): ordinary areas
/// never build exact arithmetic. `None` for non-finite coordinates or a
/// value beyond `i64` hundredths — never a fabricated `"0.00"`.
pub fn display_polygon_area_m2(polygon: &Polygon) -> Option<String> {
    let hundredths = crate::exact_area::display_hundredths_m2(polygon)?;
    if hundredths <= 0 {
        return Some("0.00".to_owned());
    }
    Some(format!("{}.{:02}", hundredths / 100, hundredths % 100))
}

/// Approximate: rounds a float area that is itself rounded. Product code
/// displays areas with [`display_polygon_area_m2`]; this remains for callers
/// that hold only a number.
pub fn display_area_m2(area_mm2: f64) -> String {
    if !area_mm2.is_finite() || area_mm2 <= 0.0 {
        return "0.00".to_owned();
    }
    let hundredths = (area_mm2 / 10_000.0).round_ties_even();
    if hundredths <= 0.0 {
        return "0.00".to_owned();
    }
    let hundredths = hundredths as i128;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// Distance from a wall face line within which a room boundary edge lies on
/// that face: ten topology quanta (ED-0110), far below any real wall gap.
const FACE_TOLERANCE_MM: f64 = 10.0 / 1024.0;

/// Wall faces bounding `face` over positive length, as `(wall id, side)`,
/// sorted and unique. An edge lies on a face when both its ends are within
/// [`FACE_TOLERANCE_MM`] of the face line and its projection overlaps the
/// wall's axis span by more than that tolerance, so collinear walls sharing
/// one room edge both match. Faces are wall-granular: a face that also bounds
/// another room is still reported whole. `solids` must be the face's level.
/// Deterministic; `O(edges × walls)`.
pub fn bounding_wall_faces(solids: &walls::LevelSolids, face: &Polygon) -> Vec<(Uuid, Side)> {
    let mut out = Vec::new();
    for (i, w) in solids.walls.iter().enumerate() {
        for side in faces_touching(w, face) {
            out.push((solids.ids[i], side));
        }
    }
    out.sort_by_key(|(id, side)| (*id, *side == Side::Right));
    out.dedup();
    out
}

/// The first effective record, in canonical face order, whose face bounds
/// each `[Left, Right]` wall face; parallel to `solids.walls`. For labelling.
/// Walls are indexed once in an AABB tree and only the edges of faces with
/// an effective record are visited, so the cost is `O(W log W + E·k)` for
/// `E` such edges and `k` walls near each; nothing is built when no face has
/// a record. `solids` must be the level `rooms` was projected from.
pub fn wall_face_records(
    rooms: &LevelRooms,
    solids: &walls::LevelSolids,
) -> Vec<[Option<Uuid>; 2]> {
    let mut out = vec![[None, None]; solids.walls.len()];
    let named: Vec<_> = (0..rooms.faces.len())
        .filter_map(|f| Some((rooms.effective_record(f)?, &rooms.faces[f])))
        .collect();
    if named.is_empty() {
        return out;
    }
    let mut index = AabbTree::new();
    let items = solids
        .walls
        .iter()
        .enumerate()
        .filter_map(|(i, w)| Some((Aabb::of(&w.rectangle()?)?, i)))
        .collect();
    index.extend_empty(items);
    for (record, face) in named {
        for ring in std::iter::once(&face.outer).chain(face.holes.iter()) {
            for k in 0..ring.len() {
                let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
                let mut probe = Aabb::from_corners(a, b);
                probe.min_x -= FACE_TOLERANCE_MM;
                probe.min_y -= FACE_TOLERANCE_MM;
                probe.max_x += FACE_TOLERANCE_MM;
                probe.max_y += FACE_TOLERANCE_MM;
                index.query(&probe, |_, _, &i| {
                    if let Some(side) = edge_side(&solids.walls[i], a, b) {
                        out[i][usize::from(side == Side::Right)].get_or_insert(record);
                    }
                });
            }
        }
    }
    out
}

fn faces_touching(w: &walls::WallSolid, face: &Polygon) -> Vec<Side> {
    let mut sides = Vec::new();
    for ring in std::iter::once(&face.outer).chain(face.holes.iter()) {
        for k in 0..ring.len() {
            if let Some(side) = edge_side(w, ring[k], ring[(k + 1) % ring.len()]) {
                if !sides.contains(&side) {
                    sides.push(side);
                }
            }
        }
    }
    sides
}

/// The face of `w` the boundary edge `a→b` lies on, if any.
fn edge_side(w: &walls::WallSolid, a: Point, b: Point) -> Option<Side> {
    let (dx, dy) = (w.end.x - w.start.x, w.end.y - w.start.y);
    let len = dx.hypot(dy);
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    let (ux, uy) = (dx / len, dy / len);
    let half = w.thickness_mm / 2.0;
    // Frame coordinates: `s` along the axis from start, `η` toward Left.
    let frame = |p: Point| {
        let (x, y) = (p.x - w.start.x, p.y - w.start.y);
        (x * ux + y * uy, -x * uy + y * ux)
    };
    let (a, b) = (frame(a), frame(b));
    let overlap = a.0.max(b.0).min(len) - a.0.min(b.0).max(0.0);
    if overlap <= FACE_TOLERANCE_MM {
        return None;
    }
    [(Side::Left, half), (Side::Right, -half)]
        .into_iter()
        .find(|(_, eta)| {
            (a.1 - eta).abs() <= FACE_TOLERANCE_MM && (b.1 - eta).abs() <= FACE_TOLERANCE_MM
        })
        .map(|(side, _)| side)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codex GPT-6-Astra's witness: a joined room of `walls::plan` within the
    /// coordinate domain whose exact area is 53,599,999.985 000…95 m² — just
    /// above the tie — so the exact display is `53599999.99` while rounding
    /// the float area gives `.98`. Product callers display through
    /// `display_polygon_area_m2`.
    #[test]
    fn displayed_areas_round_the_exact_area_not_the_float_one() {
        use crate::walls::{Endpoint, JoinMember, JoinSpec, WallSolid};
        let (width, height) = (10_239_999_999.0 / 1024.0, 5_488_639_999.0 / 1024.0);
        let corners = [
            Point::new(-5_000_100.0, -5_000_100.0),
            Point::new(-4_999_900.0 + width, -5_000_100.0),
            Point::new(-4_999_900.0 + width, -4_999_900.0 + height),
            Point::new(-5_000_100.0, -4_999_900.0 + height),
        ];
        let solids: Vec<WallSolid> = (0..4)
            .map(|i| WallSolid::new(corners[i], corners[(i + 1) % 4], 200.0))
            .collect();
        let joins: Vec<JoinSpec> = (0..4)
            .map(|i| JoinSpec {
                members: vec![
                    JoinMember {
                        wall: i,
                        endpoint: Endpoint::End,
                    },
                    JoinMember {
                        wall: (i + 1) % 4,
                        endpoint: Endpoint::Start,
                    },
                ],
            })
            .collect();
        let (_, faces) = walls::plan(&solids, &joins);
        assert_eq!(faces.len(), 1);
        assert_eq!(
            display_polygon_area_m2(&faces[0]).as_deref(),
            Some("53599999.99")
        );
        assert_eq!(display_area_m2(faces[0].area_mm2()), "53599999.98");
        // An ordinary room takes the filtered fast path and agrees.
        let b4 = b4();
        let rooms = project_level(&b4.0, b4.1);
        assert_eq!(
            display_polygon_area_m2(&rooms.faces[0]).as_deref(),
            Some("10.64")
        );
        let nan = Polygon {
            outer: vec![
                Point::new(0.0, 0.0),
                Point::new(f64::NAN, 0.0),
                Point::new(0.0, 1.0),
            ],
            holes: vec![],
        };
        assert_eq!(display_polygon_area_m2(&nan), None);
    }
    use domowik_format::{JoinMember, Level, Role, RoomRecord, Wall, WallJoin};

    /// Base arrangement B4 (BEHAVIOR-SPEC §5.4): a 4000 x 3000 closed ring of
    /// t=200 walls with corner joins. Its single face has canonical ring
    /// (100,100) (3900,100) (3900,2900) (100,2900) and area 10,640,000 mm².
    fn b4() -> (Document, Uuid) {
        let mut doc = Document::new();
        let level = Level::new("L1");
        let level_id = level.id;
        doc.levels.push(level);
        let corners = [
            (0.0, 0.0),
            (4000.0, 0.0),
            (4000.0, 3000.0),
            (0.0, 3000.0),
            (0.0, 0.0),
        ];
        let mut ids = Vec::new();
        for pair in corners.windows(2) {
            let wall = Wall::new(
                level_id,
                Point::new(pair[0].0, pair[0].1),
                Point::new(pair[1].0, pair[1].1),
                200.0,
                2500.0,
            );
            ids.push(wall.id);
            doc.walls.push(wall);
        }
        for i in 0..4 {
            doc.joins.push(WallJoin::new(vec![
                JoinMember::new(ids[i], Role::End),
                JoinMember::new(ids[(i + 1) % 4], Role::Start),
            ]));
        }
        (doc, level_id)
    }

    fn record(doc: &mut Document, level: Uuid, id: u128, seed: (f64, f64)) -> Uuid {
        let r = RoomRecord::new(Uuid::from_u128(id), level, Point::new(seed.0, seed.1));
        let id = r.id;
        doc.room_records.push(r);
        id
    }

    #[test]
    fn f25_a_seed_strictly_inside_the_face_associates() {
        let (mut doc, level) = b4();
        let r1 = record(&mut doc, level, 1, (2000.0, 1500.0));
        let rooms = project_level(&doc, level);
        assert_eq!(rooms.faces.len(), 1, "B4 has one interior face");
        let p = rooms.record(r1).expect("projected");
        assert_eq!(p.status, RoomStatus::Associated);
        assert_eq!(p.face, Some(0));
        assert_eq!(rooms.effective_record(0), Some(r1));
        assert_eq!(display_area_m2(rooms.faces[0].area_mm2()), "10.64");
    }

    #[test]
    fn f32_the_three_non_candidate_seed_statuses() {
        // (a) on the inner edge of W1's band, (b) inside the band, (c) far
        // outside. Each is accepted — seed LOCATION never rejects — and each
        // takes one exclusive status.
        for (seed, expected) in [
            ((2000.0, 100.0), RoomStatus::SeedOnBoundary),
            ((2000.0, 0.0), RoomStatus::SeedInWall),
            ((10000.0, 1500.0), RoomStatus::SeedInExterior),
        ] {
            let (mut doc, level) = b4();
            let id = record(&mut doc, level, 1, seed);
            let rooms = project_level(&doc, level);
            let p = rooms.record(id).expect("projected");
            assert_eq!(p.status, expected, "seed {seed:?}");
            assert_eq!(p.face, None, "seed {seed:?} has no face");
            assert_eq!(rooms.effective_record(0), None, "no candidate, no label");
        }
    }

    #[test]
    fn f33_two_seeds_in_one_face_resolve_by_lowest_stable_uuid() {
        let (mut doc, level) = b4();
        // Pushed in DESCENDING id order, so a projection that trusted document
        // order would name the wrong winner.
        let r2 = record(&mut doc, level, 2, (3000.0, 1500.0));
        let r1 = record(&mut doc, level, 1, (2000.0, 1500.0));
        let rooms = project_level(&doc, level);
        assert_eq!(rooms.record(r1).expect("r1").status, RoomStatus::Associated);
        assert_eq!(rooms.record(r2).expect("r2").status, RoomStatus::Superseded);
        assert_eq!(rooms.effective_record(0), Some(r1));
        // Both are candidates of the same face: superseded is impossible
        // without candidacy (R5.3.3).
        assert_eq!(rooms.record(r2).expect("r2").face, Some(0));
    }

    #[test]
    fn records_are_reported_in_stable_uuid_order_not_document_order() {
        let (mut doc, level) = b4();
        record(&mut doc, level, 3, (2000.0, 1500.0));
        record(&mut doc, level, 1, (2500.0, 1500.0));
        record(&mut doc, level, 2, (3000.0, 1500.0));
        let rooms = project_level(&doc, level);
        let ids: Vec<u128> = rooms.records.iter().map(|r| r.id.as_u128()).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn a_record_on_another_level_is_not_projected_here() {
        // R5.1.1: levels never interact.
        let (mut doc, level) = b4();
        let other = Level::new("L2");
        let other_id = other.id;
        doc.levels.push(other);
        let elsewhere = record(&mut doc, other_id, 1, (2000.0, 1500.0));
        let rooms = project_level(&doc, level);
        assert!(rooms.records.is_empty());
        assert_eq!(rooms.record(elsewhere), None);
        assert_eq!(rooms.effective_record(0), None);
    }

    #[test]
    fn r5_3_2_display_rounding_matches_every_worked_example() {
        // The four examples R5.3.2 spells out, including both tie directions.
        assert_eq!(display_area_m2(10_640_000.0), "10.64");
        assert_eq!(display_area_m2(38_200_000.0), "38.20");
        assert_eq!(
            display_area_m2(30_245_000.0),
            "30.24",
            "tie rounds to even 4"
        );
        assert_eq!(
            display_area_m2(10_655_000.0),
            "10.66",
            "tie rounds to even 6"
        );
    }

    #[test]
    fn r5_3_2_never_renders_negative_zero() {
        // Strictly positive areas are the only meaningful input, and the rule
        // forbids `-0.00` outright rather than leaving it to formatting.
        for input in [0.0, -0.0, -1.0, 1.0, 4_999.0, f64::NAN] {
            let shown = display_area_m2(input);
            assert_eq!(shown, "0.00", "input {input}");
            assert!(!shown.starts_with('-'));
        }
        // R5.3.2's two statements AGREE at the boundary rather than needing
        // one to rescue the other: 0.005 m² is 5,000 mm², which is an exact
        // tie, and the tie goes to the even last digit — 0. So "any value
        // below 0.005 m² displays as 0.00" and the tie rule land on the same
        // answer there, and 5,001 mm² is the first value that shows anything
        // (issue 0030, 2026-09-12).
        assert_eq!(display_area_m2(4_999.0), "0.00");
        assert_eq!(display_area_m2(5_000.0), "0.00", "an exact tie, to even");
        assert_eq!(display_area_m2(5_001.0), "0.01");
    }

    #[test]
    fn an_interior_point_is_found_for_a_rectangle_an_l_and_an_annulus() {
        // A rectangle: the centroid is inside and is used.
        let (doc, level) = b4();
        let rooms = project_level(&doc, level);
        let p = interior_point(&rooms.faces[0]).expect("a rectangle has an interior");
        assert_eq!(p, Point::new(2000.0, 1500.0));

        // An L: BEHAVIOR-SPEC F26's six-wall loop, whose outer-ring centroid
        // falls in the notch outside the room. The scan has to find the point.
        let mut doc = Document::new();
        let level = Level::new("L1");
        let level_id = level.id;
        doc.levels.push(level);
        let corners = [
            (0.0, 0.0),
            (8000.0, 0.0),
            (8000.0, 3000.0),
            (3000.0, 3000.0),
            (3000.0, 6000.0),
            (0.0, 6000.0),
            (0.0, 0.0),
        ];
        let mut ids = Vec::new();
        for pair in corners.windows(2) {
            let wall = Wall::new(
                level_id,
                Point::new(pair[0].0, pair[0].1),
                Point::new(pair[1].0, pair[1].1),
                200.0,
                2500.0,
            );
            ids.push(wall.id);
            doc.walls.push(wall);
        }
        for i in 0..6 {
            doc.joins.push(WallJoin::new(vec![
                JoinMember::new(ids[i], Role::End),
                JoinMember::new(ids[(i + 1) % 6], Role::Start),
            ]));
        }
        let rooms = project_level(&doc, level_id);
        let face = &rooms.faces[0];
        let p = interior_point(face).expect("an L has an interior");
        assert_eq!(
            classify_against(p, face),
            Containment::Inside,
            "the point must be STRICTLY inside, or a record seeded there would \
             land in a wall"
        );

        // And seeding a record there actually associates, which is the only
        // property a caller cares about.
        let mut record = RoomRecord::new(Uuid::from_u128(1), level_id, p);
        record.name = Some("L-room".to_owned());
        doc.room_records.push(record);
        let rooms = project_level(&doc, level_id);
        assert_eq!(
            rooms.record(Uuid::from_u128(1)).expect("projected").status,
            RoomStatus::Associated
        );
    }

    #[test]
    fn an_interior_point_avoids_a_hole_that_swallows_the_centroid() {
        // F27's courtyard: the annulus centroid sits inside the island, which
        // is a HOLE of the courtyard face and therefore not the courtyard.
        let mut doc = Document::new();
        let level = Level::new("L1");
        let level_id = level.id;
        doc.levels.push(level);
        let loop_of = |doc: &mut Document, pts: [(f64, f64); 4]| {
            let mut ids = Vec::new();
            for i in 0..4 {
                let a = pts[i];
                let b = pts[(i + 1) % 4];
                let wall = Wall::new(
                    level_id,
                    Point::new(a.0, a.1),
                    Point::new(b.0, b.1),
                    200.0,
                    2500.0,
                );
                ids.push(wall.id);
                doc.walls.push(wall);
            }
            for i in 0..4 {
                doc.joins.push(WallJoin::new(vec![
                    JoinMember::new(ids[i], Role::End),
                    JoinMember::new(ids[(i + 1) % 4], Role::Start),
                ]));
            }
        };
        loop_of(
            &mut doc,
            [(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
        );
        loop_of(
            &mut doc,
            [
                (2500.0, 2000.0),
                (5500.0, 2000.0),
                (5500.0, 4000.0),
                (2500.0, 4000.0),
            ],
        );
        let rooms = project_level(&doc, level_id);
        for face in &rooms.faces {
            let p = interior_point(face).expect("both faces have interiors");
            assert_eq!(classify_against(p, face), Containment::Inside);
        }
        // The courtyard's own point must not be in the island, and vice versa.
        let a = interior_point(&rooms.faces[0]).expect("p");
        let b = interior_point(&rooms.faces[1]).expect("p");
        assert_ne!(
            classify_against(a, &rooms.faces[1]),
            Containment::Inside,
            "the courtyard's point must not land in the island"
        );
        assert_ne!(classify_against(b, &rooms.faces[0]), Containment::Inside);
    }

    #[test]
    fn interior_point_is_deterministic() {
        let (doc, level) = b4();
        let rooms = project_level(&doc, level);
        assert_eq!(
            interior_point(&rooms.faces[0]),
            interior_point(&rooms.faces[0])
        );
    }

    #[test]
    fn f27_an_island_inside_a_courtyard_is_a_different_room() {
        // The courtyard face carries the island as a HOLE, so a seed inside
        // the island must not be read as a courtyard candidate.
        let mut doc = Document::new();
        let level = Level::new("L1");
        let level_id = level.id;
        doc.levels.push(level);
        let loop_of = |doc: &mut Document, pts: [(f64, f64); 4]| {
            let mut ids = Vec::new();
            for i in 0..4 {
                let a = pts[i];
                let b = pts[(i + 1) % 4];
                let wall = Wall::new(
                    level_id,
                    Point::new(a.0, a.1),
                    Point::new(b.0, b.1),
                    200.0,
                    2500.0,
                );
                ids.push(wall.id);
                doc.walls.push(wall);
            }
            for i in 0..4 {
                doc.joins.push(WallJoin::new(vec![
                    JoinMember::new(ids[i], Role::End),
                    JoinMember::new(ids[(i + 1) % 4], Role::Start),
                ]));
            }
        };
        loop_of(
            &mut doc,
            [(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
        );
        loop_of(
            &mut doc,
            [
                (2500.0, 2000.0),
                (5500.0, 2000.0),
                (5500.0, 4000.0),
                (2500.0, 4000.0),
            ],
        );
        let courtyard = record(&mut doc, level_id, 1, (1200.0, 3000.0));
        let island = record(&mut doc, level_id, 2, (4000.0, 3000.0));

        let rooms = project_level(&doc, level_id);
        assert_eq!(
            rooms.faces.len(),
            2,
            "courtyard annulus and island interior"
        );
        let c = rooms.record(courtyard).expect("courtyard");
        let i = rooms.record(island).expect("island");
        assert_eq!(c.status, RoomStatus::Associated);
        assert_eq!(i.status, RoomStatus::Associated);
        assert_ne!(c.face, i.face, "two distinct rooms, not one and its hole");

        // The island face is F27's, exactly: ring (2600,2100) (5400,2100)
        // (5400,3900) (2600,3900), area 2,800 x 1,800 = 5,040,000 mm².
        let island_face = i.face.expect("the island seed is a candidate");
        assert_eq!(rooms.faces[island_face].area_mm2(), 5_040_000.0);
        assert_eq!(display_area_m2(rooms.faces[island_face].area_mm2()), "5.04");

        // The courtyard face carries the island as a HOLE, which is the
        // structural claim this fixture exists to make.
        let courtyard_face = c.face.expect("the courtyard seed is a candidate");
        assert_eq!(rooms.faces[courtyard_face].holes.len(), 1);
    }

    /// F27's courtyard hole is chamfered by the R3.2.3 patch rule, and this
    /// pins that the arrangement and the fixture agree on it.
    ///
    /// F27 states the hole ring as the EIGHT-vertex chamfered ring
    /// `(2400,2000) (2400,4000) (2500,4100) (5500,4100) (5600,4000)
    /// (5600,2000) (5500,1900) (2500,1900)` and its area as
    /// 45,240,000 − 7,020,000 = 38,220,000 mm², which is what the arrangement
    /// produces.
    ///
    /// The chamfers are not a defect. Each island corner is two walls
    /// TERMINATING at a shared point, so R3.2.3's patch is the convex hull of
    /// the two cross-sections — a diamond, which seals half of the 100 x 100
    /// corner square and leaves the other half in the free space. That is
    /// exactly the construction F26 spells out for its own inner corner, where
    /// it names the diagonal edge `(3000,2900)→(2900,3000)` and ADDS the
    /// resulting 5,000 mm² triangle to the room's area, as
    /// `21,840,000 + 8,400,000 + 5,000`. Four island corners give four such
    /// triangles: 4 x 5,000 = 20,000 mm², precisely the gap.
    ///
    /// F26 and F27 once disagreed here: F27 stated a four-vertex rectangle and
    /// 38,200,000 mm², which R3.2.3's hull cannot produce. F26 was the one
    /// that showed its work, so F27 was corrected to match it (ED-0103;
    /// BEHAVIOR-SPEC §5.4), along with the three F39 values that inherited the
    /// same arithmetic. This test therefore pins CONFORMANCE rather than a
    /// divergence, and a later change to either side is still visible rather
    /// than silent. Room projection consumes `walls::plan` and does not
    /// compute patches.
    #[test]
    fn f27_courtyard_hole_is_chamfered_as_the_f26_patch_rule_requires() {
        let mut doc = Document::new();
        let level = Level::new("L1");
        let level_id = level.id;
        doc.levels.push(level);
        let loop_of = |doc: &mut Document, pts: [(f64, f64); 4]| {
            let mut ids = Vec::new();
            for i in 0..4 {
                let a = pts[i];
                let b = pts[(i + 1) % 4];
                let wall = Wall::new(
                    level_id,
                    Point::new(a.0, a.1),
                    Point::new(b.0, b.1),
                    200.0,
                    2500.0,
                );
                ids.push(wall.id);
                doc.walls.push(wall);
            }
            for i in 0..4 {
                doc.joins.push(WallJoin::new(vec![
                    JoinMember::new(ids[i], Role::End),
                    JoinMember::new(ids[(i + 1) % 4], Role::Start),
                ]));
            }
        };
        loop_of(
            &mut doc,
            [(0.0, 0.0), (8000.0, 0.0), (8000.0, 6000.0), (0.0, 6000.0)],
        );
        loop_of(
            &mut doc,
            [
                (2500.0, 2000.0),
                (5500.0, 2000.0),
                (5500.0, 4000.0),
                (2500.0, 4000.0),
            ],
        );
        let courtyard = record(&mut doc, level_id, 1, (1200.0, 3000.0));
        let rooms = project_level(&doc, level_id);
        let face = rooms
            .record(courtyard)
            .expect("courtyard")
            .face
            .expect("candidate");

        assert_eq!(
            rooms.faces[face].holes[0].len(),
            8,
            "four chamfered corners, as F27 states"
        );
        assert_eq!(
            rooms.faces[face].area_mm2(),
            38_220_000.0,
            "F27 states 38,220,000; the 20,000 below a square-cornered hole \
             is four 5,000 mm² corner triangles of exactly the kind F26 adds \
             to its own room"
        );
        assert_eq!(display_area_m2(rooms.faces[face].area_mm2()), "38.22");
    }
}
