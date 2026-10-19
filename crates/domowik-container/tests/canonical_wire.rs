//! The §7.3 entry names, the §7.4 manifest, and what the developer layout gets.
//!
//! These run against the TRACKED corpus rather than against packages this
//! writer produced, which is the point: a reader checked only against its own
//! output agrees with itself and with nothing else. The corpus was built for
//! the frozen Go reader from the normative spec, so it is the only independent
//! evidence in the repository that these shapes are right.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use domowik_container::{read_package, write_package, ContainerError, Package};
use domowik_format::Document;

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/corpus")
        .canonicalize()
        .expect("the corpus is tracked in this repository")
}

fn golden() -> Vec<PathBuf> {
    let dir = corpus().join("golden/packages");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("golden packages")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "domo"))
        .collect();
    out.sort();
    assert!(!out.is_empty(), "the golden corpus must not be empty");
    out
}

/// A package in the layout builds before ED-0108 wrote: `manifest.json` and
/// `document.json`, with the `ops/` and `blob/` names already correct.
fn developer_layout(path: &Path) {
    let file = std::fs::File::create(path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    for (name, bytes) in [
        (
            "manifest.json",
            serde_json::to_vec(&serde_json::json!({
                "formatVersion": 1,
                "generator": "domowik 0.1.0"
            }))
            .unwrap(),
        ),
        (
            "document.json",
            serde_json::to_vec(&Document::with_ground_floor()).unwrap(),
        ),
    ] {
        zip.start_file(name, options).expect("start");
        zip.write_all(&bytes).expect("write");
    }
    zip.finish().expect("finish");
}

#[test]
fn a_written_package_uses_the_entry_names_the_spec_names() {
    // FORMAT-SPEC §7.3's namespace is CLOSED, so these are not preferences.
    // An entry outside it rejects as `invalid-entry-name`, which means a
    // writer emitting `manifest.json` produces a package no conforming reader
    // may open — the defect ED-0108 exists to close.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

    let file = std::fs::File::open(&path).expect("open");
    let archive = zip::ZipArchive::new(file).expect("archive");
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.name_for_index(i).expect("name").to_owned())
        .collect();

    assert!(
        names.iter().any(|n| n == "domowik.json"),
        "§7.3 names the manifest `domowik.json`; got {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "snapshot/state.json"),
        "§7.5 names the snapshot `snapshot/state.json`; got {names:?}"
    );
    assert!(
        !names
            .iter()
            .any(|n| n == "manifest.json" || n == "document.json"),
        "the developer names must not be written at all; got {names:?}"
    );
}

#[test]
fn the_golden_manifests_parse_and_their_inventories_verify() {
    // The real interop check. These fixtures were generated from the normative
    // spec for a different implementation, so a manifest of ours that parses
    // them is evidence about the FORMAT rather than about our own writer.
    //
    // It proves four §7.4 fields at once: `formatVersion` as the §8.1 string,
    // the required `profile`, `assetRoot` as digest OBJECTS rather than bare
    // strings, and the `entries` inventory — whose digests and byte lengths
    // are checked against the fixtures' actual entries before this returns.
    for path in golden() {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        match read_package(&path) {
            // The snapshot BODY is a known deviation, recorded in
            // `SnapshotSection`: §5.1 serialises S1 as an `entities` map and
            // this version serialises the projected document. Reaching this
            // error means everything before it — names, manifest, inventory —
            // agreed with the fixture.
            Err(ContainerError::Json { entry, .. }) => {
                assert_eq!(
                    entry, "snapshot/state.json",
                    "{name}: the only expected parse failure is the §5.1 \
                     snapshot body; a failure anywhere else is a real defect"
                );
            }
            Err(other) => panic!(
                "{name}: the manifest and inventory must agree with the \
                 tracked corpus, got {other}"
            ),
            Ok(_) => panic!(
                "{name}: read fully, so the §5.1 snapshot-body deviation \
                 recorded in `SnapshotSection` is closed and this test's \
                 expectation needs updating rather than deleting"
            ),
        }
    }
}

#[test]
fn a_developer_layout_package_is_named_as_such_not_reported_as_broken() {
    // ED-0108: the diagnostic is the deliverable here. A developer's file is
    // INTACT, so `missing-manifest` sends them looking for corruption that
    // does not exist. The error must say the layout is old and say what to do.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old.domo");
    developer_layout(&path);

    match read_package(&path) {
        Err(ContainerError::UnsupportedDevelopmentLayout { path: reported }) => {
            let message =
                ContainerError::UnsupportedDevelopmentLayout { path: reported }.to_string();
            // Each of these is a separate obligation, so each is asserted.
            assert!(
                message.contains("not damaged") || message.contains("is not \ndamaged"),
                "must say the file is intact: {message}"
            );
            assert!(
                message.contains("convert-developer-package"),
                "must name the repair: {message}"
            );
            assert!(
                message.contains("domowik.json"),
                "must name the expected entry: {message}"
            );
            assert!(
                message.contains("left untouched"),
                "must say the original survives: {message}"
            );
        }
        other => panic!("expected UnsupportedDevelopmentLayout, got {other:?}"),
    }
}

#[test]
fn a_package_with_no_manifest_at_all_still_says_the_manifest_is_missing() {
    // The control for the test above. Without it, the developer-layout arm
    // could be reached by ANY package lacking `domowik.json`, and the two
    // diagnostics would be one diagnostic with two spellings.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("headless.domo");
    {
        let file = std::fs::File::create(&path).expect("create");
        let mut zip = zip::ZipWriter::new(file);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        zip.start_file("snapshot/state.json", options)
            .expect("start");
        zip.write_all(b"{}").expect("write");
        zip.finish().expect("finish");
    }

    match read_package(&path) {
        Err(ContainerError::MissingEntry { entry, .. }) => assert_eq!(entry, "domowik.json"),
        other => panic!("expected MissingEntry, got {other:?}"),
    }
}

#[test]
fn an_entry_the_inventory_does_not_list_is_refused() {
    // §7.4 makes the inventory cover EVERY other entry, so an unlisted one is
    // how something gets smuggled past a manifest that looks complete.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

    // Rebuild the archive with one extra entry and the manifest untouched.
    let smuggled = dir.path().join("smuggled.domo");
    {
        let file = std::fs::File::open(&path).expect("open");
        let mut archive = zip::ZipArchive::new(file).expect("archive");
        let out = std::fs::File::create(&smuggled).expect("create");
        let mut zip = zip::ZipWriter::new(out);
        let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.name_for_index(i).expect("name").to_owned())
            .collect();
        for name in names {
            let mut bytes = Vec::new();
            archive
                .by_name(&name)
                .expect("entry")
                .read_to_end(&mut bytes)
                .expect("read");
            zip.start_file(name, options).expect("start");
            zip.write_all(&bytes).expect("write");
        }
        zip.start_file("preview/index.json", options)
            .expect("start");
        zip.write_all(b"{\"smuggled\":true}").expect("write");
        zip.finish().expect("finish");
    }

    match read_package(&smuggled) {
        Err(ContainerError::EntryNotInInventory { entry, .. }) => {
            assert_eq!(entry, "preview/index.json")
        }
        other => panic!("expected EntryNotInInventory, got {other:?}"),
    }
}

#[test]
fn descriptors_survive_a_read_and_write_verbatim() {
    // `assets/descriptors.json` is a §7.3 entry of its own, holding the
    // ASSET-STRATEGY §10.2 documents as ONE object keyed by digest (§7.4).
    //
    // This crate does NOT interpret them. A descriptor's meaning — extent,
    // provenance, licence, and the `requirement` BEHAVIOR-SPEC §9.6 keys on —
    // belongs to `domowik-assets`. What the container owes is that the bytes
    // come back unchanged, INCLUDING members neither crate understands, which
    // is the row-L32 obligation and §8.4 U2 applied to an opaque entry.
    //
    // Field spellings follow FORMAT-SPEC §7.4's example: `displayName`,
    // `sourceDescription`, `author`, `acquiredAt`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let rewritten = dir.path().join("again.domo");

    let digest = domowik_container::digest_of(b"a background image, notionally");
    let descriptor = serde_json::json!({
        "digest": digest,
        "byteLength": 30,
        "mediaType": "image/png",
        "purpose": "background-image",
        "encoding": "identity",
        "requirement": "required",
        "provenance": {
            "displayName": "Wood floor tile",
            "sourceDescription": "user import",
            "author": "someone",
            "acquiredAt": "2026-09-12T00:00:00Z",
            "aMemberNeitherCrateUnderstands": {"deeply": ["nested", 7]}
        },
        "aTopLevelMemberNeitherCrateUnderstands": true
    });

    let mut package = Package::new(Document::with_ground_floor());
    package
        .descriptors
        .insert(digest.clone(), descriptor.clone());
    write_package(&path, &package).expect("write");

    let read = read_package(&path).expect("read");
    assert_eq!(
        read.descriptors.get(&digest),
        Some(&descriptor),
        "the descriptor must come back byte-for-byte in meaning"
    );

    // And again, because a single read proves parsing and this proves the
    // write path too: a reader that kept them and a writer that dropped them
    // would pass the assertion above.
    write_package(&rewritten, &read).expect("rewrite");
    let twice = read_package(&rewritten).expect("read again");
    assert_eq!(twice.descriptors.get(&digest), Some(&descriptor));
}

#[test]
fn a_package_with_no_descriptors_does_not_write_the_entry() {
    // §7.3 makes the descriptor entry optional. Emitting an empty object would
    // put a row in the §7.4 inventory of every package that has no assets, and
    // an absent optional entry never fails anything.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain.domo");
    write_package(&path, &Package::new(Document::with_ground_floor())).expect("write");

    let file = std::fs::File::open(&path).expect("open");
    let archive = zip::ZipArchive::new(file).expect("archive");
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.name_for_index(i).expect("name").to_owned())
        .collect();
    assert!(
        !names.iter().any(|n| n == "assets/descriptors.json"),
        "got {names:?}"
    );
}
