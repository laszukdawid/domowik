//! PNG export (issue 0017).
//!
//! Rasterises the same semantic paint model [`crate::svg`] renders, so the two
//! exports show the same plan. Issue 0017 puts "a new internal anti-aliased
//! renderer" out of scope, so the rasterisation is `tiny-skia`'s: correct
//! anti-aliasing, path filling and stroking are a large amount of subtle code
//! with no Domowik-specific content in it.
//!
//! # Explicit output contract (issue 0017's first criterion)
//!
//! - **Pixel dimensions.** Derived from the plan extent and `scale`, in pixels
//!   per millimetre, and clamped by [`MAX_DIMENSION`] and [`MAX_PIXELS`].
//! - **DPI.** Not written. A PNG's `pHYs` chunk carries physical resolution,
//!   and stating one would be a claim about print size that this exporter has
//!   no basis for. `scale` is the honest control and it is reported to the
//!   caller.
//! - **Colour space.** sRGB, untagged. No colour-management chunk is written,
//!   so a viewer applies its default, which for untagged PNG is sRGB.
//! - **Alpha.** Premultiplied internally by tiny-skia, unpremultiplied on
//!   encode. The background is painted opaque, so the output has no
//!   transparency to get wrong.
//! - **Antialiasing.** On, analytic, from tiny-skia.
//! - **Metadata.** None beyond what the encoder writes. No timestamp, no
//!   generator string — both would make byte-identical output impossible for
//!   identical input.
//! - **Encoder settings.** `png` crate defaults through
//!   `tiny_skia::Pixmap::encode_png`: 8-bit RGBA, non-interlaced, deflate.
//!
//! # Determinism
//!
//! **Pixel equality is the target; encoded-byte equality is a consequence, not
//! a promise.** The same document at the same scale produces the same pixels,
//! and a test asserts it. Byte equality additionally requires the encoder to
//! be deterministic and its version to be pinned — true today for the pinned
//! `png` version, and not something this module can guarantee across an
//! upgrade. A test asserts byte equality within one build, which is what makes
//! a regression visible, and the distinction is recorded rather than blurred.
//!
//! # Fonts
//!
//! ONE, pinned and embedded: Noto Sans Regular 2.015 under the SIL Open Font
//! License 1.1, recorded in `docs/third-party/assets.json` and `NOTICE`. It
//! arrived with room labels (BEHAVIOR-SPEC §5.3, issue 0060), which this
//! module said would make a font issue 0017's problem again.
//!
//! No shaping engine is linked, and that limit is enforced rather than left
//! implicit: [`crate::label`] draws glyph OUTLINES from the font's `cmap` for
//! a fixed repertoire of left-to-right, non-combining scripts, and REFUSES any
//! name outside it with a warning instead of drawing it incorrectly. The
//! outlines are the same ones the SVG export draws, so the two agree by
//! construction rather than by review.

use crate::label::{self, LabelWarning};
use crate::{plan_on_level, CliError};
use domowik_format::{Document, OpeningKind, Point};
use domowik_geometry::polygon::Polygon;
use domowik_geometry::rooms;
use domowik_geometry::wall_polygon;
use std::collections::BTreeMap;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};
use uuid::Uuid;

/// Margin around the drawing, in millimetres. Matches the SVG export.
const MARGIN_MM: f64 = 500.0;

/// Largest side length this exporter will produce, in pixels.
///
/// Issue 0017 requires maximum dimensions and memory use to be BOUNDED. A
/// bound stated in pixels is the one a user can reason about; the memory it
/// implies is four bytes per pixel.
pub const MAX_DIMENSION: u32 = 16_384;

/// Largest total pixel count, which is the real memory bound.
///
/// 64 megapixels is 256 MiB of RGBA. Two dimensions each under
/// [`MAX_DIMENSION`] can still multiply to far more than any machine should
/// allocate for a floor plan, so the product is bounded separately rather than
/// left implied by the sides.
pub const MAX_PIXELS: u64 = 64 << 20;

/// Colours, shared with the SVG export so the two agree.
///
/// As RGB triples rather than `Color` constants: `Color::from_rgba8` is not a
/// const function, and a `LazyLock` for six opaque colours would be more
/// machinery than the thing it holds.
const BACKGROUND: (u8, u8, u8) = (0xf7, 0xf6, 0xf3);
const ROOM_FILL: (u8, u8, u8) = (0xff, 0xff, 0xff);
const WALL_FILL: (u8, u8, u8) = (0x3c, 0x3a, 0x36);
const WALL_STROKE: (u8, u8, u8) = (0x1b, 0x1a, 0x18);
const DOOR_SILL: (u8, u8, u8) = (0x8a, 0x6f, 0x3c);
const WINDOW_FILL: (u8, u8, u8) = (0xbc, 0xd3, 0xe8);
/// Room label ink. The same value the SVG export writes for its label paths.
const LABEL_INK: (u8, u8, u8) = (0x1b, 0x1a, 0x18);

fn rgb((r, g, b): (u8, u8, u8)) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

/// What a render produced, so the caller can report it rather than guess.
#[derive(Debug)]
pub struct Raster {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Pixels per millimetre actually used, after clamping.
    pub scale: f64,
    /// The plan point that output pixel `(0, 0)` shows.
    ///
    /// Reported because the extent is derived from the content — walls plus
    /// any background image — so a caller cannot reconstruct the mapping from
    /// the document alone. With this and `scale`, plan `(x, y)` is at
    /// `((x - origin.x) * scale, (origin.y - y) * scale)`.
    pub origin_mm: [f64; 2],
    /// Room names this export declined to draw, and why.
    ///
    /// Part of the result rather than a log line: an export missing a label is
    /// invisible to whoever receives the file, so the caller has to be able to
    /// say it happened.
    pub label_warnings: Vec<LabelWarning>,
}

/// Renders `level_id` to a PNG at `scale` pixels per millimetre.
///
/// The scale is REDUCED when it would exceed the bounds, rather than the
/// render being refused: a user who asks for too much detail wants a picture,
/// not an error, and the scale actually used is returned so the caller can say
/// what happened.
pub fn render_level(doc: &Document, level_id: Uuid, scale: f64) -> Result<Raster, CliError> {
    render_level_with_assets(doc, level_id, scale, &BTreeMap::new())
}

/// [`render_level`], with the blob store available so background images can be
/// drawn beneath the plan.
///
/// A background whose blob is absent, or whose bytes this build cannot decode,
/// is SKIPPED rather than failing the render: a missing scan should not stop a
/// user exporting their walls, and FORMAT-SPEC §6 requires safe behaviour when
/// optional assets are missing. Only PNG is decodable here, because tiny-skia
/// decodes only PNG — a JPEG scan imports and stores fine and simply does not
/// appear, which is a limitation worth stating rather than a silent success.
pub fn render_level_with_assets(
    doc: &Document,
    level_id: Uuid,
    scale: f64,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Result<Raster, CliError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(CliError::Usage(format!(
            "scale must be a positive number of pixels per millimetre, got {scale}"
        )));
    }

    // The extent covers the walls AND any background image placed on this
    // level. Sizing to the walls alone means exporting a plan with a scan
    // under it shows no scan — the window simply does not reach it, which
    // looks exactly like the image failing to load.
    let (mut min, mut max) = doc
        .bounds()
        .unwrap_or((Point::new(0.0, 0.0), Point::new(1000.0, 1000.0)));
    for (a, b) in background_extents(doc, level_id, assets) {
        min = Point::new(min.x.min(a.x), min.y.min(a.y));
        max = Point::new(max.x.max(b.x), max.y.max(b.y));
    }
    let plan_w = (max.x - min.x) + 2.0 * MARGIN_MM;
    let plan_h = (max.y - min.y) + 2.0 * MARGIN_MM;
    let scale = clamp_scale(scale, plan_w, plan_h);

    // FLOOR, not ceil. Rounding each side up can push the product past
    // MAX_PIXELS even when the scale that produced them was exactly at the
    // bound — 9116 x 7363 is 67.1 million against a 67.108 million cap. The
    // fraction of a pixel lost is inside the 500 mm margin.
    let width = ((plan_w * scale).floor() as u32).max(1);
    let height = ((plan_h * scale).floor() as u32).max(1);
    debug_assert!(width <= MAX_DIMENSION && height <= MAX_DIMENSION);
    debug_assert!(u64::from(width) * u64::from(height) <= MAX_PIXELS);
    let mut pixmap = Pixmap::new(width, height)
        .ok_or_else(|| CliError::Failure(format!("could not allocate a {width}x{height} image")))?;
    pixmap.fill(rgb(BACKGROUND));

    // Plan millimetres to pixels. The plan plane is +Y up (BEHAVIOR-SPEC
    // §1.1) and an image's Y axis points down, so the transform flips Y
    // rather than every coordinate being negated at its use site.
    let ox = min.x - MARGIN_MM;
    let oy = max.y + MARGIN_MM;
    let to_px =
        |p: Point| -> (f32, f32) { (((p.x - ox) * scale) as f32, ((oy - p.y) * scale) as f32) };

    // Background images first, under everything (BEHAVIOR-SPEC §2.5), in
    // stack order.
    if let Some(level) = doc.level(level_id) {
        let mut images = level.background_images.clone();
        images.sort_by_key(|b| b.stack_ordinal);
        for image in images {
            // Two guards reach the same safe outcome, and this one is the
            // redundant half: removing it leaves an absent blob as empty
            // bytes, which the decode below then rejects. Kept because
            // "the blob is not here" and "the bytes are not a PNG" are
            // different facts, and a future change that makes decoding
            // tolerant of empty input would silently lose the first.
            let Some(bytes) = assets.get(&image.digest) else {
                continue;
            };
            let Some(source) = Pixmap::decode_png(bytes).ok() else {
                continue;
            };
            // The anchor is millimetres per image pixel, plus a translation in
            // plan millimetres; combined with the plan-to-pixel scale it gives
            // image pixels to output pixels.
            let mm_per_px = image.anchor.scale_uniform;
            if !mm_per_px.is_finite() || mm_per_px <= 0.0 {
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
            // BEHAVIOR-SPEC §2.5: p = t + s·R(θ)·q, θ in degrees
            // COUNTER-CLOCKWISE in plan. See `anchor_to_plan` for what q is
            // and why.
            //
            // The output pixmap's Y runs downward and plan Y runs up, and
            // composing that flip with the one inside q turns a plan-CCW
            // rotation into a pixel-space rotation by −θ. Derived rather than
            // guessed: M·R(θ)·M = R(−θ) for M = diag(1, −1).
            let (px, py) = to_px(Point::new(tx, ty));
            let k = (mm_per_px * scale) as f32;
            let transform = Transform::from_translate(px, py)
                .pre_rotate(-theta as f32)
                .pre_scale(k, k);
            let paint = tiny_skia::PixmapPaint::default();
            pixmap.draw_pixmap(0, 0, source.as_ref(), &paint, transform, None);
        }
    }

    let (region, faces) = plan_on_level(doc, level_id);
    let projection = rooms::project_against(doc, level_id, region, faces);

    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };

    // Rooms first, underneath: the floor a person would stand on.
    for room in &projection.faces {
        if let Some(path) = polygon_path(room, &to_px) {
            paint.set_color(rgb(ROOM_FILL));
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
        }
    }

    // The wall mass: one path per connected component, holes as subpaths under
    // the even-odd rule.
    let stroke = Stroke {
        width: (4.0 * scale) as f32,
        ..Stroke::default()
    };
    for component in &projection.occupied {
        if let Some(path) = polygon_path(component, &to_px) {
            paint.set_color(rgb(WALL_FILL));
            pixmap.fill_path(
                &path,
                &paint,
                FillRule::EvenOdd,
                Transform::identity(),
                None,
            );
            if stroke.width > 0.0 {
                paint.set_color(rgb(WALL_STROKE));
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
    }

    // Apertures on top. The occupied region deliberately includes the wall
    // under an opening — R5.1.5 keeps openings out of enclosure entirely — so
    // the gap is painted, not cut.
    for wall in doc.walls_on_level(level_id) {
        for opening in doc.openings_on_wall(wall.id) {
            let Some((s0, s1)) = opening.span(wall.length_mm()) else {
                continue;
            };
            let a = lerp(wall.start(), wall.end(), s0.clamp(0.0, 1.0));
            let b = lerp(wall.start(), wall.end(), s1.clamp(0.0, 1.0));
            let Some(rect) = wall_polygon(a, b, wall.thickness_mm) else {
                continue;
            };
            let Some(path) = ring_path(&rect, &to_px) else {
                continue;
            };
            match opening.kind {
                OpeningKind::Door => {
                    paint.set_color(rgb(BACKGROUND));
                    pixmap.fill_path(
                        &path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                    // A sill line across the gap, so a door reads as a door.
                    let mut sill = PathBuilder::new();
                    let (ax, ay) = to_px(a);
                    let (bx, by) = to_px(b);
                    sill.move_to(ax, ay);
                    sill.line_to(bx, by);
                    if let Some(line) = sill.finish() {
                        paint.set_color(rgb(DOOR_SILL));
                        pixmap.stroke_path(
                            &line,
                            &paint,
                            &Stroke {
                                width: (24.0 * scale) as f32,
                                ..Stroke::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }

                    // The leaf, in the direction R4.0.4 says it opens.
                    //
                    // SVG has drawn this since the opening fields became
                    // typed; PNG did not, so the two exports disagreed about
                    // what the same document looks like. An export that omits
                    // data the other shows is not a different style, it is a
                    // different answer.
                    let (hinge, tip) = domowik_geometry::walls::leaf_endpoints(opening, a, b);
                    let mut leaf = PathBuilder::new();
                    let (hx, hy) = to_px(hinge);
                    let (tx, ty) = to_px(tip);
                    leaf.move_to(hx, hy);
                    leaf.line_to(tx, ty);
                    if let Some(line) = leaf.finish() {
                        paint.set_color(rgb(DOOR_SILL));
                        pixmap.stroke_path(
                            &line,
                            &paint,
                            &Stroke {
                                width: (30.0 * scale) as f32,
                                ..Stroke::default()
                            },
                            Transform::identity(),
                            None,
                        );
                    }
                }
                OpeningKind::Window => {
                    paint.set_color(rgb(WINDOW_FILL));
                    pixmap.fill_path(
                        &path,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        None,
                    );
                }
            }
        }
    }

    // Room labels last, on top (BEHAVIOR-SPEC R5.3.3). The contours come from
    // the SAME module the SVG export uses, so the two agree on what a label
    // says and where it sits by construction; only the ink differs, and only
    // because one writes a hex string and the other an RGBA colour.
    let labels = label::labels_for(doc, &projection)?;
    paint.set_color(rgb(LABEL_INK));
    for placed in &labels.placed {
        for line in &placed.lines {
            let mut builder = PathBuilder::new();
            for contour in &line.contours {
                let mut points = contour.iter();
                let Some(first) = points.next() else { continue };
                let (x, y) = to_px(Point::new(first[0], first[1]));
                builder.move_to(x, y);
                for p in points {
                    let (x, y) = to_px(Point::new(p[0], p[1]));
                    builder.line_to(x, y);
                }
                builder.close();
            }
            if let Some(path) = builder.finish() {
                // Winding, not even-odd: a glyph's counters wind against its
                // outer contour and both rules agree there, but a
                // self-overlapping outline gets spurious holes under even-odd.
                pixmap.fill_path(
                    &path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    let png = pixmap
        .encode_png()
        .map_err(|e| CliError::Failure(format!("could not encode PNG: {e}")))?;
    Ok(Raster {
        png,
        width,
        height,
        scale,
        origin_mm: [ox, oy],
        label_warnings: labels.warnings,
    })
}

/// BEHAVIOR-SPEC §2.5's anchor transform, written as the specification writes
/// it: `p = t + s·R(θ)·q`, with `R(θ) = [[cosθ, −sinθ], [sinθ, cosθ]]` and θ
/// in degrees counter-clockwise.
///
/// `q` is the image point `(column, −row)`. §2.5 does not say which way an
/// image's own Y axis runs, and its worked example has `q_y = 0`, so the
/// example does not decide it. This is the reading under which θ = 0 places
/// the image by its top-left corner and grows it downward in plan — what
/// every document written so far assumes, and what the un-rotated renderer
/// already did.
pub fn anchor_to_plan(tx: f64, ty: f64, s: f64, theta_deg: f64, col: f64, row: f64) -> Point {
    let (sin, cos) = theta_deg.to_radians().sin_cos();
    let (qx, qy) = (col, -row);
    Point::new(
        tx + s * (cos * qx - sin * qy),
        ty + s * (sin * qx + cos * qy),
    )
}

/// The plan extent of each decodable background image on `level_id`.
///
/// Requires decoding, because an image's plan size is its PIXEL size times its
/// anchor scale and nothing records the pixel size separately. An image that
/// does not decode contributes no extent, which is consistent with it not
/// being drawn.
pub(crate) fn background_extents(
    doc: &Document,
    level_id: Uuid,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Vec<(Point, Point)> {
    let Some(level) = doc.level(level_id) else {
        return Vec::new();
    };
    level
        .background_images
        .iter()
        .filter_map(|image| {
            let mm = image.anchor.scale_uniform;
            if !mm.is_finite() || mm <= 0.0 {
                return None;
            }
            let theta = image.anchor.rotation_deg;
            if !theta.is_finite() {
                return None;
            }
            let source = Pixmap::decode_png(assets.get(&image.digest)?).ok()?;
            let (tx, ty) = (
                image.anchor.translation_mm[0],
                image.anchor.translation_mm[1],
            );
            // The AXIS-ALIGNED bound of the PLACED image. A rotated rectangle
            // is not its own bounding box, and sizing an export to the
            // unrotated one crops a rotated scan — which looks exactly like
            // the image having failed to load.
            let (w, h) = (f64::from(source.width()), f64::from(source.height()));
            let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
                .map(|(col, row)| anchor_to_plan(tx, ty, mm, theta, col, row));
            let min_x = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
            let max_x = corners
                .iter()
                .map(|p| p.x)
                .fold(f64::NEG_INFINITY, f64::max);
            let min_y = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
            let max_y = corners
                .iter()
                .map(|p| p.y)
                .fold(f64::NEG_INFINITY, f64::max);
            Some((Point::new(min_x, min_y), Point::new(max_x, max_y)))
        })
        .collect()
}

/// Reduces `scale` until the image fits both bounds.
///
/// Returns the largest scale at or below the request that satisfies
/// [`MAX_DIMENSION`] on each side and [`MAX_PIXELS`] on the product.
fn clamp_scale(scale: f64, plan_w: f64, plan_h: f64) -> f64 {
    let mut s = scale;
    let longest = plan_w.max(plan_h).max(f64::MIN_POSITIVE);
    s = s.min(f64::from(MAX_DIMENSION) / longest);
    let area = (plan_w * plan_h).max(f64::MIN_POSITIVE);
    s = s.min((MAX_PIXELS as f64 / area).sqrt());
    // Never round down to nothing: a 1x1 image is a poor picture but an
    // honest one, where a zero-sized image is a failed allocation.
    s.max(f64::MIN_POSITIVE)
}

fn polygon_path(poly: &Polygon, to_px: &impl Fn(Point) -> (f32, f32)) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    for ring in std::iter::once(&poly.outer).chain(poly.holes.iter()) {
        push_ring(&mut builder, ring, to_px);
    }
    builder.finish()
}

fn ring_path(ring: &[Point], to_px: &impl Fn(Point) -> (f32, f32)) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    push_ring(&mut builder, ring, to_px);
    builder.finish()
}

fn push_ring(builder: &mut PathBuilder, ring: &[Point], to_px: &impl Fn(Point) -> (f32, f32)) {
    for (i, p) in ring.iter().enumerate() {
        let (x, y) = to_px(*p);
        if i == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    if !ring.is_empty() {
        builder.close();
    }
}

fn lerp(a: Point, b: Point, t: f64) -> Point {
    Point::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{Level, Wall};

    const P: fn(f64, f64) -> Point = Point::new;

    fn closed_room() -> (Document, Uuid) {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        let corners = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
        for i in 0..4 {
            let (x0, y0) = corners[i];
            let (x1, y1) = corners[(i + 1) % 4];
            doc.walls
                .push(Wall::new(level, P(x0, y0), P(x1, y1), 200.0, 2500.0));
        }
        (doc, level)
    }

    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        let pixmap = Pixmap::decode_png(png).expect("valid png");
        (pixmap.width(), pixmap.height(), pixmap.data().to_vec())
    }

    /// SVG and PNG must agree about what a document looks like. PNG omitted
    /// the door leaf entirely once the opening fields became typed, so the two
    /// exports gave different answers for the same plan.
    #[test]
    fn a_door_leaf_reaches_the_pixels_in_png_too() {
        let render = |toward| {
            let (mut doc, level) = closed_room();
            let wall = doc.walls[0].clone();
            let mut o =
                domowik_format::Opening::door(wall.id, 0.5, 900.0, 2100.0, wall.thickness_mm);
            o.opens_toward = toward;
            o.swing_deg = 90.0;
            doc.openings.push(o);
            assert!(doc.validate().is_empty(), "{:?}", doc.validate());
            render_level(&doc, level, 1.0).expect("renders")
        };
        // A leaf that swings one way must not produce the same pixels as one
        // that swings the other.
        assert_ne!(
            render(domowik_format::Side::Left).png,
            render(domowik_format::Side::Right).png,
            "the swing direction must be visible in the raster"
        );
    }

    #[test]
    fn output_is_a_png_of_the_expected_size() {
        let (doc, level) = closed_room();
        let out = render_level(&doc, level, 0.1).expect("render");
        assert_eq!(&out.png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
        // Extent is 4200 x 3200 mm (walls plus half thickness), plus 500 mm of
        // margin on each side, at 0.1 px/mm.
        assert_eq!(out.width, 520);
        assert_eq!(out.height, 420);
        assert_eq!(out.scale, 0.1, "no clamping at this size");
        let (w, h, _) = decode(&out.png);
        assert_eq!((w, h), (out.width, out.height));
    }

    #[test]
    fn the_same_document_renders_the_same_pixels() {
        // The determinism target: PIXEL equality.
        let (doc, level) = closed_room();
        let a = render_level(&doc, level, 0.2).expect("a");
        let b = render_level(&doc, level, 0.2).expect("b");
        assert_eq!(decode(&a.png).2, decode(&b.png).2);
    }

    #[test]
    fn the_same_document_encodes_the_same_bytes_within_one_build() {
        // Byte equality is a CONSEQUENCE of a deterministic encoder at a
        // pinned version, not a promise across upgrades. Asserting it here is
        // what makes a regression visible; the distinction is in the module
        // documentation so nobody reads this as the stronger claim.
        let (doc, level) = closed_room();
        let a = render_level(&doc, level, 0.2).expect("a");
        let b = render_level(&doc, level, 0.2).expect("b");
        assert_eq!(a.png, b.png);
    }

    #[test]
    fn a_room_is_painted_lighter_than_the_walls_around_it() {
        // The cheapest assertion that the paint model actually ran: sample the
        // centre of the room and a point inside a wall.
        let (doc, level) = closed_room();
        let out = render_level(&doc, level, 0.1).expect("render");
        let pixmap = Pixmap::decode_png(&out.png).expect("valid png");
        let at = |x: f64, y: f64| -> [u8; 4] {
            let px = ((x + 500.0 - -100.0) * out.scale) as u32;
            let py = ((3100.0 + 500.0 - y) * out.scale) as u32;
            let i = ((py * pixmap.width() + px) * 4) as usize;
            let d = pixmap.data();
            [d[i], d[i + 1], d[i + 2], d[i + 3]]
        };
        let room = at(2000.0, 1500.0);
        let wall = at(2000.0, 0.0);
        assert!(
            room[0] > wall[0] && room[1] > wall[1] && room[2] > wall[2],
            "room {room:?} should be lighter than wall {wall:?}"
        );
    }

    #[test]
    fn an_empty_level_still_renders() {
        let doc = Document::with_ground_floor();
        let out = render_level(&doc, doc.levels[0].id, 0.1).expect("render");
        assert!(out.width > 0 && out.height > 0);
        assert_eq!(&out.png[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn an_absurd_scale_is_clamped_rather_than_refused() {
        // Issue 0017: maximum dimensions and memory use are BOUNDED. A user
        // who asks for too much detail wants a picture, not an error.
        let (doc, level) = closed_room();
        let out = render_level(&doc, level, 1000.0).expect("render");
        assert!(out.width <= MAX_DIMENSION, "got {}", out.width);
        assert!(out.height <= MAX_DIMENSION, "got {}", out.height);
        assert!(
            u64::from(out.width) * u64::from(out.height) <= MAX_PIXELS,
            "{}x{} exceeds the pixel bound",
            out.width,
            out.height
        );
        assert!(out.scale < 1000.0, "the scale must be reported as reduced");
    }

    #[test]
    fn a_long_thin_plan_is_bounded_on_its_long_side() {
        // The dimension bound and the pixel bound catch different shapes: this
        // one is far under 64 megapixels and far over 16,384 on one side.
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            P(0.0, 0.0),
            P(1_000_000.0, 0.0),
            200.0,
            2500.0,
        ));
        let out = render_level(&doc, level, 1.0).expect("render");
        assert!(out.width <= MAX_DIMENSION, "got {}", out.width);
    }

    #[test]
    fn a_non_positive_scale_is_a_usage_error() {
        let (doc, level) = closed_room();
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let err = render_level(&doc, level, bad).expect_err("must reject");
            assert_eq!(err.code(), crate::exit::USAGE, "scale {bad}");
        }
    }

    #[test]
    fn levels_render_independently() {
        let (mut doc, level) = closed_room();
        let upper = Level::new("Level 2");
        let upper_id = upper.id;
        doc.levels.push(upper);
        let ground = render_level(&doc, level, 0.1).expect("ground");
        let empty = render_level(&doc, upper_id, 0.1).expect("upper");
        assert_ne!(
            decode(&ground.png).2,
            decode(&empty.png).2,
            "an empty level must not render the ground floor's walls"
        );
    }

    // ---- background images (FORMAT-SPEC §6, BEHAVIOR-SPEC §2.5) ----

    /// A 2x1 PNG: one red pixel then one blue one.
    ///
    /// Synthetic rather than a fixture file, so the test asserts PLACEMENT
    /// against known pixels instead of against whatever a screenshot happens
    /// to contain. Eyeballing a real scan is how a correct render gets
    /// reported as broken.
    fn two_pixel_png() -> (String, Vec<u8>) {
        let mut source = Pixmap::new(2, 1).expect("2x1");
        source.pixels_mut()[0] =
            tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 0, 255).expect("red");
        source.pixels_mut()[1] =
            tiny_skia::PremultipliedColorU8::from_rgba(0, 0, 255, 255).expect("blue");
        let bytes = source.encode_png().expect("encodes");
        (domowik_container::digest_of(&bytes), bytes)
    }

    /// `with_background`, with the anchor rotated.
    fn with_rotated_background(
        scale_mm_per_px: f64,
        rotation_deg: f64,
    ) -> (Document, Uuid, BTreeMap<String, Vec<u8>>) {
        let (mut doc, level, assets) = with_background(scale_mm_per_px);
        doc.levels[0].background_images[0].anchor.rotation_deg = rotation_deg;
        (doc, level, assets)
    }

    #[test]
    fn a_rotated_anchor_moves_the_extent_and_the_pixels_together() {
        // The 2x1 image is WIDE. A counter-clockwise quarter turn must make
        // its plan extent TALL — and must move the painted pixels with it.
        // Asserting only the extent would pass against a renderer that
        // resized the canvas and then drew the image unrotated inside it.
        let s = 1000.0;
        let (doc, level, assets) = with_rotated_background(s, 90.0);

        // cos(90°) is not exactly zero in binary floating point, so the
        // rotated extent is compared within a tolerance far below a
        // millimetre rather than for bit equality.
        let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
        let extents = background_extents(&doc, level, &assets);
        let (min, max) = extents[0];
        assert!(close(max.x - min.x, s), "width {}", max.x - min.x);
        assert!(close(max.y - min.y, 2.0 * s), "height {}", max.y - min.y);

        let unrotated = with_background(s);
        let (umin, umax) = background_extents(&unrotated.0, unrotated.1, &unrotated.2)[0];
        assert!(close(umax.x - umin.x, 2.0 * s));
        assert!(close(umax.y - umin.y, s));

        // The red pixel is image column 0; under a CCW quarter turn it sits
        // at the BOTTOM of the rotated pair, and blue above it. Derived from
        // the §2.5 transform, not from looking at the output.
        let red_centre = anchor_to_plan(0.0, 0.0, s, 90.0, 0.5, 0.5);
        let blue_centre = anchor_to_plan(0.0, 0.0, s, 90.0, 1.5, 0.5);
        assert!(
            blue_centre.y > red_centre.y,
            "expected blue above red after a CCW turn, got {red_centre:?} and {blue_centre:?}"
        );

        let rendered = render_level_with_assets(&doc, level, 0.02, &assets).expect("render");
        let (w, h, pixels) = decode(&rendered.png);
        let at = |p: Point| -> (u8, u8, u8) {
            let x = ((p.x - rendered.origin_mm[0]) * rendered.scale) as usize;
            let y = ((rendered.origin_mm[1] - p.y) * rendered.scale) as usize;
            let i = (y.min(h as usize - 1) * w as usize + x.min(w as usize - 1)) * 4;
            (pixels[i], pixels[i + 1], pixels[i + 2])
        };
        let (rr, rg, rb) = at(red_centre);
        assert!(
            rr > 200 && rg < 60 && rb < 60,
            "red pixel is ({rr},{rg},{rb})"
        );
        let (br, bg, bb) = at(blue_centre);
        assert!(
            bb > 200 && br < 60 && bg < 60,
            "blue pixel is ({br},{bg},{bb})"
        );
    }

    fn with_background(scale_mm_per_px: f64) -> (Document, Uuid, BTreeMap<String, Vec<u8>>) {
        let (digest, bytes) = two_pixel_png();
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        // A wall well clear of the image, so the extent is predictable and
        // the image is not painted over.
        doc.walls.push(Wall::new(
            level,
            P(0.0, -4000.0),
            P(4000.0, -4000.0),
            200.0,
            2500.0,
        ));
        doc.levels[0]
            .background_images
            .push(domowik_format::BackgroundImage {
                unknown: Default::default(),
                digest: digest.clone(),
                anchor: domowik_format::Anchor {
                    translation_mm: [0.0, 0.0],
                    scale_uniform: scale_mm_per_px,
                    rotation_deg: 0.0,
                    unknown: Default::default(),
                },
                stack_ordinal: 0,
            });
        let mut assets = BTreeMap::new();
        assets.insert(digest, bytes);
        (doc, level, assets)
    }

    #[test]
    fn a_background_image_is_placed_at_its_anchor_and_scale() {
        // 1000 mm per image pixel at 0.01 output px/mm means each image pixel
        // covers 10 output pixels, anchored with its top-left at plan (0,0).
        let (doc, level, assets) = with_background(1000.0);
        let out = render_level_with_assets(&doc, level, 0.01, &assets).expect("render");
        let pixmap = Pixmap::decode_png(&out.png).expect("valid png");

        let [ox, oy] = out.origin_mm;
        let at = |mm_x: f64, mm_y: f64| -> [u8; 4] {
            let px = ((mm_x - ox) * out.scale) as u32;
            let py = ((oy - mm_y) * out.scale) as u32;
            let i = ((py * pixmap.width() + px) * 4) as usize;
            let d = pixmap.data();
            [d[i], d[i + 1], d[i + 2], d[i + 3]]
        };
        // The red pixel spans plan x 0..1000, the blue one 1000..2000, both
        // running DOWN from y = 0.
        assert_eq!(at(500.0, -500.0), [255, 0, 0, 255], "red half");
        assert_eq!(at(1500.0, -500.0), [0, 0, 255, 255], "blue half");
        // Outside the image, the background colour shows through.
        assert_eq!(at(2500.0, -500.0), [247, 246, 243, 255], "past the image");
    }

    #[test]
    fn a_background_image_scales_with_its_anchor() {
        // Doubling millimetres-per-pixel doubles the plan area each image
        // pixel covers, so the boundary between red and blue moves.
        let (doc, level, assets) = with_background(2000.0);
        let out = render_level_with_assets(&doc, level, 0.01, &assets).expect("render");
        let pixmap = Pixmap::decode_png(&out.png).expect("valid png");
        let [ox, oy] = out.origin_mm;
        let at = |mm_x: f64, mm_y: f64| -> [u8; 4] {
            let px = ((mm_x - ox) * out.scale) as u32;
            let py = ((oy - mm_y) * out.scale) as u32;
            let i = ((py * pixmap.width() + px) * 4) as usize;
            let d = pixmap.data();
            [d[i], d[i + 1], d[i + 2], d[i + 3]]
        };
        // At 1500 mm the RED pixel now reaches, where at 1000 mm/px it was blue.
        assert_eq!(at(1500.0, -500.0), [255, 0, 0, 255]);
        assert_eq!(at(2500.0, -500.0), [0, 0, 255, 255]);
    }

    #[test]
    fn a_background_whose_blob_is_missing_is_skipped_not_fatal() {
        // FORMAT-SPEC §6: safe behaviour when optional assets are missing. A
        // missing scan must not stop a user exporting their walls.
        let (doc, level, _) = with_background(1000.0);
        let out = render_level_with_assets(&doc, level, 0.01, &BTreeMap::new())
            .expect("must still render");
        assert!(out.width > 0);
        let pixmap = Pixmap::decode_png(&out.png).expect("valid png");
        let d = pixmap.data();
        assert_eq!([d[0], d[1], d[2], d[3]], [247, 246, 243, 255]);
    }

    #[test]
    fn a_background_whose_bytes_are_not_a_png_is_skipped_not_fatal() {
        // Only PNG decodes here. A JPEG scan stores fine and does not appear —
        // a limitation, not a crash.
        let (mut doc, level, mut assets) = with_background(1000.0);
        let digest = doc.levels[0].background_images[0].digest.clone();
        assets.insert(digest, b"\xff\xd8\xff not a png".to_vec());
        doc.levels[0].background_images[0].anchor.scale_uniform = 1000.0;
        assert!(render_level_with_assets(&doc, level, 0.01, &assets).is_ok());
    }

    #[test]
    fn a_background_with_a_non_positive_scale_is_skipped() {
        let (mut doc, level, assets) = with_background(1000.0);
        for bad in [0.0, -1.0, f64::NAN] {
            doc.levels[0].background_images[0].anchor.scale_uniform = bad;
            assert!(
                render_level_with_assets(&doc, level, 0.01, &assets).is_ok(),
                "scale {bad} must be skipped, not fatal"
            );
        }
    }
}
