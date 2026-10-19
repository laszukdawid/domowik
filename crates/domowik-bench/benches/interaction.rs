//! Criterion benchmarks over the interaction corpus (issue 0032).
//!
//! # Relationship to the harness binary
//!
//! `domowik-bench` (the binary) is the reporting instrument: it produces the
//! whole published table in one run, including allocation counts and the
//! locality comparison, which criterion has no way to express. These
//! benchmarks are the REGRESSION instrument: criterion keeps a baseline
//! between runs and reports a change, which a one-shot report cannot.
//!
//! Both measure the same unit — commit the batch, then rebuild the level's
//! derived geometry — so a number here and a number there are comparable.
//!
//! `scripts/verify.sh` compiles this on every run so the harness cannot rot.
//! Timings are host-dependent and are not a pass/fail gate (ED-0070).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use domowik_bench::gestures::Outcome;
use domowik_bench::{corpus, gestures, session_over};
use domowik_geometry::walls;

/// The full derived pass over one level: what every accepted edit triggers.
fn derived(c: &mut Criterion) {
    let mut group = c.benchmark_group("derived");
    for corpus in corpus::all() {
        let s = walls::level_solids(&corpus.doc, corpus.primary_level());
        group.bench_function(corpus.name, |b| {
            b.iter(|| walls::plan(black_box(&s.walls), black_box(&s.joins)))
        });
    }
    group.finish();
}

/// One gesture: commit the batch, then rebuild derived geometry.
///
/// The session and the batch are cloned in the setup closure, which criterion
/// excludes from the timing. Without that the number would be dominated by
/// copying the document.
fn mix(c: &mut Criterion) {
    for corpus in corpus::all() {
        let level = corpus.primary_level();
        let base = session_over(corpus.doc.clone());
        let mut group = c.benchmark_group(format!("gesture/{}", corpus.name));
        for g in gestures::all(&corpus.doc, level) {
            if g.outcome == Outcome::Rejected {
                // A refused batch is measured by the harness binary, which can
                // say so in its table. Criterion's output has no column for
                // "this one is supposed to fail", and an unexplained fast row
                // invites the wrong conclusion.
                continue;
            }
            group.bench_function(g.class, |b| {
                b.iter_batched(
                    || (base.clone(), g.operations.clone()),
                    |(mut session, ops)| {
                        let accepted = session.commit(ops).is_ok();
                        let s = walls::level_solids(session.document(), level);
                        (accepted, walls::plan(&s.walls, &s.joins))
                    },
                    criterion::BatchSize::SmallInput,
                )
            });
        }
        group.finish();
    }
}

criterion_group!(benches, derived, mix);
criterion_main!(benches);
