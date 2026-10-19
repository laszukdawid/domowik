//! Issue 0038: what a single commit ALLOCATES, and what that proves it copies.
//!
//! # Why an allocation test rather than a timing one
//!
//! Timings are host-dependent and are not a pass/fail gate (ED-0070), so the
//! criterion benchmark can expose a regression to a reader but cannot fail a
//! build for one. Allocation is deterministic, and the residual this issue
//! records is a COPY: `replay::apply_batch` cloned the whole document and the
//! whole interval map per batch, and `Session::candidate_for` cloned the whole
//! projection on top of that.
//!
//! # The discriminator
//!
//! Two sessions hold the SAME NUMBER of entities and differ only in how much
//! heap each entity carries, in a `unknown` payload the commit never reads.
//! Work that merely SCANS the document — the duplicate-id set in
//! `Document::validate`, the linear scan in `insert_entity` — costs the same
//! in both, because it touches ids and numbers. Work that COPIES the document
//! costs the bulky session one payload per entity.
//!
//! So the ratio between the two isolates copying from scanning without a
//! host-dependent constant, and the absolute bound states the same thing
//! directly: one commit must allocate less than one copy of the payloads it
//! did not touch.

use domowik_document::operation::Operation;
use domowik_document::session::Session;
use domowik_format::{Entity, Level, Point, Wall};
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use uuid::Uuid;

/// Bytes of `unknown` payload each bulky wall carries.
const PAYLOAD_BYTES: usize = 4096;

/// Entities in the payload-weight fixtures. Large enough that a per-entity
/// copy dwarfs the fixed cost of a commit, small enough to build twice in a
/// debug test.
const ENTITIES: usize = 400;

thread_local! {
    static BYTES: Cell<usize> = const { Cell::new(0) };
    static ARMED: Cell<bool> = const { Cell::new(false) };
}

/// A pass-through allocator that totals request sizes while armed.
///
/// Thread-local and `Cell`-based on purpose: the counters carry no destructor,
/// so reading them from inside the allocator cannot itself allocate. `realloc`
/// and `alloc_zeroed` are deliberately NOT overridden, so their default
/// implementations route through `alloc` and a growing `Vec` is counted.
struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.try_with(Cell::get).unwrap_or(false) {
            BYTES.try_with(|b| b.set(b.get() + layout.size())).ok();
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Bytes requested on this thread while `f` ran.
fn allocated<T>(f: impl FnOnce() -> T) -> (usize, T) {
    BYTES.with(|b| b.set(0));
    ARMED.with(|a| a.set(true));
    let out = f();
    ARMED.with(|a| a.set(false));
    (BYTES.with(Cell::get), out)
}

/// Wall `i` on its own stretch of the x axis, carrying `payload` bytes of
/// uninterpreted data.
///
/// The payload lives in `unknown`, which FORMAT-SPEC §8 U2 requires a writer
/// to carry verbatim. Nothing in the commit path reads it, so it is exactly
/// the weight a copy pays for and a scan does not.
fn wall(level: Uuid, i: usize, payload: usize) -> Wall {
    let x = i as f64 * 10_000.0;
    let mut w = Wall::new(
        level,
        Point::new(x, 0.0),
        Point::new(x + 4000.0, 0.0),
        200.0,
        2500.0,
    );
    if payload > 0 {
        w.unknown
            .insert("xPayload".to_owned(), Value::String("p".repeat(payload)));
    }
    w
}

/// A session of `entities` walls, each carrying `payload` bytes.
fn session(entities: usize, payload: usize) -> (Session, Uuid) {
    let mut s = Session::new();
    let level = Level::new("Level 1");
    let level_id = level.id;
    s.commit(vec![Operation::create(Entity::Level(level))])
        .expect("level");
    for i in 0..entities - 1 {
        s.commit(vec![Operation::create(Entity::Wall(wall(
            level_id, i, payload,
        )))])
        .expect("wall");
    }
    (s, level_id)
}

/// Bytes one more commit allocates onto a session of `payload`-sized walls.
fn one_more_commit(entities: usize, payload: usize) -> usize {
    let (mut s, level) = session(entities, payload);
    assert_eq!(s.document().entity_count(), entities);
    let next = wall(level, entities + 1, 0);
    let (bytes, id) = allocated(|| s.commit(vec![Operation::create(Entity::Wall(next))]));
    id.expect("commit");
    bytes
}

#[test]
fn one_commit_does_not_allocate_a_copy_of_the_document_it_does_not_touch() {
    let lean = one_more_commit(ENTITIES, 0);
    let bulky = one_more_commit(ENTITIES, PAYLOAD_BYTES);
    // What the document's uninterpreted payloads weigh in total. A commit that
    // copies the document once pays at least this.
    let one_copy = (ENTITIES - 1) * PAYLOAD_BYTES;
    // Reproduces the evidence table with `--nocapture`.
    println!("lean {lean} B, bulky {bulky} B, one payload copy {one_copy} B");

    assert!(
        bulky < one_copy,
        "one commit allocated {bulky} bytes, which is at least one copy of the \
         {one_copy} bytes of payload it never read (lean fixture: {lean})"
    );
    assert!(
        bulky <= lean * 2,
        "commit allocation tracks entity SIZE, not entity count: {bulky} bytes \
         with {PAYLOAD_BYTES}-byte payloads against {lean} without, over the \
         same {ENTITIES} entities"
    );
}

/// What a commit still allocates in proportion to the document, now that it
/// copies none of it.
///
/// Issue 0038's second acceptance criterion allows per-commit cost to depend on
/// history as long as the dependence is MEASURED and stated rather than
/// assumed away. This is the measurement. What remains is set membership and
/// scanning over ids and numbers — the duplicate-id set `Document::validate`
/// builds, and the linear position scans in `insert_entity` and `entity` —
/// none of which touch an entity's payload.
///
/// The bound is a regression alarm with generous headroom, not a target. It is
/// an order of magnitude below `size_of::<Wall>()`, so no copy of the
/// collection can hide underneath it.
#[test]
fn what_a_commit_allocates_per_entity_is_id_sized_not_entity_sized() {
    const SMALL: usize = 200;
    const LARGE: usize = 800;
    const PER_ENTITY_CEILING: usize = 64;

    let small = one_more_commit(SMALL, 0);
    let large = one_more_commit(LARGE, 0);
    println!("commit allocation: {SMALL} entities {small} B, {LARGE} entities {large} B");

    let slope = large.saturating_sub(small) / (LARGE - SMALL);
    assert!(
        slope <= PER_ENTITY_CEILING,
        "one commit allocates {slope} bytes per document entity ({small} B at \
         {SMALL} entities, {large} B at {LARGE}); above {PER_ENTITY_CEILING} \
         something is copying the collection, not scanning it"
    );
}
