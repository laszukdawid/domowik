//! ASSET-STRATEGY §10.4 profiles, and issue 0026's three size classes.

use std::collections::{BTreeMap, BTreeSet};

use domowik_assets::cache::AdmissionLimits;
use domowik_assets::descriptor::{AssetDescriptor, Hint, Provenance, Requirement};
use domowik_assets::pack::{plan_pack, size_totals, PackInputs, Profile};
use domowik_assets::CacheStore;

fn descriptor(digest: &str, byte_length: u64, hints: Vec<Hint>) -> AssetDescriptor {
    AssetDescriptor {
        digest: digest.to_owned(),
        byte_length,
        media_type: "image/png".to_owned(),
        purpose: "background-image".to_owned(),
        encoding: "identity".to_owned(),
        requirement: Requirement::Required,
        provenance: Provenance::default(),
        license: None,
        placeholder_dimensions: None,
        hints,
        unknown: serde_json::Map::new(),
    }
}

fn hint() -> Hint {
    Hint {
        url: "https://example.com/scan.png".to_owned(),
        added_at: "2026-09-12T00:00:00Z".to_owned(),
        last_verified_at: None,
        expires_at: None,
    }
}

fn embedded(pairs: &[(&str, &[u8])]) -> BTreeMap<String, Vec<u8>> {
    pairs
        .iter()
        .map(|(d, b)| ((*d).to_owned(), b.to_vec()))
        .collect()
}

fn universe(digests: &[&str]) -> BTreeSet<String> {
    digests.iter().map(|d| (*d).to_owned()).collect()
}

fn cache() -> (tempfile::TempDir, CacheStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = CacheStore::open(dir.path(), AdmissionLimits::default()).expect("cache");
    (dir, store)
}

#[test]
fn portable_embeds_the_whole_universe_and_carries_what_it_does_not_name() {
    let a = b"first asset";
    let b = b"second asset";
    let unnamed = b"referenced only by a field this version cannot read";
    let (da, db, du) = (
        domowik_container::digest_of(a),
        domowik_container::digest_of(b),
        domowik_container::digest_of(unnamed),
    );
    let blobs = embedded(&[(&da, a), (&db, b), (&du, unnamed)]);
    let u = universe(&[&da, &db]);

    let plan = plan_pack(
        Profile::Portable,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: None,
            descriptors: &[],
        },
    )
    .expect("portable plan");

    assert_eq!(plan.embed, {
        let mut v = vec![da.clone(), db.clone()];
        v.sort();
        v
    });
    assert!(plan.external.is_empty());
    // §7.4 I2: a blob the universe does not name is CARRIED, not dropped.
    assert_eq!(plan.carried, vec![du.clone()]);
    assert_eq!(
        plan.totals.package_bytes,
        (a.len() + b.len() + unnamed.len()) as u64
    );
    assert_eq!(plan.totals.cache_bytes, 0);
    assert_eq!(plan.totals.remote_bytes, 0);
}

#[test]
fn portable_names_every_unhonourable_asset_not_just_the_first() {
    // §10.4: "Any unverifiable reference aborts packing completely with an
    // omission report naming every such asset."
    let present = b"here";
    let dp = domowik_container::digest_of(present);
    let missing: Vec<String> = (0..4)
        .map(|i| domowik_container::digest_of(format!("gone {i}").as_bytes()))
        .collect();

    let blobs = embedded(&[(&dp, present)]);
    let mut all = vec![dp.clone()];
    all.extend(missing.iter().cloned());
    let u: BTreeSet<String> = all.into_iter().collect();

    let report = plan_pack(
        Profile::Portable,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: None,
            descriptors: &[],
        },
    )
    .expect_err("a portable package cannot be written over missing blobs");

    assert_eq!(report.omissions.len(), missing.len());
    assert_eq!(report.universe, u.len());
    let named: BTreeSet<&str> = report.omissions.iter().map(|o| o.digest.as_str()).collect();
    for digest in &missing {
        assert!(named.contains(digest.as_str()), "{digest} was not named");
    }
    let rendered = report.render();
    for digest in &missing {
        assert!(rendered.contains(digest.as_str()));
    }
}

#[test]
fn portable_can_pull_a_blob_out_of_the_cache_to_embed_it() {
    // The thin-to-portable conversion §10.4 describes: the blob is not in the
    // package, the cache has it, and embedding is a byte copy (§10.1).
    let bytes = b"lives in the cache";
    let d = domowik_container::digest_of(bytes);
    let (_dir, store) = cache();
    store.admit_bytes(&d, bytes).expect("admit");

    let blobs = embedded(&[]);
    let u = universe(&[&d]);
    let plan = plan_pack(
        Profile::Portable,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: Some(&store),
            descriptors: &[],
        },
    )
    .expect("portable from cache");

    assert_eq!(plan.embed, vec![d.clone()]);
    assert_eq!(plan.totals.package_bytes, bytes.len() as u64);
}

#[test]
fn portable_refuses_a_cached_blob_whose_bytes_no_longer_match() {
    let bytes = b"was intact";
    let d = domowik_container::digest_of(bytes);
    let (dir, store) = cache();
    store.admit_bytes(&d, bytes).expect("admit");
    std::fs::write(
        dir.path()
            .join(domowik_container::blob_entry_name(&d).expect("mapping")),
        b"tampered",
    )
    .expect("tamper");

    let blobs = embedded(&[]);
    let u = universe(&[&d]);
    let report = plan_pack(
        Profile::Portable,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: Some(&store),
            descriptors: &[],
        },
    )
    .expect_err("a damaged cache copy cannot be embedded");
    assert_eq!(report.omissions.len(), 1);
    assert!(report.omissions[0].reason.contains("no longer match"));
}

#[test]
fn thin_keeps_blobs_outside_when_the_cache_holds_them() {
    let bytes = b"stays in the cache";
    let d = domowik_container::digest_of(bytes);
    let (_dir, store) = cache();
    store.admit_bytes(&d, bytes).expect("admit");

    let blobs = embedded(&[(&d, bytes)]);
    let u = universe(&[&d]);
    let plan = plan_pack(
        Profile::Thin,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: Some(&store),
            descriptors: &[],
        },
    )
    .expect("thin plan");

    assert!(plan.embed.is_empty(), "thin embedded a member of U(P)");
    assert_eq!(plan.external, vec![d.clone()]);
    assert_eq!(plan.totals.cache_bytes, bytes.len() as u64);
    assert_eq!(plan.totals.package_bytes, 0);
}

#[test]
fn thin_refuses_to_write_a_package_that_would_dangle() {
    // This is the reason `--thin` was previously refused outright: without
    // somewhere for the blob to live, a thin package dangles by construction.
    // Now the refusal is per-asset and conditional rather than blanket.
    let bytes = b"embedded but not cached";
    let d = domowik_container::digest_of(bytes);
    let blobs = embedded(&[(&d, bytes)]);
    let u = universe(&[&d]);

    let report = plan_pack(
        Profile::Thin,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: None,
            descriptors: &[],
        },
    )
    .expect_err("thin must not strand a blob");
    assert_eq!(report.omissions.len(), 1);
    assert!(report.omissions[0].reason.contains("cache"));
    assert_eq!(report.profile, "thin");
}

#[test]
fn thin_accepts_a_blob_that_only_a_hint_names_and_counts_it_as_remote() {
    // §10.4: a thin package's blobs live "in the cache or a policy-gated
    // remote store/hint". The byte count is the DECLARED length, because
    // nothing has fetched or weighed it.
    let bytes = b"only reachable by hint";
    let d = domowik_container::digest_of(bytes);
    let declared = 4096;

    let blobs = embedded(&[]);
    let u = universe(&[&d]);
    let plan = plan_pack(
        Profile::Thin,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: None,
            descriptors: &[descriptor(&d, declared, vec![hint()])],
        },
    )
    .expect("thin with a hint");

    assert_eq!(plan.external, vec![d.clone()]);
    assert_eq!(plan.totals.remote_bytes, declared);
    assert_eq!(plan.totals.cache_bytes, 0);
    assert_eq!(plan.totals.package_bytes, 0);

    // Without the hint the same input is refused.
    assert!(plan_pack(
        Profile::Thin,
        &PackInputs {
            universe: &u,
            embedded: &blobs,
            cache: None,
            descriptors: &[descriptor(&d, declared, Vec::new())],
        },
    )
    .is_err());
}

#[test]
fn the_three_size_classes_partition_the_assets_and_never_double_count() {
    // Issue 0026's remaining criterion: package, cache and remote bytes are
    // distinct. One blob belongs to exactly one class.
    let in_package = b"embedded";
    let in_cache = b"cached only";
    let dp = domowik_container::digest_of(in_package);
    let dc = domowik_container::digest_of(in_cache);
    let dr = domowik_container::digest_of(b"remote only");
    let declared_remote = 9_000;

    let (_dir, store) = cache();
    store.admit_bytes(&dc, in_cache).expect("admit");
    // Also admit the embedded one, to prove it is not counted twice.
    store.admit_bytes(&dp, in_package).expect("admit");

    let blobs = embedded(&[(&dp, in_package)]);
    let u = universe(&[&dp, &dc, &dr]);
    let totals = size_totals(&PackInputs {
        universe: &u,
        embedded: &blobs,
        cache: Some(&store),
        descriptors: &[
            descriptor(&dr, declared_remote, vec![hint()]),
            descriptor(&dc, in_cache.len() as u64, Vec::new()),
        ],
    });

    assert_eq!(totals.package_bytes, in_package.len() as u64);
    assert_eq!(totals.cache_bytes, in_cache.len() as u64);
    assert_eq!(totals.remote_bytes, declared_remote);
}

#[test]
fn a_plan_is_the_same_whether_or_not_anything_is_written() {
    // §10.4 requires the totals to be reported BEFORE packing, which is only
    // meaningful if planning is pure. Running it twice must agree, and neither
    // run may touch the cache.
    let bytes = b"unchanged";
    let d = domowik_container::digest_of(bytes);
    let (_dir, store) = cache();
    store.admit_bytes(&d, bytes).expect("admit");
    let blobs = embedded(&[]);
    let u = universe(&[&d]);
    let inputs = PackInputs {
        universe: &u,
        embedded: &blobs,
        cache: Some(&store),
        descriptors: &[],
    };

    let first = plan_pack(Profile::Portable, &inputs).expect("first");
    let second = plan_pack(Profile::Portable, &inputs).expect("second");
    assert_eq!(first, second);
    assert!(store.contains(&d), "planning disturbed the cache");
    assert_eq!(store.digests().expect("digests"), vec![d]);
}
