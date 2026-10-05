//! `asset add` writes the ASSET-STRATEGY §10.2 descriptor, and everything
//! downstream of it — FORMAT-SPEC §7.4's entry, BEHAVIOR-SPEC §9.6's matrix.

use std::path::{Path, PathBuf};

use domowik_assets::{DescriptorTable, Requirement};

/// A minimal real PNG, so the media type is sniffed rather than assumed.
fn png() -> Vec<u8> {
    let mut p = tiny_skia::Pixmap::new(2, 1).expect("2x1");
    p.pixels_mut()[0] = tiny_skia::PremultipliedColorU8::from_rgba(255, 0, 0, 255).expect("red");
    p.encode_png().expect("encodes")
}

struct Fixture {
    _dir: tempfile::TempDir,
    plan: PathBuf,
    image: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let plan = dir.path().join("plan.domo");
    let image = dir.path().join("survey.png");
    domowik_cli::cmd_new(&plan, false).expect("new");
    std::fs::write(&image, png()).expect("write png");
    Fixture {
        _dir: dir,
        plan,
        image,
    }
}

fn descriptors(plan: &Path) -> DescriptorTable {
    let pkg = domowik_container::read_package(plan).expect("read");
    let (table, faults) = DescriptorTable::from_object(&pkg.descriptors);
    assert!(
        faults.is_empty(),
        "the package carries faulty descriptors: {faults:?}"
    );
    table
}

fn only_digest(plan: &Path) -> String {
    domowik_container::read_package(plan)
        .expect("read")
        .manifest
        .asset_digests()
        .first()
        .expect("one asset")
        .clone()
}

#[test]
fn asset_add_writes_a_descriptor_the_format_spec_shape_accepts() {
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");

    let table = descriptors(&f.plan);
    assert_eq!(table.len(), 1);
    let digest = only_digest(&f.plan);
    let d = table.get(&digest).expect("keyed by digest");

    assert_eq!(d.digest, digest);
    assert_eq!(d.byte_length, png().len() as u64);
    // Sniffed from content, not from the `.png` name.
    assert_eq!(d.media_type, "image/png");
    assert_eq!(d.purpose, "background-image");
    assert_eq!(d.encoding, "identity");
    assert_eq!(d.provenance.display_name.as_deref(), Some("survey.png"));
    assert!(d.validate().is_empty(), "{:?}", d.validate());
}

#[test]
fn a_background_image_descriptor_is_optional_because_the_spec_says_so() {
    // BEHAVIOR-SPEC §2.5: a level references background images "through asset
    // descriptors whose requirement level is optional". Writing `required`
    // would let one unreadable scan lock an entire plan.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");

    let digest = only_digest(&f.plan);
    assert_eq!(
        descriptors(&f.plan)
            .get(&digest)
            .expect("present")
            .requirement,
        Requirement::Optional
    );
}

#[test]
fn a_missing_optional_blob_leaves_the_document_editable() {
    // The §9.6 row this unlocks: "Optional | … | Missing … ⇒ Document opens
    // editable; placeholder rendering; non-blocking diagnostic." Before
    // descriptors persisted, every reference was undeclared and therefore
    // blocking, so one absent background image made a plan read-only.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let digest = only_digest(&f.plan);

    // Strip the blob but keep the reference and the descriptor, which is what
    // a thin package or a lost cache looks like from here.
    let mut pkg = domowik_container::read_package(&f.plan).expect("read");
    pkg.assets.remove(&digest);
    let stripped = f.plan.with_extension("stripped.domo");
    domowik_container::write_package(&stripped, &pkg).expect("write");

    let reopened = domowik_container::read_package(&stripped).expect("read back");
    let (mode, unresolved) = reopened.open_mode_from_embedded();
    assert_eq!(
        mode,
        domowik_container::OpenMode::Editable,
        "an optional missing asset must not block editing"
    );
    assert_eq!(unresolved.len(), 1, "but it is still reported");

    // And the contrast that proves the descriptor is what did it: with the
    // descriptor removed the same package is undeclared, so it blocks.
    let mut undescribed = reopened.clone();
    undescribed.descriptors.clear();
    let other = f.plan.with_extension("undescribed.domo");
    domowik_container::write_package(&other, &undescribed).expect("write");
    let (mode, _) = domowik_container::read_package(&other)
        .expect("read")
        .open_mode_from_embedded();
    assert_eq!(mode, domowik_container::OpenMode::ReadOnlyDegraded);
}

#[test]
fn the_descriptor_survives_an_ordinary_edit() {
    // §7.4 I2's sibling obligation: an edit rewrites the whole package, so a
    // descriptor that is not carried is destroyed by the next `add-wall`.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let before = descriptors(&f.plan);

    domowik_cli::cmd_add_wall(&f.plan, 0.0, 0.0, 4000.0, 0.0, 200.0, 2500.0, None)
        .expect("add-wall");

    assert_eq!(descriptors(&f.plan).to_object(), before.to_object());
}

#[test]
fn removing_an_asset_removes_its_descriptor() {
    // A descriptor left behind would describe bytes the package no longer
    // holds, and §10.5 builds U(P) partly from descriptor digests — so a stale
    // one keeps resurrecting the reference the user asked to remove.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let digest = only_digest(&f.plan);

    let message = domowik_cli::cmd_asset_remove(&f.plan, &digest).expect("remove");
    assert!(message.contains("descriptor went with it"), "{message}");
    assert!(descriptors(&f.plan).is_empty());
}

#[test]
fn packing_carries_the_descriptors_through_both_profiles() {
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let before = descriptors(&f.plan).to_object();
    let cache = f.plan.with_extension("cache");

    let portable = f.plan.with_extension("portable.domo");
    domowik_cli::cmd_pack(&f.plan, &portable, true, false, None).expect("portable");
    assert_eq!(descriptors(&portable).to_object(), before);

    let thin = f.plan.with_extension("thin.domo");
    domowik_cli::cmd_pack(&f.plan, &thin, false, true, Some(&cache)).expect("thin");
    assert_eq!(
        descriptors(&thin).to_object(),
        before,
        "a thin package kept the blobs' descriptions or it cannot resolve them later"
    );
}

#[test]
fn remote_bytes_count_a_described_blob_the_package_does_not_hold() {
    // Issue 0026's sixth criterion. Before descriptors persisted this class
    // could only ever read zero, because the declared length lives on the
    // descriptor and there were none.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let digest = only_digest(&f.plan);
    let declared = png().len() as u64;

    let mut pkg = domowik_container::read_package(&f.plan).expect("read");
    pkg.assets.remove(&digest);
    let thin_like = f.plan.with_extension("remote.domo");
    domowik_container::write_package(&thin_like, &pkg).expect("write");

    let listing = domowik_cli::cmd_asset_list(&thin_like, None).expect("list");
    assert!(
        listing.contains(&format!("remote:  {declared} byte(s)")),
        "remote bytes were not counted from the descriptor: {listing}"
    );
    assert!(listing.contains("package: 0 byte(s)"), "{listing}");
}

#[test]
fn verify_reports_a_reference_that_has_no_descriptor() {
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");

    let mut pkg = domowik_container::read_package(&f.plan).expect("read");
    pkg.descriptors.clear();
    let bare = f.plan.with_extension("bare.domo");
    domowik_container::write_package(&bare, &pkg).expect("write");

    let report = domowik_cli::cmd_asset_verify(&bare).expect("verify");
    assert!(report.contains("1 without one"), "{report}");
}

#[test]
fn re_importing_a_file_does_not_destroy_what_its_descriptor_already_carries() {
    // Found by adversarial self-review, not by a failing test. Re-import used
    // to overwrite the descriptor wholesale, so a licence or an author a user
    // had recorded was silently destroyed — §8 U2 stripping, committed by the
    // one command whose job is to add information. The digest is unchanged, so
    // the bytes are unchanged, so the existing description still holds.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("first import");

    let mut plan = domowik_container::Plan::open(&f.plan).expect("open");
    let digest = plan.manifest.asset_digests()[0].clone();
    let mut doc = plan
        .descriptors()
        .get(&digest)
        .cloned()
        .expect("descriptor");
    doc.as_object_mut().expect("object").insert(
        "license".to_owned(),
        serde_json::json!({ "expression": "CC-BY-4.0" }),
    );
    doc["provenance"]["author"] = serde_json::json!("A. Surveyor");
    plan.insert_descriptor(&digest, doc).expect("enrich");
    plan.save().expect("save");

    let message = domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("re-import");
    assert!(
        message.contains("existing descriptor was left"),
        "{message}"
    );

    let table = descriptors(&f.plan);
    let d = table.get(&digest).expect("still there");
    assert_eq!(
        d.license.as_ref().map(|l| l.expression.as_str()),
        Some("CC-BY-4.0"),
        "re-import destroyed the recorded licence"
    );
    assert_eq!(
        d.provenance.author.as_deref(),
        Some("A. Surveyor"),
        "re-import destroyed the recorded author"
    );
}

#[test]
fn a_first_import_still_writes_a_descriptor() {
    // The other half: keeping an existing descriptor must not turn into
    // never writing one.
    let f = fixture();
    let message = domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    assert!(
        message.contains("optional-requirement descriptor"),
        "{message}"
    );
    assert_eq!(descriptors(&f.plan).len(), 1);
}

#[test]
fn a_descriptor_whose_blob_and_reference_are_gone_can_still_be_removed() {
    // §10.5 puts descriptor digests in U(P), so an entry outliving its blob
    // and its references is still part of the referenced universe. Before,
    // `asset remove` refused it as "holds no asset" and there was no way to
    // clear it at all.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let digest = only_digest(&f.plan);

    // Strip the blob and every reference, keeping only the descriptor.
    let mut pkg = domowik_container::read_package(&f.plan).expect("read");
    pkg.assets.remove(&digest);
    pkg.manifest.asset_root.retain(|r| r.digest != digest);
    for level in &mut pkg.snapshot.levels {
        level.background_images.clear();
    }
    let stale = f.plan.with_extension("stale.domo");
    domowik_container::write_package(&stale, &pkg).expect("write");
    assert_eq!(
        descriptors(&stale).len(),
        1,
        "the descriptor is the only trace"
    );

    domowik_cli::cmd_asset_remove(&stale, &digest).expect("remove the stale descriptor");
    assert!(descriptors(&stale).is_empty());
}

#[test]
fn a_descriptor_that_does_not_name_its_own_digest_is_refused() {
    // Adversarial review, finding 1. The agreement check read the document's
    // `digest` with `and_then(as_str)`, so a descriptor with no `digest`, or a
    // non-string one, produced `None` and skipped the check entirely — filed
    // under the caller's key without ever claiming to describe those bytes.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let mut plan = domowik_container::Plan::open(&f.plan).expect("open");
    let digest = plan.manifest.asset_digests()[0].clone();

    for bad in [
        serde_json::json!({ "requirement": "optional" }),
        serde_json::json!({ "digest": 7, "requirement": "optional" }),
        serde_json::json!({ "digest": null, "requirement": "optional" }),
    ] {
        assert!(
            plan.insert_descriptor(&digest, bad.clone()).is_err(),
            "accepted a descriptor that names no digest of its own: {bad}"
        );
    }

    // And one that names a DIFFERENT digest is still refused.
    let other = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    assert!(plan
        .insert_descriptor(&digest, serde_json::json!({ "digest": other }))
        .is_err());
}

#[test]
fn a_key_mismatched_descriptor_cannot_unlock_an_unresolvable_asset() {
    // Adversarial review, finding 2, and the one that mattered: the §9.6 gate
    // read `requirement` out of whatever sat under the key, without checking
    // the document agreed it described those bytes. A descriptor filed under A
    // while naming B says "optional" about something else — and trusting it
    // opens a document editable on the strength of a description of different
    // content, which is a fail-closed bypass.
    //
    // `insert_descriptor` now refuses to create such an entry, so this builds
    // one directly, the way a package from another writer could.
    let f = fixture();
    domowik_cli::cmd_asset_add(&f.plan, &f.image, None, 10.0).expect("add");
    let digest = only_digest(&f.plan);

    let mut pkg = domowik_container::read_package(&f.plan).expect("read");
    pkg.assets.remove(&digest); // the blob is now unresolvable
    let other = "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    pkg.descriptors.insert(
        digest.clone(),
        serde_json::json!({ "digest": other, "requirement": "optional" }),
    );
    let forged = f.plan.with_extension("forged.domo");
    domowik_container::write_package(&forged, &pkg).expect("write");

    let (mode, _) = domowik_container::read_package(&forged)
        .expect("read")
        .open_mode_from_embedded();
    assert_eq!(
        mode,
        domowik_container::OpenMode::ReadOnlyDegraded,
        "a descriptor naming another digest was trusted to make this optional"
    );
}
