//! Benchmark corpus, operation mix and measurement harness for issue 0032.
//!
//! # Why this is a separate crate
//!
//! It is measurement tooling, not product code: nothing distributed depends on
//! it, and `publish = false` says so. Keeping it in the workspace rather than
//! in `spikes/` is deliberate — `scripts/verify.sh` builds and tests the
//! workspace on every run, so the harness cannot silently stop compiling while
//! the numbers it produced stay in the documentation looking current.
//!
//! # What it measures, and what it does not
//!
//! Measured: the cost of an editing gesture, split into the two halves an
//! editor actually pays — committing the batch (validation, log delivery,
//! projection) and rebuilding derived geometry (occupied region and room
//! faces). Reported as p50/p95/p99 wall-clock, peak live bytes and allocation
//! count.
//!
//! NOT measured here: rendering and frame budgets (issue 0033), container
//! read/write (issue 0029), and asset pipeline cost (issue 0026). Timings are
//! host-dependent and are never a pass/fail gate in `verify.sh` (ED-0070);
//! what the script gates on is that the harness still runs and that the
//! locality checks still hold.

pub mod corpus;
pub mod gestures;
pub mod locality;
pub mod metrics;
pub mod rng;

use domowik_document::session::Session;
use domowik_format::Document;
use uuid::Uuid;

/// Opens a session whose baseline is `doc` and whose log is empty.
///
/// This is the state an editor is in the instant after opening a saved plan:
/// the document is the snapshot, nothing has been committed, and the session
/// is not dirty. Building the corpus as a log of creations instead would time
/// the log rather than the gesture.
pub fn session_over(doc: Document) -> Session {
    Session::open_snapshot(
        Uuid::from_u128(0x0032_0000_0000_0000_0000_0000_0000_0001),
        doc,
        &std::collections::BTreeMap::new(),
        &[],
        Vec::new(),
    )
    .expect("an empty log over a snapshot always opens")
}
