//! Reusable construction geometry and straight stair assembly derivation.
//! All geometry uses plan-local millimetres and approximate binary64 offsets;
//! cut topology uses the existing filtered/exact snap-rounded overlay. Output is
//! deterministic in document/step order. No retained state, callbacks or GPU work.
use crate::{boolean, polygon::Polygon};
pub use domowik_format::PlanConstruction;
use domowik_format::{
    BodyRole, ConstructionBody, Document, OpeningHost, Point, RectFootprint, Staircase,
};
use uuid::Uuid;

pub fn rectangle(f: &RectFootprint) -> Polygon {
    Polygon::new(f.corners().into_iter().map(Point::from).collect())
}

/// Subtract only this host's cuts. Does not alter authoritative room topology.
pub fn cut_surfaces(doc: &Document, host: OpeningHost, surfaces: &[Polygon]) -> Vec<Polygon> {
    let cuts: Vec<_> = doc
        .hosted_openings
        .iter()
        .filter(|o| o.host == host && o.footprint.valid())
        .map(|o| rectangle(&o.footprint))
        .collect();
    boolean::difference(surfaces, &cuts)
}

pub struct StairGeometry {
    pub footprint: RectFootprint,
    pub rise_mm: f64,
    pub riser_mm: f64,
    pub going_mm: f64,
    pub incline_deg: f64,
    pub lower_elevation_mm: f64,
    pub upper_elevation_mm: f64,
    pub lower_level_id: Uuid,
    pub upper_level_id: Uuid,
    /// Derived bodies; ids are assembly ids, not independent document identities.
    pub bodies: Vec<ConstructionBody>,
}

/// Derives one straight stair, refusing missing references, invalid parameters,
/// collapsed/inverted elevations or coordinates outside the format domain.
/// O(risers), bounded at 256 before allocating. No building-code guarantee.
pub fn stair(doc: &Document, s: &Staircase) -> Result<StairGeometry, String> {
    let c = doc
        .connection(s.connection_id)
        .ok_or("missing stair connection")?;
    let lower = c.from.elevation_mm(doc).ok_or("missing lower level")?;
    let upper = c.to.elevation_mm(doc).ok_or("missing upper level")?;
    let f = RectFootprint {
        start_mm: c.from.position_mm,
        end_mm: c.to.position_mm,
        width_mm: s.width_mm,
    };
    if !(2..=256).contains(&s.risers)
        || !f.valid()
        || !lower.is_finite()
        || !upper.is_finite()
        || upper <= lower
        || upper - lower > domowik_format::DIMENSION_CAP_MM
        || !s.guard_height_mm.is_finite()
        || !(0.0..=domowik_format::DIMENSION_CAP_MM).contains(&s.guard_height_mm)
    {
        return Err("invalid stair geometry".into());
    }
    let n = usize::from(s.risers) - 1;
    let rise = upper - lower;
    let riser = rise / f64::from(s.risers);
    let going = f.length_mm() / n as f64;
    let at = |t: f64| {
        [
            f.start_mm[0] + (f.end_mm[0] - f.start_mm[0]) * t,
            f.start_mm[1] + (f.end_mm[1] - f.start_mm[1]) * t,
        ]
    };
    let mut bodies = Vec::with_capacity(n + 2);
    let base = doc
        .level(c.from.level_id)
        .ok_or("missing lower level")?
        .elevation_mm;
    // Open treads are a presentation assumption, not a structural design.
    let thickness = 50.0_f64.min(riser);
    for i in 0..n {
        let z = lower + (i + 1) as f64 * riser - thickness - base;
        bodies.push(ConstructionBody {
            id: s.id,
            level_id: c.from.level_id,
            name: format!("{} tread {}", s.name, i + 1),
            role: BodyRole::Platform,
            footprint: RectFootprint {
                start_mm: at(i as f64 / n as f64),
                end_mm: at((i + 1) as f64 / n as f64),
                width_mm: s.width_mm,
            },
            bottom_offsets_mm: [z, z],
            thickness_mm: thickness,
            unknown: Default::default(),
        });
    }
    if s.guard_height_mm > 0.0 {
        let corners = f.corners();
        for (a, b) in [(corners[0], corners[1]), (corners[3], corners[2])] {
            bodies.push(ConstructionBody {
                id: s.id,
                level_id: c.from.level_id,
                name: format!("{} rail", s.name),
                role: BodyRole::Guard,
                footprint: RectFootprint {
                    start_mm: a,
                    end_mm: b,
                    width_mm: 40.0,
                },
                bottom_offsets_mm: [
                    lower - base + s.guard_height_mm,
                    upper - base + s.guard_height_mm,
                ],
                thickness_mm: 40.0,
                unknown: Default::default(),
            });
            for i in (0..n).step_by(3).chain(std::iter::once(n)) {
                let t = i as f64 / n as f64;
                let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                let z = if i == n {
                    upper
                } else {
                    lower + (i + 1) as f64 * riser
                };
                let top = lower + (upper - lower) * t + s.guard_height_mm;
                bodies.push(ConstructionBody {
                    id: s.id,
                    level_id: c.from.level_id,
                    name: format!("{} post", s.name),
                    role: BodyRole::Guard,
                    footprint: RectFootprint {
                        start_mm: [p[0] - 15.0, p[1]],
                        end_mm: [p[0] + 15.0, p[1]],
                        width_mm: 30.0,
                    },
                    bottom_offsets_mm: [z - base, z - base],
                    thickness_mm: (top - z).max(1.0),
                    unknown: Default::default(),
                });
            }
        }
    }
    Ok(StairGeometry {
        footprint: f.clone(),
        rise_mm: rise,
        riser_mm: riser,
        going_mm: going,
        incline_deg: riser.atan2(going).to_degrees(),
        lower_elevation_mm: lower,
        upper_elevation_mm: upper,
        lower_level_id: c.from.level_id,
        upper_level_id: c.to.level_id,
        bodies,
    })
}

/// Owned semantic 2D geometry for either endpoint level. Clients paint the
/// returned segments; they must not regenerate authoritative stair geometry.
pub fn plan(doc: &Document, level: Uuid) -> Result<Vec<PlanConstruction>, String> {
    let (items, diagnostics) = plan_reporting(doc, level);
    if diagnostics.is_empty() {
        Ok(items)
    } else {
        Err(diagnostics.join("; "))
    }
}

/// Best-effort drawing of an already-open document. Invalid stair assemblies
/// are omitted individually and reported in document order, including dangling
/// connections whose level cannot be determined. Valid geometry is unchanged.
/// This does not relax the strict derivation or edit-validation contracts.
pub fn plan_reporting(doc: &Document, level: Uuid) -> (Vec<PlanConstruction>, Vec<String>) {
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    for b in doc
        .construction_bodies
        .iter()
        .filter(|b| b.level_id == level)
    {
        for p in cut_surfaces(
            doc,
            OpeningHost::ConstructionBody(b.id),
            &[rectangle(&b.footprint)],
        ) {
            out.push(PlanConstruction {
                id: b.id,
                kind: "body",
                name: b.name.clone(),
                outline_mm: p.outer.into_iter().map(Into::into).collect(),
                holes_mm: p
                    .holes
                    .into_iter()
                    .map(|r| r.into_iter().map(Into::into).collect())
                    .collect(),
                segments_mm: vec![],
                label_mm: b.footprint.start_mm,
                direction: "",
                staircase: None,
            });
        }
    }
    for s in &doc.staircases {
        let Some(c) = doc.connection(s.connection_id) else {
            diagnostics.push(stair_diagnostic(s, "missing stair connection"));
            continue;
        };
        if c.from.level_id != level && c.to.level_id != level {
            continue;
        }
        let g = match stair(doc, s) {
            Ok(g) => g,
            Err(error) => {
                diagnostics.push(stair_diagnostic(s, &error));
                continue;
            }
        };
        let mut segments = Vec::new();
        for b in g.bodies.iter().filter(|b| b.role == BodyRole::Platform) {
            let corners = b.footprint.corners();
            segments.push([corners[0], corners[3]]);
        }
        let corners = g.footprint.corners();
        segments.push([corners[1], corners[2]]);
        let a = c.from.position_mm;
        let b = c.to.position_mm;
        let at = |t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        let (tail, head) = if level == c.from.level_id {
            (at(0.15), at(0.85))
        } else {
            (at(0.85), at(0.15))
        };
        segments.push([tail, head]);
        let dx = head[0] - tail[0];
        let dy = head[1] - tail[1];
        let len = dx.hypot(dy);
        let arrow = (s.width_mm * 0.2).min(180.0);
        for sign in [-1.0, 1.0] {
            segments.push([
                head,
                [
                    head[0] - dx / len * arrow - dy / len * arrow * 0.5 * sign,
                    head[1] - dy / len * arrow + dx / len * arrow * 0.5 * sign,
                ],
            ]);
        }
        out.push(PlanConstruction {
            id: s.id,
            kind: "staircase",
            name: s.name.clone(),
            outline_mm: corners.to_vec(),
            holes_mm: vec![],
            segments_mm: segments,
            label_mm: at(0.5),
            direction: if level == c.from.level_id {
                "UP"
            } else {
                "DOWN"
            },
            staircase: Some(domowik_format::StaircaseProperties {
                width_mm: s.width_mm,
                risers: s.risers,
                guard_height_mm: s.guard_height_mm,
                start_elevation_mm: g.upper_elevation_mm,
                end_elevation_mm: g.lower_elevation_mm,
                upper_level_id: g.upper_level_id,
                lower_level_id: g.lower_level_id,
            }),
        });
    }
    (out, diagnostics)
}

/// Stable, entity-addressable rendering diagnostic; no untrusted name markup.
pub fn stair_diagnostic(stair: &Staircase, error: &str) -> String {
    format!("Skipped staircase {}: {error}", stair.id)
}
