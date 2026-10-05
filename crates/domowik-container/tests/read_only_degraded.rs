//! BEHAVIOR-SPEC §9.6: a plan whose required assets cannot be resolved opens
//! read-only, and the core refuses to edit or save it.
//!
//! §9.6 is not a UI state. It makes the blocked open a property of the
//! document and its assets, so the decision belongs where the package is
//! opened and the enforcement where mutations enter the core. A front end that
//! only greyed out its buttons would leave the CLI, a script, and any future
//! GUI free to edit and save a plan §9.6 says must not be saved normally.
//!
//! What must still work is as important as what must not. §9.6: "pan, zoom,
//! viewing, and diagnostics work, and CLI `info` and `validate` operate".
//! Inspection is asserted here alongside the refusals, because a gate that
//! closed everything would satisfy every refusal test in this file.

use domowik_container::{write_package, ContainerError, OpenMode, Package, Plan, Requirement};
use domowik_document::operation::Operation;
use domowik_format::{Anchor, BackgroundImage, Document, Entity, Level, Point, Wall};

/// A plan whose only level carries a background image, with the blob ABSENT
/// and the descriptor declaring `requirement`.
fn plan_referencing_a_missing_blob(requirement: Option<&str>) -> Package {
    let digest = format!("sha256:{}", "b".repeat(64));
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
    let level_id = level.id;

    let mut document = Document {
        levels: vec![level],
        ..Document::default()
    };
    document.walls.push(Wall::new(
        level_id,
        Point::new(0.0, 0.0),
        Point::new(4000.0, 0.0),
        200.0,
        2500.0,
    ));

    let mut package = Package::new(document);
    if let Some(requirement) = requirement {
        package
            .descriptors
            // A descriptor carries its own `digest` because §10.2 makes the
            // field required and the §9.6 gate will not trust a document that
            // does not say which bytes it describes.
            .insert(
                digest.clone(),
                serde_json::json!({"digest": digest, "requirement": requirement}),
            );
    }
    package
}

fn write(package: &Package) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, package).expect("write");
    (dir, path)
}

#[test]
fn a_missing_required_asset_opens_read_only() {
    // §9.6, Required / Thin / missing: "Editable open blocked; read-only
    // degraded inspection available".
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("required")));
    let plan = Plan::open(&path).expect("§9.6 says the package OPENS, read-only");

    assert_eq!(plan.mode(), OpenMode::ReadOnlyDegraded);
    assert_eq!(plan.unresolved().len(), 1);
    let asset = &plan.unresolved()[0];
    assert_eq!(asset.requirement, Requirement::Required);
    assert!(asset.blocks_editing());
    // §9.6 requires the diagnostic to name purpose as well as identity: which
    // levels use it, not merely that something is absent.
    assert_eq!(asset.used_by, vec!["Ground".to_owned()]);
}

#[test]
fn a_missing_optional_asset_opens_editable() {
    // §9.6, Optional / Thin / missing: "Document opens editable; placeholder
    // rendering; non-blocking diagnostic". This is the control that stops the
    // gate from being "any missing blob blocks everything" — without it, a
    // check that ignored `requirement` entirely would pass every other test
    // in this file.
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("optional")));
    let mut plan = Plan::open(&path).expect("open");

    assert_eq!(plan.mode(), OpenMode::Editable);
    // The diagnostic still reports it — non-blocking is not silent.
    assert_eq!(plan.unresolved().len(), 1);
    assert_eq!(plan.unresolved()[0].requirement, Requirement::Optional);
    assert!(!plan.unresolved()[0].blocks_editing());

    plan.session()
        .expect("editable")
        .commit(vec![Operation::create(Entity::Level(Level::new("Upper")))])
        .expect("commit");
    plan.save().expect("and save");
}

#[test]
fn an_undeclared_requirement_fails_closed() {
    // §9.6's matrix has no row for an asset whose requirement level is not
    // declared. A package that references bytes nobody can supply, and does
    // not say whether they matter, is not one to save over — so this reader
    // refuses and records the choice rather than taking the permissive reading
    // in silence. Routed to issue 0005 as a §9.6 matrix gap.
    let (_dir, path) = write(&plan_referencing_a_missing_blob(None));
    let plan = Plan::open(&path).expect("open");

    assert_eq!(plan.mode(), OpenMode::ReadOnlyDegraded);
    assert_eq!(plan.unresolved()[0].requirement, Requirement::Undeclared);
}

#[test]
fn an_embedded_blob_resolves_without_any_cache() {
    // A portable package is self-sufficient. If this needed a resolver, every
    // portable package would open degraded on a machine with no cache, which
    // is the opposite of what the profile is for.
    let mut package = plan_referencing_a_missing_blob(Some("required"));
    let digest = package.snapshot.levels[0].background_images[0]
        .digest
        .clone();
    // Real bytes for that digest, so the package genuinely embeds it.
    let bytes = b"background image bytes".to_vec();
    let real = domowik_container::digest_of(&bytes);
    package.snapshot.levels[0].background_images[0].digest = real.clone();
    package.descriptors.remove(&digest);
    package.descriptors.insert(
        real.clone(),
        serde_json::json!({"digest": real, "requirement": "required"}),
    );
    package.assets.insert(real, bytes);

    let (_dir, path) = write(&package);
    let plan = Plan::open(&path).expect("open");
    assert_eq!(plan.mode(), OpenMode::Editable);
    assert!(plan.unresolved().is_empty());
}

#[test]
fn a_caller_supplied_resolver_can_satisfy_a_thin_package() {
    // The thin case: the blob lives in the asset cache, which belongs to issue
    // 0026. This crate decides the §9.6 MATRIX and the caller answers
    // availability, so neither has to grow the other's responsibility.
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("required")));

    let degraded = Plan::open(&path).expect("open");
    assert_eq!(degraded.mode(), OpenMode::ReadOnlyDegraded);

    let resolved = Plan::open_with_resolution(&path, |_| true).expect("open");
    assert_eq!(
        resolved.mode(),
        OpenMode::Editable,
        "the same package, with the cache answering, is editable"
    );
}

#[test]
fn editing_is_refused_while_degraded() {
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("required")));
    let mut plan = Plan::open(&path).expect("open");

    let error = plan.session().expect_err("§9.6 blocks interactive editing");
    match error {
        ContainerError::ReadOnlyDegraded { ref assets, .. } => {
            // The diagnostic must be actionable: §9.6 requires identity and
            // purpose, so both are asserted rather than only the variant.
            assert!(assets.contains("sha256:"), "must name the asset: {assets}");
            assert!(assets.contains("Ground"), "must name the user: {assets}");
        }
        other => panic!("expected ReadOnlyDegraded, got {other}"),
    }
}

#[test]
fn saving_is_refused_while_degraded_even_without_an_edit() {
    // "No silent fallback normal-save from a degraded source." A plan can be
    // degraded without ever being mutated, and saving it would rewrite a
    // package whose required assets this device cannot verify — so the save
    // is gated independently of the session, not merely as a consequence of
    // mutation being gated.
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("required")));
    let before = std::fs::read(&path).expect("bytes");
    let mut plan = Plan::open(&path).expect("open");

    assert!(matches!(
        plan.save(),
        Err(ContainerError::ReadOnlyDegraded { .. })
    ));
    assert!(matches!(
        plan.commit_and_save(vec![Operation::create(Entity::Level(Level::new("X")))]),
        Err(ContainerError::ReadOnlyDegraded { .. })
    ));
    assert_eq!(
        std::fs::read(&path).expect("bytes"),
        before,
        "a refused save must not touch the file"
    );
}

#[test]
fn save_as_cannot_launder_a_degraded_plan_into_a_new_file() {
    // The obvious bypass: if `save` is blocked, try `save_as`. It must be
    // blocked too, or "read-only" means "read-only to this path" and the
    // degraded package propagates under a new name with its unresolved
    // references presented as a normal save.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plan.domo");
    write_package(&path, &plan_referencing_a_missing_blob(Some("required"))).expect("write");
    let mut plan = Plan::open(&path).expect("open");

    let elsewhere = dir.path().join("laundered.domo");
    assert!(matches!(
        plan.save_as(&elsewhere),
        Err(ContainerError::ReadOnlyDegraded { .. })
    ));
    assert!(!elsewhere.exists(), "nothing may be written");
    assert_eq!(plan.path(), Some(path.as_path()), "the path must not move");
}

#[test]
fn inspection_still_works_while_degraded() {
    // §9.6: "pan, zoom, viewing, and diagnostics work, and CLI `info` and
    // `validate` operate". THE POSITIVE CONTROL for this whole file. Every
    // other test asserts a refusal, and a gate that refused everything would
    // satisfy all of them; only this shows the mode is degraded rather than
    // closed.
    let (_dir, path) = write(&plan_referencing_a_missing_blob(Some("required")));
    let plan = Plan::open(&path).expect("open");

    assert_eq!(plan.document().levels.len(), 1, "viewing works");
    assert_eq!(plan.document().walls.len(), 1, "geometry is readable");
    assert_eq!(plan.document().levels[0].name, "Ground");
    // Read-only session access, which is what `info`, `validate` and hit
    // testing use, must not be gated.
    assert_eq!(plan.session_ref().log().batches().len(), 0);
    assert!(!plan.session_ref().is_dirty());
    // And the package itself still reads, which is what makes a diagnostic
    // possible at all.
    assert!(domowik_container::read_package(&path).is_ok());
}

#[test]
fn a_plan_that_was_never_opened_is_editable() {
    // `Plan::unsaved` references nothing that could be missing. If the mode
    // defaulted to degraded, a GUI could never start a new document.
    let mut plan = Plan::unsaved(domowik_document::session::Session::new());
    assert_eq!(plan.mode(), OpenMode::Editable);
    assert!(plan.session().is_ok());
}

#[test]
fn a_descriptor_that_does_not_name_its_own_digest_is_undeclared() {
    // The §9.6 gate reads one field out of an otherwise opaque document, so it
    // has to be sure the document is about THESE bytes. Two ways it can fail
    // to be: naming a different digest, or naming none at all. §10.2 makes
    // `digest` a required field, so both are malformed and neither is a licence
    // to treat the asset as optional.
    //
    // This also keeps the container and `domowik-assets` agreeing on every
    // input: a descriptor with no `digest` will not deserialize into an
    // `AssetDescriptor` either, so both sides see "no usable descriptor" and
    // both fail closed. Disagreeing here would put the two readers at odds on
    // exactly the malformed values where it is hardest to notice.
    let digest = format!("sha256:{}", "a".repeat(64));
    let other = format!("sha256:{}", "b".repeat(64));

    for (label, document) in [
        (
            "no digest at all",
            serde_json::json!({"requirement": "optional"}),
        ),
        (
            "a digest that is not a string",
            serde_json::json!({"digest": 7, "requirement": "optional"}),
        ),
        (
            "a digest naming other bytes",
            serde_json::json!({"digest": other, "requirement": "optional"}),
        ),
    ] {
        let mut package = plan_referencing_a_missing_blob(Some("optional"));
        package.descriptors.clear();
        package.descriptors.insert(digest.clone(), document);

        let (_dir, path) = write(&package);
        let plan = Plan::open(&path).expect("open");
        assert_eq!(
            plan.mode(),
            OpenMode::ReadOnlyDegraded,
            "{label}: an unusable descriptor was trusted to make this optional"
        );
    }
}
