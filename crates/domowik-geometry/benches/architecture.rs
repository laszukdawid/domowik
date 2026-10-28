use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_format::*;
use domowik_geometry::architecture;
use uuid::Uuid;

fn workloads(c: &mut Criterion) {
    let mut d = Document::with_ground_floor();
    let lower = d.levels[0].id;
    let mut upper = Level::new("Synthetic upper");
    upper.elevation_mm = 2600.0;
    let high = upper.id;
    d.levels.push(upper);
    let connection = LevelConnection {
        id: Uuid::now_v7(),
        name: "Synthetic connection".into(),
        from: ConnectionEndpoint {
            level_id: lower,
            position_mm: [0.0, 0.0],
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: high,
            position_mm: [3780.0, 0.0],
            offset_mm: 0.0,
        },
        unknown: Default::default(),
    };
    let s = Staircase {
        id: Uuid::now_v7(),
        name: "Synthetic stair".into(),
        connection_id: connection.id,
        width_mm: 924.0,
        risers: 15,
        guard_height_mm: 1000.0,
        opening_ids: vec![],
        unknown: Default::default(),
    };
    d.level_connections.push(connection);
    d.staircases.push(s);
    let index = domowik_geometry::hit::HitIndex::build(&d, lower);
    let footprint = architecture::stair_footprint(&d, &d.staircases[0]).unwrap();
    c.bench_function("architecture/stair_edge_resize", |b| {
        b.iter(|| {
            architecture::resize_stair_footprint(
                black_box(&footprint),
                architecture::StairResizeEdge::Left,
                black_box(Point::new(100.0, -100.0)),
            )
            .unwrap()
        })
    });
    c.bench_function("architecture/stair_end_rotate_and_snap", |b| {
        b.iter(|| {
            architecture::resize_stair_footprint_with_snap(
                black_box(&footprint),
                architecture::StairResizeEdge::Upper,
                black_box(Point::new(500.0, 300.0)),
                true,
            )
            .unwrap()
        })
    });
    c.bench_function("architecture/stair_hit_cached", |b| {
        b.iter(|| index.hit_test(black_box(Point::new(1800.0, 400.0)), black_box(60.0)))
    });
    c.bench_function("architecture/stair_marquee_cached", |b| {
        b.iter(|| {
            index.marquee(black_box(domowik_geometry::spatial::Aabb {
                min_x: 1000.0,
                min_y: -500.0,
                max_x: 2000.0,
                max_y: 500.0,
            }))
        })
    });
    let mut synced = index.clone();
    c.bench_function("architecture/stair_hit_sync_unchanged", |b| {
        b.iter(|| synced.sync(black_box(&d), &[]))
    });
    c.bench_function("architecture/straight_stair_15_risers", |b| {
        b.iter(|| architecture::stair(black_box(&d), black_box(&d.staircases[0])).unwrap())
    });
    c.bench_function("architecture/plan_two_endpoint_levels", |b| {
        b.iter(|| {
            (
                architecture::plan(black_box(&d), lower).unwrap(),
                architecture::plan(black_box(&d), high).unwrap(),
            )
        })
    });
    let f = RectFootprint {
        start_mm: [0.0, 0.0],
        end_mm: [10_000.0, 0.0],
        width_mm: 10_000.0,
    };
    let surfaces = vec![architecture::rectangle(&f)];
    d.hosted_openings.push(HostedOpening {
        id: Uuid::now_v7(),
        name: "Synthetic opening".into(),
        host: OpeningHost::LevelFloor(high),
        footprint: RectFootprint {
            start_mm: [1000.0, 0.0],
            end_mm: [4780.0, 0.0],
            width_mm: 924.0,
        },
        unknown: Default::default(),
    });
    c.bench_function("architecture/floor_with_stair_opening", |b| {
        b.iter(|| {
            architecture::cut_surfaces(
                black_box(&d),
                OpeningHost::LevelFloor(high),
                black_box(&surfaces),
            )
        })
    });
}
criterion_group!(benches, workloads);
criterion_main!(benches);
