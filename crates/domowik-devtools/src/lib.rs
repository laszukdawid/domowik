//! Repository-hygiene checks used by `scripts/verify.sh`.
//!
//! Independent tools, kept in one crate because none is large enough to
//! justify its own and all exist for the same reason: they check the
//! repository's own files rather than anything Domowik ships.
//!
//! - [`jsoncheck`] syntax-validates fenced JSON blocks in Markdown.
//! - [`corpusgen`] builds and gates the format conformance corpus.

pub mod corpusgen;
pub mod jsoncheck;
