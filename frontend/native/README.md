# Domowik frontend native bridge (ADR 0003)

Status: **evidence for ADR 0003; the gate is NOT PASSED.** macOS, Linux and
Android have dated end-to-end execution evidence, application included. iOS
is deferred. These slices are bounded by their tested ABI and environment;
Android's earlier emulator run is not current full-editor or 3D support.
Both platforms the gate names have run, but ADR 0003 remains **Proposed**:
its gate also requires an editor prototype meeting measured input and
frame-time goals, which is issue 0033's, and the Android run is on an emulator
rather than a device. See "Platforms".

For current usage, start with [Getting started](../../docs/GETTING-STARTED.md),
the [user guide](../../docs/USER-GUIDE.md) and
[compatibility](../../docs/COMPATIBILITY.md). This README retains boundary
contracts and dated experiment records rather than replacing those guides.

This is the native adapter for the actively developed frontend. Its parent
`frontend/` owns a separate Cargo workspace, excluded from the root workspace.
No release is implied. `scripts/verify.sh` builds and tests the adapter and
the actual application. Earlier issue-0024 measurements below are historical;
see [the frontend README](../README.md) for the relocation map.

## What exists

- `src/lib.rs` — a `cdylib` exposing the spike C ABI over the core's stateful
  `Session` seam. Stateful exports are panic-guarded.
- `include/domowik_bridge.h` — the C header, hand-written. The ownership rules
  live here, where a caller reads them.
- `charness/main.c` — a plain-C `dlopen` harness driving the real `.dylib`
  from 8 pthreads, including ownership abuse. Built with
  `-fsanitize=address,undefined`.
- `../dart_harness/lib/domowik_bridge.dart` — the `dart:ffi` declarations. ONE
  copy, imported by both the standalone harness and the Flutter app: two
  copies of an ABI declaration is the drift this spike exists to catch, and a
  `Uint64` that became a `Uint32` in one of them would be a silent stack
  corruption rather than a compile error.
- `../dart_harness/bin/harness.dart` — a `dart:ffi` harness using
  `DynamicLibrary.open`, the same call Flutter makes. No third-party Dart
  package is adopted. The harness also emits the measurements below.
- `../plugin/` — a Flutter FFI plugin whose macOS podspec builds the Rust
  crate as a **static** library and links it into the pod's framework, so
  Xcode embeds and signs it inside the application bundle.
- `../` — the Flutter application, with two headless probes
  that make the built bundle verify itself.

## ABI summary (internal, not a stable product contract)

```c
int32_t  domowik_abi_version(void);                      /* == 15 */
int32_t  domowik_plan_create(uint64_t* out_handle);
int32_t  domowik_plan_create_default(uint64_t* out_handle); /* clean Level 1 */
int32_t  domowik_plan_execute(uint64_t, const uint8_t* req, uint64_t,
                               domowik_buf* out);         /* JSON protocol */
int32_t  domowik_plan_assets(uint64_t, const uint8_t* req, uint64_t,
                              domowik_buf* out);         /* plan-local models */
int32_t  domowik_plan_undo(uint64_t, domowik_buf* out);
int32_t  domowik_plan_redo(uint64_t, domowik_buf* out);
int32_t  domowik_plan_state(uint64_t, domowik_buf* out);
int32_t  domowik_plan_snapshot(uint64_t, domowik_buf* out);
int32_t  domowik_plan_hit_test(uint64_t, const uint8_t* req, uint64_t,
                               domowik_buf* out);      /* ranked JSON hits */
int32_t  domowik_plan_marquee(uint64_t, const uint8_t* req, uint64_t,
                              domowik_buf* out);       /* ordered JSON IDs */
int32_t  domowik_plan_snap(uint64_t, const uint8_t* req, uint64_t,
                           domowik_buf* out);          /* ranked candidates */
int32_t  domowik_plan_edit(uint64_t, const uint8_t* req, uint64_t,
                           domowik_buf* out);           /* gesture -> batch */
int32_t  domowik_plan_derived(uint64_t, const char* level_id,
                              domowik_buf* out);        /* rooms, apertures */
int32_t  domowik_plan_open(const char* path, uint64_t* out_handle);
int32_t  domowik_plan_save(uint64_t, const char* path);
int32_t  domowik_last_error(uint64_t, domowik_buf* out);
int32_t  domowik_buffer_free(uint64_t token);            /* token-keyed */
int32_t  domowik_plan_close(uint64_t);
int32_t  domowik_debug_panic(void);                      /* containment probe */
int32_t  domowik_busy(uint64_t, uint64_t millis);        /* cancellation probe */
int32_t  domowik_cancel(uint64_t);
int64_t  domowik_outstanding_buffers(void);
int64_t  domowik_open_plans(void);
int64_t  domowik_debug_cancel_epoch(uint64_t);           /* invariant probe */
int32_t  domowik_debug_panic_holding_lock(uint64_t);     /* poison probe */
int64_t  domowik_debug_hit_index_builds(uint64_t);       /* locality probe */
int64_t  domowik_debug_snap_index_builds(uint64_t);      /* locality probe */
int64_t  domowik_debug_warnings_computations(uint64_t);  /* locality probe */
```

All 29 exports are listed. `tests/abi_documentation.rs` asserts that, because
this block silently omitted seven of them until 2026-09-12, including the four
the editor round added.

Boundary rules as implemented and tested:

- **Live 3D view (issue 0078):** the header view menu selects **2D plan**,
  **3D view**, or **2D + 3D split view**. Linux's registered GPU texture displays
  walls/openings, room slabs and cached imported meshes. Drag to orbit,
  right/Shift-drag to pan, scroll to zoom; Walk through adds mouse look, WASD,
  Q/E height and Shift speed. Fit resets the aerial camera. Camera operations
  never edit the plan, decode content, rebuild scenes or upload geometry.
  GPU mesh buffers are shared between repeated instances; 5,000-triangle fast
  LOD is a rendering approximation. The backend is experimental under ADR 0010,
  with Linux evidence and open cross-platform/hardware gates. See
  [the scene and performance evidence](../../docs/evidence/0078-live-3d-view.md).

- **Plan-local assets (issue 0074, ABI 15):** **Assets** opens a searchable
  directory library with GLB/OBJ/ZIP discovery in subdirectories. Placement
  embeds the chosen model lazily and draws an explicitly sized, named 2D
  footprint. Move, size/position/rotation edits and deletion are undoable.
  Library directories persist only with their plan; embedded models remain
  available after source removal. See [usage and evidence](../../docs/evidence/0074-plan-local-assets.md).
  **Add directory** opens a native folder chooser on Linux/macOS; **Enter path**
  and attached-path edit/browse controls support manual additions and validated
  replacement (issue 0075). Folder-provider URI support on Android is separate
  from this filesystem workflow.
  Issue 0076 now draws cached top-down mesh previews (OBJ/ZIP-OBJ and GLB,
  including Draco) with diffuse colors; the rectangle remains only a selection
  bound or diagnosed fallback. Issue 0077 remembers reusable library locations
  in user-level config across restarts and plans. See
  [preview evidence](../../docs/evidence/0076-model-previews.md) and
  [config paths](../../docs/evidence/0077-user-configuration.md).

- **Targeting (issues 0045/0047, ABI 9):** Press classification, selection
  and opening-host targeting consume the same native ordered candidates,
  using 12 logical pixels for mouse/stylus and 24 for touch, divided by the
  current zoom. Wall regions use their own butt-capped rectangles, with
  canonical geometry before UUID in tied rankings. Joined corners keep their
  join identity through selection, co-moving member endpoints and undo;
  Delete never silently substitutes a member wall. Opening placement filters
  the native order to wall regions. One active-level index is cached while
  level/document generation matches, including across reads and saves.
  Endpoint/join admission and ordering are exact over stored coordinates;
  region projections remain binary64 under issue 0012. Full snapping remains
  issue 0058.
  Native query benchmarks exclude FFI and Flutter costs;
  see [measurements](../../docs/evidence/0047-hit-ordering/MEASURED.md).
- **Snap query (issue 0058, ABI 12):** `domowik_plan_snap` returns ranked
  candidates with native feature keys and guide segments. The shared binding
  exposes this read-only API; editor gesture wiring is still pending.
  Unsupported wall-edge ownership exclusions return an error, not an ordinary
  snap miss. Projected geometry retains the core's documented numerical gaps.
- **Selection (issues 0049/0050):** Plain clicks cycle unique entity IDs in
  native feature order; Ctrl/Meta/Shift-click toggles the first entity.
  Accepted document revisions and actual pointer/modifier/view/level/visibility
  transitions reset cycles immediately, including round trips. Selection and
  its optional primary remain local view state, never document history.
- **Marquee (issue 0059, ABI 11):** Hold Ctrl, Meta or Shift and drag from
  empty canvas to add intersecting active-level walls to the selection.
  The rectangle previews the drag; one native query on release supplies the
  ordered IDs. The last newly added entity becomes primary. A query that
  adds nothing preserves both selection and primary. Escape cancels the drag;
  plain empty-space dragging still draws a wall. A drag with coincident plan
  corners takes point-hit semantics; a LINE drag stays a rectangle and selects
  everything it crosses (R7.3.2a). Level visibility flags never change
  eligibility, and a level switch clears the result. The bridge delegates
  geometry and ordering to the core and does not change document history.
  Placeholder selection and the core's numerical gaps remain open under
  issues 0012/0026; this is partial R7 conformance.
- **Wall measurements (issue 0071, ABI 14):** the wall inspector shows
  centreline length, thickness and height and submits changed values as ONE
  atomic `setWall` batch. `lengthMm` is resolved bridge-side about the stored
  Start — End moves along the existing direction with R3.4.2 join co-movement
  — so the view derives no geometry. The ABI moves because an ABI 13 library
  would ignore the unknown field and apply only the thickness/height half of
  the same form.
- **Levels (issue 0051):** Active levels render normally above visible and
  ghosted passive levels; hidden levels are omitted. Only the active level
  participates in targeting and edits. Reopen activates the lowest level and
  ghosts the others, resetting local preferences. Add Storey activates only
  after accepted creation; redo restoration does not reactivate the level.
  Derived payloads are reused while each level's authoritative inputs match;
  hidden/deleted payloads are evicted. No new ABI or persisted fields.
- **Level lifecycle (issue 0053):** GUI and CLI new plans use a clean Level 1
  baseline at elevation zero. Delete active level and CLI `remove-level` share
  core last/nonempty/referenced-level guards, also enforced during compensation
  and external admission. Empty internal snapshots/builders remain supported;
  materialized empty plans fail user validation and editing-session save.
  The GUI refuses to adopt such a legacy file; CLI inspection and explicit
  `add-level` repair remain available. Existing names/history are not migrated.
- **Level properties (issue 0054):** the properties form submits one core-owned
  `setLevel` command for name, elevation, floor thickness and wall/ceiling
  height defaults. Warning-only plans remain valid and saveable; `plan_state`
  adds a revision-cached `warnings` array with stable codes, level IDs and
  readable messages. This initially preserved ABI 9; the wall-warning change
  below supersedes that response contract. An unchanged properties submission returns an empty buffer
  without growing history. `addWall` may omit `heightMm` to resolve the target
  level default once; explicit heights still win, and existing walls and
  openings are never rewritten by a changed default. These are interaction
  and additive report changes, not a persisted format migration.
- **Wall warnings (issue 0055, ABI 10):** accepted edits and history responses
  include final-state warnings, including same-level occupied wall rectangles
  that overlap while their axes remain disjoint. Mutation responses and state
  reads share one warning computation per accepted revision. `plan_edit` now
  returns a JSON summary with `batch_id` and `warnings`, replacing ABI 9's raw
  batch-ID bytes; unchanged properties submissions still return an empty buffer.
  Execute/history summaries gain an additive `warnings` field. Callers must
  match the current ABI (15) before using the library. Buffer ownership and
  persisted files are unchanged. See [the partial implementation evidence](../../docs/evidence/0055-wall-contact/AREA-WARNINGS.md)
  for measurements and outstanding contact-conformance work.
- **History (issue 0042, ABI 8):** Undo/Redo use the live core session before
  and after save/open. Their JSON summaries and the read-only `plan_state`
  expose `entities`, `undo_depth`, `redo_depth`, `dirty`, and `batch_id`
  (empty for state/no-op history). Save clears dirty only on success and
  preserves both stacks; new accepted edits invalidate redo; open starts with
  empty local stacks. The default editor level is a non-undoable baseline.
  The editor enables history controls from these counts, with Ctrl/Cmd+Z and
  Ctrl/Cmd+Shift+Z while the canvas has keyboard focus. These additions do not
  change the document format or the buffer ownership protocol. The header is
  the complete export list; the summary above is illustrative.
- **Input (issues 0043–0044):** Click jitter up to five logical pixels does not
  move geometry. Release uses the current cursor/camera; view navigation
  reprojects previews; cancellation restores selection. Trackpad pan/zoom and
  two-touch navigation preserve document history. A second finger cancels the
  current edit before navigation; Escape/cancel restores the camera. The
  automated tests inject events, not physical trackpad/touchscreen signals.

- **No Rust reference escapes.** Plans are opaque `u64` tokens resolved in a
  registry. `Box::into_raw` would be faster and would make a forged or stale
  handle undefined behaviour — across a boundary where the caller is Dart and
  any integer can arrive, that is not a trade worth taking. Forged handles are
  *detected* (tested with `0`, `0xdeadbeef`, `u64::MAX`, and a closed handle).
- **Handles are never reissued**, so a stale handle can never address someone
  else's document.
- **Ownership:** request buffers are borrowed read-only for the call; response
  buffers transfer to the caller and are released only by
  `domowik_buffer_free`, which takes the buffer's monotonic **token**, not its
  pointer. Tokens are never reused, so an unknown or already-reclaimed one is
  refused exactly with `UNKNOWN_BUFFER`, and the caller has no pointer to get
  wrong. Double free and forged tokens are tested from Rust, C and Dart.
- **Panic containment:** every export routes through `catch_unwind`; a panic
  becomes `DOMOWIK_PANIC` and the library stays usable. Proven through C and
  through Dart, both times unwinding into a foreign frame.
- **Errors:** a sticky per-handle last-error string, valid from a failing
  return until the next call on that handle; retrieval does not mutate it
  (byte-equality tested in C, equality tested in Dart).
- **Concurrency:** calls that touch a plan's SESSION serialise on that plan's
  lock, because the core's `Session` is not safe for unsynchronised concurrent
  use; distinct handles proceed in parallel. Tested with 16 Rust threads and 8
  pthreads on one handle, and with independent handles. `domowik_cancel` and
  `domowik_busy` deliberately do **not** take that lock — a cancel that waits
  for the work it cancels is a wait — so "every call on one handle serialises"
  would be false and is not claimed.
- **Cancellation:** a per-handle monotonic EPOCH, bumped without taking the
  plan lock. Nothing ever clears it, so a recorded cancellation cannot be
  erased by another operation starting.
- **Teardown:** `domowik_plan_close` takes a *lease* count, not the plan lock.
  A lease is taken when a call resolves its handle and released when that call
  returns, so it brackets the whole export — including request parsing before
  the lock and response serialisation after it, and including `domowik_busy`,
  which never takes the lock at all. After close returns, no thread is inside
  the ABI on that plan's behalf.
- **Id exhaustion:** handle and token counters saturate rather than wrap. Once
  exhausted every request is refused with `EXHAUSTED`, which is a dead library
  rather than a silently unsound one.

## What the language change actually changed

Two Go-era obligations **disappeared**:

- **No runtime.** The Go spike's finding F3 — "no shutdown story exists for
  c-shared; the Go runtime lives until process exit" — has no analogue. There
  is nothing to initialise on a foreign thread and nothing to tear down.
- **No garbage collector**, so the no-Go-pointer rule, the largest single
  source of complexity in the Go bridge, does not exist. Response buffers are
  ordinary Rust allocations released by an ordinary Rust free.

Two got **harder**, and are most of what this crate is:

- **A panic must not cross `extern "C"`.** Rust aborts rather than unwinding
  into foreign frames. For a GUI that means one malformed request kills the
  user's editor with their work in it, so containment is a correctness
  requirement, not a nicety.
- **Ownership must be transported explicitly.** Rust's allocator is not libc's,
  so a buffer this library returns can only be freed by this library, and
  there is no runtime to outlive the mistake.

## Test evidence

Counts are from the last run, on macOS/arm64 and again on Linux/aarch64, and
the two agree. Earlier revisions of this section quoted 19, 68 and 43, which
were the numbers on 2026-09-01 and had not been touched since the spike grew
an editor.

1. `cargo test` — 81 tests, 1 ignored as host-dependent timing: stateful round
   trip, undo across calls, forged and stale handles, double free, foreign
   pointer, panic containment and usability afterwards, malformed request, a
   command the core refuses, cancellation from another thread, cancellation not
   leaking into the next operation, 16-thread contention on one handle, handle
   independence, the save/reopen and derived-geometry surface the editor
   added, and four checks that the ABI is documented where it claims to be.
2. `charness` under ASan+UBSan — 131 checks over `dlopen`, 8 pthreads, a
   cross-thread cancel, and every ownership-abuse case, ending with the
   bridge's own leak counters at zero.
3. `dart_harness` — 97 checks over `DynamicLibrary.open`, plus the
   measurements. Same ownership abuse, from a language with no borrow checker.
4. `flutter test` — 203 widget and state tests against the real library.
5. The bundle self-test — 48 checks, run from inside a built application on
   macOS, on Linux, and from inside a release APK on Android.

The first four run in `scripts/verify.sh`. The fifth runs there on macOS, and
through `docker/run-linux-app.sh` on Linux.

## Measurements (evidence-only; macOS 25.6.0, arm64, release profile)

Single host. The timing rows are the range across **three consecutive runs**
rather than one, because a single run cannot show whether a number is stable:
the spread below is the honest precision of these figures. They support **no
cross-platform or cross-language comparison** — see "On the Go numbers".

| What | Value (3 runs) |
|---|---|
| `.dylib` size | 905,360 B unstripped / 717,024 B stripped |
| Build, cold cache | 10.10 s |
| Build, warm | 3.11 s |
| Exported symbols | 13 |
| Call-overhead floor (nullary export, from Dart) | **17.2 – 18.3 ns** |
| Caller-side JSON encoding alone (Dart, no boundary) | 1.98 – 2.01 µs |
| One operation, short history | **10.05 – 10.67 µs** |
| One operation, averaged over a plan growing to 2,000 | 86.51 – 87.12 µs |
| 20 operations per call, same growth | **13.14 – 13.56 µs/op** |

Each number isolates one cost on purpose. The first draft reported a single
"round trip" that silently mixed the FFI hop, Dart's own JSON encoding, and the
core's per-commit cost growing as the measurement itself added 2,000 operations
to one plan. It was a real number that told you nothing about where the time
went.

**Reading them.** The boundary itself is ~18 ns and is not the cost of
anything. At short history a full round trip is ~10 µs, of which ~2 µs is
Dart's own JSON encoding and ~0.02 µs is the hop; the rest is `serde` and the
core. Batching is worth **~6.6x** on comparable footing (≈87 vs ≈13 µs/op over
the same 2,000-operation growth), which is the same conclusion the Go spike
reached for the same reason: it amortises per-batch core work, not the hop,
because the hop was never the cost.

The `single_op_to_2000` row is deliberately kept even though it is not a
boundary cost: it is the core's own per-commit growth (issue 0038) seen from
the far side, and dropping it would make the batching comparison look like an
FFI result when it is not.

### On the Go numbers

The Go spike measured a nullary export at ≈42 ns and a single-op round trip at
≈141 µs. Those are **not compared against** here. They were taken on
linux/amd64 through cgo; these are macOS/arm64 through `extern "C"`, and that
spike's own README says its numbers "support no comparative performance
claims". Two different machines and two different boundaries do not make a
benchmark.

## Findings

- **F1 (the sharpest edge, found by AddressSanitizer).** `domowik_buf` is
  length-delimited and **not** NUL-terminated. The first draft of the C harness
  called `strstr` straight on `buf.data`; ASan reported a heap-buffer-overflow
  on the first run. The header states the rule, and the first caller broke it
  within the hour. Dart has the identical trap in
  `Pointer<Utf8>.toDartString()`. **Any production version of this ABI should
  either NUL-terminate defensively or ship a caller-side helper**, because
  "documented" has now been demonstrated insufficient.
- **F2 (design, caught before it shipped).** The cancellation flag must live
  **outside** the per-handle mutex. The first draft put it inside the `Plan`
  struct, where a cancel could only be recorded after the operation it was
  cancelling released the lock — which is a wait, not a cancel. It reviews as
  correct and is inert. `a_cancel_stops_a_running_operation_without_waiting_for_it`
  bounds the elapsed time so the regression fails loudly.
- **F3 (panic hook).** A contained panic still prints Rust's default panic
  message to stderr. Harmless in a harness; in a shipped GUI it is console
  noise and a small information disclosure (paths, and whatever the panic
  message carries). A production bridge should install `std::panic::set_hook`.
  Not done here: a spike that hides its panics is a worse spike.
- **F4 (build configuration hazard).** `panic = "abort"` in any release profile
  silently defeats every `catch_unwind` in this file and turns each contained
  panic into a process abort. Both this crate and the root workspace set
  `unwind` explicitly with a comment saying why. It is a one-line, plausible,
  well-meaning size optimisation that would remove the boundary's most
  important safety property with no test failing — nothing here can detect it,
  because with `abort` the test process dies.
- **F5 (tooling gap on this host).** ASan **leak** detection is unavailable on
  macOS/arm64 ("detect_leaks is not supported on this platform"). Leak checking
  falls back to the bridge's own `outstanding_buffers`/`open_plans` counters,
  which only see allocations the bridge tracks. The Go spike had LeakSanitizer
  on Linux; this slice does not, and that is a real reduction in assurance.
- **F6 (the registry grows).** Plans are removed only by `domowik_plan_close`.
  A caller that leaks handles leaks documents. `open_plans()` exists so a
  harness or a GUI can assert on it; the ABI has no other backstop, and adding
  one would mean guessing when a caller is finished.

### Defects found by attacking this code before review

Three, all in the first draft, all now fixed with regression tests. They are
listed because a spike whose value is "we learned what this boundary costs"
should say what it got wrong on the way.

- **Buffer release was keyed by POINTER, and two addresses are not two
  buffers.** Rust gives every zero-length allocation the same dangling
  well-aligned address — `0x1` for `u8` — so two outstanding EMPTY buffers
  (two `domowik_last_error` calls on handles with no error) collided in the
  map. Demonstrated against the real library: the second insert overwrote the
  first, `domowik_outstanding_buffers` reported **1 for 2**, and freeing the
  second returned `UNKNOWN_BUFFER` for a buffer the library had legitimately
  issued — so the leak check under-counted and a correct caller was told it
  was wrong. The same key has a second, worse hazard: a freed address may be
  handed out again, and a stale-pointer free would then find the recycled
  address and free a LIVE buffer. **That one was NOT reproduced** — 64
  same-size attempts on this host's allocator never reused the address — so it
  is recorded as a hazard the fix removes, not as a bug demonstrated.
  Release is now keyed by a monotonic token, which cannot collide or be
  recycled, and takes no pointer at all. The ABI revision moved to **2**.
- **The leak counters could report a panic as a plausible count.**
  `domowik_outstanding_buffers` and `domowik_open_plans` returned counts
  through the status-code guard, so a contained panic came back as the number
  `6` — indistinguishable from "six outstanding buffers", and every harness
  would still have passed. They now use a separate guard returning `-1`.
- **`domowik_plan_close` returned while a call was still inside the core.** It
  removed the registry entry and returned at once; a caller that closed every
  plan and then unloaded the library would have been unloading under a running
  call. Reproduced: with a 4,000-operation commit in flight, close returned in
  2.5 µs and `execute` then reported `OK` for 4,000 entities committed into a
  plan nobody could reach. Close now drains — it releases the registry lock,
  waits on the plan lock, and only then drops — so a long commit on one plan
  cannot stall every other handle and a big `Session` is not deallocated under
  the global lock. What is still not promised, and the header now says so: a
  call already in flight completes and returns its normal status, and its
  effects go away with the plan.
- **The threading rule was overstated.** It read "calls on ONE handle
  serialise", which is false for `domowik_cancel` and `domowik_busy` by design
  — a cancel that waits for the work it cancels is a wait.

The drain test is worth a note of its own: the first version raced a big commit
against a 3 ms head start, and the commit finished first, so the assertion
passed on a build where the drain **did not exist**. It now holds the plan lock
directly and asserts close does not return. Mutation-proven. A timing race is
not a test.

### Defects found by adversarial review

Reviewed by Codex (gpt-5.6-sol, high effort) as a structured debate: the
proposition was "the documented boundary rules hold", I argued the affirmative,
and **the counter position won.** Three findings, all fixed with tests that
fail on the previous implementation.

- **High — `close` was not the quiescence barrier the header promised, and the
  promise was mine.** The previous fix strengthened the header to "after this
  returns no thread is executing on that plan's behalf" while the code only
  took the plan lock briefly. Three windows escaped it: a call resolves its
  handle and parses its request *before* taking the lock; a call drops the lock
  *before* serialising its response; and `domowik_busy` never takes the lock at
  all. Replaced with a per-plan lease that brackets the whole export.
- **Medium — a cancellation could be acknowledged and then erased.** Every long
  operation *cleared* the boolean flag on entry, so a `domowik_cancel` landing
  between another operation's lookup and its clear was overwritten and the
  caller got `OK` for a cancellation that never happened. Replaced with a
  monotonic epoch that nothing clears.
- **Low — "never reused" was not literally implemented.** `fetch_add` wraps, so
  after 2^64 issues a stale token would free a live buffer or address another
  document. 584 years at a billion a second, and irrelevant to any real run —
  but the header stated the guarantee absolutely. The counters now saturate.

**Four weaknesses in our own evidence**, which is the finding class that
matters most because it makes everything else look better tested than it was:

- The close test acquired the private mutex directly, so it could only ever
  detect the window that was already covered. It now drives real exports.
- The Dart harness *claimed* the C harness covered cross-thread cancellation.
  It did not — C ran `busy` on its main thread and nothing else. C now does it.
- The "a failed call writes no buffer" checks initialised the out-parameter to
  zero and asserted `data == NULL`, which an implementation that zeroed the
  whole struct would also pass. They now use a sentinel.
- `domowik_debug_panic` panics holding no lock, so "usable afterwards" proved
  containment and said nothing about the poisoned-lock recovery this ABI also
  claims. `domowik_debug_panic_holding_lock` now exercises it.

**One thing this review changed about how the fix was tested.** A stress test
was written first for the cancellation race, racing 200 rounds of the losing
interleaving. It passed on the broken implementation — the window is a few
instructions wide and cannot be forced from outside the ABI. A test that passes
on the bug is worse than no test, so it was deleted and the *invariant* is
asserted instead, through a debug export: the cancellation epoch only ever
moves forward. That version fails deterministically on the old shape.

## Platforms

- **macOS/arm64 — executed end to end** (cargo tests, C harness under
  ASan+UBSan, Dart FFI harness, and a Flutter application with two probes run
  from inside the built bundle).
- **Linux — executed end to end** (2026-09-12). Both slices are below. The
  entry that used to sit here said the source was platform-neutral and that
  `verify.sh` selects `.so` when `.dylib` is absent. Both halves were true.
  Neither had been run, and the entry said so, which is why it was a claim and
  is now not one.
- **Android — executed end to end** (2026-09-12), on an AVD. Delivered by a
  separate work stream; the full record is in
  [docs/evidence/0024-android/MEASURED.md](../../docs/evidence/0024-android/MEASURED.md)
  and the toolchain in `docs/third-party/toolchain-android.json`. A release APK
  on `domowik_0024_arm64` (AOSP `arm64-v8a` system image, Android 16 / API 36)
  loads the core from inside the APK and passes the same 48-check bundle
  self-test the other platforms run, plus the first-frame probe (4 walls drawn,
  57,788 non-background pixels rasterised, 0 outstanding buffers) and an
  Android-only lifecycle probe that holds `openPlans` at 1 across rotation,
  pause and resume and reaches `close=0 openPlans=0 buffers=0` at teardown in
  the same process.
- **iOS — deferred**, and it is not merely "the same but later": a `cdylib` is
  not loadable on iOS the way it is elsewhere, so that slice needs a
  `staticlib` and a framework. The macOS work builds that `staticlib` and does
  package it as a framework, so the shape is no longer unknown. Nothing has
  been built for, signed for, or run on iOS, and a shared shape is not a
  validated platform.

### Linux, slice 1: the ABI, on native arm64

`docker/run-linux-abi.sh` exits 0 and prints `=== LINUX ABI SLICE PASSED ===`.
It runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, the
76 Rust tests, a release build asserted to be an `ELF 64-bit LSB shared object,
ARM aarch64`, the C harness under ASan+UBSan (131 checks, 0 failures) and the
Dart FFI harness (97 checks, 0 failures). Panic containment across
`extern "C"` is exercised on Linux rather than inferred from the macOS run.

The base image is pinned by digest, not by tag, and the Dockerfile asserts the
full `rustc --version` string against the one `rust-toolchain.toml` pins, so
the image cannot drift under the evidence it produces.

**What the guest is, so the numbers are not quoted wrong.** A Docker Desktop
LinuxKit VM on an Apple Silicon host. The kernel, the libc, the ELF loader and
the linker are genuinely Linux's, so a functional result here is a Linux
result. The CPU is shared with macOS. Timings are therefore not bare-metal
Linux timings, and the ones the harness prints (nullary 24.2 ns against
macOS's 17.2 to 18.3 ns) are recorded as a sanity comparison, not as a
measurement.

**And the host was busy.** These runs happened while several other agents were
compiling in other worktrees on the same machine. The figures are therefore
**contended observations**: numbers that were seen, under load that was not
controlled and not measured. They are not upper bounds, because nothing here
establishes a bound, and the contention is not separable from the
virtualisation. This disclosure is not optional politeness: `docs/BENCHMARK-*`
and `docs/evidence/0047-hit-ordering/MEASURED.md` already report load
conditions alongside timings, so omitting it here would have been a departure
from how this repository reports numbers.

Nothing is withdrawn, because nothing was concluded from these figures. The
only claim made from them is that the Linux boundary cost is the same order of
magnitude as the macOS one, and a real measurement of either belongs to a run
on a quiet host. In particular no ratio taken from a contended run is treated
as though the contention cancelled: both sides of a ratio were observed under
load, and that they met the SAME load is an assumption, not something the run
measured.

### Linux, slice 2: the application

`docker/run-linux-app.sh` exits 0 and prints
`=== LINUX APPLICATION SLICE PASSED ===`. `flutter build linux --release`
builds the plugin's CMake, which builds the Rust `cdylib` and installs it into
the bundle. The probe then checks the packaging instead of trusting the build:

- the core is inside the bundle at `lib/libdomowik_bridge.so` and is an ELF
  shared object;
- the runner's `RUNPATH` is exactly `$ORIGIN/lib`, compared exactly rather
  than by substring, because a `RUNPATH` that merely contains `$ORIGIN/lib`
  alongside a build directory is the ELF version of the install-name defect;
- no bundled library contains a reference to the build tree, where the build
  path is derived at run time rather than written as a literal, and the
  detector is made to fire on a planted control every run, because "found
  nothing" is also what a broken detector reports;
- both probes run from inside the bundle: the self-test's 48 checks, with
  `dladdr` proving the loaded image is the bundle's own copy, and the
  first-frame probe;
- the bundle is copied to another directory and re-run, and reports loading
  the core from the new location.

**Why the `cdylib` ships here and could not on macOS.** A Mach-O dylib carries
an `LC_ID_DYLIB` install name, and cargo sets it to the absolute path of the
build directory, which is why the macOS slice links a `staticlib` into a
framework. ELF has no such field. The loader resolves a bare soname against
the `RUNPATH`, and Flutter's Linux runner already sets that to `$ORIGIN/lib`,
so the file cargo produces is shippable as it stands.

**Architecture, and it is not a detail.** Flutter publishes no stable Linux
arm64 archive: `releases_linux.json` lists `dart_sdk_arch: x64` and nothing
else. The application slice therefore runs linux/amd64, which on this host is
emulated. Everything it checks is genuine Linux behaviour and fails here
exactly as it would on a Linux machine. **No timing is taken from it**, and
none is recorded.

### Linux, slice 3: a clean machine

`docker/run-linux-app.sh` finishes by exporting the finished bundle and running
it on an image that has GTK, GLib, Mesa, Xvfb and nothing else. No cargo, no
rustc, no flutter, no dart, no clang, no cmake, no pkg-config, and the image
asserts their absence rather than relying on its package list. The bundle is
not rebuilt there; it is the same directory, carried over in a volume. Both
probes pass, and the self-test reports loading the core from the copy in the
unprivileged user's home directory.

**This found something, which is the argument for doing it.** The first
clean-machine run died with `Couldn't open libEGL.so.1` and aborted.
`libEGL.so.1` appears nowhere in `ldd` output for the runner or for anything in
`lib/`: the Flutter engine `dlopen`s it at run time, so reading the ELF headers
cannot reveal the dependency, and on any developer machine it is already
installed and the gap is invisible. The residual this closes, *no clean-machine
install was tested*, had been carried since the macOS application landed, and
moving a bundle around a machine that still has the whole toolchain on it was
never going to answer it.

The external runtime dependency set is therefore GTK 3, GLib, the X client
libraries, Pango, Cairo, HarfBuzz and friends, all of which `ldd` names, plus
EGL and GLES, which it does not. That set is ordinary for a GTK application;
what matters is that it is now known rather than assumed.

macOS has no equivalent run. One `.app` was executed from its build directory
and from `/tmp`, both on the machine that built it.

### What adversarial review found in the Linux evidence

Reviewed by Codex, `gpt-5.6-sol` at high effort, as a debate on "the Linux
evidence establishes that the application loads and drives the bundled core,
and the checks cannot pass on a broken bundle". The counter position won on
one point that mattered.

**The defect: a redirected cargo could bundle a stale library.**
`build_rust.sh` ran plain `cargo build` and then copied
`<spike>/target/release/libdomowik_bridge.so` unconditionally. With
`CARGO_TARGET_DIR` set in the environment, which several of this spike's own
scripts set, cargo writes the fresh library somewhere else, returns zero, and
the copy takes whatever stale file the default path still holds. Reproduced
before it was fixed: `ABI_VERSION` changed to 77 with the variable redirected,
cargo produced a 77 core, the bundle shipped 12, and the self-test passed.
That is the macOS stale-archive defect arriving by a different road, in code
written to guard against exactly it. The fix pins the target directory, unsets
`CARGO_BUILD_TARGET`, and asserts the artifact is not older than any source it
is built from, which covers the class rather than the one mechanism. The same
attack now reports 77.

**Three more, all real.** The build-path scan searched for the app directory
while cargo writes under the spike root, so an embedded `<spike>/target/...`
path never matched it. It scanned only `lib/` while its comment claimed the
whole bundle. And probe markers were substring matches, so a line reading
`NOT BUNDLE SELFTEST PASSED` satisfied the oracle; the clean-machine phase
additionally accepted either probe's marker for either probe.

**Where the review was wrong, and why that is still on me.** Its central claim
was that `LD_LIBRARY_PATH` can select an external core while both probes still
report success. It cannot: the `dladdr` provenance assertion catches it. But
the review could not see that, because the round was scoped to four files and
`selftest.dart` was not among them. That is an error in how the round was set
up, not in its reasoning. Rather than answer it in prose, the probe now
demonstrates it: every run plants a byte-identical decoy on `LD_LIBRARY_PATH`
and requires the self-test to refuse it. It does, naming
`/tmp/decoy-core/libdomowik_bridge.so` as the image it loaded.

### Android, and the six limits that come with it

Stated here rather than left to the evidence file, because a summary that
keeps only the green parts is how a caveat gets lost.

- **It is an EMULATOR, not a device.** The ARM code runs natively on an ARM
  host rather than under translation, but nothing physical was involved and no
  claim is made about any phone.
- **One ABI, `arm64-v8a`, deliberately.** `armeabi-v7a` and `x86_64` are
  neither built nor run. Adding them would buy artefacts nobody executed and a
  standing caveat of exactly the kind the macOS `x86_64` slice carried until
  it was finally run.
- **The APK is signed with the auto-generated debug key.** A development
  artefact, not a distributable one, on the same reasoning ED-0099 applied to
  macOS.
- **Activity destruction and recreation is not tested.** The manifest declares
  `configChanges` covering orientation and screen size, so rotation is handled
  by the running activity and never destroys it. Forcing destruction is owed
  work, not a closed question.
- **Reading probe output needs `adb root` on the emulator**, because from
  Android 11 the shell cannot read another application's external files
  directory and a release APK is not debuggable. That is a property of the
  harness, not of the app.
- **File open and save dialogs are not wired on Android**; the method channel
  is macOS-only. The core's own `plan_save` and `plan_open` ARE exercised from
  inside the APK against real paths and work, so this is a missing platform
  channel rather than a boundary question.

**The packaging is genuinely different from the other two platforms**, which
is why it needed its own evidence rather than an argument from analogy. With
`extractNativeLibs=false` the library is stored uncompressed in the APK and
mapped straight out of it, so there is no file on disk to point at: the loaded
image is reported as `<apk>!/lib/arm64-v8a/libdomowik_bridge.so`, and the
application's native library directory exists and is empty.

### Both Linux checks were shown capable of failing

Neither is a check that would survive the bug it guards, which is the failure
the adversarial review of this spike found twice before.

1. Stop the plugin CMake from declaring its bundled library. `lib/` comes out
   empty and the self-test fails at its first check, rather than passing on a
   bundle with no core in it.
2. Set `ABI_VERSION = 99` and rebuild **without** `flutter clean`. The bundle
   reports 99. This asks the macOS stale-archive defect directly: there, a
   script phase with a declared output was skipped and the framework linked
   the previous build's archive, and both probes passed on it. An
   `add_custom_command(OUTPUT ...)` here would reproduce that exactly, report
   12, and pass. `add_custom_target(... ALL)` has no up-to-date check, so it
   does not.
## The Flutter application (2026-09-04)

The gap recorded here until now was: *no Flutter application was built, so
bundle packaging is unvalidated.* It is now built, and this is what it showed.

### The packaging problem, and what fixes it

A cargo `cdylib` carries an `LC_ID_DYLIB` install name that is the **absolute
path of the machine that built it**:

```text
/Users/<someone>/.../target/release/deps/libdomowik_bridge.dylib
```

Copy that into a `.app` and you have an application that works for whoever
built it and is broken for everyone else. The fix is not `install_name_tool`
surgery after the fact: the crate also builds a `staticlib`, CocoaPods links it
into the plugin's framework with `-force_load`, and Xcode then embeds, signs
and gives it an `@rpath` id like any other framework. `verify.sh` asserts the
`@rpath` id directly, because it is the property that silently stops being
true.

`-force_load` is required, not decorative: nothing in the pod's own sources
references the Rust symbols, so without it the linker pulls no object files
out of the archive at all and the framework exports nothing. The cost is that
the whole archive is kept — see the size table.

### Evidence: the bundle verifies itself

"The author looked at a window" is not evidence that survives review, so the
built application checks itself and exits non-zero when anything fails. Both
probes run in `scripts/verify.sh`.

| Probe | Environment variable | What it establishes |
|---|---|---|
| Bundle self-test | `DOMOWIK_SPIKE_SELFTEST=1` | 23 checks with no window: the framework loads from inside the bundle, ABI is 3, a plan is built, a bad batch is refused *atomically with a reason*, undo removes exactly one wall, a Rust panic is contained and the plan still works, and every plan and buffer is reclaimed. |
| First-frame probe | `DOMOWIK_SPIKE_FIRSTFRAME=1` | Runs the real widgets and the real button handler, then reports the wall count **from inside `CustomPainter.paint`** — so it fails if the UI renders nothing while the FFI succeeds. |

Both pass from the build directory **and from a copy relocated to `/tmp`**,
which is the test that actually proves the bundle is self-contained rather
than quietly resolving something from the developer's tree.

### Signing and notarisation

- The bundle is **ad-hoc signed** (`Signature=adhoc`, `TeamIdentifier=not
  set`), which is what Flutter does without a Developer ID.
- After a **clean** build, `codesign --verify --deep --strict` passes.
- After an **incremental** build it FAILS with `nested code is modified or
  invalid`: Flutter rewrites `App.framework` without re-sealing the outer
  bundle. Both nested frameworks still verify individually — it is the outer
  seal that goes stale. This is Flutter's behaviour, not this spike's, and it
  is why the `verify.sh` stage builds clean before asserting the signature.
- **Notarisation was not attempted and is not validated.** `security
  find-identity -v -p codesigning` reports **0 valid identities** on this
  machine, so there is no Developer ID to sign with and notarisation has no
  input. `spctl -a -t exec` rejects the bundle, exactly as expected for an
  ad-hoc build. Nothing here was simulated or inferred.

### Measurements (same host as the table above)

| Quantity | Value |
|---|---|
| Application bundle | 43.5 MiB |
| Framework binary, universal | 4,960,736 B |
| — arm64 slice | 2,453,984 B |
| — x86_64 slice | 2,517,568 B |
| Standalone `cdylib`, arm64 | 907,168 B |
| Static archive, arm64 | 22,343,672 B |
| Clean build (`flutter clean` + build) | 25.3 s, 27.0 s |
| Incremental rebuild | 8.2 s |

The arm64 framework slice is **2.7x the size of the standalone `cdylib`**.
That is the price of `-force_load`: the linker keeps every object file in the
archive because it is told to, including code no export reaches.

### What the application does

It builds a plan through the bridge one wall at a time, draws what the core
returns, undoes, sends a batch the core must refuse, and panics inside Rust on
purpose. The drawing is deliberately crude — stroked centre lines at real
thickness — because the occupied-region geometry the core can compute is not
exposed across this ABI yet, and approximating it in the UI would be a second
geometry implementation living in the view.

## Reproduce

```sh
cd frontend
cargo test
cargo build --release

clang -std=c11 -Wall -Wextra -Werror -g -fsanitize=address,undefined \
  -o build/charness native/charness/main.c
./build/charness target/release/libdomowik_bridge.dylib

cd dart_harness
dart run bin/harness.dart ../target/release/libdomowik_bridge.dylib

cd ..
flutter build macos --release
APP=build/macos/Build/Products/Release/domowik_editor.app/Contents/MacOS/domowik_editor
DOMOWIK_SPIKE_SELFTEST=1   "$APP"
DOMOWIK_SPIKE_FIRSTFRAME=1 "$APP"
```

Or `./scripts/verify.sh` from the repository root, which runs all four.

The macOS x86_64 slice, which until 2026-09-12 was built by `lipo` and never
run. It executes under Rosetta 2 translation, which answers whether the
universal build is sound but is not Intel hardware, and yields no usable
timing:

```sh
cargo build --release --target x86_64-apple-darwin
clang -std=c11 -Wall -Wextra -Werror -g -arch x86_64 -o build/charness-x86 charness/main.c
arch -x86_64 build/charness-x86 target/x86_64-apple-darwin/release/libdomowik_bridge.dylib
```

The Dart harness has no x86_64 counterpart on this machine: an arm64 Flutter
install ships an arm64 Dart and nothing else.

Linux, from the same directory, on a machine with a reachable Docker daemon:

```sh
docker/run-linux-abi.sh   # native arm64: tests, C harness, Dart harness
docker/run-linux-app.sh   # amd64 (emulated here): the Flutter application
```

Both announce a skip and exit 0 when no daemon answers, rather than failing on
a machine that simply has no Docker. Neither is wired into `verify.sh`: the
first needs a 1.6 GB image, the second a 7 GB one, and making every developer's
verification run depend on Docker is not a trade this spike gets to make for
everyone. A CI runner that is already Linux does not need them at all, which is
the recommendation sent to issue 0061.

## Accepted static-analysis finding

Snyk Code reports **CWE-94 (code injection)** on `charness/main.c`:
`dlopen(argv[1], RTLD_NOW)` loads a library named on the command line. The
mechanism is correctly identified and the finding is **accepted, not fixed** —
loading a caller-named library is what the harness is for, the same trust model
as the dynamic loader itself, and no sanitisation preserves that purpose. What
bounds it: this binary is a test tool, never installed and never distributed,
and the only party who can supply `argv` is the party already running arbitrary
code on the machine. The rationale is repeated at the call site so it is not
re-litigated by the next reader.

No finding is reported against the bridge library itself.

## Third-party ledger

**Empty, deliberately.** The bridge depends only on core Domowik crates plus
`serde`, `serde_json` and `uuid`, all already recorded in
`docs/RUST-DEPENDENCIES.md`. The header is hand-written rather than generated,
so no binding generator is adopted. The Dart harness adopts **no** package —
`package:ffi` would have supplied `malloc` and `calloc`, and would have been a
dependency needing licence, provenance and notice records
(`docs/THIRD-PARTY-POLICY.md`) for two functions libc already exports into the
process.
