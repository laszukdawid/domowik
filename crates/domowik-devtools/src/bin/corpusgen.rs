//! `domowik-corpusgen [--check|--update] [<corpus-root>]`
//!
//! Regenerates the committed issue-0028 `.domo` artifacts from the
//! deterministic generator. `--check` (the default) compares and exits 1 on any
//! difference; `--update` rewrites them, which is the only sanctioned way to
//! change a generated fixture.
//!
//! `cargo test -p domowik-devtools` runs the same comparison as a gate, so this
//! binary exists for the regeneration half rather than to be wired into
//! `verify.sh` twice.

use std::path::PathBuf;
use std::process::ExitCode;

use domowik_devtools::corpusgen::all_artifacts;

fn main() -> ExitCode {
    let mut update = false;
    let mut root: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--update" => update = true,
            "--check" => update = false,
            other if other.starts_with('-') => {
                eprintln!("usage: domowik-corpusgen [--check|--update] [<corpus-root>]");
                return ExitCode::from(2);
            }
            other => root = Some(PathBuf::from(other)),
        }
    }
    let root = root.unwrap_or_else(|| PathBuf::from("testdata/corpus"));

    let artifacts = all_artifacts();
    let mut differing = 0usize;
    for a in &artifacts {
        let path = root.join(&a.rel_path);
        if update {
            if let Some(parent) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    eprintln!("domowik-corpusgen: {}: {e}", parent.display());
                    return ExitCode::FAILURE;
                }
            }
            if let Err(e) = std::fs::write(&path, &a.bytes) {
                eprintln!("domowik-corpusgen: {}: {e}", path.display());
                return ExitCode::FAILURE;
            }
            continue;
        }
        match std::fs::read(&path) {
            Err(e) => {
                eprintln!(
                    "{} missing; run domowik-corpusgen --update: {e}",
                    a.rel_path
                );
                differing += 1;
            }
            Ok(committed) if committed != a.bytes => {
                eprintln!("{} does not match deterministic regeneration", a.rel_path);
                differing += 1;
            }
            Ok(_) => {}
        }
    }

    if update {
        println!("rewrote {} corpus artifact(s)", artifacts.len());
        return ExitCode::SUCCESS;
    }
    if differing > 0 {
        eprintln!("{differing} of {} artifact(s) differ", artifacts.len());
        return ExitCode::FAILURE;
    }
    println!("checked {} corpus artifact(s)", artifacts.len());
    ExitCode::SUCCESS
}
