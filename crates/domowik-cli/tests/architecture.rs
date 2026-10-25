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
fn invalid_stair_exports_keep_valid_stairs_and_report_the_same_omissions() {
    use domowik_format::*;
    use uuid::Uuid;
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("invalid-stair.domo");
    let mut d = Document::with_ground_floor();
    let lower = d.levels[0].id;
    let mut upper = Level::new("Upper");
    upper.elevation_mm = 2600.0;
    let high = upper.id;
    d.levels.push(upper);
    let connection = LevelConnection {
        id: Uuid::now_v7(),
        name: "Synthetic connection".into(),
        from: ConnectionEndpoint {
            level_id: lower,
            position_mm: [0.0, 0.0],
            offset_mm: 0.0,
        },
        to: ConnectionEndpoint {
            level_id: high,
            position_mm: [3780.0, 0.0],
            offset_mm: 0.0,
        },
        unknown: Default::default(),
    };
    let valid = Staircase {
        id: Uuid::now_v7(),
        name: "Valid stair".into(),
        connection_id: connection.id,
        width_mm: 924.0,
        risers: 15,
        guard_height_mm: 1000.0,
        opening_ids: vec![],
        unknown: Default::default(),
    };
    let mut invalid = valid.clone();
    invalid.id = Uuid::now_v7();
    invalid.risers = 1;
    d.level_connections.push(connection);
    d.staircases = vec![invalid.clone(), valid.clone()];
    let (svg, _, diagnostics) =
        domowik_cli::svg::render_level_diagnostics(&d, lower, &Default::default());
    assert!(svg.contains(&format!("data-entity=\"{}\"", valid.id)));
    assert!(!svg.contains(&format!("data-entity=\"{}\"", invalid.id)));
    let png = domowik_cli::png::render_level(&d, lower, 0.1).unwrap();
    assert_eq!(png.construction_diagnostics, diagnostics);
    assert!(png.png.len() > 100);
    assert_eq!(diagnostics.len(), 1);
    domowik_container::write_package(&path, &domowik_container::Package::new(d)).unwrap();
    let before = std::fs::read(&path).unwrap();
    for kind in ["svg", "png"] {
        let out = temp.path().join(format!("drawing.{kind}"));
        let result = ok(
            &path,
            &format!("export-{kind}"),
            None,
            &[out.to_str().unwrap()],
        );
        assert!(result.contains(&diagnostics[0]));
        assert!(std::fs::metadata(out).unwrap().len() > 100);
    }
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "rendering must not repair the source"
    );
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
