//! Architectural commands shared by CLI and interactive clients. Pure builders:
//! owned operation batches, no I/O, no callbacks, no mutation on refusal.
use crate::{apply_preview, remove_wall_operations};
use domowik_document::operation::Operation;
use domowik_format::*;
use domowik_geometry::{
    architecture,
    arrangement::{classify_point, Containment},
    rooms,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Validates a complete candidate before returning an atomic creation batch.
/// IDs are caller-owned; collisions reject, never silently replace construction.
pub fn create_operations(doc: &Document, entities: Vec<Entity>) -> Result<Vec<Operation>, String> {
    let mut candidate = doc.clone();
    let mut ops = Vec::new();
    for entity in entities {
        if !matches!(
            entity,
            Entity::ConstructionBody(_)
                | Entity::HostedOpening(_)
                | Entity::LevelConnection(_)
                | Entity::Staircase(_)
        ) {
            return Err("expected an architectural entity".into());
        }
        if candidate.entity(entity.id()).is_some() {
            return Err("architectural entity ID already in use".into());
        }
        candidate.insert_entity(entity.clone());
        ops.push(Operation::create(entity));
    }
    validate(&candidate)?;
    Ok(ops)
}

fn validate(doc: &Document) -> Result<(), String> {
    let errors = doc.validate_plan();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; "))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StairRequest {
    pub name: String,
    pub from: ConnectionEndpoint,
    pub to: ConnectionEndpoint,
    pub width_mm: f64,
    pub risers: u16,
    pub guard_height_mm: f64,
    pub cut_upper_floor: bool,
}

/// A descending drawing gesture: `startMm` is on the current/upper floor and
/// `endMm` is on the nearest strictly lower level (elevation, then UUID order).
/// Optional dimensions select draft defaults, never measured construction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescendingStairRequest {
    pub upper_level_id: Uuid,
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    pub name: Option<String>,
    pub width_mm: Option<f64>,
    pub risers: Option<u16>,
    pub guard_height_mm: Option<f64>,
}

pub fn descending_stair_request(
    doc: &Document,
    r: &DescendingStairRequest,
) -> Result<StairRequest, String> {
    let upper = doc.level(r.upper_level_id).ok_or("no current level")?;
    let lower = doc
        .levels
        .iter()
        .filter(|l| l.elevation_mm.is_finite() && l.elevation_mm < upper.elevation_mm)
        .min_by(|a, b| {
            b.elevation_mm
                .total_cmp(&a.elevation_mm)
                .then(a.id.cmp(&b.id))
        })
        .ok_or("there is no level below the current floor; add a lower level first")?;
    let rise = upper.elevation_mm - lower.elevation_mm;
    let risers = r
        .risers
        .unwrap_or((rise / 175.0).ceil().clamp(2.0, 256.0) as u16);
    Ok(StairRequest {
        name: r
            .name
            .clone()
            .unwrap_or_else(|| format!("Stairs {}", doc.staircases.len() + 1)),
        from: ConnectionEndpoint {
            level_id: lower.id,
            position_mm: r.end_mm,
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: upper.id,
            position_mm: r.start_mm,
            offset_mm: 0.0,
        },
        width_mm: r.width_mm.unwrap_or(900.0),
        risers,
        guard_height_mm: r.guard_height_mm.unwrap_or(1000.0),
        cut_upper_floor: true,
    })
}

pub fn add_descending_stair_operations(
    doc: &Document,
    r: &DescendingStairRequest,
) -> Result<Vec<Operation>, String> {
    add_stair_operations(doc, &descending_stair_request(doc, r)?)
}

/// Read-only preview of the exact same drawing request used by commit. Only
/// the two endpoint datums and temporary assembly are copied, not plan geometry.
/// Returns owned plan graphics; no operation log, IDs or history are mutated.
pub fn preview_descending_stair(
    doc: &Document,
    r: &DescendingStairRequest,
) -> Result<Vec<PlanConstruction>, String> {
    let request = descending_stair_request(doc, r)?;
    let c = LevelConnection {
        id: Uuid::nil(),
        name: request.name.clone(),
        from: request.from.clone(),
        to: request.to.clone(),
        unknown: Default::default(),
    };
    let s = Staircase {
        id: Uuid::nil(),
        name: request.name,
        connection_id: c.id,
        width_mm: request.width_mm,
        risers: request.risers,
        guard_height_mm: request.guard_height_mm,
        opening_ids: vec![],
        unknown: Default::default(),
    };
    let temporary = Document {
        levels: vec![
            doc.level(c.from.level_id)
                .ok_or("missing lower level")?
                .clone(),
            doc.level(c.to.level_id)
                .ok_or("missing upper level")?
                .clone(),
        ],
        level_connections: vec![c],
        staircases: vec![s],
        ..Document::default()
    };
    architecture::plan(&temporary, r.upper_level_id)
}

/// User-facing start/end are drawing order: upper start, lower end. Absolute
/// elevations are converted to level-relative offsets by Rust, in one batch.
#[derive(Debug, Clone, Default)]
pub struct StairFields {
    pub name: Option<String>,
    pub width_mm: Option<f64>,
    pub risers: Option<u16>,
    pub guard_height_mm: Option<f64>,
    pub start_elevation_mm: Option<f64>,
    pub end_elevation_mm: Option<f64>,
}

/// New assembly and connection, plus an OPTIONAL explicit upper-floor opening.
/// All are one batch. Generated treads/guards never enter the operation log.
pub fn add_stair_operations(
    doc: &Document,
    request: &StairRequest,
) -> Result<Vec<Operation>, String> {
    let connection = LevelConnection {
        id: Uuid::now_v7(),
        name: request.name.clone(),
        from: request.from.clone(),
        to: request.to.clone(),
        unknown: Default::default(),
    };
    let mut s = Staircase {
        id: Uuid::now_v7(),
        name: request.name.clone(),
        connection_id: connection.id,
        width_mm: request.width_mm,
        risers: request.risers,
        guard_height_mm: request.guard_height_mm,
        opening_ids: vec![],
        unknown: Default::default(),
    };
    let mut entities = vec![Entity::LevelConnection(connection.clone())];
    if request.cut_upper_floor {
        let opening = HostedOpening {
            id: Uuid::now_v7(),
            name: format!("{} floor opening", request.name),
            host: OpeningHost::LevelFloor(connection.to.level_id),
            footprint: RectFootprint {
                start_mm: connection.from.position_mm,
                end_mm: connection.to.position_mm,
                width_mm: request.width_mm,
            },
            unknown: Default::default(),
        };
        s.opening_ids.push(opening.id);
        entities.push(Entity::HostedOpening(opening));
    }
    entities.push(Entity::Staircase(s));
    create_operations(doc, entities)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpperEdge {
    East,
    West,
    North,
    South,
}

/// Convert an explicitly selected rectangular room placeholder. The deck-side
/// end stays fixed; the lower end uses (risers−1)*going. Only outline walls
/// bordering exterior space are removed; shared room enclosure and its doors
/// survive. Refuses nonrectangular/ambiguous faces rather than guessing.
pub fn room_to_stair_operations(
    doc: &Document,
    room_id: Uuid,
    lower_level: Uuid,
    upper_edge: UpperEdge,
    going_mm: f64,
    risers: u16,
    guard_height_mm: f64,
) -> Result<Vec<Operation>, String> {
    let record = doc.room_record(room_id).ok_or("no such room record")?;
    if !going_mm.is_finite() || going_mm <= 0.0 || !(2..=256).contains(&risers) {
        return Err("invalid going or riser count".into());
    }
    let projection = rooms::project_level(doc, record.level_id);
    let index = (0..projection.faces.len())
        .find(|i| projection.effective_record(*i) == Some(room_id))
        .ok_or("room record has no associated face")?;
    let face = &projection.faces[index];
    if face.outer.len() != 4 || !face.holes.is_empty() {
        return Err("conversion requires an axis-aligned rectangular room".into());
    }
    let min_x = face.outer.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = face
        .outer
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = face.outer.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let max_y = face
        .outer
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    if face
        .outer
        .iter()
        .any(|p| !(p.x == min_x || p.x == max_x) || !(p.y == min_y || p.y == max_y))
    {
        return Err("conversion requires an axis-aligned rectangular room".into());
    }
    let (upper, dir, width) = match upper_edge {
        UpperEdge::East => ([max_x, (min_y + max_y) / 2.0], [1.0, 0.0], max_y - min_y),
        UpperEdge::West => ([min_x, (min_y + max_y) / 2.0], [-1.0, 0.0], max_y - min_y),
        UpperEdge::North => ([(min_x + max_x) / 2.0, max_y], [0.0, 1.0], max_x - min_x),
        UpperEdge::South => ([(min_x + max_x) / 2.0, min_y], [0.0, -1.0], max_x - min_x),
    };
    let run = going_mm * f64::from(risers - 1);
    let lower = [upper[0] - dir[0] * run, upper[1] - dir[1] * run];
    let contains = |p: Point, polygon: &domowik_geometry::polygon::Polygon| {
        classify_point(p, &polygon.outer) == Containment::Inside
            && polygon
                .holes
                .iter()
                .all(|h| classify_point(p, h) == Containment::Outside)
    };
    let mut remove = Vec::new();
    for w in doc.walls_on_level(record.level_id) {
        let a = w.start();
        let b = w.end();
        let len = w.length_mm();
        if len == 0.0 {
            continue;
        }
        let middle = Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0);
        let distance = w.thickness_mm / 2.0 + 1.0;
        let n = Point::new(-(b.y - a.y) / len * distance, (b.x - a.x) / len * distance);
        let p = Point::new(middle.x + n.x, middle.y + n.y);
        let q = Point::new(middle.x - n.x, middle.y - n.y);
        let opposite = if contains(p, face) {
            Some(q)
        } else if contains(q, face) {
            Some(p)
        } else {
            None
        };
        if let Some(opposite) = opposite {
            let shared = projection
                .faces
                .iter()
                .enumerate()
                .any(|(i, f)| i != index && contains(opposite, f));
            if !shared {
                remove.push(w.id);
            }
        }
    }
    let mut candidate = doc.clone();
    let mut ops = Vec::new();
    for id in remove {
        let edit = remove_wall_operations(&candidate, id);
        for op in &edit.operations {
            apply_preview(&mut candidate, op);
        }
        ops.extend(edit.operations);
    }
    let delete = Operation::delete(room_id);
    apply_preview(&mut candidate, &delete);
    ops.push(delete);
    let request = StairRequest {
        name: record.display_name().into(),
        from: ConnectionEndpoint {
            level_id: lower_level,
            position_mm: lower,
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: record.level_id,
            position_mm: upper,
            offset_mm: 0.0,
        },
        width_mm: width,
        risers,
        guard_height_mm,
        cut_upper_floor: true,
    };
    let add = add_stair_operations(&candidate, &request)?;
    for op in &add {
        apply_preview(&mut candidate, op);
    }
    // Validate generated geometry too, before any file/session effect.
    let stair = candidate
        .staircases
        .last()
        .ok_or("missing converted staircase")?;
    architecture::stair(&candidate, stair)?;
    ops.extend(add);
    Ok(ops)
}

/// Remove an assembly and its private connection; explicitly associated cuts
/// are retained. Their deletion is a separate, reviewable edit.
pub fn delete_stair_operations(doc: &Document, id: Uuid) -> Result<Vec<Operation>, String> {
    let s = doc.staircase(id).ok_or("no such staircase")?;
    Ok(vec![
        Operation::delete(id),
        Operation::delete(s.connection_id),
    ])
}

/// Replace editable stair parameters. Aligned associated upper-floor cuts track
/// width in the same batch; custom cut geometry refuses rather than guessing.
/// An unchanged request returns no operations and creates no history entry.
pub fn set_stair_operations(
    doc: &Document,
    id: Uuid,
    width_mm: Option<f64>,
    risers: Option<u16>,
    guard_height_mm: Option<f64>,
) -> Result<Vec<Operation>, String> {
    set_stair_fields_operations(
        doc,
        id,
        &StairFields {
            width_mm,
            risers,
            guard_height_mm,
            ..StairFields::default()
        },
    )
}

pub fn set_stair_fields_operations(
    doc: &Document,
    id: Uuid,
    changes: &StairFields,
) -> Result<Vec<Operation>, String> {
    let before = doc.staircase(id).ok_or("no such staircase")?;
    let mut s = before.clone();
    if let Some(v) = &changes.name {
        s.name = v.clone();
    }
    if let Some(v) = changes.width_mm {
        s.width_mm = v;
    }
    if let Some(v) = changes.risers {
        s.risers = v;
    }
    if let Some(v) = changes.guard_height_mm {
        s.guard_height_mm = v;
    }
    let connection_before = doc
        .connection(s.connection_id)
        .ok_or("missing stair connection")?;
    let mut connection = connection_before.clone();
    if let Some(v) = changes.start_elevation_mm {
        connection.to.offset_mm = v - doc
            .level(connection.to.level_id)
            .ok_or("missing upper level")?
            .elevation_mm;
    }
    if let Some(v) = changes.end_elevation_mm {
        connection.from.offset_mm = v - doc
            .level(connection.from.level_id)
            .ok_or("missing lower level")?
            .elevation_mm;
    }
    if changes.name.is_some() {
        connection.name = s.name.clone();
    }
    if s == *before && connection == *connection_before {
        return Ok(vec![]);
    }
    let mut candidate = doc.clone();
    candidate.insert_entity(Entity::Staircase(s.clone()));
    candidate.insert_entity(Entity::LevelConnection(connection.clone()));
    let mut ops = Vec::new();
    if s.width_mm != before.width_mm {
        let c = doc
            .connection(s.connection_id)
            .ok_or("missing stair connection")?;
        for opening_id in &s.opening_ids {
            let cut = candidate
                .hosted_openings
                .iter_mut()
                .find(|o| o.id == *opening_id)
                .ok_or("missing associated opening")?;
            if cut.host != OpeningHost::LevelFloor(c.to.level_id)
                || cut.footprint.start_mm != c.from.position_mm
                || cut.footprint.end_mm != c.to.position_mm
                || cut.footprint.width_mm != before.width_mm
            {
                return Err("associated opening has custom geometry; adjust it explicitly before changing stair width".into());
            }
            cut.footprint.width_mm = s.width_mm;
            ops.push(crate::set_field(
                cut.id,
                "footprint",
                serde_json::json!(cut.footprint),
            ));
        }
    }
    validate(&candidate)?;
    let mut connection_fields = serde_json::Map::new();
    if connection.to != connection_before.to {
        connection_fields.insert("to".into(), serde_json::json!(connection.to));
    }
    if connection.from != connection_before.from {
        connection_fields.insert("from".into(), serde_json::json!(connection.from));
    }
    if connection.name != connection_before.name {
        connection_fields.insert("name".into(), serde_json::json!(connection.name));
    }
    if !connection_fields.is_empty() {
        ops.push(Operation::set_fields(connection.id, connection_fields));
    }
    let mut fields = serde_json::Map::new();
    if s.name != before.name {
        fields.insert("name".into(), serde_json::json!(s.name));
    }
    if s.width_mm != before.width_mm {
        fields.insert("widthMm".into(), serde_json::json!(s.width_mm));
    }
    if s.risers != before.risers {
        fields.insert("risers".into(), serde_json::json!(s.risers));
    }
    if s.guard_height_mm != before.guard_height_mm {
        fields.insert("guardHeightMm".into(), serde_json::json!(s.guard_height_mm));
    }
    if !fields.is_empty() {
        ops.push(Operation::set_fields(id, fields));
    }
    Ok(ops)
}
