//! Invalidation-locality checks (issue 0032; BEHAVIOR-SPEC R3.4.8, R5.1.9,
//! ED-0111).
//!
//! No assertion here depends on a clock. Issue 0032 requires benchmarks and
//! correctness tests to be separate artifacts, and a locality property is a
//! correctness property: it holds or it does not, whatever the machine is
//! doing.
//!
//! The editor's engine (`incremental::LevelGeometry`) is audited per edit and
//! its work is checked against the permission computed independently by
//! `domowik_bench::locality`. This replaced a canary that asserted the old
//! whole-level `walls::plan` path still exceeded the closure (issue 0013).

use domowik_bench::gestures::{self, Outcome};
use domowik_bench::locality::{audit, report, Report};
use domowik_bench::{corpus, session_over};
use domowik_format::Point;
use domowik_geometry::walls::{LevelSolids, WallSolid};
use uuid::Uuid;

/// Every gesture on every corpus, its declared outcome checked against what
/// committing it actually does — a gesture that silently changes outcome
/// would drop out of every table — and every accepted one audited in both
/// engine modes.
fn reports() -> Vec<(String, Report)> {
    let mut out = Vec::new();
    for c in corpus::all().into_iter().chain(corpus::shapes()) {
        let level = c.primary_level();
        for g in gestures::all(&c.doc, level) {
            let mut session = session_over(c.doc.clone());
            let accepted = session.commit(g.operations.clone()).is_ok();
            assert_eq!(
                accepted,
                g.outcome == Outcome::Accepted,
                "{}/{}: declared {:?} but the commit {}",
                c.name,
                g.class,
                g.outcome,
                if accepted { "succeeded" } else { "was refused" }
            );
            if !accepted {
                assert_eq!(
                    session.document(),
                    &c.doc,
                    "{}/{}: a refused batch changed the document",
                    c.name,
                    g.class
                );
                continue;
            }
            let r = report(g.class, &c.doc, session.document(), level, &g.touched);
            let pre = domowik_geometry::walls::level_solids(&c.doc, level);
            let post = domowik_geometry::walls::level_solids(session.document(), level);
            let plain = audit(g.class, &pre, &post, &g.touched, false);
            assert_eq!(
                (
                    &plain.unjustified_pre,
                    &plain.unjustified_post,
                    plain.matches_full_build
                ),
                (
                    &r.unjustified_pre,
                    &r.unjustified_post,
                    r.matches_full_build
                ),
                "{}/{}: the two engine modes disagree",
                c.name,
                g.class
            );
            out.push((c.name.to_owned(), r));
        }
    }
    assert!(!out.is_empty(), "the corpus produced no gestures at all");
    out
}

/// The positive assertion that replaced the canary: on every accepted
/// gesture the engine's output equals a full build, and every wall source it
/// regenerated and every room it rebuilt lies inside the permission.
#[test]
fn every_gesture_is_confined_and_exact() {
    for (corpus, r) in reports() {
        assert!(
            r.matches_full_build,
            "{corpus}/{}: engine output differs from walls::plan",
            r.class
        );
        assert!(
            r.confined(),
            "{corpus}/{}: unjustified recomputation — walls {:?}, rooms before \
             {:?}, rooms after {:?}",
            r.class,
            r.unjustified_walls,
            r.unjustified_pre,
            r.unjustified_post
        );
    }
}

#[test]
fn the_permission_never_exceeds_the_level() {
    for (corpus, r) in reports() {
        assert!(r.closure_walls <= r.walls, "{corpus}/{}", r.class);
        assert!(r.component_pre <= r.rooms_pre, "{corpus}/{}", r.class);
        assert!(r.component_post <= r.rooms_post, "{corpus}/{}", r.class);
        assert!(r.actual_pre <= r.rooms_pre, "{corpus}/{}", r.class);
        assert!(r.actual_post <= r.rooms_post, "{corpus}/{}", r.class);
    }
}

/// The corpus must contain edits where locality is a real constraint. If
/// every row were `whole level`, the checks would pass without ever having
/// tested anything, which is the failure mode this test exists to prevent.
#[test]
fn the_corpus_contains_edits_whose_closure_is_a_strict_subset() {
    let all = reports();
    let strict: Vec<&(String, Report)> = all
        .iter()
        .filter(|(_, r)| !r.closure_is_whole_level())
        .collect();
    assert!(
        strict.len() >= 10,
        "only {} of {} gestures have a non-trivial closure; the corpus is not \
         exercising locality",
        strict.len(),
        all.len()
    );
    let corpora: std::collections::BTreeSet<&str> =
        strict.iter().map(|(c, _)| c.as_str()).collect();
    assert!(
        corpora.len() >= 3,
        "non-trivial closures come from only {corpora:?}; one shape is not a corpus"
    );
    // And on those, the engine really did less than the whole level.
    assert!(
        strict
            .iter()
            .any(|(_, r)| r.actual_post < r.rooms_post && r.rooms_post > 1),
        "no strict-subset edit left a room untouched"
    );
}

// ---------------------------------------------------------------------------
// Named shapes, as level solids.

const Q: f64 = 1.0 / 1024.0;

#[derive(Default)]
struct Level(LevelSolids);

impl Level {
    fn add(&mut self, a: (f64, f64), b: (f64, f64), t: f64) -> Uuid {
        let id = Uuid::now_v7();
        self.0.walls.push(WallSolid::new(
            Point::new(a.0, a.1),
            Point::new(b.0, b.1),
            t,
        ));
        self.0.ids.push(id);
        id
    }

    fn remove(&mut self, id: Uuid) {
        let i = self.0.ids.iter().position(|x| *x == id).unwrap();
        self.0.walls.remove(i);
        self.0.ids.remove(i);
    }

    /// Four walls whose occupied outline is `[x0, x1] × [y0, y1]`.
    fn square(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, t: f64) -> Vec<Uuid> {
        let h = t / 2.0;
        let c = [
            (x0 + h, y0 + h),
            (x1 - h, y0 + h),
            (x1 - h, y1 - h),
            (x0 + h, y1 - h),
        ];
        (0..4).map(|i| self.add(c[i], c[(i + 1) % 4], t)).collect()
    }
}

/// Audits `edit` applied to `level` in both modes, both directions (the edit
/// and its inverse), asserting exactness and confinement; returns the
/// forward provenance-mode report.
fn edit_both_ways(level: &Level, edit: impl Fn(&mut Level) -> Vec<Uuid>) -> Report {
    let mut after = Level(level.0.clone());
    let touched = edit(&mut after);
    let mut forward = None;
    for provenance in [true, false] {
        for (from, to, name) in [
            (&level.0, &after.0, "forward"),
            (&after.0, &level.0, "inverse"),
        ] {
            let r = audit(name, from, to, &touched, provenance);
            assert!(
                r.matches_full_build,
                "{name} (provenance {provenance}): output differs"
            );
            assert!(r.confined(), "{name} (provenance {provenance}): {r:?}");
            if provenance && name == "forward" {
                forward = Some(r);
            }
        }
    }
    forward.unwrap()
}

/// ED-0111's witness: a wall rounding to one topology cell has no region and
/// no component link, yet its hot cell bends the island beside it — a room
/// outside the component closure is rebuilt, and that is PERMITTED by the
/// routing dependency. The routing-only set must be non-empty here, or the
/// permission's routing half is untested.
#[test]
fn a_collapsed_wall_is_justified_only_by_routing() {
    let s = |v: f64| v * Q;
    let mut level = Level::default();
    level.add((s(0.0), s(0.0)), (s(8.0), s(6.0)), s(2.5));
    level.square(s(-22.0), s(-22.0), s(22.0), s(22.0), s(2.0));
    let r = edit_both_ways(&level, |l| {
        vec![l.add((s(1.9375), s(3.0625)), (s(2.0625), s(3.0625)), s(0.125))]
    });
    assert_eq!(r.component_post, 0, "{r:?}");
    assert!(r.routing_only_post > 0 && r.routing_used_post > 0, "{r:?}");
}

/// Corner-touching unjoined walls close a room: the room appears through
/// walls the edit never touched.
#[test]
fn a_room_closed_by_corner_contact_is_confined() {
    let mut level = Level::default();
    level.add((10.5, 0.0), (10.5, 10.0), 1.0);
    level.add((0.0, 10.5), (10.0, 10.5), 1.0);
    level.add((-0.5, 0.0), (-0.5, 10.0), 1.0);
    let r = edit_both_ways(&level, |l| vec![l.add((0.0, -0.5), (10.0, -0.5), 1.0)]);
    assert_eq!(r.rooms_post, 1);
}

/// Covering an island with a wall and uncovering it again, with a remote
/// room elsewhere on the level that must never be rebuilt.
#[test]
fn covering_an_island_leaves_a_remote_room_alone() {
    let mut level = Level::default();
    level.square(0.0, 0.0, 6000.0, 6000.0, 200.0);
    level.square(2500.0, 2500.0, 3500.0, 3500.0, 100.0);
    level.square(20_000.0, 0.0, 24_000.0, 3000.0, 200.0);
    let r = edit_both_ways(&level, |l| {
        vec![l.add((2000.0, 3000.0), (4000.0, 3000.0), 1200.0)]
    });
    assert!(r.rooms_post >= 2 && r.actual_post < r.rooms_post, "{r:?}");
}

/// Deleting the last wall of a component leaves no post-state component, and
/// the pre-state room it bounded is accounted for.
#[test]
fn deleting_a_room_accounts_for_its_pre_state() {
    let mut level = Level::default();
    let ids = level.square(0.0, 0.0, 4000.0, 3000.0, 200.0);
    level.square(10_000.0, 0.0, 14_000.0, 3000.0, 200.0);
    let r = edit_both_ways(&level, |l| {
        l.remove(ids[0]);
        vec![ids[0]]
    });
    assert_eq!((r.rooms_pre, r.rooms_post), (2, 1), "{r:?}");
    assert_eq!(r.actual_pre, 1, "{r:?}");
}

// ---------------------------------------------------------------------------
// The drawn-region (R4.2.2) cut engine (ED-0113 decision 4).

use domowik_bench::locality::{audit_drawn, drawn_permission};
use domowik_format::{Document, Level as DocLevel, Opening, Wall};

/// Every accepted corpus gesture, audited on the cut engine: exact against
/// the one-shot drawn region and confined to its permission.
#[test]
fn every_gesture_is_confined_on_the_cut_engine() {
    for c in corpus::all().into_iter().chain(corpus::shapes()) {
        let level = c.primary_level();
        for g in gestures::all(&c.doc, level) {
            let mut session = session_over(c.doc.clone());
            if session.commit(g.operations.clone()).is_err() {
                continue;
            }
            let r = audit_drawn(g.class, &c.doc, session.document(), level);
            assert!(
                r.matches_full_build,
                "{}/{}: cut engine differs",
                c.name, g.class
            );
            assert!(r.confined(), "{}/{}: {r:?}", c.name, g.class);
        }
    }
}

/// Two remote rooms; a window on the first slides along its host. The cut
/// engine rebuilds the first room's walls only — and the audit DETECTS a
/// rebuild of the remote room: a cold build, which re-traces everything,
/// is reported unjustified.
#[test]
fn an_opening_edit_rebuilds_only_its_own_component() {
    let mut doc = Document::new();
    doc.levels.push(DocLevel::new("L1"));
    let level = doc.levels[0].id;
    let room = |doc: &mut Document, x0: f64| {
        let c = [
            (x0, 0.0),
            (x0 + 4000.0, 0.0),
            (x0 + 4000.0, 3000.0),
            (x0, 3000.0),
        ];
        let ids: Vec<_> = (0..4)
            .map(|i| {
                let (a, b) = (c[i], c[(i + 1) % 4]);
                let w = Wall::new(
                    level,
                    Point::new(a.0, a.1),
                    Point::new(b.0, b.1),
                    200.0,
                    2500.0,
                );
                let id = w.id;
                doc.walls.push(w);
                id
            })
            .collect();
        ids
    };
    let near = room(&mut doc, 0.0);
    room(&mut doc, 20_000.0);
    let window = Opening::window(near[0], 0.4, 800.0, 1200.0, 900.0, 100.0);
    let id = window.id;
    doc.openings.push(window);
    let mut after = doc.clone();
    after.openings.iter_mut().find(|o| o.id == id).unwrap().u = 0.6;

    let r = audit_drawn("window-slide", &doc, &after, level);
    assert!(r.matches_full_build && r.confined(), "{r:?}");
    assert!(r.actual_post >= 1, "the edited component is rebuilt");
    assert!(
        r.actual_post < r.polygons_post,
        "the remote one is not: {r:?}"
    );

    // A cold build reports every polygon; the permission must reject the
    // remote room's.
    let (_, allowed_post) = drawn_permission(&doc, &after, level);
    let solids = domowik_geometry::walls::level_solids(&after, level);
    let cuts = domowik_geometry::cuts::level_apertures(&after, &solids);
    let mut cold = domowik_geometry::incremental::LevelGeometry::occupied_only();
    cold.set_audit(true);
    cold.sync_cut(&solids, &cuts);
    let rebuilt = cold.last_audit().unwrap().occupied_after.clone();
    assert!(
        rebuilt.iter().any(|ring| !allowed_post.contains(ring)),
        "a whole-level rebuild must be unjustified"
    );
}

fn level_doc() -> (Document, uuid::Uuid) {
    let mut doc = Document::new();
    doc.levels.push(DocLevel::new("L1"));
    let level = doc.levels[0].id;
    (doc, level)
}

fn add_wall(
    doc: &mut Document,
    level: uuid::Uuid,
    a: (f64, f64),
    b: (f64, f64),
    t: f64,
) -> uuid::Uuid {
    let w = Wall::new(level, Point::new(a.0, a.1), Point::new(b.0, b.1), t, 2500.0);
    let id = w.id;
    doc.walls.push(w);
    id
}

/// Sol's first witness: a window slides on a wall standing inside a
/// frame's HOLE. The frame's material does not overlap the change, so a
/// rebuild of the frame is NOT permitted (a hole-blind check permitted it).
#[test]
fn a_component_in_a_hole_is_not_permitted_by_the_enclosing_frame() {
    let (mut doc, level) = level_doc();
    let c = [
        (0.0, 0.0),
        (10_000.0, 0.0),
        (10_000.0, 10_000.0),
        (0.0, 10_000.0),
    ];
    for i in 0..4 {
        add_wall(&mut doc, level, c[i], c[(i + 1) % 4], 200.0);
    }
    let inner = add_wall(&mut doc, level, (3000.0, 5000.0), (7000.0, 5000.0), 200.0);
    let window = Opening::window(inner, 0.4, 800.0, 500.0, 1000.0, 100.0);
    let id = window.id;
    doc.openings.push(window);
    let mut after = doc.clone();
    after.openings.iter_mut().find(|o| o.id == id).unwrap().u = 0.6;
    let r = audit_drawn("window-slide-in-hole", &doc, &after, level);
    assert!(r.matches_full_build && r.confined(), "{r:?}");
    let (_, allowed_post) = drawn_permission(&doc, &after, level);
    assert_eq!(
        allowed_post.len(),
        1,
        "only the inner wall's polygon: {allowed_post:?}"
    );
}

/// Sol's second witness, both directions: thickening a host until it
/// engulfs a separate wall removes that wall's polygon (and thinning brings
/// it back). The engulfed polygon is permitted to change.
#[test]
fn an_engulfed_component_is_permitted_both_ways() {
    let (mut doc, level) = level_doc();
    let host = add_wall(&mut doc, level, (0.0, 1000.0), (4000.0, 1000.0), 200.0);
    add_wall(&mut doc, level, (1000.0, 0.0), (2000.0, 0.0), 200.0);
    doc.openings
        .push(Opening::window(host, 0.8, 500.0, 500.0, 1000.0, 100.0));
    let mut thick = doc.clone();
    thick
        .walls
        .iter_mut()
        .find(|w| w.id == host)
        .unwrap()
        .thickness_mm = 2400.0;
    for (from, to, name) in [(&doc, &thick, "thicken"), (&thick, &doc, "thin")] {
        let r = audit_drawn(name, from, to, level);
        assert!(r.matches_full_build && r.confined(), "{name}: {r:?}");
    }
}
