//! BEHAVIOR-SPEC §9.6's condition matrix, exercised row by row.

use std::collections::BTreeMap;

use domowik_assets::cache::AdmissionLimits;
use domowik_assets::descriptor::{AssetDescriptor, Extent, Provenance, Requirement};
use domowik_assets::resolve::{
    resolve, DegradedReason, InstanceRef, Lookup, MediaSupport, OpenMode, ResolutionState,
};
use domowik_assets::{CacheStore, RetrievalPolicy};
use uuid::Uuid;

const PNG: &str = "image/png";

fn descriptor(digest: &str, requirement: Requirement) -> AssetDescriptor {
    AssetDescriptor {
        digest: digest.to_owned(),
        byte_length: 0,
        media_type: PNG.to_owned(),
        purpose: "background-image".to_owned(),
        encoding: "identity".to_owned(),
        requirement,
        provenance: Provenance {
            display_name: Some("scan.png".to_owned()),
            ..Provenance::default()
        },
        license: None,
        placeholder_dimensions: None,
        hints: Vec::new(),
        unknown: serde_json::Map::new(),
    }
}

fn instance(digest: &str) -> InstanceRef {
    InstanceRef {
        instance_id: Uuid::now_v7(),
        level_id: Uuid::now_v7(),
        digest: digest.to_owned(),
    }
}

fn embedded(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(d, b)| ((*d).to_owned(), b.to_vec()))
        .collect()
}

struct Env {
    policy: RetrievalPolicy,
    media: MediaSupport,
}

impl Env {
    fn new() -> Self {
        Self {
            policy: RetrievalPolicy::default(),
            media: MediaSupport::new([PNG]),
        }
    }
    fn lookup<'a>(
        &'a self,
        embedded: &'a BTreeMap<String, Vec<u8>>,
        cache: Option<&'a CacheStore>,
    ) -> Lookup<'a> {
        Lookup {
            embedded,
            cache,
            retrieval: &self.policy,
            media: &self.media,
        }
    }
}

#[test]
fn an_embedded_blob_with_a_descriptor_resolves() {
    // The row this whole module exists for: a present asset must report
    // Resolved, not degraded. An implementation that assumed degradation
    // everywhere would pass every other test in this file and fail this one.
    let bytes = b"a real scan";
    let d = domowik_container::digest_of(bytes);
    let env = Env::new();
    let blobs = embedded(&[(&d, bytes)]);
    let snapshot = resolve(
        &[instance(&d)],
        &[descriptor(&d, Requirement::Required)],
        &env.lookup(&blobs, None),
    );
    assert_eq!(
        snapshot.iter().next().unwrap().state,
        ResolutionState::Resolved
    );
    assert_eq!(snapshot.open_mode(), OpenMode::Editable);
    assert!(snapshot.diagnostics(&[]).is_empty());
    assert_eq!(snapshot.offline_unavailable(), 0);
}

#[test]
fn a_required_missing_asset_blocks_editable_open_and_an_optional_one_does_not() {
    // §9.6, rows "Required | Thin | Missing" and "Optional | Thin | Missing".
    let env = Env::new();
    let blobs = embedded(&[]);
    let missing = domowik_container::digest_of(b"not here");

    for (requirement, expected) in [
        (Requirement::Required, OpenMode::ReadOnlyDegraded),
        (Requirement::Optional, OpenMode::Editable),
    ] {
        let snapshot = resolve(
            &[instance(&missing)],
            &[descriptor(&missing, requirement.clone())],
            &env.lookup(&blobs, None),
        );
        assert_eq!(snapshot.open_mode(), expected, "{requirement:?}");
        let diagnostics = snapshot.diagnostics(&[descriptor(&missing, requirement.clone())]);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].blocking,
            requirement == Requirement::Required
        );
        assert_eq!(diagnostics[0].name.as_deref(), Some("scan.png"));
    }
}

#[test]
fn a_cached_blob_resolves_and_a_damaged_cached_blob_reads_as_a_mismatch() {
    // §10.6's lookup order reaches the cache, and §9.6 makes a digest
    // mismatch behave exactly like a missing blob.
    let dir = tempfile::tempdir().expect("tempdir");
    let cache = CacheStore::open(dir.path(), AdmissionLimits::default()).expect("cache");
    let bytes = b"cached scan";
    let d = domowik_container::digest_of(bytes);
    cache.admit_bytes(&d, bytes).expect("admit");

    let env = Env::new();
    let blobs = embedded(&[]);
    let snapshot = resolve(
        &[instance(&d)],
        &[descriptor(&d, Requirement::Required)],
        &env.lookup(&blobs, Some(&cache)),
    );
    assert_eq!(
        snapshot.iter().next().unwrap().state,
        ResolutionState::Resolved
    );
    assert_eq!(snapshot.open_mode(), OpenMode::Editable);

    // Damage it where it lies.
    let path = dir
        .path()
        .join(domowik_container::blob_entry_name(&d).expect("mapping"));
    std::fs::write(&path, b"tampered").expect("tamper");

    let snapshot = resolve(
        &[instance(&d)],
        &[descriptor(&d, Requirement::Required)],
        &env.lookup(&blobs, Some(&cache)),
    );
    assert_eq!(
        snapshot.iter().next().unwrap().state,
        ResolutionState::Degraded(DegradedReason::DigestMismatch)
    );
    assert_eq!(snapshot.open_mode(), OpenMode::ReadOnlyDegraded);
}

#[test]
fn a_present_blob_of_an_undecodable_type_still_blocks_when_required() {
    // §9.6: "Required | Portable read | Embedded blob present, digest valid,
    // unsupported encoding/media type" ⇒ read-only degraded, editable open
    // blocked. The bytes are fine; this build cannot use them.
    let bytes = b"a jpeg, as far as anyone here knows";
    let d = domowik_container::digest_of(bytes);
    let mut env = Env::new();
    env.media = MediaSupport::new([PNG]);
    let blobs = embedded(&[(&d, bytes)]);

    let mut desc = descriptor(&d, Requirement::Required);
    desc.media_type = "image/jpeg".to_owned();

    let snapshot = resolve(&[instance(&d)], &[desc.clone()], &env.lookup(&blobs, None));
    assert_eq!(
        snapshot.iter().next().unwrap().state,
        ResolutionState::Degraded(DegradedReason::UnsupportedMediaType {
            media_type: "image/jpeg".to_owned()
        })
    );
    assert_eq!(snapshot.open_mode(), OpenMode::ReadOnlyDegraded);
    // The remediation says the bytes are preserved, because they are.
    assert!(snapshot.diagnostics(&[desc])[0]
        .remediation
        .contains("preserved"));
}

#[test]
fn an_instance_with_no_descriptor_is_required_missing() {
    // §10.6: a well-formed but unresolved instance→digest reference with no
    // descriptor is treated as required-missing. It must not slip through as
    // optional just because nothing said it was required.
    let env = Env::new();
    let blobs = embedded(&[]);
    let orphan = domowik_container::digest_of(b"described by nothing");

    let snapshot = resolve(&[instance(&orphan)], &[], &env.lookup(&blobs, None));
    let entry = snapshot.iter().next().unwrap();
    assert_eq!(entry.requirement, Requirement::Required);
    assert_eq!(
        entry.state,
        ResolutionState::Degraded(DegradedReason::NoDescriptor)
    );
    assert_eq!(snapshot.open_mode(), OpenMode::ReadOnlyDegraded);
}

#[test]
fn declared_extent_is_the_descriptor_verbatim_and_absent_when_undeclared() {
    // ED-0106: the §9.6 fallback box belongs to the renderer. This snapshot
    // reports what the descriptor said and nothing else, so a consumer can
    // still tell a DECLARED 1000 mm box from an absent declaration.
    let env = Env::new();
    let blobs = embedded(&[]);
    let declared = domowik_container::digest_of(b"declared");
    let undeclared = domowik_container::digest_of(b"undeclared");

    let mut with_extent = descriptor(&declared, Requirement::Optional);
    let extent = Extent {
        width_mm: 600.0,
        height_mm: 750.0,
        depth_mm: 400.0,
    };
    with_extent.placeholder_dimensions = Some(extent);

    let snapshot = resolve(
        &[instance(&declared), instance(&undeclared)],
        &[with_extent, descriptor(&undeclared, Requirement::Optional)],
        &env.lookup(&blobs, None),
    );
    let extents: Vec<_> = snapshot.iter().map(|e| e.declared_extent).collect();
    assert!(extents.contains(&Some(extent)));
    assert!(extents.contains(&None));
}

#[test]
fn resolved_instances_are_listed_alongside_degraded_ones() {
    // The snapshot is not a filter. A consumer hit-testing placeholders must
    // be able to walk one list.
    let bytes = b"present";
    let here = domowik_container::digest_of(bytes);
    let gone = domowik_container::digest_of(b"absent");
    let env = Env::new();
    let blobs = embedded(&[(&here, bytes)]);

    let snapshot = resolve(
        &[instance(&here), instance(&gone)],
        &[
            descriptor(&here, Requirement::Optional),
            descriptor(&gone, Requirement::Optional),
        ],
        &env.lookup(&blobs, None),
    );
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot.iter().filter(|e| e.state.is_degraded()).count(), 1);
    assert_eq!(
        snapshot
            .iter()
            .filter(|e| e.state == ResolutionState::Resolved)
            .count(),
        1
    );
}

#[test]
fn entries_are_ordered_by_instance_id_and_lookup_by_id_works() {
    let env = Env::new();
    let blobs = embedded(&[]);
    let d = domowik_container::digest_of(b"x");
    let instances: Vec<InstanceRef> = (0..8).map(|_| instance(&d)).collect();

    let snapshot = resolve(
        &instances,
        &[descriptor(&d, Requirement::Optional)],
        &env.lookup(&blobs, None),
    );
    let ids: Vec<Uuid> = snapshot.iter().map(|e| e.instance_id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);

    for i in &instances {
        assert_eq!(
            snapshot.get(i.instance_id).map(|e| e.instance_id),
            Some(i.instance_id)
        );
    }
    assert!(snapshot.get(Uuid::now_v7()).is_none());
}

#[test]
fn the_offline_summary_counts_assets_not_instances() {
    // §9.6: "one summary of how many assets are unavailable rather than
    // per-asset noise." Twenty instances of one missing model are ONE.
    let env = Env::new();
    let blobs = embedded(&[]);
    let model = domowik_container::digest_of(b"one missing model");
    let instances: Vec<InstanceRef> = (0..20).map(|_| instance(&model)).collect();

    let snapshot = resolve(
        &instances,
        &[descriptor(&model, Requirement::Optional)],
        &env.lookup(&blobs, None),
    );
    assert_eq!(snapshot.len(), 20);
    assert_eq!(snapshot.offline_unavailable(), 1);
    // And the diagnostic is one entry naming all twenty instances.
    let diagnostics = snapshot.diagnostics(&[descriptor(&model, Requirement::Optional)]);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].instances.len(), 20);
}

#[test]
fn consent_changes_the_reason_but_never_invents_a_blob() {
    // With retrieval off the reason is the offline one; with consent granted
    // and no fetcher the blob is simply Missing. Neither produces a resolved
    // asset, because this crate does not fetch.
    let blobs = embedded(&[]);
    let d = domowik_container::digest_of(b"remote");
    let media = MediaSupport::new([PNG]);

    for (consent, expected) in [
        (false, DegradedReason::RetrievalDisabledOffline),
        (true, DegradedReason::Missing),
    ] {
        let policy = RetrievalPolicy {
            consent,
            ..RetrievalPolicy::default()
        };
        let lookup = Lookup {
            embedded: &blobs,
            cache: None,
            retrieval: &policy,
            media: &media,
        };
        let snapshot = resolve(
            &[instance(&d)],
            &[descriptor(&d, Requirement::Optional)],
            &lookup,
        );
        assert_eq!(
            snapshot.iter().next().unwrap().state,
            ResolutionState::Degraded(expected),
            "consent = {consent}"
        );
    }
}

#[test]
fn reopening_identical_conditions_produces_an_identical_snapshot() {
    // §9.6: "Reopening applies this matrix deterministically: identical
    // conditions produce identical open modes, diagnostics, and placeholder
    // geometry."
    let bytes = b"stable";
    let here = domowik_container::digest_of(bytes);
    let gone = domowik_container::digest_of(b"absent");
    let env = Env::new();
    let blobs = embedded(&[(&here, bytes)]);
    let instances = vec![instance(&here), instance(&gone), instance(&gone)];
    let descriptors = vec![
        descriptor(&here, Requirement::Optional),
        descriptor(&gone, Requirement::Required),
    ];

    let first = resolve(&instances, &descriptors, &env.lookup(&blobs, None));
    let second = resolve(&instances, &descriptors, &env.lookup(&blobs, None));
    assert_eq!(first, second);
    assert_eq!(
        first.diagnostics(&descriptors),
        second.diagnostics(&descriptors)
    );
    assert_eq!(first.open_mode(), OpenMode::ReadOnlyDegraded);
}

#[test]
fn an_undescribed_reference_resolves_when_its_blob_is_present() {
    // §10.6's required-missing rule is scoped to a DANGLING reference that is
    // UNRESOLVED. A blob that is embedded and digest-verified is neither, so
    // an undescribed instance pointing at it is not missing — calling it
    // missing would contradict §9.6's own definition of missing.
    //
    // The breadth matters in practice: descriptors do not persist yet, so the
    // wide reading would put a document into read-only degraded inspection on
    // the presence of a single asset instance whose blob is sitting right
    // there in the package.
    let bytes = b"present, just undescribed";
    let d = domowik_container::digest_of(bytes);
    let env = Env::new();
    let blobs = embedded(&[(&d, bytes)]);

    let snapshot = resolve(&[instance(&d)], &[], &env.lookup(&blobs, None));
    let entry = snapshot.iter().next().unwrap();
    assert_eq!(entry.state, ResolutionState::Resolved);
    // Conservative on the half nothing proved: no descriptor said optional.
    assert_eq!(entry.requirement, Requirement::Required);
    assert_eq!(snapshot.open_mode(), OpenMode::Editable);
    assert!(snapshot.diagnostics(&[]).is_empty());

    // The undescribed reference that is genuinely dangling still blocks.
    let gone = domowik_container::digest_of(b"undescribed and absent");
    let snapshot = resolve(&[instance(&gone)], &[], &env.lookup(&blobs, None));
    assert_eq!(
        snapshot.iter().next().unwrap().state,
        ResolutionState::Degraded(DegradedReason::NoDescriptor)
    );
    assert_eq!(snapshot.open_mode(), OpenMode::ReadOnlyDegraded);
}

#[test]
fn the_availability_predicate_answers_only_the_availability_column() {
    // The container decides the §9.6 matrix; this crate answers whether a
    // digest is there and usable. One boolean covers every failure row,
    // because §9.6 treats a digest mismatch exactly like a missing blob and
    // puts undecodable and unsupported in the same column.
    use domowik_assets::resolve::availability;

    let dir = tempfile::tempdir().expect("tempdir");
    let cache = CacheStore::open(dir.path(), AdmissionLimits::default()).expect("cache");
    let good = b"intact in the cache";
    let dg = domowik_container::digest_of(good);
    cache.admit_bytes(&dg, good).expect("admit");

    let damaged = b"was intact";
    let dd = domowik_container::digest_of(damaged);
    cache.admit_bytes(&dd, damaged).expect("admit");
    std::fs::write(
        dir.path()
            .join(domowik_container::blob_entry_name(&dd).expect("mapping")),
        b"tampered",
    )
    .expect("tamper");

    let env = Env::new();
    let blobs = embedded(&[]);
    let lookup = env.lookup(&blobs, Some(&cache));
    let available = availability(&lookup);

    assert!(available(&dg), "an intact cached blob is available");
    assert!(
        !available(&dd),
        "a digest mismatch must read as unavailable"
    );
    assert!(!available(&domowik_container::digest_of(b"never stored")));
    assert!(
        !available("sha256:not-a-digest"),
        "a malformed digest is not available"
    );
}

#[test]
fn an_undeclared_requirement_blocks_an_editable_open() {
    // §9.6 has no row for a requirement level it does not define. The safe
    // reading of a missing row refuses to edit, and this is the same choice
    // the container's own gate makes on the same field.
    let env = Env::new();
    let blobs = embedded(&[]);
    let missing = domowik_container::digest_of(b"absent");

    let snapshot = resolve(
        &[instance(&missing)],
        &[descriptor(
            &missing,
            Requirement::Undeclared(serde_json::json!("advisory")),
        )],
        &env.lookup(&blobs, None),
    );
    assert_eq!(snapshot.open_mode(), OpenMode::ReadOnlyDegraded);

    // And it changes nothing when the blob resolves, which is what stops the
    // rule degenerating into "any unusual value blocks everything".
    let bytes = b"present";
    let here = domowik_container::digest_of(bytes);
    let blobs = embedded(&[(&here, bytes)]);
    let snapshot = resolve(
        &[instance(&here)],
        &[descriptor(
            &here,
            Requirement::Undeclared(serde_json::json!("advisory")),
        )],
        &env.lookup(&blobs, None),
    );
    assert_eq!(snapshot.open_mode(), OpenMode::Editable);
}
