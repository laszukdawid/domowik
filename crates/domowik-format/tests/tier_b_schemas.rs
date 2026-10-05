//! The FORMAT-SPEC §2.3 entity table, checked against the implementation.
//!
//! Two things are pinned here that prose cannot pin on its own: the
//! specification's own literal examples, copied in byte for byte so editing
//! the schema without the document fails; and the STRUCTURAL constraints of
//! the Tier B kinds, which are the only constraints those kinds have.
//!
//! Tier B is a wire shape, not a behaviour. FORMAT-SPEC says so in terms —
//! "structural bounds only: finiteness, L19 string caps, §3.1 parameter
//! references" — because no normative section defines what a label, a
//! dimension or an asset instance MEANS yet. Nothing here invents one.
//!
//! A separate file rather than the crate's inline module, deliberately: issue
//! 0030's join-validation repair edits the same `lib.rs`, and two owners in
//! one test module is a conflict nobody learns anything from.

use domowik_format::*;
use uuid::Uuid;

/// Every number compared as binary64 rather than by JSON spelling.
///
/// §3.2 says an authoritative number IS a finite binary64, and §3.3 says
/// whitespace and key order are insignificant. The examples write `3000`
/// where serde writes `3000.0`; treating those as different values would
/// assert a spelling the specification declines to fix.
fn normalise(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Number(n) => {
            if let Some(v) = n.as_f64() {
                *value = serde_json::json!(v);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(normalise),
        serde_json::Value::Object(fields) => fields.values_mut().for_each(normalise),
        _ => {}
    }
}

fn round_trips(literal: &str, expected_tag: &str) -> Entity {
    let entity: Entity = serde_json::from_str(literal)
        .unwrap_or_else(|e| panic!("the spec example must parse: {e}\n{literal}"));
    assert_eq!(entity.type_name(), expected_tag);
    let mut back: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&entity).expect("serialises"))
            .expect("valid json");
    let mut want: serde_json::Value = serde_json::from_str(literal).expect("valid json");
    normalise(&mut back);
    normalise(&mut want);
    assert_eq!(
        back, want,
        "the example must survive a round trip unchanged"
    );
    entity
}

#[test]
fn the_spec_label_example_round_trips() {
    let entity = round_trips(
        r#"{"id":"019534c7-0000-7000-8000-000000000007","type":"label","levelId":"019534c7-0000-7000-8000-000000000001","anchorMm":[3000,1250],"name":"Kitchen"}"#,
        "label",
    );
    let Entity::Label(label) = entity else {
        panic!("the `label` tag must select the label variant");
    };
    assert_eq!(label.anchor(), Point::new(3000.0, 1250.0));
    assert_eq!(label.name.as_deref(), Some("Kitchen"));
    assert!(
        label.unknown.is_empty(),
        "every field of the example must be TYPED, not swept into `unknown`: {:?}",
        label.unknown
    );
}

#[test]
fn the_spec_dimension_example_round_trips() {
    let entity = round_trips(
        r#"{"id":"019534c7-0000-7000-8000-000000000008","type":"dimension","levelId":"019534c7-0000-7000-8000-000000000001","startMm":[0,0],"endMm":[4000,0]}"#,
        "dimension",
    );
    let Entity::Dimension(d) = entity else {
        panic!("the `dimension` tag must select the dimension variant");
    };
    assert_eq!(d.start(), Point::new(0.0, 0.0));
    assert_eq!(d.end(), Point::new(4000.0, 0.0));
    assert_eq!(d.label_override, None, "the example carries no override");
    assert!(d.unknown.is_empty(), "{:?}", d.unknown);
}

#[test]
fn the_spec_asset_instance_example_round_trips() {
    let entity = round_trips(
        r#"{"id":"019534c7-0000-7000-8000-000000000009","type":"assetInstance","levelId":"019534c7-0000-7000-8000-000000000001","digest":"sha256:9a1f2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9","transform":{"translationMm":[5200,3100],"scaleUniform":1,"rotationDeg":0},"displayName":"Armchair"}"#,
        "assetInstance",
    );
    let Entity::AssetInstance(a) = entity else {
        panic!("the `assetInstance` tag must select the asset-instance variant");
    };
    // `origin()` is the accessor a hit path uses, so it is what the test reads.
    assert_eq!(a.origin(), Point::new(5200.0, 3100.0));
    assert_eq!(a.transform.scale_uniform, 1.0);
    assert_eq!(a.transform.rotation_deg, 0.0);
    assert_eq!(a.display_name.as_deref(), Some("Armchair"));
    assert!(is_well_formed_digest(&a.digest));
    assert!(a.unknown.is_empty(), "{:?}", a.unknown);
    assert!(a.transform.unknown.is_empty(), "{:?}", a.transform.unknown);
}

#[test]
fn each_tier_b_kind_serializes_exactly_its_specified_fields() {
    let level = Uuid::from_u128(1);
    let cases: Vec<(Entity, Vec<&str>)> = vec![
        (
            Entity::Label(Label {
                id: Uuid::from_u128(2),
                level_id: level,
                anchor_mm: [1.0, 2.0],
                name: Some("K".to_owned()),
                unknown: Default::default(),
            }),
            vec!["anchorMm", "id", "levelId", "name", "type"],
        ),
        (
            Entity::Dimension(Dimension {
                id: Uuid::from_u128(3),
                level_id: level,
                start_mm: [0.0, 0.0],
                end_mm: [1.0, 0.0],
                label_override: Some("4 m".to_owned()),
                unknown: Default::default(),
            }),
            vec!["endMm", "id", "labelOverride", "levelId", "startMm", "type"],
        ),
        (
            Entity::AssetInstance(AssetInstance {
                id: Uuid::from_u128(4),
                level_id: level,
                digest: format!("sha256:{}", "a".repeat(64)),
                transform: InstanceTransform::default(),
                display_name: Some("Chair".to_owned()),
                unknown: Default::default(),
            }),
            vec![
                "digest",
                "displayName",
                "id",
                "levelId",
                "transform",
                "type",
            ],
        ),
    ];
    for (entity, expected) in cases {
        let tag = entity.type_name();
        let value = serde_json::to_value(&entity).expect("serialises");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, expected, "{tag} carries exactly its §2.3 fields");
    }
}

#[test]
fn absent_optional_fields_are_absent_on_the_wire() {
    // Not written as null: `null` and absent are different inputs, and a
    // writer that emitted null for every unset field would make the
    // absent/empty distinction the format relies on unrepresentable.
    let label = Entity::Label(Label {
        id: Uuid::from_u128(2),
        level_id: Uuid::from_u128(1),
        anchor_mm: [0.0, 0.0],
        name: None,
        unknown: Default::default(),
    });
    let json = serde_json::to_string(&label).expect("serialises");
    assert!(!json.contains("name"), "got {json}");

    // And the empty string is a VALUE, distinct from absent, as the §2.3 row
    // says following the R5.3.3/F37d pattern.
    let Entity::Label(mut l) = label else {
        unreachable!()
    };
    l.name = Some(String::new());
    let json = serde_json::to_string(&Entity::Label(l)).expect("serialises");
    assert!(json.contains(r#""name":"""#), "got {json}");
}

fn doc_with_level() -> (Document, Uuid) {
    let mut doc = Document::new();
    let level = Level::new("Level 1");
    let id = level.id;
    doc.levels.push(level);
    (doc, id)
}

fn valid_instance(level: Uuid) -> AssetInstance {
    AssetInstance {
        id: Uuid::now_v7(),
        level_id: level,
        digest: format!("sha256:{}", "0".repeat(64)),
        transform: InstanceTransform::default(),
        display_name: None,
        unknown: Default::default(),
    }
}

#[test]
fn a_malformed_digest_is_refused_in_every_way_it_can_be_malformed() {
    let (doc, level) = doc_with_level();
    let long = format!("sha256:{}", "0".repeat(65));
    let short = format!("sha256:{}", "0".repeat(63));
    let upper = format!("sha256:{}", "A".repeat(64));
    let nonhex = format!("sha256:{}", "g".repeat(64));
    let prefix = format!("SHA256:{}", "0".repeat(64));
    let bare = "0".repeat(64);
    for bad in [
        "", "sha256:", "abc", &bare, &short, &long, &upper, &nonhex, &prefix,
    ] {
        assert!(
            !is_well_formed_digest(bad),
            "{bad:?} must not be well formed"
        );
        let mut doc = doc.clone();
        let mut instance = valid_instance(level);
        instance.digest = bad.to_owned();
        let id = instance.id;
        doc.asset_instances.push(instance);
        assert!(
            doc.validate().iter().any(
                |e| matches!(e, ValidationError::MalformedDigest { id: got, .. } if *got == id)
            ),
            "{bad:?} must be refused"
        );
    }
    // The control: a well-formed digest validates clean.
    let mut doc = doc.clone();
    doc.asset_instances.push(valid_instance(level));
    assert_eq!(doc.validate(), vec![]);
}

#[test]
fn the_instance_transform_is_checked_for_what_the_spec_states_and_no_more() {
    let (doc, level) = doc_with_level();
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut doc = doc.clone();
        let mut instance = valid_instance(level);
        instance.transform.scale_uniform = bad;
        doc.asset_instances.push(instance);
        assert!(
            doc.validate().iter().any(|e| matches!(
                e,
                ValidationError::InvalidInstanceTransform {
                    field: "scaleUniform",
                    ..
                }
            )),
            "scaleUniform {bad} must be refused"
        );
    }
    // rotationDeg: finite, and NOTHING ELSE. R4.0.4's unsigned [0, 180] is an
    // openings rule about a hinged leaf; borrowing it here would invent a
    // constraint §2.3 does not state, so these must all be ACCEPTED.
    for legal in [-720.0, -0.5, 0.0, 90.0, 359.9, 1000.0] {
        let mut doc = doc.clone();
        let mut instance = valid_instance(level);
        instance.transform.rotation_deg = legal;
        doc.asset_instances.push(instance);
        assert_eq!(
            doc.validate(),
            vec![],
            "rotationDeg {legal} is finite and must be accepted"
        );
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut doc = doc.clone();
        let mut instance = valid_instance(level);
        instance.transform.rotation_deg = bad;
        doc.asset_instances.push(instance);
        assert!(
            doc.validate().iter().any(|e| matches!(
                e,
                ValidationError::InvalidInstanceTransform {
                    field: "rotationDeg",
                    ..
                }
            )),
            "rotationDeg {bad} must be refused"
        );
    }
}

#[test]
fn tier_b_coordinates_are_bounded_and_tier_b_strings_are_capped() {
    let (doc, level) = doc_with_level();
    let over = COORD_BOUND_MM * 2.0;

    let mut d = doc.clone();
    d.labels.push(Label {
        id: Uuid::now_v7(),
        level_id: level,
        anchor_mm: [over, 0.0],
        name: None,
        unknown: Default::default(),
    });
    assert!(d.validate().iter().any(|e| matches!(
        e,
        ValidationError::TierBCoordinateOutOfBounds {
            kind: "label",
            field: "anchorMm",
            ..
        }
    )));

    let mut d = doc.clone();
    d.dimensions.push(Dimension {
        id: Uuid::now_v7(),
        level_id: level,
        start_mm: [0.0, 0.0],
        end_mm: [f64::NAN, 0.0],
        label_override: None,
        unknown: Default::default(),
    });
    assert!(d.validate().iter().any(|e| matches!(
        e,
        ValidationError::TierBCoordinateOutOfBounds {
            kind: "dimension",
            field: "endMm",
            ..
        }
    )));

    // L19 is a BYTE cap, and the boundary is where a character count would
    // differ: 1,024 two-byte characters are 2,048 bytes and must be refused.
    let mut d = doc.clone();
    d.labels.push(Label {
        id: Uuid::now_v7(),
        level_id: level,
        anchor_mm: [0.0, 0.0],
        name: Some("é".repeat(1_024)),
        unknown: Default::default(),
    });
    assert!(
        d.validate().iter().any(|e| matches!(
            e,
            ValidationError::DisplayStringTooLong {
                field: "name",
                bytes: 2048,
                ..
            }
        )),
        "the cap counts BYTES, not characters"
    );

    for (len, ok) in [
        (DISPLAY_STRING_CAP_BYTES, true),
        (DISPLAY_STRING_CAP_BYTES + 1, false),
    ] {
        let mut d = doc.clone();
        d.asset_instances.push(AssetInstance {
            display_name: Some("x".repeat(len)),
            ..valid_instance(level)
        });
        assert_eq!(
            d.validate().is_empty(),
            ok,
            "displayName of {len} bytes: expected ok={ok}"
        );
    }
}

#[test]
fn every_tier_b_kind_reports_a_dangling_level() {
    let (doc, level) = doc_with_level();
    let ghost = Uuid::now_v7();
    let mut d = doc.clone();
    d.labels.push(Label {
        id: Uuid::now_v7(),
        level_id: ghost,
        anchor_mm: [0.0, 0.0],
        name: None,
        unknown: Default::default(),
    });
    d.dimensions.push(Dimension {
        id: Uuid::now_v7(),
        level_id: ghost,
        start_mm: [0.0, 0.0],
        end_mm: [1.0, 0.0],
        label_override: None,
        unknown: Default::default(),
    });
    d.asset_instances.push(AssetInstance {
        level_id: ghost,
        ..valid_instance(level)
    });
    let kinds: Vec<&str> = d
        .validate()
        .iter()
        .filter_map(|e| match e {
            ValidationError::TierBDanglingLevelRef { kind, .. } => Some(*kind),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec!["label", "dimension", "assetInstance"]);
}

#[test]
fn style_refs_reject_empty_tokens_and_duplicates_on_both_kinds() {
    let (base, level) = doc_with_level();
    let wall = |refs: Vec<&str>| {
        let mut w = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        );
        w.style_refs = refs.into_iter().map(str::to_owned).collect();
        w
    };

    let mut d = base.clone();
    d.walls.push(wall(vec!["S0", ""]));
    assert!(d
        .validate()
        .iter()
        .any(|e| matches!(e, ValidationError::EmptyStyleRef { kind: "wall", .. })));

    let mut d = base.clone();
    d.walls.push(wall(vec!["S0", "S1", "S0"]));
    assert!(d.validate().iter().any(
        |e| matches!(e, ValidationError::DuplicateStyleRef { kind: "wall", token, .. } if token == "S0")
    ));

    let mut d = base.clone();
    let w = wall(vec![]);
    let host = w.id;
    d.walls.push(w);
    let mut opening = Opening::door(host, 0.5, 900.0, 2100.0, 200.0);
    opening.style_refs = vec!["S0".to_owned(), "S0".to_owned()];
    d.openings.push(opening);
    assert!(d.validate().iter().any(|e| matches!(
        e,
        ValidationError::DuplicateStyleRef {
            kind: "opening",
            ..
        }
    )));
}

#[test]
fn style_ref_order_is_not_semantic() {
    // C3: a set is serialized as an array under a canonical order, and
    // validators MUST treat permutations as equivalent. Two documents that
    // differ only in that order are the same document, so both validate.
    let (base, level) = doc_with_level();
    for order in [vec!["S0", "S1"], vec!["S1", "S0"]] {
        let mut d = base.clone();
        let mut w = Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(1000.0, 0.0),
            200.0,
            2500.0,
        );
        w.style_refs = order.iter().map(|s| (*s).to_owned()).collect();
        d.walls.push(w);
        assert_eq!(d.validate(), vec![], "order {order:?} must be accepted");
    }
}

#[test]
fn absent_style_refs_stay_absent_on_the_wire() {
    let wall = Wall::new(
        Uuid::from_u128(1),
        Point::new(0.0, 0.0),
        Point::new(1.0, 0.0),
        200.0,
        2500.0,
    );
    let json = serde_json::to_string(&Entity::Wall(wall)).expect("serialises");
    assert!(!json.contains("styleRefs"), "got {json}");
}

#[test]
fn unknown_fields_survive_a_round_trip_on_every_new_kind() {
    // §8 U2 forbids silent stripping EVERYWHERE, which is why the descriptor
    // inside an asset instance carries its own map as well as the entity.
    let cases = [
        r#"{"id":"019534c7-0000-7000-8000-000000000007","type":"label","levelId":"019534c7-0000-7000-8000-000000000001","anchorMm":[0,0],"futureField":{"x":1}}"#,
        r#"{"id":"019534c7-0000-7000-8000-000000000008","type":"dimension","levelId":"019534c7-0000-7000-8000-000000000001","startMm":[0,0],"endMm":[1,0],"futureField":[1,2]}"#,
        r#"{"id":"019534c7-0000-7000-8000-000000000009","type":"assetInstance","levelId":"019534c7-0000-7000-8000-000000000001","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","transform":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0,"skewDeg":5},"futureField":"keep me"}"#,
    ];
    for literal in cases {
        let entity: Entity = serde_json::from_str(literal).expect("parses");
        let back = serde_json::to_string(&entity).expect("serialises");
        assert!(
            back.contains("futureField") || back.contains("skewDeg"),
            "unknown data was dropped: {back}"
        );
        let again: Entity = serde_json::from_str(&back).expect("reparses");
        assert_eq!(
            again, entity,
            "and the drop would be observable in equality"
        );
    }
}

#[test]
fn an_unknown_descriptor_field_inside_a_transform_survives() {
    let literal = r#"{"id":"019534c7-0000-7000-8000-000000000009","type":"assetInstance","levelId":"019534c7-0000-7000-8000-000000000001","digest":"sha256:0000000000000000000000000000000000000000000000000000000000000000","transform":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0,"skewDeg":5}}"#;
    let Entity::AssetInstance(a) = serde_json::from_str(literal).expect("parses") else {
        unreachable!()
    };
    assert_eq!(
        a.transform.unknown.get("skewDeg"),
        Some(&serde_json::json!(5)),
        "the DESCRIPTOR's unknown map must catch it, not the entity's"
    );
    assert!(a.unknown.is_empty());
}

#[test]
fn the_new_kinds_insert_find_and_remove_through_the_entity_seam() {
    let (mut doc, level) = doc_with_level();
    let built: Vec<Entity> = vec![
        Entity::Label(Label {
            id: Uuid::from_u128(10),
            level_id: level,
            anchor_mm: [1.0, 2.0],
            name: Some("K".to_owned()),
            unknown: Default::default(),
        }),
        Entity::Dimension(Dimension {
            id: Uuid::from_u128(11),
            level_id: level,
            start_mm: [0.0, 0.0],
            end_mm: [1.0, 0.0],
            label_override: None,
            unknown: Default::default(),
        }),
        Entity::AssetInstance(AssetInstance {
            id: Uuid::from_u128(12),
            ..valid_instance(level)
        }),
    ];
    let before = doc.entity_count();
    for entity in &built {
        doc.insert_entity(entity.clone());
    }
    assert_eq!(doc.entity_count(), before + 3, "all three are entities");

    // A second create of the same id REPLACES in place rather than appending,
    // which is what keeps positional numbering stable.
    for entity in &built {
        assert_eq!(
            doc.entity(entity.id()).as_ref(),
            Some(entity),
            "findable by id"
        );
        doc.insert_entity(entity.clone());
    }
    assert_eq!(
        doc.entity_count(),
        before + 3,
        "re-creating an existing id must replace, not append"
    );

    // Then removal, counted down one at a time.
    for (removed, entity) in built.iter().enumerate() {
        let id = entity.id();
        assert_eq!(doc.remove_entity(id).as_ref(), Some(entity));
        assert_eq!(doc.entity(id), None);
        assert_eq!(doc.entity_count(), before + 2 - removed);
    }
    assert_eq!(doc.entity_count(), before);
}

#[test]
fn a_duplicate_id_is_reported_across_the_new_kinds_too() {
    let (mut doc, level) = doc_with_level();
    let clash = Uuid::from_u128(99);
    doc.labels.push(Label {
        id: clash,
        level_id: level,
        anchor_mm: [0.0, 0.0],
        name: None,
        unknown: Default::default(),
    });
    doc.asset_instances.push(AssetInstance {
        id: clash,
        ..valid_instance(level)
    });
    assert!(doc
        .validate()
        .iter()
        .any(|e| matches!(e, ValidationError::DuplicateId { id } if *id == clash)));
}

#[test]
fn asset_instances_are_reachable_by_level_and_by_id() {
    let (mut doc, level) = doc_with_level();
    let other = Level::new("Level 2");
    let other_id = other.id;
    doc.levels.push(other);

    let here = valid_instance(level);
    let here_id = here.id;
    let elsewhere = AssetInstance {
        level_id: other_id,
        ..valid_instance(level)
    };
    doc.asset_instances.push(here);
    doc.asset_instances.push(elsewhere);

    let on_level: Vec<Uuid> = doc.asset_instances_on_level(level).map(|a| a.id).collect();
    assert_eq!(on_level, vec![here_id], "levels never interact");
    assert_eq!(doc.asset_instance(here_id).map(|a| a.id), Some(here_id));
    assert_eq!(doc.asset_instance(Uuid::now_v7()), None);
}
