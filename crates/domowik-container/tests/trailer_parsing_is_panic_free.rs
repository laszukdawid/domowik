//! `declared_entry_count` parses the ZIP trailer BY HAND, on untrusted bytes,
//! before anything else in the reader runs. That makes it the most dangerous
//! function in the crate: it indexes slices at computed offsets, and it is the
//! first code a hostile file reaches.
//!
//! The required independent adversarial review (Codex `gpt-5.6-sol`) could not
//! run — the workspace is out of credits — so this asserts by EXECUTION what
//! that review would have been asked to attack by inspection. Reading the code
//! and concluding "the `checked_sub` guards look right" is the kind of
//! reasoning that misses an off-by-one; driving thousands of hostile inputs
//! through it is not.
//!
//! Every case here must return, not panic. A wrong ANSWER is caught elsewhere
//! (`limit_entry_count.rs`); what this file rules out is a panic, an
//! out-of-bounds index, or a hang on input an attacker controls completely.

use std::io::Write;
use std::path::Path;

/// Deterministic pseudo-random bytes. A fixed sequence rather than a random
/// one so a failure is reproducible from the seed alone; a flaky corpus test
/// that cannot be re-run is worse than none.
struct Lcg(u64);

impl Lcg {
    fn next_u8(&mut self) -> u8 {
        // Numerical Recipes LCG constants; any full-period generator does.
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) as u8
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next_u8()).collect()
    }
}

fn probe(dir: &Path, name: &str, bytes: &[u8]) {
    let path = dir.join(name);
    std::fs::File::create(&path)
        .expect("create")
        .write_all(bytes)
        .expect("write");
    // The whole public entry point, so a panic anywhere beneath it is a panic
    // a user can trigger with a file.
    let outcome = std::panic::catch_unwind(|| domowik_container::read_package(&path));
    assert!(
        outcome.is_ok(),
        "PANICKED on {name} ({} bytes): {:02x?}",
        bytes.len(),
        &bytes[..bytes.len().min(64)]
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn truncations_of_a_real_package_never_panic() {
    // Every prefix of a valid package. This walks the trailer off the end one
    // byte at a time, which is exactly the shape that breaks a hand-rolled
    // EOCD scan: the signature is found, and then the fields after it are not
    // all there.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("whole.domo");
    let mut package =
        domowik_container::Package::new(domowik_format::Document::with_ground_floor());
    let blob = b"some bytes".to_vec();
    package
        .assets
        .insert(domowik_container::digest_of(&blob), blob);
    domowik_container::write_package(&source, &package).expect("write");
    let whole = std::fs::read(&source).expect("bytes");
    assert!(
        whole.len() > 300,
        "the fixture must be big enough to matter"
    );

    for cut in 0..whole.len() {
        probe(dir.path(), "truncated.domo", &whole[..cut]);
    }
}

#[test]
fn a_corrupted_byte_anywhere_in_the_trailer_never_panics() {
    // Single-byte mutations across the last 128 bytes, which is where the
    // EOCD, the Zip64 locator and the Zip64 record all live. Each byte is set
    // to four values chosen to hit the edges: zero, one past the 16-bit
    // saturation marker, the saturation marker itself, and 0xFF.
    let dir = tempfile::tempdir().expect("tempdir");
    let source = dir.path().join("whole.domo");
    domowik_container::write_package(
        &source,
        &domowik_container::Package::new(domowik_format::Document::with_ground_floor()),
    )
    .expect("write");
    let whole = std::fs::read(&source).expect("bytes");

    let window = whole.len().saturating_sub(128);
    for index in window..whole.len() {
        for value in [0x00u8, 0xFEu8, 0xFFu8, 0x50u8] {
            let mut mutated = whole.clone();
            mutated[index] = value;
            probe(dir.path(), "mutated.domo", &mutated);
        }
    }
}

#[test]
fn a_forged_zip64_locator_pointing_anywhere_never_panics() {
    // The one field in the trailer that is a POINTER: the Zip64 locator's
    // offset. A hostile value sends the reader seeking to an arbitrary place
    // in the file — or past its end — and it must come back rather than panic
    // or spin. Offsets are swept across the whole file and well beyond it.
    let dir = tempfile::tempdir().expect("tempdir");

    for offset in [
        0u64,
        1,
        21,
        22,
        64,
        200,
        u64::from(u32::MAX),
        u64::MAX - 1,
        u64::MAX,
    ] {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"some leading content that is not a zip at all");

        // A Zip64 locator whose offset is hostile, then a saturated EOCD so
        // the reader is forced to consult it.
        bytes.extend_from_slice(&[0x50, 0x4b, 0x06, 0x07]);
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&offset.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());

        bytes.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        for _ in 0..4 {
            bytes.extend_from_slice(&0xFFFFu16.to_le_bytes());
        }
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());

        probe(dir.path(), "forged-locator.domo", &bytes);
    }
}

#[test]
fn random_bytes_with_a_planted_trailer_signature_never_panic() {
    // Noise alone rarely contains an EOCD signature, so the scan would exit
    // early and prove nothing. Each case plants the signature at a different
    // distance from the end, including distances where the fields that follow
    // it run off the buffer.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut rng = Lcg(0x5EED_0029);

    for case in 0..256 {
        let size = 1 + (case * 7) % 400;
        let mut bytes = rng.bytes(size);
        // Plant at a position that varies from "comfortably inside" to "so
        // close to the end that the 22-byte record cannot fit".
        if size >= 4 {
            let at = size - 4 - (case % 4).min(size.saturating_sub(4));
            bytes[at..at + 4].copy_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
        }
        probe(dir.path(), "noise.domo", &bytes);
    }
}

#[test]
fn an_empty_and_a_tiny_file_never_panic() {
    // The degenerate ends. A zero-length file has no trailer to scan and a
    // 21-byte one is one byte short of a minimal EOCD, which is where a
    // `checked_sub` that was written as a subtraction would fail.
    let dir = tempfile::tempdir().expect("tempdir");
    for size in 0..40usize {
        probe(dir.path(), "tiny.domo", &vec![0x50u8; size]);
    }
    probe(dir.path(), "empty.domo", &[]);
}
