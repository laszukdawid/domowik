//! `domowik-jsoncheck <markdown-root>...`
//!
//! Exits 1 when a fence fails to parse OR when no fence was found at all. The
//! second half matters: a checker that walks the wrong directory and reports
//! success is worse than one that fails, because nothing downstream can tell
//! "nothing to check" from "everything checked out".

use std::path::PathBuf;
use std::process::ExitCode;

use domowik_devtools::jsoncheck;

fn main() -> ExitCode {
    let roots: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: domowik-jsoncheck <markdown-root>...");
        return ExitCode::from(2);
    }

    let mut blocks = 0usize;
    let mut failed = false;
    for root in &roots {
        match jsoncheck::check_tree(root) {
            Ok(count) => blocks += count,
            Err(err) => {
                eprintln!("domowik-jsoncheck: {err}");
                failed = true;
            }
        }
    }

    println!("checked {blocks} json fence(s)");
    println!(
        "scope: syntax validation only; parse success implies nothing about schema conformance \
         (issues 0028/0029 own schema/conformance checks)"
    );

    if failed || blocks == 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
