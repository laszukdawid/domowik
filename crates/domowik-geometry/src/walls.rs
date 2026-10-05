//! Wall solids, join patches, occupied regions, and room faces.
//!
//! This module implements BEHAVIOR-SPEC R3.2 (derived occupied geometry) and
//! R5.1 (room topology) directly:
//!
//! - R3.2.1 — the occupied region is the union of every wall's centred
//!   butt-capped rectangle and one patch per join.
//! - R3.2.2 — at a join member endpoint, a wall contributes its cross-section:
//!   the closed segment of length `thicknessMm`, centred on the endpoint,
//!   perpendicular to that wall's own axis.
//! - R3.2.3 — the join patch is the CONVEX HULL of the member cross-sections.
//!   The specification is explicit that patches "need no miter construction
//!   and carry no miter-ratio constant", so no miter appears here.
//! - R5.1.1/R5.1.2 — a room is a bounded connected component of the complement
//!   of the occupied region, which is exactly a hole of a region component.
//!
//! Because rooms are complement components, floor area is measured to the
//! INNER faces of the walls. That is the difference between the area a person
//! can furnish and the area enclosed by the wall centre lines, and for a small
//! flat with 200 mm walls it is several square metres.

use crate::boolean::{SourceRing, SourcedPolygon};
use crate::polygon::{convex_hull, Polygon};
use crate::wall_polygon;
use domowik_format::{Document, Hinge, Opening, Point, Role, Side};
use std::collections::HashMap;
use uuid::Uuid;

/// A wall reduced to what plan geometry needs: an axis and a thickness.
///
/// Height is deliberately absent. R3.0.6: "Height affects vertical extrusion
/// only: it never influences plan contact, splitting, joining, or derived
/// occupied geometry, so walls of unequal heights join in plan."
///
/// Coordinates and thickness are millimetres.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallSolid {
    /// Axis start.
    pub start: Point,
    /// Axis end.
    pub end: Point,
    /// Full thickness; the rectangle offsets half of it to each side.
    pub thickness_mm: f64,
}

impl WallSolid {
    /// Creates a wall solid.
    pub fn new(start: Point, end: Point, thickness_mm: f64) -> Self {
        Self {
            start,
            end,
            thickness_mm,
        }
    }

    /// The centred butt-capped rectangle of R3.0.2/R3.0.3, counter-clockwise.
    ///
    /// `None` for a zero-length axis, which R3.0.5 rejects as a wall.
    pub fn rectangle(&self) -> Option<[Point; 4]> {
        wall_polygon(self.start, self.end, self.thickness_mm)
    }

    /// Axis length in millimetres.
    pub fn length_mm(&self) -> f64 {
        (self.end.x - self.start.x).hypot(self.end.y - self.start.y)
    }
}

/// Which end of a wall a join member refers to (FORMAT-SPEC `wallJoin.role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// The wall's `startMm`.
    Start,
    /// The wall's `endMm`.
    End,
}

/// One member of a join: a wall and which of its endpoints participates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JoinMember {
    /// Index into the wall slice passed alongside.
    pub wall: usize,
    /// Which endpoint of that wall.
    pub endpoint: Endpoint,
}

/// A join: at least two wall endpoints meeting at one point (R3.3.1).
///
/// Joins store no coordinates of their own; the node position is whatever the
/// member endpoints agree on (R3.3.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JoinSpec {
    /// The participating wall endpoints, order irrelevant.
    pub members: Vec<JoinMember>,
}

/// The cross-section a wall contributes at one of its endpoints (R3.2.2).
///
/// The returned pair is the two ends of a closed segment of length
/// `thickness_mm`, centred on the endpoint and perpendicular to the wall axis.
/// `None` when the axis has zero length and has no perpendicular.
pub fn cross_section(wall: &WallSolid, endpoint: Endpoint) -> Option<[Point; 2]> {
    let (at, away) = match endpoint {
        Endpoint::Start => (wall.start, wall.end),
        Endpoint::End => (wall.end, wall.start),
    };
    let (dx, dy) = (away.x - at.x, away.y - at.y);
    let len = dx.hypot(dy);
    if !len.is_finite() || len == 0.0 {
        return None;
    }
    let half = wall.thickness_mm / 2.0;
    let (nx, ny) = (-dy / len * half, dx / len * half);
    Some([
        Point::new(at.x + nx, at.y + ny),
        Point::new(at.x - nx, at.y - ny),
    ])
}

/// The patch filling a join: the convex hull of its members' cross-sections
/// (R3.2.3).
///
/// Returns the hull vertices counter-clockwise. When every contributed point
/// is collinear — a straight continuation, fixture F3 — the hull degenerates
/// to the two extremes of a closed segment; the caller unions it as a closed
/// set, and since every cross-section is centred on the shared node, that
/// segment always lies within the thickest member's own rectangle.
pub fn join_patch(walls: &[WallSolid], join: &JoinSpec) -> Vec<Point> {
    let mut points = Vec::with_capacity(join.members.len() * 2);
    for m in &join.members {
        let Some(wall) = walls.get(m.wall) else {
            continue;
        };
        if let Some(cs) = cross_section(wall, m.endpoint) {
            points.extend_from_slice(&cs);
        }
    }
    convex_hull(&points)
}

/// The occupied region of a set of walls and their joins (R3.2.1).
///
/// The result is a canonical, deterministically ordered multipolygon; it may
/// be disconnected, and components may carry holes. Walls of unequal height
/// join in plan (R3.0.6) because height is not an input here.
///
/// All walls passed in must belong to the SAME level: R3.0.6 and R5.1.1 make
/// levels non-interacting, and mixing them would fuse geometry that never
/// touches.
///
/// This computes the rooms as well and discards them — they come off the same
/// traced boundary, and separating the two paths would duplicate the splitting
/// and classification that dominate the cost. Call [`plan`] when both are
/// wanted, which is the usual case.
pub fn occupied_region(walls: &[WallSolid], joins: &[JoinSpec]) -> Vec<Polygon> {
    plan(walls, joins).0
}

/// Which input solid contributed a piece of occupied boundary.
///
/// A join names the JOIN, never one of its member walls: R3.2.3's patch is the
/// hull of every member's cross-section and exists because the members meet,
/// so attributing it to one of them would invent an ownership the geometry
/// does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BoundarySolid {
    /// Index into the `walls` slice the region was built from.
    Wall(usize),
    /// Index into the `joins` slice the region was built from.
    Join(usize),
}

/// The occupied region of R3.2.1 with the provenance of every canonical
/// boundary edge retained.
///
/// The components are the SAME canonical polygons [`occupied_region`] returns,
/// in the same order, so the R6.3.1 `(component, ring, vertex-run index)` key
/// is unchanged by asking for provenance. What is added is
/// [`SourcedPolygon::outer_runs`] / `hole_runs`: per canonical edge, the
/// pieces different solids contributed and the interior breaks between them.
///
/// This is a record of WHAT CONTRIBUTED WHAT. It decides no exclusion policy;
/// callers own that.
pub struct SourcedRegion {
    /// Canonical components, parallel to [`occupied_region`]'s output.
    pub components: Vec<SourcedPolygon>,
    wall_count: usize,
}

impl SourcedRegion {
    /// The solid a boundary tag names.
    ///
    /// Tags are assigned by [`occupied_region_with_sources`] and are only
    /// meaningful against the same `walls`/`joins` slices it was given.
    pub fn solid(&self, tag: usize) -> BoundarySolid {
        if tag < self.wall_count {
            BoundarySolid::Wall(tag)
        } else {
            BoundarySolid::Join(tag - self.wall_count)
        }
    }
}

/// [`occupied_region`], additionally recording which solid contributed each
/// piece of the resulting boundary.
///
/// Costlier than [`occupied_region`]: it transports metadata through the
/// union and does not compute the complement. Use it only where provenance is
/// needed; room geometry should still come from [`plan`].
pub fn occupied_region_with_sources(walls: &[WallSolid], joins: &[JoinSpec]) -> SourcedRegion {
    let mut rings: Vec<SourceRing> = Vec::with_capacity(walls.len() + joins.len());
    for (i, wall) in walls.iter().enumerate() {
        if let Some(rect) = wall.rectangle() {
            rings.push(SourceRing {
                points: rect.to_vec(),
                source: i,
            });
        }
    }
    for (j, join) in joins.iter().enumerate() {
        let patch = join_patch(walls, join);
        if patch.len() >= 3 {
            rings.push(SourceRing {
                points: patch,
                source: walls.len() + j,
            });
        }
    }
    SourcedRegion {
        components: crate::boolean::union_with_sources(&rings),
        wall_count: walls.len(),
    }
}

/// The occupied region and the rooms of one level, computed together.
///
/// Rooms are the bounded components of the complement of the region (R5.1.1,
/// R5.1.2), and they are read off the SAME traced boundary as the region
/// rather than recovered from it afterwards — a void enclosed by several
/// components that touch only at points belongs to none of them individually.
///
/// A wall cluster standing inside a room — an island in a courtyard — appears
/// as a hole of that room (R5.1.4), so its footprint is not counted as floor
/// area. Both lists are totally ordered by the R5.1.3 canonical comparison.
pub fn plan(walls: &[WallSolid], joins: &[JoinSpec]) -> (Vec<Polygon>, Vec<Polygon>) {
    let mut rings: Vec<Vec<Point>> = Vec::with_capacity(walls.len() + joins.len());
    for wall in walls {
        if let Some(rect) = wall.rectangle() {
            rings.push(rect.to_vec());
        }
    }
    for join in joins {
        let patch = join_patch(walls, join);
        if patch.len() >= 3 {
            rings.push(patch);
        }
    }
    crate::boolean::union_and_complement(&rings)
}

/// One level's walls and joins, with the identity each wall keeps across an
/// edit.
///
/// `ids` is parallel to `walls`. [`JoinSpec`] addresses walls POSITIONALLY, so
/// something has to translate document ids into positions; doing it once, here,
/// is what keeps the CLI, the benchmark corpus and the locality checker from
/// each inventing a different mapping.
#[derive(Debug, Clone, Default)]
pub struct LevelSolids {
    /// Wall solids in document order.
    pub walls: Vec<WallSolid>,
    /// Document ids, parallel to `walls`.
    pub ids: Vec<Uuid>,
    /// Joins of the same level, addressing `walls` positionally.
    pub joins: Vec<JoinSpec>,
}

/// Extracts one level's plan solids from a document.
///
/// Join members naming a wall that is absent, or that sits on another level,
/// are dropped rather than producing an out-of-range index: a document can be
/// mid-edit or externally supplied, and R3.3.x validation reports the dangling
/// member separately. A join left with fewer than two members still yields a
/// degenerate patch, which [`join_patch`] handles.
pub fn level_solids(doc: &Document, level_id: Uuid) -> LevelSolids {
    let mut out = LevelSolids::default();
    let mut index: HashMap<Uuid, usize> = HashMap::new();
    for wall in doc.walls_on_level(level_id) {
        index.insert(wall.id, out.walls.len());
        out.ids.push(wall.id);
        out.walls
            .push(WallSolid::new(wall.start(), wall.end(), wall.thickness_mm));
    }
    out.joins = doc
        .joins_on_level(level_id)
        .into_iter()
        .map(|j| JoinSpec {
            members: j
                .members
                .iter()
                .filter_map(|m| {
                    index.get(&m.wall_id).map(|&wall| JoinMember {
                        wall,
                        endpoint: match m.role {
                            Role::Start => Endpoint::Start,
                            Role::End => Endpoint::End,
                        },
                    })
                })
                .collect(),
        })
        .collect();
    out
}

/// The hinge point and the swung leaf tip, in PLAN millimetres.
///
/// Separated from the drawing so it can be asserted on geometrically. The
/// original test only checked that Left and Right rendered differently, which
/// passed while every `nearEnd` door swung into the wrong half-plane.
pub fn leaf_endpoints(opening: &Opening, a: Point, b: Point) -> (Point, Point) {
    // The hinge is an AXIS end, so it follows the wall's direction.
    let (hinge, free) = match opening.hinge {
        Hinge::NearStart => (a, b),
        Hinge::NearEnd => (b, a),
    };
    let (dx, dy) = (free.x - hinge.x, free.y - hinge.y);

    // The sign depends on BOTH the hinge and the opening direction, not the
    // direction alone.
    //
    // The closed leaf points from the hinge toward the other aperture end:
    // `+a` for a nearStart hinge, `-a` for nearEnd. Rotating `+a` by a
    // positive angle enters `+n` (Left); rotating `-a` by the same positive
    // angle enters `-n` (Right). A nearEnd door therefore needs the opposite
    // sign to reach the same physical half-plane, and taking the sign from
    // `opensToward` alone sent every nearEnd door the wrong way (adversarial
    // review, Codex gpt-5.6-sol, 2026-09-05).
    let toward: f64 = match opening.opens_toward {
        Side::Left => 1.0,
        Side::Right => -1.0,
    };
    let from_hinge: f64 = match opening.hinge {
        Hinge::NearStart => 1.0,
        Hinge::NearEnd => -1.0,
    };
    let theta = toward * from_hinge * opening.swing_deg.to_radians();
    let (sin, cos) = theta.sin_cos();
    (
        hinge,
        Point::new(hinge.x + dx * cos - dy * sin, hinge.y + dx * sin + dy * cos),
    )
}

/// The rooms enclosed by `walls` and `joins` on one level.
///
/// Areas are measured to the INNER faces of the walls, because that is what
/// R5.1.1 defines a room to be and what a person means by floor area.
pub fn rooms_of(walls: &[WallSolid], joins: &[JoinSpec]) -> Vec<Polygon> {
    plan(walls, joins).1
}

#[cfg(test)]
mod tests {
    use super::*;

    const P: fn(f64, f64) -> Point = Point::new;

    fn wall(x0: f64, y0: f64, x1: f64, y1: f64) -> WallSolid {
        WallSolid::new(P(x0, y0), P(x1, y1), 200.0)
    }

    /// Four walls running corner to corner around a closed rectangle.
    fn closed_loop(corners: &[(f64, f64)]) -> Vec<WallSolid> {
        (0..corners.len())
            .map(|i| {
                let (x0, y0) = corners[i];
                let (x1, y1) = corners[(i + 1) % corners.len()];
                WallSolid::new(P(x0, y0), P(x1, y1), 200.0)
            })
            .collect()
    }

    fn join(a: (usize, Endpoint), b: (usize, Endpoint)) -> JoinSpec {
        JoinSpec {
            members: vec![
                JoinMember {
                    wall: a.0,
                    endpoint: a.1,
                },
                JoinMember {
                    wall: b.0,
                    endpoint: b.1,
                },
            ],
        }
    }

    #[test]
    fn cross_section_is_perpendicular_and_thickness_long() {
        let w = wall(0.0, 0.0, 4000.0, 0.0);
        let cs = cross_section(&w, Endpoint::End).expect("non-degenerate");
        assert_eq!(cs, [P(4000.0, -100.0), P(4000.0, 100.0)]);
        let len = (cs[1].x - cs[0].x).hypot(cs[1].y - cs[0].y);
        assert_eq!(len, 200.0);
    }

    #[test]
    fn cross_section_uses_the_walls_own_axis_at_either_end() {
        let w = wall(0.0, 0.0, 0.0, 3000.0);
        assert_eq!(
            cross_section(&w, Endpoint::Start).expect("ok"),
            [P(-100.0, 0.0), P(100.0, 0.0)]
        );
    }

    #[test]
    fn cross_section_rejects_a_zero_length_axis() {
        let w = WallSolid::new(P(1.0, 1.0), P(1.0, 1.0), 200.0);
        assert!(cross_section(&w, Endpoint::Start).is_none());
    }

    // ---- BEHAVIOR-SPEC §3.5 normative fixtures ----

    #[test]
    fn fixture_f1_isolated_wall() {
        // Outline: (0,-100) (4000,-100) (4000,100) (0,100).
        let walls = [wall(0.0, 0.0, 4000.0, 0.0)];
        let region = occupied_region(&walls, &[]);
        assert_eq!(region.len(), 1, "got {region:#?}");
        assert_eq!(
            region[0].outer,
            vec![
                P(0.0, -100.0),
                P(4000.0, -100.0),
                P(4000.0, 100.0),
                P(0.0, 100.0)
            ]
        );
        assert!(
            rooms_of(&walls, &[]).is_empty(),
            "one wall encloses nothing"
        );
    }

    #[test]
    fn fixture_f2_l_join_patch_and_outline() {
        // Patch hull quad (3900,0) (4000,-100) (4100,0) (4000,100); outline
        // (0,-100) (4000,-100) (4100,0) (4100,3000) (3900,3000) (3900,100) (0,100).
        let walls = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
        ];
        let joins = [join((0, Endpoint::End), (1, Endpoint::Start))];

        let patch = join_patch(&walls, &joins[0]);
        let mut sorted = patch.clone();
        sorted.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap());
        assert_eq!(
            sorted,
            vec![
                P(3900.0, 0.0),
                P(4000.0, -100.0),
                P(4000.0, 100.0),
                P(4100.0, 0.0)
            ],
            "patch hull must be the convex hull of both cross-sections"
        );

        let region = occupied_region(&walls, &joins);
        assert_eq!(region.len(), 1, "got {region:#?}");
        assert_eq!(
            region[0].outer,
            vec![
                P(0.0, -100.0),
                P(4000.0, -100.0),
                P(4100.0, 0.0),
                P(4100.0, 3000.0),
                P(3900.0, 3000.0),
                P(3900.0, 100.0),
                P(0.0, 100.0),
            ]
        );
    }

    #[test]
    fn fixture_f3_collinear_continuation_degenerates_the_patch() {
        // Outline (0,-100) (8000,-100) (8000,100) (0,100), collinear vertices
        // eliminated; the patch is a closed segment and adds nothing.
        let walls = [wall(0.0, 0.0, 4000.0, 0.0), wall(4000.0, 0.0, 8000.0, 0.0)];
        let joins = [join((0, Endpoint::End), (1, Endpoint::Start))];

        let patch = join_patch(&walls, &joins[0]);
        assert_eq!(patch.len(), 2, "degenerate hull is a segment: {patch:?}");

        let region = occupied_region(&walls, &joins);
        assert_eq!(region.len(), 1, "got {region:#?}");
        assert_eq!(
            region[0].outer,
            vec![
                P(0.0, -100.0),
                P(8000.0, -100.0),
                P(8000.0, 100.0),
                P(0.0, 100.0)
            ]
        );
    }

    #[test]
    fn fixture_f6_acute_join_patch() {
        // W2 axis direction (-4/5, 3/5); cross-section {(2940,-80),(3060,80)}.
        // Patch = quad (3000,-100) (3060,80) (3000,100) (2940,-80).
        let walls = [
            wall(0.0, 0.0, 3000.0, 0.0),
            wall(3000.0, 0.0, 1400.0, 1200.0),
        ];
        let joins = [join((0, Endpoint::End), (1, Endpoint::Start))];
        let patch = join_patch(&walls, &joins[0]);
        let mut sorted = patch.clone();
        sorted.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap());
        let expected = [
            P(2940.0, -80.0),
            P(3000.0, -100.0),
            P(3000.0, 100.0),
            P(3060.0, 80.0),
        ];
        assert_eq!(sorted.len(), 4, "got {patch:?}");
        for (got, want) in sorted.iter().zip(expected.iter()) {
            assert!(
                (got.x - want.x).abs() < 1e-9 && (got.y - want.y).abs() < 1e-9,
                "got {sorted:?}, want {expected:?}"
            );
        }
    }

    #[test]
    fn fixture_f7_obtuse_join_patch() {
        // W2 axis direction (7/25, 24/25); cross-section {(2904,28),(3096,-28)}.
        let walls = [
            wall(0.0, 0.0, 3000.0, 0.0),
            wall(3000.0, 0.0, 3700.0, 2400.0),
        ];
        let joins = [join((0, Endpoint::End), (1, Endpoint::Start))];
        let patch = join_patch(&walls, &joins[0]);
        let mut sorted = patch.clone();
        sorted.sort_by(|a, b| (a.x, a.y).partial_cmp(&(b.x, b.y)).unwrap());
        let expected = [
            P(2904.0, 28.0),
            P(3000.0, -100.0),
            P(3000.0, 100.0),
            P(3096.0, -28.0),
        ];
        assert_eq!(sorted.len(), 4, "got {patch:?}");
        for (got, want) in sorted.iter().zip(expected.iter()) {
            assert!(
                (got.x - want.x).abs() < 1e-9 && (got.y - want.y).abs() < 1e-9,
                "got {sorted:?}, want {expected:?}"
            );
        }
    }

    #[test]
    fn fixture_f4_unequal_thickness_tee_outline() {
        // Wa split into (0,0)-(3000,0) and (3000,0)-(6000,0), both t=200;
        // Wb (3000,0)-(3000,2500) t=100. Outline per the fixture.
        let walls = [
            WallSolid::new(P(0.0, 0.0), P(3000.0, 0.0), 200.0),
            WallSolid::new(P(3000.0, 0.0), P(6000.0, 0.0), 200.0),
            WallSolid::new(P(3000.0, 0.0), P(3000.0, 2500.0), 100.0),
        ];
        let joins = [JoinSpec {
            members: vec![
                JoinMember {
                    wall: 0,
                    endpoint: Endpoint::End,
                },
                JoinMember {
                    wall: 1,
                    endpoint: Endpoint::Start,
                },
                JoinMember {
                    wall: 2,
                    endpoint: Endpoint::Start,
                },
            ],
        }];
        let region = occupied_region(&walls, &joins);
        assert_eq!(region.len(), 1, "got {region:#?}");
        assert_eq!(
            region[0].outer,
            vec![
                P(0.0, -100.0),
                P(6000.0, -100.0),
                P(6000.0, 100.0),
                P(3050.0, 100.0),
                P(3050.0, 2500.0),
                P(2950.0, 2500.0),
                P(2950.0, 100.0),
                P(0.0, 100.0),
            ]
        );
    }

    // ---- rooms ----

    #[test]
    fn four_joined_walls_enclose_one_room_measured_to_the_inner_faces() {
        // 4000 x 3000 centre lines, 200 thick: the free space is 3800 x 2800.
        let walls = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
            wall(4000.0, 3000.0, 0.0, 3000.0),
            wall(0.0, 3000.0, 0.0, 0.0),
        ];
        let joins = [
            join((0, Endpoint::End), (1, Endpoint::Start)),
            join((1, Endpoint::End), (2, Endpoint::Start)),
            join((2, Endpoint::End), (3, Endpoint::Start)),
            join((3, Endpoint::End), (0, Endpoint::Start)),
        ];
        let found = rooms_of(&walls, &joins);
        assert_eq!(found.len(), 1, "got {found:#?}");
        assert_eq!(found[0].area_mm2(), 3800.0 * 2800.0);
        assert_eq!(found[0].perimeter_mm(), 2.0 * (3800.0 + 2800.0));
        assert_eq!(
            found[0].outer,
            vec![
                P(100.0, 100.0),
                P(3900.0, 100.0),
                P(3900.0, 2900.0),
                P(100.0, 2900.0),
            ]
        );
    }

    #[test]
    fn centre_line_area_would_have_overstated_this_room() {
        // The defect this module exists to close: 12.00 m² to centre lines,
        // 10.64 m² of actual floor.
        let walls = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
            wall(4000.0, 3000.0, 0.0, 3000.0),
            wall(0.0, 3000.0, 0.0, 0.0),
        ];
        let joins = [
            join((0, Endpoint::End), (1, Endpoint::Start)),
            join((1, Endpoint::End), (2, Endpoint::Start)),
            join((2, Endpoint::End), (3, Endpoint::Start)),
            join((3, Endpoint::End), (0, Endpoint::Start)),
        ];
        let area = rooms_of(&walls, &joins)[0].area_mm2();
        assert!(
            area < 12_000_000.0,
            "must be less than the centre-line area"
        );
        assert_eq!(area, 10_640_000.0);
    }

    #[test]
    fn an_open_shape_encloses_nothing() {
        let walls = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
            wall(4000.0, 3000.0, 0.0, 3000.0),
        ];
        let joins = [
            join((0, Endpoint::End), (1, Endpoint::Start)),
            join((1, Endpoint::End), (2, Endpoint::Start)),
        ];
        assert!(rooms_of(&walls, &joins).is_empty());
    }

    #[test]
    fn a_dividing_wall_makes_two_rooms() {
        // A 6000 x 3000 shell split down the middle at x = 3000. The divider
        // meets the shell endpoint-on-interior, so R3.1.2 splits the top and
        // bottom runs; the walls below are already in that post-split state.
        let walls = [
            wall(0.0, 0.0, 3000.0, 0.0),
            wall(3000.0, 0.0, 6000.0, 0.0),
            wall(6000.0, 0.0, 6000.0, 3000.0),
            wall(6000.0, 3000.0, 3000.0, 3000.0),
            wall(3000.0, 3000.0, 0.0, 3000.0),
            wall(0.0, 3000.0, 0.0, 0.0),
            wall(3000.0, 0.0, 3000.0, 3000.0),
        ];
        let joins = [
            JoinSpec {
                members: vec![
                    JoinMember {
                        wall: 0,
                        endpoint: Endpoint::End,
                    },
                    JoinMember {
                        wall: 1,
                        endpoint: Endpoint::Start,
                    },
                    JoinMember {
                        wall: 6,
                        endpoint: Endpoint::Start,
                    },
                ],
            },
            join((1, Endpoint::End), (2, Endpoint::Start)),
            join((2, Endpoint::End), (3, Endpoint::Start)),
            JoinSpec {
                members: vec![
                    JoinMember {
                        wall: 3,
                        endpoint: Endpoint::End,
                    },
                    JoinMember {
                        wall: 4,
                        endpoint: Endpoint::Start,
                    },
                    JoinMember {
                        wall: 6,
                        endpoint: Endpoint::End,
                    },
                ],
            },
            join((4, Endpoint::End), (5, Endpoint::Start)),
            join((5, Endpoint::End), (0, Endpoint::Start)),
        ];
        let found = rooms_of(&walls, &joins);
        assert_eq!(found.len(), 2, "got {found:#?}");
        // Each half: 2800 wide (100..2900 and 3100..5900) by 2800 tall.
        assert_eq!(found[0].area_mm2(), 2800.0 * 2800.0);
        assert_eq!(found[1].area_mm2(), 2800.0 * 2800.0);
        assert_eq!(found[0].outer[0], P(100.0, 100.0));
        assert_eq!(found[1].outer[0], P(3100.0, 100.0));
    }

    #[test]
    fn an_island_inside_a_room_is_subtracted_from_its_floor_area() {
        // A closed outer shell with a free-standing closed box inside it.
        let outer = [
            wall(0.0, 0.0, 10000.0, 0.0),
            wall(10000.0, 0.0, 10000.0, 10000.0),
            wall(10000.0, 10000.0, 0.0, 10000.0),
            wall(0.0, 10000.0, 0.0, 0.0),
        ];
        let island = [
            wall(4000.0, 4000.0, 6000.0, 4000.0),
            wall(6000.0, 4000.0, 6000.0, 6000.0),
            wall(6000.0, 6000.0, 4000.0, 6000.0),
            wall(4000.0, 6000.0, 4000.0, 4000.0),
        ];
        let walls: Vec<WallSolid> = outer.iter().chain(island.iter()).copied().collect();
        let joins = [
            join((0, Endpoint::End), (1, Endpoint::Start)),
            join((1, Endpoint::End), (2, Endpoint::Start)),
            join((2, Endpoint::End), (3, Endpoint::Start)),
            join((3, Endpoint::End), (0, Endpoint::Start)),
            join((4, Endpoint::End), (5, Endpoint::Start)),
            join((5, Endpoint::End), (6, Endpoint::Start)),
            join((6, Endpoint::End), (7, Endpoint::Start)),
            join((7, Endpoint::End), (4, Endpoint::Start)),
        ];
        let found = rooms_of(&walls, &joins);
        // The ring-shaped room around the island, and the island's own inside.
        assert_eq!(found.len(), 2, "got {found:#?}");

        let big = found
            .iter()
            .max_by(|a, b| a.area_mm2().partial_cmp(&b.area_mm2()).unwrap())
            .expect("two rooms");
        assert_eq!(big.holes.len(), 1, "the island must be a hole");
        // 9800² of free space inside the shell, less the island's footprint.
        // That footprint is NOT the full 2200² bounding square: R3.2.3 fills
        // each outer corner with the convex hull of the two cross-sections,
        // which is a bevel, so a 100x100 corner square loses half its area.
        // Four corners, 5000 mm² each.
        let island_footprint = 2200.0 * 2200.0 - 4.0 * 5000.0;
        assert_eq!(big.area_mm2(), 9800.0 * 9800.0 - island_footprint);

        // The island's own inside gains the mirror-image bevel at each of its
        // four inner corners.
        let small = found
            .iter()
            .min_by(|a, b| a.area_mm2().partial_cmp(&b.area_mm2()).unwrap())
            .expect("two rooms");
        assert_eq!(small.area_mm2(), 1800.0 * 1800.0);
    }

    #[test]
    fn a_nested_island_belongs_to_the_courtyard_that_immediately_contains_it() {
        // Shell ⊃ island A ⊃ island B. Three rooms: the ring between the shell
        // and A, the ring between A and B, and B's inside. Charging B to the
        // outer room as well would subtract its footprint twice.
        let shell = closed_loop(&[
            (0.0, 0.0),
            (20000.0, 0.0),
            (20000.0, 20000.0),
            (0.0, 20000.0),
        ]);
        let a = closed_loop(&[
            (5000.0, 5000.0),
            (15000.0, 5000.0),
            (15000.0, 15000.0),
            (5000.0, 15000.0),
        ]);
        let b = closed_loop(&[
            (8000.0, 8000.0),
            (12000.0, 8000.0),
            (12000.0, 12000.0),
            (8000.0, 12000.0),
        ]);
        let walls: Vec<WallSolid> = shell.iter().chain(&a).chain(&b).copied().collect();

        let found = rooms_of(&walls, &[]);
        assert_eq!(found.len(), 3, "got {found:#?}");
        for room in &found {
            assert!(
                room.holes.len() <= 1,
                "each room has at most its immediate island: {room:#?}"
            );
        }
        // The outer ring-shaped room takes island A and NOT island B.
        let outer = found
            .iter()
            .max_by(|x, y| x.area_mm2().partial_cmp(&y.area_mm2()).unwrap())
            .expect("three rooms");
        assert_eq!(outer.holes.len(), 1);
        // Island A's outline, which starts at (4900,5000) rather than the
        // corner (4900,4900): these walls carry NO joins, so each corner keeps
        // the notch outside the two overlapping rectangles. That notch is
        // exactly what a join patch fills. Island B would start near 7900.
        assert_eq!(
            outer.holes[0][0],
            P(4900.0, 5000.0),
            "island A, not island B"
        );
    }

    #[test]
    fn a_void_enclosed_by_components_that_only_touch_at_points_is_still_a_room() {
        // R5.1.2: a room face is a bounded connected component of the free
        // space. Four wall clusters meeting only at corner points enclose one,
        // and it belongs to NO single occupied component — so a room detector
        // that reads holes off each component in turn cannot see it.
        //
        // Found by adversarial review (Codex, gpt-5.6-sol, 2026-08-31), which
        // constructed the cycle. The previous implementation returned zero
        // rooms here while correctly reporting four occupied components.
        let square = |x: f64, y: f64| {
            vec![
                P(x, y),
                P(x + 1000.0, y),
                P(x + 1000.0, y + 1000.0),
                P(x, y + 1000.0),
            ]
        };
        let rings = vec![
            square(0.0, 1000.0),
            square(1000.0, 2000.0),
            square(2000.0, 1000.0),
            square(1000.0, 0.0),
        ];
        let (occupied, free) = crate::boolean::union_and_complement(&rings);
        assert_eq!(occupied.len(), 4, "the pinches split the region");
        assert_eq!(free.len(), 1, "the enclosed void is one room: {free:#?}");
        assert_eq!(free[0].area_mm2(), 1000.0 * 1000.0);
    }

    #[test]
    fn rooms_are_the_same_whichever_way_the_walls_run() {
        // R3.2.4 / R5.1.3: derived output is invariant under wall reversal.
        let forward = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
            wall(4000.0, 3000.0, 0.0, 3000.0),
            wall(0.0, 3000.0, 0.0, 0.0),
        ];
        let fj = [
            join((0, Endpoint::End), (1, Endpoint::Start)),
            join((1, Endpoint::End), (2, Endpoint::Start)),
            join((2, Endpoint::End), (3, Endpoint::Start)),
            join((3, Endpoint::End), (0, Endpoint::Start)),
        ];
        // Same loop, every wall reversed; roles swap with the endpoints.
        let backward = [
            wall(4000.0, 0.0, 0.0, 0.0),
            wall(4000.0, 3000.0, 4000.0, 0.0),
            wall(0.0, 3000.0, 4000.0, 3000.0),
            wall(0.0, 0.0, 0.0, 3000.0),
        ];
        let bj = [
            join((0, Endpoint::Start), (1, Endpoint::End)),
            join((1, Endpoint::Start), (2, Endpoint::End)),
            join((2, Endpoint::Start), (3, Endpoint::End)),
            join((3, Endpoint::Start), (0, Endpoint::End)),
        ];
        assert_eq!(rooms_of(&forward, &fj), rooms_of(&backward, &bj));
        assert_eq!(
            occupied_region(&forward, &fj),
            occupied_region(&backward, &bj)
        );
    }

    #[test]
    fn unjoined_walls_still_enclose_a_room_where_they_overlap() {
        // Walls that cross at the corners without any join entity: the union
        // still closes, so the room exists. Joins refine corners; they are not
        // what makes a loop enclose.
        let walls = [
            wall(0.0, 0.0, 4000.0, 0.0),
            wall(4000.0, 0.0, 4000.0, 3000.0),
            wall(4000.0, 3000.0, 0.0, 3000.0),
            wall(0.0, 3000.0, 0.0, 0.0),
        ];
        let found = rooms_of(&walls, &[]);
        assert_eq!(found.len(), 1, "got {found:#?}");
        assert_eq!(found[0].area_mm2(), 3800.0 * 2800.0);
    }

    #[test]
    fn walls_of_unequal_height_are_irrelevant_to_plan_geometry() {
        // R3.0.6: height never influences derived occupied geometry. The type
        // carries no height at all, which is the enforcement — this test
        // pins the consequence: identical plan input gives identical output.
        let a = [wall(0.0, 0.0, 4000.0, 0.0)];
        let b = [WallSolid::new(P(0.0, 0.0), P(4000.0, 0.0), 200.0)];
        assert_eq!(occupied_region(&a, &[]), occupied_region(&b, &[]));
    }
}
