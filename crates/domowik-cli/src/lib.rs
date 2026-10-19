//! Domowik CLI library: the commands behind the `domowik` binary.
//!
//! The logic lives here rather than in `main.rs` so it can be tested directly,
//! and so the stream and exit-code discipline described below is enforced by
//! tests rather than by convention.
//!
//! # Stream and exit-code contract
//!
//! Reinstates the discipline the frozen Go CLI's probes P1-P3 asserted:
//!
//! - Success: result on **stdout**, nothing on stderr, exit **0**.
//! - Usage error (unknown command, bad arguments): message on **stderr**,
//!   nothing on stdout, exit **2**.
//! - Runtime failure (missing file, invalid plan): message on **stderr**,
//!   exit **1**.

#![forbid(unsafe_code)]

pub mod check;
pub mod label;
pub mod png;
pub mod refs;
pub mod svg;

use domowik_container::{read_package, write_package, ContainerError, Limits, Package};
use domowik_document::log::Delivery;
use domowik_document::operation::{Batch, Operation, OperationKind};
use domowik_document::session::CommandError;
use domowik_edit::{FieldEdit, RoomFields};
use domowik_format::{Document, Entity, Opening, OpeningKind, Point, RoomRecord, Wall};
use domowik_geometry::polygon::Polygon;
use domowik_geometry::rooms::{self, RoomStatus};
use domowik_geometry::walls::{self, JoinSpec, WallSolid};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use uuid::Uuid;

/// Process exit codes, named so the contract is visible at every call site.
pub mod exit {
    /// Completed successfully.
    pub const OK: i32 = 0;
    /// The command ran but failed.
    pub const FAILURE: i32 = 1;
    /// The command was used incorrectly.
    pub const USAGE: i32 = 2;
}

/// What went wrong, and which exit code it maps to.
#[derive(Debug)]
pub enum CliError {
    /// A machine-readable report for a plan that failed validation.
    ///
    /// Carries the report so `validate --json` can print it and STILL exit
    /// non-zero: a caller that asked for machine-readable output wants the
    /// problems, and an exit code alone does not carry them. Printed to
    /// stdout, because it is the requested RESULT — the failure is the plan's,
    /// not the command's.
    Report(String),
    /// Incorrect usage. Exit 2.
    Usage(String),
    /// A command failed. Exit 1.
    Failure(String),
}

/// An edit the algebra refused becomes an ordinary command failure.
///
/// R4.3.3's `opening-split-ambiguous` is a refusal of the whole batch, and the
/// message already names the diagnostic, so nothing is added here beyond the
/// exit code the CLI needs.
impl From<domowik_edit::EditError> for CliError {
    fn from(e: domowik_edit::EditError) -> Self {
        CliError::Failure(e.to_string())
    }
}

/// A cache fault is an ordinary command failure.
///
/// A malformed digest is a USAGE error rather than a failure: the caller typed
/// something that is not a digest, and exit 2 is what tells a script it passed
/// a bad argument rather than that the cache is broken.
impl From<domowik_assets::CacheError> for CliError {
    fn from(e: domowik_assets::CacheError) -> Self {
        match e {
            domowik_assets::CacheError::MalformedDigest { .. } => CliError::Usage(e.to_string()),
            other => CliError::Failure(other.to_string()),
        }
    }
}

impl CliError {
    pub fn code(&self) -> i32 {
        match self {
            Self::Usage(_) => exit::USAGE,
            Self::Failure(_) | Self::Report(_) => exit::FAILURE,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Usage(m) | Self::Failure(m) | Self::Report(m) => m,
        }
    }

    /// Whether this error's message is the RESULT the caller asked for, and
    /// therefore belongs on stdout rather than stderr.
    ///
    /// Only [`CliError::Report`]: `validate --json` on an invalid plan is a
    /// successful validation that found problems, not a failed command. The
    /// exit code still says the plan is invalid.
    pub fn is_report(&self) -> bool {
        matches!(self, Self::Report(_))
    }
}

impl From<ContainerError> for CliError {
    fn from(e: ContainerError) -> Self {
        Self::Failure(e.to_string())
    }
}

impl From<CommandError> for CliError {
    fn from(e: CommandError) -> Self {
        Self::Failure(e.to_string())
    }
}

type Result<T> = std::result::Result<T, CliError>;

/// An open plan file: a package, and the session that edits it.
///
/// Every mutating command follows the same three steps — open, commit, save —
/// so the operation log is the single path by which a document changes. A
/// command that edited `Document` directly would produce state no operation
/// describes, and the next reader replaying the log would disagree with the
/// file it just read.
pub use domowik_container::Plan;

// The wall-edit batch algebra now lives in `domowik-edit`, so the GUI can call
// exactly what the CLI calls. Re-exported because every command here uses it
// and because callers should not have to know which crate it moved to.
pub use domowik_edit::{
    add_wall_operations, apply_preview, detach_operations, join_operation, member_join,
    node_members, param_along, remove_wall_operations, reverse_wall_operations, set_field,
    set_wall_operations, split_operations, splits_induced, WallEdit,
};

/// Converts a plan written by an earlier development build to a new file.
///
/// Non-destructive by construction: `source` is only read, and `destination`
/// must not already exist. The reader refuses the old layout rather than
/// converting on open (ED-0108), so this is always an explicit act.
///
/// The message states what was carried, because the one thing a user needs to
/// know about a conversion is whether anything was left behind.
pub fn cmd_convert_developer_package(source: &Path, destination: &Path) -> Result<String> {
    domowik_container::convert_development_package(source, destination)?;
    let converted = read_package(destination)?;
    Ok(format!(
        "Converted {} to {}.\n           {} batch(es), {} operation(s), {} blob(s), {} asset-root entry(ies).\n           The original is unchanged; nothing was written to it.",
        source.display(),
        destination.display(),
        converted.batches.len(),
        converted
            .batches
            .iter()
            .map(|b| b.operations.len())
            .sum::<usize>(),
        converted.assets.len(),
        converted.manifest.asset_root.len(),
    ))
}

/// Creates a new plan at `path` containing one ground-floor level.
///
/// Refuses to overwrite an existing file: losing a user's plan to a mistyped
/// filename is not an acceptable outcome, and `--force` makes the intent
/// explicit.
pub fn cmd_new(path: &Path, force: bool) -> Result<String> {
    if path.exists() && !force {
        return Err(CliError::Failure(format!(
            "{} already exists; pass --force to overwrite it",
            path.display()
        )));
    }
    // The baseline level is written straight into the snapshot with an EMPTY
    // log (issue 0053): it is not an operation, so a fresh plan has nothing
    // to undo back past, matching the GUI's non-undoable
    // `domowik_plan_create_default` baseline. Existing files created before
    // this change keep their original committed-batch shape untouched —
    // this only changes what `cmd_new` writes from here on.
    let snapshot = Document::with_ground_floor();
    let level_name = snapshot.levels[0].name.clone();
    write_package(
        path,
        &Package {
            manifest: domowik_container::Manifest::default(),
            snapshot,
            frontier: Default::default(),
            tombstones: Vec::new(),
            snapshot_unknown: Default::default(),
            descriptors: Default::default(),
            batches: Vec::new(),
            assets: Default::default(),
        },
    )?;
    Ok(format!(
        "Created {} with one level ({level_name}).",
        path.display()
    ))
}

/// Removes an empty, unreferenced, non-last level named `name`.
///
/// Resolving `name` to an id (including refusing an AMBIGUOUS duplicate
/// name, R2.2 permits levels to share one) is this command's own job; every
/// other R2.3 reason a deletion may be refused — last level, still-hosted
/// walls/openings, still-referenced background images — is the shared
/// [`domowik_document::session::CommandError::LevelDeletion`] guard's,
/// reported with the same reasons the core and the GUI use, not
/// reimplemented here.
pub fn cmd_remove_level(path: &Path, name: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let matches: Vec<&domowik_format::Level> = plan
        .document()
        .levels
        .iter()
        .filter(|l| l.name == name)
        .collect();
    let id = match matches.as_slice() {
        [] => {
            return Err(CliError::Failure(format!(
                "no level named {name:?} in {}; available: {}",
                path.display(),
                level_list(plan.document())
            )));
        }
        [only] => only.id,
        many => {
            return Err(CliError::Failure(format!(
                "{} levels are named {name:?} in {}; rename one first to pick unambiguously",
                many.len(),
                path.display()
            )));
        }
    };
    plan.commit_and_save(vec![Operation::delete(id)])?;
    let mut msg = format!("Removed level {name:?}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Adds a wall to the plan's first level (or `level` when named).
/// An absent height resolves the target level's current default; an explicit
/// height is validated and retained. Existing walls are never rewritten.
#[allow(clippy::too_many_arguments)]
pub fn cmd_add_wall(
    path: &Path,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    thickness_mm: f64,
    height_mm: impl Into<Option<f64>>,
    level_name: Option<&str>,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let level_id = resolve_level(plan.document(), level_name, path)?;
    let height_mm = domowik_edit::resolve_wall_height(plan.document(), level_id, height_mm.into())?;

    let wall = Wall::new(
        level_id,
        Point::new(x1, y1),
        Point::new(x2, y2),
        thickness_mm,
        height_mm,
    );
    let wall_id = wall.id;
    let length = wall.length_mm();

    let edit = add_wall_operations(plan.document(), level_id, wall)?;
    let (splits, joins) = (edit.splits, edit.joins);
    plan.commit_and_save(edit.operations)?;

    let mut msg = format!(
        "Added wall {wall_id} ({length:.0} mm) to {}.",
        path.display()
    );
    // The new wall's number and short ID AFTER the commit: a later command
    // can name it by either, and the short ID survives renumbering.
    if plan.document().wall(wall_id).is_some() {
        msg.push_str(&format!(
            " It is {}.",
            refs::wall_label(plan.document(), wall_id)
        ));
    }
    if splits > 0 {
        msg.push_str(&format!(" Split {splits} wall(s) at the contact."));
    }
    if joins > 0 {
        msg.push_str(&format!(" Joined at {joins} corner(s)."));
    }
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Where an opening's centre goes along its host wall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placement {
    /// A fraction of the wall's length from its start, the persisted `u`.
    Fraction(f64),
    /// Millimetres along the wall from its start, converted to `u` against
    /// the host's current length.
    OffsetMm(f64),
}

/// How a door or window opens (BEHAVIOR-SPEC R4.0.4); every field optional,
/// absent ones left as they are (or at the creation default).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Orientation {
    /// Which end of the host axis the leaf is hinged at.
    pub hinge: Option<domowik_format::Hinge>,
    /// The half-plane the leaf swings into: `Left` is the host's +n̂ side,
    /// on the left walking from the wall's start to its end.
    pub opens_toward: Option<domowik_format::Side>,
    /// Swing in degrees, 0..=180.
    pub swing_deg: Option<f64>,
    pub mechanism: Option<domowik_format::OpeningMechanism>,
}

impl Orientation {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The field edits for `set-opening`, in the format's own names.
    fn operations(&self, id: Uuid) -> Vec<Operation> {
        let mut ops = Vec::new();
        let json = |v: &dyn erased::Ser| v.to_json();
        if let Some(h) = self.hinge {
            ops.push(set_field(id, "hinge", json(&h)));
        }
        if let Some(s) = self.opens_toward {
            ops.push(set_field(id, "opensToward", json(&s)));
        }
        if let Some(d) = self.swing_deg {
            ops.push(set_field(id, "swingDeg", serde_json::json!(d)));
        }
        if let Some(m) = self.mechanism {
            ops.push(set_field(id, "mechanism", json(&m)));
        }
        ops
    }
}

/// Serialisation of the small enums above, without naming each type.
mod erased {
    pub trait Ser {
        fn to_json(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_json(&self) -> serde_json::Value {
            serde_json::to_value(self).expect("unit enum serialises")
        }
    }
}

/// `u` for `placement` on a wall of `length_mm`. A zero-length wall is
/// refused by the core's fit check anyway; `u` is reported as 0 for it.
fn placement_u(placement: Placement, length_mm: f64) -> f64 {
    match placement {
        Placement::Fraction(u) => u,
        Placement::OffsetMm(mm) if length_mm > 0.0 => mm / length_mm,
        Placement::OffsetMm(_) => 0.0,
    }
}

/// Adds a door or window to a wall.
///
/// `wall` is any reference [`refs::resolve_wall`] accepts. With no
/// `placement` the opening is centred on the reference point when the wall
/// was named by a point, and at the middle of the wall otherwise.
#[allow(clippy::too_many_arguments)]
pub fn cmd_add_opening(
    path: &Path,
    wall: &str,
    kind: OpeningKind,
    placement: Option<Placement>,
    width_mm: f64,
    height_mm: f64,
    sill_mm: f64,
    orientation: Orientation,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let host = refs::resolve_wall(plan.document(), wall, path)?;
    let (wall_id, wall_len, thickness) = (host.id, host.length_mm(), host.thickness_mm);
    let wall_label = refs::wall_label(plan.document(), wall_id);
    let placement = placement.unwrap_or_else(|| match refs::parse_point(wall) {
        Some(p) => Placement::OffsetMm(refs::project(host, p).0),
        None => Placement::Fraction(0.5),
    });
    let u = placement_u(placement, wall_len);

    // The opening cuts the full thickness of its host by default (R4.0.4
    // caps `depthMm` at the host's thickness). A niche shallower than the
    // wall is a later `set-opening` edit, not a creation default.
    let mut opening = match kind {
        OpeningKind::Door => Opening::door(wall_id, u, width_mm, height_mm, thickness),
        OpeningKind::Window => Opening::window(wall_id, u, width_mm, height_mm, sill_mm, thickness),
    };
    if let Some(m) = orientation.mechanism {
        if !m.accepts(kind) {
            return Err(CliError::Usage(format!(
                "mechanism {} does not apply to a {}",
                erased::Ser::to_json(&m),
                if kind == OpeningKind::Door {
                    "door"
                } else {
                    "window"
                }
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
    plan.commit_and_save(vec![Operation::create(Entity::Opening(opening))])?;

    let doc = plan.document();
    let label = doc
        .openings
        .iter()
        .find(|o| o.id == opening_id)
        .map(|o| refs::opening_label(doc, o))
        .unwrap_or_default();
    let mut msg = format!(
        "Added {} {opening_id} ({label}) to wall {wall_label} ({wall_len:.0} mm), centred {:.0} mm from its start.",
        match kind {
            OpeningKind::Door => "door",
            OpeningKind::Window => "window",
        },
        u * wall_len
    );
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

pub fn cmd_remove_wall(path: &Path, wall: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let wall_id = refs::resolve_wall(plan.document(), wall, path)?.id;
    let wall_index = refs::wall_label(plan.document(), wall_id);

    let edit = remove_wall_operations(plan.document(), wall_id);
    let hosted = edit.cascaded_openings;
    plan.commit_and_save(edit.operations)?;

    let mut msg = if hosted == 0 {
        format!("Removed wall {wall_index}.")
    } else {
        format!("Removed wall {wall_index} and {hosted} opening(s) it hosted.")
    };
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Removes an opening named by any reference [`refs::resolve_opening`]
/// accepts.
pub fn cmd_remove_opening(path: &Path, opening: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let found = refs::resolve_opening(plan.document(), opening, path)?;
    let (id, opening_index) = (found.id, refs::opening_label(plan.document(), found));
    plan.commit_and_save(vec![Operation::delete(id)])?;
    let mut msg = format!("Removed opening {opening_index}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Summarises the plan.
pub fn cmd_info(path: &Path) -> Result<String> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let doc = &projection.document;

    let mut out = String::new();
    out.push_str(&format!("{}\n", path.display()));
    out.push_str(&format!(
        "  format version: {}\n  profile:        {}\n",
        pkg.manifest.format_version, pkg.manifest.profile
    ));
    out.push_str(&format!(
        "  operations:     {} in {} batch(es)\n",
        pkg.batches
            .iter()
            .map(|b| b.operations.len())
            .sum::<usize>(),
        pkg.batches.len()
    ));
    out.push_str(&format!(
        "  levels: {}   walls: {}   openings: {}   joins: {}   room records: {}\n",
        doc.levels.len(),
        doc.walls.len(),
        doc.openings.len(),
        doc.joins.len(),
        doc.room_records.len()
    ));

    let numbering = refs::Numbering::new(doc);
    for level in display_levels(doc) {
        let walls: Vec<_> = doc.walls_on_level(level.id).collect();
        let total: f64 = walls.iter().map(|w| w.length_mm()).sum();
        // The stored list ordinal prints only when the document customizes
        // its level list (issue 0115): an absent ordinal is the
        // elevation-derived order, and printing a zero for "not stored"
        // would look like an order the user set.
        let order = level
            .display_order
            .map(|order| format!("   list order {order}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "\n  {} (elevation {:.0} mm{order})\n    walls: {}   total length: {:.0} mm\n",
            level.name,
            level.elevation_mm,
            walls.len(),
            total
        ));
        // Index walls the way add-opening expects them: 1-based, document order.
        for (i, wall) in doc.walls.iter().enumerate() {
            if wall.level_id != level.id {
                continue;
            }
            let openings = doc.openings_on_wall(wall.id);
            out.push_str(&format!(
                "      {}. #{} ({:.0},{:.0}) -> ({:.0},{:.0})  {:.0} mm",
                i + 1,
                refs::short_id(wall.id),
                wall.start().x,
                wall.start().y,
                wall.end().x,
                wall.end().y,
                wall.length_mm()
            ));
            if openings.is_empty() {
                out.push('\n');
            } else {
                let list: Vec<String> = openings
                    .iter()
                    .map(|o| {
                        // Centre in millimetres from the wall's start as well
                        // as the persisted fraction: a resize keeps the
                        // fraction, so this is where a drifted door shows.
                        format!(
                            "{} {} {:.0} mm @ {:.0} mm ({:.0}%)",
                            numbering.opening_label(o),
                            match o.kind {
                                OpeningKind::Door => "door",
                                OpeningKind::Window => "window",
                            },
                            o.width_mm,
                            o.u * wall.length_mm(),
                            o.u * 100.0
                        )
                    })
                    .collect();
                out.push_str(&format!("   [{}]\n", list.join(", ")));
            }
        }
    }

    for level in &doc.levels {
        let rooms = rooms::project_level(doc, level.id);
        if rooms.faces.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "\n  {} — {} room(s)\n",
            level.name,
            rooms.faces.len()
        ));
        for (i, room) in rooms.faces.iter().enumerate() {
            // R5.3.3: the label text is the EFFECTIVE record's name, and a
            // face with no candidates shows no label at all. The area always
            // accompanies it, and always in m² with two decimals (R5.3.2),
            // whatever the display-unit preference says about lengths.
            let label = rooms
                .effective_record(i)
                .and_then(|id| doc.room_record(id))
                .map(|r| format!("{:?}   ", r.display_name()))
                .unwrap_or_default();
            out.push_str(&format!(
                "    {}. {label}{} m²   perimeter {:.0} mm   {} corner(s)\n",
                i + 1,
                rooms::display_polygon_area_m2(room).unwrap_or_else(|| "—".to_owned()),
                room.perimeter_mm(),
                room.outer.len()
            ));
        }
        out.push_str("    (floor area, measured to the inner faces of the walls)\n");
        let homeless = rooms
            .records
            .iter()
            .filter(|r| !r.status.is_candidate())
            .count();
        if homeless > 0 {
            out.push_str(&format!(
                "    ({homeless} room record(s) seed no face; `room list` says which)\n"
            ));
        }
    }

    if let Some((min, max)) = doc.bounds() {
        out.push_str(&format!(
            "\n  extent: {:.0} x {:.0} mm  (x {:.0}..{:.0}, y {:.0}..{:.0})\n",
            max.x - min.x,
            max.y - min.y,
            min.x,
            max.x,
            min.y,
            max.y
        ));
    }

    Ok(out)
}

/// Validates the plan, reporting every problem found.
pub fn cmd_validate(path: &Path) -> Result<String> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let errors = projection.document.validate_plan();
    let warnings = domowik_document::warnings::validation_warnings(&projection.document);
    let warning_text = if warnings.is_empty() {
        String::new()
    } else {
        format!(
            "\n{} warning(s):\n  {}",
            warnings.len(),
            warnings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n  ")
        )
    };
    if errors.is_empty() {
        return Ok(format!("{}: valid.{warning_text}", path.display()));
    }
    Err(CliError::Failure(format!(
        "{}: {} problem(s):\n  {}{warning_text}",
        path.display(),
        errors.len(),
        errors
            .iter()
            .map(|e| e.to_string())
            .collect::<Vec<_>>()
            .join("\n  ")
    )))
}

/// Renders a level to an SVG file.
pub fn cmd_export_svg(path: &Path, out: &Path, level_name: Option<&str>) -> Result<String> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let doc = &projection.document;

    let level_id: Uuid = match level_name {
        Some(name) => doc
            .levels
            .iter()
            .find(|l| l.name == name)
            .map(|l| l.id)
            .ok_or_else(|| {
                CliError::Failure(format!(
                    "no level named {name:?}; available: {}",
                    level_list(doc)
                ))
            })?,
        None => doc
            .levels
            .first()
            .map(|l| l.id)
            .ok_or_else(|| CliError::Failure(format!("{} has no levels", path.display())))?,
    };

    let (svg, label_warnings) = svg::render_level_reporting(doc, level_id, &pkg.assets);
    std::fs::write(out, svg.as_bytes())
        .map_err(|e| CliError::Failure(format!("could not write {}: {e}", out.display())))?;

    let count = doc.walls_on_level(level_id).count();
    let mut msg = format!("Wrote {} ({count} wall(s)).", out.display());
    append_label_warnings(&mut msg, &label_warnings);
    Ok(msg)
}

/// Appends every room name the export declined to draw.
///
/// On the SUCCESS message rather than as a failure: the file was written and
/// is worth having. But an export missing a label looks complete to whoever
/// receives it, so the omission has to be said out loud here or it is not said
/// anywhere.
fn append_label_warnings(msg: &mut String, warnings: &[label::LabelWarning]) {
    for warning in warnings {
        msg.push_str(&format!("\n  {warning}"));
    }
}

/// The walls of `level_id` as plan solids, in document order.
///
/// A thin wrapper over [`walls::level_solids`], which owns the id-to-position
/// mapping so the CLI, the benchmark corpus and the locality checker cannot
/// drift apart on it.
fn level_solids(doc: &Document, level_id: Uuid) -> (Vec<WallSolid>, Vec<JoinSpec>) {
    let s = walls::level_solids(doc, level_id);
    (s.walls, s.joins)
}

/// Detaches one endpoint of a wall, by its `info` number, from the join it
/// belongs to (R3.3.7). `end` is `true` for the wall's end, `false` for its
/// start. The endpoint stays where it is; a join left with one member is
/// deleted. Refused when that endpoint is not joined.
pub fn cmd_detach(path: &Path, wall: &str, end: bool) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let wall = refs::resolve_wall(plan.document(), wall, path)?.id;
    let wall_index = refs::wall_label(plan.document(), wall);
    let role = if end {
        domowik_format::Role::End
    } else {
        domowik_format::Role::Start
    };
    let join = member_join(plan.document(), wall, role)
        .ok_or_else(|| {
            CliError::Failure(format!(
                "wall {wall_index}'s {} is not joined",
                if end { "end" } else { "start" }
            ))
        })?
        .id;
    let edit = detach_operations(plan.document(), join, wall, role)
        .map_err(|e| CliError::Failure(e.to_string()))?;
    let deleted = edit.operations.iter().any(|op| {
        matches!(
            op.kind,
            domowik_document::operation::OperationKind::DeleteEntity
        )
    });
    plan.commit_and_save(edit.operations)?;
    Ok(if deleted {
        format!("Detached wall {wall_index}; its join had one member left and was deleted.")
    } else {
        format!("Detached wall {wall_index} from its join.")
    })
}

/// Reverses a wall by its `info` number.
pub fn cmd_reverse_wall(path: &Path, wall: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let wall = refs::resolve_wall(plan.document(), wall, path)?;
    let wall_index = refs::wall_label(plan.document(), wall.id);
    let edit = reverse_wall_operations(plan.document(), wall.id);
    let remapped = edit.moved_with;
    plan.commit_and_save(edit.operations)?;
    let mut msg = if remapped == 0 {
        format!("Reversed wall {wall_index}.")
    } else {
        format!("Reversed wall {wall_index} and remapped {remapped} opening(s).")
    };
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Moves or resizes a wall by its `info` number.
#[allow(clippy::too_many_arguments)]
pub fn cmd_set_wall(
    path: &Path,
    wall: &str,
    start: Option<(f64, f64)>,
    end: Option<(f64, f64)>,
    thickness_mm: Option<f64>,
    height_mm: Option<f64>,
) -> Result<String> {
    if start.is_none() && end.is_none() && thickness_mm.is_none() && height_mm.is_none() {
        return Err(CliError::Usage(
            "nothing to change; pass at least one of --start, --end, --thickness, --height"
                .to_owned(),
        ));
    }

    let mut plan = Plan::open(path)?;
    let wall_id = refs::resolve_wall(plan.document(), wall, path)?.id;
    let wall_index = refs::wall_label(plan.document(), wall_id);

    let edit = set_wall_operations(
        plan.document(),
        wall_id,
        start.map(|(x, y)| Point::new(x, y)),
        end.map(|(x, y)| Point::new(x, y)),
        thickness_mm,
        height_mm,
    )?;
    let (moved_with, new_joins) = (edit.moved_with, edit.joins);
    plan.commit_and_save(edit.operations)?;

    let mut msg = format!("Updated wall {wall_index}.");
    if moved_with > 0 {
        msg.push_str(&format!(" Moved {moved_with} joined endpoint(s) with it."));
    }
    if new_joins > 0 {
        msg.push_str(&format!(" Joined at {new_joins} new corner(s)."));
    }
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Changes an opening's parameters. `opening` is any reference
/// [`refs::resolve_opening`] accepts; `placement` moves its centre.
#[allow(clippy::too_many_arguments)]
pub fn cmd_set_opening(
    path: &Path,
    opening: &str,
    placement: Option<Placement>,
    width_mm: Option<f64>,
    height_mm: Option<f64>,
    sill_mm: Option<f64>,
    orientation: Orientation,
) -> Result<String> {
    if placement.is_none()
        && width_mm.is_none()
        && height_mm.is_none()
        && sill_mm.is_none()
        && orientation.is_empty()
    {
        return Err(CliError::Usage(
            "nothing to change; pass at least one of --at, --offset, --width, --height, --sill, \
             --hinge, --opens-toward, --swing, --mechanism"
                .to_owned(),
        ));
    }
    let mut plan = Plan::open(path)?;
    let found = refs::resolve_opening(plan.document(), opening, path)?;
    let (id, opening_index) = (found.id, refs::opening_label(plan.document(), found));
    let host_len = plan
        .document()
        .wall(found.host_wall_id)
        .map_or(0.0, |w| w.length_mm());
    let at = placement.map(|p| placement_u(p, host_len));

    let mut ops = orientation.operations(id);
    for (name, value) in [
        ("u", at),
        ("widthMm", width_mm),
        ("heightMm", height_mm),
        ("sillMm", sill_mm),
    ] {
        if let Some(v) = value {
            ops.push(set_field(id, name, serde_json::json!(v)));
        }
    }
    plan.commit_and_save(ops)?;
    let mut msg = format!("Updated opening {opening_index}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Adds a level.
pub fn cmd_add_level(path: &Path, name: &str, elevation_mm: f64) -> Result<String> {
    let mut plan = Plan::open(path)?;
    if plan.document().levels.iter().any(|l| l.name == name) {
        return Err(CliError::Failure(format!(
            "{} already has a level named {name:?}",
            path.display()
        )));
    }
    let mut level = domowik_format::Level::new(name);
    level.elevation_mm = elevation_mm;
    plan.commit_and_save(vec![Operation::create(Entity::Level(level))])?;
    let mut msg = format!("Added level {name:?} at {elevation_mm:.0} mm.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Renames a level or changes its elevation.
pub fn cmd_set_level(
    path: &Path,
    name: &str,
    rename: Option<&str>,
    elevation_mm: Option<f64>,
) -> Result<String> {
    cmd_set_level_fields(
        path,
        name,
        &domowik_edit::LevelFields {
            name: rename.map(str::to_owned),
            elevation_mm,
            ..Default::default()
        },
    )
}

/// Applies one atomic level-properties edit through the shared core builder.
/// Duplicate display names are ambiguous; they never silently select a level.
/// An unchanged edit writes neither history nor file bytes.
pub fn cmd_set_level_fields(
    path: &Path,
    name: &str,
    fields: &domowik_edit::LevelFields,
) -> Result<String> {
    if fields.name.is_none()
        && fields.elevation_mm.is_none()
        && fields.floor_thickness_mm.is_none()
        && fields.default_wall_height_mm.is_none()
        && fields.default_ceiling_height_mm.is_none()
    {
        return Err(CliError::Usage(
            "nothing to change; pass --rename, --elevation, --floor-thickness, --wall-height or --ceiling-height".to_owned(),
        ));
    }
    let mut plan = Plan::open(path)?;
    if plan
        .document()
        .levels
        .iter()
        .filter(|level| level.name == name)
        .count()
        > 1
    {
        return Err(CliError::Failure(format!("multiple levels are named {name:?}; select and rename the intended level in the editor before using name-based commands")));
    }
    let id = plan
        .document()
        .levels
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.id)
        .ok_or_else(|| {
            CliError::Failure(format!(
                "no level named {name:?} in {}; available: {}",
                path.display(),
                level_list(plan.document())
            ))
        })?;

    let ops = domowik_edit::set_level_operations(plan.document(), id, fields)?;
    if ops.is_empty() {
        return Ok(format!("Level {name:?} is unchanged."));
    }
    plan.commit_and_save(ops)?;
    let mut msg = format!("Updated level {name:?}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Stores the level-list display order (BEHAVIOR-SPEC §2.2, issue 0115).
///
/// `levels` names every level of the plan, top of the list first; each level
/// is assigned its position as its `displayOrder` ordinal. Addressing uses
/// the same unambiguous-name rule as `set-level`: a duplicate display name
/// naming any ONE listed level makes the mapping ambiguous and is refused,
/// never silently resolved. An unchanged list writes nothing.
pub fn cmd_order_levels(path: &Path, levels: &[String]) -> Result<String> {
    if levels.is_empty() {
        return Err(CliError::Usage(
            "give every level name, top of the list first".to_owned(),
        ));
    }
    let mut plan = Plan::open(path)?;
    let names = {
        let doc = plan.document();
        let mut ids = Vec::with_capacity(levels.len());
        for name in levels {
            let count = doc.levels.iter().filter(|l| &l.name == name).count();
            let id = match count {
                1 => doc.levels.iter().find(|l| &l.name == name).unwrap().id,
                0 => {
                    return Err(CliError::Failure(format!(
                        "no level named {name:?} in {}; available: {}",
                        path.display(),
                        level_list(doc)
                    )))
                }
                _ => {
                    return Err(CliError::Failure(format!(
                        "multiple levels are named {name:?}; select and rename the intended level in the editor before using name-based commands"
                    )))
                }
            };
            if ids.contains(&id) {
                return Err(CliError::Failure(format!(
                    "level {name:?} is listed more than once"
                )));
            }
            ids.push(id);
        }
        ids
    };

    let ops = domowik_edit::reorder_levels_operations(plan.document(), &names)?;
    if ops.is_empty() {
        return Ok("The level list is already in this order.".to_owned());
    }
    plan.commit_and_save(ops)?;
    let ordered = levels
        .iter()
        .map(|name| format!("{name:?}"))
        .collect::<Vec<_>>()
        .join(", then ");
    let mut msg = format!("Level list updated to run: {ordered}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// A machine-readable report of a plan, for `info --json` and `validate --json`.
///
/// Issue 0014 requires output that "identifies paths/entity IDs/digests". The
/// human report identifies walls by their 1-based position, which is what a
/// person can see on a drawing and type back; this report identifies them by
/// UUID, which is what a script needs and what survives a document being
/// reordered.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub path: String,
    /// The document's stable identity (issue 0116), when the document carries
    /// one; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<Uuid>,
    /// The §8.1 V1 schema identifier the package declares, a `MAJOR.MINOR`
    /// string. Not a number: §8.1 makes it an opaque label a writer states.
    pub format_version: String,
    /// §7.9's `thin` or `portable`.
    pub profile: String,
    pub batches: usize,
    pub operations: usize,
    pub valid: bool,
    pub problems: Vec<String>,
    /// Warning-class conditions do not affect `valid` or the exit code.
    pub warnings: Vec<WarningReport>,
    pub levels: Vec<LevelReport>,
    /// Operations replay discarded as stale. A projection outcome rather than
    /// an error (FORMAT-SPEC I5), and invisible in the human report, so a
    /// script that cares can see it.
    pub discarded: Vec<DiscardReport>,
}

#[derive(Debug, serde::Serialize)]
pub struct WarningReport {
    /// Either a §2.1 level advisory or an R3.1.2 `wall-area-overlap` pair —
    /// [`domowik_document::warnings::Warning`] is untagged, so this field's
    /// JSON shape is unchanged: a `{code, ...}` object, exactly as when it
    /// only ever held a [`domowik_format::ValidationWarning`].
    #[serde(flatten)]
    pub warning: domowik_document::warnings::Warning,
    pub message: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelReport {
    pub id: Uuid,
    pub name: String,
    pub elevation_mm: f64,
    pub floor_thickness_mm: f64,
    pub default_wall_height_mm: f64,
    pub default_ceiling_height_mm: f64,
    /// The stored level-list ordinal (issue 0115), when the document
    /// customizes its level list; omitted otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_order: Option<u64>,
    pub walls: Vec<WallReport>,
    pub rooms: Vec<RoomReport>,
    /// Room records on this level, in stable-UUID order (BEHAVIOR-SPEC §5.2).
    ///
    /// Separate from `rooms` because the two are different things: `rooms` are
    /// detected FACES, which exist whether or not any record names them, and
    /// these are the authoritative records, which persist whether or not any
    /// face contains their seed.
    pub room_records: Vec<RoomRecordReport>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WallReport {
    pub id: Uuid,
    /// The last six hex digits of `id`, accepted wherever a wall is named.
    pub short_id: String,
    /// The 1-based number the human report and the editing commands use.
    pub index: usize,
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    pub thickness_mm: f64,
    pub height_mm: f64,
    pub length_mm: f64,
    pub openings: Vec<OpeningReport>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpeningReport {
    pub id: Uuid,
    /// The last six hex digits of `id`, accepted wherever an opening is named.
    pub short_id: String,
    /// The 1-based number in document order that the commands also accept.
    pub index: usize,
    pub kind: String,
    pub u: f64,
    /// The centre in millimetres along the host from its start: `u` times
    /// the host's current length.
    pub centre_offset_mm: f64,
    /// The centre as a plan point.
    pub centre_mm: [f64; 2],
    /// `doorHinged`, `doorSliding`, `windowSideHung`, `windowDoubleSwing` or
    /// `windowLift`; the kind's default when the plan does not say.
    pub mechanism: domowik_format::OpeningMechanism,
    /// `nearStart` or `nearEnd`: the hinged end of the host axis.
    pub hinge: domowik_format::Hinge,
    /// `Left` or `Right`: the side the leaf swings into, left being on the
    /// left walking from the host's start to its end.
    pub opens_toward: domowik_format::Side,
    pub swing_deg: f64,
    pub width_mm: f64,
    pub height_mm: f64,
    pub sill_mm: f64,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomReport {
    /// Floor area, measured to the inner faces of the walls (R5.1.1).
    pub area_mm2: f64,
    /// The same area under the R5.3.2 display rule: square metres, two
    /// decimals, exact halves to even. Reported alongside the raw measure so a
    /// caller showing a plan and a caller computing with one need not each
    /// reimplement the rounding and disagree about 30.245.
    pub display_area_m2: String,
    pub perimeter_mm: f64,
    /// The canonical exterior ring (R3.2.4), so a caller can draw it.
    pub boundary_mm: Vec<[f64; 2]>,
    pub holes_mm: Vec<Vec<[f64; 2]>>,
    /// The record effectively associated with this face (R5.2.4), if any.
    pub record_id: Option<Uuid>,
    /// The label text: the effective record's name when present and non-empty,
    /// otherwise the `Room` fallback. Absent when the face has no candidates
    /// and therefore no label at all (R5.3.3).
    pub label: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomRecordReport {
    pub id: Uuid,
    /// The last six hex digits of `id`, accepted by the `room` commands.
    pub short_id: String,
    /// The 1-based number `room list`, `room set` and `room remove` use.
    pub index: usize,
    pub seed_mm: [f64; 2],
    /// The authoritative name. `null` and `""` are DIFFERENT values here and
    /// the report keeps them apart, even though both display as `Room`.
    pub name: Option<String>,
    pub floor_style_ref: Option<String>,
    pub ceiling_style_ref: Option<String>,
    /// One of `associated`, `superseded`, `seed-on-boundary`, `seed-in-wall`,
    /// `seed-in-exterior` — derived, never stored (R5.2.4).
    pub status: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardReport {
    pub batch_id: Uuid,
    pub operation_id: Uuid,
    pub entity_id: Uuid,
    pub reason: String,
}

/// Builds the machine-readable report for `path`.
pub fn report(path: &Path) -> Result<Report> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let doc = &projection.document;
    let numbering = refs::Numbering::new(doc);
    let problems: Vec<String> = doc.validate_plan().iter().map(|e| e.to_string()).collect();

    let levels = display_levels(doc)
        .into_iter()
        .map(|level| {
            let walls = doc
                .walls
                .iter()
                .enumerate()
                .filter(|(_, w)| w.level_id == level.id)
                .map(|(i, w)| WallReport {
                    id: w.id,
                    short_id: refs::short_id(w.id),
                    index: i + 1,
                    start_mm: w.start_mm,
                    end_mm: w.end_mm,
                    thickness_mm: w.thickness_mm,
                    height_mm: w.height_mm,
                    length_mm: w.length_mm(),
                    openings: doc
                        .openings_on_wall(w.id)
                        .iter()
                        .map(|o| OpeningReport {
                            id: o.id,
                            short_id: refs::short_id(o.id),
                            index: numbering.opening(o.id),
                            kind: match o.kind {
                                OpeningKind::Door => "door".to_owned(),
                                OpeningKind::Window => "window".to_owned(),
                            },
                            u: o.u,
                            centre_offset_mm: o.u * w.length_mm(),
                            mechanism: o.effective_mechanism(),
                            hinge: o.hinge,
                            opens_toward: o.opens_toward,
                            swing_deg: o.swing_deg,
                            centre_mm: {
                                let (a, b) = (w.start(), w.end());
                                [a.x + (b.x - a.x) * o.u, a.y + (b.y - a.y) * o.u]
                            },
                            width_mm: o.width_mm,
                            height_mm: o.height_mm,
                            sill_mm: o.sill_mm,
                        })
                        .collect(),
                })
                .collect();
            let projected = rooms::project_level(doc, level.id);
            let numbers: Vec<Uuid> = numbered_records(doc).iter().map(|r| r.id).collect();
            let rooms = projected
                .faces
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let record_id = projected.effective_record(i);
                    RoomReport {
                        area_mm2: r.area_mm2(),
                        display_area_m2: rooms::display_polygon_area_m2(r)
                            .unwrap_or_else(|| "—".to_owned()),
                        perimeter_mm: r.perimeter_mm(),
                        boundary_mm: r.outer.iter().map(|p| [p.x, p.y]).collect(),
                        holes_mm: r
                            .holes
                            .iter()
                            .map(|h| h.iter().map(|p| [p.x, p.y]).collect())
                            .collect(),
                        record_id,
                        label: record_id
                            .and_then(|id| doc.room_record(id))
                            .map(|r| r.display_name().to_owned()),
                    }
                })
                .collect();
            let room_records = projected
                .records
                .iter()
                .filter_map(|p| {
                    let record = doc.room_record(p.id)?;
                    Some(RoomRecordReport {
                        id: record.id,
                        short_id: refs::short_id(record.id),
                        index: numbers.iter().position(|id| *id == record.id)? + 1,
                        seed_mm: record.seed_mm,
                        name: record.name.clone(),
                        floor_style_ref: record.floor_style_ref.clone(),
                        ceiling_style_ref: record.ceiling_style_ref.clone(),
                        status: p.status.token().to_owned(),
                    })
                })
                .collect();
            LevelReport {
                id: level.id,
                name: level.name.clone(),
                elevation_mm: level.elevation_mm,
                floor_thickness_mm: level.floor_thickness_mm,
                default_wall_height_mm: level.default_wall_height_mm,
                default_ceiling_height_mm: level.default_ceiling_height_mm,
                // The stored list ordinal when the document carries one;
                // absent otherwise (issue 0115).
                display_order: level.display_order,
                walls,
                rooms,
                room_records,
            }
        })
        .collect();

    Ok(Report {
        path: path.display().to_string(),
        document_id: doc.document_id,
        format_version: pkg.manifest.format_version.clone(),
        profile: pkg.manifest.profile.to_string(),
        batches: pkg.batches.len(),
        operations: pkg.batches.iter().map(|b| b.operations.len()).sum(),
        valid: problems.is_empty(),
        warnings: domowik_document::warnings::validation_warnings(doc)
            .into_iter()
            .map(|warning| WarningReport {
                message: warning.to_string(),
                warning,
            })
            .collect(),
        problems,
        levels,
        discarded: projection
            .discarded
            .iter()
            .map(|d| DiscardReport {
                batch_id: d.batch_id,
                operation_id: d.operation_id,
                entity_id: d.entity_id,
                reason: format!("{:?}", d.reason),
            })
            .collect(),
    })
}

/// `info --json`.
pub fn cmd_info_json(path: &Path) -> Result<String> {
    let report = report(path)?;
    serde_json::to_string_pretty(&report)
        .map_err(|e| CliError::Failure(format!("could not encode the report: {e}")))
}

/// `validate --json`.
///
/// Exits NON-ZERO on an invalid plan exactly as the human form does, and still
/// prints the report — a caller that asked for machine-readable output wants
/// the problems, not just the exit code.
pub fn cmd_validate_json(path: &Path) -> Result<String> {
    let report = report(path)?;
    let body = serde_json::to_string_pretty(&report)
        .map_err(|e| CliError::Failure(format!("could not encode the report: {e}")))?;
    if report.valid {
        Ok(body)
    } else {
        Err(CliError::Report(body))
    }
}

/// Imports a file into the plan's blob store, optionally placing it as a
/// background image on a level.
///
/// FORMAT-SPEC §6 and ASSET-STRATEGY §10: content-addressed by
/// `sha256:<lowercase hex>`, so importing the same bytes twice stores one copy
/// — deduplication is not a feature here, it is what content addressing means.
pub fn cmd_asset_add(
    path: &Path,
    file: &Path,
    level_name: Option<&str>,
    scale: f64,
) -> Result<String> {
    let bytes = std::fs::read(file)
        .map_err(|e| CliError::Failure(format!("could not read {}: {e}", file.display())))?;
    if bytes.is_empty() {
        return Err(CliError::Failure(format!("{} is empty", file.display())));
    }
    if !scale.is_finite() || scale <= 0.0 {
        return Err(CliError::Usage(format!(
            "scale must be a positive number of millimetres per pixel, got {scale}"
        )));
    }

    let digest = domowik_container::digest_of(&bytes);
    let mut plan = Plan::open(path)?;
    let already = plan.assets.contains_key(&digest);
    let size = bytes.len();

    // The §10.2 descriptor, written with the blob. Without one, §9.6's matrix
    // has no requirement level to read and every reference is undeclared.
    //
    // `optional` is NOT a choice made here. BEHAVIOR-SPEC §2.5 says a level
    // references background images "through asset descriptors whose
    // requirement level is optional", so a missing scan must leave the
    // document editable with a non-blocking diagnostic. Writing `required`
    // would make one unreadable background image lock a whole plan.
    let descriptor = domowik_assets::AssetDescriptor::for_import(
        digest.clone(),
        size as u64,
        &bytes,
        "background-image",
        domowik_assets::Requirement::Optional,
        file.file_name().map(|n| n.to_string_lossy().into_owned()),
        Some(rfc3339_now()),
    );
    // A descriptor already here KEEPS its place. The digest is the same, so
    // the bytes are the same, so the existing descriptor still describes them
    // correctly — and it may carry a licence, an author or a hint that a user
    // put there. Overwriting it on re-import destroyed exactly that, which is
    // the silent stripping §8 U2 forbids, committed by the one command whose
    // job is to ADD information.
    let kept_descriptor = plan.descriptors().contains_key(&digest);

    let faults = descriptor.validate();
    if !faults.is_empty() {
        // Unreachable for a descriptor this function builds, and checked
        // anyway: a descriptor that fails its own §10.2 rules must never reach
        // a package, because every later reader trusts that it did not.
        return Err(CliError::Failure(format!(
            "refusing to write a descriptor that fails its own validation: {}",
            faults
                .iter()
                .map(|f| f.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    if !kept_descriptor {
        plan.insert_descriptor(
            &digest,
            serde_json::to_value(&descriptor).expect("a descriptor serialises"),
        )?;
    }

    plan.assets.insert(digest.clone(), bytes);

    let level_id = resolve_level(plan.document(), level_name, path)?;
    let level = plan
        .document()
        .level(level_id)
        .expect("resolve_level returned it")
        .clone();

    let placed = if level.background_images.iter().any(|b| b.digest == digest) {
        // Already placed here. The blob still has to reach the file, so save
        // without committing an operation that would change nothing.
        plan.save()?;
        None
    } else {
        let mut images = level.background_images.clone();
        let ordinal = images.iter().map(|b| b.stack_ordinal).max().unwrap_or(-1) + 1;
        images.push(domowik_format::BackgroundImage {
            unknown: Default::default(),
            digest: digest.clone(),
            anchor: domowik_format::Anchor {
                translation_mm: [0.0, 0.0],
                scale_uniform: scale,
                rotation_deg: 0.0,
                unknown: Default::default(),
            },
            stack_ordinal: ordinal,
        });
        plan.commit_and_save(vec![set_field(
            level_id,
            "backgroundImages",
            serde_json::to_value(&images).expect("images serialise"),
        )])?;
        Some(level.name.clone())
    };

    let mut msg = format!(
        "Imported {} ({size} bytes) as {digest}{}.",
        file.display(),
        if kept_descriptor {
            ""
        } else {
            ", with an optional-requirement descriptor"
        }
    );
    if kept_descriptor {
        msg.push_str(" Its existing descriptor was left as it was.");
    }
    if already {
        msg.push_str(" It was already stored; content-addressed blobs deduplicate.");
    }
    if let Some(level) = placed {
        msg.push_str(&format!(" Placed on {level:?} at {scale} mm per pixel."));
    }
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Writes a PORTABLE copy of a plan: every referenced blob embedded and
/// digest-verified (ASSET-STRATEGY §10.4).
///
/// **All-or-nothing.** Any unverifiable reference aborts packing completely
/// with an omission report naming EVERY such asset — not the first one. A
/// report that stops at the first failure makes a user fix one thing, run
/// again, and find another; §10.4 and BEHAVIOR-SPEC §9.6 both require the
/// complete list.
///
/// Byte totals are reported for embedded and external material, because §10.4
/// requires conversions to report them BEFORE packing — a user about to
/// produce a 200 MB file should learn it from the tool.
///
/// `--thin` is NOT implemented and is refused rather than approximated: a thin
/// package keeps its blobs in a cache or a policy-gated remote store, and
/// neither exists. Writing one anyway would produce a package whose references
/// dangle by construction, which is the opposite of a profile.
pub fn cmd_pack(
    path: &Path,
    out: &Path,
    portable: bool,
    thin: bool,
    cache_root: Option<&Path>,
) -> Result<String> {
    let profile = match (portable, thin) {
        (true, true) => {
            return Err(CliError::Usage(
                "--portable and --thin are the two profiles; pick one".to_owned(),
            ))
        }
        (false, false) => {
            return Err(CliError::Usage(
                "pass --portable or --thin to say which profile to write".to_owned(),
            ))
        }
        (true, false) => Profile::Portable,
        (false, true) => Profile::Thin,
    };

    let pkg = read_package(path)?;
    let doc = pkg.project().map_err(CliError::from)?.document;

    // U(P): the asset-root table plus anything the document references. The
    // table alone is not enough — a reference this version understands might
    // predate the table — and the document alone is not enough either, which
    // is invariant I2's whole point.
    // A SET, not main's `Vec`: `plan_pack` takes `&BTreeSet` because U(P) is a
    // set in §10.5 and the profiles quantify over it. Sourced from main's
    // `asset_digests()`, which is now the only way to read the table.
    let mut universe: std::collections::BTreeSet<String> =
        pkg.manifest.asset_digests().into_iter().collect();
    for level in &doc.levels {
        for image in &level.background_images {
            universe.insert(image.digest.clone());
        }
    }

    let store = match cache_root {
        Some(root) => Some(CacheStore::open(root, AdmissionLimits::default())?),
        None => None,
    };

    // A thin conversion MOVES blobs into the cache. Doing it before planning
    // is not an optimisation, it is the order that cannot lose data: the cache
    // is durable before the thin package is written, so an interruption leaves
    // the original package untouched and the cache merely over-full.
    let mut admitted = 0usize;
    if profile == Profile::Thin {
        let Some(store) = store.as_ref() else {
            return Err(CliError::Usage(
                "--thin needs --cache <dir>: a thin package keeps its blobs in a cache or behind \
                 a hint, and one written with nowhere to put them would dangle by construction"
                    .to_owned(),
            ));
        };
        let mut meta = MetaStore::open(cache_root.expect("cache root, checked above"))?;
        let mut roots = Vec::new();
        for (digest, bytes) in &pkg.assets {
            if store.admit_bytes(digest, bytes)? == domowik_assets::Admission::Stored {
                admitted += 1;
            }
            roots.push((digest.clone(), RootKind::RootTable));
        }
        // Pin what is about to leave the package, under the output path, as
        // ONE registration. Without the pins the next collection would take
        // the blobs the new thin package depends on, because nothing else
        // names them; pinning them one at a time would leave a collector free
        // to run between two of them.
        meta.register_document(&out.display().to_string(), &roots, &rfc3339_now())?;
    }

    let descriptors: Vec<domowik_assets::AssetDescriptor> = Vec::new();
    let inputs = domowik_assets::PackInputs {
        universe: &universe,
        embedded: &pkg.assets,
        cache: store.as_ref(),
        descriptors: &descriptors,
    };
    let plan = domowik_assets::plan_pack(profile, &inputs)
        .map_err(|report| CliError::Failure(format!("{}Nothing was written.", report.render())))?;

    let mut manifest = pkg.manifest.clone();
    // main's setter, not a field assignment: it KEEPS whatever each surviving
    // asset-root member carried beside its digest, which rebuilding from
    // digests alone would strip on every write-back (§8.4 U2).
    manifest.set_asset_digests(universe.iter().cloned());

    let assets = match profile {
        Profile::Portable => {
            // Everything in the plan, pulled from the package or the cache.
            let mut assets = pkg.assets.clone();
            for digest in &plan.embed {
                if assets.contains_key(digest) {
                    continue;
                }
                let store = store.as_ref().expect("planned from the cache");
                let mut bytes = Vec::new();
                let mut blob = store.open_blob(digest)?.ok_or_else(|| {
                    CliError::Failure(format!("{digest} vanished from the cache"))
                })?;
                std::io::Read::read_to_end(&mut blob, &mut bytes)
                    .map_err(|e| CliError::Failure(e.to_string()))?;
                assets.insert(digest.clone(), bytes);
            }
            assets
        }
        // Every blob now lives in the cache, pinned. §10.4 reserves a thin
        // package's embedded set for preview-class blobs, and §10.5 makes
        // previews non-members of U(P); this build has no preview namespace,
        // so a thin package embeds nothing.
        // Carried blobs are not lost with them: the thin branch above admits
        // EVERY embedded blob to the cache and pins it, `U(P)` member or not,
        // so a blob referenced only by data this version cannot read moves
        // rather than disappearing (§7.4 I2).
        Profile::Thin => std::collections::BTreeMap::new(),
    };

    let outgoing = Package {
        manifest,
        snapshot: pkg.snapshot.clone(),
        frontier: pkg.frontier.clone(),
        tombstones: pkg.tombstones.clone(),
        snapshot_unknown: pkg.snapshot_unknown.clone(),
        descriptors: pkg.descriptors.clone(),
        batches: pkg.batches.clone(),
        assets,
    };
    write_package(out, &outgoing)?;

    // Reports the profile the file DECLARES, derived through the one shared
    // rule the writer uses, never the flag the caller passed.
    //
    // §7.9 makes the profile a fact about the package rather than a
    // preference, so a request can be overtaken by the facts: with an empty
    // U(P) BOTH `--portable` and `--thin` emit portable (issue 0064), because
    // there is no blob that could fail to embed. Reporting the request would
    // then print `thin` over a manifest saying `portable` — the same class of
    // contradiction issue 0064 was opened to remove, merely pointing the other
    // way, and a reader trusting the message would be misinformed about the
    // file they now have.
    //
    // When the two differ the message SAYS so rather than silently
    // substituting, because a user who typed `--thin` is entitled to know the
    // flag did not survive contact with the format.
    let totals = plan.totals;
    // What the file actually declares. Derived, not assumed, and not re-read
    // from disk: `Profile::derive` is the same function the writer used.
    // NOTE the two `Profile` types: `profile` here is `domowik_assets::Profile`,
    // the packing plan's, while the manifest field is
    // `domowik_container::Profile`. They are compared on §7.9's wire token,
    // which is the vocabulary both are spelling.
    let emitted =
        domowik_container::Profile::derive(&outgoing.manifest.asset_root, &outgoing.assets);
    let mut msg = format!(
        "Wrote {} as a {} package: {} referenced asset(s){}.\n  \
         package {} byte(s), cache {} byte(s), remote {} byte(s) declared.",
        out.display(),
        emitted,
        universe.len(),
        match emitted {
            domowik_container::Profile::Portable => ", all embedded and verified",
            domowik_container::Profile::Thin => ", none embedded",
        },
        totals.package_bytes,
        totals.cache_bytes,
        totals.remote_bytes
    );
    if emitted.to_string() != profile.as_str() {
        msg.push_str(&format!(
            "\n  --{} was requested; a plan with no assets is {} under \
             FORMAT-SPEC §7.9, because there is no blob that could be left out.",
            profile.as_str(),
            emitted
        ));
    }
    if emitted == domowik_container::Profile::Thin {
        msg.push_str(&format!(
            "\n  {admitted} blob(s) moved into the cache and pinned under {}; \
             releasing that pin is `asset cache unpin --owner {}`.",
            out.display(),
            out.display()
        ));
    }
    if !plan.carried.is_empty() {
        msg.push_str(&format!(
            "\n  {} blob(s) carried for references this version does not understand.",
            plan.carried.len()
        ));
    }
    Ok(msg)
}

/// Lists the blobs a plan holds and what references them.
pub fn cmd_asset_list(path: &Path, cache_root: Option<&Path>) -> Result<String> {
    let pkg = read_package(path)?;
    let doc = pkg.project().map_err(CliError::from)?.document;

    let mut out = format!("{}\n", path.display());
    if pkg.assets.is_empty() && pkg.manifest.asset_root.is_empty() {
        out.push_str("  no assets\n");
        return Ok(out);
    }
    out.push_str(&format!(
        "  {} embedded blob(s), {} in the asset-root table\n",
        pkg.assets.len(),
        pkg.manifest.asset_root.len()
    ));
    out.push_str(&asset_size_report(&pkg, cache_root)?);
    out.push('\n');
    for digest in &pkg.manifest.asset_digests() {
        let bytes = pkg.assets.get(digest);
        let users: Vec<&str> = doc
            .levels
            .iter()
            .filter(|l| l.background_images.iter().any(|b| &b.digest == digest))
            .map(|l| l.name.as_str())
            .collect();
        out.push_str(&format!(
            "  {digest}\n    {}\n    referenced by: {}\n",
            match bytes {
                Some(b) => format!("{} bytes, embedded", b.len()),
                None => "NOT EMBEDDED (thin reference)".to_owned(),
            },
            if users.is_empty() {
                "nothing this version understands".to_owned()
            } else {
                users.join(", ")
            }
        ));
    }
    Ok(out)
}

/// Verifies every embedded blob against its digest, and every reference
/// against the store.
pub fn cmd_asset_verify(path: &Path) -> Result<String> {
    // Reading already verifies embedded blobs — a mismatch is a read error —
    // so what is left to check is the REFERENCE side: dangling references, and
    // blobs nothing refers to.
    let pkg = read_package(path)?;
    let doc = pkg.project().map_err(CliError::from)?.document;

    let mut referenced: Vec<String> = pkg.manifest.asset_digests();
    for level in &doc.levels {
        for image in &level.background_images {
            if !referenced.contains(&image.digest) {
                referenced.push(image.digest.clone());
            }
        }
    }

    let dangling: Vec<&String> = referenced
        .iter()
        .filter(|d| !pkg.assets.contains_key(*d))
        .collect();
    let orphans: Vec<&String> = pkg
        .assets
        .keys()
        .filter(|d| !referenced.contains(d))
        .collect();

    // §10.2 conformance of the descriptor table, reported alongside the blob
    // digests. `from_object` is the tolerant door on purpose: a malformed
    // descriptor must still be COUNTED here rather than making the whole
    // table unreadable, which is exactly what `verify` exists to surface.
    let (table, faults) = domowik_assets::DescriptorTable::from_object(&pkg.descriptors);

    let mut out = format!(
        "{}: {} blob(s) verified against their digests.\n",
        path.display(),
        pkg.assets.len()
    );
    out.push_str(&format!(
        "  {} descriptor(s) present, {} without one\n",
        table.len(),
        referenced.iter().filter(|d| table.get(d).is_none()).count()
    ));
    if !faults.is_empty() {
        out.push_str(&format!("  {} descriptor fault(s):\n", faults.len()));
        for fault in &faults {
            out.push_str(&format!("    {fault}\n"));
        }
    }
    if !orphans.is_empty() {
        // An orphan is NOT an error: §6 requires garbage collection to be
        // fail-closed, and a blob referenced only by data this version does
        // not understand looks exactly like an orphan from here.
        out.push_str(&format!(
            "  {} blob(s) referenced by nothing this version understands; \
             kept, because collection must be fail-closed.\n",
            orphans.len()
        ));
    }
    if dangling.is_empty() {
        return Ok(out);
    }
    Err(CliError::Failure(format!(
        "{out}  {} dangling reference(s):\n    {}",
        dangling.len(),
        dangling
            .iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join("\n    ")
    )))
}

/// Removes a blob and every reference to it.
pub fn cmd_asset_remove(path: &Path, digest: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    // A DESCRIPTOR alone counts as holding the asset. §10.5 puts descriptor
    // digests in `U(P)`, so an entry whose blob and references are already
    // gone is still part of the referenced universe — and without this it
    // could never be removed, because the two checks below would both miss it.
    if !plan.assets.contains_key(digest)
        && !plan.descriptors().contains_key(digest)
        && !plan
            .document()
            .levels
            .iter()
            .any(|l| l.background_images.iter().any(|b| b.digest == digest))
    {
        return Err(CliError::Failure(format!(
            "{} holds no asset {digest}",
            path.display()
        )));
    }

    // References go first, as operations, so removing an asset is undoable
    // like any other edit. The BLOB is then dropped, which is not undoable —
    // the bytes are gone from the package — and the message says so.
    let mut ops = Vec::new();
    for level in &plan.document().levels.clone() {
        if !level.background_images.iter().any(|b| b.digest == digest) {
            continue;
        }
        let remaining: Vec<_> = level
            .background_images
            .iter()
            .filter(|b| b.digest != digest)
            .cloned()
            .collect();
        ops.push(set_field(
            level.id,
            "backgroundImages",
            serde_json::to_value(&remaining).expect("images serialise"),
        ));
    }

    // The descriptor goes with the blob. Leaving it behind would describe
    // bytes the package no longer holds, and §10.5's `U(P)` is built partly
    // from descriptor digests — so a stale one would keep resurrecting a
    // reference the user asked to remove.
    let had_descriptor = plan.remove_descriptor(digest).is_some();
    plan.assets.remove(digest);
    plan.manifest.asset_root.retain(|r| r.digest != *digest);
    if ops.is_empty() {
        plan.save()?;
    } else {
        plan.commit_and_save(ops)?;
    }
    let mut msg = format!("Removed {digest}. The reference is undoable; the bytes are not.");
    if had_descriptor {
        msg.push_str(" Its descriptor went with it.");
    }
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Renders a level to a PNG file.
pub fn cmd_export_png(
    path: &Path,
    out: &Path,
    level_name: Option<&str>,
    scale: f64,
    review: bool,
) -> Result<String> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let doc = &projection.document;
    let level_id = resolve_level(doc, level_name, path)?;

    let raster = if review {
        png::render_level_review(doc, level_id, scale, &pkg.assets)?
    } else {
        png::render_level_with_assets(doc, level_id, scale, &pkg.assets)?
    };
    std::fs::write(out, &raster.png)
        .map_err(|e| CliError::Failure(format!("could not write {}: {e}", out.display())))?;

    let mut msg = format!(
        "Wrote {} ({}x{} px at {:.3} px/mm).",
        out.display(),
        raster.width,
        raster.height,
        raster.scale
    );
    if raster.scale < scale {
        msg.push_str(&format!(
            " Scale reduced from {scale:.3} to stay within {}x{} px.",
            png::MAX_DIMENSION,
            png::MAX_DIMENSION
        ));
    }
    append_label_warnings(&mut msg, &raster.label_warnings);
    Ok(msg)
}

/// Undoes the most recent local command.
///
/// BEHAVIOR-SPEC R8.1.1: this APPENDS a compensating batch rather than
/// removing history. R8.4.2 makes the stacks session-local and never
/// persisted, so across CLI invocations there is nothing to pop — this command
/// therefore reconstructs a one-entry stack from the LAST batch in the log and
/// inverts it.
///
/// That is a deliberate narrowing, and it is worth being plain about: a CLI
/// process is a session that lives for one command, so `undo` here means
/// "compensate the last batch", not "walk a stack". Repeated calls walk
/// backwards one batch at a time, and a compensating batch is itself the last
/// batch, so `undo` twice in a row undoes and then redoes. An interactive
/// front end holding a Session across commands gets the full R8 semantics with
/// no change to the core.
pub fn cmd_undo(path: &Path) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let Some(last) = plan.session_ref().log().batches().last().cloned() else {
        return Err(CliError::Failure(format!(
            "{} has no operations to undo",
            path.display()
        )));
    };
    let describe = describe_batch(&last);
    // `compensate`, not `commit`: undoing a delete recreates a tombstoned id,
    // which an ordinary command may not do (FORMAT-SPEC E6) and a compensation
    // may. That is the distinction E6 draws, and using `commit` here made
    // undoing an undo fail.
    plan.session()?.compensate(&last)?;
    plan.save()?;
    let mut msg = format!("Undid {describe}.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Applies an external operation batch from a JSON file.
///
/// R8.3.1: an accepted external batch clears both local stacks and never
/// enters them. R8.3.3: it marks the document dirty. Both are the session's
/// job, and this command exists so the rule is exercised by something a user
/// can run.
///
/// The batch is read as the FORMAT-SPEC §4.9 shape. Framing anomalies —
/// duplicate ids, overlapping ranges, a zero frontier member — reject before
/// any effect; a batch whose prerequisites are missing is HELD rather than
/// rejected, and the command says so rather than reporting success.
pub fn cmd_apply(path: &Path, batch_path: &Path, dry_run: bool) -> Result<String> {
    let (batch, enum_codes, raw) = read_external_batch(batch_path)?;
    let describe = describe_batch(&batch);

    let mut plan = Plan::open(path)?;
    if !enum_codes.is_empty() {
        // §4.4 at raw ingress: invalid opening enums were replaced by
        // placeholders only so the batch could be evaluated; it is refused,
        // with R4.4's primary code across every failure (a scratch session
        // takes the admission, the plan is untouched).
        // Admission order is preserved: a framing refusal is reported as
        // itself, and a batch that would be held or is a duplicate keeps that
        // outcome (nothing is saved for either). Only a batch that would take
        // effect — or be stored as discarded, which a placeholder-carrying
        // batch cannot honestly be — is refused, with R4.4's primary code.
        // A batch id already in the log is compared against the ORIGINAL raw
        // content, before placeholders: an identical re-delivery is ignored,
        // a different one is the E1 framing error, as on the typed path.
        let session = plan.session()?;
        if let Some(stored) = session
            .log()
            .batches()
            .iter()
            .chain(session.log().held())
            .find(|b| b.batch_id == batch.batch_id)
        {
            if serde_json::to_value(stored).ok().as_ref() == raw.as_ref() {
                return Ok(format!("Ignored {describe}: already applied."));
            }
            return Err(CliError::Failure(format!(
                "batch {} is already in the log with different content; batch ids are unique \
                 (FORMAT-SPEC E1)",
                batch.batch_id
            )));
        }
        let mut scratch = session.clone();
        let code = match scratch.accept_external(batch) {
            Err(e) => match e.primary_opening_code() {
                None => return Err(e.into()),
                found => found,
            },
            Ok(Delivery::Held) => {
                return Ok(format!(
                    "Held {describe}: its causal prerequisites are not in {}.",
                    path.display()
                ))
            }
            Ok(Delivery::Duplicate) => return Ok(format!("Ignored {describe}: already applied.")),
            Ok(Delivery::Accepted) => None,
        };
        let primary = enum_codes
            .iter()
            .map(|(c, _)| *c)
            .chain(code)
            .min()
            .expect("non-empty");
        return Err(CliError::Failure(format!(
            "command rejected: {primary}: {} is refused",
            batch_path.display()
        )));
    }
    let before = plan.document().entity_count();
    let delivery = plan.session()?.accept_external(batch)?;
    let after = plan.document().entity_count();

    if !dry_run {
        plan.save()?;
    }

    let outcome = match delivery {
        Delivery::Accepted => {
            let net = after as i64 - before as i64;
            let change = match net {
                0 => "no change to the entity count".to_string(),
                n if n > 0 => format!("{n} more entities"),
                n => format!("{} fewer entities", -n),
            };
            let mut msg = if dry_run {
                format!("Would apply {describe}: {change}.")
            } else {
                format!("Applied {describe}: {change}.")
            };
            // Reflects the batch's effect whether or not it was saved: an
            // accepted batch has already been applied to the in-memory
            // document even under --dry-run, which is exactly what makes
            // "would apply" an honest preview rather than a guess.
            append_warnings(&mut msg, plan.document());
            msg
        }
        Delivery::Duplicate => {
            let verb = if dry_run { "Would ignore" } else { "Ignored" };
            format!("{verb} {describe}: already applied.")
        }
        Delivery::Held => {
            let verb = if dry_run { "Would hold" } else { "Held" };
            format!(
                "{verb} {describe}: its causal prerequisites are not in {}.",
                path.display()
            )
        }
    };

    Ok(if dry_run {
        format!("{outcome} Nothing was written (--dry-run).")
    } else {
        outcome
    })
}

/// Reads and parses an external batch document, enforcing §9.3 limits on
/// UNTRUSTED input before it can cost anything.
///
/// Four bounds, in the order a hostile file would exercise them:
///
/// 1. **Declared size**, from the directory entry, checked BEFORE the read.
///    A 40 GB file costs one `stat`, not 40 GB of address space.
/// 2. **Actual bytes read**, capped one over the limit. Step 1 reads a size
///    that may be a lie or may be absent — a FIFO reports zero and then streams
///    forever — so the read itself is bounded independently.
/// 3. **Nesting depth** (row L11), counted over the bytes BEFORE the parser
///    sees them, so a document nesting ten thousand arrays is refused without
///    allocating the 129th level. The library's own recursion limit is not
///    this check: it sits one level tighter than L11 and would refuse a
///    CONFORMING 128-level document, which is why the boundary is counted here
///    and the library's limit stood down for the parse it has cleared.
/// 4. **Operation count** (row L23), checked after parsing and BEFORE the batch
///    reaches the session, which would otherwise validate and project every
///    operation in it.
///
/// The package reader already enforces all three (`read_package_with_limits`),
/// but a batch arriving on the `apply` path never passes through it: it is a
/// loose JSON file named on the command line. Without this it would be bounded
/// only at `save`, having already been read, parsed and applied.
#[allow(clippy::type_complexity)]
fn read_external_batch(
    batch_path: &Path,
) -> Result<(
    Batch,
    Vec<(domowik_format::OpeningCode, Uuid)>,
    Option<serde_json::Value>,
)> {
    use std::io::Read;

    let limits = Limits::default();
    let display = batch_path.display();

    let declared = std::fs::metadata(batch_path)
        .map_err(|e| CliError::Failure(format!("could not read {display}: {e}")))?
        .len();
    if !limits.permits_entry_size(declared) {
        return Err(CliError::Failure(format!(
            "refusing to read {display}: it declares {declared} bytes and the limit is {} \
             (FORMAT-SPEC §9.3)",
            limits.max_entry_bytes
        )));
    }

    let file = std::fs::File::open(batch_path)
        .map_err(|e| CliError::Failure(format!("could not read {display}: {e}")))?;
    let mut bytes = Vec::new();
    file.take(limits.max_entry_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| CliError::Failure(format!("could not read {display}: {e}")))?;
    if bytes.len() as u64 > limits.max_entry_bytes {
        return Err(CliError::Failure(format!(
            "refusing to read {display}: it exceeds the {}-byte limit (FORMAT-SPEC §9.3)",
            limits.max_entry_bytes
        )));
    }

    let checked = domowik_container::check_nesting_depth(&bytes, limits.max_nesting_depth)
        .map_err(|found| {
            CliError::Failure(format!(
                "refusing to read {display}: it nests JSON containers {found} levels deep and the \
                 limit is {} (FORMAT-SPEC §9.3 row L11) [limit-nesting-depth]",
                limits.max_nesting_depth
            ))
        })?;

    let mut value: serde_json::Value = domowik_container::parse_depth_checked(checked)
        .map_err(|e| CliError::Failure(format!("{display} is not a valid batch: {e}")))?;
    let original = value.clone();
    let (batch, enum_codes): (Batch, _) = match serde_json::from_value(value.clone()) {
        Ok(batch) => (batch, Vec::new()),
        Err(e) => {
            // A created opening naming no enum variant is a §4.4 refusal
            // (decided by the caller), not a malformed batch.
            let codes = value
                .get_mut("operations")
                .map(domowik_document::ingress::sanitize_opening_enums)
                .unwrap_or_default();
            match serde_json::from_value(value) {
                Ok(batch) if !codes.is_empty() => (batch, codes),
                _ => {
                    return Err(CliError::Failure(format!(
                        "{display} is not a valid batch: {e}"
                    )))
                }
            }
        }
    };

    let operations = batch.operations.len() as u64;
    if !limits.permits_batch_size(operations) {
        return Err(CliError::Failure(format!(
            "refusing to apply {display}: it carries {operations} operations and the limit is {} \
             (FORMAT-SPEC §9.3 row L23)",
            limits.max_operations_per_batch
        )));
    }

    let raw = (!enum_codes.is_empty()).then_some(original);
    Ok((batch, enum_codes, raw))
}

/// What `compact` was asked to do.
#[derive(Debug, Clone, Default)]
pub struct CompactOptions {
    /// Report the plan and write nothing.
    pub dry_run: bool,
    /// The caller's assertion that this document is not synchronized.
    ///
    /// Execution requires it. A package cannot show that a copy was never
    /// given to a peer that has not written yet, and dropping the log is what
    /// would strand such a peer. The flag does not establish FORMAT-SPEC §5.1
    /// S6's precondition — nothing in v0.1 can — it records who answered for
    /// the history.
    pub assert_local_only: bool,
    /// Emit the machine-readable plan instead of the human summary.
    pub json: bool,
}

/// The compaction plan, reported BEFORE anything is written.
///
/// Compaction is irreversible, so the numbers and the retention consequences
/// are the same ones a `--dry-run` prints: a report produced afterwards
/// describes a decision the caller no longer has.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactPlan {
    /// `preflight` before a write, `written` after one, `refused` when the
    /// operation will not happen, `already-compact` when there is nothing to
    /// fold.
    ///
    /// Every other field below describes the fold this plan DESCRIBES. Only
    /// `written` says whether it happened, so a `refused` plan still reports
    /// the coverage and tombstones a compaction WOULD have recorded.
    pub phase: &'static str,
    pub target: String,
    pub current_batches: usize,
    pub current_operations: usize,
    pub folded_batches: usize,
    /// The §7.6 entries folding would discard, in canonical order.
    pub folded_entries: Vec<String>,
    /// Per actor, the highest counter the fold covers: the §5.1 S2 frontier
    /// the rewritten snapshot must record.
    pub folded_through_frontier: BTreeMap<Uuid, u64>,
    /// Ids the compacted snapshot keeps a delete for (§5.1 S4).
    pub retained_tombstones: Vec<Uuid>,
    /// Every actor the retained history names. INFORMATIONAL: B2 makes an
    /// actor a writer epoch rather than a peer, so this is not a count of
    /// replicas and does not gate anything.
    pub actors: Vec<Uuid>,
    /// True when that list holds at most one actor. Reported because it is
    /// worth seeing before an irreversible fold, and for no other reason: it
    /// is neither a proof of locality nor a gate.
    pub single_actor_history: bool,
    pub assert_local_only: bool,
    pub undo_redo_effect: &'static str,
    pub recovery_effect: &'static str,
    /// Whether this report describes a write that HAPPENED.
    pub written: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refusal: Option<CompactRefusal>,
}

/// Why a compaction did not drop the log (FORMAT-SPEC §5.1 S6).
///
/// S6 requires the operation's observable result to name three things when a
/// drop is refused, and forbids recording the refusal only in debug output.
/// The three fields below are those three things, in that order.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactRefusal {
    /// (a) What became of the operation log. Always `retained`: a refusal
    /// never drops anything.
    pub log: &'static str,
    /// (b) The condition that is not established.
    pub unestablished_condition: String,
    /// (c) The range that could not be dropped, per actor.
    pub retained_range: BTreeMap<Uuid, RetainedRange>,
}

/// A retained counter range, inclusive at both ends.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetainedRange {
    pub from: u64,
    pub through: u64,
}

/// BEHAVIOR-SPEC R8.4.2 and R8.4.3, stated where a caller sees it.
const UNDO_REDO_EFFECT: &str = "session undo/redo stacks are device-local and never persisted \
                                (R8.4.2), so no stack on disk changes; a live session holding a \
                                stack over folded batches truncates it to the retained prefix \
                                with a non-blocking diagnostic (R8.4.3)";

/// FORMAT-SPEC §9.7: the replacement is a new generation of the target, and
/// recovery artifacts are separate files this path never touches.
const RECOVERY_EFFECT: &str = "recovery artifacts are separate files and are neither modified nor \
                               deleted; the target is replaced atomically, so an interrupted \
                               compaction leaves the old package or the new one (§9.7)";

/// The §5.1 S6 condition v0.1 cannot establish on its own.
const S6_CONDITION: &str = "no undelivered batch can still be positioned before a dropped batch \
                            (§5.1 S6); v0.1 defines no sync protocol, so neither peer \
                            acknowledgement nor snapshot transfer can attest it";

/// Every actor the package's retained history names.
///
/// The union of the snapshot frontier's members (§5.1 S2), the author of each
/// retained batch, and the members of each batch's own frontier — a batch that
/// observed another actor is evidence of that actor whether or not its batches
/// are still here. S4's tombstones name no actor in this version's encoding,
/// so they contribute nothing; that is the metadata gap recorded against
/// [`domowik_container::Package::compacted`], not a claim that they could not.
fn actor_universe(pkg: &Package) -> BTreeSet<Uuid> {
    let mut actors: BTreeSet<Uuid> = pkg.frontier.keys().copied().collect();
    for batch in &pkg.batches {
        actors.insert(batch.actor_id);
        actors.extend(batch.frontier.keys().copied());
    }
    actors
}

/// The counter range each actor's retained batches occupy.
fn retained_ranges(pkg: &Package) -> BTreeMap<Uuid, RetainedRange> {
    let mut ranges: BTreeMap<Uuid, RetainedRange> = BTreeMap::new();
    for batch in &pkg.batches {
        let (from, through) = batch.counter_range();
        ranges
            .entry(batch.actor_id)
            .and_modify(|r| {
                r.from = r.from.min(from);
                r.through = r.through.max(through);
            })
            .or_insert(RetainedRange { from, through });
    }
    ranges
}

/// Compacts a plan: fold the operation log into the snapshot.
///
/// The log grows with every edit and nothing trimmed it. After compaction the
/// plan projects to the same document from a snapshot and an empty log.
///
/// **Irreversible.** The operations are discarded. Undo stacks are already
/// session-local and cleared on reopen (R8.4.2), so nothing a user could still
/// reach is lost — but `ops` will no longer show the history, and `undo` will
/// have nothing to compensate.
///
/// The projection is compared BEFORE and AFTER in memory, and the compaction
/// is refused if they differ. A compaction that changes the document is a bug,
/// and finding out later means finding out with the log already gone.
///
/// # What has to be true before the log may be dropped
///
/// FORMAT-SPEC §5.1 S6 permits dropping a batch from a SYNCHRONIZED document
/// only when no undelivered batch can still be positioned before it, and says
/// in terms that v0.1 supplies no attestation of that: "v0.1 defines no sync
/// protocol (§4), so v0.1 supplies no such attestation". §5.2's S7 lets a
/// LOCAL-ONLY document compact under a documented retention policy. So the
/// whole question is which of the two this package is, and **the package
/// cannot answer it**: sharing leaves no trace in a file.
///
/// Hence `--assert-local-only`, and hence only that. The caller answers, the
/// answer is recorded in the plan, and without it execution is refused with
/// S6's diagnostic. The flag is not a protocol and cannot replace one.
///
/// The actors the history names are REPORTED, not gated on. B2 defines
/// `actorId` as "the single-writer replica epoch that produced the batch —
/// distinct from user identity; one user may operate several actors/devices",
/// so a second actor is not a peer: a hand-applied batch produces one on a
/// file that never left the machine. S6's condition is about undelivered
/// batches held by peers, which an actor count does not measure, and requiring
/// a second assertion about each epoch would have demanded more ceremony
/// without establishing anything more. It is listed because it is worth seeing
/// before an irreversible fold.
///
/// A refusal exits non-zero. S6 forbids SILENT retention, and the diagnostic
/// below is the operation's observable result rather than a debug line; the
/// exit code is what stops a script from reading "the log was dropped" into a
/// run where it was not.
pub fn cmd_compact(path: &Path, options: CompactOptions) -> Result<String> {
    let pkg = read_package(path)?;
    let batches = pkg.batches.len();
    let operations: usize = pkg.batches.iter().map(|b| b.operations.len()).sum();
    let actors = actor_universe(&pkg);

    let mut plan = CompactPlan {
        phase: "preflight",
        target: path.display().to_string(),
        current_batches: batches,
        current_operations: operations,
        folded_batches: batches,
        folded_entries: Vec::new(),
        folded_through_frontier: BTreeMap::new(),
        retained_tombstones: Vec::new(),
        actors: actors.iter().copied().collect(),
        single_actor_history: actors.len() <= 1,
        assert_local_only: options.assert_local_only,
        undo_redo_effect: UNDO_REDO_EFFECT,
        recovery_effect: RECOVERY_EFFECT,
        written: false,
        refusal: None,
    };

    if batches == 0 {
        plan.phase = "already-compact";
        plan.folded_batches = 0;
        plan.retained_tombstones = pkg.tombstones.clone();
        if options.json {
            return encode(&plan);
        }
        return Ok(format!("{} is already compact.", path.display()));
    }

    // §9.6 FIRST, because no flag clears it and it is not the caller's to
    // answer. A required asset this device cannot resolve puts the package in
    // read-only degraded mode, where "interactive editing and normal saving
    // are unavailable" — and a compaction is a normal save that also discards
    // the log irreversibly. `Plan::save` enforces this through
    // `require_editable`; this command never builds a `Plan`, which is how it
    // came to fold a degraded package at all.
    //
    // The MODE COMES FROM THE PACKAGE, not from a second reading of the
    // descriptors here: `open_mode_from_embedded` is the one place §9.6's
    // requirement matrix is interpreted, and a duplicate of that logic would
    // be a second answer to "does this asset block editing".
    //
    // Only EXECUTION is gated. §9.6 preserves inspection in terms, so `ops`
    // and `--dry-run` stay available on a degraded package, and an
    // already-compact package has returned above without writing.
    if !options.dry_run {
        let (mode, unresolved) = pkg.open_mode_from_embedded();
        if mode == domowik_container::OpenMode::ReadOnlyDegraded {
            let ranges = retained_ranges(&pkg);
            let blocking: Vec<&domowik_container::UnresolvedAsset> =
                unresolved.iter().filter(|a| a.blocks_editing()).collect();
            // §9.6 names what an actionable diagnostic must carry: identity,
            // purpose, and remediation.
            let named = blocking
                .iter()
                .map(|a| {
                    let used = if a.used_by.is_empty() {
                        "referenced by the asset-root table".to_owned()
                    } else {
                        format!("used by {}", a.used_by.join(", "))
                    };
                    format!("{} ({:?}, {used})", a.digest, a.requirement)
                })
                .collect::<Vec<_>>()
                .join("; ");
            plan.phase = "refused";
            plan.folded_batches = 0;
            plan.refusal = Some(CompactRefusal {
                log: "retained",
                unestablished_condition: format!(
                    "the package is in read-only degraded mode: {} required asset(s) cannot be \
                     resolved, and BEHAVIOR-SPEC §9.6 makes normal saving unavailable while that \
                     holds. Unresolved: {named}",
                    blocking.len()
                ),
                retained_range: ranges.clone(),
            });
            let message = format!(
                "refusing to compact {}: {} required asset(s) cannot be resolved, so this package \
                 is in read-only degraded mode and BEHAVIOR-SPEC §9.6 makes normal saving \
                 unavailable — a compaction is a normal save, and an irreversible one. \
                 Unresolved: {named}. Restore the blob into the package, re-link the file, or \
                 remove the referencing instance, then compact. The operation log is RETAINED, \
                 not dropped: {batches} batch(es), {operations} operation(s), retained range {}. \
                 `ops` and `compact --dry-run` still work on this package.",
                path.display(),
                blocking.len(),
                describe_ranges(&ranges),
            );
            return Err(refusal_error(&plan, options.json, &message));
        }
    }

    // The execution gate comes BEFORE the projection, and that ordering is
    // load-bearing rather than an optimisation. A package that parses but
    // fails to project would otherwise answer a compaction request with a
    // projection error, when what the command owes is the §5.1 S6 refusal —
    // and the refusal needs nothing but the batch headers to state.
    if !options.dry_run && !options.assert_local_only {
        let ranges = retained_ranges(&pkg);
        plan.phase = "refused";
        plan.folded_batches = 0;
        plan.refusal = Some(CompactRefusal {
            log: "retained",
            unestablished_condition: format!(
                "{S6_CONDITION}. The caller has not asserted that this document is local-only, \
                 and a package cannot prove it: whether a copy was given to a peer leaves no \
                 trace in the file (§5.2 S7)"
            ),
            retained_range: ranges.clone(),
        });
        // All three of S6's parts, in the text form as well as the JSON one.
        // The range is not decoration: it is what a caller has to re-send if
        // the answer turns out to be that a peer is waiting for it.
        let message = format!(
            "refusing to compact {}: execution requires --assert-local-only, because locality is \
             never assumed. §5.1 S6 forbids dropping a batch a peer might still need, §5.2 S7 \
             lets a local-only document compact, and nothing in the package decides which this \
             is. The flag records an answer; it is not a transfer protocol and cannot replace \
             one. The operation log is RETAINED, not dropped. Unestablished condition: \
             {S6_CONDITION}. Retained range: {}. Run with --dry-run to see the retention effects \
             first.",
            path.display(),
            describe_ranges(&ranges),
        );
        return Err(refusal_error(&plan, options.json, &message));
    }

    // The fold, computed but not written: its entries and coverage are what
    // the plan reports, and the projection equality check needs it anyway.
    let before = pkg.project().map_err(CliError::from)?;
    let compacted = pkg.compacted().map_err(CliError::from)?;
    // EVERY retained entry, not the canonical order: compaction discards the
    // whole `ops/` namespace, and canonical order omits any batch the
    // snapshot already covers (§4.5 I2) — which would have under-reported
    // exactly the entries a caller most needs to hear are going. Sorted by
    // entry name, which §7.6 makes equal to ascending (counterBase, actorId).
    plan.folded_entries = pkg
        .batches
        .iter()
        .map(|b| domowik_container::ops_entry_name(b.counter_base, b.actor_id))
        .collect();
    plan.folded_entries.sort();
    plan.folded_through_frontier = compacted.frontier.clone();
    plan.retained_tombstones = compacted.tombstones.clone();

    // The retention effects, stated BEFORE anything is written. `--dry-run`
    // exists so a caller can see them and decide; a message that reports the
    // same numbers after the fact is not the same thing for an irreversible
    // operation. It needs no assertion of its own, because it writes nothing.
    if options.dry_run {
        if options.json {
            return encode(&plan);
        }
        return Ok(format!(
            "Would compact {}: {operations} operation(s) in {batches} batch(es) \
             folded into the snapshot and DISCARDED, {} tombstone(s) retained so \
             deleted entities stay deleted. `ops` would no longer show the \
             history and `undo` would have nothing to compensate. {UNDO_REDO_EFFECT}. \
             {RECOVERY_EFFECT}. Nothing was written (--dry-run).",
            path.display(),
            plan.retained_tombstones.len()
        ));
    }

    let after = compacted.project().map_err(CliError::from)?;
    // A safety net for a defect in COMPACTION, not in the plan. Nothing a
    // caller can put in a package reaches it, so NO TEST WITNESSES IT and no
    // mutation of the condition reddens one — the ED-0072 shape, where a gate
    // is real and unwitnessable, and it is recorded rather than dressed up.
    // `the_defect_refusal_names_the_retained_state_and_the_reason` checks the
    // diagnostic; `compaction_refuses_to_write_a_plan_that_projects_differently`
    // checks only that such a fold WOULD differ. It stays because this
    // operation is irreversible: if the fold is ever wrong, finding out later
    // means finding out with the log already gone.
    if after.document != before.document {
        return Err(CliError::Failure(projection_moved_message(
            path,
            batches,
            operations,
            &retained_ranges(&pkg),
        )));
    }

    if let Err(source) = write_package(path, &compacted) {
        // §5.2 S7 again: a compaction that cannot proceed names the retained
        // state and the blocking reason. A bare I/O error names the reason
        // only, and a caller that asked for JSON got something that was not
        // JSON — so the failure is reported on the same terms as every other
        // refusal. The §9.7 write path leaves the old package in place, which
        // is what makes "retained" true here.
        let ranges = retained_ranges(&pkg);
        plan.phase = "refused";
        plan.folded_batches = 0;
        plan.folded_entries.clear();
        plan.refusal = Some(CompactRefusal {
            log: "retained",
            unestablished_condition: format!("the package could not be written: {source}"),
            retained_range: ranges.clone(),
        });
        let message = format!(
            "refusing to compact {}: the package could not be written: {source}. The old package \
             is untouched and the operation log is RETAINED, not dropped: {batches} batch(es), \
             {operations} operation(s), retained range {}.",
            path.display(),
            describe_ranges(&ranges),
        );
        return Err(refusal_error(&plan, options.json, &message));
    }

    plan.phase = "written";
    plan.written = true;
    if options.json {
        return encode(&plan);
    }
    // A non-blocking note, not a gate: the epochs whose operations were just
    // discarded, named after the fact because the caller asserted locality and
    // the fold is what they asked for.
    let multi_actor = if plan.actors.len() > 1 {
        format!(
            " The history named {} writer epochs, and the operations of all of them were folded \
             on your --assert-local-only assertion: {}.",
            plan.actors.len(),
            plan.actors
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    } else {
        String::new()
    };
    Ok(format!(
        "Compacted {}: {operations} operation(s) in {batches} batch(es) folded \
         into the snapshot. {} tombstone(s) retained so deleted entities stay \
         deleted.{multi_actor}",
        path.display(),
        plan.retained_tombstones.len()
    ))
}

/// A refusal, carrying the plan when the caller asked for JSON.
///
/// [`CliError::Report`] prints to stdout and still exits non-zero, which is
/// what `validate --json` does with an invalid plan and for the same reason: a
/// caller that asked for machine-readable output wants the diagnostic, and an
/// exit code alone does not carry it.
fn refusal_error(plan: &CompactPlan, json: bool, message: &str) -> CliError {
    if !json {
        return CliError::Failure(message.to_owned());
    }
    match serde_json::to_string_pretty(plan) {
        Ok(body) => CliError::Report(body),
        Err(e) => CliError::Failure(format!("could not encode the plan: {e}")),
    }
}

fn encode<T: serde::Serialize>(value: &T) -> Result<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| CliError::Failure(format!("could not encode the report: {e}")))
}

/// The diagnostic for a compaction refused by its own safety net.
///
/// §5.2 S7: a local-only compaction that cannot proceed at all reports a
/// diagnostic naming the RETAINED STATE and the blocking reason, and silent
/// refusal is forbidden there exactly as silent undo breakage is. Separated
/// from the branch because the branch itself cannot be reached without a
/// defect in `Package::compacted` — the same shape ED-0072 recorded for a gate
/// that is real but unwitnessable — so the message is what a test can check.
fn projection_moved_message(
    path: &Path,
    batches: usize,
    operations: usize,
    ranges: &BTreeMap<Uuid, RetainedRange>,
) -> String {
    format!(
        "refusing to compact {}: the compacted plan projects a different document, which is a \
         defect in compaction rather than in the plan. Nothing was written and the operation log \
         is RETAINED, not dropped: {batches} batch(es), {operations} operation(s), retained range \
         {}.",
        path.display(),
        describe_ranges(ranges),
    )
}

fn describe_ranges(ranges: &BTreeMap<Uuid, RetainedRange>) -> String {
    if ranges.is_empty() {
        return "nothing retained".to_owned();
    }
    ranges
        .iter()
        .map(|(actor, r)| format!("{actor} {}..={}", r.from, r.through))
        .collect::<Vec<_>>()
        .join(", ")
}

/// A machine-readable account of what a plan retains, for `ops --json`.
///
/// The human form identifies batches by a truncated actor id and a position,
/// which is what fits a terminal line; this one identifies them by batch id
/// and by their §7.6 entry name, which is what a script can look up in the
/// package.
///
/// # What it does not do
///
/// ED-0023 authorised a parse-only inspection path for this command and says
/// why: reusing the replaying load path "would violate issue 0027 AC1". So by
/// default this report is built from the manifest and from the batch headers —
/// parsed, framing-checked and ordered, never APPLIED. The operations of an
/// untrusted package are not run against a document to produce it.
///
/// **ED-0107** bounds that for the Rust seam specifically, after the owner
/// inspected `Package::log` and `OperationLog::deliver`: materialising typed
/// INERT operations and PRIVATE framing and ordering metadata is allowed, and
/// it supersedes ED-0023's Go-symbol "call Deliver" wording for this seam
/// only. Document execution, entity projection, validation callbacks, shared
/// or session mutation, and file writes remain forbidden on the default path.
/// The no-execution requirement itself is untouched, and one security parser
/// is kept rather than forked — `read_package` is that parser.
///
/// Concretely: `OperationLog::deliver` validates framing and causality and
/// builds a private ordering buffer; `replay::project` applies operations to
/// entities. Only the second is execution, and `--replay` is the only way to
/// reach it. `default_path_never_applies_the_log` and
/// `application_invalid_data_inspects_but_replay_rejects_it` are the
/// standing witnesses.
///
/// Under `--replay` the report gains what only application can know: the
/// operations replay discards as stale, and the entities the retained log
/// deletes. That is opt-in because it is exactly the step a user inspecting a
/// file they do not trust is trying to avoid.
///
/// Operation payloads are reported by kind, target and field NAMES — the same
/// description the human form prints — rather than by echoing values a
/// consumer might act on.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsReport {
    pub path: String,
    pub batches: usize,
    pub operations: usize,
    /// FORMAT-SPEC §5.1 S2: what the stored snapshot already covers. Empty
    /// when the snapshot covers nothing and the whole history is retained.
    pub snapshot_frontier: BTreeMap<Uuid, u64>,
    /// The highest counter each actor has written, snapshot and log together.
    pub watermark: BTreeMap<Uuid, u64>,
    /// Every actor the retained history names.
    pub actors: Vec<Uuid>,
    /// §5.1 S4: ids the snapshot absorbed a delete for, which is what stops a
    /// later create from resurrecting something compaction removed the
    /// operations for. Read from the manifest, so it needs no replay.
    pub retained_tombstones: Vec<Uuid>,
    /// Retained entries the snapshot already covers, which replay ignores
    /// under I2 and which therefore do not appear in `canonicalOrder`.
    ///
    /// Normally zero. It is reported because otherwise `batches` and the
    /// length of `canonicalOrder` disagree with nothing to explain them: a
    /// package can hold an `ops/` entry whose whole range is at or below the
    /// snapshot frontier, and §4.5 I2 says to ignore it silently rather than
    /// to reject the package.
    pub covered_by_snapshot: usize,
    /// Whether the log was applied to produce the two fields below.
    pub replayed: bool,
    /// Ids the RETAINED LOG deletes, under `--replay`. Distinct from the
    /// retained tombstones: a delete whose operation is still here needs no S4
    /// entry, and a package carries both once it has been compacted and edited
    /// again. Absent rather than empty when the log was not applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_tombstones: Option<Vec<Uuid>>,
    /// Batches in CANONICAL order — the order replay WOULD apply them in,
    /// computed by sorting their parsed positions rather than by applying
    /// anything.
    pub canonical_order: Vec<OpsBatchReport>,
    /// Operations replay discards as stale (I5), under `--replay`. Absent
    /// rather than empty when the log was not applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discarded: Option<Vec<DiscardReport>>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsBatchReport {
    pub batch_id: Uuid,
    /// The §7.6 entry this batch is stored as.
    pub entry: String,
    pub actor_id: Uuid,
    /// The first component of F3's ordering position, as a DECIMAL STRING.
    ///
    /// §9.3 row L33 requires exact comparison over [0, 2^79 − 2^16], which no
    /// IEEE-754 double represents and which many JSON parsers would silently
    /// round. A string cannot be rounded by accident.
    pub causal_past_size: String,
    pub counter_base: u64,
    /// The counter range this batch reserves, inclusive at both ends.
    pub counter_from: u64,
    pub counter_through: u64,
    /// The batch's own frontier (§4.4).
    pub frontier: BTreeMap<Uuid, u64>,
    /// The one-line human description, so both forms agree by construction.
    pub summary: String,
    pub operations: Vec<OpsOperationReport>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpsOperationReport {
    pub operation_id: Uuid,
    pub entity_id: Uuid,
    /// `createEntity`, `setFields` or `deleteEntity` (§4.2).
    pub kind: &'static str,
    /// The created entity's type, for a `createEntity`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<&'static str>,
    /// The field NAMES a `setFields` writes; never their values.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    pub description: String,
}

/// Builds the machine-readable `ops` report for `path`.
///
/// Reads and inspects in memory; it writes nothing. `replay` opts into
/// applying the log — see [`OpsReport`] for why that is not the default.
pub fn ops_report(path: &Path, replay: bool) -> Result<OpsReport> {
    let pkg = read_package(path)?;
    // Framing validation and causal ordering. `deliver` does not apply an
    // operation to an entity; `replay::project`, below, is what does.
    let log = pkg.log().map_err(CliError::from)?;

    // ED-0107: the ONLY application of the log in this function, and it is
    // behind `replay`. Everything above and below is framing, ordering and
    // manifest data. A future edit that hoists this out of the branch is the
    // regression the witnesses below are aimed at.
    let (log_tombstones, discarded) = if replay {
        let projection = pkg.project().map_err(CliError::from)?;
        let mut tombstones: Vec<Uuid> = projection
            .intervals
            .iter()
            .filter(|(_, i)| i.deleted_at.is_some())
            .map(|(id, _)| *id)
            .filter(|id| !pkg.tombstones.contains(id))
            .collect();
        tombstones.sort();
        let discarded = projection
            .discarded
            .iter()
            .map(|d| DiscardReport {
                batch_id: d.batch_id,
                operation_id: d.operation_id,
                entity_id: d.entity_id,
                reason: format!("{:?}", d.reason),
            })
            .collect();
        (Some(tombstones), Some(discarded))
    } else {
        (None, None)
    };

    let ordered = log.canonical_order();
    let covered_by_snapshot = pkg.batches.len().saturating_sub(ordered.len());
    let canonical_order = ordered
        .into_iter()
        .map(|batch| {
            let (from, through) = batch.counter_range();
            OpsBatchReport {
                batch_id: batch.batch_id,
                entry: domowik_container::ops_entry_name(batch.counter_base, batch.actor_id),
                actor_id: batch.actor_id,
                causal_past_size: batch.causal_past_size().to_string(),
                counter_base: batch.counter_base,
                counter_from: from,
                counter_through: through,
                frontier: batch.frontier.clone(),
                summary: describe_batch(batch),
                operations: batch.operations.iter().map(describe_for_report).collect(),
            }
        })
        .collect();

    Ok(OpsReport {
        path: path.display().to_string(),
        batches: pkg.batches.len(),
        operations: pkg.batches.iter().map(|b| b.operations.len()).sum(),
        snapshot_frontier: pkg.frontier.clone(),
        watermark: log.watermark().iter().map(|(&a, &c)| (a, c)).collect(),
        actors: actor_universe(&pkg).into_iter().collect(),
        retained_tombstones: pkg.tombstones.clone(),
        covered_by_snapshot,
        replayed: replay,
        log_tombstones,
        canonical_order,
        discarded,
    })
}

/// `ops --json`.
pub fn cmd_ops_json(path: &Path, replay: bool) -> Result<String> {
    encode(&ops_report(path, replay)?)
}

/// Lists the operation batches a plan retains.
pub fn cmd_ops(path: &Path, replay: bool) -> Result<String> {
    let report = ops_report(path, replay)?;

    let mut out = format!("{}\n", report.path);
    out.push_str(&format!(
        "  {} batch(es), {} operation(s)\n",
        report.batches, report.operations
    ));
    out.push_str(&format!("  {} actor(s):\n", report.actors.len()));
    for actor in &report.actors {
        let through = report.watermark.get(actor).copied().unwrap_or(0);
        out.push_str(&format!("    {actor} written through {through}\n"));
    }
    if !report.snapshot_frontier.is_empty() {
        out.push_str("  snapshot covers:\n");
        for (actor, counter) in &report.snapshot_frontier {
            out.push_str(&format!("    {actor} through {counter}\n"));
        }
    }
    // FORMAT-SPEC §5.1 S4. Without these a compacted delete is invisible here,
    // and the one thing a reader most needs to know about a compacted package
    // is what it still refuses to resurrect.
    if !report.retained_tombstones.is_empty() {
        out.push_str(&format!(
            "  {} retained tombstone(s) (deletes the snapshot absorbed):\n",
            report.retained_tombstones.len()
        ));
        for id in &report.retained_tombstones {
            out.push_str(&format!("    {id}\n"));
        }
    }
    if let Some(deleted) = report.log_tombstones.as_ref().filter(|d| !d.is_empty()) {
        out.push_str(&format!(
            "  {} entity(ies) deleted by the retained log:\n",
            deleted.len()
        ));
        for id in deleted {
            out.push_str(&format!("    {id}\n"));
        }
    }

    if report.covered_by_snapshot > 0 {
        out.push_str(&format!(
            "  {} retained entry(ies) the snapshot already covers, ignored by replay (I2)\n",
            report.covered_by_snapshot
        ));
    }

    // CANONICAL order, not file order: this is the order replay applies them
    // in, and showing arrival order would misrepresent what the log means.
    out.push_str("\n  canonical order (causal past, counter base, actor):\n");
    for batch in &report.canonical_order {
        out.push_str(&format!(
            "    [{:>4}, {:>4}, {}] {}..={} {}\n",
            batch.causal_past_size,
            batch.counter_base,
            &batch.actor_id.to_string()[..8],
            batch.counter_from,
            batch.counter_through,
            batch.summary
        ));
        for op in &batch.operations {
            out.push_str(&format!("        {}\n", op.description));
        }
    }

    match report.discarded.as_ref() {
        Some(discarded) if !discarded.is_empty() => {
            out.push_str(&format!(
                "\n  {} operation(s) discarded as stale:\n",
                discarded.len()
            ));
            for d in discarded {
                out.push_str(&format!(
                    "    {} on {} ({})\n",
                    &d.operation_id.to_string()[..8],
                    &d.entity_id.to_string()[..8],
                    d.reason
                ));
            }
        }
        // Said rather than left blank: an empty section and an unasked
        // question look identical otherwise.
        None => out.push_str(
            "\n  the log was not applied; pass --replay for stale discards and log deletes\n",
        ),
        Some(_) => {}
    }
    Ok(out)
}

fn describe_batch(batch: &Batch) -> String {
    match batch.operations.len() {
        1 => describe_operation(&batch.operations[0]),
        n => format!("a batch of {n} operations"),
    }
}

fn describe_operation(op: &Operation) -> String {
    let id = &op.entity_id.to_string()[..8];
    match &op.kind {
        OperationKind::CreateEntity { entity } => {
            format!("create {} {id}", entity.type_name())
        }
        OperationKind::SetFields { fields } => {
            let names: Vec<&str> = fields.keys().map(|k| k.as_str()).collect();
            format!("set {} on {id}", names.join(", "))
        }
        OperationKind::DeleteEntity => format!("delete {id}"),
    }
}

fn describe_for_report(op: &Operation) -> OpsOperationReport {
    let (kind, entity_type, fields) = match &op.kind {
        OperationKind::CreateEntity { entity } => {
            ("createEntity", Some(entity.type_name()), Vec::new())
        }
        OperationKind::SetFields { fields } => (
            "setFields",
            None,
            fields.keys().map(|k| k.to_owned()).collect(),
        ),
        OperationKind::DeleteEntity => ("deleteEntity", None, Vec::new()),
    };
    OpsOperationReport {
        operation_id: op.operation_id,
        entity_id: op.entity_id,
        kind,
        entity_type,
        fields,
        description: describe_operation(op),
    }
}

/// Every room record in the document, in the order the room commands number
/// them: level display order, then stable UUID within a level.
///
/// Document order is not canonical (FORMAT-SPEC C1), so numbering by it would
/// let the same command edit different records in two copies of one plan.
fn numbered_records(doc: &Document) -> Vec<&RoomRecord> {
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

/// Resolves a room argument: either its `room list` number, or its UUID.
///
/// Both, because the two callers want different things. A person reads a
/// number off the listing; a script that created a record already holds its
/// identifier and should not have to re-derive a position that another edit
/// may have shifted.
fn resolve_room(doc: &Document, room: &str) -> Result<Uuid> {
    if let Ok(id) = Uuid::parse_str(room) {
        return match doc.room_record(id) {
            Some(_) => Ok(id),
            None => Err(CliError::Failure(format!(
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
                Err(CliError::Usage(format!(
                    "expected a room number from `domowik room list`, a record UUID or a short ID (4e1a92), got {room:?}"
                )))
            })
        }
    };
    let records = numbered_records(doc);
    if index == 0 || index > records.len() {
        return Err(CliError::Failure(format!(
            "room {index} is out of range; this plan holds {} room record(s)",
            records.len()
        )));
    }
    Ok(records[index - 1].id)
}

/// The fields `room set` may change, before they become a [`RoomFields`].
///
/// Every "set" has a matching "clear" at the command line because the two are
/// different edits reaching different states (BEHAVIOR-SPEC F37d), and a flag
/// that took an empty string to mean "clear" would make the empty-string name
/// unreachable — which is a legal authoritative value.
#[derive(Debug, Default)]
pub struct RoomSetArgs {
    pub seed: Option<(f64, f64)>,
    pub name: Option<String>,
    pub clear_name: bool,
    pub floor_style: Option<String>,
    pub clear_floor_style: bool,
    pub ceiling_style: Option<String>,
    pub clear_ceiling_style: bool,
    pub level: Option<String>,
}

fn field_edit(set: Option<String>, clear: bool, flag: &str) -> Result<FieldEdit<String>> {
    match (set, clear) {
        (Some(_), true) => Err(CliError::Usage(format!(
            "--{flag} and --clear-{flag} ask for two different things; pass one"
        ))),
        (Some(value), false) => Ok(FieldEdit::Set(value)),
        (None, true) => Ok(FieldEdit::Clear),
        (None, false) => Ok(FieldEdit::Keep),
    }
}

/// Creates a room record (BEHAVIOR-SPEC R5.2.2 CREATE).
///
/// # Where the identifier comes from
///
/// R5.2.2 forbids AUTOMATIC id assignment: the command supplies the
/// identifier and a collision rejects. The core honours that literally —
/// `create_room_operations` takes a record whose id is already decided and
/// mints nothing. This command is the caller, so it supplies one: `--id` when
/// the user has an id in mind, and a fresh UUIDv7 otherwise. `--id` exists
/// mainly so a script can observe the collision refusal, which is a behaviour
/// with a fixture (F37a) and would otherwise be unreachable from the terminal.
///
/// # The seed is not validated for WHERE it lands
///
/// Only for being a coordinate. A seed in a wall or out in the exterior is
/// accepted and simply takes a non-associating status (R5.2.4); `room list`
/// reports which.
#[allow(clippy::too_many_arguments)]
pub fn cmd_room_add(
    path: &Path,
    x: f64,
    y: f64,
    name: Option<&str>,
    id: Option<Uuid>,
    level_name: Option<&str>,
    floor_style: Option<&str>,
    ceiling_style: Option<&str>,
) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let level_id = resolve_level(plan.document(), level_name, path)?;
    let mut record = RoomRecord::new(id.unwrap_or_else(Uuid::now_v7), level_id, Point::new(x, y));
    record.name = name.map(str::to_owned);
    record.floor_style_ref = floor_style.map(str::to_owned);
    record.ceiling_style_ref = ceiling_style.map(str::to_owned);
    let record_id = record.id;

    let ops = domowik_edit::create_room_operations(plan.document(), record)?;
    plan.commit_and_save(ops)?;

    let projection = rooms::project_level(plan.document(), level_id);
    let status = projection
        .record(record_id)
        .map(|r| r.status)
        .unwrap_or(RoomStatus::SeedInExterior);
    let mut msg = format!(
        "Added room record {record_id} at ({x:.0}, {y:.0}): {}.",
        status.token()
    );
    if let Some(name) = name {
        msg = format!(
            "Added room record {record_id} {name:?} at ({x:.0}, {y:.0}): {}.",
            status.token()
        );
    }
    msg.push_str(&unplaced_warning(status, record_id));
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// An explicit warning line for a record that labels no room. The record is
/// still created or updated — an unassociated record is a legal state
/// (R5.2) — but an agent reading the output must not miss that the name is
/// not shown anywhere.
fn unplaced_warning(status: RoomStatus, id: Uuid) -> String {
    match status {
        RoomStatus::Associated => String::new(),
        RoomStatus::Superseded => format!(
            "\nwarning: another name already labels this room, so this one is not shown; `domowik check` lists it (#{}).",
            refs::short_id(id)
        ),
        _ => format!(
            "\nwarning: the seed is not inside a room ({}), so this name labels nothing; move it with `domowik room set PLAN {} --seed X,Y`.",
            status.token(),
            refs::short_id(id)
        ),
    }
}

/// Changes a room record's fields (BEHAVIOR-SPEC R5.2.2 UPDATE).
pub fn cmd_room_set(path: &Path, room: &str, args: &RoomSetArgs) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let id = resolve_room(plan.document(), room)?;
    let level_id = match args.level.as_deref() {
        Some(name) => Some(resolve_level(plan.document(), Some(name), path)?),
        None => None,
    };
    let fields = RoomFields {
        seed: args.seed.map(|(x, y)| Point::new(x, y)),
        name: field_edit(args.name.clone(), args.clear_name, "name")?,
        floor_style_ref: field_edit(
            args.floor_style.clone(),
            args.clear_floor_style,
            "floor-style",
        )?,
        ceiling_style_ref: field_edit(
            args.ceiling_style.clone(),
            args.clear_ceiling_style,
            "ceiling-style",
        )?,
        level_id,
    };

    let ops = domowik_edit::update_room_operations(plan.document(), id, &fields)?;
    if ops.is_empty() {
        return Ok(format!("Room record {id} is already in that state."));
    }
    plan.commit_and_save(ops)?;

    let record = plan
        .document()
        .room_record(id)
        .expect("the update just committed")
        .clone();
    let projection = rooms::project_level(plan.document(), record.level_id);
    let status = projection
        .record(id)
        .map(|r| r.status)
        .unwrap_or(RoomStatus::SeedInExterior);
    let mut msg = format!(
        "Updated room record {id}: {} is now {}.",
        record.display_name(),
        status.token()
    );
    msg.push_str(&unplaced_warning(status, id));
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Deletes a room record (BEHAVIOR-SPEC R5.2.2 DELETE).
///
/// The face survives: faces exist whether or not a record names them
/// (R5.1.8). Only the label goes.
pub fn cmd_room_remove(path: &Path, room: &str) -> Result<String> {
    let mut plan = Plan::open(path)?;
    let id = resolve_room(plan.document(), room)?;
    let name = plan
        .document()
        .room_record(id)
        .map(|r| r.display_name().to_owned())
        .unwrap_or_default();
    let ops = domowik_edit::delete_room_operations(plan.document(), id)?;
    plan.commit_and_save(ops)?;
    let mut msg = format!("Removed room record {id} ({name}). The face is unchanged.");
    append_warnings(&mut msg, plan.document());
    Ok(msg)
}

/// Lists room records with their derived status and, where they have one,
/// their face's area.
pub fn cmd_room_list(path: &Path, level_name: Option<&str>) -> Result<String> {
    let pkg = read_package(path)?;
    let projection = pkg.project().map_err(CliError::from)?;
    let doc = &projection.document;
    let wanted = match level_name {
        Some(name) => Some(resolve_level(doc, Some(name), path)?),
        None => None,
    };

    let numbers: Vec<Uuid> = numbered_records(doc).iter().map(|r| r.id).collect();
    let mut out = format!(
        "{}
",
        path.display()
    );
    let mut shown = 0usize;
    for level in display_levels(doc) {
        if wanted.is_some_and(|w| w != level.id) {
            continue;
        }
        let rooms = rooms::project_level(doc, level.id);
        if rooms.records.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "
  {} — {} room record(s), {} face(s)
",
            level.name,
            rooms.records.len(),
            rooms.faces.len()
        ));
        for projected in &rooms.records {
            let record = doc
                .room_record(projected.id)
                .expect("projected from the doc");
            let number = numbers
                .iter()
                .position(|id| *id == projected.id)
                .unwrap_or(0)
                + 1;
            let area = match projected.face {
                Some(face) => format!(
                    "{} m²",
                    rooms::display_polygon_area_m2(&rooms.faces[face])
                        .unwrap_or_else(|| "—".to_owned())
                ),
                // R5.3.3: a non-candidate record has no deterministic label of
                // any kind, so there is no area to attribute to it either.
                None => "—".to_owned(),
            };
            out.push_str(&format!(
                "    {number}. {:<16} {:<17} {:>10}   seed ({:.0}, {:.0})
",
                display_quoted(record),
                projected.status.token(),
                area,
                record.seed().x,
                record.seed().y
            ));
            let styles: Vec<String> = [
                ("floor", record.floor_style_ref.as_deref()),
                ("ceiling", record.ceiling_style_ref.as_deref()),
            ]
            .into_iter()
            .filter_map(|(what, value)| value.map(|v| format!("{what} {v}")))
            .collect();
            if !styles.is_empty() {
                out.push_str(&format!(
                    "       [{}]
",
                    styles.join(", ")
                ));
            }
            shown += 1;
        }
    }
    if shown == 0 {
        out.push_str(
            "
  no room records
",
        );
    }
    Ok(out)
}

/// A record's name for display: quoted when it has one, and the R5.3.3
/// fallback marked as such when it does not.
///
/// The marker matters. An absent name and the literal name `"Room"` display
/// identically on a plan, and a listing whose whole job is to say what is
/// stored must not make them look the same there too.
fn display_quoted(record: &RoomRecord) -> String {
    match record.name.as_deref() {
        Some(name) if !name.is_empty() => format!("{name:?}"),
        Some(_) => "\"\" (empty)".to_owned(),
        None => "(unnamed)".to_owned(),
    }
}

/// The occupied region and the rooms of `level_id`, computed together
/// (BEHAVIOR-SPEC R3.2.1 and R5.1.1).
pub fn plan_on_level(doc: &Document, level_id: Uuid) -> (Vec<Polygon>, Vec<Polygon>) {
    let (solids, joins) = level_solids(doc, level_id);
    walls::plan(&solids, &joins)
}

/// The occupied region of `level_id` (BEHAVIOR-SPEC R3.2.1).
pub fn occupied_on_level(doc: &Document, level_id: Uuid) -> Vec<Polygon> {
    plan_on_level(doc, level_id).0
}

/// Rooms on `level_id`: the bounded components of the free space (R5.1.1).
///
/// Areas are measured to the INNER faces of the walls, not to their centre
/// lines, because that is what the specification defines a room to be and what
/// a user means by floor area.
pub fn rooms_on_level(doc: &Document, level_id: Uuid) -> Vec<Polygon> {
    plan_on_level(doc, level_id).1
}

/// Derived display order; never rewrites the document's entity storage.
/// Invalid elevations sort last by UUID, matching diagnostic ordering.
fn display_levels(doc: &Document) -> Vec<&domowik_format::Level> {
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

/// Resolves an unambiguous name, or the first level in display order.
/// The level named `name`, refused when absent or ambiguous.
pub(crate) fn resolve_level_id(doc: &Document, name: &str, path: &Path) -> Result<Uuid> {
    resolve_level(doc, Some(name), path)
}

fn resolve_level(doc: &Document, name: Option<&str>, path: &Path) -> Result<Uuid> {
    match name {
        Some(name) => {
            let mut matches = doc.levels.iter().filter(|l| l.name == name);
            let first = matches.next().ok_or_else(|| {
                CliError::Failure(format!(
                    "no level named {name:?} in {}; available: {}",
                    path.display(),
                    level_list(doc)
                ))
            })?;
            if matches.next().is_some() {
                return Err(CliError::Failure(format!("multiple levels are named {name:?}; select and rename the intended level in the editor before using name-based commands")));
            }
            Ok(first.id)
        }
        None => display_levels(doc).first().map(|l| l.id).ok_or_else(|| {
            CliError::Failure(format!("{} has no levels to add a wall to", path.display()))
        }),
    }
}

fn level_list(doc: &Document) -> String {
    if doc.levels.is_empty() {
        return "(none)".to_owned();
    }
    display_levels(doc)
        .into_iter()
        .map(|l| format!("{:?}", l.name))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Appends a sentence per BEHAVIOR-SPEC warning attached to `doc`'s CURRENT
/// state to a successful command's message.
///
/// R3.1.7/§2.1: a warning attaches to a successful batch result and never
/// blocks it, so callers must invoke this only AFTER a command has already
/// committed, compensated, or accepted an external batch — and its save (if
/// any) has already succeeded. It is not itself a gate: a rejected batch or a
/// failed save returns through `?` before ever reaching this, and so never
/// runs it.
fn append_warnings(msg: &mut String, doc: &Document) {
    for warning in domowik_document::warnings::validation_warnings(doc) {
        msg.push_str(&format!(" Warning: {warning}"));
    }
}

/// The three byte classes, as issue 0026 requires them kept apart.
///
/// Package and cache bytes are MEASURED. Remote bytes are the DECLARED
/// `byteLength` of descriptors for blobs held in neither place — nothing here
/// fetches, so nothing here has weighed them, and the line says so rather than
/// letting a declaration pass for a measurement.
///
/// Without `--cache` there is no cache to measure, and the line says that too
/// instead of printing a zero that reads like an empty cache.
fn asset_size_report(
    pkg: &domowik_container::Package,
    cache_root: Option<&Path>,
) -> Result<String> {
    let universe: std::collections::BTreeSet<String> =
        pkg.manifest.asset_digests().into_iter().collect();
    // Real descriptors now, not an empty list: remote bytes are the DECLARED
    // `byteLength` of a descriptor whose blob is held neither in the package
    // nor the cache, so without them that class could only ever be zero.
    let (table, _faults) = domowik_assets::DescriptorTable::from_object(&pkg.descriptors);
    let descriptors = table.as_vec();

    let store = match cache_root {
        Some(root) => Some(CacheStore::open(root, AdmissionLimits::default())?),
        None => None,
    };
    let totals = domowik_assets::size_totals(&domowik_assets::PackInputs {
        universe: &universe,
        embedded: &pkg.assets,
        cache: store.as_ref(),
        descriptors: &descriptors,
    });

    let mut out = format!("  package: {} byte(s), measured\n", totals.package_bytes);
    match cache_root {
        Some(root) => out.push_str(&format!(
            "  cache:   {} byte(s), measured in {}\n",
            totals.cache_bytes,
            root.display()
        )),
        None => out.push_str("  cache:   not examined; pass --cache <dir> to measure it\n"),
    }
    out.push_str(&format!(
        "  remote:  {} byte(s), declared by descriptors and never weighed here\n",
        totals.remote_bytes
    ));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Cache commands — ASSET-STRATEGY §10.7.
//
// §10.7 names issue 0026's `asset` commands as the surface for manual pins,
// so these live here rather than in the asset crate. They operate on a cache
// ROOT and never open a plan, which is what lets them exist before the
// `assets/descriptors.json` entry does.
// ---------------------------------------------------------------------------

use domowik_assets::cache::{AdmissionLimits, CacheStore};
use domowik_assets::gc::{ClockReading, Collector, Health, MetaStore, Pin, PlatformMode, RootKind};
use domowik_assets::pack::Profile;

/// Opens the cache and its protection records at `root`.
fn open_cache(root: &Path) -> Result<(CacheStore, MetaStore)> {
    let store = CacheStore::open(root, AdmissionLimits::default())?;
    let meta = MetaStore::open(root)?;
    Ok((store, meta))
}

/// The wall clock, as the collector reads it.
///
/// UTC seconds only. §10.7 puts macOS and Windows in UTC-only mode by default
/// and this build does not document a verified monotonic mechanism for either,
/// so supplying a monotonic reading here would claim a guarantee the platform
/// has not been shown to provide.
fn now_reading() -> ClockReading {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        // A clock before the epoch is not a reason to refuse to run; it makes
        // every lease look unexpired, which is the safe direction.
        .unwrap_or(0);
    ClockReading::UtcOnly {
        now_utc_seconds: seconds,
    }
}

/// Reports what the cache holds and whether the collector may run.
pub fn cmd_cache_status(root: &Path) -> Result<String> {
    let (store, meta) = open_cache(root)?;
    let digests = store.digests()?;
    let mut out = format!("{}:\n", root.display());
    out.push_str(&format!(
        "  {} blob(s), {} byte(s)\n",
        digests.len(),
        store.total_bytes()?
    ));
    out.push_str(&format!(
        "  {} pin(s), {} lease(s), store generation {}\n",
        meta.pins().count(),
        meta.leases().count(),
        meta.generation()
    ));
    let mode = PlatformMode::detect();
    out.push_str(&format!(
        "  lease mode: {}\n",
        match &mode {
            PlatformMode::Monotonic { boot_id } => format!("monotonic (boot {boot_id})"),
            PlatformMode::UtcOnly => "UTC-only".to_owned(),
        }
    ));
    match meta.health() {
        Health::Clean => out.push_str("  collector: ready\n"),
        Health::FailClosed(anomaly) => out.push_str(&format!(
            "  collector: FAIL-CLOSED, {anomaly}\n  \
             no blob will be collected until `asset cache rebuild` runs\n"
        )),
    }
    let abandoned = store.abandoned_staging()?;
    if !abandoned.is_empty() {
        out.push_str(&format!(
            "  {} abandoned staging file(s) from interrupted imports; \
             they are not reachable as blobs\n",
            abandoned.len()
        ));
    }
    Ok(out)
}

/// Adds a durable `manual` pin.
pub fn cmd_cache_pin(root: &Path, digest: &str, owner: &str) -> Result<String> {
    let (store, mut meta) = open_cache(root)?;
    if store.path_for(digest).is_none() {
        return Err(CliError::Usage(format!(
            "{digest} is not a well-formed sha256:<64 lowercase hex> digest"
        )));
    }
    // A pin on a blob the cache does not hold is allowed and useful: §10.7 G2
    // says admission atomically restores protection, so pinning first and
    // admitting later is the ordering that cannot lose a race.
    let held = store.contains(digest);
    meta.add_pin(Pin {
        digest: digest.to_owned(),
        owner_id: owner.to_owned(),
        root_kind: RootKind::Manual,
        created_at: rfc3339_now(),
    })?;
    Ok(format!(
        "Pinned {digest} for {owner}. It survives document deregistration and is \
         released only by `asset cache unpin`.{}",
        if held {
            ""
        } else {
            " The cache does not hold it yet; the pin protects it from the moment it arrives."
        }
    ))
}

/// Releases a manual pin.
pub fn cmd_cache_unpin(root: &Path, digest: &str, owner: &str) -> Result<String> {
    let (_store, mut meta) = open_cache(root)?;
    let held = meta
        .pins()
        .any(|p| p.digest == digest && p.owner_id == owner && p.root_kind == RootKind::Manual);
    if !held {
        return Err(CliError::Failure(format!(
            "{owner} holds no manual pin on {digest}"
        )));
    }
    meta.remove_pin(digest, owner, RootKind::Manual)?;
    Ok(format!(
        "Released {owner}'s manual pin on {digest}. Other protection records, if any, still apply."
    ))
}

/// Runs one collection pass.
///
/// Refuses a cache that holds blobs and has NO protection records at all,
/// unless the caller says explicitly that it means to.
///
/// §10.7 G4 lets the collector be wrong without endangering data because
/// "pins cover every root" — but that holds only once documents have
/// registered their roots. A cache nobody has registered against has an empty
/// root set, and an empty root set is indistinguishable from "nothing has told
/// me about the documents yet". Collecting under that reading would delete
/// every blob a package still references. Registration arrives with the
/// `assets/descriptors.json` wiring; until then the refusal is the honest
/// behaviour, and `--allow-unregistered` is how a caller who genuinely has
/// only garbage says so.
pub fn cmd_cache_gc(root: &Path, allow_unregistered: bool) -> Result<String> {
    let (store, mut meta) = open_cache(root)?;
    if !allow_unregistered
        && meta.pins().count() == 0
        && meta.leases().count() == 0
        && !store.digests()?.is_empty()
    {
        return Err(CliError::Failure(format!(
            "refusing to collect: {} blob(s) are held and nothing is pinned or leased, so no \
             document has registered its roots and every blob here looks collectable. Pin what \
             must survive with `asset cache pin`, or pass --allow-unregistered if this cache \
             really does hold only garbage.",
            store.digests()?.len()
        )));
    }
    let mode = PlatformMode::detect();
    let report = Collector::new(&store, &mut meta, mode).collect(&now_reading())?;

    if let Some(anomaly) = report.fail_closed {
        // Exit non-zero. The command did not do what was asked, and a
        // scheduled collection that exits 0 while the collector has been
        // fail-closed for months lets the cache grow with nothing to notice.
        return Err(CliError::Failure(format!(
            "collected nothing: the protection records are fail-closed ({anomaly}); \
             {} blob(s) retained. Run `asset cache rebuild` to repair.",
            report.retained.len()
        )));
    }
    let mut out = format!(
        "Collected {} blob(s); {} retained.\n",
        report.collected.len(),
        report.retained.len()
    );
    for digest in &report.collected {
        out.push_str(&format!("  collected {digest}\n"));
    }
    Ok(out)
}

/// Rebuilds the protection records and clears fail-closed mode.
///
/// The rebuilt set is the conservative superset §10.7 requires: everything
/// already believed pinned, plus anything recomputed. Nothing is dropped,
/// because this command cannot tell "the caller knows this is unreachable"
/// from "the caller could not read the document".
pub fn cmd_cache_rebuild(root: &Path) -> Result<String> {
    let (_store, mut meta) = open_cache(root)?;
    let before = matches!(meta.health(), Health::FailClosed(_));
    meta.rebuild(&[])?;
    Ok(format!(
        "Rebuilt the protection records from the conservative superset: {} pin(s) retained.{}",
        meta.pins().count(),
        if before {
            " Collection is enabled again."
        } else {
            " They were already clean."
        }
    ))
}

/// The current instant as RFC 3339 UTC, to whole seconds.
///
/// Hand-formatted from the epoch rather than through a date library, for the
/// same reason `domowik_assets::time` parses that way: the only thing written
/// here is a timestamp, and a calendar dependency would arrive with a timezone
/// database to produce it.
fn rfc3339_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (mut days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (mut year, mut month) = (1970, 1);
    let leap = |y: i64| (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = |y: i64, m: i64| match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap(y) => 29,
        _ => 28,
    };
    loop {
        let len = if leap(year) { 366 } else { 365 };
        if days < len {
            break;
        }
        days -= len;
        year += 1;
    }
    while days >= month_days(year, month) {
        days -= month_days(year, month);
        month += 1;
    }
    format!(
        "{year:04}-{month:02}-{:02}T{:02}:{:02}:{:02}Z",
        days + 1,
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_document::session::Session;
    use domowik_format::{Hinge, Role, Side};

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn new_then_info_reports_one_level_and_no_walls() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");

        let info = cmd_info(&plan).expect("info");
        assert!(info.contains("levels: 1"), "got {info}");
        assert!(info.contains("walls: 0"), "got {info}");
    }

    #[test]
    fn new_refuses_to_clobber_an_existing_plan() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("first");

        let err = cmd_new(&plan, false).expect_err("must refuse");
        assert_eq!(err.code(), exit::FAILURE);
        assert!(err.message().contains("--force"), "got {}", err.message());
    }

    #[test]
    fn new_with_force_overwrites() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("first");
        cmd_new(&plan, true).expect("forced overwrite");
    }

    #[test]
    fn a_wall_survives_a_save_and_reload() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("add");

        let info = cmd_info(&plan).expect("info");
        assert!(info.contains("walls: 1"), "got {info}");
        assert!(info.contains("total length: 4000 mm"), "got {info}");
    }

    #[test]
    fn an_invalid_wall_is_refused_and_the_file_is_left_untouched() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("good wall");
        let before = std::fs::read(&plan).expect("read");

        // Zero-length: start == end.
        let err = cmd_add_wall(&plan, 10.0, 10.0, 10.0, 10.0, 120.0, 2500.0, None)
            .expect_err("must refuse");
        assert!(
            err.message().contains("zero-length") || err.message().contains("same point"),
            "got {}",
            err.message()
        );

        assert_eq!(
            before,
            std::fs::read(&plan).expect("read"),
            "a rejected wall must not modify the file"
        );
    }

    #[test]
    fn adding_a_wall_to_a_missing_level_names_the_available_ones() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");

        let err = cmd_add_wall(&plan, 0.0, 0.0, 1.0, 1.0, 120.0, 2500.0, Some("Attic"))
            .expect_err("must fail");
        assert!(err.message().contains("Attic"), "got {}", err.message());
        assert!(err.message().contains("Level 1"), "got {}", err.message());
    }

    #[test]
    fn operating_on_a_missing_file_fails_with_exit_one() {
        let dir = tmp();
        let missing = dir.path().join("nope.domo");
        let err = cmd_info(&missing).expect_err("must fail");
        assert_eq!(err.code(), exit::FAILURE);
    }

    #[test]
    fn validate_passes_a_good_plan_and_fails_a_bad_one() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("add");
        assert!(cmd_validate(&plan).expect("valid").contains("valid"));

        // Corrupt the plan behind the CLI's back, by rewriting it as a bare
        // snapshot with no log. `validate` must read the projection, not the
        // stored snapshot alone.
        let pkg = read_package(&plan).expect("read");
        let mut doc = pkg.project().expect("projects").document;
        doc.walls[0].thickness_mm = -5.0;
        write_package(&plan, &Package::new(doc)).expect("write");

        let err = cmd_validate(&plan).expect_err("must fail");
        assert!(
            err.message().contains("thicknessMm"),
            "got {}",
            err.message()
        );
    }

    #[test]
    fn removing_a_wall_also_removes_its_openings() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("wall");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");

        let msg = cmd_remove_wall(&plan, "1").expect("remove");
        assert!(msg.contains("1 opening"), "got {msg}");

        let info = cmd_info(&plan).expect("info");
        assert!(info.contains("walls: 0"), "got {info}");
        assert!(info.contains("openings: 0"), "got {info}");
        // The document must still be valid: a dangling opening would fail here.
        assert!(cmd_validate(&plan).is_ok());
    }

    #[test]
    fn removing_a_wall_that_does_not_exist_says_how_many_there_are() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("wall");

        let err = cmd_remove_wall(&plan, "7").expect_err("must fail");
        assert!(
            err.message().contains("has 1 wall"),
            "got {}",
            err.message()
        );
        assert_eq!(err.code(), exit::FAILURE);
    }

    #[test]
    fn wall_numbering_is_one_based_and_zero_is_a_usage_error() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("wall");

        let err = cmd_remove_wall(&plan, "0").expect_err("must fail");
        assert_eq!(err.code(), exit::USAGE, "0 is a usage error, not a failure");
    }

    #[test]
    fn an_opening_can_be_removed_on_its_own() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("wall");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");

        cmd_remove_opening(&plan, "1").expect("remove opening");
        let info = cmd_info(&plan).expect("info");
        assert!(
            info.contains("walls: 1"),
            "the wall must survive; got {info}"
        );
        assert!(info.contains("openings: 0"), "got {info}");
    }

    #[test]
    fn a_rejected_opening_leaves_the_file_untouched() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 800.0, 0.0, 120.0, 2500.0, None).expect("wall");
        let before = std::fs::read(&plan).expect("read");

        // 900 mm door in an 800 mm wall.
        let err = cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect_err("must refuse");
        assert!(
            err.message().contains("does not fit"),
            "got {}",
            err.message()
        );
        assert_eq!(before, std::fs::read(&plan).expect("read"));
    }

    #[test]
    fn export_svg_writes_a_file_a_browser_can_open() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let out = dir.path().join("plan.svg");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("add");

        cmd_export_svg(&plan, &out, None).expect("export");
        let svg = std::fs::read_to_string(&out).expect("read svg");
        assert!(svg.starts_with("<svg"), "got {svg}");
        assert_eq!(svg.matches("<path").count(), 1, "got {svg}");
    }

    #[test]
    fn a_four_wall_room_round_trips_end_to_end() {
        let dir = tmp();
        let plan = dir.path().join("room.domo");
        let out = dir.path().join("room.svg");
        cmd_new(&plan, false).expect("new");

        let corners = [
            (0.0, 0.0, 4000.0, 0.0),
            (4000.0, 0.0, 4000.0, 3000.0),
            (4000.0, 3000.0, 0.0, 3000.0),
            (0.0, 3000.0, 0.0, 0.0),
        ];
        for (x1, y1, x2, y2) in corners {
            cmd_add_wall(&plan, x1, y1, x2, y2, 120.0, 2500.0, None).expect("wall");
        }

        let info = cmd_info(&plan).expect("info");
        assert!(info.contains("walls: 4"), "got {info}");
        assert!(info.contains("total length: 14000 mm"), "got {info}");
        assert!(info.contains("extent: 4120 x 3120 mm"), "got {info}");

        // Four walls placed corner to corner auto-join into four joins, and
        // the shell draws as ONE component with the room as its hole.
        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.joins.len(), 4, "corners should have auto-joined");
        assert!(info.contains("joins: 4"), "got {info}");

        cmd_export_svg(&plan, &out, None).expect("export");
        let svg = std::fs::read_to_string(&out).expect("svg");
        // One room fill plus one wall component.
        assert_eq!(svg.matches("<path").count(), 2, "got {svg}");

        // 3880 x 2880 mm of floor: measured to the inner faces of the 120 mm
        // walls, not to the 4000 x 3000 centre lines.
        assert!(info.contains("11.17 m²"), "got {info}");
        assert!(
            info.contains("floor area, measured to the inner faces"),
            "got {info}"
        );
    }

    // ---- auto-join (BEHAVIOR-SPEC R3.1.2, R3.3.5) ----

    #[test]
    fn a_wall_placed_on_another_walls_endpoint_auto_joins() {
        let dir = tmp();
        let plan = dir.path().join("l.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("first");
        let msg =
            cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("second");
        assert!(msg.contains("Joined at 1 corner"), "got {msg}");

        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.joins.len(), 1);
        assert_eq!(doc.joins[0].members.len(), 2);
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn a_wall_that_touches_nothing_creates_no_join() {
        let dir = tmp();
        let plan = dir.path().join("apart.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 1000.0, 0.0, 200.0, 2500.0, None).expect("first");
        let msg =
            cmd_add_wall(&plan, 5000.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("second");
        assert!(!msg.contains("joined"), "got {msg}");
        assert!(read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document
            .joins
            .is_empty());
    }

    #[test]
    fn a_third_wall_at_an_occupied_node_merges_into_that_join() {
        // R3.3.5: cardinality grows; no second join may exist at one point.
        let dir = tmp();
        let plan = dir.path().join("tee.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 3000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 3000.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("b");
        cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None).expect("c");

        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.joins.len(), 1, "one node, one join: {:#?}", doc.joins);
        assert_eq!(doc.joins[0].members.len(), 3);
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn walls_on_different_levels_never_join() {
        // R3.0.6: same-XY walls on different levels have no contact.
        let dir = tmp();
        let plan = dir.path().join("stack.domo");
        cmd_new(&plan, false).expect("new");
        let mut plan_file = Plan::open(&plan).expect("open");
        plan_file
            .session()
            .expect("a freshly created plan is editable")
            .commit(vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Level 2"),
            ))])
            .expect("second level");
        plan_file.save().expect("save");

        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("lower");
        let msg = cmd_add_wall(
            &plan,
            4000.0,
            0.0,
            4000.0,
            3000.0,
            200.0,
            2500.0,
            Some("Level 2"),
        )
        .expect("upper");
        assert!(!msg.contains("joined"), "got {msg}");
        assert!(read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document
            .joins
            .is_empty());
    }

    #[test]
    fn removing_a_wall_drops_its_join_memberships() {
        let dir = tmp();
        let plan = dir.path().join("drop.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("b");
        assert_eq!(
            read_package(&plan)
                .expect("read")
                .project()
                .expect("projects")
                .document
                .joins
                .len(),
            1
        );

        cmd_remove_wall(&plan, "2").expect("remove");
        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert!(
            doc.joins.is_empty(),
            "a join below two members must go with the wall: {:#?}",
            doc.joins
        );
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn removing_one_member_of_a_three_way_join_leaves_the_join_standing() {
        let dir = tmp();
        let plan = dir.path().join("tee2.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 3000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 3000.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("b");
        cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None).expect("c");

        cmd_remove_wall(&plan, "3").expect("remove the stem");
        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.joins.len(), 1);
        assert_eq!(doc.joins[0].members.len(), 2);
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn info_reports_floor_area_not_the_centre_line_area() {
        let dir = tmp();
        let plan = dir.path().join("area.domo");
        cmd_new(&plan, false).expect("new");
        let corners = [
            (0.0, 0.0, 4000.0, 0.0),
            (4000.0, 0.0, 4000.0, 3000.0),
            (4000.0, 3000.0, 0.0, 3000.0),
            (0.0, 3000.0, 0.0, 0.0),
        ];
        for (x1, y1, x2, y2) in corners {
            cmd_add_wall(&plan, x1, y1, x2, y2, 200.0, 2500.0, None).expect("wall");
        }
        let info = cmd_info(&plan).expect("info");
        // 3800 x 2800 = 10.64 m². The centre-line figure would be 12.00 m².
        assert!(info.contains("10.64 m²"), "got {info}");
        assert!(!info.contains("12.00 m²"), "got {info}");
    }

    // ---- operation log, undo, apply, ops (issues 0011, 0015, 0027) ----

    #[test]
    fn a_new_plan_has_its_level_in_the_snapshot_with_no_history_to_undo() {
        // Issue 0053: the baseline is written straight into the snapshot, not
        // committed as an operation, so a fresh plan has NOTHING to undo back
        // past — matching the GUI's non-undoable
        // `domowik_plan_create_default` baseline.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        let pkg = read_package(&plan).expect("read");
        assert!(pkg.batches.is_empty(), "the baseline is not an operation");
        assert_eq!(pkg.snapshot.levels.len(), 1, "the snapshot covers it");
        assert_eq!(pkg.snapshot.levels[0].name, "Level 1");
        assert_eq!(pkg.project().expect("projects").document.levels.len(), 1);
    }

    #[test]
    fn a_fresh_plan_has_nothing_for_undo_to_touch() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        let before = read_package(&plan).expect("read");
        let err = cmd_undo(&plan).expect_err("no operations exist yet");
        assert!(
            err.message().contains("no operations to undo"),
            "got {err:?}"
        );
        let after = read_package(&plan).expect("read");
        assert_eq!(before, after, "a refused undo must not touch the file");
    }

    #[test]
    fn undoing_a_legacy_first_create_batch_is_refused_not_silently_emptied() {
        // A file created by the OLD `cmd_new` (before issue 0053): the
        // baseline level lives in the log as an ordinary committed batch,
        // exactly like `a_new_plan_has_its_level_in_the_log...` used to
        // assert. Undoing that batch is a level DELETION that would leave
        // the plan with zero levels, which `Session`'s shared
        // `CommandError::LevelDeletion` guard refuses before it ever touches
        // the log or session state (R2.2) — old files are read as they are,
        // never silently migrated, so this must keep failing exactly the
        // way it always would have.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let level = domowik_format::Level::new("Ground");
        let mut session = Session::new();
        session
            .commit(vec![Operation::create(domowik_format::Entity::Level(
                level,
            ))])
            .expect("commit");
        write_package(
            &plan,
            &Package {
                frontier: Default::default(),
                tombstones: Vec::new(),
                snapshot_unknown: Default::default(),
                descriptors: Default::default(),
                manifest: domowik_container::Manifest::default(),
                snapshot: Document::default(),
                batches: session.log().batches().to_vec(),
                assets: Default::default(),
            },
        )
        .expect("write legacy package");
        let before = std::fs::read(&plan).expect("read bytes");

        let err = cmd_undo(&plan).expect_err("would leave zero levels");
        assert!(err.message().contains("only level"), "got {err:?}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "a refused undo must not touch the file");
    }

    // ---- level lifecycle (issue 0053) ----

    #[test]
    fn remove_level_deletes_an_empty_unreferenced_non_last_level() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_level(&plan, "Loft", 2700.0).expect("add");
        cmd_remove_level(&plan, "Loft").expect("remove");
        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.levels.len(), 1);
        assert_eq!(doc.levels[0].name, "Level 1");
    }

    #[test]
    fn remove_level_rejects_the_plan_s_only_level_byte_unchanged() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        let before = std::fs::read(&plan).expect("read bytes");

        let err = cmd_remove_level(&plan, "Level 1").expect_err("last level");
        assert!(err.message().contains("only level"), "got {err:?}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "a rejected removal must not touch the file");
    }

    #[test]
    fn remove_level_rejects_a_level_that_still_hosts_a_wall_and_opening() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_level(&plan, "Loft", 2700.0).expect("add");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, Some("Loft")).expect("wall");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let before = std::fs::read(&plan).expect("read bytes");

        let err = cmd_remove_level(&plan, "Loft").expect_err("nonempty level");
        assert!(err.message().contains("1 wall"), "got {err:?}");
        assert!(err.message().contains("1 opening"), "got {err:?}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "a rejected removal must not touch the file");
    }

    #[test]
    fn remove_level_rejects_a_level_still_carrying_a_background_image() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let mut ground = domowik_format::Level::new("Level 1");
        let mut loft = domowik_format::Level::new("Loft");
        loft.elevation_mm = 2700.0;
        loft.background_images
            .push(domowik_format::BackgroundImage {
                digest: format!("sha256:{}", "a".repeat(64)),
                anchor: domowik_format::Anchor {
                    translation_mm: [0.0, 0.0],
                    scale_uniform: 1.0,
                    rotation_deg: 0.0,
                    unknown: Default::default(),
                },
                stack_ordinal: 0,
                unknown: Default::default(),
            });
        ground.elevation_mm = 0.0;
        write_package(
            &plan,
            &Package {
                frontier: Default::default(),
                tombstones: Vec::new(),
                snapshot_unknown: Default::default(),
                // Declared OPTIONAL so the plan opens editable. The blob is
                // deliberately absent, and BEHAVIOR-SPEC §9.6 would otherwise
                // put the plan in read-only degraded mode and refuse the
                // removal for THAT reason — which is correct behaviour and
                // the wrong thing for this test to be measuring. §9.6's
                // Optional/Thin/missing row is "opens editable with
                // placeholder rendering", which is the state this test wants:
                // a level that carries a background image, whatever the blob's
                // availability.
                descriptors: [(
                    format!("sha256:{}", "a".repeat(64)),
                    serde_json::json!({
                        "digest": format!("sha256:{}", "a".repeat(64)),
                        "requirement": "optional"
                    }),
                )]
                .into_iter()
                .collect(),
                manifest: domowik_container::Manifest::default(),
                snapshot: Document {
                    levels: vec![ground, loft],
                    ..Document::default()
                },
                batches: Vec::new(),
                assets: Default::default(),
            },
        )
        .expect("write package");
        let before = std::fs::read(&plan).expect("read bytes");

        let err = cmd_remove_level(&plan, "Loft").expect_err("still referenced");
        assert!(err.message().contains("background image"), "got {err:?}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "a rejected removal must not touch the file");
    }

    #[test]
    fn remove_level_rejects_an_ambiguous_duplicate_name() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        // R2.2 permits duplicate level NAMES, even though `cmd_add_level`
        // itself refuses to create one this way — simulate the state
        // directly, as an external batch or an older client might produce.
        let mut second = domowik_format::Level::new("Level 1");
        second.elevation_mm = 3000.0;
        let mut plan_handle = Plan::open(&plan).expect("open");
        plan_handle
            .commit_and_save(vec![Operation::create(domowik_format::Entity::Level(
                second,
            ))])
            .expect("commit second same-named level");

        let err = cmd_remove_level(&plan, "Level 1").expect_err("ambiguous");
        assert!(
            err.message().contains("2 levels") && err.message().contains("ambiguous"),
            "got {err:?}"
        );
    }

    #[test]
    fn external_apply_cannot_bypass_the_last_level_guard() {
        // Regression: the guard lives in `Session`, not in
        // `cmd_remove_level`'s own name-resolution code, so an
        // externally-applied batch that deletes the plan's only level must
        // be refused exactly like the CLI's own remove-level command is.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        let id = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document
            .levels[0]
            .id;
        let before = std::fs::read(&plan).expect("read bytes");

        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![Operation::delete(id)],
        );
        let batch_path = dir.path().join("batch.json");
        std::fs::write(&batch_path, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        let err = cmd_apply(&plan, &batch_path, false).expect_err("would leave zero levels");
        assert!(err.message().contains("only level"), "got {err:?}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "a rejected apply must not touch the file");
    }

    #[test]
    fn a_raw_zero_level_package_fails_validation_and_repairs_with_add_level() {
        // A hand-built package with an empty snapshot and no batches is a
        // legal INTERNAL format state (`Document::validate` alone would
        // pass it) but not a valid materialized plan — proving
        // `validate_plan`'s split actually distinguishes the two, per its
        // own doc comment, rather than the two methods agreeing by
        // accident. `write_package` is used directly here as the
        // fixture/repair tool for an unpublished builder state; ordinary
        // commands go through `Plan`, which always requires a materialized
        // level (`Plan::save`/`Session`'s guard).
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        write_package(
            &plan,
            &Package {
                frontier: Default::default(),
                tombstones: Vec::new(),
                snapshot_unknown: Default::default(),
                descriptors: Default::default(),
                manifest: domowik_container::Manifest::default(),
                snapshot: Document::default(),
                batches: Vec::new(),
                assets: Default::default(),
            },
        )
        .expect("write raw zero-level package");
        let before = std::fs::read(&plan).expect("read bytes");

        let human_err = cmd_validate(&plan).expect_err("zero levels");
        assert!(human_err.message().contains("level"), "got {human_err:?}");

        let json_err = cmd_validate_json(&plan).expect_err("zero levels");
        let CliError::Report(body) = json_err else {
            panic!("expected a Report, got {json_err:?}");
        };
        let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["valid"], false, "got {body}");
        assert!(body.to_lowercase().contains("level"), "got {body}");

        let after = std::fs::read(&plan).expect("read bytes");
        assert_eq!(before, after, "validation is read-only");

        cmd_add_level(&plan, "Level 1", 0.0).expect("repair by adding a level");
        cmd_validate(&plan).expect("now valid");
        let repaired_json = cmd_validate_json(&plan).expect("now valid");
        let repaired: serde_json::Value = serde_json::from_str(&repaired_json).expect("valid JSON");
        assert_eq!(repaired["valid"], true, "got {repaired_json}");
    }

    #[test]
    fn every_command_records_a_batch() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let pkg = read_package(&plan).expect("read");
        // "new" no longer contributes a batch (issue 0053: non-undoable
        // baseline written straight into the snapshot).
        assert_eq!(pkg.batches.len(), 2, "add-wall, add-opening");
    }

    #[test]
    fn a_walls_creation_and_its_join_are_one_batch() {
        // R3.1.1: an accepted creation is exactly one atomic batch INCLUDING
        // its incidental join effects — and therefore one undo entry.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("first");
        cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("second");

        let pkg = read_package(&plan).expect("read");
        let last = pkg.batches.last().expect("a batch");
        assert_eq!(
            last.operations.len(),
            2,
            "the wall and its join, together: {:?}",
            last.operations
        );
    }

    #[test]
    fn undo_appends_a_compensating_batch_and_removes_the_effect() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        let before = read_package(&plan).expect("read").batches.len();

        cmd_undo(&plan).expect("undo");
        let pkg = read_package(&plan).expect("read");
        assert_eq!(pkg.batches.len(), before + 1, "history GREW");
        assert!(pkg.project().expect("projects").document.walls.is_empty());
    }

    #[test]
    fn undo_of_an_undo_restores_what_the_first_undo_removed() {
        // The second undo inverts the compensating batch against the state
        // BEFORE it — where the wall was still present. Inverting against the
        // current projection cannot do this: the entity is gone.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        let wall_id = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document
            .walls[0]
            .id;

        cmd_undo(&plan).expect("undo");
        cmd_undo(&plan).expect("undo again");

        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.walls.len(), 1);
        assert_eq!(doc.walls[0].id, wall_id, "the SAME entity id (R8.1.3)");
    }

    #[test]
    fn undo_on_a_plan_with_no_operations_fails_rather_than_pretending() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        write_package(&plan, &Package::new(Document::with_ground_floor())).expect("write");
        let err = cmd_undo(&plan).expect_err("nothing to undo");
        assert!(
            err.message().contains("no operations"),
            "got {}",
            err.message()
        );
    }

    #[test]
    fn apply_accepts_an_external_batch_and_projects_it() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");

        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        let msg = cmd_apply(&plan, &batch_file, false).expect("apply");
        assert!(msg.contains("Applied"), "got {msg}");
        let doc = read_package(&plan)
            .expect("read")
            .project()
            .expect("projects")
            .document;
        assert_eq!(doc.levels.len(), 2);
        assert!(doc.levels.iter().any(|l| l.name == "Loft"));
    }

    #[test]
    fn a_dry_run_apply_reports_the_projection_and_writes_nothing() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");

        let before = std::fs::read(&plan).expect("read plan");
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        let msg = cmd_apply(&plan, &batch_file, true).expect("dry run");
        assert!(msg.contains("Would apply"), "got {msg}");
        // The PROJECTION is reported, not just the intent. Without this the
        // dry run would be indistinguishable from printing the file name back.
        assert!(msg.contains("1 more entities"), "got {msg}");
        assert!(msg.contains("--dry-run"), "got {msg}");

        // Byte-identical, not merely "still projects the same document": a
        // dry run that rewrote the package with the same content would still
        // have taken the write path this flag exists to avoid.
        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "--dry-run wrote to the plan"
        );

        // And the batch is still deliverable afterwards, so the dry run did
        // not consume it.
        let msg = cmd_apply(&plan, &batch_file, false).expect("apply for real");
        assert!(msg.contains("Applied"), "got {msg}");
    }

    #[test]
    fn a_dry_run_reports_a_duplicate_and_a_held_batch_without_writing() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");
        let actor = Uuid::now_v7();
        let batch = Batch::new(
            actor,
            1,
            Default::default(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");
        cmd_apply(&plan, &batch_file, false).expect("apply");

        let after = std::fs::read(&plan).expect("read plan");
        let msg = cmd_apply(&plan, &batch_file, true).expect("dry run");
        assert!(msg.contains("Would ignore"), "got {msg}");
        assert!(msg.contains("already applied"), "got {msg}");

        // A batch whose prerequisite never arrived.
        let mut frontier = std::collections::BTreeMap::new();
        frontier.insert(Uuid::now_v7(), 9u64);
        let held = Batch::new(
            Uuid::now_v7(),
            1,
            frontier,
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Attic"),
            ))],
        );
        let held_file = dir.path().join("held.json");
        std::fs::write(&held_file, serde_json::to_vec(&held).unwrap()).expect("write batch");
        let msg = cmd_apply(&plan, &held_file, true).expect("dry run");
        assert!(msg.contains("Would hold"), "got {msg}");

        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            after,
            "--dry-run wrote to the plan"
        );
    }

    #[test]
    fn an_external_batch_over_the_operation_limit_is_refused_before_it_is_applied() {
        // FORMAT-SPEC §9.3 row L23. The package reader enforces this; the
        // `apply` path does not go through the package reader, so without its
        // own check the batch would be bounded only at save, having already
        // been validated and projected.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");

        let limit = Limits::default().max_operations_per_batch as usize;
        let operations: Vec<Operation> = (0..limit + 1)
            .map(|i| Operation::create(Entity::Level(domowik_format::Level::new(format!("L{i}")))))
            .collect();
        let batch = Batch::new(Uuid::now_v7(), 1, Default::default(), operations);
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        let before = std::fs::read(&plan).expect("read plan");
        let err = cmd_apply(&plan, &batch_file, false).expect_err("must refuse");
        let message = err.message().to_string();
        assert!(message.contains("L23"), "got {message}");
        assert!(message.contains(&(limit + 1).to_string()), "got {message}");
        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "a refused batch was written anyway"
        );
    }

    /// Depth under row L11's counting rule, derived by probing the guard
    /// rather than by a second scanner that could drift from it.
    fn depth_of(bytes: &[u8]) -> u32 {
        (0..=512)
            .find(|&n| domowik_container::check_nesting_depth(bytes, n).is_ok())
            .expect("depth under 512")
    }

    /// A SEMANTICALLY VALID batch creating one level, whose serialized text
    /// sits at exactly `target` levels because the level carries an unknown
    /// optional field nested to suit. §8 U2 requires that field to be
    /// preserved, so nothing here is contrived: it is a batch a newer writer
    /// could legitimately send to an older reader.
    fn batch_at_depth(target: u32) -> Vec<u8> {
        for pad in 1..512u32 {
            let mut level = domowik_format::Level::new("Loft");
            let mut nest = serde_json::json!(1);
            for _ in 0..pad {
                nest = serde_json::Value::Array(vec![nest]);
            }
            level.unknown.insert("futureField".to_owned(), nest);
            let batch = Batch::new(
                Uuid::now_v7(),
                1,
                Default::default(),
                vec![Operation::create(Entity::Level(level))],
            );
            let bytes = serde_json::to_vec(&batch).unwrap();
            if depth_of(&bytes) == target {
                return bytes;
            }
        }
        panic!("no padding reaches depth {target}");
    }

    /// Entry point B, the `apply` path, across the whole boundary.
    ///
    /// The accepting and rejecting depths are read OFF THE CORPUS TWINS rather
    /// than written here a second time (ED-0089). The corpus artifacts are bare
    /// arrays and no schema admits one as a batch, so they cannot be applied
    /// directly; what they supply is the two depths, and the payloads below are
    /// valid batches built to match them.
    #[test]
    fn apply_admits_127_and_128_and_rejects_129() {
        let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus");
        let at = depth_of(
            &std::fs::read(corpus.join("golden/parser-twin-nesting-depth-at-L11.json"))
                .expect("golden twin"),
        );
        let over = depth_of(
            &std::fs::read(corpus.join("hostile/json/nesting-depth-over-L11.json"))
                .expect("hostile twin"),
        );
        assert_eq!(at, Limits::default().max_nesting_depth);
        assert_eq!(over, at + 1);

        // 127 and 128 are ADMITTED, and admitted all the way: the batch applies
        // and the level lands. Depth admission is separated from later
        // semantics by there being no later failure to confuse it with.
        for target in [at - 1, at] {
            let dir = tmp();
            let plan = dir.path().join("plan.domo");
            let batch_file = dir.path().join("batch.json");
            cmd_new(&plan, false).expect("new");
            std::fs::write(&batch_file, batch_at_depth(target)).expect("write batch");

            let msg = cmd_apply(&plan, &batch_file, false).unwrap_or_else(|e| {
                panic!("depth {target} conforms and must apply: {}", e.message())
            });
            assert!(msg.contains("Applied"), "depth {target}: got {msg}");

            let doc = read_package(&plan)
                .expect("read")
                .project()
                .expect("projects")
                .document;
            let loft = doc
                .levels
                .iter()
                .find(|l| l.name == "Loft")
                .expect("the level applied");
            assert!(
                loft.unknown.contains_key("futureField"),
                "depth {target}: §8 U2 preservation must survive the depth guard"
            );
        }

        // 129 is REFUSED, for depth, before the parser sees it.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");
        std::fs::write(&batch_file, batch_at_depth(over)).expect("write batch");

        let before = std::fs::read(&plan).expect("read plan");
        let message = cmd_apply(&plan, &batch_file, false)
            .expect_err("depth 129 must refuse")
            .message()
            .to_string();
        assert!(message.contains("limit-nesting-depth"), "got {message}");
        assert!(message.contains("L11"), "got {message}");
        assert!(message.contains(&over.to_string()), "got {message}");
        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "a refused batch was written anyway"
        );
    }

    #[test]
    fn an_external_batch_nested_past_row_l11_is_refused_before_it_is_parsed() {
        // FORMAT-SPEC §9.3 row L11, on the path that does not go through the
        // package reader. Issue 0015's "depth is enforced" criterion had no
        // enforcement behind it here: the only thing bounding depth was
        // serde_json's incidental recursion limit, which is not this limit and
        // does not report this token.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("deep.json");
        cmd_new(&plan, false).expect("new");

        // One level past the limit, wrapped so the bytes are a plausible batch
        // rather than a bare array. The depth is what must reject it.
        let limit = Limits::default().max_nesting_depth;
        let mut json = String::from("{\"operations\":");
        for _ in 0..limit {
            json.push('[');
        }
        json.push('1');
        for _ in 0..limit {
            json.push(']');
        }
        json.push('}');
        std::fs::write(&batch_file, &json).expect("write batch");

        let before = std::fs::read(&plan).expect("read plan");
        let err = cmd_apply(&plan, &batch_file, false).expect_err("must refuse");
        let message = err.message().to_string();
        assert!(message.contains("L11"), "got {message}");
        assert!(message.contains("limit-nesting-depth"), "got {message}");
        assert!(
            message.contains(&(limit + 1).to_string()),
            "the message must name the offending depth: {message}"
        );
        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "a refused batch was written anyway"
        );
    }

    #[test]
    fn an_external_batch_file_over_the_size_limit_is_refused_before_it_is_read() {
        // The declared size is checked BEFORE the read, so this costs a stat
        // rather than the bytes. A sparse file makes the test cheap on disk
        // while still declaring a length past the limit.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("huge.json");
        cmd_new(&plan, false).expect("new");

        let f = std::fs::File::create(&batch_file).expect("create");
        f.set_len(Limits::default().max_entry_bytes + 1)
            .expect("grow");
        drop(f);

        let err = cmd_apply(&plan, &batch_file, false).expect_err("must refuse");
        let message = err.message().to_string();
        assert!(message.contains("9.3"), "got {message}");
        assert!(message.contains("declares"), "got {message}");
    }

    #[test]
    fn applying_the_same_batch_twice_is_ignored_not_duplicated() {
        // FORMAT-SPEC V1/I2.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        cmd_apply(&plan, &batch_file, false).expect("first");
        let msg = cmd_apply(&plan, &batch_file, false).expect("second");
        assert!(msg.contains("already applied"), "got {msg}");
        assert_eq!(
            read_package(&plan)
                .expect("read")
                .project()
                .expect("projects")
                .document
                .levels
                .len(),
            2
        );
    }

    #[test]
    fn apply_reports_a_held_batch_rather_than_claiming_success() {
        // F3: a batch whose causal prerequisites are absent is HELD. Saying
        // "applied" would be a lie, and saying "rejected" would be wrong too.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");

        let ghost = Uuid::now_v7();
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            [(ghost, 7)].into_iter().collect(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write batch");

        let msg = cmd_apply(&plan, &batch_file, false).expect("held");
        assert!(msg.contains("Held"), "got {msg}");
        assert_eq!(
            read_package(&plan)
                .expect("read")
                .project()
                .expect("projects")
                .document
                .levels
                .len(),
            1,
            "a held batch has no effect"
        );
    }

    #[test]
    fn apply_rejects_a_malformed_batch_before_any_effect() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let batch_file = dir.path().join("batch.json");
        cmd_new(&plan, false).expect("new");
        std::fs::write(&batch_file, b"{not json").expect("write");
        let err = cmd_apply(&plan, &batch_file, false).expect_err("must reject");
        assert!(
            err.message().contains("not a valid batch"),
            "got {}",
            err.message()
        );
    }

    #[test]
    fn ops_lists_batches_in_canonical_order() {
        // The baseline level is no longer a batch (issue 0053), so an
        // explicit `add-level` stands in for it to keep this test's
        // "create level" and "create wall" coverage meaningful.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_level(&plan, "Loft", 2700.0).expect("level");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        let out = cmd_ops(&plan, false).expect("ops");
        assert!(out.contains("2 batch(es)"), "got {out}");
        assert!(out.contains("create level"), "got {out}");
        assert!(out.contains("create wall"), "got {out}");
        assert!(out.contains("canonical order"), "got {out}");
    }

    #[test]
    fn the_projection_survives_a_save_and_reload_unchanged() {
        // Issue 0011's session-persistence criterion: what the log projects
        // must be what the file reopens as.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        for (x1, y1, x2, y2) in [
            (0.0, 0.0, 4000.0, 0.0),
            (4000.0, 0.0, 4000.0, 3000.0),
            (4000.0, 3000.0, 0.0, 3000.0),
            (0.0, 3000.0, 0.0, 0.0),
        ] {
            cmd_add_wall(&plan, x1, y1, x2, y2, 200.0, 2500.0, None).expect("wall");
        }
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");

        let first = read_package(&plan)
            .expect("read")
            .project()
            .expect("p")
            .document;
        // Reopen and save without editing; the projection must not drift.
        let mut plan_file = Plan::open(&plan).expect("open");
        plan_file.save().expect("save");
        let second = read_package(&plan)
            .expect("read")
            .project()
            .expect("p")
            .document;
        assert_eq!(first, second);
        assert_eq!(first.joins.len(), 4);
    }

    // ---- editing commands (BEHAVIOR-SPEC R3.4) ----

    fn corner_plan(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let plan = dir.path().join("corner.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("b");
        plan
    }

    fn doc_of(plan: &Path) -> Document {
        read_package(plan)
            .expect("read")
            .project()
            .expect("projects")
            .document
    }

    #[test]
    fn moving_a_joined_endpoint_moves_the_whole_node() {
        // R3.4.2: a numeric edit of a joined endpoint acts on the JOIN NODE,
        // moving every member endpoint rigidly together in one batch.
        let dir = tmp();
        let plan = corner_plan(&dir);
        let msg = cmd_set_wall(&plan, "1", None, Some((4500.0, 500.0)), None, None).expect("move");
        assert!(msg.contains("Moved 1 joined endpoint"), "got {msg}");

        let doc = doc_of(&plan);
        assert_eq!(doc.walls[0].end_mm, [4500.0, 500.0]);
        assert_eq!(
            doc.walls[1].start_mm,
            [4500.0, 500.0],
            "the other member must move with the node"
        );
        assert_eq!(doc.validate(), vec![], "R3.3.4 coordinate agreement holds");
    }

    #[test]
    fn moving_a_free_endpoint_moves_only_that_wall() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        let msg = cmd_set_wall(&plan, "1", Some((-500.0, 0.0)), None, None, None).expect("move");
        assert!(!msg.contains("Moved"), "got {msg}");
        let doc = doc_of(&plan);
        assert_eq!(doc.walls[0].start_mm, [-500.0, 0.0]);
        assert_eq!(doc.walls[1].start_mm, [4000.0, 0.0]);
    }

    #[test]
    fn an_edit_keeps_the_walls_in_their_existing_order() {
        // The CLI numbers walls by position, so an edit that renumbered them
        // would make the same command mean two different things on two runs.
        let dir = tmp();
        let plan = corner_plan(&dir);
        let before: Vec<Uuid> = doc_of(&plan).walls.iter().map(|w| w.id).collect();
        cmd_set_wall(&plan, "1", None, None, Some(150.0), None).expect("thicken");
        let after: Vec<Uuid> = doc_of(&plan).walls.iter().map(|w| w.id).collect();
        assert_eq!(before, after);
        assert_eq!(doc_of(&plan).walls[0].thickness_mm, 150.0);
    }

    #[test]
    fn an_edit_that_moves_a_wall_onto_another_endpoint_joins_in_the_same_batch() {
        // R3.4.4: incidental resolution joins the SAME single batch as the
        // edit, so the whole thing is one undo entry.
        let dir = tmp();
        let plan = dir.path().join("apart.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 6000.0, 0.0, 6000.0, 3000.0, 200.0, 2500.0, None).expect("b");
        assert!(doc_of(&plan).joins.is_empty());

        let before = read_package(&plan).expect("read").batches.len();
        let msg = cmd_set_wall(&plan, "1", None, Some((6000.0, 0.0)), None, None).expect("move");
        assert!(msg.contains("Joined at 1 new corner"), "got {msg}");

        let pkg = read_package(&plan).expect("read");
        assert_eq!(pkg.batches.len(), before + 1, "ONE batch, not two");
        assert_eq!(pkg.project().expect("p").document.joins.len(), 1);
    }

    #[test]
    fn an_edit_that_would_produce_an_invalid_plan_is_rejected_atomically() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        let before = doc_of(&plan);
        let batches = read_package(&plan).expect("read").batches.len();

        let err = cmd_set_wall(&plan, "1", None, None, Some(-5.0), None).expect_err("must reject");
        assert!(
            err.message().contains("thicknessMm"),
            "got {}",
            err.message()
        );
        assert_eq!(doc_of(&plan), before, "no effect may reach the file");
        assert_eq!(read_package(&plan).expect("read").batches.len(), batches);
    }

    #[test]
    fn set_wall_with_no_change_requested_is_a_usage_error() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        let err = cmd_set_wall(&plan, "1", None, None, None, None).expect_err("usage");
        assert_eq!(err.code(), exit::USAGE);
    }

    #[test]
    fn set_opening_changes_its_parameters() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        cmd_set_opening(
            &plan,
            "1",
            Some(Placement::Fraction(0.25)),
            Some(800.0),
            None,
            None,
            Orientation::default(),
        )
        .expect("edit");
        let doc = doc_of(&plan);
        assert_eq!(doc.openings[0].u, 0.25);
        assert_eq!(doc.openings[0].width_mm, 800.0);
        assert_eq!(doc.openings[0].height_mm, 2100.0, "untouched");
    }

    #[test]
    fn an_opening_edit_that_would_not_fit_its_host_is_rejected() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let err = cmd_set_opening(
            &plan,
            "1",
            Some(Placement::Fraction(0.99)),
            None,
            None,
            None,
            Orientation::default(),
        )
        .expect_err("reject");
        assert!(
            err.message().contains("does not fit"),
            "got {}",
            err.message()
        );
        assert_eq!(doc_of(&plan).openings[0].u, 0.5, "unchanged");
    }

    #[test]
    fn add_level_and_set_level_round_trip() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        cmd_add_level(&plan, "Loft", 2700.0).expect("add");
        let doc = doc_of(&plan);
        assert_eq!(doc.levels.len(), 2);
        assert_eq!(doc.levels[1].elevation_mm, 2700.0);

        cmd_set_level(&plan, "Loft", Some("Attic"), Some(3000.0)).expect("rename");
        let doc = doc_of(&plan);
        assert_eq!(doc.levels[1].name, "Attic");
        assert_eq!(doc.levels[1].elevation_mm, 3000.0);
    }

    #[test]
    fn adding_a_level_with_an_existing_name_is_refused() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        let err = cmd_add_level(&plan, "Level 1", 0.0).expect_err("duplicate");
        assert!(
            err.message().contains("already has a level"),
            "got {}",
            err.message()
        );
    }

    #[test]
    fn level_properties_are_atomic_persistent_and_compensatable() {
        let dir = tmp();
        let path = dir.path().join("fields.domo");
        cmd_new(&path, false).unwrap();
        let original = doc_of(&path);
        let fields = domowik_edit::LevelFields {
            name: Some("  Basement  ".into()),
            elevation_mm: Some(-2800.0),
            floor_thickness_mm: Some(0.0),
            default_wall_height_mm: Some(3200.0),
            default_ceiling_height_mm: Some(2900.0),
        };
        cmd_set_level_fields(&path, "Level 1", &fields).unwrap();
        let changed = doc_of(&path);
        let level = &changed.levels[0];
        assert_eq!(level.name, "  Basement  ");
        assert_eq!(level.elevation_mm, -2800.0);
        assert_eq!(level.floor_thickness_mm, 0.0);
        assert_eq!(level.default_wall_height_mm, 3200.0);
        assert_eq!(level.default_ceiling_height_mm, 2900.0);
        let package = read_package(&path).unwrap();
        assert_eq!(package.batches.len(), 1);
        assert_eq!(package.batches[0].operations.len(), 1);
        let before = std::fs::read(&path).unwrap();
        cmd_set_level_fields(&path, "  Basement  ", &fields).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "unchanged means no save"
        );
        let bad = domowik_edit::LevelFields {
            name: Some("partial".into()),
            default_wall_height_mm: Some(0.0),
            ..Default::default()
        };
        assert!(cmd_set_level_fields(&path, "  Basement  ", &bad).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        cmd_undo(&path).unwrap();
        assert_eq!(
            doc_of(&path),
            original,
            "one compensation restores all five fields"
        );
    }

    #[test]
    fn level_warnings_are_structured_and_do_not_invalidate_the_plan() {
        let dir = tmp();
        let path = dir.path().join("warnings.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_level(&path, "Upper", 0.0).unwrap();
        cmd_set_level_fields(
            &path,
            "Upper",
            &domowik_edit::LevelFields {
                default_ceiling_height_mm: Some(2000.0),
                ..Default::default()
            },
        )
        .unwrap();
        let before = std::fs::read(&path).unwrap();
        let human = cmd_validate(&path).unwrap();
        assert!(human.contains("valid."));
        assert!(human.contains("3 warning(s)"), "{human}");
        let report = serde_json::to_value(report(&path).unwrap()).unwrap();
        assert_eq!(report["valid"], true);
        assert_eq!(report["problems"], serde_json::json!([]));
        let warnings = report["warnings"].as_array().unwrap();
        assert_eq!(warnings.len(), 3);
        for warning in warnings {
            assert!(warning["code"].is_string());
            assert!(warning["level"].is_string());
            assert!(warning["message"].as_str().unwrap().contains("level"));
        }
        assert_eq!(report["levels"][1]["defaultCeilingHeightMm"], 2000.0);
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "validation is read-only"
        );
    }

    #[test]
    fn wall_area_overlap_warning_is_surfaced_immediately_by_add_wall_and_by_validate() {
        let dir = tmp();
        let path = dir.path().join("overlap.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_wall(&path, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).unwrap();

        // Parallel, 50mm apart with t=200 each: bands [-100,100] and
        // [-50,150] overlap. No axis contact at all (R3.1.2's last row).
        let msg = cmd_add_wall(&path, 0.0, 50.0, 4000.0, 50.0, 200.0, 2500.0, None)
            .expect("the warning does not block the batch");
        assert!(
            msg.contains("wall-area-overlap"),
            "add-wall's own output must surface the warning immediately, not only \
             a later `validate`, got {msg}"
        );

        let human = cmd_validate(&path).unwrap();
        assert!(human.contains("wall-area-overlap"), "{human}");
        let report = serde_json::to_value(report(&path).unwrap()).unwrap();
        let warnings = report["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w["code"] == "wall-area-overlap"),
            "{report}"
        );
    }

    #[test]
    fn wall_area_overlap_warning_lifecycle_across_move_thickness_and_undo() {
        let dir = tmp();
        let path = dir.path().join("lifecycle.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_wall(&path, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).unwrap();
        let msg = cmd_add_wall(&path, 0.0, 250.0, 4000.0, 250.0, 200.0, 2500.0, None).unwrap();
        assert!(
            !msg.contains("wall-area-overlap"),
            "250mm apart at t=200 each is clear, got {msg}"
        );

        // Widening wall 2's thickness alone brings its band down to overlap
        // wall 1's — no axis write at all, and it must still be reclassified.
        let msg = cmd_set_wall(&path, "2", None, None, Some(400.0), None).unwrap();
        assert!(
            msg.contains("wall-area-overlap"),
            "a thickness-only change must be reclassified, got {msg}"
        );

        // Moving wall 2 far away clears it.
        let msg = cmd_set_wall(
            &path,
            "2",
            Some((0.0, 5000.0)),
            Some((4000.0, 5000.0)),
            None,
            None,
        )
        .unwrap();
        assert!(!msg.contains("wall-area-overlap"), "moved clear, got {msg}");

        // Undoing the move restores the position — and with it, the warning.
        let msg = cmd_undo(&path).unwrap();
        assert!(
            msg.contains("wall-area-overlap"),
            "undo must restore the warning along with the position it compensated, got {msg}"
        );
    }

    #[test]
    fn no_warning_is_emitted_for_a_rejected_batch_or_a_non_effective_apply() {
        let dir = tmp();
        let path = dir.path().join("rejected.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_wall(&path, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).unwrap();
        let before = std::fs::read(&path).unwrap();

        // Exact coincidence: R3.1.2 rejects the whole batch outright, before
        // any warning could ever be computed from it.
        let err = cmd_add_wall(&path, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None)
            .expect_err("coincidence must reject");
        assert!(
            !err.message().contains("Warning:"),
            "a rejected batch must never surface a warning, got {}",
            err.message()
        );
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "the file must be untouched by the rejection"
        );

        // A batch already applied once: the second `apply` is a no-op
        // (Duplicate), never an occasion to compute warnings from.
        let batch_file = dir.path().join("b.json");
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![set_field(Uuid::now_v7(), "name", serde_json::json!("x"))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).unwrap();
        cmd_apply(&path, &batch_file, false).unwrap();
        let msg = cmd_apply(&path, &batch_file, false).unwrap();
        assert!(msg.contains("Ignored"), "{msg}");
        assert!(!msg.contains("Warning:"), "{msg}");
    }

    #[test]
    fn level_display_order_and_default_target_follow_elevation_edits() {
        let dir = tmp();
        let path = dir.path().join("order.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_level(&path, "Upper", 3000.0).unwrap();
        cmd_set_level(&path, "Upper", None, Some(-3000.0)).unwrap();
        let report = report(&path).unwrap();
        assert_eq!(
            report
                .levels
                .iter()
                .map(|l| l.name.as_str())
                .collect::<Vec<_>>(),
            ["Upper", "Level 1"]
        );
        let info = cmd_info(&path).unwrap();
        assert!(info.find("Upper").unwrap() < info.find("Level 1").unwrap());
        cmd_add_wall(&path, 0.0, 0.0, 4000.0, 0.0, 120.0, None, None).unwrap();
        let doc = doc_of(&path);
        assert_eq!(doc.level(doc.walls[0].level_id).unwrap().name, "Upper");
        cmd_set_level(&path, "Upper", None, Some(0.0)).unwrap();
        let tied = super::report(&path).unwrap();
        assert!(tied.levels[0].id < tied.levels[1].id);
        cmd_set_level(&path, "Upper", Some("Level 1"), None).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(cmd_add_wall(
            &path,
            0.0,
            2000.0,
            4000.0,
            2000.0,
            120.0,
            None,
            Some("Level 1")
        )
        .is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    /// Issue 0115: `order-levels` stores a whole-list order, reorders nothing
    /// but the list, and reports idempotence on a repeat.
    #[test]
    fn order_levels_stores_the_list_and_is_idempotent() {
        let dir = tmp();
        let path = dir.path().join("ordered.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_level(&path, "Lower", -2600.0).unwrap();
        let before = std::fs::read(&path).unwrap();

        let msg = cmd_order_levels(&path, &["Level 1".to_owned(), "Lower".to_owned()]).unwrap();
        assert!(
            msg.contains("\"Level 1\"") && msg.contains("then"),
            "the message lists the recorded order, got {msg}"
        );
        // The report lists levels in the ELEVATION order; look the stored
        // fields up by name instead of by position.
        let report = report(&path).unwrap();
        let find = |name: &str| report.levels.iter().find(|l| l.name == name).unwrap();
        assert_eq!(find("Level 1").display_order, Some(0));
        assert_eq!(find("Lower").display_order, Some(1));
        // Reordering the list means the list moved; nothing else did.
        assert_eq!(find("Level 1").elevation_mm, 0.0);
        assert_eq!(find("Lower").elevation_mm, -2600.0);

        // The derived display levels still list Lower (elevation -2600)
        // before Level 1 (elevation 0) in the human info: the ordinal carries
        // the panel order, not the elevation. The idempotent repeat writes
        // no byte, and undo restores the pre-ordinal document.
        let info = cmd_info(&path).unwrap();
        assert!(
            info.find("Lower").unwrap() < info.find("Level 1").unwrap(),
            "the derived display order is still the elevation order, got {info}"
        );
        let bytes = std::fs::read(&path).unwrap();
        assert_ne!(bytes, before, "the batch was written");
        assert!(
            cmd_order_levels(&path, &["Level 1".to_owned(), "Lower".to_owned()])
                .unwrap()
                .contains("already in this order"),
            "a repeat is a no-op"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        cmd_undo(&path).unwrap();
        let doc = doc_of(&path);
        assert!(
            doc.levels.iter().all(|l| l.display_order.is_none()),
            "undo compensates the whole list: {:?}",
            doc.levels
                .iter()
                .map(|l| l.display_order)
                .collect::<Vec<_>>()
        );
    }

    /// `order-levels` refuses anything that is not exactly the current level
    /// list, addressed unambiguously — an incomplete list, an ambiguous
    /// duplicate name, a repeated one, an unknown one, an empty list —
    /// leaving the file untouched.
    #[test]
    fn order_levels_refuses_wrong_lists_and_leaves_the_file_untouched() {
        let dir = tmp();
        let path = dir.path().join("refusal.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_level(&path, "Third", 6000.0).unwrap();
        // R2.2 permits duplicate level NAMES in a document (an external
        // batch can produce them, though `cmd_add_level` refuses); such a
        // name anywhere in the requested list makes the ordering ambiguous.
        let mut twin = domowik_format::Level::new("Level 1");
        twin.elevation_mm = 3000.0;
        Plan::open(&path)
            .unwrap()
            .commit_and_save(vec![Operation::create(domowik_format::Entity::Level(twin))])
            .unwrap();
        let before = std::fs::read(&path).unwrap();

        for bad in [
            // "Third" alone resolves, but one name cannot order three levels.
            vec!["Third".to_owned()],
            // "Level 1" is ambiguous: the document holds two of them.
            vec!["Level 1".to_owned()],
            vec!["Level 1".to_owned(), "Third".to_owned()],
            // A repeated name, and an unknown one.
            vec!["Third".to_owned(), "Third".to_owned()],
            vec!["Third".to_owned(), "Nobody".to_owned()],
            // An empty list states no order at all.
            vec![],
        ] {
            let result = cmd_order_levels(&path, &bad);
            assert!(result.is_err(), "refused {bad:?}");
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "no refusal touched the file"
        );
    }

    // ---- document identity (FORMAT-SPEC §5.1 S0, issue 0116) ----

    /// §5.1 S0: `new` mints the identity once and EVERY ordinary writer carries
    /// it verbatim; `info --json` reports it. No command ever regenerates or
    /// sets it, so a reader can key per-device state (issue 0117) on it.
    #[test]
    fn document_identity_is_minted_once_and_carried_by_the_writers() {
        let dir = tmp();
        let plan = dir.path().join("identity.domo");
        cmd_new(&plan, false).expect("new");
        let minted = doc_of(&plan).document_id.expect("a new plan has identity");

        let reported: serde_json::Value =
            serde_json::from_str(&cmd_info_json(&plan).expect("json")).expect("parses");
        assert_eq!(
            reported["documentId"].as_str(),
            Some(minted.to_string().as_str()),
            "`info --json` exposes the identity: {reported}"
        );
        let human = cmd_info(&plan).expect("info");
        assert!(
            !human.contains(&minted.to_string()),
            "the machine-readable form carries script-only fields: {human}"
        );

        // A carried-through save, compaction and package-profile conversion
        // keep the SAME value.
        cmd_add_level(&plan, "Upstairs", 3000.0).expect("save");
        assert_eq!(
            doc_of(&plan).document_id,
            Some(minted),
            "an ordinary save carries the identity"
        );
        cmd_compact(&plan, execute()).expect("compact");
        assert_eq!(
            doc_of(&plan).document_id,
            Some(minted),
            "compaction carries the identity"
        );
        let packed = dir.path().join("packed.domo");
        cmd_pack(&plan, &packed, true, false, None).expect("pack");
        assert_eq!(
            read_package(&packed)
                .expect("read")
                .project()
                .expect("projects")
                .document
                .document_id,
            Some(minted),
            "the portable conversion carries the identity"
        );
    }

    /// The pre-0116 shape: a document without an identity is readable, its
    /// report OMITS the member (not a null), and no user command mints one —
    /// only a new-plan entry point does, per §5.1 S0. Consumers (issue 0117)
    /// see no identity and fall back to elevation-derived defaults.
    #[test]
    fn a_plan_without_identity_reports_none_and_no_command_mints_one() {
        let dir = tmp();
        let plan = dir.path().join("pre.domo");
        cmd_new(&plan, false).expect("new");
        let mut doc = doc_of(&plan);
        doc.document_id = None;
        write_package(&plan, &Package::new(doc)).expect("write the pre-0116 shape");

        let reported: serde_json::Value =
            serde_json::from_str(&cmd_info_json(&plan).expect("json")).expect("parses");
        assert!(
            !reported
                .as_object()
                .expect("object")
                .contains_key("documentId"),
            "the key is omitted, not null, got {reported}"
        );

        // An editing command and a whole-list reorder never mint identity.
        cmd_add_level(&plan, "Upstairs", 3000.0).expect("save");
        cmd_order_levels(&plan, &["Level 1".to_owned(), "Upstairs".to_owned()])
            .expect("store ordinals");
        assert_eq!(
            doc_of(&plan).document_id,
            None,
            "no user command mints an identity"
        );
    }

    #[test]
    fn future_walls_copy_changed_level_height_but_explicit_heights_win() {
        let dir = tmp();
        let path = dir.path().join("height.domo");
        cmd_new(&path, false).unwrap();
        cmd_add_wall(&path, 0.0, 0.0, 1000.0, 0.0, 120.0, None, None).unwrap();
        cmd_set_level_fields(
            &path,
            "Level 1",
            &domowik_edit::LevelFields {
                default_wall_height_mm: Some(3400.0),
                ..Default::default()
            },
        )
        .unwrap();
        cmd_add_wall(&path, 0.0, 2000.0, 1000.0, 2000.0, 120.0, None, None).unwrap();
        cmd_add_wall(&path, 0.0, 4000.0, 1000.0, 4000.0, 120.0, 1800.0, None).unwrap();
        let heights: Vec<_> = doc_of(&path)
            .walls
            .iter()
            .map(|wall| wall.height_mm)
            .collect();
        assert_eq!(heights, vec![2500.0, 3400.0, 1800.0]);
        cmd_add_level(&path, "Other", 3000.0).unwrap();
        cmd_set_level(&path, "Other", Some("Level 1"), None).unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(cmd_set_level(&path, "Level 1", Some("Ambiguous"), None).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn an_edit_can_be_undone() {
        let dir = tmp();
        let plan = corner_plan(&dir);
        cmd_set_wall(&plan, "1", None, None, Some(150.0), None).expect("thicken");
        assert_eq!(doc_of(&plan).walls[0].thickness_mm, 150.0);
        cmd_undo(&plan).expect("undo");
        assert_eq!(doc_of(&plan).walls[0].thickness_mm, 200.0);
    }

    #[test]
    fn undoing_a_node_move_restores_every_member() {
        // The inverse of a multi-wall edit has to restore all of it, or the
        // corner comes apart on undo and the next command fails validation.
        let dir = tmp();
        let plan = corner_plan(&dir);
        cmd_set_wall(&plan, "1", None, Some((4500.0, 500.0)), None, None).expect("move");
        cmd_undo(&plan).expect("undo");
        let doc = doc_of(&plan);
        assert_eq!(doc.walls[0].end_mm, [4000.0, 0.0]);
        assert_eq!(doc.walls[1].start_mm, [4000.0, 0.0]);
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn the_writing_actor_is_chosen_deterministically_when_watermarks_tie() {
        // Two actors at the same counter: without a tie-break, `max_by_key`
        // over a HashMap returns an arbitrary one and the same plan continues
        // a different stream on each invocation.
        let dir = tmp();
        let plan = dir.path().join("tie.domo");
        cmd_new(&plan, false).expect("new");

        // A second actor with the same watermark as the first.
        let batch_file = dir.path().join("b.json");
        let other = Uuid::now_v7();
        let batch = Batch::new(
            other,
            1,
            Default::default(),
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(&plan, &batch_file, false).expect("apply");

        let chosen: Vec<Uuid> = (0..12)
            .map(|_| Plan::open(&plan).expect("open").session_ref().actor())
            .collect();
        assert!(
            chosen.windows(2).all(|w| w[0] == w[1]),
            "actor selection must not vary between opens: {chosen:?}"
        );
    }

    // ---- machine-readable output (issue 0014) ----

    #[test]
    fn the_json_report_identifies_entities_by_id_and_by_index() {
        // Issue 0014: output "identifies paths/entity IDs". The human report
        // numbers walls by position, which is what a person can see and type
        // back; a script needs the id, which survives reordering.
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");

        let report = report(&plan).expect("report");
        assert_eq!(report.path, plan.display().to_string());
        assert!(report.valid);
        assert_eq!(report.levels.len(), 1);
        let wall = &report.levels[0].walls[0];
        assert_eq!(wall.index, 1);
        assert_eq!(
            wall.id,
            doc_of(&plan).walls[0].id,
            "the id must be the entity's, not a position"
        );
        assert_eq!(wall.openings.len(), 1);
        assert_eq!(wall.openings[0].kind, "door");
    }

    #[test]
    fn the_json_report_carries_room_geometry_a_caller_can_draw() {
        let dir = tmp();
        let plan = dir.path().join("room.domo");
        cmd_new(&plan, false).expect("new");
        for (x1, y1, x2, y2) in [
            (0.0, 0.0, 4000.0, 0.0),
            (4000.0, 0.0, 4000.0, 3000.0),
            (4000.0, 3000.0, 0.0, 3000.0),
            (0.0, 3000.0, 0.0, 0.0),
        ] {
            cmd_add_wall(&plan, x1, y1, x2, y2, 200.0, 2500.0, None).expect("wall");
        }
        let report = report(&plan).expect("report");
        let rooms = &report.levels[0].rooms;
        assert_eq!(rooms.len(), 1);
        assert_eq!(
            rooms[0].area_mm2,
            3800.0 * 2800.0,
            "inner faces, not centre lines"
        );
        assert_eq!(rooms[0].boundary_mm.len(), 4);
        assert_eq!(rooms[0].boundary_mm[0], [100.0, 100.0]);
    }

    /// Issue 0064's acceptance: what `pack` REPORTS and what the manifest
    /// DECLARES must agree for an asset-free plan, which is the one case where
    /// they used to disagree.
    ///
    /// Both halves are asserted. Checking only the message would pass against
    /// the old behaviour too, because the message always reported the
    /// requested profile — the manifest was the side that was wrong.
    #[test]
    fn pack_portable_on_an_asset_free_plan_reports_what_the_manifest_says() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let packed = dir.path().join("packed.domo");
        cmd_new(&plan, false).expect("new");

        let msg = cmd_pack(&plan, &packed, true, false, None).expect("pack --portable");
        assert!(
            msg.contains("portable package"),
            "the report must say portable: {msg}"
        );

        let manifest = read_package(&packed).expect("read").manifest;
        assert!(
            manifest.asset_root.is_empty(),
            "this plan must have an empty U(P) or the test proves nothing"
        );
        assert_eq!(
            manifest.profile,
            domowik_container::Profile::Portable,
            "the manifest must agree with the report, not contradict it"
        );

        let reported: serde_json::Value =
            serde_json::from_str(&cmd_info_json(&packed).expect("json")).expect("parses");
        assert_eq!(
            reported["profile"], "portable",
            "and `info` must agree with both"
        );
    }

    /// The other half of the empty-universe boundary: `--thin` on a plan with
    /// no assets ALSO emits portable, because §7.9 makes the profile a fact
    /// rather than a preference.
    ///
    /// This is the case that caught issue 0064's first fix. Removing the
    /// writer's emptiness guard made `--thin` on an empty U(P) emit portable
    /// while `pack` still printed the requested `thin` — the same
    /// message-contradicts-manifest defect 0064 exists to remove, pointing the
    /// other way. The report must name what was WRITTEN, and say that the flag
    /// did not survive.
    #[test]
    fn pack_thin_on_an_asset_free_plan_emits_portable_and_says_so() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let packed = dir.path().join("packed.domo");
        let cache = dir.path().join("cache");
        cmd_new(&plan, false).expect("new");

        let msg = cmd_pack(&plan, &packed, false, true, Some(&cache)).expect("pack --thin");

        let manifest = read_package(&packed).expect("read").manifest;
        assert!(manifest.asset_root.is_empty(), "U(P) must be empty");
        assert_eq!(
            manifest.profile,
            domowik_container::Profile::Portable,
            "§7.9: an empty U(P) is portable whichever flag was passed"
        );
        assert!(
            msg.contains("portable package"),
            "the report must name what was WRITTEN, not what was asked for: {msg}"
        );
        assert!(
            !msg.contains("as a thin package"),
            "reporting the request over a portable manifest is the defect: {msg}"
        );
        assert!(
            msg.contains("--thin was requested"),
            "a flag that did not survive must be said out loud: {msg}"
        );

        let reported: serde_json::Value =
            serde_json::from_str(&cmd_info_json(&packed).expect("json")).expect("parses");
        assert_eq!(reported["profile"], "portable", "`info` must agree too");
    }

    /// The non-empty behaviour issue 0064 promised not to touch: with a real
    /// blob, `--thin` still emits thin and the report still says so.
    #[test]
    fn pack_thin_with_assets_still_emits_thin() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        let packed = dir.path().join("packed.domo");
        let cache = dir.path().join("cache");
        cmd_new(&plan, false).expect("new");
        let image = dir.path().join("bg.png");
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).expect("4x4");
        pixmap.fill(tiny_skia::Color::WHITE);
        std::fs::write(&image, pixmap.encode_png().expect("png")).expect("write");
        cmd_asset_add(&plan, &image, None, 100.0).expect("add asset");

        let msg = cmd_pack(&plan, &packed, false, true, Some(&cache)).expect("pack --thin");

        let manifest = read_package(&packed).expect("read").manifest;
        assert!(
            !manifest.asset_root.is_empty(),
            "this plan must carry a blob or it tests the empty case again"
        );
        assert_eq!(
            manifest.profile,
            domowik_container::Profile::Thin,
            "a real unembedded blob keeps the package thin"
        );
        assert!(msg.contains("thin package"), "got {msg}");
        assert!(
            !msg.contains("was requested"),
            "nothing was overtaken, so nothing should be explained: {msg}"
        );
    }

    #[test]
    fn info_json_is_valid_json() {
        let dir = tmp();
        let plan = dir.path().join("plan.domo");
        cmd_new(&plan, false).expect("new");
        let out = cmd_info_json(&plan).expect("json");
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("parses");
        // §8.1 V1: the identifier is the `MAJOR.MINOR` STRING a writer states,
        // so the JSON report carries `"0.1"` and not the integer 1. A consumer
        // matching on a number is matching on a shape the spec does not define.
        assert_eq!(parsed["formatVersion"], domowik_container::EMITTED_SCHEMA);
        // §7.9 via issue 0064: a new plan has an empty U(P), which is
        // portable by vacuous truth rather than thin by a writer's guard.
        assert_eq!(parsed["profile"], "portable");
        assert_eq!(parsed["valid"], true);
    }

    #[test]
    fn validate_json_prints_the_report_and_fails_on_an_invalid_plan() {
        // A caller that asked for machine-readable output wants the problems,
        // not just an exit code — so the report is the RESULT and goes to
        // stdout, while the exit code still says the plan is invalid.
        let dir = tmp();
        let plan = dir.path().join("bad.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 120.0, 2500.0, None).expect("wall");

        let pkg = read_package(&plan).expect("read");
        let mut doc = pkg.project().expect("p").document;
        doc.walls[0].thickness_mm = -5.0;
        write_package(&plan, &Package::new(doc)).expect("write");

        let err = cmd_validate_json(&plan).expect_err("invalid");
        assert_eq!(err.code(), exit::FAILURE);
        assert!(err.is_report(), "the report belongs on stdout");
        let parsed: serde_json::Value = serde_json::from_str(err.message()).expect("parses");
        assert_eq!(parsed["valid"], false);
        assert!(
            parsed["problems"][0]
                .as_str()
                .expect("a problem")
                .contains("thicknessMm"),
            "got {parsed}"
        );
    }

    #[test]
    fn the_json_report_surfaces_stale_discards_the_human_report_hides() {
        // FORMAT-SPEC I5 makes a stale discard a projection outcome rather
        // than an error, so it is silent to a user. A script that cares must
        // still be able to see it.
        let dir = tmp();
        let plan = dir.path().join("stale.domo");
        cmd_new(&plan, false).expect("new");
        let batch_file = dir.path().join("b.json");

        // An external batch editing an entity that does not exist.
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(),
            vec![set_field(Uuid::now_v7(), "name", serde_json::json!("x"))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(&plan, &batch_file, false).expect("apply");

        let report = report(&plan).expect("report");
        assert_eq!(report.discarded.len(), 1, "{:?}", report.discarded);
        assert_eq!(report.discarded[0].reason, "Stale(BeforeCreation)");
        assert!(report.valid, "a stale discard is not a validity problem");
    }

    // ---- R3.1.2 splitting (fixtures F4, F5) ----

    #[test]
    fn fixture_f4_an_endpoint_on_another_walls_interior_splits_it() {
        // F4: Wb.Start lands on Wa's interior → split Wa at (3000,0). The
        // child containing the ORIGINAL start keeps the original wall id
        // (R3.1.3); one join absorbs the touching endpoint and both children's
        // adjacent endpoints (R3.1.2 row 2).
        let dir = tmp();
        let plan = dir.path().join("tee.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        let original = doc_of(&plan).walls[0].id;

        let msg =
            cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 100.0, 2100.0, None).expect("wb");
        assert!(msg.contains("Split 1 wall"), "got {msg}");

        let doc = doc_of(&plan);
        assert_eq!(doc.walls.len(), 3, "Wa split in two, plus Wb");
        assert_eq!(
            doc.walls[0].id, original,
            "R3.1.3: the start child keeps the id"
        );
        assert_eq!(doc.walls[0].end_mm, [3000.0, 0.0]);
        assert_eq!(doc.joins.len(), 1, "ONE join at the split point");
        assert_eq!(doc.joins[0].members.len(), 3, "{:?}", doc.joins[0].members);
        assert_eq!(doc.validate(), vec![]);

        // The children inherit the parent's properties (R3.1.3).
        let new_child = doc
            .walls
            .iter()
            .find(|w| w.start_mm == [3000.0, 0.0] && w.end_mm == [6000.0, 0.0])
            .expect("the far child");
        assert_eq!(new_child.thickness_mm, 200.0);
        assert_eq!(new_child.height_mm, 2500.0);
        assert_eq!(new_child.level_id, doc.levels[0].id);
    }

    #[test]
    fn fixture_f5_a_proper_crossing_splits_both_walls() {
        // F5: interior x interior at (2000,0) → split BOTH, one explicit
        // multi-member endpoint join.
        let dir = tmp();
        let plan = dir.path().join("x.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("w1");
        let msg =
            cmd_add_wall(&plan, 2000.0, -2000.0, 2000.0, 2000.0, 200.0, 2500.0, None).expect("w2");
        assert!(msg.contains("Split 2 wall"), "got {msg}");

        let doc = doc_of(&plan);
        assert_eq!(doc.walls.len(), 4);
        assert_eq!(doc.joins.len(), 1, "ONE join, not two");
        assert_eq!(
            doc.joins[0].members.len(),
            4,
            "all four children meet there: {:?}",
            doc.joins[0].members
        );
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn a_split_is_one_batch_and_therefore_one_undo() {
        // R3.1.1: an accepted creation is exactly ONE atomic batch including
        // its incidental split and join effects.
        let dir = tmp();
        let plan = dir.path().join("tee.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        let before = read_package(&plan).expect("read").batches.len();

        cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None).expect("wb");
        assert_eq!(
            read_package(&plan).expect("read").batches.len(),
            before + 1,
            "one batch"
        );

        cmd_undo(&plan).expect("undo");
        let doc = doc_of(&plan);
        assert_eq!(doc.walls.len(), 1, "the split is undone too: {doc:#?}");
        assert_eq!(
            doc.walls[0].end_mm,
            [6000.0, 0.0],
            "the parent is whole again"
        );
        assert!(doc.joins.is_empty());
        assert_eq!(doc.validate(), vec![]);
    }

    #[test]
    fn a_split_keeps_the_far_join_attached_to_the_wall_that_reaches_it() {
        // R3.1.4: a membership on the original End role follows the original
        // end endpoint into the FINAL child's End. Without the remap the join
        // would still name the parent, whose End is now the split point.
        let dir = tmp();
        let plan = dir.path().join("remap.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 6000.0, 0.0, 6000.0, 3000.0, 200.0, 2500.0, None).expect("b");
        assert_eq!(doc_of(&plan).joins.len(), 1, "a corner join at (6000,0)");

        cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None).expect("stem");
        let doc = doc_of(&plan);
        assert_eq!(doc.validate(), vec![], "coordinate agreement must hold");

        let far = doc
            .joins
            .iter()
            .find(|j| {
                j.members.iter().any(|m| {
                    doc.wall(m.wall_id).is_some_and(|w| {
                        w.end_mm == [6000.0, 3000.0] || w.start_mm == [6000.0, 3000.0]
                    })
                })
            })
            .expect("the far join survives");
        for m in &far.members {
            let w = doc.wall(m.wall_id).expect("member exists");
            let p = match m.role {
                Role::Start => w.start_mm,
                Role::End => w.end_mm,
            };
            assert_eq!(p, [6000.0, 0.0], "every member must be AT the node");
        }
    }

    /// R4.3.1: reversing a host must carry the field map, or every door
    /// silently teleports to its mirror position and points the wrong way
    /// while the document still validates.
    #[test]
    fn reversing_a_host_preserves_world_intent() {
        let dir = tmp();
        let plan = dir.path().join("reverse.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.25)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");

        let before = doc_of(&plan);
        let wall_before = before.walls[0].clone();
        let o_before = before.openings[0].clone();
        // The door's centre in WORLD terms, which is what must not move.
        let world_before =
            wall_before.start().x + o_before.u * (wall_before.end().x - wall_before.start().x);

        cmd_reverse_wall(&plan, "1").expect("reverse");

        let after = doc_of(&plan);
        let wall_after = &after.walls[0];
        let o_after = &after.openings[0];

        assert_eq!(wall_after.start(), wall_before.end(), "endpoints swapped");
        assert_eq!(wall_after.end(), wall_before.start());
        assert_eq!(o_after.id, o_before.id, "identity is untouched");

        let world_after =
            wall_after.start().x + o_after.u * (wall_after.end().x - wall_after.start().x);
        assert!(
            (world_after - world_before).abs() < 1e-9,
            "the door moved: {world_before} -> {world_after}"
        );

        assert_eq!(o_after.side, Side::Right, "side flips (was Left)");
        assert_eq!(o_after.opens_toward, Side::Right, "opensToward flips");
        assert_eq!(o_after.hinge, Hinge::NearEnd, "hinge flips");
        assert_eq!(o_after.swing_deg, o_before.swing_deg, "swing is untouched");
        assert_eq!(o_after.width_mm, o_before.width_mm, "dimensions untouched");
        assert!(after.validate().is_empty(), "{:?}", after.validate());
    }

    /// Reversing a JOINED wall. The previous tests used a lone wall and so
    /// missed that a membership names an axis END: without swapping the roles
    /// the join's members disagree about where the node is and R3.3.4 rejects
    /// the whole command.
    /// R3.3.7 through the CLI: `detach` deletes the two-member join, saves,
    /// and leaves both endpoints in place; an unjoined endpoint is refused.
    #[test]
    fn detach_removes_a_membership_and_saves() {
        let dir = tmp();
        let plan = dir.path().join("detach.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("w1");
        cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("w2");
        assert_eq!(doc_of(&plan).joins.len(), 1);
        let msg = cmd_detach(&plan, "2", false).expect("detaches");
        assert!(msg.contains("deleted"), "{msg}");
        let after = doc_of(&plan);
        assert!(after.joins.is_empty());
        assert!(after.validate().is_empty(), "{:?}", after.validate());
        assert_eq!(
            after.walls[1].start(),
            Point::new(4000.0, 0.0),
            "the endpoint stays"
        );
        assert!(
            cmd_detach(&plan, "2", false).is_err(),
            "nothing left to detach"
        );
    }

    #[test]
    fn reversing_a_joined_wall_keeps_its_joins_intact() {
        let dir = tmp();
        let plan = dir.path().join("joined.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("w1");
        cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("w2");
        let before = doc_of(&plan);
        assert_eq!(before.joins.len(), 1, "the two walls meet at a corner");

        cmd_reverse_wall(&plan, "1").expect("reversing a joined wall must work");

        let after = doc_of(&plan);
        assert!(after.validate().is_empty(), "{:?}", after.validate());
        assert_eq!(after.joins.len(), 1, "the join survives");

        // The node has not moved; only the role naming it has changed.
        let wall = &after.walls[0];
        let member = after.joins[0]
            .members
            .iter()
            .find(|m| m.wall_id == wall.id)
            .expect("still a member");
        let at = match member.role {
            Role::Start => wall.start(),
            Role::End => wall.end(),
        };
        assert_eq!(at, Point::new(4000.0, 0.0), "the node stayed put");
    }

    /// Reversing twice is the identity, which is the cheapest check that the    /// Reversing twice is the identity, which is the cheapest check that the
    /// map is a true involution rather than four independent flips that
    /// happen to look right once.
    #[test]
    fn reversing_twice_restores_the_original() {
        let dir = tmp();
        let plan = dir.path().join("twice.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Window,
            Some(Placement::Fraction(0.3)),
            1200.0,
            1400.0,
            900.0,
            Orientation::default(),
        )
        .expect("window");
        let before = doc_of(&plan);

        cmd_reverse_wall(&plan, "1").expect("once");
        cmd_reverse_wall(&plan, "1").expect("twice");

        let after = doc_of(&plan);
        assert_eq!(after.walls[0].start(), before.walls[0].start());
        assert_eq!(after.walls[0].end(), before.walls[0].end());
        let (a, b) = (&after.openings[0], &before.openings[0]);
        assert!((a.u - b.u).abs() < 1e-12, "u drifted: {} vs {}", a.u, b.u);
        assert_eq!(a.side, b.side);
        assert_eq!(a.hinge, b.hinge);
        assert_eq!(a.opens_toward, b.opens_toward);
    }

    /// R4.3.3: an opening whose aperture is strictly inside ONE child moves
    /// to that child, keeping its identity, with `u` recomputed against the
    /// child's own span.
    #[test]
    fn a_split_remaps_an_opening_to_the_child_that_contains_it() {
        let dir = tmp();
        let plan = dir.path().join("hosted.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        // Aperture spans 750..1650 mm along a 6000 mm wall.
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.2)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let opening_id = doc_of(&plan).openings[0].id;

        // Split at 3000 mm, well clear of the aperture.
        cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None).expect("split");

        let doc = doc_of(&plan);
        assert_eq!(doc.openings.len(), 1, "identity never changes (R4.3.3)");
        let o = &doc.openings[0];
        assert_eq!(o.id, opening_id, "the opening keeps its id");

        let host = doc.wall(o.host_wall_id).expect("host exists");
        assert!(
            (host.length_mm() - 3000.0).abs() < 1e-6,
            "it belongs to the 0..3000 child, got a {} mm host",
            host.length_mm()
        );
        // 1200 mm along a 3000 mm child is u = 0.4.
        assert!((o.u - 0.4).abs() < 1e-9, "u remapped to {}", o.u);
        assert!(doc.validate().is_empty(), "{:?}", doc.validate());
    }

    /// R4.3.3: an aperture that straddles the split point has no strictly
    /// containing child, so the ENTIRE batch rejects — including the wall
    /// creation that caused the split — and the file is untouched.
    #[test]
    fn a_split_through_an_aperture_rejects_the_whole_batch() {
        let dir = tmp();
        let plan = dir.path().join("straddle.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        // Centred at 3000 mm, so the aperture spans 2550..3450.
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let before = doc_of(&plan);

        let err = cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None)
            .expect_err("must refuse");
        assert!(
            err.message().contains("opening-split-ambiguous"),
            "got {}",
            err.message()
        );
        assert_eq!(doc_of(&plan), before, "nothing may change");
    }

    /// Zero clearance is a rejection, not a coin toss: R4.3.3 requires STRICT
    /// containment, so an aperture edge landing exactly on the split point has
    /// no valid host.
    #[test]
    fn an_aperture_touching_the_split_point_rejects() {
        let dir = tmp();
        let plan = dir.path().join("touching.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 6000.0, 0.0, 200.0, 2500.0, None).expect("wa");
        // Centred at 2550 with width 900 puts the far edge exactly at 3000.
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.425)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        let before = doc_of(&plan);

        let err = cmd_add_wall(&plan, 3000.0, 0.0, 3000.0, 2500.0, 200.0, 2500.0, None)
            .expect_err("must refuse");
        assert!(
            err.message().contains("opening-split-ambiguous"),
            "got {}",
            err.message()
        );
        assert_eq!(doc_of(&plan), before, "nothing may change");
    }

    #[test]
    fn a_wall_that_merely_touches_an_endpoint_joins_without_splitting() {
        // R3.1.2 row 1: endpoint-endpoint auto-joins, NO split. The split
        // classifier must not fire on a shared endpoint.
        let dir = tmp();
        let plan = dir.path().join("corner.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("a");
        let msg = cmd_add_wall(&plan, 4000.0, 0.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("b");
        assert!(!msg.contains("Split"), "got {msg}");
        assert_eq!(doc_of(&plan).walls.len(), 2);
    }

    #[test]
    fn several_walls_meeting_one_point_split_a_new_wall_there_only_once() {
        // The case the deduplication exists for. Two crossing walls become
        // four children that all END at (2000,0); a new wall through that
        // point then meets FOUR endpoints on its own interior, each of which
        // asks for a split at the same place. Without dedup the new wall is
        // cut four times at one point, producing three zero-length walls —
        // which R3.0.5 rejects, so the whole command fails.
        let dir = tmp();
        let plan = dir.path().join("star.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 2000.0, -1000.0, 2000.0, 1000.0, 200.0, 2500.0, None).expect("a");
        cmd_add_wall(&plan, 1000.0, -1000.0, 3000.0, 1000.0, 200.0, 2500.0, None).expect("b");
        assert_eq!(doc_of(&plan).walls.len(), 4, "a and b split each other");

        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("n");
        let doc = doc_of(&plan);
        assert_eq!(doc.walls.len(), 6, "the new wall is cut ONCE: {doc:#?}");
        assert!(
            doc.walls.iter().all(|w| w.length_mm() > 0.0),
            "a repeated cut would make zero-length walls"
        );
        assert_eq!(doc.validate(), vec![]);
        assert_eq!(doc.joins.len(), 1, "one node, one join");
        assert_eq!(doc.joins[0].members.len(), 6, "{:?}", doc.joins[0].members);
    }

    #[test]
    fn a_wall_crossing_two_others_splits_all_three() {
        // Two independent crossings on one new wall: each splits the crossed
        // wall AND the new one, giving three cuts and three joins.
        let dir = tmp();
        let plan = dir.path().join("ladder.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 1000.0, 4000.0, 1000.0, 200.0, 2500.0, None).expect("r1");
        cmd_add_wall(&plan, 0.0, 3000.0, 4000.0, 3000.0, 200.0, 2500.0, None).expect("r2");
        cmd_add_wall(&plan, 2000.0, 0.0, 2000.0, 4000.0, 200.0, 2500.0, None).expect("post");

        let doc = doc_of(&plan);
        // Two rails split in two each, and the post split in three.
        assert_eq!(doc.walls.len(), 2 * 2 + 3, "got {} walls", doc.walls.len());
        assert_eq!(doc.joins.len(), 2, "one join per crossing");
        for join in &doc.joins {
            assert_eq!(join.members.len(), 4, "{:?}", join.members);
        }
        assert_eq!(doc.validate(), vec![]);
    }

    // ---- pack (ASSET-STRATEGY §10.4) ----

    fn plan_with_asset(dir: &tempfile::TempDir) -> (std::path::PathBuf, String) {
        let plan = dir.path().join("p.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall");
        let image = dir.path().join("bg.png");
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).expect("4x4");
        pixmap.fill(tiny_skia::Color::WHITE);
        std::fs::write(&image, pixmap.encode_png().expect("png")).expect("write");
        cmd_asset_add(&plan, &image, None, 100.0).expect("add");
        let digest = read_package(&plan).expect("read").manifest.asset_digests()[0].clone();
        (plan, digest)
    }

    #[test]
    fn asset_add_and_remove_still_surface_a_pre_existing_wall_overlap_warning() {
        // An accepted asset batch is a document mutation like any other:
        // R3.1.7's warnings attach to it too, even though neither command
        // touches the wall that causes the overlap.
        let dir = tmp();
        let plan = dir.path().join("p.domo");
        cmd_new(&plan, false).expect("new");
        cmd_add_wall(&plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None).expect("wall 1");
        cmd_add_wall(&plan, 0.0, 50.0, 4000.0, 50.0, 200.0, 2500.0, None)
            .expect("wall 2, overlapping wall 1");

        let image = dir.path().join("bg.png");
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).expect("4x4");
        pixmap.fill(tiny_skia::Color::WHITE);
        std::fs::write(&image, pixmap.encode_png().expect("png")).expect("write");

        let msg = cmd_asset_add(&plan, &image, None, 100.0).expect("add");
        assert!(
            msg.contains("wall-area-overlap"),
            "asset add must still surface the pre-existing overlap, got {msg}"
        );

        let digest = read_package(&plan).expect("read").manifest.asset_digests()[0].clone();
        let msg = cmd_asset_remove(&plan, &digest).expect("remove");
        assert!(
            msg.contains("wall-area-overlap"),
            "asset remove must still surface the pre-existing overlap, got {msg}"
        );
    }

    #[test]
    fn pack_portable_embeds_and_verifies_every_referenced_asset() {
        let dir = tmp();
        let (plan, digest) = plan_with_asset(&dir);
        let out = dir.path().join("portable.domo");

        let msg = cmd_pack(&plan, &out, true, false, None).expect("pack");
        assert!(msg.contains("1 referenced asset(s)"), "got {msg}");
        assert!(msg.contains("all embedded and verified"), "got {msg}");
        // The three classes are reported whether or not a cache was named.
        assert!(msg.contains("package 86 byte(s)"), "got {msg}");
        assert!(msg.contains("remote 0 byte(s) declared"), "got {msg}");

        let packed = read_package(&out).expect("read");
        assert!(packed.assets.contains_key(&digest));
        assert_eq!(packed.manifest.asset_digests(), vec![digest]);
        // The plan itself survives the conversion unchanged.
        assert_eq!(
            packed.project().expect("p").document,
            read_package(&plan)
                .expect("read")
                .project()
                .expect("p")
                .document
        );
    }

    #[test]
    fn pack_portable_aborts_with_a_complete_omission_report() {
        // §10.4: any unverifiable reference aborts packing completely, with a
        // report naming EVERY such asset. Stopping at the first makes a user
        // fix one thing, run again, and find another.
        let dir = tmp();
        let (plan, _) = plan_with_asset(&dir);

        // Two references with no blobs behind them.
        let mut pkg = read_package(&plan).expect("read");
        let ghosts = [
            format!("sha256:{}", "1".repeat(64)),
            format!("sha256:{}", "2".repeat(64)),
        ];
        pkg.manifest.set_asset_digests(
            pkg.manifest
                .asset_digests()
                .into_iter()
                .chain(ghosts.iter().cloned()),
        );
        write_package(&plan, &pkg).expect("write");

        let out = dir.path().join("portable.domo");
        let err = cmd_pack(&plan, &out, true, false, None).expect_err("must abort");
        let message = err.message();
        for ghost in &ghosts {
            assert!(
                message.contains(ghost),
                "every omission must be named: {message}"
            );
        }
        assert!(message.contains("2 of 3"), "got {message}");
        assert!(!out.exists(), "nothing may be written");
    }

    #[test]
    fn pack_thin_without_a_cache_is_refused_rather_than_approximated() {
        // A thin package keeps its blobs in a cache or behind a hint. Written
        // with nowhere to put them, its references dangle by construction —
        // so the refusal is now about the missing cache, not about the
        // profile being unimplemented.
        let dir = tmp();
        let (plan, _) = plan_with_asset(&dir);
        let out = dir.path().join("thin.domo");
        let err = cmd_pack(&plan, &out, false, true, None).expect_err("must refuse");
        assert_eq!(err.code(), exit::USAGE);
        assert!(err.message().contains("dangle"), "got {}", err.message());
        assert!(!out.exists());
    }

    #[test]
    fn pack_thin_moves_blobs_into_the_cache_and_pins_them() {
        // §10.4: thin embeds no member of U(P). The blobs have to survive
        // somewhere, and a pin is what stops the next collection taking the
        // very blobs the new package depends on.
        let dir = tmp();
        let (plan, digest) = plan_with_asset(&dir);
        let cache = dir.path().join("cache");
        let out = dir.path().join("thin.domo");

        let msg = cmd_pack(&plan, &out, false, true, Some(&cache)).expect("thin");
        assert!(msg.contains("thin package"), "{msg}");
        assert!(msg.contains("moved into the cache and pinned"), "{msg}");

        // The package no longer embeds it...
        let written = domowik_container::read_package(&out).expect("read back");
        assert!(written.assets.is_empty(), "a thin package embedded a blob");
        // ...but still enumerates it, and the cache holds it.
        assert!(written.manifest.asset_digests().contains(&digest));
        let store = CacheStore::open(&cache, AdmissionLimits::default()).expect("cache");
        assert!(store.contains(&digest));
        assert_eq!(store.verify(&digest).expect("verify"), Some(true));

        // And a collection does not take it, because packing pinned it.
        let report = cmd_cache_gc(&cache, false).expect("gc");
        assert!(report.contains("0 blob(s); 1 retained"), "{report}");
        assert!(store.contains(&digest));
    }

    #[test]
    fn pack_portable_can_rebuild_from_a_thin_package_and_its_cache() {
        // The round trip §10.4 describes: thin out, portable back, with the
        // blob travelling through the cache as a byte copy (§10.1).
        let dir = tmp();
        let (plan, digest) = plan_with_asset(&dir);
        let cache = dir.path().join("cache");
        let thin = dir.path().join("thin.domo");
        let portable = dir.path().join("portable.domo");

        cmd_pack(&plan, &thin, false, true, Some(&cache)).expect("thin");
        let msg = cmd_pack(&thin, &portable, true, false, Some(&cache)).expect("portable");
        assert!(msg.contains("portable package"), "{msg}");

        let rebuilt = domowik_container::read_package(&portable).expect("read back");
        let original = domowik_container::read_package(&plan).expect("read original");
        assert_eq!(
            rebuilt.assets.get(&digest),
            original.assets.get(&digest),
            "the round trip changed the bytes"
        );
    }

    #[test]
    fn pack_without_a_profile_says_which_flag_to_pass() {
        let dir = tmp();
        let (plan, _) = plan_with_asset(&dir);
        let out = dir.path().join("out.domo");
        let err = cmd_pack(&plan, &out, false, false, None).expect_err("no profile");
        assert_eq!(err.code(), exit::USAGE);
        let both = cmd_pack(&plan, &out, true, true, None).expect_err("both profiles");
        assert_eq!(both.code(), exit::USAGE);
        assert!(!out.exists());
    }

    #[test]
    fn pack_carries_blobs_nothing_understood_references() {
        // §6 requires garbage collection to be fail-closed. A blob referenced
        // only by data this version cannot read is indistinguishable from an
        // orphan, so packing must not drop it.
        let dir = tmp();
        let (plan, _) = plan_with_asset(&dir);
        let mut pkg = read_package(&plan).expect("read");
        let extra = b"referenced by something from the future".to_vec();
        let extra_digest = domowik_container::digest_of(&extra);
        pkg.assets.insert(extra_digest.clone(), extra);
        write_package(&plan, &pkg).expect("write");

        let out = dir.path().join("portable.domo");
        let msg = cmd_pack(&plan, &out, true, false, None).expect("pack");
        assert!(msg.contains("carried for references"), "got {msg}");
        assert!(read_package(&out)
            .expect("read")
            .assets
            .contains_key(&extra_digest));
    }

    // ---- compaction (FORMAT-SPEC §5.1, issue 0027) ----

    /// An ordinary local compaction: locality asserted, one actor, so no
    /// per-actor accounting is needed.
    fn execute() -> CompactOptions {
        CompactOptions {
            assert_local_only: true,
            ..CompactOptions::default()
        }
    }

    /// A dry run needs no assertion; it writes nothing.
    fn dry_run() -> CompactOptions {
        CompactOptions {
            dry_run: true,
            ..CompactOptions::default()
        }
    }

    fn plan_with_history(dir: &tempfile::TempDir) -> std::path::PathBuf {
        let plan = dir.path().join("h.domo");
        cmd_new(&plan, false).expect("new");
        for (x1, y1, x2, y2) in [
            (0.0, 0.0, 4000.0, 0.0),
            (4000.0, 0.0, 4000.0, 3000.0),
            (4000.0, 3000.0, 0.0, 3000.0),
            (0.0, 3000.0, 0.0, 0.0),
        ] {
            cmd_add_wall(&plan, x1, y1, x2, y2, 200.0, 2500.0, None).expect("wall");
        }
        cmd_add_opening(
            &plan,
            "1",
            OpeningKind::Door,
            Some(Placement::Fraction(0.5)),
            900.0,
            2100.0,
            0.0,
            Orientation::default(),
        )
        .expect("door");
        cmd_remove_opening(&plan, "1").expect("delete it again, to make a tombstone");
        plan
    }

    #[test]
    fn compaction_preserves_the_projection_exactly() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let before = doc_of(&plan);
        assert!(read_package(&plan).expect("read").batches.len() > 1);

        cmd_compact(&plan, execute()).expect("compact");
        assert_eq!(doc_of(&plan), before, "the document must not move");
        assert!(
            read_package(&plan).expect("read").batches.is_empty(),
            "the log is folded into the snapshot"
        );
    }

    #[test]
    fn compaction_retains_tombstones_so_a_delete_stays_deleted() {
        // FORMAT-SPEC I3. A compacted delete leaves no operation behind, so
        // without S4's tombstones a later create finds nothing blocking it and
        // resurrects what the user deleted.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let deleted = read_package(&plan)
            .expect("read")
            .project()
            .expect("p")
            .intervals
            .iter()
            .find(|(_, i)| i.deleted_at.is_some())
            .map(|(id, _)| *id)
            .expect("the opening was deleted");

        cmd_compact(&plan, execute()).expect("compact");
        let pkg = read_package(&plan).expect("read");
        assert!(pkg.tombstones.contains(&deleted));

        // A batch that DOES observe the snapshot, so the dominance guard lets
        // it through and the tombstone is what must stop it.
        let batch_file = dir.path().join("revive.json");
        let opening = Opening::door(Uuid::now_v7(), 0.5, 900.0, 2100.0, 200.0);
        let mut revived = opening.clone();
        revived.id = deleted;
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            pkg.frontier.clone(),
            vec![Operation::create(Entity::Opening(revived))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(&plan, &batch_file, false).expect("delivered");

        let projection = read_package(&plan).expect("read").project().expect("p");
        assert!(
            projection.document.openings.is_empty(),
            "the deleted opening must stay deleted"
        );
        assert_eq!(projection.discarded.len(), 1);
        assert_eq!(
            projection.discarded[0].reason,
            domowik_document::replay::DiscardReason::Stale(
                domowik_document::replay::TargetState::AfterDeletion
            ),
            "refused as a tombstone, not merely as an unknown id"
        );
    }

    #[test]
    fn a_session_opened_on_a_compacted_plan_sees_the_same_document_as_the_package() {
        // `Package::project` always applied the retained tombstones (§5.1 S4);
        // `Session::open_snapshot` did not take them at all. A compacted
        // delete leaves no operation behind, so the live session resurrected
        // what the file did not contain, and the two views of one plan
        // disagreed until the next reopen. I3 forbids the resurrection and §5
        // makes the snapshot authoritative for both. Found by project review,
        // 2026-09-01, when validating external batches started rejecting a
        // document only the session could build.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact");

        let pkg = read_package(&plan).expect("read");
        assert!(
            !pkg.tombstones.is_empty(),
            "the fixture must retain a tombstone"
        );
        let deleted = pkg.tombstones[0];

        // The two views agree on open.
        let opened = Plan::open(&plan).expect("open");
        assert_eq!(opened.document(), &pkg.project().expect("p").document);

        // And the session refuses to resurrect the tombstoned id, rather than
        // showing an entity the file does not have.
        let mut opened = opened;
        let mut revived = Opening::door(Uuid::now_v7(), 0.5, 900.0, 2100.0, 200.0);
        revived.id = deleted;
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            pkg.frontier.clone(),
            vec![Operation::create(Entity::Opening(revived))],
        );
        opened
            .session()
            .expect("editable")
            .accept_external(batch)
            .expect("delivered");
        assert!(
            opened.document().openings.is_empty(),
            "the session resurrected a tombstoned entity"
        );
    }

    #[test]
    fn a_batch_concurrent_with_the_snapshot_is_refused_not_guessed_at() {
        // The guard that makes §5.1 S3's winner positions unnecessary. A batch
        // that does not observe the snapshot cannot be placed relative to the
        // operations it absorbed, and applying it anyway would silently
        // produce a different document than a replica that never compacted.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact");
        let before = doc_of(&plan);

        let batch_file = dir.path().join("concurrent.json");
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            Default::default(), // observes nothing
            vec![Operation::create(Entity::Level(
                domowik_format::Level::new("Loft"),
            ))],
        );
        std::fs::write(&batch_file, serde_json::to_vec(&batch).unwrap()).expect("write");

        let err = cmd_apply(&plan, &batch_file, false).expect_err("must refuse");
        assert!(
            err.message()
                .contains("concurrent with this plan's snapshot"),
            "got {}",
            err.message()
        );
        assert_eq!(doc_of(&plan), before, "nothing may change");
    }

    #[test]
    fn editing_continues_normally_after_a_compaction() {
        // The case that actually matters: one person, one plan, keeps working.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact");

        cmd_add_wall(&plan, 2000.0, 0.0, 2000.0, 3000.0, 200.0, 2500.0, None).expect("wall");
        let doc = doc_of(&plan);
        assert_eq!(doc.validate(), vec![]);
        assert!(doc.walls.len() > 4, "the split produced children");

        // And the new work is undoable, even though the compacted history is
        // not.
        cmd_undo(&plan).expect("undo");
        assert_eq!(doc_of(&plan).walls.len(), 4);
    }

    #[test]
    fn a_batch_from_before_a_compaction_is_still_recognised_as_a_duplicate() {
        // What the retained frontier is FOR. §5.1 S2 calls it "coverage record
        // AND the per-actor duplicate watermark for I2". Without it a
        // re-delivered pre-compaction batch is no longer recognised as already
        // applied, and its operations replay on top of the snapshot — applying
        // the same edit twice.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let earlier = read_package(&plan)
            .expect("read")
            .batches
            .last()
            .cloned()
            .expect("history exists");
        let batch_file = dir.path().join("earlier.json");
        std::fs::write(&batch_file, serde_json::to_vec(&earlier).unwrap()).expect("write");

        cmd_compact(&plan, execute()).expect("compact");
        let after_compaction = doc_of(&plan);

        let msg = cmd_apply(&plan, &batch_file, false).expect("delivered");
        assert!(msg.contains("already applied"), "got {msg}");
        assert_eq!(
            doc_of(&plan),
            after_compaction,
            "a duplicate must not replay onto the snapshot"
        );
    }

    #[test]
    fn compaction_refuses_to_write_a_plan_that_projects_differently() {
        // The guard exists for a defect in COMPACTION, not in the plan, so
        // nothing a caller can put in a package exercises it. What this shows
        // is the premise the guard rests on: a fold that lost the levels DOES
        // project a different document, so the comparison the guard makes is
        // capable of catching one. It does NOT call `cmd_compact`, and saying
        // it did would be claiming a witness that does not exist.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let pkg = read_package(&plan).expect("read");

        let mut broken = pkg.compacted().expect("compacts");
        broken.snapshot.levels.clear(); // a compaction that lost the levels
        assert_ne!(
            broken.project().expect("p").document,
            pkg.project().expect("p").document,
            "the fixture must actually differ, or this proves nothing"
        );
    }

    #[test]
    fn a_dry_run_compaction_states_the_retention_effects_and_writes_nothing() {
        // Issue 0027's third criterion: "undo/recovery retention effects are
        // explicit BEFORE execution". Reporting them in the success message is
        // after the fact, and compaction is irreversible.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let before = std::fs::read(&plan).expect("read plan");
        let batches = read_package(&plan).expect("read").batches.len();

        let msg = cmd_compact(&plan, dry_run()).expect("dry run");
        assert!(msg.contains("Would compact"), "got {msg}");
        assert!(msg.contains("DISCARDED"), "got {msg}");
        assert!(msg.contains("tombstone"), "got {msg}");
        assert!(msg.contains("undo"), "got {msg}");
        assert!(msg.contains("--dry-run"), "got {msg}");

        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "--dry-run wrote to the plan"
        );
        assert_eq!(
            read_package(&plan).expect("read").batches.len(),
            batches,
            "the log is still there"
        );

        // And the numbers it reported are the ones the real run then reports.
        let real = cmd_compact(&plan, execute()).expect("compact");
        let count = |m: &str| {
            m.split_whitespace()
                .filter_map(|w| w.parse::<usize>().ok())
                .collect::<Vec<_>>()
        };
        assert_eq!(count(&msg), count(&real), "{msg} vs {real}");
    }

    #[test]
    fn compacting_an_already_compact_plan_is_a_no_op() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("first");
        let msg = cmd_compact(&plan, execute()).expect("second");
        assert!(msg.contains("already compact"), "got {msg}");
    }

    #[test]
    fn compaction_keeps_the_assets() {
        // §7.4 invariant I2 does not stop applying because the log was folded.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let image = dir.path().join("bg.png");
        let mut pixmap = tiny_skia::Pixmap::new(4, 4).expect("4x4");
        pixmap.fill(tiny_skia::Color::WHITE);
        std::fs::write(&image, pixmap.encode_png().expect("png")).expect("write");
        cmd_asset_add(&plan, &image, None, 100.0).expect("add");

        cmd_compact(&plan, execute()).expect("compact");
        let pkg = read_package(&plan).expect("read");
        assert_eq!(pkg.assets.len(), 1, "the blob survives");
        assert_eq!(
            pkg.manifest.asset_root.len(),
            1,
            "and so does its root entry"
        );
        assert_eq!(
            doc_of(&plan).levels[0].background_images.len(),
            1,
            "and the reference"
        );
    }

    /// Delivers a batch written by a SECOND actor, which is what makes a
    /// package's retained history multi-actor. `apply` is the only way a v0.1
    /// package acquires one, and §5.1 S6's refusal has to be reachable to be
    /// testable.
    fn apply_foreign_batch(dir: &tempfile::TempDir, plan: &std::path::Path) -> Uuid {
        let pkg = read_package(plan).expect("read");
        let frontier = pkg.log().expect("log").frontier();
        let level = pkg.project().expect("p").document.levels[0].id;
        let actor = Uuid::now_v7();
        let wall = Wall::new(
            level,
            Point::new(0.0, 9000.0),
            Point::new(4000.0, 9000.0),
            200.0,
            2500.0,
        );
        let batch = Batch::new(
            actor,
            1,
            frontier,
            vec![Operation::create(Entity::Wall(wall))],
        );
        let file = dir.path().join("foreign.json");
        std::fs::write(&file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(plan, &file, false).expect("delivered");
        actor
    }

    #[test]
    fn a_routine_local_plan_never_acquires_a_second_actor() {
        // The evidence behind the multi-actor gate, because a gate that fired
        // on ordinary local editing would be a bug rather than a safeguard.
        // `Plan::open` CONTINUES the highest-watermark actor rather than
        // minting one per invocation, so every command below writes as the
        // same B2 epoch — through edits, an undo, a compaction, and the edits
        // after it.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_undo(&plan).expect("undo");
        cmd_add_wall(&plan, 0.0, 5000.0, 1000.0, 5000.0, 200.0, 2500.0, None).expect("wall");
        assert_eq!(actors_of(&plan).len(), 1, "before compaction");

        cmd_compact(&plan, execute()).expect("compact");
        cmd_add_wall(&plan, 0.0, 6000.0, 1000.0, 6000.0, 200.0, 2500.0, None).expect("wall");
        cmd_add_wall(&plan, 0.0, 7000.0, 1000.0, 7000.0, 200.0, 2500.0, None).expect("wall");
        assert_eq!(actors_of(&plan).len(), 1, "after compaction and more edits");

        // So the ordinary local run needs no per-actor accounting at all.
        cmd_compact(&plan, execute()).expect("compact again");
    }

    fn actors_of(plan: &std::path::Path) -> Vec<Uuid> {
        let mut actors: Vec<Uuid> = read_package(plan)
            .expect("read")
            .log()
            .expect("log")
            .watermark()
            .keys()
            .copied()
            .collect();
        actors.sort();
        actors
    }

    #[test]
    fn a_multi_actor_history_is_reported_and_compacts_on_the_assertion() {
        // B2 makes an actor a WRITER EPOCH, not a peer: "one user may operate
        // several actors/devices". A hand-applied batch produces a second one
        // on a file that never left the machine, so an actor count does not
        // measure what §5.1 S6 is about — undelivered batches held by peers —
        // and refusing on it would reject a legitimate local plan. The actors
        // are reported; the assertion is the gate.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let foreign = apply_foreign_batch(&dir, &plan);
        let expected = doc_of(&plan);
        assert_eq!(actors_of(&plan).len(), 2, "the fixture is multi-actor");

        let msg = cmd_compact(&plan, execute()).expect("the assertion is the only gate");
        assert_eq!(doc_of(&plan), expected, "the document must not move");
        assert!(read_package(&plan).expect("read").batches.is_empty());
        // Non-blocking, and after the fact: it says what was folded.
        assert!(msg.contains("2 writer epochs"), "got {msg}");
        assert!(msg.contains(&foreign.to_string()), "got {msg}");
    }

    #[test]
    fn the_actors_are_reported_before_an_irreversible_fold() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let foreign = apply_foreign_batch(&dir, &plan);

        let body = cmd_compact(
            &plan,
            CompactOptions {
                dry_run: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("dry run");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["singleActorHistory"], false, "{body}");
        let listed: Vec<String> = v["actors"]
            .as_array()
            .expect("actors")
            .iter()
            .map(|x| x.as_str().expect("an id").to_owned())
            .collect();
        assert!(listed.contains(&foreign.to_string()), "{body}");
        // Informational only: nothing here is a refusal.
        assert_eq!(v["phase"], "preflight");
        assert!(v["refusal"].is_null(), "{body}");
    }

    #[test]
    fn execution_requires_the_local_only_assertion_but_a_dry_run_does_not() {
        // One actor in the history is NECESSARY but not sufficient: a copy
        // given to a peer that has not written yet leaves no trace in the
        // package. The assertion records who answered for that.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let before = std::fs::read(&plan).expect("read plan");

        let err = cmd_compact(&plan, CompactOptions::default()).expect_err("must refuse");
        // S6's three parts, in the TEXT form: the log is retained, the
        // condition that is unestablished, and the range that was kept. The
        // range was missing from this message until an adversarial round
        // pointed out that only the JSON form carried it.
        let msg = err.message();
        assert!(msg.contains("--assert-local-only"), "{msg}");
        assert!(msg.contains("RETAINED, not dropped"), "{msg}");
        assert!(msg.contains("§5.1 S6"), "{msg}");
        assert!(msg.contains("Retained range:"), "{msg}");
        let actor = actors_of(&plan)[0].to_string();
        assert!(msg.contains(&actor), "the range names the actor: {msg}");
        assert_eq!(std::fs::read(&plan).expect("read plan"), before);

        // The dry run needs no assertion: it writes nothing.
        cmd_compact(&plan, dry_run()).expect("dry run");
        assert_eq!(std::fs::read(&plan).expect("read plan"), before);
    }

    #[test]
    fn the_locality_refusal_is_machine_readable_and_still_fails() {
        // S6 requires the diagnostic to be part of the operation's observable
        // RESULT rather than a debug line, so a caller reading JSON gets it —
        // and still gets a non-zero exit, because the log was not dropped.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let actor = actors_of(&plan)[0];

        let err = cmd_compact(
            &plan,
            CompactOptions {
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect_err("must refuse");
        let body = match &err {
            CliError::Report(body) => body.clone(),
            other => panic!("expected a machine-readable report, got {other:?}"),
        };
        assert_eq!(err.code(), exit::FAILURE, "a refusal is not a success");

        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["phase"], "refused");
        assert_eq!(v["written"], false);
        assert_eq!(v["foldedBatches"], 0);
        assert_eq!(v["refusal"]["log"], "retained");
        assert!(
            v["refusal"]["unestablishedCondition"]
                .as_str()
                .expect("a condition")
                .contains("§5.1 S6"),
            "{body}"
        );
        let range = &v["refusal"]["retainedRange"][actor.to_string()];
        assert!(range["from"].as_u64().expect("from") >= 1, "{body}");
        assert!(
            range["through"].as_u64().expect("through") >= range["from"].as_u64().unwrap(),
            "{body}"
        );
        assert!(
            !read_package(&plan).expect("read").batches.is_empty(),
            "the log is retained"
        );
    }

    #[test]
    fn the_compaction_plan_is_machine_readable_before_the_write() {
        // Criterion 3 in the form a script can consume: the retention effects,
        // the §5.1 S2 coverage the fold would record, and the §7.6 entries it
        // would discard, all BEFORE anything is written.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let before = std::fs::read(&plan).expect("read plan");
        let batches = read_package(&plan).expect("read").batches.len();

        let body = cmd_compact(
            &plan,
            CompactOptions {
                dry_run: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("dry run");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["phase"], "preflight");
        assert_eq!(v["written"], false);
        assert_eq!(v["currentBatches"], batches);
        assert_eq!(v["foldedBatches"], batches);
        assert_eq!(
            v["foldedEntries"].as_array().expect("entries").len(),
            batches,
            "{body}"
        );
        assert!(
            v["foldedEntries"][0]
                .as_str()
                .expect("an entry")
                .starts_with("ops/"),
            "{body}"
        );
        assert!(
            !v["foldedThroughFrontier"]
                .as_object()
                .expect("a frontier")
                .is_empty(),
            "{body}"
        );
        assert!(
            !v["retainedTombstones"]
                .as_array()
                .expect("tombstones")
                .is_empty(),
            "the fixture deletes an opening: {body}"
        );
        assert!(
            v["undoRedoEffect"]
                .as_str()
                .expect("undo")
                .contains("R8.4.3"),
            "{body}"
        );
        assert!(
            v["recoveryEffect"]
                .as_str()
                .expect("recovery")
                .contains("§9.7"),
            "{body}"
        );
        assert_eq!(std::fs::read(&plan).expect("read plan"), before);

        // And the plan it reported is what the write then records.
        let written = cmd_compact(
            &plan,
            CompactOptions {
                assert_local_only: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("compact");
        let w: serde_json::Value = serde_json::from_str(&written).expect("valid JSON");
        assert_eq!(w["phase"], "written");
        assert_eq!(w["written"], true);
        assert_eq!(w["foldedEntries"], v["foldedEntries"]);
        assert_eq!(w["foldedThroughFrontier"], v["foldedThroughFrontier"]);
        assert_eq!(w["retainedTombstones"], v["retainedTombstones"]);
    }

    #[test]
    fn a_dry_run_writes_nothing_even_with_the_assertion_given() {
        // The gates are ordered, and an ordering bug is the plausible defect:
        // a dry run that fell through to the write because the assertion was
        // present would be an irreversible fold nobody asked for.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let before = std::fs::read(&plan).expect("read plan");

        let msg = cmd_compact(
            &plan,
            CompactOptions {
                dry_run: true,
                assert_local_only: true,
                ..CompactOptions::default()
            },
        )
        .expect("dry run");
        assert!(msg.contains("Would compact"), "got {msg}");
        assert_eq!(std::fs::read(&plan).expect("read plan"), before);
        assert!(!read_package(&plan).expect("read").batches.is_empty());
    }

    #[test]
    fn the_defect_refusal_names_the_retained_state_and_the_reason() {
        // §5.2 S7: a compaction that cannot proceed at all names the retained
        // state AND the blocking reason. The branch it guards is NOT reachable
        // without a defect in `Package::compacted`, so this checks the
        // diagnostic rather than pretending to reach the branch — the
        // alternative was a test that would pass whatever the message said.
        let ranges = BTreeMap::from([(
            Uuid::nil(),
            RetainedRange {
                from: 1,
                through: 7,
            },
        )]);
        let msg = projection_moved_message(std::path::Path::new("p.domo"), 4, 7, &ranges);
        assert!(msg.contains("projects a different"), "the reason: {msg}");
        assert!(msg.contains("RETAINED, not dropped"), "the state: {msg}");
        assert!(msg.contains("1..=7"), "and its extent: {msg}");
        assert!(msg.contains("Nothing was written"), "{msg}");
    }

    #[test]
    fn the_folded_entries_are_the_7_6_names_of_the_batches_that_go() {
        // C5 truthfulness: `foldedEntries` claims to name the §7.6 entries the
        // fold discards. Re-derived here from each batch's own counterBase and
        // actor rather than by calling the helper the report calls, so a
        // padding or field mix-up shows up instead of agreeing with itself.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let pkg = read_package(&plan).expect("read");
        let expected: std::collections::BTreeSet<String> = pkg
            .batches
            .iter()
            .map(|b| format!("ops/{:019}-{}.json", b.counter_base, b.actor_id))
            .collect();

        let body = cmd_compact(
            &plan,
            CompactOptions {
                dry_run: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("dry run");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let listed: std::collections::BTreeSet<String> = v["foldedEntries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e.as_str().expect("a name").to_owned())
            .collect();
        assert_eq!(listed, expected, "{body}");

        cmd_compact(&plan, execute()).expect("compact");
        assert!(
            read_package(&plan).expect("read").batches.is_empty(),
            "and the entries it named are the ones that went"
        );
    }

    #[test]
    fn an_already_compact_plan_still_reports_what_it_retains() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact");

        let body = cmd_compact(
            &plan,
            CompactOptions {
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("a no-op is not a failure");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["phase"], "already-compact");
        assert_eq!(v["written"], false);
        assert_eq!(v["foldedBatches"], 0);
        assert!(
            !v["retainedTombstones"]
                .as_array()
                .expect("tombstones")
                .is_empty(),
            "the tombstones it keeps are still reported: {body}"
        );
    }

    #[test]
    fn compaction_records_the_coverage_the_snapshot_absorbed() {
        // Criterion 2's second half. The rewritten manifest must carry the
        // §5.1 S2 frontier of everything it folded; without it the reader
        // cannot tell an already-applied batch from one whose predecessors are
        // missing, and holds the latter forever.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let watermark: std::collections::BTreeMap<Uuid, u64> = read_package(&plan)
            .expect("read")
            .log()
            .expect("log")
            .watermark()
            .iter()
            .map(|(&a, &c)| (a, c))
            .collect();
        assert!(!watermark.is_empty(), "the fixture has written something");

        cmd_compact(&plan, execute()).expect("compact");
        let pkg = read_package(&plan).expect("read");
        assert_eq!(
            pkg.frontier, watermark,
            "the snapshot must record exactly what it absorbed"
        );
    }

    #[test]
    fn an_interrupted_compaction_leaves_the_old_package_or_the_new_one() {
        // Issue 0027's fifth criterion. Compaction writes through the §9.7
        // replacement path, so this reproduces the FILESYSTEM STATE each
        // injection point leaves and checks the plan still opens as one whole
        // package — never a mixture, and never something the reader refuses.
        //
        // Reproducing the state rather than killing a process mid-write is the
        // same deliberate limit the container's injected-save suite records:
        // it tests the recovery contract, not the kernel's ordering.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let expected = doc_of(&plan);
        let old_bytes = std::fs::read(&plan).expect("read plan");

        // The bytes a completed compaction produces, taken from a copy so the
        // original is still the pre-compaction package.
        let copy = dir.path().join("copy.domo");
        std::fs::write(&copy, &old_bytes).expect("copy");
        cmd_compact(&copy, execute()).expect("compact the copy");
        let new_bytes = std::fs::read(&copy).expect("read copy");
        assert_ne!(old_bytes, new_bytes, "compaction changed the bytes");

        // F2: the candidate was written but the rename never happened.
        std::fs::write(dir.path().join(".h.domo.999.0.tmp"), &new_bytes).expect("candidate");
        let pkg = read_package(&plan).expect("the old plan must still open");
        assert_eq!(pkg.project().expect("p").document, expected);
        assert!(!pkg.batches.is_empty(), "the old package still has its log");

        // F5: the rename completed. The plan is the new package, and the
        // document it projects is the same one.
        std::fs::write(&plan, &new_bytes).expect("replace");
        let pkg = read_package(&plan).expect("the new plan must open");
        assert_eq!(pkg.project().expect("p").document, expected);
        assert!(pkg.batches.is_empty(), "the new package is compacted");

        // Either outcome is editable, which is what "readable" has to mean.
        cmd_add_wall(&plan, 0.0, 7000.0, 1000.0, 7000.0, 200.0, 2500.0, None).expect("edit");
    }

    #[test]
    #[cfg(unix)]
    fn a_compaction_that_cannot_write_leaves_the_old_plan_untouched() {
        // The other side of the fifth criterion, and the one a fabricated
        // filesystem state cannot prove: when the write itself fails, the
        // plan must still be the OLD package rather than a truncated one.
        // §9.7 S0 puts the candidate in the target's directory, so a
        // directory that cannot be written to fails the save at its first
        // step, with the target never opened for writing.
        use std::os::unix::fs::PermissionsExt;

        let dir = tmp();
        let plan = plan_with_history(&dir);
        let expected = doc_of(&plan);
        let before = std::fs::read(&plan).expect("read plan");

        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("make the directory read-only");
        let result = cmd_compact(&plan, execute());
        std::fs::set_permissions(dir.path(), mode).expect("restore");

        let err = result.expect_err("the write cannot succeed in a read-only directory");
        // §5.2 S7: a compaction that cannot proceed names the retained state
        // AND the blocking reason. It named only the reason until adversarial
        // review pointed out that a bare I/O error says nothing about what
        // survived — and, under --json, was not even JSON.
        let msg = err.message();
        assert!(msg.contains("could not be written"), "the reason: {msg}");
        assert!(msg.contains("RETAINED, not dropped"), "the state: {msg}");
        assert!(msg.contains("retained range"), "and its extent: {msg}");
        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before,
            "the old package must be byte-identical"
        );
        let pkg = read_package(&plan).expect("the old plan must still open");
        assert_eq!(pkg.project().expect("p").document, expected);
        assert!(!pkg.batches.is_empty(), "with its log");
    }

    #[test]
    #[cfg(unix)]
    fn a_write_failure_is_reported_as_json_when_json_was_asked_for() {
        // The other half of the same finding: a caller that asked for a
        // machine-readable result got a bare error string.
        use std::os::unix::fs::PermissionsExt;

        let dir = tmp();
        let plan = plan_with_history(&dir);
        let mode = std::fs::metadata(dir.path())
            .expect("metadata")
            .permissions();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
            .expect("read-only");
        let result = cmd_compact(
            &plan,
            CompactOptions {
                assert_local_only: true,
                json: true,
                ..CompactOptions::default()
            },
        );
        std::fs::set_permissions(dir.path(), mode).expect("restore");

        let err = result.expect_err("the write cannot succeed");
        let body = match &err {
            CliError::Report(body) => body.clone(),
            other => panic!("expected a machine-readable report, got {other:?}"),
        };
        assert_eq!(err.code(), exit::FAILURE);
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["phase"], "refused");
        assert_eq!(v["written"], false);
        assert_eq!(v["refusal"]["log"], "retained");
        assert!(
            v["refusal"]["unestablishedCondition"]
                .as_str()
                .expect("a condition")
                .contains("could not be written"),
            "{body}"
        );
        assert!(
            !v["refusal"]["retainedRange"]
                .as_object()
                .expect("a range")
                .is_empty(),
            "{body}"
        );
    }

    #[test]
    fn the_refusal_does_not_depend_on_the_package_projecting() {
        // The gate runs BEFORE the projection. A package that parses but
        // cannot project would otherwise answer a compaction request with a
        // projection error instead of the §5.1 S6 refusal it owes — and the
        // refusal needs nothing but the batch headers to state.
        //
        // The witness is the ORDER, since a package that fails to project is
        // not constructible from outside: a fold that is impossible stands in
        // for it. `compacted()` is never reached, because the refusal returns
        // first, and this checks the refusal is what comes back.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let err = cmd_compact(&plan, CompactOptions::default()).expect_err("must refuse");
        assert!(err.message().contains("RETAINED, not dropped"), "{err:?}");

        // And the refused plan does NOT describe a fold it never computed.
        let body = match cmd_compact(
            &plan,
            CompactOptions {
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect_err("must refuse")
        {
            CliError::Report(body) => body,
            other => panic!("expected a report, got {other:?}"),
        };
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["foldedBatches"], 0, "{body}");
        assert!(
            v["foldedEntries"].as_array().expect("entries").is_empty(),
            "a refusal must not claim entries it did not enumerate: {body}"
        );
    }

    #[test]
    fn the_folded_entries_include_one_the_snapshot_already_covers() {
        // Compaction discards the whole `ops/` namespace, but the listing was
        // built from CANONICAL ORDER, which omits a batch the snapshot already
        // covers (§4.5 I2). So an entry that was about to be deleted did not
        // appear in the list of entries about to be deleted. Found by
        // adversarial review, against a fixture this branch had already built
        // for the `ops` side of the same gap.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let original = read_package(&plan).expect("read");
        let covered = original.batches[0].clone();
        cmd_compact(&plan, execute()).expect("compact");

        let mut pkg = read_package(&plan).expect("read");
        pkg.batches.push(covered.clone());
        write_package(&plan, &pkg).expect("a package may hold one");

        let body = cmd_compact(
            &plan,
            CompactOptions {
                dry_run: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect("dry run");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        let expected = format!("ops/{:019}-{}.json", covered.counter_base, covered.actor_id);
        let listed: Vec<&str> = v["foldedEntries"]
            .as_array()
            .expect("entries")
            .iter()
            .map(|e| e.as_str().expect("a name"))
            .collect();
        assert_eq!(v["currentBatches"], 1, "{body}");
        assert_eq!(listed, vec![expected.as_str()], "{body}");

        // And it really does go.
        cmd_compact(&plan, execute()).expect("compact");
        assert!(read_package(&plan).expect("read").batches.is_empty());
    }

    // ---- §9.6 degraded compaction (R15 audit, R26 fix) ----

    /// Gives `plan` a background image whose blob is absent, declared at
    /// `requirement`. Returns its digest.
    ///
    /// The body NAMES ITS OWN DIGEST, and that one field is load-bearing.
    /// `Package::requirement_of` reads a descriptor's requirement level only
    /// after checking that the descriptor agrees it describes those bytes:
    /// one filed under a digest while naming another says "optional" about
    /// different content, and trusting it would open a document editable on
    /// the strength of a description of something else. A body without
    /// `digest` therefore reads as `Undeclared`, which BLOCKS — so the
    /// optional control, whose whole point is that optional does not block,
    /// would fail for a reason it is not about.
    ///
    /// The remaining §10.2 fields are not read on this path and are not
    /// needed; the digest is 64 lowercase hex because that is what a real one
    /// looks like, not because anything here checks its shape.
    fn add_missing_background(plan: &std::path::Path, requirement: &str) -> String {
        let pkg = read_package(plan).expect("read");
        let hex = match requirement {
            "required" => "a",
            "optional" => "b",
            other => panic!("§10.2 declares required or optional, not {other:?}"),
        };
        let digest = format!("sha256:{}", hex.repeat(64));
        let mut degraded = pkg.clone();
        let level = degraded.snapshot.levels[0].id;
        let lvl = degraded
            .snapshot
            .levels
            .iter_mut()
            .find(|l| l.id == level)
            .expect("level");
        lvl.background_images.push(domowik_format::BackgroundImage {
            digest: digest.clone(),
            anchor: Default::default(),
            stack_ordinal: 0,
            unknown: Default::default(),
        });
        degraded.descriptors.insert(
            digest.clone(),
            serde_json::json!({ "digest": digest, "requirement": requirement }),
        );
        write_package(plan, &degraded).expect("the package itself is writable");
        digest
    }

    #[test]
    fn a_required_missing_asset_refuses_compaction_even_when_locality_is_asserted() {
        // BEHAVIOR-SPEC §9.6: a required asset this device cannot resolve puts
        // the package in read-only degraded mode, where "interactive editing
        // and normal saving are unavailable". A compaction is a normal save
        // AND it discards the log irreversibly. `cmd_compact` never builds a
        // `Plan`, so it never reached `Plan::save`'s gate and folded such a
        // package happily — found by audit, not by a failing test.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let digest = add_missing_background(&plan, "required");

        let before_bytes = std::fs::read(&plan).expect("read plan");
        let before_mtime = std::fs::metadata(&plan)
            .expect("meta")
            .modified()
            .expect("mtime");
        let before_batches = read_package(&plan).expect("read").batches.len();
        assert!(before_batches > 0, "there must be a log to lose");

        // The assertion does not clear it: locality is the caller's to answer,
        // and an unresolvable required asset is not.
        let err = cmd_compact(&plan, execute()).expect_err("must refuse");
        let msg = err.message();
        assert!(msg.contains("read-only degraded"), "{msg}");
        assert!(msg.contains("§9.6"), "{msg}");
        assert!(msg.contains(&digest), "names the asset: {msg}");
        assert!(msg.contains("Required"), "and its requirement level: {msg}");
        assert!(
            msg.contains("Restore the blob"),
            "and the remediation: {msg}"
        );
        // S7's retained state, on this path as on every other refusal.
        assert!(msg.contains("RETAINED, not dropped"), "{msg}");
        assert!(msg.contains("retained range"), "{msg}");

        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before_bytes,
            "a refusal must not rewrite the package"
        );
        assert_eq!(
            std::fs::metadata(&plan)
                .expect("meta")
                .modified()
                .expect("mtime"),
            before_mtime,
            "and must not touch it at all"
        );
        assert_eq!(
            read_package(&plan).expect("read").batches.len(),
            before_batches,
            "the log is retained"
        );
    }

    #[test]
    fn the_degraded_refusal_is_machine_readable() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let digest = add_missing_background(&plan, "required");

        let err = cmd_compact(
            &plan,
            CompactOptions {
                assert_local_only: true,
                json: true,
                ..CompactOptions::default()
            },
        )
        .expect_err("must refuse");
        let body = match &err {
            CliError::Report(body) => body.clone(),
            other => panic!("expected a machine-readable report, got {other:?}"),
        };
        assert_eq!(err.code(), exit::FAILURE);
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["phase"], "refused");
        assert_eq!(v["written"], false);
        assert_eq!(v["foldedBatches"], 0);
        assert_eq!(v["refusal"]["log"], "retained");
        let condition = v["refusal"]["unestablishedCondition"]
            .as_str()
            .expect("a condition");
        assert!(condition.contains("read-only degraded"), "{body}");
        assert!(condition.contains(&digest), "{body}");
        assert!(
            !v["refusal"]["retainedRange"]
                .as_object()
                .expect("a range")
                .is_empty(),
            "{body}"
        );
    }

    #[test]
    fn a_degraded_package_still_inspects_and_dry_runs() {
        // §9.6 preserves inspection in terms: "pan, zoom, viewing, and
        // diagnostics work, and CLI `info` and `validate` operate". A gate that
        // also closed `ops` would be a different rule than the one written.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        add_missing_background(&plan, "required");
        let before = std::fs::read(&plan).expect("read plan");

        cmd_ops(&plan, false).expect("inspection stays available");
        cmd_ops(&plan, true).expect("and so does replay inspection");
        cmd_ops_json(&plan, false).expect("in both output forms");
        let dry = cmd_compact(&plan, dry_run()).expect("a dry run writes nothing");
        assert!(dry.contains("Would compact"), "got {dry}");

        assert_eq!(std::fs::read(&plan).expect("read plan"), before);
    }

    #[test]
    fn an_optional_missing_asset_does_not_block_compaction() {
        // The positive control, and the reason the gate reads the CENTRAL
        // matrix rather than testing for "an absent blob": §9.6's optional row
        // opens editable with placeholder rendering, so compaction proceeds.
        // A gate that merely looked for missing bytes would fail this.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        add_missing_background(&plan, "optional");
        let expected = doc_of(&plan);

        let msg = cmd_compact(&plan, execute()).expect("optional does not block");
        assert!(msg.contains("folded into the snapshot"), "got {msg}");
        assert!(read_package(&plan).expect("read").batches.is_empty());
        assert_eq!(doc_of(&plan), expected, "the document must not move");
    }

    #[test]
    fn an_already_compact_degraded_package_is_still_a_no_op_not_a_refusal() {
        // Nothing is written, so §9.6 has nothing to forbid. Refusing here
        // would report a problem the caller cannot act on and does not have.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact while still editable");
        add_missing_background(&plan, "required");

        let msg = cmd_compact(&plan, execute()).expect("a no-op is not a save");
        assert!(msg.contains("already compact"), "got {msg}");
    }

    // ---- `ops` reporting (issue 0027 criterion 1) ----

    #[test]
    fn ops_json_reports_batches_actors_counters_and_the_frontier() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let body = cmd_ops_json(&plan, false).expect("ops --json");
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");

        let batches = read_package(&plan).expect("read").batches.len();
        assert_eq!(v["batches"], batches);
        assert!(v["operations"].as_u64().expect("operations") >= batches as u64);
        assert_eq!(
            v["actors"].as_array().expect("actors").len(),
            1,
            "one writer edited this plan: {body}"
        );
        let actor = v["actors"][0].as_str().expect("an actor id");
        assert!(
            v["watermark"][actor].as_u64().expect("a watermark") > 0,
            "{body}"
        );

        let order = v["canonicalOrder"].as_array().expect("canonical order");
        assert_eq!(order.len(), batches);
        let first = &order[0];
        assert!(
            first["entry"].as_str().expect("entry").starts_with("ops/"),
            "the §7.6 entry a script can look up: {body}"
        );
        assert_eq!(
            first["actorId"],
            serde_json::Value::String(actor.to_owned())
        );
        assert!(
            first["counterThrough"].as_u64().expect("through")
                >= first["counterFrom"].as_u64().expect("from"),
            "{body}"
        );
        // L33's range has no IEEE-754 representation, so this is a string.
        assert!(
            first["causalPastSize"].is_string(),
            "causalPastSize must not be a JSON number: {body}"
        );
        assert!(
            first["operations"][0]["kind"].is_string(),
            "operations are described by kind: {body}"
        );

        // Canonical order, not file order: the positions are non-decreasing.
        let keys: Vec<(u128, u64)> = order
            .iter()
            .map(|b| {
                (
                    b["causalPastSize"].as_str().unwrap().parse().unwrap(),
                    b["counterBase"].as_u64().unwrap(),
                )
            })
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "{body}");
    }

    #[test]
    fn ops_accounts_for_a_retained_entry_the_snapshot_already_covers() {
        // §4.5 I2 says a batch at or below the snapshot frontier is already
        // applied and is ignored SILENTLY, so such an entry is counted in
        // `batches` and absent from `canonicalOrder`. Without saying so the
        // two numbers disagree with nothing to explain them, and a reader
        // would reasonably suspect the listing of dropping a batch.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let original = read_package(&plan).expect("read");
        let covered = original.batches[0].clone();
        cmd_compact(&plan, execute()).expect("compact");

        // The compacted package, plus one entry the new frontier covers.
        let mut pkg = read_package(&plan).expect("read");
        assert!(pkg.batches.is_empty(), "compaction emptied the log");
        assert!(
            pkg.frontier[&covered.actor_id] >= covered.range_end(),
            "the frontier must cover it, or this tests nothing"
        );
        pkg.batches.push(covered);
        write_package(&plan, &pkg).expect("a package may hold one");

        let report = ops_report(&plan, false).expect("ops");
        assert_eq!(report.batches, 1);
        assert!(report.canonical_order.is_empty(), "I2 ignores it");
        assert_eq!(report.covered_by_snapshot, 1);
        assert!(
            cmd_ops(&plan, false)
                .expect("ops")
                .contains("the snapshot already covers"),
            "the human form says so too"
        );
    }

    #[test]
    fn ops_reports_the_tombstones_a_compaction_retained() {
        // Criterion 1 names tombstones, and a compacted package is exactly
        // where they matter: the deletes have no operations left, so this
        // listing is the only account of what the plan still refuses to
        // resurrect.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        cmd_compact(&plan, execute()).expect("compact");
        cmd_add_wall(&plan, 0.0, 6000.0, 1000.0, 6000.0, 200.0, 2500.0, None).expect("edit");

        let retained = read_package(&plan).expect("read").tombstones;
        assert!(!retained.is_empty(), "the fixture deleted an opening");

        let text = cmd_ops(&plan, false).expect("ops");
        assert!(text.contains("retained tombstone"), "got {text}");
        for id in &retained {
            assert!(text.contains(&id.to_string()), "got {text}");
        }

        let v: serde_json::Value =
            serde_json::from_str(&cmd_ops_json(&plan, false).expect("json")).expect("valid JSON");
        let listed: Vec<String> = v["retainedTombstones"]
            .as_array()
            .expect("tombstones")
            .iter()
            .map(|x| x.as_str().expect("an id").to_owned())
            .collect();
        assert_eq!(
            listed,
            retained.iter().map(|i| i.to_string()).collect::<Vec<_>>()
        );
        // And the snapshot coverage is visible in both forms.
        assert!(text.contains("snapshot covers"), "got {text}");
        assert!(!v["snapshotFrontier"]
            .as_object()
            .expect("a frontier")
            .is_empty());
    }

    #[test]
    fn ops_reports_a_delete_the_retained_log_still_carries() {
        // The other half of "tombstones": before a compaction the delete is
        // still an operation, and reporting only the manifest's list would
        // show nothing at all for the ordinary case.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        assert!(
            read_package(&plan).expect("read").tombstones.is_empty(),
            "nothing is compacted yet"
        );
        // Absent without --replay, because only applying the log can know it.
        let quiet: serde_json::Value =
            serde_json::from_str(&cmd_ops_json(&plan, false).expect("json")).expect("valid JSON");
        assert_eq!(quiet["replayed"], false);
        assert!(quiet["logTombstones"].is_null(), "{quiet}");

        let v: serde_json::Value =
            serde_json::from_str(&cmd_ops_json(&plan, true).expect("json")).expect("valid JSON");
        assert_eq!(v["replayed"], true);
        assert!(
            !v["logTombstones"]
                .as_array()
                .expect("log tombstones")
                .is_empty(),
            "the fixture deleted an opening"
        );
        assert!(
            cmd_ops(&plan, true)
                .expect("ops")
                .contains("deleted by the retained log"),
            "the human form says so too"
        );
    }

    /// A batch that is structurally valid and application-invalid: the framing
    /// is conforming, and the payload cannot produce a valid entity.
    ///
    /// ED-0023's validation gate asks for exactly this shape — data that
    /// inspects successfully while the replaying path rejects it — and ED-0107
    /// keeps the obligation while narrowing what "inspect" may do.
    fn deliver_application_invalid_batch(dir: &tempfile::TempDir, plan: &std::path::Path) -> Uuid {
        let pkg = read_package(plan).expect("read");
        let wall = pkg.project().expect("p").document.walls[0].id;
        let mut fields = serde_json::Map::new();
        fields.insert("thicknessMm".to_owned(), serde_json::json!("not a number"));
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            pkg.log().expect("log").frontier(),
            vec![Operation::set_fields(wall, fields)],
        );
        let file = dir.path().join("application-invalid.json");
        std::fs::write(&file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(plan, &file, false).expect("structurally valid, so it is delivered");
        wall
    }

    #[test]
    fn application_invalid_data_inspects_but_replay_rejects_it() {
        // ED-0023's differential witness, kept under ED-0107. The SAME package
        // inspects cleanly on the default path and is rejected by the
        // replaying one — which is what makes "inspection does not execute" an
        // observable difference rather than a claim about call graphs.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let wall = deliver_application_invalid_batch(&dir, &plan);

        // Inspects: the batch and its operation are reported, by kind and by
        // field NAME, with no verdict on whether the payload would apply.
        let quiet = ops_report(&plan, false).expect("inspection succeeds");
        assert!(!quiet.replayed);
        assert!(quiet.discarded.is_none(), "no verdict without replay");
        assert!(
            quiet
                .canonical_order
                .iter()
                .flat_map(|b| &b.operations)
                .any(|o| o.entity_id == wall && o.fields.iter().any(|f| f == "thicknessMm")),
            "the operation is reported"
        );

        // Replays: the same operation is REJECTED as a malformed payload.
        let loud = ops_report(&plan, true).expect("replay reports rather than errors");
        assert!(loud.replayed);
        let discarded = loud.discarded.expect("a verdict");
        assert!(
            discarded
                .iter()
                .any(|d| d.entity_id == wall && d.reason.contains("MalformedPayload")),
            "replay must reject it: {discarded:?}"
        );

        // And the rejection did not reach the document either way.
        let doc = doc_of(&plan);
        assert!(
            doc.walls.iter().any(|w| w.id == wall),
            "the wall is untouched"
        );
    }

    #[test]
    fn default_path_never_applies_the_log() {
        // ED-0107 permits typed inert operations and private ordering metadata
        // on the default path, and forbids entity projection there. This pins
        // the boundary from the outside: every default-path output — text and
        // JSON — is produced for a package whose replay would discard an
        // operation, and NONE of them carries replay's verdict.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        deliver_application_invalid_batch(&dir, &plan);

        for body in [
            cmd_ops(&plan, false).expect("text"),
            cmd_ops_json(&plan, false).expect("json"),
        ] {
            assert!(
                !body.contains("MalformedPayload"),
                "a replay verdict reached a default path: {body}"
            );
            assert!(
                !body.contains("discarded as stale"),
                "a replay verdict reached a default path: {body}"
            );
        }
        let v: serde_json::Value =
            serde_json::from_str(&cmd_ops_json(&plan, false).expect("json")).expect("valid JSON");
        assert_eq!(v["replayed"], false);
        assert!(v["discarded"].is_null());
        assert!(v["logTombstones"].is_null());

        // The opt-in path is the only one that carries it.
        assert!(cmd_ops_json(&plan, true)
            .expect("json")
            .contains("MalformedPayload"));
    }

    #[test]
    fn ops_reads_without_writing_or_acting_on_what_it_reads() {
        // "Without executing untrusted content" (criterion 1). The log is
        // input, not instructions: `ops` reports a malformed payload as a
        // discarded operation, leaves the document alone, and does not touch a
        // byte of the file in either output form.
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let pkg = read_package(&plan).expect("read");
        let wall = pkg.project().expect("p").document.walls[0].id;
        let mut fields = serde_json::Map::new();
        fields.insert("thicknessMm".to_owned(), serde_json::json!("not a number"));
        let batch = Batch::new(
            Uuid::now_v7(),
            1,
            pkg.log().expect("log").frontier(),
            vec![Operation::set_fields(wall, fields)],
        );
        let file = dir.path().join("hostile.json");
        std::fs::write(&file, serde_json::to_vec(&batch).unwrap()).expect("write");
        cmd_apply(&plan, &file, false).expect("delivered");

        let before_doc = doc_of(&plan);
        let before_bytes = std::fs::read(&plan).expect("read plan");

        // A read-only DIRECTORY is what makes this discriminate. The §9.7
        // write path creates a candidate beside the target and renames over
        // it, so any write at all fails here — where a byte comparison alone
        // would pass, a rewrite of the same package being byte-identical.
        #[cfg(unix)]
        let restore = {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path())
                .expect("metadata")
                .permissions();
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))
                .expect("read-only");
            mode
        };
        let text = cmd_ops(&plan, false).expect("ops");
        let body = cmd_ops_json(&plan, false).expect("ops --json");
        let replayed_text = cmd_ops(&plan, true).expect("ops --replay");
        let replayed_body = cmd_ops_json(&plan, true).expect("ops --json --replay");
        #[cfg(unix)]
        std::fs::set_permissions(dir.path(), restore).expect("restore");

        assert_eq!(
            std::fs::read(&plan).expect("read plan"),
            before_bytes,
            "ops wrote to the plan"
        );
        assert_eq!(doc_of(&plan), before_doc, "ops changed the document");

        // The default does not apply the log at all (ED-0023: reusing the
        // replaying path "would violate issue 0027 AC1"), so it reports the
        // batch that carries the malformed payload without ever handing that
        // payload to replay. It says so rather than printing an empty section.
        let v: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
        assert_eq!(v["replayed"], false);
        assert!(v["discarded"].is_null(), "{body}");
        assert!(text.contains("the log was not applied"), "got {text}");
        assert!(!text.contains("discarded as stale"), "got {text}");

        // --replay is the opt-in, and THEN the stale discard is reported.
        assert!(
            replayed_text.contains("discarded as stale"),
            "got {replayed_text}"
        );
        let r: serde_json::Value = serde_json::from_str(&replayed_body).expect("valid JSON");
        assert_eq!(r["replayed"], true);
        assert!(
            !r["discarded"].as_array().expect("discarded").is_empty(),
            "{replayed_body}"
        );
        // The field NAME is reported; the value it carried is not echoed.
        assert!(body.contains("thicknessMm"), "{body}");
        assert!(!body.contains("not a number"), "{body}");
    }

    #[test]
    fn ops_and_ops_json_describe_the_same_log() {
        let dir = tmp();
        let plan = plan_with_history(&dir);
        let text = cmd_ops(&plan, false).expect("ops");
        let v: serde_json::Value =
            serde_json::from_str(&cmd_ops_json(&plan, false).expect("json")).expect("valid JSON");
        for batch in v["canonicalOrder"].as_array().expect("order") {
            assert!(
                text.contains(batch["summary"].as_str().expect("a summary")),
                "the human form omits {}: {text}",
                batch["summary"]
            );
        }
        assert!(
            text.contains(&format!("{} batch(es)", v["batches"])),
            "{text}"
        );
    }
}
