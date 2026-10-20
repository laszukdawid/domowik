//! `domowik check`: drafting sanity, separate from `validate` (issue 0103).
//!
//! `validate` answers "is this a well-formed plan?". This answers "does this
//! plan look like what its author meant?": everything here is LEGAL in the
//! format and usually a drafting mistake — a wall that stops just short of
//! the wall it was meant to meet, a room name sitting inside a wall, walls
//! that enclose nothing.
//!
//! Detecting a suspicious state is not knowing the intended correction, so
//! the report keeps them apart:
//!
//! - a `suggestion` is a command whose GEOMETRIC effect has been verified
//!   against the plan (the near-miss extension is simulated through the same
//!   edit builder `set-wall` uses, and offered only if it forms the contact);
//!   whether it is what the author meant is still for the caller to judge;
//! - `candidates` list the alternatives when the right edit depends on
//!   intent, such as which room an unplaced name belongs to.
//!
//! Cost per level: one pass over walls with a join-membership set built
//! once, one room projection, interior points computed once per room, and a
//! near-miss scan that compares each UNJOINED wall end with the level's
//! walls (few in a finished drawing; walls × walls when nothing is joined).

use crate::error::Result;
use crate::refs::{self, project, Numbering};
use domowik_document::warnings::{WallAreaOverlapBlock, Warning};
use domowik_format::{Document, Point, Role, RoomRecord, ValidationWarning, Wall};
use domowik_geometry::rooms::{self, LevelRooms, RoomStatus};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// A wall end this close to another wall's axis, and not joined, probably
/// meant to meet it.
pub const NEAR_MISS_MM: f64 = 300.0;
/// Candidate points fully verified per run. Verifying one runs the edit
/// builder and a trial commit, each proportional to the plan, so verifying
/// every near miss on a draft with hundreds of them is quadratic (about
/// 0.4 ms each on a 1,000-wall plan). Later near misses are still reported,
/// without a suggestion until a later run: an agent fixes one and re-checks
/// anyway.
pub const MAX_VERIFIED_CANDIDATES: usize = 64;

/// How much a finding matters. `--strict` fails on [`Severity::Warning`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Info,
}

/// One alternative the caller chooses between.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// What the alternative is, for a reader.
    pub description: String,
    /// The command that applies it. `PLAN` stands for the plan file.
    pub command: String,
}

/// One finding.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Stable kebab-case code.
    pub code: &'static str,
    pub severity: Severity,
    /// The level's name; empty for a document-wide finding.
    pub level: String,
    /// Entities the finding is about, by UUID.
    pub entities: Vec<Uuid>,
    pub message: String,
    /// A command whose geometric effect was verified. `PLAN` stands for the
    /// plan file. Not a statement of intent: confirm it matches the design.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    /// Alternatives when the right edit depends on intent.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub candidates: Vec<Candidate>,
}

/// The whole report.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub path: String,
    pub warnings: usize,
    pub infos: usize,
    /// Candidate points fully attempted (edit built, admission tried), at
    /// most [`MAX_VERIFIED_CANDIDATES`]: the bound on this run's costly work.
    pub verified_candidates: usize,
    pub findings: Vec<Finding>,
}

/// Display form: millimetres, to two decimals only where they matter, so a
/// sub-millimetre gap is not hidden by rounding.
fn shown(p: Point) -> String {
    let mm = |v: f64| {
        let t = format!("{v:.2}");
        t.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    format!("{},{}", mm(p.x), mm(p.y))
}

/// Command form: the exact coordinates, shortest round-trip decimal. A
/// display rounding must never move a repair target.
fn exact(p: Point) -> String {
    format!("{},{}", p.x, p.y)
}

/// A shell word: unchanged when safe, single-quoted otherwise.
fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._,-/:+".contains(c))
    {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

fn endpoint(wall: &Wall, role: Role) -> Point {
    match role {
        Role::Start => wall.start(),
        Role::End => wall.end(),
    }
}

fn role_word(role: Role) -> &'static str {
    match role {
        Role::Start => "start",
        Role::End => "end",
    }
}

/// The point on `other`'s axis nearest to `p`, and the distance to it.
fn nearest_on_axis(other: &Wall, p: Point) -> (Point, f64) {
    let (along, off) = project(other, p);
    let (a, b) = (other.start(), other.end());
    let len = other.length_mm();
    let t = if len > 0.0 { along / len } else { 0.0 };
    (
        Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t),
        off,
    )
}

/// Whether `p` lies exactly on `other`'s axis segment, by the exact
/// orientation predicate the core's contact rule uses. A cheap filter: only
/// such points can make an end meet the axis, so only they are worth a full
/// edit and admission check.
fn on_axis(other: &Wall, p: Point) -> bool {
    let (a, b) = (other.start(), other.end());
    domowik_geometry::orientation(a, b, p) == domowik_geometry::Orientation::Collinear
        && p.x >= a.x.min(b.x)
        && p.x <= a.x.max(b.x)
        && p.y >= a.y.min(b.y)
        && p.y <= a.y.max(b.y)
}

/// A point near `target` on `other`'s axis that `wall`'s `role` end can move
/// to: the edit builder `set-wall` runs must make that end a join member,
/// and the whole batch must be admitted by a trial commit on a scratch copy
/// of the plan's session, exactly as `set-wall` would commit it. The exact
/// projection onto a diagonal axis is usually not representable ON it, so
/// nearby whole-millimetre points are tried too, nearest first, after the
/// cheap exact on-axis filter. `None` when no point passes.
fn verified_target(
    ctx: &Ctx,
    wall: &Wall,
    other: &Wall,
    role: Role,
    target: Point,
) -> Option<Point> {
    ctx.session?;
    let doc = ctx.doc;
    let (cx, cy) = (target.x.round(), target.y.round());
    let mut grid: Vec<Point> = (-2..=2)
        .flat_map(|dx| (-2..=2).map(move |dy| Point::new(cx + f64::from(dx), cy + f64::from(dy))))
        // The exact target is tried first; never test it twice.
        .filter(|p| *p != target)
        .collect();
    let distance = |p: &Point| (p.x - target.x).hypot(p.y - target.y);
    grid.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
    for p in std::iter::once(target).chain(grid) {
        if !on_axis(other, p) {
            continue;
        }
        // The budget is checked and consumed before EVERY expensive attempt,
        // and an edit the builder refuses counts: it cost the same work.
        if ctx.verified.get() >= MAX_VERIFIED_CANDIDATES {
            return None;
        }
        ctx.verified.set(ctx.verified.get() + 1);
        let (start, end) = match role {
            Role::Start => (Some(p), None),
            Role::End => (None, Some(p)),
        };
        let Ok(edit) = domowik_edit::set_wall_operations(doc, wall.id, start, end, None, None)
        else {
            continue;
        };
        if ends_in_join(&edit, wall.id, role) && ctx.admits(edit.operations) {
            return Some(p);
        }
    }
    None
}

/// Whether `edit` makes `wall`'s `role` end a join member, and splits at most
/// the wall it meets. Any contact is not enough: a point a hair PAST the
/// other axis makes the walls cross, which also joins, but splits both and
/// leaves a stub beyond the crossing.
fn ends_in_join(edit: &domowik_edit::WallEdit, wall: Uuid, role: Role) -> bool {
    use domowik_document::operation::OperationKind;
    use domowik_format::{Entity, JoinMember};
    let member = JoinMember {
        wall_id: wall,
        role,
    };
    let joined = edit.operations.iter().any(|op| match &op.kind {
        OperationKind::CreateEntity {
            entity: Entity::WallJoin(join),
        } => join.members.contains(&member),
        OperationKind::SetFields { fields } => fields
            .get("members")
            .and_then(|v| serde_json::from_value::<Vec<JoinMember>>(v.clone()).ok())
            .is_some_and(|members| members.contains(&member)),
        _ => false,
    });
    joined && edit.splits <= 1
}

struct Ctx<'a> {
    doc: &'a Document,
    /// The plan's editing session, for trial commits; `None` when the plan
    /// cannot be edited, in which case no command is suggested.
    session: Option<&'a domowik_document::session::Session>,
    /// One scratch copy of `session` per run, cloned on first use: each
    /// trial commit is undone, which restores its document. Cloning per
    /// trial copied the whole operation log every time.
    scratch: std::cell::RefCell<Option<domowik_document::session::Session>>,
    /// Candidate points verified so far, against [`MAX_VERIFIED_CANDIDATES`].
    verified: std::cell::Cell<usize>,
    numbering: Numbering,
    joined: HashSet<(Uuid, Role)>,
    records: HashMap<Uuid, &'a RoomRecord>,
}

impl Ctx<'_> {
    /// Whether the plan's session would admit `operations`, judged by a
    /// trial commit on the scratch copy, undone at once. A scratch that
    /// fails to undo is dropped and re-cloned on the next trial.
    fn admits(&self, operations: Vec<domowik_document::operation::Operation>) -> bool {
        let Some(session) = self.session else {
            return false;
        };
        let mut scratch = self.scratch.borrow_mut();
        let trial = scratch.get_or_insert_with(|| session.clone());
        if trial.commit(operations).is_err() {
            return false;
        }
        if !matches!(trial.undo(), Ok(Some(_))) {
            *scratch = None;
        }
        true
    }

    fn wall_label(&self, id: Uuid) -> String {
        format!("W{} #{}", self.numbering.wall(id), refs::short_id(id))
    }
}

fn wall_findings(ctx: &Ctx, level: &str, walls: &[&Wall], out: &mut Vec<Finding>) {
    for wall in walls {
        for role in [Role::Start, Role::End] {
            if ctx.joined.contains(&(wall.id, role)) {
                continue;
            }
            let p = endpoint(wall, role);
            let label = ctx.wall_label(wall.id);
            let near = walls
                .iter()
                .filter(|other| other.id != wall.id)
                .map(|other| (other, nearest_on_axis(other, p)))
                .filter(|(_, (_, d))| *d <= NEAR_MISS_MM)
                .min_by(|a, b| a.1 .1.total_cmp(&b.1 .1));
            let Some((other, (target, gap))) = near else {
                out.push(Finding {
                    code: "wall-end-free",
                    severity: Severity::Info,
                    level: level.to_owned(),
                    entities: vec![wall.id],
                    message: format!(
                        "{label}'s {} at ({}) meets no other wall",
                        role_word(role),
                        shown(p)
                    ),
                    suggestion: None,
                    candidates: vec![],
                });
                continue;
            };
            let other_label = ctx.wall_label(other.id);
            // Already exactly on the axis, by the core's own exact rule. On
            // the other wall's INTERIOR, off its end cells, the other wall was
            // never split there: `contact_findings` reports that as a
            // warning. Otherwise it touches an end but was detached or drawn
            // without a join. Any nonzero gap, however small, goes through
            // the search below instead.
            if on_axis(other, p) {
                let cell = domowik_format::topology_cell(p);
                let at_an_end = [other.start(), other.end()]
                    .iter()
                    .any(|q| domowik_format::topology_cell(*q) == cell);
                if !at_an_end {
                    continue;
                }
                out.push(Finding {
                    code: "wall-end-touching-unjoined",
                    severity: Severity::Info,
                    level: level.to_owned(),
                    entities: vec![wall.id, other.id],
                    message: format!(
                        "{label}'s {} at ({}) touches {other_label} but is not joined to it (detached, or drawn without a join); moving it would change nothing",
                        role_word(role),
                        shown(p)
                    ),
                    suggestion: None,
                    candidates: vec![],
                });
                continue;
            }
            let suggestion = verified_target(ctx, wall, other, role, target).map(|t| {
                format!(
                    "domowik set-wall PLAN {} --{} {}",
                    refs::short_id(wall.id),
                    role_word(role),
                    exact(t)
                )
            });
            out.push(Finding {
                code: "wall-end-near-miss",
                severity: Severity::Warning,
                level: level.to_owned(),
                entities: vec![wall.id, other.id],
                message: format!(
                    "{label}'s {} at ({}) is not joined but lies {gap:.1} mm from the axis of {other_label}; if they should meet, extend it{}",
                    role_word(role),
                    shown(p),
                    if suggestion.is_some() {
                        ""
                    } else if ctx.verified.get() >= MAX_VERIFIED_CANDIDATES {
                        " (too many near misses to verify a suggestion for each; fix some and check again)"
                    } else {
                        " (no nearby meeting point both joins and keeps the plan valid, e.g. a door on the wall would no longer fit; edit by hand)"
                    }
                ),
                suggestion,
                candidates: vec![],
            });
        }
    }
}

fn room_findings(ctx: &Ctx, level: &str, projection: &LevelRooms, out: &mut Vec<Finding>) {
    let named: Vec<bool> = (0..projection.faces.len())
        .map(|i| projection.effective_record(i).is_some())
        .collect();
    // Interior points once per room; every candidate list reuses them.
    let interior: Vec<Option<Point>> = projection.faces.iter().map(rooms::interior_point).collect();
    let face_name = |i: usize| -> String {
        projection
            .effective_record(i)
            .and_then(|id| ctx.records.get(&id))
            .map_or("unnamed".to_owned(), |r| {
                format!("named {:?}", r.display_name())
            })
    };
    let area = |i: usize| {
        rooms::display_polygon_area_m2(&projection.faces[i]).unwrap_or_else(|| "—".to_owned())
    };
    for record in &projection.records {
        let Some(stored) = ctx.records.get(&record.id) else {
            continue;
        };
        let name = stored.display_name().to_owned();
        let short = refs::short_id(record.id);
        match record.status {
            RoomStatus::Associated => {}
            RoomStatus::Superseded => {
                let candidates: Vec<Candidate> = projection
                    .records
                    .iter()
                    .filter(|r| r.face == record.face && r.id != record.id)
                    .filter_map(|r| ctx.records.get(&r.id))
                    .map(|r| Candidate {
                        description: format!(
                            "keep {name:?}: remove the other name {:?} (#{})",
                            r.display_name(),
                            refs::short_id(r.id)
                        ),
                        command: format!("domowik room remove PLAN {}", refs::short_id(r.id)),
                    })
                    .chain(std::iter::once(Candidate {
                        description: format!("remove {name:?} (#{short}) instead"),
                        command: format!("domowik room remove PLAN {short}"),
                    }))
                    .collect();
                out.push(Finding {
                    code: "room-name-duplicate",
                    severity: Severity::Warning,
                    level: level.to_owned(),
                    entities: vec![record.id],
                    message: format!(
                        "room name {name:?} (#{short}) shares its room with another name and is not shown; decide which name is right, or whether a dividing wall is missing"
                    ),
                    suggestion: None,
                    candidates,
                });
            }
            status => {
                let seed = stored.seed();
                let mut options: Vec<(usize, Point, f64)> = interior
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| p.map(|p| (i, p, (p.x - seed.x).hypot(p.y - seed.y))))
                    .collect();
                options.sort_by(|a, b| a.2.total_cmp(&b.2));
                let candidates = options
                    .into_iter()
                    .take(4)
                    .map(|(i, p, d)| Candidate {
                        description: format!(
                            "the {} m² room ({}) with an inside point at ({}), {d:.0} mm away",
                            area(i),
                            face_name(i),
                            shown(p)
                        ),
                        command: format!("domowik room set PLAN {short} --seed {}", exact(p)),
                    })
                    .collect();
                out.push(Finding {
                    code: "room-name-unplaced",
                    severity: Severity::Warning,
                    level: level.to_owned(),
                    entities: vec![record.id],
                    message: format!(
                        "room name {name:?} (#{short}) at ({}) labels no room ({}); choose the room it belongs to from your photos and plan, not by distance",
                        shown(seed),
                        status.token()
                    ),
                    suggestion: None,
                    candidates,
                });
            }
        }
    }
    let level_flag = format!("--level {}", quote(level));
    for (i, p) in interior.iter().enumerate() {
        if named[i] {
            continue;
        }
        out.push(Finding {
            code: "room-unnamed",
            severity: Severity::Info,
            level: level.to_owned(),
            entities: vec![],
            message: format!("a {} m² room has no name", area(i)),
            suggestion: p.map(|p| {
                format!(
                    "domowik room add PLAN {} {} --name NAME {level_flag}",
                    p.x, p.y
                )
            }),
            candidates: vec![],
        });
    }
}

/// The document's own validation warnings, with their codes, levels and
/// entities, limited to `wanted` when a level was selected.
fn validation_findings(ctx: &Ctx, wanted: Option<Uuid>, out: &mut Vec<Finding>) {
    let level_name = |id: Uuid| {
        ctx.doc
            .level(id)
            .map_or_else(|| id.to_string(), |l| l.name.clone())
    };
    for warning in domowik_document::warnings::validation_warnings(ctx.doc) {
        let (code, level, entities) = match &warning {
            Warning::WallAreaOverlap(WallAreaOverlapBlock::WallAreaOverlap {
                level,
                wall,
                other,
            }) => ("wall-area-overlap", *level, vec![*wall, *other]),
            Warning::Level(ValidationWarning::SlabIntrudesBelowLevel { level, below }) => {
                ("slab-intrudes-below-level", *level, vec![*level, *below])
            }
            Warning::Level(ValidationWarning::CeilingBelowWallHeight { level }) => {
                ("ceiling-below-wall-height", *level, vec![*level])
            }
            Warning::Level(ValidationWarning::DuplicateElevation { level, other }) => {
                ("duplicate-elevation", *level, vec![*level, *other])
            }
        };
        if wanted.is_some_and(|w| w != level) {
            continue;
        }
        out.push(Finding {
            code,
            severity: Severity::Warning,
            level: level_name(level),
            entities,
            message: warning.to_string(),
            suggestion: None,
            candidates: vec![],
        });
    }
}

/// Contacts no newly admitted edit can create, present in the STORED plan
/// because its history predates the rule (ED-0119): overlapping or identical
/// axes, walls crossing without a join, and an end lying on another wall's
/// interior without splitting it. Each is a warning. Redrawing is the repair
/// for the last two — the builder `add-wall` uses splits and joins what the
/// stored history did not — so no single verified command is offered.
fn contact_findings(ctx: &Ctx, wanted: Option<Uuid>, out: &mut Vec<Finding>) {
    use domowik_document::wall_contact::{document_contact_blocks, WallContactBlock as B};
    for block in document_contact_blocks(ctx.doc) {
        let (B::WallCoincidence { wall, other }
        | B::CollinearOverlap { wall, other }
        | B::UnresolvedCrossing { wall, other }
        | B::UnsplitContact { wall, other }) = block;
        let Some(level) = ctx.doc.wall(wall).map(|w| w.level_id) else {
            continue;
        };
        if wanted.is_some_and(|w| w != level) {
            continue;
        }
        let (a, b) = (ctx.wall_label(wall), ctx.wall_label(other));
        let redraw = |w: Uuid| {
            format!(
                "redraw {}: `domowik remove-wall PLAN {}`, then `domowik add-wall` with the same \
                 coordinates, thickness and height (and re-add its openings), which splits \
                 and joins it",
                ctx.wall_label(w),
                refs::short_id(w)
            )
        };
        let (code, message) = match block {
            B::WallCoincidence { .. } => (
                "wall-coincidence",
                format!("{a} and {b} have the same axis; remove one of them"),
            ),
            B::CollinearOverlap { .. } => (
                "collinear-overlap",
                format!("{a} and {b} overlap along their axes; remove or shorten one of them"),
            ),
            B::UnresolvedCrossing { .. } => (
                "unresolved-wall-crossing",
                format!(
                    "{a} and {b} cross without a join (walls on one level must meet where they \
                     cross); {}",
                    redraw(other)
                ),
            ),
            B::UnsplitContact { .. } => (
                "unsplit-wall-contact",
                format!(
                    "an end of {b} lies on {a}'s interior, but {a} is not split there; {}",
                    redraw(other)
                ),
            ),
        };
        out.push(Finding {
            code,
            severity: Severity::Warning,
            level: ctx
                .doc
                .level(level)
                .map_or_else(|| level.to_string(), |l| l.name.clone()),
            entities: vec![wall, other],
            message,
            suggestion: None,
            candidates: vec![],
        });
    }
}

/// Runs every check on every level, or on the named one.
pub fn run(
    doc: &Document,
    session: Option<&domowik_document::session::Session>,
    plan: &str,
    level_name: Option<&str>,
) -> Result<Report> {
    let wanted = match level_name {
        Some(name) => Some(crate::levels::resolve_level(doc, Some(name), plan)?),
        None => None,
    };
    let ctx = Ctx {
        doc,
        session,
        scratch: std::cell::RefCell::new(None),
        verified: std::cell::Cell::new(0),
        numbering: Numbering::new(doc),
        joined: doc
            .joins
            .iter()
            .flat_map(|j| j.members.iter().map(|m| (m.wall_id, m.role)))
            .collect(),
        records: doc.room_records.iter().map(|r| (r.id, r)).collect(),
    };
    let mut findings = Vec::new();
    for level in &doc.levels {
        if wanted.is_some_and(|w| w != level.id) {
            continue;
        }
        let walls: Vec<&Wall> = doc
            .walls
            .iter()
            .filter(|w| w.level_id == level.id)
            .collect();
        let projection = rooms::project_level(doc, level.id);
        wall_findings(&ctx, &level.name, &walls, &mut findings);
        room_findings(&ctx, &level.name, &projection, &mut findings);
        if !walls.is_empty() && projection.faces.is_empty() {
            findings.push(Finding {
                code: "no-rooms",
                severity: Severity::Warning,
                level: level.name.clone(),
                entities: vec![],
                message: format!(
                    "{} wall(s) on {} enclose no room; a room needs a closed ring of walls",
                    walls.len(),
                    level.name
                ),
                suggestion: None,
                candidates: vec![],
            });
        }
    }
    contact_findings(&ctx, wanted, &mut findings);
    validation_findings(&ctx, wanted, &mut findings);
    Ok(Report {
        verified_candidates: ctx.verified.get(),
        path: plan.to_owned(),
        warnings: findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count(),
        infos: findings
            .iter()
            .filter(|f| f.severity == Severity::Info)
            .count(),
        findings,
    })
}

/// The human rendering: one line per finding, warnings first.
pub fn render(report: &Report) -> String {
    let mut out = format!(
        "{}: {} warning(s), {} note(s)\n",
        report.path, report.warnings, report.infos
    );
    for severity in [Severity::Warning, Severity::Info] {
        for f in report.findings.iter().filter(|f| f.severity == severity) {
            out.push_str(&format!(
                "  {} {}: {}\n",
                match severity {
                    Severity::Warning => "warning",
                    Severity::Info => "note",
                },
                f.code,
                f.message
            ));
            if let Some(s) = &f.suggestion {
                out.push_str(&format!("      suggestion: {s}\n"));
            }
            for c in &f.candidates {
                out.push_str(&format!(
                    "      option: {} — {}\n",
                    c.description, c.command
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_document::operation::Operation;
    use domowik_document::session::Session;
    use domowik_format::{Entity, Level, Opening};

    /// Review R3-1's fixture: each stub's extension would split its host
    /// through a door, so the edit builder refuses every candidate. Those
    /// refusals cost as much as any attempt and must count.
    #[test]
    fn refused_attempts_count_against_the_verification_budget() {
        let mut session = Session::new();
        let level = Level::new("Level 1");
        let level_id = level.id;
        session
            .commit(vec![Operation::create(Entity::Level(level))])
            .unwrap();
        for i in 0..40 {
            let ox = i as f64 * 12_000.0;
            let host = Wall::new(
                level_id,
                Point::new(ox, 0.0),
                Point::new(ox + 4000.0, 0.0),
                200.0,
                2500.0,
            );
            let host_id = host.id;
            let edit =
                domowik_edit::add_wall_operations(session.document(), level_id, host).unwrap();
            session.commit(edit.operations).unwrap();
            let door = Opening::door(host_id, 0.25, 1000.0, 2100.0, 200.0);
            session
                .commit(vec![Operation::create(Entity::Opening(door))])
                .unwrap();
            let stub = Wall::new(
                level_id,
                Point::new(ox + 1000.0, 130.0),
                Point::new(ox + 1000.0, 2000.0),
                120.0,
                2500.0,
            );
            let edit =
                domowik_edit::add_wall_operations(session.document(), level_id, stub).unwrap();
            session.commit(edit.operations).unwrap();
        }
        let doc = session.document().clone();
        let report = run(&doc, Some(&session), "t.domo", None).unwrap();
        assert_eq!(
            report.verified_candidates, MAX_VERIFIED_CANDIDATES,
            "spent exactly the budget"
        );
        let near: Vec<&Finding> = report
            .findings
            .iter()
            .filter(|f| f.code == "wall-end-near-miss")
            .collect();
        assert_eq!(near.len(), 40);
        assert!(near.iter().all(|f| f.suggestion.is_none()));
        assert!(
            near.last().unwrap().message.contains("check again"),
            "{}",
            near.last().unwrap().message
        );
    }

    /// A legacy plan whose history predates ED-0119: one wall crossing the
    /// host unsplit, one ending on its interior unsplit. Both are warnings
    /// with a redraw repair; the second is no longer the "touching … moving it
    /// would change nothing" note.
    #[test]
    fn legacy_unresolved_contacts_are_warnings_with_a_redraw_repair() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let w = |a: (f64, f64), b: (f64, f64)| {
            Wall::new(
                level,
                Point::new(a.0, a.1),
                Point::new(b.0, b.1),
                200.0,
                2500.0,
            )
        };
        doc.walls = vec![
            w((0.0, 0.0), (4000.0, 0.0)),
            w((2000.0, -1000.0), (2000.0, 1000.0)),
            w((1000.0, 0.0), (1000.0, 1000.0)),
        ];
        let report = run(&doc, None, "t.domo", None).unwrap();
        let codes: Vec<&str> = report.findings.iter().map(|f| f.code).collect();
        for code in ["unresolved-wall-crossing", "unsplit-wall-contact"] {
            let f = report.findings.iter().find(|f| f.code == code).expect(code);
            assert_eq!(f.severity, Severity::Warning);
            assert!(
                f.message.contains("remove-wall") && f.message.contains("add-wall"),
                "{}",
                f.message
            );
        }
        assert!(!codes.contains(&"wall-end-touching-unjoined"), "{codes:?}");
    }
}
