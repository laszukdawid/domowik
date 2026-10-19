//! The engine against its oracle: `walls::plan` of the same inputs, after
//! every edit, plus the full invariant check.

use super::*;
use crate::walls::{occupied_region_with_sources, plan, Endpoint, JoinMember, JoinSpec, WallSolid};
use std::collections::BTreeMap;
use uuid::Uuid;

const Q: f64 = 1.0 / 1024.0;

/// The ring-bucket side in millimetres.
const COARSE_MM: f64 = super::index::COARSE / crate::overlay::CELLS_PER_MM;

/// A level under edit: solids plus stable ids, mutated in place.
#[derive(Clone, Default)]
struct Level {
    solids: LevelSolids,
}

impl Level {
    fn add(&mut self, a: (f64, f64), b: (f64, f64), t: f64) -> Uuid {
        let id = Uuid::now_v7();
        self.solids.walls.push(WallSolid::new(
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            t,
        ));
        self.solids.ids.push(id);
        id
    }

    fn position(&self, id: Uuid) -> usize {
        self.solids
            .ids
            .iter()
            .position(|&x| x == id)
            .expect("wall exists")
    }

    fn remove(&mut self, id: Uuid) {
        let i = self.position(id);
        self.solids.walls.remove(i);
        self.solids.ids.remove(i);
        // Positional joins: drop members of the removed wall, shift the rest.
        for join in &mut self.solids.joins {
            join.members.retain(|m| m.wall != i);
            for m in &mut join.members {
                if m.wall > i {
                    m.wall -= 1;
                }
            }
        }
        self.solids.joins.retain(|j| j.members.len() >= 2);
    }

    fn join(&mut self, a: Uuid, ea: Endpoint, b: Uuid, eb: Endpoint) {
        let (ia, ib) = (self.position(a), self.position(b));
        self.solids.joins.push(JoinSpec {
            members: vec![
                JoinMember {
                    wall: ia,
                    endpoint: ea,
                },
                JoinMember {
                    wall: ib,
                    endpoint: eb,
                },
            ],
        });
    }
}

/// Syncs, validates, and compares with a full rebuild.
fn check(g: &mut LevelGeometry, level: &Level, context: &str) {
    g.sync(&level.solids);
    g.validate();
    let expected = plan(&level.solids.walls, &level.solids.joins);
    let actual = g.plan();
    assert_eq!(actual.0, expected.0, "occupied differs after {context}");
    assert_eq!(actual.1, expected.1, "rooms differ after {context}");
    check_seeds(g, &expected.0, &expected.1, context);
}

/// `locate_seed` against the scan over every polygon, at seeds where the
/// answer changes: every ring vertex and edge midpoint, points a hair either
/// side of them, and a grid over (and between) the random generator's lines.
fn check_seeds(g: &LevelGeometry, occupied: &[Polygon], rooms: &[Polygon], context: &str) {
    const HAIR: f64 = 1.0 / (1u64 << 20) as f64;
    let mut seeds = Vec::new();
    for polygon in occupied.iter().chain(rooms) {
        for ring in std::iter::once(&polygon.outer).chain(&polygon.holes) {
            for (i, &p) in ring.iter().enumerate() {
                let q = ring[(i + 1) % ring.len()];
                for c in [p, Point::new((p.x + q.x) / 2.0, (p.y + q.y) / 2.0)] {
                    seeds.push(c);
                    for (dx, dy) in [(HAIR, 0.0), (-HAIR, 0.0), (0.0, HAIR), (0.0, -HAIR)] {
                        seeds.push(Point::new(c.x + dx, c.y + dy));
                    }
                }
            }
        }
    }
    for i in -1..=17 {
        for j in -1..=17 {
            seeds.push(Point::new(i as f64 * 250.0, j as f64 * 250.0));
            seeds.push(Point::new(i as f64 * 250.0 + 37.5, j as f64 * 250.0 + 12.5));
        }
    }
    for seed in seeds {
        assert_eq!(
            g.locate_seed(seed),
            crate::rooms::classify_seed(seed, occupied, rooms),
            "seed {seed:?} after {context}"
        );
    }
}

/// One boundary ring with its runs, tags named by source key.
type KeyedRing = (Vec<Point>, Vec<(Vec<Point>, Vec<Vec<SourceKey>>)>);

/// The engine's occupied boundary keyed by `(component, ring)` ordinal.
fn engine_sourced(g: &LevelGeometry) -> BTreeMap<(usize, usize), KeyedRing> {
    let mut out = BTreeMap::new();
    for key in g.occupied.keys() {
        let (Some(ord), Some(ring)) = (g.occupied_ordinals(key), g.occupied_ring(key)) else {
            continue;
        };
        let runs = ring
            .runs()
            .map(|run| {
                let pieces = run
                    .sources
                    .iter()
                    .map(|tags| {
                        let mut keys: Vec<SourceKey> = tags
                            .iter()
                            .map(|&t| g.source_key(t).expect("tag names a live source").clone())
                            .collect();
                        keys.sort();
                        keys
                    })
                    .collect();
                (run.breaks, pieces)
            })
            .collect();
        assert!(
            out.insert(ord, (ring.points.to_vec(), runs)).is_none(),
            "two rings share ordinals {ord:?}"
        );
    }
    out
}

/// `occupied_region_with_sources` of the same level in the same shape.
fn full_sourced(solids: &LevelSolids) -> BTreeMap<(usize, usize), KeyedRing> {
    let region = occupied_region_with_sources(&solids.walls, &solids.joins);
    let joins = sources::join_keys(solids);
    let key = |t: usize| {
        if t < solids.walls.len() {
            SourceKey::Wall(solids.ids[t])
        } else {
            joins[t - solids.walls.len()].0.clone()
        }
    };
    let keyed = |runs: &[crate::boolean::BoundaryRun]| {
        runs.iter()
            .map(|run| {
                let pieces = run
                    .sources
                    .iter()
                    .map(|tags| {
                        let mut keys: Vec<SourceKey> = tags.iter().map(|&t| key(t)).collect();
                        keys.sort();
                        keys
                    })
                    .collect();
                (run.breaks.clone(), pieces)
            })
            .collect::<Vec<_>>()
    };
    let mut out = BTreeMap::new();
    for (c, comp) in region.components.iter().enumerate() {
        out.insert(
            (c, 0),
            (comp.polygon.outer.clone(), keyed(&comp.outer_runs)),
        );
        for (i, hole) in comp.polygon.holes.iter().enumerate() {
            out.insert((c, 1 + i), (hole.clone(), keyed(&comp.hole_runs[i])));
        }
    }
    out
}

/// [`check`] plus the provenance oracle.
fn check_sourced(g: &mut LevelGeometry, level: &Level, context: &str) {
    check(g, level, context);
    assert_eq!(
        engine_sourced(g),
        full_sourced(&level.solids),
        "sourced boundary differs after {context}"
    );
}

#[test]
fn an_empty_level_is_empty() {
    let g = LevelGeometry::new();
    assert_eq!(g.plan(), (Vec::new(), Vec::new()));
    g.validate();
}

#[test]
fn building_a_room_wall_by_wall_and_taking_it_apart_matches_full_builds() {
    let mut level = Level::default();
    let mut g = LevelGeometry::new();
    let corners = [(0.0, 0.0), (4000.0, 0.0), (4000.0, 3000.0), (0.0, 3000.0)];
    let mut ids = Vec::new();
    for i in 0..4 {
        ids.push(level.add(corners[i], corners[(i + 1) % 4], 200.0));
        check(&mut g, &level, &format!("adding wall {i}"));
    }
    for i in 0..4 {
        level.join(ids[i], Endpoint::End, ids[(i + 1) % 4], Endpoint::Start);
        check(&mut g, &level, &format!("joining corner {i}"));
    }
    assert_eq!(g.rooms().len(), 1);
    for (i, id) in ids.into_iter().enumerate() {
        level.remove(id);
        check(&mut g, &level, &format!("removing wall {i}"));
    }
    assert_eq!(g.plan(), (Vec::new(), Vec::new()));
}

/// Codex review of 0013, counterexample 1: the room exists only once the
/// fourth, unlinked wall arrives, and needs the other three to be built.
#[test]
fn a_room_closed_by_corner_contact_appears_and_disappears() {
    let mut level = Level::default();
    let mut g = LevelGeometry::new();
    level.add((10.5, 0.0), (10.5, 10.0), 1.0);
    level.add((0.0, 10.5), (10.0, 10.5), 1.0);
    level.add((-0.5, 0.0), (-0.5, 10.0), 1.0);
    check(&mut g, &level, "three walls");
    assert!(g.rooms().is_empty());
    let bottom = level.add((0.0, -0.5), (10.0, -0.5), 1.0);
    check(&mut g, &level, "closing wall");
    assert_eq!(g.rooms().len(), 1);
    level.remove(bottom);
    check(&mut g, &level, "removing it again");
    assert!(g.rooms().is_empty());
}

/// Codex review of 0013, counterexample 2: a wall that rounds to a single
/// cell occupies nothing but its hot cell bends the island next to it.
#[test]
fn a_collapsed_wall_still_reroutes_its_neighbour() {
    let s = |v: f64| v * Q;
    let mut level = Level::default();
    let mut g = LevelGeometry::new();
    level.add((s(0.0), s(0.0)), (s(8.0), s(6.0)), s(2.5));
    level.add((s(-20.0), s(-21.0)), (s(20.0), s(-21.0)), s(2.0));
    level.add((s(21.0), s(-20.0)), (s(21.0), s(20.0)), s(2.0));
    level.add((s(20.0), s(21.0)), (s(-20.0), s(21.0)), s(2.0));
    level.add((s(-21.0), s(20.0)), (s(-21.0), s(-20.0)), s(2.0));
    check(&mut g, &level, "frame and island");
    let before = g.rooms();
    let b = level.add((s(1.9375), s(3.0625)), (s(2.0625), s(3.0625)), s(0.125));
    check(&mut g, &level, "collapsed wall");
    assert_ne!(g.rooms(), before, "the island's boundary must bend");
    assert!(g.last_edit().segments_rerouted > 0);
    level.remove(b);
    check(&mut g, &level, "removing it");
    assert_eq!(g.rooms(), before);
}

/// A wall laid over an island raises the winding of faces whose cycles do
/// not change at all: the kept-face winding shift.
#[test]
fn covering_and_uncovering_an_island_shifts_kept_windings() {
    let mut level = Level::default();
    let mut g = LevelGeometry::new();
    let island: Vec<Uuid> = [
        ((100.0, 100.0), (300.0, 100.0)),
        ((300.0, 100.0), (300.0, 300.0)),
        ((300.0, 300.0), (100.0, 300.0)),
        ((100.0, 300.0), (100.0, 100.0)),
    ]
    .into_iter()
    .map(|(a, b)| level.add(a, b, 20.0))
    .collect();
    check(&mut g, &level, "island");
    let cover = level.add((0.0, 200.0), (400.0, 200.0), 400.0);
    check(&mut g, &level, "covering wall");
    assert_eq!(g.rooms().len(), 0, "the island's room is now occupied");
    level.remove(cover);
    check(&mut g, &level, "uncovering");
    assert_eq!(g.rooms().len(), 1);
    level.remove(island[0]);
    check(&mut g, &level, "opening the island");
}

/// A deterministic xorshift, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() * n as f64) as usize % n.max(1)
    }

    /// A coordinate on a coarse grid (so walls touch, overlap and enclose),
    /// sometimes nudged off it by a fraction of a cell or a few cells.
    fn coord(&mut self) -> f64 {
        let base = self.below(9) as f64 * 500.0;
        match self.below(6) {
            0 => base + (self.next() - 0.5) * 4.0 * Q,
            1 => base + (self.next() - 0.5) * 300.0,
            _ => base,
        }
    }
}

#[test]
fn random_edit_sequences_match_full_rebuilds_after_every_edit() {
    random_edit_sequences(1..=12, LevelGeometry::new, check);
}

#[test]
fn random_edit_sequences_match_full_sourced_rebuilds_after_every_edit() {
    random_edit_sequences(101..=110, LevelGeometry::with_provenance, check_sourced);
}

/// Replacing a wall by a geometrically identical one under a new id changes
/// no weight, face or ring, only who owns the boundary.
#[test]
fn an_identical_replacement_retags_without_retracing() {
    let mut level = Level::default();
    let mut g = LevelGeometry::with_provenance();
    let a = level.add((0.0, 0.0), (4000.0, 0.0), 200.0);
    level.add((4000.0, 0.0), (4000.0, 3000.0), 200.0);
    check_sourced(&mut g, &level, "two walls");
    let i = level.position(a);
    level.solids.ids[i] = Uuid::now_v7();
    check_sourced(&mut g, &level, "same wall, new id");
    let stats = g.last_edit().clone();
    assert_eq!(stats.occupied_rings_retraced, 0, "{stats:?}");
    assert!(stats.occupied_rings_retagged > 0, "{stats:?}");
}

fn random_edit_sequences(
    seeds: std::ops::RangeInclusive<u64>,
    fresh: fn() -> LevelGeometry,
    check: fn(&mut LevelGeometry, &Level, &str),
) {
    for seed in seeds {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0xff51_afd7_ed55_8ccd));
        let mut level = Level::default();
        let mut g = fresh();
        let mut ids: Vec<Uuid> = Vec::new();
        for step in 0..60 {
            let op = rng.below(10);
            let context = format!("seed {seed} step {step} op {op}");
            if op < 5 || ids.len() < 3 {
                let a = (rng.coord(), rng.coord());
                let b = if rng.below(3) == 0 {
                    (rng.coord(), rng.coord())
                } else if rng.below(2) == 0 {
                    (a.0, rng.coord())
                } else {
                    (rng.coord(), a.1)
                };
                if a == b {
                    continue;
                }
                let t = [20.0, 100.0, 200.0, 2.0 * Q][rng.below(4)];
                ids.push(level.add(a, b, t));
            } else if op < 7 {
                let id = ids.swap_remove(rng.below(ids.len()));
                level.remove(id);
            } else if op < 9 {
                // Move one end of a wall: a replacement under the same id.
                let id = ids[rng.below(ids.len())];
                let i = level.position(id);
                let w = level.solids.walls[i];
                let end = Point::new(rng.coord(), rng.coord());
                if end == w.start {
                    continue;
                }
                level.solids.walls[i] = WallSolid::new(w.start, end, w.thickness_mm);
            } else {
                let (a, b) = (ids[rng.below(ids.len())], ids[rng.below(ids.len())]);
                if a != b {
                    level.join(a, Endpoint::End, b, Endpoint::Start);
                }
            }
            check(&mut g, &level, &context);
        }
    }
}

/// The point of the engine: one edit in a large connected plan touches a
/// bounded amount of state, independent of the plan's size.
#[test]
fn a_local_edit_in_a_large_plan_does_local_work() {
    let mut level = Level::default();
    let s = 3000.0;
    let k = 12;
    for i in 0..=k {
        for j in 0..k {
            let (a, b, c) = (i as f64 * s, j as f64 * s, (j + 1) as f64 * s);
            level.add((b, a), (c, a), 200.0);
            level.add((a, b), (a, c), 200.0);
        }
    }
    let mut g = LevelGeometry::from_solids(&level.solids);
    // Nudge one interior wall sideways by 50 mm.
    let i = level.solids.walls.len() / 2;
    let w = level.solids.walls[i];
    let shift = Point::new(w.start.x + 50.0, w.start.y);
    let shift_end = Point::new(w.end.x + 50.0, w.end.y);
    level.solids.walls[i] = WallSolid::new(shift, shift_end, w.thickness_mm);
    check(&mut g, &level, "nudging one wall");
    let stats = g.last_edit().clone();
    assert_eq!(stats.sources_changed, 1);
    assert!(stats.segments_rerouted <= 24, "{stats:?}");
    assert!(stats.faces_dropped <= 24, "{stats:?}");
    assert_eq!(stats.ray_casts, 0, "{stats:?}");
}

/// Four walls of thickness `t` whose occupied outline is `[x0, x1] × [y0, y1]`
/// grown by nothing: centre lines sit `t/2` inside it.
fn square_walls(level: &mut Level, x0: f64, y0: f64, x1: f64, y1: f64, t: f64) {
    let h = t / 2.0;
    let (a, b, c, d) = (
        (x0 + h, y0 + h),
        (x1 - h, y0 + h),
        (x1 - h, y1 - h),
        (x0 + h, y1 - h),
    );
    level.add(a, b, t);
    level.add(b, c, t);
    level.add(c, d, t);
    level.add(d, a, t);
}

/// Codex GPT-6-Astra's counterexamples for seed location: a shell whose
/// hole holds an occupied island, and seeds a hair either side of the
/// shell's inner boundary (seeds are never snapped to the topology grid).
#[test]
fn seeds_in_a_shell_hole_island_and_a_hair_off_its_boundary() {
    use crate::rooms::SeedPlacement::*;
    let mut level = Level::default();
    square_walls(&mut level, -1.0, -1.0, 11.0, 11.0, 1.0);
    square_walls(&mut level, 4.0, 4.0, 6.0, 6.0, 1.0);
    let mut g = LevelGeometry::new();
    check(&mut g, &level, "shell with an island");
    let hair = 1.0 / (1u64 << 20) as f64;
    for (seed, expected) in [
        ((5.0, 5.0), InWall),
        ((4.0, 5.0), OnBoundary),
        ((2.0, 5.0), InFace(0)),
        ((0.0, 5.0), OnBoundary),
        ((hair, 5.0), InFace(0)),
        ((-hair, 5.0), InWall),
        ((20.0, 5.0), InExterior),
    ] {
        assert_eq!(
            g.locate_seed(Point::new(seed.0, seed.1)),
            expected,
            "{seed:?}"
        );
    }
}

/// Rooms straddling the ≈16 m ring-bucket boundary on both signs: a seed
/// exactly on the bucket line, and a hair either side, is found.
#[test]
fn seeds_on_ring_bucket_boundaries_are_found() {
    let line = COARSE_MM;
    let mut level = Level::default();
    square_walls(&mut level, line - 500.0, -300.0, line + 500.0, 300.0, 100.0);
    square_walls(
        &mut level,
        -line - 500.0,
        -line - 300.0,
        -line + 500.0,
        -line + 300.0,
        100.0,
    );
    let mut g = LevelGeometry::new();
    check(&mut g, &level, "rooms across bucket lines");
    let hair = 1.0 / (1u64 << 20) as f64;
    for x in [line, line - hair, line + hair] {
        assert!(
            matches!(
                g.locate_seed(Point::new(x, 0.0)),
                crate::rooms::SeedPlacement::InFace(_)
            ),
            "x = {x}"
        );
    }
    for x in [-line, -line - hair, -line + hair] {
        assert!(
            matches!(
                g.locate_seed(Point::new(x, -line)),
                crate::rooms::SeedPlacement::InFace(_)
            ),
            "x = {x}"
        );
    }
}

/// An explicit `apply` changes sources behind `sync`'s document cache; the
/// next `sync` must restore equality with a full build anyway (Codex
/// GPT-6-Astra's reproducer: remove a wall by `apply`, then `sync` the
/// unchanged document — the wall stayed absent), in both modes, and must not
/// re-route sources that are already right.
#[test]
fn sync_after_an_explicit_apply_restores_the_document() {
    for fresh in [LevelGeometry::new, LevelGeometry::with_provenance] {
        let mut level = Level::default();
        let a = level.add((0.0, 0.0), (4000.0, 0.0), 200.0);
        let b = level.add((4000.0, 0.0), (4000.0, 3000.0), 200.0);
        level.add((4000.0, 3000.0), (0.0, 3000.0), 200.0);
        level.add((0.0, 3000.0), (0.0, 0.0), 200.0);
        level.join(a, Endpoint::End, b, Endpoint::Start);
        let mut g = fresh();
        check(&mut g, &level, "a joined room");

        g.apply(vec![SourceChange::Remove(SourceKey::Wall(a))]);
        check(&mut g, &level, "sync after removing a wall by apply");
        let stats = g.last_edit().clone();
        assert_eq!(stats.sources_changed, 1, "only the missing wall: {stats:?}");

        let i = level.position(b);
        let moved = level.solids.walls[i]
            .rectangle()
            .unwrap()
            .map(|p| Point::new(p.x + 50.0, p.y));
        g.apply(vec![
            SourceChange::Upsert(SourceKey::Wall(b), moved.to_vec()),
            SourceChange::Upsert(SourceKey::Wall(Uuid::now_v7()), moved.to_vec()),
        ]);
        check(
            &mut g,
            &level,
            "sync after replacing and inventing sources by apply",
        );

        g.apply(Vec::new());
        check(&mut g, &level, "sync after an empty apply");
        assert_eq!(g.last_edit().sources_changed, 0, "{:?}", g.last_edit());
        check(&mut g, &level, "an ordinary sync afterwards");
    }
}

/// The drawn-region engine (`occupied_only` + `sync_cut`) against the
/// one-shot `cuts::drawn_region` after every step of random edit sequences
/// that also add, move, resize and delete openings (both faces, partial and
/// full depth, touching and overlapping spans), with `validate()` each step.
#[test]
fn random_cut_sequences_match_the_one_shot_drawn_region() {
    use crate::cuts::{drawn_region, Aperture};
    use domowik_format::Side;
    for seed in 201..=212u64 {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15 ^ seed.wrapping_mul(0xff51_afd7_ed55_8ccd));
        let mut level = Level::default();
        let mut ids: Vec<Uuid> = Vec::new();
        // Openings keyed by host id: (u, width, side, depth).
        let mut openings: Vec<(Uuid, f64, f64, Side, f64)> = Vec::new();
        let mut g = LevelGeometry::occupied_only();
        for step in 0..60 {
            let op = rng.below(12);
            if op < 4 || ids.len() < 3 {
                let a = (rng.coord(), rng.coord());
                let b = if rng.below(2) == 0 {
                    (a.0, rng.coord())
                } else {
                    (rng.coord(), a.1)
                };
                if a == b {
                    continue;
                }
                ids.push(level.add(a, b, [100.0, 200.0, 300.0][rng.below(3)]));
            } else if op < 5 {
                let id = ids.swap_remove(rng.below(ids.len()));
                level.remove(id);
                openings.retain(|o| o.0 != id);
            } else if op < 6 {
                let id = ids[rng.below(ids.len())];
                let i = level.position(id);
                let w = level.solids.walls[i];
                let end = Point::new(rng.coord(), rng.coord());
                if end != w.start {
                    level.solids.walls[i] = WallSolid::new(w.start, end, w.thickness_mm);
                }
            } else if op < 9 {
                let host = ids[rng.below(ids.len())];
                let side = if rng.below(2) == 0 {
                    Side::Left
                } else {
                    Side::Right
                };
                let depth = [50.0, 100.0, 1000.0][rng.below(3)];
                let u = [0.25, 0.5, 0.75, 0.3][rng.below(4)];
                let width = [200.0, 400.0, 800.0][rng.below(3)];
                openings.push((host, u, width, side, depth));
            } else if op < 11 && !openings.is_empty() {
                let k = rng.below(openings.len());
                openings[k].1 = [0.2, 0.4, 0.6][rng.below(3)];
            } else if !openings.is_empty() {
                openings.swap_remove(rng.below(openings.len()));
            }
            let apertures: Vec<Aperture> = openings
                .iter()
                .filter_map(|&(host, u, width, side, depth)| {
                    let i = level.solids.ids.iter().position(|x| *x == host)?;
                    let w = &level.solids.walls[i];
                    let len = w.length_mm();
                    (len > width).then(|| Aperture {
                        wall: i,
                        s: (u - width / 2.0 / len, u + width / 2.0 / len),
                        side,
                        depth: depth.min(w.thickness_mm),
                    })
                })
                .collect();
            g.sync_cut(&level.solids, &apertures);
            g.validate();
            assert_eq!(
                g.occupied(),
                drawn_region(&level.solids.walls, &level.solids.joins, &apertures),
                "seed {seed} step {step}"
            );
            assert!(g.rooms().is_empty());
        }
    }
}
