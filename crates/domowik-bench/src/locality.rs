//! Invalidation-locality checks (issue 0032, BEHAVIOR-SPEC R3.4.8 / R5.1.9,
//! ED-0111).
//!
//! # The question these answer
//!
//! After an accepted batch, how much of the level was the editor ALLOWED to
//! recompute, and how much did it actually recompute? The editor's derived
//! geometry is `incremental::LevelGeometry`, so "actually recomputed" is
//! what its audit ([`LevelGeometry::set_audit`]) records for the edit: the
//! wall sources it regenerated and the rooms it rebuilt or patched, as they
//! were before the edit and as they are after it.
//!
//! # What is permitted
//!
//! Two parts, computed independently of the engine (taking the engine's own
//! dirty set as its permission would make the check circular):
//!
//! - **component** (R5.1.9): the rooms in the pre-edit and post-edit regions
//!   of the components the closure ([`locality::recomputation_closure`])
//!   admits;
//! - **routing** (ED-0111): rooms whose boundary passes through a cell whose
//!   hot status the edit flips, or contains a piece of a segment whose route
//!   changed — from the FULL builder's own hot cells and routes
//!   ([`overlay::routing_reference`]) before and after the edit.
//!
//! A room rebuilt outside both is UNJUSTIFIED. Rooms are identified by the
//! canonical points of their outer ring, within the state they belong to.
//! Counts are presentation; the check is set containment, so rebuilding the
//! wrong one of two rooms fails even when both counts are one.
//!
//! # What the numbers say
//!
//! On a normally drawn floor plan every partition meets the shell, so the
//! level is ONE join-connected component and the component permission is the
//! whole level: confinement there demonstrates nothing, and the report says
//! so. Locality is a constraint on levels with several clusters — wings, a
//! plan part-way through being drawn, islands in a courtyard — which is why
//! the corpus holds both families.
//!
//! These checks carry NO timing assertion.

use domowik_format::{Document, Point};
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::locality::{self, State};
use domowik_geometry::overlay::{routing_reference, Cell};
use domowik_geometry::polygon::Polygon;
use domowik_geometry::walls::{self, LevelSolids};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

/// What one edit was permitted to recompute, and what was recomputed.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The gesture class this describes.
    pub class: String,
    /// Join-connected components on the level after the edit.
    pub components: usize,
    /// Walls on the level after the edit.
    pub walls: usize,
    /// Rooms before and after the edit.
    pub rooms_pre: usize,
    pub rooms_post: usize,
    /// Walls the component closure admits.
    pub closure_walls: usize,
    /// Rooms the component closure admits, before / after.
    pub component_pre: usize,
    pub component_post: usize,
    /// Rooms admitted only by routing dependencies (ED-0111), before / after.
    pub routing_only_pre: usize,
    pub routing_only_post: usize,
    /// Wall sources the engine regenerated.
    pub wall_sources: usize,
    /// Rooms the engine rebuilt or patched, before / after.
    pub actual_pre: usize,
    pub actual_post: usize,
    /// Of those, rooms justified only by routing dependencies.
    pub routing_used_pre: usize,
    pub routing_used_post: usize,
    /// Work outside the permission: wall sources, rooms before / after.
    pub unjustified_walls: Vec<Uuid>,
    pub unjustified_pre: Vec<Vec<Point>>,
    pub unjustified_post: Vec<Vec<Point>>,
    /// Whether the engine's output equals a full `walls::plan` of the
    /// post-edit level.
    pub matches_full_build: bool,
}

impl Report {
    /// True when every recomputation stayed inside the permission.
    pub fn confined(&self) -> bool {
        self.unjustified_walls.is_empty()
            && self.unjustified_pre.is_empty()
            && self.unjustified_post.is_empty()
    }

    /// True when the component permission alone is the whole level, so
    /// confinement is trivial. Reported separately because a check passing
    /// for this reason has demonstrated nothing.
    pub fn closure_is_whole_level(&self) -> bool {
        self.closure_walls >= self.walls
            && self.component_pre >= self.rooms_pre
            && self.component_post >= self.rooms_post
    }
}

/// Audits one edit of `level` from `before` to `after` in the editor's mode
/// (provenance tracking on, as `SnapIndex` runs it). `touched` is the set of
/// wall ids the batch created, modified or deleted.
pub fn report(
    class: &str,
    before: &Document,
    after: &Document,
    level: Uuid,
    touched: &[Uuid],
) -> Report {
    audit(
        class,
        &walls::level_solids(before, level),
        &walls::level_solids(after, level),
        touched,
        true,
    )
}

/// [`report`] over level solids, in either engine mode.
pub fn audit(
    class: &str,
    pre: &LevelSolids,
    post: &LevelSolids,
    touched: &[Uuid],
    provenance: bool,
) -> Report {
    // Actual work: the engine at the pre-edit state, then the edit, audited.
    let mut engine = if provenance {
        LevelGeometry::with_provenance()
    } else {
        LevelGeometry::new()
    };
    engine.sync(pre);
    engine.set_audit(true);
    engine.sync(post);
    let work = engine.last_audit().expect("auditing").clone();

    let (_, rooms_pre) = walls::plan(&pre.walls, &pre.joins);
    let full_post = walls::plan(&post.walls, &post.joins);
    let matches_full_build = engine.plan() == full_post;
    let rooms_post = full_post.1;

    // Component permission, in both states.
    let components_pre = locality::components_of(&pre.walls, &pre.joins);
    let components_post = locality::components_of(&post.walls, &post.joins);
    let closure = locality::recomputation_closure(
        &State {
            walls: &pre.walls,
            ids: &pre.ids,
            joins: &pre.joins,
        },
        &State {
            walls: &post.walls,
            ids: &post.ids,
            joins: &post.joins,
        },
        touched,
    );
    let scoped =
        |components: &locality::Components, admitted: &BTreeSet<usize>, rooms: &[Polygon]| {
            let regions: Vec<Polygon> = admitted
                .iter()
                .flat_map(|&c| components.region(c).iter().cloned())
                .collect();
            locality::faces_in_scope(rooms, &regions)
                .into_iter()
                .map(|i| rooms[i].outer.clone())
                .collect::<Vec<_>>()
        };
    let component_pre = scoped(&components_pre, &closure.components_pre, &rooms_pre);
    let component_post = scoped(&components_post, &closure.components_post, &rooms_post);

    // Routing permission (ED-0111), from the full builder's own state.
    let (hot_pre, routes_pre) = routing_reference(&walls::source_rings(&pre.walls, &pre.joins));
    let (hot_post, routes_post) = routing_reference(&walls::source_rings(&post.walls, &post.joins));
    let hot_pre: BTreeSet<Cell> = hot_pre.into_iter().collect();
    let hot_post: BTreeSet<Cell> = hot_post.into_iter().collect();
    let flipped: Vec<Cell> = hot_pre.symmetric_difference(&hot_post).copied().collect();
    let (pieces_pre, pieces_post) = changed_pieces(&routes_pre, &routes_post);
    let routed = |rooms: &[Polygon], pieces: &[(Cell, Cell)]| {
        rooms
            .iter()
            .filter(|room| boundary_meets(room, &flipped, pieces))
            .map(|room| room.outer.clone())
            .collect::<Vec<_>>()
    };
    let routing_pre = routed(&rooms_pre, &pieces_pre);
    let routing_post = routed(&rooms_post, &pieces_post);

    let only = |routing: &[Vec<Point>], component: &[Vec<Point>]| {
        routing.iter().filter(|r| !component.contains(r)).count()
    };
    let unjustified = |actual: &[Vec<Point>], component: &[Vec<Point>], routing: &[Vec<Point>]| {
        actual
            .iter()
            .filter(|r| !component.contains(r) && !routing.contains(r))
            .cloned()
            .collect::<Vec<_>>()
    };
    let routing_used = |actual: &[Vec<Point>], component: &[Vec<Point>]| {
        actual.iter().filter(|r| !component.contains(r)).count()
    };
    let permitted_walls: BTreeSet<Uuid> = closure.walls.iter().chain(touched).copied().collect();

    Report {
        class: class.to_owned(),
        components: components_post.len(),
        walls: post.walls.len(),
        rooms_pre: rooms_pre.len(),
        rooms_post: rooms_post.len(),
        closure_walls: closure
            .walls
            .iter()
            .filter(|id| post.ids.contains(id))
            .count(),
        component_pre: component_pre.len(),
        component_post: component_post.len(),
        routing_only_pre: only(&routing_pre, &component_pre),
        routing_only_post: only(&routing_post, &component_post),
        wall_sources: work.wall_sources.len(),
        actual_pre: work.rooms_before.len(),
        actual_post: work.rooms_after.len(),
        routing_used_pre: routing_used(&work.rooms_before, &component_pre)
            - unjustified(&work.rooms_before, &component_pre, &routing_pre).len(),
        routing_used_post: routing_used(&work.rooms_after, &component_post)
            - unjustified(&work.rooms_after, &component_post, &routing_post).len(),
        unjustified_walls: work
            .wall_sources
            .iter()
            .filter(|id| !permitted_walls.contains(id))
            .copied()
            .collect(),
        unjustified_pre: unjustified(&work.rooms_before, &component_pre, &routing_pre),
        unjustified_post: unjustified(&work.rooms_after, &component_post, &routing_post),
        matches_full_build,
    }
}

type Routes = Vec<((Point, Point), Vec<Cell>)>;

/// Consecutive hot cells of one route: an edge piece.
type Piece = (Cell, Cell);

/// The route pieces (consecutive hot cells of one segment's route) of every
/// segment whose routes differ between the states, as they are before and
/// after. Segments are matched by their scaled endpoints; a segment present
/// in only one state has changed.
fn changed_pieces(pre: &Routes, post: &Routes) -> (Vec<Piece>, Vec<Piece>) {
    type Key = (u64, u64, u64, u64);
    let by_key = |routes: &Routes| {
        let mut map: BTreeMap<Key, Vec<Vec<Cell>>> = BTreeMap::new();
        for ((a, b), route) in routes {
            let key = (a.x.to_bits(), a.y.to_bits(), b.x.to_bits(), b.y.to_bits());
            map.entry(key).or_default().push(route.clone());
        }
        for list in map.values_mut() {
            list.sort();
        }
        map
    };
    let (pre, post) = (by_key(pre), by_key(post));
    let keys: BTreeSet<&Key> = pre.keys().chain(post.keys()).collect();
    let mut out = (Vec::new(), Vec::new());
    let pieces = |routes: Option<&Vec<Vec<Cell>>>, into: &mut Vec<(Cell, Cell)>| {
        for route in routes.into_iter().flatten() {
            into.extend(route.windows(2).map(|w| (w[0], w[1])));
        }
    };
    for key in keys {
        let (a, b) = (pre.get(key), post.get(key));
        if a != b {
            pieces(a, &mut out.0);
            pieces(b, &mut out.1);
        }
    }
    out
}

/// Whether `room`'s boundary (outer ring or a hole) passes through a cell of
/// `cells` or contains a piece of `pieces`. Exact: room vertices are cell
/// centres, and a cell's integer coordinates are its centre in scaled units.
fn boundary_meets(room: &Polygon, cells: &[Cell], pieces: &[(Cell, Cell)]) -> bool {
    let scaled = |p: &Point| ((p.x * 1024.0) as i128, (p.y * 1024.0) as i128);
    let on = |c: Cell, a: (i128, i128), b: (i128, i128)| {
        let (x, y) = (c.x as i128, c.y as i128);
        (b.0 - a.0) * (y - a.1) == (b.1 - a.1) * (x - a.0)
            && a.0.min(b.0) <= x
            && x <= a.0.max(b.0)
            && a.1.min(b.1) <= y
            && y <= a.1.max(b.1)
    };
    std::iter::once(&room.outer).chain(&room.holes).any(|ring| {
        (0..ring.len()).any(|i| {
            let (a, b) = (scaled(&ring[i]), scaled(&ring[(i + 1) % ring.len()]));
            cells.iter().any(|&c| on(c, a, b))
                || pieces.iter().any(|&(p, q)| on(p, a, b) && on(q, a, b))
        })
    })
}

/// What one edit made the DRAWN-region engine (R4.2.2, the editor's
/// occupied-only cut engine) rebuild, against what it was permitted to
/// (ED-0113 decision 4): drawn polygons whose boundary passes through a
/// cell the edit makes hot or cold or along a rerouted piece of the cut
/// sources (ED-0111, from the full builder's own routing of the drawn
/// sources), or that contain a vertex of a cut source the edit changed
/// (overlap dependency). Computed independently of the engine.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawnReport {
    pub class: String,
    /// Drawn polygons before / after the edit.
    pub polygons_pre: usize,
    pub polygons_post: usize,
    /// Drawn polygons the engine rebuilt or patched, before / after.
    pub actual_pre: usize,
    pub actual_post: usize,
    /// Rebuilt polygons outside the permission, before / after.
    pub unjustified_pre: Vec<Vec<Point>>,
    pub unjustified_post: Vec<Vec<Point>>,
    /// Whether the engine's output equals the one-shot drawn region.
    pub matches_full_build: bool,
}

impl DrawnReport {
    pub fn confined(&self) -> bool {
        self.unjustified_pre.is_empty() && self.unjustified_post.is_empty()
    }
}

/// Audits one edit of `level` on the drawn-region engine.
pub fn audit_drawn(class: &str, before: &Document, after: &Document, level: Uuid) -> DrawnReport {
    use domowik_geometry::cuts::{drawn_region, level_apertures};
    let state = |doc: &Document| {
        let solids = walls::level_solids(doc, level);
        let apertures = level_apertures(doc, &solids);
        (solids, apertures)
    };
    let (pre, pre_cuts) = state(before);
    let (post, post_cuts) = state(after);

    let mut engine = LevelGeometry::occupied_only();
    engine.sync_cut(&pre, &pre_cuts);
    engine.set_audit(true);
    engine.sync_cut(&post, &post_cuts);
    let work = engine.last_audit().expect("auditing").clone();

    let drawn_pre = drawn_region(&pre.walls, &pre.joins, &pre_cuts);
    let drawn_post = drawn_region(&post.walls, &post.joins, &post_cuts);
    let matches_full_build = engine.occupied() == drawn_post;

    let (allowed_pre, allowed_post) = drawn_permission(before, after, level);
    let outside = |actual: &[Vec<Point>], allowed: &[Vec<Point>]| {
        actual
            .iter()
            .filter(|r| !allowed.contains(r))
            .cloned()
            .collect::<Vec<_>>()
    };
    DrawnReport {
        class: class.to_owned(),
        polygons_pre: drawn_pre.len(),
        polygons_post: drawn_post.len(),
        actual_pre: work.occupied_before.len(),
        actual_post: work.occupied_after.len(),
        unjustified_pre: outside(&work.occupied_before, &allowed_pre),
        unjustified_post: outside(&work.occupied_after, &allowed_post),
        matches_full_build,
    }
}

/// The drawn polygons (outer rings, before / after) an edit of `level` is
/// permitted to rebuild — see [`DrawnReport`].
pub fn drawn_permission(
    before: &Document,
    after: &Document,
    level: Uuid,
) -> (Vec<Vec<Point>>, Vec<Vec<Point>>) {
    use domowik_geometry::cuts::{drawn_region, drawn_sources, level_apertures};
    let state = |doc: &Document| {
        let solids = walls::level_solids(doc, level);
        let apertures = level_apertures(doc, &solids);
        (solids, apertures)
    };
    let (pre, pre_cuts) = state(before);
    let (post, post_cuts) = state(after);
    let drawn_pre = drawn_region(&pre.walls, &pre.joins, &pre_cuts);
    let drawn_post = drawn_region(&post.walls, &post.joins, &post_cuts);
    let sources_pre = drawn_sources(&pre.walls, &pre.joins, &pre_cuts);
    let sources_post = drawn_sources(&post.walls, &post.joins, &post_cuts);
    // Changed source rings: those in one state but not the other (multiset).
    let changed: Vec<&Vec<Point>> = {
        let mut out = Vec::new();
        let mut post_left: Vec<&Vec<Point>> = sources_post.iter().collect();
        for ring in &sources_pre {
            match post_left.iter().position(|r| *r == ring) {
                Some(i) => {
                    post_left.swap_remove(i);
                }
                None => out.push(ring),
            }
        }
        out.extend(post_left);
        out
    };
    let (hot_pre, routes_pre) = routing_reference(&sources_pre);
    let (hot_post, routes_post) = routing_reference(&sources_post);
    let hot_pre: BTreeSet<Cell> = hot_pre.into_iter().collect();
    let hot_post: BTreeSet<Cell> = hot_post.into_iter().collect();
    let flipped: Vec<Cell> = hot_pre.symmetric_difference(&hot_post).copied().collect();
    let (pieces_pre, pieces_post) = changed_pieces(&routes_pre, &routes_post);
    let permitted = |polygons: &[Polygon], pieces: &[(Cell, Cell)]| {
        polygons
            .iter()
            .filter(|p| {
                boundary_meets(p, &flipped, pieces) || changed.iter().any(|ring| overlaps(p, ring))
            })
            .map(|p| p.outer.clone())
            .collect::<Vec<_>>()
    };
    let allowed_pre = permitted(&drawn_pre, &pieces_pre);
    let allowed_post = permitted(&drawn_post, &pieces_post);
    (allowed_pre, allowed_post)
}

/// Whether polygon `p`'s MATERIAL (outer minus holes, closed) and the
/// simple `ring`'s closed region share a point: a vertex of either inside
/// or on the other, or a proper edge crossing. Exact (orientation
/// predicate, exact point classification); symmetric, so a polygon engulfed
/// by changed material and changed material inside a polygon both count,
/// while a component sitting in another polygon's HOLE does not (Codex
/// GPT-6.1-Sol's two witnesses).
fn overlaps(p: &Polygon, ring: &[Point]) -> bool {
    use domowik_geometry::arrangement::{classify_point, Containment};
    use domowik_geometry::{orientation, Orientation};
    let in_material = |v: Point| {
        classify_point(v, &p.outer) != Containment::Outside
            && p.holes
                .iter()
                .all(|h| classify_point(v, h) != Containment::Inside)
    };
    if ring.iter().any(|&v| in_material(v)) {
        return true;
    }
    let rings = || std::iter::once(&p.outer).chain(&p.holes);
    if rings()
        .flatten()
        .any(|&v| classify_point(v, ring) != Containment::Outside)
    {
        return true;
    }
    let crosses = |a: Point, b: Point, c: Point, d: Point| {
        let (o1, o2) = (orientation(a, b, c), orientation(a, b, d));
        let (o3, o4) = (orientation(c, d, a), orientation(c, d, b));
        o1 != o2 && o3 != o4 && ![o1, o2, o3, o4].contains(&Orientation::Collinear)
    };
    rings().any(|r| {
        (0..r.len()).any(|i| {
            let (a, b) = (r[i], r[(i + 1) % r.len()]);
            (0..ring.len()).any(|j| crosses(a, b, ring[j], ring[(j + 1) % ring.len()]))
        })
    })
}
