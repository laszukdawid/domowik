//! Real CLI-process contact refusals, persistence and atomic edit history.

use std::path::Path;
use std::process::{Command, Output};

fn run(path: &Path, command: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_domowik"))
        .arg(command)
        .arg(path)
        .args(args)
        .output()
        .expect("CLI starts")
}

fn accepted(path: &Path, command: &str, args: &[&str]) -> Output {
    let result = run(path, command, args);
    assert!(
        result.status.success(),
        "{command} {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result
}

fn refused(path: &Path, command: &str, args: &[&str], diagnostic: &str) {
    let before = std::fs::read(path).unwrap();
    let result = run(path, command, args);
    assert!(!result.status.success(), "{command} unexpectedly succeeded");
    assert!(
        String::from_utf8_lossy(&result.stderr).contains(diagnostic),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stdout.is_empty(),
        "rejected command printed success output"
    );
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "refusal rewrote file bytes"
    );
}

fn wall_count(path: &Path) -> usize {
    let output = accepted(path, "info", &["--json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    report["levels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|level| level["walls"].as_array().unwrap().len())
        .sum()
}

#[test]
fn creation_refuses_partial_and_reversed_overlap_but_accepts_touch_and_other_levels() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("contact.domo");
    accepted(&path, "new", &[]);
    accepted(
        &path,
        "add-wall",
        &["0", "0", "4000", "0", "--thickness", "200"],
    );
    for (args, diagnostic) in [
        (["2000", "0", "6000", "0"], "collinear-overlap"),
        (["6000", "0", "2000", "0"], "collinear-overlap"),
        (["4000", "0", "0", "0"], "wall-coincidence"),
    ] {
        refused(&path, "add-wall", &args, diagnostic);
        assert_eq!(wall_count(&path), 1);
    }
    accepted(&path, "add-wall", &["4000", "0", "8000", "0"]);
    assert_eq!(wall_count(&path), 2);
    accepted(&path, "add-level", &["Upper"]);
    accepted(
        &path,
        "add-wall",
        &["0", "0", "4000", "0", "--level", "Upper"],
    );
    assert_eq!(wall_count(&path), 3);
}

#[test]
fn numeric_move_refuses_overlap_then_crossing_splits_and_one_undo_restores_both_walls() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("edit.domo");
    accepted(&path, "new", &[]);
    accepted(&path, "add-wall", &["0", "0", "4000", "0"]);
    accepted(&path, "add-wall", &["2000", "1000", "2000", "3000"]);
    refused(
        &path,
        "set-wall",
        &["2", "--start", "0,0", "--end", "4000,0", "--height", "3100"],
        "wall-coincidence",
    );
    refused(
        &path,
        "set-wall",
        &["2", "--start", "2000,0", "--end", "6000,0"],
        "collinear-overlap",
    );
    let original = domowik_container::Plan::open(&path)
        .unwrap()
        .document()
        .clone();
    accepted(
        &path,
        "set-wall",
        &["2", "--start", "2000,-1000", "--end", "2000,1000"],
    );
    assert_eq!(
        wall_count(&path),
        4,
        "both axes split at the proper crossing"
    );
    accepted(&path, "undo", &[]);
    assert_eq!(
        domowik_container::Plan::open(&path).unwrap().document(),
        &original
    );
}

#[test]
fn raw_apply_and_its_dry_run_share_the_contact_guard() {
    use domowik_document::operation::{Batch, Operation};
    use domowik_format::{Entity, Point, Wall};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("raw.domo");
    accepted(&path, "new", &[]);
    accepted(&path, "add-wall", &["0", "0", "4000", "0"]);
    let plan = domowik_container::Plan::open(&path).unwrap();
    let level = plan.document().levels[0].id;
    let batch = Batch::new(
        uuid::Uuid::now_v7(),
        1,
        plan.session_ref().log().frontier(),
        vec![Operation::create(Entity::Wall(Wall::new(
            level,
            Point::new(2000.0, 0.0),
            Point::new(6000.0, 0.0),
            200.0,
            2500.0,
        )))],
    );
    let incoming = dir.path().join("batch.json");
    std::fs::write(&incoming, serde_json::to_vec(&batch).unwrap()).unwrap();
    let incoming = incoming.to_str().unwrap();
    refused(
        &path,
        "apply",
        &[incoming, "--dry-run"],
        "collinear-overlap",
    );
    refused(&path, "apply", &[incoming], "collinear-overlap");
    assert_eq!(wall_count(&path), 1);
}
