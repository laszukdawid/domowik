//! ASSET-STRATEGY §10.2 descriptor validation and the FORMAT-SPEC §7.4
//! `assets/descriptors.json` entry.

use domowik_assets::descriptor::{
    l19, AssetDescriptor, DescriptorError, DescriptorTable, Extent, Hint, License, Provenance,
    Requirement,
};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn valid() -> AssetDescriptor {
    AssetDescriptor {
        digest: DIGEST.to_owned(),
        byte_length: 2048,
        media_type: "image/png".to_owned(),
        purpose: "background-image".to_owned(),
        encoding: "identity".to_owned(),
        requirement: Requirement::Required,
        provenance: Provenance {
            display_name: Some("Wood floor tile".to_owned()),
            source_description: Some("user import".to_owned()),
            author: Some(String::new()),
            acquired_at: Some("2026-08-24T00:00:00Z".to_owned()),
        },
        license: None,
        placeholder_dimensions: None,
        hints: Vec::new(),
        unknown: serde_json::Map::new(),
    }
}

fn hint(url: &str) -> Hint {
    Hint {
        url: url.to_owned(),
        added_at: "2026-09-12T00:00:00Z".to_owned(),
        last_verified_at: None,
        expires_at: None,
    }
}

#[test]
fn the_normative_example_from_the_format_spec_validates() {
    // FORMAT-SPEC §7.4's descriptor example, read out of the specification
    // rather than retyped: ED-0089 forbids restating a specification's values
    // as a second literal, and retyping the example would test my transcription
    // of it against itself.
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/FORMAT-SPEC.md"),
    )
    .expect("FORMAT-SPEC.md");
    let marker = "`assets/descriptors.json` holds the descriptor documents";
    let from = spec.find(marker).expect("the §7.4 descriptors paragraph");
    let block_start = spec[from..].find("```json").expect("an example block") + from + 7;
    let block_end = spec[block_start..].find("```").expect("the block ends") + block_start;
    let object: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(spec[block_start..block_end].trim()).expect("the example is JSON");

    let table = DescriptorTable::from_object_strict(&object).expect("the example validates");
    assert_eq!(table.len(), 1);
    let descriptor = table.iter().next().expect("one descriptor");
    assert_eq!(descriptor.requirement, Requirement::Required);
    assert_eq!(descriptor.purpose, "background-image");
    // The provenance member names come from the specification, not from me.
    assert_eq!(
        descriptor.provenance.display_name.as_deref(),
        Some("Wood floor tile")
    );
    assert_eq!(
        descriptor.provenance.source_description.as_deref(),
        Some("user import")
    );
    // And it round-trips back to the same object.
    assert_eq!(table.to_object(), object);
}

#[test]
fn a_valid_descriptor_has_no_faults() {
    assert_eq!(valid().validate(), Vec::new());
}

#[test]
fn every_fault_is_reported_not_just_the_first() {
    // A hand-edited descriptor is usually wrong in several ways at once.
    let mut d = valid();
    d.digest = "sha256:NOTHEX".to_owned();
    d.media_type = "imagepng".to_owned();
    d.purpose = String::new();
    d.provenance.acquired_at = Some("yesterday".to_owned());

    let faults = d.validate();
    assert!(
        faults.len() >= 4,
        "only {} fault(s): {faults:?}",
        faults.len()
    );
    assert!(faults
        .iter()
        .any(|f| matches!(f, DescriptorError::Digest { .. })));
    assert!(faults
        .iter()
        .any(|f| matches!(f, DescriptorError::MediaType { .. })));
    assert!(faults
        .iter()
        .any(|f| matches!(f, DescriptorError::Empty { field: "purpose" })));
    assert!(faults
        .iter()
        .any(|f| matches!(f, DescriptorError::Timestamp { .. })));
}

#[test]
fn the_row_l19_limits_are_the_ones_the_format_spec_states() {
    // ED-0089 again: parsed out of the L19 row, so changing a constant without
    // changing the specification fails here.
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/FORMAT-SPEC.md"),
    )
    .expect("FORMAT-SPEC.md");
    let row = spec
        .lines()
        .find(|l| l.starts_with("| L19 |"))
        .expect("FORMAT-SPEC §9.3 has an L19 row");

    let number_after = |needle: &str| -> usize {
        let from = row.find(needle).expect(needle) + needle.len();
        let digits: String = row[from..]
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .filter(|c| c.is_ascii_digit())
            .collect();
        digits.parse().expect("a number follows")
    };
    assert_eq!(l19::MEDIA_TYPE_BYTES, number_after("mediaType"));
    assert_eq!(l19::URI_BYTES, number_after("URI/hint URL"));
    assert_eq!(l19::DISPLAY_BYTES, number_after("display strings"));
    assert_eq!(l19::DIGEST_CHARS, number_after("digest identifier exactly"));
    // And the digest length the constant names is the length of a real one.
    assert_eq!(DIGEST.chars().count(), l19::DIGEST_CHARS);
}

#[test]
fn oversized_strings_are_refused_at_their_row_l19_boundary() {
    // Derived from the constants, so the boundary moves with the row rather
    // than being asserted twice.
    let mut d = valid();
    d.media_type = format!("image/{}", "x".repeat(l19::MEDIA_TYPE_BYTES));
    d.provenance.display_name = Some("n".repeat(l19::DISPLAY_BYTES + 1));
    d.hints = vec![hint(&format!(
        "https://example.com/{}",
        "p".repeat(l19::URI_BYTES)
    ))];

    let faults = d.validate();
    let over: Vec<&str> = faults
        .iter()
        .filter_map(|f| match f {
            DescriptorError::TooLong { field, .. } => Some(*field),
            _ => None,
        })
        .collect();
    assert!(over.contains(&"mediaType"));
    assert!(over.contains(&"provenance.displayName"));
    assert!(over.contains(&"hints[].url"));

    // Exactly at the limit is accepted: L19 states a maximum, not an exclusive
    // bound.
    let mut at_limit = valid();
    at_limit.provenance.display_name = Some("n".repeat(l19::DISPLAY_BYTES));
    assert_eq!(at_limit.validate(), Vec::new());
}

#[test]
fn hints_have_set_semantics_over_their_normalised_form() {
    // §10.5: "set semantics, duplicate URLs rejected". §10.8 folds scheme and
    // host, so two spellings of one URL are one hint.
    let mut d = valid();
    d.hints = vec![
        hint("https://example.com/scan.png"),
        hint("HTTPS://Example.COM./scan.png"),
    ];
    assert!(d
        .validate()
        .iter()
        .any(|f| matches!(f, DescriptorError::DuplicateHint { .. })));

    // Genuinely different URLs are fine.
    let mut d = valid();
    d.hints = vec![
        hint("https://example.com/a.png"),
        hint("https://example.com/b.png"),
    ];
    assert_eq!(d.validate(), Vec::new());
}

#[test]
fn a_credential_bearing_hint_fails_descriptor_validation() {
    // The §10.8 rejection has to reach descriptor validation, or a package
    // carrying a userinfo URL parses cleanly and only fails later.
    let mut d = valid();
    d.hints = vec![hint("https://user:pass@example.com/scan.png")];
    let faults = d.validate();
    assert!(faults
        .iter()
        .any(|f| matches!(f, DescriptorError::Hint { .. })));
}

#[test]
fn a_placeholder_extent_must_be_a_box_a_renderer_can_draw() {
    for bad in [f64::NAN, f64::INFINITY, -1.0] {
        let mut d = valid();
        d.placeholder_dimensions = Some(Extent {
            width_mm: bad,
            height_mm: 100.0,
            depth_mm: 100.0,
        });
        assert!(
            d.validate().iter().any(|f| matches!(
                f,
                DescriptorError::Extent {
                    axis: "widthMm",
                    ..
                }
            )),
            "{bad} was accepted as a box side"
        );
    }
    let mut d = valid();
    d.placeholder_dimensions = Some(Extent {
        width_mm: 0.0,
        height_mm: 750.0,
        depth_mm: 400.0,
    });
    assert_eq!(d.validate(), Vec::new(), "a flat extent is a legal box");
}

#[test]
fn a_table_key_that_disagrees_with_its_descriptor_is_a_fault() {
    // §7.4 says the object is keyed by digest. Trusting the key would look up
    // a descriptor for other bytes; trusting the body would lose the entry the
    // key named. Neither is safe, so it is refused.
    let other = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut object = serde_json::Map::new();
    object.insert(
        other.to_owned(),
        serde_json::to_value(valid()).expect("serialises"),
    );
    let faults = DescriptorTable::from_object_strict(&object).expect_err("mismatched key");
    assert!(matches!(
        faults.as_slice(),
        [DescriptorError::KeyMismatch { .. }]
    ));
}

#[test]
fn a_descriptor_that_does_not_parse_is_reported_against_its_key() {
    let mut object = serde_json::Map::new();
    object.insert(DIGEST.to_owned(), serde_json::json!({ "digest": 7 }));
    let faults = DescriptorTable::from_object_strict(&object).expect_err("unparsable");
    assert!(matches!(
        faults.as_slice(),
        [DescriptorError::Unparsable { key, .. }] if key == DIGEST
    ));
}

#[test]
fn a_table_round_trips_and_keeps_unknown_fields() {
    // Row L32 and §8 U2: silent stripping is forbidden on the write-back path.
    let mut value = serde_json::to_value(valid()).expect("serialises");
    value
        .as_object_mut()
        .expect("object")
        .insert("futureField".to_owned(), serde_json::json!([1, 2, 3]));
    let mut object = serde_json::Map::new();
    object.insert(DIGEST.to_owned(), value);

    let table = DescriptorTable::from_object_strict(&object).expect("valid");
    assert_eq!(table.to_object(), object);
    assert!(table
        .get(DIGEST)
        .expect("present")
        .unknown
        .contains_key("futureField"));
}

#[test]
fn insert_and_remove_key_by_the_descriptors_own_digest() {
    let mut table = DescriptorTable::default();
    assert!(table.is_empty());
    table.insert(valid());
    assert_eq!(table.len(), 1);
    assert!(table.get(DIGEST).is_some());
    assert_eq!(table.as_vec().len(), 1);

    let mut optional = valid();
    optional.requirement = Requirement::Optional;
    optional.license = Some(License {
        expression: "CC-BY-4.0".to_owned(),
        copyright: None,
    });
    table.insert(optional);
    assert_eq!(table.len(), 1, "the same digest replaced rather than added");
    assert_eq!(
        table.get(DIGEST).expect("present").requirement,
        Requirement::Optional
    );

    assert!(table.remove(DIGEST).is_some());
    assert!(table.is_empty());
}

#[test]
fn an_unrecognised_requirement_is_kept_verbatim_and_fails_closed() {
    // §8 U2 and row L32 forbid silent stripping on write-back, so a value a
    // later version defines must survive a read/write cycle here. And the
    // container's open-mode gate reads exactly this field and treats anything
    // unrecognised as undeclared — disagreeing with it on malformed values is
    // the divergence that would be hardest to notice.
    let mut value = serde_json::to_value(valid()).expect("serialises");
    value
        .as_object_mut()
        .expect("object")
        .insert("requirement".to_owned(), serde_json::json!("advisory"));
    let mut object = serde_json::Map::new();
    object.insert(DIGEST.to_owned(), value.clone());

    let table = DescriptorTable::from_object(&object).0;
    let descriptor = table.get(DIGEST).expect("present");
    assert_eq!(
        descriptor.requirement,
        Requirement::Undeclared(serde_json::json!("advisory"))
    );
    assert_eq!(descriptor.requirement.as_str(), Some("advisory"));
    // Fail closed: an unknown level is never quietly more permissive than a
    // known one.
    assert!(descriptor.requirement.blocks_editing());
    assert!(!Requirement::Optional.blocks_editing());
    assert!(Requirement::Required.blocks_editing());

    // And it round-trips as written.
    assert_eq!(table.to_object(), object);
}

#[test]
fn the_media_type_is_sniffed_from_content_not_from_a_name() {
    use domowik_assets::descriptor::sniff_media_type;

    // Real leading bytes, not a guess at them.
    assert_eq!(
        sniff_media_type(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"),
        "image/png"
    );
    assert_eq!(sniff_media_type(&[0xFF, 0xD8, 0xFF, 0xE0]), "image/jpeg");
    assert_eq!(sniff_media_type(b"GIF89a..."), "image/gif");
    assert_eq!(sniff_media_type(b"RIFF\0\0\0\0WEBPVP8 "), "image/webp");
    assert_eq!(sniff_media_type(b"<svg xmlns=..."), "image/svg+xml");

    // Bytes nothing here identifies claim nothing.
    assert_eq!(
        sniff_media_type(b"just some text"),
        "application/octet-stream"
    );
    assert_eq!(sniff_media_type(b""), "application/octet-stream");

    // A near miss is not a match: one wrong byte in the PNG signature is not
    // a PNG, and saying it is would make §9.6's decodability row lie.
    assert_eq!(
        sniff_media_type(b"\x89PNG\r\n\x1a\x00"),
        "application/octet-stream"
    );
}

#[test]
fn an_imported_descriptor_validates_and_records_the_import() {
    let bytes = b"\x89PNG\r\n\x1a\nrest of the file";
    let digest = domowik_container::digest_of(bytes);
    let descriptor = AssetDescriptor::for_import(
        digest.clone(),
        bytes.len() as u64,
        bytes,
        "background-image",
        Requirement::Optional,
        Some("survey.png".to_owned()),
        Some("2026-09-12T00:00:00Z".to_owned()),
    );

    assert_eq!(descriptor.validate(), Vec::new());
    assert_eq!(descriptor.digest, digest);
    assert_eq!(descriptor.byte_length, bytes.len() as u64);
    assert_eq!(descriptor.media_type, "image/png");
    // §10.1: this crate never transcodes, so the stored form is the imported
    // form.
    assert_eq!(descriptor.encoding, "identity");
    assert_eq!(descriptor.requirement, Requirement::Optional);
    assert_eq!(
        descriptor.provenance.display_name.as_deref(),
        Some("survey.png")
    );
    // Nothing is invented: no licence is claimed for a user's own file.
    assert!(descriptor.license.is_none());
    assert!(descriptor.hints.is_empty());
}

#[test]
fn an_import_whose_type_is_unrecognised_still_produces_a_valid_descriptor() {
    // §10.2 requires a mediaType, and refusing to import bytes nobody
    // recognises would make the asset store useless for exactly the opaque
    // content §6 says to preserve.
    let bytes = b"not a format this build knows";
    let descriptor = AssetDescriptor::for_import(
        domowik_container::digest_of(bytes),
        bytes.len() as u64,
        bytes,
        "attachment",
        Requirement::Optional,
        None,
        None,
    );
    assert_eq!(descriptor.validate(), Vec::new());
    assert_eq!(descriptor.media_type, "application/octet-stream");
}

/// The complete `requirement` contract, as a table.
///
/// This exists because issue 0029's open-mode gate reads this one field and
/// recognises exactly `"required"` and `"optional"`, treating everything else
/// — including absent — as undeclared and failing closed. If this crate
/// normalised case, trimmed whitespace, or accepted a synonym, the two would
/// disagree on precisely the malformed values, and silently. Each row below is
/// a value that must NOT be normalised.
#[test]
fn the_requirement_field_is_matched_exactly_and_never_normalised() {
    let cases: Vec<(serde_json::Value, bool, bool)> = vec![
        // (value, blocks_editing, is a recognised level)
        (serde_json::json!("required"), true, true),
        (serde_json::json!("optional"), false, true),
        // No case folding.
        (serde_json::json!("Required"), true, false),
        (serde_json::json!("OPTIONAL"), true, false),
        // No whitespace trimming.
        (serde_json::json!(" required "), true, false),
        (serde_json::json!("optional\n"), true, false),
        // No synonyms.
        (serde_json::json!("mandatory"), true, false),
        (serde_json::json!("advisory"), true, false),
        // Not even a string.
        (serde_json::json!(null), true, false),
        (serde_json::json!(7), true, false),
        (serde_json::json!(true), true, false),
        (serde_json::json!({}), true, false),
        (serde_json::json!(""), true, false),
    ];

    for (value, blocks, recognised) in cases {
        let mut body = serde_json::to_value(valid()).expect("serialises");
        body.as_object_mut()
            .expect("object")
            .insert("requirement".to_owned(), value.clone());
        let mut object = serde_json::Map::new();
        object.insert(DIGEST.to_owned(), body.clone());

        // Always parses. The gate must be able to read this field precisely
        // when the descriptor is malformed.
        let (table, faults) = DescriptorTable::from_object(&object);
        let descriptor = table
            .get(DIGEST)
            .unwrap_or_else(|| panic!("{value} made the descriptor unreadable"));

        assert_eq!(
            descriptor.requirement.blocks_editing(),
            blocks,
            "{value} blocks_editing"
        );
        assert_eq!(
            matches!(
                descriptor.requirement,
                Requirement::Required | Requirement::Optional
            ),
            recognised,
            "{value} recognised"
        );
        // Kept verbatim, including its JSON type.
        assert_eq!(table.to_object(), object, "{value} did not round-trip");
        // And an unrecognised level is REPORTED, not swallowed.
        assert_eq!(
            faults
                .iter()
                .any(|f| matches!(f, DescriptorError::UnknownRequirement { .. })),
            !recognised,
            "{value} fault reporting"
        );
    }
}

#[test]
fn an_absent_requirement_parses_blocks_and_round_trips_as_absent() {
    // Issue 0029 treats absent as undeclared and fails closed. This crate used
    // to reject the whole descriptor, which is worse than diverging: the field
    // would have been invisible rather than merely unrecognised.
    let mut body = serde_json::to_value(valid()).expect("serialises");
    body.as_object_mut().expect("object").remove("requirement");
    let mut object = serde_json::Map::new();
    object.insert(DIGEST.to_owned(), body);

    let (table, faults) = DescriptorTable::from_object(&object);
    let descriptor = table.get(DIGEST).expect("an absent field still parses");
    assert_eq!(descriptor.requirement, Requirement::Absent);
    assert!(descriptor.requirement.blocks_editing());
    assert_eq!(descriptor.requirement.as_str(), None);
    // §10.2 makes the field required, so it is a fault — reported, not fatal.
    assert!(faults.iter().any(|f| matches!(
        f,
        DescriptorError::Empty {
            field: "requirement"
        }
    )));
    // Absence round-trips as absence: writing `"requirement": ""` back would
    // invent data, which §8 U2 forbids as firmly as dropping it.
    assert_eq!(table.to_object(), object);
}

#[test]
fn one_malformed_descriptor_does_not_hide_the_others() {
    // The fragility behind the divergence: any fault used to make the WHOLE
    // table unreadable, so a single bad entry took every other descriptor's
    // requirement level with it.
    let other = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut good = valid();
    good.digest = other.to_owned();

    let mut broken = serde_json::to_value(valid()).expect("serialises");
    broken
        .as_object_mut()
        .expect("object")
        .insert("requirement".to_owned(), serde_json::json!(null));

    let mut object = serde_json::Map::new();
    object.insert(DIGEST.to_owned(), broken);
    object.insert(
        other.to_owned(),
        serde_json::to_value(&good).expect("serialises"),
    );

    let (table, faults) = DescriptorTable::from_object(&object);
    assert_eq!(table.len(), 2, "a malformed entry hid a sound one");
    assert_eq!(
        table.get(other).expect("the sound one").requirement,
        Requirement::Required
    );
    assert!(table
        .get(DIGEST)
        .expect("the broken one")
        .requirement
        .blocks_editing());
    assert!(!faults.is_empty(), "the fault must still be reported");
    // The strict door still refuses, for callers that want all or nothing.
    assert!(DescriptorTable::from_object_strict(&object).is_err());
}

#[test]
fn entries_this_version_cannot_interpret_survive_a_round_trip() {
    // §8 U2 forbids silent stripping "everywhere". A descriptor that will not
    // parse, or whose key disagrees with its digest, is not usable here — but
    // it is still someone's data, and a read/write cycle that dropped it would
    // destroy it on the next save.
    let other = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    let mut object = serde_json::Map::new();

    // A sound entry, to prove the carried ones do not displace it.
    object.insert(
        DIGEST.to_owned(),
        serde_json::to_value(valid()).expect("serialises"),
    );
    // Will not parse at all.
    object.insert(
        "sha256:1111111111111111111111111111111111111111111111111111111111111111".to_owned(),
        serde_json::json!({ "digest": 7, "somethingElse": [1, 2] }),
    );
    // Parses, but the key disagrees with the body's digest.
    object.insert(
        other.to_owned(),
        serde_json::to_value(valid()).expect("serialises"),
    );

    let (table, faults) = DescriptorTable::from_object(&object);
    assert_eq!(table.len(), 1, "only the sound entry is interpreted");
    assert!(table.get(DIGEST).is_some());
    assert_eq!(faults.len(), 2, "both problems are reported: {faults:?}");
    assert_eq!(table.carried_keys().count(), 2);

    // The whole object comes back byte-identical.
    assert_eq!(
        table.to_object(),
        object,
        "a read/write cycle lost an entry it did not understand"
    );
}

#[test]
fn a_descriptor_this_build_creates_carries_only_a_recognised_level() {
    // The write side of the contract issue 0029 depends on: descriptors this
    // build ORIGINATES always name one of the two valid strings, so their gate
    // meets an unrecognised level only in a descriptor that came from
    // somewhere else and was carried through.
    for requirement in [Requirement::Required, Requirement::Optional] {
        let bytes = b"\x89PNG\r\n\x1a\n";
        let descriptor = AssetDescriptor::for_import(
            domowik_container::digest_of(bytes),
            bytes.len() as u64,
            bytes,
            "background-image",
            requirement.clone(),
            None,
            None,
        );
        let written = serde_json::to_value(&descriptor).expect("serialises");
        assert_eq!(
            written["requirement"],
            serde_json::json!(requirement
                .as_str()
                .expect("a recognised level is a string"))
        );
        assert!(descriptor.validate().is_empty());
    }
}

#[test]
fn the_plan_footprint_is_width_by_depth_with_height_vertical() {
    // Issue 0012's finding: §9.6 wants a PLAN box and §10.2 does not bind
    // width/height/depth to axes. The naive width-by-height reading is wrong
    // for a floor plan.
    //
    // Their example, kept as the test: a door-like 600 x 2100 x 400 object has
    // a footprint of 600 by 400. Width by height would make it 600 by 2100 —
    // more than three times too deep, and overlapping walls it does not touch.
    let door = Extent {
        width_mm: 600.0,
        height_mm: 2100.0,
        depth_mm: 400.0,
    };
    assert_eq!(door.plan_footprint(), (600.0, 400.0));
    assert_ne!(
        door.plan_footprint(),
        (door.width_mm, door.height_mm),
        "the footprint must not be width by height"
    );

    // Height is free to vary without changing the footprint at all, which is
    // the property that makes it the vertical axis.
    let taller = Extent {
        height_mm: 2400.0,
        ..door
    };
    assert_eq!(taller.plan_footprint(), door.plan_footprint());
}
