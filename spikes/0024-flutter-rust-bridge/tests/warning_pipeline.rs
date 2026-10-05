//! Native warning-pipeline smoke and opt-in timing harness.
//!
//! This stays at the public C-ABI boundary: it measures/validates an accepted
//! wall mutation, warning JSON serialization, caller buffer release, history,
//! and a subsequent cached state read together. Fixture construction is always
//! outside a timed section.

use domowik_bridge::{
    domowik_buffer_free, domowik_debug_warnings_computations, domowik_plan_close,
    domowik_plan_create, domowik_plan_execute, domowik_plan_redo, domowik_plan_state,
    domowik_plan_undo, status, DomowikBuf,
};
use domowik_document::operation::Operation;
use domowik_format::{Entity, Level, Point, Wall};
use serde_json::{json, Value};
use std::ptr;
use std::time::{Duration, Instant};

struct Handle(u64);

impl Handle {
    fn new() -> Self {
        let mut handle = 0;
        assert_eq!(unsafe { domowik_plan_create(&mut handle) }, status::OK);
        Self(handle)
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        assert_eq!(domowik_plan_close(self.0), status::OK);
    }
}

struct Buffer(DomowikBuf);

impl Buffer {
    fn json(&self) -> Value {
        // Checked before the slice, not inferred from the JSON parsing that
        // follows. A successful ABI return does promise a non-empty buffer,
        // but `from_raw_parts` needs a non-null aligned pointer even for a
        // zero length, so the safety precondition is asserted where it is
        // relied on rather than left to a downstream `expect`.
        assert!(!self.0.data.is_null(), "a transferred buffer is non-null");
        assert!(self.0.len > 0, "a transferred JSON buffer is non-empty");
        let bytes = unsafe { std::slice::from_raw_parts(self.0.data, self.0.len as usize) };
        serde_json::from_slice(bytes).expect("ABI response is JSON")
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        assert_eq!(domowik_buffer_free(self.0.token), status::OK);
    }
}

fn out() -> DomowikBuf {
    DomowikBuf {
        data: ptr::null_mut(),
        len: 0,
        token: 0,
    }
}

fn execute(handle: &Handle, request: &[u8]) -> Buffer {
    let mut response = out();
    assert_eq!(
        unsafe {
            domowik_plan_execute(
                handle.0,
                request.as_ptr(),
                request.len() as u64,
                &mut response,
            )
        },
        status::OK
    );
    Buffer(response)
}

fn undo(handle: &Handle) -> Buffer {
    let mut response = out();
    assert_eq!(
        unsafe { domowik_plan_undo(handle.0, &mut response) },
        status::OK
    );
    Buffer(response)
}

fn redo(handle: &Handle) -> Buffer {
    let mut response = out();
    assert_eq!(
        unsafe { domowik_plan_redo(handle.0, &mut response) },
        status::OK
    );
    Buffer(response)
}

fn state(handle: &Handle) -> Buffer {
    let mut response = out();
    assert_eq!(
        unsafe { domowik_plan_state(handle.0, &mut response) },
        status::OK
    );
    Buffer(response)
}

struct Fixture {
    setup: Vec<u8>,
    mutation: Vec<u8>,
    warnings: usize,
}

/// `walls` is the total wall count, not a pair count. Every adjacent pair is
/// spatially isolated from every other pair, has disjoint axes, and overlaps
/// by positive area. At 2,000 walls the last pair ends at x=9,992,000mm, still
/// inside the stored-coordinate bound.
fn fixture(walls: usize) -> Fixture {
    assert!(walls > 0 && walls.is_multiple_of(2));
    let level = Level::new("Warning benchmark level");
    let level_id = level.id;
    let mut operations = vec![Operation::create(Entity::Level(level))];
    let mut first_wall = None;
    for i in 0..walls {
        let pair = i / 2;
        let x = pair as f64 * 10_000.0;
        let y = if i.is_multiple_of(2) { 0.0 } else { 50.0 };
        let wall = Wall::new(
            level_id,
            Point::new(x, y),
            Point::new(x + 2_000.0, y),
            200.0,
            2_500.0,
        );
        first_wall.get_or_insert(wall.id);
        operations.push(Operation::create(Entity::Wall(wall)));
    }
    let mut fields = serde_json::Map::new();
    // A height-only edit is accepted and leaves the warning geometry unchanged.
    fields.insert("heightMm".to_owned(), json!(2501.0));
    Fixture {
        setup: serde_json::to_vec(&json!({ "operations": operations })).unwrap(),
        mutation: serde_json::to_vec(&json!({
            "operations": [Operation::set_fields(first_wall.expect("one wall"), fields)]
        }))
        .unwrap(),
        warnings: walls / 2,
    }
}

fn assert_warnings(response: &Value, expected: usize) {
    let warnings = response["warnings"].as_array().expect("warnings array");
    assert_eq!(warnings.len(), expected);
    assert!(warnings
        .iter()
        .all(|warning| warning["code"] == "wall-area-overlap"));
}

#[test]
fn public_abi_mutation_history_and_state_surface_fixture_warnings() {
    let fixture = fixture(100);
    let handle = Handle::new();
    assert_warnings(&execute(&handle, &fixture.setup).json(), fixture.warnings);
    assert_eq!(domowik_debug_warnings_computations(handle.0), 1);

    let mutation = execute(&handle, &fixture.mutation).json();
    assert_warnings(&mutation, fixture.warnings);
    assert_eq!(domowik_debug_warnings_computations(handle.0), 2);
    let cached_state = state(&handle).json();
    assert_warnings(&cached_state, fixture.warnings);
    assert_eq!(mutation["warnings"], cached_state["warnings"]);
    assert_eq!(
        domowik_debug_warnings_computations(handle.0),
        2,
        "state must serialize the mutation's revision cache rather than recompute it"
    );

    assert_warnings(&undo(&handle).json(), fixture.warnings);
    assert_warnings(&redo(&handle).json(), fixture.warnings);
}

#[derive(Clone, Copy)]
enum Prepared {
    Mutation,
    Undo,
    Redo,
    CachedState,
}

/// Creates the initial document and any prerequisite accepted history outside
/// the timed section. The returned operation is always an accepted ABI call.
fn prepared(walls: usize, kind: Prepared) -> (Handle, Fixture) {
    let fixture = fixture(walls);
    let handle = Handle::new();
    drop(execute(&handle, &fixture.setup));
    match kind {
        Prepared::Mutation => {}
        Prepared::Undo | Prepared::Redo | Prepared::CachedState => {
            drop(execute(&handle, &fixture.mutation));
        }
    }
    if matches!(kind, Prepared::Redo) {
        drop(undo(&handle));
    }
    (handle, fixture)
}

/// Order statistics OF THE SAMPLE, not estimates of a population.
///
/// With `SAMPLES` at 31 the 95th-percentile index is 29, so it is a genuine
/// interior order statistic rather than the maximum — which is what it
/// degenerates to for a handful of runs, and why a five-sample "p95" would be
/// a maximum wearing a percentile's name. No distributional claim is made
/// about either figure, and the maximum is printed alongside so a single
/// outlier cannot hide behind the other two.
fn order_statistics(mut samples: Vec<Duration>) -> (Duration, Duration, Duration) {
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95 = samples[((samples.len() * 95).div_ceil(100)).saturating_sub(1)];
    let max = *samples.last().expect("at least one sample");
    (median, p95, max)
}

/// Samples per row. Chosen so the 95th-percentile index is interior; see
/// [`order_statistics`].
const SAMPLES: usize = 31;

/// Performs exactly the ABI call a row times, and returns its response so a
/// caller outside the timed interval can check it.
fn perform(handle: &Handle, fixture: &Fixture, kind: Prepared) -> Buffer {
    match kind {
        Prepared::Mutation => execute(handle, &fixture.mutation),
        Prepared::Undo => undo(handle),
        Prepared::Redo => redo(handle),
        Prepared::CachedState => state(handle),
    }
}

/// Host-dependent timing evidence. Run explicitly with
/// `cargo test --release -- --ignored warning_pipeline_timing --nocapture`.
/// Each row times ONE ABI operation; setup, preparation and handle closure are
/// deliberately outside the interval, while response serialization and buffer
/// release are inside it.
#[test]
#[ignore = "host-dependent timing"]
fn warning_pipeline_timing() {
    for walls in [100usize, 500, 2_000] {
        for (name, kind) in [
            ("mutation", Prepared::Mutation),
            ("undo", Prepared::Undo),
            ("redo", Prepared::Redo),
            ("cached-state", Prepared::CachedState),
        ] {
            // PREFLIGHT, outside every timed interval and at EVERY size. The
            // timed loop below discards its responses, so without this a
            // larger fixture that silently produced fewer warnings would be
            // timed doing less work and the row would look faster for the
            // wrong reason. Asserting once per row costs one extra fixture and
            // buys the guarantee that the thing measured is the thing meant.
            {
                let (handle, fixture) = prepared(walls, kind);
                assert_warnings(&perform(&handle, &fixture, kind).json(), fixture.warnings);
            }

            let mut samples = Vec::with_capacity(SAMPLES);
            for _ in 0..SAMPLES {
                let (handle, fixture) = prepared(walls, kind);
                let started = Instant::now();
                drop(perform(&handle, &fixture, kind));
                samples.push(started.elapsed());
                drop(handle);
            }
            let (median, p95, max) = order_statistics(samples);
            println!(
                "warning-pipeline/{name}/{walls}: \
                 median={median:?} p95={p95:?} max={max:?} n={SAMPLES}"
            );
        }
    }
}
