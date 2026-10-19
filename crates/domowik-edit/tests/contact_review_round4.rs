//! Regressions from the Codex GPT-6.1-Sol review of issue 0055, round four.
//!
//! Undo, redo and compensation retrace a batch's intermediate states,
//! including after an LWW4 replacement, across walls, openings, rooms and
//! levels.

use domowik_document::log::Delivery;
use domowik_document::operation::{Batch, Operation, OperationKind};
use domowik_document::session::Session;
use domowik_edit::set_wall_operations;
use domowik_format::{Document, Entity, Level, Opening, Point, RoomRecord, Wall};
use uuid::Uuid;

fn session(document: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), document, &Default::default(), &[], vec![]).unwrap()
}

fn field(id: Uuid, name: &str, value: serde_json::Value) -> Operation {
    let mut fields = serde_json::Map::new();
    fields.insert(name.to_owned(), value);
    Operation::set_fields(id, fields)
}

fn wall(level: Uuid, x: f64) -> Wall {
    Wall::new(
        level,
        Point::new(x, 0.0),
        Point::new(x + 1000.0, 0.0),
        120.0,
        2500.0,
    )
}

fn detached_corner() -> (Document, Uuid) {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let host = wall(level, 0.0);
    let id = host.id;
    let side = Wall::new(
        level,
        Point::new(1000.0, 0.0),
        Point::new(1000.0, 1000.0),
        120.0,
        2500.0,
    );
    document
        .openings
        .push(Opening::door(id, 0.5, 100.0, 2100.0, 120.0));
    document.walls.extend([host, side]);
    assert!(document.validate().is_empty());
    (document, id)
}

#[test]
fn lww_replacement_then_split_can_be_compensated() {
    for replacement in [false, true] {
        let (document, id) = detached_corner();
        let mut operations = set_wall_operations(
            &document,
            id,
            None,
            Some(Point::new(4000.0, 0.0)),
            None,
            None,
        )
        .unwrap()
        .operations;
        if replacement {
            let mut moved = document.wall(id).unwrap().clone();
            moved.end_mm = [4000.0, 0.0];
            assert!(matches!(
                operations[0].kind,
                OperationKind::SetFields { .. }
            ));
            operations[0] = Operation::create(Entity::Wall(moved));
        }
        let mut editor = session(document.clone());
        let batch = Batch::new(Uuid::now_v7(), 1, editor.log().frontier(), operations);
        editor.accept_external(batch.clone()).unwrap();
        assert_eq!(editor.document().wall(id), document.wall(id));
        let after = editor.document().clone();
        let frontier = editor.log().frontier();
        let inverse = editor.invert_batch(&batch).unwrap();
        println!("replacement={replacement}; inverse={inverse:?}");
        let result = editor.compensate(&batch);
        println!("replacement={replacement}; compensation={result:?}; state_unchanged={}; frontier_unchanged={}", editor.document() == &after, editor.log().frontier() == frontier);
        result.unwrap();
        assert_eq!(editor.document(), &document);
    }
}

#[test]
fn repeated_fields_and_delete_restore_levels_walls_openings_and_rooms() {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let second = Level::new("Upper");
    let second_id = second.id;
    document.levels.push(second);
    let host = wall(level, 0.0);
    let host_id = host.id;
    let opening = Opening::door(host_id, 0.5, 100.0, 2100.0, 120.0);
    let opening_id = opening.id;
    let room = RoomRecord::new(Uuid::now_v7(), level, Point::new(100.0, 100.0));
    let room_id = room.id;
    document.walls.push(host);
    document.openings.push(opening);
    document.room_records.push(room);
    let operations = vec![
        field(level, "name", serde_json::json!("Middle")),
        field(room_id, "name", serde_json::json!("One")),
        field(opening_id, "u", serde_json::json!(0.25)),
        field(host_id, "heightMm", serde_json::json!(3000.0)),
        field(level, "name", serde_json::json!("Final")),
        field(opening_id, "u", serde_json::json!(0.75)),
        field(room_id, "name", serde_json::Value::Null),
        field(host_id, "heightMm", serde_json::json!(3500.0)),
        field(second_id, "name", serde_json::json!("Upper changed")),
        Operation::delete(opening_id),
        Operation::delete(room_id),
        Operation::delete(second_id),
    ];
    let mut editor = session(document.clone());
    editor.commit(operations).unwrap();
    let after = editor.document().clone();
    editor.undo().unwrap().unwrap();
    assert_eq!(editor.document(), &document);
    editor.redo().unwrap().unwrap();
    assert_eq!(editor.document(), &after);
    let redo = editor.log().batches().last().unwrap().clone();
    editor.compensate(&redo).unwrap();
    assert_eq!(editor.document(), &document);
}

#[test]
fn create_then_edit_repeatedly_and_compensate_on_same_actor() {
    let document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let host = wall(level, 0.0);
    let id = host.id;
    let operations = vec![
        Operation::create(Entity::Wall(host)),
        field(id, "heightMm", serde_json::json!(3000.0)),
        field(id, "heightMm", serde_json::json!(4000.0)),
    ];
    let mut editor = session(document.clone());
    editor.commit(operations).unwrap();
    let after = editor.document().clone();
    editor.undo().unwrap().unwrap();
    assert_eq!(editor.document(), &document);
    editor.redo().unwrap().unwrap();
    assert_eq!(editor.document(), &after);
    let redo = editor.log().batches().last().unwrap().clone();
    editor.compensate(&redo).unwrap();
    assert_eq!(editor.document(), &document);
}

#[test]
fn held_repeated_writes_promote_and_compensate() {
    let document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let peer = Uuid::now_v7();
    let mut editor = session(document.clone());
    let first = Batch::new(
        peer,
        1,
        editor.log().frontier(),
        vec![field(level, "name", serde_json::json!("Before repeated"))],
    );
    let mut base = editor.log().frontier();
    base.insert(peer, first.range_end());
    let second = Batch::new(
        peer,
        first.range_end() + 1,
        base,
        vec![
            field(level, "name", serde_json::json!("Intermediate")),
            field(level, "name", serde_json::json!("Final")),
        ],
    );
    assert_eq!(
        editor.accept_external(second.clone()).unwrap(),
        Delivery::Held
    );
    assert_eq!(editor.document(), &document);
    editor.accept_external(first).unwrap();
    assert!(editor.log().held().is_empty());
    assert_eq!(editor.document().level(level).unwrap().name, "Final");
    editor.compensate(&second).unwrap();
    assert_eq!(
        editor.document().level(level).unwrap().name,
        "Before repeated"
    );
}

#[test]
fn pure_lww_replacements_and_following_edits_restore_original_fields() {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let host = wall(level, 0.0);
    let host_id = host.id;
    let mut room = RoomRecord::new(Uuid::now_v7(), level, Point::new(100.0, 100.0));
    room.name = Some("Original".into());
    let room_id = room.id;
    document.walls.push(host.clone());
    document.room_records.push(room.clone());
    let mut replacement_wall = host;
    replacement_wall.height_mm = 3000.0;
    let mut first_room = room.clone();
    first_room.name = Some("Replacement 1".into());
    let mut second_room = room;
    second_room.name = Some("Replacement 2".into());
    let operations = vec![
        field(room_id, "name", serde_json::json!("Before replacement")),
        Operation::create(Entity::RoomRecord(first_room)),
        field(room_id, "seedMm", serde_json::json!([200.0, 300.0])),
        Operation::create(Entity::RoomRecord(second_room)),
        field(room_id, "reviewValue", serde_json::json!([1, 2, 3])),
        Operation::create(Entity::Wall(replacement_wall)),
        field(host_id, "heightMm", serde_json::json!(3500.0)),
    ];
    let mut editor = session(document.clone());
    let batch = Batch::new(Uuid::now_v7(), 1, editor.log().frontier(), operations);
    editor.accept_external(batch.clone()).unwrap();
    editor.compensate(&batch).unwrap();
    assert_eq!(editor.document(), &document);
}

#[test]
fn malformed_repeated_write_and_stale_compensation_are_atomic() {
    let mut document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let host = wall(level, 0.0);
    let host_id = host.id;
    document.walls.push(host);
    let mut editor = session(document.clone());
    let before_frontier = editor.log().frontier();
    assert!(editor
        .commit(vec![
            field(level, "name", serde_json::json!(7)),
            field(level, "name", serde_json::json!("Repaired type"))
        ])
        .is_err());
    assert_eq!(editor.document(), &document);
    assert_eq!(editor.log().frontier(), before_frontier);
    assert_eq!(editor.undo_depth(), 0);
    assert!(!editor.is_dirty());
    editor.commit(vec![Operation::delete(host_id)]).unwrap();
    let deleted = editor.document().clone();
    let stale = Batch::new(
        Uuid::now_v7(),
        1,
        editor.log().frontier(),
        vec![
            field(host_id, "heightMm", serde_json::json!(3000.0)),
            field(host_id, "heightMm", serde_json::json!(3500.0)),
        ],
    );
    assert_eq!(
        editor.accept_external(stale.clone()).unwrap(),
        Delivery::Accepted
    );
    assert_eq!(editor.document(), &deleted);
    assert_eq!(editor.projection().discarded.len(), 1);
    let frontier = editor.log().frontier();
    assert!(editor.compensate(&stale).is_err());
    assert_eq!(editor.document(), &deleted);
    assert_eq!(editor.log().frontier(), frontier);
}

#[test]
fn held_promotion_fences_repeated_local_writes() {
    let document = Document::with_ground_floor();
    let level = document.levels[0].id;
    let mut editor = session(document);
    editor
        .commit(vec![field(
            level,
            "floorThicknessMm",
            serde_json::json!(150.0),
        )])
        .unwrap();
    assert_eq!(editor.undo_depth(), 1);
    let mut base = editor.log().frontier();
    base.insert(editor.actor(), 3);
    let peer = Batch::new(
        Uuid::now_v7(),
        1,
        base,
        vec![field(level, "name", serde_json::json!("Remote"))],
    );
    assert_eq!(editor.accept_external(peer).unwrap(), Delivery::Held);
    editor
        .commit(vec![
            field(level, "name", serde_json::json!("Middle")),
            field(level, "name", serde_json::json!("Local final")),
        ])
        .unwrap();
    assert_eq!(editor.document().level(level).unwrap().name, "Remote");
    assert!(editor.log().held().is_empty());
    assert_eq!(editor.undo_depth(), 0);
    assert_eq!(editor.redo_depth(), 0);
    assert_eq!(editor.undo().unwrap(), None);
}
