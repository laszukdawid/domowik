//! Repository-hygiene checks used by `scripts/verify.sh`.
//!
//! Independent tools, kept in one crate because none is large enough to
//! justify its own and all exist for the same reason: they check the
//! repository's own files rather than anything Domowik ships.
//!
//! - [`jsoncheck`] syntax-validates fenced ```json blocks in committed
//!   Markdown (issue 0004).
//! - [`tracker`] checks issue files against the tracker index (ED-0078).
//! - [`corpusgen`] builds and gates the issue-0028 conformance corpus.
//!
//! Ported from Go under issue 0062, behaviour-for-behaviour including exit
//! codes and message text, so a failing verifier run reads the same as before.

pub mod corpusgen;
pub mod jsoncheck;
pub mod tracker;
