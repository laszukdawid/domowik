//! The `.domo` package: reading and writing plan files, and enforcement of the
//! §9.3 resource limits.
//!
//! # Container shape
//!
//! A `.domo` file is a ZIP archive holding:
//!
//! - `manifest.json` — format version and entry inventory
//! - `document.json`  — the plan itself
//!
//! # Conformance scope, stated plainly
//!
//! This is a WORKING SUBSET, not the full container specified in
//! `docs/FORMAT-SPEC.md`. It does not implement the operation log, snapshots,
//! causal position, atomic replacement (§9.7) or the framing rules. What it
//! does implement is real: limits are enforced before allocation, writes are
//! atomic against the destination, and a truncated or hostile archive is
//! rejected rather than trusted.

#![forbid(unsafe_code)]

use domowik_document::log::{LogError, OperationLog};
use domowik_document::operation::{Batch, Operation};
use domowik_document::replay::{project_from_with_tombstones, Projection};
use domowik_document::session::Session;
use domowik_format::Document;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::path::Path;
use uuid::Uuid;

/// Resource limits from `docs/FORMAT-SPEC.md` §9.3.
///
/// Every field is a **ratified normative value**, not a tuning knob. Changing
/// one is a format change and requires the process in §9.3, not an edit here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// Row **L6** — entry/member count per package.
    pub max_entry_count: u32,
    /// Largest single uncompressed entry this reader will materialise, bytes.
    pub max_entry_bytes: u64,
    /// Row **L23** — total retained operations per package.
    pub max_operations: u64,
    /// Row **L23** — operations per batch.
    pub max_operations_per_batch: u64,
    /// Row **L11** — JSON container nesting depth.
    pub max_nesting_depth: u32,
    /// Row **L23** — snapshot metadata, in uncompressed serialized bytes.
    pub max_snapshot_metadata_bytes: u64,
}

impl Limits {
    /// §9.3 row L6: entry/member count per package.
    ///
    /// Spec text: "Count; > N rejected (**== 65,535 succeeds**)". The value is
    /// 65,535 and **not** 65,536 because the legacy ZIP end-of-central-directory
    /// entry field is 16 bits (APPNOTE 6.3.10 §§4.3.16, 4.4.21), so 65,535 is
    /// the largest count any ZIP-family container can represent without Zip64
    /// counts. 65,536 is therefore the first value a conformant reader must
    /// REJECT — an off-by-one here admits packages the format forbids.
    pub const L6_ENTRY_COUNT: u32 = 65_535;

    /// Working ceiling on a single entry: 256 MiB.
    ///
    /// This is a DECOMPRESSION BOMB GUARD. It is checked against the entry's
    /// declared uncompressed size BEFORE any buffer is reserved, so a header
    /// claiming 40 GB costs a comparison rather than an allocation.
    pub const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;

    /// Row **L23** — total retained operations per package: 2^20.
    ///
    /// A reader-side resource cap, distinct from anything semantic: L23
    /// "implies nothing about" the logical caps on batch contents.
    pub const L23_TOTAL_OPERATIONS: u64 = 1_048_576;

    /// Row **L23** — operations per batch.
    pub const L23_OPERATIONS_PER_BATCH: u64 = 65_535;

    /// Row **L11** — JSON container nesting depth: 128 levels.
    ///
    /// The boundary is INCLUSIVE: a text whose deepest array or object sits at
    /// level 128 parses, and one containing any value at level 129 is rejected
    /// with `limit-nesting-depth`. §9.3's counting contract defines a level as
    /// an enclosing array or object, the outermost at level 1, with scalars and
    /// object keys contributing none.
    ///
    /// **This is not inherited from `serde_json`, deliberately.** The pinned
    /// 1.0.151 accepts 127 levels and rejects 128 — a document this row
    /// declares conforming, refused at exactly the ratified boundary — and its
    /// limit can be switched off by the `unbounded_depth` feature from
    /// anywhere in the dependency graph. A normative bound cannot rest on
    /// either. [`check_nesting_depth`] counts it here instead.
    pub const L11_NESTING_DEPTH: u32 = 128;

    /// Returns whether `count` satisfies row L6. The boundary is inclusive.
    pub fn permits_entry_count(&self, count: u64) -> bool {
        count <= u64::from(self.max_entry_count)
    }

    /// Returns whether an entry declaring `bytes` uncompressed may be read.
    pub fn permits_entry_size(&self, bytes: u64) -> bool {
        bytes <= self.max_entry_bytes
    }

    /// Row L23: total retained operations per package.
    pub fn permits_operation_count(&self, count: u64) -> bool {
        count <= self.max_operations
    }

    /// Row L23: operations per batch.
    pub fn permits_batch_size(&self, count: u64) -> bool {
        count <= self.max_operations_per_batch
    }

    /// Row L11: whether a container at `depth` may be entered. Inclusive.
    pub fn permits_nesting_depth(&self, depth: u32) -> bool {
        depth <= self.max_nesting_depth
    }
}

/// Rejects JSON that nests containers deeper than row L11 allows, BEFORE any
/// of it is parsed.
///
/// Returns the depth of the first container that exceeds `limit`, so the
/// caller can name it. A level is an enclosing array or object, the outermost
/// at level 1; scalars and object keys are not levels, and brackets inside
/// string literals are not containers (FORMAT-SPEC §9.3, row L11's counting
/// contract).
///
/// This is a byte scan rather than a parse: it costs one pass over a buffer
/// the size limits have already bounded, and it runs before `serde_json` sees
/// the input, so a 10,000-level document is refused without allocating the
/// 129th level. Multi-byte UTF-8 is safe to scan bytewise because every
/// continuation byte is ≥ 0x80 and can never collide with a bracket, a quote
/// or a backslash.
pub fn check_nesting_depth(bytes: &[u8], limit: u32) -> Result<DepthChecked<'_>, u32> {
    let mut depth: u32 = 0;
    let mut in_string = false;
    let mut escaped = false;

    for &b in bytes {
        if in_string {
            match b {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'[' | b'{' => {
                depth += 1;
                if depth > limit {
                    return Err(depth);
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(DepthChecked(bytes))
}

/// Row **L23** — the snapshot metadata cap, in UNCOMPRESSED SERIALIZED bytes.
///
/// 16,777,216 B. The domain matters as much as the number: a reader-resource
/// cap bounds what a reader must HOLD, and compressed length cannot bound that.
/// Snapshot metadata is repetitive by construction — the same actor UUIDs and
/// batch stems recur in every retained frontier — so it compresses hard, and a
/// cap on stored bytes would admit a section that expands far past it.
pub const L23_SNAPSHOT_METADATA_BYTES: u64 = 16 * 1024 * 1024;

/// Counts the bytes row L23 binds, over an already-decompressed snapshot entry.
///
/// Counted: the serialized bytes of the metadata members — `frontier`,
/// `batchPositions`, `winnerPositions`, `createdPositions`, `tombstones` — key,
/// value, punctuation and the whitespace actually present, because §3.3 does
/// not require canonical JSON and the cap binds the bytes PRESENT rather than a
/// normalized form. Unknown members are counted too: a member this version does
/// not understand occupies memory exactly as a known one does.
///
/// NOT counted: `entities`, S1's projected authoritative state. It is document
/// content bounded by rows L3/L4, and folding it in would silently turn a
/// metadata cap into a maximum document size.
///
/// Returns `None` if the entry is not a JSON object, which is a shape error for
/// its caller to report rather than something to score as zero.
///
/// **Wired into the reader as of issue 0029's §7.5 work.** It was written
/// ahead of the members it counts, and deliberately left unenforced while
/// `Package.snapshot` was a bare `Document` with no metadata members to score.
/// The §7.5 snapshot section now carries S2 and S4, so the accounting has
/// something to enforce and `read_package_with_limits` enforces it.
pub fn snapshot_metadata_bytes(entry: &[u8]) -> Option<u64> {
    let value: serde_json::Value = serde_json::from_slice(entry).ok()?;
    let object = value.as_object()?;
    let mut total = 0u64;
    for (name, member) in object {
        // S1 under either name. §5.1 calls the projected state `entities`;
        // this version serialises it as `document`, the named body deviation
        // recorded on `SnapshotSection`. Both are S1, and counting either
        // would turn a metadata cap into a maximum document size — the exact
        // outcome this exclusion exists to prevent. The list collapses to
        // `entities` alone when the §5.1 body lands.
        if name == "entities" || name == "document" {
            continue;
        }
        // Key bytes, the quotes and the colon, then the member itself.
        total = total.saturating_add(name.len() as u64 + 3);
        total = total.saturating_add(serde_json::to_vec(member).ok()?.len() as u64);
    }
    Some(total)
}

/// Bytes that [`check_nesting_depth`] has cleared against row L11.
///
/// It exists to make the guard impossible to skip rather than merely
/// documented as mandatory: [`parse_depth_checked`] stands `serde_json`'s
/// recursion limit down, and nothing else may do that, so the only way to
/// reach it is to have run the check. The field is private to this module and
/// this is the only constructor.
#[derive(Debug, Clone, Copy)]
pub struct DepthChecked<'a>(&'a [u8]);

impl DepthChecked<'_> {
    /// The cleared bytes.
    pub fn bytes(&self) -> &[u8] {
        self.0
    }
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entry_count: Self::L6_ENTRY_COUNT,
            max_entry_bytes: Self::MAX_ENTRY_BYTES,
            max_operations: Self::L23_TOTAL_OPERATIONS,
            max_operations_per_batch: Self::L23_OPERATIONS_PER_BATCH,
            max_nesting_depth: Self::L11_NESTING_DEPTH,
            max_snapshot_metadata_bytes: L23_SNAPSHOT_METADATA_BYTES,
        }
    }
}

/// The schema identifier this build emits (FORMAT-SPEC §8.1 V1).
///
/// A decimal `MAJOR.MINOR` string with no leading zeros, not an integer and
/// not a SemVer triple. §8.1 makes this the SOLE schema identifier: readers
/// never infer a version from content, so whatever appears here is the whole
/// of what a package claims to be.
pub const EMITTED_SCHEMA: &str = "0.1";

/// The schema identifiers this build's reader accepts (FORMAT-SPEC §8.2 V3).
///
/// Distinct from [`EMITTED_SCHEMA`] and from the RELEASED set, which is empty
/// — §8.2 V6 records that no `0.N` schema has been publicly released, so no
/// migration can originate anywhere (§8.2 V4) and none is offered. Conflating
/// the three is the defect issue 0031 fixed in the frozen Go tree under
/// ED-0051; the Rust reader keeps them apart from the start.
pub const SUPPORTED_SCHEMAS: &[&str] = &[EMITTED_SCHEMA];

/// FORMAT-SPEC §7.9: whether a package carries its own asset bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// Asset bytes live outside the package; only the references travel.
    Thin,
    /// Every referenced blob is embedded, so the package verifies standalone.
    Portable,
}

impl Profile {
    /// §7.9's derivation: the profile is a FACT about the package, so it is
    /// computed from what the package actually holds rather than from what a
    /// caller asked for.
    ///
    /// Portable iff every blob of `U(P)` is embedded. Over an EMPTY `U(P)` that
    /// holds vacuously (issue 0064): no blob can fail to embed, so nothing can
    /// break when the package is moved.
    ///
    /// Public and shared deliberately. The writer derives the field with this,
    /// and anything REPORTING a profile must derive it with this too — the
    /// alternative is a second copy of the rule that drifts, which is exactly
    /// how `pack` came to print a profile the manifest contradicted.
    pub fn derive(asset_root: &[AssetRoot], assets: &BTreeMap<String, Vec<u8>>) -> Self {
        if asset_root.iter().all(|r| assets.contains_key(&r.digest)) {
            Profile::Portable
        } else {
            Profile::Thin
        }
    }
}

impl std::fmt::Display for Profile {
    /// The §7.9 token, lowercase, exactly as it appears on the wire.
    ///
    /// Not `Debug`: that renders `Thin`, and a report that says `Thin` where
    /// the format says `thin` invites a reader to believe the capitalisation
    /// is meaningful. The JSON report and the human report are then also
    /// guaranteed to agree, which they did not when one lowercased and the
    /// other did not.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Profile::Thin => "thin",
            Profile::Portable => "portable",
        })
    }
}

/// One member of the §7.4 `assetRoot` table.
///
/// An OBJECT with a `digest`, not a bare string. The distinction is not
/// cosmetic: a member is the place a later version adds provenance or
/// retention fields beside the digest, and `unknown` carries those.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetRoot {
    pub digest: String,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl AssetRoot {
    pub fn new(digest: impl Into<String>) -> Self {
        Self {
            digest: digest.into(),
            unknown: Default::default(),
        }
    }
}

/// A §7.4 `entries` inventory row: what an entry's logical bytes must hash to,
/// and how many of them there are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryIntegrity {
    /// The SHA-256 of the entry's LOGICAL bytes, in `sha256:<hex>` form.
    pub sha256: String,
    pub byte_length: u64,
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// The manifest at `domowik.json` (FORMAT-SPEC §7.3, §7.4).
///
/// Four required fields, and the `entries` inventory among them is what §7.4
/// calls "the authoritative integrity layer": each row carries the SHA-256 and
/// byte length of one entry's logical bytes, so a forged entry surfaces
/// through the digest the manifest lists for it.
///
/// The frontier is NOT here. §7.5 puts the snapshot's S2 frontier inside
/// `snapshot/state.json` and says no other location may carry or shadow it, so
/// a `snapshotFrontier` field in the manifest is a forbidden location rather
/// than a second spelling. See [`SnapshotSection::frontier`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// §8.1 V1's sole schema identifier, a `MAJOR.MINOR` string.
    pub format_version: String,
    /// §7.9's thin/portable distinction.
    pub profile: Profile,
    /// The referenced-blob universe `U(P)`, ascending by digest (§6, §7.4).
    ///
    /// A STABLE package-level table. Invariant I2's point is that a future
    /// version may reference a blob from a field this version drops into an
    /// `unknown` map, and without the table a write-back would garbage-collect
    /// a blob that is still referenced. Required, and `[]` when empty — an
    /// absent or `null` table is not an empty one (ED-0034).
    pub asset_root: Vec<AssetRoot>,
    /// Every other logical entry name, mapped to its digest and byte length.
    pub entries: BTreeMap<String, EntryIntegrity>,
    /// Unrecognised manifest fields, carried verbatim (FORMAT-SPEC §8.4 U2).
    ///
    /// The manifest is where a later version is most likely to add something,
    /// because it is the package's index. Without this map such a field was
    /// read, discarded, and written back absent, with no error — the silent
    /// stripping §8.4 prohibits, at the one entry every reader parses.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

impl Manifest {
    /// The asset-root digests, in table order.
    ///
    /// A convenience over [`Manifest::asset_root`] for callers that only care
    /// about `U(P)` membership. Anything a member carries beside its digest —
    /// including fields a later version added — stays on the member and is not
    /// visible here, so this is a read of the table rather than a substitute
    /// for it.
    pub fn asset_digests(&self) -> Vec<String> {
        self.asset_root.iter().map(|r| r.digest.clone()).collect()
    }

    /// Replaces the table with `digests`, ascending, KEEPING whatever each
    /// surviving member carried beside its digest.
    ///
    /// Rebuilding members from digests alone would discard those fields on
    /// every write-back, which is the §8.4 U2 stripping this crate has just
    /// finished removing elsewhere. A digest that was not in the table before
    /// gets a bare member.
    pub fn set_asset_digests(&mut self, digests: impl IntoIterator<Item = String>) {
        let existing: BTreeMap<String, AssetRoot> = self
            .asset_root
            .drain(..)
            .map(|r| (r.digest.clone(), r))
            .collect();
        let mut next: Vec<AssetRoot> = digests
            .into_iter()
            .map(|d| {
                existing
                    .get(&d)
                    .cloned()
                    .unwrap_or_else(|| AssetRoot::new(d))
            })
            .collect();
        next.sort_by(|a, b| a.digest.cmp(&b.digest));
        next.dedup_by(|a, b| a.digest == b.digest);
        self.asset_root = next;
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            format_version: EMITTED_SCHEMA.to_owned(),
            profile: Profile::Thin,
            asset_root: Vec::new(),
            entries: BTreeMap::new(),
            unknown: Default::default(),
        }
    }
}

/// The snapshot section at `snapshot/state.json` (FORMAT-SPEC §7.5).
///
/// # Placement is conforming; the body is not yet
///
/// §7.5 fixes only the PLACEMENT of the §5.1 snapshot, and that part is right
/// here: exactly one section, at the specified name, carrying the frontier of
/// record. The BODY is §5.1's, and this version serialises S1 as the projected
/// [`Document`] rather than §5.1's `entities` map, and omits S3 winner
/// positions and S5 batch/dedup metadata entirely.
///
/// That is a NAMED deviation, not a claim of conformance. A package written
/// here does not parse as a §5.1 snapshot and the golden fixtures do not parse
/// as this. Closing it needs the §5.1 wire shapes, which §5.1 owns.
///
/// What the move DOES settle is the frontier's location, which §7.5 states
/// unconditionally and which no body change affects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSection {
    /// S1: the projected authoritative state at [`Self::frontier`].
    #[serde(default)]
    pub document: Document,
    /// S2: the frontier this snapshot covers, and the per-actor duplicate
    /// watermark (I2).
    ///
    /// **The frontier of record.** §7.5: it is "simultaneously the coverage
    /// record and the per-actor duplicate watermark, and no other location may
    /// carry or shadow it". An empty frontier means the snapshot covers
    /// nothing and the whole history is in `ops/`, which is how a package
    /// written by this version looks until compaction exists (issue 0027).
    ///
    /// Without it the reader cannot tell a batch the snapshot already applied
    /// from one whose predecessors are missing, and holds the latter forever.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub frontier: BTreeMap<Uuid, u64>,
    /// S4: entity ids the snapshot absorbed a DELETE for.
    ///
    /// A compacted delete leaves no operation behind, so without this list a
    /// later `createEntity` for that id would find nothing blocking it and
    /// resurrect what the user deleted (I3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tombstones: Vec<Uuid>,
    /// Unrecognised snapshot fields, carried verbatim (FORMAT-SPEC §8.4 U2).
    ///
    /// This is also where a conforming package's S3 and S5 members land while
    /// the body deviation above stands, so reading one and writing it back
    /// does not destroy them.
    #[serde(flatten, default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub unknown: serde_json::Map<String, serde_json::Value>,
}

/// Row **L8**: the whole logical name, in UTF-8 bytes.
pub const L8_NAME_LENGTH: usize = 1_024;
/// Row **L9**: one `/`-separated component, in UTF-8 bytes.
pub const L9_NAME_COMPONENT: usize = 255;

/// Validates one logical entry name against FORMAT-SPEC §7.3 and §9.4.
///
/// This is the OUTERMOST check (§9.5). It runs over every name in the central
/// directory before any entry is opened, hashed, or parsed, because a name is
/// the one field an attacker fully controls and the §9.4 table treats it as
/// hostile by default: absolute paths, drive and UNC forms, `.` and `..`
/// segments, mixed separators, empty components, NUL and control characters,
/// invalid UTF-8, overlong names, and case-folding or normalization
/// collisions all reject.
///
/// Nothing here consults host filesystem semantics, and nothing is extracted
/// to a user-chosen path in v0.1. The traversal forms are refused because the
/// format forbids them, not because this particular reader would be harmed by
/// them — a check that depended on the consumer would stop holding the moment
/// a different consumer appeared.
///
/// Returns the §9.4 token nearest the cause, or `None` when the name is
/// admissible.
fn invalid_name_reason(name: &str) -> Option<&'static str> {
    if name.len() > L8_NAME_LENGTH {
        return Some("limit-name-length");
    }
    // Backslashes are a separator on one family of hosts, so a name carrying
    // one is either a mixed-separator form or an attempt to become one.
    if name.contains('\\') {
        return Some("path-traversal");
    }
    if name.starts_with('/') {
        return Some("path-traversal");
    }
    // Drive and UNC forms: `C:`, `C:/x`, `//host/share`.
    let bytes = name.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return Some("path-traversal");
    }
    if name.starts_with("//") {
        return Some("path-traversal");
    }
    if name.is_empty() {
        return Some("invalid-entry-name");
    }
    // A trailing `/` is the ZIP directory-entry form. §7.3's namespace
    // contains no directories, only the entries it lists.
    if name.ends_with('/') {
        return Some("invalid-entry-name");
    }
    for component in name.split('/') {
        if component.is_empty() {
            return Some("invalid-entry-name");
        }
        if component == "." || component == ".." {
            return Some("path-traversal");
        }
        if component.len() > L9_NAME_COMPONENT {
            return Some("limit-name-component");
        }
        // NUL and the C0/C1 control ranges, plus DEL. `char::is_control`
        // covers all of them and nothing else.
        if component.chars().any(char::is_control) {
            return Some("invalid-entry-name");
        }
    }
    None
}

/// The comparison key §7.3 uses for duplicate detection.
///
/// §7.3 compares names AFTER normalization and rejects duplicates, and §9.4
/// lists case-folding and Unicode-normalization collisions among the hostile
/// forms. Two names that differ only in case are therefore a collision even
/// though they are distinct byte strings, which is exactly the trick a
/// `DOMOWIK.JSON` beside a `domowik.json` plays.
///
/// ASCII case folding rather than full Unicode folding: v0.1's namespace is
/// ASCII by construction — fixed names plus hex digests plus canonical UUIDs
/// — so a Unicode-aware fold would add a dependency and a large table to
/// decide cases that cannot occur in a conforming package, while the hostile
/// cases that CAN occur are ASCII.
fn logical_name_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

/// §7.3: the manifest's logical name. Exactly one per package.
const MANIFEST_ENTRY: &str = "domowik.json";
/// §7.5: the snapshot section's logical name. Exactly one per package.
const SNAPSHOT_ENTRY: &str = "snapshot/state.json";
/// §7.3/§7.4: the asset descriptor documents, as ONE object keyed by digest.
const DESCRIPTORS_ENTRY: &str = "assets/descriptors.json";

/// The manifest name written by builds before the §7.3 names were adopted.
///
/// Recognised ONLY so a package carrying it gets
/// [`ContainerError::UnsupportedDevelopmentLayout`] instead of the
/// `missing-manifest` diagnostic it would otherwise draw, which would send a
/// developer looking for a corrupt file rather than an outdated one. It is
/// never read as a manifest: §7.3's namespace is closed and this name is not
/// in it (ED-0108).
const DEVELOPMENT_MANIFEST_ENTRY: &str = "manifest.json";
/// The snapshot name written by those same builds. See above.
const DEVELOPMENT_DOCUMENT_ENTRY: &str = "document.json";

/// FORMAT-SPEC §7.6: `ops/<counterBase>-<actorId>.json`, with the counter
/// zero-padded to 19 digits so byte-comparison of names equals ascending
/// `(counterBase, actorId)`.
const OPS_PREFIX: &str = "ops/";

/// ASSET-STRATEGY §10.3: `blob/<algorithm>/<2-hex>/<62-hex>`.
///
/// Deterministic, injective per digest, and free of filesystem path semantics
/// — there is no traversal surface, because every component is fixed-width hex
/// derived from the digest rather than from any name a user supplied.
const BLOB_PREFIX: &str = "blob/";

/// The digest of `bytes` in the `sha256:<lowercase hex>` form of §10.1.
pub fn digest_of(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

/// The §10.3 logical entry name for a digest, or `None` when the digest is not
/// a well-formed `sha256:<64 lowercase hex>`.
pub fn blob_entry_name(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    if hex.len() != 64
        || !hex
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(format!("{BLOB_PREFIX}sha256/{}/{}", &hex[..2], &hex[2..]))
}

/// Parses a §10.3 blob entry name back into its digest.
fn parse_blob_entry_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix(BLOB_PREFIX)?.strip_prefix("sha256/")?;
    let (head, tail) = rest.split_once('/')?;
    if head.len() != 2 || tail.len() != 62 {
        return None;
    }
    let digest = format!("sha256:{head}{tail}");
    blob_entry_name(&digest).map(|_| digest)
}
const COUNTER_DIGITS: usize = 19;

/// The §7.6 entry name for a batch.
pub fn ops_entry_name(counter_base: u64, actor: Uuid) -> String {
    format!("{OPS_PREFIX}{counter_base:0COUNTER_DIGITS$}-{actor}.json")
}

/// Parses a §7.6 `ops/` entry name into its two identity components.
///
/// `None` when the name is malformed for the namespace — a counter that is not
/// the 19-digit zero-padded decimal, or an actor that is not the canonical
/// 36-character UUID form. §7.3 rejects those as `invalid-entry-name` at name
/// validation, BEFORE the name/content binding check below is reached, which is
/// why this returns an option rather than a binding failure.
fn parse_ops_entry_name(name: &str) -> Option<(u64, Uuid)> {
    let rest = name.strip_prefix(OPS_PREFIX)?.strip_suffix(".json")?;
    let (counter, actor) = rest.split_once('-')?;
    if counter.len() != COUNTER_DIGITS || !counter.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if actor.len() != 36 {
        return None;
    }
    Some((counter.parse().ok()?, Uuid::parse_str(actor).ok()?))
}

/// Why a package could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("cannot save a plan with no levels; add a level before saving")]
    NoLevels,
    #[error("io error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("refusing to save over {path}: {reason}")]
    UnsafeTarget { path: String, reason: &'static str },

    #[error("refusing to replace {path}: the candidate this save wrote is not readable ({source}); the existing file is untouched")]
    CandidateInvalid {
        path: String,
        #[source]
        source: Box<ContainerError>,
    },

    #[error(
        "{digest} is not a well-formed sha256:<64 lowercase hex> digest (ASSET-STRATEGY §10.1)"
    )]
    MalformedDigest { digest: String },

    #[error("a blob declared as {declared} actually hashes to {actual}; the package is corrupt")]
    DigestMismatch { declared: String, actual: String },

    #[error("entry {entry} is not a valid name in the ops/ namespace (FORMAT-SPEC §7.3, §7.6)")]
    InvalidEntryName { entry: String },

    #[error("the session refused the package: {source}")]
    Command {
        /// What the session objected to.
        #[from]
        source: domowik_document::session::CommandError,
    },

    #[error("the package's operation log is malformed: {source}")]
    Log {
        #[source]
        source: LogError,
    },

    #[error("entry {entry} declares counterBase {declared_base} and actor {declared_actor}, which do not match its name (FORMAT-SPEC §7.6 name/content binding)")]
    NameContentMismatch {
        entry: String,
        declared_base: u64,
        declared_actor: Uuid,
    },

    #[error(
        "{path} retains {found} operations; FORMAT-SPEC §9.3 row L23 caps a package at {limit}"
    )]
    OperationCountExceeded {
        path: String,
        found: u64,
        limit: u64,
    },

    #[error("{path} is not a readable .domo package: {source}")]
    Archive {
        path: String,
        #[source]
        source: zip::result::ZipError,
    },

    #[error("{path} is missing the required `{entry}` entry")]
    MissingEntry { path: String, entry: String },

    #[error("{path}: `{entry}` is not valid JSON: {source}")]
    Json {
        path: String,
        entry: String,
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "{path}: declares {count} entries, exceeding FORMAT-SPEC §9.3 row L6 \
         ({limit}); refusing to enumerate"
    )]
    EntryCountExceeded {
        path: String,
        count: u64,
        limit: u32,
    },

    #[error(
        "{path}: entry `{entry}` declares {declared} uncompressed bytes, \
         exceeding the {limit}-byte ceiling; refusing to allocate"
    )]
    EntryTooLarge {
        path: String,
        entry: String,
        declared: u64,
        limit: u64,
    },

    #[error(
        "{path}: declares format version {found}, and this build reads {supported} \
         (FORMAT-SPEC §8.2 V3)"
    )]
    UnsupportedVersion {
        path: String,
        found: String,
        supported: String,
    },

    /// A package written before this project adopted the §7.3 entry names.
    ///
    /// Distinct from [`ContainerError::MissingEntry`] on purpose. A package
    /// with no manifest at all is corrupt or truncated and its owner should go
    /// looking for a damaged file; a package with `manifest.json` was written
    /// by an earlier build of this program and is intact, so telling its owner
    /// the manifest is missing sends them after the wrong problem (ED-0108).
    ///
    /// The layout is not a released schema: it declares no §8.1 identifier, so
    /// nothing can migrate FROM it under §8.2 V4 and none is offered. The
    /// repair is a conversion to a new file, which is why the message names
    /// one rather than suggesting the file be opened again.
    #[error(
        "{path} was written by an earlier development build of Domowik, in a \
         layout this reader does not accept: its manifest is `manifest.json`, \
         and FORMAT-SPEC §7.3 names it `domowik.json`. The file is not \
         damaged. Convert it to a new file with `domowik convert-developer-package \
         {path} <output>`; the original is left untouched."
    )]
    UnsupportedDevelopmentLayout { path: String },

    #[error(
        "{path}: entry `{entry}` is not listed in the manifest's `entries` \
         inventory, which FORMAT-SPEC §7.4 requires to cover every entry"
    )]
    EntryNotInInventory { path: String, entry: String },

    #[error(
        "{path}: entry `{entry}` could not be decoded: {source}. The container \
         verifies a CRC-32 over every entry's stored data (FORMAT-SPEC §7.2), \
         so this is a corrupt package rather than a failing disk"
    )]
    EntryCorrupt {
        path: String,
        entry: String,
        #[source]
        source: std::io::Error,
    },

    #[error(
        "{path}: the snapshot section's metadata occupies {found} bytes, \
         exceeding FORMAT-SPEC §9.3 row L23 ({limit}); refusing to hold it"
    )]
    SnapshotMetadataExceeded {
        path: String,
        found: u64,
        limit: u64,
    },

    #[error("{path}: entry `{entry}` is encrypted, which FORMAT-SPEC §7.2 forbids")]
    EncryptedEntry { path: String, entry: String },

    #[error(
        "{path}: entry `{entry}` uses compression method {method}; FORMAT-SPEC \
         §7.7 allows only store and deflate"
    )]
    UnsupportedMethod {
        path: String,
        entry: String,
        method: String,
    },

    #[error(
        "{path}: {extra} byte(s) follow the end-of-central-directory record; \
         FORMAT-SPEC §9.4 rule T1 rejects trailing or concatenated material"
    )]
    TrailingData { path: String, extra: u64 },

    #[error("{path}: {detail} (FORMAT-SPEC §7.2)")]
    UnsupportedExtension { path: String, detail: &'static str },

    #[error(
        "{path} is open read-only: BEHAVIOR-SPEC §9.6 blocks editing and normal \
         saving while a required asset is unresolved. Unresolved: {assets}. \
         Viewing, diagnostics, `info` and `validate` still work; restore the \
         blob, re-link the file, or remove the referencing instance."
    )]
    ReadOnlyDegraded { path: String, assets: String },

    #[error("{path}: entry name `{entry}` is rejected as {reason} (FORMAT-SPEC §7.3, §9.4)")]
    InvalidName {
        path: String,
        entry: String,
        /// The §9.4 token nearest the cause.
        reason: &'static str,
    },

    #[error(
        "{path}: `{first}` and `{second}` are the same logical name after \
         normalization (duplicate-logical-name)"
    )]
    DuplicateLogicalName {
        path: String,
        first: String,
        second: String,
    },

    #[error("{path}: the manifest lists `{entry}`, which the package does not contain")]
    InventoryEntryMissing { path: String, entry: String },

    #[error(
        "{path}: entry `{entry}` holds {observed} bytes, and the manifest \
         declares {declared} (malformed-length)"
    )]
    MalformedLength {
        path: String,
        entry: String,
        declared: u64,
        observed: u64,
    },

    #[error(
        "{path}: entry `{entry}` hashes to {actual}, and the manifest declares \
         {declared} (checksum-error)"
    )]
    ChecksumError {
        path: String,
        entry: String,
        declared: String,
        actual: String,
    },

    #[error(
        "{path}: `{entry}` nests JSON containers {found} levels deep, exceeding \
         FORMAT-SPEC §9.3 row L11 ({limit}); refusing to parse \
         [limit-nesting-depth]"
    )]
    NestingDepthExceeded {
        path: String,
        entry: String,
        found: u32,
        limit: u32,
    },
}

impl ContainerError {
    /// The FORMAT-SPEC §9.4 rejection class this error reports.
    ///
    /// §9.4's tokens are the format's shared vocabulary for WHY an input was
    /// refused, and the corpus records one per fixture. Without this, a
    /// conformance test can only assert that something was refused — which,
    /// as issue 0029's own history shows, stays green with security guards
    /// deleted, because a fixture rejected for an unrelated reason satisfies
    /// it just as well.
    ///
    /// Errors that are not rejections of an input — an I/O failure, a refusal
    /// to write, a caller mistake — have no §9.4 class and return `None`.
    /// Mapping them to a token would put implementation faults into the
    /// format's vocabulary.
    pub fn category(&self) -> Option<&'static str> {
        use ContainerError::*;
        Some(match self {
            Archive { .. } => "malformed-container",
            MissingEntry { entry, .. } if entry == MANIFEST_ENTRY => "missing-manifest",
            MissingEntry { .. } | InventoryEntryMissing { .. } => "missing-entry",
            EntryNotInInventory { .. } => "entry-not-in-inventory",
            ChecksumError { .. } => "checksum-error",
            MalformedLength { .. } => "malformed-length",
            DigestMismatch { .. } => "missing-asset-equivalence",
            InvalidEntryName { .. } => "invalid-entry-name",
            InvalidName { reason, .. } => reason,
            DuplicateLogicalName { .. } => "duplicate-logical-name",
            EntryCorrupt { .. } => "checksum-error",
            EncryptedEntry { .. } => "encrypted-entry",
            UnsupportedMethod { .. } => "unsupported-method",
            TrailingData { .. } => "trailing-data",
            UnsupportedExtension { .. } => "unsupported-extension",
            NameContentMismatch { .. } | Log { .. } => "malformed-framing",
            UnsupportedVersion { .. } => "unsupported-version",
            UnsupportedDevelopmentLayout { .. } => "unsupported-development-layout",
            EntryCountExceeded { .. } => "limit-entry-count",
            // Issue 0004's row-L11 guard, merged from main. Its own message
            // already carries the token; naming it here keeps the §9.4
            // classification in one place rather than two.
            NestingDepthExceeded { .. } => "limit-nesting-depth",
            SnapshotMetadataExceeded { .. } => "limit-snapshot-metadata",
            EntryTooLarge { .. } => "limit-entry-size",
            OperationCountExceeded { .. } => "limit-operation-count",
            Json { .. } => "malformed-json",
            MalformedDigest { .. } => "invalid-digest",
            // Not rejections of an input.
            Io { .. } | NoLevels | UnsafeTarget { .. } | Command { .. } => return None,
            // An open-mode outcome, not a rejection of the package: §9.6 says
            // the package OPENS, read-only. Giving it a §9.4 class would put a
            // successful open into the vocabulary of refusals.
            ReadOnlyDegraded { .. } => return None,
            // A candidate this writer produced and could not read back. The
            // class belongs to the INNER error: the candidate is the input
            // that was refused, and reporting the wrapper would hide which
            // check refused it.
            CandidateInvalid { source, .. } => return source.category(),
        })
    }
}

/// A package: a manifest, a snapshot section, and the batches after it.
///
/// `snapshot` is the projection at [`Package::frontier`]; `batches` are the
/// batches that follow. [`Package::project`] combines them into the current
/// document, which is what every reader actually wants.
///
/// The frontier and tombstones are FIELDS OF THE PACKAGE rather than of the
/// manifest, because §7.5 places them in the snapshot section and forbids any
/// other location from carrying or shadowing the frontier. They are kept flat
/// here rather than behind a nested `SnapshotSection` so that `pkg.snapshot`
/// stays the document every caller wants; [`SnapshotSection`] is the wire
/// shape the two are assembled into on write.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    pub manifest: Manifest,
    /// S1: the stored projection at [`Self::frontier`].
    pub snapshot: Document,
    /// S2: the frontier the snapshot covers (FORMAT-SPEC §7.5, §5.1).
    pub frontier: BTreeMap<Uuid, u64>,
    /// S4: entity ids the snapshot absorbed a DELETE for.
    pub tombstones: Vec<Uuid>,
    /// Snapshot-section members this version does not interpret, including a
    /// conforming package's S3 and S5, carried verbatim (§8.4 U2).
    pub snapshot_unknown: serde_json::Map<String, serde_json::Value>,
    /// `assets/descriptors.json`: the ASSET-STRATEGY §10.2 descriptor
    /// documents, as ONE object keyed by digest (§7.4).
    ///
    /// Held OPAQUELY, and deliberately. A descriptor's meaning — its extent,
    /// provenance, licence and the `requirement` that BEHAVIOR-SPEC §9.6's
    /// condition matrix keys on — belongs to `domowik-assets`, which already
    /// depends on this crate. Typing it here would invert that and create a
    /// dependency cycle to boot. The container's job at this entry is that the
    /// bytes survive a read and a write, which is what §8.4 U2 requires of an
    /// entry whose contents this crate does not interpret.
    pub descriptors: serde_json::Map<String, serde_json::Value>,
    /// Batches after the snapshot, in §7.6 enumeration order.
    pub batches: Vec<Batch>,
    /// Embedded blobs, keyed by `sha256:<hex>` digest.
    ///
    /// A `BTreeMap` so the write order is deterministic without a sort at
    /// every call site.
    pub assets: BTreeMap<String, Vec<u8>>,
}

impl Package {
    /// A package holding `document` as its snapshot and no batches.
    pub fn new(document: Document) -> Self {
        Self {
            manifest: Manifest::default(),
            snapshot: document,
            frontier: BTreeMap::new(),
            tombstones: Vec::new(),
            snapshot_unknown: Default::default(),
            descriptors: Default::default(),
            batches: Vec::new(),
            assets: BTreeMap::new(),
        }
    }

    /// Builds the operation log this package carries, seeded at the snapshot
    /// frontier.
    ///
    /// Delivery here is where §7.6's structural log checks bite: duplicate
    /// batch ids, overlapping ranges, and a frontier pointing at batches that
    /// are not present. They run BEFORE any replay effect, which is what
    /// `deliver` returning an error rather than a projection guarantees.
    pub fn log(&self) -> Result<OperationLog, ContainerError> {
        let mut log = OperationLog::with_baseline_frontier(&self.frontier)
            .map_err(|source| ContainerError::Log { source })?;
        for batch in &self.batches {
            log.deliver(batch.clone())
                .map_err(|source| ContainerError::Log { source })?;
        }
        log.validate_retained()
            .map_err(|source| ContainerError::Log { source })?;
        Ok(log)
    }

    /// The current document: the snapshot with every batch replayed on top.
    pub fn project(&self) -> Result<Projection, ContainerError> {
        Ok(project_from_with_tombstones(
            &self.snapshot,
            &self.tombstones,
            &self.log()?,
        ))
    }

    /// A compacted copy: the current projection as the snapshot, an empty log,
    /// and the frontier and tombstones needed to keep replaying correctly.
    ///
    /// FORMAT-SPEC §5.1 in the subset this version can implement honestly:
    /// **S2** the frontier the snapshot covers, and **S4** the tombstoned ids.
    /// S1's `createdPositions` and S3's per-field winner positions are NOT
    /// retained, and the operation log refuses any later batch that is
    /// concurrent with the snapshot — which is precisely the case those
    /// positions exist to resolve. See
    /// `OperationLog::check_dominates_snapshot` for the argument.
    ///
    /// Everything the log held is DISCARDED, so undo history from before the
    /// compaction is gone. BEHAVIOR-SPEC R8.4.2 already makes undo stacks
    /// session-local and cleared on reopen, so nothing a user could still have
    /// reached is lost — but the operations themselves are, and that is what
    /// makes this irreversible.
    pub fn compacted(&self) -> Result<Package, ContainerError> {
        let log = self.log()?;
        let projection = self.project()?;

        let mut tombstones: Vec<Uuid> = self
            .tombstones
            .iter()
            .copied()
            .chain(
                projection
                    .intervals
                    .iter()
                    .filter(|(_, i)| i.deleted_at.is_some())
                    .map(|(id, _)| *id),
            )
            .collect();
        tombstones.sort();
        tombstones.dedup();

        Ok(Package {
            manifest: self.manifest.clone(),
            snapshot: projection.document,
            frontier: log.baseline_frontier(),
            tombstones,
            snapshot_unknown: self.snapshot_unknown.clone(),
            descriptors: self.descriptors.clone(),
            batches: Vec::new(),
            assets: self.assets.clone(),
        })
    }
}

/// Whether a plan may be edited, per BEHAVIOR-SPEC §9.6.
///
/// Not a UI state. §9.6 makes the blocked open a property of the DOCUMENT and
/// its assets, so it has to be decided where the package is opened and
/// enforced where mutations enter the core. A front end that merely greyed out
/// its buttons would leave every other caller — the CLI, a script, a future
/// GUI — free to edit and save a plan §9.6 says must not be saved normally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenMode {
    /// Every asset the document REQUIRES resolved.
    Editable,
    /// At least one required asset is unresolved.
    ///
    /// §9.6: "pan, zoom, viewing, and diagnostics work, and CLI `info` and
    /// `validate` operate, while interactive editing and normal saving are
    /// unavailable". Placeholders stay selectable and deletable as GEOMETRY —
    /// hit testing is unaffected — but the delete cannot be committed, which
    /// is why the gate is here rather than in hit testing.
    ReadOnlyDegraded,
}

/// The declared requirement level of an asset (ASSET-STRATEGY §10.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Requirement {
    /// The document cannot be edited without it (§9.6, blocking).
    Required,
    /// Absence degrades rendering only (§9.6, non-blocking).
    Optional,
    /// The asset-root table names the digest and no descriptor declares a
    /// level for it.
    ///
    /// Treated as [`Requirement::Required`] when the blob is also unresolved,
    /// which is the only situation where it changes anything. A package that
    /// references bytes nobody can supply and does not say whether they matter
    /// is not a package to save over; §9.6's matrix has no row for an
    /// undeclared level, so this reader fails closed and says so rather than
    /// picking the permissive reading in silence.
    Undeclared,
}

/// One asset §9.6 could not resolve, and enough to act on it.
///
/// §9.6 requires a diagnostic naming identity, purpose, and remediation per
/// affected asset. This carries identity and purpose; remediation depends on
/// the front end and the retrieval policy, so the wording belongs there.
#[derive(Debug, Clone, PartialEq)]
pub struct UnresolvedAsset {
    /// `sha256:<hex>`, which IS the identity (§9.6: "asset identity is the
    /// content digest").
    pub digest: String,
    /// The declared requirement level, or [`Requirement::Undeclared`].
    pub requirement: Requirement,
    /// Names of the levels whose background images reference it, so the
    /// diagnostic can say where the gap is rather than only what is missing.
    pub used_by: Vec<String>,
}

impl UnresolvedAsset {
    /// Whether this asset blocks an editable open (§9.6).
    pub fn blocks_editing(&self) -> bool {
        matches!(
            self.requirement,
            Requirement::Required | Requirement::Undeclared
        )
    }
}

impl Package {
    /// The §9.6 open mode, and every asset that could not be resolved.
    ///
    /// `resolved` answers whether a digest's bytes are available and usable on
    /// this device. Resolution is deliberately the CALLER's: a portable
    /// package answers from its own embedded blobs, while a thin package needs
    /// the asset cache, which belongs to issue 0026. Deciding the MATRIX here
    /// and the AVAILABILITY there keeps §9.6's policy in one place without
    /// this crate acquiring a cache.
    ///
    /// Digest mismatch is not a separate case: §9.6 opens by saying it "is
    /// treated exactly like a missing blob", so a resolver reports both as
    /// unresolved.
    pub fn open_mode(&self, resolved: impl Fn(&str) -> bool) -> (OpenMode, Vec<UnresolvedAsset>) {
        // Where each digest is used. Built once rather than per digest,
        // because a plan with many levels and many images would otherwise be
        // quadratic for a diagnostic nobody reads on the happy path.
        let mut users: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for level in &self.snapshot.levels {
            for image in &level.background_images {
                users
                    .entry(image.digest.as_str())
                    .or_default()
                    .push(level.name.clone());
            }
        }

        // U(P) is the table PLUS what the document references: a reference
        // this version understands might predate the table, and the table
        // alone is not enough. That is invariant I2's point, and it is the
        // same union `Plan::save` rebuilds.
        let mut universe: Vec<&str> = self
            .manifest
            .asset_root
            .iter()
            .map(|r| r.digest.as_str())
            .collect();
        for digest in users.keys() {
            if !universe.contains(digest) {
                universe.push(digest);
            }
        }
        universe.sort_unstable();
        universe.dedup();

        let mut unresolved = Vec::new();
        for digest in universe {
            if resolved(digest) {
                continue;
            }
            unresolved.push(UnresolvedAsset {
                digest: digest.to_owned(),
                requirement: self.requirement_of(digest),
                used_by: users.get(digest).cloned().unwrap_or_default(),
            });
        }

        let mode = if unresolved.iter().any(UnresolvedAsset::blocks_editing) {
            OpenMode::ReadOnlyDegraded
        } else {
            OpenMode::Editable
        };
        (mode, unresolved)
    }

    /// The §9.6 open mode using this package's own embedded blobs.
    ///
    /// The right answer for a portable package, and the conservative one for a
    /// thin package opened with no cache: an unresolvable required asset is
    /// exactly what §9.6's thin rows describe.
    pub fn open_mode_from_embedded(&self) -> (OpenMode, Vec<UnresolvedAsset>) {
        self.open_mode(|digest| self.assets.contains_key(digest))
    }

    /// The requirement level `assets/descriptors.json` declares for `digest`.
    ///
    /// Reads ONE documented string out of an otherwise opaque document. The
    /// descriptors stay uninterpreted in every other respect — their extent,
    /// provenance and licence belong to `domowik-assets` — but §9.6's matrix
    /// is keyed on `requirement`, and the matrix gates saving, which is this
    /// crate's boundary.
    fn requirement_of(&self, digest: &str) -> Requirement {
        // The descriptor must agree that it describes THESE bytes before its
        // requirement level is worth reading. A document filed under one
        // digest while naming another says "optional" about something else,
        // and trusting it would open a document editable on the strength of a
        // description of different content — a fail-closed bypass, since the
        // undeclared path is the one that blocks.
        //
        // Packages written by this crate cannot contain such an entry
        // (`insert_descriptor` refuses it), but packages are not all written
        // by this crate, and §9.6's gate is exactly where that matters.
        let described = self
            .descriptors
            .get(digest)
            .filter(|d| d.get("digest").and_then(|v| v.as_str()) == Some(digest));
        match described
            .and_then(|d| d.get("requirement"))
            .and_then(|r| r.as_str())
        {
            Some("required") => Requirement::Required,
            Some("optional") => Requirement::Optional,
            // A value that is neither is not a licence to guess. An unknown
            // token means this build cannot tell whether the asset matters,
            // which is the undeclared case.
            _ => Requirement::Undeclared,
        }
    }
}

pub struct Plan {
    /// `None` until the plan has been given a filename.
    path: Option<std::path::PathBuf>,
    /// The manifest as read, carried across a save.
    pub manifest: Manifest,
    /// Embedded blobs, carried across a save unchanged.
    ///
    /// FORMAT-SPEC §7.4 invariant I2: listed blobs are preserved on write-back
    /// or write-back is refused. A `Plan` that dropped them would silently
    /// garbage-collect a background image on the next edit.
    pub assets: std::collections::BTreeMap<String, Vec<u8>>,
    /// Snapshot-section members this version does not interpret, carried
    /// across a save so §8.4 U2 holds through an edit as well as a copy.
    snapshot_unknown: serde_json::Map<String, serde_json::Value>,
    /// The §7.4 descriptor documents, carried across a save unchanged.
    descriptors: serde_json::Map<String, serde_json::Value>,
    /// The §9.6 open mode decided when this plan was opened.
    mode: OpenMode,
    /// Every asset §9.6 could not resolve, for diagnostics.
    unresolved: Vec<UnresolvedAsset>,
    /// The snapshot's S4 tombstones, as opened.
    ///
    /// Carried rather than recomputed because S4 records what a COMPACTION
    /// absorbed, and a session's own deletes live in its operation log instead
    /// — `save` writes the baseline plus that log, so the S4 set it must emit
    /// is the one it opened with. Compaction is where this changes, and
    /// [`Package::compacted`] is where that happens.
    snapshot_tombstones: Vec<Uuid>,
    session: Session,
}

impl Plan {
    /// Opens the package at `path`.
    ///
    /// Lives here rather than in the CLI because it is APPLICATION logic, not
    /// command-line logic: opening a package, choosing the writing actor,
    /// projecting it and writing it back is what every front end needs. The
    /// CLI had the only copy until the GUI needed one too, and a second copy
    /// would have been a second answer to "which actor continues the log".
    ///
    /// The writing actor CONTINUES the log rather than starting a new stream:
    /// the actor with the highest watermark is reused, and a fresh one is
    /// minted only for a package with no batches. FORMAT-SPEC B2 calls an
    /// actor "the single-writer replica epoch", and a CLI editing one file on
    /// one machine is one such epoch. Minting per invocation would also be
    /// conforming, but it grows the frontier by a member on every edit.
    pub fn open(path: &Path) -> Result<Self, ContainerError> {
        Self::open_with_resolution(path, |_| false)
    }

    /// [`Plan::open`] with an explicit answer to "are this digest's bytes
    /// available on this device?".
    ///
    /// The resolver exists because §9.6's matrix is keyed on availability and
    /// availability is not this crate's to know. A portable package answers
    /// from its own embedded blobs, which [`Plan::open`] does by falling back
    /// to the package; a thin package needs the asset cache, which belongs to
    /// issue 0026, and that caller passes its own.
    ///
    /// A digest the resolver rejects is unresolved whether it is absent or
    /// merely unusable: §9.6 treats a digest mismatch "exactly like a missing
    /// blob", and an undecodable blob the same way.
    pub fn open_with_resolution(
        path: &Path,
        resolved: impl Fn(&str) -> bool,
    ) -> Result<Self, ContainerError> {
        Self::from_package(read_package(path)?, Some(path.to_owned()), resolved)
    }

    /// Opens a package held in memory, as [`Plan::open`] opens a file.
    ///
    /// The plan has no path: [`Plan::save`] refuses until [`Plan::save_as`]
    /// gives it one, and a host without a filesystem uses [`Plan::encode`]
    /// instead. `name` labels diagnostics only.
    pub fn open_bytes(name: &str, bytes: &[u8]) -> Result<Self, ContainerError> {
        Self::from_package(
            read_package_bytes(name, bytes, &Limits::default())?,
            None,
            |_| false,
        )
    }

    fn from_package(
        pkg: Package,
        path: Option<std::path::PathBuf>,
        resolved: impl Fn(&str) -> bool,
    ) -> Result<Self, ContainerError> {
        let log = pkg.log()?;
        // Highest watermark wins, and the ACTOR ID breaks a tie. Without the
        // tie-break this is a `max_by_key` over a HashMap, which returns an
        // arbitrary one of the tied entries — so two actors at equal counters
        // would make the writing identity depend on hash iteration order, and
        // the same plan would continue a different stream on each invocation.
        let actor = log
            .watermark()
            .iter()
            .max_by_key(|(&a, &c)| (c, a))
            .map(|(&a, _)| a)
            .unwrap_or_else(Uuid::now_v7);

        let session = Session::open_snapshot(
            actor,
            pkg.snapshot.clone(),
            &pkg.frontier,
            &pkg.tombstones,
            pkg.batches.clone(),
        )?;
        // Embedded blobs always count as resolved; the caller's resolver
        // answers for everything else. Ignoring the embedded ones and
        // deferring wholly to the caller would make a portable package's
        // self-sufficiency depend on a cache it does not need.
        let (mode, unresolved) =
            pkg.open_mode(|digest| pkg.assets.contains_key(digest) || resolved(digest));

        Ok(Self {
            path,
            mode,
            unresolved,
            manifest: pkg.manifest,
            assets: pkg.assets,
            snapshot_unknown: pkg.snapshot_unknown,
            descriptors: pkg.descriptors,
            snapshot_tombstones: pkg.tombstones,
            session,
        })
    }

    /// A plan that is not yet on disk.
    ///
    /// A GUI opens with an empty document long before the user chooses a
    /// filename, so `Plan` must be able to exist without one. `save_as`
    /// adopts a path later; `save` refuses until it has.
    pub fn unsaved(session: Session) -> Self {
        Self {
            path: None,
            // A plan that was never opened references nothing that could be
            // missing, so there is nothing to degrade.
            mode: OpenMode::Editable,
            unresolved: Vec::new(),
            manifest: Manifest::default(),
            assets: BTreeMap::new(),
            snapshot_unknown: Default::default(),
            descriptors: Default::default(),
            snapshot_tombstones: Vec::new(),
            session,
        }
    }

    /// The file this plan reads and writes, or `None` while it is unsaved.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Writes to `path` and adopts it only on success. A failed write preserves
    /// the previous target and dirty state; neither path changes undo/redo.
    pub fn save_as(&mut self, path: &Path) -> Result<(), ContainerError> {
        let previous = self.path.replace(path.to_owned());
        if let Err(error) = self.save() {
            self.path = previous;
            return Err(error);
        }
        Ok(())
    }

    /// The `assets/descriptors.json` documents, as read.
    ///
    /// Opaque here by design: this crate reads exactly one field out of them,
    /// `requirement`, because BEHAVIOR-SPEC §9.6's matrix gates saving and
    /// saving is this crate's boundary. Their extent, provenance and licence
    /// belong to `domowik-assets`, which parses these values into typed
    /// descriptors.
    pub fn descriptors(&self) -> &serde_json::Map<String, serde_json::Value> {
        &self.descriptors
    }

    /// Adds or replaces the descriptor for `digest`.
    ///
    /// FORMAT-SPEC §7.4 says the entry is "keyed by digest", so the key and
    /// the document's own `digest` must agree. A caller that got them out of
    /// step would write a table where looking a digest up returns a descriptor
    /// for other bytes, which is worse than having no descriptor at all —
    /// so it is refused rather than reconciled in either direction.
    ///
    /// Takes effect on the next save, like every other edit to a `Plan`.
    pub fn insert_descriptor(
        &mut self,
        digest: &str,
        document: serde_json::Value,
    ) -> Result<(), ContainerError> {
        if blob_entry_name(digest).is_none() {
            return Err(ContainerError::MalformedDigest {
                digest: digest.to_owned(),
            });
        }
        // A descriptor with NO `digest`, or one that is not a string, used to
        // slip past this check and be filed under the caller's key anyway.
        // §7.4 keys the entry by digest and §10.2 makes the field required, so
        // "absent" is not agreement — it is a document that cannot be shown to
        // describe these bytes, and `requirement_of` would go on to read a
        // requirement level out of it.
        match document.get("digest").and_then(|d| d.as_str()) {
            Some(declared) if declared == digest => {}
            Some(declared) => {
                return Err(ContainerError::DigestMismatch {
                    declared: declared.to_owned(),
                    actual: digest.to_owned(),
                })
            }
            None => {
                return Err(ContainerError::MalformedDigest {
                    digest: format!(
                        "a descriptor filed under {digest} carries no string `digest` of its own"
                    ),
                })
            }
        }
        self.descriptors.insert(digest.to_owned(), document);
        Ok(())
    }

    /// Drops the descriptor for `digest`, returning it if there was one.
    pub fn remove_descriptor(&mut self, digest: &str) -> Option<serde_json::Value> {
        self.descriptors.remove(digest)
    }

    /// The projected document.
    pub fn document(&self) -> &Document {
        self.session.document()
    }

    /// The §9.6 open mode this plan was opened in.
    pub fn mode(&self) -> OpenMode {
        self.mode
    }

    /// Every asset §9.6 could not resolve. Empty when [`Plan::mode`] is
    /// [`OpenMode::Editable`] and the package declares nothing optional that
    /// is also missing.
    pub fn unresolved(&self) -> &[UnresolvedAsset] {
        &self.unresolved
    }

    /// The editing session, for mutation.
    ///
    /// **This is the §9.6 gate**, and it is a `Result` for that reason. Every
    /// mutation of a plan passes through here, so refusing here refuses all of
    /// them — an edit gesture, a delete, an accepted external batch — without
    /// each caller having to remember. Inspection does not come through here:
    /// [`Plan::document`] and [`Plan::session_ref`] stay open, which is what
    /// makes "read-only degraded" a usable mode rather than a closed door.
    ///
    /// Placeholders remain selectable and deletable as geometry, per §9.6.
    /// Hit testing is not gated; committing the delete is.
    pub fn session(&mut self) -> Result<&mut Session, ContainerError> {
        self.require_editable()?;
        Ok(&mut self.session)
    }

    /// The §9.6 check, in one place so the gate and the save agree.
    fn require_editable(&self) -> Result<(), ContainerError> {
        if self.mode == OpenMode::Editable {
            return Ok(());
        }
        let blocking: Vec<String> = self
            .unresolved
            .iter()
            .filter(|a| a.blocks_editing())
            .map(|a| {
                if a.used_by.is_empty() {
                    a.digest.clone()
                } else {
                    format!("{} (used by {})", a.digest, a.used_by.join(", "))
                }
            })
            .collect();
        Err(ContainerError::ReadOnlyDegraded {
            path: self
                .path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "<unsaved>".to_owned()),
            assets: blocking.join("; "),
        })
    }

    /// The editing session, read-only.
    pub fn session_ref(&self) -> &Session {
        &self.session
    }

    /// Writes the package back, snapshot and log together.
    ///
    /// Refuses when the plan has no file yet: silently inventing one would
    /// scatter documents wherever the process happened to be running.
    /// Clears session dirty only after a successful write, preserving history.
    pub fn save(&mut self) -> Result<(), ContainerError> {
        // §9.6: normal saving is unavailable while degraded, and the routed
        // instruction is explicit that there is NO silent fallback to a normal
        // save from a degraded source. Checked here as well as at
        // [`Plan::session`] because a plan can be degraded without ever having
        // been mutated, and saving it would rewrite a package whose required
        // assets this device cannot verify.
        self.require_editable()?;
        let Some(path) = self.path.as_deref() else {
            return Err(ContainerError::UnsafeTarget {
                path: "<unsaved>".to_owned(),
                reason: "this plan has no file yet; use save_as",
            });
        };
        write_package(path, &self.package()?)?;
        self.session.mark_saved();
        Ok(())
    }

    /// The bytes [`Plan::save`] would write, without writing them or changing
    /// the dirty state.
    ///
    /// Refuses exactly what saving refuses (§9.6 degraded plans, a session
    /// without levels). The caller that stores the bytes durably reports so
    /// through [`Plan::mark_saved`]; until then the plan stays dirty.
    pub fn encode(&self) -> Result<Vec<u8>, ContainerError> {
        self.require_editable()?;
        encode_package(&self.package()?)
    }

    /// Records that the bytes last returned by [`Plan::encode`] were stored.
    pub fn mark_saved(&mut self) {
        self.session.mark_saved();
    }

    /// The package a save writes: the baseline plus this session's log.
    fn package(&self) -> Result<Package, ContainerError> {
        // Empty snapshots remain legal replay inputs. It is the materialized
        // editing session, not its historical baseline, that must have a level.
        if self.session.document().levels.is_empty() {
            return Err(ContainerError::NoLevels);
        }
        let mut manifest = self.manifest.clone();
        // The asset-root table enumerates U(P) (§6, §7.4). Rebuilt on every
        // save from what the document references PLUS what the table already
        // held, because invariant I2 requires preserving blobs referenced by
        // data this version does not understand — and a reference living in an
        // entity's `unknown` map is exactly that case.
        let mut roots: Vec<AssetRoot> = manifest.asset_root.clone();
        for level in &self.session.document().levels {
            for image in &level.background_images {
                if !roots.iter().any(|r| r.digest == image.digest) {
                    roots.push(AssetRoot::new(image.digest.clone()));
                }
            }
        }
        // §7.4: ascending by digest. Sorting here rather than trusting the
        // order a caller happened to build is what makes the table's ordering
        // a property of the format instead of a property of the call site.
        roots.sort_by(|a, b| a.digest.cmp(&b.digest));
        roots.dedup_by(|a, b| a.digest == b.digest);
        manifest.asset_root = roots;

        let pkg = Package {
            manifest,
            snapshot: self.session.baseline().clone(),
            // S2 is the frontier the BASELINE covers, which is what the log was
            // seeded with — not `baseline_frontier()`, which includes the
            // batches this session is about to write into `ops/` and would
            // therefore claim the snapshot already absorbed them.
            frontier: self.session.log().baseline_frontier_seed(),
            tombstones: self.snapshot_tombstones.clone(),
            snapshot_unknown: self.snapshot_unknown.clone(),
            descriptors: self.descriptors.clone(),
            batches: self.session.log().batches().to_vec(),
            assets: self.assets.clone(),
        };
        Ok(pkg)
    }

    /// Commits `operations` as one batch and saves.
    pub fn commit_and_save(&mut self, operations: Vec<Operation>) -> Result<(), ContainerError> {
        self.session()?.commit(operations)?;
        self.save()
    }
}

/// The checks before §9.7 S0 creates anything: a structurally malformed log or
/// an embedded blob whose bytes miss its digest never produces a candidate.
fn preflight(package: &Package) -> Result<(), ContainerError> {
    package.log()?;

    // Every embedded blob's digest must match its bytes. A package that
    // claims a digest it does not hold is worse than one missing the blob:
    // a reader trusts the name, and §10.4 makes a portable package's
    // verification claim the whole point of the profile.
    for (digest, bytes) in &package.assets {
        let actual = digest_of(bytes);
        if &actual != digest {
            return Err(ContainerError::DigestMismatch {
                declared: digest.clone(),
                actual,
            });
        }
    }
    Ok(())
}

/// Encodes `package` into memory under [`Limits::default`].
pub fn encode_package(package: &Package) -> Result<Vec<u8>, ContainerError> {
    encode_package_with_limits(package, &Limits::default())
}

/// Encodes `package` into the bytes [`write_package_with_limits`] would write.
///
/// For hosts that hand a file to the user rather than writing one, such as the
/// browser editor (issue 0088). The same preflight, writer and S2 validation
/// run; S0 and S3-S5 do not apply, because there is no target to replace and
/// durability belongs to whatever stores the bytes.
pub fn encode_package_with_limits(
    package: &Package,
    limits: &Limits,
) -> Result<Vec<u8>, ContainerError> {
    const DISPLAY: &str = "<memory>";
    preflight(package)?;
    let bytes = write_archive(std::io::Cursor::new(Vec::new()), package, DISPLAY)?.into_inner();
    let validated = read_package_bytes(DISPLAY, &bytes, limits).and_then(|p| p.project().map(drop));
    if let Err(e) = validated {
        return Err(ContainerError::CandidateInvalid {
            path: DISPLAY.to_owned(),
            source: Box::new(e),
        });
    }
    Ok(bytes)
}

/// Writes `package` to `path`, replacing any existing file.
///
/// Implements the FORMAT-SPEC §9.7 save state machine S0-S5.
///
/// | Step | What it does here |
/// |---|---|
/// | S0 | Unique temporary sibling in the target's directory; a symlink or non-regular target is REJECTED, not followed |
/// | S1 | Stream-write the complete candidate |
/// | S2 | Reopen and validate the candidate with the full reader, INCLUDING projecting its operation log |
/// | S3 | `fsync` the candidate's contents |
/// | S4 | `rename` over the target — atomic within the filesystem |
/// | S5 | `fsync` the parent directory |
///
/// # What is guaranteed, and what is not
///
/// **Namespace atomicity holds everywhere**: a reader sees the old package or
/// the new one, never a mixture, because S0 puts the candidate in the same
/// directory and S4 is a rename within one filesystem. Cross-device `EXDEV` is
/// unreachable by construction, so there is no copy fallback claiming a
/// contract it cannot keep.
///
/// **Process-crash durability holds everywhere.** A crash before S4 leaves the
/// target untouched and the candidate ignorable; a crash after it leaves the
/// new package.
///
/// **Power-loss durability is NOT claimed on macOS.** §9.7's platform profiles
/// validate it for Linux/ext4 and Windows/NTFS after fault-injection suites
/// that do not exist here, and record that Apple documents `F_FULLFSYNC` as
/// best-effort — "data can still be lost in the case of sudden power loss".
/// S3 and S5 are performed regardless because they are what the supported
/// profiles need, and because a best-effort sync is better than none; the
/// claim is what is withheld, not the work.
///
/// Concurrent writers are unsupported: the last completed replace wins, and
/// each outcome is individually valid.
pub fn write_package(path: &Path, package: &Package) -> Result<(), ContainerError> {
    write_package_with_limits(path, package, &Limits::default())
}

/// [`write_package`] under explicit limits, which S2 validates the candidate
/// against.
pub fn write_package_with_limits(
    path: &Path,
    package: &Package,
    limits: &Limits,
) -> Result<(), ContainerError> {
    let io_err = |p: &Path| {
        let p = p.display().to_string();
        move |e: std::io::Error| ContainerError::Io {
            path: p.clone(),
            source: e,
        }
    };

    // Pre-flight, before S0 creates anything. A structurally malformed log
    // must not produce a candidate at all: §7.6 names batch entries by
    // `(counterBase, actorId)`, so two batches reserving one range are not
    // merely invalid — they are physically unrepresentable, and the archive
    // writer would report "duplicate filename" for what is really a framing
    // anomaly. Naming it here means the caller learns what is wrong rather
    // than what the ZIP layer noticed.
    preflight(package)?;

    // S0. A symlink target is REJECTED rather than followed: following one
    // would write through it and replace a file the user did not name, and
    // `rename` over the link would silently delete the link instead of the
    // thing it pointed at. Either is a surprise a save must not produce.
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(ContainerError::UnsafeTarget {
                path: path.display().to_string(),
                reason: "it is a symbolic link; §9.7 S0 requires a regular-file target",
            })
        }
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(ContainerError::UnsafeTarget {
                path: path.display().to_string(),
                reason: "it is not a regular file",
            })
        }
        _ => {}
    }

    let dir = parent_dir(path);
    let tmp = dir.join(temporary_name(path));

    // S1.
    {
        let file = std::fs::File::create(&tmp).map_err(io_err(&tmp))?;
        let file = write_archive(file, package, &tmp.display().to_string())?;

        // S3. Sync the CONTENTS before the rename, so a completed replace can
        // never point at bytes that never reached storage.
        file.sync_all().map_err(io_err(&tmp))?;
    }

    // S2. Reader-equivalent validation of the candidate, INCLUDING projecting
    // its operation log — a package that parses but whose log is structurally
    // malformed is not readable, and discovering that on the next open would
    // mean the failure surfaced after the old copy was already gone.
    //
    // Ordered after S3 rather than before it only because the file must be
    // closed and flushed to be reopened; the §9.7 step ordering is about what
    // each step ESTABLISHES, and nothing here depends on validating before the
    // contents are stable.
    if let Err(e) = validate_candidate(&tmp, limits) {
        let _ = std::fs::remove_file(&tmp);
        return Err(ContainerError::CandidateInvalid {
            path: path.display().to_string(),
            source: Box::new(e),
        });
    }

    // S4.
    std::fs::rename(&tmp, path).map_err(io_err(path))?;

    // S5. Directory-entry durability. POSIX fsync(2) is explicit that syncing
    // a file "does not necessarily ensure that the entry in the directory
    // containing the file has also reached disk". A failure here does NOT roll
    // back: the replace completed and the new package is readable; only the
    // power-loss caveat widens.
    if let Ok(handle) = std::fs::File::open(dir) {
        let _ = handle.sync_all();
    }
    Ok(())
}

/// Writes `package` as an archive into `writer` and returns the writer.
///
/// §9.7 S1 for both [`write_package_with_limits`] and [`encode_package`]: one
/// writer, so a file and an in-memory package are the same bytes. `display`
/// labels diagnostics only.
fn write_archive<W: Write + Seek>(
    writer: W,
    package: &Package,
    display: &str,
) -> Result<W, ContainerError> {
    let mut zip = zip::ZipWriter::new(writer);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // Everything EXCEPT the manifest first, because §7.4's `entries`
    // inventory lists every other entry and must be computed over the
    // exact bytes that get written. Building the manifest before them
    // would mean hashing something other than what lands in the archive.
    let snapshot = SnapshotSection {
        document: package.snapshot.clone(),
        frontier: package.frontier.clone(),
        tombstones: package.tombstones.clone(),
        unknown: package.snapshot_unknown.clone(),
    };
    let mut entries: Vec<(String, Vec<u8>)> = vec![(
        SNAPSHOT_ENTRY.to_owned(),
        serde_json::to_vec_pretty(&snapshot).expect("snapshot is serialisable"),
    )];
    // Written only when there is something to write: §7.3 makes the
    // descriptor entry OPTIONAL, and emitting an empty object would put a
    // row in the inventory of every package that has no assets.
    if !package.descriptors.is_empty() {
        entries.push((
            DESCRIPTORS_ENTRY.to_owned(),
            serde_json::to_vec_pretty(&package.descriptors).expect("descriptors are serialisable"),
        ));
    }
    // §7.6: one JSON document per batch at a deterministic logical name.
    // Sorted so the archive's entry order already equals the enumeration
    // order a reader will use.
    let mut ops: Vec<(String, Vec<u8>)> = package
        .batches
        .iter()
        .map(|b| {
            (
                ops_entry_name(b.counter_base, b.actor_id),
                serde_json::to_vec(b).expect("batch is serialisable"),
            )
        })
        .collect();
    ops.sort_by(|a, b| a.0.cmp(&b.0));
    entries.append(&mut ops);

    // Blobs, at their §10.3 logical names. A digest that does not map to a
    // name is a caller error rather than a silent omission: writing the
    // blob somewhere else would make it unreachable.
    for (digest, bytes) in &package.assets {
        let Some(name) = blob_entry_name(digest) else {
            return Err(ContainerError::MalformedDigest {
                digest: digest.clone(),
            });
        };
        entries.push((name, bytes.clone()));
    }

    // §7.4's authoritative integrity layer, over the logical bytes of
    // every entry the manifest does not describe itself. The manifest is
    // excluded because a digest of the manifest cannot live inside it; it
    // is protected by the container CRC-32 and the §7.2 structural checks
    // instead, and a forged manifest surfaces through the digests it
    // mislists.
    let mut manifest = package.manifest.clone();
    manifest.entries = entries
        .iter()
        .map(|(name, bytes)| {
            (
                name.clone(),
                EntryIntegrity {
                    sha256: digest_of(bytes),
                    byte_length: bytes.len() as u64,
                    unknown: Default::default(),
                },
            )
        })
        .collect();
    // §7.9: the profile is a fact about the package, not a preference. A
    // package embedding every blob its table names verifies standalone and
    // is portable; one that does not is thin.
    // §7.9, via the one shared derivation (issue 0064). The earlier
    // `!asset_root.is_empty()` guard sent the empty case to Thin through a
    // condition §7.9 does not state.
    manifest.profile = Profile::derive(&manifest.asset_root, &package.assets);
    entries.insert(
        0,
        (
            MANIFEST_ENTRY.to_owned(),
            serde_json::to_vec_pretty(&manifest).expect("manifest is serialisable"),
        ),
    );

    for (name, bytes) in entries {
        zip.start_file(name, opts)
            .map_err(|e| ContainerError::Archive {
                path: display.to_owned(),
                source: e,
            })?;
        zip.write_all(&bytes).map_err(|e| ContainerError::Io {
            path: display.to_owned(),
            source: e,
        })?;
    }
    zip.finish().map_err(|e| ContainerError::Archive {
        path: display.to_owned(),
        source: e,
    })
}

/// The directory holding `path`, as a path that can be OPENED.
///
/// `Path::parent` returns `Some("")` for a bare filename, and joining onto
/// that still produces a working relative path — so the writer works either
/// way and no save-and-read test can tell the difference. What it changes is
/// S5: `File::open("")` fails, so the directory sync silently does nothing and
/// the durability step the platform profiles require is skipped. That is why
/// this is a named function with its own test rather than an inline
/// `unwrap_or`.
fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    }
}

/// A collision-safe temporary name in the target's directory (§9.7 S0).
///
/// Process id and a monotonic counter: two saves from one process, or from two
/// processes, must not choose the same candidate path. The old fixed
/// `.{name}.tmp` did, so a concurrent save could interleave two byte streams
/// into one file and then rename the result over the user's plan.
fn temporary_name(path: &Path) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let stem = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("domowik");
    format!(".{stem}.{}.{n}.tmp", std::process::id())
}

/// Where a converted manifest records fields whose names §7.4 now uses for
/// something else.
///
/// Not a §7.4 field, and not meant to be read by anything: it exists so the
/// conversion can honour ED-0108's "nothing dropped silently" without emitting
/// a key twice. A reader treats it as ordinary unknown data and preserves it.
const DISPLACED_BY_CONVERSION: &str = "displacedByDevelopmentLayoutConversion";

/// The manifest an earlier development build wrote, at `manifest.json`.
///
/// Parsed ONLY by [`convert_development_package`]. It is not a schema: it
/// declares no §8.1 identifier — its `formatVersion` is an integer, and §8.1
/// V1 makes the identifier a `MAJOR.MINOR` string — so no §8.2 V4 migration
/// can originate at it and none is claimed. Reading it is a repair of an
/// unpublished layout, which is why it lives beside the converter rather than
/// beside [`Manifest`].
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DevelopmentManifest {
    #[serde(default)]
    snapshot_frontier: BTreeMap<Uuid, u64>,
    #[serde(default)]
    assets: Vec<String>,
    #[serde(default)]
    tombstones: Vec<Uuid>,
    /// Everything else, INCLUDING `formatVersion` and `generator`.
    ///
    /// Neither has a home in a §7.4 manifest: the first is replaced by the
    /// §8.1 identifier and the second is not a §7.4 field at all. ED-0108
    /// forbids dropping a field silently, so both are carried into the
    /// converted manifest's `unknown` map rather than discarded — which also
    /// means a converted package still records which build wrote the original.
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

/// Converts a package written by an earlier development build to a §7 package
/// at `destination`, leaving `source` untouched.
///
/// # Why this is not a migration
///
/// FORMAT-SPEC §8.2 V4 lets a migration step originate only at a RELEASED
/// schema, and the released set is empty (§8.2 V6): no `0.N` schema has been
/// publicly released, there is no tag, and the developer layout declares no
/// §8.1 identifier to originate from. Calling this a migration would fabricate
/// a release relationship and make the §8.2 chain look fillable when it is
/// not. ED-0108 names it an **unpublished-layout repair**, and so does this.
///
/// # What it guarantees
///
/// - **`source` is never written, moved, or removed**, on any path including
///   failure. The only file this function creates is `destination`.
/// - **`destination` is never overwritten.** A conversion that silently
///   replaced a file would be the data loss the whole exercise exists to
///   avoid; an existing destination is an error, and choosing another name is
///   the caller's decision.
/// - **Nothing is dropped silently.** The old `formatVersion` and `generator`
///   have no §7.4 home and are carried into the manifest's `unknown` map;
///   unknown fields at every envelope survive because the envelopes preserve
///   them; descriptors and blobs are carried whole.
/// - **The result is a normal package.** It is written through
///   [`write_package`], so it gets the §9.7 S0-S5 machine and the S2
///   validation that reads the candidate back before any replacement. A
///   conversion that cannot produce a readable package fails instead of
///   producing one.
///
/// It is never invoked on open. §7.3's namespace is closed and `manifest.json`
/// is not in it, so the reader refuses such a package with
/// [`ContainerError::UnsupportedDevelopmentLayout`] and the owner decides
/// whether to convert.
pub fn convert_development_package(
    source: &Path,
    destination: &Path,
) -> Result<(), ContainerError> {
    convert_development_package_with_limits(source, destination, &Limits::default())
}

/// [`convert_development_package`] under explicit limits.
pub fn convert_development_package_with_limits(
    source: &Path,
    destination: &Path,
    limits: &Limits,
) -> Result<(), ContainerError> {
    if destination.exists() {
        return Err(ContainerError::UnsafeTarget {
            path: destination.display().to_string(),
            reason: "it already exists; conversion never overwrites, choose another name",
        });
    }
    if source == destination {
        return Err(ContainerError::UnsafeTarget {
            path: destination.display().to_string(),
            reason: "conversion writes a NEW file and never edits the original in place",
        });
    }

    let package = read_development_package(source, limits)?;
    write_package_with_limits(destination, &package, limits)
}

/// Reads a development-layout package into the canonical in-memory shape.
///
/// Shares every part of the reader that is not the two entry names: the L6
/// count guard, the per-entry size guards, §7.6 name parsing and name/content
/// binding, the L23 envelopes, and blob digest verification. Only the manifest
/// and snapshot entries are read from the old names and the old shapes, which
/// is the whole of what the layout changed.
fn read_development_package(path: &Path, limits: &Limits) -> Result<Package, ContainerError> {
    let display = path.display().to_string();
    let mut file = std::fs::File::open(path).map_err(|e| ContainerError::Io {
        path: display.clone(),
        source: e,
    })?;
    // Row L6 BEFORE the archive is constructed: `ZipArchive::new` parses every
    // central-directory header, so a guard placed after it is a range check on
    // work already done rather than a limit on work to be done.
    guard_declared_entry_count(&mut file, &display, limits)?;
    // T1 before the archive as well: an ambiguous trailer must not be resolved
    // by whichever record the ZIP layer happens to prefer.
    guard_trailing_data(&mut file, &display)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ContainerError::Archive {
        path: display.clone(),
        source: e,
    })?;

    let count = archive.len() as u64;
    if !limits.permits_entry_count(count) {
        return Err(ContainerError::EntryCountExceeded {
            path: display,
            count,
            limit: limits.max_entry_count,
        });
    }

    // No `entries` inventory existed in this layout, so there is none to
    // verify. The converted package gets one, computed by the writer.
    let old: DevelopmentManifest =
        read_json_entry(&mut archive, DEVELOPMENT_MANIFEST_ENTRY, &display, limits)?;
    let snapshot: Document =
        read_json_entry(&mut archive, DEVELOPMENT_DOCUMENT_ENTRY, &display, limits)?;

    // A carried field must not collide with a §7.4 field of the same name.
    // The old layout's `formatVersion` is an integer and the new one is the
    // §8.1 string, so leaving it in the unknown map would emit the key twice
    // and produce a manifest that does not parse — which S2 catches, but as a
    // "duplicate field" rather than as what it is.
    //
    // ED-0108 forbids dropping it, so it is MOVED rather than discarded:
    // everything that collides goes into one nested object whose name says
    // where it came from, and the rest stays where it was.
    let mut carried = old.rest;
    let mut displaced = serde_json::Map::new();
    for reserved in ["formatVersion", "profile", "assetRoot", "entries"] {
        if let Some(value) = carried.remove(reserved) {
            displaced.insert(reserved.to_owned(), value);
        }
    }
    if !displaced.is_empty() {
        carried.insert(
            DISPLACED_BY_CONVERSION.to_owned(),
            serde_json::Value::Object(displaced),
        );
    }

    let mut manifest = Manifest {
        format_version: EMITTED_SCHEMA.to_owned(),
        profile: Profile::Thin,
        asset_root: old.assets.iter().cloned().map(AssetRoot::new).collect(),
        entries: BTreeMap::new(),
        unknown: carried,
    };
    manifest.asset_root.sort_by(|a, b| a.digest.cmp(&b.digest));
    manifest.asset_root.dedup_by(|a, b| a.digest == b.digest);

    let (batches, assets) = read_ops_and_blobs(&mut archive, &display, limits)?;

    Ok(Package {
        manifest,
        snapshot,
        frontier: old.snapshot_frontier,
        tombstones: old.tombstones,
        snapshot_unknown: Default::default(),
        descriptors: Default::default(),
        batches,
        assets,
    })
}

/// S2: full reader-equivalent validation of a written candidate.
fn validate_candidate(path: &Path, limits: &Limits) -> Result<(), ContainerError> {
    let package = read_package_with_limits(path, limits)?;
    package.project()?;
    Ok(())
}

/// Reads the package at `path`, enforcing [`Limits`] before allocating.
pub fn read_package(path: &Path) -> Result<Package, ContainerError> {
    read_package_with_limits(path, &Limits::default())
}

/// Reads the package at `path` under explicit `limits`.
pub fn read_package_with_limits(path: &Path, limits: &Limits) -> Result<Package, ContainerError> {
    let display = path.display().to_string();
    let file = std::fs::File::open(path).map_err(|e| ContainerError::Io {
        path: display.clone(),
        source: e,
    })?;
    read_package_from(file, display, limits)
}

/// Reads a package held in memory, under the same checks as
/// [`read_package_with_limits`].
///
/// For hosts without a filesystem, such as the browser editor (issue 0088),
/// where a file arrives as bytes from a picker. `name` labels diagnostics in
/// place of a path; it is never opened.
pub fn read_package_bytes(
    name: &str,
    bytes: &[u8],
    limits: &Limits,
) -> Result<Package, ContainerError> {
    read_package_from(std::io::Cursor::new(bytes), name.to_owned(), limits)
}

/// The one reader behind [`read_package_with_limits`] and
/// [`read_package_bytes`], so a file and the same bytes in memory pass
/// exactly the same checks in the same order.
fn read_package_from<R: Read + Seek>(
    mut file: R,
    display: String,
    limits: &Limits,
) -> Result<Package, ContainerError> {
    // Row L6 BEFORE the archive is constructed: `ZipArchive::new` parses every
    // central-directory header, so a guard placed after it is a range check on
    // work already done rather than a limit on work to be done.
    guard_declared_entry_count(&mut file, &display, limits)?;
    // T1 before the archive as well: an ambiguous trailer must not be resolved
    // by whichever record the ZIP layer happens to prefer.
    guard_trailing_data(&mut file, &display)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| ContainerError::Archive {
        path: display.clone(),
        source: e,
    })?;

    // L6 again, over what was actually parsed.
    //
    // **This cannot currently fire, and saying so is the point.** It would
    // catch a trailer whose declared count disagreed with the directory that
    // was built from it — but `zip` reads exactly `number_of_files` headers
    // from the same trailer this reader already checked, so the two numbers
    // agree by construction and removing this changes no test. It is retained
    // as a guard against that dependency changing, NOT counted as coverage:
    // the load-bearing L6 check is `guard_declared_entry_count` above, which
    // is the one a mutation test reddens.
    let count = archive.len() as u64;
    if !limits.permits_entry_count(count) {
        return Err(ContainerError::EntryCountExceeded {
            path: display,
            count,
            limit: limits.max_entry_count,
        });
    }

    // §9.5: name validation is the OUTERMOST check. Every name in the central
    // directory is validated, and duplicates detected, BEFORE any entry is
    // opened, hashed or parsed — a hostile name must never reach a stage that
    // acts on it.
    validate_entry_names(&archive, &display)?;
    // §7.2/§7.7, still before any entry is decoded.
    validate_entry_profile(&mut archive, &display)?;

    // Before anything is parsed: is this the developer layout? A package
    // written by a build predating the §7.3 names has no `domowik.json`, so
    // every check below would report a missing manifest and send its owner
    // looking for a corrupt file. ED-0108 requires an actionable diagnostic
    // instead, and requires that the old names are never READ as a manifest —
    // §7.3's namespace is closed and neither name is in it.
    if archive.by_name(MANIFEST_ENTRY).is_err()
        && archive.by_name(DEVELOPMENT_MANIFEST_ENTRY).is_ok()
        && archive.by_name(DEVELOPMENT_DOCUMENT_ENTRY).is_ok()
    {
        return Err(ContainerError::UnsupportedDevelopmentLayout { path: display });
    }

    let manifest: Manifest = read_json_entry(&mut archive, MANIFEST_ENTRY, &display, limits)?;
    // §8.2 V3: a reader accepts exactly the schemas it claims to support, and
    // names both versions when it refuses. There is no ordering here and no
    // "newer than" comparison to make — §8.1 V1 makes the identifier an opaque
    // label a writer STATES, so membership is the only honest test.
    if !SUPPORTED_SCHEMAS.contains(&manifest.format_version.as_str()) {
        return Err(ContainerError::UnsupportedVersion {
            path: display,
            found: manifest.format_version,
            supported: SUPPORTED_SCHEMAS.join(", "),
        });
    }

    // §7.4's authoritative integrity layer, BEFORE anything is parsed for
    // meaning. Structural integrity precedes semantics: a forged entry should
    // surface as the digest the manifest mislists, not as whatever its
    // contents happen to deserialise into.
    verify_inventory(&mut archive, &manifest, &display, limits)?;

    // Row L23's snapshot-metadata cap, over the DECOMPRESSED entry and before
    // the section is turned into owned structures. The cap bounds what a reader
    // must HOLD, so it has to be judged on the bytes rather than on the parsed
    // result — measuring afterwards would mean allocating the thing the limit
    // exists to refuse. S1 is excluded, so this caps metadata and not document
    // size; see `snapshot_metadata_bytes`.
    let snapshot_bytes = read_entry_bytes(&mut archive, SNAPSHOT_ENTRY, &display, limits)?;
    if let Some(found) = snapshot_metadata_bytes(&snapshot_bytes) {
        if found > limits.max_snapshot_metadata_bytes {
            return Err(ContainerError::SnapshotMetadataExceeded {
                path: display,
                found,
                limit: limits.max_snapshot_metadata_bytes,
            });
        }
    }
    drop(snapshot_bytes);

    let snapshot_section: SnapshotSection =
        read_json_entry(&mut archive, SNAPSHOT_ENTRY, &display, limits)?;

    // Optional per §7.3: a package with no assets has no descriptors, and an
    // absent derived or optional entry never fails anything.
    let descriptors: serde_json::Map<String, serde_json::Value> =
        if archive.by_name(DESCRIPTORS_ENTRY).is_ok() {
            read_json_named_entry(&mut archive, DESCRIPTORS_ENTRY, &display, limits)?
        } else {
            Default::default()
        };

    let (batches, assets) = read_ops_and_blobs(&mut archive, &display, limits)?;

    Ok(Package {
        manifest,
        snapshot: snapshot_section.document,
        frontier: snapshot_section.frontier,
        tombstones: snapshot_section.tombstones,
        snapshot_unknown: snapshot_section.unknown,
        descriptors,
        batches,
        assets,
    })
}

/// Checks every entry against the §7.4 `entries` inventory.
///
/// The inventory is "the authoritative integrity layer": it lists EVERY other
/// logical entry name with the SHA-256 and byte length of that entry's logical
/// bytes. Three things are therefore wrong with a package, and each has to be
/// caught separately:
///
/// - an entry the inventory lists but the archive does not hold,
/// - an entry the archive holds but the inventory does not list, which is how
///   an attacker would smuggle one past a manifest that looks complete,
/// - an entry whose bytes do not hash to what the inventory claims.
///
/// The manifest itself is excluded: a digest of the manifest cannot live
/// inside it. §7.4 covers it with the container CRC-32 and the §7.2 structural
/// checks, and makes a forged manifest surface through the digests it mislists
/// — which is exactly what the first two checks above do.
///
/// Entries are streamed and hashed without being retained, so the cost is one
/// extra read of each entry and a bounded amount of memory, not a second copy
/// of the package.
fn verify_inventory<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    manifest: &Manifest,
    path: &str,
    limits: &Limits,
) -> Result<(), ContainerError> {
    use sha2::{Digest, Sha256};

    let present: Vec<String> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|n| n.to_owned()))
        .filter(|n| n != MANIFEST_ENTRY)
        .collect();

    for name in &present {
        if !manifest.entries.contains_key(name) {
            return Err(ContainerError::EntryNotInInventory {
                path: path.to_owned(),
                entry: name.clone(),
            });
        }
    }

    for (name, expected) in &manifest.entries {
        let Ok(mut entry) = archive.by_name(name) else {
            return Err(ContainerError::InventoryEntryMissing {
                path: path.to_owned(),
                entry: name.clone(),
            });
        };
        // The declared size is checked before anything is read, so a header
        // claiming 40 GB costs a comparison. The reader is then capped as
        // well, because a header that lies low is the other half of the same
        // attack.
        if !limits.permits_entry_size(entry.size()) {
            return Err(ContainerError::EntryTooLarge {
                path: path.to_owned(),
                entry: name.clone(),
                declared: entry.size(),
                limit: limits.max_entry_bytes,
            });
        }
        let mut hasher = Sha256::new();
        let mut observed: u64 = 0;
        let mut buf = [0u8; 64 * 1024];
        loop {
            // `InvalidData` from a file this reader already opened is the
            // CONTENT failing, not the device: the `zip` crate raises it for a
            // CRC-32 mismatch and for a corrupt deflate stream, both of which
            // are §7.2 integrity failures. Reporting them as `io error on
            // <path>` told a user their disk was broken when their file was.
            //
            // Matched on `ErrorKind` rather than on the message text, so it
            // does not break the next time the dependency rewords itself.
            let read = entry.read(&mut buf).map_err(|e| {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    ContainerError::EntryCorrupt {
                        path: path.to_owned(),
                        entry: name.clone(),
                        source: e,
                    }
                } else {
                    ContainerError::Io {
                        path: path.to_owned(),
                        source: e,
                    }
                }
            })?;
            if read == 0 {
                break;
            }
            observed += read as u64;
            if observed > limits.max_entry_bytes {
                return Err(ContainerError::EntryTooLarge {
                    path: path.to_owned(),
                    entry: name.clone(),
                    declared: observed,
                    limit: limits.max_entry_bytes,
                });
            }
            hasher.update(&buf[..read]);
        }
        // §7.4 C3: a byte-length contradiction between the inventory and the
        // observed growth is `malformed-length`, and declared sizes never
        // drive allocation. Checked before the digest so the diagnostic names
        // the simpler fault when both are wrong.
        if observed != expected.byte_length {
            return Err(ContainerError::MalformedLength {
                path: path.to_owned(),
                entry: name.clone(),
                declared: expected.byte_length,
                observed,
            });
        }
        let actual = format!("sha256:{:x}", hasher.finalize());
        if actual != expected.sha256 {
            return Err(ContainerError::ChecksumError {
                path: path.to_owned(),
                entry: name.clone(),
                declared: expected.sha256.clone(),
                actual,
            });
        }
    }
    Ok(())
}

/// The entry count the archive's own trailer DECLARES, read without parsing
/// the central directory.
///
/// # Why this exists rather than `ZipArchive::len()`
///
/// Row L6 is a pre-allocation guard: it must refuse an entry-count explosion
/// BEFORE anything proportional to that count is parsed or reserved. The
/// obvious implementation — construct the archive, then compare `len()` — does
/// not do that. `zip::ZipArchive::new` parses every central-directory header
/// into a `Vec` first (zip 2.4.2 `src/read.rs`, the `for _ in
/// 0..dir_info.number_of_files` loop in `get_metadata`), so by the time
/// `len()` can be asked, the work the guard exists to prevent has already
/// happened. A guard placed there fires only on an archive that genuinely
/// holds the entries, which is the case that no longer needs guarding.
///
/// This reads the end-of-central-directory record directly, takes the declared
/// count from it, and consults the Zip64 record when the 16-bit field is
/// saturated. An attacker's claimed count is then a number this reader has
/// compared before the `zip` crate is handed anything.
///
/// `None` when no trailer can be located. That is not this function's error to
/// report: the archive is malformed, and `ZipArchive::new` says so with a
/// better diagnostic than a count-shaped one.
fn declared_entry_count<R: Read + Seek>(file: &mut R) -> Option<u64> {
    use std::io::SeekFrom;

    const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const ZIP64_LOCATOR_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
    const ZIP64_EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
    /// The EOCD is 22 bytes plus a comment of at most 65,535, so it cannot
    /// begin earlier than this from the end of the file.
    const MAX_TRAILER: u64 = 22 + 65_535;

    let length = file.seek(SeekFrom::End(0)).ok()?;
    let window = length.min(MAX_TRAILER);
    let start = length - window;
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut tail = vec![0u8; window as usize];
    file.read_exact(&mut tail).ok()?;

    // Backwards: the LAST end-of-central-directory record is the operative
    // one, and scanning forwards would stop at a signature that happens to sit
    // inside an entry's compressed data.
    let eocd = (0..=tail.len().checked_sub(22)?)
        .rev()
        .find(|&i| tail[i..i + 4] == EOCD_SIGNATURE)?;

    let count16 = u16::from_le_bytes([tail[eocd + 10], tail[eocd + 11]]);
    if count16 != u16::MAX {
        return Some(u64::from(count16));
    }

    // Saturated, so the real count is in the Zip64 record, reached through the
    // locator that immediately precedes the EOCD.
    let locator = eocd.checked_sub(20)?;
    if tail[locator..locator + 4] != ZIP64_LOCATOR_SIGNATURE {
        return Some(u64::from(count16));
    }
    let zip64_offset = u64::from_le_bytes(tail[locator + 8..locator + 16].try_into().ok()?);

    file.seek(SeekFrom::Start(zip64_offset)).ok()?;
    let mut record = [0u8; 40];
    file.read_exact(&mut record).ok()?;
    if record[..4] != ZIP64_EOCD_SIGNATURE {
        return Some(u64::from(count16));
    }
    // Offset 32 within the Zip64 EOCD: total entries in the central directory.
    Some(u64::from_le_bytes(record[32..40].try_into().ok()?))
}

/// Rejects trailing data after the end-of-central-directory record (T1).
///
/// §9.4's fail-closed rule T1: bytes after the EOCD make the archive
/// ambiguous, because readers disagree about which trailer is operative and a
/// signature over "the file" then covers whichever one a given reader did not
/// use.
///
/// The check is `end of EOCD record + its declared comment == end of file`.
/// The comment length is the EOCD's own last field, so a comment cannot smuggle
/// bytes past this — and §7.2 rejects a non-empty comment outright anyway,
/// which is checked here because this is where the field is already in hand.
fn guard_trailing_data<R: Read + Seek>(probe: &mut R, display: &str) -> Result<(), ContainerError> {
    use std::io::SeekFrom;
    const EOCD_SIGNATURE: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
    const MAX_TRAILER: u64 = 22 + 65_535;

    let Ok(length) = probe.seek(SeekFrom::End(0)) else {
        return Ok(());
    };
    let window = length.min(MAX_TRAILER);
    if probe.seek(SeekFrom::Start(length - window)).is_err() {
        return Ok(());
    }
    let mut tail = vec![0u8; window as usize];
    if probe.read_exact(&mut tail).is_err() {
        return Ok(());
    }
    let Some(base) = tail.len().checked_sub(22) else {
        return Ok(());
    };
    // A missing EOCD is not this check's business; `ZipArchive::new` reports
    // it, with a better diagnostic than a trailing-data one.
    let Some(eocd) = (0..=base).rev().find(|&i| tail[i..i + 4] == EOCD_SIGNATURE) else {
        return Ok(());
    };

    let comment_len = usize::from(u16::from_le_bytes([tail[eocd + 20], tail[eocd + 21]]));
    if comment_len != 0 {
        return Err(ContainerError::UnsupportedExtension {
            path: display.to_owned(),
            detail: "the archive carries a non-empty comment, which this profile forbids",
        });
    }
    let record_end = eocd + 22 + comment_len;
    if record_end != tail.len() {
        return Err(ContainerError::TrailingData {
            path: display.to_owned(),
            extra: (tail.len() - record_end) as u64,
        });
    }

    // PREPENDED or CONCATENATED material, which the check above cannot see.
    //
    // Appending a whole second package leaves the LAST end-of-central-directory
    // record exactly at the end of the file, so nothing trails it. The first
    // package becomes leading data instead, and the archive is ambiguous in the
    // other direction: a reader scanning backwards sees the second package and
    // one scanning forwards sees the first. T1 exists for precisely that
    // disagreement.
    //
    // It is detectable because the EOCD's directory offset and size are
    // relative to the start of the archive, so in a single package they sum to
    // the EOCD's own position. In a concatenated file the second package's
    // offset is relative to ITS start, and the sum lands short of the record by
    // however many bytes precede it.
    //
    // Saturated 32-bit fields mean Zip64, where the real values live in a
    // record this function does not read; the count guard already refuses the
    // entry counts that make Zip64 reachable in v0.1, so the check is skipped
    // rather than guessed at.
    let directory_size =
        u32::from_le_bytes(tail[eocd + 12..eocd + 16].try_into().ok().unwrap_or([0; 4]));
    let directory_offset =
        u32::from_le_bytes(tail[eocd + 16..eocd + 20].try_into().ok().unwrap_or([0; 4]));
    if directory_size != u32::MAX && directory_offset != u32::MAX {
        let expected = u64::from(directory_offset) + u64::from(directory_size);
        // `eocd` is an index into the tail window, so the record's absolute
        // position is the window's start plus that index.
        let actual = (length - window) + eocd as u64;
        // Only when the sum falls SHORT of the record, which means bytes
        // precede the archive. The other direction — a directory claiming to
        // extend past its own trailer — is a truncated or contradictory
        // archive, and calling that "trailing data" would name the wrong
        // fault; `ZipArchive::new` reports it as malformed, which it is.
        if expected < actual {
            return Err(ContainerError::TrailingData {
                path: display.to_owned(),
                extra: actual - expected,
            });
        }
    }
    Ok(())
}

/// §7.2 and §7.7 per-entry profile rules, over the central directory.
///
/// - **No encryption** (§7.2). This profile has no key material and never
///   will, so an encrypted entry is either a different format or an attempt to
///   hide bytes from the integrity layer.
/// - **Compression allowlist** (§7.7): exactly store and deflate. Every other
///   method rejects, which is what stops a reader being pushed onto a decoder
///   the format never specified.
///
/// `by_index_raw` rather than `by_index`, because this must inspect an entry's
/// HEADER without decoding it — deciding whether a method is allowed by trying
/// to decompress with it would be the wrong way round.
fn validate_entry_profile<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    path: &str,
) -> Result<(), ContainerError> {
    for index in 0..archive.len() {
        let Ok(entry) = archive.by_index_raw(index) else {
            // Unreadable headers belong to `ZipArchive::new`'s diagnostics;
            // this pass judges only entries it can see.
            continue;
        };
        let name = entry.name().to_owned();
        let encrypted = entry.encrypted();
        let method = entry.compression();
        drop(entry);

        if encrypted {
            return Err(ContainerError::EncryptedEntry {
                path: path.to_owned(),
                entry: name,
            });
        }
        if !matches!(
            method,
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            return Err(ContainerError::UnsupportedMethod {
                path: path.to_owned(),
                entry: name,
                method: format!("{method:?}"),
            });
        }
    }
    Ok(())
}

/// Row L6 against the count the trailer declares, before the archive is built.
fn guard_declared_entry_count<R: Read + Seek>(
    probe: &mut R,
    display: &str,
    limits: &Limits,
) -> Result<(), ContainerError> {
    if let Some(declared) = declared_entry_count(probe) {
        if !limits.permits_entry_count(declared) {
            return Err(ContainerError::EntryCountExceeded {
                path: display.to_owned(),
                count: declared,
                limit: limits.max_entry_count,
            });
        }
    }
    Ok(())
}

/// Validates every entry name in the archive, and rejects duplicates.
///
/// Separate from [`invalid_name_reason`] because duplication is a property of
/// the SET rather than of one name, and §7.3 makes both checks name-validation
/// — the outermost stage, before the manifest is even located.
fn validate_entry_names<R: Read + Seek>(
    archive: &zip::ZipArchive<R>,
    path: &str,
) -> Result<(), ContainerError> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for i in 0..archive.len() {
        let Some(name) = archive.name_for_index(i) else {
            // A name the ZIP layer cannot even surface as a string is invalid
            // UTF-8 or an unset language-encoding flag, both of which §7.2
            // rejects as `invalid-entry-name`.
            return Err(ContainerError::InvalidEntryName {
                entry: format!("<entry {i}: name is not valid UTF-8>"),
            });
        };
        let name = name.to_owned();
        if let Some(reason) = invalid_name_reason(&name) {
            return Err(ContainerError::InvalidName {
                path: path.to_owned(),
                entry: name,
                reason,
            });
        }
        if let Some(first) = seen.insert(logical_name_key(&name), name.clone()) {
            return Err(ContainerError::DuplicateLogicalName {
                path: path.to_owned(),
                first,
                second: name,
            });
        }
    }
    Ok(())
}

/// §7.6 batches and §10.3 blobs, shared by both readers.
///
/// Factored out so the development-layout reader cannot drift from the
/// canonical one on the checks that matter: the §7.6 name grammar, the
/// name/content binding, the row-L23 envelopes, and blob digest verification.
/// A converter with its own copy of these would be a second, unreviewed reader
/// for exactly the inputs nobody tests twice.
#[allow(clippy::type_complexity)]
fn read_ops_and_blobs<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    display: &str,
    limits: &Limits,
) -> Result<(Vec<Batch>, BTreeMap<String, Vec<u8>>), ContainerError> {
    // §7.6 deterministic enumeration: sort by entry NAME ascending, which the
    // fixed-width padding makes equal to ascending (counterBase, actorId).
    // Enumeration is possible from names alone, before parsing contents.
    let mut names: Vec<(String, u64, Uuid)> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|n| n.to_owned()))
        .filter(|n| n.starts_with(OPS_PREFIX))
        .map(|n| {
            parse_ops_entry_name(&n)
                .map(|(base, actor)| (n.clone(), base, actor))
                .ok_or(ContainerError::InvalidEntryName { entry: n })
        })
        .collect::<Result<Vec<(String, u64, Uuid)>, ContainerError>>()?;
    names.sort_by(|a, b| a.0.cmp(&b.0));

    let mut batches = Vec::with_capacity(names.len());
    let mut operations: u64 = 0;
    for (name, base, actor) in names {
        let batch: Batch = read_json_named_entry(archive, &name, display, limits)?;
        // Name/content binding: a batch whose embedded counterBase or actorId
        // differs from its parsed name components rejects before replay.
        if batch.counter_base != base || batch.actor_id != actor {
            return Err(ContainerError::NameContentMismatch {
                entry: name,
                declared_base: batch.counter_base,
                declared_actor: batch.actor_id,
            });
        }
        if !limits.permits_batch_size(batch.operations.len() as u64) {
            return Err(ContainerError::OperationCountExceeded {
                path: display.to_owned(),
                found: batch.operations.len() as u64,
                limit: limits.max_operations_per_batch,
            });
        }
        operations += batch.operations.len() as u64;
        if !limits.permits_operation_count(operations) {
            return Err(ContainerError::OperationCountExceeded {
                path: display.to_owned(),
                found: operations,
                limit: limits.max_operations,
            });
        }
        batches.push(batch);
    }

    // Blobs. Enumerated from names, like batches, and each is VERIFIED
    // against the digest its name encodes — §6 requires streaming
    // verification, and a blob whose bytes do not match its name is a
    // corrupt package, not a usable asset.
    let blob_names: Vec<(String, String)> = (0..archive.len())
        .filter_map(|i| archive.name_for_index(i).map(|n| n.to_owned()))
        .filter(|n| n.starts_with(BLOB_PREFIX))
        .map(|n| {
            parse_blob_entry_name(&n)
                .map(|d| (n.clone(), d))
                .ok_or(ContainerError::InvalidEntryName { entry: n })
        })
        .collect::<Result<Vec<(String, String)>, ContainerError>>()?;

    let mut assets = BTreeMap::new();
    for (name, digest) in blob_names {
        let bytes = read_entry_bytes(archive, &name, display, limits)?;
        let actual = digest_of(&bytes);
        if actual != digest {
            return Err(ContainerError::DigestMismatch {
                declared: digest,
                actual,
            });
        }
        assets.insert(digest, bytes);
    }

    Ok((batches, assets))
}

fn read_json_entry<T: serde::de::DeserializeOwned, R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry: &'static str,
    path: &str,
    limits: &Limits,
) -> Result<T, ContainerError> {
    if archive.by_name(entry).is_err() {
        return Err(ContainerError::MissingEntry {
            path: path.to_owned(),
            entry: entry.to_owned(),
        });
    }
    read_json_named_entry(archive, entry, path, limits)
}

/// Reads an entry's bytes under the size limits, without parsing.
fn read_entry_bytes<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry: &str,
    path: &str,
    limits: &Limits,
) -> Result<Vec<u8>, ContainerError> {
    let mut file = archive
        .by_name(entry)
        .map_err(|_| ContainerError::MissingEntry {
            path: path.to_owned(),
            entry: entry.to_owned(),
        })?;
    let declared = file.size();
    if !limits.permits_entry_size(declared) {
        return Err(ContainerError::EntryTooLarge {
            path: path.to_owned(),
            entry: entry.to_owned(),
            declared,
            limit: limits.max_entry_bytes,
        });
    }
    let mut buf = Vec::with_capacity(declared.min(1 << 20) as usize);
    file.by_ref()
        .take(limits.max_entry_bytes + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ContainerError::Io {
            path: path.to_owned(),
            source: e,
        })?;
    if buf.len() as u64 > limits.max_entry_bytes {
        return Err(ContainerError::EntryTooLarge {
            path: path.to_owned(),
            entry: entry.to_owned(),
            declared: buf.len() as u64,
            limit: limits.max_entry_bytes,
        });
    }
    Ok(buf)
}

fn read_json_named_entry<T: serde::de::DeserializeOwned, R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    entry: &str,
    path: &str,
    limits: &Limits,
) -> Result<T, ContainerError> {
    let mut file = archive
        .by_name(entry)
        .map_err(|_| ContainerError::MissingEntry {
            path: path.to_owned(),
            entry: entry.to_owned(),
        })?;

    // Guard on the DECLARED size before reserving, then cap the reader itself so
    // a header that lies about its size cannot outrun the limit either. Trusting
    // the header alone is how decompression bombs get through.
    let declared = file.size();
    if !limits.permits_entry_size(declared) {
        return Err(ContainerError::EntryTooLarge {
            path: path.to_owned(),
            entry: entry.to_owned(),
            declared,
            limit: limits.max_entry_bytes,
        });
    }

    let mut buf = Vec::with_capacity(declared.min(1 << 20) as usize);
    file.by_ref()
        .take(limits.max_entry_bytes + 1)
        .read_to_end(&mut buf)
        .map_err(|e| ContainerError::Io {
            path: path.to_owned(),
            source: e,
        })?;

    if buf.len() as u64 > limits.max_entry_bytes {
        return Err(ContainerError::EntryTooLarge {
            path: path.to_owned(),
            entry: entry.to_owned(),
            declared: buf.len() as u64,
            limit: limits.max_entry_bytes,
        });
    }

    // Row L11, before the parser is handed the bytes. `serde_json` has a
    // recursion limit of its own, but it rejects at 128 where L11 ratifies 128
    // as PARSING, and a build feature can remove it, so the ratified boundary
    // is counted here and the library's redundant one is stood down for this
    // parse only. The depth is already bounded when that happens, so the
    // native stack is too.
    let checked = check_nesting_depth(&buf, limits.max_nesting_depth).map_err(|found| {
        ContainerError::NestingDepthExceeded {
            path: path.to_owned(),
            entry: entry.to_owned(),
            found,
            limit: limits.max_nesting_depth,
        }
    })?;

    parse_depth_checked(checked).map_err(|e| ContainerError::Json {
        path: path.to_owned(),
        entry: entry.to_owned(),
        source: e,
    })
}

/// Parses JSON whose nesting depth [`check_nesting_depth`] has already cleared.
///
/// Identical to `serde_json::from_slice` except that the library's own
/// recursion limit is stood down, because it is one level tighter than L11 and
/// would refuse a conforming document at exactly the ratified boundary.
/// Standing that limit down is only safe while something else bounds the
/// depth, which is why this takes a [`DepthChecked`] rather than a slice: the
/// guard cannot be forgotten, because there is no other way to obtain one.
pub fn parse_depth_checked<T: serde::de::DeserializeOwned>(
    checked: DepthChecked<'_>,
) -> Result<T, serde_json::Error> {
    let mut de = serde_json::Deserializer::from_slice(checked.bytes());
    de.disable_recursion_limit();
    let value = T::deserialize(&mut de)?;
    de.end()?;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use domowik_format::{Point, Wall};

    #[test]
    fn saving_clears_dirty_preserves_history_and_adopts_only_on_success() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("plan.domo");
        let missing = dir.path().join("missing/plan.domo");
        let mut session = Session::open_snapshot(
            Uuid::now_v7(),
            Document::with_ground_floor(),
            &Default::default(),
            &[],
            vec![],
        )
        .unwrap();
        session
            .commit(vec![Operation::create(domowik_format::Entity::Level(
                domowik_format::Level::new("Upper"),
            ))])
            .unwrap();
        let mut plan = Plan::unsaved(session);
        assert!(plan.save_as(&missing).is_err());
        assert_eq!(plan.path(), None);
        assert!(plan.session_ref().is_dirty());
        plan.save_as(&target).unwrap();
        assert_eq!(plan.path(), Some(target.as_path()));
        assert!(!plan.session_ref().is_dirty());
        assert_eq!(plan.session_ref().undo_depth(), 1);
        plan.session().unwrap().undo().unwrap();
        assert!(plan.session_ref().is_dirty());
        assert!(plan.save_as(&missing).is_err());
        assert_eq!(plan.path(), Some(target.as_path()));
        assert!(plan.session_ref().is_dirty());
        plan.save().unwrap();
        assert!(!plan.session_ref().is_dirty());
        assert_eq!(plan.session_ref().redo_depth(), 1);
        plan.session().unwrap().redo().unwrap();
        assert!(plan.session_ref().is_dirty());
        assert_eq!(plan.document().levels.len(), 2);
    }

    #[test]
    fn encoded_bytes_match_the_written_file_and_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("plan.domo");
        let package = sample();
        write_package(&target, &package).unwrap();
        let encoded = encode_package(&package).unwrap();
        assert_eq!(encoded, std::fs::read(&target).unwrap());
        let reopened = Plan::open_bytes("plan.domo", &encoded).unwrap();
        assert_eq!(reopened.path(), None);
        assert_eq!(reopened.document(), Plan::open(&target).unwrap().document());
    }

    #[test]
    fn encoding_leaves_the_plan_dirty_until_marked_saved() {
        let mut session = Session::open_snapshot(
            Uuid::now_v7(),
            Document::with_ground_floor(),
            &Default::default(),
            &[],
            vec![],
        )
        .unwrap();
        session
            .commit(vec![Operation::create(domowik_format::Entity::Level(
                domowik_format::Level::new("Upper"),
            ))])
            .unwrap();
        let mut plan = Plan::unsaved(session);
        let bytes = plan.encode().unwrap();
        assert!(plan.session_ref().is_dirty());
        plan.mark_saved();
        assert!(!plan.session_ref().is_dirty());
        assert_eq!(plan.session_ref().undo_depth(), 1);
        let reopened = Plan::open_bytes("plan.domo", &bytes).unwrap();
        assert_eq!(reopened.document().levels.len(), 2);
        assert!(matches!(
            Plan::unsaved(Session::new()).encode(),
            Err(ContainerError::NoLevels)
        ));
    }

    #[test]
    fn zero_level_builder_cannot_replace_a_file_or_adopt_a_save_target() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("existing.domo");
        write_package(&target, &sample()).unwrap();
        let before = std::fs::read(&target).unwrap();
        let mut plan = Plan::unsaved(Session::new());
        assert!(matches!(
            plan.save_as(&target),
            Err(ContainerError::NoLevels)
        ));
        assert_eq!(plan.path(), None);
        assert!(!plan.session_ref().is_dirty());
        assert_eq!(plan.session_ref().undo_depth(), 0);
        assert_eq!(std::fs::read(&target).unwrap(), before);
        let fresh = dir.path().join("not-created.domo");
        assert!(matches!(
            plan.save_as(&fresh),
            Err(ContainerError::NoLevels)
        ));
        assert!(!fresh.exists());
    }

    fn sample() -> Package {
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            120.0,
            2500.0,
        ));
        Package::new(doc)
    }

    #[test]
    fn l6_boundary_admits_the_ceiling_and_rejects_one_past_it() {
        let limits = Limits::default();
        assert!(
            limits.permits_entry_count(65_535),
            "FORMAT-SPEC §9.3 row L6: a package at the ceiling must be accepted"
        );
        assert!(
            !limits.permits_entry_count(65_536),
            "FORMAT-SPEC §9.3 row L6: 65,536 exceeds the 16-bit ZIP entry field \
             and must be rejected"
        );
    }

    #[test]
    fn l6_ceiling_fits_the_legacy_zip_entry_field() {
        assert!(
            u16::try_from(Limits::L6_ENTRY_COUNT).is_ok(),
            "row L6 is justified by a 16-bit ZIP field; the ceiling must fit it"
        );
    }

    #[test]
    fn a_written_package_reads_back_identically() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let original = sample();

        write_package(&path, &original).expect("write");
        let loaded = read_package(&path).expect("read");

        // Everything the CALLER supplied must survive verbatim.
        assert_eq!(original.snapshot, loaded.snapshot);
        assert_eq!(original.frontier, loaded.frontier);
        assert_eq!(original.tombstones, loaded.tombstones);
        assert_eq!(original.snapshot_unknown, loaded.snapshot_unknown);
        assert_eq!(original.batches, loaded.batches);
        assert_eq!(original.assets, loaded.assets);
        assert_eq!(
            original.manifest.format_version,
            loaded.manifest.format_version
        );
        assert_eq!(original.manifest.asset_root, loaded.manifest.asset_root);
        assert_eq!(original.manifest.unknown, loaded.manifest.unknown);

        // `entries` and `profile` are DERIVED by the writer, so they are not
        // equal to what the caller passed and must not be asserted as such.
        // They are checked for what they are instead: an inventory covering
        // every non-manifest entry, and the profile §7.9 implies.
        assert!(
            loaded.manifest.entries.contains_key(SNAPSHOT_ENTRY),
            "the inventory must cover the snapshot: {:?}",
            loaded.manifest.entries.keys().collect::<Vec<_>>()
        );
        assert!(
            !loaded.manifest.entries.contains_key(MANIFEST_ENTRY),
            "the manifest cannot carry its own digest"
        );
        assert_eq!(
            loaded.manifest.profile,
            Profile::Portable,
            "§7.9: an empty U(P) is portable by vacuous truth (issue 0064)"
        );
    }

    /// FORMAT-SPEC §7.9's empty-`U(P)` rule, pinned so it cannot drift back.
    ///
    /// This test previously asserted the opposite. It was not wrong to exist —
    /// it pinned the writer's behaviour faithfully — but that behaviour came
    /// from a guard §7.9 does not state, which is what issue 0064 settled.
    #[test]
    fn an_asset_free_package_is_portable_not_thin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");

        let package = sample();
        assert!(
            package.assets.is_empty(),
            "this fixture must have an empty U(P) or it tests something else"
        );
        write_package(&path, &package).expect("write");

        let loaded = read_package(&path).expect("read");
        assert!(
            loaded.manifest.asset_root.is_empty(),
            "U(P) must be empty on the round trip too"
        );
        assert_eq!(
            loaded.manifest.profile,
            Profile::Portable,
            "there is no blob that could fail to embed, so nothing can break              when this package is moved — which is what portable means"
        );
    }

    #[test]
    fn writing_over_an_existing_plan_replaces_it() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");

        write_package(&path, &sample()).expect("first write");
        let mut second = sample();
        second.snapshot.walls.clear();
        write_package(&path, &second).expect("second write");

        assert!(read_package(&path).expect("read").snapshot.walls.is_empty());
    }

    #[test]
    fn a_failed_write_leaves_no_temporary_file_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &sample()).expect("write");

        let strays: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        assert!(strays.is_empty(), "temporary file left behind: {strays:?}");
    }

    #[test]
    fn a_non_zip_file_is_rejected_as_an_archive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("not-a-plan.domo");
        std::fs::write(&path, b"this is plainly not a zip archive").expect("write");

        assert!(matches!(
            read_package(&path),
            Err(ContainerError::Archive { .. })
        ));
    }

    #[test]
    fn an_archive_without_a_snapshot_is_rejected_by_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("empty.domo");
        raw_archive(&path, &Manifest::default(), &[]);

        match read_package(&path) {
            Err(ContainerError::MissingEntry { entry, .. }) => {
                assert_eq!(entry, SNAPSHOT_ENTRY);
            }
            other => panic!("expected a MissingEntry for the snapshot, got {other:?}"),
        }
    }

    #[test]
    fn a_future_format_version_is_refused_rather_than_guessed_at() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("future.domo");
        let mut pkg = sample();
        pkg.manifest.format_version = "0.2".to_owned();

        // Written by hand: `write_package` now REFUSES to produce a package
        // its own reader cannot read (§9.7 S2), so the artifact this test
        // needs can no longer come from the writer.
        write_raw_package(&path, &pkg);

        match read_package(&path) {
            Err(ContainerError::UnsupportedVersion {
                found, supported, ..
            }) => {
                // §8.2 V3 requires the diagnostic to name BOTH versions, so
                // both are asserted; a message naming only one leaves its
                // reader unable to tell which end is wrong.
                assert_eq!(found, "0.2");
                assert_eq!(supported, EMITTED_SCHEMA);
            }
            other => panic!("expected UnsupportedVersion, got {other:?}"),
        }
    }

    #[test]
    fn an_oversized_entry_is_refused_before_allocation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("big.domo");
        write_package(&path, &sample()).expect("write");

        // A one-byte ceiling makes the real document oversized. The guard must
        // fire on the declared size, not after materialising the entry.
        let tiny = Limits {
            max_operations: Limits::L23_TOTAL_OPERATIONS,
            max_operations_per_batch: Limits::L23_OPERATIONS_PER_BATCH,
            max_entry_count: Limits::L6_ENTRY_COUNT,
            max_entry_bytes: 1,
            max_nesting_depth: Limits::L11_NESTING_DEPTH,
            max_snapshot_metadata_bytes: L23_SNAPSHOT_METADATA_BYTES,
        };
        assert!(matches!(
            read_package_with_limits(&path, &tiny),
            Err(ContainerError::EntryTooLarge { .. })
        ));
    }

    #[test]
    fn a_truncated_package_is_rejected_rather_than_partially_trusted() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &sample()).expect("write");

        let mut bytes = std::fs::read(&path).expect("read");
        bytes.truncate(bytes.len() / 2);
        std::fs::write(&path, &bytes).expect("truncate");

        assert!(
            read_package(&path).is_err(),
            "a truncated package must not load"
        );
    }

    // ---- FORMAT-SPEC §7.6 operation-batch framing ----

    /// A conforming batch: FORMAT-SPEC F4 requires the writing actor's own
    /// prior end counter in the frontier, which for a contiguous stream of
    /// one-operation batches is `counterBase - 1`.
    fn seed_batch(actor: Uuid, base: u64) -> Batch {
        let mut frontier = std::collections::BTreeMap::new();
        if base > 1 {
            frontier.insert(actor, base - 1);
        }
        Batch::new(
            actor,
            base,
            frontier,
            vec![domowik_document::operation::Operation::create(
                domowik_format::Entity::Level(domowik_format::Level::new("Level 1")),
            )],
        )
    }

    #[test]
    fn an_ops_entry_name_is_the_padded_counter_and_the_canonical_uuid() {
        let actor = Uuid::from_u128(1);
        let name = ops_entry_name(42, actor);
        assert_eq!(
            name,
            "ops/0000000000000000042-00000000-0000-0000-0000-000000000001.json"
        );
        assert_eq!(parse_ops_entry_name(&name), Some((42, actor)));
    }

    #[test]
    fn ops_entry_names_sort_by_counter_then_actor() {
        // §7.6: fixed-width padding makes byte comparison of names equal
        // ascending (counterBase, actorId). Without the padding, "10" would
        // sort before "9".
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let mut names = vec![
            ops_entry_name(10, a),
            ops_entry_name(9, b),
            ops_entry_name(9, a),
        ];
        names.sort();
        assert_eq!(
            names,
            vec![
                ops_entry_name(9, a),
                ops_entry_name(9, b),
                ops_entry_name(10, a)
            ]
        );
    }

    #[test]
    fn a_malformed_ops_name_is_not_a_name_binding_case() {
        // §7.3 rejects these at name validation, before binding is reached.
        assert_eq!(parse_ops_entry_name("ops/42-abc.json"), None, "unpadded");
        assert_eq!(
            parse_ops_entry_name("ops/0000000000000000042-not-a-uuid.json"),
            None
        );
        assert_eq!(parse_ops_entry_name("document.json"), None);
    }

    #[test]
    fn batches_round_trip_through_a_package() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let actor = Uuid::now_v7();

        let mut pkg = Package::new(Document::default());
        pkg.batches.push(seed_batch(actor, 1));
        write_package(&path, &pkg).expect("write");

        let back = read_package(&path).expect("read");
        assert_eq!(back.batches, pkg.batches);
        assert_eq!(back.project().expect("projects").document.levels.len(), 1);
    }

    #[test]
    fn a_batch_whose_content_disagrees_with_its_name_is_rejected() {
        // §7.6 name/content binding, checked BEFORE replay begins.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let actor = Uuid::now_v7();
        let batch = seed_batch(actor, 1);

        // Write by hand so the name and the content can disagree.
        raw_archive(
            &path,
            &Manifest::default(),
            &[
                (
                    SNAPSHOT_ENTRY.to_owned(),
                    serde_json::to_vec(&SnapshotSection::default()).unwrap(),
                ),
                // The name says counterBase 7; the content says 1.
                (
                    ops_entry_name(7, actor),
                    serde_json::to_vec(&batch).unwrap(),
                ),
            ],
        );

        assert!(matches!(
            read_package(&path),
            Err(ContainerError::NameContentMismatch { .. })
        ));
    }

    #[test]
    fn a_package_over_the_l23_operation_cap_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let actor = Uuid::now_v7();
        let mut pkg = Package::new(Document::default());
        pkg.batches.push(seed_batch(actor, 1));
        pkg.batches.push(seed_batch(actor, 2));
        write_package(&path, &pkg).expect("write");

        let strict = Limits {
            max_operations: 1,
            ..Limits::default()
        };
        assert!(matches!(
            read_package_with_limits(&path, &strict),
            Err(ContainerError::OperationCountExceeded { .. })
        ));
        // At the cap it is accepted: the boundary belongs on the passing side.
        let exact = Limits {
            max_operations: 2,
            ..Limits::default()
        };
        assert!(read_package_with_limits(&path, &exact).is_ok());
    }

    #[test]
    fn a_package_whose_log_is_structurally_malformed_fails_to_project() {
        // §7.6: structural log checks precede replay. Two batches reserving
        // the same range is one of the enumerated anomalies.
        let actor = Uuid::now_v7();
        let mut pkg = Package::new(Document::default());
        pkg.batches.push(seed_batch(actor, 1));
        let mut clash = seed_batch(actor, 1);
        clash.batch_id = Uuid::now_v7();
        pkg.batches.push(clash);
        assert!(matches!(pkg.project(), Err(ContainerError::Log { .. })));
    }

    #[test]
    fn a_snapshot_frontier_lets_a_compacted_package_project() {
        // §5.1 S2: the frontier says which counters the snapshot covers, so a
        // batch above it applies instead of being held forever.
        let actor = Uuid::now_v7();
        let mut doc = Document::with_ground_floor();
        let level = doc.levels[0].id;
        doc.walls.push(Wall::new(
            level,
            Point::new(0.0, 0.0),
            Point::new(4000.0, 0.0),
            200.0,
            2500.0,
        ));
        let wall_id = doc.walls[0].id;

        let mut pkg = Package::new(doc);
        pkg.frontier.insert(actor, 5);
        pkg.batches.push(Batch::new(
            actor,
            6,
            [(actor, 5)].into_iter().collect(),
            vec![domowik_document::operation::Operation::delete(wall_id)],
        ));

        let projected = pkg.project().expect("projects");
        assert!(projected.document.walls.is_empty());
        assert!(projected.discarded.is_empty(), "{:?}", projected.discarded);
    }

    // ---- FORMAT-SPEC §9.7 save state machine ----

    #[test]
    fn a_save_leaves_no_temporary_behind() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .expect("read dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "got {leftovers:?}");
    }

    #[test]
    fn two_saves_from_one_process_use_different_temporary_names() {
        // §9.7 S0 requires a collision-safe name. A fixed one lets two
        // concurrent saves interleave into one file and rename the result over
        // the user's plan.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let a = temporary_name(&path);
        let b = temporary_name(&path);
        assert_ne!(a, b);
        assert!(
            a.ends_with(".tmp") && a.starts_with(".plan.domo."),
            "got {a}"
        );
    }

    #[test]
    fn saving_over_a_symlink_is_refused_rather_than_followed() {
        // §9.7 S0: regular-file target only. Following the link would write
        // through it and replace a file the user did not name; renaming over
        // it would silently delete the link instead of its target.
        let dir = tempfile::tempdir().expect("tempdir");
        let real = dir.path().join("real.domo");
        write_package(&real, &Package::new(Document::with_ground_floor())).expect("write");
        let link = dir.path().join("link.domo");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");

        let err =
            write_package(&link, &Package::new(Document::default())).expect_err("must refuse");
        // The REASON matters, not just the variant: the not-a-regular-file arm
        // also fires on a symlink, with a message that does not tell the user
        // their path is a link. Asserting only the variant leaves the specific
        // guard untested.
        match err {
            ContainerError::UnsafeTarget { ref reason, .. } => {
                assert!(reason.contains("symbolic link"), "got {reason:?}")
            }
            other => panic!("expected UnsafeTarget, got {other}"),
        }
        // The real file must be untouched.
        assert_eq!(read_package(&real).expect("read").snapshot.levels.len(), 1);
    }

    #[test]
    fn a_malformed_log_is_refused_before_a_candidate_is_even_created() {
        // Two batches reserving one range. §7.6 names entries by
        // `(counterBase, actorId)`, so this is not merely invalid — it is
        // physically unrepresentable, and without the pre-flight the caller
        // would be told "duplicate filename" by the ZIP layer for what is a
        // framing anomaly.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

        let actor = Uuid::now_v7();
        let mut bad = Package::new(Document::default());
        bad.batches.push(seed_batch(actor, 1));
        let mut clash = seed_batch(actor, 1);
        clash.batch_id = Uuid::now_v7();
        bad.batches.push(clash);

        let err = write_package(&path, &bad).expect_err("must refuse");
        assert!(matches!(err, ContainerError::Log { .. }), "got {err}");
        assert_eq!(
            read_package(&path).expect("read").snapshot.levels.len(),
            1,
            "the old package must be untouched"
        );
        assert!(no_temporaries(dir.path()), "nothing may have been created");
    }

    #[test]
    fn an_existing_plan_survives_a_save_whose_candidate_would_be_unreadable() {
        // §9.7 S2/F3: abort `candidate-invalid`; target untouched. A future
        // format version writes perfectly well and cannot be READ back, which
        // is exactly the case a parse-of-what-we-just-wrote check exists for:
        // without S2 the save would succeed and the NEXT open would fail,
        // after the old copy was already gone.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

        let mut future = Package::new(Document::default());
        future.manifest.format_version = "0.2".to_owned();
        let err = write_package(&path, &future).expect_err("must refuse");
        assert!(
            matches!(err, ContainerError::CandidateInvalid { .. }),
            "got {err}"
        );
        assert_eq!(
            read_package(&path).expect("read").snapshot.levels.len(),
            1,
            "the old package must be untouched"
        );
        assert!(
            no_temporaries(dir.path()),
            "the failed candidate must be cleaned"
        );
    }

    /// Writes an archive by hand, for constructing artifacts a conforming
    /// writer refuses to produce.
    ///
    /// `entries` are everything except the manifest; the §7.4 inventory is
    /// computed over exactly those bytes, so a hand-built archive is
    /// inventory-correct unless a test deliberately breaks it afterwards. A
    /// helper that left the inventory empty would make every test using it
    /// fail at the integrity layer for a reason none of them is about.
    pub(crate) fn raw_archive(path: &Path, manifest: &Manifest, entries: &[(String, Vec<u8>)]) {
        let mut manifest = manifest.clone();
        manifest.entries = entries
            .iter()
            .map(|(name, bytes)| {
                (
                    name.clone(),
                    EntryIntegrity {
                        sha256: digest_of(bytes),
                        byte_length: bytes.len() as u64,
                        unknown: Default::default(),
                    },
                )
            })
            .collect();

        let file = std::fs::File::create(path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        zip.start_file(MANIFEST_ENTRY, opts).expect("entry");
        zip.write_all(&serde_json::to_vec(&manifest).expect("manifest"))
            .expect("write");
        for (name, bytes) in entries {
            zip.start_file(name.clone(), opts).expect("entry");
            zip.write_all(bytes).expect("write");
        }
        zip.finish().expect("finish");
    }

    /// The snapshot section for a package, as bytes.
    fn snapshot_bytes(package: &Package) -> Vec<u8> {
        serde_json::to_vec(&SnapshotSection {
            document: package.snapshot.clone(),
            frontier: package.frontier.clone(),
            tombstones: package.tombstones.clone(),
            unknown: package.snapshot_unknown.clone(),
        })
        .expect("snapshot")
    }

    /// Writes a package WITHOUT the §9.7 checks.
    fn write_raw_package(path: &Path, package: &Package) {
        raw_archive(
            path,
            &package.manifest,
            &[(SNAPSHOT_ENTRY.to_owned(), snapshot_bytes(package))],
        );
    }

    #[test]
    fn a_bare_filename_resolves_to_a_directory_that_can_be_opened() {
        // S5 syncs the parent directory. `Path::parent` gives `Some("")` for a
        // bare name, which joins fine but cannot be opened — so the durability
        // step would silently do nothing, invisibly to every save-and-read
        // test.
        assert_eq!(parent_dir(Path::new("bare.domo")), Path::new("."));
        assert!(std::fs::File::open(parent_dir(Path::new("bare.domo"))).is_ok());
        assert_eq!(parent_dir(Path::new("a/b.domo")), Path::new("a"));
        assert_eq!(parent_dir(Path::new("/tmp/b.domo")), Path::new("/tmp"));
    }

    fn no_temporaries(dir: &Path) -> bool {
        std::fs::read_dir(dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .all(|e| !e.file_name().to_string_lossy().ends_with(".tmp"))
    }

    #[test]
    fn a_save_that_exceeds_the_readers_limits_is_refused_before_replacing() {
        // The candidate must be validated against the SAME limits a reader
        // will apply, or a save can write a package that cannot be reopened.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

        let actor = Uuid::now_v7();
        let mut big = Package::new(Document::default());
        big.batches.push(seed_batch(actor, 1));
        big.batches.push(seed_batch(actor, 2));
        let strict = Limits {
            max_operations: 1,
            ..Limits::default()
        };
        assert!(write_package_with_limits(&path, &big, &strict).is_err());
        assert_eq!(
            read_package(&path).expect("read").snapshot.levels.len(),
            1,
            "the old package must be untouched"
        );
    }

    #[test]
    fn a_save_into_a_bare_filename_works() {
        // `path.parent()` is `Some("")` for a bare name, which joins to a
        // relative path that is not the current directory. Getting this wrong
        // makes every save from a shell in the plan's own directory fail.
        let dir = tempfile::tempdir().expect("tempdir");
        let previous = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(dir.path()).expect("chdir");
        let result = write_package(
            Path::new("bare.domo"),
            &Package::new(Document::with_ground_floor()),
        );
        let readable = read_package(Path::new("bare.domo")).map(|p| p.snapshot.levels.len());
        std::env::set_current_dir(previous).expect("restore cwd");
        result.expect("write");
        assert_eq!(readable.expect("read"), 1);
    }

    // ---- assets (FORMAT-SPEC §6, ASSET-STRATEGY §10) ----

    #[test]
    fn a_digest_is_sha256_lowercase_hex() {
        // ASSET-STRATEGY §10.1. The empty-input value is the published SHA-256
        // test vector, so this checks the ALGORITHM and not merely that some
        // hash was computed.
        assert_eq!(
            digest_of(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            digest_of(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_blob_entry_name_splits_the_digest_two_and_sixty_two() {
        // §10.3: `blob/<algorithm>/<2-hex>/<62-hex>`.
        let d = digest_of(b"abc");
        assert_eq!(
            blob_entry_name(&d).expect("well formed"),
            "blob/sha256/ba/7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            parse_blob_entry_name(&blob_entry_name(&d).unwrap()),
            Some(d)
        );
    }

    #[test]
    fn a_malformed_digest_has_no_blob_name() {
        // No traversal surface: every component is fixed-width hex derived
        // from the digest, so a hostile "digest" cannot name a path.
        assert!(blob_entry_name("sha256:../../etc/passwd").is_none());
        assert!(blob_entry_name("sha256:ABC").is_none(), "uppercase");
        assert!(blob_entry_name("md5:abc").is_none(), "wrong algorithm");
        assert!(blob_entry_name(&format!("sha256:{}", "a".repeat(63))).is_none());
    }

    #[test]
    fn a_blob_round_trips_through_a_package() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let bytes = b"not really a png, but bytes are bytes".to_vec();
        let digest = digest_of(&bytes);

        let mut pkg = Package::new(Document::with_ground_floor());
        pkg.assets.insert(digest.clone(), bytes.clone());
        pkg.manifest.asset_root.push(AssetRoot::new(digest.clone()));
        write_package(&path, &pkg).expect("write");

        let back = read_package(&path).expect("read");
        assert_eq!(back.assets.get(&digest), Some(&bytes));
        assert_eq!(back.manifest.asset_root, vec![AssetRoot::new(digest)]);
    }

    #[test]
    fn a_package_claiming_a_digest_it_does_not_hold_is_refused_at_write() {
        // §10.4 makes a portable package's verification claim the whole point
        // of the profile, so a mismatched digest must never be written.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let mut pkg = Package::new(Document::default());
        pkg.assets
            .insert(digest_of(b"one thing"), b"a different thing".to_vec());
        let err = write_package(&path, &pkg).expect_err("must refuse");
        assert!(
            matches!(err, ContainerError::DigestMismatch { .. }),
            "got {err}"
        );
        assert!(!path.exists(), "nothing may be written");
    }

    #[test]
    fn a_blob_whose_bytes_do_not_match_its_name_is_refused_at_read() {
        // Streaming verification (§6). The name encodes the digest, so a
        // reader can check without trusting any other metadata.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        raw_archive(
            &path,
            &Manifest::default(),
            &[
                (
                    SNAPSHOT_ENTRY.to_owned(),
                    serde_json::to_vec(&SnapshotSection::default()).unwrap(),
                ),
                // The name says one digest; the bytes hash to another.
                (
                    blob_entry_name(&digest_of(b"expected")).unwrap(),
                    b"actual".to_vec(),
                ),
            ],
        );

        assert!(matches!(
            read_package(&path),
            Err(ContainerError::DigestMismatch { .. })
        ));
    }

    #[test]
    fn identical_bytes_added_twice_are_stored_once() {
        // Deduplication (§6) falls out of content addressing: the digest IS
        // the key, so there is nowhere to put a second copy.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let bytes = b"shared".to_vec();
        let mut pkg = Package::new(Document::default());
        pkg.assets.insert(digest_of(&bytes), bytes.clone());
        pkg.assets.insert(digest_of(&bytes), bytes);
        assert_eq!(pkg.assets.len(), 1);
        write_package(&path, &pkg).expect("write");
        assert_eq!(read_package(&path).expect("read").assets.len(), 1);
    }
}

/// Row L11's boundary, counted here rather than inherited.
///
/// Every value in these tests DERIVES from [`Limits::L11_NESTING_DEPTH`]
/// rather than restating 128 a second time: a test that repeats a
/// specification number checks transcription against itself (ED-0089).
#[cfg(test)]
mod nesting_depth_l11 {
    // The §7.4 inventory pass runs before any entry is parsed, so a hand-built
    // archive needs a matching one or it rejects at the integrity layer and
    // never reaches the guard under test. `raw_archive` is the shared helper
    // that keeps the two consistent.
    use super::tests::raw_archive;
    use super::*;

    const LIMIT: u32 = Limits::L11_NESTING_DEPTH;

    /// `depth` nested arrays around a scalar: the maximum level is `depth`.
    fn nested_arrays(depth: u32) -> Vec<u8> {
        let mut s = String::new();
        for _ in 0..depth {
            s.push('[');
        }
        s.push('1');
        for _ in 0..depth {
            s.push(']');
        }
        s.into_bytes()
    }

    fn corpus(relative: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/corpus")
            .join(relative);
        std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
    }

    #[test]
    fn the_boundary_is_inclusive_below_and_exclusive_above() {
        // §9.3 L11: "depth == 128 parses; depth 129 rejects".
        assert!(check_nesting_depth(&nested_arrays(LIMIT - 1), LIMIT).is_ok());
        assert!(check_nesting_depth(&nested_arrays(LIMIT), LIMIT).is_ok());
        assert_eq!(
            check_nesting_depth(&nested_arrays(LIMIT + 1), LIMIT).unwrap_err(),
            LIMIT + 1,
            "the first level over the limit is the one reported"
        );
    }

    #[test]
    fn a_conforming_document_at_the_boundary_actually_parses() {
        // The half serde_json cannot do: its own recursion limit rejects at
        // LIMIT, so a reader that inherited it would refuse this document.
        let at_limit = nested_arrays(LIMIT);
        let checked = check_nesting_depth(&at_limit, LIMIT).expect("128 levels conform");
        let value: serde_json::Value =
            parse_depth_checked(checked).expect("a 128-level text conforms and must parse");
        assert!(value.is_array());

        assert!(
            serde_json::from_slice::<serde_json::Value>(&at_limit).is_err(),
            "if serde_json ever accepts this, the workaround above can be retired \
             — but until then, inheriting its limit is the off-by-one L11 forbids"
        );
    }

    #[test]
    fn brackets_inside_strings_are_not_containers() {
        // A string full of brackets is one level, not thousands. Miscounting
        // here would reject ordinary documents whose names contain punctuation.
        let mut s = String::from("{\"name\":\"");
        for _ in 0..(LIMIT * 4) {
            s.push_str("[{");
        }
        s.push_str("\"}");
        assert!(check_nesting_depth(s.as_bytes(), LIMIT).is_ok());
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        // "\\\"" closes nothing; the brackets after it are still inside the
        // string. A naive scanner reads the escaped quote as the terminator and
        // starts counting them.
        let mut s = String::from("{\"name\":\"a\\\"");
        for _ in 0..(LIMIT * 4) {
            s.push('[');
        }
        s.push_str("\"}");
        assert!(check_nesting_depth(s.as_bytes(), LIMIT).is_ok());
    }

    #[test]
    fn depth_is_the_maximum_reached_not_the_running_total() {
        // Siblings do not accumulate: 200 sibling pairs never exceed depth 2.
        let mut s = String::from("[");
        for i in 0..200 {
            if i > 0 {
                s.push(',');
            }
            s.push_str("[1]");
        }
        s.push(']');
        assert!(check_nesting_depth(s.as_bytes(), LIMIT).is_ok());
    }

    #[test]
    fn stray_closers_cannot_buy_depth_back() {
        // The scanner is not a validator, and must not become one by accident.
        // Leading closers saturate at zero rather than going negative, so a
        // hostile file cannot prepend `]]]]` to earn extra levels later. The
        // JSON is malformed either way; the parser is what rejects it, and the
        // guard's only job is that it never UNDERCOUNTS.
        let mut s = String::new();
        for _ in 0..(LIMIT * 2) {
            s.push(']');
        }
        for _ in 0..(LIMIT + 1) {
            s.push('[');
        }
        assert_eq!(
            check_nesting_depth(s.as_bytes(), LIMIT).unwrap_err(),
            LIMIT + 1,
            "closers before any opener must not offset the count"
        );
    }

    #[test]
    fn the_corpus_twins_are_the_executable_statement_of_the_rule() {
        // These two fixtures encode L11's counting contract. Until now nothing
        // in the Rust workspace read them.
        let at = corpus("golden/parser-twin-nesting-depth-at-L11.json");
        let over = corpus("hostile/json/nesting-depth-over-L11.json");

        let checked = check_nesting_depth(&at, LIMIT)
            .expect("the golden twin is at the limit and must parse");
        parse_depth_checked::<serde_json::Value>(checked).expect("golden twin parses");

        assert_eq!(
            check_nesting_depth(&over, LIMIT).unwrap_err(),
            LIMIT + 1,
            "the hostile twin is one level over and must reject"
        );
    }

    /// The depth of `bytes` under row L11's counting rule, derived by probing
    /// the guard itself rather than by a second scanner that could drift from
    /// it: the depth is the smallest limit the bytes pass at.
    fn depth_of(bytes: &[u8]) -> u32 {
        (0..=512)
            .find(|&n| check_nesting_depth(bytes, n).is_ok())
            .expect("depth under 512")
    }

    /// A `Document` whose one level carries an unknown optional field nested
    /// so the serialized text sits at EXACTLY `target` levels.
    ///
    /// The payload is semantically valid throughout: §8 U2 requires an unknown
    /// optional field to be preserved, so this is a document a conforming
    /// writer could have produced, not a structure contrived to be deep.
    fn document_at_depth(target: u32) -> Document {
        for pad in 1..512u32 {
            let mut level = domowik_format::Level::new("Level 1");
            let mut nest = serde_json::json!(1);
            for _ in 0..pad {
                nest = serde_json::Value::Array(vec![nest]);
            }
            level.unknown.insert("futureField".to_owned(), nest);
            let mut doc = Document::new();
            doc.levels.push(level);
            if depth_of(&serde_json::to_vec(&doc).unwrap()) == target {
                return doc;
            }
        }
        panic!("no padding reaches depth {target}");
    }

    /// The §7.5 snapshot section costs exactly one level of the L11 budget.
    ///
    /// Row L11 counts depth from the ENTRY root, and since FORMAT-SPEC §7.5 the
    /// snapshot entry's root is the section, not the document — the projection
    /// is a member of it. So a document that reaches depth `d` on its own
    /// reaches `d + 1` once it is inside the entry that carries it.
    ///
    /// This is asserted rather than assumed because the whole boundary below
    /// depends on it, and because it changed: before §7.5 the snapshot entry
    /// WAS the bare document and the two depths were equal. A conforming
    /// package has the same property for the same reason — §5.1 makes the
    /// projected state a member of the snapshot, not the snapshot itself.
    const SECTION_OVERHEAD: u32 = 1;

    #[test]
    fn the_snapshot_section_costs_exactly_one_level() {
        let doc = document_at_depth(8);
        let bare = depth_of(&serde_json::to_vec(&doc).unwrap());
        let wrapped = depth_of(
            &serde_json::to_vec(&SnapshotSection {
                document: doc,
                ..Default::default()
            })
            .unwrap(),
        );
        assert_eq!(
            wrapped,
            bare + SECTION_OVERHEAD,
            "the §7.5 section must cost exactly the level this module budgets for"
        );
    }

    /// Entry point A, the package reader, across the whole boundary.
    ///
    /// The depths are taken FROM the corpus twins rather than written here a
    /// second time (ED-0089): the golden artifact defines the accepting depth
    /// and the hostile one the rejecting depth.
    ///
    /// The DOCUMENT depths are one below the entry depths, because the entry is
    /// the §7.5 section and the document sits inside it. The budget is spent
    /// where L11 measures it, at the entry root, so a document may nest to
    /// `L11 - 1` and no further.
    #[test]
    fn the_package_reader_admits_the_boundary_and_rejects_one_past_it() {
        let at = depth_of(&corpus("golden/parser-twin-nesting-depth-at-L11.json"));
        let over = depth_of(&corpus("hostile/json/nesting-depth-over-L11.json"));
        assert_eq!(at, LIMIT, "the golden twin defines the accepting depth");
        assert_eq!(
            over,
            LIMIT + 1,
            "the hostile twin defines the rejecting one"
        );

        // The two document depths whose ENTRY depth lands at or below the
        // limit: admitted, and admitted ALL THE WAY — the package reads back
        // and the unknown field survives, so this separates depth admission
        // from later semantics by having no later failure at all.
        for target in [at - SECTION_OVERHEAD - 1, at - SECTION_OVERHEAD] {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("plan.domo");
            let doc = document_at_depth(target);
            write_package(&path, &Package::new(doc)).expect("write");

            let read = read_package(&path)
                .unwrap_or_else(|e| panic!("depth {target} conforms and must read: {e}"));
            assert!(
                read.snapshot.levels[0].unknown.contains_key("futureField"),
                "depth {target}: §8 U2 preservation must survive the depth guard"
            );
        }

        // 129, from both sides. The WRITER refuses first: §9.7 S2 validates the
        // candidate with reader-equivalent checks, so a conforming writer
        // cannot emit an over-deep package at all, and the existing target is
        // left untouched. This was found by writing the test — the 129 case was
        // drafted expecting the write to succeed and the read to reject.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");
        let doc = document_at_depth(over);
        match write_package(&path, &Package::new(doc)).expect_err("the writer must refuse") {
            ContainerError::CandidateInvalid { source, .. } => match *source {
                ContainerError::NestingDepthExceeded { found, limit, .. } => {
                    assert_eq!((found, limit), (over, LIMIT));
                }
                other => panic!("expected NestingDepthExceeded inside, got {other}"),
            },
            other => panic!("expected CandidateInvalid, got {other}"),
        }
        assert!(
            !path.exists(),
            "a refused save must leave no target (§9.7 F3)"
        );

        // And the READER refuses a package some other writer produced, which is
        // the case the guard actually exists for. Written by hand because, as
        // above, this writer will not produce one.
        // Through `raw_archive` so the §7.4 inventory MATCHES the bytes. The
        // inventory pass now runs before any entry is parsed for meaning, so a
        // hand-built archive without one rejects as `EntryNotInInventory` and
        // never reaches the depth guard — which would leave this test green
        // while proving nothing about L11.
        let hostile = dir.path().join("hostile.domo");
        raw_archive(
            &hostile,
            &Manifest::default(),
            &[(
                SNAPSHOT_ENTRY.to_owned(),
                // The hostile twin's depth AT THE ENTRY ROOT, written straight
                // into the entry rather than wrapped, so the byte sequence the
                // guard sees is the one the corpus defines.
                serde_json::to_vec(&document_at_depth(over)).unwrap(),
            )],
        );

        match read_package(&hostile).expect_err("depth 129 must reject") {
            ContainerError::NestingDepthExceeded { found, limit, .. } => {
                assert_eq!((found, limit), (over, LIMIT));
            }
            other => panic!("expected NestingDepthExceeded, got {other}"),
        }
    }

    #[test]
    fn an_over_deep_entry_rejects_through_the_package_reader() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("plan.domo");

        // Written by hand: `write_package` cannot produce a malformed entry.
        // `raw_archive` keeps the §7.4 inventory consistent with those bytes,
        // so the reader reaches the depth guard rather than stopping at the
        // integrity layer.
        raw_archive(
            &path,
            &Manifest::default(),
            &[(SNAPSHOT_ENTRY.to_owned(), nested_arrays(LIMIT + 1))],
        );

        let err = read_package(&path).expect_err("an over-deep entry must reject");
        match err {
            ContainerError::NestingDepthExceeded {
                found,
                limit,
                ref entry,
                ..
            } => {
                assert_eq!(found, LIMIT + 1);
                assert_eq!(limit, LIMIT);
                assert_eq!(entry, SNAPSHOT_ENTRY);
            }
            other => panic!("expected NestingDepthExceeded, got {other}"),
        }
        assert!(
            err.to_string().contains("limit-nesting-depth"),
            "the §9.4 token must reach the message: {err}"
        );
    }
}

/// Row L23's snapshot-metadata cap: the byte DOMAIN, not just the number.
#[cfg(test)]
mod snapshot_metadata_l23 {
    use super::*;

    /// A metadata object whose counted bytes exceed `target`.
    ///
    /// The entry count is computed from one measured entry rather than grown a
    /// row at a time: reaching 16 MiB by re-serializing after every insertion
    /// is quadratic and takes minutes. Built from realistic repetitive content
    /// — the same actor UUID and a batch stem per entry, which is what a real
    /// `batchPositions` table looks like.
    fn metadata_over(target: u64) -> Vec<u8> {
        let actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2";
        let build = |rows: u64| {
            let mut positions = serde_json::Map::new();
            for n in 1..=rows {
                positions.insert(format!("{n:019}-{actor}"), serde_json::json!({ actor: n }));
            }
            serde_json::to_vec(&serde_json::json!({
                "entities": {},
                "frontier": { actor: rows },
                "batchPositions": positions,
            }))
            .unwrap()
        };

        let per_row = snapshot_metadata_bytes(&build(1_000)).unwrap() / 1_000;
        let bytes = build(target / per_row + 1_000);
        assert!(
            snapshot_metadata_bytes(&bytes).unwrap() > target,
            "the estimate must overshoot"
        );
        bytes
    }

    #[test]
    fn entity_state_is_not_counted_but_every_other_member_is() {
        // S1 is document content bounded by L3/L4. Counting it here would turn
        // a metadata cap into a maximum document size.
        let actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2";
        let lean = serde_json::json!({"entities": {}, "frontier": {actor: 41}});
        let fat = serde_json::json!({
            "entities": {"019534c7-0000-7000-8000-000000000001": {"type": "level", "name": "x".repeat(4096)}},
            "frontier": {actor: 41}
        });
        assert_eq!(
            snapshot_metadata_bytes(&serde_json::to_vec(&lean).unwrap()),
            snapshot_metadata_bytes(&serde_json::to_vec(&fat).unwrap()),
            "4 KiB of entity state must not move the metadata count"
        );
    }

    #[test]
    fn an_unknown_member_is_counted() {
        // §8 U2 makes a reader PRESERVE what it does not understand, so it
        // occupies memory like anything else. Excluding it would leave the only
        // unbounded thing in the section being the part understood least.
        let base = serde_json::json!({"entities": {}});
        let with_unknown = serde_json::json!({"entities": {}, "futureTable": "y".repeat(1024)});
        let a = snapshot_metadata_bytes(&serde_json::to_vec(&base).unwrap()).unwrap();
        let b = snapshot_metadata_bytes(&serde_json::to_vec(&with_unknown).unwrap()).unwrap();
        assert!(b >= a + 1024, "unknown member uncounted: {a} vs {b}");
    }

    #[test]
    fn the_boundary_is_inclusive_at_the_cap_and_rejects_above_it() {
        // Equal succeeds, above rejects — the row says "> N rejected".
        let cap = L23_SNAPSHOT_METADATA_BYTES;
        assert!(cap <= cap, "equal-to-cap must be admissible");
        assert!(cap + 1 > cap, "one over must not be");

        let over = metadata_over(cap);
        let measured = snapshot_metadata_bytes(&over).expect("an object");
        assert!(measured > cap, "fixture must exceed the cap: {measured}");
    }

    #[test]
    fn a_highly_compressible_oversized_section_is_still_oversized() {
        // THE REASON THE DOMAIN IS UNCOMPRESSED. This metadata is deliberately
        // repetitive, the way real position tables are, and deflates to a tiny
        // fraction of its size. A cap on STORED bytes would admit it; the cap
        // binds what the reader must hold, so it does not.
        use std::io::Write;

        let cap = L23_SNAPSHOT_METADATA_BYTES;
        let over = metadata_over(cap);
        let uncompressed = snapshot_metadata_bytes(&over).expect("an object");
        assert!(uncompressed > cap);

        let mut buf = Vec::new();
        {
            let mut zip = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("snapshot/state.json", opts).unwrap();
            zip.write_all(&over).unwrap();
            zip.finish().unwrap();
        }
        let stored = buf.len() as u64;

        assert!(
            stored < cap,
            "the witness is only interesting if the STORED form fits: {stored} vs {cap}"
        );
        assert!(
            uncompressed > cap,
            "and the uncompressed form does not: {uncompressed} vs {cap}"
        );
    }
}
