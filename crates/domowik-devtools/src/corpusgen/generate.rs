//! Deterministic construction of the committed issue-0028 corpus artifacts.
//!
//! Ported from `internal/corpusgen/generate.go` under issue 0062. Every
//! artifact is byte-identical to the Go original's output; the committed tree
//! is the oracle, and `tests/corpus_gates.rs` compares against it on every run.
//!
//! This module contains NO reader, writer, or validator behavior for the
//! Domowik format. It materializes inert fixture bytes and nothing else.

use std::collections::BTreeMap;

use serde::Serialize;
use sha2::{Digest as _, Sha256};

use super::zipwriter::{
    crc_of, put_u32, put_u64, write_archive, Entry, EOCD_DISK_NUMBER_OFFSET, EOCD_FIXED_LEN,
    METHOD_DEFLATE, SIG_EOCD64, SIG_LOC64,
};

// Named quantities (ED-0019 discipline): every numeric constant below cites
// its definition site. No independent magnitude literals appear elsewhere.
const BLOB_A_HEX: &str = "9ca758f319e76338ee864522157a8733f1ce25b9e101e637d69a8d240b832022";

const BLOB_A: &[u8] = b"domowik synthetic background tile A (issue 0028 golden fixture)";

const ACTOR_X: &str = "01948f5a-ca77-7a09-9e76-5ace11b12fd2";

const EMPTY_SNAPSHOT: &str = r#"{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}"#;

const EMPTY_DESCRIPTORS: &str = "{}";

const BATCH_DOC: &str = r#"{"batchId":"01948f5b-c4e4-7e42-b996-f552a18df77f","actorId":"01948f5a-ca77-7a09-9e76-5ace11b12fd2","counterBase":42,"frontier":{"01948f5a-ca77-7a09-9e76-5ace11b12fd2":41},"operations":[{"operationId":"01948f5d-6e18-7c01-8f0d-77f974bd8036","kind":"createEntity","entityId":"01948f55-6b18-7d3a-a153-1ee2f33cc402","fields":{"id":"01948f55-6b18-7d3a-a153-1ee2f33cc402","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}}]}"#;

/// `counterBase` zero-padded to the 19 digits §7.6 requires.
const BATCH_COUNTER_BASE: u64 = 42;

// ===== Pinned DEFLATE streams =====
//
// THE GENERATOR DOES NOT COMPRESS. Two hostile fixtures ship a raw DEFLATE
// stream, and both were produced by Go's `compress/flate` at `BestCompression`
// when the corpus was authored. No Rust compressor reproduces those bytes:
// `flate2`/`miniz_oxide` was measured at every level from 0 to 9 against both
// payloads and matched neither, differing in LENGTH as well as content (372 vs
// 369 bytes for the twin, 51 vs 55 for the snapshot). Recompressing here would
// therefore change two committed artifacts, their recorded sha256 digests in
// `EXPECTATIONS.md`, and the ratio sidecar — for no gain in what the fixtures
// assert.
//
// So the streams are pinned as recorded generator inputs, with their
// provenance and digests below, and `tests/corpus_gates.rs` INFLATES each
// stream and requires it to equal the payload the package declares.
//
// THE TRADE, NAMED (ED-0104). This is a DIFFERENT guarantee, not a better one.
// Recompression proved that the stored stream is REGENERABLE from its payload
// by a named compressor; inflation proves that the stored stream EXPANDS to the
// payload the package declares. Each covers what the other does not:
// regeneration would catch a payload edit that inflation cannot see if the
// stream were edited to match, and inflation catches a stream that expands to
// the wrong document, which regeneration never asserted. What is lost is real —
// payload-to-compressor regeneration — and what replaces it is a semantic check
// plus a pinned digest, not a superset.
//
// A future compressed fixture is not blocked by this: it carries its own
// stream the same way, generated once by any deterministic encoder and pinned
// with the same inflate check.

/// Raw DEFLATE of [`EMPTY_SNAPSHOT`].
///
/// Provenance: Go 1.27.0 `compress/flate` at `BestCompression`, the encoder
/// that produced `hostile/container/limit-expansion-ratio-over-L15.domo`.
/// sha256 of the stream: `08ee023e9aecf884642b526c1c450fa6c8f29109be0d83ad9df5a61af09539fb`.
const EMPTY_SNAPSHOT_DEFLATE_HEX: &str = concat!(
    "aa564acd2bc92cc94c2d56b2aaaed5514a2bcacf2bc94c2d82f0ca33f3f2528b",
    "bc532ba1b225f9b949c525f97910d5b5800100",
);

/// Raw DEFLATE of [`ratio_twin_payload`].
///
/// Provenance: Go 1.27.0 `compress/flate` at `BestCompression`, the encoder
/// that produced `hostile/container/ratio-under-L15-success-twin.domo`.
/// sha256 of the stream: `3ff7c08f44968924d70c564d2341999330c0d6a07d10821250ff9b406ca806ac`.
const RATIO_TWIN_DEFLATE_HEX: &str = concat!(
    "eccc314a053110c7e1ab486a4ff04a3b0bc15e2cf276474c6102c980c5b27717",
    "d142de056cbeaf9b1fc3ff28d1b3658b552ec7795fdee6e8d962fe5cd79adbfb",
    "f3582ddbe8bf1f9fadf79837719b5133f69b9ae3e3ba72f4587fe69e22eb5eb3",
    "96cb51666cd1f3e13b3feeab5c5e5ecf3b000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000000000000000000000000000000000000000",
    "0000000000000000000000000000e09f9d5f0300",
);

/// Decodes one of the pinned hex streams.
///
/// # Panics
///
/// Panics on a malformed constant, which is a source defect rather than a
/// runtime condition.
fn unhex(s: &str) -> Vec<u8> {
    assert!(
        s.len().is_multiple_of(2),
        "pinned stream has an odd hex length"
    );
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).expect("pinned stream is not hex"))
        .collect()
}

/// Raw DEFLATE of [`EMPTY_SNAPSHOT`], as shipped.
pub fn empty_snapshot_deflate() -> Vec<u8> {
    unhex(EMPTY_SNAPSHOT_DEFLATE_HEX)
}

/// Raw DEFLATE of [`ratio_twin_payload`], as shipped.
pub fn ratio_twin_deflate() -> Vec<u8> {
    unhex(RATIO_TWIN_DEFLATE_HEX)
}

/// The logical payload of the under-L15 twin, for the inflate check.
pub fn empty_snapshot_payload() -> Vec<u8> {
    EMPTY_SNAPSHOT.as_bytes().to_vec()
}

/// One generated corpus file.
#[derive(Clone, Debug)]
pub struct Artifact {
    /// Path relative to `testdata/corpus`, always with forward slashes.
    pub rel_path: String,
    /// The exact committed bytes.
    pub bytes: Vec<u8>,
}

fn sha256hex(b: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(b);
    h.finalize().iter().map(|x| format!("{x:02x}")).collect()
}

/// Serializes `v` the way Go's `encoding/json` would.
///
/// Go HTML-escapes `<`, `>` and `&` in string values and `serde_json` does not,
/// so a payload containing one of them would silently diverge from the
/// committed bytes as the corpus grows. No current fixture contains one; this
/// refuses to emit one rather than emitting something different from what the
/// Go generator did.
///
/// # Panics
///
/// Panics when the encoded document contains a character Go would have escaped.
fn must_json<T: Serialize>(v: &T) -> Vec<u8> {
    let out = serde_json::to_vec(v).expect("corpus JSON must serialize");
    let text = std::str::from_utf8(&out).expect("serde_json emits UTF-8");
    for bad in ['<', '>', '&', '\u{2028}', '\u{2029}'] {
        assert!(
            !text.contains(bad),
            "generated JSON contains {bad:?}, which Go's encoding/json escapes and serde_json \
             does not; the committed bytes and this generator would diverge"
        );
    }
    out
}

#[derive(Serialize)]
struct Manifest {
    #[serde(rename = "formatVersion")]
    format_version: &'static str,
    profile: String,
    #[serde(rename = "assetRoot")]
    asset_root: Vec<AssetRootEntry>,
    entries: BTreeMap<String, DigestRecord>,
}

#[derive(Serialize)]
struct AssetRootEntry {
    digest: String,
}

#[derive(Serialize)]
struct DigestRecord {
    sha256: String,
    #[serde(rename = "byteLength")]
    byte_length: usize,
}

/// Builds the §7.4 manifest document over `others`.
///
/// `asset_root` is initialized non-empty-but-present so an EMPTY root
/// serializes as `[]` and never as `null`: FS 7.4 defines assetRoot as a
/// required ascending-by-digest ARRAY, and null is not an array. The reader
/// currently tolerates null, but that is parser laxity and cannot make a
/// fixture conforming.
fn build_manifest(profile: &str, roots: &[&str], others: &[Entry]) -> Vec<u8> {
    let mut m = Manifest {
        format_version: "0.1",
        profile: profile.to_string(),
        asset_root: Vec::new(),
        entries: BTreeMap::new(),
    };
    for r in roots {
        m.asset_root.push(AssetRootEntry {
            digest: format!("sha256:{r}"),
        });
    }
    for e in others {
        if e.name.as_slice() == b"domowik.json" {
            continue;
        }
        m.entries.insert(
            // Lossy on purpose: Go's encoding/json replaces invalid UTF-8 in a
            // map key with U+FFFD rather than failing, so a hostile non-UTF-8
            // entry name lands in the manifest the same way it did there.
            String::from_utf8_lossy(&e.name).into_owned(),
            DigestRecord {
                sha256: format!("sha256:{}", sha256hex(&e.data)),
                byte_length: e.data.len(),
            },
        );
    }
    must_json(&m)
}

#[derive(Serialize)]
struct Provenance {
    #[serde(rename = "acquiredAt")]
    acquired_at: &'static str,
    author: &'static str,
    #[serde(rename = "displayName")]
    display_name: &'static str,
    #[serde(rename = "sourceDescription")]
    source_description: &'static str,
}

/// Field order matches the key order Go's `encoding/json` produces for a
/// `map[string]any`: ascending by key, at every level.
#[derive(Serialize)]
struct BlobDescriptor {
    #[serde(rename = "byteLength")]
    byte_length: usize,
    digest: String,
    encoding: &'static str,
    #[serde(rename = "mediaType")]
    media_type: &'static str,
    provenance: Provenance,
    purpose: &'static str,
    requirement: &'static str,
}

fn blob_descriptors() -> Vec<u8> {
    let mut m: BTreeMap<String, BlobDescriptor> = BTreeMap::new();
    m.insert(
        format!("sha256:{BLOB_A_HEX}"),
        BlobDescriptor {
            byte_length: BLOB_A.len(),
            digest: format!("sha256:{BLOB_A_HEX}"),
            encoding: "identity",
            media_type: "image/png",
            provenance: Provenance {
                acquired_at: "2026-08-24T00:00:00Z",
                author: "",
                display_name: "Synthetic floor tile",
                source_description: "authored for the issue 0028 corpus",
            },
            purpose: "background-image",
            requirement: "required",
        },
    );
    must_json(&m)
}

fn ops_entry_name() -> String {
    format!("ops/{BATCH_COUNTER_BASE:019}-{ACTOR_X}.json")
}

fn blob_entry_name() -> String {
    format!("blob/sha256/{}/{}", &BLOB_A_HEX[..2], &BLOB_A_HEX[2..])
}

/// Orders entries ascending by name, matching the Go generator's stable
/// insertion sort. Physical order is not semantic; a stable rule keeps the
/// bytes reproducible.
fn sort_entries(es: &mut [Entry]) {
    es.sort_by(|a, b| a.name.cmp(&b.name));
}

fn thin_others(snapshot: &[u8]) -> Vec<Entry> {
    vec![
        Entry::new("assets/descriptors.json", EMPTY_DESCRIPTORS.as_bytes()),
        Entry::new(ops_entry_name(), BATCH_DOC.as_bytes()),
        Entry::new("snapshot/state.json", snapshot),
    ]
}

/// The canonical thin entry set carrying `snapshot`, with `domowik.json` built
/// over exactly those bytes.
///
/// Any fixture whose oracle is SUCCESS must be assembled this way so the
/// manifest inventory describes the payload it actually ships.
fn base_with_snapshot(snapshot: &[u8]) -> Vec<Entry> {
    let others = thin_others(snapshot);
    let mut all = vec![Entry::new(
        "domowik.json",
        build_manifest("thin", &[], &others),
    )];
    all.extend(others);
    sort_entries(&mut all);
    all
}

/// The canonical thin entries plus `domowik.json`, before archive
/// serialization, so hostile variants can patch single fields.
fn base_for() -> Vec<Entry> {
    base_with_snapshot(EMPTY_SNAPSHOT.as_bytes())
}

fn portable_others() -> Vec<Entry> {
    vec![
        Entry::new("assets/descriptors.json", blob_descriptors()),
        Entry::new(blob_entry_name(), BLOB_A),
        Entry::new(ops_entry_name(), BATCH_DOC.as_bytes()),
        Entry::new("snapshot/state.json", EMPTY_SNAPSHOT.as_bytes()),
    ]
}

fn base_for_port() -> Vec<Entry> {
    let others = portable_others();
    let mut all = vec![Entry::new(
        "domowik.json",
        build_manifest("portable", &[BLOB_A_HEX], &others),
    )];
    all.extend(others);
    sort_entries(&mut all);
    all
}

/// A conforming thin package (store method throughout).
fn valid_thin() -> Vec<u8> {
    write_archive(&base_for(), None)
}

/// A conforming portable package embedding blob A.
fn valid_portable() -> Vec<u8> {
    write_archive(&base_for_port(), None)
}

fn serialize(es: &[Entry]) -> Vec<u8> {
    write_archive(es, None)
}

fn with_patched(index: usize, mutate: impl FnOnce(&mut Entry), es: &[Entry]) -> Vec<Entry> {
    let mut out = es.to_vec();
    mutate(&mut out[index]);
    out
}

/// Returns `es` minus the entry carrying `name`.
///
/// Callers that need to remove a specific logical entry must use this rather
/// than a slice index: [`sort_entries`] orders by name, so positions are not
/// stable.
///
/// # Panics
///
/// Panics when no entry carries `name`.
fn without_name(es: &[Entry], name: &str) -> Vec<Entry> {
    let out: Vec<Entry> = es
        .iter()
        .filter(|e| e.name.as_slice() != name.as_bytes())
        .cloned()
        .collect();
    assert!(out.len() != es.len(), "no entry {name}");
    out
}

/// Index of the entry named `name`.
///
/// # Panics
///
/// Panics when no entry carries `name`.
fn find_name(es: &[Entry], name: &str) -> usize {
    es.iter()
        .position(|e| e.name.as_slice() == name.as_bytes())
        .unwrap_or_else(|| panic!("no entry {name}"))
}

fn mutate_byte(b: &[u8], i: usize) -> Vec<u8> {
    let mut out = b.to_vec();
    out[i] ^= 0xff;
    out
}

/// The complete FS 5.1 S1-S5 snapshot document for an empty projection, in the
/// member set and order the writer itself emits: S1 entities and
/// createdPositions, S2 frontier, S3 batchPositions and winnerPositions, S4
/// tombstones, S5 batchMetadata.recentBatchIds.
///
/// The reader does not treat these members as optional: §5.1 says a snapshot
/// contains ALL of S1-S5, and omitting one is absent required data.
const EMPTY_WRITER_SNAPSHOT: &str = concat!(
    r#"{"entities":{},"frontier":{},"batchPositions":{},"#,
    r#""winnerPositions":{},"createdPositions":{},"tombstones":{},"#,
    r#""batchMetadata":{"recentBatchIds":[]}}"#,
);

/// Builds the under-L15 twin's logical payload: the complete empty S1-S5
/// snapshot document, padded to [`MICRO_BOMB_LOGICAL_BYTES`] with one run of
/// insignificant inter-token whitespace (FS 3.3), so a genuine high-ratio
/// deflate stream still expands to a fully conforming snapshot.
///
/// Public so the gates recompute the expansion from the SAME bytes the package
/// ships instead of assuming an incompressible-run stand-in.
pub fn ratio_twin_payload() -> Vec<u8> {
    // Pad before the final brace: whitespace between tokens is insignificant,
    // so the padded document parses identically.
    let (head, tail) = EMPTY_WRITER_SNAPSHOT.split_at(EMPTY_WRITER_SNAPSHOT.len() - 1);
    let pad = MICRO_BOMB_LOGICAL_BYTES - head.len() - tail.len();
    let mut out = Vec::with_capacity(MICRO_BOMB_LOGICAL_BYTES);
    out.extend_from_slice(head.as_bytes());
    out.extend(std::iter::repeat_n(b' ', pad));
    out.extend_from_slice(tail.as_bytes());
    out
}

#[derive(Serialize)]
struct RatioTwinMeta {
    #[serde(rename = "logicalBytes")]
    logical_bytes: i64,
    #[serde(rename = "ratioPercent")]
    ratio_percent: i64,
    #[serde(rename = "storedBytes")]
    stored_bytes: i64,
}

/// Every generated corpus artifact, in the order the Go generator emitted them.
///
/// # Panics
///
/// Panics when a fixture's own preconditions are broken — a missing entry name
/// or a JSON payload Go and `serde_json` would encode differently. Both are
/// source defects in this file.
#[must_use]
pub fn all_artifacts() -> Vec<Artifact> {
    let mut arts: Vec<Artifact> = Vec::new();
    let mut add = |rel: &str, b: Vec<u8>| {
        arts.push(Artifact {
            rel_path: rel.to_string(),
            bytes: b,
        });
    };

    add("golden/packages/thin-valid.domo", valid_thin());
    add("golden/packages/portable-valid.domo", valid_portable());

    let base = base_for();
    let snap_idx = find_name(&base, "snapshot/state.json");

    add("hostile/container/empty-file.domo", Vec::new());
    add(
        "hostile/container/bad-signature.domo",
        mutate_byte(&valid_thin(), 0),
    );
    add(
        "hostile/container/truncated-header.domo",
        valid_thin()[10..].to_vec(),
    );
    let vt = valid_thin();
    add(
        "hostile/container/truncated-eocd.domo",
        vt[..vt.len() - 8].to_vec(),
    );
    let mut trailing = vt.clone();
    trailing.extend(std::iter::repeat_n(0x44u8, 16));
    add("hostile/container/trailing-data.domo", trailing);
    let mut concatenated = vt.clone();
    concatenated.extend_from_slice(&vt);
    add("hostile/container/concatenated-packages.domo", concatenated);

    // Central-directory offset moved into the middle of the first local
    // payload region: overlapping regions (T1).
    add(
        "hostile/container/overlapping-regions.domo",
        write_archive(
            &base,
            Some(&|b: &mut Vec<u8>| {
                let n = b.len();
                b[n - 6] = 0x20; // low byte of cdOffset -> points inside local data
            }),
        ),
    );

    add(
        "hostile/container/malformed-length.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.local_only_uncomp = Some(e.data.len() as u32 + DECLARED_LENGTH_LIE_DELTA),
            &base,
        )),
    );
    add(
        "hostile/container/encrypted-entry.domo",
        serialize(&with_patched(snap_idx, |e| e.flags |= 0x0001, &base)),
    );
    add(
        "hostile/container/unsupported-method.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.method = UNSUPPORTED_METHOD_PROBE,
            &base,
        )),
    );
    add(
        "hostile/container/checksum-error.domo",
        serialize(&with_patched(
            snap_idx,
            |e| {
                let c = crc_of(&e.data) ^ 0xffff;
                e.override_crc = Some(c);
                e.central_only_crc = Some(c);
            },
            &base,
        )),
    );
    add(
        "hostile/container/utf8-flag-clear.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.suppress_utf8_flag = true,
            &base,
        )),
    );
    add(
        "hostile/container/nonzero-comment.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.comment = "hello".to_string(),
            &base,
        )),
    );
    // §7.2's comment rule has TWO limbs — "Entry comments AND the archive
    // comment MUST be zero-length" — and `nonzero-comment.domo` above exercises
    // only the entry limb. A reader that enforces entry comments and ignores the
    // EOCD's own comment passed the whole corpus (issue 0029, `corpus-defect`).
    //
    // BOTH halves of the EOCD comment are written: the bytes, and the
    // commentLength field they follow. Writing only the bytes leaves them
    // outside the declared record, so the reject becomes `trailing-data` under
    // T1 and the comment rule goes untested again; writing only the length
    // declares a comment that is not there and truncates the archive. Either
    // way the fixture would look like coverage and not be — which is how the
    // entry-limb-only fixture got here.
    add(
        "hostile/container/nonzero-archive-comment.domo",
        write_archive(
            &base,
            Some(&|b: &mut Vec<u8>| {
                let comment = ARCHIVE_COMMENT.as_bytes();
                let n = b.len();
                b[n - 2..n].copy_from_slice(&(comment.len() as u16).to_le_bytes());
                b.extend_from_slice(comment);
            }),
        ),
    );

    add(
        "hostile/container/foreign-extra-field.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.extra = vec![0x75, 0x70, 2, 0, 1, 2],
            &base,
        )),
    );
    add(
        "hostile/container/version-needed-over.domo",
        serialize(&with_patched(
            snap_idx,
            |e| e.version_needed = MAX_SUPPORTED_VERSION_NEEDED + 1,
            &base,
        )),
    );
    add(
        "hostile/container/directory-entry-attr.domo",
        serialize(&with_patched(snap_idx, |e| e.ext_attrs = 0x0010, &base)),
    );
    add(
        "hostile/container/symlink-designation.domo",
        serialize(&with_patched(
            snap_idx,
            |e| {
                e.version_made_by = 0x0300;
                e.ext_attrs = 0o120_777 << 16;
            },
            &base,
        )),
    );

    let mut dup = base.clone();
    dup.push(Entry::new("snapshot/state.json", b"{}".to_vec()));
    add(
        "hostile/container/duplicate-logical-name.domo",
        serialize(&dup),
    );

    let mut casepair = base.clone();
    casepair.push(Entry::new("SNAPSHOT/STATE.JSON", b"{}".to_vec()));
    sort_entries(&mut casepair);
    add(
        "hostile/container/name-casefold-collision.domo",
        serialize(&casepair),
    );

    // The second manifest is built over the SAME entry set as the first, so the
    // only difference between them is the case-folded name. It was built over
    // `base[1..]` — the descriptors-less slice — which made the two manifests
    // disagree on their inventories for a reason the fixture does not name.
    //
    // What the reader does with an inventory that must list `DOMOWIK.JSON`,
    // itself a name the closed namespace forbids, is NOT settled here: §7.4
    // says the reject is `duplicate-logical-name` at name validation, which is
    // outermost per §9.5 and fires before either inventory is read. The
    // residual is recorded in EXPECTATIONS.md and routed to issue 0004.
    let mut ambig = base.clone();
    ambig.push(Entry::new(
        "DOMOWIK.JSON",
        build_manifest("thin", &[], &base_others_for_ambiguous()),
    ));
    sort_entries(&mut ambig);
    add(
        "hostile/container/ambiguous-manifest.domo",
        serialize(&ambig),
    );

    // FS 9.4 missing-manifest row: a structurally valid archive with no
    // domowik.json at all. The entry is removed BY NAME; sort_entries orders
    // ascending, so "assets/descriptors.json" precedes it and a positional
    // drop removes the wrong entry.
    add(
        "hostile/container/missing-manifest.domo",
        serialize(&without_name(&base, "domowik.json")),
    );

    // A name fixture must carry EXACTLY ONE defect: the hostile name.
    //
    // It therefore ships the full canonical entry set plus the hostile entry,
    // with `domowik.json` rebuilt OVER that set — §7.4 requires `entries` to
    // map "EVERY other logical entry name", so an archive member the inventory
    // omits is a second, unrelated defect.
    //
    // This previously sliced the sorted base at index 1, meaning "everything
    // but the manifest". Sorted ascending, index 0 is
    // `assets/descriptors.json`, so the slice dropped the DESCRIPTORS and kept
    // a manifest that still inventoried them. Every one of these fixtures
    // shipped a missing inventory member and an un-inventoried extra member on
    // top of the name it meant to isolate (issue 0062 review; ED-0104).
    let traverse = |name: &str| -> Vec<Entry> {
        let mut others = thin_others(EMPTY_SNAPSHOT.as_bytes());
        others.push(Entry::new(name, b"hostile".to_vec()));
        let mut all = vec![Entry::new(
            "domowik.json",
            build_manifest("thin", &[], &others),
        )];
        all.extend(others);
        sort_entries(&mut all);
        all
    };
    add(
        "hostile/container/traversal-dotdot.domo",
        serialize(&traverse("../domowik.json")),
    );
    add(
        "hostile/container/absolute-path.domo",
        serialize(&traverse("/snapshot/state.json")),
    );
    add(
        "hostile/container/drive-form.domo",
        serialize(&traverse("C:snapshot-state.json")),
    );
    add(
        "hostile/container/backslash-name.domo",
        serialize(&traverse("snapshot\\state.json")),
    );
    add(
        "hostile/container/nul-byte-name.domo",
        serialize(&traverse("snapshot/state\u{0}x.json")),
    );
    add(
        "hostile/container/dot-segment-name.domo",
        serialize(&traverse("snapshot/./state.json")),
    );
    add(
        "hostile/container/overlong-component-L9.domo",
        serialize(&traverse(&format!("preview/{}.json", "a".repeat(300)))),
    );
    add(
        "hostile/container/overlong-name-L8.domo",
        serialize(&traverse(&format!("{}c.json", "b/".repeat(600)))),
    );

    // FS 7.2 single-disk requirement: "this disk number" != 0. The field is
    // addressed from the END of the fixed-length EOCD record so the patch
    // cannot drift into cdOffset.
    add(
        "hostile/container/spanned-disk-number.domo",
        write_archive(
            &base,
            Some(&|b: &mut Vec<u8>| {
                let n = b.len();
                b[n - EOCD_FIXED_LEN + EOCD_DISK_NUMBER_OFFSET] = 1;
            }),
        ),
    );

    // Zip64 locator with contradictory trailing offset appended after EOCD.
    let mut loc = [0u8; 20];
    put_u32(&mut loc[0..], SIG_LOC64);
    put_u32(&mut loc[4..], 0xdead_beef);
    put_u64(&mut loc[8..], 1 << 40);
    put_u32(&mut loc[16..], 1);
    let mut with_loc = vt.clone();
    with_loc.extend_from_slice(&loc);
    add("hostile/container/zip64-malformed-locator.domo", with_loc);

    let mut eocd64 = [0u8; 44]; // truncated: the real record is 56 bytes
    put_u32(&mut eocd64[0..], SIG_EOCD64);
    let mut with_eocd64 = vt.clone();
    with_eocd64.extend_from_slice(&eocd64);
    add("hostile/container/zip64-truncated-eocd64.domo", with_eocd64);

    // Declared expanded size one byte beyond the ratified L3 quantity in BOTH
    // headers with a tiny honest stream: the deterministic rejection class is
    // malformed-length (FS 9.4 declared-vs-observed row; FS 9.5 physical stage
    // wins), with the L3 boundary expressed only through the constant.
    add(
        "hostile/container/declared-size-lie-limit-L3.domo",
        serialize(&with_patched(
            snap_idx,
            |e| {
                let lie = L3_ENTRY_EXPANDED_BYTES + 1;
                e.local_only_uncomp = Some(lie);
                e.override_uncomp = Some(lie);
            },
            &base,
        )),
    );

    // Genuine high-ratio stream just UNDER L15 (inert boundary twin), whose
    // oracle is SUCCESS: the package must therefore stay conforming end to end.
    // The payload is a valid FS 5.1 snapshot document padded to the corpus
    // safety cap with insignificant inter-token whitespace (FS 3.3), and the
    // manifest is rebuilt OVER the patched entry set.
    let ratio_data = ratio_twin_payload();
    let stream = ratio_twin_deflate();
    let ratio_entries = base_with_snapshot(&ratio_data);
    let ratio_idx = find_name(&ratio_entries, "snapshot/state.json");
    let stream_for_entry = stream.clone();
    add(
        "hostile/container/ratio-under-L15-success-twin.domo",
        serialize(&with_patched(
            ratio_idx,
            move |e| {
                e.method = METHOD_DEFLATE;
                e.stored_override = Some(stream_for_entry);
            },
            &ratio_entries,
        )),
    );
    add(
        "hostile/container/ratio-under-L15-success-twin.meta.json",
        must_json(&RatioTwinMeta {
            logical_bytes: ratio_data.len() as i64,
            stored_bytes: stream.len() as i64,
            ratio_percent: ratio_data.len() as i64 * 100 / stream.len() as i64,
        }),
    );

    // Declared-metadata probe ABOVE the L15 ratio branch threshold: the
    // declared uncompressed size encodes >1000:1 against a stored stream at
    // least L15_SMALL_INPUT_FLOOR_BYTES long (FS 9.3 L15 switches to an
    // absolute expanded bound below that floor). Expected token is
    // malformed-length: FS 9.4/9.5 make the physical declared-vs-observed
    // stage deterministic.
    add(
        "hostile/container/limit-expansion-ratio-over-L15.domo",
        serialize(&with_patched(
            snap_idx,
            |e| {
                let stream = empty_snapshot_deflate();
                let stored_len = stream.len().max(L15_SMALL_INPUT_FLOOR_BYTES) as u32;
                e.method = METHOD_DEFLATE;
                e.stored_override = Some(stream);
                // Declared ratio one step beyond L15's rejection point.
                let lie = stored_len * (L15_REJECTION_RATIO + 1);
                e.override_uncomp = Some(lie);
                e.local_only_uncomp = Some(lie);
            },
            &base,
        )),
    );

    // Portable package whose embedded blob no longer matches its listed
    // digest: portability claim broken (missing-asset-equivalence).
    add(
        "hostile/container/digest-mismatch-portable.domo",
        portable_broken_digest(),
    );

    arts
}

/// The entry set the ambiguous-manifest fixture's SECOND manifest describes:
/// the same canonical thin entries the first one describes.
fn base_others_for_ambiguous() -> Vec<Entry> {
    thin_others(EMPTY_SNAPSHOT.as_bytes())
}

fn portable_broken_digest() -> Vec<u8> {
    let mut es = base_for_port();
    // Corrupt the blob payload AFTER the manifest was built from it.
    let i = find_name(&es, &blob_entry_name());
    es[i].data[0] ^= 0xff;
    serialize(&es)
}

/// Named corpus safety parameter (not a specification quantity): committed
/// artifacts stay at or under this bound so every fixture remains inert.
/// Introduced by reviewer amendment 2 and recorded in
/// `testdata/corpus/README.md`.
pub const CORPUS_ARTIFACT_CAP_BYTES: u64 = 256 << 10;

/// The logical expansion of the under-L15 genuine stream twin, chosen as the
/// corpus safety cap itself: the largest inert size the safety gate permits.
const MICRO_BOMB_LOGICAL_BYTES: usize = CORPUS_ARTIFACT_CAP_BYTES as usize;

/// Restates the FS 9.3 row L15 boundary quantity: inputs whose physical size is
/// below this floor use the absolute expanded bound instead of the ratio, so
/// declared-ratio probes must store at least this many bytes to engage the
/// ratio branch deterministically.
const L15_SMALL_INPUT_FLOOR_BYTES: usize = 64;

/// An arbitrary-but-fixed contradiction margin for the malformed-length probe;
/// the class is triggered by ANY declared-vs-observed mismatch, so the delta
/// carries no limit semantics.
const DECLARED_LENGTH_LIE_DELTA: u32 = 4096;

/// The archive comment the §7.2 archive-comment limb probe carries. Any
/// non-empty value violates the rule identically; the text names the fixture so
/// a hexdump explains itself.
const ARCHIVE_COMMENT: &str = "domowik corpus archive comment probe";

/// An arbitrary method ID outside the ratified FS 7.7 allowlist {0 store,
/// 8 deflate}; any such value rejects identically.
const UNSUPPORTED_METHOD_PROBE: u16 = 12;

/// Restates the FS 7.2 version bound: values above 45 reject as
/// unsupported-version.
const MAX_SUPPORTED_VERSION_NEEDED: u16 = 45;

/// Restates the FS 9.3 row L15 ratified quantity: expansion ratios greater than
/// 1000:1 reject; exactly 1000 succeeds.
const L15_REJECTION_RATIO: u32 = 1000;

/// Restates the FS 9.3 row L3 ratified quantity (per-entry expanded bytes,
/// 512 MiB); used only as the reference point (+1) for the declared-size
/// contradiction probe.
const L3_ENTRY_EXPANDED_BYTES: u32 = 1 << 29;

#[cfg(test)]
mod tests {
    use super::*;

    /// The Go generator used `encoding/json`, which HTML-escapes `<`, `>` and
    /// `&`. `serde_json` does not. No current fixture contains one, so the
    /// guard has nothing to catch today — which is exactly when a guard is
    /// worth proving, because the corpus is expected to grow.
    #[test]
    #[should_panic(expected = "which Go's encoding/json escapes")]
    fn html_escaping_divergence_is_refused() {
        let mut m: BTreeMap<String, String> = BTreeMap::new();
        m.insert("k".to_string(), "a & b".to_string());
        let _ = must_json(&m);
    }

    /// The twin payload is a conforming snapshot document padded with
    /// insignificant whitespace, not an incompressible run.
    #[test]
    fn ratio_twin_payload_is_a_padded_snapshot_document() {
        let payload = ratio_twin_payload();
        assert_eq!(payload.len(), MICRO_BOMB_LOGICAL_BYTES);
        let doc: serde_json::Value =
            serde_json::from_slice(&payload).expect("the padded payload still parses");
        for member in [
            "entities",
            "frontier",
            "batchPositions",
            "winnerPositions",
            "createdPositions",
            "tombstones",
            "batchMetadata",
        ] {
            assert!(
                doc.get(member).is_some(),
                "the twin payload must carry the complete FS 5.1 S1-S5 member set; {member} is absent"
            );
        }
    }

    /// Go strings carry arbitrary bytes, so the Go generator could author a
    /// hostile fixture whose entry name is not valid UTF-8 — a real §7.6 name
    /// vector. A `String` here could not, so the name is raw bytes and this
    /// proves they reach the archive untouched, in both headers.
    #[test]
    fn entry_names_may_be_invalid_utf8() {
        let name = vec![b's', 0xff, 0xfe, b'.', b'j', b's', b'o', b'n'];
        let out = write_archive(&[Entry::new(name.clone(), b"x".to_vec())], None);
        let occurrences = out
            .windows(name.len())
            .filter(|w| *w == name.as_slice())
            .count();
        assert_eq!(
            occurrences, 2,
            "the raw name must appear verbatim in the local and the central header"
        );
    }

    /// A compressed entry without its stream is a generator defect, not a
    /// fixture: the generator does not compress.
    #[test]
    #[should_panic(expected = "does not compress")]
    fn deflate_without_a_stored_stream_is_refused() {
        let e = Entry {
            method: METHOD_DEFLATE,
            ..Entry::new("snapshot/state.json", EMPTY_SNAPSHOT.as_bytes())
        };
        let _ = write_archive(&[e], None);
    }
}
