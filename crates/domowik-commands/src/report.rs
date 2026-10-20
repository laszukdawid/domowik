//! The machine-readable report of a plan: `info --json`, `validate --json`
//! and the hosted service's plan inspection.

use crate::levels::{display_levels, numbered_records};
use crate::refs;
use domowik_container::Manifest;
use domowik_document::operation::Batch;
use domowik_document::replay::Projection;
use domowik_format::OpeningKind;
use domowik_geometry::rooms;
use uuid::Uuid;

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

/// Builds the machine-readable report of a plan named `label`, from its
/// manifest, its retained batches and their projection.
pub fn build(
    label: &str,
    manifest: &Manifest,
    batches: &[Batch],
    projection: &Projection,
) -> Report {
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

    Report {
        path: label.to_owned(),
        document_id: doc.document_id,
        format_version: manifest.format_version.clone(),
        profile: manifest.profile.to_string(),
        batches: batches.len(),
        operations: batches.iter().map(|b| b.operations.len()).sum(),
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
    }
}
