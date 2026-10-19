//! `domowik asset cache …` — ASSET-STRATEGY §10.7's stated surface.

use std::path::Path;

use domowik_cli::{
    cmd_cache_gc, cmd_cache_pin, cmd_cache_rebuild, cmd_cache_status, cmd_cache_unpin,
};

const DIGEST: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn admit(root: &Path, bytes: &[u8]) -> String {
    let store = domowik_assets::CacheStore::open(root, domowik_assets::AdmissionLimits::default())
        .expect("cache");
    let digest = domowik_container::digest_of(bytes);
    store.admit_bytes(&digest, bytes).expect("admit");
    digest
}

#[test]
fn a_pinned_blob_survives_collection_and_an_unpinned_one_does_not() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let kept = admit(root, b"pinned");
    let loose = admit(root, b"unpinned");

    cmd_cache_pin(root, &kept, "cli").expect("pin");
    let report = cmd_cache_gc(root, false).expect("gc");
    assert!(report.contains("Collected 1 blob(s)"), "{report}");
    assert!(report.contains(&loose), "{report}");

    let status = cmd_cache_status(root).expect("status");
    assert!(status.contains("1 blob(s)"), "{status}");
    assert!(status.contains("1 pin(s)"), "{status}");
    assert!(status.contains("collector: ready"), "{status}");
}

#[test]
fn a_pin_may_precede_the_blob_it_protects() {
    // §10.7 G2: admission atomically restores protection, so pinning first and
    // admitting afterwards is the ordering that cannot lose a race.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let message = cmd_cache_pin(root, DIGEST, "cli").expect("pin");
    assert!(message.contains("does not hold it yet"), "{message}");

    let digest = admit(root, b"");
    assert_eq!(digest, DIGEST, "the empty input's published SHA-256");
    let report = cmd_cache_gc(root, false).expect("gc");
    assert!(report.contains("0 blob(s); 1 retained"), "{report}");
}

#[test]
fn a_malformed_digest_is_a_usage_error_and_a_missing_pin_is_a_failure() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();

    let usage = cmd_cache_pin(root, "not-a-digest", "cli").expect_err("malformed");
    assert_eq!(usage.code(), 2, "a bad argument must exit 2, not 1");

    let failure = cmd_cache_unpin(root, DIGEST, "cli").expect_err("no such pin");
    assert_eq!(failure.code(), 1);
}

#[test]
fn unpinning_releases_protection_so_the_next_collection_takes_the_blob() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let temporary = admit(root, b"temporarily protected");
    let permanent = admit(root, b"stays pinned throughout");

    // A second, lasting pin keeps the cache registered, so the collection
    // below tests what unpinning does rather than tripping the unregistered
    // guard.
    cmd_cache_pin(root, &permanent, "cli").expect("pin the lasting one");
    cmd_cache_pin(root, &temporary, "cli").expect("pin the temporary one");
    assert!(cmd_cache_gc(root, false)
        .expect("gc")
        .contains("0 blob(s); 2 retained"));

    cmd_cache_unpin(root, &temporary, "cli").expect("unpin");
    let report = cmd_cache_gc(root, false).expect("gc");
    assert!(report.contains("Collected 1 blob(s)"), "{report}");
    assert!(report.contains(&temporary), "{report}");

    let store = domowik_assets::CacheStore::open(root, domowik_assets::AdmissionLimits::default())
        .expect("cache");
    assert!(store.contains(&permanent));
    assert!(!store.contains(&temporary));
}

#[test]
fn a_damaged_journal_stops_collection_and_says_so_with_a_non_zero_exit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let loose = admit(root, b"would otherwise be collected");
    cmd_cache_pin(root, DIGEST, "cli").expect("pin");

    std::fs::write(
        root.join("meta/journal"),
        std::fs::read_to_string(root.join("meta/journal")).expect("journal") + "garbage\n",
    )
    .expect("damage");

    let error = cmd_cache_gc(root, false).expect_err("a fail-closed collector must not exit 0");
    assert_eq!(error.code(), 1);
    assert!(
        error.message().contains("fail-closed"),
        "{}",
        error.message()
    );

    let status = cmd_cache_status(root).expect("status");
    assert!(status.contains("FAIL-CLOSED"), "{status}");
    // Zero collection, so the unprotected blob is still there.
    let store = domowik_assets::CacheStore::open(root, domowik_assets::AdmissionLimits::default())
        .expect("cache");
    assert!(store.contains(&loose));

    let rebuilt = cmd_cache_rebuild(root).expect("rebuild");
    assert!(rebuilt.contains("Collection is enabled again"), "{rebuilt}");
    assert!(cmd_cache_status(root)
        .expect("status")
        .contains("collector: ready"));
    // And now it collects.
    assert!(cmd_cache_gc(root, false)
        .expect("gc")
        .contains("Collected 1 blob(s)"));
}

#[test]
fn status_reports_abandoned_staging_from_an_interrupted_import() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    admit(root, b"fine");
    std::fs::write(root.join("incoming").join("leftover.partial"), b"half").expect("staging");

    let status = cmd_cache_status(root).expect("status");
    assert!(status.contains("abandoned staging"), "{status}");
    assert!(status.contains("not reachable as blobs"), "{status}");
}

#[test]
fn collection_refuses_a_cache_no_document_has_registered_against() {
    // §10.7 G4's safety rests on "pins cover every root". Before any document
    // registers, the root set is empty — which is indistinguishable from
    // nothing having told the collector about the documents yet. Collecting
    // under that reading empties the cache of blobs packages still reference.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    let digest = admit(root, b"referenced by a package the collector cannot see");

    let error = cmd_cache_gc(root, false).expect_err("an unregistered cache must not be collected");
    assert_eq!(error.code(), 1);
    assert!(
        error.message().contains("registered"),
        "{}",
        error.message()
    );

    let store = domowik_assets::CacheStore::open(root, domowik_assets::AdmissionLimits::default())
        .expect("cache");
    assert!(store.contains(&digest), "the blob was taken anyway");

    // The caller can say explicitly that the cache holds only garbage.
    let report = cmd_cache_gc(root, true).expect("explicit override");
    assert!(report.contains("Collected 1 blob(s)"), "{report}");
    assert!(!store.contains(&digest));
}

#[test]
fn an_empty_cache_collects_without_the_override() {
    // The refusal is about blobs at risk, not about ceremony.
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(cmd_cache_gc(dir.path(), false)
        .expect("nothing to lose")
        .contains("Collected 0 blob(s)"));
}
