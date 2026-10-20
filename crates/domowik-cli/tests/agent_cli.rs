//! Issue 0103: the CLI as an AI agent drives it — stable references,
//! millimetre placement, `check` and the review export — through the real
//! binary, on the three-room house of the 2026-10-10 trial.

use std::path::Path;
use std::process::{Command, Output};

fn run(path: &Path, command: &str, args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_domowik"));
    // `room` takes its subcommand before the plan path.
    match command.split_once(' ') {
        Some((top, sub)) => cmd.arg(top).arg(sub),
        None => cmd.arg(command),
    };
    cmd.arg(path).args(args).output().expect("CLI starts")
}

fn ok(path: &Path, command: &str, args: &[&str]) -> String {
    let out = run(path, command, args);
    assert!(
        out.status.success(),
        "{command} {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn refused(path: &Path, command: &str, args: &[&str], needle: &str) -> String {
    let before = std::fs::read(path).unwrap();
    let out = run(path, command, args);
    assert!(
        !out.status.success(),
        "{command} {args:?} unexpectedly succeeded"
    );
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(err.contains(needle), "stderr: {err}");
    assert_eq!(
        std::fs::read(path).unwrap(),
        before,
        "a refusal rewrote the file"
    );
    err
}

fn report(path: &Path) -> serde_json::Value {
    serde_json::from_str(&ok(path, "info", &["--json"])).unwrap()
}

fn walls(path: &Path) -> Vec<serde_json::Value> {
    report(path)["levels"][0]["walls"]
        .as_array()
        .unwrap()
        .clone()
}

fn openings(path: &Path) -> Vec<serde_json::Value> {
    walls(path)
        .iter()
        .flat_map(|w| w["openings"].as_array().unwrap().clone())
        .collect()
}

/// The trial house: 9 x 7 m outer box (200 mm), a partition at x = 5000 and a
/// hallway wall at y = 2500 (120 mm), both meeting walls mid-span. Built once
/// per test run and copied: each CLI call is slow in a debug build.
fn house(dir: &Path) -> std::path::PathBuf {
    static TEMPLATE: std::sync::OnceLock<(tempfile::TempDir, std::path::PathBuf)> =
        std::sync::OnceLock::new();
    let (_, template) = TEMPLATE.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let path = build_house(dir.path());
        (dir, path)
    });
    let path = dir.join("house.domo");
    std::fs::copy(template, &path).unwrap();
    path
}

fn build_house(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("house.domo");
    ok(&path, "new", &[]);
    for w in [
        ["0", "0", "9000", "0"],
        ["9000", "0", "9000", "7000"],
        ["9000", "7000", "0", "7000"],
        ["0", "7000", "0", "0"],
    ] {
        ok(
            &path,
            "add-wall",
            &[w[0], w[1], w[2], w[3], "--thickness", "200"],
        );
    }
    ok(
        &path,
        "add-wall",
        &["5000", "0", "5000", "7000", "--thickness", "120"],
    );
    ok(
        &path,
        "add-wall",
        &["5000", "2500", "9000", "2500", "--thickness", "120"],
    );
    path
}

#[test]
fn a_short_id_still_names_its_wall_after_a_split_renumbers_the_plan() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("p.domo");
    ok(&path, "new", &[]);
    ok(
        &path,
        "add-wall",
        &["0", "0", "9000", "0", "--thickness", "200"],
    );
    ok(
        &path,
        "add-wall",
        &["9000", "0", "9000", "7000", "--thickness", "200"],
    );
    let east = walls(&path)[1]["shortId"].as_str().unwrap().to_owned();
    // A partition meeting the south wall mid-span splits it: numbering shifts.
    ok(
        &path,
        "add-wall",
        &["4000", "0", "4000", "3000", "--thickness", "120"],
    );
    let moved = ok(&path, "set-wall", &[&east, "--height", "2700"]);
    assert!(moved.contains(&east), "{moved}");
    let east_now = walls(&path)
        .into_iter()
        .find(|w| w["shortId"] == east.as_str())
        .unwrap();
    assert_eq!(east_now["heightMm"], 2700.0);
    assert_eq!(east_now["startMm"], serde_json::json!([9000.0, 0.0]));
}

#[test]
fn openings_are_placed_by_point_and_by_millimetre_offset() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    // A point picks the right segment after splits and centres the door on it.
    let out = ok(&path, "add-opening", &["7000,0", "door", "--width", "1000"]);
    assert!(out.contains("centred 2000 mm from its start"), "{out}");
    // A point on the face, not the axis, still finds the wall.
    // A window with no --height: the window default fits a 2500 mm wall.
    ok(
        &path,
        "add-opening",
        &["50,3500", "window", "--width", "1500"],
    );
    // An explicit offset on a wall named by short ID.
    let north = walls(&path)
        .into_iter()
        .find(|w| w["startMm"] == serde_json::json!([9000.0, 7000.0]))
        .unwrap();
    let short = north["shortId"].as_str().unwrap().to_owned();
    ok(
        &path,
        "add-opening",
        &[&short, "window", "--offset", "1500", "--width", "800"],
    );

    let all = openings(&path);
    let centre = |x: f64, y: f64| {
        all.iter()
            .any(|o| o["centreMm"] == serde_json::json!([x, y]))
    };
    assert!(
        centre(7000.0, 0.0) && centre(0.0, 3500.0) && centre(7500.0, 7000.0),
        "{all:#?}"
    );
    let window = all
        .iter()
        .find(|o| o["centreMm"] == serde_json::json!([7500.0, 7000.0]))
        .unwrap();
    assert_eq!(window["centreOffsetMm"], 1500.0);

    // set-opening by point, moved by offset.
    ok(&path, "set-opening", &["7000,0", "--offset", "1000"]);
    assert!(openings(&path)
        .iter()
        .any(|o| o["centreMm"] == serde_json::json!([6000.0, 0.0])));
}

#[test]
fn ambiguous_missing_and_misfit_references_are_refused_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    refused(
        &path,
        "add-opening",
        &["5000,2500", "door"],
        "lies on 3 walls",
    );
    refused(&path, "add-opening", &["2000,3000", "door"], "nearest is");
    refused(
        &path,
        "set-wall",
        &["#zzzz", "--height", "1"],
        "expected a wall",
    );
    refused(
        &path,
        "set-wall",
        &["0", "--height", "1"],
        "numbering starts at 1",
    );
    refused(&path, "remove-opening", &["7000,0"], "no door or window");
    // An offset that does not fit is refused by the core like --at.
    refused(
        &path,
        "add-opening",
        &["7000,0", "door", "--offset", "3900"],
        "does not fit",
    );
    let both = run(
        &path,
        "add-opening",
        &["7000,0", "door", "--at", "0.5", "--offset", "100"],
    );
    assert_eq!(both.status.code(), Some(2), "--at and --offset conflict");
}

fn check_json(path: &Path, args: &[&str]) -> (Option<i32>, serde_json::Value) {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    let out = run(path, "check", &all);
    (
        out.status.code(),
        serde_json::from_slice(&out.stdout).unwrap(),
    )
}

fn finding<'a>(report: &'a serde_json::Value, code: &str) -> Option<&'a serde_json::Value> {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["code"] == code)
}

/// Runs a `PLAN`-templated command from a report against `path`.
fn apply(path: &Path, command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().collect();
    assert_eq!(words[0], "domowik");
    let (cmd, rest) = if words[1] == "room" {
        (format!("room {}", words[2]), &words[4..])
    } else {
        (words[1].to_owned(), &words[3..])
    };
    assert_eq!(words[2 + usize::from(words[1] == "room")], "PLAN");
    ok(path, &cmd, rest)
}

#[test]
fn check_reports_the_trial_mistakes_with_verified_suggestions_and_candidates() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    ok(
        &path,
        "room add",
        &["2500", "3500", "--name", "Living room"],
    );
    let added = ok(&path, "room add", &["5000", "3000", "--name", "Bedroom"]);
    assert!(
        added.contains("warning: the seed is not inside a room"),
        "{added}"
    );
    // A partition stopping 130 mm short of the south wall's axis.
    ok(
        &path,
        "add-wall",
        &["1500", "7000", "1500", "130", "--thickness", "120"],
    );

    let (code, report) = check_json(&path, &["--strict"]);
    assert_eq!(code, Some(1), "warnings fail --strict");
    let near = finding(&report, "wall-end-near-miss").expect("near miss");
    let unplaced = finding(&report, "room-name-unplaced").expect("unplaced name");
    assert!(finding(&report, "room-unnamed").is_some());
    // The unplaced name is NOT auto-fixed: it lists rooms to choose from.
    assert!(unplaced.get("suggestion").is_none(), "{unplaced}");
    let candidates = unplaced["candidates"].as_array().unwrap();
    assert!(candidates.len() >= 2, "{candidates:#?}");

    // The near-miss suggestion is verified: applying it joins the walls.
    apply(&path, near["suggestion"].as_str().unwrap());
    // The author knows the name belongs to the NE room (7000, 4750): pick
    // the candidate in that room by its seed, as an agent would from photos.
    let bedroom = candidates
        .iter()
        .map(|c| c["command"].as_str().unwrap())
        .find(|c| {
            let seed = c.rsplit(' ').next().unwrap();
            let (x, y) = seed.split_once(',').unwrap();
            let (x, y): (f64, f64) = (x.parse().unwrap(), y.parse().unwrap());
            x > 5000.0 && y > 2500.0
        })
        .expect("a candidate in the bedroom");
    apply(&path, bedroom);

    let (code, report) = check_json(&path, &["--strict"]);
    assert_eq!(code, Some(0), "{report:#}");
    let records = report_records(&path);
    assert!(records
        .iter()
        .any(|(name, x, y)| name == "Bedroom" && *x > 5000.0 && *y > 2500.0));
}

fn report_records(path: &Path) -> Vec<(String, f64, f64)> {
    report(path)["levels"][0]["roomRecords"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["name"].as_str().unwrap_or_default().to_owned(),
                r["seedMm"][0].as_f64().unwrap(),
                r["seedMm"][1].as_f64().unwrap(),
            )
        })
        .collect()
}

#[test]
fn a_diagonal_near_miss_suggestion_actually_joins_and_a_touching_end_gets_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("diag.domo");
    ok(&path, "new", &[]);
    ok(
        &path,
        "add-wall",
        &["0", "0", "5000", "3000", "--thickness", "200"],
    );
    ok(
        &path,
        "add-wall",
        &["1001", "4000", "1001", "850", "--thickness", "120"],
    );
    let (_, report) = check_json(&path, &[]);
    let near = finding(&report, "wall-end-near-miss").expect("near miss");
    apply(
        &path,
        near["suggestion"].as_str().expect("a verified suggestion"),
    );
    let (_, after) = check_json(&path, &[]);
    assert!(finding(&after, "wall-end-near-miss").is_none(), "{after:#}");
    assert!(finding(&after, "wall-area-overlap").is_none(), "{after:#}");

    // An end detached from a join it still touches: no no-op suggestion.
    let path = dir.path().join("touch.domo");
    ok(&path, "new", &[]);
    ok(
        &path,
        "add-wall",
        &["0", "0", "4000", "0", "--thickness", "200"],
    );
    ok(
        &path,
        "add-wall",
        &["4000", "0", "4000", "3000", "--thickness", "200"],
    );
    ok(&path, "detach", &["2", "--end", "start"]);
    let (_, report) = check_json(&path, &[]);
    assert!(
        finding(&report, "wall-end-near-miss").is_none(),
        "{report:#}"
    );
    let touching = finding(&report, "wall-end-touching-unjoined").expect("touching");
    assert!(touching.get("suggestion").is_none());
}

#[test]
fn level_scoping_reaches_suggestions_and_document_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("two.domo");
    ok(&path, "new", &[]);
    ok(&path, "add-level", &["Top floor", "--elevation", "3000"]);
    for level in ["Level 1", "Top floor"] {
        for w in [
            ["0", "0", "4000", "0"],
            ["4000", "0", "4000", "3000"],
            ["4000", "3000", "0", "3000"],
            ["0", "3000", "0", "0"],
        ] {
            ok(
                &path,
                "add-wall",
                &[w[0], w[1], w[2], w[3], "--level", level],
            );
        }
    }
    ok(&path, "room add", &["2000", "1500", "--name", "Kitchen"]);
    // Overlapping walls on the top floor only.
    ok(
        &path,
        "add-wall",
        &[
            "500",
            "1000",
            "3500",
            "1000",
            "--thickness",
            "200",
            "--level",
            "Top floor",
        ],
    );
    ok(
        &path,
        "add-wall",
        &[
            "500",
            "1100",
            "3500",
            "1100",
            "--thickness",
            "200",
            "--level",
            "Top floor",
        ],
    );

    let (code, ground) = check_json(&path, &["--level", "Level 1", "--strict"]);
    assert_eq!(
        code,
        Some(0),
        "the other floor's overlap must not fail Level 1: {ground:#}"
    );
    let (_, top) = check_json(&path, &["--level", "Top floor"]);
    let overlap = finding(&top, "wall-area-overlap").expect("overlap on the top floor");
    assert_eq!(overlap["level"], "Top floor");
    assert_eq!(overlap["entities"].as_array().unwrap().len(), 2);
    // The unnamed-room suggestion names its level, quoted, and lands there.
    let unnamed = finding(&top, "room-unnamed").expect("unnamed top-floor room");
    let command = unnamed["suggestion"].as_str().unwrap();
    assert!(command.ends_with("--level 'Top floor'"), "{command}");
    // `--level 'Top floor'` is two shell words; run it through the shell.
    let line = command
        .replace("PLAN", path.to_str().unwrap())
        .replace("NAME", "Study")
        .replacen("domowik", env!("CARGO_BIN_EXE_domowik"), 1);
    let out = Command::new("sh").arg("-c").arg(&line).output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let levels = report(&path)["levels"].clone();
    let top_level = levels
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["name"] == "Top floor")
        .unwrap();
    assert!(top_level["roomRecords"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["name"] == "Study"));
}

#[test]
fn an_all_digit_room_short_id_names_its_own_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    ok(
        &path,
        "room add",
        &["2500", "3500", "--name", "Living room"],
    );
    // A record whose short ID is all digits with leading zeros, and whose
    // UUID sorts after the first record so it is number 2.
    ok(
        &path,
        "room add",
        &[
            "7000",
            "4750",
            "--name",
            "Bedroom",
            "--id",
            "f1a12924-5f8d-7f83-bca9-c910c6000001",
        ],
    );
    ok(&path, "room set", &["000001", "--name", "Main bedroom"]);
    let names: Vec<String> = report_records(&path).into_iter().map(|r| r.0).collect();
    assert!(names.contains(&"Living room".to_owned()), "{names:?}");
    assert!(names.contains(&"Main bedroom".to_owned()), "{names:?}");
}

#[test]
fn door_orientation_is_settable_and_reported() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    ok(
        &path,
        "add-opening",
        &[
            "7000,0",
            "door",
            "--hinge",
            "near-end",
            "--opens-toward",
            "left",
        ],
    );
    ok(
        &path,
        "set-opening",
        &["7000,0", "--opens-toward", "right", "--swing", "100"],
    );
    let door = openings(&path).into_iter().next().unwrap();
    assert_eq!(door["hinge"], "nearEnd");
    assert_eq!(door["opensToward"], "Right");
    assert_eq!(door["swingDeg"], 100.0);
    assert_eq!(door["mechanism"], "doorHinged");
    let wrong = run(
        &path,
        "add-opening",
        &["3000,0", "door", "--mechanism", "lift"],
    );
    assert_eq!(wrong.status.code(), Some(2));
}

#[test]
fn review_export_differs_from_plain_and_plain_is_unchanged_by_the_flag() {
    let dir = tempfile::tempdir().unwrap();
    let path = house(dir.path());
    let (plain, plain2, review) = (
        dir.path().join("plain.png"),
        dir.path().join("plain2.png"),
        dir.path().join("review.png"),
    );
    ok(&path, "export-png", &[plain.to_str().unwrap()]);
    ok(&path, "export-png", &[plain2.to_str().unwrap()]);
    ok(&path, "export-png", &[review.to_str().unwrap(), "--review"]);
    assert_eq!(
        std::fs::read(&plain).unwrap(),
        std::fs::read(&plain2).unwrap()
    );
    assert_ne!(
        std::fs::read(&plain).unwrap(),
        std::fs::read(&review).unwrap()
    );
}

#[test]
fn a_repair_the_plan_would_refuse_is_not_suggested() {
    // Review N1: extending the wall would leave its door poking past the
    // new end, so set-wall would refuse; check must not offer it.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("door.domo");
    ok(&path, "new", &[]);
    ok(
        &path,
        "add-wall",
        &["0", "0", "4000", "0", "--thickness", "200"],
    );
    ok(
        &path,
        "add-opening",
        &["1", "door", "--offset", "3600", "--width", "790"],
    );
    ok(&path, "add-wall", &["3800", "100", "3800", "2000"]);
    let (_, report) = check_json(&path, &[]);
    for f in report["findings"].as_array().unwrap() {
        if let Some(command) = f.get("suggestion").and_then(|s| s.as_str()) {
            if command.starts_with("domowik set-wall") {
                apply(&path, command); // panics if the CLI refuses it
            }
        }
    }
}

#[test]
fn a_submillimetre_gap_is_a_near_miss_with_a_working_suggestion() {
    // Review N2: a 0.25 mm gap is not "already touching".
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("gap.domo");
    ok(&path, "new", &[]);
    ok(&path, "add-wall", &["0", "0", "4000", "0"]);
    ok(&path, "add-wall", &["4000", "0.25", "4000", "3000"]);
    ok(&path, "add-wall", &["4000", "3000", "0", "3000"]);
    ok(&path, "add-wall", &["0", "3000", "0", "0"]);
    let (code, report) = check_json(&path, &["--strict"]);
    assert_eq!(code, Some(1));
    assert!(
        finding(&report, "wall-end-touching-unjoined").is_none(),
        "{report:#}"
    );
    let near = finding(&report, "wall-end-near-miss").expect("near miss");
    apply(&path, near["suggestion"].as_str().expect("a suggestion"));
    let (code, report) = check_json(&path, &["--strict"]);
    assert_eq!(code, Some(0), "{report:#}");
}

/// `room set` reports the first of several errors in the order it always
/// did: the room, then the level, then conflicting flags.
#[test]
fn room_set_reports_a_missing_level_before_a_flag_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("p.domo");
    assert!(run(&plan, "new", &[]).status.success());
    ok(&plan, "room add", &["500", "500", "--name", "Hall"]);
    let out = run(
        &plan,
        "room set",
        &["1", "--level", "Attic", "--name", "x", "--clear-name"],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("no level named \"Attic\""));
}

/// Setting a wall or opening to what it already is writes nothing, as
/// `set-level` and `room set` already did.
#[test]
fn unchanged_set_wall_and_set_opening_write_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let plan = dir.path().join("p.domo");
    ok(&plan, "new", &[]);
    ok(
        &plan,
        "add-wall",
        &["0", "0", "4000", "0", "--thickness", "200"],
    );
    ok(&plan, "add-opening", &["2000,0", "door"]);
    let before = std::fs::read(&plan).unwrap();
    let wall = ok(
        &plan,
        "set-wall",
        &["1000,0", "--thickness", "200", "--start", "0,0"],
    );
    assert!(wall.contains("is unchanged"), "{wall}");
    let opening = ok(&plan, "set-opening", &["2000,0", "--width", "900"]);
    assert!(opening.contains("is unchanged"), "{opening}");
    assert_eq!(std::fs::read(&plan).unwrap(), before, "nothing was written");
}
