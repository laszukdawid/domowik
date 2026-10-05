//! The machine-readable corpus contract: `testdata/corpus/EXPECTATIONS.md`.
//!
//! Record and audit types, the closed vocabularies FS §9.3/§9.4 ratified, and
//! the pure validators the corpus gates run. The validators are separated from
//! the gates deliberately so their NEGATIVE invariants can be tested — a
//! validator that never fires is documentation, not a gate.
//!
//! Ported from `internal/corpusgen/corpus_test.go` under issue 0062.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

/// One file owned by a recipe, carrying its sha256 so input tampering is
/// detected by recomputation on every run (BLOCKER-3).
#[derive(Clone, Debug, Deserialize)]
pub struct RecipeInput {
    /// Path relative to `testdata/corpus`.
    pub path: String,
    /// Expected sha256, recomputed from the bytes on every run.
    pub sha256: String,
}

/// The machine-checkable at-limit/one-over semantic of a limit probe: which
/// §9.3 quantity is probed and what outcome each side of the exact boundary
/// must produce.
#[derive(Clone, Debug, Deserialize)]
pub struct Boundary {
    /// The §9.3 quantity token being probed.
    #[serde(default)]
    pub quantity: String,
    /// The §9.3 row ID.
    #[serde(default)]
    pub row: String,
    /// Outcome exactly at the limit.
    #[serde(default, rename = "atLimitOutcome")]
    pub at_limit_outcome: String,
    /// Outcome one step beyond the limit.
    #[serde(default, rename = "oneOverOutcome")]
    pub one_over_outcome: String,
}

/// A fixture described by construction rather than by committed bytes.
#[derive(Clone, Debug, Deserialize)]
pub struct Recipe {
    /// One of [`valid_recipe_kinds`].
    #[serde(default)]
    pub kind: String,
    /// Explanatory expression over §9.3 row names.
    #[serde(default, rename = "declaredLogicalSize")]
    pub declared_logical_size: String,
    /// `none` or `bounded`.
    #[serde(default)]
    pub materialization: String,
    /// §9.3 rows this recipe binds.
    #[serde(default, rename = "boundRows")]
    pub bound_rows: Vec<String>,
    /// Structured boundary semantics.
    #[serde(default)]
    pub boundaries: Vec<Boundary>,
    /// Files owned by this recipe.
    #[serde(default)]
    pub inputs: Vec<RecipeInput>,
    /// Bytes the recipe commits, when it commits any.
    #[serde(default, rename = "committedPhysicalBytes")]
    pub committed_physical_bytes: Option<i64>,
}

/// Names assertions of an OTHERWISE EXECUTING record as not-yet-provable, with
/// the route that owns them.
///
/// Deliberately neither a pass nor a skip: the harness still executes the
/// record and pins the divergence, while the coverage audit refuses to count
/// the record as evidence. `EXPECTATIONS.md` is the single authority for this
/// state — no harness-local map may restate it.
#[derive(Clone, Debug, Deserialize)]
pub struct ExecutionBlocked {
    /// One of [`valid_blocked_states`].
    #[serde(default)]
    pub state: String,
    /// The blocked assertion names; each must exist in the record's assert block.
    #[serde(default)]
    pub assertions: Vec<String>,
    /// The route that owns the blocker.
    #[serde(default, rename = "routedTo")]
    pub routed_to: String,
    /// The normative clauses in tension.
    #[serde(default)]
    pub clauses: Vec<String>,
    /// Supporting citations.
    #[serde(default)]
    pub citations: Vec<String>,
    /// Why the assertion cannot be proven yet.
    #[serde(default)]
    pub reason: String,
    /// The CURRENT non-conformant value of each blocked assertion, so the
    /// harness pins the specific divergence instead of merely asserting
    /// inequality with the oracle. Without it, drift to a third value stays
    /// green.
    #[serde(default)]
    pub observed: BTreeMap<String, Value>,
}

/// One record of the corpus contract.
#[derive(Clone, Debug, Deserialize)]
pub struct Expectation {
    /// Stable record id, unique across the manifest.
    #[serde(default)]
    pub id: String,
    /// One of [`valid_classes`].
    #[serde(default)]
    pub class: String,
    /// Path relative to `testdata/corpus`, when the record is artifact-backed.
    #[serde(default)]
    pub artifact: String,
    /// Recomputed on every run; never trusted from a sidecar.
    #[serde(default)]
    pub sha256: String,
    /// Construction description, when the record is not artifact-backed.
    #[serde(default)]
    pub recipe: Option<Recipe>,
    /// One of [`valid_outcomes`].
    #[serde(default)]
    pub outcome: String,
    /// The §9.3/§9.4 error class, or null where no ratified class exists.
    #[serde(default)]
    pub category: Option<String>,
    /// Free-text diagnostic expectation.
    #[serde(default)]
    pub diagnostic: String,
    /// Normative clause citations.
    #[serde(default)]
    pub clauses: Vec<String>,
    /// True when the vector carries no conformance claim yet.
    #[serde(default, rename = "implementationDeferred")]
    pub implementation_deferred: bool,
    /// True when the artifact is deliberately not a JSON object.
    #[serde(default, rename = "rawByDesign")]
    pub raw_by_design: bool,
    /// Free-text notes.
    #[serde(default)]
    pub notes: String,
    /// §9.3 rows this record probes directly (byte-level evidence).
    #[serde(default, rename = "rowRefs")]
    pub row_refs: Vec<String>,
    /// Structured normative assertions.
    ///
    /// BLOCKER-6: assertions live in the MANIFEST, never inside wire fixture
    /// bytes; selected fields are cross-checked against the artifact content.
    #[serde(default)]
    pub assert: Option<BTreeMap<String, Value>>,
    /// The blocked-assertion marker, when the record carries one.
    #[serde(default, rename = "executionBlocked")]
    pub execution_blocked: Option<ExecutionBlocked>,
}

impl Expectation {
    /// The §9.3 rows the record's recipe binds.
    #[must_use]
    pub fn bound_rows(&self) -> Vec<String> {
        self.recipe
            .as_ref()
            .map(|r| r.bound_rows.clone())
            .unwrap_or_default()
    }

    /// The record's structured boundaries, nil-safe.
    #[must_use]
    pub fn recipe_boundaries(&self) -> Vec<Boundary> {
        self.recipe
            .as_ref()
            .map(|r| r.boundaries.clone())
            .unwrap_or_default()
    }

    /// Every §9.3 row the record maps, through recipe rows, row refs, or
    /// structured boundaries.
    #[must_use]
    pub fn mapped_rows(&self) -> BTreeSet<String> {
        let mut rows: BTreeSet<String> = self.bound_rows().into_iter().collect();
        rows.extend(self.row_refs.iter().cloned());
        rows.extend(self.recipe_boundaries().into_iter().map(|b| b.row));
        rows
    }
}

/// One coverage-audit row: a ratified category token or a §9.3 limit row.
#[derive(Clone, Debug, Deserialize)]
pub struct AuditEntry {
    /// The category token, for `categoryAudit` rows.
    #[serde(default)]
    pub token: String,
    /// The §9.3 row ID, for `rowAudit` rows.
    #[serde(default)]
    pub row: String,
    /// `covered`, or one of the blocked statuses.
    #[serde(default)]
    pub status: String,
    /// Record ids claimed as evidence.
    #[serde(default, rename = "coveredBy")]
    pub covered_by: Vec<String>,
    /// The blocked-open-question record that owns the gap.
    #[serde(default, rename = "gapRecord")]
    pub gap_record: String,
    /// Explanatory note for a blocked entry.
    #[serde(default)]
    pub note: String,
}

/// The reader-facing half of the honest-coverage contract: which records are
/// NOT executed evidence, and who owns them.
#[derive(Clone, Debug, Deserialize)]
pub struct ResidualBlocker {
    /// The blocked record's id.
    #[serde(default)]
    pub record: String,
    /// The record's blocked state.
    #[serde(default)]
    pub state: String,
    /// The record's blocked assertion names.
    #[serde(default, rename = "blockedAssertions")]
    pub blocked_assertions: Vec<String>,
    /// The route that owns the blocker.
    #[serde(default, rename = "routedTo")]
    pub routed_to: String,
}

/// The coverage-audit fence.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct CoverageAudit {
    /// One entry per ratified §9.4 category token.
    #[serde(default, rename = "categoryAudit")]
    pub category_audit: Vec<AuditEntry>,
    /// One entry per §9.3 limit row.
    #[serde(default, rename = "rowAudit")]
    pub row_audit: Vec<AuditEntry>,
    /// Every blocked record, in both directions.
    #[serde(default, rename = "residualBlockers")]
    pub residual_blockers: Vec<ResidualBlocker>,
}

// ===== Closed vocabularies =====

/// The closed vocabulary of non-executing states.
///
/// Each is distinct from both "passed" and "skipped": the record runs, and what
/// is withheld is a named assertion, not the whole record.
#[must_use]
pub fn valid_blocked_states() -> BTreeSet<&'static str> {
    [
        "blocked-implementation-nonconformance",
        "blocked-open-question",
    ]
    .into_iter()
    .collect()
}

/// The closed fixture-class vocabulary.
#[must_use]
pub fn valid_classes() -> BTreeSet<&'static str> {
    [
        "golden",
        "adversarial",
        "migration",
        "hostile",
        "declarative",
    ]
    .into_iter()
    .collect()
}

/// The closed outcome vocabulary.
#[must_use]
pub fn valid_outcomes() -> BTreeSet<&'static str> {
    [
        "success",
        "reject",
        "warning",
        "degraded-read-only-inspection",
        "missing-asset-equivalence",
        "safe-write-refusal",
        "blocked-open-question",
    ]
    .into_iter()
    .collect()
}

/// The closed token set from FORMAT-SPEC §9.4 plus the §9.3 error-category
/// column. Proposed-but-unratified diagnostics are deliberately absent.
#[must_use]
pub fn ratified_category_tokens() -> BTreeSet<&'static str> {
    [
        "malformed-container",
        "malformed-length",
        "unsupported-method",
        "unsupported-extension",
        "unsupported-version",
        "encrypted-entry",
        "checksum-error",
        "trailing-data",
        "overlapping-regions",
        "path-traversal",
        "invalid-entry-name",
        "duplicate-logical-name",
        "ambiguous-manifest",
        "missing-manifest",
        "nonfinite-value",
        "numeric-overflow",
        "malformed-framing",
        "invalid-field-syntax",
        "invalid-reference",
        "limit-total-expanded",
        "limit-total-physical",
        "limit-entry-expanded",
        "limit-entry-physical",
        "limit-blob-bytes",
        "limit-entry-count",
        "limit-manifest-reserve",
        "limit-archive-depth",
        "limit-nesting-depth",
        "limit-number-token",
        "limit-array-count",
        "limit-member-count",
        "limit-image-dimensions",
        "limit-model-complexity",
        "limit-chunk-count",
        "limit-name-length",
        "limit-name-component",
        "limit-expansion-ratio",
        "limit-aggregate-expansion",
        "limit-operation-count",
        "limit-snapshot-metadata",
        "limit-transfer-bytes",
        "limit-transfer-time",
        "limit-redirect-hops",
        "limit-hints-session",
        "limit-concurrent-retrieval",
    ]
    .into_iter()
    .collect()
}

/// The closed recipe-kind vocabulary.
#[must_use]
pub fn valid_recipe_kinds() -> BTreeSet<&'static str> {
    [
        "limit-probe",
        "batch-pair",
        "policy-vector",
        "pure-function-vector",
        "fault-vector",
        "precedence-vector",
        "blocked-obligation",
    ]
    .into_iter()
    .collect()
}

/// The closed machine vocabulary for projection outcomes asserted in records.
#[must_use]
pub fn canonical_outcome_enum() -> BTreeSet<&'static str> {
    [
        "apply",
        "hold-then-apply",
        "discard-stale",
        "ignore-duplicate",
    ]
    .into_iter()
    .collect()
}

// ===== Pattern matchers =====
//
// Written out rather than pulled from `regex`: each is a fixed-shape test that
// plain character classification expresses directly, and the dependency
// inventory exists to make an added crate visible.

/// Matches `^L([1-9]|[12][0-9]|3[0-3])$` — a ratified §9.3 row ID, L1 to L33.
#[must_use]
pub fn is_limit_row(s: &str) -> bool {
    let Some(digits) = s.strip_prefix('L') else {
        return false;
    };
    if digits.is_empty() || digits.len() > 2 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if digits.starts_with('0') {
        return false;
    }
    matches!(digits.parse::<u32>(), Ok(n) if (1..=33).contains(&n))
}

fn all_hex_lower(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Matches `^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`.
#[must_use]
pub fn is_uuid36(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lens = [8, 4, 4, 4, 12];
    for (p, want) in parts.iter().zip(lens) {
        if p.len() != want || !all_hex_lower(p) {
            return false;
        }
    }
    let v = parts[2].as_bytes()[0];
    if !(b'1'..=b'8').contains(&v) {
        return false;
    }
    let var = parts[3].as_bytes()[0];
    matches!(var, b'8' | b'9' | b'a' | b'b')
}

/// Matches the §7.6 entry stem: `counterBase` zero-padded to 19 digits (the
/// signed-64-bit ceiling L13 makes 19 total-order-preserving under byte
/// comparison) followed by the canonical actorId.
///
/// The padding is load-bearing, so a short form is a different name rather than
/// a normalizable one.
#[must_use]
pub fn is_batch_stem(s: &str) -> bool {
    if s.len() < 20 {
        return false;
    }
    let (counter, rest) = s.split_at(19);
    if !counter.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    match rest.strip_prefix('-') {
        Some(uuid) => is_uuid36(uuid),
        None => false,
    }
}

// ===== Go-compatible JSON value equality =====

/// Compares two decoded JSON values the way Go's `reflect.DeepEqual` compares
/// values decoded into `any`.
///
/// Go decodes EVERY JSON number into `float64`, so `150` and `150.0` are the
/// same value there. `serde_json` keeps the distinction, which would make an
/// `observed` value silently differ from an oracle written with a different
/// spelling of the same number. Numbers are therefore compared as `f64`.
#[must_use]
pub fn go_json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => x == y,
            _ => x == y,
        },
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| go_json_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| go_json_equal(v, w)))
        }
        _ => a == b,
    }
}

/// Decodes `data` the way Go's `json.Unmarshal(data, &map[string]any{})` does.
///
/// Go accepts a top-level `null` into a map WITHOUT error, leaving the map nil
/// — and a nil map reads exactly like an empty one, so `null` is returned here
/// as an empty map rather than as a parse failure. Getting this wrong would
/// have weakened the `rawByDesign` gate: a fixture containing the four bytes
/// `null` parses in Go and would therefore be caught as a false raw claim,
/// while `serde_json::from_slice::<Map>` errors and would have let it through.
///
/// Returns `None` when the bytes are not valid JSON, or are valid JSON that is
/// neither an object nor `null`.
#[must_use]
pub fn go_unmarshal_object(data: &[u8]) -> Option<serde_json::Map<String, Value>> {
    match serde_json::from_slice::<Value>(data) {
        Ok(Value::Object(m)) => Some(m),
        Ok(Value::Null) => Some(serde_json::Map::new()),
        _ => None,
    }
}

/// A JSON number as Go's `int(...)` conversion would see it, when that
/// conversion is DEFINED.
///
/// Go decodes every JSON number into `float64` and the validators then convert
/// with `int(...)`. For a value outside the integer range that conversion is
/// undefined in Go, so there is no behaviour to match — this returns `None`
/// instead, which makes the validator reject rather than accept. Deliberately
/// stricter than both, and well defined.
#[must_use]
pub fn as_go_integer(v: &Value) -> Option<i64> {
    let n = v.as_f64()?;
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    // i64::MIN/MAX are not exactly representable as f64 bounds; compare against
    // the nearest powers of two that are.
    if !(-(2f64.powi(63))..2f64.powi(63)).contains(&n) {
        return None;
    }
    Some(n as i64)
}

// ===== Validators =====

/// Validates a record's `executionBlocked` marker.
///
/// The marker is the only thing standing between a green non-conformance
/// witness and a false coverage claim, so a marker naming an assertion the
/// record does not carry — or missing its route, reason, or clauses — is an
/// error rather than documentation.
#[must_use]
pub fn execution_blocked_violations(rec: &Expectation) -> Vec<String> {
    let Some(eb) = rec.execution_blocked.as_ref() else {
        return Vec::new();
    };
    let empty = BTreeMap::new();
    let asserts = rec.assert.as_ref().unwrap_or(&empty);
    let mut errs = Vec::new();
    if !valid_blocked_states().contains(eb.state.as_str()) {
        errs.push(format!(
            "executionBlocked.state {:?} is not a known blocked state",
            eb.state
        ));
    }
    if eb.assertions.is_empty() {
        errs.push("executionBlocked must name at least one blocked assertion".to_string());
    }
    for name in &eb.assertions {
        if !asserts.contains_key(name) {
            errs.push(format!(
                "executionBlocked names assertion {name:?}, which is absent from the record's assert block"
            ));
        }
    }
    if eb.routed_to.is_empty() {
        errs.push("executionBlocked must name the route that owns it".to_string());
    }
    if eb.reason.is_empty() {
        errs.push("executionBlocked must state its reason".to_string());
    }
    if eb.clauses.is_empty() {
        errs.push("executionBlocked must cite the normative clauses in tension".to_string());
    }
    // Every blocked assertion must record what is observed instead, and the
    // observation must genuinely differ from the oracle: an "observed" value
    // equal to the oracle would mean the record is not blocked at all.
    let blocked: BTreeSet<&String> = eb.assertions.iter().collect();
    for name in &eb.assertions {
        if !eb.observed.contains_key(name) {
            errs.push(format!(
                "executionBlocked names assertion {name:?} but records no observed value for it"
            ));
        }
    }
    for (name, got) in &eb.observed {
        if !blocked.contains(name) {
            errs.push(format!(
                "executionBlocked records an observed value for {name:?}, which it does not list as blocked"
            ));
            continue;
        }
        if let Some(want) = asserts.get(name) {
            if go_json_equal(got, want) {
                errs.push(format!(
                    "executionBlocked observed value for {name:?} equals the oracle ({want}): the assertion is not blocked"
                ));
            }
        }
    }
    errs
}

/// Validates one `covered` category entry against the record index.
///
/// A blocked record may appear alongside real evidence, but never as the whole
/// of it: that is exactly the substitution issue 0018's audit condemned, where
/// `missing-manifest` was recorded as covered by a single record that skipped.
#[must_use]
pub fn covered_category_violations(
    e: &AuditEntry,
    rec_ids: &BTreeSet<String>,
    cat_of_rec: &BTreeMap<String, String>,
    blocked: &BTreeSet<String>,
) -> Vec<String> {
    let mut errs = Vec::new();
    if e.covered_by.is_empty() {
        errs.push("covered without records".to_string());
    }
    let mut executing = 0usize;
    for id in &e.covered_by {
        if !rec_ids.contains(id) {
            errs.push(format!("covers unknown record {id}"));
            continue;
        }
        let cat = cat_of_rec.get(id).cloned().unwrap_or_default();
        if cat != e.token {
            errs.push(format!("record {id} has category {cat:?}"));
        }
        if !blocked.contains(id) {
            executing += 1;
        }
    }
    if !e.covered_by.is_empty() && executing == 0 {
        errs.push(
            "covered only by records whose assertions are blocked: that is not executed coverage"
                .to_string(),
        );
    }
    errs
}

/// Validates one `covered` row entry against the record row mapping.
#[must_use]
pub fn covered_row_violations(
    e: &AuditEntry,
    rec_ids: &BTreeSet<String>,
    rec_rows: &BTreeMap<String, BTreeSet<String>>,
    blocked: &BTreeSet<String>,
) -> Vec<String> {
    let mut errs = Vec::new();
    if e.covered_by.is_empty() {
        errs.push("covered without records".to_string());
    }
    let mut executing = 0usize;
    for id in &e.covered_by {
        if !rec_ids.contains(id) {
            errs.push(format!("covers unknown record {id}"));
            continue;
        }
        if !rec_rows.get(id).is_some_and(|rows| rows.contains(&e.row)) {
            errs.push(format!("record {id} does not map row {}", e.row));
        }
        if !blocked.contains(id) {
            executing += 1;
        }
    }
    if !e.covered_by.is_empty() && executing == 0 {
        errs.push(
            "covered only by records whose assertions are blocked: that is not executed coverage"
                .to_string(),
        );
    }
    errs
}

/// Reports ratified categories missing from or duplicated in the audit.
#[must_use]
pub fn audited_token_set_violations(entries: &[AuditEntry]) -> Vec<String> {
    let mut errs = Vec::new();
    let ratified = ratified_category_tokens();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for e in entries {
        if !seen.insert(e.token.as_str()) {
            errs.push(format!("duplicate audit entry {}", e.token));
        }
        if !ratified.contains(e.token.as_str()) {
            errs.push(format!("token {:?} not in ratified set", e.token));
        }
    }
    for tok in &ratified {
        if !seen.contains(tok) {
            errs.push(format!("ratified category {tok:?} missing from audit"));
        }
    }
    errs
}

/// The ONLY generator-produced metadata sidecar in the corpus.
///
/// The sidecar exemption in [`assert_checks`] matches this exact path, not a
/// suffix pattern (BLOCKER-8).
pub const SIDECAR_REL_PATH: &str = "hostile/container/ratio-under-L15-success-twin.meta.json";

/// Validates the known typed assertion fields whenever they are present.
///
/// Conditional-on-presence is deliberate and documented: the construct-specific
/// REQUIREMENTS are enforced elsewhere (`canonicalOutcome` `discard-stale` =>
/// `discardedBatchIds`; warnings => warning outcome; `limit-probe` =>
/// structured boundaries), never by prose.
#[must_use]
pub fn check_typed_assert(rec: &Expectation, raw: &BTreeMap<String, Value>) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    if let Some(co) = raw.get("canonicalOutcome") {
        match co.as_str() {
            Some(s) if canonical_outcome_enum().contains(s) => {
                if s == "discard-stale" {
                    let ok = raw
                        .get("discardedBatchIds")
                        .and_then(Value::as_array)
                        .is_some_and(|a| !a.is_empty());
                    if !ok {
                        errs.push(
                            "canonicalOutcome discard-stale requires non-empty discardedBatchIds"
                                .to_string(),
                        );
                    }
                }
            }
            _ => errs.push(format!("assert.canonicalOutcome {co} outside closed enum")),
        }
    }
    if let Some(w) = raw.get("winner") {
        match w.as_object() {
            None => errs.push("assert.winner must be an object".to_string()),
            Some(m) => {
                // The winner is a POSITION: a reference into the snapshot's
                // batchPositions table (a §7.6 entry stem) plus the operation's
                // index within that batch (FS §4.6 LWW1, §5.1 S3). The
                // pre-ED-0037 (operationCounter, actorId) pair cannot place a
                // write against a later concurrent one and is no longer a valid
                // shape here.
                let field = m.get("field").and_then(Value::as_str).unwrap_or_default();
                let batch = m.get("batch").and_then(Value::as_str).unwrap_or_default();
                let actor = m.get("actorId").and_then(Value::as_str).unwrap_or_default();
                let idx_ok = m
                    .get("operationIndex")
                    .and_then(as_go_integer)
                    .is_some_and(|v| v >= 0);
                if field.is_empty() || !is_batch_stem(batch) || !idx_ok || !is_uuid36(actor) {
                    errs.push(
                        "assert.winner shape invalid (field/batch/operationIndex/actorId)"
                            .to_string(),
                    );
                }
            }
        }
    }
    if let Some(tr) = raw.get("tombstoneRetained") {
        if !tr.is_boolean() {
            errs.push("assert.tombstoneRetained must be boolean".to_string());
        }
    }
    if let Some(dp) = raw.get("deletingPositionBatch") {
        if !dp.as_str().is_some_and(is_batch_stem) {
            errs.push("assert.deletingPositionBatch must be a §7.6 entry stem".to_string());
        }
    }
    for name in [
        "winnerPositionsPresent",
        "createdPositionsPresent",
        "batchPositionsPresent",
    ] {
        if let Some(v) = raw.get(name) {
            if !v.is_boolean() {
                errs.push(format!("assert.{name} must be boolean"));
            }
        }
    }
    if let Some(ids) = raw.get("discardedBatchIds") {
        if !ids.is_null() {
            match ids.as_array() {
                None => errs.push("assert.discardedBatchIds must be an array".to_string()),
                Some(arr) => {
                    for v in arr {
                        if v.as_str().map_or(0, str::len) != 36 {
                            errs.push(
                                "discardedBatchIds entries must be 36-character batch identifiers"
                                    .to_string(),
                            );
                            break;
                        }
                    }
                }
            }
        }
    }
    if let Some(w) = raw.get("warnings") {
        match as_go_integer(w) {
            Some(n) if n >= 1 => {
                if rec.outcome != "warning" {
                    errs.push("assert.warnings requires record outcome warning".to_string());
                }
            }
            _ => errs.push("assert.warnings must be a positive integer".to_string()),
        }
    }
    if let Some(c) = raw.get("commits") {
        match c.as_bool() {
            None => errs.push("assert.commits must be boolean".to_string()),
            Some(true) if rec.outcome == "reject" => {
                errs.push("assert.commits=true contradicts reject outcome".to_string());
            }
            Some(_) => {}
        }
    }
    if let Some(pf) = raw.get("preservedFields") {
        if !pf.as_array().is_some_and(|a| !a.is_empty()) {
            errs.push("assert.preservedFields must be a non-empty array".to_string());
        }
    }
    if let Some(wb) = raw.get("writeBackContract") {
        let ok = matches!(wb.as_str(), Some("preserve" | "safe-write-refusal"));
        if !ok {
            errs.push("assert.writeBackContract must be preserve|safe-write-refusal".to_string());
        }
    }
    errs
}

/// Ties selected manifest assertions to the actual bytes under test
/// (BLOCKER-6): assertions live in the manifest, but these fields are verified
/// against the artifact document itself.
#[must_use]
pub fn cross_check_assert_bytes(
    rec: &Expectation,
    raw: &BTreeMap<String, Value>,
    corpus_root: &Path,
) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    let data = match std::fs::read(corpus_root.join(&rec.artifact)) {
        Ok(d) => d,
        Err(e) => {
            errs.push(format!("cannot read artifact for cross-check: {e}"));
            return errs;
        }
    };
    let Some(doc) = go_unmarshal_object(&data) else {
        errs.push(format!(
            "artifact {} does not parse as a JSON object during cross-check",
            rec.artifact
        ));
        return errs;
    };
    if let Some(ek) = raw.get("entityKind").and_then(Value::as_str) {
        let got = doc.get("type").and_then(Value::as_str).unwrap_or_default();
        if got != ek {
            errs.push(format!(
                "assert.entityKind {ek:?} does not match artifact type {got:?}"
            ));
        }
    }
    if let Some(cb) = raw.get("counterBase").and_then(Value::as_f64) {
        let got = doc
            .get("counterBase")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if (got - cb).abs() > f64::EPSILON {
            errs.push(format!(
                "assert.counterBase {cb} does not match artifact counterBase {got}"
            ));
        }
    }
    if let Some(dc) = raw.get("descriptorCount").and_then(Value::as_f64) {
        if doc.len() as f64 != dc {
            errs.push(format!(
                "assert.descriptorCount {} does not match artifact member count {}",
                dc as i64,
                doc.len()
            ));
        }
    }
    if let Some(ec) = raw.get("entityCount").and_then(Value::as_f64) {
        let ents = doc.get("entities").and_then(Value::as_object);
        if !ents.is_some_and(|m| m.len() as f64 == ec) {
            errs.push(format!(
                "assert.entityCount {} does not match artifact entities {}",
                ec as i64,
                doc.get("entities").unwrap_or(&Value::Null)
            ));
        }
    }
    errs
}

/// Returns the violation messages for one record's assert contract; an empty
/// vector means compliant.
#[must_use]
pub fn assert_checks(rec: &Expectation, corpus_root: &Path) -> Vec<String> {
    let mut errs: Vec<String> = Vec::new();
    if !rec.artifact.ends_with(".json") {
        return errs; // binary fixtures carry no assert contract
    }
    if rec.artifact == SIDECAR_REL_PATH {
        if rec.assert.is_some() {
            errs.push("sidecar records must not carry assert objects".to_string());
        }
        return errs;
    }
    let data = match std::fs::read(corpus_root.join(&rec.artifact)) {
        Ok(d) => d,
        Err(e) => {
            errs.push(format!("cannot read artifact for assert validation: {e}"));
            return errs;
        }
    };
    let parsed = go_unmarshal_object(&data);

    if rec.raw_by_design {
        // BLOCKER-8: the flag must be TRUE of the bytes — the document must
        // genuinely be non-JSON or a non-object top level.
        if parsed.is_some() {
            errs.push(format!(
                "{} is marked rawByDesign but parses as a strict JSON object",
                rec.artifact
            ));
        }
        if rec.assert.is_some() {
            errs.push("rawByDesign records must not carry assert objects".to_string());
        }
        return errs;
    }
    if parsed.is_none() {
        errs.push(format!(
            "{} does not parse as a JSON object but is not marked rawByDesign",
            rec.artifact
        ));
        return errs;
    }
    // BLOCKER-7: a normal parseable JSON artifact MUST carry its manifest
    // assert; omission is no longer tolerated.
    let Some(raw) = rec.assert.as_ref() else {
        errs.push(format!(
            "{} lacks its required manifest assert object",
            rec.artifact
        ));
        return errs;
    };
    let Some(outcome) = raw.get("outcome").and_then(Value::as_str) else {
        errs.push("assert must carry a string outcome machine assertion".to_string());
        return errs;
    };
    if !valid_outcomes().contains(outcome) && outcome != "success-on-read" {
        errs.push(format!("assert.outcome {outcome:?} outside vocabulary"));
    }
    let contradicts = |want: &[&str]| -> bool { !want.contains(&outcome) };
    let mismatch = match rec.outcome.as_str() {
        "success" => contradicts(&["success", "success-on-read"]),
        "reject" => contradicts(&["reject"]),
        "warning" => contradicts(&["warning"]),
        "degraded-read-only-inspection" => contradicts(&["degraded-read-only-inspection"]),
        "missing-asset-equivalence" => contradicts(&["missing-asset-equivalence"]),
        "safe-write-refusal" => contradicts(&["safe-write-refusal"]),
        _ => false,
    };
    if mismatch {
        errs.push(format!(
            "assert.outcome {outcome:?} contradicts record outcome {:?}",
            rec.outcome
        ));
    }
    errs.extend(check_typed_assert(rec, raw));
    errs.extend(cross_check_assert_bytes(rec, raw, corpus_root));
    errs
}

// ===== Manifest loading =====

/// Returns every fenced ```json block body in a Markdown file.
///
/// # Errors
///
/// Returns an error when the file cannot be read or a fence is unterminated.
pub fn extract_fenced_json(path: &Path) -> Result<Vec<String>, String> {
    let data = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut blocks = Vec::new();
    let mut inside = false;
    let mut buf: Vec<&str> = Vec::new();
    for line in data.split('\n') {
        let trimmed = line.trim();
        if !inside && trimmed.starts_with("```json") {
            inside = true;
            buf.clear();
        } else if inside && trimmed == "```" {
            blocks.push(buf.join("\n"));
            inside = false;
        } else if inside {
            buf.push(line);
        }
    }
    if inside {
        return Err(format!("{}: unterminated json fence", path.display()));
    }
    Ok(blocks)
}

/// Loads every expectation record from `EXPECTATIONS.md`.
///
/// Object fences carry the coverage audit rather than records and are skipped.
///
/// # Errors
///
/// Returns an error when a fence is not valid JSON or is not an array of
/// records.
pub fn load_expectations(corpus_root: &Path) -> Result<Vec<Expectation>, String> {
    let blocks = extract_fenced_json(&corpus_root.join("EXPECTATIONS.md"))?;
    let mut all = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let probe: Value =
            serde_json::from_str(b).map_err(|e| format!("EXPECTATIONS.md fence {i}: {e}"))?;
        if probe.is_object() {
            continue; // coverage-audit object fence, not a record array
        }
        let batch: Vec<Expectation> =
            serde_json::from_str(b).map_err(|e| format!("EXPECTATIONS.md fence {i}: {e}"))?;
        all.extend(batch);
    }
    Ok(all)
}

/// Loads the single coverage-audit fence.
///
/// # Errors
///
/// Returns an error when the fence is missing, duplicated, or malformed.
pub fn load_coverage_audit(corpus_root: &Path) -> Result<CoverageAudit, String> {
    let blocks = extract_fenced_json(&corpus_root.join("EXPECTATIONS.md"))?;
    let mut found: Option<CoverageAudit> = None;
    for (i, b) in blocks.iter().enumerate() {
        let Ok(probe) = serde_json::from_str::<serde_json::Map<String, Value>>(b) else {
            continue;
        };
        if !probe.contains_key("categoryAudit") {
            continue;
        }
        if found.is_some() {
            return Err("multiple audit fences".to_string());
        }
        found = Some(serde_json::from_str(b).map_err(|e| format!("audit fence {i}: {e}"))?);
    }
    found.ok_or_else(|| "coverage audit fence missing".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_rows_are_l1_through_l33_without_leading_zeros() {
        for ok in ["L1", "L9", "L10", "L19", "L20", "L29", "L30", "L33"] {
            assert!(is_limit_row(ok), "{ok} must be a ratified row");
        }
        for bad in [
            "L0", "L34", "L99", "L01", "L", "l5", "L5x", "5", "LL5", "L100",
        ] {
            assert!(!is_limit_row(bad), "{bad} must not be a ratified row");
        }
    }

    #[test]
    fn uuid36_pins_version_and_variant_nibbles() {
        assert!(is_uuid36("01948f5a-ca77-7a09-9e76-5ace11b12fd2"));
        // version nibble outside 1-8
        assert!(!is_uuid36("01948f5a-ca77-0a09-9e76-5ace11b12fd2"));
        assert!(!is_uuid36("01948f5a-ca77-9a09-9e76-5ace11b12fd2"));
        // variant nibble outside 8-b
        assert!(!is_uuid36("01948f5a-ca77-7a09-7e76-5ace11b12fd2"));
        // uppercase hex, wrong lengths, missing groups
        assert!(!is_uuid36("01948F5A-ca77-7a09-9e76-5ace11b12fd2"));
        assert!(!is_uuid36("01948f5a-ca7-7a09-9e76-5ace11b12fd2"));
        assert!(!is_uuid36("01948f5a-ca77-7a09-9e76"));
    }

    #[test]
    fn batch_stem_padding_is_load_bearing() {
        let stem = "0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2";
        assert!(is_batch_stem(stem));
        // A short counter is a DIFFERENT name, not a normalizable one.
        assert!(!is_batch_stem("42-01948f5a-ca77-7a09-9e76-5ace11b12fd2"));
        assert!(!is_batch_stem("0000000000000000042-not-a-uuid"));
        assert!(!is_batch_stem("0000000000000000042"));
    }

    /// Go's `json.Unmarshal` accepts a top-level `null` into a map without
    /// error, leaving it nil. `serde_json::from_slice::<Map>` refuses it. The
    /// difference is not academic: it decides whether a fixture containing the
    /// four bytes `null` can claim `rawByDesign` and skip its assert contract.
    #[test]
    fn top_level_null_parses_as_an_empty_object_like_go() {
        assert_eq!(go_unmarshal_object(b"null").map(|m| m.len()), Some(0));
        assert_eq!(go_unmarshal_object(br#"{"a":1}"#).map(|m| m.len()), Some(1));
        assert!(go_unmarshal_object(b"[1,2]").is_none());
        assert!(go_unmarshal_object(b"42").is_none());
        assert!(go_unmarshal_object(br#""text""#).is_none());
        assert!(go_unmarshal_object(b"{").is_none());
    }

    /// Go converts these with `int(...)`, which is UNDEFINED outside the
    /// integer range, so there is no behaviour to match out there. Rejecting is
    /// the well-defined choice and is stricter than either side.
    #[test]
    fn go_integer_rejects_what_int_conversion_cannot_represent() {
        assert_eq!(as_go_integer(&serde_json::json!(42)), Some(42));
        assert_eq!(as_go_integer(&serde_json::json!(42.0)), Some(42));
        assert_eq!(as_go_integer(&serde_json::json!(-1)), Some(-1));
        assert_eq!(as_go_integer(&serde_json::json!(1.5)), None);
        assert_eq!(as_go_integer(&serde_json::json!(1e300)), None);
        assert_eq!(as_go_integer(&serde_json::json!(-1e300)), None);
        assert_eq!(as_go_integer(&serde_json::json!("42")), None);
        assert_eq!(as_go_integer(&Value::Null), None);
    }

    /// Go decodes every JSON number into `float64`, so an oracle written `150`
    /// and an observation written `150.0` are the same value there. Without
    /// this, the blocked-assertion check would stop firing on a record that
    /// merely spelled its number differently.
    #[test]
    fn number_spelling_does_not_change_equality() {
        assert!(go_json_equal(
            &serde_json::json!(150),
            &serde_json::json!(150.0)
        ));
        assert!(go_json_equal(
            &serde_json::json!({"a": [1, 2.0]}),
            &serde_json::json!({"a": [1.0, 2]})
        ));
        assert!(!go_json_equal(
            &serde_json::json!(150),
            &serde_json::json!(151)
        ));
        assert!(!go_json_equal(
            &serde_json::json!("150"),
            &serde_json::json!(150)
        ));
    }
}
