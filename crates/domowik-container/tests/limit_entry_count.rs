//! Row L6, on the READER path, against a declared count rather than a real one.
//!
//! # Why the obvious test does not test this
//!
//! L6 caps entries per package at 65,535 — the largest value the legacy ZIP
//! end-of-central-directory field can hold, so 65,536 is the first count a
//! conforming reader must refuse. A test that BUILDS 65,536 entries proves
//! little and costs the very allocation the guard exists to prevent; the guard
//! is there so a hostile package never gets that far.
//!
//! The input therefore only CLAIMS the count. The archive is a few hundred
//! bytes with a Zip64 trailer declaring 65,536 entries, which is exactly the
//! shape an attacker sends: cheap to produce, expensive to trust.
//!
//! This also pins WHERE the guard sits. It must precede
//! `zip::ZipArchive::new`, which parses every central-directory header into a
//! `Vec` before `len()` can be asked (zip 2.4.2 `src/read.rs`, the
//! `for _ in 0..dir_info.number_of_files` loop in `get_metadata`). A guard
//! after that point is a range check on work already done. Asserting the
//! CATEGORY is what holds that: if the guard moves back after construction,
//! the crate's own parse failure answers first and the class changes.

use std::io::Write;

use domowik_container::{read_package, ContainerError, Limits, Package};
use domowik_format::Document;

/// Appends a Zip64 end-of-central-directory record, its locator, and an EOCD
/// whose 16-bit entry count is saturated, so the declared total comes from the
/// Zip64 record.
///
/// Built by hand because the `zip` crate will not write a trailer that
/// disagrees with what it stored, which is correct of it and is why the
/// fixture cannot come from the writer.
fn archive_declaring(entries: u64) -> Vec<u8> {
    let mut out = Vec::new();

    // One real, empty central directory: the directory starts where the (also
    // empty) local-header region ends, at offset 0.
    let directory_offset: u64 = 0;
    let directory_size: u64 = 0;

    let zip64_eocd_offset = out.len() as u64;
    out.extend_from_slice(&[0x50, 0x4b, 0x06, 0x06]); // Zip64 EOCD signature
    out.extend_from_slice(&44u64.to_le_bytes()); // size of remainder
    out.extend_from_slice(&45u16.to_le_bytes()); // version made by
    out.extend_from_slice(&45u16.to_le_bytes()); // version needed
    out.extend_from_slice(&0u32.to_le_bytes()); // this disk
    out.extend_from_slice(&0u32.to_le_bytes()); // disk with directory
    out.extend_from_slice(&entries.to_le_bytes()); // entries on this disk
    out.extend_from_slice(&entries.to_le_bytes()); // entries total (offset 32)
    out.extend_from_slice(&directory_size.to_le_bytes());
    out.extend_from_slice(&directory_offset.to_le_bytes());

    out.extend_from_slice(&[0x50, 0x4b, 0x06, 0x07]); // Zip64 locator
    out.extend_from_slice(&0u32.to_le_bytes()); // disk with Zip64 EOCD
    out.extend_from_slice(&zip64_eocd_offset.to_le_bytes());
    out.extend_from_slice(&1u32.to_le_bytes()); // total disks

    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // EOCD
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // this disk, saturated
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // disk with directory
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // entries here, saturated
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // entries total, saturated
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // directory size
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // directory offset
    out.extend_from_slice(&0u16.to_le_bytes()); // comment length
    out
}

fn write(bytes: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("hostile.domo");
    std::fs::File::create(&path)
        .expect("create")
        .write_all(bytes)
        .expect("write");
    (dir, path)
}

#[test]
fn a_package_declaring_more_than_l6_entries_is_refused_before_the_directory_is_parsed() {
    let bytes = archive_declaring(u64::from(Limits::L6_ENTRY_COUNT) + 1);
    assert!(
        bytes.len() < 1024,
        "the point is that the input is TINY and the count is a claim; got {} bytes",
        bytes.len()
    );
    let (_dir, path) = write(&bytes);

    let error = read_package(&path).expect_err("65,536 declared entries must be refused");
    assert_eq!(
        error.category(),
        Some("limit-entry-count"),
        "it must be refused BY L6, not incidentally by a later parse: {error}"
    );
    match error {
        ContainerError::EntryCountExceeded { count, limit, .. } => {
            assert_eq!(count, 65_536, "the DECLARED count is what was judged");
            assert_eq!(limit, 65_535);
        }
        other => panic!("expected EntryCountExceeded, got {other}"),
    }
}

#[test]
fn the_l6_ceiling_itself_is_not_refused_by_the_count_guard() {
    // The boundary belongs on the passing side: §9.3 row L6 says "> N
    // rejected (== 65,535 succeeds)". Without this, a guard written with `>=`
    // would pass the test above and quietly reject conforming packages.
    //
    // A declaration of exactly 65,535 gets PAST the count guard and then fails
    // in the `zip` crate, which cannot find the directory the trailer promises.
    // That is the right outcome: the count was admissible and the archive is
    // malformed for a different reason.
    let bytes = archive_declaring(u64::from(Limits::L6_ENTRY_COUNT));
    let (_dir, path) = write(&bytes);

    let error = read_package(&path).expect_err("the archive is still malformed");
    assert_ne!(
        error.category(),
        Some("limit-entry-count"),
        "65,535 is admissible under L6 and must not be refused by the count \
         guard: {error}"
    );
}

#[test]
fn an_ordinary_package_passes_the_count_guard() {
    // The positive control. Every assertion above is that something was
    // REFUSED, and a guard that rejected every archive would satisfy the
    // first and could be written to satisfy the second. This is the only case
    // here that must be ACCEPTED, and it exercises the same trailer-reading
    // path on a real file.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    domowik_container::write_package(&path, &Package::new(Document::with_ground_floor()))
        .expect("write");
    assert!(read_package(&path).is_ok());
}

#[test]
fn a_declared_count_is_judged_against_the_caller_s_limit() {
    // The guard must consult the LIMITS it was given rather than the constant,
    // or `read_package_with_limits` silently ignores its own argument — the
    // shape a caller tightening a limit for untrusted input would never see.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    domowik_container::write_package(&path, &Package::new(Document::with_ground_floor()))
        .expect("write");

    let strict = Limits {
        max_entry_count: 1,
        ..Limits::default()
    };
    let error = domowik_container::read_package_with_limits(&path, &strict)
        .expect_err("two entries exceed a limit of one");
    assert_eq!(error.category(), Some("limit-entry-count"), "{error}");
}

// ---- Row L23: the snapshot-metadata cap, wired at last ----

/// Issue 0004 specified and tested this accounting ahead of the members it
/// counts, and left it DELIBERATELY unwired because `Package.snapshot` was a
/// bare `Document` with no metadata members to score. Issue 0029's §7.5 work
/// landed those members, so these assert the enforcement rather than the
/// arithmetic — which `snapshot_metadata_l23` already covers.
#[test]
fn an_oversized_snapshot_frontier_is_refused_by_row_l23() {
    // The frontier is the member that grows without bound in practice: one row
    // per actor that has ever written, and nothing prunes it. A tight limit
    // stands in for a 16 MiB one, so the test costs a few hundred bytes rather
    // than sixteen megabytes — the guard compares a count either way.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");

    let mut package = Package::new(Document::with_ground_floor());
    for _ in 0..64 {
        package.frontier.insert(uuid::Uuid::now_v7(), 41);
    }
    domowik_container::write_package(&path, &package).expect("write");

    let strict = Limits {
        max_snapshot_metadata_bytes: 128,
        ..Limits::default()
    };
    let error = domowik_container::read_package_with_limits(&path, &strict)
        .expect_err("64 frontier rows exceed a 128-byte metadata cap");
    assert_eq!(
        error.category(),
        Some("limit-snapshot-metadata"),
        "it must be refused BY L23, not incidentally: {error}"
    );
}

#[test]
fn row_l23_does_not_count_the_projected_document() {
    // The distinction the accounting exists for. A large DOCUMENT with a tiny
    // frontier must pass a tight metadata cap, or the cap has silently become
    // a maximum document size — which is what `snapshot_metadata_bytes`
    // excludes S1 to prevent, under either of the two names S1 currently has.
    //
    // Without this, the guard above would pass just as happily with the
    // exclusion deleted, and the first big plan anyone saved would stop
    // opening.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.domo");

    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    for i in 0..400 {
        document.walls.push(domowik_format::Wall::new(
            level,
            domowik_format::Point::new(0.0, i as f64 * 10.0),
            domowik_format::Point::new(4000.0, i as f64 * 10.0),
            120.0,
            2500.0,
        ));
    }
    let package = Package::new(document);
    domowik_container::write_package(&path, &package).expect("write");

    // The document is far larger than this cap; the metadata is empty.
    let strict = Limits {
        max_snapshot_metadata_bytes: 128,
        ..Limits::default()
    };
    let read = domowik_container::read_package_with_limits(&path, &strict)
        .expect("a large document with no metadata must pass a metadata cap");
    assert_eq!(read.snapshot.walls.len(), 400);
}
