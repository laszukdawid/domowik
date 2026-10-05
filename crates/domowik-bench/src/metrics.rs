//! Timing percentiles and allocation accounting.
//!
//! # Why percentiles and not a mean
//!
//! Issue 0032 asks for p50/p95/p99 because an editor is judged on its worst
//! frames, not its average one. A mean hides exactly the tail that a user
//! feels, and a mean over a distribution with allocator or cache outliers is
//! not even a good summary of the typical case.
//!
//! # The method, stated so a number can be reproduced
//!
//! Nearest-rank: samples are sorted ascending and the p-th percentile is the
//! sample at index `ceil(p/100 * n) - 1`. No interpolation, no smoothing, no
//! outlier rejection. Every sample taken is in the summary — a harness that
//! discarded its slowest measurements would report a machine that does not
//! exist.
//!
//! Each sample times ONE gesture against a freshly prepared state. Preparation
//! is outside the timed region; if it were inside, the number would be
//! dominated by cloning the document.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::time::Instant;

/// Sorted wall-clock samples in nanoseconds.
#[derive(Debug, Clone, Default)]
pub struct Samples {
    sorted: Vec<u64>,
}

impl Samples {
    /// Sorts and takes ownership of raw nanosecond samples.
    pub fn new(mut raw: Vec<u64>) -> Self {
        raw.sort_unstable();
        Self { sorted: raw }
    }

    /// How many samples were taken.
    pub fn len(&self) -> usize {
        self.sorted.len()
    }

    /// True when nothing was measured.
    pub fn is_empty(&self) -> bool {
        self.sorted.is_empty()
    }

    /// The nearest-rank percentile in nanoseconds, or 0 when empty.
    ///
    /// `p` is in `0..=100`.
    pub fn percentile(&self, p: f64) -> u64 {
        if self.sorted.is_empty() {
            return 0;
        }
        let n = self.sorted.len() as f64;
        let rank = (p / 100.0 * n).ceil().max(1.0) as usize;
        self.sorted[rank.min(self.sorted.len()) - 1]
    }

    /// Median.
    pub fn p50(&self) -> u64 {
        self.percentile(50.0)
    }

    /// 95th percentile.
    pub fn p95(&self) -> u64 {
        self.percentile(95.0)
    }

    /// 99th percentile.
    pub fn p99(&self) -> u64 {
        self.percentile(99.0)
    }

    /// Fastest sample.
    pub fn min(&self) -> u64 {
        self.sorted.first().copied().unwrap_or(0)
    }

    /// Slowest sample. Reported because a single stall is a real user
    /// experience even when it never reaches p99.
    pub fn max(&self) -> u64 {
        self.sorted.last().copied().unwrap_or(0)
    }
}

/// Times `body` `iterations` times, preparing fresh state before each one.
///
/// `warmup` runs are executed and discarded first, so the first sample does
/// not carry page faults and cold caches that no later one pays.
pub fn measure<T, R>(
    iterations: usize,
    warmup: usize,
    mut setup: impl FnMut() -> T,
    mut body: impl FnMut(T) -> R,
) -> Samples {
    for _ in 0..warmup {
        let state = setup();
        std::hint::black_box(body(state));
    }
    let mut raw = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let state = setup();
        let started = Instant::now();
        let out = body(state);
        let elapsed = started.elapsed();
        std::hint::black_box(out);
        raw.push(elapsed.as_nanos() as u64);
    }
    Samples::new(raw)
}

static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicI64 = AtomicI64::new(0);
static PEAK_BYTES: AtomicI64 = AtomicI64::new(0);

/// Wraps an allocator and counts what passes through it.
///
/// Declare it as the binary's `#[global_allocator]`. Counting is process-wide
/// and lock-free; the harness is single-threaded, so the counters describe the
/// work being measured and nothing else.
pub struct Counting<A>(pub A);

// SAFETY: every method forwards to the wrapped allocator with the same
// arguments and returns its result unchanged, so the allocator contract is
// whatever `A` already guarantees. The counters are atomics and touch no
// allocation state.
unsafe impl<A: GlobalAlloc> GlobalAlloc for Counting<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { self.0.alloc(layout) };
        if !p.is_null() {
            note_alloc(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { self.0.alloc_zeroed(layout) };
        if !p.is_null() {
            note_alloc(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size() as i64, Ordering::Relaxed);
        unsafe { self.0.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = unsafe { self.0.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            let delta = new_size as i64 - layout.size() as i64;
            let live = LIVE_BYTES.fetch_add(delta, Ordering::Relaxed) + delta;
            PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
        }
        p
    }
}

fn note_alloc(size: usize) {
    ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(size as i64, Ordering::Relaxed) + size as i64;
    PEAK_BYTES.fetch_max(live, Ordering::Relaxed);
}

/// Allocation counters at one instant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocStats {
    /// Allocations and reallocations since the process started.
    pub allocations: u64,
    /// Bytes currently live, as the allocator sees them.
    ///
    /// Signed: a buffer allocated before counting began and freed after it can
    /// drive the figure below zero, and clamping that to zero would silently
    /// misreport rather than showing an obviously impossible number.
    pub live_bytes: i64,
    /// Greatest `live_bytes` seen since the last [`arm`].
    pub peak_bytes: i64,
}

/// Reads the counters.
pub fn stats() -> AllocStats {
    AllocStats {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_bytes: PEAK_BYTES.load(Ordering::Relaxed),
    }
}

/// Resets the peak watermark to the current live figure.
///
/// Call immediately before the region of interest: the peak is then relative
/// to whatever was already resident, which is what "this gesture's memory
/// cost" means.
pub fn arm() {
    PEAK_BYTES.store(LIVE_BYTES.load(Ordering::Relaxed), Ordering::Relaxed);
}

/// What one execution of `body` cost the allocator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllocCost {
    /// Allocations and reallocations performed.
    pub allocations: u64,
    /// Peak bytes live ABOVE the level resident when the region began.
    pub peak_extra_bytes: i64,
    /// Bytes still live afterwards, above the starting level. Positive means
    /// the gesture retained memory, which for an editing gesture it should:
    /// the document grew.
    pub retained_bytes: i64,
}

/// Measures one execution of `body`, with `setup` excluded from the counters.
pub fn measure_allocations<T, R>(
    setup: impl FnOnce() -> T,
    body: impl FnOnce(T) -> R,
) -> AllocCost {
    let state = setup();
    arm();
    let before = stats();
    let out = body(state);
    let after = stats();
    std::hint::black_box(out);
    AllocCost {
        allocations: after.allocations - before.allocations,
        peak_extra_bytes: after.peak_bytes - before.live_bytes,
        retained_bytes: after.live_bytes - before.live_bytes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles_pick_real_samples() {
        let s = Samples::new((1..=100).collect());
        assert_eq!(s.p50(), 50);
        assert_eq!(s.p95(), 95);
        assert_eq!(s.p99(), 99);
        assert_eq!(s.min(), 1);
        assert_eq!(s.max(), 100);
    }

    #[test]
    fn a_single_sample_is_every_percentile() {
        let s = Samples::new(vec![42]);
        assert_eq!(s.p50(), 42);
        assert_eq!(s.p99(), 42);
    }

    #[test]
    fn an_empty_series_reports_zero_rather_than_panicking() {
        let s = Samples::default();
        assert!(s.is_empty());
        assert_eq!(s.p99(), 0);
        assert_eq!(s.max(), 0);
    }

    #[test]
    fn percentiles_never_index_past_the_end() {
        for n in 1..40usize {
            let s = Samples::new((0..n as u64).collect());
            assert_eq!(s.percentile(100.0), n as u64 - 1);
            assert_eq!(s.percentile(0.0), 0);
        }
    }

    #[test]
    fn measure_runs_the_body_once_per_sample() {
        let mut runs = 0usize;
        let s = measure(7, 3, || (), |()| runs += 1);
        assert_eq!(s.len(), 7);
        assert_eq!(runs, 10, "seven measured runs and three warmups");
    }
}
