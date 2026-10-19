//! `check` on drafts with many unjoined ends (issue 0103). Timings are
//! host-dependent and never a gate; verify.sh only compiles this.
//!
//! The diagonal case is the expensive one: each near-miss end searches for a
//! point the core accepts on a diagonal axis. An exact on-axis filter keeps
//! that search from running the edit builder and a trial commit for points
//! that cannot form a contact (review finding N3: 2 s before the filter).

use domowik_document::operation::Operation;
use domowik_document::session::Session;
use domowik_format::{Entity, Level, Point, Wall};
use std::path::Path;
use std::time::Instant;

fn draft(pairs: usize, diagonal: bool) -> Session {
    let mut session = Session::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    session
        .commit(vec![Operation::create(Entity::Level(level))])
        .expect("level");
    for i in 0..pairs {
        let (ox, oy) = ((i % 25) as f64 * 12_000.0, (i / 25) as f64 * 12_000.0);
        let host_end = if diagonal {
            (4999.0, 3001.0)
        } else {
            (5000.0, 0.0)
        };
        let stub = if diagonal {
            ((1001.0, 4000.0), (1001.0, 850.0))
        } else {
            ((1000.0, 3000.0), (1000.0, 200.0))
        };
        for (a, b) in [((0.0, 0.0), host_end), stub] {
            let wall = Wall::new(
                level_id,
                Point::new(ox + a.0, oy + a.1),
                Point::new(ox + b.0, oy + b.1),
                120.0,
                2500.0,
            );
            let edit = domowik_edit::add_wall_operations(session.document(), level_id, wall)
                .expect("separated walls admit");
            session.commit(edit.operations).expect("commits");
        }
    }
    session
}

fn main() {
    for (name, diagonal) in [("axis-aligned", false), ("diagonal", true)] {
        let session = draft(500, diagonal);
        let doc = session.document().clone();
        let mut samples = Vec::new();
        for _ in 0..7 {
            let start = Instant::now();
            let report =
                domowik_cli::check::run(&doc, Some(&session), Path::new("bench.domo"), None)
                    .expect("runs");
            samples.push(start.elapsed());
            assert!(report.warnings >= 500, "every stub is a near miss");
        }
        samples.sort();
        println!(
            "check, 1000 walls, 500 {name} near misses: median {:?}",
            samples[samples.len() / 2]
        );
    }
}
