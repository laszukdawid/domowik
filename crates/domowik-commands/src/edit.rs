//! The drafting commands, acting on an open [`Plan`].
//!
//! Each command resolves its references, builds the edit through
//! `domowik-edit`, and commits it as ONE batch to the plan's session. None
//! of them saves: the CLI writes the file afterwards, the hosted service
//! journals the batch. A result's `batch_id` is the committed batch, or
//! `None` when the command changed nothing and committed no batch.
//!
//! `plan` names the plan in messages only (a path for the CLI, a title for
//! the service).

use crate::error::{CommandError, Result};
use crate::levels::{level_list, resolve_level, resolve_room};
use crate::refs;
use domowik_container::Plan;
use domowik_document::operation::{Operation, OperationKind};
use domowik_edit::{FieldEdit, LevelFields, RoomFields};
use domowik_format::{
    Entity, Hinge, Opening, OpeningKind, OpeningMechanism, Point, RoomRecord, Side, Wall,
};
use domowik_geometry::rooms::{self, RoomStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Wall thickness when a command does not give one.
pub const DEFAULT_WALL_THICKNESS_MM: f64 = 120.0;
/// Opening width when a command does not give one.
pub const DEFAULT_OPENING_WIDTH_MM: f64 = 900.0;
/// Door height when a command does not give one.
pub const DEFAULT_DOOR_HEIGHT_MM: f64 = 2100.0;
/// Window height when a command does not give one. A door-height window on
/// a 900 mm sill reaches 3000 mm and is refused by any ordinary 2500 mm wall.
pub const DEFAULT_WINDOW_HEIGHT_MM: f64 = 1200.0;
/// Window sill when a command does not give one.
pub const DEFAULT_SILL_MM: f64 = 900.0;

/// The default height of a new opening of `kind`.
pub fn default_opening_height(kind: OpeningKind) -> f64 {
    match kind {
        OpeningKind::Door => DEFAULT_DOOR_HEIGHT_MM,
        OpeningKind::Window => DEFAULT_WINDOW_HEIGHT_MM,
    }
}

fn commit(plan: &mut Plan, ops: Vec<Operation>) -> Result<Uuid> {
    Ok(plan.session()?.commit(ops)?)
}

fn opening_kind_name(kind: OpeningKind) -> &'static str {
    match kind {
        OpeningKind::Door => "door",
        OpeningKind::Window => "window",
    }
}

// ---------------------------------------------------------------- levels

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelAdded {
    pub batch_id: Uuid,
    pub level_id: Uuid,
}

/// Adds a level. Names must be unique: name-based commands could not
/// address a duplicate.
pub fn add_level(
    plan: &mut Plan,
    name: &str,
    elevation_mm: f64,
    label: &str,
) -> Result<LevelAdded> {
    if plan.document().levels.iter().any(|l| l.name == name) {
        return Err(CommandError::refused(format!(
            "{label} already has a level named {name:?}"
        )));
    }
    let mut level = domowik_format::Level::new(name);
    level.elevation_mm = elevation_mm;
    let level_id = level.id;
    let batch_id = commit(plan, vec![Operation::create(Entity::Level(level))])?;
    Ok(LevelAdded { batch_id, level_id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelUpdated {
    pub batch_id: Option<Uuid>,
    pub level_id: Uuid,
}

/// Whether `fields` asks for any change.
pub fn level_fields_empty(fields: &LevelFields) -> bool {
    fields.name.is_none()
        && fields.elevation_mm.is_none()
        && fields.floor_thickness_mm.is_none()
        && fields.default_wall_height_mm.is_none()
        && fields.default_ceiling_height_mm.is_none()
}

/// Applies one atomic level-properties edit through the shared core builder.
/// Duplicate display names are ambiguous; they never silently select a level.
/// An unchanged edit commits no batch.
pub fn set_level(
    plan: &mut Plan,
    name: &str,
    fields: &LevelFields,
    label: &str,
) -> Result<LevelUpdated> {
    if level_fields_empty(fields) {
        return Err(CommandError::usage(
            "nothing to change; pass --rename, --elevation, --floor-thickness, --wall-height or --ceiling-height",
        ));
    }
    let doc = plan.document();
    if doc.levels.iter().filter(|level| level.name == name).count() > 1 {
        return Err(resolve_level(doc, Some(name), label).unwrap_err());
    }
    let level_id = doc
        .levels
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.id)
        .ok_or_else(|| {
            CommandError::not_found(format!(
                "no level named {name:?} in {label}; available: {}",
                level_list(doc)
            ))
        })?;
    let ops = domowik_edit::set_level_operations(doc, level_id, fields)?;
    if ops.is_empty() {
        return Ok(LevelUpdated {
            batch_id: None,
            level_id,
        });
    }
    let batch_id = Some(commit(plan, ops)?);
    Ok(LevelUpdated { batch_id, level_id })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelRemoved {
    pub batch_id: Uuid,
    pub level_id: Uuid,
}

/// Removes an empty, unreferenced, non-last level named `name`.
///
/// Resolving `name` (refusing an ambiguous duplicate) is this command's job;
/// every other R2.3 reason a deletion may be refused is the shared
/// `CommandError::LevelDeletion` guard's.
pub fn remove_level(plan: &mut Plan, name: &str, label: &str) -> Result<LevelRemoved> {
    let doc = plan.document();
    let matches: Vec<&domowik_format::Level> =
        doc.levels.iter().filter(|l| l.name == name).collect();
    let level_id = match matches.as_slice() {
        [] => {
            return Err(CommandError::not_found(format!(
                "no level named {name:?} in {label}; available: {}",
                level_list(doc)
            )));
        }
        [only] => only.id,
        many => {
            return Err(CommandError::ambiguous(
                format!(
                    "{} levels are named {name:?} in {label}; rename one first to pick unambiguously",
                    many.len()
                ),
                many.iter()
                    .map(|l| crate::error::Candidate {
                        id: l.id,
                        label: l.name.clone(),
                        level: None,
                    })
                    .collect(),
            ));
        }
    };
    let batch_id = commit(plan, vec![Operation::delete(level_id)])?;
    Ok(LevelRemoved { batch_id, level_id })
}

// ----------------------------------------------------------------- walls

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallAdded {
    pub batch_id: Uuid,
    pub wall_id: Uuid,
    pub length_mm: f64,
    /// `W3 #4e1a92` after the commit; `None` when the wall did not survive
    /// as itself (a split can retire it).
    pub label: Option<String>,
    pub splits: usize,
    pub joins: usize,
}

/// Adds a wall to the plan's first level (or `level` when named).
/// An absent height resolves the target level's current default; an explicit
/// height is validated and retained. Existing walls are never rewritten.
pub fn add_wall(
    plan: &mut Plan,
    start: Point,
    end: Point,
    thickness_mm: f64,
    height_mm: Option<f64>,
    level: Option<&str>,
    label: &str,
) -> Result<WallAdded> {
    let level_id = resolve_level(plan.document(), level, label)?;
    let height_mm = domowik_edit::resolve_wall_height(plan.document(), level_id, height_mm)?;
    let wall = Wall::new(level_id, start, end, thickness_mm, height_mm);
    let wall_id = wall.id;
    let length_mm = wall.length_mm();
    let edit = domowik_edit::add_wall_operations(plan.document(), level_id, wall)?;
    let (splits, joins) = (edit.splits, edit.joins);
    let batch_id = commit(plan, edit.operations)?;
    let label = plan
        .document()
        .wall(wall_id)
        .is_some()
        .then(|| refs::wall_label(plan.document(), wall_id));
    Ok(WallAdded {
        batch_id,
        wall_id,
        length_mm,
        label,
        splits,
        joins,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallUpdated {
    /// `None` when every requested value was already the wall's.
    pub batch_id: Option<Uuid>,
    pub wall_id: Uuid,
    /// The label before the edit, which is what the caller named.
    pub label: String,
    pub moved_with: usize,
    pub joins: usize,
}

/// Moves or resizes a wall named by any reference [`refs::resolve_wall`]
/// accepts.
pub fn set_wall(
    plan: &mut Plan,
    wall: &str,
    start: Option<Point>,
    end: Option<Point>,
    thickness_mm: Option<f64>,
    height_mm: Option<f64>,
    label: &str,
) -> Result<WallUpdated> {
    if start.is_none() && end.is_none() && thickness_mm.is_none() && height_mm.is_none() {
        return Err(CommandError::usage(
            "nothing to change; pass at least one of --start, --end, --thickness, --height",
        ));
    }
    let current = refs::resolve_wall(plan.document(), wall, label)?;
    let wall_id = current.id;
    let wall_label = refs::wall_label(plan.document(), wall_id);
    // A value equal to the stored one is not a change: it must neither
    // commit a redundant batch nor reach the builder as an empty move.
    let start = start.filter(|p| *p != current.start());
    let end = end.filter(|p| *p != current.end());
    let thickness_mm = thickness_mm.filter(|t| *t != current.thickness_mm);
    let height_mm = height_mm.filter(|h| *h != current.height_mm);
    let unchanged = WallUpdated {
        batch_id: None,
        wall_id,
        label: wall_label.clone(),
        moved_with: 0,
        joins: 0,
    };
    if start.is_none() && end.is_none() && thickness_mm.is_none() && height_mm.is_none() {
        return Ok(unchanged);
    }
    let edit = domowik_edit::set_wall_operations(
        plan.document(),
        wall_id,
        start,
        end,
        thickness_mm,
        height_mm,
    )?;
    if edit.operations.is_empty() {
        return Ok(unchanged);
    }
    let (moved_with, joins) = (edit.moved_with, edit.joins);
    let batch_id = Some(commit(plan, edit.operations)?);
    Ok(WallUpdated {
        batch_id,
        wall_id,
        label: wall_label,
        moved_with,
        joins,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallRemoved {
    pub batch_id: Uuid,
    pub wall_id: Uuid,
    pub label: String,
    pub cascaded_openings: usize,
}

/// Removes a wall and the openings it hosts.
pub fn remove_wall(plan: &mut Plan, wall: &str, label: &str) -> Result<WallRemoved> {
    let wall_id = refs::resolve_wall(plan.document(), wall, label)?.id;
    let wall_label = refs::wall_label(plan.document(), wall_id);
    let edit = domowik_edit::remove_wall_operations(plan.document(), wall_id);
    let cascaded_openings = edit.cascaded_openings;
    let batch_id = commit(plan, edit.operations)?;
    Ok(WallRemoved {
        batch_id,
        wall_id,
        label: wall_label,
        cascaded_openings,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallReversed {
    pub batch_id: Uuid,
    pub wall_id: Uuid,
    pub label: String,
    pub remapped_openings: usize,
}

/// Reverses a wall's direction, remapping its openings.
pub fn reverse_wall(plan: &mut Plan, wall: &str, label: &str) -> Result<WallReversed> {
    let wall_id = refs::resolve_wall(plan.document(), wall, label)?.id;
    let wall_label = refs::wall_label(plan.document(), wall_id);
    let edit = domowik_edit::reverse_wall_operations(plan.document(), wall_id);
    let remapped_openings = edit.moved_with;
    let batch_id = commit(plan, edit.operations)?;
    Ok(WallReversed {
        batch_id,
        wall_id,
        label: wall_label,
        remapped_openings,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallDetached {
    pub batch_id: Uuid,
    pub wall_id: Uuid,
    pub label: String,
    /// The join had one member left and was deleted.
    pub join_deleted: bool,
}

/// Detaches one endpoint of a wall from the join it belongs to (R3.3.7).
/// `end` is `true` for the wall's end, `false` for its start. The endpoint
/// stays where it is; a join left with one member is deleted. Refused when
/// that endpoint is not joined.
pub fn detach(plan: &mut Plan, wall: &str, end: bool, label: &str) -> Result<WallDetached> {
    let wall_id = refs::resolve_wall(plan.document(), wall, label)?.id;
    let wall_label = refs::wall_label(plan.document(), wall_id);
    let role = if end {
        domowik_format::Role::End
    } else {
        domowik_format::Role::Start
    };
    let join = domowik_edit::member_join(plan.document(), wall_id, role)
        .ok_or_else(|| {
            CommandError::refused(format!(
                "wall {wall_label}'s {} is not joined",
                if end { "end" } else { "start" }
            ))
        })?
        .id;
    let edit = domowik_edit::detach_operations(plan.document(), join, wall_id, role)?;
    let join_deleted = edit
        .operations
        .iter()
        .any(|op| matches!(op.kind, OperationKind::DeleteEntity));
    let batch_id = commit(plan, edit.operations)?;
    Ok(WallDetached {
        batch_id,
        wall_id,
        label: wall_label,
        join_deleted,
    })
}

// -------------------------------------------------------------- openings

/// Where an opening's centre goes along its host wall.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Placement {
    /// A fraction of the wall's length from its start, the persisted `u`.
    Fraction(f64),
    /// Millimetres along the wall from its start, converted to `u` against
    /// the host's current length.
    OffsetMm(f64),
}

/// `u` for `placement` on a wall of `length_mm`. A zero-length wall is
/// refused by the core's fit check anyway; `u` is reported as 0 for it.
pub fn placement_u(placement: Placement, length_mm: f64) -> f64 {
    match placement {
        Placement::Fraction(u) => u,
        Placement::OffsetMm(mm) if length_mm > 0.0 => mm / length_mm,
        Placement::OffsetMm(_) => 0.0,
    }
}

/// How a door or window opens (BEHAVIOR-SPEC R4.0.4); every field optional,
/// absent ones left as they are (or at the creation default).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Orientation {
    /// Which end of the host axis the leaf is hinged at.
    pub hinge: Option<Hinge>,
    /// The half-plane the leaf swings into: `Left` is the host's +n̂ side,
    /// on the left walking from the wall's start to its end.
    pub opens_toward: Option<Side>,
    /// Swing in degrees, 0..=180.
    pub swing_deg: Option<f64>,
    pub mechanism: Option<OpeningMechanism>,
}

impl Orientation {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The field edits for `set-opening`, in the format's own names.
    fn operations(&self, id: Uuid) -> Vec<Operation> {
        let mut ops = Vec::new();
        if let Some(h) = self.hinge {
            ops.push(domowik_edit::set_field(id, "hinge", to_json(&h)));
        }
        if let Some(s) = self.opens_toward {
            ops.push(domowik_edit::set_field(id, "opensToward", to_json(&s)));
        }
        if let Some(d) = self.swing_deg {
            ops.push(domowik_edit::set_field(
                id,
                "swingDeg",
                serde_json::json!(d),
            ));
        }
        if let Some(m) = self.mechanism {
            ops.push(domowik_edit::set_field(id, "mechanism", to_json(&m)));
        }
        ops
    }
}

fn to_json<T: Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("unit enum serialises")
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningAdded {
    pub batch_id: Uuid,
    pub opening_id: Uuid,
    pub kind: OpeningKind,
    /// `D2 #91c0de` after the commit.
    pub label: String,
    pub wall_id: Uuid,
    pub wall_label: String,
    pub wall_length_mm: f64,
    /// The centre's distance from the wall's start.
    pub centre_offset_mm: f64,
}

/// The fields of a new opening besides its host.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NewOpening {
    pub kind: OpeningKind,
    pub placement: Option<Placement>,
    pub width_mm: f64,
    pub height_mm: f64,
    pub sill_mm: f64,
    pub orientation: Orientation,
}

/// Adds a door or window to a wall named by any reference
/// [`refs::resolve_wall`] accepts. With no placement the opening is centred
/// on the reference point when the wall was named by a point, and at the
/// middle of the wall otherwise.
pub fn add_opening(
    plan: &mut Plan,
    wall: &str,
    new: NewOpening,
    label: &str,
) -> Result<OpeningAdded> {
    let host = refs::resolve_wall(plan.document(), wall, label)?;
    let (wall_id, wall_len, thickness) = (host.id, host.length_mm(), host.thickness_mm);
    let wall_label = refs::wall_label(plan.document(), wall_id);
    let placement = new
        .placement
        .unwrap_or_else(|| match refs::parse_point(wall) {
            Some(p) => Placement::OffsetMm(refs::project(host, p).0),
            None => Placement::Fraction(0.5),
        });
    let u = placement_u(placement, wall_len);

    // The opening cuts the full thickness of its host by default (R4.0.4
    // caps `depthMm` at the host's thickness). A niche shallower than the
    // wall is a later `set-opening` edit, not a creation default.
    let mut opening = match new.kind {
        OpeningKind::Door => Opening::door(wall_id, u, new.width_mm, new.height_mm, thickness),
        OpeningKind::Window => Opening::window(
            wall_id,
            u,
            new.width_mm,
            new.height_mm,
            new.sill_mm,
            thickness,
        ),
    };
    let orientation = new.orientation;
    if let Some(m) = orientation.mechanism {
        if !m.accepts(new.kind) {
            return Err(CommandError::usage(format!(
                "mechanism {} does not apply to a {}",
                to_json(&m),
                opening_kind_name(new.kind)
            )));
        }
        opening.mechanism = Some(m);
    }
    if let Some(h) = orientation.hinge {
        opening.hinge = h;
    }
    if let Some(s) = orientation.opens_toward {
        opening.opens_toward = s;
    }
    if let Some(d) = orientation.swing_deg {
        opening.swing_deg = d;
    }
    let opening_id = opening.id;
    let batch_id = commit(plan, vec![Operation::create(Entity::Opening(opening))])?;

    let doc = plan.document();
    let opening_label = doc
        .openings
        .iter()
        .find(|o| o.id == opening_id)
        .map(|o| refs::opening_label(doc, o))
        .unwrap_or_default();
    Ok(OpeningAdded {
        batch_id,
        opening_id,
        kind: new.kind,
        label: opening_label,
        wall_id,
        wall_label,
        wall_length_mm: wall_len,
        centre_offset_mm: u * wall_len,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningUpdated {
    /// `None` when every requested value was already the opening's.
    pub batch_id: Option<Uuid>,
    pub opening_id: Uuid,
    pub label: String,
}

/// The fields `set-opening` may change; absent ones are left as they are.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OpeningChanges {
    pub placement: Option<Placement>,
    pub width_mm: Option<f64>,
    pub height_mm: Option<f64>,
    pub sill_mm: Option<f64>,
    pub orientation: Orientation,
}

/// Changes an opening named by any reference [`refs::resolve_opening`]
/// accepts.
pub fn set_opening(
    plan: &mut Plan,
    opening: &str,
    changes: OpeningChanges,
    label: &str,
) -> Result<OpeningUpdated> {
    if changes.placement.is_none()
        && changes.width_mm.is_none()
        && changes.height_mm.is_none()
        && changes.sill_mm.is_none()
        && changes.orientation.is_empty()
    {
        return Err(CommandError::usage(
            "nothing to change; pass at least one of --at, --offset, --width, --height, --sill, \
             --hinge, --opens-toward, --swing, --mechanism",
        ));
    }
    let found = refs::resolve_opening(plan.document(), opening, label)?;
    let (opening_id, opening_label) = (found.id, refs::opening_label(plan.document(), found));
    let host_len = plan
        .document()
        .wall(found.host_wall_id)
        .map_or(0.0, |w| w.length_mm());
    let at = changes.placement.map(|p| placement_u(p, host_len));

    // Only values that differ from the stored ones become operations.
    let orientation = Orientation {
        hinge: changes.orientation.hinge.filter(|h| *h != found.hinge),
        opens_toward: changes
            .orientation
            .opens_toward
            .filter(|side| *side != found.opens_toward),
        swing_deg: changes
            .orientation
            .swing_deg
            .filter(|d| *d != found.swing_deg),
        mechanism: changes
            .orientation
            .mechanism
            .filter(|m| Some(*m) != found.mechanism),
    };
    let mut ops = orientation.operations(opening_id);
    for (name, value, stored) in [
        ("u", at, found.u),
        ("widthMm", changes.width_mm, found.width_mm),
        ("heightMm", changes.height_mm, found.height_mm),
        ("sillMm", changes.sill_mm, found.sill_mm),
    ] {
        if let Some(v) = value.filter(|v| *v != stored) {
            ops.push(domowik_edit::set_field(
                opening_id,
                name,
                serde_json::json!(v),
            ));
        }
    }
    let batch_id = if ops.is_empty() {
        None
    } else {
        Some(commit(plan, ops)?)
    };
    Ok(OpeningUpdated {
        batch_id,
        opening_id,
        label: opening_label,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningRemoved {
    pub batch_id: Uuid,
    pub opening_id: Uuid,
    pub label: String,
}

/// Removes an opening named by any reference [`refs::resolve_opening`]
/// accepts.
pub fn remove_opening(plan: &mut Plan, opening: &str, label: &str) -> Result<OpeningRemoved> {
    let found = refs::resolve_opening(plan.document(), opening, label)?;
    let (opening_id, opening_label) = (found.id, refs::opening_label(plan.document(), found));
    let batch_id = commit(plan, vec![Operation::delete(opening_id)])?;
    Ok(OpeningRemoved {
        batch_id,
        opening_id,
        label: opening_label,
    })
}

// ----------------------------------------------------------------- rooms

/// A room record's derived status (R5.2.4) after an edit.
fn room_status(plan: &Plan, level_id: Uuid, record_id: Uuid) -> RoomStatus {
    rooms::project_level(plan.document(), level_id)
        .record(record_id)
        .map(|r| r.status)
        .unwrap_or(RoomStatus::SeedInExterior)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomAdded {
    pub batch_id: Uuid,
    pub record_id: Uuid,
    /// `associated`, `superseded`, `seed-on-boundary`, `seed-in-wall` or
    /// `seed-in-exterior`. Anything but `associated` labels no room.
    pub status: &'static str,
    #[serde(skip)]
    pub room_status: RoomStatus,
}

/// The fields of a new room record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewRoom {
    pub seed: [f64; 2],
    pub name: Option<String>,
    /// R5.2.2 forbids automatic id assignment inside the core; the caller
    /// supplies one, or a fresh UUIDv7 is used.
    pub id: Option<Uuid>,
    pub level: Option<String>,
    pub floor_style: Option<String>,
    pub ceiling_style: Option<String>,
}

/// Creates a room record (BEHAVIOR-SPEC R5.2.2 CREATE). The seed is only
/// checked for being a coordinate; where it lands decides the status.
pub fn room_add(plan: &mut Plan, new: NewRoom, label: &str) -> Result<RoomAdded> {
    let level_id = resolve_level(plan.document(), new.level.as_deref(), label)?;
    let mut record = RoomRecord::new(
        new.id.unwrap_or_else(Uuid::now_v7),
        level_id,
        Point::new(new.seed[0], new.seed[1]),
    );
    record.name = new.name;
    record.floor_style_ref = new.floor_style;
    record.ceiling_style_ref = new.ceiling_style;
    let record_id = record.id;
    let ops = domowik_edit::create_room_operations(plan.document(), record)?;
    let batch_id = commit(plan, ops)?;
    let room_status = room_status(plan, level_id, record_id);
    Ok(RoomAdded {
        batch_id,
        record_id,
        status: room_status.token(),
        room_status,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomUpdated {
    pub batch_id: Option<Uuid>,
    pub record_id: Uuid,
    pub display_name: String,
    pub status: &'static str,
    #[serde(skip)]
    pub room_status: RoomStatus,
}

/// The fields `room set` may change. `name` and the styles distinguish
/// keeping, setting and clearing, because an empty string is a legal value.
#[derive(Debug, Clone, Default)]
pub struct RoomChanges {
    pub seed: Option<[f64; 2]>,
    pub name: FieldEdit<String>,
    pub floor_style: FieldEdit<String>,
    pub ceiling_style: FieldEdit<String>,
    pub level: Option<String>,
}

/// Changes a room record's fields (BEHAVIOR-SPEC R5.2.2 UPDATE). An
/// unchanged edit commits no batch.
pub fn room_set(
    plan: &mut Plan,
    room: &str,
    changes: RoomChanges,
    label: &str,
) -> Result<RoomUpdated> {
    let record_id = resolve_room(plan.document(), room)?;
    let level_id = match changes.level.as_deref() {
        Some(name) => Some(resolve_level(plan.document(), Some(name), label)?),
        None => None,
    };
    let fields = RoomFields {
        seed: changes.seed.map(|[x, y]| Point::new(x, y)),
        name: changes.name,
        floor_style_ref: changes.floor_style,
        ceiling_style_ref: changes.ceiling_style,
        level_id,
    };
    let ops = domowik_edit::update_room_operations(plan.document(), record_id, &fields)?;
    let batch_id = if ops.is_empty() {
        None
    } else {
        Some(commit(plan, ops)?)
    };
    let record = plan
        .document()
        .room_record(record_id)
        .expect("resolved above and not deleted by an update")
        .clone();
    let room_status = room_status(plan, record.level_id, record_id);
    Ok(RoomUpdated {
        batch_id,
        record_id,
        display_name: record.display_name().to_owned(),
        status: room_status.token(),
        room_status,
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRemoved {
    pub batch_id: Uuid,
    pub record_id: Uuid,
    pub display_name: String,
}

/// Deletes a room record (BEHAVIOR-SPEC R5.2.2 DELETE). The face survives:
/// faces exist whether or not a record names them (R5.1.8).
pub fn room_remove(plan: &mut Plan, room: &str) -> Result<RoomRemoved> {
    let record_id = resolve_room(plan.document(), room)?;
    let display_name = plan
        .document()
        .room_record(record_id)
        .map(|r| r.display_name().to_owned())
        .unwrap_or_default();
    let ops = domowik_edit::delete_room_operations(plan.document(), record_id)?;
    let batch_id = commit(plan, ops)?;
    Ok(RoomRemoved {
        batch_id,
        record_id,
        display_name,
    })
}

// ------------------------------------------------------------- undo/redo

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryStep {
    /// The compensating batch, or `None` when the stack was empty.
    pub batch_id: Option<Uuid>,
    pub undo_depth: usize,
    pub redo_depth: usize,
}

/// Undoes the session's last local command (BEHAVIOR-SPEC R8: the stacks
/// belong to the editing session, not the file). An empty stack is a no-op.
pub fn undo(plan: &mut Plan) -> Result<HistoryStep> {
    let session = plan.session()?;
    let batch_id = session.undo()?;
    Ok(HistoryStep {
        batch_id,
        undo_depth: session.undo_depth(),
        redo_depth: session.redo_depth(),
    })
}

/// Redoes the session's last undone command. An empty stack is a no-op.
pub fn redo(plan: &mut Plan) -> Result<HistoryStep> {
    let session = plan.session()?;
    let batch_id = session.redo()?;
    Ok(HistoryStep {
        batch_id,
        undo_depth: session.undo_depth(),
        redo_depth: session.redo_depth(),
    })
}
