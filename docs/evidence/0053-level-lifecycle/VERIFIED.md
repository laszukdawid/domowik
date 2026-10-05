# Authoritative level lifecycle — 2026-09-05

Issue 0053, original Domowik implementation against BEHAVIOR-SPEC 2.2–2.3
and section 8. No new dependency, ABI signature or persisted field.

## Compatibility and authority

New GUI and CLI plans contain one elevation-zero level named Level 1 as a
clean, non-undoable snapshot baseline. Older CLI files used an empty snapshot
plus a level-create batch; those files continue to replay unchanged. Existing
names, IDs, snapshots and logs are never silently migrated. Compensating an
old initial creation now refuses if it would remove the last level.

`Document::validate()` retains entity-local validation for intermediate
builders and replay. `validate_plan()` additionally requires a materialized
level and is used by human/JSON CLI reports. Raw package read/write remains a
structural API usable by inspection, repair and adversarial fixtures, not a
claim that every readable package is a valid floor plan. `Plan::save` refuses
an empty materialized session before file work; failed Save As preserves its
old target, dirty/history state and existing bytes. CLI `add-level` can repair
a legacy empty plan. GUI open refuses such a file without transferring a
handle or replacing the current editor document.

Deletion is checked by one shared core predicate before each effective level
delete in operation order. It reports last-level protection, contained walls,
hosted openings, member joins and attached background images in a fixed order
with counts. Explicit earlier operations may remove contents or detach
references; there is no implicit level-delete cascade. A replacement level
must be created before deleting the last old one, not afterwards.

Local commit, undo, redo and explicit compensation use guarded scratch
application. External admission and held-batch promotion check newly applicable
batches at their canonical positions; accepted historical batches replay with
the prior rules. A refused admission publishes no partial document/log/history
change. Level refusals are deferred until the whole batch proves applicable:
a later stale or malformed operation still discards the whole batch under the
existing replay contract. Discarded external batches can be accepted into the
log and mark the session dirty; they are not successful document deletions.

Admission also checks the final materialized level count. Root found a bypass
after the first green suite: from baseline A+B, accept delete-A, then receive
a concurrent delete-B whose canonical position is earlier. Its own deletion
sees two levels, but historical delete-A subsequently leaves zero. A fixed
actor-order regression returned `Accepted` before the fix and rejects with
NoLevels afterwards, preserving the full admission snapshot. Local paths use
the same final-state protection defensively. A proposed local-reordering
fixture was withdrawn: its frontier was not transitively closed, so it did
not prove the claimed promotion/reordering. Separate valid held-promotion
tests prove refusal through both local and external prerequisite delivery.

The CLI resolves a unique level name and submits the same core operation;
duplicate names refuse as ambiguous. The GUI exposes Delete active level,
submits `deleteLevel` through the existing edit JSON protocol, and refreshes
the authoritative snapshot. The existing view controller then picks the lowest
survivor, preserves other flags and clears selection on activation change.
Undo restores the deleted ID ghosted without restoring historical view flags.

## Review and tests

The Herdr skill coordinated Opus 5's core implementation, Sonnet 5's CLI
implementation and Codex's format/container/native/Flutter integration.
Cross-review rejected a commit-only guard, duplicated CLI blocker logic and
an early refusal that would change stale-batch discard behavior. Hash-set wall
membership avoids scanning all walls separately for each opening or join member. The ordinary
no-delete path remains incremental; it does not replay the entire log.

Scoped evidence currently includes 107 core document tests, 52 format tests,
35 container unit tests plus its integration tests, 50 bridge tests and all
131 Flutter tests (51 native lifecycle). New cases cover operation order,
every blocking kind, backgrounds, compensation refusal, held promotion,
stale/malformed discard parity, legacy empty snapshot replay, last-level
protection, byte-preserving refusal, GUI deletion/undo/reopen and selection
preservation on nonempty refusal. Native ownership checks remain enabled.

History fixtures that previously undid the sole level now use a clean
baseline plus an added level when testing successful undo; separate regressions
prove the former transition is refused. The first Flutter run accidentally
loaded the old release library after only a debug rebuild; rebuilding the
release artifact made all 131 tests pass. No production relaxation was made
to accommodate those initial fixture failures.

Full `./scripts/verify.sh` exited 0 on implementation commit `26f09fc`, ending
with `=== VERIFICATION PASSED === all stages`. This includes the Rust/Go
checks, native sanitizer/FFI harnesses, all 131 Flutter tests and clean macOS
bundle/signature/runtime/first-frame probes. Tracker closure is committed
separately and the complete verification rerun against that final tree.
The CLI's 127 tests include human/JSON rejection of a raw empty plan and
explicit repair via `add-level`. `verify.sh` adds a named P16 CLI probe for
baseline/no-op undo, byte-preserving last-level refusal, accepted deletion,
compensation and reload, alongside the existing native and macOS probes.

## Ordinary-edit performance

Opus ran, with other workers' builds/tests stopped:

```sh
cargo bench -p domowik-document --bench session -- \
  --sample-size 20 --warm-up-time 1 --measurement-time 2 \
  '^commit/(100|2000)$'
```

Exit 0. Host: Apple M4 Max, 16 logical processors, 48 GiB, macOS 26.6.2
build 25G83, arm64. Rust 1.98.0 (`88d9e12ae`, 2026-08-18), LLVM 22.1.8,
Cargo 1.98.0; optimized bench profile with debuginfo. Criterion reported:

| Existing wall operations | Time estimate | Reported interval |
|---|---:|---:|
| 100 | 14.729 µs | 14.335–15.165 µs |
| 2000 | 190.15 µs | 189.28–190.95 µs |

These are Criterion time estimates/intervals, not frame-time percentiles.
The harness measures a further wall-creation commit, with session cloning in
setup; the timed closure owns/drops the cloned session. It exercises the
ordinary no-level-deletion path and stays below the harness's named-host 1 ms
alarm at 2000 operations. It does not measure the full editor, rendering,
deletion blocker enumeration or external full replay. Criterion's automatic
comparison against a previous local sample is not treated as controlled
before/after evidence, and no speedup is claimed.
The run includes the final nonempty-to-empty admission check. A subsequent
localized optimization reuses wall-ID membership for join blocker counting;
that deletion-only branch is not exercised by this ordinary-commit workload.

## Remaining scope

0054 owns authoritative field controls, duplicate-elevation/slab-overlap
warnings and default-copy integration. Room records and other future entity
types remain their existing schema/behavior issues; introducing them must
extend the deletion blocker inventory. Opaque future fields do not give this
version a general reference graph. This increment does not implement full
format compatibility/migrations, remote synchronization, 3D, physical-device
validation or whole-application frame/memory budgets.
