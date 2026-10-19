//! Regressions from the Codex GPT-6.1-Sol review of issue 0055, round two.
//!
//! Each test asserts the intended behaviour of a state that failed: rehosts
//! the batch never certifies are refused, collapsed walls release their
//! joins, merged joins undo and redo, and re-noding is bounded and
//! independent of collection order.

use domowik_document::operation::{Batch, Operation};
use domowik_document::session::Session;
use domowik_document::wall_contact::{document_contact_blocks, wall_contact_refusals};
use domowik_edit::{add_wall_operations, apply_preview, set_wall_operations};
use domowik_format::{Document, Entity, Opening, Point, Wall};
use uuid::Uuid;
fn wall(l: Uuid, a: (f64, f64), b: (f64, f64)) -> Wall {
    Wall::new(l, Point::new(a.0, a.1), Point::new(b.0, b.1), 120.0, 2500.0)
}
fn session(d: Document) -> Session {
    Session::open_snapshot(Uuid::now_v7(), d, &Default::default(), &[], vec![]).unwrap()
}
fn draw(d: &mut Document, l: Uuid, a: (f64, f64), b: (f64, f64)) {
    let e = add_wall_operations(d, l, wall(l, a, b)).unwrap();
    for op in &e.operations {
        apply_preview(d, op);
    }
    assert!(d.validate().is_empty(), "{:?}", d.validate());
}
fn field(id: Uuid, k: &str, v: serde_json::Value) -> Operation {
    let mut fields = serde_json::Map::new();
    fields.insert(k.to_owned(), v);
    Operation::set_fields(id, fields)
}
#[test]
fn all_point_walls_release_their_joins() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    draw(&mut d, l, (0.0, 0.0), (1000.0, 0.0));
    draw(&mut d, l, (0.0, 0.0001), (1000.0, 0.0));
    assert_eq!(d.joins.len(), 2);
    let id = d.walls[0].id;
    let e = set_wall_operations(&d, id, None, Some(Point::new(0.0002, 0.0)), None, None).unwrap();
    let mut s = session(d);
    s.commit(e.operations).unwrap();
    let after = s.document();
    let mut independent = after.clone();
    independent.joins.clear();
    // Released memberships leave the walls independent: moving one start
    // no longer co-moves the other into coincidence.
    let free = set_wall_operations(
        &independent,
        id,
        Some(Point::new(1000.0, 0.0)),
        None,
        None,
        None,
    );
    assert!(free.is_ok());
    assert!(
        after.joins.is_empty(),
        "each wall is now a point and must have no membership"
    );
}
fn append_and_rehost() -> (Document, Vec<Operation>) {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    let host = wall(l, (0.0, 0.0), (1000.0, 0.0));
    let opening = Opening::door(host.id, 0.5, 100.0, 2100.0, 120.0);
    let child = wall(l, (1000.0, 0.0), (4000.0, 0.0));
    let id = opening.id;
    let next = child.id;
    d.walls.push(host);
    d.openings.push(opening);
    assert!(d.validate().is_empty());
    let ops = vec![
        Operation::create(Entity::Wall(child)),
        field(id, "hostWallId", serde_json::json!(next)),
        field(id, "u", serde_json::json!(1.0 / 3.0)),
    ];
    (d, ops)
}
#[test]
fn forward_rehost_without_moving_the_host_is_refused() {
    let (d, ops) = append_and_rehost();
    let mut s = session(d.clone());
    let r = s.commit(ops);
    println!("forward rehost result={r:?}; after={:?}", s.document());
    assert!(
        r.is_err(),
        "the original host is never moved or split by this batch"
    );
}
#[test]
fn external_rehost_without_moving_the_host_is_refused() {
    let (d, ops) = append_and_rehost();
    let mut s = session(d);
    let batch = Batch::new(Uuid::now_v7(), 1, s.log().frontier(), ops);
    let r = s.accept_external(batch);
    assert!(r.is_err());
}
#[test]
fn inverse_rehost_without_merging_the_parent_is_refused() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    let parent = wall(l, (0.0, 0.0), (1000.0, 0.0));
    let child = wall(l, (1000.0, 0.0), (4000.0, 0.0));
    let opening = Opening::door(child.id, 1.0 / 3.0, 100.0, 2100.0, 120.0);
    let p = parent.id;
    let c = child.id;
    let id = opening.id;
    d.walls = vec![parent, child];
    d.openings.push(opening);
    assert!(d.validate().is_empty());
    let ops = vec![
        field(id, "hostWallId", serde_json::json!(p)),
        field(id, "u", serde_json::json!(0.5)),
        Operation::delete(c),
    ];
    let mut s = session(d);
    let r = s.commit(ops);
    println!("inverse rehost={r:?}; after={:?}", s.document());
    assert!(r.is_err(), "parent neither extends nor moves");
}
#[test]
fn merged_join_can_be_undone_and_redone() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    draw(&mut d, l, (-1000.0, 0.0), (0.0, 0.0));
    draw(&mut d, l, (0.0, 0.0), (0.0, 1000.0));
    draw(&mut d, l, (1000.0, 0.0), (2000.0, 0.0));
    draw(&mut d, l, (1000.0, 0.0), (1000.0, -1000.0));
    let e = set_wall_operations(
        &d,
        d.walls[0].id,
        None,
        Some(Point::new(1000.0, 0.0)),
        None,
        None,
    )
    .unwrap();
    let mut s = session(d.clone());
    s.commit(e.operations).unwrap();
    assert_eq!(s.document().joins.len(), 1);
    let done = s.document().clone();
    s.undo().unwrap().unwrap();
    assert_eq!(s.document().walls, d.walls);
    assert_eq!(s.document().joins.len(), 2);
    assert!(s.document().validate().is_empty());
    s.redo().unwrap().unwrap();
    assert_eq!(s.document(), &done);
    assert!(document_contact_blocks(s.document()).is_empty());
    assert!(wall_contact_refusals(&d, s.document()).is_empty());
}
fn legacy_chain(count: usize) -> Document {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    for i in 0..count {
        let j = (i / 2) as f64;
        let w = if i % 2 == 0 {
            wall(
                l,
                (j * 3000.0 - 1000.0, j * 3000.0),
                ((j + 1.0) * 3000.0 + 1000.0, j * 3000.0),
            )
        } else {
            wall(
                l,
                ((j + 1.0) * 3000.0, j * 3000.0 - 1000.0),
                ((j + 1.0) * 3000.0, (j + 1.0) * 3000.0 + 1000.0),
            )
        };
        d.walls.push(w);
    }
    assert!(d.validate().is_empty());
    d
}
fn geometry(d: &Document) -> Vec<String> {
    let mut x = d
        .walls
        .iter()
        .map(|w| format!("{:?}->{:?}", w.start_mm, w.end_mm))
        .collect::<Vec<_>>();
    x.sort();
    x
}
#[test]
fn renoding_cap_and_collection_order() {
    for n in [8, 10] {
        let d = legacy_chain(n);
        let l = d.levels[0].id;
        let new = wall(l, (0.0, 0.0), (0.0, 1000.0));
        let mut outputs = Vec::new();
        for reversed in [false, true] {
            let mut input = d.clone();
            if reversed {
                input.walls.reverse();
            }
            let e = add_wall_operations(&input, l, new.clone());
            if n == 8 {
                let e = e.unwrap();
                let mut s = session(input);
                s.commit(e.operations).unwrap();
                assert!(document_contact_blocks(s.document()).is_empty());
                outputs.push(geometry(s.document()));
            } else {
                assert!(
                    matches!(e,Err(domowik_edit::EditError::WallContact(ref b)) if b.iter().all(|x|!x.is_axis_refusal()))
                );
            }
        }
        if n == 8 {
            assert_eq!(outputs[0], outputs[1]);
        }
    }
}
#[test]
fn touching_the_host_does_not_certify_a_fictitious_extension() {
    let mut d = Document::with_ground_floor();
    let l = d.levels[0].id;
    let host = wall(l, (0.0, 0.0), (1000.0, 0.0));
    let opening = Opening::door(host.id, 0.5, 100.0, 2100.0, 120.0);
    let child = wall(l, (950.0, 0.0), (4000.0, 0.0));
    let host_id = host.id;
    let opening_id = opening.id;
    let child_id = child.id;
    d.walls.push(host);
    d.openings.push(opening);
    let ops = vec![
        field(host_id, "endMm", serde_json::json!([950.0, 0.0])),
        Operation::create(Entity::Wall(child)),
        field(opening_id, "hostWallId", serde_json::json!(child_id)),
        field(opening_id, "u", serde_json::json!(1050.0 / 3050.0)),
    ];
    let mut s = session(d);
    let r = s.commit(ops);
    println!(
        "shortened host and fictitious extension={r:?}; host end={:?}",
        s.document().walls[0].end_mm
    );
    assert!(r.is_err());
}
