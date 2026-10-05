//! Converting a package written by an earlier development build (ED-0108).
//!
//! The conversion is an **unpublished-layout repair**, not a migration.
//! FORMAT-SPEC §8.2 V4 lets a migration step originate only at a released
//! schema and §8.2 V6 records that none has been released, so there is nothing
//! to migrate from; the layout this reads declares no §8.1 identifier at all.
//!
//! What these tests are mostly about is what must NOT happen: the source is
//! not touched, the destination is not overwritten, and nothing the old file
//! carried disappears without saying so.

use std::io::Write;
use std::path::Path;

use domowik_container::{
    convert_development_package, read_package, ContainerError, Package, Profile,
};
use domowik_document::operation::{Batch, Operation};
use domowik_format::{Document, Entity, Level};

/// Writes a package in the pre-ED-0108 layout, with `ops/` and `blob/` at the
/// names they already had.
fn developer_package(path: &Path, manifest: serde_json::Value, batches: &[Batch]) {
    let file = std::fs::File::create(path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();

    let mut entries = vec![
        (
            "manifest.json".to_owned(),
            serde_json::to_vec(&manifest).unwrap(),
        ),
        (
            "document.json".to_owned(),
            serde_json::to_vec(&Document::with_ground_floor()).unwrap(),
        ),
    ];
    for batch in batches {
        entries.push((
            domowik_container::ops_entry_name(batch.counter_base, batch.actor_id),
            serde_json::to_vec(batch).unwrap(),
        ));
    }
    for (name, bytes) in entries {
        zip.start_file(name, options).expect("start");
        zip.write_all(&bytes).expect("write");
    }
    zip.finish().expect("finish");
}

fn one_batch() -> Batch {
    Batch::new(
        uuid::Uuid::now_v7(),
        1,
        Default::default(),
        vec![Operation::create(Entity::Level(Level::new("Upper")))],
    )
}

#[test]
fn a_converted_package_is_readable_and_keeps_its_history() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    let destination = dir.path().join("new.domo");
    let batch = one_batch();
    developer_package(
        &source,
        serde_json::json!({"formatVersion": 1, "generator": "domowik 0.1.0"}),
        std::slice::from_ref(&batch),
    );

    convert_development_package(&source, &destination).expect("convert");

    let converted = read_package(&destination).expect("the result must be readable");
    assert_eq!(converted.manifest.format_version, "0.1");
    // §7.9: this fixture carries no assets, and an empty U(P) is portable by
    // vacuous truth (issue 0064). The assertion read Thin while the writer's
    // guard sent the empty case there; the conversion itself is unchanged.
    assert_eq!(converted.manifest.profile, Profile::Portable);
    assert_eq!(converted.batches, vec![batch], "the log must survive");
    assert_eq!(converted.snapshot.levels.len(), 1);
    // Two levels after replay: the snapshot's, plus the one the batch creates.
    assert_eq!(
        converted.project().expect("projects").document.levels.len(),
        2
    );
}

#[test]
fn conversion_does_not_touch_the_source() {
    // The guarantee the whole command exists to make. Asserted on the BYTES
    // and the mtime, not on the absence of an error: a conversion that
    // rewrote the source in a way that happened to round-trip would still
    // have violated it.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    let destination = dir.path().join("new.domo");
    developer_package(
        &source,
        serde_json::json!({"formatVersion": 1, "generator": "domowik 0.1.0"}),
        &[one_batch()],
    );

    let before = std::fs::read(&source).expect("bytes");
    let before_mtime = std::fs::metadata(&source).expect("meta").modified().ok();

    convert_development_package(&source, &destination).expect("convert");

    assert_eq!(
        std::fs::read(&source).expect("bytes"),
        before,
        "the source must be byte-identical after a conversion"
    );
    assert_eq!(
        std::fs::metadata(&source).expect("meta").modified().ok(),
        before_mtime,
        "the source must not even be rewritten with identical bytes"
    );
}

#[test]
fn conversion_refuses_to_overwrite_an_existing_destination() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    let destination = dir.path().join("taken.domo");
    developer_package(
        &source,
        serde_json::json!({"formatVersion": 1}),
        &[one_batch()],
    );
    std::fs::write(&destination, b"something the user cares about").expect("occupy");

    let error = convert_development_package(&source, &destination).expect_err("must refuse");
    assert!(
        matches!(error, ContainerError::UnsafeTarget { .. }),
        "got {error}"
    );
    assert_eq!(
        std::fs::read(&destination).expect("bytes"),
        b"something the user cares about",
        "the destination must be untouched"
    );
}

#[test]
fn conversion_refuses_to_write_over_the_source_itself() {
    // `source == destination` would pass the existence check only by accident
    // of ordering, so it is refused explicitly. An in-place conversion is the
    // one shape ED-0108 rules out by name.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    developer_package(
        &source,
        serde_json::json!({"formatVersion": 1}),
        &[one_batch()],
    );

    let error = convert_development_package(&source, &source).expect_err("must refuse");
    assert!(
        matches!(error, ContainerError::UnsafeTarget { .. }),
        "got {error}"
    );
}

#[test]
fn conversion_carries_fields_that_have_no_home_in_the_new_manifest() {
    // ED-0108: no silent dropped fields. `formatVersion` as an integer and
    // `generator` are both absent from §7.4, so neither has a typed home —
    // which is exactly when a converter is tempted to discard them. They go to
    // the manifest's `unknown` map, so the converted package still records
    // which build wrote the original.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    let destination = dir.path().join("new.domo");
    developer_package(
        &source,
        serde_json::json!({
            "formatVersion": 1,
            "generator": "domowik 0.1.0",
            "someFieldNobodyRemembersAdding": {"value": 41}
        }),
        &[one_batch()],
    );

    convert_development_package(&source, &destination).expect("convert");
    let converted = read_package(&destination).expect("read");

    assert_eq!(
        converted.manifest.unknown.get("generator"),
        Some(&serde_json::json!("domowik 0.1.0")),
        "the generator must be carried, not dropped"
    );
    assert_eq!(
        converted
            .manifest
            .unknown
            .get("someFieldNobodyRemembersAdding"),
        Some(&serde_json::json!({"value": 41}))
    );
    // The old integer `formatVersion` must not survive as the NEW one, which
    // is the §8.1 string — but it must not vanish either. It moves to a nested
    // object whose name says where it came from, because emitting the key
    // twice produces a manifest that does not parse.
    assert_eq!(converted.manifest.format_version, "0.1");
    assert_eq!(
        converted
            .manifest
            .unknown
            .get("displacedByDevelopmentLayoutConversion")
            .and_then(|v| v.get("formatVersion")),
        Some(&serde_json::json!(1)),
        "the old integer version must be carried, not dropped: {:?}",
        converted.manifest.unknown
    );
}

#[test]
fn the_frontier_and_tombstones_move_into_the_snapshot_section() {
    // The old layout put both in the manifest. §7.5 says the S2 frontier lives
    // in the snapshot section and that no other location may carry or shadow
    // it, so a conversion that copied the manifest field across would produce
    // a package that is still wrong in the way that mattered most.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("old.domo");
    let destination = dir.path().join("new.domo");
    let actor = uuid::Uuid::now_v7();
    let tombstone = uuid::Uuid::now_v7();
    developer_package(
        &source,
        serde_json::json!({
            "formatVersion": 1,
            "snapshotFrontier": {actor.to_string(): 5},
            "tombstones": [tombstone]
        }),
        &[],
    );

    convert_development_package(&source, &destination).expect("convert");
    let converted = read_package(&destination).expect("read");

    assert_eq!(converted.frontier.get(&actor), Some(&5));
    assert_eq!(converted.tombstones, vec![tombstone]);

    // And the manifest must not shadow them.
    let file = std::fs::File::open(&destination).expect("open");
    let mut archive = zip::ZipArchive::new(file).expect("archive");
    let mut manifest = String::new();
    use std::io::Read;
    archive
        .by_name("domowik.json")
        .expect("manifest")
        .read_to_string(&mut manifest)
        .expect("utf8");
    assert!(
        !manifest.contains("snapshotFrontier"),
        "§7.5 forbids a second location for the frontier: {manifest}"
    );
}

#[test]
fn a_canonical_package_is_not_accepted_by_the_converter() {
    // The converter reads the OLD names. Pointing it at a current package must
    // fail rather than silently produce a copy, or `convert` becomes an
    // undocumented `cp` that users would reach for by mistake.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("current.domo");
    let destination = dir.path().join("copy.domo");
    domowik_container::write_package(&source, &Package::new(Document::with_ground_floor()))
        .expect("write");

    let error = convert_development_package(&source, &destination).expect_err("must refuse");
    assert!(
        matches!(error, ContainerError::MissingEntry { .. }),
        "got {error}"
    );
    assert!(!destination.exists(), "nothing may be created");
}
