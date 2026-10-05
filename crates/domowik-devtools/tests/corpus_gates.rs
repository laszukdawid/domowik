//! The issue-0028 corpus gates.
//!
//! Ported from `internal/corpusgen/corpus_test.go` under issue 0062, gate for
//! gate: deterministic artifact regeneration, manifest-shape conformance, the
//! inert-artifact safety cap, the machine-readable expectation manifest with
//! recomputed digests, the honest-coverage audit and its negative invariants,
//! the ratio-twin sidecar, and the enforcement proofs for the assert contract
//! and the `executionBlocked` marker.
//!
//! One gate changed shape and is documented where it lives: the ratio twin's
//! stored stream is INFLATED and compared to the payload instead of being
//! recompressed and compared to the stream.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use domowik_devtools::corpusgen::expectations::{
    assert_checks, audited_token_set_violations, covered_category_violations,
    covered_row_violations, execution_blocked_violations, is_limit_row, load_coverage_audit,
    load_expectations, ratified_category_tokens, valid_classes, valid_outcomes, valid_recipe_kinds,
    AuditEntry, ExecutionBlocked, Expectation, SIDECAR_REL_PATH,
};
use domowik_devtools::corpusgen::generate::{
    empty_snapshot_deflate, empty_snapshot_payload, ratio_twin_deflate,
};
use domowik_devtools::corpusgen::{all_artifacts, ratio_twin_payload, CORPUS_ARTIFACT_CAP_BYTES};

use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

/// The corpus tree, relative to this crate.
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/corpus")
}

/// Restates the named corpus safety parameter defined by the generator (a
/// corpus-local inertness bound, NOT a specification quantity; reviewer
/// amendment 2). Taken from the generator constant so the gate and the
/// generator cannot drift apart.
const ARTIFACT_CAP_BYTES: u64 = CORPUS_ARTIFACT_CAP_BYTES;

/// Guards against silent manifest truncation; the bound tracks the authored
/// record count, not any specification quantity.
const MINIMUM_EXPECTED_RECORDS: usize = 150;

/// Reports every collected violation at once rather than the first.
fn report(errs: &[String]) {
    assert!(
        errs.is_empty(),
        "{} violation(s):\n  {}",
        errs.len(),
        errs.join("\n  ")
    );
}

fn sha256hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

/// Every file under `dir`, as slash-separated paths relative to `root`.
///
/// Recursion is decided by `file_type()`, which does NOT follow symlinks —
/// matching Go's `filepath.WalkDir`, which walks a symlinked directory as a
/// leaf. `Path::is_dir` follows them, so a symlink planted in the corpus tree
/// could have walked outside it or looped forever.
///
/// A filename that is not valid UTF-8 is a hard error rather than a lossy
/// string: silently mapping it to U+FFFD would let two different files collapse
/// onto one indexed name, and the two-way fixture coverage check rests on those
/// names being exact.
fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("directory entry");
        let file_type = entry.file_type().expect("directory entry type");
        let path = entry.path();
        if file_type.is_dir() {
            walk(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).expect("path under root");
            let rel = rel
                .to_str()
                .unwrap_or_else(|| panic!("{}: corpus paths must be valid UTF-8", rel.display()));
            out.insert(rel.replace('\\', "/"));
        }
    }
}

fn all_corpus_files() -> BTreeSet<String> {
    let root = corpus_root();
    let mut out = BTreeSet::new();
    walk(&root, &root, &mut out);
    out
}

/// The indexable fixture set: everything under the four fixture trees, minus
/// the two prose documents and the shared parameter table.
fn fixture_files() -> BTreeSet<String> {
    all_corpus_files()
        .into_iter()
        .filter(|rel| {
            if rel == "README.md" || rel == "EXPECTATIONS.md" || rel == "parameters.json" {
                return false;
            }
            let top = rel.split('/').next().unwrap_or_default();
            matches!(top, "golden" | "adversarial" | "migration" | "hostile")
        })
        .collect()
}

fn expectations() -> Vec<Expectation> {
    load_expectations(&corpus_root()).expect("EXPECTATIONS.md must load")
}

/// Regenerates every committed generated artifact and asserts byte equality.
///
/// Only `.domo` packages and the ratio sidecar are generator-produced; all
/// other committed fixtures carry a recorded sha256 that
/// [`expectations_manifest`] recomputes on every run (narrowed claim per
/// reviewer amendment HIGH-2).
#[test]
fn generate_corpus_artifacts() {
    let root = corpus_root();
    let mut errs = Vec::new();
    for a in all_artifacts() {
        let path = root.join(&a.rel_path);
        match std::fs::read(&path) {
            Err(e) => errs.push(format!(
                "{} missing; run `cargo run -p domowik-devtools --bin domowik-corpusgen -- --update`: {e}",
                a.rel_path
            )),
            Ok(committed) if committed != a.bytes => errs.push(format!(
                "{} does not match deterministic regeneration",
                a.rel_path
            )),
            Ok(_) => {}
        }
    }
    report(&errs);
}

/// Pins the FS 7.4 manifest field SHAPES in every generated package,
/// independently of whether the reader happens to tolerate a looser one.
///
/// `assetRoot` is a REQUIRED ascending-by-digest array; null is not an array,
/// and the reader's tolerance of null is parser laxity that cannot make a
/// fixture conforming. Thin packages — including the
/// `golden/packages/thin-valid.domo` success fixture and every hostile base
/// derived from it — emitted null until this gate existed, which meant a
/// SUCCESS oracle rested on non-conforming bytes.
#[test]
fn generated_manifests_are_conforming() {
    let mut errs = Vec::new();
    let mut checked = 0usize;
    let mut checked_paths: BTreeSet<String> = BTreeSet::new();
    for a in all_artifacts() {
        if !a.rel_path.ends_with(".domo") {
            continue;
        }
        let Ok(mut zr) = zip::ZipArchive::new(Cursor::new(a.bytes.clone())) else {
            continue; // deliberately malformed container fixtures
        };
        for i in 0..zr.len() {
            let mut raw = Vec::new();
            {
                let Ok(mut f) = zr.by_index(i) else {
                    continue; // deliberately corrupted entry fixtures
                };
                if f.name() != "domowik.json" {
                    continue;
                }
                if f.read_to_end(&mut raw).is_err() {
                    continue;
                }
            }
            let Ok(m) = serde_json::from_slice::<serde_json::Map<String, Value>>(&raw) else {
                continue; // manifests corrupted on purpose
            };
            checked += 1;
            checked_paths.insert(a.rel_path.clone());
            let Some(root) = m.get("assetRoot") else {
                errs.push(format!(
                    "{}: manifest omits the required assetRoot field (FS 7.4)",
                    a.rel_path
                ));
                continue;
            };
            if root.is_null() {
                errs.push(format!(
                    "{}: manifest assetRoot is null, but FS 7.4 requires an ascending-by-digest ARRAY; an empty root serializes as []",
                    a.rel_path
                ));
                continue;
            }
            if !root.is_array() {
                errs.push(format!(
                    "{}: manifest assetRoot is {root}, which is not a JSON array (FS 7.4)",
                    a.rel_path
                ));
            }
            if !m.contains_key("entries") {
                errs.push(format!(
                    "{}: manifest omits the required entries field (FS 7.4)",
                    a.rel_path
                ));
            }
        }
    }
    assert!(
        checked > 0,
        "no generated manifest was inspected; the gate is vacuous"
    );
    // Anti-vacuity beyond a bare count: the two conforming golden packages MUST
    // be readable, so a container-reader change that quietly stopped opening
    // packages cannot leave this gate green on a handful of stragglers.
    for required in [
        "golden/packages/thin-valid.domo",
        "golden/packages/portable-valid.domo",
    ] {
        assert!(
            checked_paths.contains(required),
            "{required} contributed no manifest check; the gate stopped reading conforming packages"
        );
    }
    report(&errs);
}

/// The independent inert-safety gate: no committed corpus file may exceed the
/// bounded-artifact cap.
#[test]
fn corpus_artifact_safety() {
    let root = corpus_root();
    let mut errs = Vec::new();
    for rel in all_corpus_files() {
        let path = root.join(&rel);
        let size = std::fs::metadata(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .len();
        if size > ARTIFACT_CAP_BYTES {
            errs.push(format!(
                "{rel} exceeds {ARTIFACT_CAP_BYTES} bytes ({size}): corpus artifacts must stay inert and bounded"
            ));
        }
    }
    report(&errs);
}

/// Enforces the honest-coverage contract: every ratified category token and
/// every FS 9.3 row appears exactly once; covered entries point at real records
/// carrying that category/row; blocked entries point at an existing
/// `blocked-open-question` gap record.
///
/// The audit's existence with blocked entries means full taxonomy coverage is
/// NOT claimed.
#[test]
fn coverage_audit() {
    let root = corpus_root();
    let audit = load_coverage_audit(&root).expect("coverage audit fence");
    let mut errs: Vec<String> = Vec::new();

    let mut rec_ids: BTreeSet<String> = BTreeSet::new();
    let mut cat_of_rec: BTreeMap<String, String> = BTreeMap::new();
    let mut rec_rows: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut gap_ids: BTreeSet<String> = BTreeSet::new();
    let mut blocked_ids: BTreeSet<String> = BTreeSet::new();
    let mut blocked_state: BTreeMap<String, String> = BTreeMap::new();
    let mut blocked_asserts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut blocked_route: BTreeMap<String, String> = BTreeMap::new();

    for b in expectations() {
        rec_ids.insert(b.id.clone());
        if let Some(cat) = b.category.clone() {
            cat_of_rec.insert(b.id.clone(), cat);
        }
        if let Some(eb) = b.execution_blocked.as_ref() {
            blocked_ids.insert(b.id.clone());
            blocked_state.insert(b.id.clone(), eb.state.clone());
            blocked_asserts.insert(b.id.clone(), eb.assertions.clone());
            blocked_route.insert(b.id.clone(), eb.routed_to.clone());
        }
        rec_rows.insert(b.id.clone(), b.mapped_rows());
        if b.outcome == "blocked-open-question" {
            gap_ids.insert(b.id.clone());
            blocked_ids.insert(b.id.clone());
            blocked_state
                .entry(b.id.clone())
                .or_insert_with(|| "blocked-open-question".to_string());
        }
    }

    let ratified = ratified_category_tokens();
    let mut blocked_cats = 0usize;
    for e in &audit.category_audit {
        if !ratified.contains(e.token.as_str()) {
            errs.push(format!("audit token {:?} not in ratified set", e.token));
        }
        match e.status.as_str() {
            "covered" => {
                for msg in covered_category_violations(e, &rec_ids, &cat_of_rec, &blocked_ids) {
                    errs.push(format!("category {} {msg}", e.token));
                }
            }
            "blocked-upstream-spec" | "no-reject-trigger-by-design" => {
                blocked_cats += 1;
                if e.covered_by.is_empty() && e.note.is_empty() && e.gap_record.is_empty() {
                    errs.push(format!(
                        "blocked category {} needs a gapRecord or explanatory note",
                        e.token
                    ));
                }
                if !e.gap_record.is_empty() && !gap_ids.contains(&e.gap_record) {
                    errs.push(format!(
                        "category {} gapRecord {} missing or not blocked-open-question",
                        e.token, e.gap_record
                    ));
                }
            }
            other => errs.push(format!("category {} unknown status {other:?}", e.token)),
        }
    }
    errs.extend(audited_token_set_violations(&audit.category_audit));
    assert!(
        blocked_cats > 0,
        "audit claims complete taxonomy coverage; blocked entries must be present while gaps exist"
    );

    let mut seen_rows: BTreeSet<String> = BTreeSet::new();
    for e in &audit.row_audit {
        if !seen_rows.insert(e.row.clone()) {
            errs.push(format!("duplicate row audit entry {}", e.row));
        }
        if !is_limit_row(&e.row) {
            errs.push(format!("row audit entry {:?} invalid", e.row));
        }
        match e.status.as_str() {
            "covered" => {
                for msg in covered_row_violations(e, &rec_ids, &rec_rows, &blocked_ids) {
                    errs.push(format!("row {} {msg}", e.row));
                }
            }
            "blocked-owner-deferred" | "blocked-upstream-spec" => {
                if !gap_ids.contains(&e.gap_record) {
                    errs.push(format!(
                        "row {} gapRecord {} missing or not blocked-open-question",
                        e.row, e.gap_record
                    ));
                }
            }
            other => errs.push(format!("row {} unknown status {other:?}", e.row)),
        }
    }
    for n in 1..=33 {
        if !seen_rows.contains(&format!("L{n}")) {
            errs.push(format!("row L{n} missing from audit"));
        }
    }

    // residualBlockers is the reader-facing statement of what is NOT executed
    // evidence. It must match the blocked record set exactly in both
    // directions, so the manifest cannot claim coverage the harness withholds,
    // nor withhold coverage the manifest does not disclose.
    let mut listed: BTreeSet<String> = BTreeSet::new();
    for rb in &audit.residual_blockers {
        if !rec_ids.contains(&rb.record) {
            errs.push(format!(
                "residualBlockers names unknown record {}",
                rb.record
            ));
            continue;
        }
        if !listed.insert(rb.record.clone()) {
            errs.push(format!("residualBlockers lists {} twice", rb.record));
        }
        if !blocked_ids.contains(&rb.record) {
            errs.push(format!(
                "residualBlockers lists {}, which carries neither executionBlocked nor a blocked-open-question outcome",
                rb.record
            ));
            continue;
        }
        let want_state = blocked_state.get(&rb.record).cloned().unwrap_or_default();
        if rb.state != want_state {
            errs.push(format!(
                "residualBlockers {}: state {:?} disagrees with the record's {want_state:?}",
                rb.record, rb.state
            ));
        }
        let want_asserts = blocked_asserts.get(&rb.record).cloned().unwrap_or_default();
        if rb.blocked_assertions != want_asserts
            && !(rb.blocked_assertions.is_empty() && want_asserts.is_empty())
        {
            errs.push(format!(
                "residualBlockers {}: blockedAssertions {:?} disagree with the record's {want_asserts:?}",
                rb.record, rb.blocked_assertions
            ));
        }
        if let Some(route) = blocked_route.get(&rb.record) {
            if !route.is_empty() && &rb.routed_to != route {
                errs.push(format!(
                    "residualBlockers {}: routedTo {:?} disagrees with the record's {route:?}",
                    rb.record, rb.routed_to
                ));
            }
        }
        if rb.routed_to.is_empty() {
            errs.push(format!(
                "residualBlockers {}: must name the route that owns it",
                rb.record
            ));
        }
    }
    for id in &blocked_ids {
        if !listed.contains(id) {
            errs.push(format!(
                "record {id} is blocked but absent from residualBlockers: a reader cannot tell executed coverage from residual blockers"
            ));
        }
    }
    assert!(
        !audit.residual_blockers.is_empty(),
        "residualBlockers is empty while blocked records exist; full coverage must not be implied"
    );
    report(&errs);
}

/// Enforces the machine-readable contract: vocabulary, uniqueness, clause
/// citations, artifact digest recomputation, recipe shape and bound rows,
/// category tokens, structured asserts, recipe-input validity, and full two-way
/// coverage between `EXPECTATIONS.md` and the committed tree.
#[test]
fn expectations_manifest() {
    let root = corpus_root();
    let recs = expectations();
    assert!(
        recs.len() >= MINIMUM_EXPECTED_RECORDS,
        "suspiciously small manifest: {} records",
        recs.len()
    );
    let classes = valid_classes();
    let outcomes = valid_outcomes();
    let ratified = ratified_category_tokens();
    let kinds = valid_recipe_kinds();

    let mut errs: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut indexed: BTreeSet<String> = BTreeSet::new();

    for r in &recs {
        assert!(!r.id.is_empty(), "record with empty id");
        if !seen.insert(r.id.clone()) {
            errs.push(format!("duplicate id {}", r.id));
        }
        if !classes.contains(r.class.as_str()) {
            errs.push(format!("{}: bad class {:?}", r.id, r.class));
        }
        if !outcomes.contains(r.outcome.as_str()) {
            errs.push(format!("{}: bad outcome {:?}", r.id, r.outcome));
        }
        if r.clauses.is_empty() {
            errs.push(format!("{}: missing clauses", r.id));
        }
        for row in &r.row_refs {
            if !is_limit_row(row) {
                errs.push(format!("{}: rowRefs entry {row:?} invalid", r.id));
            }
        }
        // Reject records in executable scope must name their §9.3/§9.4 class;
        // declarative vectors may leave category null when no ratified class
        // exists (MED-8), but any present category must be a ratified token.
        match r.category.as_deref() {
            Some(cat) => {
                if !ratified.contains(cat) {
                    errs.push(format!(
                        "{}: category {cat:?} is not a ratified §9.3/§9.4 token",
                        r.id
                    ));
                }
            }
            None => {
                if r.outcome == "reject" && r.class != "declarative" {
                    errs.push(format!(
                        "{}: reject record must carry a category token",
                        r.id
                    ));
                }
            }
        }

        for msg in execution_blocked_violations(r) {
            errs.push(format!("{}: {msg}", r.id));
        }

        let has_artifact = !r.artifact.is_empty();
        let has_recipe = r.recipe.is_some();
        if !has_artifact && !has_recipe {
            errs.push(format!("{}: needs an artifact or a recipe", r.id));
        }

        if has_artifact {
            let p = root.join(&r.artifact);
            match std::fs::metadata(&p) {
                Err(_) => errs.push(format!("{}: artifact {} missing", r.id, r.artifact)),
                Ok(info) => {
                    if info.len() > ARTIFACT_CAP_BYTES {
                        errs.push(format!("{}: artifact exceeds cap", r.id));
                    }
                    // HIGH-2: digests are RECOMPUTED from the referenced bytes
                    // on every run, never trusted from a sidecar.
                    if r.sha256.is_empty() {
                        errs.push(format!("{}: artifact record missing sha256", r.id));
                    } else {
                        let body = std::fs::read(&p).expect("artifact bytes");
                        let got = sha256hex(&body);
                        if got != r.sha256 {
                            errs.push(format!(
                                "{}: sha256 mismatch for {}: manifest {} vs actual {got}",
                                r.id, r.artifact, r.sha256
                            ));
                        }
                    }
                }
            }
            if !indexed.insert(r.artifact.clone()) {
                errs.push(format!("artifact {} indexed more than once", r.artifact));
            }
            for msg in assert_checks(r, &root) {
                errs.push(format!("{}: {msg}", r.id));
            }
        }

        if let Some(rc) = r.recipe.as_ref() {
            if !kinds.contains(rc.kind.as_str()) {
                errs.push(format!(
                    "{}: recipe kind {:?} outside whitelist",
                    r.id, rc.kind
                ));
            }
            if rc.materialization != "none" && rc.materialization != "bounded" {
                errs.push(format!(
                    "{}: recipe materialization must be none|bounded",
                    r.id
                ));
            }
            if rc
                .committed_physical_bytes
                .is_some_and(|n| n > ARTIFACT_CAP_BYTES as i64)
            {
                errs.push(format!("{}: recipe commits oversized bytes", r.id));
            }
            for row in &rc.bound_rows {
                if !is_limit_row(row) {
                    errs.push(format!(
                        "{}: boundRows entry {row:?} is not a ratified §9.3 row ID (L1–L32)",
                        r.id
                    ));
                }
            }
            if rc.kind == "limit-probe" {
                if rc.bound_rows.is_empty() {
                    errs.push(format!(
                        "{}: limit-probe recipe needs at least one boundRows entry",
                        r.id
                    ));
                }
                if rc.declared_logical_size.is_empty() {
                    errs.push(format!(
                        "{}: limit-probe recipe needs a declaredLogicalSize expression over §9.3 row names",
                        r.id
                    ));
                }
            }
            // HIGH-3/BLOCKER-3: recipe inputs must exist, carry a sha256 that
            // is RECOMPUTED from the bytes on every run (tamper detection), be
            // covered exclusively by THIS record, and satisfy kind-specific
            // structural checks.
            for input in &rc.inputs {
                let ip = root.join(&input.path);
                match std::fs::read(&ip) {
                    Err(_) => {
                        errs.push(format!("{}: recipe input {} missing", r.id, input.path));
                        continue;
                    }
                    Ok(body) => {
                        let got = sha256hex(&body);
                        if got != input.sha256 {
                            errs.push(format!(
                                "{}: recipe input {} sha256 mismatch (manifest {} vs actual {got})",
                                r.id, input.path, input.sha256
                            ));
                        }
                    }
                }
                if !indexed.insert(input.path.clone()) {
                    errs.push(format!(
                        "input {} covered both by {} and its own record",
                        input.path, r.id
                    ));
                }
            }
            for b in &rc.boundaries {
                if !is_limit_row(&b.row) {
                    errs.push(format!(
                        "{}: boundary row {:?} is not a ratified §9.3 row ID",
                        r.id, b.row
                    ));
                }
                for oc in [&b.at_limit_outcome, &b.one_over_outcome] {
                    if !outcomes.contains(oc.as_str()) && oc != "success-on-read" {
                        errs.push(format!(
                            "{}: boundary outcome {oc:?} outside vocabulary",
                            r.id
                        ));
                    }
                }
                if b.quantity.is_empty() {
                    errs.push(format!("{}: boundary requires a quantity token", r.id));
                }
            }
            if rc.kind == "limit-probe" && rc.boundaries.is_empty() {
                errs.push(format!(
                    "{}: limit-probe requires at least one structured boundary (BLOCKER-3)",
                    r.id
                ));
            }
            if rc.kind == "batch-pair" {
                if rc.inputs.len() != 2 {
                    errs.push(format!("{}: batch-pair requires exactly two inputs", r.id));
                    continue;
                }
                let mut ids: Vec<String> = Vec::new();
                for input in &rc.inputs {
                    let body = std::fs::read(root.join(&input.path)).expect("batch-pair input");
                    match serde_json::from_slice::<Value>(&body) {
                        Err(_) => errs.push(format!(
                            "{}: batch-pair input {} does not parse as a batch document",
                            r.id, input.path
                        )),
                        Ok(doc) => ids.push(
                            doc.get("batchId")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        ),
                    }
                }
                if ids.len() == 2 && (ids[0].is_empty() || ids[0] != ids[1]) {
                    errs.push(format!(
                        "{}: batch-pair documents do not share one embedded batchId ({:?} vs {:?})",
                        r.id, ids[0], ids[1]
                    ));
                }
            }
        }

        if r.outcome == "blocked-open-question" && !r.implementation_deferred {
            errs.push(format!(
                "{}: blocked-open-question must be marked implementationDeferred",
                r.id
            ));
        }
        if r.class == "declarative" && !r.implementation_deferred {
            errs.push(format!(
                "{}: declarative records must be marked implementationDeferred",
                r.id
            ));
        }
    }

    let files = fixture_files();
    for f in &files {
        if !indexed.contains(f) {
            errs.push(format!(
                "fixture {f} present but not indexed in EXPECTATIONS.md"
            ));
        }
    }
    for f in &indexed {
        if !files.contains(f) {
            errs.push(format!(
                "EXPECTATIONS.md indexes {f} but no such fixture exists"
            ));
        }
    }
    report(&errs);
}

/// Recomputes the measured expansion of the genuine stream twin
/// deterministically (MED-9) and ties the committed package to its sidecar
/// (BLOCKER-5).
///
/// **This gate changed MECHANISM under issue 0062, and the guarantee changed
/// with it (ED-0104).** The Go original recompressed the payload and compared
/// the result to the sidecar's byte count, which asserted that the stored
/// stream is REGENERABLE from its payload by a named compressor. No Rust
/// compressor reproduces those bytes, so the stream is a pinned input now and
/// the check runs the other way round: the stored stream is INFLATED and
/// required to equal the payload the package declares, which asserts that it
/// EXPANDS to a fully conforming snapshot document.
///
/// Neither subsumes the other. Regeneration is lost; a semantic expansion check
/// and a pinned digest replace it. The trade is named rather than sold as an
/// upgrade.
#[test]
fn ratio_twin_meta() {
    let root = corpus_root();
    let raw = std::fs::read(root.join(SIDECAR_REL_PATH)).expect("ratio twin sidecar");
    let meta: Value = serde_json::from_slice(&raw).expect("sidecar parses");
    let logical = meta["logicalBytes"].as_i64().expect("logicalBytes");
    let stored = meta["storedBytes"].as_i64().expect("storedBytes");
    let ratio_percent = meta["ratioPercent"].as_i64().expect("ratioPercent");

    let payload = ratio_twin_payload();
    assert_eq!(
        payload.len() as i64,
        logical,
        "logicalBytes drifted: meta {logical} vs payload {}",
        payload.len()
    );
    let stream = ratio_twin_deflate();
    assert_eq!(
        stream.len() as i64,
        stored,
        "storedBytes drifted: meta {stored} vs pinned stream {}",
        stream.len()
    );
    assert_eq!(
        logical * 100 / stored,
        ratio_percent,
        "ratioPercent inconsistent"
    );
    // Below the ratified L15 rejection ratio of 1000:1 (FS §9.3 L15).
    assert!(
        logical < stored * 1000,
        "expansion ratio {logical}:{stored} reached or exceeded L15"
    );
    assert!(
        logical <= ARTIFACT_CAP_BYTES as i64,
        "logical expansion exceeds the inert cap"
    );

    // BLOCKER-5: the committed PACKAGE must correspond to its sidecar — locate
    // the deflate entry inside the twin .domo, check that its stored and
    // uncompressed sizes are exactly the sidecar's numbers, and (new) that the
    // bytes it expands to are exactly the payload.
    let pkg_bytes = std::fs::read(root.join("hostile/container/ratio-under-L15-success-twin.domo"))
        .expect("ratio twin package");
    let mut pkg = zip::ZipArchive::new(Cursor::new(pkg_bytes)).expect("twin package opens");
    let mut found = false;
    for i in 0..pkg.len() {
        let mut expanded = Vec::new();
        {
            let Ok(mut f) = pkg.by_index(i) else { continue };
            if f.compression() != zip::CompressionMethod::Deflated {
                continue;
            }
            if f.size() != logical as u64 || f.compressed_size() != stored as u64 {
                continue;
            }
            f.read_to_end(&mut expanded).expect("deflate entry expands");
        }
        assert_eq!(
            expanded, payload,
            "the twin's deflate entry does not expand to the declared payload"
        );
        found = true;
        break;
    }
    assert!(
        found,
        "no package entry matches the sidecar sizes ({logical} logical / {stored} stored): payload and sidecar diverge"
    );
}

/// Proves the pinned DEFLATE streams are what the generator says they are.
///
/// The generator does not compress (see `generate.rs`), so the two streams it
/// ships are recorded inputs. This is the gate that keeps them honest: each is
/// inflated and required to equal its payload exactly, and each is checked
/// against the sha256 recorded beside it. A stream edited by hand, or swapped
/// for one that expands to something else, fails here rather than shipping.
#[test]
fn pinned_deflate_streams_expand_to_their_payloads() {
    fn inflate(stream: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        flate2::read::DeflateDecoder::new(stream)
            .read_to_end(&mut out)
            .expect("pinned stream is a valid raw DEFLATE stream");
        out
    }

    let twin = ratio_twin_deflate();
    assert_eq!(
        sha256hex(&twin),
        "3ff7c08f44968924d70c564d2341999330c0d6a07d10821250ff9b406ca806ac",
        "the ratio twin's pinned stream no longer matches its recorded digest"
    );
    assert_eq!(
        inflate(&twin),
        ratio_twin_payload(),
        "the ratio twin's pinned stream does not expand to the twin payload"
    );

    let snap = empty_snapshot_deflate();
    assert_eq!(
        sha256hex(&snap),
        "08ee023e9aecf884642b526c1c450fa6c8f29109be0d83ad9df5a61af09539fb",
        "the snapshot's pinned stream no longer matches its recorded digest"
    );
    assert_eq!(
        inflate(&snap),
        empty_snapshot_payload(),
        "the snapshot's pinned stream does not expand to the snapshot document"
    );
}

/// Pins the exact enforcement behavior of the assert contract
/// (BLOCKER-7/8): required-assert omission, false `rawByDesign` claims, sidecar
/// pollution, and outcome contradictions each MUST produce a violation;
/// compliant records produce none.
#[test]
fn validate_assert_invariants() {
    let root = corpus_root();
    let good = || {
        let mut e = Expectation {
            id: "test/ok".to_string(),
            class: "golden".to_string(),
            artifact: "golden/entities/level-minimal.json".to_string(),
            outcome: "success".to_string(),
            ..blank_expectation()
        };
        e.assert = Some(BTreeMap::from([("outcome".to_string(), json!("success"))]));
        e
    };
    let errs = assert_checks(&good(), &root);
    assert!(errs.is_empty(), "compliant record flagged: {errs:?}");

    let mut missing = good();
    missing.assert = None;
    let errs = assert_checks(&missing, &root);
    assert!(
        errs.first()
            .is_some_and(|m| m.contains("required manifest assert object")),
        "BLOCKER-7 unenforced: missing assert passed: {errs:?}"
    );

    let mut false_raw = good();
    false_raw.raw_by_design = true;
    let errs = assert_checks(&false_raw, &root);
    assert!(
        errs.join("; ").contains("rawByDesign"),
        "BLOCKER-8 unenforced: false rawByDesign claim passed: {errs:?}"
    );

    let mut sidecar = good();
    sidecar.artifact = SIDECAR_REL_PATH.to_string();
    let errs = assert_checks(&sidecar, &root);
    assert!(
        errs.first().is_some_and(|m| m.contains("sidecar")),
        "sidecar pollution unenforced: {errs:?}"
    );

    let mut mismatch = good();
    mismatch.assert = Some(BTreeMap::from([("outcome".to_string(), json!("reject"))]));
    let errs = assert_checks(&mismatch, &root);
    assert!(
        errs.first()
            .is_some_and(|m| m.contains("contradicts record outcome")),
        "outcome contradiction unenforced: {errs:?}"
    );

    let mut warn = good();
    warn.outcome = "warning".to_string();
    warn.assert = Some(BTreeMap::from([("outcome".to_string(), json!("success"))]));
    let errs = assert_checks(&warn, &root);
    assert!(
        errs.first().is_some_and(|m| m.contains("contradicts")),
        "warning mismatch unenforced: {errs:?}"
    );
}

/// A fixture whose whole content is `null` must NOT be able to claim
/// `rawByDesign`.
///
/// Go's `json.Unmarshal` accepts a top-level `null` into a map without error,
/// so the Go gate caught this as a false raw claim.
/// `serde_json::from_slice::<Map>` refuses `null`, which would have read as
/// "genuinely not an object" and let the record skip its assert contract
/// entirely. Found by adversarial review of the port; the gate is only as
/// strong as its weakest parse.
#[test]
fn a_null_document_cannot_claim_raw_by_design() {
    let tmp = tempfile::tempdir().expect("temp corpus root");
    let rel = "hostile/json/null-document.json";
    let path = tmp.path().join(rel);
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(&path, b"null").expect("write");

    let mut rec = Expectation {
        id: "hostile/json/null-document".to_string(),
        class: "hostile".to_string(),
        artifact: rel.to_string(),
        outcome: "reject".to_string(),
        raw_by_design: true,
        ..blank_expectation()
    };
    let errs = assert_checks(&rec, tmp.path());
    assert!(
        errs.join("; ").contains("marked rawByDesign but parses"),
        "a null document must be refused a rawByDesign claim: {errs:?}"
    );

    // And the same bytes without the claim must demand an assert block rather
    // than being written off as unparseable.
    rec.raw_by_design = false;
    let errs = assert_checks(&rec, tmp.path());
    assert!(
        errs.join("; ").contains("required manifest assert object"),
        "a null document still owes its assert contract: {errs:?}"
    );
}

/// Proves omission and wrong-claim detection for the audit validators: a
/// ratified token absent from the audit, a wrong-row claim, an unknown-record
/// claim, and an empty `coveredBy` each MUST fail.
#[test]
fn coverage_audit_invariants() {
    let tok = "malformed-container";
    let empty_blocked: BTreeSet<String> = BTreeSet::new();
    let entry = |token: &str, row: &str, covered: &[&str]| AuditEntry {
        token: token.to_string(),
        row: row.to_string(),
        status: "covered".to_string(),
        covered_by: covered.iter().map(|s| (*s).to_string()).collect(),
        gap_record: String::new(),
        note: String::new(),
    };
    let entries = vec![entry(tok, "", &["hostile/container/empty-file"])];
    let mut rec_ids: BTreeSet<String> = ["hostile/container/empty-file".to_string()]
        .into_iter()
        .collect();
    let mut cat_of_rec: BTreeMap<String, String> =
        BTreeMap::from([("hostile/container/empty-file".to_string(), tok.to_string())]);
    let mut rec_rows: BTreeMap<String, BTreeSet<String>> = BTreeMap::from([(
        "hostile/container/empty-file".to_string(),
        ["L1".to_string()].into_iter().collect(),
    )]);

    // omit a different ratified token -> must be flagged
    assert!(
        !audited_token_set_violations(&entries).is_empty(),
        "omitted ratified tokens not flagged"
    );

    let wrong_cat = entry("checksum-error", "", &["hostile/container/empty-file"]);
    assert!(
        !covered_category_violations(&wrong_cat, &rec_ids, &cat_of_rec, &empty_blocked).is_empty(),
        "wrong-category claim not flagged"
    );

    let wrong_row = entry("", "L3", &["hostile/container/empty-file"]);
    let errs = covered_row_violations(&wrong_row, &rec_ids, &rec_rows, &empty_blocked);
    assert!(
        errs.join("; ").contains("does not map row L3"),
        "wrong-row claim not flagged: {errs:?}"
    );

    let right_row = entry("", "L1", &["hostile/container/empty-file"]);
    let errs = covered_row_violations(&right_row, &rec_ids, &rec_rows, &empty_blocked);
    assert!(errs.is_empty(), "correct row claim flagged: {errs:?}");

    let unknown_rec = entry("", "L1", &["no/such/record"]);
    assert!(
        !covered_row_violations(&unknown_rec, &rec_ids, &rec_rows, &empty_blocked).is_empty(),
        "unknown-record claim not flagged"
    );

    let empty_covered = entry(tok, "", &[]);
    assert!(
        !covered_category_violations(&empty_covered, &rec_ids, &cat_of_rec, &empty_blocked)
            .is_empty(),
        "empty coveredBy not flagged"
    );

    // A "covered" claim resting only on blocked records is the exact
    // substitution issue 0018's audit condemned: missing-manifest was recorded
    // as covered by a single record that skipped. These negative invariants
    // prove the rule bites rather than merely existing.
    let blocked_only: BTreeSet<String> = ["hostile/container/empty-file".to_string()]
        .into_iter()
        .collect();
    let sole_blocked_cat = entry(tok, "", &["hostile/container/empty-file"]);
    let errs = covered_category_violations(&sole_blocked_cat, &rec_ids, &cat_of_rec, &blocked_only);
    assert!(
        errs.join("; ").contains("not executed coverage"),
        "category covered solely by a blocked record not flagged: {errs:?}"
    );
    let sole_blocked_row = entry("", "L1", &["hostile/container/empty-file"]);
    let errs = covered_row_violations(&sole_blocked_row, &rec_ids, &rec_rows, &blocked_only);
    assert!(
        errs.join("; ").contains("not executed coverage"),
        "row covered solely by a blocked record not flagged: {errs:?}"
    );

    // A blocked record ALONGSIDE executing evidence is fine: the rule forbids
    // substitution, not co-listing.
    rec_ids.insert("hostile/container/bad-signature".to_string());
    cat_of_rec.insert(
        "hostile/container/bad-signature".to_string(),
        tok.to_string(),
    );
    rec_rows.insert(
        "hostile/container/bad-signature".to_string(),
        ["L1".to_string()].into_iter().collect(),
    );
    let mixed_cat = entry(
        tok,
        "",
        &[
            "hostile/container/empty-file",
            "hostile/container/bad-signature",
        ],
    );
    let errs = covered_category_violations(&mixed_cat, &rec_ids, &cat_of_rec, &blocked_only);
    assert!(
        errs.is_empty(),
        "blocked record co-listed with executing evidence must be accepted: {errs:?}"
    );
    let mixed_row = entry(
        "",
        "L1",
        &[
            "hostile/container/empty-file",
            "hostile/container/bad-signature",
        ],
    );
    let errs = covered_row_violations(&mixed_row, &rec_ids, &rec_rows, &blocked_only);
    assert!(
        errs.is_empty(),
        "blocked record co-listed with executing evidence must be accepted: {errs:?}"
    );
}

/// Proves the manifest marker's validation bites.
///
/// The marker is the only thing standing between a green non-conformance
/// witness and a false coverage claim, so a marker naming an assertion the
/// record does not carry — or missing its route, reason, or clauses — must be a
/// hard error rather than documentation.
#[test]
fn execution_blocked_marker_is_enforced() {
    let base = || Expectation {
        id: "adversarial/vectors/probe".to_string(),
        class: "adversarial".to_string(),
        outcome: "success".to_string(),
        clauses: vec!["FS §4.7 V2".to_string()],
        assert: Some(BTreeMap::from([(
            "finalThicknessMm".to_string(),
            json!(150.0),
        )])),
        execution_blocked: Some(ExecutionBlocked {
            state: "blocked-implementation-nonconformance".to_string(),
            assertions: vec!["finalThicknessMm".to_string()],
            routed_to: "ED-0032".to_string(),
            clauses: vec!["FS §4.6 LWW1".to_string()],
            citations: Vec::new(),
            reason: "recorded reason".to_string(),
            observed: BTreeMap::from([("finalThicknessMm".to_string(), json!(120.0))]),
        }),
        ..blank_expectation()
    };

    type Mutate = fn(&mut Expectation);
    let cases: Vec<(&str, Mutate, bool)> = vec![
        ("well-formed", |_| {}, false),
        (
            "unknown state",
            |r| r.execution_blocked.as_mut().unwrap().state = "flaky".to_string(),
            true,
        ),
        (
            "no assertions named",
            |r| r.execution_blocked.as_mut().unwrap().assertions.clear(),
            true,
        ),
        (
            "assertion absent from assert block",
            |r| {
                r.execution_blocked.as_mut().unwrap().assertions =
                    vec!["noSuchAssertion".to_string()];
            },
            true,
        ),
        (
            "no route",
            |r| r.execution_blocked.as_mut().unwrap().routed_to.clear(),
            true,
        ),
        (
            "no reason",
            |r| r.execution_blocked.as_mut().unwrap().reason.clear(),
            true,
        ),
        (
            "no clauses",
            |r| r.execution_blocked.as_mut().unwrap().clauses.clear(),
            true,
        ),
        (
            "no observed value for a blocked assertion",
            |r| r.execution_blocked.as_mut().unwrap().observed.clear(),
            true,
        ),
        (
            "observed value equals the oracle",
            |r| {
                r.execution_blocked.as_mut().unwrap().observed =
                    BTreeMap::from([("finalThicknessMm".to_string(), json!(150.0))]);
            },
            true,
        ),
        (
            "observed value written with a different number spelling still equals the oracle",
            |r| {
                r.assert
                    .as_mut()
                    .unwrap()
                    .insert("finalThicknessMm".to_string(), json!(150));
                r.execution_blocked.as_mut().unwrap().observed =
                    BTreeMap::from([("finalThicknessMm".to_string(), json!(150.0))]);
            },
            true,
        ),
        (
            "observed value for an unblocked assertion",
            |r| {
                r.assert
                    .as_mut()
                    .unwrap()
                    .insert("other".to_string(), json!(1.0));
                r.execution_blocked
                    .as_mut()
                    .unwrap()
                    .observed
                    .insert("other".to_string(), json!(2.0));
            },
            true,
        ),
    ];

    for (name, mutate, want_err) in cases {
        let mut r = base();
        mutate(&mut r);
        let errs = execution_blocked_violations(&r);
        if want_err {
            assert!(!errs.is_empty(), "{name}: expected a violation, got none");
        } else {
            assert!(errs.is_empty(), "{name}: unexpected violations: {errs:?}");
        }
    }
}

/// An `Expectation` with every field at its zero value, for test construction.
fn blank_expectation() -> Expectation {
    serde_json::from_value(json!({})).expect("an empty record decodes to defaults")
}
