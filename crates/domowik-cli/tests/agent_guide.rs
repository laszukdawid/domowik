//! Issue 0103: `docs/AGENT-GUIDE.md`'s worked example is executed exactly as
//! written, so the guide an agent follows cannot drift from the CLI it
//! describes.

use std::path::Path;
use std::process::Command;

const GUIDE: &str = include_str!("../../../docs/AGENT-GUIDE.md");

/// The `sh` block under "## Worked example".
fn example() -> Vec<String> {
    let section = GUIDE
        .split("## Worked example")
        .nth(1)
        .expect("the guide has a worked example");
    let block = section
        .split("```sh")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .expect("the example has a sh block");
    block
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Runs one line through `sh`, with `domowik` resolving to this build.
fn sh(dir: &Path, line: &str) -> std::process::Output {
    let bin = Path::new(env!("CARGO_BIN_EXE_domowik"));
    let path = format!(
        "{}:{}",
        bin.parent().unwrap().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    Command::new("sh")
        .arg("-c")
        .arg(line)
        .current_dir(dir)
        .env("PATH", path)
        .output()
        .expect("sh starts")
}

#[test]
fn the_worked_example_runs_as_written_and_passes_strict_check() {
    let dir = tempfile::tempdir().unwrap();
    let lines = example();
    assert!(
        lines.iter().all(|l| l.starts_with("domowik ")),
        "{lines:#?}"
    );
    for line in &lines {
        let out = sh(dir.path(), line);
        assert!(
            out.status.success(),
            "{line}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    assert!(dir.path().join("flat-review.png").exists());

    // The correction the guide shows, then the numbers it says to verify.
    let correction = GUIDE
        .split("`domowik set-wall flat.domo ")
        .nth(1)
        .and_then(|rest| rest.split('`').next())
        .map(|args| format!("domowik set-wall flat.domo {args}"))
        .expect("the guide shows a correction");
    let out = sh(dir.path(), &correction);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let info = sh(dir.path(), "domowik info flat.domo --json");
    let report: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap();
    let rooms = report["levels"][0]["rooms"].as_array().unwrap();
    let area = |name: &str| {
        rooms
            .iter()
            .find(|r| r["label"] == name)
            .map(|r| r["displayAreaM2"].as_str().unwrap().to_owned())
    };
    // Hallway 4000 x 2000 between centre lines: (4000-60-100) x (2000-100-60).
    assert_eq!(area("Hallway").as_deref(), Some("7.07"));
    assert_eq!(area("Bedroom").as_deref(), Some("18.59"));
    assert_eq!(area("Living room").as_deref(), Some("32.91"));
    let check = sh(dir.path(), "domowik check flat.domo --strict");
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stdout)
    );
}
