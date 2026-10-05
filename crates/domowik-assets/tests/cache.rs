//! ASSET-STRATEGY §10.6 cache admission and §10.3 path mapping.

use std::io::{self, Read};

use domowik_assets::{Admission, AdmissionLimits, CacheError, CacheStore};

fn store(limit: u64) -> (tempfile::TempDir, CacheStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = CacheStore::open(
        dir.path(),
        AdmissionLimits {
            max_blob_bytes: limit,
        },
    )
    .expect("open");
    (dir, store)
}

fn digest(bytes: &[u8]) -> String {
    domowik_container::digest_of(bytes)
}

#[test]
fn admission_streams_and_stores_under_the_10_3_path() {
    let (dir, store) = store(1024);
    let bytes = b"a background scan".to_vec();
    let d = digest(&bytes);

    assert_eq!(
        store.admit_bytes(&d, &bytes).expect("admit"),
        Admission::Stored
    );

    // §10.3: `blob/<algorithm>/<2-hex>/<62-hex>`, and the same mapping the
    // package uses — derived from the container rather than spelled again
    // here, so the two stores cannot drift apart silently.
    let expected = dir
        .path()
        .join(domowik_container::blob_entry_name(&d).expect("well-formed digest"));
    assert!(expected.is_file(), "blob is not at its §10.3 path");
    assert_eq!(std::fs::read(&expected).expect("read"), bytes);
    assert!(store.contains(&d));
    assert_eq!(store.stored_bytes(&d), Some(bytes.len() as u64));
}

#[test]
fn a_second_admission_of_the_same_digest_writes_nothing() {
    // §10.6: "writers deduplicate before write."
    let (_dir, store) = store(1024);
    let bytes = b"dedupe me".to_vec();
    let d = digest(&bytes);
    assert_eq!(
        store.admit_bytes(&d, &bytes).expect("first"),
        Admission::Stored
    );
    assert_eq!(
        store.admit_bytes(&d, &bytes).expect("second"),
        Admission::AlreadyPresent
    );
    assert_eq!(store.digests().expect("digests"), vec![d]);
}

#[test]
fn bytes_that_do_not_hash_to_the_expected_digest_are_refused_and_leave_nothing() {
    let (_dir, store) = store(1024);
    let claimed = digest(b"what the caller says it is");
    let actual = b"what the caller actually supplied";

    let error = store
        .admit_bytes(&claimed, actual)
        .expect_err("a lie about content must not be admitted");
    match error {
        CacheError::DigestMismatch {
            expected,
            actual: a,
        } => {
            assert_eq!(expected, claimed);
            assert_eq!(a, digest(actual));
        }
        other => panic!("expected a digest mismatch, got {other:?}"),
    }

    assert!(!store.contains(&claimed), "the refused digest is readable");
    assert!(
        store.digests().expect("digests").is_empty(),
        "a refused admission left a blob behind"
    );
    assert!(
        store.abandoned_staging().expect("staging").is_empty(),
        "a refused admission left its staging file behind"
    );
}

#[test]
fn a_declared_length_that_disagrees_with_the_bytes_is_refused() {
    // §10.1: "declared length ≠ actual length ⇒ rejection."
    let (_dir, store) = store(1024);
    let bytes = b"eleven byte".to_vec();
    let d = digest(&bytes);
    let wrong = bytes.len() as u64 + 1;

    let error = store
        .admit(&d, Some(wrong), &mut io::Cursor::new(&bytes))
        .expect_err("declared length must be enforced");
    match error {
        CacheError::LengthMismatch {
            declared, actual, ..
        } => {
            assert_eq!(declared, wrong);
            assert_eq!(actual, bytes.len() as u64);
        }
        other => panic!("expected a length mismatch, got {other:?}"),
    }
    assert!(!store.contains(&d));
}

/// A reader that never ends, so the ceiling has to stop the admission rather
/// than the input running out.
struct Endless;

impl Read for Endless {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        buf.fill(b'x');
        Ok(buf.len())
    }
}

#[test]
fn the_admission_ceiling_aborts_during_receipt_not_after() {
    // §9.3 row L5's enforcement point is "during streaming digest
    // verification, before cache admission". An endless reader can only be
    // stopped by a check inside the loop, so this test cannot pass against an
    // implementation that measures the size after reading to the end.
    let limit = 4096;
    let (_dir, store) = store(limit);
    let anything = digest(b"never reached");

    let error = store
        .admit(&anything, None, &mut Endless)
        .expect_err("an unbounded stream must be refused");
    match error {
        CacheError::TooLarge {
            limit: reported, ..
        } => assert_eq!(reported, limit),
        other => panic!("expected a size refusal, got {other:?}"),
    }
    assert!(store.abandoned_staging().expect("staging").is_empty());
}

#[test]
fn the_default_ceiling_is_the_format_spec_row_it_cites() {
    // ED-0089: derive from the specification row, never restate its value as a
    // second literal. The row is parsed out of FORMAT-SPEC §9.3 here, so
    // editing the constant without editing the specification fails this test.
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/FORMAT-SPEC.md"),
    )
    .expect("FORMAT-SPEC.md");
    let row = spec
        .lines()
        .find(|l| l.starts_with("| L5 |"))
        .expect("FORMAT-SPEC §9.3 has an L5 row");
    // The cell reads "536,870,912 B (512 MiB)": the authoritative number is
    // the one before the unit, and the parenthesised form is a restatement.
    let value_cell = row.split('|').nth(3).expect("L5 row has a value column");
    let byte_count = value_cell
        .split_whitespace()
        .next()
        .expect("the L5 value column is not empty");
    let from_spec: u64 = byte_count
        .replace(',', "")
        .parse()
        .expect("the L5 value column starts with a byte count");

    assert_eq!(AdmissionLimits::L5_BLOB_BYTES, from_spec);
    assert_eq!(AdmissionLimits::default().max_blob_bytes, from_spec);
}

#[test]
fn a_malformed_digest_names_no_path_at_all() {
    // §10.3 requires the mapping to be free of traversal surface. Every
    // component of a real path is fixed-width hex, so none of these can name
    // one.
    let (_dir, store) = store(1024);
    for hostile in [
        "../../etc/passwd",
        "sha256:../../etc/passwd",
        "sha256:E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "sha256:short",
        "sha256:",
    ] {
        assert!(
            store.path_for(hostile).is_none(),
            "{hostile:?} was mapped to a path"
        );
        assert!(!store.contains(hostile));
        assert!(matches!(
            store.admit_bytes(hostile, b"x"),
            Err(CacheError::MalformedDigest { .. })
        ));
    }
}

#[test]
fn verify_reports_a_stored_blob_that_no_longer_matches_its_name() {
    let (dir, store) = store(1024);
    let bytes = b"honest bytes".to_vec();
    let d = digest(&bytes);
    store.admit_bytes(&d, &bytes).expect("admit");
    assert_eq!(store.verify(&d).expect("verify"), Some(true));

    // Bit rot, a bad restore, a hand-edited cache: the file is present under a
    // name its content no longer justifies.
    let path = dir
        .path()
        .join(domowik_container::blob_entry_name(&d).expect("mapping"));
    std::fs::write(&path, b"tampered").expect("overwrite");

    assert_eq!(store.verify(&d).expect("verify"), Some(false));
    // Reporting is not deleting. The only copy of something survives a
    // verification failure.
    assert!(path.is_file());
    assert_eq!(store.verify("sha256:not-a-digest").ok(), None);
}

#[test]
fn totals_and_enumeration_cover_only_well_formed_blobs() {
    let (dir, store) = store(1024);
    let a = b"first".to_vec();
    let b = b"second".to_vec();
    store.admit_bytes(&digest(&a), &a).expect("a");
    store.admit_bytes(&digest(&b), &b).expect("b");

    let mut expected = vec![digest(&a), digest(&b)];
    expected.sort();
    assert_eq!(store.digests().expect("digests"), expected);
    assert_eq!(
        store.total_bytes().expect("total"),
        (a.len() + b.len()) as u64
    );

    // A stray file whose name does not round-trip through the §10.3 mapping
    // is not a blob and must not be counted as one.
    let shard = dir.path().join("blob").join("sha256").join("zz");
    std::fs::create_dir_all(&shard).expect("shard");
    std::fs::write(shard.join("notablob"), b"junk").expect("stray");
    assert_eq!(store.digests().expect("digests"), expected);
}

#[test]
fn an_interrupted_admission_leaves_only_unreachable_staging() {
    // Stands in for a crash: the staging file exists, and nothing can read it
    // as a blob because it is not under a digest name.
    let (dir, store) = store(1024);
    let bytes = b"half written".to_vec();
    let d = digest(&bytes);

    let incoming = dir.path().join("incoming");
    std::fs::write(incoming.join("abandoned.partial"), b"half wr").expect("staging");

    assert_eq!(store.abandoned_staging().expect("staging").len(), 1);
    assert!(store.digests().expect("digests").is_empty());
    assert!(!store.contains(&d));

    // And the store still works afterwards.
    assert_eq!(
        store.admit_bytes(&d, &bytes).expect("admit"),
        Admission::Stored
    );
    assert!(store.contains(&d));
}

#[test]
fn import_names_the_digest_the_bytes_turned_out_to_have() {
    // Import is not admission: a file a user is importing has no digest yet,
    // so there is nothing to verify against and the operation must REPORT the
    // digest rather than check one.
    let (dir, store) = store(1024);
    let bytes = b"a scan nobody has hashed yet".to_vec();

    let (digest, admission) = store.import(&mut io::Cursor::new(&bytes)).expect("import");
    assert_eq!(admission, Admission::Stored);
    assert_eq!(digest, self::digest(&bytes));

    let path = dir
        .path()
        .join(domowik_container::blob_entry_name(&digest).expect("mapping"));
    assert_eq!(std::fs::read(&path).expect("read"), bytes);
    assert_eq!(store.verify(&digest).expect("verify"), Some(true));
    assert!(store.abandoned_staging().expect("staging").is_empty());
}

#[test]
fn importing_the_same_content_twice_stores_it_once() {
    let (_dir, store) = store(1024);
    let bytes = b"identical either way".to_vec();

    let (first, a) = store.import(&mut io::Cursor::new(&bytes)).expect("first");
    let (second, b) = store.import(&mut io::Cursor::new(&bytes)).expect("second");

    assert_eq!(first, second);
    assert_eq!(a, Admission::Stored);
    assert_eq!(b, Admission::AlreadyPresent);
    assert_eq!(store.digests().expect("digests"), vec![first]);
    // The redundant staging copy is cleaned up rather than left to accumulate
    // on every re-import of a file the user already has.
    assert!(store.abandoned_staging().expect("staging").is_empty());
}

#[test]
fn an_oversized_import_is_stopped_during_receipt_and_stores_nothing() {
    let limit = 4096;
    let (_dir, store) = store(limit);
    let error = store
        .import(&mut Endless)
        .expect_err("an unbounded import must be refused");
    assert!(matches!(error, CacheError::TooLarge { .. }));
    assert!(store.digests().expect("digests").is_empty());
    assert!(store.abandoned_staging().expect("staging").is_empty());
}

#[test]
fn import_file_reads_from_disk_and_reports_the_stored_length() {
    let (_dir, store) = store(1024);
    let source = tempfile::tempdir().expect("tempdir");
    let path = source.path().join("scan.png");
    let bytes = b"pretend this is a PNG".to_vec();
    std::fs::write(&path, &bytes).expect("write");

    let (digest, len, admission) = store.import_file(&path).expect("import");
    assert_eq!(digest, self::digest(&bytes));
    assert_eq!(len, bytes.len() as u64);
    assert_eq!(admission, Admission::Stored);

    assert!(store
        .import_file(&source.path().join("absent.png"))
        .is_err());
}
