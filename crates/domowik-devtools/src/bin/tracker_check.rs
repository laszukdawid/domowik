//! `domowik-tracker-check` (no arguments), run from the repository root.
//!
//! Exit codes: 0 consistent, 1 inconsistency (diagnostic on stderr), 2 usage.

use std::path::Path;
use std::process::ExitCode;

use domowik_devtools::tracker;

fn main() -> ExitCode {
    if std::env::args_os().len() > 1 {
        eprintln!("domowik-tracker-check: usage: domowik-tracker-check (no arguments)");
        return ExitCode::from(2);
    }

    let issues = Path::new("issues");
    if !issues.is_dir() {
        eprintln!("domowik-tracker-check: issues/ directory not found; run from repository root");
        return ExitCode::from(2);
    }

    match tracker::check_tracker(issues) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("domowik-tracker-check: {err}");
            ExitCode::FAILURE
        }
    }
}
