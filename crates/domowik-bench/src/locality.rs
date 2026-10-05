//! Invalidation-locality checks (issue 0032, BEHAVIOR-SPEC R3.4.8 / R5.1.9).
//!
//! # The question these answer
//!
//! After an accepted batch, how much of the level was the editor ALLOWED to
//! recompute, and how much did it actually recompute? The first is
//! [`domowik_geometry::locality`]; the second is a property of the derived
//! geometry entry point, reported by [`engine_scope`].
//!
//! # What the numbers already say
//!
//! On a normally drawn floor plan the two are the same, and legitimately so.
//! Every partition meets the shell at a shared node, so the level is ONE
//! join-connected component and R5.1.9's closure is the whole level: there is
//! nothing smaller to confine an edit to. Locality only becomes a constraint
//! on levels that hold several disconnected clusters — separate wings, a plan
//! part-way through being drawn, islands in a courtyard. The corpus contains
//! both families for exactly this reason, and the report prints the ratio so
//! the distinction is visible rather than assumed.
//!
//! These checks carry NO timing assertion. A correctness test that fails when
//! a machine is busy teaches people to ignore it.

use domowik_format::Document;
use domowik_geometry::locality::{self, State};
use domowik_geometry::walls;
use uuid::Uuid;

/// What one gesture was permitted to recompute, and what was recomputed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// The gesture class this describes.
    pub class: String,
    /// Join-connected components on the level after the edit.
    pub components: usize,
    /// Components the R5.1.9 closure admits, over both states.
    pub closure_components: usize,
    /// Walls on the level after the edit.
    pub walls: usize,
    /// Walls inside the closure.
    pub closure_walls: usize,
    /// Room faces on the level after the edit.
    pub faces: usize,
    /// Faces the closure permits recomputing.
    pub closure_faces: usize,
    /// Walls the derived-geometry entry point actually consumed.
    pub engine_walls: usize,
    /// Faces it actually produced.
    pub engine_faces: usize,
}

impl Report {
    /// True when recomputation stayed inside the proven closure.
    pub fn confined(&self) -> bool {
        self.engine_walls <= self.closure_walls && self.engine_faces <= self.closure_faces
    }

    /// True when the closure is the whole level, so confinement is trivial.
    ///
    /// Reported separately because a check that passes for this reason has not
    /// demonstrated locality; it has demonstrated that there was none to
    /// demonstrate.
    pub fn closure_is_whole_level(&self) -> bool {
        self.closure_walls >= self.walls && self.closure_faces >= self.faces
    }
}

/// What the derived-geometry entry point recomputes for an edit on `level`.
///
/// There is exactly one such entry point — [`walls::plan`] — and it takes a
/// whole level's walls and returns a whole level's faces, so the answer is
/// always the level. This is computed from the call rather than written down
/// as a constant so that adding a SCOPED entry point changes the number here
/// instead of leaving a stale claim in a comment.
pub fn engine_scope(doc: &Document, level: Uuid) -> (usize, usize) {
    let solids = walls::level_solids(doc, level);
    let (_, faces) = walls::plan(&solids.walls, &solids.joins);
    (solids.walls.len(), faces.len())
}

/// Compares the permitted scope with the recomputed scope for one edit.
///
/// `touched` is the set of wall ids the batch created, modified or deleted.
pub fn report(
    class: &str,
    before: &Document,
    after: &Document,
    level: Uuid,
    touched: &[Uuid],
) -> Report {
    let pre = walls::level_solids(before, level);
    let post = walls::level_solids(after, level);
    let pre_state = State {
        walls: &pre.walls,
        ids: &pre.ids,
        joins: &pre.joins,
    };
    let post_state = State {
        walls: &post.walls,
        ids: &post.ids,
        joins: &post.joins,
    };

    let components = locality::components_of(&post.walls, &post.joins);
    let closure = locality::recomputation_closure(&pre_state, &post_state, touched);

    let (_, faces) = walls::plan(&post.walls, &post.joins);
    let closed_regions: Vec<_> = closure
        .components_post
        .iter()
        .flat_map(|&c| components.region(c).iter().cloned())
        .collect();
    let closure_faces = locality::faces_in_scope(&faces, &closed_regions).len();

    let (engine_walls, engine_faces) = engine_scope(after, level);

    Report {
        class: class.to_owned(),
        components: components.len(),
        closure_components: closure.components_post.len(),
        walls: post.walls.len(),
        closure_walls: closure
            .walls
            .iter()
            .filter(|id| post.ids.contains(id))
            .count(),
        faces: faces.len(),
        closure_faces,
        engine_walls,
        engine_faces,
    }
}
