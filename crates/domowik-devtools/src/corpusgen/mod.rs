//! Development-only tooling for issue 0028: it builds the committed `.domo`
//! corpus artifacts deterministically and validates the machine-readable
//! expectation manifest, the coverage audit and the artifact-safety cap.
//!
//! It contains NO reader, writer, or validator behavior for the Domowik format
//! itself; the ZIP assembly in [`zipwriter`] exists solely to materialize inert
//! fixture bytes.
//!
//! Ported from the Go package `internal/corpusgen` under issue 0062. The port
//! is behaviour-for-behaviour: every generated artifact is byte-identical and
//! every gate asserts what its Go original asserted. The one deliberate
//! difference is recorded on the pinned DEFLATE constants in [`generate`].

pub mod expectations;
pub mod generate;
pub mod zipwriter;

pub use generate::{all_artifacts, ratio_twin_payload, Artifact, CORPUS_ARTIFACT_CAP_BYTES};
