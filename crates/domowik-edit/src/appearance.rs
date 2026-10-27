//! Solid-colour commands shared by editor and CLI. Pure, caller-owned operations;
//! no I/O, callbacks or mutation. The caller commits the whole result atomically.
use domowik_document::operation::Operation;
use domowik_format::{
    appearance::{self, DoorStyle, MaterialColors, RgbColor},
    Document, OpeningKind,
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

/// Sets or resets one or more same-level walls. A reset uses `None`; a repeated
/// colour emits no operation. Unknown/duplicate/cross-level targets refuse whole.
pub fn walls(
    doc: &Document,
    ids: &[Uuid],
    color: Option<RgbColor>,
) -> Result<Vec<Operation>, String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut level = None;
    let mut ops = vec![];
    if ids.is_empty() {
        return Err("choose at least one wall".into());
    }
    for id in ids {
        let wall = doc.wall(*id).ok_or_else(|| format!("unknown wall {id}"))?;
        if !seen.insert(*id) || level.is_some_and(|l| l != wall.level_id) {
            return Err("choose distinct walls on one level".into());
        }
        level = Some(wall.level_id);
        if appearance::wall_color(wall)? != color {
            ops.extend(change(
                *id,
                "color",
                wall.unknown.get("color"),
                json!(color),
            ));
        }
    }
    Ok(ops)
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
