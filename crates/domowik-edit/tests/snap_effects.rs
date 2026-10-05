//! The command batches BEHAVIOR-SPEC section 6.7 states for snapped commits.
//!
//! Section 6 quantizes an input; section 3 decides what the resulting command
//! MEANS. These fixtures check the second half: given the coordinates the snap
//! produced, the batch that lands is the one the fixture writes down — one
//! batch, and therefore one undo entry.

use domowik_document::operation::{Operation, OperationKind};
use domowik_edit::add_wall_operations;
use domowik_format::{Document, Entity, Point, Wall};
use uuid::Uuid;

fn wall(level: Uuid, a: (f64, f64), b: (f64, f64)) -> Wall {
    Wall::new(
        level,
        Point::new(a.0, a.1),
        Point::new(b.0, b.1),
        200.0,
        2500.0,
    )
}

/// Created walls, created joins, and anything else, so a fixture states the
/// SHAPE of its batch without depending on ordering section 6 does not fix.
fn created(operations: &[Operation]) -> (usize, usize, usize) {
    let (mut walls, mut joins, mut other) = (0, 0, 0);
    for op in operations {
        match &op.kind {
            OperationKind::CreateEntity {
                entity: Entity::Wall(_),
            } => walls += 1,
            OperationKind::CreateEntity {
                entity: Entity::WallJoin(_),
            } => joins += 1,
            _ => other += 1,
        }
    }
    (walls, joins, other)
}

#[test]
fn s4a_a_collinear_continuation_commits_one_wall_and_one_join() {
    // S4 part (a): the snapped commit runs (3000,0) -> (3150,0), sharing ONLY
    // the endpoint with seg1, axes collinear, so a degenerate-patch
    // continuation auto-join is legal per F3.
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(wall(level, (0.0, 0.0), (3000.0, 0.0)));
    assert_eq!(doc.validate(), vec![]);

    let edit = add_wall_operations(&doc, level, wall(level, (3000.0, 0.0), (3150.0, 0.0)))
        .expect("a legal continuation");

    // One wall, one join, nothing else — in particular NO split, because the
    // contact is endpoint to endpoint rather than endpoint to span.
    assert_eq!(
        created(&edit.operations),
        (1, 1, 0),
        "{:#?}",
        edit.operations
    );
    assert_eq!(edit.joins, 1);
    assert_eq!(edit.splits, 0);
    assert_eq!(edit.cascaded_openings, 0);
}

#[test]
fn s4b_an_overlapping_third_segment_still_commits_one_batch() {
    // S4 part (b): seg3 runs (3150,0) -> (3600,1700) past an isolated Wv. The
    // occupied rectangles overlap with positive area, which R3.1.7 makes a
    // WARNING attached to the batch result rather than a rejection, so the
    // batch still commits as one wall plus the continuation join.
    let mut doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    doc.walls.push(wall(level, (0.0, 0.0), (3000.0, 0.0)));
    doc.walls.push(wall(level, (3000.0, 0.0), (3150.0, 0.0)));
    doc.walls
        .push(wall(level, (3600.0, -500.0), (3600.0, 1500.0)));
    assert_eq!(doc.validate(), vec![]);

    let edit = add_wall_operations(&doc, level, wall(level, (3150.0, 0.0), (3600.0, 1700.0)))
        .expect("a warning does not abort the batch");

    assert_eq!(
        created(&edit.operations),
        (1, 1, 0),
        "{:#?}",
        edit.operations
    );
    assert_eq!(edit.joins, 1);
    assert_eq!(edit.splits, 0);
}

#[test]
fn s6_an_unsnapped_commit_on_an_empty_scene_is_one_wall_and_no_join() {
    // S6: Alt disabled every class, so raw coordinates commit. An empty scene
    // has nothing to contact, so the batch is a single create.
    let doc = Document::with_ground_floor();
    let level = doc.levels[0].id;
    let edit = add_wall_operations(&doc, level, wall(level, (1237.0, 589.0), (2237.0, 1589.0)))
        .expect("an empty scene accepts any legal wall");

    assert_eq!(
        created(&edit.operations),
        (1, 0, 0),
        "{:#?}",
        edit.operations
    );
    assert_eq!(edit.joins, 0);
    assert_eq!(edit.splits, 0);
}
