//! `domowik check`: drafting sanity, separate from `validate` (issue 0103).
//!
//! The checks live in `domowik-commands` so the hosted service runs the same
//! ones; this module is the command-line front end.

pub use domowik_commands::check::{render, Candidate, Finding, Report, Severity};

use crate::{CliError, Result};
use domowik_format::Document;
use std::path::Path;

/// Runs every check on every level, or on the named one.
pub fn run(
    doc: &Document,
    session: Option<&domowik_document::session::Session>,
    path: &Path,
    level_name: Option<&str>,
) -> Result<Report> {
    Ok(domowik_commands::check::run(
        doc,
        session,
        &path.display().to_string(),
        level_name,
    )?)
}

/// The `check` command. Fails with exit 1 under `strict` when any warning
/// was found; the report is printed either way.
pub fn cmd_check(path: &Path, level: Option<&str>, json: bool, strict: bool) -> Result<String> {
    // Opened as `set-wall` opens it, so a suggestion is trial-committed
    // against the same session it would really be committed to.
    let mut plan = domowik_container::Plan::open(path)?;
    let doc = plan.document().clone();
    let session = plan.session().ok().map(|s| &*s);
    let report = run(&doc, session, path, level)?;
    let text = if json {
        serde_json::to_string_pretty(&report)
            .map_err(|e| CliError::Failure(format!("could not encode the report: {e}")))?
    } else {
        render(&report)
    };
    if strict && report.warnings > 0 {
        // Report, not Failure: the findings ARE the result, printed to
        // stdout, and the exit status still says the plan is not clean.
        return Err(CliError::Report(text));
    }
    Ok(text)
}
