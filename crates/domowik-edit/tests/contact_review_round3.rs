//! Regressions from the Codex GPT-6.1-Sol review of issue 0055, round three.
//!
//! Move-then-split rehosting is certified by an extent the host actually
//! held during the batch, forward and inverse: undo retraces intermediate
//! states, so a genuine undo passes and a raw direct rehost does not.

use domowik_document::operation::{Batch, Operation};
use domowik_document::session::Session;
use domowik_document::wall_contact::document_contact_blocks;
use domowik_edit::{add_wall_operations, apply_preview, detach_operations, set_wall_operations};
use domowik_format::{Document, Entity, Opening, Point, Role, Wall};
use uuid::Uuid;

fn wall(level: Uuid, a: (f64, f64), b: (f64, f64)) -> Wall {
    Wall::new(
        level,
        Point::new(a.0, a.1),
        Point::new(b.0, b.1),
        120.0,
        2500.0,
    )
}

fn session(document: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), document, &Default::default(), &[], vec![]).unwrap()
}

fn field(id: Uuid, name: &str, value: serde_json::Value) -> Operation {
    let mut fields = serde_json::Map::new();
    fields.insert(name.to_owned(), value);
    Operation::set_fields(id, fields)
}

fn detached_corner(length: f64) -> (Document, Uuid) {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let host = wall(level, (0.0, 0.0), (length, 0.0));
    let id = host.id;
    document.walls.push(host);
    let side = wall(level, (1000.0, 0.0), (1000.0, 1000.0));
    let built = add_wall_operations(&document, level, side).unwrap();
    for operation in &built.operations {
        apply_preview(&mut document, operation);
    }
    if length == 1000.0 {
        let edit = detach_operations(&document, document.joins[0].id, id, Role::End).unwrap();
        for operation in &edit.operations {
            apply_preview(&mut document, operation);
        }
        assert!(document.joins.is_empty());
    }
    document
        .openings
        .push(Opening::door(id, 0.5, 100.0, 2100.0, 120.0));
    assert!(document.validate().is_empty());
    assert!(document_contact_blocks(&document).is_empty());
    (document, id)
}

#[test]
fn real_move_and_split_with_unchanged_retained_axis_can_be_undone() {
    let (document, id) = detached_corner(1000.0);
    let mut editor = session(document.clone());
    let edit = set_wall_operations(
        editor.document(),
        id,
        None,
        Some(Point::new(4000.0, 0.0)),
        None,
        None,
    )
    .unwrap();

    editor.commit(edit.operations).unwrap();
    assert_eq!(editor.document().wall(id), document.wall(id));
    assert_ne!(editor.document().openings[0].host_wall_id, id);
    assert!(editor.document().validate().is_empty());
    assert!(document_contact_blocks(editor.document()).is_empty());
    let after = editor.document().clone();
    let frontier = editor.log().frontier();
    let undo = editor.undo();
    println!(
        "undo={undo:?}; depths={}/{}; projection_unchanged={}; frontier_unchanged={}",
        editor.undo_depth(),
        editor.redo_depth(),
        editor.document() == &after,
        editor.log().frontier() == frontier
    );
    undo.unwrap().unwrap();
    assert_eq!(editor.document(), &document);
    editor.redo().unwrap().unwrap();
    assert_eq!(editor.document(), &after);
}

#[test]
fn raw_inverse_axis_rewrite_matches_genuine_undo() {
    let (document, id) = detached_corner(950.0);
    let mut editor = session(document.clone());
    let edit = set_wall_operations(
        editor.document(),
        id,
        None,
        Some(Point::new(4000.0, 0.0)),
        None,
        None,
    )
    .unwrap();
    editor.commit(edit.operations).unwrap();
    let before_undo = editor.document().clone();
    assert_eq!(before_undo.wall(id).unwrap().end(), Point::new(1000.0, 0.0));
    editor.undo().unwrap().unwrap();
    assert_eq!(editor.document(), &document);

    let inverse = editor.log().batches().last().unwrap().operations.clone();
    let mut raw = session(before_undo.clone());
    raw.commit(inverse.clone()).unwrap();
    assert_eq!(raw.document(), &document);
    let mut external = session(before_undo);
    let batch = Batch::new(Uuid::now_v7(), 1, external.log().frontier(), inverse);
    external.accept_external(batch).unwrap();
    assert_eq!(external.document(), &document);
}

#[test]
fn forward_certificate_refuses_unrelated_or_inexact_pieces() {
    for case in [
        "valid",
        "unheld_end",
        "gap",
        "off_axis",
        "different_props",
        "reversed",
        "wrong_u",
        "changed_opening",
    ] {
        let mut document = Document::with_ground_floor();
        let level = document.levels[0].id;
        let host = wall(level, (0.0, 0.0), (1000.0, 0.0));
        let opening = Opening::door(host.id, 0.5, 100.0, 2100.0, 120.0);
        let mut child = wall(level, (1000.0, 0.0), (4000.0, 0.0));
        match case {
            "gap" => child.start_mm[0] = 1001.0,
            "off_axis" => {
                child.start_mm[1] = 1.0;
                child.end_mm[1] = 1.0;
            }
            "different_props" => child.thickness_mm = 121.0,
            "reversed" => {
                child.start_mm[0] = 4000.0;
                child.end_mm[0] = 1000.0;
            }
            _ => {}
        }
        let host_id = host.id;
        let opening_id = opening.id;
        let child_id = child.id;
        document.walls.push(host);
        document.openings.push(opening);
        let end = if case == "unheld_end" { 3950.0 } else { 4000.0 };
        let u: f64 = 1.0 / 3.0;
        let u = if case == "wrong_u" {
            f64::from_bits(u.to_bits() + 1)
        } else {
            u
        };
        let mut operations = vec![
            field(host_id, "endMm", serde_json::json!([end, 0.0])),
            Operation::create(Entity::Wall(child)),
            field(host_id, "endMm", serde_json::json!([1000.0, 0.0])),
            field(opening_id, "hostWallId", serde_json::json!(child_id)),
            field(opening_id, "u", serde_json::json!(u)),
        ];
        if case == "changed_opening" {
            operations.push(field(opening_id, "widthMm", serde_json::json!(90.0)));
        }
        let mut local = session(document.clone());
        let result = local.commit(operations.clone());

        assert_eq!(result.is_ok(), case == "valid");
        let mut external = session(document.clone());
        let batch = Batch::new(Uuid::now_v7(), 1, external.log().frontier(), operations);
        let result = external.accept_external(batch);
        assert_eq!(result.is_ok(), case == "valid");
        if case != "valid" {
            assert_eq!(local.document(), &document);
            assert_eq!(external.document(), &document);
        }
    }
}

#[test]
fn inverse_certificate_refuses_broken_chain_or_remap() {
    for case in [
        "valid",
        "gap",
        "off_axis",
        "different_props",
        "reversed",
        "wrong_u",
        "changed_opening",
    ] {
        let mut document = Document::with_ground_floor();
        let level = document.levels[0].id;
        let parent = wall(level, (0.0, 0.0), (1000.0, 0.0));
        let mut child = wall(level, (1000.0, 0.0), (4000.0, 0.0));
        match case {
            "gap" => child.start_mm[0] = 1001.0,
            "off_axis" => {
                child.start_mm[1] = 1.0;
                child.end_mm[1] = 1.0;
            }
            "different_props" => child.thickness_mm = 121.0,
            "reversed" => {
                child.start_mm[0] = 4000.0;
                child.end_mm[0] = 1000.0;
            }
            _ => {}
        }
        let opening = Opening::door(child.id, 1.0 / 3.0, 100.0, 2100.0, 120.0);
        let parent_id = parent.id;
        let child_id = child.id;
        let opening_id = opening.id;
        document.walls.extend([parent, child]);
        document.openings.push(opening);
        assert!(document.validate().is_empty());
        let u = if case == "wrong_u" { 0.500001 } else { 0.5 };
        // A genuine undo retraces the move: it writes the moved extent back
        // before restoring the parent's original end.
        let mut operations = vec![
            field(parent_id, "endMm", serde_json::json!([4000.0, 0.0])),
            field(parent_id, "endMm", serde_json::json!([950.0, 0.0])),
            field(opening_id, "hostWallId", serde_json::json!(parent_id)),
            field(opening_id, "u", serde_json::json!(u)),
            Operation::delete(child_id),
        ];
        if case == "changed_opening" {
            operations.push(field(opening_id, "widthMm", serde_json::json!(90.0)));
        }
        let mut local = session(document.clone());
        let result = local.commit(operations.clone());

        assert_eq!(result.is_ok(), case == "valid");
        let mut external = session(document.clone());
        let batch = Batch::new(Uuid::now_v7(), 1, external.log().frontier(), operations);
        let result = external.accept_external(batch);
        assert_eq!(result.is_ok(), case == "valid");
        if case != "valid" {
            assert_eq!(local.document(), &document);
            assert_eq!(external.document(), &document);
        }
    }
}

/// An inverse that only nets the parent's change, never holding the extent
/// its pieces span, does not certify a merge: it is a direct rehost.
#[test]
fn inverse_without_the_moved_extent_is_refused() {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let parent = wall(level, (0.0, 0.0), (1000.0, 0.0));
    let child = wall(level, (1000.0, 0.0), (4000.0, 0.0));
    let opening = Opening::door(child.id, 1.0 / 3.0, 100.0, 2100.0, 120.0);
    let (parent_id, child_id, opening_id) = (parent.id, child.id, opening.id);
    document.walls.extend([parent, child]);
    document.openings.push(opening);
    let operations = vec![
        field(parent_id, "endMm", serde_json::json!([950.0, 0.0])),
        field(opening_id, "hostWallId", serde_json::json!(parent_id)),
        field(opening_id, "u", serde_json::json!(0.5)),
        Operation::delete(child_id),
    ];
    let mut local = session(document.clone());
    assert!(local.commit(operations).is_err());
    assert_eq!(local.document(), &document);
}
