//! Every container fixture, against the §9.4 class EXPECTATIONS.md records.
//!
//! # Why this replaces "everything was refused"
//!
//! `hostile_corpus.rs` asserts that no hostile fixture is accepted. That is
//! true, and it stays true with security guards deleted: this issue proved it
//! by removing the row-L6 reader guard and blob digest verification together
//! and watching the suite stay green, because the fixtures were rejected
//! earlier for an unrelated reason and never reached either check.
//!
//! Asserting the CLASS is what makes a corpus test discriminate. A guard that
//! stops firing changes which check refuses the input, and that changes the
//! token — so the test goes red for the fixture whose guard was removed rather
//! than staying green on a coincidence.
//!
//! # The divergence table is the honest part
//!
//! This reader does not yet produce the normative token for every fixture.
//! Each case is PINNED below with the token it does produce and why, and the
//! pin fails in BOTH directions: if a divergence disappears the pin is stale
//! and the test says so, and if a fixture starts producing something neither
//! expected nor pinned it fails as an unexpected change. Nothing is skipped
//! and no expectation is weakened to match the implementation.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/corpus")
        .canonicalize()
        .expect("the corpus is tracked in this repository")
}

/// What this reader produces today for a fixture whose normative class it does
/// not yet reach, and the reason. One row per fixture, never a wildcard.
///
/// The reasons cluster into three causes, and keeping them distinct matters
/// because they have different owners and different fixes:
///
/// - **`snapshot-body`** — the fixture reaches the snapshot parse and dies on
///   the §5.1 body deviation recorded on `SnapshotSection`. These are not
///   twelve problems; they are one, and they resolve together when the body
///   lands. Until then the fixture's real defect is never reached.
/// - **`profile-check-absent`** — a §7.2 container-profile rule this reader
///   does not implement. T1, the encryption flag, the compression allowlist
///   and the archive comment are now implemented; what remains absent is the
///   local-header/directory structural scan (overlapping regions), the
///   data-descriptor and language-encoding flag checks, `versionNeeded`,
///   foreign extra fields, and symlink/special-file designations, which live
///   in the external-attributes field where a name check cannot see them.
/// - **`corpus-defect`** — the fixture does not contain the condition it is
///   named for, verified by byte inspection. The rule it targets therefore has
///   NO coverage, and saying so is the point: a pin here is a gap in the
///   corpus, not in the reader.
/// - **`earlier-guard`** — a DIFFERENT guard of ours fires first. The input is
///   refused, and refused for a real reason, but not the one the corpus names.
///   These are ordering questions rather than missing checks.
fn pinned() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    use std::iter::FromIterator;
    BTreeMap::from_iter([
        // ---- snapshot-body: one cause, ten fixtures ----
        (
            "golden/packages/thin-valid.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "golden/packages/portable-valid.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/ratio-under-L15-success-twin.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/directory-entry-attr.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/foreign-extra-field.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/limit-expansion-ratio-over-L15.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/malformed-length.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/symlink-designation.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/utf8-flag-clear.domo",
            ("malformed-json", "snapshot-body"),
        ),
        (
            "hostile/container/version-needed-over.domo",
            ("malformed-json", "snapshot-body"),
        ),
        // ---- profile-check-absent ----
        // Overlapping local-header regions need a structural scan of the
        // local headers against the directory, which this reader does not do.
        (
            "hostile/container/overlapping-regions.domo",
            ("malformed-container", "profile-check-absent"),
        ),
        // ---- corpus-defect: the fixture does not contain what it is named for ----
        // Verified by byte inspection: this archive's single EOCD declares
        // `commentLength = 0`, its record ends exactly at EOF, and no comment
        // bytes exist. The reader's §7.2 comment check is implemented and
        // correct; there is nothing here for it to catch. Routed to the corpus
        // owner (issues 0028/0062) rather than worked around, and NOT counted
        // as coverage of the comment rule, which therefore has no fixture.
        (
            "hostile/container/nonzero-comment.domo",
            ("malformed-json", "corpus-defect"),
        ),
        // ---- earlier-guard ----
        // §7.2 item 4 routes malformed or truncated Zip64 records to §9.4's
        // rule T1, and T1 is what fires. The corpus names the broader
        // `malformed-container`; both refuse, and the more specific token is
        // arguably the better diagnostic, so this is a classification
        // disagreement to settle with §7's owner rather than a missing check.
        (
            "hostile/container/zip64-malformed-locator.domo",
            ("trailing-data", "earlier-guard"),
        ),
        (
            "hostile/container/zip64-truncated-eocd64.domo",
            ("trailing-data", "earlier-guard"),
        ),
        // The §7.4 inventory's byte-length check (C3) precedes blob digest
        // verification, and the twin entries differ in length.
        (
            "hostile/container/duplicate-logical-name.domo",
            ("malformed-length", "earlier-guard"),
        ),
        // The L-guard on declared entry size fires before the inventory's
        // length comparison. Both are correct refusals of a declared-size lie.
        (
            "hostile/container/declared-size-lie-limit-L3.domo",
            ("limit-entry-size", "earlier-guard"),
        ),
        // The inventory covers blob entries too, so a blob whose bytes do not
        // match hits `checksum-error` before the §6 digest check reports it as
        // an asset-equivalence failure.
        (
            "hostile/container/digest-mismatch-portable.domo",
            ("checksum-error", "earlier-guard"),
        ),
    ])
}

/// One fixture's normative expectation, parsed from EXPECTATIONS.md.
struct Expectation {
    artifact: String,
    outcome: String,
    category: Option<String>,
}

/// EXPECTATIONS.md is the oracle, read at runtime rather than transcribed.
///
/// Transcribing it would test this file against itself: ED-0089's finding is
/// that a constant restated next to its assertion tests the transcription and
/// nothing else. Parsing the tracked oracle means a fixture added or
/// re-classified upstream reaches this test without anyone remembering to
/// copy it across.
fn expectations() -> Vec<Expectation> {
    let text = std::fs::read_to_string(corpus().join("EXPECTATIONS.md")).expect("the oracle");
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("```json\n") {
        rest = &rest[start + 8..];
        let Some(end) = rest.find("\n```") else { break };
        let (body, after) = rest.split_at(end);
        rest = after;
        let Ok(serde_json::Value::Array(records)) = serde_json::from_str(body) else {
            continue;
        };
        for record in records {
            let Some(artifact) = record.get("artifact").and_then(|v| v.as_str()) else {
                continue;
            };
            if !artifact.ends_with(".domo") {
                continue;
            }
            out.push(Expectation {
                artifact: artifact.to_owned(),
                outcome: record
                    .get("outcome")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_owned(),
                category: record
                    .get("category")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
            });
        }
    }
    out
}

/// The §9.4 class this reader assigns, or `ACCEPTED`.
fn observed(path: &Path) -> String {
    match domowik_container::read_package(path) {
        Ok(_) => "ACCEPTED".to_owned(),
        Err(error) => error.category().unwrap_or("<none>").to_owned(),
    }
}

#[test]
fn the_oracle_is_present_and_covers_the_container_fixtures() {
    // A test that parsed nothing would report green over an empty loop, which
    // is the failure mode this whole file exists to remove.
    let found = expectations();
    assert!(
        found.len() >= 30,
        "expected the tracked container expectations, parsed {}",
        found.len()
    );
}

#[test]
fn every_fixture_produces_its_normative_class_or_a_pinned_divergence() {
    let pins = pinned();
    let mut wrong = Vec::new();
    let mut stale = Vec::new();

    for expectation in expectations() {
        let path = corpus().join(&expectation.artifact);
        if !path.exists() {
            continue;
        }
        let want = match expectation.outcome.as_str() {
            "success" => "ACCEPTED".to_owned(),
            _ => match &expectation.category {
                Some(category) => category.clone(),
                None => expectation.outcome.clone(),
            },
        };
        let got = observed(&path);

        match pins.get(expectation.artifact.as_str()) {
            None => {
                if got != want {
                    wrong.push(format!(
                        "{}: FORMAT-SPEC §9.4 class is `{want}`, this reader reports `{got}`",
                        expectation.artifact
                    ));
                }
            }
            Some((pinned_class, reason)) => {
                if got == want {
                    stale.push(format!(
                        "{}: now produces the normative `{want}`, so the `{reason}` pin is \
                         stale and must be REMOVED rather than left to rot",
                        expectation.artifact
                    ));
                } else if &got != pinned_class {
                    wrong.push(format!(
                        "{}: pinned as `{pinned_class}` ({reason}), but now reports `{got}`; \
                         the normative class is `{want}`",
                        expectation.artifact
                    ));
                }
            }
        }
    }

    assert!(
        stale.is_empty(),
        "{} pin(s) are stale — a divergence closed and the table did not \
         follow:\n  {}",
        stale.len(),
        stale.join("\n  ")
    );
    assert!(
        wrong.is_empty(),
        "{} fixture(s) report an unexpected §9.4 class:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}

#[test]
fn the_pin_table_names_only_fixtures_that_exist() {
    // A pin for a deleted or renamed fixture silently protects nothing, and
    // would make the table look like it covers more than it does.
    let mut missing = Vec::new();
    for artifact in pinned().keys() {
        if !corpus().join(artifact).exists() {
            missing.push(*artifact);
        }
    }
    assert!(missing.is_empty(), "pins for absent fixtures: {missing:?}");
}

#[test]
fn the_name_validation_fixtures_reach_their_own_guard() {
    // The eight §9.4 name forms, asserted as a group because they are the
    // check this reader gained most recently and the one an attacker reaches
    // first. If name validation regresses, these stop being name diagnostics
    // and become whatever the next stage happens to say — which is precisely
    // how they behaved before it existed.
    for (fixture, class) in [
        ("absolute-path.domo", "path-traversal"),
        ("backslash-name.domo", "path-traversal"),
        ("dot-segment-name.domo", "path-traversal"),
        ("drive-form.domo", "path-traversal"),
        ("traversal-dotdot.domo", "path-traversal"),
        ("nul-byte-name.domo", "invalid-entry-name"),
        ("overlong-name-L8.domo", "limit-name-length"),
        ("overlong-component-L9.domo", "limit-name-component"),
        ("name-casefold-collision.domo", "duplicate-logical-name"),
        ("ambiguous-manifest.domo", "duplicate-logical-name"),
    ] {
        let path = corpus().join("hostile/container").join(fixture);
        assert_eq!(observed(&path), class, "{fixture}");
    }
}

#[test]
fn a_conforming_name_is_not_refused_by_the_name_check() {
    // The positive control for the test above. Every assertion there is that
    // something was REFUSED, and a name validator that refused everything
    // would satisfy all ten. Only a name that must be ACCEPTED shows the
    // check discriminates.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let mut package =
        domowik_container::Package::new(domowik_format::Document::with_ground_floor());
    let bytes = b"blob bytes".to_vec();
    package
        .assets
        .insert(domowik_container::digest_of(&bytes), bytes);

    domowik_container::write_package(&path, &package).expect("write");
    // `blob/sha256/<2>/<62>` and `ops/<19 digits>-<uuid>.json` are the longest
    // and most punctuated names v0.1 produces; if the validator rejected any
    // conforming form, it would be one of those.
    assert_eq!(observed(&path), "ACCEPTED");
}

#[test]
fn the_container_profile_fixtures_reach_their_own_guard() {
    // The §7.2/§7.7 rules that ARE implemented, asserted as a group like the
    // name forms above. These fixtures previously died at the missing manifest
    // and now reach the check each was built for.
    for (fixture, class) in [
        ("encrypted-entry.domo", "encrypted-entry"),
        ("unsupported-method.domo", "unsupported-method"),
        ("trailing-data.domo", "trailing-data"),
        ("concatenated-packages.domo", "trailing-data"),
        ("checksum-error.domo", "checksum-error"),
    ] {
        let path = corpus().join("hostile/container").join(fixture);
        assert_eq!(observed(&path), class, "{fixture}");
    }
}

#[test]
fn a_package_this_writer_produced_passes_every_profile_check() {
    // The positive control for the profile rules. Each one is a REFUSAL, and a
    // check that refused everything would satisfy all four above. This is also
    // the case that catches the likeliest mistake in the concatenation guard:
    // it compares the directory offset and size against the trailer's own
    // position, and an off-by-one there would reject every valid package.
    //
    // Deflate matters here. The allowlist admits store and deflate, and the
    // writer uses deflate, so a mis-stated allowlist would fail this rather
    // than pass unnoticed.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    let mut package =
        domowik_container::Package::new(domowik_format::Document::with_ground_floor());
    let blob = vec![b'x'; 4096];
    package
        .assets
        .insert(domowik_container::digest_of(&blob), blob);
    domowik_container::write_package(&path, &package).expect("write");

    assert_eq!(observed(&path), "ACCEPTED");
}

#[test]
fn a_byte_appended_to_a_valid_package_is_refused_as_trailing_data() {
    // Rule T1 at its smallest. One byte is enough: the point is ambiguity, not
    // volume, and a guard written with a tolerance would pass the corpus
    // fixture while admitting the attack it exists to stop.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    domowik_container::write_package(
        &path,
        &domowik_container::Package::new(domowik_format::Document::with_ground_floor()),
    )
    .expect("write");

    let mut bytes = std::fs::read(&path).expect("read");
    bytes.push(0x00);
    std::fs::write(&path, &bytes).expect("append");

    assert_eq!(observed(&path), "trailing-data");
}

#[test]
fn a_package_with_bytes_prepended_is_refused_as_trailing_data() {
    // The other direction, which the "is the record at EOF" test cannot see:
    // leading bytes leave the trailer exactly where it was relative to the end
    // of the file. This is the shape a concatenated second package takes, and
    // it is why the guard compares the directory's own offset against the
    // record's absolute position.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    domowik_container::write_package(
        &path,
        &domowik_container::Package::new(domowik_format::Document::with_ground_floor()),
    )
    .expect("write");

    let original = std::fs::read(&path).expect("read");
    let mut bytes = b"a decoy that is not part of this archive".to_vec();
    bytes.extend_from_slice(&original);
    std::fs::write(&path, &bytes).expect("prepend");

    assert_eq!(observed(&path), "trailing-data");
}
