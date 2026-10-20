//! Naming levels and room records in commands.

use crate::error::{Candidate, CommandError, Result};
use crate::refs;
use domowik_format::{Document, Level, RoomRecord};
use uuid::Uuid;

/// Derived display order; never rewrites the document's entity storage.
/// Invalid elevations sort last by UUID, matching diagnostic ordering.
pub fn display_levels(doc: &Document) -> Vec<&Level> {
    let mut levels: Vec<_> = doc.levels.iter().collect();
    let valid = |e: f64| e.is_finite() && e.abs() <= domowik_format::COORD_BOUND_MM;
    levels.sort_by(|a, b| {
        match (valid(a.elevation_mm), valid(b.elevation_mm)) {
            (true, true) => a.elevation_mm.partial_cmp(&b.elevation_mm).unwrap(),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.id.cmp(&b.id))
    });
    levels
}

/// The level named `name`, refused when absent or ambiguous; with no name,
/// the first level in display order. `plan` names the plan in messages.
pub fn resolve_level(doc: &Document, name: Option<&str>, plan: &str) -> Result<Uuid> {
    match name {
        Some(name) => {
            let matches: Vec<&Level> = doc.levels.iter().filter(|l| l.name == name).collect();
            match matches.as_slice() {
                [] => Err(CommandError::not_found(format!(
                    "no level named {name:?} in {plan}; available: {}",
                    level_list(doc)
                ))),
                [only] => Ok(only.id),
                many => Err(CommandError::ambiguous(
                    format!("multiple levels are named {name:?}; select and rename the intended level in the editor before using name-based commands"),
                    many.iter()
                        .map(|l| Candidate {
                            id: l.id,
                            label: l.name.clone(),
                            level: None,
                        })
                        .collect(),
                )),
            }
        }
        None => display_levels(doc)
            .first()
            .map(|l| l.id)
            .ok_or_else(|| CommandError::refused(format!("{plan} has no levels to add a wall to"))),
    }
}

/// The level names, quoted, in display order, for messages.
pub fn level_list(doc: &Document) -> String {
    if doc.levels.is_empty() {
        return "(none)".to_owned();
    }
    display_levels(doc)
        .into_iter()
        .map(|l| format!("{:?}", l.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every room record in the document, in the order the room commands number
/// them: level display order, then stable UUID within a level.
///
/// Document order is not canonical (FORMAT-SPEC C1), so numbering by it would
/// let the same command edit different records in two copies of one plan.
pub fn numbered_records(doc: &Document) -> Vec<&RoomRecord> {
    let mut out: Vec<&RoomRecord> = Vec::with_capacity(doc.room_records.len());
    for level in display_levels(doc) {
        let mut on_level: Vec<&RoomRecord> = doc.room_records_on_level(level.id).collect();
        on_level.sort_by_key(|r| r.id);
        out.extend(on_level);
    }
    // A record whose `levelId` dangles belongs to no level and would vanish
    // from the listing entirely — which is the one case where a user most
    // needs to see it, since the only way to fix it is to address it.
    let mut orphans: Vec<&RoomRecord> = doc
        .room_records
        .iter()
        .filter(|r| doc.level(r.level_id).is_none())
        .collect();
    orphans.sort_by_key(|r| r.id);
    out.extend(orphans);
    out
}

/// Resolves a room argument: its `room list` number, its UUID or a short ID.
///
/// A person reads a number off the listing; a script that created a record
/// already holds its identifier and should not have to re-derive a position
/// that another edit may have shifted.
pub fn resolve_room(doc: &Document, room: &str) -> Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(room) {
        return match doc.room_record(id) {
            Some(_) => Ok(id),
            None => Err(CommandError::not_found(format!(
                "no room record {id} in this plan; `domowik room list` shows what is there"
            ))),
        };
    }
    // Six or more hex digits are a short ID even when all digits, exactly as
    // for walls and openings: `000001` must never mean room number 1.
    if refs::is_long_short_id(room) {
        return refs::resolve_room_short(doc, room).expect("shaped like a short ID");
    }
    let index: usize = match room.parse() {
        Ok(index) => index,
        Err(_) => {
            return refs::resolve_room_short(doc, room).unwrap_or_else(|| {
                Err(CommandError::usage(format!(
                    "expected a room number from `domowik room list`, a record UUID or a short ID (4e1a92), got {room:?}"
                )))
            })
        }
    };
    let records = numbered_records(doc);
    if index == 0 || index > records.len() {
        return Err(CommandError::not_found(format!(
            "room {index} is out of range; this plan holds {} room record(s)",
            records.len()
        )));
    }
    Ok(records[index - 1].id)
}
