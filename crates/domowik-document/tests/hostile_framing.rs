//! The operation log against the hostile framing corpus (issue 0028).
//!
//! FORMAT-SPEC §4.3 row 4 requires a reader to reject framing anomalies BEFORE
//! replay begins. These fixtures encode that requirement; the corpus was
//! written for the frozen Go reader, so running the Rust log against it tests
//! the format's threat model rather than one implementation's habits.
//!
//! # What "rejected" means here
//!
//! A fixture is handled correctly when the log does not accept it as a
//! well-formed retained log. That is satisfied three ways, and all three
//! count, because a reader that refuses earlier is not a worse reader:
//!
//! 1. the bytes do not parse as a batch at all (`torn-batch-document`);
//! 2. `deliver` returns an error;
//! 3. `deliver` HOLDS it — a legitimate outcome for a batch that overtook its
//!    predecessor — and `validate_retained` then refuses the log as final.
//!
//! Counting a parse failure as a pass is the one that needs justifying: it
//! could hide a fixture that fails to parse for an unrelated reason. The
//! fixture shapes are therefore classified from the JSON itself, so a document
//! that was MEANT to be a batch — one carrying `batchId` — is never dismissed
//! as "not a batch".

use domowik_document::log::OperationLog;
use domowik_document::operation::Batch;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/corpus/hostile/framing")
        .canonicalize()
        .expect("the hostile framing corpus is tracked in this repository")
}

/// Fixtures this crate is not the right reader for, with the reason.
///
/// Listed rather than silently skipped: a corpus entry excluded without a
/// stated reason is indistinguishable from one that was forgotten.
const NOT_THE_LOGS_JOB: &[(&str, &str)] = &[(
    "name-content-binding-mismatch.json",
    "its defect is the ENTRY NAME disagreeing with the content, a §7.6 \
     container check; the log never sees an entry name, and \
     domowik-container covers it",
)];

/// Delivers every batch a fixture carries and reports whether the log ever
/// accepted the whole thing as a well-formed retained log.
fn log_accepts(batches: Vec<Batch>) -> bool {
    let mut log = OperationLog::new();
    for batch in batches {
        if log.deliver(batch).is_err() {
            return false;
        }
    }
    log.validate_retained().is_ok()
}

#[test]
fn every_batch_shaped_framing_fixture_is_refused() {
    let mut checked: Vec<String> = Vec::new();
    let mut accepted: Vec<String> = Vec::new();

    let mut files: Vec<PathBuf> = std::fs::read_dir(corpus())
        .expect("readable")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();

    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if NOT_THE_LOGS_JOB.iter().any(|(f, _)| *f == name) {
            continue;
        }
        let bytes = std::fs::read(&path).expect("readable fixture");

        // Shape 1: not even JSON. A torn document is refused by the parser,
        // which is the earliest correct place.
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            checked.push(name);
            continue;
        };

        // Shape 2: a pair, `{"a": batch, "b": batch}`, malformed only in
        // combination.
        if let (Some(a), Some(b)) = (value.get("a"), value.get("b")) {
            let parsed: Vec<Batch> = [a, b]
                .iter()
                .filter_map(|v| serde_json::from_value::<Batch>((*v).clone()).ok())
                .collect();
            checked.push(name.clone());
            if parsed.len() == 2 && log_accepts(parsed) {
                accepted.push(format!("{name}: the pair was accepted"));
            }
            continue;
        }

        // Shape 3: a single batch document.
        if value.get("batchId").is_some() {
            checked.push(name.clone());
            match serde_json::from_value::<Batch>(value) {
                Err(_) => continue,
                Ok(batch) => {
                    if log_accepts(vec![batch]) {
                        accepted.push(format!("{name}: accepted as a well-formed log"));
                    }
                }
            }
            continue;
        }

        // Shape 4: a snapshot document. Its frontier still has to survive the
        // log's baseline seed, which is where a zero member must be refused.
        if let Some(frontier) = value.get("frontier").and_then(|f| f.as_object()) {
            let seed: std::collections::BTreeMap<uuid::Uuid, u64> = frontier
                .iter()
                .filter_map(|(k, v)| Some((k.parse().ok()?, v.as_u64()?)))
                .collect();
            if seed.is_empty() {
                continue;
            }
            checked.push(name.clone());
            if OperationLog::with_baseline_frontier(&seed).is_ok() {
                accepted.push(format!("{name}: its frontier seeded a log"));
            }
        }
    }

    assert!(
        checked.len() >= 7,
        "only {} fixture(s) were exercised; the corpus may have moved: {checked:?}",
        checked.len()
    );
    assert!(
        accepted.is_empty(),
        "{} framing fixture(s) were NOT refused:\n  {}",
        accepted.len(),
        accepted.join("\n  ")
    );
}
