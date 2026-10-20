//! Rows L1 and L15 on the reader path.
//!
//! Per-entry limits alone let many acceptable entries demand far more memory
//! than one; L1 bounds the total of one open, and a host admitting packages
//! from strangers sets it lower than the format's ceiling. Each refusal has
//! a twin just inside the limit, so the test fails if the guard is removed
//! AND if it is set too tight.

use domowik_container::{encode_package, read_package_bytes, Limits, Package};
use domowik_format::{Anchor, BackgroundImage, Document, Level};

/// A plan whose level shows `bytes` as an embedded background image.
fn package_embedding(bytes: Vec<u8>) -> Package {
    let digest = domowik_container::digest_of(&bytes);
    let mut level = Level::new("Ground");
    level.background_images.push(BackgroundImage {
        digest: digest.clone(),
        anchor: Anchor {
            translation_mm: [0.0, 0.0],
            scale_uniform: 1.0,
            rotation_deg: 0.0,
            unknown: Default::default(),
        },
        stack_ordinal: 0,
        unknown: Default::default(),
    });
    let mut package = Package::new(Document {
        levels: vec![level],
        ..Document::default()
    });
    package.descriptors.insert(
        digest.clone(),
        serde_json::json!({"digest": digest, "requirement": "optional"}),
    );
    package.assets.insert(digest, bytes);
    package
}

/// Bytes DEFLATE cannot shrink much: a xorshift stream.
fn incompressible(len: usize) -> Vec<u8> {
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

#[test]
fn the_total_expanded_size_of_one_open_is_bounded() {
    let bytes = encode_package(&package_embedding(incompressible(300_000))).expect("encode");
    let total = |limit| Limits {
        max_total_expanded_bytes: limit,
        ..Limits::default()
    };

    let error = read_package_bytes("big.domo", &bytes, &total(250_000))
        .expect_err("300 kB of entries exceed a 250 kB total");
    assert_eq!(error.category(), Some("limit-total-expanded"), "{error}");

    // Every entry is individually far below the per-entry ceiling, so only
    // the total can have refused it; and the same bytes open under a total
    // that covers them.
    read_package_bytes("big.domo", &bytes, &total(400_000)).expect("within the total");
}

/// Rewrites every entry of `archive` with DEFLATE, as a writer that never
/// stores would have.
fn all_deflated(archive: &[u8]) -> Vec<u8> {
    use std::io::{Read, Write};
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
    let mut output = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..input.len() {
        let mut entry = input.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        output.start_file(entry.name().to_owned(), opts).unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

fn method_of(archive: &[u8], prefix: &str) -> zip::CompressionMethod {
    let mut input = zip::ZipArchive::new(std::io::Cursor::new(archive)).unwrap();
    let name = input
        .file_names()
        .find(|n| n.starts_with(prefix))
        .unwrap()
        .to_owned();
    let method = input.by_name(&name).unwrap().compression();
    method
}

#[test]
fn an_entry_expanding_more_than_a_thousandfold_is_refused_and_the_writer_stores_it() {
    // A megabyte of zeros deflates to about a kilobyte: over 1,000 : 1. The
    // writer stores that entry instead (FORMAT-SPEC §7.2 permits it), so the
    // package it writes reads back...
    let bytes = encode_package(&package_embedding(vec![0u8; 1 << 20])).expect("stored");
    assert_eq!(method_of(&bytes, "blob/"), zip::CompressionMethod::Stored);
    assert_eq!(
        method_of(&bytes, "snapshot/"),
        zip::CompressionMethod::Deflated
    );
    read_package_bytes("zeros.domo", &bytes, &Limits::default()).expect("reads");

    // ...while the same entry deflated is refused on L15.
    let error = read_package_bytes("deflated.domo", &all_deflated(&bytes), &Limits::default())
        .expect_err("zeros expand beyond L15");
    assert_eq!(error.category(), Some("limit-expansion-ratio"), "{error}");

    // A repeating pattern with a long period compresses, but nowhere near
    // 1,000 : 1, and stays deflated.
    let twin: Vec<u8> = incompressible(4096)
        .into_iter()
        .cycle()
        .take(1 << 20)
        .collect();
    let bytes = encode_package(&package_embedding(twin)).expect("within L15");
    assert_eq!(method_of(&bytes, "blob/"), zip::CompressionMethod::Deflated);
    read_package_bytes("twin.domo", &bytes, &Limits::default()).expect("reads");
}

/// Review round 2: a valid package whose manifest carries very repetitive
/// unknown data must stay writable after an edit, not become unexportable.
#[test]
fn a_repetitive_manifest_extension_is_written_stored() {
    let mut package = package_embedding(incompressible(100));
    package.manifest.unknown.insert(
        "x-padding".to_owned(),
        serde_json::json!("x".repeat(12 << 20)),
    );
    // Deflated, this manifest sits at the edge of L15 (about 1,000 : 1); the
    // writer stores it when it crosses, deflates it when it does not, and
    // either way the package reads back.
    let bytes = encode_package(&package).expect("written");
    let reread = read_package_bytes("m.domo", &bytes, &Limits::default()).expect("reads");
    assert_eq!(
        reread.manifest.unknown["x-padding"].as_str().unwrap().len(),
        12 << 20
    );
}

/// §8.4 U2 for inventory rows: a row's unknown members survive a write of
/// unchanged bytes, and a write of changed bytes is refused, not stripped.
#[test]
fn inventory_row_extensions_are_preserved_or_the_write_is_refused() {
    let original = encode_package(&package_embedding(incompressible(1000))).expect("encode");
    let mut package = read_package_bytes("p.domo", &original, &Limits::default()).expect("read");
    let snapshot_entry = "snapshot/state.json";
    package
        .manifest
        .entries
        .get_mut(snapshot_entry)
        .expect("snapshot row")
        .unknown
        .insert("x-retention".to_owned(), serde_json::json!("keep"));

    let unchanged = encode_package(&package).expect("unchanged bytes keep their row");
    let reread = read_package_bytes("p.domo", &unchanged, &Limits::default()).expect("read");
    assert_eq!(
        reread.manifest.entries[snapshot_entry]
            .unknown
            .get("x-retention"),
        Some(&serde_json::json!("keep"))
    );

    package.snapshot.levels[0].name = "Renamed".to_owned();
    let error = encode_package(&package).expect_err("changed bytes cannot keep the row");
    assert!(
        matches!(
            error,
            domowik_container::ContainerError::UnpreservableEntryFields { ref entry, .. }
                if entry == snapshot_entry
        ),
        "{error}"
    );
}

/// The manifest is decompressed and parsed before the inventory pass and is
/// not in its own inventory; rows L1 and L15 hold for it all the same.
#[test]
fn the_manifest_counts_toward_the_expansion_limits() {
    let with_manifest_field = |value: String| {
        let mut package = package_embedding(incompressible(100));
        package
            .manifest
            .unknown
            .insert("x-extension".to_owned(), serde_json::json!(value));
        package
    };

    // Hex of random bytes compresses about 2 : 1, well inside L15.
    let hex: String = incompressible(300_000)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let bytes = encode_package(&with_manifest_field(hex)).expect("encode");
    let total = |limit| Limits {
        max_total_expanded_bytes: limit,
        ..Limits::default()
    };
    let error = read_package_bytes("m.domo", &bytes, &total(500_000)).expect_err("over the total");
    assert_eq!(error.category(), Some("limit-total-expanded"), "{error}");
    read_package_bytes("m.domo", &bytes, &total(1_000_000)).expect("within the total");
    // L15 applies to the manifest through the same bound as every entry
    // (`l15_bound`); a JSON manifest cannot be made to cross 1,000 : 1 with
    // this writer (a megabyte of one character deflates about 758 : 1), so
    // the entry test above is that rule's twin.
}
