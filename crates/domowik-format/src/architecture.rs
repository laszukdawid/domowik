//! Architectural entities; see `docs/ARCHITECTURAL-PRIMITIVES.md`.
//! Owned, cloneable document values; no shared mutable state or external resources.
use crate::{Document, Point, ValidationError, COORD_BOUND_MM, DIMENSION_CAP_MM};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Renderer-neutral derived plan graphics. Never persisted as document state;
/// clients borrow/read these owned values and only paint the supplied geometry.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanConstruction {
    pub id: Uuid,
    pub kind: &'static str,
    pub name: String,
    pub outline_mm: Vec<[f64; 2]>,
    pub holes_mm: Vec<Vec<[f64; 2]>>,
    pub segments_mm: Vec<[[f64; 2]; 2]>,
    pub label_mm: [f64; 2],
    pub direction: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub staircase: Option<StaircaseProperties>,
}

/// Derived inspector values in drawing order (upper start, lower end), in mm.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StaircaseProperties {
    pub width_mm: f64,
    pub risers: u16,
    pub guard_height_mm: f64,
    pub start_elevation_mm: f64,
    pub end_elevation_mm: f64,
    pub upper_level_id: Uuid,
    pub lower_level_id: Uuid,
}

/// A bounded rectangular footprint in plan millimetres. Width is perpendicular
/// to start→end; the endpoints are centres of the two end edges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RectFootprint {
    pub start_mm: [f64; 2],
    pub end_mm: [f64; 2],
    pub width_mm: f64,
}

impl RectFootprint {
    pub fn length_mm(&self) -> f64 {
        Point::from(self.start_mm).distance_to(self.end_mm.into())
    }

    /// Approximate binary64 corners, counterclockwise, not closed. Requires a
    /// validated footprint. No allocation or topology predicates.
    pub fn corners(&self) -> [[f64; 2]; 4] {
        let [ax, ay] = self.start_mm;
        let [bx, by] = self.end_mm;
        let scale = self.width_mm / (2.0 * self.length_mm());
        let (nx, ny) = (-(by - ay) * scale, (bx - ax) * scale);
        [
            [ax - nx, ay - ny],
            [bx - nx, by - ny],
            [bx + nx, by + ny],
            [ax + nx, ay + ny],
        ]
    }

    pub fn valid(&self) -> bool {
        Point::from(self.start_mm).is_within_bounds()
            && Point::from(self.end_mm).is_within_bounds()
            && self.start_mm != self.end_mm
            && positive(self.width_mm)
            && self.length_mm() <= DIMENSION_CAP_MM
            && self
                .corners()
                .iter()
                .flatten()
                .all(|v| v.is_finite() && v.abs() <= crate::DERIVED_COORD_BOUND_MM)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BodyRole {
    Slab,
    Platform,
    Column,
    Beam,
    Guard,
    Generic,
}

/// Physical construction; offsets are relative to the referenced level. Both
/// bottom and top slope linearly along the footprint axis; thickness is vertical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstructionBody {
    pub id: Uuid,
    pub level_id: Uuid,
    pub name: String,
    pub role: BodyRole,
    pub footprint: RectFootprint,
    pub bottom_offsets_mm: [f64; 2],
    pub thickness_mm: f64,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "id", rename_all = "camelCase")]
pub enum OpeningHost {
    ConstructionBody(Uuid),
    LevelFloor(Uuid),
}

/// A through-cut, independent of any stair, door or traversal semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostedOpening {
    pub id: Uuid,
    pub name: String,
    pub host: OpeningHost,
    pub footprint: RectFootprint,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConnectionEndpoint {
    pub level_id: Uuid,
    pub position_mm: [f64; 2],
    pub offset_mm: f64,
}

impl ConnectionEndpoint {
    pub fn elevation_mm(&self, doc: &Document) -> Option<f64> {
        Some(doc.level(self.level_id)?.elevation_mm + self.offset_mm)
    }
}

/// Connectivity only; assemblies supply physical geometry and movement type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelConnection {
    pub id: Uuid,
    pub name: String,
    pub from: ConnectionEndpoint,
    pub to: ConnectionEndpoint,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// A straight-flight assembly. N risers produce N−1 tread bodies; the upper
/// endpoint is the upper landing edge. Generated bodies are not stored entities.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Staircase {
    pub id: Uuid,
    pub name: String,
    pub connection_id: Uuid,
    pub width_mm: f64,
    pub risers: u16,
    pub guard_height_mm: f64,
    #[serde(default)]
    pub opening_ids: Vec<Uuid>,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

fn coordinate(v: f64) -> bool {
    v.is_finite() && v.abs() <= COORD_BOUND_MM
}
fn positive(v: f64) -> bool {
    v.is_finite() && v > 0.0 && v <= DIMENSION_CAP_MM
}

impl Document {
    pub fn connection(&self, id: Uuid) -> Option<&LevelConnection> {
        self.level_connections.iter().find(|c| c.id == id)
    }
    pub fn staircase(&self, id: Uuid) -> Option<&Staircase> {
        self.staircases.iter().find(|s| s.id == id)
    }
    pub fn construction_body(&self, id: Uuid) -> Option<&ConstructionBody> {
        self.construction_bodies.iter().find(|b| b.id == id)
    }
    pub fn opening_host_level(&self, host: OpeningHost) -> Option<Uuid> {
        match host {
            OpeningHost::ConstructionBody(id) => Some(self.construction_body(id)?.level_id),
            OpeningHost::LevelFloor(id) => Some(self.level(id)?.id),
        }
    }
    pub fn has_architecture(&self) -> bool {
        !self.construction_bodies.is_empty()
            || !self.hosted_openings.is_empty()
            || !self.level_connections.is_empty()
            || !self.staircases.is_empty()
    }
    pub(crate) fn validate_architecture(&self, errors: &mut Vec<ValidationError>) {
        let mut fail = |id, reason: &str| {
            errors.push(ValidationError::Architecture {
                id,
                reason: reason.into(),
            })
        };
        for b in &self.construction_bodies {
            if !b.footprint.valid() || !positive(b.thickness_mm) {
                fail(b.id, "invalid body footprint or thickness");
            }
            if !self.level(b.level_id).is_some_and(|l| {
                b.bottom_offsets_mm.iter().all(|v| {
                    coordinate(*v)
                        && coordinate(l.elevation_mm + v)
                        && coordinate(l.elevation_mm + v + b.thickness_mm)
                })
            }) {
                fail(b.id, "missing level or out-of-range body elevation");
            }
        }
        for o in &self.hosted_openings {
            if !o.footprint.valid() {
                fail(o.id, "invalid opening footprint");
            }
            if self.opening_host_level(o.host).is_none() {
                fail(o.id, "missing opening host");
            }
        }
        for c in &self.level_connections {
            for e in [&c.from, &c.to] {
                if !Point::from(e.position_mm).is_within_bounds()
                    || !coordinate(e.offset_mm)
                    || !e.elevation_mm(self).is_some_and(coordinate)
                {
                    fail(c.id, "invalid connection endpoint or missing level");
                }
            }
        }
        let mut used = std::collections::HashSet::new();
        for s in &self.staircases {
            if !positive(s.width_mm)
                || !(2..=256).contains(&s.risers)
                || !s.guard_height_mm.is_finite()
                || !(0.0..=DIMENSION_CAP_MM).contains(&s.guard_height_mm)
            {
                fail(s.id, "invalid stair width, riser count or guard height");
            }
            match self.connection(s.connection_id) {
                None => fail(s.id, "missing stair connection"),
                Some(c) => {
                    let footprint = RectFootprint {
                        start_mm: c.from.position_mm,
                        end_mm: c.to.position_mm,
                        width_mm: s.width_mm,
                    };
                    let rise =
                        c.to.elevation_mm(self)
                            .zip(c.from.elevation_mm(self))
                            .map(|(a, b)| a - b);
                    if !footprint.valid() || !rise.is_some_and(positive) {
                        fail(
                            s.id,
                            "stair must have a positive bounded rise and nonzero run",
                        );
                    }
                    if let Some(top) = c.to.elevation_mm(self) {
                        if !coordinate(top + s.guard_height_mm) {
                            fail(s.id, "stair guards exceed elevation bounds");
                        }
                    }
                }
            }
            if !used.insert(s.connection_id) {
                fail(s.id, "connection already has a staircase");
            }
            let mut ids = std::collections::HashSet::new();
            for id in &s.opening_ids {
                if !ids.insert(id) || !self.hosted_openings.iter().any(|o| o.id == *id) {
                    fail(s.id, "missing or duplicate associated opening");
                }
            }
        }
        for (id, kind, name) in self
            .construction_bodies
            .iter()
            .map(|b| (b.id, "constructionBody", &b.name))
            .chain(
                self.hosted_openings
                    .iter()
                    .map(|o| (o.id, "hostedOpening", &o.name)),
            )
            .chain(
                self.level_connections
                    .iter()
                    .map(|c| (c.id, "levelConnection", &c.name)),
            )
            .chain(self.staircases.iter().map(|s| (s.id, "staircase", &s.name)))
        {
            crate::check_display_string(errors, id, kind, "name", &Some(name.clone()));
        }
    }
}
