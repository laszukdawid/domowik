//! Headless architectural editing; all semantics are shared Rust edit builders.
use crate::{CliError, Plan, Result};
use domowik_edit::architecture::{self, StairRequest, UpperEdge};
use domowik_format::{ConnectionEndpoint, Document, Staircase};
use std::path::Path;
use uuid::Uuid;

fn level(doc: &Document, name: &str) -> Result<Uuid> {
    let matches: Vec<_> = doc
        .levels
        .iter()
        .filter(|l| l.name == name || l.id.to_string() == name)
        .collect();
    match matches.as_slice() {
        [l] => Ok(l.id),
        _ => Err(CliError::Usage(format!(
            "level {name:?} is missing or ambiguous"
        ))),
    }
}
fn error(e: String) -> CliError {
    CliError::Usage(e)
}

pub fn add_body(
    path: &Path,
    level_name: &str,
    name: &str,
    role: domowik_format::BodyRole,
    footprint: domowik_format::RectFootprint,
    bottom: [f64; 2],
    thickness: f64,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let body = domowik_format::ConstructionBody {
        id: Uuid::now_v7(),
        level_id: level(plan.document(), level_name)?,
        name: name.into(),
        role,
        footprint,
        bottom_offsets_mm: bottom,
        thickness_mm: thickness,
        unknown: Default::default(),
    };
    let id = body.id;
    let ops = architecture::create_operations(
        plan.document(),
        vec![domowik_format::Entity::ConstructionBody(body)],
    )
    .map_err(error)?;
    plan.commit_and_save(ops)?;
    Ok(format!(
        "Created construction body {name:?} #{}.",
        crate::refs::short_id(id)
    ))
}

pub fn add_cut(
    path: &Path,
    name: &str,
    level_name: Option<&str>,
    body: Option<&str>,
    footprint: domowik_format::RectFootprint,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let host = match (level_name, body) {
        (Some(l), None) => domowik_format::OpeningHost::LevelFloor(level(plan.document(), l)?),
        (None, Some(reference)) => {
            let bodies: Vec<_> = plan
                .document()
                .construction_bodies
                .iter()
                .filter(|b| {
                    b.name == reference
                        || b.id.to_string() == reference
                        || crate::refs::short_id(b.id) == reference
                })
                .collect();
            match bodies.as_slice() {
                [b] => domowik_format::OpeningHost::ConstructionBody(b.id),
                _ => return Err(CliError::Usage("body is missing or ambiguous".into())),
            }
        }
        _ => {
            return Err(CliError::Usage(
                "choose exactly one --level or --body host".into(),
            ))
        }
    };
    let cut = domowik_format::HostedOpening {
        id: Uuid::now_v7(),
        name: name.into(),
        host,
        footprint,
        unknown: Default::default(),
    };
    let id = cut.id;
    let ops = architecture::create_operations(
        plan.document(),
        vec![domowik_format::Entity::HostedOpening(cut)],
    )
    .map_err(error)?;
    plan.commit_and_save(ops)?;
    Ok(format!(
        "Created hosted opening {name:?} #{}.",
        crate::refs::short_id(id)
    ))
}

pub fn add_connection(
    path: &Path,
    name: &str,
    lower_level: &str,
    upper_level: &str,
    start: [f64; 2],
    end: [f64; 2],
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let c = domowik_format::LevelConnection {
        id: Uuid::now_v7(),
        name: name.into(),
        from: ConnectionEndpoint {
            level_id: level(plan.document(), lower_level)?,
            position_mm: start,
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: level(plan.document(), upper_level)?,
            position_mm: end,
            offset_mm: 0.0,
        },
        unknown: Default::default(),
    };
    let id = c.id;
    let ops = architecture::create_operations(
        plan.document(),
        vec![domowik_format::Entity::LevelConnection(c)],
    )
    .map_err(error)?;
    plan.commit_and_save(ops)?;
    Ok(format!(
        "Created connection {name:?} #{} (no physical assembly).",
        crate::refs::short_id(id)
    ))
}

pub fn remove_primitive(path: &Path, reference: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let ids: Vec<_> = plan
        .document()
        .construction_bodies
        .iter()
        .map(|b| (b.id, b.name.as_str()))
        .chain(
            plan.document()
                .hosted_openings
                .iter()
                .map(|o| (o.id, o.name.as_str())),
        )
        .chain(
            plan.document()
                .level_connections
                .iter()
                .map(|c| (c.id, c.name.as_str())),
        )
        .filter(|(id, name)| {
            *name == reference
                || id.to_string() == reference
                || crate::refs::short_id(*id) == reference
        })
        .collect();
    let id = match ids.as_slice() {
        [(id, _)] => *id,
        _ => {
            return Err(CliError::Usage(
                "construction reference is missing or ambiguous".into(),
            ))
        }
    };
    plan.commit_and_save(vec![domowik_document::operation::Operation::delete(id)])?;
    Ok(format!(
        "Removed construction primitive #{}.",
        crate::refs::short_id(id)
    ))
}

pub fn stair_id(doc: &Document, reference: &str) -> Result<Uuid> {
    let matches: Vec<_> = doc
        .staircases
        .iter()
        .filter(|s| {
            s.name == reference
                || s.id.to_string() == reference
                || crate::refs::short_id(s.id) == reference
        })
        .collect();
    match matches.as_slice() {
        [s] => Ok(s.id),
        _ => Err(CliError::Usage(format!(
            "staircase {reference:?} is missing or ambiguous"
        ))),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn add(
    path: &Path,
    name: &str,
    lower_level: &str,
    upper_level: &str,
    start: [f64; 2],
    end: [f64; 2],
    width: f64,
    risers: u16,
    guards: f64,
    cut: bool,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let request = StairRequest {
        name: name.into(),
        from: ConnectionEndpoint {
            level_id: level(plan.document(), lower_level)?,
            position_mm: start,
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: level(plan.document(), upper_level)?,
            position_mm: end,
            offset_mm: 0.0,
        },
        width_mm: width,
        risers,
        guard_height_mm: guards,
        cut_upper_floor: cut,
    };
    let ops = architecture::add_stair_operations(plan.document(), &request).map_err(error)?;
    plan.commit_and_save(ops)?;
    describe(
        plan.document(),
        plan.document().staircases.last().expect("created stair"),
    )
}

pub fn from_room(
    path: &Path,
    room: &str,
    lower_level: &str,
    upper_edge: UpperEdge,
    going: f64,
    risers: u16,
    guards: f64,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let records: Vec<_> = plan
        .document()
        .room_records
        .iter()
        .filter(|r| {
            r.name.as_deref() == Some(room)
                || r.id.to_string() == room
                || crate::refs::short_id(r.id) == room
        })
        .collect();
    let room_id = match records.as_slice() {
        [r] => r.id,
        _ => {
            return Err(CliError::Usage(format!(
                "room {room:?} is missing or ambiguous"
            )))
        }
    };
    let ops = architecture::room_to_stair_operations(
        plan.document(),
        room_id,
        level(plan.document(), lower_level)?,
        upper_edge,
        going,
        risers,
        guards,
    )
    .map_err(error)?;
    plan.commit_and_save(ops)?;
    describe(
        plan.document(),
        plan.document().staircases.last().expect("converted stair"),
    )
}

pub fn set(
    path: &Path,
    reference: &str,
    width: Option<f64>,
    risers: Option<u16>,
    guards: Option<f64>,
) -> Result<String> {
    set_properties(
        path,
        reference,
        &architecture::StairFields {
            width_mm: width,
            risers,
            guard_height_mm: guards,
            ..Default::default()
        },
    )
}

/// Apply the same name/count/endpoint-elevation edit the Floor panel submits.
pub fn set_properties(
    path: &Path,
    reference: &str,
    changes: &architecture::StairFields,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let id = stair_id(plan.document(), reference)?;
    let ops =
        architecture::set_stair_fields_operations(plan.document(), id, changes).map_err(error)?;
    if !ops.is_empty() {
        plan.commit_and_save(ops)?;
    }
    describe(
        plan.document(),
        plan.document().staircase(id).expect("updated stair"),
    )
}
pub fn remove(path: &Path, reference: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let id = stair_id(plan.document(), reference)?;
    let ops = architecture::delete_stair_operations(plan.document(), id).map_err(error)?;
    plan.commit_and_save(ops)?;
    Ok("Removed staircase and connection; associated floor openings were retained.".into())
}

pub fn describe(doc: &Document, s: &Staircase) -> Result<String> {
    let g = domowik_geometry::architecture::stair(doc, s).map_err(error)?;
    Ok(format!("Staircase {:?} #{}: {} → {}; rise {:.1} mm, {} risers × {:.1} mm, {} goings × {:.1} mm; width {:.1} mm, incline {:.1}°.\nGeometric configuration only; not building-code approval.",s.name,crate::refs::short_id(s.id),doc.level(g.lower_level_id).expect("lower level").name,doc.level(g.upper_level_id).expect("upper level").name,g.rise_mm,s.risers,g.riser_mm,s.risers-1,g.going_mm,s.width_mm,g.incline_deg))
}
