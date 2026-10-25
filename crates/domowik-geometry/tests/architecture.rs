use domowik_format::*;
use domowik_geometry::{architecture::*, polygon::Polygon};
use uuid::Uuid;

fn rect(a: [f64; 2], b: [f64; 2], width: f64) -> RectFootprint {
    RectFootprint {
        start_mm: a,
        end_mm: b,
        width_mm: width,
    }
}

#[test]
fn hosted_cuts_preserve_holes_multiple_components_and_other_hosts() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    let outer = rectangle(&rect([0.0, 0.0], [4000.0, 0.0], 4000.0));
    let opening = HostedOpening {
        id: Uuid::now_v7(),
        name: "Original test cut".into(),
        host: OpeningHost::LevelFloor(l),
        footprint: rect([1000.0, 0.0], [3000.0, 0.0], 1000.0),
        unknown: Default::default(),
    };
    d.hosted_openings.push(opening);
    let cut = cut_surfaces(&d, OpeningHost::LevelFloor(l), std::slice::from_ref(&outer));
    assert_eq!(cut.len(), 1);
    assert_eq!(cut[0].holes.len(), 1);
    assert_eq!(cut[0].area_mm2(), 14_000_000.0);
    assert_eq!(
        cut_surfaces(
            &d,
            OpeningHost::LevelFloor(Uuid::now_v7()),
            std::slice::from_ref(&outer)
        ),
        vec![outer.clone()]
    );
    d.hosted_openings[0].footprint = rect([1000.0, 0.0], [3000.0, 0.0], 6000.0);
    let cut = cut_surfaces(&d, OpeningHost::LevelFloor(l), std::slice::from_ref(&outer));
    assert_eq!(cut.len(), 2);
    assert_eq!(cut.iter().map(Polygon::area_mm2).sum::<f64>(), 8_000_000.0);
    d.hosted_openings[0].footprint = rect([-1000.0, 0.0], [5000.0, 0.0], 6000.0);
    assert!(cut_surfaces(&d, OpeningHost::LevelFloor(l), &[outer]).is_empty());
}

#[test]
fn rotated_rectangles_and_cut_output_are_deterministic_at_large_coordinates() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    let f = rect(
        [9_900_000.0, 9_900_000.0],
        [9_903_000.0, 9_904_000.0],
        1000.0,
    );
    assert!(f.valid());
    let p = rectangle(&f);
    assert!((p.area_mm2() - 5_000_000.0).abs() < 0.01);
    d.hosted_openings.push(HostedOpening {
        id: Uuid::now_v7(),
        name: "Test rotated cut".into(),
        host: OpeningHost::LevelFloor(l),
        footprint: rect(
            [9_901_000.0, 9_901_000.0],
            [9_902_000.0, 9_902_000.0],
            300.0,
        ),
        unknown: Default::default(),
    });
    let a = cut_surfaces(&d, OpeningHost::LevelFloor(l), std::slice::from_ref(&p));
    let b = cut_surfaces(&d, OpeningHost::LevelFloor(l), &[p]);
    assert_eq!(a, b);
}
