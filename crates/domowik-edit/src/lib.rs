//! The wall-edit batch algebra: what one editing gesture commits.
//!
//! # Why this is its own crate
//!
//! It lived in `domowik-cli`, where it was reachable only by the command
//! line. The GUI then needed the same gestures and, having no way to call
//! them, began assembling batches by hand — which meant a wall drawn in the
//! GUI got no auto-join (R3.1.2), no induced split (R3.1.1), and no
//! remap-or-reject for the openings on the wall it crossed (R4.3.3). Two front
//! ends with two different ideas of what "draw a wall" means is not a
//! difference in presentation; it is a difference in the document.
//!
//! Nothing here touches files or the terminal. A gesture takes a `Document`
//! and returns the operations that perform it, so it can be tested, measured
//! and replayed without either.
//!
//! # What a gesture owes
//!
//! BEHAVIOR-SPEC R3.1.1 makes the incidental effects part of the SAME batch as
//! the edit that caused them: the split a new wall induces, the join it lands
//! on, the openings that must be remapped. That is why these return whole
//! batches rather than single operations, and why several of them can refuse:
//! R4.3.3 rejects the entire edit — the wall creation included — when a hosted
//! opening has no child that strictly contains it.

use domowik_document::operation::{Operation, OperationKind};
use domowik_document::wall_contact::{
    axis_contact_refusals, wall_contact_refusals, WallContactBlock,
};
use domowik_format::{
    topology_cell, Document, Entity, Hinge, JoinMember, Point, Role, RoomRecord, Side, Wall,
    WallJoin, COORD_BOUND_MM, DIMENSION_CAP_MM,
};
use domowik_geometry::wall_contact::{axis_contact, crossing_point, AxisContact, AxisRole};
use uuid::Uuid;

/// Why an edit could not be built.
#[derive(Debug, thiserror::Error)]
pub enum EditError {
    /// §3 F10: a commanded coordinate is not finite (`nonfinite-coordinate`).
    #[error("nonfinite-coordinate: wall {wall} would get a non-finite endpoint")]
    NonFiniteCoordinate { wall: Uuid },
    /// R3.3.7: the named join does not hold the named member.
    #[error("join {join} has no member ({wall}, {role:?})")]
    NotAJoinMember { join: Uuid, wall: Uuid, role: Role },
    /// R4.3.3: a hosted opening has no child that strictly contains it, so the
    /// whole batch — including whatever caused the split — is refused.
    #[error("{0}")]
    OpeningSplitAmbiguous(String),
    /// The command names a level this document does not hold.
    #[error("no level {0} in this document")]
    UnknownLevel(Uuid),
    /// A level field value outside its BEHAVIOR-SPEC §2.1 constraint. The
    /// whole command is refused; no partial operation is emitted.
    #[error("level {level}: {field} is {value}, outside its permitted range")]
    InvalidLevelField {
        level: Uuid,
        field: &'static str,
        value: f64,
    },
    /// R3.1.2, and the "Wall-contact admission" compatibility rule: the
    /// batch's final same-level wall axes contain a refusing contact — full
    /// coincidence or a positive-length collinear overlap — against another
    /// final axis. Judged over every wall the batch affected (created,
    /// moved, split, or co-moved via R3.4.2), not only the one the command
    /// named. The whole batch, including whatever incidental split or join
    /// it induced, is refused; nothing here is partially applied.
    #[error("{}", .0.iter().map(WallContactBlock::to_string).collect::<Vec<_>>().join("; "))]
    WallContact(Vec<WallContactBlock>),
    /// R3.1.2: a split plan would produce a child with `startMm == endMm`.
    /// Checked before R4.3.3's opening remap below, so a plan that violates
    /// both reports this refusal rather than a remap failure it would never
    /// have reached had the split been well-formed.
    #[error(
        "zero-length-split-child: splitting wall {wall} at ({x}, {y}) would produce a \
         wall with startMm == endMm (BEHAVIOR-SPEC R3.1.2); the whole edit is refused"
    )]
    ZeroLengthSplitChild { wall: Uuid, x: f64, y: f64 },

    /// R5.2.2: an update or delete named a record id the document does not
    /// hold. Rejected deterministically with an empty batch.
    #[error("no room record {0} in this document")]
    UnknownRoom(Uuid),

    /// R5.2.2: a CREATE supplied an identifier already in use anywhere in the
    /// document. No automatic id assignment ever occurs for rooms, so the
    /// collision is the caller's to resolve; the command rejects with an empty
    /// batch, no state change and no undo entry (the §2.7 pattern).
    #[error(
        "supplied-id-in-use: {id} already identifies a {kind} in this document; \
         room records take a command-supplied identifier and none is ever \
         assigned automatically (BEHAVIOR-SPEC R5.2.2)"
    )]
    RoomIdInUse { id: Uuid, kind: &'static str },

    /// R5.2.2's `nonfinite-coordinate`: the seed is not a number.
    ///
    /// Seed LOCATION never rejects — in a wall, on a boundary or in the
    /// exterior are all legal (R5.2.4). Only a seed that is not a coordinate
    /// at all does.
    #[error("nonfinite-coordinate: room record seed ({x}, {y}) must be finite")]
    RoomSeedNotFinite { x: f64, y: f64 },

    /// FORMAT-SPEC §2.3's magnitude bound on `seedMm`.
    ///
    /// Split from the above because they are different defects: a seed of
    /// 2 x 10⁷ mm is perfectly finite, and telling its author it is "not
    /// finite" points them at the wrong thing to change.
    #[error(
        "coordinate-out-of-range: room record seed ({x}, {y}) must have \
         components within {bound} mm"
    )]
    RoomSeedOutOfRange { x: f64, y: f64, bound: f64 },

    /// R5.2.2/R5.3.4's `invalid-style-reference`: the empty string is not a
    /// token. Clearing a style reference uses null, never `""`.
    #[error(
        "invalid-style-reference: {field} must be a non-empty opaque token; \
         clear it with null instead of an empty string"
    )]
    InvalidStyleReference { field: &'static str },

    /// A whole-list level reorder (issue 0115) was not exactly the list of
    /// the current levels: a missing, extra or duplicated id, or the wrong
    /// length. The command is refused whole, before any effect.
    #[error("list-order: {0}")]
    LevelListOrder(String),
}

/// The result of building one edit.
pub type Result<T> = std::result::Result<T, EditError>;

/// The level fields one command may change (BEHAVIOR-SPEC §2.1).
///
/// Every field is optional and `None` means LEAVE ALONE, which is why this is
/// a struct of options rather than a partial `Level`: a caller editing the
/// name must not be forced to restate an elevation it never looked at, and a
/// builder that carried whole entities could not tell "unchanged" from
/// "reset to what I happened to read earlier".
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LevelFields {
    pub name: Option<String>,
    pub elevation_mm: Option<f64>,
    pub floor_thickness_mm: Option<f64>,
    pub default_wall_height_mm: Option<f64>,
    pub default_ceiling_height_mm: Option<f64>,
}

/// Operations changing `level_id`'s authoritative fields, as ONE `setFields`.
///
/// # What it returns, and what an empty result means
///
/// At most one operation. A field whose requested value equals what the level
/// already holds is omitted, so a command that asks for nothing new returns an
/// EMPTY vector rather than an operation with an empty payload — an empty
/// batch is a framing reject, and a no-op that grew the log, moved the undo
/// stack and marked the document dirty would be worse than nothing. Callers
/// must treat `Ok(vec![])` as "already in that state", not as failure.
///
/// # Validation scope
///
/// Field constraints only, on the values supplied, against the level named:
/// finite values, `floorThicknessMm` in `[0, cap]`, both heights in
/// `(0, cap]`, and an elevation within the coordinate bound. It does NOT
/// validate the whole document and does not clone it — one level is read by
/// reference and nothing else is touched. Cross-entity consequences are not
/// this function's: §2.1's slab and ceiling advisories are WARNINGS reported
/// by [`domowik_format::Document::warnings`] and never block a command, and
/// admission — including R2.2/R2.3 level deletion — remains the Session's.
///
/// An unknown `level_id`, or a value outside its field constraint, is
/// rejected whole: no partial operation is emitted.
pub fn set_level_operations(
    doc: &Document,
    level_id: Uuid,
    fields: &LevelFields,
) -> Result<Vec<Operation>> {
    let Some(level) = doc.level(level_id) else {
        return Err(EditError::UnknownLevel(level_id));
    };

    let mut changed = serde_json::Map::new();
    if let Some(name) = &fields.name {
        if name != &level.name {
            changed.insert("name".to_owned(), serde_json::json!(name));
        }
    }
    let numeric: [(&str, Option<f64>, f64, LevelRange); 4] = [
        (
            "elevationMm",
            fields.elevation_mm,
            level.elevation_mm,
            LevelRange::Coordinate,
        ),
        (
            "floorThicknessMm",
            fields.floor_thickness_mm,
            level.floor_thickness_mm,
            LevelRange::NonNegative,
        ),
        (
            "defaultWallHeightMm",
            fields.default_wall_height_mm,
            level.default_wall_height_mm,
            LevelRange::Positive,
        ),
        (
            "defaultCeilingHeightMm",
            fields.default_ceiling_height_mm,
            level.default_ceiling_height_mm,
            LevelRange::Positive,
        ),
    ];
    for (name, requested, current, range) in numeric {
        let Some(value) = requested else { continue };
        if !range.permits(value) {
            return Err(EditError::InvalidLevelField {
                level: level_id,
                field: name,
                value,
            });
        }
        // NUMERIC equality, not bit equality: FORMAT-SPEC's numeric policy
        // maps −0 to +0, so a signed zero is not a change and must not emit an
        // operation. An emitted zero is normalized to +0 for the same reason —
        // writing −0 would store a distinction the format does not carry.
        if value != current {
            let value = if value == 0.0 { 0.0 } else { value };
            changed.insert(name.to_owned(), serde_json::json!(value));
        }
    }

    if changed.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Operation::set_fields(level_id, changed)])
}

/// Operations placing every current level in the supplied list order (BEHAVIOR‑
/// SPEC §2.2, issue 0115).
///
/// The order is WHOLE-LIST: `order` must be a permutation of the document's
/// current level ids, and every reordered position becomes ONE per-level
/// `setFields` operation carrying the ascending `displayOrder` ordinal the
/// document-wide rule assigns (position 0 in the list is ordinal 0). A level
/// already holding its ordinal contributes nothing, so a duplicate with no
/// effect yields an EMPTY vec — the caller treats that as "already in this
/// order", not as failure, exactly as [`set_level_operations`] documents.
///
/// # What is NOT here
///
/// Only ordinals move. Elevation, paint tiers, the 3-D scene, diagnostics and
/// default targeting remain elevation-derived (§2.2's non-consumer list); this
/// builder never reorders levels in `doc.levels`, emits a `create`/`delete`
/// pair, or touches any other field.
///
/// # Validation scope
///
/// The list must be exactly the document's current levels, each once. An
/// empty document has no levels and is refused implicitly because an empty
/// `order` would be an empty batch — a caller plotting to reorder "nothing"
/// has already lost the batch; a full-level document is the only kind this
/// accepts. Values themselves are positions here, so the format's ordinal
/// bound can never trigger through this builder.
pub fn reorder_levels_operations(doc: &Document, order: &[Uuid]) -> Result<Vec<Operation>> {
    if doc.levels.is_empty() {
        return Err(EditError::LevelListOrder(
            "the document has no levels to list".to_owned(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for id in order {
        if doc.level(*id).is_none() {
            return Err(EditError::LevelListOrder(format!(
                "the list names {id} before any level it holds"
            )));
        }
        if !seen.insert(*id) {
            return Err(EditError::LevelListOrder(
                "the list names a level more than once".to_owned(),
            ));
        }
    }
    if order.len() != doc.levels.len() {
        return Err(EditError::LevelListOrder(format!(
            "the document has {} level(s), the list has {}",
            doc.levels.len(),
            order.len()
        )));
    }

    let mut ops = Vec::new();
    for (ordinal, id) in order.iter().enumerate() {
        let ordinal = ordinal as u64;
        if doc.level(*id).map(|l| l.display_order) == Some(Some(ordinal)) {
            continue;
        }
        let mut fields = serde_json::Map::new();
        fields.insert("displayOrder".to_owned(), serde_json::json!(ordinal));
        ops.push(Operation::set_fields(*id, fields));
    }
    Ok(ops)
}

/// Which constraint a level field must satisfy (BEHAVIOR-SPEC §2.1).
///
/// Three ranges, spelled out because collapsing them is what rejects a legal
/// zero-thickness slab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LevelRange {
    /// Finite, `|v| <= COORD_BOUND_MM`, sign free.
    Coordinate,
    /// Finite, in `[0, DIMENSION_CAP_MM]` — zero is legal.
    NonNegative,
    /// Finite, in `(0, DIMENSION_CAP_MM]`.
    Positive,
}

impl LevelRange {
    fn permits(self, v: f64) -> bool {
        v.is_finite()
            && match self {
                Self::Coordinate => v.abs() <= COORD_BOUND_MM,
                Self::NonNegative => (0.0..=DIMENSION_CAP_MM).contains(&v),
                Self::Positive => v > 0.0 && v <= DIMENSION_CAP_MM,
            }
    }
}

/// The height a wall on `level_id` takes (BEHAVIOR-SPEC §2.1, R3.0.6).
///
/// `requested` is what the command asked for. `Some(h)` is an EXPLICIT height
/// and is preserved verbatim once it satisfies its field constraint; `None`
/// resolves from the level's `defaultWallHeightMm` AT THIS MOMENT.
///
/// Resolution happens once, here, and the result is stored on the wall. That
/// is the whole point of §2.1's "level-owned resolution rule": a wall does not
/// inherit from its level, it copies at creation, so changing a level default
/// later never rewrites walls that already exist.
pub fn resolve_wall_height(doc: &Document, level_id: Uuid, requested: Option<f64>) -> Result<f64> {
    let Some(level) = doc.level(level_id) else {
        return Err(EditError::UnknownLevel(level_id));
    };
    match requested {
        Some(height) if LevelRange::Positive.permits(height) => Ok(height),
        Some(height) => Err(EditError::InvalidLevelField {
            level: level_id,
            field: "heightMm",
            value: height,
        }),
        None if LevelRange::Positive.permits(level.default_wall_height_mm) => {
            Ok(level.default_wall_height_mm)
        }
        None => Err(EditError::InvalidLevelField {
            level: level_id,
            field: "defaultWallHeightMm",
            value: level.default_wall_height_mm,
        }),
    }
}

/// Re-noding attempts after the first resolution. Each adds the contacts the
/// previous result still held — exposed by a split's rounding, or left by
/// stored history on a wall the gesture split — and redoes the splits. One
/// suffices for nearly every gesture that needs any; past the cap the final
/// guard refuses.
const MAX_RENODING: usize = 8;

/// The topology quantum (ED-0110): 2⁻¹⁰ mm. Two points are the same point
/// exactly when they share a [`topology_cell`] (ED-0103 N-2); this builder
/// decides contact points and join membership by that identity, and keeps the
/// distance only for "is there a join near here" queries.
pub const JOIN_COINCIDENCE_MM: f64 = domowik_format::JOIN_COINCIDENCE_MM;

/// Every join member whose endpoint sits at `point` on `level_id`.
///
/// Empty when no join occupies that point. R3.4.2 needs the whole membership,
/// not just the wall being edited, because the node moves rigidly.
/// The join holding wall `wall_id`'s endpoint `role`, by MEMBERSHIP — not by
/// where the endpoint lies. After an explicit detach (R3.3.7) an endpoint
/// can still coincide with a join it no longer belongs to; dragging it must
/// neither move that join nor leave the endpoint behind.
pub fn member_join(doc: &Document, wall_id: Uuid, role: Role) -> Option<&WallJoin> {
    doc.joins.iter().find(|j| {
        j.members
            .iter()
            .any(|m| m.wall_id == wall_id && m.role == role)
    })
}

/// Builds the batch detaching one member from a join (R3.3.7): the member
/// is removed, and a join left with fewer than two members is deleted.
/// Endpoints are untouched and no contact resolution runs — the explicit
/// command must not immediately re-join what it detached (implicit
/// re-joining by coincidence is what R3.3.7 forbids in reverse).
pub fn detach_operations(
    doc: &Document,
    join_id: Uuid,
    wall_id: Uuid,
    role: Role,
) -> Result<WallEdit> {
    let refused = || EditError::NotAJoinMember {
        join: join_id,
        wall: wall_id,
        role,
    };
    let join = doc
        .joins
        .iter()
        .find(|j| j.id == join_id)
        .ok_or_else(refused)?;
    if !join
        .members
        .iter()
        .any(|m| m.wall_id == wall_id && m.role == role)
    {
        return Err(refused());
    }
    let remaining: Vec<JoinMember> = join
        .members
        .iter()
        .filter(|m| !(m.wall_id == wall_id && m.role == role))
        .copied()
        .collect();
    let operations = if remaining.len() < 2 {
        vec![Operation::delete(join_id)]
    } else {
        vec![set_field(
            join_id,
            "members",
            serde_json::to_value(&remaining).expect("members serialise"),
        )]
    };
    Ok(WallEdit {
        operations,
        joins: 1,
        ..WallEdit::default()
    })
}

pub fn node_members(doc: &Document, level_id: Uuid, point: Point) -> Vec<JoinMember> {
    doc.join_at(level_id, point)
        .map(|j| j.members.clone())
        .unwrap_or_default()
}

/// Applies an operation to a scratch document, so later operations in the same
/// batch see the earlier ones.
///
/// FORMAT-SPEC B4 makes a batch's operations apply in array order, so contact
/// classification for a later operation must see what the earlier ones did.
/// Classifying every operation against the pre-batch state would place a join
/// at a corner that the edit had already moved away from.
pub fn apply_preview(doc: &mut Document, op: &Operation) {
    match &op.kind {
        OperationKind::CreateEntity { entity } => doc.insert_entity(entity.clone()),
        OperationKind::DeleteEntity => {
            doc.remove_entity(op.entity_id);
        }
        OperationKind::SetFields { fields } => {
            if let Some(current) = doc.entity(op.entity_id) {
                if let Ok(mut value) = serde_json::to_value(&current) {
                    if let Some(object) = value.as_object_mut() {
                        for (k, v) in fields {
                            object.insert(k.clone(), v.clone());
                        }
                    }
                    if let Ok(updated) = serde_json::from_value(value) {
                        doc.insert_entity(updated);
                    }
                }
            }
        }
    }
}

/// A point at which an existing wall must be split, and what meets it there.
#[derive(Debug, Clone, Copy)]
pub struct SplitPoint {
    /// The wall to split.
    pub wall: Uuid,
    /// Where, in plan millimetres.
    pub at: Point,
}

/// Classifies the contact a new or moved wall makes at its FINAL positions
/// (BEHAVIOR-SPEC R3.1.2), returning the splits it induces.
///
/// Three of R3.1.2's rows are handled here:
///
/// - **Endpoint-on-interior** — the new wall's endpoint lands on another
///   wall's interior: split that wall at the point.
/// - **Proper crossing** — the two axes cross at interior points of both:
///   split BOTH walls there, at the exactly constructed, correctly rounded
///   [`crossing_point`].
/// - **Endpoint-endpoint** — handled by [`join_operation`], which needs no
///   split.
///
/// # Contact points are topology points
///
/// Classification is exact, but a contact POINT is identified on the
/// topology grid, the one identity relation for points (ED-0103 N-2,
/// ED-0110, ED-0119). A contact point that shares a cell with an endpoint of a
/// wall IS that endpoint, so that wall is not split there:
///
/// - an endpoint on another wall's interior, in the cell of one of that
///   wall's endpoints, is an endpoint-endpoint contact;
/// - a crossing in the cell of one wall's endpoint splits only the OTHER
///   wall — the wall passes through that end — at the crossing itself, so
///   its children stay on its axis; the end and the new children's ends then
///   share a cell and join;
/// - a crossing in the cell of an endpoint of each is a corner: no split.
///
/// Without this a wall drawn through an existing crossing, or ending a
/// fraction of a micrometre from a corner, split a neighbour into a child
/// whose ends share a cell, and the batch was refused for a contact the user
/// cannot see.
///
/// Full axis coincidence and positive-length collinear overlap are NOT
/// handled here: both are R3.1.2 REJECT classes, resolved by
/// [`domowik_document::wall_contact::wall_contact_refusals`] ahead of any
/// call to this function, never by manufacturing a split point.
///
/// Deterministic and duplicate-free: at most one split point per wall per
/// cell, preferring a stored endpoint coordinate over a constructed one.
pub fn splits_induced(
    doc: &Document,
    level_id: Uuid,
    new_wall: Uuid,
    start: Point,
    end: Point,
) -> Vec<SplitPoint> {
    let mut candidates = split_candidates(doc, level_id, new_wall, start, end);
    dedup_by_cell(&mut candidates);
    candidates.into_iter().map(|c| c.split).collect()
}

/// A split point, and whether its coordinate was CONSTRUCTED (a rounded
/// crossing) rather than read from a stored endpoint.
#[derive(Debug, Clone, Copy)]
struct Candidate {
    split: SplitPoint,
    constructed: bool,
}

/// The stored endpoint of `s→e` that shares `p`'s topology cell, if any.
fn endpoint_in_cell(s: Point, e: Point, p: Point) -> Option<Point> {
    let cell = topology_cell(p);
    if topology_cell(s) == cell {
        Some(s)
    } else if topology_cell(e) == cell {
        Some(e)
    } else {
        None
    }
}

/// [`splits_induced`] before deduplication.
fn split_candidates(
    doc: &Document,
    level_id: Uuid,
    new_wall: Uuid,
    start: Point,
    end: Point,
) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    for other in doc.walls_on_level(level_id) {
        if other.id != new_wall {
            pair_candidates(
                (new_wall, start, end),
                (other.id, other.start(), other.end()),
                &mut out,
            );
        }
    }
    out
}

/// The split points one pair's contact induces, by R3.1.2 and the
/// topology-point rule of [`splits_induced`].
fn pair_candidates(
    (new_wall, start, end): (Uuid, Point, Point),
    (other, os, oe): (Uuid, Point, Point),
    out: &mut Vec<Candidate>,
) {
    let stored = |wall, at| Candidate {
        split: SplitPoint { wall, at },
        constructed: false,
    };
    // Classification, not construction: `axis_contact` decides the class by
    // exact sign tests alone, so a collinear touch (full coincidence or a
    // positive-length overlap) can never be mistaken for a genuine
    // transversal endpoint-on-interior split — that would otherwise
    // manufacture a split point for a pair R3.1.2 rejects outright.
    match axis_contact(start, end, os, oe) {
        // One of OUR endpoints lands strictly inside the other wall's axis,
        // and the axes are not collinear: split the OTHER wall — unless the
        // point is one of its endpoints on the grid.
        Some(AxisContact::EndpointOnInterior {
            split: AxisRole::Second,
            at,
        }) => {
            if endpoint_in_cell(os, oe, at).is_none() {
                out.push(stored(other, at));
            }
        }
        // The mirror case: one of THEIR endpoints lands strictly inside ours.
        // The new wall is split too — R3.1.2 does not privilege the wall
        // being drawn.
        Some(AxisContact::EndpointOnInterior {
            split: AxisRole::First,
            at,
        }) => {
            if endpoint_in_cell(start, end, at).is_none() {
                out.push(stored(new_wall, at));
            }
        }
        // Proper crossing: interior x interior. Split BOTH at the exact,
        // correctly rounded crossing — except a wall with an end in the
        // crossing's cell: that end IS the crossing, so that wall is not
        // split, and the other wall's new ends join it by cell. The split
        // point stays the crossing, never the nearby end's coordinate, so
        // children stay on their parent's axis (R4.3.3 rehosting checks
        // exactly that).
        Some(AxisContact::ProperCrossing) => {
            let Some(at) = crossing_point(start, end, os, oe) else {
                return;
            };
            for (wall, s, e) in [(new_wall, start, end), (other, os, oe)] {
                if endpoint_in_cell(s, e, at).is_none() {
                    out.push(Candidate {
                        split: SplitPoint { wall, at },
                        constructed: true,
                    });
                }
            }
        }
        // Coincidence and collinear overlap are R3.1.2 REJECT classes, not
        // split points — `wall_contact_refusals` catches those. Endpoint
        // touch resolves via the join stage, not a split. `Disjoint` and an
        // out-of-domain (`None`) pair induce nothing.
        _ => {}
    }
}

/// At most one split point per wall per topology cell: two points in one
/// cell are one point (ED-0103 N-2), and splitting twice inside a cell would
/// leave a child both of whose ends are that point. A stored coordinate wins
/// over a constructed one, so a wall passing through a node meets it exactly;
/// then the least coordinates, so the choice never depends on input order.
fn dedup_by_cell(candidates: &mut Vec<Candidate>) {
    let key = |c: &Candidate| (c.split.wall, topology_cell(c.split.at), c.constructed);
    candidates.sort_by(|a, b| {
        key(a)
            .cmp(&key(b))
            .then(a.split.at.x.total_cmp(&b.split.at.x))
            .then(a.split.at.y.total_cmp(&b.split.at.y))
    });
    candidates.dedup_by(|a, b| {
        a.split.wall == b.split.wall && topology_cell(a.split.at) == topology_cell(b.split.at)
    });
}

/// The operations that split `wall` at `points` (R3.1.3, R3.1.4).
///
/// R3.1.3: "The child containing the ORIGINAL start endpoint keeps the
/// original wall id. Every other child receives a command-supplied unused
/// entity id, assigned in chain order from Start." So the parent is SHORTENED
/// rather than deleted, which is also what keeps its joins, its openings and
/// its position in the collection.
///
/// R3.1.4: a membership on the original End role follows the original end
/// endpoint into the FINAL child's End, so an existing join at the far end
/// stays attached to the wall that still reaches it.
/// The operations that split `wall_id` at `points`, including the R4.3.3
/// remapping of every opening it hosts.
///
/// Returns `Err` when any hosted opening cannot be remapped, which R4.3.3
/// makes an ATOMIC rejection of the whole batch — including the wall creation
/// or endpoint move that caused the split — with `opening-split-ambiguous`.
pub fn split_operations(doc: &Document, wall_id: Uuid, points: &[Point]) -> Result<Vec<Operation>> {
    let Some(wall) = doc.wall(wall_id) else {
        return Ok(Vec::new());
    };
    let (start, end) = (wall.start(), wall.end());
    let length = wall.length_mm();

    // Ordered along Start→End, which is what makes "chain order from Start"
    // well defined.
    let mut cuts: Vec<Point> = points.to_vec();
    cuts.sort_by(|p, q| {
        param_along(start, end, *p)
            .partial_cmp(&param_along(start, end, *q))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // R3.1.2: a split plan producing a zero-length child rejects the whole
    // batch (`zero-length-split-child`). Checked here, before anything below
    // — including R4.3.3's opening remap — so a plan that violates both
    // reports THIS refusal, not a remap failure it would never have reached
    // had the split been well-formed. Exact equality, per R3.0.5: no cap
    // defines a minimum length or epsilon.
    let mut boundary = start;
    for cut in cuts.iter().chain(std::iter::once(&end)) {
        if boundary.x == cut.x && boundary.y == cut.y {
            return Err(EditError::ZeroLengthSplitChild {
                wall: wall_id,
                x: cut.x,
                y: cut.y,
            });
        }
        boundary = *cut;
    }

    let mut ops = Vec::new();
    let Some(&first_cut) = cuts.first() else {
        return Ok(ops);
    };

    // Children in chain order, each with its span [s, e] as a DISTANCE along
    // the original axis. R4.3.3 states containment in those terms, so they are
    // carried here rather than recomputed from the child walls afterwards —
    // the child's own length is subject to its own rounding.
    let mut children: Vec<(Uuid, f64, f64)> = Vec::with_capacity(cuts.len() + 1);

    // The original id keeps the segment containing the original START
    // (R3.1.3), so the parent is SHORTENED rather than deleted — which is
    // also what keeps its start-role joins, its position in the collection,
    // and its identity for anything already referring to it.
    children.push((wall_id, 0.0, param_along(start, end, first_cut) * length));

    // Every child clones the PARENT struct rather than rebuilding it field by
    // field: `Wall::new` only knows the fields this version interprets, and a
    // hand-built child would silently drop `unknown` — the forward-compat
    // fields FORMAT-SPEC §8 U2 requires a writer to carry verbatim. Cloning
    // preserves those, and every field this version does know, by
    // construction; only identity and span are then overwritten per child.
    let mut previous = first_cut;
    for cut in cuts.iter().skip(1) {
        let mut child = wall.clone();
        child.id = Uuid::now_v7();
        child.start_mm = previous.into();
        child.end_mm = (*cut).into();
        children.push((
            child.id,
            param_along(start, end, previous) * length,
            param_along(start, end, *cut) * length,
        ));
        ops.push(Operation::create(Entity::Wall(child)));
        previous = *cut;
    }
    let mut last = wall.clone();
    last.id = Uuid::now_v7();
    last.start_mm = previous.into();
    last.end_mm = end.into();
    let last_id = last.id;
    children.push((last_id, param_along(start, end, previous) * length, length));
    ops.push(Operation::create(Entity::Wall(last)));

    // Shorten the parent to its first segment.
    ops.push(set_field(
        wall_id,
        "endMm",
        serde_json::json!([first_cut.x, first_cut.y]),
    ));

    // R3.1.4: a membership on the original End role follows the original end
    // endpoint into the FINAL child's End.
    for join in &doc.joins {
        if let Some(i) = join
            .members
            .iter()
            .position(|m| m.wall_id == wall_id && m.role == Role::End)
        {
            let mut members = join.members.clone();
            members[i] = JoinMember::new(last_id, Role::End);
            ops.push(set_field(
                join.id,
                "members",
                serde_json::to_value(&members).expect("members serialise"),
            ));
        }
    }

    // R4.3.3 remap-or-reject. An opening moves to the child that STRICTLY
    // contains its closed aperture interval; strictness is the whole point,
    // because an aperture touching a split point at zero clearance has no
    // well-defined host and the specification rejects it rather than picking
    // a side.
    for opening in doc.openings_on_wall(wall_id) {
        let lo = opening.u * length - opening.width_mm / 2.0;
        let hi = opening.u * length + opening.width_mm / 2.0;
        let containing: Vec<&(Uuid, f64, f64)> = children
            .iter()
            .filter(|(_, s, e)| *s < lo && hi < *e)
            .collect();
        let [&(child_id, s, e)] = containing.as_slice() else {
            return Err(EditError::OpeningSplitAmbiguous(format!(
                "opening-split-ambiguous: opening {} spans {:.1}..{:.1} mm along \
                 wall {}, which no single child segment strictly contains; the \
                 whole edit is refused (BEHAVIOR-SPEC R4.3.3)",
                opening.id, lo, hi, wall_id
            )));
        };
        // Identity never changes; the host and the parameter do.
        if child_id != wall_id {
            ops.push(set_field(
                opening.id,
                "hostWallId",
                serde_json::json!(child_id),
            ));
        }
        // The shared formula admission verifies a host change against.
        let u = domowik_format::remap_u(opening.u, length, s, e);
        ops.push(set_field(opening.id, "u", serde_json::json!(u)));
    }

    Ok(ops)
}

/// Shared with admission's split-remap verification (`domowik-format`).
pub use domowik_format::param_along;

/// The operation that puts `wall`'s endpoint at `point` into a join.
///
/// Returns `None` when nothing else meets that point, so no join is needed.
///
/// R3.1.2 classifies only ENDPOINT-endpoint contact, and an occupied point
/// already covered by an existing join, here. Endpoint-on-interior and proper
/// crossings additionally SPLIT the walls they touch first — see
/// [`splits_induced`] and [`split_operations`] — which is why callers run
/// this against the POST-split geometry: a split's new endpoints are exactly
/// what this then joins.
pub fn join_operation(
    doc: &Document,
    level_id: Uuid,
    wall_id: Uuid,
    role: Role,
    point: Point,
) -> Option<Operation> {
    let member = JoinMember::new(wall_id, role);

    // R3.3.5: merge into the join already at this point rather than making a
    // second one there. `members` is replaced wholesale, which is exactly what
    // FORMAT-SPEC O3 says a set-valued field does.
    let cell = topology_cell(point);
    if let Some(existing) = join_in_cell(doc, level_id, cell) {
        if existing.contains(member) {
            return None;
        }
        let mut members = existing.members.clone();
        members.push(member);
        return Some(set_field(
            existing.id,
            "members",
            serde_json::to_value(&members).expect("members serialise"),
        ));
    }

    let mut members = vec![member];
    for other in doc.walls_on_level(level_id) {
        if other.id == wall_id {
            continue;
        }
        for other_role in [Role::Start, Role::End] {
            let p = match other_role {
                Role::Start => other.start(),
                Role::End => other.end(),
            };
            if topology_cell(p) == cell {
                members.push(JoinMember::new(other.id, other_role));
            }
        }
    }
    if members.len() < 2 {
        return None;
    }
    Some(Operation::create(Entity::WallJoin(WallJoin::new(members))))
}

/// The join on `level_id` whose members' endpoints lie in `cell`.
///
/// [`Document::join_at`] answers "is there a join NEAR this point" with a
/// distance; membership needs the identity instead, or a member merged into a
/// join across a cell boundary would fail R3.3.4 validation.
fn join_in_cell(doc: &Document, level_id: Uuid, cell: (i64, i64)) -> Option<&WallJoin> {
    doc.joins.iter().find(|j| {
        j.members.iter().any(|m| {
            doc.wall(m.wall_id).is_some_and(|w| {
                w.level_id == level_id
                    && topology_cell(match m.role {
                        Role::Start => w.start(),
                        Role::End => w.end(),
                    }) == cell
            })
        })
    })
}

/// A single-field `setFields` operation.
pub fn set_field(entity_id: Uuid, name: &str, value: serde_json::Value) -> Operation {
    let mut fields = serde_json::Map::new();
    fields.insert(name.to_owned(), value);
    Operation::set_fields(entity_id, fields)
}

/// What one UPDATE does to an OPTIONAL field.
///
/// Three outcomes, not two, and BEHAVIOR-SPEC F37d is why a plain `Option`
/// cannot express them: `{name:""}` sets the authoritative name to the empty
/// string, `{name:null}` clears it to absent, and the two are distinct states
/// that display identically. A field the command never mentions is a fourth
/// thing again — untouched — so "leave alone", "set", and "clear" each need
/// their own spelling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FieldEdit<T> {
    /// Not mentioned by this command; the stored value stands.
    #[default]
    Keep,
    /// Set to this value.
    Set(T),
    /// Clear to absent — `null` on the wire (FORMAT-SPEC §4.2 O2).
    Clear,
}

/// The fields one room UPDATE command may change (BEHAVIOR-SPEC R5.2.2).
///
/// Exactly the settable set the rule names: `seedMm`, the name, either style
/// reference, and `levelId`. The record's own id is not settable by anything.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoomFields {
    /// A new seed. Required-field, so it can be set but never cleared.
    pub seed: Option<Point>,
    pub name: FieldEdit<String>,
    pub floor_style_ref: FieldEdit<String>,
    pub ceiling_style_ref: FieldEdit<String>,
    /// A move between levels. Required-field, so it can be set but never
    /// cleared.
    pub level_id: Option<Uuid>,
}

/// Whether `seed` is a coordinate at all (R5.2.2 `nonfinite-coordinate`).
fn check_seed(seed: Point) -> Result<()> {
    if !seed.x.is_finite() || !seed.y.is_finite() {
        return Err(EditError::RoomSeedNotFinite {
            x: seed.x,
            y: seed.y,
        });
    }
    if !seed.is_within_bounds() {
        return Err(EditError::RoomSeedOutOfRange {
            x: seed.x,
            y: seed.y,
            bound: COORD_BOUND_MM,
        });
    }
    Ok(())
}

/// Whether a style reference, when present, is a non-empty token (R5.3.4).
fn check_style(field: &'static str, value: Option<&str>) -> Result<()> {
    if value == Some("") {
        Err(EditError::InvalidStyleReference { field })
    } else {
        Ok(())
    }
}

/// The CREATE command of R5.2.2: one atomic batch, one undo boundary.
///
/// # What it refuses, and what it deliberately does not
///
/// Refused: an unknown `levelId`, a non-finite or out-of-domain seed, an
/// empty-string style reference, and a supplied identifier already in use
/// anywhere in the document. Each rejects with an empty batch, no state change
/// and no undo entry.
///
/// NOT refused, and never even warned about: where the seed LANDS. A seed
/// inside a wall, on a wall's edge, or out in the exterior is a perfectly
/// legal record that simply takes a non-associating status (R5.2.4). R5.2.2
/// says so in one line — "seed location NEVER rejects or warns" — and it has
/// to be that way, because the walls move afterwards and a record whose seed
/// is momentarily homeless must survive to associate again (F30/F31).
///
/// Name and style references are optional with NO default sourcing: nothing is
/// inherited from the level, from a wall, or from a preference (R3.0.6).
pub fn create_room_operations(doc: &Document, record: RoomRecord) -> Result<Vec<Operation>> {
    if doc.level(record.level_id).is_none() {
        return Err(EditError::UnknownLevel(record.level_id));
    }
    check_seed(record.seed())?;
    check_style("floorStyleRef", record.floor_style_ref.as_deref())?;
    check_style("ceilingStyleRef", record.ceiling_style_ref.as_deref())?;
    if let Some(existing) = doc.entity(record.id) {
        return Err(EditError::RoomIdInUse {
            id: record.id,
            kind: existing.type_name(),
        });
    }
    Ok(vec![Operation::create(Entity::RoomRecord(record))])
}

/// The UPDATE command of R5.2.2, as ONE `setFields` operation.
///
/// # An empty result means "already in that state"
///
/// Same convention as [`set_level_operations`], for the same reason: a field
/// asked to take the value it already holds is omitted, and a command that
/// asks for nothing new returns `Ok(vec![])` rather than an operation with an
/// empty payload. An empty batch is a framing reject, and a no-op that grew
/// the log, moved the undo stack and dirtied the document would be worse than
/// nothing.
///
/// # The level move is one batch, and the projection follows
///
/// R5.2.2 requires a `levelId` update to recompute and reproject BOTH the
/// source and the target level in ONE batch, with undo restoring both. That
/// falls out of the design rather than needing code here: the projection is
/// derived from the document (R5.2.5 — reconciliation emits ZERO operations),
/// so moving the record's `levelId` in a single operation moves both
/// projections at once, and inverting that one operation moves them back.
/// Nothing else is touched, and no other level is touched (R5.1.9).
pub fn update_room_operations(
    doc: &Document,
    room_id: Uuid,
    fields: &RoomFields,
) -> Result<Vec<Operation>> {
    let record = doc
        .room_record(room_id)
        .ok_or(EditError::UnknownRoom(room_id))?;

    if let Some(seed) = fields.seed {
        check_seed(seed)?;
    }
    if let FieldEdit::Set(value) = &fields.floor_style_ref {
        check_style("floorStyleRef", Some(value.as_str()))?;
    }
    if let FieldEdit::Set(value) = &fields.ceiling_style_ref {
        check_style("ceilingStyleRef", Some(value.as_str()))?;
    }
    if let Some(level_id) = fields.level_id {
        if doc.level(level_id).is_none() {
            return Err(EditError::UnknownLevel(level_id));
        }
    }

    let mut payload = serde_json::Map::new();
    if let Some(seed) = fields.seed {
        let value: [f64; 2] = seed.into();
        if value != record.seed_mm {
            payload.insert("seedMm".to_owned(), serde_json::json!(value));
        }
    }
    if let Some(level_id) = fields.level_id {
        if level_id != record.level_id {
            payload.insert("levelId".to_owned(), serde_json::json!(level_id));
        }
    }
    for (name, edit, current) in [
        ("name", &fields.name, record.name.as_deref()),
        (
            "floorStyleRef",
            &fields.floor_style_ref,
            record.floor_style_ref.as_deref(),
        ),
        (
            "ceilingStyleRef",
            &fields.ceiling_style_ref,
            record.ceiling_style_ref.as_deref(),
        ),
    ] {
        match edit {
            FieldEdit::Keep => {}
            // `Some("")` and `None` are different states, so this comparison
            // must be against the OPTION and not against the string.
            FieldEdit::Set(value) if current == Some(value.as_str()) => {}
            FieldEdit::Set(value) => {
                payload.insert(name.to_owned(), serde_json::json!(value));
            }
            FieldEdit::Clear if current.is_none() => {}
            FieldEdit::Clear => {
                payload.insert(name.to_owned(), serde_json::Value::Null);
            }
        }
    }

    if payload.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![Operation::set_fields(room_id, payload)])
}

/// The DELETE command of R5.2.2: exactly that record, nothing cascaded.
///
/// Faces exist without records (R5.1.8), so the face the record named persists
/// untouched and simply stops showing a label. Deletion is authoritative and
/// no later wall edit, reconciliation pass, or reload brings the record back
/// (R5.2.7); only an explicit create, or undo of THIS batch, does.
pub fn delete_room_operations(doc: &Document, room_id: Uuid) -> Result<Vec<Operation>> {
    if doc.room_record(room_id).is_none() {
        return Err(EditError::UnknownRoom(room_id));
    }
    Ok(vec![Operation::delete(room_id)])
}

/// The operations one wall edit contributes, and what the command reports.
///
/// Batch construction is separated from file IO so it can be TESTED and
/// MEASURED without a plan on disk: issue 0032 benchmarks these gestures, and
/// a harness that had to write a container to time a wall drag would be
/// measuring the container.
///
/// The counters are what the CLI prints back. They are part of the return
/// value rather than recomputed by the caller because only the builder knows
/// which operations were incidental (R3.1.1 puts induced splits and joins in
/// the SAME batch as the edit that caused them).
#[derive(Debug, Default)]
pub struct WallEdit {
    /// The batch, in application order (FORMAT-SPEC B4).
    pub operations: Vec<Operation>,
    /// Split points introduced by contact reclassification (R3.1.2).
    pub splits: usize,
    /// Join entities created or extended (R3.1.2, R3.4.4).
    pub joins: usize,
    /// Endpoints that moved with a join node rather than alone (R3.4.2).
    pub moved_with: usize,
    /// Openings removed by host cascade (R3.1.8).
    pub cascaded_openings: usize,
}

/// Resolves R3.1.2 contact for a batch's EXPLICIT axis changes — the
/// reject-class precheck, every induced split, and every induced or existing
/// join — shared by [`add_wall_operations`] and [`set_wall_operations`] so a
/// wall dragged across another axis is split exactly as one drawn across it
/// would be.
///
/// `affected` names every wall whose axis this batch's explicit operations
/// already changed in `projected` (the new wall; the named wall and every
/// R3.4.2 co-moved neighbor) — NOT a geometry-neutral thickness, height or
/// metadata edit, which invents no axis and so must pass `&[]` here. This
/// function extends both `projected` and `edit` with whatever splits and
/// joins those axes induce.
///
/// # Precedence (R3.1.2)
///
/// Full coincidence and positive-length collinear overlap are REJECT
/// classes and are checked FIRST, against the explicit/co-move state alone —
/// before any split is attempted, and so before R4.3.3's remap-or-reject can
/// even run. A batch that would hit both reports the contact refusal, never
/// the remap failure. Splits and joins are then resolved, and the same guard
/// runs AGAIN at the end: splitting can change a THIRD wall's stored axis
/// (the shortened parent, or a new child), and that indirectly affected axis
/// must be free of refusing contacts too, not only the ones named by
/// `affected`.
fn resolve_contacts(
    doc: &Document,
    level_id: Uuid,
    projected: &mut Document,
    edit: &mut WallEdit,
    affected: &[Uuid],
) -> Result<()> {
    if affected.is_empty() {
        // A thickness/height/metadata-only edit invents no axis: nothing here
        // has anything to classify, and running the checks below would only
        // spend an O(n) pass proving what is already known.
        return Ok(());
    }

    // Only the refusing classes here: an unresolved crossing is exactly what
    // the splits below resolve.
    let refusals = axis_contact_refusals(doc, projected);
    if !refusals.is_empty() {
        return Err(EditError::WallContact(refusals));
    }

    // The result is checked, and an unresolved contact it still holds adds
    // its point to the wall the piece came from; the splits are then redone
    // from the unsplit state, so every wall still splits once and every
    // opening remaps in one step. Two things leave such a contact. A wall
    // the gesture split may carry a contact its stored history left
    // unresolved. And splitting moves a piece's axis by up to half an ulp at
    // its new end, so a piece can cross a wall that passes a fraction of an
    // ulp from a node, which the unsplit axes did not show. Nearly every
    // gesture passes on the first attempt, and the check is the final guard,
    // which runs anyway.
    let gesture_ops = edit.operations.len();
    let mut extra: Vec<Candidate> = Vec::new();
    for attempt in 0..=MAX_RENODING {
        if attempt > 0 {
            *projected = doc.clone();
            for op in &edit.operations[..gesture_ops] {
                apply_preview(projected, op);
            }
            edit.operations.truncate(gesture_ops);
            edit.splits = 0;
            edit.joins = 0;
        }
        let origin = split_and_join(level_id, projected, edit, affected, &extra)?;

        // "Wall-contact admission", in full: judged at the batch's COMPLETE
        // final positions, which include every split child and shortened
        // parent. The unresolved classes are included, so the builder can
        // never emit a batch that session admission would refuse for them.
        let refusals = wall_contact_refusals(doc, projected);
        if refusals.is_empty() {
            return Ok(());
        }
        if refusals[0].is_axis_refusal() || attempt == MAX_RENODING {
            return Err(EditError::WallContact(refusals));
        }
        let before = extra.len();
        for block in &refusals {
            let (WallContactBlock::UnresolvedCrossing { wall, other }
            | WallContactBlock::UnsplitContact { wall, other }) = *block
            else {
                continue;
            };
            let (Some(a), Some(b)) = (projected.wall(wall), projected.wall(other)) else {
                continue;
            };
            let mut found = Vec::new();
            pair_candidates(
                (a.id, a.start(), a.end()),
                (b.id, b.start(), b.end()),
                &mut found,
            );
            extra.extend(found.into_iter().map(|mut c| {
                c.split.wall = origin.get(&c.split.wall).copied().unwrap_or(c.split.wall);
                c
            }));
        }
        if extra.len() == before {
            return Err(EditError::WallContact(refusals));
        }
    }
    unreachable!("the last attempt returns")
}

/// One pass of contact resolution over `projected`: classify, split, join.
/// Returns which original wall each piece came from.
///
/// `extra` holds split points found by re-noding (see [`resolve_contacts`]),
/// already named by original wall.
fn split_and_join(
    level_id: Uuid,
    projected: &mut Document,
    edit: &mut WallEdit,
    affected: &[Uuid],
    extra: &[Candidate],
) -> Result<std::collections::HashMap<Uuid, Uuid>> {
    // BEHAVIOR-SPEC R3.1.2, rows two and three: an endpoint landing on
    // another wall's interior splits that wall, and a proper crossing splits
    // BOTH. Classified against every `affected` wall — not only a single
    // named one — so a wall dragged onto or across a third axis splits it
    // exactly as a newly drawn wall would (R3.1.1 puts these in the SAME
    // atomic batch as the edit that caused them). All classification runs
    // before any split is applied: a split changes identities mid-stream, and
    // a pair can otherwise be classified once from each side, so the raw
    // points are deduplicated together before grouping by target.
    let mut nodes: Vec<Point> = Vec::new();
    for &wall_id in affected {
        if let Some(wall) = projected.wall(wall_id) {
            nodes.push(wall.start());
            nodes.push(wall.end());
        }
    }
    // A wall the gesture splits has a changed axis too, and must end up free
    // of unresolved contacts — a legacy crossing elsewhere on a host the
    // gesture split. Classifying every such wall again here would cost a pass
    // over the level per split wall, and in a conforming plan find nothing:
    // those contacts were resolved when they were made. The final guard in
    // [`resolve_contacts`] finds the ones that are not, and re-noding adds
    // them (`extra`), so they cost only where they exist.
    let mut candidates: Vec<Candidate> = extra.to_vec();
    for &wall_id in affected {
        let Some(wall) = projected.wall(wall_id) else {
            continue;
        };
        let (start, end) = (wall.start(), wall.end());
        candidates.extend(split_candidates(projected, level_id, wall_id, start, end));
    }
    dedup_by_cell(&mut candidates);
    nodes.extend(candidates.iter().map(|c| c.split.at));

    let mut by_wall: std::collections::BTreeMap<Uuid, Vec<Point>> =
        std::collections::BTreeMap::new();
    for c in &candidates {
        by_wall.entry(c.split.wall).or_default().push(c.split.at);
    }
    let mut origin: std::collections::HashMap<Uuid, Uuid> = std::collections::HashMap::new();
    for (target, points) in &by_wall {
        // R4.3.3 rejects the ENTIRE batch — this edit included — when a
        // hosted opening has no strictly containing child. Propagating the
        // error is what makes that atomic.
        let split_ops = split_operations(projected, *target, points)?;
        for op in &split_ops {
            apply_preview(projected, op);
            if let OperationKind::CreateEntity {
                entity: Entity::Wall(child),
            } = &op.kind
            {
                origin.insert(child.id, *target);
            }
        }
        if !split_ops.is_empty() {
            edit.splits += points.len();
        }
        edit.operations.extend(split_ops);
    }

    // Joins are classified LAST, against the post-split geometry: a split
    // creates new endpoints at the split point, and those are exactly what the
    // endpoint-endpoint row then joins — including merging into a join that
    // already occupies the point, which grows its membership rather than
    // minting a second one there. Membership is by topology cell, the
    // identity validation applies (R3.3.4), so a join never spans two cells.
    let cells: std::collections::BTreeSet<(i64, i64)> =
        nodes.iter().map(|p| topology_cell(*p)).collect();
    for op in node_join_operations(projected, level_id, &cells) {
        apply_preview(projected, &op);
        if !matches!(op.kind, OperationKind::DeleteEntity) {
            edit.joins += 1;
        }
        edit.operations.push(op);
    }
    Ok(origin)
}

/// The join operations for every topology cell in `cells` where two or more
/// walls meet: a new join, a merge into the join already there, or — when an
/// edit brings two existing joins into one cell — one surviving join holding
/// every member, the others deleted in the same batch (R3.3.2: an endpoint
/// belongs to at most one join).
///
/// A wall whose two ends share a cell is a point at topology resolution and
/// takes no membership there — a join may never hold both ends of one wall
/// (R3.3.3), and such a wall alone is legal (R3.0.5 sets no minimum length).
/// A join it already belonged to, now in that cell, lets it go; a join left
/// with fewer than two members is deleted.
///
/// The batched form of [`join_operation`] for a whole gesture. One pass over
/// the level's walls finds the endpoints in those cells and one over the joins
/// finds those already there, so the cost is O(n + m) for n walls and m join
/// memberships, however many nodes the gesture touches. Calling
/// [`join_operation`] per endpoint rescans the walls and joins for each one,
/// which a gesture crossing many walls of a large plan turned into seconds.
fn node_join_operations(
    doc: &Document,
    level_id: Uuid,
    cells: &std::collections::BTreeSet<(i64, i64)>,
) -> Vec<Operation> {
    let mut at: std::collections::BTreeMap<(i64, i64), Vec<JoinMember>> =
        std::collections::BTreeMap::new();
    let mut ends: std::collections::HashMap<JoinMember, (i64, i64)> =
        std::collections::HashMap::new();
    // Walls that are a point at topology resolution, in a touched cell.
    let mut points: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for wall in doc.walls_on_level(level_id) {
        let (start, end) = (topology_cell(wall.start()), topology_cell(wall.end()));
        if start == end {
            if cells.contains(&start) {
                points.insert(wall.id);
                for role in [Role::Start, Role::End] {
                    ends.insert(JoinMember::new(wall.id, role), start);
                }
            }
            continue;
        }
        for (role, cell) in [(Role::Start, start), (Role::End, end)] {
            if cells.contains(&cell) {
                let member = JoinMember::new(wall.id, role);
                ends.insert(member, cell);
                at.entry(cell).or_default().push(member);
            }
        }
    }
    // The joins in each cell. `ends` holds only endpoints in the touched
    // cells, so any hit is one of them.
    let mut existing: std::collections::BTreeMap<(i64, i64), Vec<&WallJoin>> =
        std::collections::BTreeMap::new();
    for join in &doc.joins {
        if let Some(cell) = join.members.iter().find_map(|m| ends.get(m)) {
            existing.entry(*cell).or_default().push(join);
        }
    }

    // Every cell with eligible ends OR an existing join: a cell holding only
    // walls collapsed into it still has joins to release.
    let mut every: std::collections::BTreeSet<(i64, i64)> = at.keys().copied().collect();
    every.extend(existing.keys().copied());
    let mut ops = Vec::new();
    for cell in every {
        let members = at.remove(&cell).unwrap_or_default();
        let mut joins = existing.remove(&cell).unwrap_or_default();
        joins.sort_by_key(|j| j.id);
        let Some((survivor, others)) = joins.split_first() else {
            if members.len() >= 2 {
                ops.push(Operation::create(Entity::WallJoin(WallJoin::new(members))));
            }
            continue;
        };
        let mut merged = survivor.members.clone();
        for join in others {
            ops.push(Operation::delete(join.id));
            merged.extend(join.members.iter().copied());
        }
        merged.extend(members);
        let mut seen = std::collections::HashSet::new();
        // A wall that became a point here leaves the join: holding both its
        // ends would be degenerate (R3.3.3).
        merged.retain(|m| !points.contains(&m.wall_id) && seen.insert(*m));
        if merged.len() < 2 {
            ops.push(Operation::delete(survivor.id));
        } else if merged != survivor.members {
            ops.push(set_field(
                survivor.id,
                "members",
                serde_json::to_value(&merged).expect("members serialise"),
            ));
        }
    }
    ops
}

/// Builds the batch that adds `wall`, including every split and join its final
/// position induces (R3.1.1, R3.1.2).
///
/// Returns an error when the batch's final axes contain a refusing R3.1.2
/// contact (`EditError::WallContact`), or when a split it induces would
/// leave a hosted opening with no child that strictly contains it
/// (`EditError::OpeningSplitAmbiguous`, R4.3.3) — either refuses the whole
/// batch, the wall creation included.
pub fn add_wall_operations(doc: &Document, level_id: Uuid, wall: Wall) -> Result<WallEdit> {
    let wall_id = wall.id;
    if [wall.start(), wall.end()]
        .iter()
        .any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err(EditError::NonFiniteCoordinate { wall: wall_id });
    }
    let mut edit = WallEdit {
        operations: vec![Operation::create(Entity::Wall(wall))],
        ..Default::default()
    };

    // A scratch document the batch's earlier operations have been applied to.
    // FORMAT-SPEC B4 makes a batch apply in array order, so a later operation
    // must be classified against what the earlier ones did — classifying
    // everything against the pre-batch state would place a join at a corner
    // the split had already moved.
    let mut projected = doc.clone();
    apply_preview(&mut projected, &edit.operations[0]);

    resolve_contacts(doc, level_id, &mut projected, &mut edit, &[wall_id])?;

    Ok(edit)
}

/// Builds the batch that moves or resizes one wall.
///
/// BEHAVIOR-SPEC R3.4.2 is the rule that makes this more than a field write:
/// a numeric edit of a JOINED endpoint acts on the JOIN NODE, moving every
/// member endpoint rigidly together in ONE batch. Moving only the named wall
/// would tear the corner apart and leave a join whose members disagree, which
/// R3.3.4 rejects — so the command that did the obvious thing would fail
/// validation on its own next run.
///
/// R3.4.4: the edit reclassifies contact at its final positions, in the SAME
/// batch — a wall moved onto another wall's endpoint auto-joins, one moved
/// across another wall's interior splits it (R3.1.2), and one dragged
/// through a live crossing splits BOTH, exactly as [`add_wall_operations`]
/// would for a wall newly drawn there. Unchanged undirected axes, including
/// explicitly supplied unchanged endpoints, trigger no contact resolution.
///
/// Returns an empty batch when nothing was asked for; the caller decides
/// whether that is a usage error.
///
/// Returns `Err(EditError::WallContact(_))` when the batch's final axes —
/// the named wall's, and every R3.4.2 co-moved neighbor's — contain a
/// refusing contact, checked ahead of any split (BEHAVIOR-SPEC R3.1.2's
/// precedence; see the "Wall-contact admission" rule in §3.1) — or
/// `Err(EditError::OpeningSplitAmbiguous(_))` when an induced split would
/// leave a hosted opening with no strictly containing child (R4.3.3).
pub fn set_wall_operations(
    doc: &Document,
    wall_id: Uuid,
    start: Option<Point>,
    end: Option<Point>,
    thickness_mm: Option<f64>,
    height_mm: Option<f64>,
) -> Result<WallEdit> {
    let Some(wall) = doc.wall(wall_id) else {
        return Ok(WallEdit::default());
    };
    let level_id = wall.level_id;
    // A non-finite coordinate cannot even be serialized faithfully (JSON
    // has no NaN): refuse it here, by name, before any batch exists.
    if [start, end]
        .into_iter()
        .flatten()
        .any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err(EditError::NonFiniteCoordinate { wall: wall_id });
    }

    let mut edit = WallEdit::default();
    // Every wall whose `startMm`/`endMm` this edit actually writes — the
    // named wall, and every R3.4.2 co-moved neighbor. Empty for a
    // geometry-neutral thickness/height-only edit, which must invent no
    // axis change and so induces no split or join reclassification.
    let mut affected: Vec<Uuid> = Vec::new();

    for (role, target) in [(Role::Start, start), (Role::End, end)] {
        let Some(to) = target else { continue };
        // An endpoint asked to stay where it is is no request. Co-moving its
        // node would still rewrite every member to these exact coordinates,
        // and members may legally store different points in one cell — so
        // a full property form with unchanged coordinates would move a
        // neighbour and resolve its legacy contacts, where the same form
        // without coordinates is neutral.
        let current = match role {
            Role::Start => wall.start(),
            Role::End => wall.end(),
        };
        if to.x == current.x && to.y == current.y {
            continue;
        }
        // R3.4.2: the whole node moves, not just this wall's endpoint — the
        // node this endpoint is a MEMBER of, never one it merely touches.
        let node: Vec<JoinMember> = member_join(doc, wall_id, role)
            .map(|j| j.members.clone())
            .unwrap_or_default();
        for member in &node {
            let field = match member.role {
                Role::Start => "startMm",
                Role::End => "endMm",
            };
            if member.wall_id != wall_id {
                edit.moved_with += 1;
            }
            edit.operations.push(set_field(
                member.wall_id,
                field,
                serde_json::json!([to.x, to.y]),
            ));
            affected.push(member.wall_id);
        }
        if node.is_empty() {
            let field = match role {
                Role::Start => "startMm",
                Role::End => "endMm",
            };
            edit.operations
                .push(set_field(wall_id, field, serde_json::json!([to.x, to.y])));
            affected.push(wall_id);
        }
    }

    if let Some(t) = thickness_mm {
        edit.operations
            .push(set_field(wall_id, "thicknessMm", serde_json::json!(t)));
    }
    if let Some(h) = height_mm {
        edit.operations
            .push(set_field(wall_id, "heightMm", serde_json::json!(h)));
    }

    affected.sort();
    affected.dedup();

    let mut projected = doc.clone();
    for op in &edit.operations {
        apply_preview(&mut projected, op);
    }

    // A caller may submit a full property form with unchanged coordinates.
    // Classify actual final axis changes, not the presence of coordinate keys:
    // neutral edits must not silently split a legacy unresolved crossing.
    affected.retain(|id| match (doc.wall(*id), projected.wall(*id)) {
        (Some(before), Some(after)) => {
            before.level_id != after.level_id
                || !((before.start_mm == after.start_mm && before.end_mm == after.end_mm)
                    || (before.start_mm == after.end_mm && before.end_mm == after.start_mm))
        }
        _ => true,
    });

    resolve_contacts(doc, level_id, &mut projected, &mut edit, &affected)?;

    Ok(edit)
}

/// Translates a same-level selection in millimetres (R7.1 owner amendment,
/// issue 0107). IDs are deduplicated and sorted for deterministic effect order.
/// Each selected wall and its attached nodes move once; contacts are resolved
/// only after all coordinates have been projected. Models retain their other
/// transform fields. Openings may accompany their selected host, not move alone.
/// Unknown/unsupported IDs, mixed levels, non-finite displacement, or refusing
/// final contacts fail the whole builder. Session admission enforces coordinate
/// bounds and asset mutation policy. Zero displacement emits no operations.
pub fn translate_selection_operations(
    doc: &Document,
    ids: &[Uuid],
    delta: Point,
) -> std::result::Result<Vec<Operation>, String> {
    if !delta.x.is_finite() || !delta.y.is_finite() {
        return Err("selection displacement must be finite".into());
    }
    let ids: std::collections::BTreeSet<_> = ids.iter().copied().collect();
    let mut level = None;
    let mut nodes = std::collections::BTreeSet::new();
    let mut edit = WallEdit::default();
    for id in &ids {
        let entity_level = if let Some(wall) = doc.wall(*id) {
            for role in [Role::Start, Role::End] {
                nodes.insert((*id, role == Role::Start));
                if let Some(join) = member_join(doc, *id, role) {
                    nodes.extend(
                        join.members
                            .iter()
                            .map(|m| (m.wall_id, m.role == Role::Start)),
                    );
                }
            }
            wall.level_id
        } else if let Some(asset) = doc.asset_instance(*id) {
            let mut transform = asset.transform.clone();
            transform.translation_mm[0] += delta.x;
            transform.translation_mm[1] += delta.y;
            edit.operations
                .push(set_field(*id, "transform", serde_json::json!(transform)));
            asset.level_id
        } else if let Some(opening) = doc.openings.iter().find(|opening| opening.id == *id) {
            if !ids.contains(&opening.host_wall_id) {
                return Err("group movement requires selecting an opening's host wall".into());
            }
            doc.wall(opening.host_wall_id)
                .ok_or("opening host not found")?
                .level_id
        } else {
            return Err(format!("unsupported group movement target {id}"));
        };
        if level.is_some_and(|previous| previous != entity_level) {
            return Err("group movement must stay on one level".into());
        }
        level = Some(entity_level);
    }
    if delta.x == 0.0 && delta.y == 0.0 {
        return Ok(Vec::new());
    }
    let mut affected = Vec::new();
    for (id, is_start) in nodes {
        let wall = doc.wall(id).ok_or("joined wall not found")?;
        let (field, at) = match is_start {
            true => ("startMm", wall.start()),
            false => ("endMm", wall.end()),
        };
        let to = [at.x + delta.x, at.y + delta.y];
        if !to.iter().all(|v| v.is_finite()) {
            return Err("translated endpoint must be finite".into());
        }
        edit.operations
            .push(set_field(id, field, serde_json::json!(to)));
        affected.push(id);
    }
    affected.sort();
    affected.dedup();
    if let Some(level) = level {
        let mut projected = doc.clone();
        for op in &edit.operations {
            apply_preview(&mut projected, op);
        }
        resolve_contacts(doc, level, &mut projected, &mut edit, &affected)
            .map_err(|error| error.to_string())?;
    }
    Ok(edit.operations)
}

/// Builds the batch that REVERSES a wall's direction (R3.3.6, R4.3.1).
///
/// Swapping `startMm` and `endMm` alone is wrong, and wrong silently: every
/// hosted opening is positioned and oriented in AXIS terms, so a bare swap
/// teleports each door to the mirror-image position and points it the wrong
/// way while leaving a document that still validates. R4.3.1 therefore
/// requires the field map to travel in the SAME batch:
///
/// - `u := 1 − u`, so `P(1−u)` on the reversed axis is `P(u)` on the original;
/// - `side`, `opensToward` flip, because `n̂` flipped and the same physical
///   face now has the other name;
/// - `hinge` flips, because `nearStart` and `nearEnd` are axis terms;
/// - `swingDeg`, dimensions, `kind` and styles are untouched.
///
/// World intent is preserved exactly: nothing moves, and only the names of
/// where things are change.
pub fn reverse_wall_operations(doc: &Document, wall_id: Uuid) -> WallEdit {
    let Some(wall) = doc.wall(wall_id) else {
        return WallEdit::default();
    };
    let (s, e) = (wall.start(), wall.end());
    let mut edit = WallEdit {
        operations: vec![
            set_field(wall_id, "startMm", serde_json::json!([e.x, e.y])),
            set_field(wall_id, "endMm", serde_json::json!([s.x, s.y])),
        ],
        ..Default::default()
    };

    // R3.3.6: a membership names an axis END, so reversing the wall swaps
    // which role each membership refers to. Forgetting this leaves every join
    // on the wall disagreeing about where its node is, and R3.3.4 then rejects
    // the whole command — so a reversal that "worked" in a test with no joins
    // fails on every real plan.
    for join in &doc.joins {
        if !join.members.iter().any(|m| m.wall_id == wall_id) {
            continue;
        }
        let members: Vec<JoinMember> = join
            .members
            .iter()
            .map(|m| {
                if m.wall_id != wall_id {
                    return *m;
                }
                JoinMember::new(
                    m.wall_id,
                    match m.role {
                        Role::Start => Role::End,
                        Role::End => Role::Start,
                    },
                )
            })
            .collect();
        edit.operations.push(set_field(
            join.id,
            "members",
            serde_json::to_value(&members).expect("members serialise"),
        ));
        edit.joins += 1;
    }

    let flip_side = |v: Side| match v {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
    };
    for opening in doc.openings_on_wall(wall_id) {
        let mut fields = serde_json::Map::new();
        fields.insert("u".to_owned(), serde_json::json!(1.0 - opening.u));
        fields.insert(
            "side".to_owned(),
            serde_json::to_value(flip_side(opening.side)).expect("side serialises"),
        );
        fields.insert(
            "opensToward".to_owned(),
            serde_json::to_value(flip_side(opening.opens_toward)).expect("opensToward serialises"),
        );
        fields.insert(
            "hinge".to_owned(),
            serde_json::to_value(match opening.hinge {
                Hinge::NearStart => Hinge::NearEnd,
                Hinge::NearEnd => Hinge::NearStart,
            })
            .expect("hinge serialises"),
        );
        edit.operations
            .push(Operation::set_fields(opening.id, fields));
        edit.moved_with += 1;
    }
    edit
}

/// Removes a wall by its `info` number, along with any openings it hosts.
///
/// Cascading to the openings is deliberate: leaving them would produce a
/// document that fails validation, so the alternative is refusing the delete
/// and making the user remove them one at a time first.
/// Builds the batch that deletes one wall, with its cascades.
///
/// Cascades are explicit as OPERATIONS rather than hidden in a projection
/// step: every effect of a command is in its batch, or replaying the log would
/// not reproduce what the user saw.
pub fn remove_wall_operations(doc: &Document, wall_id: Uuid) -> WallEdit {
    let mut edit = WallEdit::default();

    // R3.1.8 / §3 F20: hosted openings are deleted FIRST — the wall's
    // deletion depends on them — so undo, applying the inverse in reverse,
    // recreates the wall before its openings.
    let hosted: Vec<Uuid> = doc
        .openings
        .iter()
        .filter(|o| o.host_wall_id == wall_id)
        .map(|o| o.id)
        .collect();
    edit.cascaded_openings = hosted.len();
    for id in &hosted {
        edit.operations.push(Operation::delete(*id));
    }
    edit.operations.push(Operation::delete(wall_id));

    // A join is a set of wall endpoints (R3.3.1). Losing a wall removes its
    // memberships, and a join left below two members is deleted with it.
    for join in &doc.joins {
        if !join.members.iter().any(|m| m.wall_id == wall_id) {
            continue;
        }
        let remaining: Vec<JoinMember> = join
            .members
            .iter()
            .filter(|m| m.wall_id != wall_id)
            .copied()
            .collect();
        if remaining.len() < 2 {
            edit.operations.push(Operation::delete(join.id));
        } else {
            edit.operations.push(set_field(
                join.id,
                "members",
                serde_json::to_value(&remaining).expect("members serialise"),
            ));
        }
    }

    edit
}

/// The point where two segments cross at INTERIOR points of both, if any.
///
/// `None` when they merely touch at an endpoint (that is the endpoint-endpoint
/// or endpoint-on-interior row, not a crossing) or do not meet. The class is
/// decided exactly by [`axis_contact`]; the point is the correctly rounded
/// [`crossing_point`].
pub fn proper_crossing(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    match axis_contact(a, b, c, d) {
        Some(AxisContact::ProperCrossing) => crossing_point(a, b, c, d),
        _ => None,
    }
}

#[cfg(test)]
mod level_tests {
    use super::*;
    use domowik_format::Level;

    fn one_level() -> (Document, Uuid) {
        let mut doc = Document::new();
        let level = Level::new("Level 1");
        let id = level.id;
        doc.levels.push(level);
        (doc, id)
    }

    fn fields_of(ops: &[Operation]) -> serde_json::Map<String, serde_json::Value> {
        assert_eq!(ops.len(), 1, "ONE setFields, never several");
        match &ops[0].kind {
            OperationKind::SetFields { fields } => fields.clone(),
            other => panic!("expected setFields, got {other:?}"),
        }
    }

    #[test]
    fn only_the_changed_fields_appear_and_they_share_one_operation() {
        let (doc, id) = one_level();
        let ops = set_level_operations(
            &doc,
            id,
            &LevelFields {
                name: Some("Basement".to_owned()),
                elevation_mm: Some(-3000.0),
                // Unchanged: 200.0 is what `Level::new` already holds.
                floor_thickness_mm: Some(200.0),
                ..Default::default()
            },
        )
        .expect("valid");
        let fields = fields_of(&ops);
        assert_eq!(fields.len(), 2, "the unchanged thickness is omitted");
        assert_eq!(fields["name"], "Basement");
        assert_eq!(fields["elevationMm"], -3000.0);
    }

    #[test]
    fn a_command_that_changes_nothing_returns_no_operations() {
        let (doc, id) = one_level();
        let level = doc.level(id).unwrap().clone();
        let ops = set_level_operations(
            &doc,
            id,
            &LevelFields {
                name: Some(level.name.clone()),
                elevation_mm: Some(level.elevation_mm),
                floor_thickness_mm: Some(level.floor_thickness_mm),
                default_wall_height_mm: Some(level.default_wall_height_mm),
                default_ceiling_height_mm: Some(level.default_ceiling_height_mm),
            },
        )
        .expect("valid");
        assert!(
            ops.is_empty(),
            "an empty batch is a framing reject; emit nothing instead"
        );
        assert!(set_level_operations(&doc, id, &LevelFields::default())
            .expect("valid")
            .is_empty());
    }

    #[test]
    fn an_unknown_level_or_an_out_of_range_value_is_refused_whole() {
        let (doc, id) = one_level();
        assert!(matches!(
            set_level_operations(&doc, Uuid::now_v7(), &LevelFields::default()),
            Err(EditError::UnknownLevel(_))
        ));
        for bad in [
            LevelFields {
                // A legal change alongside an illegal one: NOTHING is emitted.
                name: Some("Renamed".to_owned()),
                default_wall_height_mm: Some(0.0),
                ..Default::default()
            },
            LevelFields {
                floor_thickness_mm: Some(-1.0),
                ..Default::default()
            },
            LevelFields {
                elevation_mm: Some(f64::NAN),
                ..Default::default()
            },
            LevelFields {
                elevation_mm: Some(COORD_BOUND_MM * 2.0),
                ..Default::default()
            },
            LevelFields {
                default_ceiling_height_mm: Some(DIMENSION_CAP_MM * 2.0),
                ..Default::default()
            },
        ] {
            assert!(
                matches!(
                    set_level_operations(&doc, id, &bad),
                    Err(EditError::InvalidLevelField { .. })
                ),
                "{bad:?} must be refused"
            );
        }
    }

    #[test]
    fn a_zero_floor_thickness_is_legal_and_emitted() {
        let (doc, id) = one_level();
        let ops = set_level_operations(
            &doc,
            id,
            &LevelFields {
                floor_thickness_mm: Some(0.0),
                ..Default::default()
            },
        )
        .expect("zero is legal");
        assert_eq!(fields_of(&ops)["floorThicknessMm"], 0.0);
    }

    /// Issue 0115: the whole-list reorder assigns each listed level its list
    /// position, skips levels already holding it, and is empty for one with
    /// no effect at all.
    #[test]
    fn the_whole_list_reorder_writes_only_the_moved_levels() {
        let (mut doc, first) = one_level();
        let mut second = Level::new("Lower");
        second.elevation_mm = -2600.0;
        let second_id = second.id;
        doc.levels.push(second);

        // A list already matching storage (displayOrder is absent and a
        // reordered copy would assign ordinals first) still writes, because
        // the ordinals themselves are new: 0 then 1.
        let ops = reorder_levels_operations(&doc, &[first, second_id]).expect("a full permutation");
        assert_eq!(ops.len(), 2, "an absent ordinal is not 'already stored'");
        assert_eq!(
            fields_of(&[ops[0].clone()])["displayOrder"],
            serde_json::json!(0),
            "the FIRST listed level is ordinal 0"
        );
        assert_eq!(fields_of(&[ops[1].clone()])["displayOrder"], 1);
        for op in &ops {
            apply_preview(&mut doc, op);
        }
        assert_eq!(doc.level(first).unwrap().display_order, Some(0));
        assert_eq!(doc.level(second_id).unwrap().display_order, Some(1));

        // Replaying the same list changes nothing: every level already holds
        // its ordinal, so the caller sees "already in this order".
        assert!(reorder_levels_operations(&doc, &[first, second_id])
            .expect("valid")
            .is_empty());

        // A swap rewrites the minimum: two touched levels, one operation each.
        let ops = reorder_levels_operations(&doc, &[second_id, first]).expect("valid");
        assert_eq!(ops.len(), 2, "both levels change when they trade places");
        for op in &ops {
            apply_preview(&mut doc, op);
        }
        assert_eq!(doc.level(second_id).unwrap().display_order, Some(0));
        assert_eq!(doc.level(first).unwrap().display_order, Some(1));
        assert!(
            reorder_levels_operations(&doc, &[second_id, first])
                .expect("valid")
                .is_empty(),
            "the swapped order is now stored"
        );
    }

    /// The reorder is Whole-list: a partial list, a duplicate, a foreign id
    /// and an empty document are all refused before any operation exists.
    #[test]
    fn a_reorder_that_is_not_exactly_the_current_levels_is_refused() {
        let (doc, first) = one_level();
        assert!(matches!(
            reorder_levels_operations(&doc, &[first, Uuid::now_v7()]),
            Err(EditError::LevelListOrder(_))
        ));
        assert!(matches!(
            reorder_levels_operations(&doc, &[first, first]),
            Err(EditError::LevelListOrder(_))
        ));
        // A one-level document can be listed exactly: that is a legal
        // (vacuous) order, not a refusal case.
        let ops = reorder_levels_operations(&doc, &[first]).expect("legal");
        assert_eq!(ops.len(), 1);
        assert!(matches!(
            reorder_levels_operations(&Document::new(), &[]),
            Err(EditError::LevelListOrder(_))
        ));
        assert!(matches!(
            reorder_levels_operations(&doc, &[Uuid::nil()]),
            Err(EditError::LevelListOrder(_))
        ));
    }

    #[test]
    fn a_signed_zero_is_not_a_change_and_an_emitted_zero_is_positive() {
        // FORMAT-SPEC's numeric policy maps −0 to +0, so neither direction is
        // a change and nothing emitted may carry a negative zero.
        let (mut doc, id) = one_level();
        doc.levels[0].floor_thickness_mm = 0.0;
        assert!(
            set_level_operations(
                &doc,
                id,
                &LevelFields {
                    floor_thickness_mm: Some(-0.0),
                    ..Default::default()
                },
            )
            .expect("valid")
            .is_empty(),
            "−0 against a stored +0 is not a change"
        );

        doc.levels[0].floor_thickness_mm = 200.0;
        let ops = set_level_operations(
            &doc,
            id,
            &LevelFields {
                floor_thickness_mm: Some(-0.0),
                ..Default::default()
            },
        )
        .expect("valid");
        let emitted = fields_of(&ops)["floorThicknessMm"].as_f64().unwrap();
        assert_eq!(emitted, 0.0);
        assert!(
            emitted.is_sign_positive(),
            "an emitted zero is normalized to +0"
        );
    }

    #[test]
    fn wall_height_resolves_from_the_level_only_when_absent() {
        let (mut doc, id) = one_level();
        assert_eq!(
            resolve_wall_height(&doc, id, None).expect("default"),
            2500.0
        );
        assert_eq!(
            resolve_wall_height(&doc, id, Some(3000.0)).expect("explicit"),
            3000.0,
            "an explicit height is preserved, never overridden by the default"
        );
        assert!(matches!(
            resolve_wall_height(&doc, id, Some(0.0)),
            Err(EditError::InvalidLevelField { .. })
        ));
        assert!(matches!(
            resolve_wall_height(&doc, Uuid::now_v7(), None),
            Err(EditError::UnknownLevel(_))
        ));

        // Changing the default changes what a LATER resolution yields, and
        // nothing else: §2.1's copy-at-creation rule means walls already
        // built keep the height they resolved to.
        doc.levels[0].default_wall_height_mm = 2700.0;
        assert_eq!(
            resolve_wall_height(&doc, id, None).expect("default"),
            2700.0
        );
    }

    #[test]
    fn an_absent_height_against_an_invalid_stored_default_is_refused() {
        // Resolution reads the level's own `defaultWallHeightMm`, which a
        // document written before that field was constrained — or one whose
        // validation errors have not been repaired — may violate. The refusal
        // names the LEVEL's field, not the caller's, so the message points at
        // what actually needs fixing.
        let (mut doc, id) = one_level();
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, DIMENSION_CAP_MM * 2.0] {
            doc.levels[0].default_wall_height_mm = bad;
            let err = resolve_wall_height(&doc, id, None)
                .expect_err("an invalid stored default cannot be resolved from");
            assert!(
                matches!(
                    err,
                    EditError::InvalidLevelField {
                        field: "defaultWallHeightMm",
                        ..
                    }
                ),
                "got {err:?} for {bad}"
            );
            // An EXPLICIT height still resolves: the broken default is only
            // consulted when the command supplied nothing.
            assert_eq!(
                resolve_wall_height(&doc, id, Some(2500.0)).expect("explicit"),
                2500.0
            );
        }
    }

    #[test]
    fn changing_a_default_emits_no_operation_for_existing_walls() {
        // The retroactive-edit check: a level default is an authoritative
        // source value, not an inherited one, so raising it must not rewrite
        // a single wall.
        let (mut doc, id) = one_level();
        doc.walls.push(Wall::new(
            id,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        ));
        let ops = set_level_operations(
            &doc,
            id,
            &LevelFields {
                default_wall_height_mm: Some(2700.0),
                ..Default::default()
            },
        )
        .expect("valid");
        let fields = fields_of(&ops);
        assert_eq!(fields.len(), 1);
        assert!(fields.contains_key("defaultWallHeightMm"));
        assert_eq!(
            ops[0].entity_id, id,
            "one operation, targeting the LEVEL and no wall"
        );
        assert_eq!(doc.walls[0].height_mm, 2500.0, "the wall is untouched");
    }
}

#[cfg(test)]
mod wall_tests {
    use super::*;
    use domowik_format::Opening;

    fn one_level_doc() -> (Document, Uuid) {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        (doc, level)
    }

    #[test]
    fn unchanged_endpoint_fields_do_not_resolve_legacy_crossings() {
        let (mut doc, level) = one_level_doc();
        doc.walls = vec![
            Wall::new(
                level,
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                200.0,
                2500.0,
            ),
            Wall::new(
                level,
                Point::new(2000.0, -1000.0),
                Point::new(2000.0, 1000.0),
                200.0,
                2500.0,
            ),
        ];
        let id = doc.walls[0].id;
        let edit = set_wall_operations(
            &doc,
            id,
            Some(Point::new(-0.0, 0.0)),
            Some(doc.walls[0].end()),
            None,
            Some(3000.0),
        )
        .unwrap();
        assert_eq!(
            edit.splits, 0,
            "unchanged axes must not induce incidental cuts"
        );
        assert_eq!(edit.joins, 0);
        for operation in &edit.operations {
            apply_preview(&mut doc, operation);
        }
        assert_eq!(doc.walls.len(), 2);
        assert_eq!(doc.wall(id).unwrap().height_mm, 3000.0);
        assert!(doc.joins.is_empty());
    }

    #[test]
    fn add_wall_rejects_full_axis_coincidence() {
        let (mut doc, level) = one_level_doc();
        let existing = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let existing_id = existing.id;
        doc.walls.push(existing);

        let new_wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let new_id = new_wall.id;
        let err = add_wall_operations(&doc, level, new_wall).expect_err("coincidence must reject");
        let (a, b) = (existing_id.min(new_id), existing_id.max(new_id));
        assert!(
            matches!(&err, EditError::WallContact(blocks)
                if blocks == &vec![WallContactBlock::WallCoincidence { wall: a, other: b }]),
            "expected WallCoincidence, got {err:?}"
        );
    }

    #[test]
    fn add_wall_rejects_collinear_overlap_without_a_spurious_split() {
        let (mut doc, level) = one_level_doc();
        let existing = Wall::new(
            level,
            Point::new(2000.0, 0.0),
            Point::new(10000.0, 0.0),
            200.0,
            2500.0,
        );
        let existing_id = existing.id;
        doc.walls.push(existing);

        // A classifier driven by bounding-box "strictly inside" alone would
        // see the existing wall's Start (2000,0) as landing inside the new
        // wall's [0,6000] span and induce a split there. It is a
        // positive-length COLLINEAR overlap — a reject class, never a split.
        let new_wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(6000.0, 0.0),
            200.0,
            2500.0,
        );
        let new_id = new_wall.id;
        let err = add_wall_operations(&doc, level, new_wall).expect_err("overlap must reject");
        let (a, b) = (existing_id.min(new_id), existing_id.max(new_id));
        assert!(
            matches!(&err, EditError::WallContact(blocks)
                if blocks == &vec![WallContactBlock::CollinearOverlap { wall: a, other: b }]),
            "expected CollinearOverlap and no split, got {err:?}"
        );
    }

    #[test]
    fn add_wall_still_splits_a_genuine_transversal_touch() {
        // Guards against over-correction: rejecting collinear touches must
        // not also stop a real endpoint-on-interior split (non-collinear
        // axes) from firing.
        let (mut doc, level) = one_level_doc();
        let spine = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let spine_id = spine.id;
        doc.walls.push(spine);

        let branch = Wall::new(
            level,
            Point::new(2000.0, 0.0),
            Point::new(2000.0, 3000.0),
            200.0,
            2500.0,
        );
        let edit = add_wall_operations(&doc, level, branch).expect("a T-junction is not refused");
        assert_eq!(
            edit.splits, 1,
            "the spine must still split at the T-junction"
        );
        assert!(
            edit.operations.iter().any(|op| op.entity_id == spine_id),
            "the spine's shortening must be part of the same batch"
        );
    }

    #[test]
    fn split_children_preserve_every_parent_field_via_clone() {
        let (mut doc, level) = one_level_doc();
        let mut parent = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        // A field this version does not interpret (FORMAT-SPEC §8 U2):
        // rebuilding the child field-by-field would silently drop it.
        parent
            .unknown
            .insert("futureField".to_owned(), serde_json::json!(42));
        let parent_id = parent.id;
        doc.walls.push(parent);

        let ops = split_operations(&doc, parent_id, &[Point::new(2000.0, 0.0)])
            .expect("a single interior cut is a valid split");
        let mut found_child = false;
        for op in &ops {
            if let OperationKind::CreateEntity {
                entity: Entity::Wall(w),
            } = &op.kind
            {
                assert_eq!(w.level_id, level);
                assert_eq!(w.thickness_mm, 200.0);
                assert_eq!(w.height_mm, 2500.0);
                assert_eq!(
                    w.unknown.get("futureField"),
                    Some(&serde_json::json!(42)),
                    "the child must carry the parent's unknown fields verbatim"
                );
                found_child = true;
            }
        }
        assert!(found_child, "expected at least one created child wall");
    }

    #[test]
    fn zero_length_split_child_is_refused_before_the_opening_remap_check() {
        let (mut doc, level) = one_level_doc();
        let wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let wall_id = wall.id;
        doc.walls.push(wall);
        // An aperture straddling the 1000mm cut: if this call ever reached
        // R4.3.3's remap it would refuse as `opening-split-ambiguous`
        // instead, which is exactly the ordering this test pins down.
        doc.openings.push(Opening::door(
            wall_id,
            1000.0 / 4000.0,
            200.0,
            2000.0,
            200.0,
        ));

        let cuts = [Point::new(1000.0, 0.0), Point::new(4000.0, 0.0)];
        let err = split_operations(&doc, wall_id, &cuts).expect_err("a zero-length last child");
        assert!(
            matches!(err, EditError::ZeroLengthSplitChild { wall, .. } if wall == wall_id),
            "expected ZeroLengthSplitChild ahead of any opening-remap error, got {err:?}"
        );
    }

    #[test]
    fn set_wall_reclassifies_every_co_moved_neighbors_final_axis() {
        let (mut doc, level) = one_level_doc();
        let a = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        );
        let a_id = a.id;
        // Only B's Start moves with the node; its End is unaffected, so it
        // must already sit where B's FINAL axis needs it — on D's line.
        let b = Wall::new(
            level,
            Point::new(1000.0, 0.0),
            Point::new(1500.0, 2000.0),
            200.0,
            2500.0,
        );
        let b_id = b.id;
        let d = Wall::new(
            level,
            Point::new(1500.0, 0.0),
            Point::new(1500.0, 2000.0),
            200.0,
            2500.0,
        );
        let d_id = d.id;
        doc.walls.push(a);
        doc.walls.push(b);
        doc.walls.push(d);
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(a_id, Role::End),
            JoinMember::new(b_id, Role::Start),
        ]));

        // Moving A's End drags the joined node — and so B's Start — with it
        // (R3.4.2). A's own final axis never touches D; only B's does. A
        // check scoped to `a_id` alone would miss this refusal entirely.
        let err = set_wall_operations(&doc, a_id, None, Some(Point::new(1500.0, 0.0)), None, None)
            .expect_err("B's co-moved axis must be reclassified against D");
        let (lo, hi) = (b_id.min(d_id), b_id.max(d_id));
        assert!(
            matches!(&err, EditError::WallContact(blocks)
                if blocks == &vec![WallContactBlock::WallCoincidence { wall: lo, other: hi }]),
            "expected B/D coincidence, got {err:?}"
        );
    }

    #[test]
    fn set_wall_move_across_a_crossing_splits_both_into_four_children() {
        // Root review: `set_wall_operations` must induce splits exactly as
        // `add_wall_operations` does — moving a wall INTO a crossing is not
        // different from drawing one there.
        let (mut doc, level) = one_level_doc();
        let a = Wall::new(
            level,
            Point::new(0.0, 5000.0),
            Point::new(4000.0, 5000.0),
            200.0,
            2500.0,
        );
        let a_id = a.id;
        let b = Wall::new(
            level,
            Point::new(2000.0, -3000.0),
            Point::new(2000.0, 3000.0),
            200.0,
            2500.0,
        );
        doc.walls.push(a);
        doc.walls.push(b);

        let edit = set_wall_operations(
            &doc,
            a_id,
            Some(Point::new(0.0, 0.0)),
            Some(Point::new(4000.0, 0.0)),
            None,
            None,
        )
        .expect("a proper crossing is resolved, not refused");
        assert_eq!(edit.splits, 2, "both A and B split at the crossing");

        let mut scratch = doc.clone();
        for op in &edit.operations {
            apply_preview(&mut scratch, op);
        }
        assert_eq!(
            scratch.walls_on_level(level).count(),
            4,
            "a proper crossing must leave four wall entities, not the original two"
        );
    }

    #[test]
    fn set_wall_move_drags_a_co_moved_neighbor_into_a_crossing_it_must_still_split() {
        let (mut doc, level) = one_level_doc();
        let a = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        );
        let a_id = a.id;
        let b = Wall::new(
            level,
            Point::new(1000.0, 0.0),
            Point::new(1000.0, 5000.0),
            200.0,
            2500.0,
        );
        let b_id = b.id;
        let c = Wall::new(
            level,
            Point::new(0.0, 2000.0),
            Point::new(2000.0, 2000.0),
            200.0,
            2500.0,
        );
        doc.walls.push(a);
        doc.walls.push(b);
        doc.walls.push(c);
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(a_id, Role::End),
            JoinMember::new(b_id, Role::Start),
        ]));

        // Moving A's End drags B's Start with it (R3.4.2). A's own final
        // axis never reaches C — only the CO-MOVED B's does — so a check
        // scoped to the named wall alone would miss this split entirely.
        let edit = set_wall_operations(&doc, a_id, None, Some(Point::new(500.0, 0.0)), None, None)
            .expect("B's crossing with C must be resolved, not missed");
        assert_eq!(
            edit.splits, 2,
            "B and C must both split at the crossing B is dragged into"
        );

        let mut scratch = doc.clone();
        for op in &edit.operations {
            apply_preview(&mut scratch, op);
        }
        // A is shortened but not itself split; B and C each split in two.
        assert_eq!(scratch.walls_on_level(level).count(), 5);
    }

    #[test]
    fn set_wall_move_onto_an_existing_join_merges_into_it() {
        let (mut doc, level) = one_level_doc();
        let x = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        );
        let x_id = x.id;
        let y = Wall::new(
            level,
            Point::new(1000.0, 0.0),
            Point::new(1000.0, 1000.0),
            200.0,
            2500.0,
        );
        let y_id = y.id;
        let z = Wall::new(
            level,
            Point::new(2000.0, 0.0),
            Point::new(3000.0, 0.0),
            200.0,
            2500.0,
        );
        let z_id = z.id;
        doc.walls.push(x);
        doc.walls.push(y);
        doc.walls.push(z);
        let join = WallJoin::new(vec![
            JoinMember::new(x_id, Role::End),
            JoinMember::new(y_id, Role::Start),
        ]);
        let join_id = join.id;
        doc.joins.push(join);

        // Z moves onto a point an existing join ALREADY occupies: R3.1.2
        // says this merges into that join — cardinality grows — rather than
        // minting a second join at the same point.
        let edit = set_wall_operations(&doc, z_id, Some(Point::new(1000.0, 0.0)), None, None, None)
            .expect("landing on an occupied join point merges into it");
        assert_eq!(edit.joins, 1);
        let merge = edit
            .operations
            .iter()
            .find(|op| op.entity_id == join_id)
            .expect("the EXISTING join must be extended, not replaced by a new one");
        match &merge.kind {
            OperationKind::SetFields { fields } => {
                let members = fields.get("members").expect("members field");
                assert_eq!(
                    members.as_array().unwrap().len(),
                    3,
                    "cardinality grows to three members"
                );
            }
            other => panic!("expected setFields on the existing join, got {other:?}"),
        }
    }

    #[test]
    fn add_wall_reports_coincidence_ahead_of_an_opening_split_ambiguity_it_never_reaches() {
        // Precedence (R3.1.2): reject classes are checked BEFORE any contact
        // class is resolved. D coincides with the new wall outright; E would
        // only become ambiguous if a split were attempted against it — which
        // must never happen once the coincidence is found.
        let (mut doc, level) = one_level_doc();
        let d = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let d_id = d.id;
        let e = Wall::new(
            level,
            Point::new(2000.0, -2000.0),
            Point::new(2000.0, 2000.0),
            200.0,
            2500.0,
        );
        let e_id = e.id;
        doc.walls.push(d);
        doc.walls.push(e);
        // Straddles E's midpoint (2000,0), exactly where the new wall's
        // proper crossing would split E were it ever attempted.
        doc.openings
            .push(Opening::door(e_id, 0.5, 400.0, 1800.0, 200.0));

        let new_wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let new_id = new_wall.id;
        let err = add_wall_operations(&doc, level, new_wall)
            .expect_err("the coincidence with D must win precedence");
        let (a, b) = (d_id.min(new_id), d_id.max(new_id));
        assert!(
            matches!(&err, EditError::WallContact(blocks)
                if blocks == &vec![WallContactBlock::WallCoincidence { wall: a, other: b }]),
            "expected the coincidence refusal, checked before any split is attempted; got {err:?}"
        );
    }

    // ---- ED-0119: contact points are topology points ----

    fn draw(doc: &mut Document, level: Uuid, a: (f64, f64), b: (f64, f64)) -> Result<WallEdit> {
        let wall = Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        let edit = add_wall_operations(doc, level, wall)?;
        for op in &edit.operations {
            apply_preview(doc, op);
        }
        Ok(edit)
    }

    /// Valid, and nothing a newly admitted batch could not have produced.
    fn assert_resolved(doc: &Document) {
        assert_eq!(doc.validate(), vec![], "the result validates");
        assert_eq!(
            wall_contact_refusals(&Document::new(), doc),
            vec![],
            "no refusing or unresolved contact remains"
        );
    }

    /// The reproduction that reopened 0055: two walls cross at the
    /// non-dyadic point (21000/17, 7000/17) and are split there, rounded; a
    /// third wall passes exactly through that point. It used to be refused
    /// with `zero-length-split-child`.
    #[test]
    fn a_wall_through_an_existing_crossing_merges_into_its_join() {
        let (mut doc, level) = one_level_doc();
        draw(&mut doc, level, (0.0, 0.0), (3000.0, 1000.0)).unwrap();
        draw(&mut doc, level, (0.0, 700.0), (3000.0, 0.0)).unwrap();
        let node = doc.walls[0].end();
        let edit = draw(&mut doc, level, (0.0, 1000.0), (21000.0, -9000.0))
            .expect("a wall through a node joins it");
        assert_eq!(edit.splits, 1, "only the new wall splits, at the node");
        assert_eq!(doc.walls.len(), 6);
        let join = doc.joins.iter().find(|j| j.members.len() == 6);
        assert!(
            join.is_some(),
            "one join holds all six ends: {:?}",
            doc.joins
        );
        let cell = topology_cell(node);
        assert!(doc
            .walls
            .iter()
            .all(|w| topology_cell(w.start()) == cell || topology_cell(w.end()) == cell));
        assert_resolved(&doc);
    }

    #[test]
    fn an_end_a_hair_from_a_corner_joins_it_and_the_next_cell_does_not() {
        for (x, joins) in [(3999.9999, 1), (4000.0004, 1), (4000.0009, 0)] {
            let (mut doc, level) = one_level_doc();
            draw(&mut doc, level, (0.0, 0.0), (4000.0, 0.0)).unwrap();
            let edit =
                draw(&mut doc, level, (x, 0.0), (x, 3000.0)).unwrap_or_else(|e| panic!("{x}: {e}"));
            assert_eq!(edit.splits, 0, "{x}: no micro-child is split off");
            assert_eq!(edit.joins, joins, "{x}");
            assert_eq!(doc.walls.len(), 2);
            assert_resolved(&doc);
        }
    }

    #[test]
    fn a_crossing_in_one_walls_end_cell_splits_only_the_other_on_its_axis() {
        let (mut doc, level) = one_level_doc();
        draw(&mut doc, level, (0.0, 0.0), (4000.0, 0.0)).unwrap();
        let edit = draw(&mut doc, level, (3999.9999, -1000.0), (3999.9999, 1000.0)).unwrap();
        assert_eq!((edit.splits, edit.joins), (1, 1));
        let end = Point::new(4000.0, 0.0);
        assert_eq!(doc.walls[0].end(), end, "the host is not split");
        // The crossing wall splits at the crossing, on its own axis, which
        // shares the host end's cell: all three ends join there.
        let at = Point::new(3999.9999, 0.0);
        assert!(doc.walls[1..]
            .iter()
            .all(|w| w.start().x == at.x && w.end().x == at.x));
        let join = doc.joins.iter().find(|j| j.members.len() == 3);
        assert!(join.is_some(), "{:?}", doc.joins);
        assert_resolved(&doc);
    }

    #[test]
    fn a_crossing_in_the_cells_of_both_ends_is_a_corner() {
        let (mut doc, level) = one_level_doc();
        draw(&mut doc, level, (0.0, 0.0), (4000.0, 0.0)).unwrap();
        let edit = draw(&mut doc, level, (3999.9998, 0.0001), (4000.0001, -1000.0)).unwrap();
        assert_eq!((edit.splits, edit.joins), (0, 1));
        assert_resolved(&doc);
    }

    /// The builder never emits a batch session admission refuses for a
    /// contact or rehosting reason.
    ///
    /// Random gestures over a grid with irregular and sub-micrometre offsets:
    /// new walls, endpoint moves (which co-move joined neighbours) and doors.
    /// That produces crossings at non-binary64 points, walls through nodes,
    /// ends a fraction of a cell from corners, ends on interiors, collinear
    /// refusals, and splits of door-hosting walls. Every batch the builder
    /// accepts must commit, unless a move leaves doors that no longer fit
    /// their host (R3.1.8: R4.4 fit is admission's check) or collapses a wall
    /// to a point. The builder may refuse only
    /// for an axis refusal or an R4.3.3 door it cannot rehost. Undo and redo
    /// of accepted batches are admitted too.
    #[test]
    fn random_gestures_only_produce_batches_admission_accepts() {
        use domowik_document::session::Session;
        let (mut accepted, mut refused) = (0, 0);
        for seed in 1..=8u64 {
            let mut state = seed.wrapping_mul(0x2545_f491_4f6c_dd1d);
            let mut next = move |n: u64| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state % n
            };
            let mut session = Session::open_snapshot(
                Uuid::now_v7(),
                Document::with_ground_floor(),
                &Default::default(),
                &[],
                vec![],
            )
            .unwrap();
            let level = session.document().levels[0].id;
            for _ in 0..40 {
                let mut coord = || {
                    let base = (next(6) * 1000) as f64;
                    match next(4) {
                        0 => base + (next(1000) as f64) / 7.0,
                        // A fraction of a topology cell off the grid.
                        1 => base + (next(9) as f64 - 4.0) * 0.0001,
                        _ => base,
                    }
                };
                let (a, b) = (Point::new(coord(), coord()), Point::new(coord(), coord()));
                let doc = session.document().clone();
                let pick = next(5);
                let batch = match pick {
                    0 | 1 if !doc.walls.is_empty() => {
                        let wall = &doc.walls[next(doc.walls.len() as u64) as usize];
                        if next(2) == 0 {
                            set_wall_operations(&doc, wall.id, Some(a), None, None, None)
                        } else {
                            set_wall_operations(&doc, wall.id, None, Some(b), None, None)
                        }
                        .map(|e| e.operations)
                    }
                    2 if !doc.walls.is_empty() => {
                        let wall = &doc.walls[next(doc.walls.len() as u64) as usize];
                        if wall.length_mm() < 2000.0 {
                            continue;
                        }
                        let door =
                            domowik_format::Opening::door(wall.id, 0.5, 700.0, 2000.0, 120.0);
                        Ok(vec![Operation::create(Entity::Opening(door))])
                    }
                    _ if a != b => {
                        add_wall_operations(&doc, level, Wall::new(level, a, b, 120.0, 2500.0))
                            .map(|e| e.operations)
                    }
                    _ => continue,
                };
                match batch {
                    // A move to where the end already is: nothing to commit.
                    Ok(ops) if ops.is_empty() => {}
                    Ok(ops) => {
                        // A door may overlap one already there; only the
                        // builder's own batches are under test.
                        let door = matches!(
                            ops.first().map(|o| &o.kind),
                            Some(OperationKind::CreateEntity {
                                entity: Entity::Opening(_)
                            })
                        );
                        match session.commit(ops) {
                            // R3.1.8: a move keeps a door's parameters and
                            // admission checks R4.4 fit at final positions;
                            // the builder does not pre-validate fit.
                            // A move straight onto the wall's other end asks
                            // for a zero-length wall, which validation refuses.
                            Err(e)
                                if [
                                    "aperture-not-strict-fit",
                                    "opening-overlap",
                                    "start and end are the same point",
                                ]
                                .iter()
                                .any(|c| e.to_string().contains(c)) => {}
                            Ok(_) => {
                                accepted += 1;
                                // Undo and redo are new admissions of the
                                // inverse and of the batch again.
                                if next(3) == 0 {
                                    let done = session.document().clone();
                                    session.undo().expect("undo is admitted").expect("an entry");
                                    assert_eq!(
                                        canonical(session.document()),
                                        canonical(&doc),
                                        "seed {seed}: undo restores"
                                    );
                                    session.redo().expect("redo is admitted").expect("an entry");
                                    assert_eq!(
                                        canonical(session.document()),
                                        canonical(&done),
                                        "seed {seed}: redo reapplies"
                                    );
                                }
                            }
                            Err(_) if door => {}
                            Err(e) => {
                                panic!("seed {seed}: gesture {pick}: builder batch refused: {e}")
                            }
                        }
                    }
                    Err(EditError::WallContact(blocks)) => {
                        // An axis refusal leads. A wall moved onto a chain of
                        // pieces that matches it to within rounding is caught
                        // only after splitting, when the pieces coincide; the
                        // refusal then also lists crossings left unsplit.
                        // An axis refusal leads: a wall moved onto a chain
                        // of pieces that matches it to within rounding is
                        // caught only after splitting, when the pieces
                        // coincide, and the refusal may list crossings too.
                        assert!(
                            blocks[0].is_axis_refusal(),
                            "seed {seed}: an axis refusal must lead: {blocks:?}"
                        );
                        refused += 1;
                    }
                    Err(EditError::OpeningSplitAmbiguous(_)) => refused += 1,
                    Err(e) => panic!("seed {seed}: unexpected refusal: {e}"),
                }
            }
            assert_resolved(session.document());
        }
        assert!(
            accepted > 80 && refused > 5,
            "{accepted} accepted, {refused} refused"
        );
    }

    /// A document with its collections in id order. Undo re-creates a deleted
    /// entity at the end of its collection, and collection order carries no
    /// meaning (FORMAT-SPEC: entities are identified by id).
    fn canonical(doc: &Document) -> Document {
        let mut doc = doc.clone();
        doc.walls.sort_by_key(|w| w.id);
        doc.joins.sort_by_key(|j| j.id);
        for join in &mut doc.joins {
            join.members
                .sort_by_key(|m| (m.wall_id, m.role == Role::End));
        }
        doc.openings.sort_by_key(|o| o.id);
        doc
    }

    /// Review findings (Codex GPT-6.1-Sol, 0055 round one), each the state
    /// that failed, now resolved by the builder and committed by a session.
    fn commits(doc: &Document, ops: Vec<Operation>) {
        let mut session = domowik_document::session::Session::open_snapshot(
            Uuid::now_v7(),
            doc.clone(),
            &Default::default(),
            &[],
            vec![],
        )
        .unwrap();
        session
            .commit(ops)
            .expect("the builder's batch is admitted");
        assert_resolved(session.document());
    }

    /// Moving a door-hosting wall so that it crosses another wall moves the
    /// host (keeping the door's `u`) and splits it in one batch, the door
    /// remapping to the piece that holds it. Admission used to verify the
    /// remap against the host before the move and refused it
    /// (`opening-direct-rehost`), on `main` as well.
    #[test]
    fn moving_a_door_host_across_a_wall_commits_and_undoes() {
        let (mut doc, level) = one_level_doc();
        let host = Wall::new(
            level,
            Point::new(2500.0, 0.0),
            Point::new(4000.0, 0.0),
            120.0,
            2500.0,
        );
        let host_id = host.id;
        doc.openings
            .push(Opening::door(host_id, 0.6, 600.0, 2100.0, 120.0));
        doc.walls.push(host);
        draw(&mut doc, level, (2000.0, -1000.0), (2000.0, 1000.0)).unwrap();
        // Start moved: the door lands on the far piece. End moved past the
        // crossing, reversing nothing: the door lands on the near piece.
        for (start, end) in [
            (Some(Point::new(1000.0, 0.0)), None),
            (None, Some(Point::new(500.0, 0.0))),
        ] {
            let mut session = domowik_document::session::Session::open_snapshot(
                Uuid::now_v7(),
                doc.clone(),
                &Default::default(),
                &[],
                vec![],
            )
            .unwrap();
            let edit = set_wall_operations(&doc, host_id, start, end, None, None).unwrap();
            assert!(edit.splits >= 1);
            session
                .commit(edit.operations)
                .expect("move and split are admitted");
            let moved = session.document().clone();
            assert_ne!(
                moved.openings[0].host_wall_id, host_id,
                "the door moved to a piece"
            );
            session.undo().unwrap().unwrap();
            assert_eq!(session.document(), &doc);
            session.redo().unwrap().unwrap();
            assert_eq!(session.document(), &moved);
        }
    }

    #[test]
    fn a_wall_too_short_to_leave_its_cell_never_joins_itself() {
        let (doc, level) = one_level_doc();
        let wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(0.0001, 0.0),
            120.0,
            2500.0,
        );
        let edit = add_wall_operations(&doc, level, wall).unwrap();
        assert_eq!((edit.operations.len(), edit.joins), (1, 0));
        commits(&doc, edit.operations);
    }

    #[test]
    fn moving_one_join_onto_another_merges_them() {
        let (mut doc, level) = one_level_doc();
        draw(&mut doc, level, (-1000.0, 0.0), (0.0, 0.0)).unwrap();
        draw(&mut doc, level, (0.0, 0.0), (0.0, 1000.0)).unwrap();
        draw(&mut doc, level, (1000.0, 0.0), (2000.0, 0.0)).unwrap();
        draw(&mut doc, level, (1000.0, 0.0), (1000.0, -1000.0)).unwrap();
        assert_eq!(doc.joins.len(), 2);
        let a = doc.walls[0].id;
        let edit =
            set_wall_operations(&doc, a, None, Some(Point::new(1000.0, 0.0)), None, None).unwrap();
        let mut after = doc.clone();
        for op in &edit.operations {
            apply_preview(&mut after, op);
        }
        assert_eq!(after.joins.len(), 1, "one survivor");
        assert_eq!(after.joins[0].members.len(), 4);
        commits(&doc, edit.operations);
    }

    #[test]
    fn a_crossing_beside_a_door_splits_the_host_on_its_axis() {
        let (mut doc, level) = one_level_doc();
        let host = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            120.0,
            2500.0,
        );
        doc.openings
            .push(Opening::door(host.id, 0.75, 500.0, 2100.0, 120.0));
        doc.walls.push(host);
        let wall = Wall::new(
            level,
            Point::new(2000.0001, 0.0001),
            Point::new(2000.0001, -1000.0),
            120.0,
            2500.0,
        );
        let edit = add_wall_operations(&doc, level, wall).unwrap();
        commits(&doc, edit.operations);
    }

    #[test]
    fn splitting_a_host_resolves_the_contacts_its_pieces_carry() {
        // A sub-cell neighbour beside the host, and a legacy crossing on it.
        let (mut doc, level) = one_level_doc();
        draw(&mut doc, level, (0.0, 0.0), (4000.0, 0.0)).unwrap();
        draw(&mut doc, level, (1000.0, 0.00005), (1500.0, 0.00005)).unwrap();
        let wall = Wall::new(
            level,
            Point::new(2000.0001, 0.0001),
            Point::new(2000.0001, -1000.0),
            120.0,
            2500.0,
        );
        commits(
            &doc,
            add_wall_operations(&doc, level, wall).unwrap().operations,
        );

        let (mut doc, level) = one_level_doc();
        doc.walls = vec![
            Wall::new(
                level,
                Point::new(0.0, 0.0),
                Point::new(4000.0, 0.0),
                120.0,
                2500.0,
            ),
            Wall::new(
                level,
                Point::new(1000.0, -1000.0),
                Point::new(1000.0, 1000.0),
                120.0,
                2500.0,
            ),
        ];
        let tee = Wall::new(
            level,
            Point::new(3000.0, 0.0),
            Point::new(3000.0, 1000.0),
            120.0,
            2500.0,
        );
        let edit = add_wall_operations(&doc, level, tee).unwrap();
        assert_eq!(
            edit.splits, 3,
            "the host at the tee and at the legacy crossing, and the crossing wall"
        );
        commits(&doc, edit.operations);
    }

    #[test]
    fn an_unchanged_property_form_is_as_neutral_as_omitting_coordinates() {
        let (mut doc, level) = one_level_doc();
        let a = Wall::new(
            level,
            Point::new(-2000.0, 0.0),
            Point::new(0.0, 0.0),
            120.0,
            2500.0,
        );
        // Joined to `a` in one cell, stored a fraction of a cell away.
        let b = Wall::new(
            level,
            Point::new(0.0004, 0.0004),
            Point::new(0.0004, 2000.0),
            120.0,
            2500.0,
        );
        // A legacy unsplit crossing of `b`, through a door.
        let c = Wall::new(
            level,
            Point::new(-1000.0, 1000.0),
            Point::new(1000.0, 1000.0),
            120.0,
            2500.0,
        );
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(a.id, Role::End),
            JoinMember::new(b.id, Role::Start),
        ]));
        doc.openings
            .push(Opening::door(c.id, 0.5, 500.0, 2100.0, 120.0));
        let (id, start, end) = (a.id, a.start(), a.end());
        doc.walls = vec![a, b, c];
        assert_eq!(doc.validate(), vec![]);
        let edit =
            set_wall_operations(&doc, id, Some(start), Some(end), Some(180.0), None).unwrap();
        assert_eq!(edit.operations.len(), 1, "only the thickness is written");
    }
}

#[cfg(test)]
mod room_tests {
    use super::*;
    use domowik_format::Level;

    fn one_level() -> (Document, Uuid) {
        let doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        (doc, level)
    }

    fn seeded(level: Uuid) -> RoomRecord {
        let mut r = RoomRecord::new(Uuid::from_u128(1), level, Point::new(2000.0, 1500.0));
        r.name = Some("Living".to_owned());
        r
    }

    /// A one-level document already holding the F25 record `R1`.
    fn with_record() -> (Document, Uuid, Uuid) {
        let (mut doc, level) = one_level();
        let ops = create_room_operations(&doc, seeded(level)).expect("create");
        apply(&mut doc, &ops);
        (doc, level, Uuid::from_u128(1))
    }

    /// Applies operations to a document, the way replay would.
    fn apply(doc: &mut Document, ops: &[Operation]) {
        for op in ops {
            match &op.kind {
                OperationKind::CreateEntity { entity } => doc.insert_entity(entity.clone()),
                OperationKind::DeleteEntity => {
                    doc.remove_entity(op.entity_id);
                }
                OperationKind::SetFields { .. } => apply_preview(doc, op),
            }
        }
    }

    fn payload(ops: &[Operation]) -> serde_json::Map<String, serde_json::Value> {
        match &ops[0].kind {
            OperationKind::SetFields { fields } => fields.clone(),
            other => panic!("expected setFields, got {other:?}"),
        }
    }

    #[test]
    fn f25_create_emits_exactly_one_operation() {
        let (doc, level) = one_level();
        let ops = create_room_operations(&doc, seeded(level)).expect("accepted");
        assert_eq!(ops.len(), 1, "one atomic batch, no incidental effects");
        assert!(matches!(
            &ops[0].kind,
            OperationKind::CreateEntity {
                entity: Entity::RoomRecord(_)
            }
        ));
    }

    #[test]
    fn f37a_a_supplied_id_already_in_use_rejects_with_an_empty_batch() {
        let (doc, level, _r1) = with_record();
        // The §2.7 pattern: a collision is invalid INPUT, not a request to
        // mint a different id.
        let clash = RoomRecord::new(Uuid::from_u128(1), level, Point::new(2000.0, 1500.0));
        let err = create_room_operations(&doc, clash).expect_err("must reject");
        assert!(
            matches!(err, EditError::RoomIdInUse { id, kind } if id == Uuid::from_u128(1) && kind == "roomRecord"),
            "got {err:?}"
        );
        assert_eq!(doc.room_records.len(), 1, "no state change");
    }

    #[test]
    fn a_supplied_id_belonging_to_another_entity_kind_also_rejects() {
        // "unused anywhere in the document" is not "unused by another room".
        let (doc, level) = one_level();
        let clash = RoomRecord::new(level, level, Point::new(2000.0, 1500.0));
        let err = create_room_operations(&doc, clash).expect_err("must reject");
        assert!(
            matches!(err, EditError::RoomIdInUse { kind, .. } if kind == "level"),
            "got {err:?}"
        );
    }

    #[test]
    fn f32_seed_location_never_rejects() {
        // In a wall, on a boundary, in the exterior, at the domain edge: all
        // legal creates. Only the status differs, and status is projection.
        let (doc, level) = one_level();
        for seed in [
            Point::new(2000.0, 0.0),
            Point::new(2000.0, 100.0),
            Point::new(10_000.0, 1500.0),
            Point::new(COORD_BOUND_MM, -COORD_BOUND_MM),
        ] {
            let record = RoomRecord::new(Uuid::now_v7(), level, seed);
            assert!(
                create_room_operations(&doc, record).is_ok(),
                "seed {seed:?} must be accepted"
            );
        }
    }

    #[test]
    fn f37h_a_non_finite_seed_rejects_on_create_and_on_update() {
        let (doc, level) = one_level();
        for bad in [f64::NAN, f64::INFINITY] {
            let record = RoomRecord::new(Uuid::now_v7(), level, Point::new(bad, 0.0));
            assert!(
                matches!(
                    create_room_operations(&doc, record).expect_err("reject"),
                    EditError::RoomSeedNotFinite { .. }
                ),
                "create must refuse {bad} as non-finite"
            );
        }
        // FINITE but out of domain is a DIFFERENT refusal. Calling this one
        // "nonfinite-coordinate" told its author the wrong thing.
        let far = RoomRecord::new(Uuid::now_v7(), level, Point::new(COORD_BOUND_MM * 2.0, 0.0));
        assert!(
            matches!(
                create_room_operations(&doc, far).expect_err("reject"),
                EditError::RoomSeedOutOfRange { .. }
            ),
            "a finite out-of-domain seed must not be called non-finite"
        );
        let (doc, _level, r1) = with_record();
        let fields = RoomFields {
            seed: Some(Point::new(f64::NAN, 0.0)),
            ..RoomFields::default()
        };
        let err = update_room_operations(&doc, r1, &fields).expect_err("reject");
        assert!(
            matches!(err, EditError::RoomSeedNotFinite { .. }),
            "{err:?}"
        );
    }

    #[test]
    fn f37g_an_empty_style_reference_rejects_but_null_clears() {
        let (doc, level) = one_level();
        let mut record = seeded(level);
        record.ceiling_style_ref = Some(String::new());
        assert!(matches!(
            create_room_operations(&doc, record).expect_err("reject"),
            EditError::InvalidStyleReference {
                field: "ceilingStyleRef"
            }
        ));

        let (doc, _level, id) = with_record();
        let empty = RoomFields {
            ceiling_style_ref: FieldEdit::Set(String::new()),
            ..RoomFields::default()
        };
        assert!(matches!(
            update_room_operations(&doc, id, &empty).expect_err("reject"),
            EditError::InvalidStyleReference { .. }
        ));

        // Clearing an ABSENT reference is a no-op, not a rejection.
        let clear = RoomFields {
            ceiling_style_ref: FieldEdit::Clear,
            ..RoomFields::default()
        };
        assert_eq!(
            update_room_operations(&doc, id, &clear).expect("ok").len(),
            0
        );
    }

    #[test]
    fn f37def_name_set_empty_and_clear_are_three_distinct_edits() {
        let (mut doc, _level, id) = with_record();

        let set = |value: FieldEdit<String>| RoomFields {
            name: value,
            ..RoomFields::default()
        };

        let ops = update_room_operations(&doc, id, &set(FieldEdit::Set("Den".to_owned())))
            .expect("accepted");
        assert_eq!(payload(&ops)["name"], serde_json::json!("Den"));
        apply(&mut doc, &ops);
        assert_eq!(doc.room_record(id).expect("live").display_name(), "Den");

        // The empty string is an authoritative VALUE, and it displays as the
        // fallback without becoming absent.
        let ops = update_room_operations(&doc, id, &set(FieldEdit::Set(String::new())))
            .expect("accepted");
        assert_eq!(payload(&ops)["name"], serde_json::json!(""));
        apply(&mut doc, &ops);
        let record = doc.room_record(id).expect("live");
        assert_eq!(record.name.as_deref(), Some(""), "present, and empty");
        assert_eq!(record.display_name(), "Room");

        // Clearing is a DIFFERENT edit reaching a DIFFERENT state that looks
        // identical on screen (F37d).
        let ops = update_room_operations(&doc, id, &set(FieldEdit::Clear)).expect("accepted");
        assert_eq!(payload(&ops)["name"], serde_json::Value::Null);
        apply(&mut doc, &ops);
        let record = doc.room_record(id).expect("live");
        assert_eq!(record.name, None, "absent, not empty");
        assert_eq!(record.display_name(), "Room");
    }

    #[test]
    fn an_update_that_asks_for_nothing_new_emits_nothing() {
        let (doc, level, id) = with_record();
        let same = RoomFields {
            seed: Some(Point::new(2000.0, 1500.0)),
            name: FieldEdit::Set("Living".to_owned()),
            level_id: Some(level),
            ..RoomFields::default()
        };
        assert!(
            update_room_operations(&doc, id, &same)
                .expect("accepted")
                .is_empty(),
            "already in that state"
        );
    }

    #[test]
    fn f37i_a_level_move_is_one_operation_and_the_target_must_exist() {
        let (mut doc, _level, id) = with_record();
        let l2 = Level::new("L2");
        let l2_id = l2.id;
        doc.levels.push(l2);

        let move_to = |target| RoomFields {
            level_id: Some(target),
            ..RoomFields::default()
        };
        let ops = update_room_operations(&doc, id, &move_to(l2_id)).expect("accepted");
        assert_eq!(ops.len(), 1, "ONE batch reprojects both levels");
        assert_eq!(payload(&ops)["levelId"], serde_json::json!(l2_id));

        let unknown = Uuid::now_v7();
        assert!(matches!(
            update_room_operations(&doc, id, &move_to(unknown)).expect_err("reject"),
            EditError::UnknownLevel(_)
        ));
    }

    #[test]
    fn an_unknown_target_rejects_deterministically_for_update_and_delete() {
        let (doc, _level) = one_level();
        let ghost = Uuid::now_v7();
        assert!(matches!(
            update_room_operations(&doc, ghost, &RoomFields::default()).expect_err("reject"),
            EditError::UnknownRoom(id) if id == ghost
        ));
        assert!(matches!(
            delete_room_operations(&doc, ghost).expect_err("reject"),
            EditError::UnknownRoom(id) if id == ghost
        ));
    }

    #[test]
    fn f37j_delete_is_exactly_one_operation_and_cascades_nothing() {
        let (mut doc, _level, id) = with_record();
        let before = doc.levels.len();
        let ops = delete_room_operations(&doc, id).expect("accepted");
        assert_eq!(ops.len(), 1);
        assert!(matches!(ops[0].kind, OperationKind::DeleteEntity));
        apply(&mut doc, &ops);
        assert!(doc.room_records.is_empty());
        assert_eq!(doc.levels.len(), before, "nothing cascaded");
    }

    #[test]
    fn create_refuses_an_unknown_level() {
        let (doc, _level) = one_level();
        let record = RoomRecord::new(Uuid::now_v7(), Uuid::now_v7(), Point::new(0.0, 0.0));
        assert!(matches!(
            create_room_operations(&doc, record).expect_err("reject"),
            EditError::UnknownLevel(_)
        ));
    }

    // ---------------------------------------------------------------------
    // Scrutiny the advisory round did not supply.
    //
    // The required Codex review returned no findings — it ran out of workspace
    // credits and exited 0 — so these are the two questions it was asked about
    // this module, answered by construction rather than by argument.
    // ---------------------------------------------------------------------

    /// Every combination of stored state and requested edit, checked against
    /// what the payload must contain.
    ///
    /// The omission of unchanged fields is the risk: it is what keeps a
    /// no-op command from growing the log, and it is also the mechanism that
    /// would silently drop a real edit if "unchanged" were computed against
    /// the wrong thing. Comparing a `String` instead of an `Option<String>`
    /// would make "set to the empty string" look unchanged when the name is
    /// ABSENT, and F37d says those are different states.
    #[test]
    fn the_settable_field_matrix_omits_only_what_is_genuinely_unchanged() {
        let stored: [Option<&str>; 3] = [None, Some(""), Some("Salon")];
        let edits: [FieldEdit<String>; 4] = [
            FieldEdit::Keep,
            FieldEdit::Clear,
            FieldEdit::Set(String::new()),
            FieldEdit::Set("Salon".to_owned()),
        ];
        for current in stored {
            for edit in edits.clone() {
                let (mut doc, level) = one_level();
                let mut record =
                    RoomRecord::new(Uuid::from_u128(1), level, Point::new(2000.0, 1500.0));
                record.name = current.map(str::to_owned);
                let id = record.id;
                apply(&mut doc, &[Operation::create(Entity::RoomRecord(record))]);

                let fields = RoomFields {
                    name: edit.clone(),
                    ..RoomFields::default()
                };
                let ops = update_room_operations(&doc, id, &fields).expect("accepted");

                // What the payload SHOULD hold, derived from the rule rather
                // than from the implementation.
                let expected = match &edit {
                    FieldEdit::Keep => None,
                    FieldEdit::Clear if current.is_none() => None,
                    FieldEdit::Clear => Some(serde_json::Value::Null),
                    FieldEdit::Set(v) if current == Some(v.as_str()) => None,
                    FieldEdit::Set(v) => Some(serde_json::json!(v)),
                };
                match expected {
                    None => assert!(
                        ops.is_empty(),
                        "stored {current:?} + {edit:?} must emit nothing"
                    ),
                    Some(value) => {
                        assert_eq!(
                            payload(&ops)["name"],
                            value,
                            "stored {current:?} + {edit:?}"
                        );
                    }
                }

                // And applying it reaches the state the edit asked for.
                apply(&mut doc, &ops);
                let after = doc.room_record(id).expect("live").name.clone();
                let wanted = match &edit {
                    FieldEdit::Keep => current.map(str::to_owned),
                    FieldEdit::Clear => None,
                    FieldEdit::Set(v) => Some(v.clone()),
                };
                assert_eq!(after, wanted, "stored {current:?} + {edit:?}");
            }
        }
    }

    /// A seed asked to move to where it already is emits nothing, and a seed
    /// asked to move anywhere else emits the move — including to a position
    /// that differs only in its last representable step.
    #[test]
    fn a_seed_edit_is_omitted_only_when_the_coordinate_is_identical() {
        let (doc, _level, id) = with_record();
        let unchanged = RoomFields {
            seed: Some(Point::new(2000.0, 1500.0)),
            ..RoomFields::default()
        };
        assert!(update_room_operations(&doc, id, &unchanged)
            .expect("accepted")
            .is_empty());

        for moved in [
            Point::new(2000.0, 1500.000000001),
            Point::new(f64::from_bits(2000.0f64.to_bits() + 1), 1500.0),
            Point::new(-2000.0, 1500.0),
        ] {
            let fields = RoomFields {
                seed: Some(moved),
                ..RoomFields::default()
            };
            let ops = update_room_operations(&doc, id, &fields).expect("accepted");
            assert_eq!(ops.len(), 1, "the move to {moved:?} must be emitted");
        }
    }

    /// Several fields in one command stay ONE operation, and the unchanged
    /// ones are dropped from it rather than dropping the whole command.
    #[test]
    fn a_mixed_command_emits_one_operation_carrying_only_the_changes() {
        let (doc, level, id) = with_record();
        let fields = RoomFields {
            // Unchanged.
            seed: Some(Point::new(2000.0, 1500.0)),
            level_id: Some(level),
            // Changed.
            name: FieldEdit::Set("Kuchnia".to_owned()),
            floor_style_ref: FieldEdit::Set("F-tile".to_owned()),
            ceiling_style_ref: FieldEdit::Keep,
        };
        let ops = update_room_operations(&doc, id, &fields).expect("accepted");
        assert_eq!(ops.len(), 1, "one atomic batch, one undo boundary");
        let payload = payload(&ops);
        let mut keys: Vec<&str> = payload.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["floorStyleRef", "name"]);
    }

    /// A supplied identifier already used by ANY entity kind rejects.
    ///
    /// R5.2.2 says "unused anywhere in the document", not "unused by another
    /// room". A check that only searched `room_records` would let a record
    /// take a wall's id, and the two would then be one entity to every
    /// operation that addresses them.
    #[test]
    fn a_supplied_id_collides_with_every_entity_kind() {
        let (mut doc, level) = one_level();
        let wall = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        );
        let wall_id = wall.id;
        doc.walls.push(wall);
        let opening = domowik_format::Opening::door(wall_id, 0.5, 900.0, 2100.0, 200.0);
        let opening_id = opening.id;
        doc.openings.push(opening);
        let join = WallJoin::new(vec![
            JoinMember::new(wall_id, Role::Start),
            JoinMember::new(wall_id, Role::End),
        ]);
        let join_id = join.id;
        doc.joins.push(join);
        let existing = RoomRecord::new(Uuid::from_u128(9), level, Point::new(1.0, 1.0));
        let room_id = existing.id;
        doc.room_records.push(existing);

        for (id, kind) in [
            (level, "level"),
            (wall_id, "wall"),
            (opening_id, "opening"),
            (join_id, "wallJoin"),
            (room_id, "roomRecord"),
        ] {
            let record = RoomRecord::new(id, level, Point::new(2000.0, 1500.0));
            let err = create_room_operations(&doc, record).expect_err("must reject");
            assert!(
                matches!(err, EditError::RoomIdInUse { id: got, kind: k }
                    if got == id && k == kind),
                "an id already identifying a {kind} must collide; got {err:?}"
            );
        }

        // A fresh id is accepted, so the guard refuses collisions rather than
        // everything.
        let fresh = RoomRecord::new(Uuid::now_v7(), level, Point::new(2000.0, 1500.0));
        assert!(create_room_operations(&doc, fresh).is_ok());
    }
}
