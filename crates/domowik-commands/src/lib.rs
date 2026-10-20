//! Semantic plan-editing commands, shared by the CLI and other front ends,
//! such as the separate `domowik-serve` hosted service.
//!
//! A command takes an open [`domowik_container::Plan`], resolves its
//! references (issue 0103: UUID, short-ID suffix, point), builds the edit
//! through `domowik-edit` and commits it as one batch. It never reads or
//! writes a file and never formats terminal output: the CLI saves and words
//! the result, the service journals the batch and sends the result as JSON.
//! One implementation therefore decides splits, joins, placement and
//! reference resolution for both.
//!
//! This crate must stay free of CLI, HTTP, SQL and rendering dependencies.

#![forbid(unsafe_code)]

pub mod check;
pub mod edit;
pub mod error;
pub mod levels;
pub mod refs;
pub mod report;

pub use error::{Candidate, CommandError, ErrorKind, Result};
