//! Invalidation-locality checks (issue 0032; BEHAVIOR-SPEC R3.4.8, R5.1.9).
//!
//! No assertion here depends on a clock. Issue 0032 requires benchmarks and
//! correctness tests to be separate artifacts, and a locality property is a
//! correctness property: it holds or it does not, whatever the machine is
//! doing.

use domowik_bench::gestures::{self, Outcome};
use domowik_bench::locality::{report, Report};
use domowik_bench::{corpus, session_over};

/// Every accepted gesture on every corpus, with its locality report.
fn reports() -> Vec<(String, Report)> {
    let mut out = Vec::new();
    for c in corpus::all().into_iter().chain(corpus::shapes()) {
        let level = c.primary_level();
        for g in gestures::all(&c.doc, level) {
            if g.outcome == Outcome::Rejected {
                continue;
            }
            let mut session = session_over(c.doc.clone());
            if session.commit(g.operations.clone()).is_err() {
                continue;
            }
            let r = report(g.class, &c.doc, session.document(), level, &g.touched);
            out.push((c.name.to_owned(), r));
        }
    }
    assert!(!out.is_empty(), "the corpus produced no gestures at all");
    out
}

#[test]
fn the_closure_never_exceeds_the_level() {
    for (corpus, r) in reports() {
        assert!(
            r.closure_walls <= r.walls,
            "{corpus}/{}: closure admits {} walls on a {}-wall level",
            r.class,
            r.closure_walls,
            r.walls
        );
        assert!(
            r.closure_faces <= r.faces,
            "{corpus}/{}: closure admits {} faces on a {}-face level",
            r.class,
            r.closure_faces,
            r.faces
        );
        assert!(
            r.closure_components <= r.components,
            "{corpus}/{}: closure admits more components than exist",
            r.class
        );
    }
}

/// A gesture always touches something, so the closure is never empty — except
/// where the gesture creates a component that encloses nothing, which is the
/// `wall-create-free` case and is why the FACE closure may legitimately be 0.
#[test]
fn every_gesture_puts_at_least_one_component_in_scope() {
    for (corpus, r) in reports() {
        assert!(
            r.closure_components >= 1,
            "{corpus}/{}: nothing in scope for an accepted edit",
            r.class
        );
    }
}

/// The corpus must contain shapes where locality is a real constraint. If
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
}

/// CANARY — this test records a KNOWN DEFECT and is expected to FAIL the day
/// it is fixed.
///
/// `walls::plan` is the only derived-geometry entry point and it takes a whole
/// level, so every edit rebuilds every component of the level. On the
/// multi-cluster corpora that exceeds the scope R3.4.8 permits. Issue 0013
/// owns the scoped recomputation that will close it; issue 0032 owns this
/// measurement.
///
/// It asserts the violation still EXISTS rather than asserting it is absent,
/// because a test that merely tolerated the defect would pass in both worlds
/// and tell nobody when the work landed.
#[test]
fn whole_level_recomputation_still_exceeds_the_permitted_closure() {
    let all = reports();
    let violations: Vec<&(String, Report)> = all.iter().filter(|(_, r)| !r.confined()).collect();
    assert!(
        !violations.is_empty(),
        "no gesture exceeds its closure any more — recomputation appears to \
         have been scoped. Delete this canary, record the change against \
         issues 0013 and 0032, and replace it with the positive assertion \
         that every gesture is confined."
    );

    // The worst case, reported so the report and the test cannot drift apart.
    let worst = violations
        .iter()
        .max_by_key(|(_, r)| r.engine_faces.saturating_sub(r.closure_faces))
        .expect("violations is non-empty");
    println!(
        "worst overshoot: {}/{} recomputes {} faces where {} are permitted \
         ({} walls where {} are permitted)",
        worst.0,
        worst.1.class,
        worst.1.engine_faces,
        worst.1.closure_faces,
        worst.1.engine_walls,
        worst.1.closure_walls
    );
}

/// Whatever the engine's scope is, it must at least be CORRECT: the closure
/// the specification proves is a subset of what a whole-level rebuild covers,
/// so a closure larger than the level would mean the checker is wrong.
#[test]
fn the_engine_is_never_narrower_than_the_closure() {
    for (corpus, r) in reports() {
        assert!(
            r.engine_walls >= r.closure_walls && r.engine_faces >= r.closure_faces,
            "{corpus}/{}: engine recomputes {}/{} but the closure demands \
             {}/{} — the engine is missing required work",
            r.class,
            r.engine_walls,
            r.engine_faces,
            r.closure_walls,
            r.closure_faces
        );
    }
}
