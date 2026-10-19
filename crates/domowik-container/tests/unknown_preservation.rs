//! FORMAT-SPEC §8.4 U2 at the three envelopes this container persists.
//!
//! The entity types inside a document have carried `unknown` maps since v0.1.
//! The envelopes AROUND them — the manifest, the document's own top level, and
//! each batch — did not, so a field a later version added beside `walls`, or
//! beside `operations`, or at the top of the manifest, was read, discarded, and
//! written back absent with no error. §8.4 U2 requires it preserved or
//! write-back refused; neither happened.
//!
//! Each assertion here fails if its `#[serde(flatten)] unknown` map is removed,
//! which is what makes them worth having. A test that only round-tripped a
//! package the writer produced would pass either way, because a package this
//! version wrote has nothing this version does not understand.

use std::io::{Read, Write};
use std::path::Path;

use domowik_container::{read_package, write_package, Package};
use domowik_document::operation::{Batch, Operation};
use domowik_format::{Document, Entity, Level};

/// Reads every entry of a package as JSON.
fn entries(path: &Path) -> Vec<(String, serde_json::Value)> {
    let file = std::fs::File::open(path).expect("open");
    let mut archive = zip::ZipArchive::new(file).expect("archive");
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.name_for_index(i).expect("name").to_owned())
        .collect();
    names
        .into_iter()
        .map(|name| {
            let mut text = String::new();
            archive
                .by_name(&name)
                .expect("entry")
                .read_to_string(&mut text)
                .expect("utf8");
            (name, serde_json::from_str(&text).expect("json"))
        })
        .collect()
}

fn write_entries(path: &Path, entries: &[(String, serde_json::Value)]) {
    let file = std::fs::File::create(path).expect("create");
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
    for (name, value) in entries {
        zip.start_file(name.clone(), options).expect("start");
        zip.write_all(&serde_json::to_vec(value).expect("serialise"))
            .expect("write");
    }
    zip.finish().expect("finish");
}

/// A package with one batch, written by the real writer so every shape is the
/// one the reader actually meets.
fn seed(path: &Path) {
    let mut package = Package::new(Document::with_ground_floor());
    package.batches.push(Batch::new(
        uuid::Uuid::now_v7(),
        1,
        Default::default(),
        vec![Operation::create(Entity::Level(Level::new("Upper")))],
    ));
    write_package(path, &package).expect("seed");
}

/// Decorates every entry with a field no version of this reader knows, the way
/// a later writer would leave one behind, and repairs the §7.4 inventory over
/// the decorated bytes.
///
/// The repair is not incidental. Decorating an entry changes its bytes, so
/// leaving the inventory alone would make the package fail at the integrity
/// layer and every assertion below would pass for that reason instead of the
/// one it names. A later writer that added these fields would have written a
/// matching inventory, and this reproduces that writer rather than a corrupt
/// package.
fn decorate(source: &Path, target: &Path) -> Vec<String> {
    const MANIFEST: &str = "domowik.json";
    let mut all = entries(source);
    let mut names = Vec::new();
    for (name, value) in all.iter_mut() {
        let object = value.as_object_mut().expect("a JSON object");
        object.insert(
            "aFieldThisVersionDoesNotKnow".to_owned(),
            serde_json::json!({"nested": ["structure", 1, true]}),
        );
        names.push(name.clone());
    }

    let inventory: serde_json::Map<String, serde_json::Value> = all
        .iter()
        .filter(|(name, _)| name != MANIFEST)
        .map(|(name, value)| {
            let bytes = serde_json::to_vec(value).expect("serialise");
            (
                name.clone(),
                serde_json::json!({
                    "sha256": domowik_container::digest_of(&bytes),
                    "byteLength": bytes.len(),
                }),
            )
        })
        .collect();
    for (name, value) in all.iter_mut() {
        if name == MANIFEST {
            value
                .as_object_mut()
                .expect("object")
                .insert("entries".to_owned(), serde_json::Value::Object(inventory));
            break;
        }
    }

    write_entries(target, &all);
    names
}

#[test]
fn unknown_fields_survive_a_read_and_write_at_every_envelope() {
    let dir = tempfile::tempdir().expect("tempdir");
    let seeded = dir.path().join("seed.domo");
    let decorated = dir.path().join("decorated.domo");
    let rewritten = dir.path().join("rewritten.domo");

    seed(&seeded);
    let decorated_names = decorate(&seeded, &decorated);
    assert!(
        decorated_names.len() >= 3,
        "the seed must have a manifest, a document and a batch; got {decorated_names:?}"
    );

    let package = read_package(&decorated).expect("a package with unknown fields must still read");
    write_package(&rewritten, &package).expect("and must write back");

    let after: std::collections::BTreeMap<String, serde_json::Value> =
        entries(&rewritten).into_iter().collect();

    let mut stripped = Vec::new();
    for name in &decorated_names {
        let kept = after
            .get(name)
            .and_then(|value| value.get("aFieldThisVersionDoesNotKnow"))
            .is_some();
        if !kept {
            stripped.push(name.clone());
        }
    }
    assert!(
        stripped.is_empty(),
        "FORMAT-SPEC §8.4 U2 forbids silent stripping; these entries lost their \
         unknown field on write-back: {stripped:?}"
    );
}

#[test]
fn a_preserved_unknown_field_keeps_its_value_not_merely_its_key() {
    // Preserving the KEY and rewriting the VALUE would satisfy a presence
    // check while still corrupting the data §8.4 U2 exists to protect. U2's
    // words are "identical meaning", so the value is what has to match.
    let dir = tempfile::tempdir().expect("tempdir");
    let seeded = dir.path().join("seed.domo");
    let decorated = dir.path().join("decorated.domo");
    let rewritten = dir.path().join("rewritten.domo");

    seed(&seeded);
    decorate(&seeded, &decorated);
    let package = read_package(&decorated).expect("read");
    write_package(&rewritten, &package).expect("write");

    let expected = serde_json::json!({"nested": ["structure", 1, true]});
    for (name, value) in entries(&rewritten) {
        let actual = value
            .get("aFieldThisVersionDoesNotKnow")
            .unwrap_or_else(|| panic!("{name} lost its unknown field"));
        assert_eq!(actual, &expected, "{name} changed the value it carried");
    }
}

#[test]
fn a_package_this_version_wrote_gains_no_unknown_fields() {
    // The control. Without it the assertions above would still pass if the
    // reader invented an `aFieldThisVersionDoesNotKnow` key, and a writer that
    // emits spurious keys is its own defect: an older reader would treat them
    // as unknown data and propagate them forever.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("plain.domo");
    seed(&path);

    for (name, value) in entries(&path) {
        let object = value.as_object().expect("a JSON object");
        assert!(
            !object.contains_key("aFieldThisVersionDoesNotKnow"),
            "{name} carries a key nothing put there"
        );
        assert!(
            !object.contains_key("unknown"),
            "{name} serialised its unknown MAP as a field named `unknown`; \
             `#[serde(flatten)]` must splice the entries into the object"
        );
    }
}
