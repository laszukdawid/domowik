# 0063: Make native close quiescence tests deterministic

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024

## Problem

The parallel native test run for issue 0059 failed
`close_waits_for_a_call_that_has_not_reached_the_core_yet`: the worker returned
status 2 instead of 0. The test sleeps for 2 ms before closing its handle, so
it cannot prove that the worker has acquired a lease before close begins.
A serial rerun passed, which does not establish the cause or fix the race.
The neighboring busy-call test uses a similar sleep. Both tests also sample
a flag written by the caller after the export returns; lease release inside
the export does not order that later caller instruction before close returns.

## Acceptance criteria

- [x] Determine whether the failure is in the implementation, test scheduling,
      or both, using the handle/lease contract and a controlled interleaving.
- [x] Tests prove close waits for an admitted call outside the session lock,
      without relying on fixed sleeps or caller-side post-return ordering.
      (Reopened by Root's completion audit after round 1 — see Round 2 below —
      then re-closed once the `sleep(50ms)` + `is_finished()` proof was
      replaced with a genuine wait-seam signal and demonstrated RED against a
      reintroduced defect.)
- [x] Synchronization hooks, if needed, are test-only and scoped to the tested
      handle so parallel tests cannot interfere.
- [x] Repeat parallel native tests and verify the full repository; retain
      failure and passing evidence without calling a serial retry a fix.
      (Parallel native tests repeated; see the evidence below. Full repository
      verification passed at commit `d0be026` on 2026-09-07.)

## Closure

The final empty-handle control also uses a bounded thread join so a regression
cannot hang its test thread indefinitely. After integration, the canonical
`./scripts/verify.sh` run completed with `=== VERIFICATION PASSED === all stages`
and no skipped stages. It included 72 native unit tests and 171 Flutter tests.
Log: `/private/tmp/domowik-0059-0063-final-verify.log` (local evidence).

## Progress

**Diagnosis.** Both failure modes were in the TESTS, not in `close`/`Gate`/
`Lease`. Two distinct bugs, confirmed by code reading and by a peer review
(domowik-review):

1. The 2 ms sleep before calling `close` was a guess at scheduling, not a
   guarantee the worker had reached `resolve()` and taken its lease. Under
   load it can lose that race outright — which is exactly what the reported
   run's `status 2` (`UNKNOWN_HANDLE`) shows: the worker's own guard,
   `assert_eq!(code, status::OK, "... must have been IN FLIGHT ...")`,
   caught it and failed loudly rather than passing vacuously. That guard was
   the single most useful line in the old test.
2. Separately, and not yet observed failing but real: both tests sampled an
   `AtomicBool` the WORKER set immediately after its export call returned,
   from the MAIN thread right when `close` returned. The lease is dropped
   INSIDE the export, before it returns to the worker's own code, so the
   flag-store is ordered after the lease drop only in the worker's own
   program order — nothing orders the MAIN thread's read of that flag
   relative to the store. `close` can correctly return (the lease really is
   gone) before the worker gets around to setting the flag. A green run was
   never evidence this could not happen; the assertion asserted a property
   `close` does not, and should not, promise.

Code reading (independently confirmed by review) also showed the actual
`Gate`/`Lease`/`wait_for_quiescence` mechanism is correct and, on the
"published output" side, stronger than the old tests demonstrated: every
handle-taking export binds its lease for the whole guarded closure, and
`release`/`fail` publish the response or last-error before that closure
returns — so output publication already happens-before the lease drop. The
gap was entirely in how the tests *observed* the contract, not the contract
itself.

**Fix.** Replaced both sleeps and both post-return flags with a `#[cfg(test)]`
`test_hooks` module in `src/lib.rs` (compiles to nothing outside `cfg(test)`;
no production behavior change, no new ABI export, no feature flag):

- A per-`(handle, Phase)` hold: `Phase::Admitted` fires in `resolve`/
  `resolve_epoch` right after a lease is taken, still outside the plan lock
  (window 1 of the original 0055/0059 finding). `Phase::PreLeaseDrop` fires
  at the top of `Lease::drop`, i.e. after the call's own work — including
  publishing its output — is done and nothing remains but releasing the
  lease (window 2). A held call blocks on a channel until the test releases
  it, with a bounded `recv_timeout` (30s) rather than an unbounded `recv()`,
  so a broken fix fails the test's own assertion instead of hanging
  `cargo test` with no per-test timeout to save it.
- A one-shot registry-removal watcher, so a test can observe the exact
  instant `domowik_plan_close` removes its registry entry, distinct from
  when quiescence completes.
- `arm()` returns a `ReleaseGuard` whose `Drop` both removes any
  still-unconsumed hold (so an armed-but-never-reached hold cannot leak for
  the life of the process) and releases an already-parked call — so a test
  that panics or fails an assertion mid-scenario cannot wedge a held call
  forever.
- The three tests now watch `close` directly (run it on its own thread, then
  assert `!closer.is_finished()` while a lease is deliberately held open) and
  join with a bounded `join_with_timeout` helper, rather than inferring
  anything from timing. `close_waits_for_a_call_that_has_not_reached_the_core_yet`
  and `close_waits_for_a_call_that_never_touches_the_session` are the
  original two, converted. `no_new_call_can_be_admitted_after_the_registry_entry_is_removed`
  is new: it holds a call at `PreLeaseDrop`, confirms the registry entry is
  removed while that lease is still open, and confirms a fresh call on the
  same handle is refused (`UNKNOWN_HANDLE`) during that window — a gap the
  peer review flagged that the old tests gave no coverage for at all.

**Documentation correction (Root-ratified).** `domowik_plan_close`'s doc, the
`Gate`/`Lease` doc above it, and the C header's `domowik_plan_close` comment
all overstated the guarantee: they read as "no thread is executing library
text" once `close` returns, which would license unloading the library. The
actual guarantee is narrower: every admitted call's lease-bracketed work, and
every effect it publishes THROUGH the library (session mutations, a written
`out` buffer with its token registered, a last-error update), is done and
visible. A call's own status CODE is delivered to its own caller only after
the lease drops (the closure returns, `guard` matches, the `extern "C"`
sequence hands the value back) — so a call whose only output is a status
code (`domowik_busy`, `domowik_cancel`) publishes nothing this promise
covers. `close` is not permission to `dlclose`/`FreeLibrary`: a caller doing
that must still join its own threads and free its own buffers first. Updated
consistently in `src/lib.rs` and `include/domowik_bridge.h`; no Dart/Flutter
doc referenced this claim, so nothing there needed a matching change.

**Verified.** `cargo fmt --check`, `cargo build --release --lib`, and
`cargo clippy --all-targets -- -D warnings` all exit 0
(`/private/tmp/domowik-0063-cargo-fmt-20260907-091917.log`,
`/private/tmp/domowik-0063-cargo-build-release-20260907-091917.log`,
`/private/tmp/domowik-0063-cargo-clippy-20260907-091917.log`). `cargo test
--lib` repeated 15 times back to back: 68/68 passed every run, 0 failures,
exit 0 each time
(`/private/tmp/domowik-0063-repeat-parallel-20260907-091917.log`).
`crates/domowik-geometry` (the marquee fix) was not touched.

**Deferred, still not in this fix.** Two concurrent `close` calls on the same
handle; a holder panicking while holding a lease. Also: because the hooks are
`#[cfg(test)]`, the C and Dart harnesses — built without `cfg(test)` —
exercise none of these interleavings; they remain sleep-based wherever they
touch `close` and should not be cited as evidence for the quiescence
contract. (The other three items originally deferred here — prompt-empty-close,
close-on-A-while-B-in-flight, and buffer-freeable-after-close — were
escalated from "deferred" to "required" by Root's audit; see Round 2.)

## Round 2 (Root completion audit)

Root's audit rejected the acceptance-criterion-2 checkmark as premature and
found three concrete gaps, all independently confirmed by a second peer
review pass. All three are now fixed; nothing here was optional.

1. **`sleep(50ms)` + `!closer.is_finished()` cannot catch a skipped-wait
   mutant.** `is_finished()` reads `false` both when `close` is genuinely
   parked in `wait_for_quiescence` AND when its thread simply has not been
   scheduled yet — so a `close` that skipped the wait entirely could still
   pass, if its thread just hadn't run by the time the sleep elapsed. Fixed
   by adding a THIRD test hook, `on_quiescence_wait`, fired from inside
   `Gate::wait_for_quiescence` itself, under the `active` mutex, immediately
   before the call that actually blocks (`idle.wait`). Receiving that signal
   is airtight rather than probable: once reached with `*active > 0`, the
   loop has no path out except a `Lease::drop` bringing the count to zero, so
   a mutant that skips the wait never reaches the seam at all and the
   signal's `recv_timeout` fires the failure. The hook is notify-only and
   never blocks — it fires while holding `Gate::active`, and `Lease::drop`
   needs that same mutex to decrement, so a blocking hook there would be an
   instant, timeout-proof deadlock. All three affected tests (the original
   two, plus the new `close_on_one_handle_does_not_stall_progress_on_another`
   below) now wait on this signal instead of a sleep; `!closer.is_finished()`
   is kept only as corroboration once the signal has already been received.

2. **No negative test proved the fixture could go red.** Fixed by adding
   `close_without_waiting` — a faithful, test-only reproduction of the
   pre-fix `close` (remove the registry entry, return at once, never wait;
   memory-safe because an in-flight call holds its own `Arc<Gate>`/
   `Arc<Mutex<Plan>>` clones) — plus a shared `probe_close` helper returning
   a `CloseVerdict` (`BlockedUntilReleased` / `ReturnedWhileLeaseHeld`) and
   two tests: `close_blocks_until_the_admitted_calls_lease_is_released`
   (asserts the real `close` yields `BlockedUntilReleased`) and
   `close_without_waiting_is_caught_returning_while_a_lease_is_still_held`
   (asserts the mutant yields `ReturnedWhileLeaseHeld`). Both verdicts are
   themselves determined by the `on_quiescence_wait` signal within a bounded
   bound, not by racing timing.

   Additionally demonstrated by hand, as Root asked: with
   `Gate::wait_for_quiescence`'s loop condition temporarily forced false
   (`while false && *active > 0`), the four tests that depend on the wait
   seam (`close_blocks_until_the_admitted_calls_lease_is_released`,
   `close_waits_for_a_call_that_has_not_reached_the_core_yet`,
   `close_waits_for_a_call_that_never_touches_the_session`,
   `close_on_one_handle_does_not_stall_progress_on_another`) go RED with a
   clear timeout message, not a hang or a silent pass
   (`/private/tmp/domowik-0063-red-proof-wait-bypassed-20260907-092711.log`).
   The bypass was then reverted and the full suite re-confirmed GREEN
   (72/72). This hand demonstration is not left in the codebase; the
   permanent, always-run regression coverage for the same property is the
   `close_without_waiting` pair above.

3. **The post-output test used `domowik_busy(h, 0)`, which has no output.**
   Parking a call with nothing to publish at `PreLeaseDrop` only
   re-demonstrated what `Admitted` already covers. Fixed:
   `no_new_call_can_be_admitted_after_the_registry_entry_is_removed` now
   holds `domowik_plan_snapshot` (a real buffered export) at `PreLeaseDrop`;
   the worker copies its response bytes and buffer token out (`DomowikBuf`
   is not `Send`) after the held call is released and the export returns.
   What proves the buffer
   was published before `close` could return is the SEAM POSITION —
   `PreLeaseDrop` fires at the top of `Lease::drop`, after `release()` in
   that call's own program order — combined with having observed admission
   before racing `close`, not a post-hoc read of the buffer. The test also
   now calls `domowik_buffer_free` on that token AFTER `close` returns and
   asserts `OK`, picking up the "buffer token still freeable after close"
   item that round 1 had left deferred; Root's audit made this required, not
   optional.

Also added, per Root's explicit ask (not optional deferred scope):

- `close_with_nothing_in_flight_returns_promptly` — without it, a `close`
  that blocked FOREVER, not just one that skips waiting, would still pass
  every other test here, since every other test releases its own held call
  before joining.
- `close_on_one_handle_does_not_stall_progress_on_another` — exercises TWO
  handles at once: holds A's lease, confirms `close` on A has genuinely
  reached its wait (via `on_quiescence_wait`), then confirms a call on B and
  `close` on B both succeed while A's close is still blocked — the first
  test to actually check `close`'s own doc claim that the registry lock is
  released before waiting so "a long-running commit on one plan cannot stall
  every other handle."

**Re-verified after round 2.** `cargo fmt --check`, `cargo build --release
--lib`, `cargo clippy --all-targets -- -D warnings` all exit 0
(`/private/tmp/domowik-0063-cargo-fmt-20260907-092711.log`,
`/private/tmp/domowik-0063-cargo-build-release-20260907-092711.log`,
`/private/tmp/domowik-0063-cargo-clippy-20260907-092711.log`). `cargo test
--lib` repeated 15 times back to back: 72/72 passed every run, 0 failures,
exit 0 each time
(`/private/tmp/domowik-0063-repeat-parallel-20260907-092711.log`). Sent to
domowik-review for re-review before this is treated as final, per Root's
instruction to receive Opus review before finalizing.

## Round 3 (peer re-review)

domowik-review re-verified round 2 against the source directly (lock
ordering between `HOLDS`, `QUIESCENCE_WAIT` and `Gate::active` — confirmed no
cycle) and found one MUST FIX the round-1 bypass run had silently missed,
plus one SHOULD FIX and one OPTIONAL improvement:

- **MUST FIX.** `no_new_call_can_be_admitted_after_the_registry_entry_is_removed`
  was the one test where "same treatment" was not applied: its
  `!closer.is_finished()` rested only on `watch_registry_removal`, which
  proves close STARTED, not that it reached the wait — a remove-then-return
  mutant fires that latch too, and the round-2 bypass run had filtered this
  test out of its command (`cargo test --lib close_`, which does not match
  a name starting with `no_new_call`), so nothing had actually exercised it
  under the bypass. Fixed by also arming `watch_quiescence_wait(h)` and
  receiving it, alongside `watch_registry_removal`, before the
  `!closer.is_finished()` check. Re-ran the bypass proof UNFILTERED this
  time (`cargo test --lib`, no name filter): all 5 tests that depend on the
  real wait now go RED together, including this one, with its own honest
  timeout message
  (`/private/tmp/domowik-0063-red-proof-wait-bypassed-unfiltered-20260907-093323.log`).
- **SHOULD FIX.** `probe_close`'s `Err(_)` branch (the "no signal within 1s"
  path) could misdiagnose a real `close` that was merely slow to reach the
  seam as a hang: it joined the closer before releasing the hold, and a
  genuinely-blocked (not defective) close cannot return until released, so
  `join_with_timeout` would burn 5 more seconds and panic with a "treat this
  as a deadlock" message that names the wrong cause. Fixed with one
  assertion first — `assert!(closer.is_finished(), "close neither reached
  its quiescence wait nor returned")` — so a late-but-correct close fails
  fast with an honest message instead.
- **OPTIONAL, taken.** `close_with_nothing_in_flight_returns_promptly` now
  arms `watch_quiescence_wait` and asserts `try_recv().is_err()` instead of
  a 500ms wall-clock bound — the actual property ("close never even
  reached its wait when nothing was outstanding"), with no timing
  dependence, and it incidentally confirms the seam is conditioned on
  `*active > 0` rather than firing unconditionally.

**Re-verified after round 3.** `cargo fmt --check`, `cargo build --release
--lib`, `cargo clippy --all-targets -- -D warnings` all exit 0
(`/private/tmp/domowik-0063-cargo-fmt-20260907-093323.log`,
`/private/tmp/domowik-0063-cargo-build-release-20260907-093323.log`,
`/private/tmp/domowik-0063-cargo-clippy-20260907-093323.log`). `cargo test
--lib` repeated 15 times back to back: 72/72 passed every run, 0 failures
(`/private/tmp/domowik-0063-repeat-parallel-20260907-093323.log`). Sent back
to domowik-review to confirm.

**Peer-confirmed.** domowik-review re-verified round 3 against the source
(both watchers armed pre-spawn in the publication test, so no lost-notification
window; `probe_close`'s fast-fail ordering; no wall-clock assertions left in
the group) and against the evidence (unfiltered bypass: 67 passed / 5 failed,
the 5 being exactly the wait-dependent tests, each with a named timeout or
`ReturnedWhileLeaseHeld`; the two that stay green under the bypass — the
nothing-in-flight control and the mutation test itself — confirmed correct
to stay green, not accidental passes; 15/15 repeat clean). Nothing
outstanding from that review. One point it asked to keep visible rather than
let fade, already noted above under "Deferred": with `cfg(test)` only, none
of this is exercised by the C or Dart harnesses — this is Rust-unit coverage,
and the harnesses' own `close` paths remain sleep-based and untested by any
of this work.

## Provenance

Original Domowik ABI tests and local integration output:
`/private/tmp/domowik-0059-final-cargo-test-20260907-063137.log`.
No third-party implementation or fixtures consulted.
