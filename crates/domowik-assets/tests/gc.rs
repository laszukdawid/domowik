//! ASSET-STRATEGY §10.7 — pins, leases, and fail-closed collection.

use std::io::Write;

use domowik_assets::cache::{AdmissionLimits, CacheStore};
use domowik_assets::gc::{
    lease_is_expired, Anomaly, ClockReading, CollectReport, Collector, Health, Lease, MetaStore,
    MonoAnchor, Pin, PlatformMode, RecordsLock, DEFAULT_CROSS_BOOT_GRACE_SECONDS,
};
use domowik_assets::time::parse_rfc3339_utc;

const AT: &str = "2026-09-12T00:00:00Z";

fn fixture() -> (tempfile::TempDir, CacheStore, MetaStore) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = CacheStore::open(dir.path(), AdmissionLimits::default()).expect("cache");
    let meta = MetaStore::open(dir.path()).expect("meta");
    (dir, store, meta)
}

fn admit(store: &CacheStore, bytes: &[u8]) -> String {
    let d = domowik_container::digest_of(bytes);
    store.admit_bytes(&d, bytes).expect("admit");
    d
}

fn pin(digest: &str, owner: &str, kind: domowik_assets::gc::RootKind) -> Pin {
    Pin {
        digest: digest.to_owned(),
        owner_id: owner.to_owned(),
        root_kind: kind,
        created_at: AT.to_owned(),
    }
}

fn now_utc(stamp: &str) -> ClockReading {
    ClockReading::UtcOnly {
        now_utc_seconds: parse_rfc3339_utc(stamp).expect("timestamp"),
    }
}

fn run(store: &CacheStore, meta: &mut MetaStore, now: &ClockReading) -> CollectReport {
    Collector::new(store, meta, PlatformMode::UtcOnly)
        .collect(now)
        .expect("collect")
}

#[test]
fn an_unpinned_blob_is_collected_and_a_pinned_one_is_not() {
    use domowik_assets::gc::RootKind;
    let (_dir, store, mut meta) = fixture();
    let kept = admit(&store, b"referenced by a document");
    let loose = admit(&store, b"nothing points at this");

    meta.add_pin(pin(&kept, "doc-a", RootKind::Snapshot))
        .expect("pin");

    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.collected, vec![loose.clone()]);
    assert_eq!(report.retained, vec![kept.clone()]);
    assert!(report.fail_closed.is_none());
    assert!(store.contains(&kept));
    assert!(!store.contains(&loose));
}

#[test]
fn a_manual_pin_survives_deregistration_and_the_others_do_not() {
    // §10.7: "`manual` pins survive document deregistration".
    use domowik_assets::gc::RootKind;
    let (_dir, store, mut meta) = fixture();
    let by_hand = admit(&store, b"the user pinned this themselves");
    let by_document = admit(&store, b"a snapshot root");

    meta.register_document(
        "doc-a",
        &[
            (by_hand.clone(), RootKind::Manual),
            (by_document.clone(), RootKind::Snapshot),
        ],
        AT,
    )
    .expect("register");
    meta.deregister_document("doc-a").expect("deregister");

    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.retained, vec![by_hand.clone()]);
    assert_eq!(report.collected, vec![by_document.clone()]);
    assert!(store.contains(&by_hand));
    assert!(!store.contains(&by_document));
}

#[test]
fn an_unexpired_lease_protects_and_an_expired_one_does_not() {
    let (_dir, store, mut meta) = fixture();
    let live = admit(&store, b"a holder is using this right now");
    let stale = admit(&store, b"a holder used this yesterday");

    meta.add_lease(Lease {
        digest: live.clone(),
        holder_id: "holder-1".into(),
        expires_at_utc: "2026-09-13T00:00:00Z".into(),
        anchor: None,
    })
    .expect("live lease");
    meta.add_lease(Lease {
        digest: stale.clone(),
        holder_id: "holder-2".into(),
        expires_at_utc: "2026-09-10T00:00:00Z".into(),
        anchor: None,
    })
    .expect("stale lease");

    // Cross-boot/UTC-only expiry is the deadline PLUS M, so a lease that
    // lapsed two days ago is collectable and one lapsing tomorrow is not.
    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.retained, vec![live.clone()]);
    assert_eq!(report.collected, vec![stale.clone()]);
}

#[test]
fn the_expiry_decision_prefers_the_monotonic_anchor_within_one_boot() {
    // §10.7: same-boot collection requires now_mono > monoDeadline. A forward
    // wall-clock jump must not shorten a lease whose boot is unchanged.
    let lease = Lease {
        digest: "sha256:00".into(),
        holder_id: "h".into(),
        // Long past, and deliberately so: the wall clock says "expired".
        expires_at_utc: "2000-01-01T00:00:00Z".into(),
        anchor: Some(MonoAnchor {
            boot_id: "boot-1".into(),
            mono_deadline_nanos: 5_000,
        }),
    };
    let same_boot_before = ClockReading::Monotonic {
        boot_id: "boot-1".into(),
        now_mono_nanos: 4_999,
        now_utc_seconds: parse_rfc3339_utc(AT).expect("now"),
    };
    let same_boot_after = ClockReading::Monotonic {
        boot_id: "boot-1".into(),
        now_mono_nanos: 5_001,
        now_utc_seconds: parse_rfc3339_utc(AT).expect("now"),
    };
    assert!(!lease_is_expired(
        &lease,
        &same_boot_before,
        DEFAULT_CROSS_BOOT_GRACE_SECONDS
    ));
    assert!(lease_is_expired(
        &lease,
        &same_boot_after,
        DEFAULT_CROSS_BOOT_GRACE_SECONDS
    ));

    // A DIFFERENT boot cannot compare monotonic readings, so the rule falls
    // back to persisted UTC plus M — and this lease is long past that.
    let other_boot = ClockReading::Monotonic {
        boot_id: "boot-2".into(),
        now_mono_nanos: 1,
        now_utc_seconds: parse_rfc3339_utc(AT).expect("now"),
    };
    assert!(lease_is_expired(
        &lease,
        &other_boot,
        DEFAULT_CROSS_BOOT_GRACE_SECONDS
    ));
}

#[test]
fn the_cross_boot_rule_honours_the_grace_period_on_both_sides_of_it() {
    let deadline = "2026-09-12T00:00:00Z";
    let lease = Lease {
        digest: "sha256:00".into(),
        holder_id: "h".into(),
        expires_at_utc: deadline.into(),
        anchor: None,
    };
    let at = parse_rfc3339_utc(deadline).expect("deadline");
    let grace = DEFAULT_CROSS_BOOT_GRACE_SECONDS;

    // Derived from the deadline and M, not restated as a second wall-clock
    // literal: the boundary is "past deadline + M", so deadline + M exactly is
    // still protected.
    for (offset, expected) in [(0, false), (grace, false), (grace + 1, true)] {
        let now = ClockReading::UtcOnly {
            now_utc_seconds: at + offset,
        };
        assert_eq!(
            lease_is_expired(&lease, &now, grace),
            expected,
            "offset {offset}s from the deadline"
        );
    }
}

#[test]
fn an_unreadable_deadline_never_reads_as_expired() {
    // Failing open here would collect a blob because its metadata was damaged
    // — precisely the direction §10.7 forbids.
    let lease = Lease {
        digest: "sha256:00".into(),
        holder_id: "h".into(),
        expires_at_utc: "not a timestamp".into(),
        anchor: None,
    };
    assert!(!lease_is_expired(
        &lease,
        &now_utc("2999-01-01T00:00:00Z"),
        DEFAULT_CROSS_BOOT_GRACE_SECONDS
    ));
}

/// Damages the journal the way the named anomaly class would, then reopens.
fn reopen_damaged(dir: &std::path::Path, damage: impl FnOnce(&mut String)) -> MetaStore {
    let path = dir.join("meta/journal");
    let mut text = std::fs::read_to_string(&path).expect("journal");
    damage(&mut text);
    std::fs::write(&path, text).expect("rewrite");
    MetaStore::open(dir).expect("reopen")
}

#[test]
fn a_checksum_failure_stops_all_collection_not_just_the_bad_record() {
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let a = admit(&store, b"protected");
    let b = admit(&store, b"unprotected");
    meta.add_pin(pin(&a, "doc-a", RootKind::Snapshot))
        .expect("pin");
    drop(meta);

    // Flip a byte inside the record's content, leaving its checksum stale.
    let mut meta = reopen_damaged(dir.path(), |text| {
        *text = text.replace("\"ownerId\":\"doc-a\"", "\"ownerId\":\"doc-b\"");
    });
    assert!(matches!(
        meta.health(),
        Health::FailClosed(Anomaly::Checksum { .. })
    ));

    let report = run(&store, &mut meta, &now_utc(AT));
    assert!(report.collected.is_empty(), "collected while fail-closed");
    assert!(report.fail_closed.is_some());
    // Zero collection: the UNPROTECTED blob survives too. A collector that
    // merely skipped the damaged record would have taken this one.
    assert!(store.contains(&a));
    assert!(store.contains(&b));
}

#[test]
fn a_generation_gap_and_a_regression_both_fail_closed() {
    use domowik_assets::gc::RootKind;
    for damage in ["gap", "regression"] {
        let (dir, store, mut meta) = fixture();
        let loose = admit(&store, b"unprotected");
        meta.add_pin(pin("sha256:aa", "doc-a", RootKind::Snapshot))
            .expect("pin one");
        meta.add_pin(pin("sha256:bb", "doc-a", RootKind::Snapshot))
            .expect("pin two");
        drop(meta);

        let mut meta = reopen_damaged(dir.path(), |text| {
            let replacement = if damage == "gap" { 7 } else { 1 };
            let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
            lines[1] =
                lines[1].replace("\"generation\":2", &format!("\"generation\":{replacement}"));
            *text = lines.join("\n") + "\n";
        });

        assert!(
            matches!(
                meta.health(),
                Health::FailClosed(Anomaly::Generation { .. })
            ),
            "{damage}: expected a generation anomaly, got {:?}",
            meta.health()
        );
        let report = run(&store, &mut meta, &now_utc(AT));
        assert!(report.collected.is_empty(), "{damage}: collected anyway");
        assert!(store.contains(&loose), "{damage}: blob was taken");
    }
}

#[test]
fn a_torn_tail_from_a_crash_mid_append_fails_closed() {
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let loose = admit(&store, b"unprotected");
    meta.add_pin(pin("sha256:aa", "doc-a", RootKind::Snapshot))
        .expect("pin");
    drop(meta);

    // A partial line: the process died between `write_all` and the newline.
    let path = dir.path().join("meta/journal");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open");
    file.write_all(b"{\"generation\":2,\"record\":{\"op\":\"addP")
        .expect("torn write");
    drop(file);

    let mut meta = MetaStore::open(dir.path()).expect("reopen");
    assert!(matches!(
        meta.health(),
        Health::FailClosed(Anomaly::Unparsable { .. })
    ));
    let report = run(&store, &mut meta, &now_utc(AT));
    assert!(report.collected.is_empty());
    assert!(store.contains(&loose));
}

#[test]
fn rebuild_restores_collection_and_keeps_the_conservative_superset() {
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let known = admit(&store, b"a pin the store still remembers");
    let recomputed = admit(&store, b"a root the caller recomputed");
    let loose = admit(&store, b"genuinely unreferenced");
    meta.add_pin(pin(&known, "doc-a", RootKind::Snapshot))
        .expect("pin");
    drop(meta);

    let mut meta = reopen_damaged(dir.path(), |text| text.push_str("{ this is not json\n"));
    assert!(matches!(meta.health(), Health::FailClosed(_)));

    meta.rebuild(&[pin(&recomputed, "doc-b", RootKind::RootTable)])
        .expect("rebuild");
    assert_eq!(meta.health(), &Health::Clean);

    let report = run(&store, &mut meta, &now_utc(AT));
    // Both survive: the one the store remembered AND the one the caller
    // recomputed. A rebuild that took only the recomputed set would have
    // dropped protection for a document that could not be read.
    assert_eq!(report.retained, {
        let mut v = vec![known.clone(), recomputed.clone()];
        v.sort();
        v
    });
    assert_eq!(report.collected, vec![loose.clone()]);

    // And the rebuilt journal survives a restart.
    drop(meta);
    let meta = MetaStore::open(dir.path()).expect("reopen after rebuild");
    assert_eq!(meta.health(), &Health::Clean);
    assert_eq!(meta.pins().count(), 2);
}

#[test]
fn pins_and_leases_survive_a_restart() {
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let d = admit(&store, b"durably protected");
    meta.add_pin(pin(&d, "doc-a", RootKind::OperationFrontier))
        .expect("pin");
    let before = meta.generation();
    drop(meta);

    let mut meta = MetaStore::open(dir.path()).expect("reopen");
    assert_eq!(meta.health(), &Health::Clean);
    assert_eq!(meta.generation(), before);
    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.retained, vec![d.clone()]);
    assert!(store.contains(&d));
}

#[test]
fn the_collector_reports_the_platform_mode_it_started_in() {
    // §10.7: "Active mode is observable at collector startup."
    let (_dir, store, mut meta) = fixture();
    let report = Collector::new(&store, &mut meta, PlatformMode::UtcOnly)
        .collect(&now_utc(AT))
        .expect("collect");
    assert_eq!(report.mode, PlatformMode::UtcOnly);

    let detected = PlatformMode::detect();
    if cfg!(target_os = "linux") {
        assert!(matches!(detected, PlatformMode::Monotonic { .. }));
    } else {
        // §10.7 defaults Windows and macOS to UTC-only until a verified
        // monotonic mechanism is documented for them.
        assert_eq!(detected, PlatformMode::UtcOnly);
    }
}

#[test]
fn rebuild_keeps_pins_recorded_after_the_damaged_record() {
    // The conservative superset has to survive an anomaly in the MIDDLE of the
    // journal, not only at its end. Replay stops at the damaged record, so a
    // rebuild that took only the pins it managed to read would drop every pin
    // recorded after the damage — and then enable collection, which deletes
    // exactly those blobs. §10.7's "unreadable documents' previously pinned
    // digests stay protected" is the rule this would break, and the failure
    // mode is data loss rather than over-retention.
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let before = admit(&store, b"pinned before the damage");
    let after = admit(&store, b"pinned after the damage");

    meta.add_pin(pin(&before, "doc-a", RootKind::Snapshot))
        .expect("first pin");
    meta.add_pin(pin(&after, "doc-b", RootKind::Snapshot))
        .expect("second pin");
    drop(meta);

    // Damage the FIRST record, leaving the second intact behind it.
    let mut meta = reopen_damaged(dir.path(), |text| {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[0] = lines[0].replace("\"ownerId\":\"doc-a\"", "\"ownerId\":\"doc-x\"");
        *text = lines.join("\n") + "\n";
    });
    assert!(matches!(meta.health(), Health::FailClosed(_)));

    meta.rebuild(&[]).expect("rebuild");
    let report = run(&store, &mut meta, &now_utc(AT));

    assert!(
        report.collected.is_empty(),
        "a rebuild dropped protection and collected {:?}",
        report.collected
    );
    assert!(
        store.contains(&before),
        "the pre-damage pin's blob was taken"
    );
    assert!(
        store.contains(&after),
        "the post-damage pin's blob was taken"
    );
}

#[test]
fn a_fail_closed_store_refuses_new_records_instead_of_compounding_the_damage() {
    // Replay stops applying at the anomaly, so an append would reuse a
    // generation number the surviving tail already carries, and the next
    // replay would meet a fresh anomaly somewhere new. Nothing is being
    // collected meanwhile, so nothing urgently needs protecting.
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let digest = admit(&store, b"protected");
    meta.add_pin(pin(&digest, "doc-a", RootKind::Snapshot))
        .expect("pin");
    drop(meta);

    let mut meta = reopen_damaged(dir.path(), |text| text.push_str("{ not json\n"));
    assert!(matches!(meta.health(), Health::FailClosed(_)));

    let error = meta
        .add_pin(pin("sha256:aa", "doc-b", RootKind::Manual))
        .expect_err("a fail-closed store must not be appended to");
    assert!(error.to_string().contains("fail-closed"), "got {error}");

    // The journal on disk is untouched by the refusal, so a rebuild still
    // sees everything it saw before.
    meta.rebuild(&[]).expect("rebuild");
    assert_eq!(meta.health(), &Health::Clean);
    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.retained, vec![digest]);
}

#[test]
fn a_held_lock_stops_a_second_collector_and_stops_new_pins() {
    // §10.7 G2: admission and collection are mutually atomic against the
    // metadata store. A pin recorded after a collector has computed its
    // protected set but before it reaches that digest would otherwise be
    // invisible, and the blob it protects would be deleted.
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let digest = admit(&store, b"at risk during the window");

    let held = RecordsLock::acquire(dir.path()).expect("first holder");

    let pin_error = meta
        .add_pin(pin(&digest, "doc-a", RootKind::Snapshot))
        .expect_err("a pin must not slip past a held lock");
    assert!(
        pin_error.to_string().contains("collection holds"),
        "{pin_error}"
    );

    let collect_error = Collector::new(&store, &mut meta, PlatformMode::UtcOnly)
        .collect(&now_utc(AT))
        .expect_err("a second collector must not run");
    assert!(
        collect_error.to_string().contains("collection holds"),
        "{collect_error}"
    );

    // Nothing was deleted while the lock was contended.
    assert!(store.contains(&digest));

    drop(held);
    meta.add_pin(pin(&digest, "doc-a", RootKind::Snapshot))
        .expect("pin once the lock is free");
    let report = run(&store, &mut meta, &now_utc(AT));
    assert_eq!(report.retained, vec![digest]);
}

#[test]
fn the_lock_is_released_when_its_guard_goes_out_of_scope() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let _held = RecordsLock::acquire(dir.path()).expect("acquire");
        assert!(RecordsLock::acquire(dir.path()).is_err());
    }
    // Released on drop, so ordinary sequential use never meets the lock.
    RecordsLock::acquire(dir.path()).expect("free again");
}

#[test]
fn registering_a_document_is_one_locked_span_not_one_per_root() {
    // A collector running between two roots would find the later ones
    // unprotected and delete them. The registration either happens under the
    // lock or does not happen at all.
    use domowik_assets::gc::RootKind;
    let (dir, store, mut meta) = fixture();
    let a = admit(&store, b"root one");
    let b = admit(&store, b"root two");
    let roots = vec![
        (a.clone(), RootKind::Snapshot),
        (b.clone(), RootKind::OperationFrontier),
    ];

    let held = RecordsLock::acquire(dir.path()).expect("hold the lock");
    let error = meta
        .register_document("doc-a", &roots, AT)
        .expect_err("registration must not proceed under a held lock");
    assert!(error.to_string().contains("collection holds"), "{error}");
    // Nothing partial was written.
    assert_eq!(meta.pins().count(), 0);
    drop(held);

    meta.register_document("doc-a", &roots, AT)
        .expect("register");
    assert_eq!(meta.pins().count(), 2);
    let report = run(&store, &mut meta, &now_utc(AT));
    assert!(report.collected.is_empty());
    assert_eq!(report.retained, {
        let mut v = vec![a, b];
        v.sort();
        v
    });
}
