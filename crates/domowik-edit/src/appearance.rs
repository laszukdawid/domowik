//! Solid-colour commands shared by editor and CLI. Pure, caller-owned operations;
//! no I/O, callbacks or mutation. The caller commits the whole result atomically.
use domowik_document::operation::Operation;
use domowik_format::{
    appearance::{self, DoorStyle, MaterialColors, RgbColor},
    Document, OpeningKind, Side, Wall,
};
use serde_json::{json, Map, Value};
use uuid::Uuid;

fn change(id: Uuid, field: &str, old: Option<&Value>, value: Value) -> Vec<Operation> {
    if old == Some(&value) || (old.is_none() && value.is_null()) {
        return vec![];
    }
    vec![Operation::set_fields(
        id,
        Map::from_iter([(field.into(), value)]),
    )]
}

/// Sets or resets one face (`Some(side)`) or both faces (`None`) of one or
/// more same-level walls. A reset uses `None` colour (the view default). A
/// repeated colour emits no operation. Unknown/duplicate/cross-level targets
/// refuse whole.
pub fn walls(
    doc: &Document,
    ids: &[Uuid],
    side: Option<Side>,
    color: Option<RgbColor>,
) -> Result<Vec<Operation>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut level = None;
    let mut ops = vec![];
    if ids.is_empty() {
        return Err("choose at least one wall".into());
    }
    let mask = match side {
        None => [true, true],
        Some(Side::Left) => [true, false],
        Some(Side::Right) => [false, true],
    };
    for id in ids {
        let wall = doc.wall(*id).ok_or_else(|| format!("unknown wall {id}"))?;
        if !seen.insert(*id) || level.is_some_and(|l| l != wall.level_id) {
            return Err("choose distinct walls on one level".into());
        }
        level = Some(wall.level_id);
        ops.extend(paint(wall, mask, color)?);
    }
    Ok(ops)
}

/// Sets both face colours of one wall at once (`None` resets a face). One
/// operation at most; unchanged effective faces emit none.
pub fn wall_faces(
    doc: &Document,
    id: Uuid,
    faces: [Option<RgbColor>; 2],
) -> Result<Vec<Operation>, String> {
    let wall = doc.wall(id).ok_or_else(|| format!("unknown wall {id}"))?;
    Ok(store(wall, faces)?.into_iter().collect())
}

/// Paints every wall face bounding a room's detected area, in one batch. A
/// face shared with another room is painted whole (faces are wall-granular).
/// Refuses a room with no enclosed area or no bounding walls.
pub fn room(doc: &Document, id: Uuid, color: Option<RgbColor>) -> Result<Vec<Operation>, String> {
    let record = doc.room_record(id).ok_or("unknown room")?;
    let rooms = domowik_geometry::rooms::project_level(doc, record.level_id);
    let face = rooms
        .record(id)
        .and_then(|r| r.face)
        .and_then(|f| rooms.faces.get(f))
        .ok_or("room has no enclosed area")?;
    let solids = domowik_geometry::walls::level_solids(doc, record.level_id);
    let faces = domowik_geometry::rooms::bounding_wall_faces(&solids, face);
    if faces.is_empty() {
        return Err("room has no bounding walls".into());
    }
    let mut ops = vec![];
    for wall_faces in faces.chunk_by(|a, b| a.0 == b.0) {
        let wall = doc.wall(wall_faces[0].0).expect("level wall");
        let mut mask = [false; 2];
        for (_, side) in wall_faces {
            mask[usize::from(*side == Side::Right)] = true;
        }
        ops.extend(paint(wall, mask, color)?);
    }
    Ok(ops)
}

fn paint(
    wall: &Wall,
    mask: [bool; 2],
    color: Option<RgbColor>,
) -> Result<Option<Operation>, String> {
    let old = appearance::wall_faces(wall);
    store(wall, [0, 1].map(|i| if mask[i] { color } else { old[i] }))
}

/// One wall's canonical colour fields for `[Left, Right]` faces: equal faces
/// store only `color`; differing faces store only the side fields. Unchanged
/// effective faces emit nothing.
fn store(wall: &Wall, new: [Option<RgbColor>; 2]) -> Result<Option<Operation>, String> {
    appearance::validate_wall(wall)?;
    if new == appearance::wall_faces(wall) {
        return Ok(None);
    }
    let [left, right] = new;
    let wanted = if left == right {
        [json!(left), Value::Null, Value::Null]
    } else {
        [Value::Null, json!(left), json!(right)]
    };
    let mut fields = Map::new();
    for (field, value) in ["color", "colorLeft", "colorRight"].into_iter().zip(wanted) {
        let stored = wall.unknown.get(field);
        if stored != Some(&value) && !(stored.is_none() && value.is_null()) {
            fields.insert(field.into(), value);
        }
    }
    Ok(Some(Operation::set_fields(wall.id, fields)))
}

/// Replaces one placed instance's palette, never the imported model. Empty resets
/// to source colours. Material membership is checked against decoded bytes by the
/// authoring client; unavailable keys remain inert on replay/roundtrip.
pub fn asset(doc: &Document, id: Uuid, colors: &MaterialColors) -> Result<Vec<Operation>, String> {
    appearance::validate_material_colors(colors)?;
    let a = doc.asset_instance(id).ok_or("unknown asset instance")?;
    if appearance::material_colors(a)? == *colors {
        return Ok(vec![]);
    }
    Ok(change(
        id,
        "materialColors",
        a.unknown.get("materialColors"),
        if colors.is_empty() {
            Value::Null
        } else {
            json!(colors)
        },
    ))
}

/// Replaces one door's optional leaf color and style as one operation. `None`
/// resets color; solid removes the style override. Unknown targets and windows
/// refuse without operations. No geometry, style references or other fields change.
pub fn door(
    doc: &Document,
    id: Uuid,
    color: Option<RgbColor>,
    style: DoorStyle,
) -> Result<Vec<Operation>, String> {
    let opening = doc
        .openings
        .iter()
        .find(|o| o.id == id)
        .ok_or("unknown door")?;
    if opening.kind != OpeningKind::Door {
        return Err("appearance target must be a door".into());
    }
    let mut fields = Map::new();
    if appearance::door_color(opening)? != color {
        fields.insert("color".into(), json!(color));
    }
    if appearance::door_style(opening)? != style {
        fields.insert(
            "doorStyle".into(),
            if style == DoorStyle::Solid {
                Value::Null
            } else {
                json!(style)
            },
        );
    }
    Ok(if fields.is_empty() {
        vec![]
    } else {
        vec![Operation::set_fields(id, fields)]
    })
}
