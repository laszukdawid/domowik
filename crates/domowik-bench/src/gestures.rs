//! The operation mix: one batch per editing gesture (issue 0032).
//!
//! # What a gesture is
//!
//! What a user does in one action, expressed as the atomic batch it commits.
//! BEHAVIOR-SPEC R3.1.1 makes the incidental effects part of the SAME batch —
//! a wall dropped onto another wall splits it, and the split is in the batch —
//! so the unit that can honestly be timed is the batch, not the field write
//! inside it.
//!
//! # Where each batch comes from
//!
//! [`Source::Product`] batches are built by the same functions the CLI runs,
//! so the number measured is the product's cost. [`Source::Harness`] batches
//! are assembled here from primitive operations because NO product command
//! builds them yet; they are what a GUI would emit, and they are marked so a
//! reader never mistakes "measured" for "shipped".
//!
//! # Targets are chosen deterministically
//!
//! Every selector is a total order over the document — first in document
//! order, greatest coordinate, most members — so the same corpus always yields
//! the same gesture. A gesture that picked a random target would produce a
//! different number on every run and could not detect a regression.

use domowik_cli::{add_wall_operations, remove_wall_operations, set_wall_operations};
use domowik_document::operation::Operation;
use domowik_format::{Document, Entity, JoinMember, Opening, OpeningKind, Point, Role, Wall};
use uuid::Uuid;

/// Who built the batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Built by the function the CLI itself calls.
    Product,
    /// Assembled here from primitive operations; no command builds it yet.
    Harness,
}

/// What the batch is expected to do when committed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The session accepts it.
    Accepted,
    /// The session rejects it atomically, leaving state unchanged.
    Rejected,
}

/// One measurable editing gesture.
pub struct Gesture {
    /// Stable class name; the row key in every report.
    pub class: &'static str,
    /// What the gesture represents, for the published table.
    pub description: &'static str,
    /// Who built the batch.
    pub source: Source,
    /// Whether the batch should commit or be refused.
    pub outcome: Outcome,
    /// The batch, in application order.
    pub operations: Vec<Operation>,
    /// Wall ids whose geometry the batch changes, for the R5.1.9 closure.
    ///
    /// A wall the batch creates or deletes counts as touched, as does the host
    /// of an opening the batch changes: an aperture is cut out of the wall's
    /// drawn region, so its geometry moves even though the wall record does
    /// not (R4.2.2).
    pub touched: Vec<Uuid>,
}

fn set_field(entity: Uuid, name: &str, value: serde_json::Value) -> Operation {
    let mut fields = serde_json::Map::new();
    fields.insert(name.to_owned(), value);
    Operation::set_fields(entity, fields)
}

/// The wall endpoint with the greatest `(x, y)`: always a corner of the plan's
/// bounding box, so a wall leaving it outward meets nothing.
fn extreme_node(doc: &Document, level: Uuid) -> Option<Point> {
    let mut best: Option<Point> = None;
    for w in doc.walls_on_level(level) {
        for p in [w.start(), w.end()] {
            best = Some(match best {
                None => p,
                Some(b) if (p.x, p.y) > (b.x, b.y) => p,
                Some(b) => b,
            });
        }
    }
    best
}

/// The horizontal wall with the greatest midpoint `y` that hosts no opening.
///
/// Its midpoint is on the plan's top edge, so a wall dropped there splits it
/// (R3.1.2 endpoint-on-interior) and reaches into empty space. Openings are
/// excluded because splitting a host is refused until R4.3 remapping exists.
fn top_edge_wall(doc: &Document, level: Uuid) -> Option<&Wall> {
    doc.walls_on_level(level)
        .filter(|w| (w.start().y - w.end().y).abs() < 1e-9)
        .filter(|w| !doc.openings.iter().any(|o| o.host_wall_id == w.id))
        .max_by(|a, b| {
            let ma = (a.start().y + a.end().y) / 2.0;
            let mb = (b.start().y + b.end().y) / 2.0;
            ma.partial_cmp(&mb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
}

/// A wall endpoint that belongs to no join: what "drag a loose end" acts on.
fn free_endpoint(doc: &Document, level: Uuid) -> Option<(Uuid, Role, Point)> {
    for w in doc.walls_on_level(level) {
        for (role, p) in [(Role::Start, w.start()), (Role::End, w.end())] {
            let member = JoinMember::new(w.id, role);
            if !doc.joins.iter().any(|j| j.contains(member)) {
                return Some((w.id, role, p));
            }
        }
    }
    None
}

/// The join with the most members, then the least id: the busiest node, which
/// is the worst case for R3.4.2's rigid node move.
fn busiest_join(doc: &Document, level: Uuid) -> Option<(Uuid, Role, Point)> {
    let join = doc
        .joins_on_level(level)
        .into_iter()
        .max_by_key(|j| (j.members.len(), std::cmp::Reverse(j.id)))?;
    let member = *join.members.first()?;
    let wall = doc.wall(member.wall_id)?;
    let p = match member.role {
        Role::Start => wall.start(),
        Role::End => wall.end(),
    };
    Some((wall.id, member.role, p))
}

/// The first wall in document order that hosts at least one opening.
fn hosting_wall(doc: &Document, level: Uuid) -> Option<&Wall> {
    doc.walls_on_level(level)
        .find(|w| doc.openings.iter().any(|o| o.host_wall_id == w.id))
}

/// The first wall in document order long enough to take a door.
fn opening_candidate(doc: &Document, level: Uuid) -> Option<&Wall> {
    doc.walls_on_level(level)
        .find(|w| w.length_mm() >= 1600.0 && !doc.openings.iter().any(|o| o.host_wall_id == w.id))
}

/// Every gesture the corpus can supply a target for, in report order.
///
/// A class with no valid target is OMITTED rather than faked. Which classes a
/// corpus omits is itself a reported fact: a shape that cannot exercise a
/// gesture is a gap in the corpus, and hiding it behind a substitute target
/// would report a number for a workload that never ran.
pub fn all(doc: &Document, level: Uuid) -> Vec<Gesture> {
    let mut out = Vec::new();
    let mut push = |g: Option<Gesture>| {
        if let Some(g) = g {
            out.push(g);
        }
    };

    push(wall_create_free(doc, level));
    push(wall_create_joined(doc, level));
    push(wall_create_split(doc, level));
    push(endpoint_drag(doc, level));
    push(joint_drag(doc, level));
    push(body_drag(doc, level));
    push(detach(doc, level));
    push(numeric_edit(doc, level));
    push(wall_delete(doc, level));
    push(opening_create(doc, level));
    push(opening_edit(doc, level));
    push(opening_delete(doc, level));
    push(opening_cascade_delete(doc, level));
    push(host_reverse(doc, level));
    push(host_shorten_reject(doc, level));
    out
}

fn new_wall(doc: &Document, level: Uuid, a: Point, b: Point) -> Wall {
    let height = doc
        .level(level)
        .map(|l| l.default_wall_height_mm)
        .unwrap_or(2500.0);
    Wall::new(level, a, b, crate::corpus::THICKNESS_MM, height)
}

/// A wall in empty space: no split, no join. The floor of the create cost.
fn wall_create_free(doc: &Document, level: Uuid) -> Option<Gesture> {
    let node = extreme_node(doc, level)?;
    let a = Point::new(node.x + 20_000.0, node.y + 20_000.0);
    let b = Point::new(a.x + 3000.0, a.y);
    let wall = new_wall(doc, level, a, b);
    let id = wall.id;
    let edit = add_wall_operations(doc, level, wall).ok()?;
    Some(Gesture {
        class: "wall-create-free",
        description: "a wall drawn in empty space; no contact to classify",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![id],
    })
}

/// A wall leaving an existing corner node: endpoint-endpoint contact, so the
/// batch also extends that join (R3.1.2 row one, R3.3.5).
fn wall_create_joined(doc: &Document, level: Uuid) -> Option<Gesture> {
    let node = extreme_node(doc, level)?;
    let b = Point::new(node.x + 3000.0, node.y + 3000.0);
    let wall = new_wall(doc, level, node, b);
    let id = wall.id;
    let edit = add_wall_operations(doc, level, wall).ok()?;
    let mut touched = vec![id];
    touched.extend(
        doc.walls_on_level(level)
            .filter(|w| w.start() == node || w.end() == node)
            .map(|w| w.id),
    );
    Some(Gesture {
        class: "wall-create-joined",
        description: "a wall drawn from an occupied node; extends that join",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched,
    })
}

/// A wall whose endpoint lands on another wall's interior: the host SPLITS in
/// the same batch (R3.1.2 row two, R3.1.1).
fn wall_create_split(doc: &Document, level: Uuid) -> Option<Gesture> {
    let host = top_edge_wall(doc, level)?;
    let host_id = host.id;
    let mid = Point::new(
        (host.start().x + host.end().x) / 2.0,
        (host.start().y + host.end().y) / 2.0,
    );
    let wall = new_wall(doc, level, mid, Point::new(mid.x, mid.y + 3000.0));
    let id = wall.id;
    let edit = add_wall_operations(doc, level, wall).ok()?;
    if edit.splits == 0 {
        return None;
    }
    Some(Gesture {
        class: "wall-create-split",
        description: "a wall landing mid-span; the host splits in the same batch",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![id, host_id],
    })
}

/// Dragging a loose end: one field write, no node to carry.
fn endpoint_drag(doc: &Document, level: Uuid) -> Option<Gesture> {
    let (wall, role, at) = free_endpoint(doc, level)?;
    let to = Point::new(at.x + 250.0, at.y + 250.0);
    let (start, end) = match role {
        Role::Start => (Some(to), None),
        Role::End => (None, Some(to)),
    };
    let edit = set_wall_operations(doc, wall, start, end, None, None).ok()?;
    if edit.operations.is_empty() {
        return None;
    }
    Some(Gesture {
        class: "endpoint-drag",
        description: "dragging an endpoint that belongs to no join",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![wall],
    })
}

/// Dragging a join node: R3.4.2 moves every member endpoint rigidly, in one
/// batch, or the members would disagree and R3.3.4 would reject the result.
fn joint_drag(doc: &Document, level: Uuid) -> Option<Gesture> {
    let (wall, role, at) = busiest_join(doc, level)?;
    let to = Point::new(at.x + 150.0, at.y + 150.0);
    let (start, end) = match role {
        Role::Start => (Some(to), None),
        Role::End => (None, Some(to)),
    };
    let edit = set_wall_operations(doc, wall, start, end, None, None).ok()?;
    if edit.operations.is_empty() {
        return None;
    }
    let touched: Vec<Uuid> = doc
        .walls_on_level(level)
        .filter(|w| w.start() == at || w.end() == at)
        .map(|w| w.id)
        .collect();
    Some(Gesture {
        class: "joint-drag",
        description: "dragging the busiest join node; every member moves with it",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched,
    })
}

/// Dragging a wall by its body: both of its nodes move, and every wall joined
/// at either stretches with its opposite endpoint fixed.
fn body_drag(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = doc.walls_on_level(level).next()?;
    let (id, s, e) = (wall.id, wall.start(), wall.end());
    let d = Point::new(120.0, -80.0);
    let edit = set_wall_operations(
        doc,
        id,
        Some(Point::new(s.x + d.x, s.y + d.y)),
        Some(Point::new(e.x + d.x, e.y + d.y)),
        None,
        None,
    )
    .ok()?;
    if edit.operations.is_empty() {
        return None;
    }
    let touched: Vec<Uuid> = doc
        .walls_on_level(level)
        .filter(|w| [w.start(), w.end()].iter().any(|p| *p == s || *p == e))
        .map(|w| w.id)
        .collect();
    Some(Gesture {
        class: "body-drag",
        description: "translating a wall; both attached nodes travel with it",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched,
    })
}

/// Detaching a wall from a node: the membership goes, then the endpoint moves.
///
/// Harness-built. No CLI command detaches, so this is the batch a GUI would
/// emit — a join edit and a field write, atomic together, because a wall left
/// listed in a join it no longer meets fails R3.3.4 on the next command.
fn detach(doc: &Document, level: Uuid) -> Option<Gesture> {
    let join = doc
        .joins_on_level(level)
        .into_iter()
        .find(|j| j.members.len() >= 3)?;
    let member = *join.members.first()?;
    let wall = doc.wall(member.wall_id)?;
    let at = match member.role {
        Role::Start => wall.start(),
        Role::End => wall.end(),
    };
    let remaining: Vec<JoinMember> = join
        .members
        .iter()
        .filter(|m| **m != member)
        .copied()
        .collect();

    let mut operations = vec![set_field(
        join.id,
        "members",
        serde_json::to_value(&remaining).ok()?,
    )];
    let field = match member.role {
        Role::Start => "startMm",
        Role::End => "endMm",
    };
    operations.push(set_field(
        wall.id,
        field,
        serde_json::json!([at.x + 400.0, at.y + 400.0]),
    ));
    Some(Gesture {
        class: "detach",
        description: "removing a wall from a node and moving it clear",
        source: Source::Harness,
        outcome: Outcome::Accepted,
        operations,
        touched: vec![wall.id],
    })
}

/// A pure numeric edit: thickness only, no topology change.
fn numeric_edit(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = doc.walls_on_level(level).next()?;
    let edit = set_wall_operations(doc, wall.id, None, None, Some(150.0), None).ok()?;
    if edit.operations.is_empty() {
        return None;
    }
    Some(Gesture {
        class: "numeric-edit",
        description: "changing a wall's thickness; geometry changes, topology does not",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![wall.id],
    })
}

/// Deleting a wall that hosts nothing: memberships drop, thin joins go.
fn wall_delete(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = doc
        .walls_on_level(level)
        .find(|w| !doc.openings.iter().any(|o| o.host_wall_id == w.id))?;
    let edit = remove_wall_operations(doc, wall.id);
    Some(Gesture {
        class: "wall-delete",
        description: "deleting a wall; join memberships drop with it",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![wall.id],
    })
}

/// Deleting a wall that hosts openings: R3.1.8 cascades them in the same batch.
fn opening_cascade_delete(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = hosting_wall(doc, level)?;
    let edit = remove_wall_operations(doc, wall.id);
    if edit.cascaded_openings == 0 {
        return None;
    }
    Some(Gesture {
        class: "opening-cascade-delete",
        description: "deleting a host; its openings cascade in the same batch",
        source: Source::Product,
        outcome: Outcome::Accepted,
        operations: edit.operations,
        touched: vec![wall.id],
    })
}

fn opening_create(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = opening_candidate(doc, level)?;
    let opening = Opening::door(wall.id, 0.5, 900.0, 2100.0, wall.thickness_mm);
    Some(Gesture {
        class: "opening-create",
        description: "adding a door; the host's drawn region gains an aperture",
        source: Source::Harness,
        outcome: Outcome::Accepted,
        operations: vec![Operation::create(Entity::Opening(opening))],
        touched: vec![wall.id],
    })
}

fn opening_edit(doc: &Document, level: Uuid) -> Option<Gesture> {
    let opening = doc.openings.iter().find(|o| {
        doc.wall(o.host_wall_id)
            .is_some_and(|w| w.level_id == level)
    })?;
    let host = opening.host_wall_id;
    Some(Gesture {
        class: "opening-edit",
        description: "sliding an opening along its host; R4.2.2 recuts the aperture",
        source: Source::Harness,
        outcome: Outcome::Accepted,
        operations: vec![set_field(opening.id, "u", serde_json::json!(0.4))],
        touched: vec![host],
    })
}

fn opening_delete(doc: &Document, level: Uuid) -> Option<Gesture> {
    let opening = doc.openings.iter().find(|o| {
        doc.wall(o.host_wall_id)
            .is_some_and(|w| w.level_id == level)
    })?;
    Some(Gesture {
        class: "opening-delete",
        description: "removing an opening; the host's region closes up",
        source: Source::Harness,
        outcome: Outcome::Accepted,
        operations: vec![Operation::delete(opening.id)],
        touched: vec![opening.host_wall_id],
    })
}

/// Reversing a host wall. R4.3.1 requires `u` to map to `1 - u` in the SAME
/// batch, or the opening silently moves to the other end of the wall.
///
/// Harness-built, and worth reading twice: NO product command reverses a wall,
/// so the field map this batch performs is unimplemented behaviour that the
/// corpus states rather than measures away.
fn host_reverse(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = hosting_wall(doc, level)?;
    let (s, e) = (wall.start(), wall.end());
    let mut operations = vec![
        set_field(wall.id, "startMm", serde_json::json!([e.x, e.y])),
        set_field(wall.id, "endMm", serde_json::json!([s.x, s.y])),
    ];
    for o in doc.openings.iter().filter(|o| o.host_wall_id == wall.id) {
        operations.push(set_field(o.id, "u", serde_json::json!(1.0 - o.u)));
    }
    Some(Gesture {
        class: "host-reverse",
        description: "reversing a host wall with the R4.3.1 field map applied",
        source: Source::Harness,
        outcome: Outcome::Accepted,
        operations,
        touched: vec![wall.id],
    })
}

/// Shortening a host until its opening no longer fits. R4.3.2 revalidates and
/// the whole batch must be REFUSED, leaving the document unchanged.
fn host_shorten_reject(doc: &Document, level: Uuid) -> Option<Gesture> {
    let wall = hosting_wall(doc, level)?;
    let s = wall.start();
    let e = wall.end();
    let widest = doc
        .openings
        .iter()
        .filter(|o| o.host_wall_id == wall.id)
        .map(|o| o.width_mm)
        .fold(0.0_f64, f64::max);
    // A wall shorter than its widest opening cannot host it at any `u`.
    let len = wall.length_mm();
    if len <= 0.0 {
        return None;
    }
    let t = (widest / 2.0) / len;
    let to = Point::new(s.x + (e.x - s.x) * t, s.y + (e.y - s.y) * t);
    let edit = set_wall_operations(doc, wall.id, None, Some(to), None, None).ok()?;
    if edit.operations.is_empty() {
        return None;
    }
    Some(Gesture {
        class: "host-shorten-reject",
        description: "shortening a host below its opening's width; refused atomically",
        source: Source::Product,
        outcome: Outcome::Rejected,
        operations: edit.operations,
        touched: vec![wall.id],
    })
}

/// The `OpeningKind` a corpus opening carries, exposed so a report can say
/// what it measured rather than assuming doors.
pub fn kinds(doc: &Document) -> (usize, usize) {
    let doors = doc
        .openings
        .iter()
        .filter(|o| o.kind == OpeningKind::Door)
        .count();
    (doors, doc.openings.len() - doors)
}
