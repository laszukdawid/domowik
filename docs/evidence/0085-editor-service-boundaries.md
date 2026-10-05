# Editor service boundaries — issue 0085

## Provenance and scope

Original owner approval, 2026-10-04, following a read-only application-boundary
review. Implemented from existing Domowik code and BEHAVIOR-SPEC §9/§2; no
external source, assets, dependencies, geometry algorithms or schema changes.

## Ownership and flow

```text
Editor widget (UI choices, dialogs, gestures)
  ├─ FileWorkflowController (serialized actions and leave approval)
  │    └─ FileDialogs (native picker and close-request adapter)
  ├─ PlanSession (native plan handle and file-operation allocations)
  │    └─ Rust core (document/session persistence and guarantees)
  └─ PlanGeometryCache (content-key reuse and visibility eviction)
       └─ Rust-derived geometry → existing PlanPainter
```

`PlanSession` owns exactly one native handle, locally releases file-operation
allocations and captures save error detail before subsequent reads. A candidate
plan opens before the old one closes; failure preserves handle/path. Disposal
is idempotent. Editing and rendering borrow the handle; they do not close it.
These calls remain synchronous on the UI isolate, as before. Moving file I/O
off that isolate requires a separate concurrency/lifetime design, not a claim
that returning a Future makes synchronous native I/O asynchronous.

`FileWorkflowController` owns the busy gate used by New/Open/Save/close and
existing property dialogs. It rejects overlapping actions rather than queuing
stale user choices, cancels active gestures before an action, handles explicit
Cancel/Discard/Save decisions, and suppresses UI/error callbacks after disposal.
It has no Flutter widget or native handle ownership. `FileDialogs` is injected
through PlanPage, with the existing method-channel adapter as default. UI
presentation and successful New/Open reset share one widget reset function.

`PlanGeometryCache` owns session/content-key invalidation and visible-level
eviction. Failed reads remove stale values and remain retryable; missing keys
are never cached. Camera changes do not enter the cache API. Moving two maps
into one keyed entry preserves constant-time lookup and stores key/data
together. It does not derive geometry or own images/GPU resources.

## Remaining boundaries

This is an incremental extraction, not completion of the architecture review.
The widget still assembles render layers, owns asynchronous model-preview
images, and performs other editing/query FFI calls. `ThreeDView` still owns
surface/upload scheduling together with its input/presentation. A preview
resource owner and 3D renderer controller remain separate follow-up work.
2D and 3D rendering interfaces intentionally remain distinct.

## Checks

Seventeen added tests exercise workflow decisions, overlap refusal, errors,
disposal, native dialog path/suggestion/cancel behavior, real native session
replacement/save-failure ownership, geometry reuse/eviction/retry, and injected
picker integration/unmount. Existing lifecycle tests exercise file actions,
native close, undo/redo and per-level cache behavior against the real core.

Flutter analysis, all 305 Flutter tests and the Linux release build pass.
Canonical verification passed all executed checks through tracker consistency
and ended with `=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1
because the workspace contains uncommitted changes. macOS execution was
explicitly skipped on Linux. No full clean-tree pass is claimed.
