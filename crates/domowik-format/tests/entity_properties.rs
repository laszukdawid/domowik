//! Property coverage over random valid and invalid entity sets.
//!
//! Issue 0010's fourth criterion asks for exactly this, and ED-0096 records
//! honestly that what existed was "example-based plus one pseudo-random
//! command stream, which is not the same thing". The difference that matters:
//! an example asserts one answer, a property asserts a RELATION that must hold
//! over inputs nobody chose by hand — so it fails on cases the author never
//! thought of, which is the entire point.
//!
//! # Why a hand-rolled generator rather than a proptest dependency
//!
//! Adding one would change `docs/RUST-DEPENDENCIES.md` and oblige a licence
//! review for test scaffolding. A seeded SplitMix64 is twenty lines, is
//! reproducible by construction, and prints the seed on failure so a red run
//! is replayable exactly. The project already hand-rolls a deterministic RNG
//! in `domowik-bench` for the same reason.
//!
//! Every case reports its seed. A failure here is never "it went red once".

use domowik_format::*;
use uuid::Uuid;

/// SplitMix64. Deterministic, and good enough to shuffle structure.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }

    /// A finite coordinate comfortably inside the domain.
    fn coord(&mut self) -> f64 {
        let span = COORD_BOUND_MM / 2.0;
        (self.next_u64() as f64 / u64::MAX as f64) * 2.0 * span - span
    }

    fn uuid(&mut self) -> Uuid {
        Uuid::from_u128(((self.next_u64() as u128) << 64) | self.next_u64() as u128)
    }

    fn maybe_name(&mut self) -> Option<String> {
        // Absent, empty and non-empty all generated, because the format keeps
        // the three apart and a generator that only produced one would never
        // exercise that.
        match self.below(3) {
            0 => None,
            1 => Some(String::new()),
            _ => Some(format!("n{}", self.next_u64() % 1000)),
        }
    }
}

/// A random document that MUST validate clean.
///
/// Everything references a level that exists, every coordinate is in domain,
/// every dimension is positive, every digest is well formed, and openings fit
/// their hosts. The generator's job is to vary STRUCTURE, not to produce
/// defects — defects are injected deliberately below, so a failure always says
/// which rule broke.
fn valid_document(rng: &mut Rng) -> Document {
    let mut doc = Document::new();
    let level_count = 1 + rng.below(3);
    let mut levels = Vec::new();
    for i in 0..level_count {
        let mut level = Level::new(format!("Level {i}"));
        level.id = rng.uuid();
        level.elevation_mm = 3000.0 * i as f64;
        levels.push(level.id);
        doc.levels.push(level);
    }

    for _ in 0..rng.below(6) {
        let level = levels[rng.below(levels.len())];
        // Non-zero length, well inside the domain.
        let (x, y) = (rng.coord(), rng.coord());
        let mut wall = Wall::new(
            level,
            Point::new(x, y),
            Point::new(x + 1000.0 + rng.below(3000) as f64, y),
            200.0,
            2500.0,
        );
        wall.id = rng.uuid();
        if rng.below(2) == 0 {
            // A SET, so the tokens must differ.
            wall.style_refs = (0..1 + rng.below(3)).map(|i| format!("S{i}")).collect();
        }
        doc.walls.push(wall);
    }

    // AT MOST ONE opening per wall. Two centred openings on one host overlap,
    // which R4.1 refuses — the generator's job is to vary structure, not to
    // manufacture defects, and the first run of these properties caught it
    // doing exactly that.
    let mut hosts: Vec<(Uuid, f64)> = doc.walls.iter().map(|w| (w.id, w.length_mm())).collect();
    for _ in 0..rng.below(4) {
        if hosts.is_empty() {
            break;
        }
        let (host, length) = hosts.remove(rng.below(hosts.len()));
        // Narrow and central, so it always fits whatever the host's length.
        let width = (length / 4.0).min(900.0);
        let mut opening = Opening::door(host, 0.5, width, 2100.0, 200.0);
        opening.id = rng.uuid();
        if rng.below(2) == 0 {
            opening.style_refs = vec![format!("S{}", rng.below(5))];
        }
        doc.openings.push(opening);
    }

    for _ in 0..rng.below(4) {
        let level = levels[rng.below(levels.len())];
        doc.labels.push(Label {
            id: rng.uuid(),
            level_id: level,
            anchor_mm: [rng.coord(), rng.coord()],
            name: rng.maybe_name(),
            unknown: Default::default(),
        });
    }
    for _ in 0..rng.below(4) {
        let level = levels[rng.below(levels.len())];
        doc.dimensions.push(Dimension {
            id: rng.uuid(),
            level_id: level,
            start_mm: [rng.coord(), rng.coord()],
            end_mm: [rng.coord(), rng.coord()],
            label_override: rng.maybe_name(),
            unknown: Default::default(),
        });
    }
    for _ in 0..rng.below(4) {
        let level = levels[rng.below(levels.len())];
        let hex: String = (0..64)
            .map(|_| char::from_digit(rng.below(16) as u32, 16).expect("hex"))
            .collect();
        doc.asset_instances.push(AssetInstance {
            id: rng.uuid(),
            level_id: level,
            digest: format!("sha256:{hex}"),
            transform: InstanceTransform {
                translation_mm: [rng.coord(), rng.coord()],
                scale_uniform: 0.5 + rng.below(100) as f64 / 50.0,
                rotation_deg: rng.coord() / 1000.0,
                unknown: Default::default(),
            },
            display_name: rng.maybe_name(),
            unknown: Default::default(),
        });
    }
    for _ in 0..rng.below(3) {
        let level = levels[rng.below(levels.len())];
        let mut record = RoomRecord::new(rng.uuid(), level, Point::new(rng.coord(), rng.coord()));
        record.name = rng.maybe_name();
        doc.room_records.push(record);
    }
    doc
}

/// 512 documents per property: enough to hit the branch combinations the
/// generator can produce, fast enough to run on every build.
const CASES: u64 = 512;

fn for_each_document(property: impl Fn(u64, Document)) {
    for seed in 0..CASES {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xDEAD_BEEF);
        property(seed, valid_document(&mut rng));
    }
}

#[test]
fn a_valid_document_validates_clean() {
    for_each_document(|seed, doc| {
        assert_eq!(
            doc.validate(),
            vec![],
            "seed {seed}: the generator must only produce valid documents; \
             if this fails the GENERATOR is wrong, which is worth knowing too"
        );
    });
}

#[test]
fn every_document_round_trips_through_json_unchanged() {
    for_each_document(|seed, doc| {
        let text = serde_json::to_string(&doc).expect("serialises");
        let back: Document = serde_json::from_str(&text).expect("deserialises");
        assert_eq!(
            back, doc,
            "seed {seed}: round trip must preserve the document"
        );
        // And serialising twice is stable, so the encoding is a function of
        // the value rather than of anything ambient.
        assert_eq!(serde_json::to_string(&back).expect("serialises"), text);
    });
}

#[test]
fn entity_count_is_the_sum_of_every_collection() {
    for_each_document(|seed, doc| {
        let sum = doc.levels.len()
            + doc.walls.len()
            + doc.openings.len()
            + doc.joins.len()
            + doc.room_records.len()
            + doc.labels.len()
            + doc.dimensions.len()
            + doc.asset_instances.len();
        assert_eq!(doc.entity_count(), sum, "seed {seed}");
    });
}

#[test]
fn validation_is_deterministic_in_content_and_in_order() {
    // Two calls on one document, and two calls on equal documents built from
    // one seed: a validator whose output depended on hash iteration order
    // would pass the first and fail the second.
    for seed in 0..CASES {
        let build = || {
            let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xDEAD_BEEF);
            valid_document(&mut rng)
        };
        let (a, b) = (build(), build());
        assert_eq!(a, b, "seed {seed}: the generator must be deterministic");
        assert_eq!(
            a.validate(),
            a.validate(),
            "seed {seed}: twice on one value"
        );
        assert_eq!(
            a.validate(),
            b.validate(),
            "seed {seed}: across equal values"
        );
    }
}

#[test]
fn every_entity_survives_removal_and_reinsertion_through_the_seam() {
    for_each_document(|seed, doc| {
        let ids: Vec<Uuid> = (doc.levels.iter().map(|l| l.id))
            .chain(doc.walls.iter().map(|w| w.id))
            .chain(doc.openings.iter().map(|o| o.id))
            .chain(doc.room_records.iter().map(|r| r.id))
            .chain(doc.labels.iter().map(|l| l.id))
            .chain(doc.dimensions.iter().map(|d| d.id))
            .chain(doc.asset_instances.iter().map(|a| a.id))
            .collect();

        let mut stripped = doc.clone();
        let mut taken = Vec::new();
        for id in &ids {
            let entity = stripped
                .remove_entity(*id)
                .unwrap_or_else(|| panic!("seed {seed}: {id} must be removable"));
            assert_eq!(entity.id(), *id);
            taken.push(entity);
        }
        assert_eq!(stripped.entity_count(), 0, "seed {seed}: nothing left");

        for entity in taken {
            stripped.insert_entity(entity);
        }
        assert_eq!(
            stripped.entity_count(),
            doc.entity_count(),
            "seed {seed}: every entity came back"
        );
        // Every entity is findable again, which is the property that matters
        // rather than collection ORDER — C1 says order is not semantic.
        for id in &ids {
            assert_eq!(
                stripped.entity(*id),
                doc.entity(*id),
                "seed {seed}: {id} must be restored identically"
            );
        }
    });
}

/// A named defect: how to injure a document, and which error must result.
///
/// `inject` returns false when the generated document has nothing of that
/// shape to injure, so the case is skipped rather than faked.
type Defect = (
    &'static str,
    fn(&mut Document, &mut Rng) -> bool,
    fn(&ValidationError) -> bool,
);

/// One injected defect, and the variant it must produce.
///
/// The property is not "some error appears" — that passes for a validator
/// that reports the wrong thing. It is "the error NAMING THIS DEFECT appears",
/// which is what a person reading the diagnostic needs.
fn defects() -> Vec<Defect> {
    vec![
        (
            "dangling level on a label",
            |doc, rng| {
                if doc.labels.is_empty() {
                    return false;
                }
                doc.labels[0].level_id = rng.uuid();
                true
            },
            |e| {
                matches!(
                    e,
                    ValidationError::TierBDanglingLevelRef { kind: "label", .. }
                )
            },
        ),
        (
            "malformed digest",
            |doc, _| {
                if doc.asset_instances.is_empty() {
                    return false;
                }
                doc.asset_instances[0].digest = "sha256:nope".to_owned();
                true
            },
            |e| matches!(e, ValidationError::MalformedDigest { .. }),
        ),
        (
            "non-positive scale",
            |doc, _| {
                if doc.asset_instances.is_empty() {
                    return false;
                }
                doc.asset_instances[0].transform.scale_uniform = 0.0;
                true
            },
            |e| {
                matches!(
                    e,
                    ValidationError::InvalidInstanceTransform {
                        field: "scaleUniform",
                        ..
                    }
                )
            },
        ),
        (
            "duplicate style token",
            |doc, _| {
                if doc.walls.is_empty() {
                    return false;
                }
                doc.walls[0].style_refs = vec!["S0".to_owned(), "S0".to_owned()];
                true
            },
            |e| matches!(e, ValidationError::DuplicateStyleRef { .. }),
        ),
        (
            "empty style token",
            |doc, _| {
                if doc.openings.is_empty() {
                    return false;
                }
                doc.openings[0].style_refs = vec![String::new()];
                true
            },
            |e| {
                matches!(
                    e,
                    ValidationError::EmptyStyleRef {
                        kind: "opening",
                        ..
                    }
                )
            },
        ),
        (
            "out-of-domain coordinate",
            |doc, _| {
                if doc.dimensions.is_empty() {
                    return false;
                }
                doc.dimensions[0].start_mm = [COORD_BOUND_MM * 3.0, 0.0];
                true
            },
            |e| {
                matches!(
                    e,
                    ValidationError::TierBCoordinateOutOfBounds {
                        kind: "dimension",
                        ..
                    }
                )
            },
        ),
        (
            "over-long display string",
            |doc, _| {
                if doc.asset_instances.is_empty() {
                    return false;
                }
                doc.asset_instances[0].display_name =
                    Some("x".repeat(DISPLAY_STRING_CAP_BYTES + 1));
                true
            },
            |e| matches!(e, ValidationError::DisplayStringTooLong { .. }),
        ),
        (
            "duplicate id across kinds",
            |doc, _| {
                if doc.labels.is_empty() || doc.dimensions.is_empty() {
                    return false;
                }
                doc.dimensions[0].id = doc.labels[0].id;
                true
            },
            |e| matches!(e, ValidationError::DuplicateId { .. }),
        ),
    ]
}

#[test]
fn an_injected_defect_is_always_reported_by_the_rule_that_names_it() {
    let mut exercised = vec![0usize; defects().len()];
    for seed in 0..CASES {
        for (index, (what, inject, expect)) in defects().into_iter().enumerate() {
            let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xDEAD_BEEF);
            let mut doc = valid_document(&mut rng);
            // Only inject where the document has something to injure; a
            // generated document may hold no labels at all.
            if !inject(&mut doc, &mut rng) {
                continue;
            }
            exercised[index] += 1;
            let errors = doc.validate();
            assert!(
                errors.iter().any(expect),
                "seed {seed}: {what} was not reported by its own rule; got {errors:?}"
            );
        }
    }
    // A property test that silently skipped every case would pass while
    // proving nothing, so the coverage is asserted rather than assumed.
    for (index, (what, _, _)) in defects().into_iter().enumerate() {
        assert!(
            exercised[index] >= 20,
            "{what} was only exercised {} times; the generator is not producing \
             the shape this defect needs",
            exercised[index]
        );
    }
}

#[test]
fn validation_reports_every_violation_rather_than_the_first() {
    // `Document::validate` promises ALL violations, and a fixer who gets one
    // at a time rediscovers the same file for an afternoon.
    let mut seen_multi = 0;
    for seed in 0..CASES {
        let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) ^ 0xDEAD_BEEF);
        let mut doc = valid_document(&mut rng);
        let mut injected = 0;
        if !doc.labels.is_empty() {
            doc.labels[0].level_id = rng.uuid();
            injected += 1;
        }
        if !doc.asset_instances.is_empty() {
            doc.asset_instances[0].digest = "nope".to_owned();
            injected += 1;
        }
        if !doc.dimensions.is_empty() {
            doc.dimensions[0].end_mm = [f64::NAN, 0.0];
            injected += 1;
        }
        if injected < 2 {
            continue;
        }
        seen_multi += 1;
        assert!(
            doc.validate().len() >= injected,
            "seed {seed}: {injected} defects must produce at least {injected} errors"
        );
    }
    assert!(
        seen_multi >= 20,
        "too few multi-defect cases to prove anything"
    );
}
