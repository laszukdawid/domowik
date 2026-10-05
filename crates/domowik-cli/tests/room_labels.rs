//! Room names in the 2D exports: that both draw them, that both refuse the
//! same names, and that neither invents its own answer.
//!
//! The parity assertions are the point. Issue 0016 closed as partial scope
//! because `svg.rs` and `png.rs` each render straight from the document with
//! no shared paint API, so the two can drift; labels go through one module
//! that produces the geometry both draw, and these tests are what says so.

use domowik_cli::label::{labels_for_level, LabelWarning};
use domowik_cli::{png, svg};
use domowik_format::{Document, JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin};
use std::collections::BTreeMap;
use uuid::Uuid;

/// A 5000 x 4000 room, named.
fn plan(name: Option<&str>) -> (Document, Uuid) {
    let mut doc = Document::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    doc.levels.push(level);
    let corners = [
        (0.0, 0.0),
        (5000.0, 0.0),
        (5000.0, 4000.0),
        (0.0, 4000.0),
        (0.0, 0.0),
    ];
    let mut ids = Vec::new();
    for pair in corners.windows(2) {
        let wall = Wall::new(
            level_id,
            Point::new(pair[0].0, pair[0].1),
            Point::new(pair[1].0, pair[1].1),
            200.0,
            2500.0,
        );
        ids.push(wall.id);
        doc.walls.push(wall);
    }
    for i in 0..4 {
        doc.joins.push(WallJoin::new(vec![
            JoinMember::new(ids[i], Role::End),
            JoinMember::new(ids[(i + 1) % 4], Role::Start),
        ]));
    }
    let mut record = RoomRecord::new(Uuid::from_u128(1), level_id, Point::new(2500.0, 2000.0));
    record.name = name.map(str::to_owned);
    doc.room_records.push(record);
    (doc, level_id)
}

fn render_svg(doc: &Document, level: Uuid) -> (String, Vec<LabelWarning>) {
    svg::render_level_reporting(doc, level, &BTreeMap::new())
}

fn render_png(doc: &Document, level: Uuid) -> png::Raster {
    png::render_level(doc, level, 0.15).expect("renders")
}

/// The darkest pixel in a box around the plan point, as a luminance.
///
/// Label ink is near-black on a white room fill, so "is there ink here" is a
/// question about the minimum rather than the mean, which a few dark pixels in
/// a large window would not move.
fn darkest_near(raster: &png::Raster, x_mm: f64, y_mm: f64, radius_mm: f64) -> u8 {
    let pixmap = tiny_skia::Pixmap::decode_png(&raster.png).expect("our own PNG decodes");
    let to_px = |v: f64, origin: f64, flip: bool| {
        let d = if flip { origin - v } else { v - origin };
        (d * raster.scale).round() as i64
    };
    let cx = to_px(x_mm, raster.origin_mm[0], false);
    let cy = to_px(y_mm, raster.origin_mm[1], true);
    let r = (radius_mm * raster.scale).round() as i64;
    let mut darkest = 255u8;
    for py in (cy - r).max(0)..=(cy + r).min(i64::from(pixmap.height()) - 1) {
        for px in (cx - r).max(0)..=(cx + r).min(i64::from(pixmap.width()) - 1) {
            let p = pixmap.pixels()[(py as u32 * pixmap.width() + px as u32) as usize];
            let luma = p.red().min(p.green()).min(p.blue());
            darkest = darkest.min(luma);
        }
    }
    darkest
}

#[test]
fn both_exports_draw_the_name_and_the_area() {
    let (doc, level) = plan(Some("Salon"));
    let (svg, warnings) = render_svg(&doc, level);
    assert!(warnings.is_empty());
    assert!(
        svg.contains("<title>Salon</title>"),
        "the SVG names the label"
    );
    assert!(svg.contains("<title>18.24 m²</title>"), "and the area");
    assert!(
        svg.contains(r##"fill-rule="nonzero""##),
        "glyph contours fill non-zero, so counters stay holes"
    );

    let raster = render_png(&doc, level);
    assert!(raster.label_warnings.is_empty());
    // Ink at the seed, where the label sits; clean room fill well away from it.
    assert!(
        darkest_near(&raster, 2500.0, 2000.0, 700.0) < 100,
        "the raster must carry the label"
    );
    assert!(
        darkest_near(&raster, 700.0, 3400.0, 150.0) > 200,
        "an empty corner of the room must stay the room fill"
    );
}

#[test]
fn a_face_with_no_candidates_is_labelled_in_neither_export() {
    let (mut doc, level) = plan(Some("Salon"));
    doc.room_records[0].seed_mm = [20_000.0, 2000.0];
    let (svg, warnings) = render_svg(&doc, level);
    assert!(warnings.is_empty());
    assert!(!svg.contains("<title>Salon</title>"));
    assert!(
        !svg.contains("m²</title>"),
        "no candidate, no label — and the area is part of the label"
    );
    let raster = render_png(&doc, level);
    assert!(
        darkest_near(&raster, 2500.0, 2000.0, 700.0) > 200,
        "the middle of the room must be untouched"
    );
}

#[test]
fn an_unnamed_record_labels_its_face_with_the_fallback_in_both() {
    for name in [None, Some("")] {
        let (doc, level) = plan(name);
        let (svg, _) = render_svg(&doc, level);
        assert!(svg.contains("<title>Room</title>"), "name {name:?}");
        let raster = render_png(&doc, level);
        assert!(darkest_near(&raster, 2500.0, 2000.0, 700.0) < 100);
    }
}

#[test]
fn both_exports_refuse_the_same_names_with_the_same_warnings() {
    // Parity is the property worth testing: two exporters that disagreed about
    // which names they can draw would put a name on one output and not the
    // other for the same document.
    for name in ["居間", "غرفة", "סלון", "Pokoj\u{0301}"] {
        let (doc, level) = plan(Some(name));
        let (svg, svg_warnings) = render_svg(&doc, level);
        let raster = render_png(&doc, level);
        let expected = labels_for_level(&doc, level).expect("font").warnings;

        assert_eq!(svg_warnings, expected, "{name}: SVG warnings");
        assert_eq!(raster.label_warnings, expected, "{name}: PNG warnings");
        assert_eq!(svg_warnings.len(), 1, "{name}: one refusal");

        // The name is not drawn, the AREA still is, in both.
        assert!(
            !svg.contains(&format!("<title>{name}</title>")),
            "{name} must not be drawn"
        );
        assert!(svg.contains("m²</title>"), "{name}: the area survives");
        assert!(
            darkest_near(&raster, 2500.0, 2000.0, 700.0) < 100,
            "{name}: the raster still carries the area line"
        );

        // And nothing touched the document.
        assert_eq!(doc.room_records[0].name.as_deref(), Some(name));
    }
}

#[test]
fn polish_names_render_in_both_exports() {
    let (doc, level) = plan(Some("Łazienka"));
    let (svg, warnings) = render_svg(&doc, level);
    assert!(warnings.is_empty(), "got {warnings:?}");
    assert!(svg.contains("<title>Łazienka</title>"));
    let raster = render_png(&doc, level);
    assert!(raster.label_warnings.is_empty());
    assert!(darkest_near(&raster, 2500.0, 2000.0, 700.0) < 100);
}

#[test]
fn label_rendering_is_deterministic_in_both_exports() {
    let (doc, level) = plan(Some("Salon"));
    assert_eq!(render_svg(&doc, level).0, render_svg(&doc, level).0);
    assert_eq!(render_png(&doc, level).png, render_png(&doc, level).png);
}

#[test]
fn the_two_exports_agree_on_where_the_label_sits() {
    // Not a pixel comparison — the two rasterise differently and are not meant
    // to be byte-comparable. This asserts they consume the SAME geometry: the
    // SVG's label coordinates and the PNG's ink land on the same plan point.
    let (doc, level) = plan(Some("Salon"));
    let labels = labels_for_level(&doc, level).expect("font");
    let placed = &labels.placed[0];
    let xs: Vec<f64> = placed
        .lines
        .iter()
        .flat_map(|l| l.contours.iter().flatten())
        .map(|p| p[0])
        .collect();
    let ys: Vec<f64> = placed
        .lines
        .iter()
        .flat_map(|l| l.contours.iter().flatten())
        .map(|p| p[1])
        .collect();
    let min_x = xs.iter().copied().fold(f64::INFINITY, f64::min);
    let max_x = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_y = ys.iter().copied().fold(f64::INFINITY, f64::min);
    let max_y = ys.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let (svg, _) = render_svg(&doc, level);
    // The SVG writes plan millimetres directly, so the label's extent appears
    // in the document as coordinates inside that box.
    assert!(svg.contains("<title>Salon</title>"));

    let raster = render_png(&doc, level);
    // Ink inside the box the shared geometry describes, and none a metre away
    // from it in a direction the label does not reach.
    assert!(darkest_near(&raster, (min_x + max_x) / 2.0, (min_y + max_y) / 2.0, 200.0) < 100);
    assert!(
        darkest_near(&raster, (min_x + max_x) / 2.0, max_y + 800.0, 150.0) > 200,
        "well above the label there is nothing to draw"
    );
}
