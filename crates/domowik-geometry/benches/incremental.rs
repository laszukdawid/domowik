//! Edit latency of the incremental engine against a full rebuild (issue 0013).
//!
//! | Benchmark | Measures |
//! |---|---|
//! | `full/tiled-N` | `walls::plan` from scratch: what every edit cost before |
//! | `cold/tiled-N` | `LevelGeometry::from_solids`: the one-time build |
//! | `edit-sync/tiled-N` | one wall nudged, via `sync` (includes the O(N) diff by id) |
//! | `edit-apply/tiled-N` | the same edit as an explicit `SourceChange` |
//! | `edit-read/tiled-N` | `edit-sync` then reading every room polygon |
//! | `edit-sync/isolated-N` | one wall nudged among N separate walls |
//! | `cold-sourced/tiled-N`, `edit-sync-sourced/tiled-N` | the same in provenance mode (issue 0012) |
//!
//! | `rooms-scan/tiled-N-rR`, `rooms-retained/tiled-N-rR` | room projection of `R` records: `project_against` (every seed against every polygon) vs `project_retained` (retained ring buckets) |
//! | `rooms-scan/nested-N-r1`, `rooms-retained/nested-N-r1` | the same, one seed at the centre of `N` concentric loops |
//! | `cuts/dense-host/M` | one host's cut contours under `M` touching windows (R4.2.2) |
//!
//! Each edit alternates between two positions of one interior wall, so the
//! state returns to where it started every second iteration. Timings are
//! host-dependent and not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use domowik_format::{Document, Level, Point, RoomRecord, Wall};
use domowik_geometry::incremental::{LevelGeometry, SourceChange, SourceKey};
use domowik_geometry::rooms;
use domowik_geometry::walls::{level_solids, plan, LevelSolids, WallSolid};
use uuid::Uuid;

fn level(walls: Vec<WallSolid>) -> LevelSolids {
    let ids = (0..walls.len()).map(|_| Uuid::now_v7()).collect();
    LevelSolids {
        walls,
        ids,
        joins: Vec::new(),
    }
}

fn tiled(k: usize) -> LevelSolids {
    let s = 3000.0;
    let mut walls = Vec::new();
    for i in 0..=k {
        for j in 0..k {
            let (a, b, c) = (i as f64 * s, j as f64 * s, (j + 1) as f64 * s);
            walls.push(WallSolid::new(Point::new(b, a), Point::new(c, a), 200.0));
            walls.push(WallSolid::new(Point::new(a, b), Point::new(a, c), 200.0));
        }
    }
    level(walls)
}

fn isolated(n: usize) -> LevelSolids {
    let side = (n as f64).sqrt().ceil() as usize;
    level(
        (0..n)
            .map(|i| {
                let (x, y) = ((i % side) as f64 * 3000.0, (i / side) as f64 * 3000.0);
                WallSolid::new(Point::new(x, y), Point::new(x + 1000.0, y), 200.0)
            })
            .collect(),
    )
}

/// The middle wall shifted by `dx`.
fn nudged(solids: &LevelSolids, dx: f64) -> (LevelSolids, usize) {
    let mut s = solids.clone();
    let i = s.walls.len() / 2;
    let w = s.walls[i];
    s.walls[i] = WallSolid::new(
        Point::new(w.start.x + dx, w.start.y),
        Point::new(w.end.x + dx, w.end.y),
        w.thickness_mm,
    );
    (s, i)
}

fn bench(c: &mut Criterion) {
    let families: Vec<(String, LevelSolids)> = [7, 22, 70]
        .into_iter()
        .map(|k| (format!("tiled-{}", 2 * k * (k + 1)), tiled(k)))
        .chain([1000, 10_000].map(|n| (format!("isolated-{n}"), isolated(n))))
        .collect();

    let mut group = c.benchmark_group("incremental");
    group.sample_size(10);
    for (name, solids) in &families {
        let (moved, i) = nudged(solids, 50.0);
        let key = SourceKey::Wall(solids.ids[i]);
        let ring = |s: &LevelSolids| s.walls[i].rectangle().unwrap().to_vec();

        if name.starts_with("tiled") {
            group.bench_with_input(BenchmarkId::new("full", name), solids, |b, s| {
                b.iter(|| plan(black_box(&s.walls), &s.joins))
            });
            group.bench_with_input(BenchmarkId::new("cold", name), solids, |b, s| {
                b.iter(|| LevelGeometry::from_solids(black_box(s)))
            });
        }

        let mut g = LevelGeometry::from_solids(solids);
        let mut flip = false;
        group.bench_function(BenchmarkId::new("edit-sync", name), |b| {
            b.iter(|| {
                flip = !flip;
                g.sync(black_box(if flip { &moved } else { solids }));
            })
        });

        if name.starts_with("tiled") {
            group.bench_with_input(BenchmarkId::new("cold-sourced", name), solids, |b, s| {
                b.iter(|| {
                    let mut g = LevelGeometry::with_provenance();
                    g.sync(black_box(s));
                    g
                })
            });
            let mut g = LevelGeometry::with_provenance();
            g.sync(solids);
            let mut flip = false;
            group.bench_function(BenchmarkId::new("edit-sync-sourced", name), |b| {
                b.iter(|| {
                    flip = !flip;
                    g.sync(black_box(if flip { &moved } else { solids }));
                })
            });
        }

        if name.starts_with("tiled") {
            let mut g = LevelGeometry::from_solids(solids);
            let mut flip = false;
            group.bench_function(BenchmarkId::new("edit-read", name), |b| {
                b.iter(|| {
                    flip = !flip;
                    g.sync(black_box(if flip { &moved } else { solids }));
                    black_box(g.rooms_iter().map(|p| p.outer.len()).sum::<usize>())
                })
            });
        }

        if name.starts_with("tiled") {
            let mut g = LevelGeometry::from_solids(solids);
            let (a, z) = (ring(solids), ring(&moved));
            let mut flip = false;
            group.bench_function(BenchmarkId::new("edit-apply", name), |b| {
                b.iter(|| {
                    flip = !flip;
                    let r = if flip { z.clone() } else { a.clone() };
                    g.apply(vec![SourceChange::Upsert(key.clone(), r)]);
                })
            });
        }
    }
    group.finish();
}

/// A `k × k` grid of 3 m rooms on one level with `records` room records
/// seeded at room centres, spread over the grid. The walls form ONE occupied
/// polygon with `k²` holes — the case a per-polygon scan handles worst.
fn tiled_document(k: usize, records: usize) -> (Document, uuid::Uuid) {
    let mut doc = Document::new();
    doc.levels.push(Level::new("L1"));
    let level = doc.levels[0].id;
    let s = 3000.0;
    for i in 0..=k {
        for j in 0..k {
            let (a, b, c) = (i as f64 * s, j as f64 * s, (j + 1) as f64 * s);
            for (p, q) in [((b, a), (c, a)), ((a, b), (a, c))] {
                doc.walls.push(Wall::new(
                    level,
                    Point::new(p.0, p.1),
                    Point::new(q.0, q.1),
                    200.0,
                    2500.0,
                ));
            }
        }
    }
    let rooms = k * k;
    for r in 0..records.min(rooms) {
        let cell = r * rooms / records.min(rooms);
        let (i, j) = (cell / k, cell % k);
        doc.room_records.push(RoomRecord::new(
            uuid::Uuid::from_u128(r as u128 + 1),
            level,
            Point::new((j as f64 + 0.5) * s, (i as f64 + 0.5) * s),
        ));
    }
    (doc, level)
}

fn bench_rooms(c: &mut Criterion) {
    let mut group = c.benchmark_group("incremental");
    group.sample_size(10);
    for k in [7, 22, 70] {
        for records in [10, k * k] {
            let (doc, level) = tiled_document(k, records);
            let walls = 2 * k * (k + 1);
            let name = format!("tiled-{walls}-r{records}");
            let geometry = LevelGeometry::from_solids(&level_solids(&doc, level));
            let (occupied, faces) = geometry.plan();
            assert_eq!(
                rooms::project_retained(&doc, level, &geometry),
                rooms::project_against(&doc, level, occupied.clone(), faces.clone())
            );
            group.bench_function(BenchmarkId::new("rooms-scan", &name), |b| {
                b.iter(|| {
                    let (o, f) = geometry.plan();
                    rooms::project_against(black_box(&doc), level, o, f)
                })
            });
            group.bench_function(BenchmarkId::new("rooms-retained", &name), |b| {
                b.iter(|| rooms::project_retained(black_box(&doc), level, &geometry))
            });
        }
    }
    // `n` concentric unjoined loops with one record at their centre: every
    // ring's bounds hold the seed, so nesting depth, not plan size, is the
    // cost (Codex GPT-6.1-Sol's review case).
    for n in [800, 3200] {
        let mut doc = Document::new();
        doc.levels.push(Level::new("L1"));
        let level = doc.levels[0].id;
        for i in 1..=n {
            let r = 2.0 * i as f64;
            let c = [(-r, -r), (r, -r), (r, r), (-r, r)];
            for k in 0..4 {
                let (p, q) = (c[k], c[(k + 1) % 4]);
                doc.walls.push(Wall::new(
                    level,
                    Point::new(p.0, p.1),
                    Point::new(q.0, q.1),
                    0.5,
                    2500.0,
                ));
            }
        }
        doc.room_records.push(RoomRecord::new(
            uuid::Uuid::from_u128(1),
            level,
            Point::new(0.0, 0.0),
        ));
        let geometry = LevelGeometry::from_solids(&level_solids(&doc, level));
        let (occupied, faces) = geometry.plan();
        assert_eq!(
            rooms::project_retained(&doc, level, &geometry),
            rooms::project_against(&doc, level, occupied, faces)
        );
        let name = format!("nested-{n}-r1");
        group.bench_function(BenchmarkId::new("rooms-scan", &name), |b| {
            b.iter(|| {
                let (o, f) = geometry.plan();
                rooms::project_against(black_box(&doc), level, o, f)
            })
        });
        group.bench_function(BenchmarkId::new("rooms-retained", &name), |b| {
            b.iter(|| rooms::project_retained(black_box(&doc), level, &geometry))
        });
    }
    group.finish();
}

/// One host cut by `m` touching windows of alternating depth over
/// `[1/8, 7/8]` of its axis (Codex GPT-6.1-Sol's dense-host family): the
/// per-host contour must scale ~linearly in `m`.
fn bench_cuts(c: &mut Criterion) {
    use domowik_format::Side;
    use domowik_geometry::cuts::{wall_pieces, Aperture};
    let mut group = c.benchmark_group("cuts");
    let host = WallSolid::new(Point::new(0.0, 0.0), Point::new(1_048_576.0, 0.0), 200.0);
    for m in [1024usize, 4096, 16384] {
        let cuts: Vec<Aperture> = (0..m)
            .map(|i| Aperture {
                wall: 0,
                s: (
                    0.125 + 0.75 * i as f64 / m as f64,
                    0.125 + 0.75 * (i + 1) as f64 / m as f64,
                ),
                side: Side::Left,
                depth: if i % 2 == 0 { 40.0 } else { 80.0 },
            })
            .collect();
        group.bench_function(BenchmarkId::new("dense-host", m), |b| {
            b.iter(|| wall_pieces(black_box(&host), &cuts))
        });
    }
    group.finish();
}

criterion_group!(benches, bench, bench_rooms, bench_cuts);
criterion_main!(benches);
