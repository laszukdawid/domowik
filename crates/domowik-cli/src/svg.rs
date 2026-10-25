//! SVG export.
//!
//! Renders a level as its DERIVED OCCUPIED REGION (BEHAVIOR-SPEC R3.2.1) — one
//! filled path per connected component of the wall mass, with rooms beneath it
//! and apertures painted over it. The output is a complete, standalone SVG
//! document that opens in any browser, which is the point: it is the first way
//! a user can SEE a plan they have built.
//!
//! Drawing the region rather than one rectangle per wall is what makes corners
//! look like corners. Per-wall rectangles overlap at every join and leave a
//! notch outside the overlap, so an L corner rendered that way shows a seam and
//! a bite taken out of its outside edge.

use crate::label::{self, LabelWarning};
use crate::plan_on_level;
use domowik_format::{Document, Hinge, Opening, OpeningKind, Point, Side};
use domowik_geometry::cuts::{level_drawn_region, level_has_openings, Aperture};
use domowik_geometry::polygon::Polygon;
use domowik_geometry::rooms;
use domowik_geometry::walls;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use uuid::Uuid;

/// Margin around the drawing, in millimetres.
const MARGIN_MM: f64 = 500.0;

/// Renders `level_id`'s walls to a standalone SVG document.
///
/// # Coordinate system
///
/// The plan plane is right-handed with +Y up (BEHAVIOR-SPEC §1.1); SVG's Y axis
/// points DOWN. The viewBox is therefore emitted with a flipped Y transform
/// rather than by negating every coordinate, so the numbers in the output still
/// match the numbers in the document and stay readable.
pub fn render_level(doc: &Document, level_id: Uuid) -> String {
    render_level_with_assets(doc, level_id, &BTreeMap::new())
}

/// [`render_level`], with the blob store available so background images appear.
///
/// The image is embedded as a `data:` URI rather than referenced by path: an
/// SVG that points at a file next to it stops working the moment either moves,
/// and a floor plan handed to someone else is exactly the case this export is
/// for.
///
/// PARITY WITH THE PNG EXPORT IS THE POINT. Two exports of one plan showing
/// different things is worse than either being incomplete, and the SVG showed
/// no background at all until this existed.
pub fn render_level_with_assets(
    doc: &Document,
    level_id: Uuid,
    assets: &BTreeMap<String, Vec<u8>>,
) -> String {
    render_level_reporting(doc, level_id, assets).0
}

/// [`render_level_with_assets`], also returning every room name it declined to
/// draw.
///
/// The document always renders; a name outside the exporter's repertoire is
/// left out of the drawing and reported here (BEHAVIOR-SPEC §5.3 labels,
/// [`crate::label`]). Callers that can speak to a person should say so — an
/// export silently missing a label is the failure worth avoiding.
pub fn render_level_reporting(
    doc: &Document,
    level_id: Uuid,
    assets: &BTreeMap<String, Vec<u8>>,
) -> (String, Vec<LabelWarning>) {
    let (svg, labels, _) = render_level_diagnostics(doc, level_id, assets);
    (svg, labels)
}

/// Best-effort SVG with both label warnings and omitted-assembly diagnostics.
/// Interactive/CLI callers should report the latter, not silently discard them.
pub fn render_level_diagnostics(
    doc: &Document,
    level_id: Uuid,
    assets: &BTreeMap<String, Vec<u8>>,
) -> (String, Vec<LabelWarning>, Vec<String>) {
    let walls: Vec<_> = doc.walls_on_level(level_id).collect();

    let (mut min, mut max) = doc
        .bounds()
        .unwrap_or((Point::new(0.0, 0.0), Point::new(1000.0, 1000.0)));
    for (a, b) in crate::png::background_extents(doc, level_id, assets) {
        min = Point::new(min.x.min(a.x), min.y.min(a.y));
        max = Point::new(max.x.max(b.x), max.y.max(b.y));
    }
    let (min_x, min_y) = (min.x - MARGIN_MM, min.y - MARGIN_MM);
    let width = (max.x - min.x) + 2.0 * MARGIN_MM;
    let height = (max.y - min.y) + 2.0 * MARGIN_MM;

    let level_name = doc
        .level(level_id)
        .map(|l| l.name.as_str())
        .unwrap_or("Unknown level");

    let mut svg = String::with_capacity(512 + walls.len() * 160);
    let _ = write!(
        svg,
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{min_x} {min_y} {width} {height}" width="{px_w}" height="{px_h}">
<title>{title}</title>
<rect x="{min_x}" y="{min_y}" width="{width}" height="{height}" fill="#f7f6f3"/>
<g transform="translate(0,{flip}) scale(1,-1)">
"##,
        min_x = fmt(min_x),
        min_y = fmt(min_y),
        width = fmt(width),
        height = fmt(height),
        // Cap the pixel size so a large plan does not produce an absurd canvas.
        px_w = fmt((width / 10.0).clamp(200.0, 2000.0)),
        px_h = fmt((height / 10.0).clamp(200.0, 2000.0)),
        title = escape(level_name),
        flip = fmt(2.0 * min_y + height),
    );

    if walls.is_empty() {
        svg.push_str("<!-- no walls on this level -->\n");
    }

    // Background images first, under everything (BEHAVIOR-SPEC §2.5).
    //
    // The Y flip is already applied by the enclosing group, and an <image>
    // would be drawn upside down by it — so each image carries a second flip
    // about its own box, which cancels the first.
    if let Some(level) = doc.level(level_id) {
        let mut images = level.background_images.clone();
        images.sort_by_key(|b| b.stack_ordinal);
        for image in images {
            let Some(bytes) = assets.get(&image.digest) else {
                continue;
            };
            let Some((w_px, h_px)) = png_dimensions(bytes) else {
                continue;
            };
            let mm = image.anchor.scale_uniform;
            if !mm.is_finite() || mm <= 0.0 {
                continue;
            }
            let (tx, ty) = (
                image.anchor.translation_mm[0],
                image.anchor.translation_mm[1],
            );
            let theta = image.anchor.rotation_deg;
            if !theta.is_finite() {
                continue;
            }
            // BEHAVIOR-SPEC §2.5's transform, emitted as SVG reads it right to
            // left: scale(s, −s) turns an image pixel (column, row) into
            // q = s·(column, −row); rotate(θ) is counter-clockwise in this
            // group's plan coordinates, which the enclosing scale(1,-1)
            // already established; translate(t) finishes p = t + s·R(θ)·q.
            //
            // This replaces a per-image flip about its own box. That flip was
            // the θ = 0 case of this transform written out, and it had nowhere
            // to put a rotation.
            let _ = writeln!(
                svg,
                r##"<g transform="translate({},{}) rotate({}) scale({},{})"><image x="0" y="0" width="{}" height="{}" href="data:image/png;base64,{}"/></g>"##,
                fmt(tx),
                fmt(ty),
                fmt(theta),
                fmt(mm),
                fmt(-mm),
                w_px,
                h_px,
                base64(bytes)
            );
        }
    }

    let (region, plan_rooms) = plan_on_level(doc, level_id);
    let projection = rooms::project_against(doc, level_id, region, plan_rooms);

    // Rooms first, underneath: the floor a person would stand on.
    for room in &domowik_geometry::architecture::cut_surfaces(
        doc,
        domowik_format::OpeningHost::LevelFloor(level_id),
        &projection.faces,
    ) {
        let _ = writeln!(
            svg,
            r##"<path d="{d}" fill-rule="evenodd" fill="#ffffff"/>"##,
            d = path_data(room)
        );
    }

    // Aperture tint first, BENEATH the wall mass: the wall mass below is the
    // drawn region with the cuts already taken out (R4.2.2), so a tint painted
    // over it would erase join patches and other walls' material.
    let footprint = |wall: &domowik_format::Wall, opening: &domowik_format::Opening| {
        let host = walls::WallSolid::new(wall.start(), wall.end(), wall.thickness_mm);
        Aperture::of(opening, 0, &host)?.footprint(&host)
    };
    for wall in &walls {
        for opening in doc.openings_on_wall(wall.id) {
            let Some(poly) = footprint(wall, opening) else {
                continue;
            };
            let points = poly
                .iter()
                .map(|p| format!("{},{}", fmt(p.x), fmt(p.y)))
                .collect::<Vec<_>>()
                .join(" ");
            let fill = match opening.kind {
                OpeningKind::Door => "#f7f6f3",
                OpeningKind::Window => "#bcd3e8",
            };
            let _ = writeln!(svg, r##"<polygon points="{points}" fill="{fill}"/>"##);
        }
    }

    // The wall mass: the drawn region (occupied region with opening cuts,
    // R4.2.2), one path per connected component, holes as subpaths under the
    // even-odd rule. Rooms above stay unamended (R4.2.5).
    // With no opening on the level the drawn region IS the occupied region
    // already built above: do not build the arrangement twice.
    let cut;
    let drawn = if level_has_openings(doc, level_id) {
        cut = level_drawn_region(doc, level_id);
        &cut
    } else {
        &projection.occupied
    };
    for component in drawn {
        let _ = writeln!(
            svg,
            r##"<path d="{d}" fill-rule="evenodd" fill="#3c3a36" stroke="#1b1a18" stroke-width="4"/>"##,
            d = path_data(component)
        );
    }

    // Architectural bodies and stair graphics share Rust-derived geometry.
    let (construction, construction_diagnostics) =
        domowik_geometry::architecture::plan_reporting(doc, level_id);
    {
        let font = label::Annotator::new().ok();
        for item in construction {
            let p = Polygon {
                outer: item.outline_mm.iter().copied().map(Point::from).collect(),
                holes: item
                    .holes_mm
                    .iter()
                    .map(|h| h.iter().copied().map(Point::from).collect())
                    .collect(),
            };
            let _ = writeln!(
                svg,
                r##"<path data-entity="{}" d="{}" fill-rule="evenodd" fill="#e6dfd2" stroke="#405363" stroke-width="12"><title>{}</title></path>"##,
                item.id,
                path_data(&p),
                escape(&item.name)
            );
            for [a, b] in &item.segments_mm {
                let _ = writeln!(
                    svg,
                    r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#405363" stroke-width="12"/>"##,
                    fmt(a[0]),
                    fmt(a[1]),
                    fmt(b[0]),
                    fmt(b[1])
                );
            }
            if !item.direction.is_empty() {
                if let Some((contours, _)) = font.as_ref().and_then(|f| {
                    f.text(
                        item.direction,
                        120.0,
                        item.label_mm[0],
                        item.label_mm[1] + 80.0,
                    )
                }) {
                    for contour in contours {
                        let p = Polygon::new(contour.into_iter().map(Point::from).collect());
                        let _ = writeln!(
                            svg,
                            r##"<path d="{}" fill="#243b53"><title>{}</title></path>"##,
                            path_data(&p),
                            item.direction
                        );
                    }
                }
            }
        }
    }

    // Door sills and swings over the wall mass.
    for wall in &walls {
        for opening in doc.openings_on_wall(wall.id) {
            if opening.kind != OpeningKind::Door {
                continue;
            }
            let Some((s0, s1)) = opening.span(wall.length_mm()) else {
                continue;
            };
            let a = lerp(wall.start(), wall.end(), s0.clamp(0.0, 1.0));
            let b = lerp(wall.start(), wall.end(), s1.clamp(0.0, 1.0));
            let _ = writeln!(
                svg,
                r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#8a6f3c" stroke-width="24"/>"##,
                fmt(a.x),
                fmt(a.y),
                fmt(b.x),
                fmt(b.y)
            );
            draw_swing(&mut svg, opening, a, b);
        }
    }

    // Room labels last, on top of everything (BEHAVIOR-SPEC R5.3.3).
    //
    // Glyph contours are plan-space polygons in millimetres with +Y up, and
    // the enclosing group already flips plan Y to screen Y — the same
    // transform the rooms and walls above go through — so they need no
    // handling of their own. That is the point of drawing text as outlines:
    // it is geometry, and every path in this file is treated alike.
    let labels = match label::labels_for(doc, &projection) {
        Ok(labels) => labels,
        // A font that will not parse is a build defect, and it must not take
        // the whole export with it: the plan is still worth having without its
        // labels, and the failure is reported rather than hidden.
        Err(error) => {
            let _ = writeln!(
                svg,
                "<!-- room labels unavailable: {} -->",
                escape(error.message())
            );
            label::Labels::default()
        }
    };
    for placed in &labels.placed {
        for line in &placed.lines {
            let mut d = String::new();
            for contour in &line.contours {
                for (i, p) in contour.iter().enumerate() {
                    let cmd = if i == 0 { 'M' } else { 'L' };
                    let _ = write!(d, "{cmd}{} {} ", fmt(p[0]), fmt(p[1]));
                }
                d.push_str("Z ");
            }
            // Non-zero, not even-odd: a glyph's counters wind against its
            // outer contour and both rules agree there, but a self-overlapping
            // outline gets spurious holes punched in it under even-odd.
            let _ = writeln!(
                svg,
                r##"<path d="{d}" fill-rule="nonzero" fill="#1b1a18"><title>{t}</title></path>"##,
                d = d.trim_end(),
                t = escape(&line.text)
            );
        }
    }

    svg.push_str("</g>\n</svg>\n");
    (svg, labels.warnings, construction_diagnostics)
}

/// Draws a door leaf and its swing arc from the R4.0.4 fields.
///
/// This exists because typed data nobody can see is data nobody can check.
/// `hinge`, `opensToward` and `swingDeg` were carried opaquely until
/// 2026-09-04 and therefore could not be drawn at all; a plan reader had no
/// way to tell a left-hand door from a right-hand one.
///
/// A closed leaf (`swingDeg == 0`) lies in the wall plane and is drawn without
/// an arc, which is exactly what R4.0.4 says zero means.
fn draw_swing(svg: &mut String, opening: &Opening, a: Point, b: Point) {
    let (hinge, tip) = domowik_geometry::walls::leaf_endpoints(opening, a, b);
    let free = match opening.hinge {
        Hinge::NearStart => b,
        Hinge::NearEnd => a,
    };
    let leaf = (free.x - hinge.x).hypot(free.y - hinge.y);
    if leaf <= 0.0 {
        return;
    }

    let _ = writeln!(
        svg,
        r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#8a6f3c" stroke-width="30"/>"##,
        fmt(hinge.x),
        fmt(hinge.y),
        fmt(tip.x),
        fmt(tip.y)
    );

    if opening.swing_deg <= 0.0 {
        return;
    }
    // The sweep follows the rotation actually applied, which is why it is
    // recomputed from the same two inputs rather than from `opensToward`.
    let positive = matches!(
        (opening.opens_toward, opening.hinge),
        (Side::Left, Hinge::NearStart) | (Side::Right, Hinge::NearEnd)
    );
    let sweep = if positive { 1 } else { 0 };
    let _ = writeln!(
        svg,
        r##"<path d="M {} {} A {} {} 0 0 {sweep} {} {}" fill="none" stroke="#b9a77e" stroke-width="12" stroke-dasharray="40 30"/>"##,
        fmt(free.x),
        fmt(free.y),
        fmt(leaf),
        fmt(leaf),
        fmt(tip.x),
        fmt(tip.y)
    );
}

/// An SVG path `d` attribute for a polygon and its holes.
///
/// Every ring is emitted as an independent closed subpath; the caller pairs
/// this with `fill-rule="evenodd"` so holes subtract regardless of the
/// direction each ring winds.
fn path_data(poly: &Polygon) -> String {
    let mut d = String::new();
    for ring in std::iter::once(&poly.outer).chain(poly.holes.iter()) {
        for (i, p) in ring.iter().enumerate() {
            let cmd = if i == 0 { 'M' } else { 'L' };
            let _ = write!(d, "{cmd}{} {} ", fmt(p.x), fmt(p.y));
        }
        d.push_str("Z ");
    }
    d.trim_end().to_owned()
}

/// The pixel dimensions of a PNG, read from its IHDR chunk.
///
/// Enough to lay the image out without decoding it: an SVG embeds the original
/// bytes, so the pixels themselves are the viewer's problem. The signature is
/// checked first, so a file that merely ends in `.png` is not measured.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (w > 0 && h > 0).then_some((w, h))
}

/// Standard base64 (RFC 4648 §4), for `data:` URIs.
///
/// Twenty lines rather than a dependency: the alphabet and the padding rule
/// are the whole specification, and a wrong implementation fails visibly on
/// the first image rather than subtly later.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

/// Formats a millimetre value without a trailing `.0`, so the output stays
/// compact and byte-identical across runs for the same input.
fn fmt(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v:.3}")
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::Wall;

    /// Every renderer fixture must be a VALID document.
    ///
    /// Several were not: they put a 200 mm-deep opening in a 120 mm wall, so
    /// the renderer was being exercised on documents the core would refuse
    /// (adversarial review, Codex gpt-5.6-sol, 2026-09-05). A test drawing
    /// something impossible proves nothing about drawing something real.
    fn assert_valid(doc: &Document) {
        assert!(
            doc.validate().is_empty(),
            "fixture is invalid: {:?}",
            doc.validate()
        );
    }

    fn doc_with_two_walls() -> (Document, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            120.0,
            2500.0,
        ));
        doc.walls.push(Wall::new(
            level,
            Point::new(4000.0, 0.0),
            Point::new(4000.0, 3000.0),
            120.0,
            2500.0,
        ));
        (doc, level)
    }

    /// A closed 4000 x 3000 shell with every corner joined.
    fn closed_room() -> Document {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let corners = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
        for i in 0..4 {
            let (x0, y0) = corners[i];
            let (x1, y1) = corners[(i + 1) % 4];
            doc.walls.push(Wall::new(
                level,
                Point::new(x0, y0),
                Point::new(x1, y1),
                200.0,
                2500.0,
            ));
        }
        for i in 0..4 {
            let a = doc.walls[i].id;
            let b = doc.walls[(i + 1) % 4].id;
            doc.joins.push(domowik_format::WallJoin::new(vec![
                domowik_format::JoinMember::new(a, domowik_format::Role::End),
                domowik_format::JoinMember::new(b, domowik_format::Role::Start),
            ]));
        }
        assert_eq!(doc.validate(), vec![]);
        doc
    }

    #[test]
    fn output_is_a_complete_svg_document() {
        let (doc, level) = doc_with_two_walls();
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert!(svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn walls_are_drawn_as_the_occupied_region_not_one_shape_each() {
        // The two walls in the fixture meet at (4000,0), so the region is one
        // connected component. Drawing a rectangle per wall would emit two.
        let (doc, level) = doc_with_two_walls();
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert_eq!(svg.matches("<path").count(), 1, "got {svg}");
        assert!(svg.contains(r##"fill="#3c3a36""##));
    }

    #[test]
    fn disjoint_walls_are_drawn_as_separate_components() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        ));
        doc.walls.push(Wall::new(
            level,
            Point::new(5000.0, 0.0),
            Point::new(6000.0, 0.0),
            200.0,
            2500.0,
        ));
        assert_eq!(render_level(&doc, level).matches("<path").count(), 2);
    }

    #[test]
    fn walls_on_other_levels_are_not_drawn() {
        let (mut doc, level) = doc_with_two_walls();
        let other = domowik_format::Level::new("Level 2");
        let other_id = other.id;
        doc.levels.push(other);
        // Far from the level-1 walls: were it drawn, it would be a second
        // component and the count would change.
        doc.walls.push(Wall::new(
            other_id,
            Point::new(50_000.0, 0.0),
            Point::new(51_000.0, 0.0),
            120.0,
            2500.0,
        ));
        assert_eq!(render_level(&doc, level).matches("<path").count(), 1);
    }

    #[test]
    fn a_room_is_filled_beneath_the_walls() {
        let doc = closed_room();
        let level = doc.levels[0].id;
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        // One room path plus one wall path.
        assert_eq!(svg.matches("<path").count(), 2, "got {svg}");
        let room_at = svg.find(r##"fill="#ffffff""##).expect("room fill");
        let wall_at = svg.find(r##"fill="#3c3a36""##).expect("wall fill");
        assert!(room_at < wall_at, "the room must be painted first");
    }

    #[test]
    fn a_wall_component_with_a_room_emits_the_hole_as_a_second_subpath() {
        let doc = closed_room();
        let svg = render_level(&doc, doc.levels[0].id);
        let wall_path = svg
            .lines()
            .find(|l| l.contains(r##"fill="#3c3a36""##))
            .expect("a wall path");
        assert_eq!(
            wall_path.matches('Z').count(),
            2,
            "outer ring and the room hole; got {wall_path}"
        );
        assert!(wall_path.contains(r##"fill-rule="evenodd""##));
    }

    #[test]
    fn a_door_cuts_the_wall_mass_with_its_tint_beneath_and_a_sill_line() {
        let (mut doc, level) = doc_with_two_walls();
        let wall = doc.walls[0].id;
        doc.openings.push(domowik_format::Opening::door(
            wall, 0.5, 900.0, 2100.0, 120.0,
        ));

        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert_eq!(
            svg.matches("<polygon").count(),
            1,
            "one aperture tint; got {svg}"
        );
        assert!(
            svg.contains(r##"<polygon points="##) && svg.contains(r##"fill="#f7f6f3"/>"##),
            "gap should read as background"
        );
        assert!(svg.contains("<line"), "a door should draw a sill line");
        // R4.2.2: the full-depth door cuts the wall mass into two components,
        // and the tint lies BENEATH it so it can never erase kept material.
        // Match the polygon, not the colour: the background <rect> uses the
        // same fill and appears at the top of every document.
        assert_eq!(svg.matches(r##"fill="#3c3a36""##).count(), 2, "cut in two");
        let wall_at = svg.find(r##"fill="#3c3a36""##).expect("wall fill");
        let gap_at = svg.find("<polygon").expect("aperture polygon");
        assert!(gap_at < wall_at, "tint beneath the wall mass");
        let sill_at = svg.find("<line").expect("sill");
        assert!(sill_at > wall_at, "sill over the wall mass");
    }

    #[test]
    fn a_window_is_drawn_as_a_band_the_full_wall_thickness() {
        let (mut doc, level) = doc_with_two_walls();
        let wall = doc.walls[0].id;
        doc.openings.push(domowik_format::Opening::window(
            wall, 0.5, 1200.0, 1200.0, 900.0, 120.0,
        ));
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert_eq!(svg.matches("<polygon").count(), 1, "got {svg}");
        assert!(svg.contains("#bcd3e8"), "window band should be drawn");
    }

    /// A door's hinge, swing and opening side were carried opaquely until
    /// 2026-09-04 and so could not be drawn. This is the check that they now
    /// reach the page.
    #[test]
    fn a_door_draws_its_leaf_and_swing_arc() {
        let (mut doc, level) = doc_with_two_walls();
        let wall = doc.walls[0].id;
        doc.openings.push(domowik_format::Opening::door(
            wall, 0.5, 900.0, 2100.0, 120.0,
        ));
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert!(
            svg.contains("<path d=\"M "),
            "a swung door draws an arc: {svg}"
        );
        assert!(
            svg.matches("stroke=\"#8a6f3c\"").count() >= 2,
            "sill line plus leaf: {svg}"
        );
    }

    /// R4.0.4: zero means closed, and a closed leaf sweeps nothing. Drawing an
    /// arc there would invent motion the data does not describe.
    #[test]
    fn a_closed_leaf_draws_no_arc() {
        let (mut doc, level) = doc_with_two_walls();
        let wall = doc.walls[0].id;
        let mut o = domowik_format::Opening::door(wall, 0.5, 900.0, 2100.0, 120.0);
        o.swing_deg = 0.0;
        doc.openings.push(o);
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert!(
            !svg.contains("<path d=\"M "),
            "no arc for a closed leaf: {svg}"
        );
    }

    /// The leaf must reach the half-plane `opensToward` names, FOR BOTH
    /// HINGES.
    ///
    /// "Left and Right render differently" was the original check and it was
    /// far too weak: it passed while every nearEnd door swung into the wrong
    /// half-plane. This asserts the geometry instead.
    #[test]
    fn the_leaf_enters_the_half_plane_it_says_it_does() {
        for hinge in [Hinge::NearStart, Hinge::NearEnd] {
            for toward in [Side::Left, Side::Right] {
                let (doc, _) = doc_with_two_walls();
                let wall = doc.walls[0].clone();
                let mut o = domowik_format::Opening::door(wall.id, 0.5, 900.0, 2100.0, 120.0);
                o.hinge = hinge;
                o.opens_toward = toward;
                o.swing_deg = 90.0;

                let (s0, s1) = o.span(wall.length_mm()).expect("aperture fits");
                let a = lerp(wall.start(), wall.end(), s0);
                let b = lerp(wall.start(), wall.end(), s1);
                let (hinge_pt, tip) = domowik_geometry::walls::leaf_endpoints(&o, a, b);

                // n is the wall's LEFT normal: the unit axis turned +90 deg.
                let ax = wall.end().x - wall.start().x;
                let ay = wall.end().y - wall.start().y;
                let len = ax.hypot(ay);
                let (nx, ny) = (-ay / len, ax / len);
                let reach = (tip.x - hinge_pt.x) * nx + (tip.y - hinge_pt.y) * ny;

                assert_eq!(
                    reach > 0.0,
                    toward == Side::Left,
                    "hinge {hinge:?} toward {toward:?}: reached {reach} along n"
                );
                let leaf = (tip.x - hinge_pt.x).hypot(tip.y - hinge_pt.y);
                assert!(
                    (leaf - 900.0).abs() < 1.0,
                    "leaf was {leaf} mm, expected the 900 mm aperture width"
                );
            }
        }
    }

    #[test]
    fn two_openings_on_one_wall_paint_two_apertures() {
        let (mut doc, level) = doc_with_two_walls();
        let wall = doc.walls[0].id;
        doc.openings.push(domowik_format::Opening::door(
            wall, 0.25, 600.0, 2100.0, 120.0,
        ));
        doc.openings.push(domowik_format::Opening::door(
            wall, 0.75, 600.0, 2100.0, 120.0,
        ));
        assert_valid(&doc);
        let svg = render_level(&doc, level);
        assert_eq!(svg.matches("<polygon").count(), 2, "got {svg}");
    }

    #[test]
    fn rendering_is_deterministic_for_the_same_document() {
        let (doc, level) = doc_with_two_walls();
        assert_eq!(render_level(&doc, level), render_level(&doc, level));
    }

    #[test]
    fn an_empty_level_still_produces_valid_svg() {
        let doc = Document::with_ground_floor();
        let svg = render_level(&doc, doc.levels[0].id);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("no walls"));
        assert_eq!(svg.matches("<polygon").count(), 0);
    }

    #[test]
    fn a_level_name_with_markup_characters_is_escaped() {
        let mut doc = Document::new();
        let mut level = domowik_format::Level::new("A & B <test>");
        level.name = "A & B <test>".to_owned();
        let id = level.id;
        doc.levels.push(level);
        let svg = render_level(&doc, id);
        assert!(svg.contains("A &amp; B &lt;test&gt;"), "got {svg}");
        assert!(!svg.contains("<test>"));
    }

    #[test]
    fn base64_matches_the_rfc_4648_test_vectors() {
        // RFC 4648 §10. Padding is where a hand-written encoder goes wrong,
        // and these vectors cover all three residues.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn png_dimensions_reads_the_ihdr_and_rejects_anything_else() {
        let mut pixmap = tiny_skia::Pixmap::new(7, 3).expect("7x3");
        pixmap.fill(tiny_skia::Color::WHITE);
        let bytes = pixmap.encode_png().expect("encodes");
        assert_eq!(png_dimensions(&bytes), Some((7, 3)));

        assert_eq!(png_dimensions(b"not a png at all really"), None);
        assert_eq!(png_dimensions(&[]), None);
        // A correct signature with a truncated header must not be measured.
        assert_eq!(png_dimensions(&bytes[..20]), None);
    }

    #[test]
    fn a_background_image_is_embedded_as_a_data_uri() {
        // Embedded rather than referenced: an SVG pointing at a neighbouring
        // file stops working the moment either moves, and handing the plan to
        // someone else is what this export is for.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, -2000.0),
            Point::new(2000.0, -2000.0),
            200.0,
            2500.0,
        ));
        let mut pixmap = tiny_skia::Pixmap::new(4, 2).expect("4x2");
        pixmap.fill(tiny_skia::Color::from_rgba8(0, 128, 255, 255));
        let bytes = pixmap.encode_png().expect("encodes");
        let digest = domowik_container::digest_of(&bytes);
        doc.levels[0]
            .background_images
            .push(domowik_format::BackgroundImage {
                unknown: Default::default(),
                digest: digest.clone(),
                anchor: domowik_format::Anchor {
                    translation_mm: [0.0, 0.0],
                    scale_uniform: 500.0,
                    rotation_deg: 0.0,
                    unknown: Default::default(),
                },
                stack_ordinal: 0,
            });
        let mut assets = BTreeMap::new();
        assets.insert(digest, bytes.clone());

        let svg = render_level_with_assets(&doc, level, &assets);
        assert!(svg.contains("<image "), "got {svg}");
        assert!(svg.contains("data:image/png;base64,"), "got {svg}");
        assert!(svg.contains(&base64(&bytes)), "the original bytes");
        // 4 x 2 PIXELS, with the millimetres-per-pixel scale carried by the
        // transform. BEHAVIOR-SPEC §2.5's anchor is a transform on image
        // points, so emitting it as one keeps the rotation and the scale in
        // the same place instead of baking one into the element's size.
        assert!(
            svg.contains(r#"transform="translate(0,0) rotate(0) scale(500,-500)""#),
            "got {svg}"
        );
        assert!(svg.contains(r#"width="4" height="2""#), "got {svg}");
        // The image is drawn BEFORE the walls, so the plan sits on top.
        let image_at = svg.find("<image").expect("image");
        let wall_at = svg.find(r##"fill="#3c3a36""##).expect("wall");
        assert!(image_at < wall_at);
    }

    #[test]
    fn the_svg_extent_covers_a_background_larger_than_the_walls() {
        // Parity with the PNG export: sizing to the walls alone put the image
        // outside the viewBox, which looks exactly like it failing to load.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, -100.0),
            Point::new(200.0, -100.0),
            20.0,
            2500.0,
        ));
        let mut pixmap = tiny_skia::Pixmap::new(10, 10).expect("10x10");
        pixmap.fill(tiny_skia::Color::WHITE);
        let bytes = pixmap.encode_png().expect("encodes");
        let digest = domowik_container::digest_of(&bytes);
        doc.levels[0]
            .background_images
            .push(domowik_format::BackgroundImage {
                unknown: Default::default(),
                digest: digest.clone(),
                anchor: domowik_format::Anchor {
                    translation_mm: [0.0, 0.0],
                    scale_uniform: 1000.0,
                    rotation_deg: 0.0,
                    unknown: Default::default(),
                },
                stack_ordinal: 0,
            });
        let mut assets = BTreeMap::new();
        assets.insert(digest, bytes);

        let svg = render_level_with_assets(&doc, level, &assets);
        // The image spans 10,000 mm; the walls span 200. The viewBox must
        // reach the image.
        let view = svg
            .split(r#"viewBox=""#)
            .nth(1)
            .and_then(|s| s.split('"').next())
            .expect("a viewBox");
        let parts: Vec<f64> = view
            .split_whitespace()
            .map(|v| v.parse().unwrap())
            .collect();
        assert!(parts[2] >= 10_000.0, "width {} too small: {view}", parts[2]);
    }

    #[test]
    fn a_background_whose_blob_is_missing_leaves_the_svg_intact() {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        ));
        doc.levels[0]
            .background_images
            .push(domowik_format::BackgroundImage {
                unknown: Default::default(),
                digest: "sha256:".to_owned() + &"a".repeat(64),
                anchor: Default::default(),
                stack_ordinal: 0,
            });
        let svg = render_level_with_assets(&doc, level, &BTreeMap::new());
        assert!(svg.starts_with("<svg"));
        assert!(!svg.contains("<image"), "nothing to draw");
    }
}
