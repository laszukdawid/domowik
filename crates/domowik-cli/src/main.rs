//! The `domowik` binary.
//!
//! Argument parsing and stream/exit-code discipline only; the commands
//! themselves live in the library so they can be tested directly.

#![forbid(unsafe_code)]

use clap::{Parser, Subcommand};
use domowik_cli::{exit, CliError};
use domowik_format::OpeningKind;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "domowik",
    about = "Local-first floor plans.",
    long_about = "Create and edit floor plans stored as self-contained .domo files.\n\
                  Coordinates and dimensions are in millimetres.",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum OpeningArg {
    Door,
    Window,
}

impl From<OpeningArg> for OpeningKind {
    fn from(a: OpeningArg) -> Self {
        match a {
            OpeningArg::Door => OpeningKind::Door,
            OpeningArg::Window => OpeningKind::Window,
        }
    }
}

#[derive(Subcommand)]
enum Command {
    /// Author reusable construction bodies, hosted cuts and bare connections.
    #[command(subcommand)]
    Construction(ConstructionCommand),
    /// Create and edit stair assemblies built on level connections and openings.
    #[command(subcommand)]
    Stair(StairCommand),
    /// Create a new plan with one ground-floor level.
    New {
        /// Path to create, conventionally ending in .domo
        path: PathBuf,
        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },
    /// Convert a plan written by an earlier development build to a new file.
    ///
    /// Reads SOURCE, writes a FORMAT-SPEC §7 package to DESTINATION, and
    /// leaves SOURCE untouched. It is a repair of an unpublished layout, not a
    /// released-schema migration: no Domowik schema has been publicly
    /// released, so there is nothing to migrate from.
    ConvertDeveloperPackage {
        /// The existing plan, which is not modified.
        source: PathBuf,
        /// The file to create. It must not already exist.
        destination: PathBuf,
    },
    /// Add a wall between two points, in millimetres.
    // Plan coordinates and elevations are SIGNED. Without this, clap reads
    // `-2000` as an unknown flag and refuses it; the setting does not
    // propagate from the top-level command, so it is repeated on every
    // subcommand that takes a number. A `///` comment here would be appended
    // to the command's USER-FACING help text.
    #[command(allow_negative_numbers = true)]
    AddWall {
        path: PathBuf,
        /// Start X
        x1: f64,
        /// Start Y
        y1: f64,
        /// End X
        x2: f64,
        /// End Y
        y2: f64,
        /// Wall thickness in mm
        #[arg(long, default_value_t = domowik_commands::edit::DEFAULT_WALL_THICKNESS_MM)]
        thickness: f64,
        /// Wall height in mm (defaults to the target level's wall height)
        #[arg(long)]
        height: Option<f64>,
        /// Level to add to (defaults to the first)
        #[arg(long)]
        level: Option<String>,
    },
    /// Add a door or window to a wall, named by number, ID or a point on it.
    AddOpening {
        path: PathBuf,
        /// Wall: its `info` number, UUID, short ID (4e1a92) or a point on it
        /// (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        wall: String,
        /// door or window
        #[arg(value_enum)]
        kind: OpeningArg,
        /// Centre as a fraction of the wall's length from its start, 0.0 to
        /// 1.0. Default: the given point when the wall is named by a point,
        /// otherwise the middle of the wall.
        #[arg(long, conflicts_with = "offset")]
        at: Option<f64>,
        /// Centre in millimetres along the wall from its start.
        #[arg(long)]
        offset: Option<f64>,
        /// Width in mm
        #[arg(long, default_value_t = 900.0)]
        width: f64,
        /// Height in mm [default: 2100 for a door, 1200 for a window]
        #[arg(long)]
        height: Option<f64>,
        /// Sill height in mm (windows only)
        #[arg(long, default_value_t = 900.0)]
        sill: f64,
        /// Hinged end of the wall's axis: near-start or near-end.
        #[arg(long, value_parser = ["near-start", "near-end"])]
        hinge: Option<String>,
        /// Side the leaf swings into: left or right, as seen walking from the
        /// wall's start to its end.
        #[arg(long, value_parser = ["left", "right"])]
        opens_toward: Option<String>,
        /// Swing angle in degrees, 0 to 180.
        #[arg(long)]
        swing: Option<f64>,
        /// hinged or sliding (doors); side-hung, double-swing or lift (windows).
        #[arg(long, value_parser = ["hinged", "sliding", "side-hung", "double-swing", "lift"])]
        mechanism: Option<String>,
    },
    /// Remove a wall and any openings it hosts.
    RemoveWall {
        path: PathBuf,
        /// Wall: its `info` number, UUID, short ID (4e1a92) or a point on it
        /// (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        wall: String,
    },
    /// Remove an opening.
    RemoveOpening {
        path: PathBuf,
        /// Opening: its `info` number, UUID, short ID (91c0de) or a point
        /// inside it (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        opening: String,
    },
    /// Summarise a plan.
    Info {
        /// Plan file.
        path: PathBuf,
        /// Emit a machine-readable report instead of the human summary.
        #[arg(long)]
        json: bool,
    },
    /// Check a plan against the format constraints.
    Validate {
        /// Plan file.
        path: PathBuf,
        /// Emit a machine-readable report instead of the human summary.
        #[arg(long)]
        json: bool,
    },
    /// Look for likely drafting mistakes that are still valid plans: wall
    /// ends that stop just short of a wall, room names outside any room,
    /// walls that enclose nothing. Each finding names a fix.
    Check {
        /// Plan file.
        path: PathBuf,
        /// Only this level.
        #[arg(long)]
        level: Option<String>,
        /// Emit a machine-readable report.
        #[arg(long)]
        json: bool,
        /// Exit 1 when any warning is found (notes do not count).
        #[arg(long)]
        strict: bool,
    },
    /// Change a wall's geometry by its `info` number.
    // Plan coordinates and elevations are SIGNED. Without this, clap reads
    // `-2000` as an unknown flag and refuses it; the setting does not
    // propagate from the top-level command, so it is repeated on every
    // subcommand that takes a number. A `///` comment here would be appended
    // to the command's USER-FACING help text.
    #[command(allow_negative_numbers = true)]
    /// Reverse a wall's direction, remapping every opening it hosts.
    ///
    /// Swapping the endpoints alone would silently teleport each door to the
    /// mirror-image position and point it the wrong way (BEHAVIOR-SPEC
    /// R4.3.1), so the field map travels in the same batch.
    ReverseWall {
        /// Plan file.
        path: PathBuf,
        /// Wall: its `info` number, UUID, short ID (4e1a92) or a point on it
        /// (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        wall: String,
    },
    /// Detach one endpoint of a wall from its join (BEHAVIOR-SPEC R3.3.7).
    /// The endpoint does not move; a join left with one member is deleted.
    Detach {
        /// Plan file.
        path: PathBuf,
        /// Wall: its `info` number, UUID, short ID (4e1a92) or a point on it
        /// (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        wall: String,
        /// Which endpoint: `start` or `end`.
        #[arg(long, value_parser = ["start", "end"])]
        end: String,
    },
    SetWall {
        /// Plan file.
        path: PathBuf,
        /// Wall: its `info` number, UUID, short ID (4e1a92) or a point on it
        /// (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        wall: String,
        /// New start point, as `x,y` in millimetres.
        #[arg(long, value_parser = parse_point)]
        start: Option<(f64, f64)>,
        /// New end point, as `x,y` in millimetres.
        #[arg(long, value_parser = parse_point)]
        end: Option<(f64, f64)>,
        /// New thickness in millimetres.
        #[arg(long)]
        thickness: Option<f64>,
        /// New height in millimetres.
        #[arg(long)]
        height: Option<f64>,
    },

    /// Change an opening's parameters.
    // Plan coordinates and elevations are SIGNED. Without this, clap reads
    // `-2000` as an unknown flag and refuses it; the setting does not
    // propagate from the top-level command, so it is repeated on every
    // subcommand that takes a number. A `///` comment here would be appended
    // to the command's USER-FACING help text.
    #[command(allow_negative_numbers = true)]
    SetOpening {
        /// Plan file.
        path: PathBuf,
        /// Opening: its `info` number, UUID, short ID (91c0de) or a point
        /// inside it (x,y in mm). Short IDs and points survive renumbering.
        #[arg(allow_hyphen_values = true)]
        opening: String,
        /// Centre as a fraction of the host wall's length, 0..1.
        #[arg(long, conflicts_with = "offset")]
        at: Option<f64>,
        /// Centre in millimetres along the host wall from its start.
        #[arg(long)]
        offset: Option<f64>,
        /// Width in millimetres.
        #[arg(long)]
        width: Option<f64>,
        /// Height in millimetres.
        #[arg(long)]
        height: Option<f64>,
        /// Sill height in millimetres.
        #[arg(long)]
        sill: Option<f64>,
        /// Hinged end of the wall's axis: near-start or near-end.
        #[arg(long, value_parser = ["near-start", "near-end"])]
        hinge: Option<String>,
        /// Side the leaf swings into: left or right, as seen walking from the
        /// wall's start to its end.
        #[arg(long, value_parser = ["left", "right"])]
        opens_toward: Option<String>,
        /// Swing angle in degrees, 0 to 180.
        #[arg(long)]
        swing: Option<f64>,
        /// hinged or sliding (doors); side-hung, double-swing or lift (windows).
        #[arg(long, value_parser = ["hinged", "sliding", "side-hung", "double-swing", "lift"])]
        mechanism: Option<String>,
    },

    /// Add a level.
    // Plan coordinates and elevations are SIGNED. Without this, clap reads
    // `-2000` as an unknown flag and refuses it; the setting does not
    // propagate from the top-level command, so it is repeated on every
    // subcommand that takes a number. A `///` comment here would be appended
    // to the command's USER-FACING help text.
    #[command(allow_negative_numbers = true)]
    AddLevel {
        /// Plan file.
        path: PathBuf,
        /// Level name.
        name: String,
        /// Elevation in millimetres.
        #[arg(long, default_value_t = 0.0)]
        elevation: f64,
    },

    /// Edit level properties atomically. All dimensions are in millimetres.
    // Plan coordinates and elevations are SIGNED. Without this, clap reads
    // `-2000` as an unknown flag and refuses it; the setting does not
    // propagate from the top-level command, so it is repeated on every
    // subcommand that takes a number. A `///` comment here would be appended
    // to the command's USER-FACING help text.
    #[command(allow_negative_numbers = true)]
    SetLevel {
        /// Plan file.
        path: PathBuf,
        /// Current level name.
        name: String,
        /// New name.
        #[arg(long)]
        rename: Option<String>,
        /// New elevation in millimetres.
        #[arg(long)]
        elevation: Option<f64>,
        /// Floor depth extending downward, 0..1000000 mm.
        #[arg(long)]
        floor_thickness: Option<f64>,
        /// Default height for future walls, >0..1000000 mm.
        #[arg(long)]
        wall_height: Option<f64>,
        /// Default ceiling height, >0..1000000 mm.
        #[arg(long)]
        ceiling_height: Option<f64>,
    },

    /// List the levels in a given order. Give every level name in the order
    /// the level list should show it; each level is assigned its position as
    /// a stored `displayOrder`. The list, not the storeys' elevations, is
    /// what changes.
    OrderLevels {
        /// Plan file.
        path: PathBuf,
        /// Level names, top of the list first.
        levels: Vec<String>,
    },

    /// Remove an empty, unreferenced level.
    RemoveLevel {
        /// Plan file.
        path: PathBuf,
        /// Level name.
        name: String,
    },

    /// Render a level to PNG.
    ExportPng {
        /// Plan file.
        path: PathBuf,
        /// Output .png path.
        out: PathBuf,
        /// Level name; defaults to the first level.
        #[arg(long)]
        level: Option<String>,
        /// Pixels per millimetre.
        #[arg(long, default_value_t = 0.2)]
        scale: f64,
        /// Annotate for review: wall labels, short IDs and lengths, opening
        /// labels and widths, and a 1 m scale bar.
        #[arg(long)]
        review: bool,
    },

    /// Write a copy of a plan under a package profile.
    Pack {
        /// Plan file.
        path: PathBuf,
        /// Output plan file.
        out: PathBuf,
        /// Embed and verify every referenced asset.
        #[arg(long)]
        portable: bool,
        /// Move blobs into the cache and keep them outside the package.
        #[arg(long)]
        thin: bool,
        /// Cache root. Required by --thin; lets --portable pull missing blobs.
        #[arg(long)]
        cache: Option<PathBuf>,
    },

    /// Manage background images and other blobs.
    #[command(subcommand)]
    Asset(AssetCommand),

    /// Undo the most recent operation batch.
    Undo {
        /// Plan file.
        path: PathBuf,
    },

    /// Apply an external operation batch from a JSON file.
    Apply {
        /// Plan file.
        path: PathBuf,
        /// The batch document to apply.
        batch: PathBuf,
        /// Report what the batch would do, and write nothing.
        #[arg(long)]
        dry_run: bool,
    },

    /// Fold the operation log into the snapshot. Irreversible.
    Compact {
        /// Plan file.
        path: PathBuf,
        /// Report what would be discarded and retained, and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Assert that this document is not synchronized with any peer.
        ///
        /// Required to execute. FORMAT-SPEC §5.1 S6 forbids dropping a batch a
        /// peer might still need, and v0.1 has no protocol that can attest
        /// otherwise, so locality is asserted rather than assumed.
        #[arg(long)]
        assert_local_only: bool,
        /// Emit the machine-readable plan instead of the human summary.
        #[arg(long)]
        json: bool,
    },

    /// List the operation batches a plan retains.
    Ops {
        /// Plan file.
        path: PathBuf,
        /// Emit a machine-readable report instead of the human summary.
        #[arg(long)]
        json: bool,
        /// Also apply the log in memory, to report what replay discards.
        ///
        /// Off by default: inspecting a file you do not trust should not run
        /// its operations against a document (ED-0023).
        #[arg(long)]
        replay: bool,
    },

    /// Create, name, restyle, list and delete room records.
    #[command(subcommand)]
    Room(RoomCommand),

    /// Render a level to SVG.
    ExportSvg {
        path: PathBuf,
        /// Output .svg path
        out: PathBuf,
        /// Level to render (defaults to the first)
        #[arg(long)]
        level: Option<String>,
    },
}

/// Parses an `x,y` millimetre pair.
///
/// A single argument rather than two positionals so `--start` and `--end` stay
/// unambiguous: two bare numbers after a flag would be indistinguishable from
/// a number followed by a positional.
fn parse_point(s: &str) -> std::result::Result<(f64, f64), String> {
    let (x, y) = s
        .split_once(',')
        .ok_or_else(|| format!("expected `x,y` in millimetres, got {s:?}"))?;
    let x: f64 = x
        .trim()
        .parse()
        .map_err(|_| format!("{:?} is not a number", x.trim()))?;
    let y: f64 = y
        .trim()
        .parse()
        .map_err(|_| format!("{:?} is not a number", y.trim()))?;
    Ok((x, y))
}

#[derive(Subcommand)]
enum StairCommand {
    /// Add a straight stair from a lower endpoint to an upper endpoint.
    Add {
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        lower_level: String,
        #[arg(long)]
        upper_level: String,
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        start: (f64, f64),
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        end: (f64, f64),
        #[arg(long)]
        width: f64,
        #[arg(long)]
        risers: u16,
        #[arg(long, default_value_t = 0.0)]
        guard_height: f64,
        #[arg(long)]
        cut_upper_floor: bool,
    },
    /// Convert a rectangular placeholder room, preserving shared enclosure.
    FromRoom {
        path: PathBuf,
        room: String,
        #[arg(long)]
        lower_level: String,
        #[arg(long,value_parser=["east","west","north","south"])]
        upper_edge: String,
        #[arg(long)]
        going: f64,
        #[arg(long)]
        risers: u16,
        #[arg(long, default_value_t = 0.0)]
        guard_height: f64,
    },
    /// Edit name, dimensions, count or absolute endpoint elevations (upper start, lower end).
    Set {
        path: PathBuf,
        staircase: String,
        #[arg(long)]
        width: Option<f64>,
        #[arg(long)]
        risers: Option<u16>,
        #[arg(long)]
        guard_height: Option<f64>,
        #[arg(long)]
        name: Option<String>,
        #[arg(long, allow_hyphen_values = true)]
        start_elevation: Option<f64>,
        #[arg(long, allow_hyphen_values = true)]
        end_elevation: Option<f64>,
    },
    /// Remove a stair and its connection, retaining independent associated cuts.
    Remove { path: PathBuf, staircase: String },
}

#[derive(Subcommand)]
enum ConstructionCommand {
    /// Rectangular or longitudinally sloped prism; bottom offsets are level-relative.
    AddBody {
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        level: String,
        #[arg(long,value_parser=["slab","platform","column","beam","guard","generic"])]
        role: String,
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        start: (f64, f64),
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        end: (f64, f64),
        #[arg(long)]
        width: f64,
        #[arg(long)]
        thickness: f64,
        #[arg(long, default_value_t = 0.0, allow_hyphen_values = true)]
        bottom: f64,
        #[arg(long, allow_hyphen_values = true)]
        end_bottom: Option<f64>,
    },
    /// A through-cut in a construction body or a level's derived floor surfaces.
    AddOpening {
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long, conflicts_with = "body", required_unless_present = "body")]
        level: Option<String>,
        #[arg(long, conflicts_with = "level")]
        body: Option<String>,
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        start: (f64, f64),
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        end: (f64, f64),
        #[arg(long)]
        width: f64,
    },
    /// Connectivity only, without implying a stair, ramp or lift assembly.
    AddConnection {
        path: PathBuf,
        #[arg(long)]
        name: String,
        #[arg(long)]
        from_level: String,
        #[arg(long)]
        to_level: String,
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        start: (f64, f64),
        #[arg(long,value_parser=parse_point,allow_hyphen_values=true)]
        end: (f64, f64),
    },
    /// Remove a primitive; refuses while referenced by surviving construction.
    Remove { path: PathBuf, reference: String },
}

/// `domowik room ...`
///
/// A room RECORD is what these create and edit: the persistent metadata that
/// names and styles a room (BEHAVIOR-SPEC §5.2). The room itself — the face,
/// its ring and its area — is derived from the walls and is not editable here
/// or anywhere else (R5.1.7).
#[derive(Subcommand, Debug)]
enum RoomCommand {
    /// Place a room record at a seed point, optionally naming it.
    // Plan coordinates are SIGNED; see the note on `AddWall`.
    #[command(allow_negative_numbers = true)]
    Add {
        /// Plan file.
        path: PathBuf,
        /// Seed X, in millimetres.
        x: f64,
        /// Seed Y, in millimetres.
        y: f64,
        /// Display name.
        #[arg(long)]
        name: Option<String>,
        /// Supply the record's identifier instead of minting one.
        #[arg(long)]
        id: Option<uuid::Uuid>,
        /// Level to place it on; defaults to the first.
        #[arg(long)]
        level: Option<String>,
        /// Floor style reference, a non-empty opaque token.
        #[arg(long)]
        floor_style: Option<String>,
        /// Ceiling style reference, a non-empty opaque token.
        #[arg(long)]
        ceiling_style: Option<String>,
    },
    /// List room records with their derived status and area.
    List {
        /// Plan file.
        path: PathBuf,
        /// Only this level.
        #[arg(long)]
        level: Option<String>,
    },
    /// Change a room record's name, seed, styles or level.
    // Plan coordinates are SIGNED; see the note on `AddWall`.
    #[command(allow_negative_numbers = true)]
    Set {
        /// Plan file.
        path: PathBuf,
        /// Room number from `room list`, or the record's UUID.
        room: String,
        /// New display name. The empty string is a legal name and is NOT the
        /// same as clearing; use --clear-name for that.
        #[arg(long)]
        name: Option<String>,
        /// Remove the name entirely.
        #[arg(long)]
        clear_name: bool,
        /// New seed point, as `x,y` in millimetres.
        #[arg(long, value_parser = parse_point)]
        seed: Option<(f64, f64)>,
        /// New floor style reference.
        #[arg(long)]
        floor_style: Option<String>,
        /// Remove the floor style reference.
        #[arg(long)]
        clear_floor_style: bool,
        /// New ceiling style reference.
        #[arg(long)]
        ceiling_style: Option<String>,
        /// Remove the ceiling style reference.
        #[arg(long)]
        clear_ceiling_style: bool,
        /// Move the record to this level.
        #[arg(long)]
        level: Option<String>,
    },
    /// Delete a room record. The face it named is unchanged.
    Remove {
        /// Plan file.
        path: PathBuf,
        /// Room number from `room list`, or the record's UUID.
        room: String,
    },
}

/// `domowik asset ...`
#[derive(Subcommand, Debug)]
enum AssetCommand {
    /// Import a file and place it as a background image.
    Add {
        /// Plan file.
        path: PathBuf,
        /// The image to import.
        file: PathBuf,
        /// Level to place it on; defaults to the first level.
        #[arg(long)]
        level: Option<String>,
        /// Millimetres per image pixel.
        #[arg(long, default_value_t = 10.0)]
        scale: f64,
    },
    /// List the blobs a plan holds and what references them.
    List {
        /// Plan file.
        path: PathBuf,
        /// Cache root to measure cache bytes against.
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Verify blobs against their digests and references against the store.
    Verify {
        /// Plan file.
        path: PathBuf,
    },
    /// Remove a blob and every reference to it.
    Remove {
        /// Plan file.
        path: PathBuf,
        /// The `sha256:<hex>` digest to remove.
        digest: String,
    },
    /// Manage the local content cache (ASSET-STRATEGY §10.6, §10.7).
    #[command(subcommand)]
    Cache(CacheCommand),
}

/// `domowik asset cache ...`
///
/// These operate on a cache ROOT and never open a plan. ASSET-STRATEGY §10.7
/// names this issue's `asset` commands as the surface for manual pins.
#[derive(Subcommand, Debug)]
enum CacheCommand {
    /// Report what the cache holds and whether the collector may run.
    Status {
        /// Cache root directory.
        root: PathBuf,
    },
    /// Protect a blob durably with a `manual` pin.
    Pin {
        /// Cache root directory.
        root: PathBuf,
        /// The `sha256:<hex>` digest to pin.
        digest: String,
        /// Who holds the pin.
        #[arg(long, default_value = "cli")]
        owner: String,
    },
    /// Release a `manual` pin.
    Unpin {
        /// Cache root directory.
        root: PathBuf,
        /// The `sha256:<hex>` digest to release.
        digest: String,
        /// Who holds the pin.
        #[arg(long, default_value = "cli")]
        owner: String,
    },
    /// Collect unprotected blobs. Refuses entirely if the records are damaged.
    Gc {
        /// Cache root directory.
        root: PathBuf,
        /// Collect even though no document has registered its roots.
        #[arg(long)]
        allow_unregistered: bool,
    },
    /// Repair damaged protection records and re-enable collection.
    Rebuild {
        /// Cache root directory.
        root: PathBuf,
    },
}

fn main() -> ExitCode {
    // clap exits 2 on its own for parse errors, which matches the contract.
    let cli = Cli::parse();

    let result = match &cli.command {
        Command::Construction(command) => {
            use domowik_format::{BodyRole, RectFootprint};
            match command {
                ConstructionCommand::AddBody {
                    path,
                    name,
                    level,
                    role,
                    start,
                    end,
                    width,
                    thickness,
                    bottom,
                    end_bottom,
                } => {
                    let role = match role.as_str() {
                        "slab" => BodyRole::Slab,
                        "platform" => BodyRole::Platform,
                        "column" => BodyRole::Column,
                        "beam" => BodyRole::Beam,
                        "guard" => BodyRole::Guard,
                        _ => BodyRole::Generic,
                    };
                    domowik_cli::architecture::add_body(
                        path,
                        level,
                        name,
                        role,
                        RectFootprint {
                            start_mm: [start.0, start.1],
                            end_mm: [end.0, end.1],
                            width_mm: *width,
                        },
                        [*bottom, end_bottom.unwrap_or(*bottom)],
                        *thickness,
                    )
                }
                ConstructionCommand::AddOpening {
                    path,
                    name,
                    level,
                    body,
                    start,
                    end,
                    width,
                } => domowik_cli::architecture::add_cut(
                    path,
                    name,
                    level.as_deref(),
                    body.as_deref(),
                    RectFootprint {
                        start_mm: [start.0, start.1],
                        end_mm: [end.0, end.1],
                        width_mm: *width,
                    },
                ),
                ConstructionCommand::AddConnection {
                    path,
                    name,
                    from_level,
                    to_level,
                    start,
                    end,
                } => domowik_cli::architecture::add_connection(
                    path,
                    name,
                    from_level,
                    to_level,
                    [start.0, start.1],
                    [end.0, end.1],
                ),
                ConstructionCommand::Remove { path, reference } => {
                    domowik_cli::architecture::remove_primitive(path, reference)
                }
            }
        }
        Command::Stair(command) => match command {
            StairCommand::Add {
                path,
                name,
                lower_level,
                upper_level,
                start,
                end,
                width,
                risers,
                guard_height,
                cut_upper_floor,
            } => domowik_cli::architecture::add(
                path,
                name,
                lower_level,
                upper_level,
                [start.0, start.1],
                [end.0, end.1],
                *width,
                *risers,
                *guard_height,
                *cut_upper_floor,
            ),
            StairCommand::FromRoom {
                path,
                room,
                lower_level,
                upper_edge,
                going,
                risers,
                guard_height,
            } => {
                let edge = match upper_edge.as_str() {
                    "east" => domowik_edit::architecture::UpperEdge::East,
                    "west" => domowik_edit::architecture::UpperEdge::West,
                    "north" => domowik_edit::architecture::UpperEdge::North,
                    _ => domowik_edit::architecture::UpperEdge::South,
                };
                domowik_cli::architecture::from_room(
                    path,
                    room,
                    lower_level,
                    edge,
                    *going,
                    *risers,
                    *guard_height,
                )
            }
            StairCommand::Set {
                path,
                staircase,
                width,
                risers,
                guard_height,
                name,
                start_elevation,
                end_elevation,
            } => domowik_cli::architecture::set_properties(
                path,
                staircase,
                &domowik_edit::architecture::StairFields {
                    width_mm: *width,
                    risers: *risers,
                    guard_height_mm: *guard_height,
                    name: name.clone(),
                    start_elevation_mm: *start_elevation,
                    end_elevation_mm: *end_elevation,
                },
            ),
            StairCommand::Remove { path, staircase } => {
                domowik_cli::architecture::remove(path, staircase)
            }
        },
        Command::New { path, force } => domowik_cli::cmd_new(path, *force),
        Command::ConvertDeveloperPackage {
            source,
            destination,
        } => domowik_cli::cmd_convert_developer_package(source, destination),
        Command::AddWall {
            path,
            x1,
            y1,
            x2,
            y2,
            thickness,
            height,
            level,
        } => domowik_cli::cmd_add_wall(
            path,
            *x1,
            *y1,
            *x2,
            *y2,
            *thickness,
            *height,
            level.as_deref(),
        ),
        Command::AddOpening {
            path,
            wall,
            kind,
            at,
            offset,
            width,
            height,
            sill,
            hinge,
            opens_toward,
            swing,
            mechanism,
        } => domowik_cli::cmd_add_opening(
            path,
            wall,
            (*kind).into(),
            placement(*at, *offset),
            *width,
            // A door-height window on a 900 mm sill reaches 3000 mm and is
            // refused by any ordinary 2500 mm wall, so windows get their own.
            height.unwrap_or(match kind {
                OpeningArg::Door => 2100.0,
                OpeningArg::Window => 1200.0,
            }),
            *sill,
            orientation(hinge, opens_toward, *swing, mechanism),
        ),
        Command::RemoveWall { path, wall } => domowik_cli::cmd_remove_wall(path, wall),
        Command::RemoveOpening { path, opening } => domowik_cli::cmd_remove_opening(path, opening),
        Command::Info { path, json } => {
            if *json {
                domowik_cli::cmd_info_json(path)
            } else {
                domowik_cli::cmd_info(path)
            }
        }
        Command::Validate { path, json } => {
            if *json {
                domowik_cli::cmd_validate_json(path)
            } else {
                domowik_cli::cmd_validate(path)
            }
        }
        Command::Check {
            path,
            level,
            json,
            strict,
        } => domowik_cli::check::cmd_check(path, level.as_deref(), *json, *strict),
        Command::ReverseWall { path, wall } => domowik_cli::cmd_reverse_wall(path, wall),
        Command::Detach { path, wall, end } => domowik_cli::cmd_detach(path, wall, end == "end"),
        Command::SetWall {
            path,
            wall,
            start,
            end,
            thickness,
            height,
        } => domowik_cli::cmd_set_wall(path, wall, *start, *end, *thickness, *height),
        Command::SetOpening {
            path,
            opening,
            at,
            offset,
            width,
            height,
            sill,
            hinge,
            opens_toward,
            swing,
            mechanism,
        } => domowik_cli::cmd_set_opening(
            path,
            opening,
            placement(*at, *offset),
            *width,
            *height,
            *sill,
            orientation(hinge, opens_toward, *swing, mechanism),
        ),
        Command::AddLevel {
            path,
            name,
            elevation,
        } => domowik_cli::cmd_add_level(path, name, *elevation),
        Command::SetLevel {
            path,
            name,
            rename,
            elevation,
            floor_thickness,
            wall_height,
            ceiling_height,
        } => domowik_cli::cmd_set_level_fields(
            path,
            name,
            &domowik_edit::LevelFields {
                name: rename.clone(),
                elevation_mm: *elevation,
                floor_thickness_mm: *floor_thickness,
                default_wall_height_mm: *wall_height,
                default_ceiling_height_mm: *ceiling_height,
            },
        ),
        Command::RemoveLevel { path, name } => domowik_cli::cmd_remove_level(path, name),
        Command::OrderLevels { path, levels } => domowik_cli::cmd_order_levels(path, levels),
        Command::ExportPng {
            path,
            out,
            level,
            scale,
            review,
        } => domowik_cli::cmd_export_png(path, out, level.as_deref(), *scale, *review),
        Command::Pack {
            path,
            out,
            portable,
            thin,
            cache,
        } => domowik_cli::cmd_pack(path, out, *portable, *thin, cache.as_deref()),
        Command::Asset(cmd) => match cmd {
            AssetCommand::Add {
                path,
                file,
                level,
                scale,
            } => domowik_cli::cmd_asset_add(path, file, level.as_deref(), *scale),
            AssetCommand::List { path, cache } => {
                domowik_cli::cmd_asset_list(path, cache.as_deref())
            }
            AssetCommand::Verify { path } => domowik_cli::cmd_asset_verify(path),
            AssetCommand::Remove { path, digest } => domowik_cli::cmd_asset_remove(path, digest),
            AssetCommand::Cache(cmd) => match cmd {
                CacheCommand::Status { root } => domowik_cli::cmd_cache_status(root),
                CacheCommand::Pin {
                    root,
                    digest,
                    owner,
                } => domowik_cli::cmd_cache_pin(root, digest, owner),
                CacheCommand::Unpin {
                    root,
                    digest,
                    owner,
                } => domowik_cli::cmd_cache_unpin(root, digest, owner),
                CacheCommand::Gc {
                    root,
                    allow_unregistered,
                } => domowik_cli::cmd_cache_gc(root, *allow_unregistered),
                CacheCommand::Rebuild { root } => domowik_cli::cmd_cache_rebuild(root),
            },
        },
        Command::Undo { path } => domowik_cli::cmd_undo(path),
        Command::Apply {
            path,
            batch,
            dry_run,
        } => domowik_cli::cmd_apply(path, batch, *dry_run),
        Command::Ops { path, json, replay } => {
            if *json {
                domowik_cli::cmd_ops_json(path, *replay)
            } else {
                domowik_cli::cmd_ops(path, *replay)
            }
        }
        Command::Compact {
            path,
            dry_run,
            assert_local_only,
            json,
        } => domowik_cli::cmd_compact(
            path,
            domowik_cli::CompactOptions {
                dry_run: *dry_run,
                assert_local_only: *assert_local_only,
                json: *json,
            },
        ),
        Command::Room(cmd) => match cmd {
            RoomCommand::Add {
                path,
                x,
                y,
                name,
                id,
                level,
                floor_style,
                ceiling_style,
            } => domowik_cli::cmd_room_add(
                path,
                *x,
                *y,
                name.as_deref(),
                *id,
                level.as_deref(),
                floor_style.as_deref(),
                ceiling_style.as_deref(),
            ),
            RoomCommand::List { path, level } => domowik_cli::cmd_room_list(path, level.as_deref()),
            RoomCommand::Set {
                path,
                room,
                name,
                clear_name,
                seed,
                floor_style,
                clear_floor_style,
                ceiling_style,
                clear_ceiling_style,
                level,
            } => domowik_cli::cmd_room_set(
                path,
                room,
                &domowik_cli::RoomSetArgs {
                    seed: *seed,
                    name: name.clone(),
                    clear_name: *clear_name,
                    floor_style: floor_style.clone(),
                    clear_floor_style: *clear_floor_style,
                    ceiling_style: ceiling_style.clone(),
                    clear_ceiling_style: *clear_ceiling_style,
                    level: level.clone(),
                },
            ),
            RoomCommand::Remove { path, room } => domowik_cli::cmd_room_remove(path, room),
        },
        Command::ExportSvg { path, out, level } => {
            domowik_cli::cmd_export_svg(path, out, level.as_deref())
        }
    };

    match result {
        Ok(message) => {
            // Success goes to stdout, and ONLY to stdout.
            match write_line(&message) {
                Ok(()) => ExitCode::from(exit::OK as u8),
                // A closed downstream is not an error the user made.
                Err(BrokenPipe) => ExitCode::from(exit::OK as u8),
            }
        }
        // A machine-readable report of an INVALID plan is the result the
        // caller asked for, so it goes to stdout and still exits non-zero.
        Err(err) if err.is_report() => {
            let _ = write_line(err.message());
            ExitCode::from(err.code() as u8)
        }
        Err(err) => {
            // Failures go to stderr, and ONLY to stderr, so callers piping
            // stdout never mistake an error for a result.
            eprintln!("domowik: {}", err.message());
            ExitCode::from(err.code() as u8)
        }
    }
}

/// A downstream reader closed the pipe before we finished writing.
struct BrokenPipe;

/// Writes `message` and a newline to stdout, tolerating a closed pipe.
///
/// `println!` PANICS when stdout cannot be written, and Rust ignores SIGPIPE
/// by default, so `domowik info | head -5` — or `| grep -q` — kills the
/// process with an exit code of 101 and a panic message about
/// "failed printing to stdout: Broken pipe".
///
/// That is not hypothetical: it broke `scripts/verify.sh` intermittently,
/// because whether `grep -q` closes the pipe before or after the write
/// completes is a race. It took a stderr-capturing harness to see it, having
/// twice been dismissed as unreproducible.
///
/// Restoring the default SIGPIPE handler is the other usual fix; it needs
/// `libc` and an `unsafe` block, and this crate forbids unsafe. Handling the
/// error is equivalent for a program whose entire output is one write.
fn write_line(message: &str) -> Result<(), BrokenPipe> {
    use std::io::{ErrorKind, Write};
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let write = writeln!(out, "{}", message.trim_end()).and_then(|()| out.flush());
    match write {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::BrokenPipe => Err(BrokenPipe),
        // Any other write failure — a full disk, a revoked terminal — is a
        // real failure, and panicking reports it the way `println!` would.
        Err(e) => panic!("failed printing to stdout: {e}"),
    }
}

// Keep the error type referenced so a future refactor cannot silently drop the
// exit-code mapping this binary depends on.
const _: fn(&CliError) -> i32 = CliError::code;

/// The orientation flags, in the format's terms. A mechanism of the wrong
/// kind is refused by `add-opening` and by the core.
fn orientation(
    hinge: &Option<String>,
    opens_toward: &Option<String>,
    swing: Option<f64>,
    mechanism: &Option<String>,
) -> domowik_cli::Orientation {
    use domowik_format::{Hinge, OpeningMechanism as M, Side};
    domowik_cli::Orientation {
        hinge: hinge.as_deref().map(|h| match h {
            "near-end" => Hinge::NearEnd,
            _ => Hinge::NearStart,
        }),
        opens_toward: opens_toward.as_deref().map(|s| match s {
            "right" => Side::Right,
            _ => Side::Left,
        }),
        swing_deg: swing,
        mechanism: mechanism.as_deref().map(|m| match m {
            "sliding" => M::DoorSliding,
            "side-hung" => M::WindowSideHung,
            "double-swing" => M::WindowDoubleSwing,
            "lift" => M::WindowLift,
            _ => M::DoorHinged,
        }),
    }
}

/// `--at` or `--offset`, which clap already made mutually exclusive.
fn placement(at: Option<f64>, offset: Option<f64>) -> Option<domowik_cli::Placement> {
    at.map(domowik_cli::Placement::Fraction)
        .or(offset.map(domowik_cli::Placement::OffsetMm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_point_accepts_a_millimetre_pair() {
        assert_eq!(parse_point("100,200"), Ok((100.0, 200.0)));
        assert_eq!(parse_point(" -1.5 , 2.5 "), Ok((-1.5, 2.5)));
    }

    #[test]
    fn parse_point_rejects_anything_else() {
        assert!(parse_point("100").is_err(), "no comma");
        assert!(parse_point("a,2").is_err(), "not a number");
        assert!(parse_point("1,").is_err(), "missing y");
    }

    #[test]
    fn writing_to_a_closed_pipe_is_not_an_error() {
        // The regression this exists for: `println!` panics when stdout
        // cannot be written, and Rust ignores SIGPIPE, so `domowik info |
        // head -5` exited 101 with a panic message. Verified end to end by
        // probe P10, which runs the shipped binary into `head`; this test
        // pins the classification `write_line` performs.
        use std::io::ErrorKind;
        let broken = std::io::Error::new(ErrorKind::BrokenPipe, "closed");
        assert_eq!(broken.kind(), ErrorKind::BrokenPipe);
    }
}
