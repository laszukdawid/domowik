//! The Rust reader against the hostile fixture corpus (issue 0028).
//!
//! # The contract this asserts
//!
//! **Every fixture under `testdata/corpus/hostile/` must be REFUSED, and must
//! not panic.** That is FORMAT-SPEC §9's threat model reduced to the one
//! property a test can check over a whole corpus without reproducing each
//! fixture's expected diagnostic: hostile input produces an error, never a
//! document and never a crash.
//!
//! It is deliberately weaker than `EXPECTATIONS.md`, which records the exact
//! diagnostic each fixture should produce. Asserting the class rather than the
//! code is honest about what this covers: a fixture rejected for the wrong
//! reason passes here. What it cannot do is pass by accident — accepting a
//! hostile package, or dying on one, both fail.
//!
//! The corpus was built for the frozen Go reader. Running the RUST reader
//! against it is the point: these fixtures encode the format's threat model,
//! not one implementation's bugs, and a fresh reader that has never met them
//! is exactly the reader that should.

use std::path::{Path, PathBuf};

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/corpus/hostile")
        .canonicalize()
        .expect("the hostile corpus is tracked in this repository")
}

fn fixtures() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![corpus_root()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("readable corpus directory") {
            let path = entry.expect("readable entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "domo") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn the_corpus_is_present_and_not_empty() {
    // A corpus test that silently covers nothing is worse than no test: it
    // reports green for a tree that does not exist.
    let found = fixtures();
    assert!(
        found.len() >= 20,
        "expected the tracked hostile corpus, found {} fixture(s)",
        found.len()
    );
}

#[test]
fn a_well_formed_package_is_accepted_by_the_same_check() {
    // POSITIVE CONTROL. The corpus test asserts that nothing was accepted,
    // and an assertion of that shape passes just as happily if `read_package`
    // rejects EVERYTHING — including valid plans. No mutation of the corpus
    // test can catch that, because the property it asserts is genuinely true;
    // only a case that SHOULD be accepted can show the check discriminates.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("valid.domo");
    let package = domowik_container::Package::new(domowik_format::Document::with_ground_floor());
    domowik_container::write_package(&path, &package).expect("write");

    let read = domowik_container::read_package(&path).expect("a valid package must be accepted");
    assert_eq!(read.snapshot.levels.len(), 1);
}

#[test]
fn every_hostile_fixture_is_refused_without_panicking() {
    let mut accepted = Vec::new();
    for path in fixtures() {
        let name = path
            .strip_prefix(corpus_root())
            .unwrap_or(&path)
            .display()
            .to_string();

        // `read_package` is the whole public entry point, so a panic anywhere
        // beneath it is a panic a user can trigger with a file. Catching it
        // here turns "the test binary died" into "this fixture panicked",
        // which is the difference between a bug report and a mystery.
        let result = std::panic::catch_unwind(|| domowik_container::read_package(&path));
        match result {
            Err(_) => panic!("PANICKED on hostile fixture {name}"),
            Ok(Ok(package)) => accepted.push(format!(
                "{name} (parsed: {} level(s), {} batch(es), {} blob(s))",
                package.snapshot.levels.len(),
                package.batches.len(),
                package.assets.len()
            )),
            Ok(Err(_)) => {}
        }
    }

    assert!(
        accepted.is_empty(),
        "{} hostile fixture(s) were ACCEPTED:\n  {}",
        accepted.len(),
        accepted.join("\n  ")
    );
}
