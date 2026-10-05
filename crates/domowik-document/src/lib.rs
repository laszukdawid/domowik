//! Document model: the operation log, replay, and the command session.
//!
//! # What this crate is
//!
//! FORMAT-SPEC §4 in Rust: operations and batches ([`operation`]), the log
//! that validates their framing and orders them causally ([`log`]), replay
//! into a projection ([`replay`]), and the local command layer with undo and
//! redo ([`session`]) that BEHAVIOR-SPEC §8 specifies.
//!
//! # What it is NOT
//!
//! **The operation log is not a CRDT** (FORMAT-SPEC §4 preamble, quoted
//! because it is load-bearing): "It provides replay and synchronization
//! metadata only. The v0.1 conflict rule is deterministic per-field
//! last-writer-wins and is LOSSY: it may discard a concurrent value. Nothing
//! here provides convergence." Real-time or multi-user merge is out of v0.1
//! scope and no API here should be described as providing it.
//!
//! # Not yet implemented
//!
//! Snapshots and compaction (§5) — replay is always from the full log, so a
//! long session grows unbounded. That is issue 0027's work and the reason this
//! crate carries no `snapshot` module rather than a stub of one.

#![forbid(unsafe_code)]

pub mod log;
pub mod operation;
pub mod replay;
pub mod session;
pub mod wall_contact;
pub mod warnings;
