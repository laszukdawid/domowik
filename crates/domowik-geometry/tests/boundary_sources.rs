use domowik_format::Point;
use domowik_geometry::boolean::{union, union_with_sources, SourceRing};
use domowik_geometry::polygon::Polygon;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64, source: usize) -> SourceRing {
    SourceRing {
        points: vec![
            Point::new(x0, y0),
            Point::new(x1, y0),
            Point::new(x1, y1),
            Point::new(x0, y1),
        ],
        source,
    }
}

fn polygons(out: &[domowik_geometry::boolean::SourcedPolygon]) -> Vec<Polygon> {
    out.iter().map(|p| p.polygon.clone()).collect()
}

#[test]
fn sourced_geometry_matches_the_existing_union() {
    let rings = vec![
        rect(0.0, 0.0, 10.0, 10.0, 41),
        rect(5.0, 0.0, 15.0, 10.0, 99),
    ];
    let sourced = union_with_sources(&rings);
    let plain = union(&rings.iter().map(|r| r.points.clone()).collect::<Vec<_>>());
    assert_eq!(polygons(&sourced), plain);
}

#[test]
fn source_tags_are_stable_under_input_reordering_and_ring_reversal() {
    let a = rect(0.0, 0.0, 10.0, 10.0, 7001);
    let mut b = rect(5.0, 0.0, 15.0, 10.0, 9009);
    b.points.reverse();
    let forward = union_with_sources(&[a.clone(), b.clone()]);
    let reverse = union_with_sources(&[b, a]);
    assert_eq!(forward, reverse);
    let disconnected = vec![
        rect(100.0, 0.0, 104.0, 4.0, 19),
        rect(-20.0, 0.0, -10.0, 10.0, 73),
    ];
    let normal = union_with_sources(&disconnected);
    assert_eq!(normal.len(), 2);
    assert_eq!(normal[0].outer_sources(), vec![vec![73]; 4]);
    assert_eq!(normal[1].outer_sources(), vec![vec![19]; 4]);
    assert_eq!(
        polygons(&normal),
        union(
            &disconnected
                .iter()
                .map(|r| r.points.clone())
                .collect::<Vec<_>>()
        )
    );
    let mut reordered = disconnected.clone();
    reordered.reverse();
    for ring in &mut reordered {
        ring.points.reverse();
    }
    assert_eq!(normal, union_with_sources(&reordered));
}

#[test]
fn degenerate_source_does_not_renumber_valid_sources() {
    let mut degenerate = rect(0.0, 0.0, 1.0, 1.0, 1234);
    degenerate.points = vec![Point::new(2.0, 2.0), Point::new(3.0, 3.0)];
    let out = union_with_sources(&[degenerate, rect(0.0, 0.0, 10.0, 10.0, 88)]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].outer_sources(), vec![vec![88]; 4]);
}

#[test]
fn adjacent_edges_merge_and_swallowed_sources_disappear() {
    let out = union_with_sources(&[
        rect(0.0, 0.0, 10.0, 10.0, 1),
        rect(10.0, 0.0, 20.0, 10.0, 2),
        rect(2.0, 2.0, 8.0, 8.0, 3),
    ]);
    let plain = union(&[
        vec![
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(0.0, 10.0),
        ],
        vec![
            Point::new(10.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(10.0, 10.0),
        ],
        vec![
            Point::new(2.0, 2.0),
            Point::new(8.0, 2.0),
            Point::new(8.0, 8.0),
            Point::new(2.0, 8.0),
        ],
    ]);
    assert_eq!(polygons(&out), plain);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].polygon.outer,
        vec![
            Point::new(0.0, 0.0),
            Point::new(20.0, 0.0),
            Point::new(20.0, 10.0),
            Point::new(0.0, 10.0)
        ]
    );
    assert_eq!(
        out[0].outer_sources(),
        vec![vec![1, 2], vec![2], vec![1, 2], vec![1]]
    );
}

#[test]
fn identical_rings_retain_both_sources_and_frame_hole_edges_are_tagged() {
    let identical = union_with_sources(&[
        rect(0.0, 0.0, 10.0, 10.0, 11),
        rect(0.0, 0.0, 10.0, 10.0, 22),
    ]);
    assert_eq!(identical[0].outer_sources(), vec![vec![11, 22]; 4]);

    let frame = union_with_sources(&[
        rect(0.0, 0.0, 10.0, 2.0, 101),
        rect(8.0, 0.0, 10.0, 10.0, 102),
        rect(0.0, 8.0, 10.0, 10.0, 103),
        rect(0.0, 0.0, 2.0, 10.0, 104),
    ]);
    assert_eq!(frame.len(), 1);
    assert_eq!(frame[0].polygon.holes.len(), 1);
    assert_eq!(frame[0].hole_sources().len(), 1);
    assert_eq!(
        frame[0].polygon.holes[0],
        vec![
            Point::new(2.0, 2.0),
            Point::new(2.0, 8.0),
            Point::new(8.0, 8.0),
            Point::new(8.0, 2.0)
        ]
    );
    assert_eq!(
        frame[0].hole_sources()[0],
        vec![vec![104], vec![103], vec![102], vec![101]]
    );
}

#[test]
fn deterministic_mixed_ring_corpus_preserves_geometry_and_source_order() {
    // Small independently generated integer-coordinate rectangles/triangles.
    // Compare geometry with the established union, then permute the inputs
    // while retaining their caller identities. No random dependency or seed
    // external to this test is needed.
    let mut seed = 0x004d_4f57_494bu64;
    let mut next = || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((seed >> 32) % 24) as f64
    };
    for case in 0usize..64 {
        let rings: Vec<_> = (0..6)
            .map(|i| {
                let (x, y, w, h) = (next(), next(), next() + 1.0, next() + 1.0);
                let mut ring = rect(x, y, x + w, y + h, 100 + i * 17);
                if (case + i).is_multiple_of(3) {
                    ring.points.remove(2);
                }
                ring
            })
            .collect();
        let out = union_with_sources(&rings);
        let plain = union(&rings.iter().map(|r| r.points.clone()).collect::<Vec<_>>());
        assert_eq!(polygons(&out), plain, "geometry case {case}");
        assert!(!out.is_empty());
        for polygon in &out {
            let outer_sources = polygon.outer_sources();
            let hole_sources = polygon.hole_sources();
            assert_eq!(outer_sources.len(), polygon.polygon.outer.len());
            assert_eq!(hole_sources.len(), polygon.polygon.holes.len());
            for (ring, sources) in std::iter::once((&polygon.polygon.outer, &outer_sources))
                .chain(polygon.polygon.holes.iter().zip(&hole_sources))
            {
                assert_eq!(ring.len(), sources.len());
                for edge in sources {
                    assert!(!edge.is_empty());
                    assert!(edge.windows(2).all(|pair| pair[0] < pair[1]));
                    assert!(edge
                        .iter()
                        .all(|source| rings.iter().any(|r| r.source == *source)));
                }
            }
        }
        let mut permuted = rings;
        permuted.reverse();
        for ring in &mut permuted {
            ring.points.reverse();
        }
        assert_eq!(
            out,
            union_with_sources(&permuted),
            "source order case {case}"
        );
    }
}

// ---------------------------------------------------------------------------
// Wall-level provenance (issue 0058)
// ---------------------------------------------------------------------------

use domowik_geometry::walls::{
    occupied_region, occupied_region_with_sources, BoundarySolid, Endpoint, JoinMember, JoinSpec,
    WallSolid,
};

fn wall(x0: f64, y0: f64, x1: f64, y1: f64) -> WallSolid {
    WallSolid::new(Point::new(x0, y0), Point::new(x1, y1), 200.0)
}

/// The edge of `region`'s only component whose canonical start/end match, plus
/// its run. Panics rather than returning an option: a fixture that cannot
/// locate its own edge is a broken fixture, not a failed assertion.
fn edge_run(
    region: &domowik_geometry::walls::SourcedRegion,
    start: Point,
    end: Point,
) -> (Point, Point, &domowik_geometry::boolean::BoundaryRun) {
    let polygon = &region.components[0];
    let ring = &polygon.polygon.outer;
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if (a == start && b == end) || (a == end && b == start) {
            return (a, b, &polygon.outer_runs[i]);
        }
    }
    panic!("no canonical edge between {start:?} and {end:?} in {ring:?}");
}

#[test]
fn sourced_occupied_region_reproduces_the_untagged_region_exactly() {
    // A corner join, so the patch is a real polygon rather than a dropped
    // degenerate segment, and the components carry a hole-free corner.
    let walls = vec![
        wall(0.0, 0.0, 2000.0, 0.0),
        wall(2000.0, 0.0, 2000.0, 1500.0),
    ];
    let joins = vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
        ],
    }];
    let sourced = occupied_region_with_sources(&walls, &joins);
    // The R6.3.1 canonical key is (component, ring, vertex-run index), so
    // asking for provenance must not renumber anything.
    assert_eq!(
        sourced
            .components
            .iter()
            .map(|c| c.polygon.clone())
            .collect::<Vec<_>>(),
        occupied_region(&walls, &joins)
    );
    for component in &sourced.components {
        assert_eq!(component.outer_runs.len(), component.polygon.outer.len());
        assert_eq!(component.hole_runs.len(), component.polygon.holes.len());
    }
}

#[test]
fn every_boundary_tag_decodes_to_a_wall_or_the_join_that_produced_it() {
    let walls = vec![
        wall(0.0, 0.0, 2000.0, 0.0),
        wall(2000.0, 0.0, 2000.0, 1500.0),
    ];
    let joins = vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
        ],
    }];
    let sourced = occupied_region_with_sources(&walls, &joins);
    let mut seen = std::collections::HashSet::new();
    for component in &sourced.components {
        for run in &component.outer_runs {
            assert_eq!(run.sources.len(), run.breaks.len() + 1);
            for piece in &run.sources {
                assert!(!piece.is_empty());
                for tag in piece {
                    seen.insert(sourced.solid(*tag));
                }
            }
        }
    }
    // Both walls and the join patch reach the boundary of an L; nothing else
    // exists to be named.
    assert!(seen.contains(&BoundarySolid::Wall(0)));
    assert!(seen.contains(&BoundarySolid::Wall(1)));
    assert!(seen.contains(&BoundarySolid::Join(0)));
    assert!(seen
        .iter()
        .all(|s| matches!(s, BoundarySolid::Wall(0 | 1) | BoundarySolid::Join(0))));
}

#[test]
fn a_collinear_run_keeps_each_walls_own_half_of_the_merged_face() {
    // Two collinear walls of equal thickness merge into ONE rectangle, so the
    // long faces are single canonical edges spanning both walls. This is the
    // case where a merged tag union cannot tell a wall's own face from its
    // neighbour's — the per-piece record is what makes it answerable.
    let walls = vec![wall(0.0, 0.0, 1000.0, 0.0), wall(1000.0, 0.0, 2000.0, 0.0)];
    let joins = vec![JoinSpec {
        members: vec![
            JoinMember {
                wall: 0,
                endpoint: Endpoint::End,
            },
            JoinMember {
                wall: 1,
                endpoint: Endpoint::Start,
            },
        ],
    }];
    let sourced = occupied_region_with_sources(&walls, &joins);
    assert_eq!(sourced.components.len(), 1);
    assert_eq!(
        sourced.components[0].polygon.outer,
        vec![
            Point::new(0.0, -100.0),
            Point::new(2000.0, -100.0),
            Point::new(2000.0, 100.0),
            Point::new(0.0, 100.0),
        ]
    );

    for y in [-100.0, 100.0] {
        let (start, end, run) = edge_run(&sourced, Point::new(0.0, y), Point::new(2000.0, y));
        // The union answer alone names both walls and can distinguish neither.
        assert_eq!(run.merged_sources().len(), 2);
        assert_eq!(run.breaks, vec![Point::new(1000.0, y)]);
        let solids = |p: Point| {
            run.piece_at(start, end, p)
                .unwrap()
                .iter()
                .map(|t| sourced.solid(*t))
                .collect::<Vec<_>>()
        };
        assert_eq!(solids(Point::new(250.0, y)), vec![BoundarySolid::Wall(0)]);
        assert_eq!(solids(Point::new(1750.0, y)), vec![BoundarySolid::Wall(1)]);
    }
}
