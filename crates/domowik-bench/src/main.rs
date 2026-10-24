//! The interaction benchmark harness (issue 0032).
//!
//! Prints a Markdown report: the corpus, the operation mix, per-class
//! percentiles, allocation cost, and the invalidation-locality comparison.
//!
//! ```text
//! domowik-bench [--smoke] [--iterations N] [--warmup N]
//! ```
//!
//! `--smoke` runs three iterations of everything. It exists so
//! `scripts/verify.sh` can prove the harness still WORKS on any host without
//! pretending that three samples on a shared CI machine are a measurement.
//! Timings are never a pass/fail gate (ED-0070); the gate is that the harness
//! runs and that the locality comparison still holds.

use domowik_bench::gestures::{Outcome, Source};
use domowik_bench::metrics::{self, Counting};
use domowik_bench::{corpus, gestures, locality, session_over};
use domowik_geometry::cuts;
use domowik_geometry::incremental::LevelGeometry;
use domowik_geometry::locality as geo_locality;
use domowik_geometry::rooms;
use domowik_geometry::snap::SnapIndex;
use domowik_geometry::walls;
use std::alloc::System;

#[global_allocator]
static ALLOCATOR: Counting<System> = Counting(System);

struct Options {
    iterations: usize,
    warmup: usize,
    /// `--export NAME PATH`: write that corpus as a `.domo` package and exit.
    export: Option<(String, std::path::PathBuf)>,
}

fn parse_options() -> Result<Options, String> {
    let mut o = Options {
        iterations: 50,
        warmup: 5,
        export: None,
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--smoke" => {
                o.iterations = 3;
                o.warmup = 1;
            }
            "--iterations" | "--warmup" => {
                let v: usize = args
                    .next()
                    .ok_or_else(|| format!("{arg} needs a value"))?
                    .parse()
                    .map_err(|_| format!("{arg} needs a whole number"))?;
                if arg == "--iterations" {
                    if v == 0 {
                        return Err("--iterations must be at least 1".to_owned());
                    }
                    o.iterations = v;
                } else {
                    o.warmup = v;
                }
            }
            "--export" => {
                let name = args.next().ok_or("--export needs a corpus name")?;
                let path = args.next().ok_or("--export needs an output path")?;
                o.export = Some((name, path.into()));
            }
            "--help" | "-h" => return Err("usage".to_owned()),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(o)
}

/// Nanoseconds as a human figure, without inventing precision.
fn ns(v: u64) -> String {
    if v < 10_000 {
        format!("{v} ns")
    } else if v < 1_000_000 {
        format!("{:.1} µs", v as f64 / 1000.0)
    } else {
        format!("{:.2} ms", v as f64 / 1_000_000.0)
    }
}

fn bytes(v: i64) -> String {
    let a = v.unsigned_abs();
    let sign = if v < 0 { "-" } else { "" };
    if a < 8192 {
        format!("{sign}{a} B")
    } else if a < 1 << 20 {
        format!("{sign}{:.1} KiB", a as f64 / 1024.0)
    } else {
        format!("{sign}{:.1} MiB", a as f64 / (1024.0 * 1024.0))
    }
}

fn main() {
    let options = match parse_options() {
        Ok(o) => o,
        Err(message) => {
            eprintln!("domowik-bench: {message}");
            eprintln!(
                "usage: domowik-bench [--smoke] [--iterations N] [--warmup N] | --export CORPUS PATH"
            );
            std::process::exit(2);
        }
    };
    if let Some((name, path)) = &options.export {
        if let Err(message) = corpus::export(name, path) {
            eprintln!("domowik-bench: {message}");
            std::process::exit(1);
        }
        return;
    }

    println!("# Interaction benchmark run\n");
    print_environment(&options);
    let corpora: Vec<corpus::Corpus> = corpus::all().into_iter().chain(corpus::shapes()).collect();
    print_corpus_table(&corpora);
    print_derived_table(&corpora, &options);
    print_gesture_tables(&corpora, &options);
    print_locality_table(&corpora);
}

fn print_environment(options: &Options) {
    println!("## Environment\n");
    println!("| Property | Value |");
    println!("|---|---|");
    println!("| Architecture | `{}` |", std::env::consts::ARCH);
    println!("| Operating system | `{}` |", std::env::consts::OS);
    println!(
        "| Build | `{}` |",
        if cfg!(debug_assertions) {
            "debug — timings from this profile are NOT comparable"
        } else {
            "release"
        }
    );
    println!(
        "| Logical CPUs | {} |",
        std::thread::available_parallelism()
            .map(|n| n.get().to_string())
            .unwrap_or_else(|_| "unknown".to_owned())
    );
    println!("| Iterations | {} |", options.iterations);
    println!("| Warmup | {} |", options.warmup);
    println!(
        "\nThe CPU model, memory and machine identity are NOT readable from \
         inside this process and are recorded by hand in \
         `docs/BENCHMARK-INTERACTION.md`. A number without that record is not \
         a measurement.\n"
    );
}

fn print_corpus_table(corpora: &[corpus::Corpus]) {
    println!("## Corpus\n");
    println!("| Corpus | Levels | Walls | Joins | Openings | Components | Faces | Shape |");
    println!("|---|---:|---:|---:|---:|---:|---:|---|");
    for c in corpora {
        let level = c.primary_level();
        let s = walls::level_solids(&c.doc, level);
        let components = geo_locality::components_of(&s.walls, &s.joins);
        let (_, faces) = walls::plan(&s.walls, &s.joins);
        println!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} |",
            c.name,
            c.levels.len(),
            c.doc.walls.len(),
            c.doc.joins.len(),
            c.doc.openings.len(),
            components.len(),
            faces.len(),
            c.description
        );
    }
    println!(
        "\nComponents and faces are for the FIRST level only; walls, joins and \
         openings count the whole document.\n"
    );
}

fn print_derived_table(corpora: &[corpus::Corpus], options: &Options) {
    println!("## Derived geometry: occupied region and room faces\n");
    println!(
        "The whole-level rebuild every edit currently triggers. `region` is \
         R3.2.1; `plan` additionally traces the R5.1.1 faces off the same \
         boundary, so the difference between the two columns is the true cost \
         of room detection.\n"
    );
    println!(
        "| Corpus | p50 region | p50 plan | p95 plan | p99 plan | allocations | peak | \
         p50 cold engine | p99 cold engine |"
    );
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");
    for c in corpora {
        let s = walls::level_solids(&c.doc, c.primary_level());
        let region = metrics::measure(
            options.iterations,
            options.warmup,
            || (),
            |()| walls::occupied_region(&s.walls, &s.joins),
        );
        let plan = metrics::measure(
            options.iterations,
            options.warmup,
            || (),
            |()| walls::plan(&s.walls, &s.joins),
        );
        let cost = metrics::measure_allocations(|| (), |()| walls::plan(&s.walls, &s.joins));
        // The retained engine built from nothing, in the editor's provenance
        // mode: what the first query of a level pays once.
        let cold = metrics::measure(
            options.iterations,
            options.warmup,
            || (),
            |()| {
                let mut g = LevelGeometry::with_provenance();
                g.sync(&s);
                g
            },
        );
        println!(
            "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} |",
            c.name,
            ns(region.p50()),
            ns(plan.p50()),
            ns(plan.p95()),
            ns(plan.p99()),
            cost.allocations,
            bytes(cost.peak_extra_bytes),
            ns(cold.p50()),
            ns(cold.p99())
        );
    }
    println!();
}

fn print_gesture_tables(corpora: &[corpus::Corpus], options: &Options) {
    println!("## Operation mix\n");
    println!(
        "One sample times one gesture against freshly prepared state: commit \
         the batch, then rebuild the level's derived geometry with the one-shot \
         `walls::plan`. The `editor` columns time what the editor actually \
         runs instead: commit, sync the retained `SnapIndex` (whose \
         incremental geometry is the level's plan), project rooms through \
         it, and sync the drawn region's cut engine when the level hosts \
         openings (R4.2.2). Allocation and peak columns are for the one-shot path. \
         Preparation is outside the timed region. `built by` distinguishes a batch the \
         product's own code produces from one this harness assembles because \
         no command builds it yet.\n"
    );
    for c in corpora {
        let level = c.primary_level();
        let all = gestures::all(&c.doc, level);
        if all.is_empty() {
            println!(
                "### `{}`\n\nNo gesture has a target on this shape.\n",
                c.name
            );
            continue;
        }
        let base = session_over(c.doc.clone());
        // The editor's retained state for the level before the gesture: the
        // snapping index owns the level's incremental geometry (provenance
        // mode), exactly as the native bridge keeps it.
        let mut retained = SnapIndex::new(level);
        retained.sync(&c.doc);
        // And the drawn region's occupied-only engine (R4.2.2), kept only
        // while the level hosts an opening — as the bridge keeps it.
        let pre = walls::level_solids(&c.doc, level);
        let pre_cuts = cuts::level_apertures(&c.doc, &pre);
        let drawn = (!pre_cuts.is_empty()).then(|| {
            let mut g = LevelGeometry::occupied_only();
            g.sync_cut(&pre, &pre_cuts);
            g
        });
        println!("### `{}`\n", c.name);
        println!(
            "| Class | Built by | Ops | Outcome | p50 | p95 | p99 | max | allocations | peak | \
             editor p50 | editor p95 | editor p99 | editor max |"
        );
        println!("|---|---|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
        for g in &all {
            let samples = metrics::measure(
                options.iterations,
                options.warmup,
                || (base.clone(), g.operations.clone()),
                |(mut session, ops)| {
                    let accepted = session.commit(ops);
                    let s = walls::level_solids(session.document(), level);
                    let derived = walls::plan(&s.walls, &s.joins);
                    (accepted.is_ok(), derived)
                },
            );
            let cost = metrics::measure_allocations(
                || (base.clone(), g.operations.clone()),
                |(mut session, ops)| {
                    let accepted = session.commit(ops);
                    let s = walls::level_solids(session.document(), level);
                    let derived = walls::plan(&s.walls, &s.joins);
                    (accepted.is_ok(), derived)
                },
            );
            // What the editor pays instead: commit, sync the retained level
            // geometry, and project rooms through it. Cloning the retained
            // state is preparation, outside the sample.
            let editor = metrics::measure(
                options.iterations,
                options.warmup,
                || {
                    (
                        base.clone(),
                        retained.clone(),
                        drawn.clone(),
                        g.operations.clone(),
                    )
                },
                |(mut session, mut snap, mut drawn, ops)| {
                    let accepted = session.commit(ops);
                    let doc = session.document();
                    snap.sync(doc);
                    let rooms = rooms::project_retained(doc, level, snap.geometry());
                    let solids = walls::level_solids(doc, level);
                    let apertures = cuts::level_apertures(doc, &solids);
                    let region = if apertures.is_empty() {
                        drawn = None;
                        None
                    } else {
                        let g = drawn.get_or_insert_with(LevelGeometry::occupied_only);
                        g.sync_cut(&solids, &apertures);
                        Some(g.occupied())
                    };
                    (accepted.is_ok(), session, snap, drawn, rooms, region)
                },
            );
            println!(
                "| `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                g.class,
                match g.source {
                    Source::Product => "product",
                    Source::Harness => "harness",
                },
                g.operations.len(),
                match g.outcome {
                    Outcome::Accepted => "accepted",
                    Outcome::Rejected => "rejected",
                },
                ns(samples.p50()),
                ns(samples.p95()),
                ns(samples.p99()),
                ns(samples.max()),
                cost.allocations,
                bytes(cost.peak_extra_bytes),
                ns(editor.p50()),
                ns(editor.p95()),
                ns(editor.p99()),
                ns(editor.max())
            );
        }
        println!();
        let missing: Vec<&str> = ALL_CLASSES
            .iter()
            .filter(|c| !all.iter().any(|g| g.class == **c))
            .copied()
            .collect();
        if !missing.is_empty() {
            println!(
                "Not exercised on this shape (no valid target): {}.\n",
                missing
                    .iter()
                    .map(|c| format!("`{c}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

/// Every class the mix defines, so a corpus that cannot supply a target for
/// one is reported as a gap instead of silently having a shorter table.
const ALL_CLASSES: &[&str] = &[
    "wall-create-free",
    "wall-create-joined",
    "wall-create-split",
    "endpoint-drag",
    "joint-drag",
    "body-drag",
    "detach",
    "numeric-edit",
    "wall-delete",
    "opening-create",
    "opening-edit",
    "opening-delete",
    "opening-cascade-delete",
    "host-reverse",
    "host-shorten-reject",
];

fn print_locality_table(corpora: &[corpus::Corpus]) {
    println!("## Invalidation locality (R3.4.8, R5.1.9, ED-0111)\n");
    println!(
        "What the editor's engine (`incremental::LevelGeometry`, provenance \
         mode) actually recomputed for each accepted gesture, against what it \
         was permitted to. Rooms are counted before / after the edit. \
         `component` is R5.1.9's closure; `routing` the rooms ED-0111 adds \
         (boundary through a cell the edit makes hot or cold, or along a \
         rerouted segment), outside the component closure. `whole level` \
         marks rows whose component permission is the whole level anyway, so \
         confinement demonstrates nothing.\n"
    );
    println!(
        "| Corpus | Class | Components | Walls | Wall sources | Rooms | \
         Component | Routing | Rebuilt | Via routing | Confined | Whole level |"
    );
    println!("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|");
    for c in corpora {
        let level = c.primary_level();
        for g in gestures::all(&c.doc, level) {
            if g.outcome == Outcome::Rejected {
                continue;
            }
            let mut session = session_over(c.doc.clone());
            session
                .commit(g.operations.clone())
                .expect("a gesture declared accepted commits (checked by the gesture tables)");
            let r = locality::report(g.class, &c.doc, session.document(), level, &g.touched);
            let pair = |a: usize, b: usize| format!("{a} / {b}");
            println!(
                "| `{}` | `{}` | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                c.name,
                r.class,
                r.components,
                r.walls,
                r.wall_sources,
                pair(r.rooms_pre, r.rooms_post),
                pair(r.component_pre, r.component_post),
                pair(r.routing_only_pre, r.routing_only_post),
                pair(r.actual_pre, r.actual_post),
                pair(r.routing_used_pre, r.routing_used_post),
                if r.confined() && r.matches_full_build {
                    "yes"
                } else {
                    "**no**"
                },
                if r.closure_is_whole_level() {
                    "yes"
                } else {
                    "no"
                }
            );
        }
    }
    println!();
}
