# ADR 0003: Flutter GUI and wgpu-native 3D renderer

- **Status:** Proposed
- **Date:** 2026-08-22

## Context

The GUI must eventually support desktop editing, mobile editing/capture, a
high-performance 2D canvas, full 3D scenes, and Gaussian Splat viewing.
Candidates include Flutter, a web/Tauri stack, Qt, Compose Multiplatform, and
per-platform native applications.

## Proposed decision

- Flutter provides the application shell and interactive 2D canvas.
- A modern native GPU layer provides 3D and splat rendering; wgpu-native is the
  leading candidate, consumed through its C API.
- The GPU surface bridge belongs to the GUI/platform plugin, not the core.

## Validation gates

Flutter becomes Accepted after the core/native bridge works on target Phase 2
platforms and an editor prototype meets measured input/frame-time goals.

wgpu-native becomes Accepted only after a Phase 3 spike proves:

- Flutter surface or texture integration and lifecycle handling;
- Linux and Android binary packaging without assuming upstream mobile
  prebuilts;
- callback/thread scheduling and GPU resource ownership;
- a triangle plus representative instanced geometry and sample splat shader;
- measured startup, memory, and frame behavior on named devices.

## Alternatives and failure response

The strongest alternative is a web UI packaged with Tauri, especially if
browser distribution becomes primary. Native GL may be a limited fallback but
does not receive automatic acceptance. A failed wgpu spike requires a new ADR,
not hidden platform-specific divergence.

## Re-assessment after ADR 0008 — 2026-09-01

[ADR 0008](0008-rust-core-implementation.md) moved the core from Go to Rust.
This ADR is re-assessed against that, as issue 0037 requires, and it **stays
Proposed with its gates unchanged**.

**What changed.**

- **The bridge is a Rust FFI, not a cgo one.** Every obligation this ADR's
  gates name — ABI packaging, memory ownership, errors, threading, callbacks,
  cancellation, runtime lifecycle — is now assessed against `extern "C"` from
  Rust. Two things got EASIER and neither is a free pass: Rust has no managed
  runtime to initialise or shut down on a foreign thread, and no garbage
  collector whose moving/pinning rules constrain pointers handed across the
  boundary. Two things got HARDER: panics must be caught at the boundary or
  they are undefined behaviour across FFI, and ownership must be transported
  explicitly because there is no runtime to outlive a mistake.
- **Issue 0024's title is stale.** It reads "Flutter-to-Go bridge spike"; its
  subject is now the Rust boundary. The issue is renamed when the spike is
  activated, not before, so the tracker id stays stable.
- **The Phase 3 mobile-prebuilt concern narrowed.** wgpu-native is itself Rust,
  so the GPU layer and the core share a toolchain and a cross-compilation
  story rather than needing two.

**What holds.**

- Flutter for the shell and 2D canvas, for the same reasons: the candidate set
  in Context did not change, and nothing in ADR 0008 speaks to the GUI.
- wgpu-native as the leading 3D candidate, through its C API.
- Every validation gate, verbatim. **This ADR does not become Accepted because
  the core changed language** — no spike has run, and the ADR 0008 evidence is
  about geometry predicates, which say nothing about frame times or surface
  integration.
- The failure response: a failed spike requires a new ADR, not
  platform-specific divergence hidden in the code.

## Evidence — issue 0024 bridge spike, macOS/arm64 slice (2026-09-01)

**Location update (2026-10-05, issue 0092):** the actively developed interface
now lives at `frontend/`, with the native adapter at `frontend/native/`.
[Its README](../../frontend/README.md) maps historical paths. This promotion
does not change this ADR's Proposed status or its validation gates. The
issue-0025 crate contained only dependency setup and a placeholder and was
subsequently removed in issue 0093; its requirements remain in the tracker.
No wgpu surface renderer has been implemented or merged into the interface.

`spikes/0024-flutter-rust-bridge/` carries the run and its numbers. **The gate
is not passed** and this ADR stays Proposed; what follows is what the slice
does and does not establish.

**Established.** A Rust `cdylib` over the core's stateful `Session` seam loads
under `DynamicLibrary.open` — the call Flutter makes — and is driven from Dart
and from plain C through `dlopen`. Opaque handles, explicit buffer ownership,
panic containment across a foreign frame, a sticky per-handle error, per-handle
serialisation with parallel distinct handles, and cancellation that does not
wait for the work it cancels are all tested rather than asserted: 14 Rust
tests, 60 C checks under AddressSanitizer and UBSan, 37 Dart checks. All three
run in `scripts/verify.sh`.

**The boundary is not the cost.** A nullary export measures 18.0 ns. A full
round trip at short history is 10.4 µs, of which about 2 µs is the caller's own
JSON encoding; batching twenty operations per call is worth 6.7x on comparable
footing. The library is 905 KB unstripped, 717 KB stripped, and builds in
about 3 s warm.

**Two of ADR 0002's Go-era concerns are void, not solved.** There is no runtime
to shut down, so the Go slice's F3 has no analogue; and with no garbage
collector, the no-Go-pointer rule that dominated the cgo bridge does not exist.
Two obligations replace them, both discharged here: a panic must never cross
`extern "C"`, and ownership must be transported explicitly because Rust's
allocator is not libc's.

**Not established, and the gate turns on these.** No Flutter APPLICATION was
built, so bundle packaging, signing and notarisation are unvalidated — the
Dart VM's FFI is what was exercised, which proves the boundary and not the
bundle. Linux is assessed-and-deferred with no host available; Android is
assessed-and-deferred with no SDK, NDK or device and no cross-compilation
target installed; iOS is deferred and is a *different* packaging story, because
a `cdylib` is not loadable there and that slice needs a `staticlib` and a
framework. Nothing was simulated.

**The spike was then reviewed adversarially** (Codex, gpt-5.6-sol, high
effort), as a debate on the proposition "the documented boundary rules hold".
The counter position won. It found that `close` was not the quiescence barrier
the header promised — three windows escaped the plan lock, including one where
a call had resolved its handle but not yet entered the core — that a
cancellation could be acknowledged and then erased by an unrelated operation
starting, and that "handles are never reused" was not literally implemented
because the counter wraps. All three are fixed, with tests that fail on the
previous implementation.

It also found four weaknesses in the spike's own EVIDENCE, which is the more
important class: they made the rest look better tested than it was. The most
instructive is that a stress test written for the cancellation race passed on
the broken implementation, because the losing interleaving is a few
instructions wide and cannot be forced from outside the ABI. It was deleted
rather than kept, and the invariant is asserted directly instead. **A gate
should not be passed on evidence that would survive the bug.**

**One finding is worth carrying into any production ABI.** `domowik_buf` is
length-delimited and not NUL-terminated; the header says so, and the first C
caller broke the rule inside an hour, with AddressSanitizer catching the
heap-buffer-overflow. Documentation has been demonstrated insufficient for that
rule. The spike README records it as F1 with five others, including a build
configuration — `panic = "abort"` — that would silently void every panic guard
in the bridge with no test able to detect it.

## Evidence — the Flutter application (2026-09-04)

The bridge evidence above was gathered by harnesses that load the core from a
path handed to them on the command line. A real application has no such path,
so **bundle packaging** stood recorded as assessed rather than answered. It is
now answered for macOS, and the ADR still stays **Proposed**.

**What is now demonstrated.** A Flutter macOS application drives the Rust core
through `dart:ffi` from inside its own signed bundle: it builds a plan, draws
what the core returns, undoes, has a batch refused atomically with a reason,
and survives a deliberate Rust panic without the window going away — which is
what the `panic = "unwind"` profile decision exists for, now shown in a GUI
process rather than only in a test. The core is a `staticlib` linked into the
plugin framework, so Xcode embeds and signs it with an `@rpath` id; a cargo
`cdylib` could not be shipped this way, because its install name is the
absolute path of the machine that built it.

**Why this does NOT move the gate.** The gate above says "target Phase 2
platforms", and this ADR's own re-assessment names those as **Linux and
Android**. Neither has a host, an NDK, or an installed Rust target here.
macOS is a third platform that happens to be available, and a validated third
platform is not one of the two the gate asks for.

**macOS is a local-development target only** (ED-0099, owner, 2026-09-04), so
notarisation, Gatekeeper and the hardened runtime are out of scope for it
rather than owed. Those obligations still exist for whatever Domowik does
distribute on; they are not macOS's to carry, and this ADR's gate is unaffected
either way.

**The methodological point from the previous round held, and was needed
again.** Both new probes initially proved less than they claimed:
`DynamicLibrary.open` succeeding was reported as "loaded from inside the
bundle" when an identically named library elsewhere would satisfy it, and the
render probe counted loop iterations rather than drawing. They now resolve the
loaded image with `dladdr` and count rasterised pixels, and each was checked
against a negative control. Separately, the build wiring was found linking a
STALE Rust archive while both probes passed — caught by mutating
`ABI_VERSION` in the Rust source and watching the bundle report the old value.

**A gate should not be passed on evidence that would survive the bug** — and
that rule caught three more defects here, two of them in the evidence itself.


## Evidence — Linux (2026-09-12)

Linux is the first of the two platforms this ADR's gate actually names, and it
is now executed rather than assessed. The ADR still stays **Proposed**, because
the gate names two.

**What is now demonstrated.** The core builds as a native `aarch64` ELF shared
object and passes every check the macOS slice passes: 76 Rust tests, the C
harness under ASan+UBSan at 131 checks, and the Dart FFI harness at 97 checks,
with panic containment across `extern "C"` exercised on Linux rather than
inferred from macOS. A Flutter Linux application then builds, bundles the core
at `lib/libdomowik_bridge.so`, resolves it through the runner's `$ORIGIN/lib`
`RUNPATH`, and runs both bundle probes from inside the bundle, with `dladdr`
confirming the image loaded is the bundle's own copy. The bundle was copied
elsewhere and re-run, and reported loading from the new location.

**What Linux cost at the boundary: nothing.** The `cdylib` ships as cargo
produces it. ELF has no `LC_ID_DYLIB`, so the install-name defect that forced
the macOS slice into a `staticlib` inside a framework simply does not exist
here, and the packaging work was getting one file into one directory. That is
a result worth stating plainly, because the Go-era spike's Linux findings were
about a runtime that no longer exists and said nothing about this.

**Two constraints on how this evidence may be quoted.** The ABI slice runs in a
virtual machine, so its timings are not bare-metal Linux timings and are
recorded as a comparison rather than a measurement. The application slice runs
on emulated amd64, because Flutter publishes no stable Linux arm64 archive, so
it produces no timings at all. Loader behaviour, `RUNPATH` resolution, bundle
layout and the GTK runner are genuine Linux behaviour in both.

**The evidence was attacked before it was accepted.** Removing the plugin's
bundled-library declaration empties the bundle's `lib/` and fails the self-test
at its first check. Bumping `ABI_VERSION` and rebuilding without a clean
reports the new value, which asks the macOS stale-archive defect directly: the
CMake step is an always-run target precisely because an output-keyed one would
reproduce that defect and pass.

**Android followed the same day.** See the evidence section below.

## Evidence — Android (2026-09-12)

Both platforms this ADR's gate names have now been executed. **It still does
not become Accepted**, and the reason has moved rather than disappeared.

**What is now demonstrated.** A release APK on a named ARM64 AVD loads the
Rust core from inside the APK and passes the same 48-check bundle self-test
macOS and Linux run, the first-frame probe, and an Android-only lifecycle
probe that holds the open-plan count at 1 across rotation, pause and resume
and reaches zero handles and zero outstanding buffers at teardown in the same
process. The record is in `docs/evidence/0024-android/MEASURED.md`.

**A third packaging shape, and it is genuinely a third.** macOS needed a
`staticlib` inside a framework because a Mach-O dylib carries its build path
as its install name. Linux ships the `cdylib` untouched because ELF has no
such field. Android ships the `cdylib` too, but with `extractNativeLibs=false`
it is stored uncompressed in the APK and mapped straight out of it, so there
is no file on disk to name: the loaded image is
`<apk>!/lib/<abi>/libdomowik_bridge.so` and the application's native library
directory exists and is empty. Three platforms, three mechanisms, one ABI and
one Dart call site. That is the substantive thing this spike establishes about
the boundary, and it could not have been reached by reasoning from one
platform.

**Why the gate is still not passed.** It has two halves. The bridge half is
now answered on both named platforms. The other half asks for an editor
prototype meeting **measured input and frame-time goals**, and those numbers
belong to issue 0033, which has not run. The spike has paid what it owed; the
ADR is waiting on something else.

**What the Android evidence does not cover**, so it is not quoted further than
it goes: an emulator rather than a physical device, one ABI (`arm64-v8a`) by
deliberate choice rather than oversight, a debug-signed APK, and no test of
activity destruction and recreation. iOS remains untouched.

## Amendment — the C API is unnecessary for a Rust caller (2026-09-12, ED-0109)

This ADR's Proposed decision says the GPU layer is "consumed through its C
API", and its gates name "wgpu-native". Both were written when the core was Go
and the GUI plugin was not assumed to be Rust. **The status of this ADR does
not change: it remains Proposed.** What changes is which package issue 0025
links.

**The name is a trap.** The crates.io package called `wgpu-native` is version
0.5.1, published 2020-07-10, under **MPL-2.0** — a licence absent from
`docs/third-party/licenses.json` `allowedLicenses`, so the project's own
registry would reject it. A reader following this ADR by name lands on a
six-year-old weak-copyleft dependency in the distributed graph. The live
project, `gfx-rs/wgpu-native`, is Apache-2.0 and maintained, but it is a C API
wrapper around the `wgpu` crate, existing to serve callers that are not Rust.

**Ours is Rust.** Issue 0024 established that the GUI plugin is a cargo-built
library that packages on macOS, Linux and Android. Rust calling a C wrapper
around Rust would add a second FFI boundary inside one process, with its own
ownership, lifetime and panic rules, to satisfy an assumption that no longer
holds. Issue 0025 therefore depends on `wgpu` 30.0.1 (MIT OR Apache-2.0)
directly, per ED-0109.

**Nothing about the gates moves.** Which package is linked says nothing about
Flutter surface or texture integration, lifecycle handling, Linux and Android
packaging, callback and thread scheduling, GPU resource ownership, or measured
startup, memory and frame behaviour. Every one of those is still owed, and the
failure clause above still applies: a failed spike requires a new ADR, not a
hidden platform-specific divergence.

**One gate wording is now stale and is flagged rather than edited.** The gate
says "Linux and Android binary packaging without assuming upstream mobile
prebuilts". Depending on a Rust crate satisfies the *prebuilts* half by
construction, since cargo builds it from source for each target. The packaging
half is not satisfied and is not claimed to be.
