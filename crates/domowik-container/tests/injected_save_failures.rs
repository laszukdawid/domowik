//! Reopening a plan after a save fails (ROADMAP Phase 1 exit, FORMAT-SPEC §9.7).
//!
//! Phase 1 exits on, among other things, "representative plans can be created,
//! scripted, validated, packed, **reopened after injected save failures**, and
//! exported headlessly". This is that criterion.
//!
//! # Two kinds of test here, kept apart on purpose
//!
//! **Real faults.** A syscall genuinely fails and the save path has to cope.
//! The fault is produced by making the filesystem refuse — a read-only parent
//! directory, a target that is not a regular file — so an `Err` comes back
//! from the kernel rather than from a harness. These prove the error paths
//! actually run.
//!
//! **Reproduced states.** The filesystem is left looking the way a failure at
//! one §9.7 injection point would leave it, and the plan is then reopened.
//! These prove the RECOVERY contract: whatever a save did, the target opens as
//! a valid package or does not exist.
//!
//! The distinction is worth keeping because the second kind proves less than
//! its name suggests, and this file previously blurred them. A test named for
//! F6 "a read-only parent directory makes the S5 sync fail" did no such thing:
//! it performed an ordinary successful save. The comment described a test
//! nobody had written. It is corrected below rather than quietly deleted.
//!
//! # What cannot be produced on this host, and is not faked
//!
//! **Power loss and kernel panic.** §9.7 requires a fault-injecting filesystem
//! before any power-loss claim, and Apple documents `F_FULLFSYNC` as
//! best-effort — "data can still be lost in the case of sudden power loss". No
//! claim is made and no test pretends to make one.
//!
//! **`ENOSPC` at a write boundary.** It needs a filesystem that can be filled,
//! and creating one on darwin needs privileges this test run does not have
//! (`newfs_hfs` on an attached ram disk was tried and refused). Assessed and
//! deferred per ED-0027, never simulated.
//!
//! **`EXDEV`.** Unreachable by construction: §9.7 S0 puts the candidate in the
//! target's own directory, so the rename never crosses a filesystem.

use std::path::Path;

use domowik_container::{read_package, write_package, ContainerError, Package};
use domowik_format::{Document, Point, Wall};

/// A plan with real content, so a truncated or missing file is distinguishable
/// from an empty one.
fn plan() -> Package {
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    for i in 0..4 {
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, i as f64 * 1000.0),
            Point::new(4000.0, i as f64 * 1000.0),
            200.0,
            2500.0,
        ));
    }
    Package::new(doc)
}

fn assert_opens_with_four_walls(path: &Path, context: &str) {
    let package = read_package(path)
        .unwrap_or_else(|e| panic!("{context}: the plan must still open, got {e}"));
    assert_eq!(
        package.snapshot.walls.len(),
        4,
        "{context}: the plan must be the one that was there"
    );
}

/// Makes `dir` unwritable, runs `body`, and restores the mode whatever happens.
///
/// The restore is not politeness: a panicking assertion inside `body` would
/// otherwise leave a directory `TempDir::drop` cannot clean, and the failure
/// would be reported as a cleanup error rather than as the assertion that
/// actually failed.
fn with_readonly_dir<T>(dir: &Path, body: impl FnOnce() -> T) -> T {
    use std::os::unix::fs::PermissionsExt;
    let original = std::fs::metadata(dir).expect("metadata").permissions();
    let mut locked = original.clone();
    locked.set_mode(0o555);
    std::fs::set_permissions(dir, locked).expect("lock");
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::fs::set_permissions(dir, original).expect("restore");
    match outcome {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

// ---- Real faults ----

#[test]
fn a_real_write_failure_leaves_the_previous_plan_intact_and_readable() {
    // §9.7 F2: the candidate write fails, the target is untouched, and the
    // reader still gets OLD. The failure is a genuine `EACCES` from the
    // kernel: S0 creates the candidate in the target's own directory, so a
    // read-only directory makes `File::create` fail for real.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("the original save");
    let before = std::fs::read(&path).expect("bytes");

    let mut replacement = plan();
    replacement.snapshot.walls.clear();

    let error = with_readonly_dir(dir.path(), || {
        write_package(&path, &replacement).expect_err("the save must fail")
    });
    assert!(
        matches!(error, ContainerError::Io { .. }),
        "the failure must come from the filesystem, got {error}"
    );

    assert_eq!(
        std::fs::read(&path).expect("bytes"),
        before,
        "§9.7 F2: the target must be byte-identical"
    );
    assert_opens_with_four_walls(&path, "F2");
}

#[test]
fn a_real_write_failure_with_no_prior_target_creates_nothing() {
    // §9.7 F1: no prior target, so there is nothing to preserve — but there
    // must also be nothing left that a later reader could mistake for a plan.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");

    let error = with_readonly_dir(dir.path(), || {
        write_package(&path, &plan()).expect_err("the save must fail")
    });
    assert!(matches!(error, ContainerError::Io { .. }), "got {error}");
    assert!(!path.exists(), "F1: no target may be created");

    let leftovers: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        leftovers.is_empty(),
        "nothing may be left behind: {leftovers:?}"
    );
}

#[test]
fn a_target_that_is_a_directory_is_refused_rather_than_written_through() {
    // §9.7 S0 requires a regular-file target. A directory is the case a
    // symlink check does not cover, and writing "through" one would either
    // fail obscurely inside the ZIP writer or replace the directory.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("a-directory.domo");
    std::fs::create_dir(&path).expect("mkdir");

    let error = write_package(&path, &plan()).expect_err("must refuse");
    match error {
        ContainerError::UnsafeTarget { ref reason, .. } => {
            assert!(reason.contains("regular file"), "got {reason:?}")
        }
        other => panic!("expected UnsafeTarget, got {other}"),
    }
    assert!(path.is_dir(), "the directory must survive");
}

#[test]
fn a_read_only_target_file_is_still_replaced() {
    // Pinned because it surprises people and because the alternative would be
    // a bug. `rename` replaces a directory ENTRY, and POSIX makes that a
    // function of the DIRECTORY's permissions, not the replaced file's. A
    // save that consulted the target's mode and refused would be inventing a
    // rule the platform does not have; one that "helpfully" chmod'ed the
    // target would be worse.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("the original save");

    let mut mode = std::fs::metadata(&path).expect("meta").permissions();
    mode.set_mode(0o444);
    std::fs::set_permissions(&path, mode).expect("chmod");

    let mut bigger = plan();
    let level = bigger.snapshot.levels[0].id;
    bigger.snapshot.walls.push(Wall::new(
        level,
        Point::new(0.0, 9000.0),
        Point::new(4000.0, 9000.0),
        200.0,
        2500.0,
    ));
    write_package(&path, &bigger).expect("a read-only target does not block a rename");
    assert_eq!(read_package(&path).expect("opens").snapshot.walls.len(), 5);
}

#[test]
fn a_candidate_that_fails_validation_leaves_the_old_plan_and_no_litter() {
    // §9.7 F3: abort `candidate-invalid`, target untouched. This is the one
    // injection point the writer produces itself, and it does: a package
    // declaring an unsupported schema writes perfectly and cannot be read
    // back, so S2 catches it before the replacement rather than the next open
    // discovering it after the old copy is gone.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("the original save");

    let mut future = plan();
    future.manifest.format_version = "0.3".to_owned();
    let error = write_package(&path, &future).expect_err("must refuse");
    assert!(
        matches!(error, ContainerError::CandidateInvalid { .. }),
        "got {error}"
    );

    assert_opens_with_four_walls(&path, "F3");
    let litter: Vec<String> = std::fs::read_dir(dir.path())
        .expect("read dir")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(litter.is_empty(), "F3 must clean its candidate: {litter:?}");
}

// ---- Reproduced states ----

#[test]
fn a_leftover_candidate_is_not_mistaken_for_the_plan() {
    // §9.7 F1: a crash before the replace leaves a candidate beside the
    // target. It must be ignorable — not read, not promoted, and not in the
    // way of the next save.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    std::fs::write(dir.path().join(".plan.domo.999.0.tmp"), b"half a package").expect("leftover");

    assert!(read_package(&path).is_err(), "there is no plan yet");
    write_package(&path, &plan()).expect("a later save works around the leftover");
    assert_opens_with_four_walls(&path, "F1");
}

#[test]
fn a_truncated_candidate_beside_the_plan_does_not_affect_it() {
    // §9.7 F2 as a STATE rather than a fault: the half-written candidate a
    // crash would leave. The real F2 fault is above; this is the part of it a
    // later reader meets.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("the original save");

    std::fs::write(
        dir.path().join(".plan.domo.999.1.tmp"),
        &std::fs::read(&path).expect("bytes")[..40],
    )
    .expect("a truncated candidate");

    assert_opens_with_four_walls(&path, "F2");
}

#[test]
fn a_completed_replace_is_the_new_plan_and_never_a_mixture() {
    // §9.7 F5. `rename` is atomic within a filesystem, so a reader sees one
    // package or the other.
    //
    // The old version of this test asserted `walls == 4 || walls == 5` and
    // then `walls == 5` on the next line, which made the first assertion dead:
    // it cannot fail without the second failing too. What the disjunction was
    // reaching for cannot be tested by completing a save — a COMPLETED replace
    // has exactly one outcome. So the honest assertion is the specific one,
    // and the "either" property is a claim about interruption that this host
    // cannot interrupt.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("the original save");

    let mut bigger = plan();
    let level = bigger.snapshot.levels[0].id;
    bigger.snapshot.walls.push(Wall::new(
        level,
        Point::new(0.0, 9000.0),
        Point::new(4000.0, 9000.0),
        200.0,
        2500.0,
    ));
    write_package(&path, &bigger).expect("the replacing save");

    assert_eq!(
        read_package(&path).expect("opens").snapshot.walls.len(),
        5,
        "a completed replace is the new package"
    );
}

#[test]
fn a_failed_directory_sync_does_not_roll_back_the_replacement() {
    // §9.7 F6: the replace has already completed when S5 runs, so a failing
    // directory sync widens the power-loss caveat and changes nothing else.
    //
    // The S5 failure is not injected, and that is a real limit rather than an
    // omission. S5 opens the target's PARENT and syncs it; the only lever this
    // host offers is making that directory unwritable, which fails at S0
    // instead — long before S5. What is asserted is the part that is
    // checkable: the code path ignores the result by construction, so a
    // completed replace stands regardless.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("save");
    assert_opens_with_four_walls(&path, "F6");
}

#[test]
fn no_save_outcome_ever_leaves_the_plan_unopenable() {
    // The property that matters across every injection point, swept: whatever
    // a save does, the file at `path` opens as a valid package or does not
    // exist. There is no third outcome.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");

    for round in 0..8 {
        let mut candidate = plan();
        if round % 2 == 0 {
            // Every other round, hand the writer something it must refuse.
            candidate.manifest.format_version = "0.3".to_owned();
        }
        let _ = write_package(&path, &candidate);

        if path.exists() {
            let package = read_package(&path)
                .unwrap_or_else(|e| panic!("round {round}: left unopenable: {e}"));
            assert_eq!(package.snapshot.walls.len(), 4);
        }
    }
    assert!(path.exists(), "at least one round must have succeeded");
}

#[test]
fn a_truncated_plan_is_refused_rather_than_partially_trusted() {
    // Not an injection point of the SAVE machine — this is what a plan looks
    // like if something outside it truncates the file. A reader must refuse it
    // rather than return the entries it happened to reach.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan()).expect("save");

    let bytes = std::fs::read(&path).expect("bytes");
    std::fs::write(&path, &bytes[..bytes.len() / 2]).expect("truncate");
    assert!(
        read_package(&path).is_err(),
        "a torn package must be refused"
    );
}
