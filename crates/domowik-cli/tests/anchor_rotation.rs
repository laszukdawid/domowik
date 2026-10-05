//! BEHAVIOR-SPEC §2.5's anchor rotation, in export.
//!
//! The anchor carried a `rotationDeg` that both exporters ignored. Storing a
//! rotation and silently not applying it is worse than rejecting one: a user
//! aligns a scan to a wall, saves, exports, and the export disagrees with the
//! editor with nothing to say why.

use domowik_cli::png::anchor_to_plan;

/// §2.5's own worked example, read out of the specification rather than
/// retyped (ED-0089).
#[test]
fn the_worked_example_from_the_specification_holds() {
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/BEHAVIOR-SPEC.md"),
    )
    .expect("BEHAVIOR-SPEC.md");
    let from = spec
        .find("A vector example:")
        .expect("§2.5 carries a worked example");
    let example: String = spec[from..from + 240]
        .split("- ")
        .next()
        .unwrap()
        .to_owned();

    // The example: t = (0, 0), s = 10, θ = 90° maps image (100 px, 0 px) to
    // plan (0 mm, 1000 mm). Each number is taken from the sentence above.
    let numbers: Vec<f64> = example
        .replace(['(', ')', ',', '°'], " ")
        .split_whitespace()
        .filter_map(|t| t.trim_end_matches("px").trim_end_matches("mm").parse().ok())
        .collect();
    // t.x, t.y, s, θ, q.x, q.y, p.x, p.y
    assert_eq!(numbers.len(), 8, "parsed {numbers:?} from {example:?}");
    let (tx, ty, s, theta) = (numbers[0], numbers[1], numbers[2], numbers[3]);
    let (qx, qy) = (numbers[4], numbers[5]);
    let (px, py) = (numbers[6], numbers[7]);

    let got = anchor_to_plan(tx, ty, s, theta, qx, qy);
    assert!(
        (got.x - px).abs() < 1e-9 && (got.y - py).abs() < 1e-9,
        "§2.5's example maps to {got:?}, not ({px}, {py})"
    );
}

#[test]
fn an_unrotated_anchor_still_places_the_image_by_its_top_left() {
    // The regression that matters: every document written so far assumes
    // θ = 0 anchors the top-left corner and grows the image downward in plan.
    let (tx, ty, s) = (300.0, 900.0, 10.0);
    let top_left = anchor_to_plan(tx, ty, s, 0.0, 0.0, 0.0);
    let bottom_right = anchor_to_plan(tx, ty, s, 0.0, 40.0, 20.0);

    assert_eq!((top_left.x, top_left.y), (tx, ty));
    assert_eq!(bottom_right.x, tx + 40.0 * s);
    assert_eq!(bottom_right.y, ty - 20.0 * s);
}

#[test]
fn rotation_is_counter_clockwise_in_plan() {
    // §2.5: "θ in degrees counter-clockwise". A quarter turn sends a point on
    // plan +X to plan +Y.
    let on_plus_x = anchor_to_plan(0.0, 0.0, 1.0, 0.0, 100.0, 0.0);
    assert!((on_plus_x.x - 100.0).abs() < 1e-9 && on_plus_x.y.abs() < 1e-9);

    let turned = anchor_to_plan(0.0, 0.0, 1.0, 90.0, 100.0, 0.0);
    assert!(
        turned.x.abs() < 1e-9 && (turned.y - 100.0).abs() < 1e-9,
        "a CCW quarter turn should reach plan +Y, got {turned:?}"
    );

    // And a clockwise quarter turn reaches plan −Y.
    let back = anchor_to_plan(0.0, 0.0, 1.0, -90.0, 100.0, 0.0);
    assert!(
        back.x.abs() < 1e-9 && (back.y + 100.0).abs() < 1e-9,
        "{back:?}"
    );
}

#[test]
fn a_full_turn_returns_the_point_and_the_transform_is_a_similarity() {
    // Distances scale by exactly s at every angle: the anchor is a SIMILARITY
    // transform (§2.5), so a rotation must not stretch a scan.
    let s = 7.0;
    for theta in [0.0, 17.0, 90.0, 180.0, 270.0, 359.9] {
        let a = anchor_to_plan(50.0, 60.0, s, theta, 0.0, 0.0);
        let b = anchor_to_plan(50.0, 60.0, s, theta, 30.0, 40.0);
        let d = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        // The image-space distance from (0,0) to (30,40) is 50.
        assert!(
            (d - 50.0 * s).abs() < 1e-9,
            "θ = {theta} scaled a 50 px span to {d}, not {}",
            50.0 * s
        );
    }

    let full = anchor_to_plan(1.0, 2.0, 3.0, 360.0, 10.0, 20.0);
    let none = anchor_to_plan(1.0, 2.0, 3.0, 0.0, 10.0, 20.0);
    assert!((full.x - none.x).abs() < 1e-9 && (full.y - none.y).abs() < 1e-9);
}
