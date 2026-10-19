//! Room labels: what they say, where they sit, and the outlines both
//! exporters draw.
//!
//! BEHAVIOR-SPEC R5.3.3 fixes the CONTENT of a room label — one per face with
//! at least one candidate, anchored at its effective record's seed, reading
//! that record's name or the fallback token `Room`, always accompanied by the
//! R5.3.2 displayed area. Visual style is view-local (R3.0.7), so the sizes
//! and spacing here are this exporter's choice; they are constants rather than
//! options because two exports of one plan must not differ.
//!
//! # Why this produces OUTLINES and not text
//!
//! Issue 0060 requires SVG and PNG to render names deterministically through
//! the shared path issue 0016 leaves in place, "rather than a third renderer".
//! There is no renderer-neutral paint API yet — that gap is exactly what keeps
//! 0016 open — so the shared path is this module: it turns a document and its
//! derived faces into placed closed contours in plan millimetres, and each
//! exporter draws them with the primitive it already has (`<path>` in SVG, a
//! filled `tiny_skia::Path` in the raster).
//!
//! An SVG `<text>` element would have been less code and would not have been
//! deterministic: what it looks like depends on which fonts the viewer has,
//! and the PNG could not use it at all. Outlines make the two exports agree by
//! construction, which is the property issue 0016 says is worth having.
//!
//! # What this is NOT: a text shaping engine
//!
//! Glyphs are looked up one Unicode scalar at a time through the font's `cmap`
//! and placed at their own advance widths. That is correct for the scripts in
//! [`SUPPORTED_RANGES`] and WRONG for many others: it applies no `GSUB`
//! substitution, no `GPOS` positioning or kerning, no bidirectional reordering,
//! and no mark attachment. Rendering Arabic, Devanagari or a combining
//! diacritic this way does not merely look plain, it produces text that reads
//! incorrectly — disconnected letterforms, marks beside their base instead of
//! over it, right-to-left words spelled backwards.
//!
//! So a name that needs shaping is REFUSED rather than drawn. The label keeps
//! its area line, the name is left out, and [`LabelWarning`] names the record
//! and the character responsible. Refusing is the honest behaviour: a missing
//! name is visibly missing, and a wrongly shaped one looks like a name.
//!
//! The authoritative string is never altered — not normalised, not
//! transliterated, not truncated. It is stored, reported and exported as
//! given; only whether it is DRAWN depends on the repertoire.

use crate::CliError;
use domowik_format::Document;
use domowik_geometry::rooms::{self, LevelRooms};
use std::fmt;
use uuid::Uuid;

/// Noto Sans Regular 2.015, unhinted, unmodified, under the SIL Open Font
/// License 1.1.
///
/// Provenance, hashes and the licence text are recorded in
/// `docs/third-party/assets.json` and `NOTICE`; the licence travels beside the
/// file at `assets/NotoSans-Regular.OFL.txt`.
///
/// Embedded rather than loaded from disk. An exporter that went looking for a
/// font at run time would render differently depending on what it found, and
/// "deterministically" is the word the acceptance criterion uses.
const FONT: &[u8] = include_bytes!("../assets/NotoSans-Regular.ttf");

/// Em size of the name line, in plan millimetres.
///
/// 260 mm is about a fifth of the short side of a small bedroom, which is
/// legible on a page-sized export of a flat and not overbearing on a house.
const NAME_EM_MM: f64 = 260.0;

/// Em size of the area line. Smaller, because the name is what is being read
/// and the area is what is being checked.
const AREA_EM_MM: f64 = 190.0;

/// Vertical space between the two lines, in plan millimetres.
const LINE_GAP_MM: f64 = 90.0;

/// Segments per curve when flattening a glyph contour.
///
/// FIXED rather than adaptive on purpose. An adaptive tolerance produces a
/// different vertex count at different scales, and the PNG exporter picks its
/// scale from the plan extent — so the same name in two plans would flatten
/// differently and the outputs would stop being comparable. Sixteen segments
/// on a 260 mm em leaves a chord error far under the width of a rendered
/// pixel at any scale this exporter allows.
const CURVE_SEGMENTS: usize = 16;

/// The Unicode ranges this exporter will draw with `cmap` lookup alone.
///
/// Every range here is left-to-right, needs no reordering, and contains no
/// combining marks, so one glyph per scalar at its own advance width is
/// correct text rather than approximately correct text. Precomposed letters
/// are covered — Polish `Ł`, `ż`, `ó` and the rest of Latin Extended-A are
/// single scalars — which is why a Polish room name renders and a decomposed
/// `e` + U+0301 does not.
///
/// Deliberate exclusions, each of which would render WRONGLY rather than
/// plainly: U+00AD soft hyphen (invisible, and a shaper decides whether it
/// appears at all), U+0300–U+036F combining diacritics and the combining
/// ranges inside Cyrillic, and every script that needs `GSUB`/`GPOS` or
/// bidirectional handling.
pub const SUPPORTED_RANGES: &[(u32, u32)] = &[
    // Basic Latin, printable.
    (0x0020, 0x007E),
    // Latin-1 Supplement, minus the soft hyphen at U+00AD.
    (0x00A0, 0x00AC),
    (0x00AE, 0x00FF),
    // Latin Extended-A and Extended-B: Polish, Czech, Turkish, Welsh, the
    // Baltic and most of the rest of Latin Europe.
    (0x0100, 0x024F),
    // Greek, split around the Coptic letters at U+03E2–U+03EF. Coptic shares
    // the block but is a different script, and Noto Sans leaves it to Noto
    // Sans Coptic, so claiming it here would promise coverage the pinned font
    // does not have.
    (0x0370, 0x03E1),
    (0x03F0, 0x03FF),
    // Cyrillic, split around its combining block U+0483–U+0489.
    (0x0400, 0x0482),
    (0x048A, 0x04FF),
    // The general punctuation people actually put in a room name: dashes,
    // curly quotes, the ellipsis.
    (0x2010, 0x2015),
    (0x2018, 0x201F),
    (0x2026, 0x2026),
];

/// Code points inside [`SUPPORTED_RANGES`] that Unicode does not assign.
///
/// The ranges are written as blocks because blocks are what a reader can check
/// against the standard, and every block has reserved holes. No string can
/// contain these — they are not characters — so they are excluded from the
/// font-coverage promise rather than from the ranges, which would turn two
/// legible ranges into nine.
#[cfg(test)]
const UNASSIGNED: &[(u32, u32)] = &[
    (0x0378, 0x0379),
    (0x0380, 0x0383),
    (0x038B, 0x038B),
    (0x038D, 0x038D),
    (0x03A2, 0x03A2),
];

/// Why a label's name could not be drawn.
///
/// Both variants keep the record id AND the offending character, because the
/// two questions a reader has are "which room" and "what do I change".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelWarning {
    /// The character is outside [`SUPPORTED_RANGES`]: drawing it with
    /// `cmap` alone would produce text that reads incorrectly.
    UnsupportedScript { record: Uuid, character: char },
    /// The character is in a supported range but the pinned font has no glyph
    /// for it.
    MissingGlyph { record: Uuid, character: char },
}

impl fmt::Display for LabelWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedScript { record, character } => write!(
                f,
                "room {record}: the name contains U+{:04X} ({character:?}), which this \
                 export cannot draw correctly without text shaping; the name is omitted \
                 and the area is still shown. The stored name is unchanged.",
                *character as u32
            ),
            Self::MissingGlyph { record, character } => write!(
                f,
                "room {record}: the bundled font has no glyph for U+{:04X} ({character:?}); \
                 the name is omitted and the area is still shown. The stored name is \
                 unchanged.",
                *character as u32
            ),
        }
    }
}

/// One line of a label: its text, and the closed contours that draw it.
#[derive(Debug, Clone, PartialEq)]
pub struct LabelLine {
    /// The text these contours spell, for a caller that wants to say so in
    /// metadata. The contours are what gets drawn.
    pub text: String,
    /// Closed contours in PLAN millimetres, +Y up, to be filled with the
    /// NON-ZERO rule.
    ///
    /// Non-zero, not even-odd: a glyph's counters — the hole in an `o` — wind
    /// opposite to its outer contour, and the two rules agree on that. They
    /// disagree on self-overlapping outlines, which some glyphs have, and
    /// even-odd punches spurious holes in those.
    pub contours: Vec<Vec<[f64; 2]>>,
}

/// A label placed on one face.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedLabel {
    /// The record the label belongs to.
    pub record: Uuid,
    /// The anchor: the effective record's seed, which R5.2.3 guarantees is
    /// strictly inside the face, so it never needs clamping.
    pub anchor_mm: [f64; 2],
    /// Name line then area line, or the area line alone when the name could
    /// not be drawn.
    pub lines: Vec<LabelLine>,
}

/// Every label of one level, and every reason a name was left out.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Labels {
    pub placed: Vec<PlacedLabel>,
    pub warnings: Vec<LabelWarning>,
}

/// Whether `c` is in a range this exporter draws with `cmap` alone.
pub fn is_supported(c: char) -> bool {
    let v = c as u32;
    SUPPORTED_RANGES
        .iter()
        .any(|&(low, high)| v >= low && v <= high)
}

/// The labels of `level_id`, in the canonical face order (R5.1.3).
///
/// Faces with no candidates produce nothing at all: R5.3.3's absence rule is
/// not a styling choice, and a face nobody named has no label to place.
pub fn labels_for_level(doc: &Document, level_id: Uuid) -> Result<Labels, CliError> {
    let rooms = rooms::project_level(doc, level_id);
    labels_for(doc, &rooms)
}

/// [`labels_for_level`] against a projection the caller already has.
pub fn labels_for(doc: &Document, rooms: &LevelRooms) -> Result<Labels, CliError> {
    let font = Font::load()?;
    let mut out = Labels::default();

    for (index, face) in rooms.faces.iter().enumerate() {
        let Some(record_id) = rooms.effective_record(index) else {
            continue;
        };
        let Some(record) = doc.room_record(record_id) else {
            continue;
        };
        let seed = record.seed();
        let area = format!(
            "{} m²",
            rooms::display_polygon_area_m2(face).unwrap_or_else(|| "—".to_owned())
        );

        // The name is attempted first so that a refusal is reported before the
        // area line is laid out, and the layout knows how many lines there are.
        let name = match font.render(record.display_name(), NAME_EM_MM) {
            Ok(contours) => Some(contours),
            Err(character) => {
                out.warnings.push(font.classify(record_id, character));
                None
            }
        };
        let area_contours = match font.render(&area, AREA_EM_MM) {
            Ok(contours) => Some(contours),
            // Unreachable with the pinned font: the area string is digits, a
            // full stop, a space, `m` and U+00B2, all of which Noto Sans has
            // and all of which are in a supported range. Reported rather than
            // unwrapped, because "unreachable" is a claim about a font file
            // and font files can be replaced.
            Err(character) => {
                out.warnings.push(font.classify(record_id, character));
                None
            }
        };

        // The block is centred vertically on the seed: the name's baseline sits
        // half a gap above it and the area's cap height half a gap below, so a
        // one-line label and a two-line label both read as belonging to the
        // same point.
        let name_baseline = seed.y + LINE_GAP_MM / 2.0;
        let area_baseline = seed.y - LINE_GAP_MM / 2.0 - font.cap_height(AREA_EM_MM);

        let mut lines = Vec::with_capacity(2);
        if let Some(glyphs) = name {
            lines.push(LabelLine {
                text: record.display_name().to_owned(),
                contours: place(glyphs, seed.x, name_baseline),
            });
        }
        if let Some(glyphs) = area_contours {
            lines.push(LabelLine {
                text: area,
                contours: place(glyphs, seed.x, area_baseline),
            });
        }
        if lines.is_empty() {
            continue;
        }
        out.placed.push(PlacedLabel {
            record: record_id,
            anchor_mm: [seed.x, seed.y],
            lines,
        });
    }
    Ok(out)
}

/// Plain text for drawing annotations (issue 0103's review overlay), with
/// the same pinned font and glyph pipeline as room labels.
pub struct Annotator {
    font: Font,
}

impl Annotator {
    pub fn new() -> Result<Self, CliError> {
        Ok(Self {
            font: Font::load()?,
        })
    }

    /// Cap height at `em_mm`.
    pub fn cap_height(&self, em_mm: f64) -> f64 {
        self.font.cap_height(em_mm)
    }

    /// `text` centred on `centre_x` with its baseline at `baseline_y`, as
    /// closed plan-millimetre contours (non-zero fill), and its advance
    /// width. `None` when the font cannot draw a character in it.
    pub fn text(
        &self,
        text: &str,
        em_mm: f64,
        centre_x: f64,
        baseline_y: f64,
    ) -> Option<(Vec<Vec<[f64; 2]>>, f64)> {
        let run = self.font.render(text, em_mm).ok()?;
        let advance = run.advance_mm;
        Some((place(run, centre_x, baseline_y), advance))
    }

    /// Advance width of `text` at `em_mm`, `None` when it cannot be drawn.
    pub fn width(&self, text: &str, em_mm: f64) -> Option<f64> {
        self.font.render(text, em_mm).ok().map(|r| r.advance_mm)
    }
}

/// A run of glyph contours laid out from the origin along a baseline at y = 0,
/// with the total advance that centres it.
struct Run {
    contours: Vec<Vec<[f64; 2]>>,
    advance_mm: f64,
}

/// Centres `run` on `centre_x` and lifts it to `baseline_y`.
fn place(run: Run, centre_x: f64, baseline_y: f64) -> Vec<Vec<[f64; 2]>> {
    let dx = centre_x - run.advance_mm / 2.0;
    run.contours
        .into_iter()
        .map(|contour| {
            contour
                .into_iter()
                .map(|[x, y]| [x + dx, y + baseline_y])
                .collect()
        })
        .collect()
}

/// The pinned font, parsed.
struct Font {
    face: ttf_parser::Face<'static>,
}

impl Font {
    fn load() -> Result<Self, CliError> {
        let face = ttf_parser::Face::parse(FONT, 0).map_err(|e| {
            CliError::Failure(format!(
                "the bundled label font did not parse ({e}); this is a build defect, \
                 not something a plan can cause"
            ))
        })?;
        Ok(Self { face })
    }

    /// Font units per plan millimetre at `em_mm`.
    fn scale(&self, em_mm: f64) -> f64 {
        em_mm / f64::from(self.face.units_per_em())
    }

    /// Cap height at `em_mm`, from the font's own metrics rather than a guess.
    fn cap_height(&self, em_mm: f64) -> f64 {
        let units = self
            .face
            .capital_height()
            .map(f64::from)
            // Noto Sans declares one. The fallback keeps a replaced font from
            // collapsing the layout to a single line rather than merely
            // shifting it.
            .unwrap_or_else(|| f64::from(self.face.units_per_em()) * 0.7);
        units * self.scale(em_mm)
    }

    /// Which refusal `character` earns: outside the repertoire, or inside it
    /// and absent from the font.
    fn classify(&self, record: Uuid, character: char) -> LabelWarning {
        if is_supported(character) {
            LabelWarning::MissingGlyph { record, character }
        } else {
            LabelWarning::UnsupportedScript { record, character }
        }
    }

    /// Lays `text` out along a baseline at y = 0 starting at x = 0.
    ///
    /// Returns the first character it will not draw, which refuses the WHOLE
    /// string: half a name is not a name, and a label reading `Salon` when the
    /// room is called `Salonik` is worse than one reading nothing.
    fn render(&self, text: &str, em_mm: f64) -> Result<Run, char> {
        let scale = self.scale(em_mm);
        let mut contours = Vec::new();
        let mut pen = 0.0f64;

        for character in text.chars() {
            if !is_supported(character) {
                return Err(character);
            }
            let glyph = self.face.glyph_index(character).ok_or(character)?;
            let advance = self
                .face
                .glyph_hor_advance(glyph)
                .map(f64::from)
                .ok_or(character)?;

            let mut outliner = Outliner {
                contours: Vec::new(),
                current: Vec::new(),
                scale,
                origin_x: pen,
            };
            // `None` means the glyph has no outline, which is what a space is.
            // It still advances the pen, so it is not an error.
            self.face.outline_glyph(glyph, &mut outliner);
            outliner.flush();
            contours.extend(outliner.contours);

            pen += advance * scale;
        }

        Ok(Run {
            contours,
            advance_mm: pen,
        })
    }
}

/// Collects a glyph's contours, flattened and scaled into plan millimetres.
///
/// Plan space and font space both put +Y up, so only the scale and the pen
/// offset apply; no flip is needed here and the exporters apply their own.
struct Outliner {
    contours: Vec<Vec<[f64; 2]>>,
    current: Vec<[f64; 2]>,
    scale: f64,
    origin_x: f64,
}

impl Outliner {
    fn point(&self, x: f32, y: f32) -> [f64; 2] {
        [
            self.origin_x + f64::from(x) * self.scale,
            f64::from(y) * self.scale,
        ]
    }

    /// Ends the contour in progress, discarding one too short to enclose area.
    fn flush(&mut self) {
        let contour = std::mem::take(&mut self.current);
        if contour.len() >= 3 {
            self.contours.push(contour);
        }
    }
}

impl ttf_parser::OutlineBuilder for Outliner {
    fn move_to(&mut self, x: f32, y: f32) {
        self.flush();
        let p = self.point(x, y);
        self.current.push(p);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.current.push(p);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let Some(&from) = self.current.last() else {
            return;
        };
        let control = self.point(x1, y1);
        let to = self.point(x, y);
        for step in 1..=CURVE_SEGMENTS {
            let t = step as f64 / CURVE_SEGMENTS as f64;
            let u = 1.0 - t;
            self.current.push([
                u * u * from[0] + 2.0 * u * t * control[0] + t * t * to[0],
                u * u * from[1] + 2.0 * u * t * control[1] + t * t * to[1],
            ]);
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let Some(&from) = self.current.last() else {
            return;
        };
        let c1 = self.point(x1, y1);
        let c2 = self.point(x2, y2);
        let to = self.point(x, y);
        for step in 1..=CURVE_SEGMENTS {
            let t = step as f64 / CURVE_SEGMENTS as f64;
            let u = 1.0 - t;
            self.current.push([
                u * u * u * from[0]
                    + 3.0 * u * u * t * c1[0]
                    + 3.0 * u * t * t * c2[0]
                    + t * t * t * to[0],
                u * u * u * from[1]
                    + 3.0 * u * u * t * c1[1]
                    + 3.0 * u * t * t * c2[1]
                    + t * t * t * to[1],
            ]);
        }
    }

    fn close(&mut self) {
        self.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{JoinMember, Level, Point, Role, RoomRecord, Wall, WallJoin};

    fn b4_with(name: Option<&str>) -> (Document, Uuid) {
        let mut doc = Document::new();
        let level = Level::new("Level 1");
        let level_id = level.id;
        doc.levels.push(level);
        let corners = [
            (0.0, 0.0),
            (4000.0, 0.0),
            (4000.0, 3000.0),
            (0.0, 3000.0),
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
        let mut record = RoomRecord::new(Uuid::from_u128(1), level_id, Point::new(2000.0, 1500.0));
        record.name = name.map(str::to_owned);
        doc.room_records.push(record);
        (doc, level_id)
    }

    fn labels(name: Option<&str>) -> Labels {
        let (doc, level) = b4_with(name);
        labels_for_level(&doc, level).expect("the bundled font parses")
    }

    #[test]
    fn a_named_face_gets_a_name_line_and_an_area_line() {
        let out = labels(Some("Living"));
        assert!(out.warnings.is_empty());
        assert_eq!(out.placed.len(), 1);
        let label = &out.placed[0];
        assert_eq!(label.anchor_mm, [2000.0, 1500.0]);
        assert_eq!(label.lines.len(), 2);
        assert_eq!(label.lines[0].text, "Living");
        assert_eq!(label.lines[1].text, "10.64 m²");
        assert!(label.lines.iter().all(|l| !l.contours.is_empty()));
    }

    #[test]
    fn an_unnamed_record_still_labels_its_face_with_the_fallback() {
        // R5.3.3: absent and empty names both display as `Room`.
        for name in [None, Some("")] {
            let out = labels(name);
            assert_eq!(out.placed[0].lines[0].text, "Room", "name {name:?}");
        }
    }

    #[test]
    fn a_face_with_no_candidates_has_no_label_at_all() {
        let (mut doc, level) = b4_with(Some("Living"));
        doc.room_records[0].seed_mm = [10_000.0, 1500.0];
        let out = labels_for_level(&doc, level).expect("font");
        assert!(out.placed.is_empty(), "R5.3.3 absence, not an empty label");
        assert!(out.warnings.is_empty());
    }

    #[test]
    fn polish_diacritics_render_because_they_are_precomposed_scalars() {
        // The reason this case matters: an ASCII-only alphabet would have
        // silently mangled every one of these.
        for name in [
            "Łazienka",
            "Kuchnia",
            "Pokój dzienny",
            "Sypialnia",
            "Poddasze",
        ] {
            let out = labels(Some(name));
            assert!(
                out.warnings.is_empty(),
                "{name} must render without warning, got {:?}",
                out.warnings
            );
            assert_eq!(out.placed[0].lines[0].text, name);
            assert!(!out.placed[0].lines[0].contours.is_empty());
        }
    }

    #[test]
    fn greek_and_cyrillic_render_too() {
        for name in ["Κουζίνα", "Кухня"] {
            let out = labels(Some(name));
            assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        }
    }

    #[test]
    fn a_combining_mark_is_refused_rather_than_placed_beside_its_base() {
        // "Pokój" spelled with a combining acute instead of the precomposed
        // ó. Without mark positioning the accent lands after the letter, not
        // over it, so this is refused.
        let decomposed = "Pokoj\u{0301}";
        let out = labels(Some(decomposed));
        assert_eq!(
            out.warnings,
            vec![LabelWarning::UnsupportedScript {
                record: Uuid::from_u128(1),
                character: '\u{0301}',
            }]
        );
        // The area line survives, and the name is simply not drawn.
        assert_eq!(out.placed.len(), 1);
        assert_eq!(out.placed[0].lines.len(), 1);
        assert_eq!(out.placed[0].lines[0].text, "10.64 m²");
    }

    #[test]
    fn cjk_and_right_to_left_names_are_refused_with_the_offending_character() {
        for (name, expected) in [("居間", '居'), ("غرفة", 'غ'), ("סלון", 'ס')] {
            let out = labels(Some(name));
            assert_eq!(
                out.warnings,
                vec![LabelWarning::UnsupportedScript {
                    record: Uuid::from_u128(1),
                    character: expected,
                }],
                "{name} must be refused, not drawn"
            );
            assert_eq!(out.placed[0].lines.len(), 1, "{name} keeps its area line");
        }
    }

    #[test]
    fn a_refused_name_is_not_altered_in_the_document() {
        // The whole point of refusing: the stored string is authoritative and
        // the export is a view of it.
        let (doc, level) = b4_with(Some("居間"));
        let out = labels_for_level(&doc, level).expect("font");
        assert!(!out.warnings.is_empty());
        assert_eq!(doc.room_records[0].name.as_deref(), Some("居間"));
        assert_eq!(doc.room_records[0].display_name(), "居間");
    }

    #[test]
    fn the_warning_names_the_room_the_codepoint_and_what_happened() {
        let out = labels(Some("居間"));
        let text = out.warnings[0].to_string();
        assert!(text.contains("U+5C45"), "got {text}");
        assert!(text.contains("the name is omitted"), "got {text}");
        assert!(text.contains("area is still shown"), "got {text}");
        assert!(text.contains("stored name is unchanged"), "got {text}");
    }

    #[test]
    fn layout_is_deterministic_for_one_document() {
        let a = labels(Some("Salon"));
        let b = labels(Some("Salon"));
        assert_eq!(a, b, "the same plan must produce the same outlines");
    }

    #[test]
    fn the_two_lines_straddle_the_seed() {
        let out = labels(Some("Salon"));
        let highest = |line: &LabelLine| {
            line.contours
                .iter()
                .flatten()
                .map(|p| p[1])
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let lowest = |line: &LabelLine| {
            line.contours
                .iter()
                .flatten()
                .map(|p| p[1])
                .fold(f64::INFINITY, f64::min)
        };
        let (name, area) = (&out.placed[0].lines[0], &out.placed[0].lines[1]);
        assert!(lowest(name) >= 1500.0, "the name sits above the seed");
        assert!(highest(area) <= 1500.0, "the area sits below it");
    }

    #[test]
    fn a_label_is_centred_on_its_seed() {
        let out = labels(Some("Salon"));
        for line in &out.placed[0].lines {
            let xs: Vec<f64> = line.contours.iter().flatten().map(|p| p[0]).collect();
            let min = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let max = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let centre = (min + max) / 2.0;
            // Side bearings make the inked centre differ slightly from the
            // advance centre, so this is a bound rather than an equality.
            assert!(
                (centre - 2000.0).abs() < NAME_EM_MM * 0.2,
                "{:?} centred at {centre}",
                line.text
            );
        }
    }

    #[test]
    fn the_supported_ranges_exclude_what_would_render_wrongly() {
        for c in [
            '\u{0301}', '\u{00AD}', '\u{0483}', '居', 'غ', 'ס', '\n', '\u{0915}',
            // Coptic shares the Greek block and the pinned font omits it.
            '\u{03E2}',
        ] {
            assert!(!is_supported(c), "U+{:04X} must not be supported", c as u32);
        }
        for c in [
            'A', 'z', '0', ' ', '.', 'm', '²', 'Ł', 'ż', 'ó', 'Κ', 'К', '—', '…',
        ] {
            assert!(is_supported(c), "U+{:04X} must be supported", c as u32);
        }
    }

    #[test]
    fn every_supported_range_is_covered_by_the_pinned_font() {
        // The repertoire is a promise about what renders. If the font were
        // replaced by one with narrower coverage, this says so rather than
        // letting every affected name degrade to a warning at export time.
        let font = Font::load().expect("the bundled font parses");
        let unassigned = |v: u32| UNASSIGNED.iter().any(|&(low, high)| v >= low && v <= high);
        let mut missing = Vec::new();
        for &(low, high) in SUPPORTED_RANGES {
            for value in low..=high {
                let Some(c) = char::from_u32(value) else {
                    continue;
                };
                if unassigned(value) {
                    continue;
                }
                if font.face.glyph_index(c).is_none() {
                    missing.push(format!("U+{value:04X}"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "the pinned font lacks {} of the supported code points: {}",
            missing.len(),
            missing.join(", ")
        );
    }
}
