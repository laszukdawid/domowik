//! Stable references to walls and openings (issue 0103).
//!
//! A wall's `info` number is its position in the document, and almost every
//! edit changes it: a T-junction splits the wall it meets, a removal shifts
//! every later wall. An agent working across commands needs a name that
//! survives that, so every wall or opening argument accepts, in order:
//!
//! 1. a full UUID;
//! 2. a point `X,Y` in millimetres — the wall whose band holds it, or the
//!    opening whose aperture holds it;
//! 3. the `info` number, for people reading a listing;
//! 4. a short ID: the last hex digits of the UUID (`#` optional, 4 or more
//!    digits, unique). The LAST digits because UUIDv7 leads with a
//!    timestamp, so entities created together share their first digits.
//!
//! Six or more hex digits are always a short ID (no plan has 100,000
//! walls); four or five digits are a number when in range, else a short ID.
//! The `#` is optional and best left out on a command line: an unquoted `#`
//! after a space starts a shell COMMENT, silently dropping the reference.
//! Commands this crate prints therefore write short IDs bare. Resolution is
//! against the document the command opened, once per command; nothing is
//! cached.

use crate::CliError;
use domowik_format::{Document, Opening, OpeningKind, Point, Wall};
use std::path::Path;
use uuid::Uuid;

/// Digits printed for a short ID.
pub const SHORT_ID_LEN: usize = 6;
/// Fewest digits accepted as a short ID.
const MIN_SHORT_ID_LEN: usize = 4;
/// A point this close outside a wall band still counts as on the wall, so a
/// coordinate computed in floating point on the face is not refused.
const POINT_SLACK_MM: f64 = 0.5;

/// The short ID printed for `id`: its last six hex digits.
pub fn short_id(id: Uuid) -> String {
    let hex = id.simple().to_string();
    hex[hex.len() - SHORT_ID_LEN..].to_owned()
}

/// The 1-based `info` number of wall `id`.
pub fn wall_number(doc: &Document, id: Uuid) -> usize {
    doc.walls
        .iter()
        .position(|w| w.id == id)
        .map_or(0, |i| i + 1)
}

/// The 1-based number of opening `id` in document order.
pub fn opening_number(doc: &Document, id: Uuid) -> usize {
    doc.openings
        .iter()
        .position(|o| o.id == id)
        .map_or(0, |i| i + 1)
}

/// Wall and opening numbers for a whole document, computed once. Listing
/// every entity with [`wall_number`]/[`opening_number`] would scan the
/// document per entity — quadratic in the plan — so listings use this.
pub struct Numbering {
    walls: std::collections::HashMap<Uuid, usize>,
    openings: std::collections::HashMap<Uuid, usize>,
}

impl Numbering {
    pub fn new(doc: &Document) -> Self {
        Self {
            walls: doc
                .walls
                .iter()
                .enumerate()
                .map(|(i, w)| (w.id, i + 1))
                .collect(),
            openings: doc
                .openings
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id, i + 1))
                .collect(),
        }
    }

    pub fn wall(&self, id: Uuid) -> usize {
        self.walls.get(&id).copied().unwrap_or(0)
    }

    pub fn opening(&self, id: Uuid) -> usize {
        self.openings.get(&id).copied().unwrap_or(0)
    }

    /// As [`opening_label`].
    pub fn opening_label(&self, opening: &Opening) -> String {
        format!(
            "{}{} #{}",
            opening_letter(opening.kind),
            self.opening(opening.id),
            short_id(opening.id)
        )
    }
}

/// `W3 #4e1a92`: how commands name a wall back to the caller.
pub fn wall_label(doc: &Document, id: Uuid) -> String {
    format!("W{} #{}", wall_number(doc, id), short_id(id))
}

/// `D2 #91c0de` (door) or `Win2 #…` (window): how commands name an opening.
pub fn opening_label(doc: &Document, opening: &Opening) -> String {
    format!(
        "{}{} #{}",
        opening_letter(opening.kind),
        opening_number(doc, opening.id),
        short_id(opening.id)
    )
}

/// `D` for a door, `Win` for a window: a bare `W` already names walls, and a
/// label such as `W3` must never be read two ways.
pub fn opening_letter(kind: OpeningKind) -> &'static str {
    match kind {
        OpeningKind::Door => "D",
        OpeningKind::Window => "Win",
    }
}

/// Parses `x,y` in millimetres.
pub fn parse_point(text: &str) -> Option<Point> {
    let (x, y) = text.split_once(',')?;
    let (x, y) = (x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?);
    (x.is_finite() && y.is_finite()).then(|| Point::new(x, y))
}

/// Projection of `p` onto the wall's axis: (distance along from the start,
/// perpendicular distance from the axis). Distance along is clamped to the
/// wall; a zero-length wall projects onto its start.
pub fn project(wall: &Wall, p: Point) -> (f64, f64) {
    let (a, b) = (wall.start(), wall.end());
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return (0.0, ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt());
    }
    let t = (((p.x - a.x) * dx + (p.y - a.y) * dy) / len).clamp(0.0, len);
    let (cx, cy) = (a.x + dx * t / len, a.y + dy * t / len);
    (t, ((p.x - cx).powi(2) + (p.y - cy).powi(2)).sqrt())
}

/// `p` in the wall's own frame, unclamped: (distance along the axis from the
/// start, which may be negative or exceed the length; perpendicular distance
/// from the axis line).
fn local(wall: &Wall, p: Point) -> (f64, f64) {
    let (a, b) = (wall.start(), wall.end());
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = (dx * dx + dy * dy).sqrt();
    if len == 0.0 {
        return (0.0, ((p.x - a.x).powi(2) + (p.y - a.y).powi(2)).sqrt());
    }
    let along = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len;
    let across = ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / len;
    (along, across)
}

/// Whether the wall's rectangular band — flat ends, not rounded caps —
/// holds `p`, with [`POINT_SLACK_MM`] at every boundary.
fn band_contains(wall: &Wall, p: Point) -> bool {
    let (along, across) = local(wall, p);
    (-POINT_SLACK_MM..=wall.length_mm() + POINT_SLACK_MM).contains(&along)
        && across <= wall.thickness_mm / 2.0 + POINT_SLACK_MM
}

/// Walls whose band holds `p`, in document order. Every level is searched:
/// a point on a multi-storey plan can hit walls on several floors and is
/// then refused as ambiguous, naming each wall's level.
pub fn walls_at(doc: &Document, p: Point) -> Vec<&Wall> {
    doc.walls.iter().filter(|w| band_contains(w, p)).collect()
}

fn short_matches<'a, T>(items: &'a [T], id: impl Fn(&T) -> Uuid, digits: &str) -> Vec<&'a T> {
    items
        .iter()
        .filter(|item| id(item).simple().to_string().ends_with(digits))
        .collect()
}

/// The short-ID digits in `text`, if it is shaped like one.
fn short_digits(text: &str) -> Option<String> {
    let digits = text.strip_prefix('#').unwrap_or(text).to_ascii_lowercase();
    (digits.len() >= MIN_SHORT_ID_LEN && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(digits)
}

fn list_walls(doc: &Document, walls: &[&Wall]) -> String {
    let levels: std::collections::BTreeSet<Uuid> = walls.iter().map(|w| w.level_id).collect();
    walls
        .iter()
        .map(|w| match (levels.len() > 1, doc.level(w.level_id)) {
            (true, Some(level)) => format!("{} on {}", wall_label(doc, w.id), level.name),
            _ => wall_label(doc, w.id),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves a wall argument. See the module documentation for the forms.
pub fn resolve_wall<'a>(doc: &'a Document, text: &str, path: &Path) -> Result<&'a Wall, CliError> {
    let text = text.trim();
    if let Ok(id) = Uuid::parse_str(text) {
        return doc
            .wall(id)
            .ok_or_else(|| CliError::Failure(format!("no wall {id} in {}", path.display())));
    }
    if let Some(p) = parse_point(text) {
        let hits = walls_at(doc, p);
        return match hits.as_slice() {
            [wall] => Ok(wall),
            [] => {
                let nearest = doc
                    .walls
                    .iter()
                    .min_by(|a, b| project(a, p).1.total_cmp(&project(b, p).1));
                Err(CliError::Failure(match nearest {
                    Some(w) => format!(
                        "no wall at ({:.0}, {:.0}); the nearest is {}, {:.0} mm from its axis",
                        p.x,
                        p.y,
                        wall_label(doc, w.id),
                        project(w, p).1
                    ),
                    None => format!("no wall at ({:.0}, {:.0}); the plan has no walls", p.x, p.y),
                }))
            }
            many => Err(CliError::Failure(format!(
                "({:.0}, {:.0}) lies on {} walls ({}); pick a point inside one wall, or name it",
                p.x,
                p.y,
                many.len(),
                list_walls(doc, many)
            ))),
        };
    }
    let long_short = is_long_short_id(text);
    if let (false, Ok(number)) = (long_short, text.parse::<usize>()) {
        if number == 0 {
            return Err(CliError::Usage("wall numbering starts at 1".to_owned()));
        }
        if number <= doc.walls.len() {
            return Ok(&doc.walls[number - 1]);
        }
        if short_digits(text).is_none() {
            return Err(CliError::Failure(format!(
                "no wall {number}; {} has {} wall(s)",
                path.display(),
                doc.walls.len()
            )));
        }
    }
    if let Some(digits) = short_digits(text) {
        let hits = short_matches(&doc.walls, |w| w.id, &digits);
        return match hits.as_slice() {
            [wall] => Ok(wall),
            [] => Err(CliError::Failure(format!(
                "no wall with ID ending in {digits} in {}",
                path.display()
            ))),
            many => Err(CliError::Failure(format!(
                "#{digits} matches {} walls ({}); give more digits",
                many.len(),
                list_walls(doc, many)
            ))),
        };
    }
    Err(CliError::Usage(format!(
        "expected a wall number, UUID, short ID (4e1a92) or point (x,y), got {text:?}"
    )))
}

/// Openings whose aperture holds `p`: on a wall whose band holds `p`, with
/// `p`'s distance along that wall within half the opening's width of its
/// centre.
fn openings_at(doc: &Document, p: Point) -> Vec<&Opening> {
    let walls = walls_at(doc, p);
    doc.openings
        .iter()
        .filter(|o| {
            walls.iter().any(|w| {
                w.id == o.host_wall_id
                    && (local(w, p).0 - o.u * w.length_mm()).abs()
                        <= o.width_mm / 2.0 + POINT_SLACK_MM
            })
        })
        .collect()
}

fn list_openings(doc: &Document, openings: &[&Opening]) -> String {
    openings
        .iter()
        .map(|o| opening_label(doc, o))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Resolves an opening argument: UUID, point, number in document order, or
/// short ID, as for walls.
pub fn resolve_opening<'a>(
    doc: &'a Document,
    text: &str,
    path: &Path,
) -> Result<&'a Opening, CliError> {
    let text = text.trim();
    if let Ok(id) = Uuid::parse_str(text) {
        return doc
            .openings
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| CliError::Failure(format!("no opening {id} in {}", path.display())));
    }
    if let Some(p) = parse_point(text) {
        let hits = openings_at(doc, p);
        return match hits.as_slice() {
            [opening] => Ok(opening),
            [] => Err(CliError::Failure(format!(
                "no door or window at ({:.0}, {:.0})",
                p.x, p.y
            ))),
            many => Err(CliError::Failure(format!(
                "({:.0}, {:.0}) lies in {} openings ({}); name one",
                p.x,
                p.y,
                many.len(),
                list_openings(doc, many)
            ))),
        };
    }
    let long_short = is_long_short_id(text);
    if let (false, Ok(number)) = (long_short, text.parse::<usize>()) {
        if number == 0 {
            return Err(CliError::Usage("opening numbering starts at 1".to_owned()));
        }
        if number <= doc.openings.len() {
            return Ok(&doc.openings[number - 1]);
        }
        if short_digits(text).is_none() {
            return Err(CliError::Failure(format!(
                "no opening {number}; {} has {} opening(s)",
                path.display(),
                doc.openings.len()
            )));
        }
    }
    if let Some(digits) = short_digits(text) {
        let hits = short_matches(&doc.openings, |o| o.id, &digits);
        return match hits.as_slice() {
            [opening] => Ok(opening),
            [] => Err(CliError::Failure(format!(
                "no opening with ID ending in {digits} in {}",
                path.display()
            ))),
            many => Err(CliError::Failure(format!(
                "#{digits} matches {} openings ({}); give more digits",
                many.len(),
                list_openings(doc, many)
            ))),
        };
    }
    Err(CliError::Usage(format!(
        "expected an opening number, UUID, short ID (91c0de) or point (x,y), got {text:?}"
    )))
}

/// Whether `text` must be read as a short ID before any number: six or more
/// hex digits, `#` optional. Shared by every resolver so walls, openings and
/// rooms agree.
pub fn is_long_short_id(text: &str) -> bool {
    text.trim_start_matches('#').len() >= SHORT_ID_LEN && short_digits(text).is_some()
}

/// Resolves a room-record short ID, for `room` commands that already accept
/// a number or a UUID. `None` when `text` is not shaped like one.
pub fn resolve_room_short(doc: &Document, text: &str) -> Option<Result<Uuid, CliError>> {
    let digits = short_digits(text)?;
    let hits = short_matches(&doc.room_records, |r| r.id, &digits);
    Some(match hits.as_slice() {
        [record] => Ok(record.id),
        [] => Err(CliError::Failure(format!(
            "no room record with ID ending in {digits}"
        ))),
        many => Err(CliError::Failure(format!(
            "#{digits} matches {} room records; give more digits",
            many.len()
        ))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::Document;

    fn wall(doc: &mut Document, a: (f64, f64), b: (f64, f64)) -> Uuid {
        let level = doc.levels[0].id;
        let w = Wall::new(
            level,
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            200.0,
            2500.0,
        );
        let id = w.id;
        doc.walls.push(w);
        id
    }

    fn doc() -> Document {
        let mut doc = Document::default();
        doc.levels.push(domowik_format::Level::new("Level 1"));
        doc
    }

    #[test]
    fn short_ids_are_the_last_six_hex_digits() {
        let id = Uuid::parse_str("01a12924-5f8d-7f83-bca9-c910c6d95d14").unwrap();
        assert_eq!(short_id(id), "d95d14");
    }

    #[test]
    fn every_reference_form_resolves_to_the_same_wall() {
        let mut d = doc();
        wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        let id = wall(&mut d, (4000.0, 0.0), (4000.0, 3000.0));
        let p = Path::new("p.domo");
        let short = short_id(id);
        for text in [
            id.to_string(),
            "2".to_owned(),
            format!("#{short}"),
            short.clone(),
            "4000,1500".to_owned(),
            " 4050 , 1500 ".to_owned(),
        ] {
            assert_eq!(resolve_wall(&d, &text, p).unwrap().id, id, "{text}");
        }
    }

    #[test]
    fn a_point_on_a_junction_is_ambiguous_and_a_miss_names_the_nearest_wall() {
        let mut d = doc();
        wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        wall(&mut d, (4000.0, 0.0), (4000.0, 3000.0));
        let p = Path::new("p.domo");
        let err = resolve_wall(&d, "4000,0", p)
            .unwrap_err()
            .message()
            .to_owned();
        assert!(err.contains("lies on 2 walls"), "{err}");
        let err = resolve_wall(&d, "2000,700", p)
            .unwrap_err()
            .message()
            .to_owned();
        assert!(
            err.contains("nearest is W1") && err.contains("700 mm"),
            "{err}"
        );
        // On the face, not past it: thickness 200, so 100 mm off the axis.
        assert!(resolve_wall(&d, "2000,100", p).is_ok());
        assert!(resolve_wall(&d, "2000,101", p).is_err());
    }

    #[test]
    fn six_digit_short_ids_win_over_numbers_and_need_no_hash() {
        let mut d = doc();
        let id = wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        // Even an all-digit six-digit ID that is also a valid number.
        let digits = short_id(id);
        let p = Path::new("p.domo");
        assert_eq!(resolve_wall(&d, &digits, p).unwrap().id, id);
        assert_eq!(resolve_wall(&d, "1", p).unwrap().id, id);
    }

    #[test]
    fn wall_ends_are_flat_not_rounded() {
        let mut d = doc();
        let host = wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        let door = Opening::door(host, 450.1 / 4000.0, 900.0, 2100.0, 200.0);
        d.openings.push(door);
        let p = Path::new("p.domo");
        // 50 mm before the flat start: outside the band and the aperture.
        assert!(resolve_wall(&d, "-50,0", p).is_err());
        assert!(resolve_opening(&d, "-50,0", p).is_err());
        // On the end face itself, and inside the corner of the band.
        assert!(resolve_wall(&d, "0,0", p).is_ok());
        assert!(resolve_wall(&d, "0.4,99", p).is_ok());
        assert!(resolve_wall(&d, "4000.4,-99", p).is_ok());
        assert!(resolve_wall(&d, "4001,0", p).is_err());
    }

    #[test]
    fn out_of_range_numbers_and_unknown_forms_are_refused() {
        let mut d = doc();
        wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        let p = Path::new("p.domo");
        assert!(resolve_wall(&d, "7", p)
            .unwrap_err()
            .message()
            .to_owned()
            .contains("no wall 7"));
        assert!(matches!(
            resolve_wall(&d, "kitchen", p),
            Err(CliError::Usage(_))
        ));
        assert!(resolve_wall(&d, "#zzzz", p).is_err());
    }

    #[test]
    fn openings_resolve_by_point_inside_their_aperture() {
        let mut d = doc();
        let host = wall(&mut d, (0.0, 0.0), (4000.0, 0.0));
        let door = Opening::door(host, 0.25, 900.0, 2100.0, 200.0);
        let id = door.id;
        d.openings.push(door);
        let p = Path::new("p.domo");
        assert_eq!(resolve_opening(&d, "1000,0", p).unwrap().id, id);
        assert_eq!(resolve_opening(&d, "1440,50", p).unwrap().id, id);
        assert!(resolve_opening(&d, "1500,0", p).is_err());
        assert_eq!(
            resolve_opening(&d, &format!("#{}", short_id(id)), p)
                .unwrap()
                .id,
            id
        );
    }
}
