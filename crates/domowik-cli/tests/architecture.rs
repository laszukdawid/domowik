use serde_json::Value;
use std::{path::Path, process::Command};

fn run(path: &Path, top: &str, sub: Option<&str>, args: &[&str]) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_domowik"));
    c.arg(top);
    if let Some(s) = sub {
        c.arg(s);
    }
    c.arg(path).args(args).output().unwrap()
}
fn ok(path: &Path, top: &str, sub: Option<&str>, args: &[&str]) -> String {
    let out = run(path, top, sub, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn info(path: &Path) -> Value {
    serde_json::from_str(&ok(path, "info", None, &["--json"])).unwrap()
}

#[test]
fn stair_cli_reopens_exports_undoes_and_refuses_invalid_changes_without_writing() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("synthetic.domo");
    ok(&p, "new", None, &[]);
    ok(&p, "add-level", None, &["Upper", "--elevation", "2600"]);
    assert_eq!(info(&p)["formatVersion"], "0.1");
    ok(
        &p,
        "stair",
        Some("add"),
        &[
            "--name",
            "Synthetic stair",
            "--lower-level",
            "Level 1",
            "--upper-level",
            "Upper",
            "--start",
            "-1000,1000",
            "--end",
            "2780,1000",
            "--width",
            "900",
            "--risers",
            "15",
            "--guard-height",
            "1000",
            "--cut-upper-floor",
        ],
    );
    let report = info(&p);
    assert_eq!(report["formatVersion"], "0.2");
    assert_eq!(report["valid"], true);
    assert_eq!(report["levels"][0]["staircases"][0]["direction"], "UP");
    assert_eq!(report["levels"][1]["staircases"][0]["direction"], "DOWN");
    for (level, direction) in [("Level 1", "UP"), ("Upper", "DOWN")] {
        let svg = temp.path().join(format!("{direction}.svg"));
        ok(
            &p,
            "export-svg",
            None,
            &[svg.to_str().unwrap(), "--level", level],
        );
        let text = std::fs::read_to_string(svg).unwrap();
        assert!(text.contains(&format!("<title>{direction}</title>")));
        assert!(text.contains("data-entity="));
        let png = temp.path().join(format!("{direction}.png"));
        ok(
            &p,
            "export-png",
            None,
            &[png.to_str().unwrap(), "--level", level],
        );
        assert!(std::fs::metadata(png).unwrap().len() > 100);
    }
    let before = std::fs::read(&p).unwrap();
    let out = run(
        &p,
        "stair",
        Some("set"),
        &["Synthetic stair", "--risers", "1"],
    );
    assert!(!out.status.success());
    assert_eq!(std::fs::read(&p).unwrap(), before);
    ok(
        &p,
        "stair",
        Some("set"),
        &["Synthetic stair", "--width", "1000"],
    );
    let report = info(&p);
    assert_eq!(
        report["levels"][1]["hostedOpenings"][0]["footprint"]["widthMm"],
        1000.0
    );
    let before = std::fs::read(&p).unwrap();
    ok(
        &p,
        "stair",
        Some("set"),
        &["Synthetic stair", "--width", "1000"],
    );
    assert_eq!(
        std::fs::read(&p).unwrap(),
        before,
        "no-op creates no history or rewrite"
    );
    ok(&p, "stair", Some("remove"), &["Synthetic stair"]);
    assert!(info(&p)["levels"][0]["staircases"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        info(&p)["levels"][1]["hostedOpenings"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(&p, "undo", None, &[]);
    assert_eq!(
        info(&p)["levels"][0]["staircases"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    ok(
        &p,
        "stair",
        Some("set"),
        &[
            "Synthetic stair",
            "--name",
            "Garden stairs",
            "--start-elevation",
            "2450",
            "--end-elevation",
            "200",
            "--risers",
            "13",
        ],
    );
    let changed = info(&p);
    let s = &changed["levels"][0]["staircases"][0];
    assert_eq!(s["name"], "Garden stairs");
    assert_eq!(s["startElevationMm"], 2450.0);
    assert_eq!(s["endElevationMm"], 200.0);
    assert_eq!(s["riseMm"], 2250.0);
    assert_eq!(s["risers"], 13);
    ok(&p, "undo", None, &[]);
    assert_eq!(
        info(&p)["levels"][0]["staircases"][0]["name"],
        "Synthetic stair"
    );
}

#[test]
fn independent_bodies_cuts_and_connections_are_authorable_and_references_guard_deletion() {
    let temp = tempfile::tempdir().unwrap();
    let p = temp.path().join("construction.domo");
    ok(&p, "new", None, &[]);
    ok(
        &p,
        "construction",
        Some("add-body"),
        &[
            "--name",
            "Test platform",
            "--level",
            "Level 1",
            "--role",
            "platform",
            "--start",
            "0,0",
            "--end",
            "4000,0",
            "--width",
            "2000",
            "--thickness",
            "200",
            "--bottom",
            "-200",
            "--end-bottom",
            "300",
        ],
    );
    ok(
        &p,
        "construction",
        Some("add-opening"),
        &[
            "--name",
            "Test void",
            "--body",
            "Test platform",
            "--start",
            "1000,0",
            "--end",
            "2000,0",
            "--width",
            "500",
        ],
    );
    assert_eq!(
        info(&p)["levels"][0]["constructionBodies"][0]["bottomOffsetsMm"],
        serde_json::json!([-200.0, 300.0])
    );
    let before = std::fs::read(&p).unwrap();
    assert!(!run(&p, "construction", Some("remove"), &["Test platform"])
        .status
        .success());
    assert_eq!(std::fs::read(&p).unwrap(), before);
    ok(&p, "construction", Some("remove"), &["Test void"]);
    ok(&p, "construction", Some("remove"), &["Test platform"]);
    assert_eq!(
        info(&p)["formatVersion"],
        "0.2",
        "history remains version-gated after deletion"
    );
}
