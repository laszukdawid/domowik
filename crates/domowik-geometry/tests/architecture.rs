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
fn resizing_stair_edges_projects_the_drag_and_keeps_the_opposite_edge_fixed() {
    let f = rect([100.0, 200.0], [3100.0, 4200.0], 1000.0);
    let original = f.corners();
    for (edge, delta, fixed, width, length) in [
        (
            StairResizeEdge::Left,
            Point::new(580.0, 440.0),
            [0, 1],
            800.0,
            5000.0,
        ),
        (
            StairResizeEdge::Right,
            Point::new(260.0, 680.0),
            [2, 3],
            800.0,
            5000.0,
        ),
        (
            StairResizeEdge::Lower,
            Point::new(300.0, 400.0),
            [1, 2],
            1000.0,
            4500.0,
        ),
        (
            StairResizeEdge::Upper,
            Point::new(-300.0, -400.0),
            [0, 3],
            1000.0,
            4500.0,
        ),
    ] {
        let resized = resize_stair_footprint(&f, edge, delta).unwrap();
        assert!((resized.width_mm - width).abs() < 1e-9);
        assert!((resized.length_mm() - length).abs() < 1e-9);
        let corners = resized.corners();
        for i in fixed {
            for axis in 0..2 {
                assert!((corners[i][axis] - original[i][axis]).abs() < 1e-9);
            }
        }
        assert_eq!(
            resize_stair_footprint(&f, edge, Point::new(0.0, 0.0)).unwrap(),
            f
        );
    }
    for (edge, delta) in [
        (StairResizeEdge::Left, Point::new(1600.0, -1200.0)),
        (StairResizeEdge::Right, Point::new(-1600.0, 1200.0)),
        (StairResizeEdge::Lower, Point::new(3000.0, 4000.0)),
        (StairResizeEdge::Upper, Point::new(-3000.0, -4000.0)),
        (StairResizeEdge::Left, Point::new(f64::NAN, 0.0)),
        (StairResizeEdge::Upper, Point::new(f64::INFINITY, 0.0)),
        (
            StairResizeEdge::Upper,
            Point::new(COORD_BOUND_MM, COORD_BOUND_MM),
        ),
    ] {
        assert!(resize_stair_footprint(&f, edge, delta).is_err());
    }
}

#[test]
fn stair_end_drags_rotate_freely_or_snap_in_all_quadrants_without_moving_the_pivot() {
    let f = rect([100.0, 200.0], [4100.0, 200.0], 900.0);
    for edge in [StairResizeEdge::Lower, StairResizeEdge::Upper] {
        let lower = matches!(edge, StairResizeEdge::Lower);
        let (anchor, grabbed) = if lower {
            (f.end_mm, f.start_mm)
        } else {
            (f.start_mm, f.end_mm)
        };
        for step in -12..12 {
            for deviation in [-4.0_f64, 4.0] {
                let angle = f64::from(step) * std::f64::consts::PI / 12.0;
                let raw = angle + deviation.to_radians();
                let target = [
                    anchor[0] + raw.cos() * 3000.0,
                    anchor[1] + raw.sin() * 3000.0,
                ];
                let delta = Point::new(target[0] - grabbed[0], target[1] - grabbed[1]);
                for snap in [false, true] {
                    let resized = resize_stair_footprint_with_snap(&f, edge, delta, snap).unwrap();
                    let (pivot, end) = if lower {
                        (resized.end_mm, resized.start_mm)
                    } else {
                        (resized.start_mm, resized.end_mm)
                    };
                    assert_eq!(pivot, anchor);
                    assert_eq!(resized.width_mm, f.width_mm);
                    let expected = if snap {
                        let length = 3000.0 * deviation.to_radians().cos();
                        [
                            anchor[0] + angle.cos() * length,
                            anchor[1] + angle.sin() * length,
                        ]
                    } else {
                        target
                    };
                    for i in 0..2 {
                        assert!(
                            (end[i] - expected[i]).abs() < 1e-9,
                            "{edge:?} step={step} snap={snap}"
                        );
                    }
                }
            }
        }
        let mut off_angle = f.clone();
        off_angle.end_mm[1] += 200.0;
        assert_eq!(
            resize_stair_footprint_with_snap(&off_angle, edge, Point::new(0.0, 0.0), true).unwrap(),
            off_angle
        );
        let collapse = Point::new(anchor[0] - grabbed[0], anchor[1] - grabbed[1]);
        assert!(resize_stair_footprint_with_snap(&f, edge, collapse, true).is_err());
    }
    for edge in [StairResizeEdge::Left, StairResizeEdge::Right] {
        let delta = Point::new(300.0, 100.0);
        assert_eq!(
            resize_stair_footprint_with_snap(&f, edge, delta, true).unwrap(),
            resize_stair_footprint(&f, edge, delta).unwrap()
        );
    }
}

#[test]
fn stair_selection_is_exact_level_scoped_and_incrementally_updated() {
    use domowik_geometry::{
        hit::{self, HitClass, HitIndex},
        spatial::Aabb,
    };
    let mut d = Document::with_ground_floor();
    let low = d.levels[0].id;
    let mut upper = Level::new("Synthetic upper");
    upper.elevation_mm = 2800.0;
    let high = upper.id;
    d.levels.push(upper);
    let connection = LevelConnection {
        id: Uuid::now_v7(),
        name: "Test connection".into(),
        from: ConnectionEndpoint {
            level_id: low,
            position_mm: [0.0, 0.0],
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: high,
            position_mm: [3000.0, 4000.0],
            offset_mm: 0.0,
        },
        unknown: Default::default(),
    };
    let s = Staircase {
        id: Uuid::now_v7(),
        name: "Test stair".into(),
        connection_id: connection.id,
        width_mm: 1000.0,
        risers: 17,
        guard_height_mm: 1000.0,
        opening_ids: vec![],
        unknown: Default::default(),
    };
    let id = s.id;
    d.level_connections.push(connection);
    d.staircases.push(s);
    let point = Point::new(1500.0, 2000.0);
    let rect = Aabb {
        min_x: 1450.0,
        min_y: 1950.0,
        max_x: 1550.0,
        max_y: 2050.0,
    };
    for level in [low, high] {
        let mut index = HitIndex::build(&d, level);
        index.validate();
        assert_eq!(index.sync(&d, &[]), 0);
        let hits = index.hit_test(point, 0.0);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].class, HitClass::Staircase);
        assert_eq!(hits[0].entity, id);
        assert_eq!(hits[0].distance_squared, 0.0);
        assert_eq!(index.marquee(rect), vec![id]);
        assert_eq!(index.marquee(rect), hit::marquee(&d, level, rect));
        // Offset 500 mm perpendicular to a 3-4-5 axis: exactly on the side.
        assert_eq!(
            index
                .hit_test_screen(Point::new(1900.0, 1700.0), 0.0, 0.2)
                .len(),
            1
        );
        assert!(index
            .hit_test(Point::new(1900.000001, 1700.0), 0.0)
            .is_empty());
        // Inside the broad-phase box but outside the actual rotated footprint.
        let miss = Aabb {
            min_x: 0.0,
            min_y: 3500.0,
            max_x: 50.0,
            max_y: 3550.0,
        };
        assert!(index.marquee(miss).is_empty());
        let mut moved = d.clone();
        moved.level_connections[0].from.position_mm[0] += 10000.0;
        moved.level_connections[0].to.position_mm[0] += 10000.0;
        assert!(index.sync(&moved, &[]) > 0);
        assert!(index.hit_test(point, 0.0).is_empty());
        assert_eq!(
            index.hit_test(Point::new(11500.0, 2000.0), 0.0),
            HitIndex::build(&moved, level).hit_test(Point::new(11500.0, 2000.0), 0.0)
        );
        moved.staircases[0].risers = 1;
        index.sync(&moved, &[]);
        assert!(index.hit_test(Point::new(11500.0, 2000.0), 0.0).is_empty());
        index.sync(&d, &[]);
        assert_eq!(index.marquee(rect), vec![id]);
        let mut deleted = d.clone();
        deleted.staircases.clear();
        index.sync(&deleted, &[]);
        assert!(index.marquee(rect).is_empty());
    }
    assert!(HitIndex::build(&d, Uuid::now_v7())
        .hit_test(point, 100.0)
        .is_empty());
    let mut second_connection = d.level_connections[0].clone();
    second_connection.id = Uuid::now_v7();
    let mut second_stair = d.staircases[0].clone();
    second_stair.id = Uuid::now_v7();
    second_stair.connection_id = second_connection.id;
    let mut stair_ids = vec![id, second_stair.id];
    stair_ids.sort();
    d.level_connections.push(second_connection);
    d.staircases.push(second_stair);
    d.walls.push(Wall::new(
        low,
        Point::new(1000.0, 2000.0),
        Point::new(2000.0, 2000.0),
        100.0,
        2500.0,
    ));
    let hits = HitIndex::build(&d, low).hit_test(point, 0.0);
    assert_eq!(hits[0].class, HitClass::Region);
    assert_eq!(
        hits[1..].iter().map(|h| h.entity).collect::<Vec<_>>(),
        stair_ids
    );
    d.staircases.reverse();
    assert_eq!(HitIndex::build(&d, low).hit_test(point, 0.0), hits);
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
