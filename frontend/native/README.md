# Domowik native bridge

The editor's Rust adapter exposes the shared document/editing core through a
length-delimited C ABI. `frontend/Cargo.toml` owns the separate frontend
workspace. This is an experimental application interface, not a stable SDK.
See [Getting started](../../docs/GETTING-STARTED.md),
[Architecture](../../docs/ARCHITECTURE.md) and
[Compatibility](../../docs/COMPATIBILITY.md) for current platform boundaries.

## Layout

- `src/`: native bridge, plan-local model library and scene operations.
- `include/domowik_bridge.h`: complete C declarations and ownership/safety rules.
- `charness/main.c`: native ABI and ownership-abuse harness, including threads.
- `tests/`: ABI documentation and native warning-pipeline contracts.
- `../dart_harness/`: shared hand-written Dart bindings and standalone harness.
- `../plugin/`: platform build and packaging integration.
- `../lib/core/`: the common editor interface and native/browser hosts.

## ABI summary (internal, not a stable product contract)

```c
int32_t domowik_abi_version(void); /* 25 */
int32_t domowik_plan_create(uint64_t* out_handle);
int32_t domowik_plan_create_default(uint64_t* out_handle);
int32_t domowik_plan_execute(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_assets(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_undo(uint64_t, domowik_buf*);
int32_t domowik_plan_redo(uint64_t, domowik_buf*);
int32_t domowik_plan_state(uint64_t, domowik_buf*);
int32_t domowik_plan_snapshot(uint64_t, domowik_buf*);
int32_t domowik_plan_hit_test(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_marquee(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_snap(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_stair_preview(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_edit(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_derived(uint64_t, const char*, domowik_buf*);
int32_t domowik_plan_open(const char*, uint64_t* out_handle);
int32_t domowik_plan_save(uint64_t, const char*);
int32_t domowik_plan_open_bytes(const uint8_t*, uint64_t, uint64_t* out_handle);
int32_t domowik_plan_encode(uint64_t, uint64_t* out_revision, domowik_buf*);
int32_t domowik_plan_mark_saved(uint64_t, uint64_t revision);
int32_t domowik_plan_import_model(uint64_t, const uint8_t*, uint64_t,
                                  const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_scene_frame(uint64_t, const uint8_t*, uint64_t, domowik_buf*);
int32_t domowik_plan_export_usdz(uint64_t, domowik_buf*, domowik_buf* diagnostics);
uint8_t* domowik_web_alloc(uint32_t len); /* browser only */
void domowik_web_free(uint8_t*, uint32_t len); /* browser only */
int32_t domowik_last_error(uint64_t, domowik_buf*);
int32_t domowik_buffer_free(uint64_t token);
int32_t domowik_plan_close(uint64_t);
int32_t domowik_debug_panic(void);
int32_t domowik_busy(uint64_t, uint64_t millis);
int32_t domowik_cancel(uint64_t);
int64_t domowik_outstanding_buffers(void);
int64_t domowik_open_plans(void);
int64_t domowik_debug_cancel_epoch(uint64_t);
int32_t domowik_debug_panic_holding_lock(uint64_t);
int64_t domowik_debug_hit_index_builds(uint64_t);
int64_t domowik_debug_snap_index_builds(uint64_t);
int64_t domowik_debug_warnings_computations(uint64_t);
```

All 38 declared exports are checked against the implementation and header by
`tests/abi_documentation.rs`. The two `domowik_web_*` exports exist only in the
browser module. Read the header for complete parameter and failure contracts;
this summary does not replace its pointer-validity preconditions.

## Ownership, errors and concurrency

- Plans are opaque `u64` registry tokens, never exported Rust references.
  Forged, stale and closed handles are refused. Handles and buffer tokens are
  never reissued; counters saturate and report exhaustion instead of wrapping.
- Request bytes are borrowed read-only for the call. Response bytes are owned
  by the library and released using `domowik_buffer_free` with their token,
  never `free` or a caller allocator. They are length-delimited, not necessarily
  NUL-terminated. Unknown/double-freed tokens are refused.
- Native stateful exports contain Rust unwinding and transport a panic as a
  status. Keep the native unwinding profile; aborting would invalidate that
  contract. The browser instead traps and stops the core host, losing unsaved
  tab state; see [ADR 0012](../../docs/decisions/0012-browser-editor.md).
- Errors use a sticky per-handle last-error string. Retrieving it does not
  mutate it. Failure transport must not be mistaken for a successful empty
  response.
- Session-mutating calls serialize per handle; distinct handles may proceed
  independently. Cancellation and the bounded busy probe deliberately do not
  wait for the session lock. There are no foreign callbacks in this interface.
- Cancellation advances a monotonic per-handle epoch; starting another operation
  does not erase an acknowledged cancellation.
- Each call holds a lease across request parsing, core work and serialization.
  Close drains leases, so after close returns no thread remains in the ABI on
  that plan's behalf.

## Document, assets and rendering

The bridge forwards accepted edits as atomic batches to the authoritative core.
Undo/redo, warnings, hit/snap queries and derived geometry are core behavior;
selection, clipboard and camera state remain editor-session concerns. Save
clears dirty only after success; opening does not restore a GUI undo stack.

`domowik_plan_stair_preview` is read-only. It derives the same upper-to-lower
drawing request committed by `addDescendingStaircase`, including nearest-lower-
level selection and draft defaults. The returned construction graphics include
Rust-derived Floor-panel properties. `setStaircase` applies name, count and
absolute endpoint elevations atomically, storing elevations as offsets from the
existing level datums. Preview never changes the document or undo history.

ABI 25 includes stair footprints in point/marquee selection on either endpoint
level. `translateSelection` accepts stair assembly IDs and translates both plan
endpoints with aligned associated upper-floor cuts in one undoable batch.
Custom or shared cuts refuse atomically. No new persisted fields are needed.

ABI 25 adds `resizeStaircase {request:{staircaseId,levelId,edge,deltaMm}}`;
`edge` is `left`, `right`, `lower` or `upper` relative to the stored lower-to-upper
axis. The same request passed to `domowik_plan_stair_preview` is read-only.
Rust projects side drags, retains the opposite side/end, and updates aligned cuts in
one batch without changing riser count or elevations. Invalid dimensions and
custom/shared cuts refuse atomically; unchanged edits add no history.

An end handle can move freely in two dimensions, editing length
and angle around the opposite endpoint. Optional `snapAngle` (default false)
constrains its direction to absolute 15° increments using projected length.
Side handles remain width-only. Preview and commit use the same Rust calculation;
no persistence fields, symbols or ownership/threading rules change.

The plan-local model library embeds placed content by digest. Model decoding,
texturing and preview limits are described in [Compatibility](../../docs/COMPATIBILITY.md).
No external-cache or remote-retrieval resolver is configured by the editor.
The additive `thumbnail` asset request derives angled library art from an
embedded digest or a bounded file within a selected library directory. It is
read-only: unlike placement, it neither embeds content nor attaches directories.
See the C header for ownership, threading, close/drain and failure guarantees.
Required degraded content permits inspection but refuses mutation and normal
saving; optional missing content retains diagnosed placeholders.

Scenes are renderer-neutral data derived by Rust. Presenters own GPU resources,
surfaces and camera updates. Camera motion must not decode models, serialize a
fresh scene or upload unchanged geometry. Platform and whole-editor performance
gates remain open; no host-independent latency promise is made here.

## Build and check

From the repository root:

```sh
cd frontend
cargo test --locked
cargo build --release --locked
flutter pub get
flutter analyze
flutter test
```

`./scripts/verify.sh` from the repository root also drives C/Dart boundary
harnesses and applicable bundle probes. Native widget tests require the real
library. macOS links a Rust static library into the plugin framework; Linux
bundles the shared library. Earlier Android execution is not a current full
editor/3D support claim; iOS and native Windows remain unvalidated.

The C harness deliberately loads a caller-named library with `dlopen`. That
capability is part of a local test tool's trust model, not an untrusted network
input, and the harness is not distributed as application functionality.

Dependencies and public-format/test-data provenance are recorded in
[RUST-DEPENDENCIES.md](../../docs/RUST-DEPENDENCIES.md),
[NOTICE](../../NOTICE), [Provenance](../../docs/PROVENANCE.md) and the
[third-party policy](../../docs/THIRD-PARTY-POLICY.md).
