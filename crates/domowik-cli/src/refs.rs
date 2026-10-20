//! Stable references to walls and openings (issue 0103).
//!
//! The implementation lives in `domowik-commands` so the hosted service
//! resolves references exactly as the CLI does. These wrappers name the plan
//! by its path in messages and return the CLI's error type.

pub use domowik_commands::refs::*;

use crate::CliError;
use domowik_format::{Document, Opening, Wall};
use std::path::Path;

/// Resolves a wall argument; see [`domowik_commands::refs::resolve_wall`].
pub fn resolve_wall<'a>(doc: &'a Document, text: &str, path: &Path) -> Result<&'a Wall, CliError> {
    Ok(domowik_commands::refs::resolve_wall(
        doc,
        text,
        &path.display().to_string(),
    )?)
}

/// Resolves an opening argument; see
/// [`domowik_commands::refs::resolve_opening`].
pub fn resolve_opening<'a>(
    doc: &'a Document,
    text: &str,
    path: &Path,
) -> Result<&'a Opening, CliError> {
    Ok(domowik_commands::refs::resolve_opening(
        doc,
        text,
        &path.display().to_string(),
    )?)
}
