# 0024: Spike Flutter-to-Rust bridge

- **Phase:** 2
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0008, 0011

Activated 2026-08-25 under ED-0020 on branch `issue/0024-bridge-spike`.
Consumes the Done scaffold and issue 0011's delivered material as a labeled
provisional handoff. Environment audit first; absent toolchains are
assessed-and-deferred with explicit gaps, never simulated. Ledger-before-
reliance third-party records for all Dart/Flutter/native tooling; main Go
module stays pure-Go single-module. No Done while 0011 remains unclosed under
ED-0018 gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25) — Linux evidence slice

Branch `issue/0024-bridge-spike` (head `c4ffe81`) merged to main by `8dd26db`
with SBOM refresh `48e867e`; full main `./scripts/verify.sh` exits 0.
Delivered and independently verified (reviewer rerun plus `-race`, ASan/UBSan
C harness, real Flutter Linux app execution): real dart:ffi
`DynamicLibrary.open` loading of a c-shared Go core and stateful operation;
documented C ABI with opaque handles, C.malloc buffer ownership, no-Go-pointer
rules; tested errors, panic containment, callbacks/re-entry, threading,
cancellation; measurements recorded (library 5.0 MB / stripped 3.5 MB; nullary
export ≈42 ns; single-op round-trip ≈141 µs low-history; batched-20 ≈
9.4 µs/op-equiv; release bundle 20.5 MiB); findings F1–F5 in
`spikes/0024-flutter-go-bridge/README.md` including the F1 superlinear
Execute-cost signal feeding ADR 0002 gate 2; third-party ledger entries in
`docs/third-party/tools.json`; ADR 0002 evidence added while remaining
Proposed. One use-after-free was found during review and fixed with sanitizer
confirmation before acceptance.

**Not Done — explicit residuals:** Android validation is assessed-and-
deferred: this machine has no Android SDK, NDK, gradle, or emulator/device,
so that slice requires owner-side toolchain provisioning plus its own
ledger-before-reliance pass before work can resume. iOS remains deferred per
ADR 0002. Per ED-0020 this issue also cannot close while 0011 remains
unclosed under ED-0018 gates. Dependents may consume the Linux-slice evidence
as a labeled provisional handoff under partial-scope semantics.

## Problem

Validate ADR 0002's native bridge before building the GUI. A normal Go
cross-build is not sufficient.

## Acceptance criteria

- [ ] Linux and Android Flutter apps load ABI-appropriate libraries and call a
      stateful core operation; iOS support is explicitly assessed or deferred.
- [ ] Dedicated Go bridge package, exported C ABI, opaque handles, buffers,
      allocation/free, and no-Go-pointer rules are documented.
- [ ] Errors, panic containment, callbacks, thread scheduling, cancellation,
      and runtime shutdown are tested.
- [ ] Binary size, build time, call overhead, batching strategy, and packaging
      are measured.
- [ ] Exact tools/libraries/generated bindings, versions, provenance, direct
      and transitive licenses, notices, and distribution obligations are
      recorded.
- [ ] Evidence is added to ADR 0002; it remains Proposed until its geometry and
      Phase 1 workload/GC gates also pass.

## Status set by ED-0095 — 2026-09-01

**Blocked.** Blocked on Phase 2 starting. The spike's subject also changed: ADR 0003 assumed a Go core, and the core is Rust.

## Re-scoped and reactivated — 2026-09-01 (ED-0097)

**The subject changed.** ADR 0008 moved the core to Rust; ADR 0003's
re-assessment (2026-09-01) records what that changes at the boundary. The
Go-slice evidence above is HISTORICAL: it validated a bridge to a core that no
longer exists, and it is kept because its findings about the FLUTTER side —
dart:ffi loading, callback delivery, bundle packaging — survive the core's
language, while everything about the Go runtime does not.

**Criteria, re-scoped to the Rust boundary.** The originals below are kept
verbatim for the record; these supersede them.

- [ ] A Rust `cdylib` exposes a C ABI over the core's stateful `Session` seam,
      with opaque handles, explicit buffer ownership, and the rule that no
      Rust reference escapes the boundary.
- [ ] Panic containment, error transport, cancellation, threading, and handle
      lifetime are TESTED, not asserted. Rust has no runtime to initialise and
      no GC to constrain pointers — but a panic crossing `extern "C"` is
      undefined behaviour, so containment is a correctness requirement here,
      not a nicety.
- [ ] A plain-C harness drives the real library through `dlopen`, from several
      threads, including ownership abuse: forged handles, stale handles,
      double free, foreign pointers.
- [ ] A Dart/Flutter harness loads the same library with
      `DynamicLibrary.open` and calls a stateful operation.
- [ ] Library size, build time, call overhead, and batching are measured on a
      named host.
- [ ] Platform support is stated: what was executed, what is
      assessed-and-deferred with the reason, and what is simulated (nothing).
- [ ] Evidence is added to ADR 0003, which remains Proposed.

## Original Go-era criteria (superseded, kept for the record)

See the "Acceptance criteria" section above; every item there names Go, cgo,
or the Go runtime and is not answerable about a Rust core.

## Delivered — 2026-09-01, macOS/arm64 slice

`spikes/0024-flutter-rust-bridge/` and its README carry the run, the numbers
and six findings. ADR 0003 has the evidence and stays **Proposed**; the gate is
not passed.

| Re-scoped criterion | State |
|---|---|
| A Rust `cdylib` over the stateful `Session` seam, opaque handles, explicit buffer ownership, no Rust reference escaping | **Met.** 13 exports; handles and buffer-release keys are both monotonic never-reused tokens |
| Panic containment, error transport, cancellation, threading, handle lifetime TESTED | **Met.** 19 Rust tests, 68 C checks under ASan+UBSan, 43 Dart checks, plus poison recovery and a cross-thread cancel added by review |
| A plain-C `dlopen` harness from several threads, with ownership abuse | **Met.** 8 pthreads; forged handles, stale handles, double free, forged tokens |
| A Dart/Flutter harness loading the same library with `DynamicLibrary.open` | **Met for macOS** (2026-09-04). Plain Dart proves the boundary; a Flutter macOS application now proves bundle packaging, with two probes run from inside the built bundle. Signing is ad-hoc only and notarisation is unvalidated — see below |
| Library size, build time, call overhead, batching measured on a named host | **Met.** 905 KB / 717 KB stripped, 3.1 s warm, 17.2-18.3 ns nullary, ~10 µs round trip at short history, batching worth ~6.6x; ranges over three runs, not one |
| Platform support stated: executed, deferred with reason, nothing simulated | **Met.** macOS executed (universal binary built; at the time of this row only the arm64 slice had been EXECUTED, the x86_64 slice having been produced and named by `lipo`; the x86_64 slice was executed under Rosetta on 2026-09-12); Linux, Android assessed-and-deferred; iOS deferred, though the `staticlib`-in-a-framework packaging it needs is now the shape macOS uses |
| Evidence added to ADR 0003, which remains Proposed | **Met** |

**Why this is not Done.** One criterion is partly met (no Flutter application)
and the platform coverage is a single desktop OS. The gate ADR 0003 states is
about *target Phase 2 platforms*, which are Linux and Android; neither has a
host or a toolchain here, and neither was simulated.

**Adversarially reviewed** (Codex, gpt-5.6-sol, high effort) as a debate on
"the documented boundary rules hold". The counter position won: `close` was not
the quiescence barrier the header promised, a cancellation could be
acknowledged and then erased, and "never reused" was not literally implemented.
All fixed, with tests that fail on the previous implementation. The review also
found four weaknesses in the spike's own evidence — including a stress test
that passed on the broken implementation and was therefore deleted rather than
kept. ABI revision 3; 19 Rust tests, 68 C checks, 43 Dart checks.

**Three further defects were found in the bridge's own first draft** and fixed with
mutation-proven tests: buffer release keyed by pointer (every zero-length
allocation shares address `0x1`, so two empty buffers collided — demonstrated);
`close` returning while a call was still inside the core (demonstrated); and
the leak counters reporting a contained panic as the plausible count `6`. The
spike README records them, because a spike whose value is "we learned what this
boundary costs" should say what it got wrong on the way.

## Delivered — 2026-09-04, the Flutter application

The largest gap this issue carried — *no Flutter application was built, so
bundle packaging is unvalidated* — is closed for macOS. See the spike README
for the full record.

**What was built.** `flutter_plugin/` builds the Rust crate as a `staticlib`
and links it into the pod's framework with `-force_load`, so Xcode embeds,
signs and `@rpath`-names it inside the bundle. `flutter_app/` is a real
application that builds a plan, draws what the core returns, undoes, has a
batch refused, and survives a deliberate Rust panic.

**Why a `staticlib` and not the `cdylib`.** A cargo `cdylib`'s `LC_ID_DYLIB`
is the absolute path of the machine that built it, so copying it into a bundle
produces an app that works only for whoever built it. This was observed
directly, not assumed.

**Evidence is produced by the bundle, not by the author looking at a window.**
Two headless probes run in `verify.sh`: `DOMOWIK_SPIKE_SELFTEST=1` (23 checks)
and `DOMOWIK_SPIKE_FIRSTFRAME=1`, which drives the real widgets and reports
from inside `CustomPainter.paint`. Both pass from the build directory and from
a copy relocated to `/tmp`.

### Three defects found in this round, two of them in our own evidence

1. **The framework linked a STALE archive.** The podspec declared
   `:output_files` with no inputs, so Xcode treated the phase as up to date and
   skipped cargo entirely. Found by setting `ABI_VERSION = 99` in the Rust
   source, rebuilding, and watching the bundle still report 3 — **and both
   probes passed on that stale build.** Fixed with `:always_out_of_date`;
   re-running the same mutation now fails the self-test, which is the proof
   the fix works. Note for the record: the reviewer reasoned that a phase with
   outputs but no inputs would still run every build. The experiment says
   otherwise, and the experiment wins.
2. **The probes proved less than they claimed** (adversarial review, Codex
   gpt-5.6-sol at high effort). `DynamicLibrary.open` succeeding was reported
   as "loaded from inside the bundle" while proving nothing of the kind — an
   identically named framework elsewhere satisfies it. Now resolved with
   `dladdr` on a real exported symbol and compared against the bundle root.
3. **The first-frame probe counted loop iterations, not drawing.**
   `wallsPainted` incremented BEFORE `drawLine`, so a painter whose draw call
   was deleted still reported four walls. It now increments after the call and
   the probe additionally rasterises the painter and counts non-background
   pixels (8,216 of them). Negative control: neutralising `drawLine` yields 0
   pixels and fails the probe.

The review also raised the pinned-toolchain hazard — `--manifest-path` does
not make rustup honour `rust-toolchain.toml`, because rustup resolves
directory overrides from the process's working directory — which is fixed by
building from inside the spike.

### What is still NOT validated, stated precisely

- ~~**Notarisation.**~~ **Out of scope (ED-0099, owner, 2026-09-04).** macOS
  is a local-development target, not a distribution target. Notarisation,
  Gatekeeper acceptance and the hardened runtime are no longer residuals of
  this issue. For the record of what was observed: `security find-identity -v
  -p codesigning` reports 0 valid identities here, the bundle is ad-hoc signed,
  and `spctl` rejects it, exactly as expected.
- ~~**Distribution shape.**~~ **Out of scope (ED-0099).** No archive, DMG or
  pkg is owed for a target nobody distributes.
- ~~**The x86_64 slice was built but never executed.**~~ **Executed
  2026-09-12.** `cargo build --release --target x86_64-apple-darwin` and the C
  harness compiled `-arch x86_64`, run under `arch -x86_64`: 131 checks, 0
  failures, panic containment included. The x86_64 code really ran, so the
  question this residual actually asked, whether the universal build is sound,
  is answered. Two limits stay: it ran under Rosetta 2 translation rather than
  on Intel hardware, and no timing is taken from a translated process. The
  Dart harness has no x86_64 counterpart here, because an arm64 Flutter
  install ships only an arm64 Dart.
- **No clean-machine install was tested** — still true for macOS. One `.app`
  was run from the build directory and from `/tmp`, both on the machine that
  built it. **Answered for Linux on 2026-09-12**: the bundle runs on an image
  carrying GTK, GLib, Mesa and Xvfb, with no cargo, rustc, flutter, dart,
  clang, cmake or pkg-config, and the image asserts their absence. That run
  found a dependency nothing else would have: the first attempt aborted with
  `Couldn't open libEGL.so.1`, which appears in no `ldd` output because the
  Flutter engine dlopens it.
- **An incremental Flutter build leaves the bundle signature invalid**
  (`nested code is modified or invalid`): Flutter rewrites `App.framework`
  without re-sealing the outer bundle. A clean build is valid. This is
  Flutter's behaviour, not the spike's, and it is why the `verify.sh` stage
  builds clean.
- **Concurrent builds can race** on the single archive path in the plugin
  source tree. Acceptable for one `flutter build` at a time; a product build
  would write per-configuration into `DERIVED_FILE_DIR`.
- ~~**Linux and Android remain unexecuted**~~ — true when written on
  2026-09-04, and both were executed on 2026-09-12. Linux: ABI on a native
  arm64 kernel, the Flutter application, and a clean-machine run. Android: a
  release APK on an AVD. See the delivery sections below.

**Identity note.** A macOS bundle needs an application identifier, which is an
identity claim, and `docs/NAMING.md` forbids one pending separately
established authority — `domowik.dev` is registered by a third party (dossier
O4). The spike therefore uses the RFC 2606 `.invalid` TLD
(`invalid.domowik-spike.domowikSpikeApp`), which can never be delegated to
anyone. Issue 0034 settles the real one.

## Delivered — 2026-09-05, a usable editor

ABI revisions 4 through 7, and the app is now something a person can draw a
plan in rather than a demonstration that a call crosses a boundary.

| Added | Why it had to be in the core |
|---|---|
| `domowik_plan_derived` | The snapshot carries centre lines, so the app was drawing sticks. Rooms, the occupied region and apertures are COMPUTED and never stored; rebuilding them in Dart would be a second geometry implementation in the view |
| `domowik_plan_open` / `_save` | An editor you cannot save from is not an editor. Opening CONTINUES the file's log, so a plan edited by the CLI and then the GUI stays one history |
| `domowik_plan_edit` | A gesture is an intention whose consequences belong to the specification — see ED-0101 |

**What the app does now.** Drag empty space to draw a wall, with a 12-degree
axis snap. Drag an endpoint and the whole join node moves with it (R3.4.2).
Drag a wall body to translate it. Click a wall to select, Delete to remove.
Click a wall with Door or Window armed to place one, projected onto the host's
centre line. Scroll to zoom about the cursor, Cmd- or Alt-drag to pan. Rooms
are filled and labelled with the area the core measured to the inner faces.
Open and Save use a native AppKit dialog through a method channel.

**The finding that mattered most** was not about the boundary at all. Building
a second front end revealed that the CLI owned the editing semantics, so the
GUI had been assembling batches by hand and quietly producing different
documents for the same action. ED-0101 records the move and the rule it
establishes.

Evidence: 31 Rust tests in the bridge, 33 checks in the bundle self-test —
which now saves, reopens and compares DERIVED geometry from inside the built
`.app` — 43 Dart checks, 68 C checks under ASan+UBSan.


## Delivered — 2026-09-12, Linux

Linux is executed end to end: the ABI on a native `aarch64` kernel, and the
Flutter application with its bundle probes. Two scripts under
`spikes/0024-flutter-rust-bridge/docker/` reproduce both, each exiting 0 with
its own banner. The spike README carries the detail and the ADR 0003 evidence
section carries the summary.

| Re-scoped criterion | State after this round |
|---|---|
| A Rust `cdylib` over the stateful `Session` seam | **Met on macOS and Linux.** The `cdylib` ships unmodified on Linux; ELF has no install-name field, so the defect that forced macOS into a `staticlib` does not exist there |
| Panic containment, error transport, cancellation, threading, handle lifetime TESTED | **Met on macOS and Linux.** 76 Rust tests, 131 C checks under ASan+UBSan and 97 Dart checks all run on Linux, not inferred from the macOS run |
| A plain-C `dlopen` harness from several threads, with ownership abuse | **Met on macOS and Linux** |
| A Dart/Flutter harness loading the same library | **Met on macOS and Linux.** The Linux application bundles the core, resolves it through the runner's `$ORIGIN/lib` `RUNPATH`, passes 48 bundle checks with `dladdr` provenance, and still passes after the bundle is copied elsewhere |
| Library size, build time, call overhead, batching measured on a named host | **Met on macOS.** The Linux numbers are a virtual-machine comparison, and the application slice is emulated amd64 and yields none. Stated, not quietly folded in |
| Platform support stated: executed, deferred with reason, nothing simulated | **Met.** macOS, Linux and Android all executed, each with its limits named; iOS deferred; nothing simulated |
| Evidence added to ADR 0003, which remains Proposed | **Met** |

**What this round also changed, and it was overdue.** `openBridge` was
`if (Platform.isMacOS)` and a throw. That shape cannot be tested for any
platform but the host running the test, so the Linux and Android answers would
have shipped unexamined. Library resolution and the provenance root are now
looked up from a platform string, with 11 checks over them, and both were
shown to fail under mutation before being accepted.

**Adversarially reviewed** (Codex, `gpt-5.6-sol`, high effort) as a debate on
"the Linux evidence establishes that the application loads and drives the
bundled core". The counter position won on one point: `build_rust.sh` could
bundle a STALE library, because it copied a fixed path after a `cargo build`
whose output an inherited `CARGO_TARGET_DIR` can redirect. Reproduced with
`ABI_VERSION = 77` redirected elsewhere, the bundle shipped 12 and the
self-test passed. Fixed by pinning the target directory and asserting the
artifact is not older than its sources. Three further findings were real and
fixed: the build-path scan used the wrong root and scanned one subdirectory
while claiming the whole bundle, and probe markers were substring rather than
whole-line matches.

The review's central claim, that `LD_LIBRARY_PATH` can select an external core
while the probes still pass, does not hold, and the round was handicapped by
my own scoping: `selftest.dart`, which carries the `dladdr` defence, was not
among the four files it was given. The probe now plants a decoy on
`LD_LIBRARY_PATH` every run and requires the self-test to refuse it, so the
defence is demonstrated rather than argued.

**Android items 2 and 3 landed** (2026-09-12) against strings observed on a
named ARM64 emulator by the Android work stream. The predicate that looks obvious is wrong:
with `extractNativeLibs=false` nothing is extracted, the loader maps the
library out of the APK, and the application's native library directory exists
and is EMPTY, so comparing against it fails on a correctly packaged APK. The
provenance root is the APK path, supplied through the process environment
because both install-scoped segments change on every install.

## Delivered — 2026-09-12, Android

Executed by a separate work stream and merged here with its commits intact.
A release APK on the `domowik_0024_arm64` AVD (AOSP `arm64-v8a` system image,
Android 16 / API 36) loads the core from inside the APK and passes the same
48-check bundle self-test the other platforms run, the first-frame probe (4
walls drawn, 57,788 non-background pixels rasterised, 0 outstanding buffers)
and an Android-only lifecycle probe holding `openPlans` at 1 across rotation,
pause and resume, reaching `close=0 openPlans=0 buffers=0` at teardown in the
same process. APK 18,672,066 B; core 2,612,904 B as cargo builds it and
1,947,336 B as the Android Gradle Plugin packages it. Full record in
`docs/evidence/0024-android/MEASURED.md`, toolchain in
`docs/third-party/toolchain-android.json`.

**Six limits, recorded rather than smoothed away.** It is an emulator, not a
device, and no claim is made about any phone. One ABI, `arm64-v8a`,
deliberately, so no unexecuted artefact is acquired. The APK is debug-signed,
a development artefact on the same reasoning ED-0099 applied to macOS.
Activity destruction and recreation is not tested, because `configChanges`
means rotation never destroys the activity. Reading probe output needs
`adb root`, a property of the harness. File dialogs are not wired on Android,
though the core's own `plan_save` and `plan_open` are exercised from inside
the APK, so that is a missing platform channel and not a boundary question.

**The packaging genuinely differs**, which is why it needed its own evidence
instead of an argument from analogy: with `extractNativeLibs=false` the
library is stored uncompressed and mapped straight out of the APK, so the
loaded image is `<apk>!/lib/arm64-v8a/libdomowik_bridge.so` and the native
library directory exists and is empty.

## The evidence is ABI 12; the head is ABI 13 (2026-09-12)

Issue 0060's room work landed `feat(bridge): name rooms from the editor at ABI
13`, so the boundary this spike validated has moved underneath its evidence.

**Every probe result recorded above was taken at ABI 12 and is now
HISTORICAL.** It is not wrong and it is not withdrawn: it is proof about a
revision that is no longer at the head. What it is not is proof about the
current head, and the difference matters here more than it usually would,
because this spike's whole contribution is evidence rather than product code.

What the merge does establish, because it was run: the ABI documentation guard
passes at ABI 13 with the export count unchanged at 28, so the room work was
additive inside an existing export rather than a new surface. That is a much
weaker statement than "the bridge works at ABI 13", and it is deliberately not
being reported as one.

**Owed before this issue can close**, per root: a clean integrated verification
at ABI 13, and the Linux and Android probes re-run against it for real. Neither
is done. The Android work stream has ended, so the Android re-run needs an
owner for the AVD or a decision that the ABI 12 Android result stands with its
revision named.

## Where this issue now stands

Every re-scoped acceptance criterion is met at ABI 12, on three platforms
rather than one, with the limits of each stated. The head is now ABI 13 and
that evidence has not been re-taken against it. **ADR 0003 nevertheless stays Proposed,
and this issue does not close on its own authority**: the ADR's gate is not
only about the bridge working on Linux and Android, it also requires an editor
prototype meeting measured input and frame-time goals, and those belong to
issue 0033. What this spike owed the gate, it has now paid.

Remaining residuals, none of which this issue can close alone: no physical
Android device; one Android ABI; no macOS clean-machine run; iOS untouched;
and the Android and Linux timings are contended observations, taken on a host
where other agents were building, so a performance figure for either platform
is owed from a quiet run and is not claimed here.

**Third-party records were made before reliance,** covering the container
runtime, the pinned base image, the Debian compiler packages, the Dart SDK and
the Flutter Linux SDK. The container runtime's own licence eligibility is
recorded as unassessed rather than cleared; that row is not a compliance
finding in either direction.


## Re-verified at ABI 13 — 2026-09-12

The evidence above was taken at ABI 12 and was marked historical. It has now
been re-taken at ABI 13, on the integrated head, and the re-run found a defect
that the ABI 12 evidence had been hiding.

| Slice | Result at ABI 13 |
|---|---|
| Full `./scripts/verify.sh` | exit 0, `=== VERIFICATION PASSED === all stages`, no skipped stages, clean tree |
| Linux ABI, native arm64 | 78 Rust tests (76 at ABI 12; rooms added two), 131 C checks under ASan+UBSan, 97 Dart checks, aarch64 ELF asserted |
| Linux application | bundle self-test 48 checks, first-frame probe, external-core decoy still refused, relocated copy still passes |
| Linux clean machine | both probes pass on an image with no toolchain |
| Android, AVD `domowik_0024_arm64` | self-test 48 checks, lifecycle probe, first-frame probe — **after a defect was fixed, below** |

### The re-run earned its keep: the first-frame probe was a race

Android failed on the first attempt: 4 walls in the document, 57,788 pixels
rasterised, and `walls drawn by the painter: 0`. Pixels were being drawn while
the counter insisted nothing had been.

The painter was not at fault. A single `addPostFrameCallback` fires after
whichever frame was already in flight, and that is not necessarily the frame
which laid out the `setState` from `_addWall`. The probe read the counter once,
immediately, so which value it saw was a race. macOS and Linux won that race;
Android lost it.

The probe now waits for the painter, bounded at 120 frames, and prints how many
frames it waited. `painted == 4` is still required, so it cannot pass
vacuously.

**Demonstrating the fix took four runs, and the first one proved nothing.** It
waited 0 frames and passed, which means the code path added had not executed.
Across four runs the waits were 0, 1, 0, 1 — and the two runs that waited a
frame are runs that would have read 0 and failed under the old one-shot
sample. That is the mechanism, observed rather than argued.

The general lesson is worth more than the fix: **this probe was green on two
platforms for weeks while containing a race, and only a third platform's
timing exposed it.**

### The other probes were then examined for the same shape

Raising that risk and leaving it hanging would have been worse than not
raising it, so every site that could carry it was checked. There are four
`addPostFrameCallback` or frame-scheduling sites in the application and one
accumulating probe:

| Site | Carries the race? |
|---|---|
| `_runFirstFrameProbe` | **Yes.** Fixed above |
| `main.dart:250`, the probe's own dispatch | No. It only *starts* the probe; the verdict is not read there |
| `main.dart:1460`, snap re-evaluation after a camera change | No. Production behaviour, not a verdict. It re-checks `mounted` and the drag state before acting, so a stale frame makes it do nothing rather than conclude something |
| `main.dart:1478`, chain hover re-evaluation | No, for the same reason |
| The Android lifecycle probe | No. Its verdict is ACCUMULATED — `_seen`, `_sizes`, `_brokenAfter` gathered across transitions — rather than sampled at an instant, and it fails closed: if the harness never delivered a rotation, `_sizes.length < 2` and the run FAILS rather than passing on the missing evidence |
| The bundle self-test | No. It runs before `runApp`, so there are no frames to race |

So the defect was confined to the one probe, and the answer is bounded rather
than reassuring-sounding. The lifecycle probe is worth singling out: it had
already been repaired once for the adjacent failure of containing an assertion
that could not fail where it was evaluated, and requiring two distinct physical
sizes is what makes its rotation claim load-bearing.

### Snapshot caveat

`snap0058` has uncommitted work on `arrangement.rs` that is not yet wired into
`split_all`, so anything here that exercises boolean output reflects the
current construction path and not the one about to replace it. Recorded so a
later divergence is not a surprise.
