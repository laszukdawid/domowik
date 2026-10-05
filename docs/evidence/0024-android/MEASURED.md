# Issue 0024, Android slice — 2026-09-12

Android was the largest thing issue 0024 had recorded as assessed-and-deferred:
the machine had no JDK, no SDK, no NDK, no emulator and no Android Rust target,
so the platform ADR 0003's gate actually names had never been executed. It has
now been executed. This records what ran, on what, what it measured, and what
is still not true.

Nothing here is simulated. Every number comes from a run on the named virtual
device, and the one check that fails is named rather than rounded off.

## What was provisioned

All of it lives under `/private/tmp/domowik-android-0024`, outside the
repository and outside the user's home. No global installation, no shell
profile change, nothing privileged. Total 8.2 GiB.

Exact versions, artifact checksums, licences and the SDK licence acceptance are
recorded in [docs/third-party/toolchain-android.json](../../third-party/toolchain-android.json).
In short: Temurin JDK 21.0.12.1+1, Android command-line tools 23.0, platform
android-36, build-tools 36.0.0, NDK 28.2.13676358, emulator 37.1.11, and the
**AOSP** `arm64-v8a` system image rather than a Google APIs one, so no Google
Play terms are drawn in. `rustup target add aarch64-linux-android` against the
toolchain `rust-toolchain.toml` already pins is the only change outside that
directory.

## The device

| | |
|---|---|
| AVD | `domowik_0024_arm64`, pixel_6 profile |
| System image | AOSP `android-36`, `default`, `arm64-v8a` |
| Reported | Android 16, API 36, `ro.product.cpu.abi = arm64-v8a` |
| Emulator | 37.1.11, headless (`-no-window`), software GL |
| Host | Apple M4 Max, macOS 26.6 |

It is an emulator. The ARM code runs natively on an ARM host rather than under
translation, but **no claim is made here about any physical device**: a phone
has a different CPU, thermal envelope, GPU and storage, and nothing below was
measured on one.

## How the core gets into the app, and why it differs from macOS

macOS could not ship the cargo `cdylib`, because its `LC_ID_DYLIB` is the
absolute path of the build machine; the crate is built as a `staticlib` and
linked into the plugin's framework instead. **Android has no such problem.** An
ELF shared object is resolved by soname against the app process's own
native-library search path, so the `cdylib` is packaged unchanged and opened by
bare name:

```dart
DynamicLibrary.open('libdomowik_bridge.so')
```

`flutter_plugin/android/build.gradle` runs cargo for each configured ABI and
copies the result into the library's `jniLibs`, so AGP packages it like any
other native library. Three properties are asserted by
`scripts/android-probe.sh` rather than assumed, because each is the kind that
silently stops being true:

- the APK contains **exactly one** ABI directory,
- `lib/arm64-v8a/libdomowik_bridge.so` is present,
- and it is **Stored**, not deflated, which is what lets the platform map it
  straight out of the APK.

That last one is why the image path the probes report contains `base.apk!/`:
AGP sets `extractNativeLibs="false"`, so the library is never unpacked to a
directory at all.

## The staleness question, answered by experiment

The macOS slice shipped a bundle containing a Rust archive from an earlier
build, and **both of its self-verifying probes passed on it**. That is the
defect class this build graph had to be tested against, not reasoned about.

`scripts/android-probe.sh --mutate-abi` sets `ABI_VERSION` to 99 in the Rust
source, rebuilds, reinstalls, and requires the installed app to report 99. It
does, so the Gradle-to-cargo-to-`jniLibs`-to-APK chain cannot serve a stale
core. The script restores the source afterwards and checks the restored file's
hash against the original, so a mutation cannot escape into a commit.

The Gradle task is deliberately `outputs.upToDateWhen { false }`. Declaring the
crate's sources as Gradle inputs would work until somebody edited a core crate
outside the declared set, and that failure is silent. Cargo already knows what
changed, and a warm no-op costs about a second.

## Probe results

Four probes run inside the installed **release** APK. The first two are the
shared ones the macOS bundle runs; the last two are Android's own, because they
ask questions no other platform raises.

| Probe | Result |
|---|---|
| Bundle self-test | **48 checks, 0 failures** |
| First-frame probe | **PASSED** — 4 walls in the document, 4 drawn by the painter, 57,788 non-background pixels rasterised, 0 outstanding buffers |
| Lifecycle (Android-only) | **PASSED** |
| Benchmark (Android-only) | **PASSED**, leak counters zero |

All four are green as of the integration of the shared loader and provenance
helper. Until that landed, the self-test stood at 48 checks and **one** failure
and the first-frame probe failed with it, both on the single provenance
assertion described below; that is recorded rather than erased, because the
slice spent most of its life in that state and the reason is the interesting
part.

**The check that was failing, and what fixed it.** `bundleRoot()` in `selftest.dart`
derives a `.app` root from `Platform.resolvedExecutable`, three levels up from
`Contents/MacOS`. On Android `Platform.resolvedExecutable` is
`/system/bin/app_process64` — the zygote image, which has nothing to do with
this app — so the function returns null and the provenance assertion fails.
Everything it guards is otherwise satisfied: `dladdr` names the loaded image,
and the image is inside this app's own APK. Both observed paths, from two
separate installs:

```text
/data/app/~~0WqORK6wDoVfGAriRA4UWQ==/invalid.domowik_spike.domowik_spike_app-imX9IJ3SdsU2HvW6f7auWw==/base.apk!/lib/arm64-v8a/libdomowik_bridge.so
/data/app/~~F2Io2gRC_NpZ0T3-FLj8NQ==/invalid.domowik_spike.domowik_spike_app-IIBpR0DAEA7NWdjAMs_cuA==/base.apk!/lib/arm64-v8a/libdomowik_bridge.so
```

Both base64-looking segments change on every install, so neither can be pinned.
The helper is a shared cross-platform concern owned by the Linux/platform work,
so it was NOT patched here and no known-failure allowance was added — an
allowance is how a suite quietly goes green on a defect. Instead the observed
strings were handed over and the platform side landed a
`platformProvidedRoot` seam that takes the APK path the runner publishes. The
comparison itself is unchanged and has no Android branch.

Everything else in the self-test passes from inside the APK: ABI 12, the clean
Level 1 baseline, derived geometry measured to the inner faces, hit-test
ordering with endpoints outranking regions, a zero-length wall of negative
thickness refused atomically with a reason, undo removing exactly one wall, a
Rust panic contained with the plan still usable afterwards, a save-and-reopen
round trip comparing derived geometry, and both leak counters at zero.

## Lifecycle

Android may stop, destroy and recreate the thing holding a native handle, which
no desktop window does. A plan handle is a registry entry only
`domowik_plan_close` removes (finding F6), so an app that leaks an activity
leaks a document. The probe holds a live plan, re-drives the core after every
transition, and reads the bridge's own counters rather than tracking anything
in Dart.

| Transition | Driven by | Observed |
|---|---|---|
| Rotate to landscape | `settings put system user_rotation 1` | metrics 1080×2400 → 2400×1080 |
| Rotate back | `user_rotation 0` | metrics 2400×1080 → 1080×2400 |
| Pause | `input keyevent KEYCODE_HOME` | inactive → hidden → paused |
| Resume | `am start` | hidden → inactive → resumed |
| Teardown | probe closes its own handle | `close=0 openPlans=0 buffers=0` |

`openPlans` stayed at exactly 1 across every transition and every `execute`
returned 0, so the handle kept working rather than merely continuing to exist.
The process ID was identical before and while backgrounded, so nothing was
being reported about a replaced session.

**Rotation is part of the verdict, and that was proven by a negative control.**
The probe requires two distinct physical sizes before it can pass. Run with a
pause and a resume but no rotation, it reports
`LIFECYCLE PROBE FAILED: … distinctSizes=1`; run with the rotation, it passes.
The earlier version would have passed both, because nothing in its verdict
depended on a rotation having happened — it required only that some transition
had been seen, which is a condition the code reaches `finish` by observing
anyway.

One thing this does **not** establish: rotation never destroyed the activity,
because the manifest declares `configChanges` covering orientation and
screenSize, so Android hands the change to the running activity. Destruction
and recreation of the activity — and whether a handle survives that — is not
tested here, and forcing it with the "don't keep activities" developer option
is owed work, not a closed question.

## Measurements

Release profile, on the emulated device named above. Medians of repeated
rounds, with the spread shown, because a single figure from a virtual device
hides how much scheduler noise is in it.

| What | Value |
|---|---|
| Nullary export, from Dart | 7–8 ns (min 7, max 29, n=21 per run) |
| Dart's own JSON encoding, no boundary | 1,987–2,335 ns |
| One operation at a time, over a plan growing to 100 | 40,770–46,340 ns |
| 20 operations per call, same work | 7,090–7,990 ns/op |
| Clean build (`flutter clean`, then build) | 44 s |
| Incremental rebuild | 2–3 s |
| Release APK | 18,672,066 B |
| Rust core, as cargo builds it | 2,612,904 B |
| Rust core, as AGP packages it | 1,947,336 B |

Ranges are across separate runs of the whole probe, not one round, because a
single figure from a virtual device hides how much of it is scheduler noise.

**The machine was not quiet.** These were taken while several other agents were
building and testing in other worktrees on the same host, which
`docs/evidence/0047-hit-ordering/MEASURED.md` had to say about its own numbers
for the same reason.

Every row here is therefore a **contended observation**: a number that was
seen, under load that was neither controlled nor measured. They are not upper
bounds — nothing in this run establishes a bound — and the contention is not
separable from the emulation. The spread says as much on its own: the nullary
row reaches 29 ns against a 7 ns median and the single-operation row reaches
107 µs against roughly 44 µs.

No ratio taken from these rows is treated as though the contention cancelled.
Both sides of any ratio were observed under load, and that they met the SAME
load is an assumption this run did not test. The measurement probe is behind
`--benchmark` so a future run can be taken on an idle machine, and that run,
not this one, is where a performance claim would come from.

None of the correctness results depend on any of this.

The nullary row is the boundary floor and is **not the cost of anything**. The
probe accumulates and prints the returned values (`nullary_checksum 5064000`,
which is 12 × 422,000 for 422,000 calls), so "the compiler might have removed
the call" is settled by arithmetic rather than argued about.

The third row is named for what the loop does rather than for the interactive
case it stands in for: history is empty at the start of each round and 100
operations deep at the end, so the figure averages over that growth and is not
a steady-state cost at any one depth. An earlier draft labelled it "short
history", which overstated it.

**Batching is worth about 5.8x here** (≈41 µs against ≈7.1 µs per operation),
against ~6.6x measured on macOS for the same reason: it amortises per-batch
core work, not the boundary hop, because the hop was never the cost.

**These do not compare to the macOS table, and no comparison is drawn.** The
macOS figures were taken through a JIT desktop Dart harness; these come from an
AOT release build on a virtual ARM device. Two different runtimes on two
different machines do not make a benchmark — the same reason this spike already
refuses to compare its Rust numbers against the retired Go ones.

The packaged core is 665,568 B smaller than the one cargo produces, because AGP
strips it during packaging.

## Three Android-only defects, each found by running rather than by thinking

1. **The first probe run exited in complete silence.** Android points an app's
   file descriptors 1 and 2 at `/dev/null`. The `log.redirect-stdio` system
   property does not help: it replaces Java's `System.out` and `System.err`,
   while the Dart VM writes to the descriptors directly. The process started,
   ran, exited cleanly, and produced not one line.
2. **The fix for that lost the verdict.** Repointing the descriptors at a pipe
   drained into logcat worked for most of a run and then dropped the last few
   lines every time — including the banner, which is the only line that
   matters. A probe ends with `exit()`, which tears the process down at once,
   and the thread draining the pipe lived *inside* that process. Probe output
   now goes to a file, which has no reader to lose. A truncated probe log is
   the worst available failure, because it is indistinguishable from a hang.
3. **`Directory.systemTemp` is `/tmp`, and an Android app cannot write it.**
   The shared self-test's save-and-reopen round trip died there with an
   uncaught exception and took the verdict with it. The runner now sets
   `TMPDIR` to the app's cache directory before the engine starts, so the
   *shared* probe needs no Android special case at all.

A fourth, in this slice's own first draft: the app's `build.gradle.kts` used
`ndk { abiFilters }` with a comment claiming it restricted the APK to arm64.
The built APK disproved it — Flutter packaged its own engine for three ABIs
while the Rust core was in one, which is worse than not filtering, because such
an APK installs on a device the core was never built for and fails at
`DynamicLibrary.open`. The ABI is now chosen where Flutter actually decides it,
with `--target-platform android-arm64`, and the script asserts the result.

A fifth, in the script rather than the app: `unzip -l … | grep -q` under
`pipefail` reports failure when the grep *matches*, because the early exit
kills `unzip` with SIGPIPE. The first run of the packaging assertions failed on
a correct APK. Listings are now taken once into variables.

## Where the library actually lives, which is not where it looks like it lives

The provenance check needs a root to compare `dladdr`'s answer against, and on
Android no root can be derived: `Platform.resolvedExecutable` is the zygote, and
the install directory carries the package name plus an installation id that
changes on every install. Only the platform side knows it, so the runner
publishes it with `Os.setenv` before the engine starts — the same mechanism that
supplies `TMPDIR` — and the shared `platformProvidedRoot` seam consumes it
without Android needing a branch in the comparison itself.

Two paths are published, because the obvious one is wrong. All three strings
below come from ONE run on ONE install, so the prefix relation is directly
checkable rather than assembled from separate runs:

```text
APK    /data/app/~~dpTArVC25hKepTsix9qvCA==/invalid.domowik_spike.domowik_spike_app-TWu4w0_XfdAQeSKztybDoA==/base.apk
libdir /data/app/~~dpTArVC25hKepTsix9qvCA==/invalid.domowik_spike.domowik_spike_app-TWu4w0_XfdAQeSKztybDoA==/lib/arm64
image  /data/app/~~dpTArVC25hKepTsix9qvCA==/invalid.domowik_spike.domowik_spike_app-TWu4w0_XfdAQeSKztybDoA==/base.apk!/lib/arm64-v8a/libdomowik_bridge.so
```

`image.startsWith(APK)` is **true**. `image.startsWith(libdir)` is **false**.

**The native library directory exists and is empty.** `ls` on it shows only `.`
and `..`; nothing is ever extracted into it, because `extractNativeLibs` is
false and the loader maps the library straight out of the APK. It is the
obvious thing to write a predicate against and it would fail on a correctly
packaged APK. Comparing against the APK path is also the tighter assertion: it
proves the library came out of *this APK file*, not merely out of this
directory.

The values are printed by the **consumer**, in Dart, not by the producer. Two
attempts to report them from Kotlin produced nothing on a real run — `println`
because Android discards Java's stdout, and a direct write to descriptor 1
because whatever `FileDescriptor.out` refers to in an app process is not the
descriptor `dup2` repointed. Both were removed rather than left looking like
they report something. A value the sender believes it sent is not evidence.

## Review

The adversarial round required by `INSTRUCTIONS.md` — Codex on `gpt-5.6-sol` at
high reasoning effort — **did not produce an adversary, twice.**

The first invocation terminated with `ERROR: Your workspace is out of credits.`
before reading anything. The second, run later against the four Android files
after a trivial prompt confirmed credits had returned, started with the correct
banner, read all four files, spent 33,173 tokens, and terminated on the same
error before writing a single finding. Both banners are recorded. No findings
were returned and none are invented here: the workspace has enough credit for a
toy prompt and not enough for a review, and refilling it is an owner action.

A failed advisory round is not permission to skip the scrutiny, so the four
questions put to the advisor were answered against this code directly. Four
defects came out of it, all in files added by this slice, all fixed:

1. **The lifecycle verdict did not depend on a rotation.** A harness that never
   rotated produced a PASSED run reported as evidence that rotation was
   survived. Now two distinct physical sizes are required, with the negative
   control above.
2. **An assertion that could not fail.** The same verdict required
   `_seen.isNotEmpty`, evaluated at a point the code only reaches by having
   observed transitions. Replaced with the explicit pause-and-return round trip.
3. **The per-transition health check was printed, not checked.** `_report`
   re-drove the core after every transition and reported the status code while
   the verdict ignored it, so a transition after which every call failed would
   still have passed. Failures are counted now and gate the verdict.
4. **The redirected file descriptor was leaked.** `Os.dup2` onto 1 and 2 left
   the original open for the life of the process; it is closed.

A fifth was a labelling error rather than a defect, and is described under
Measurements.

The second round's question about `--mutate-abi` restoring a tracked source on
every exit path was answered the same way, and it is worth recording what did
**not** turn out to be true. An `EXIT`-only trap was expected to miss `SIGINT`.
Tested rather than assumed — two scripts, one trapping `EXIT` and one trapping
`EXIT INT TERM`, both sent `SIGINT` — and **both restored**, so that defect does
not exist in this shell and is not claimed. The real hole is the one no trap can
cover: a killed process leaves `ABI_VERSION` at 99, and the only symptom on the
next run is a single self-test line that reads exactly like the known provenance
failure. The script now refuses at startup to build from a modified bridge
source, naming the file and the restore command. It refuses rather than repairs,
because a script that silently rewrites a tracked source file is worse than one
that stops.

## Two things the integration run established that the earlier runs could not

**The build cannot be fooled by a hostile cargo environment.** The Linux side of
this spike was found to bundle a stale library because its build script ran
plain `cargo build` and then copied unconditionally, so an inherited
`CARGO_TARGET_DIR` sent cargo somewhere else while the copy picked up an old
file — ABI 77 built, ABI 12 shipped, self-test passing. The Gradle path here is
the same shape, so it was tested rather than reasoned about: with
`CARGO_TARGET_DIR` pointed at a scratch directory and `CARGO_BUILD_TARGET` set
to `x86_64-apple-darwin`, the APK still received a freshly built aarch64 Android
object, the hostile directory received nothing, and `--mutate-abi` still
propagated ABI 99 end to end. The task passes `CARGO_TARGET_DIR` explicitly in
the environment it hands cargo, and `--target` on the command line outranks
`CARGO_BUILD_TARGET`.

**The mutation test used to litter the source tree.** `sed -i'' -e` is read
differently by BSD and GNU sed, and the BSD reading left a `src/lib.rs-e` file
behind — in a script whose entire purpose is to leave the tree as it found it,
and which the repository's own clean-tree stage would have failed on. Found by
running it and looking at `git status` rather than at the script. It uses
`perl -pi` now, and the tree is verified spotless after the run.

## What is still not true

- **One ABI.** `arm64-v8a` only. armeabi-v7a and x86_64 are not built and not
  run. This is deliberate: the macOS slice carries a standing caveat about an
  x86_64 slice that was built and never executed, and adding unexecuted
  artifacts here would buy a second one.
- **No physical device.** Emulator only.
- **Activity destruction and recreation is untested**, as described above.
- **The APK is signed with the auto-generated debug key.** For the reason
  ED-0099 gives about macOS, this is a development target; there is no signing
  identity and no store. If Android becomes a distribution target, a real key
  is an owner decision and several ledger rows become distributed dependencies.
- **Reading probe output needs `adb root`.** From Android 11 the shell cannot
  read another app's external files directory, and a release APK is not
  debuggable, so `run-as` is unavailable. This is a property of the harness on
  an emulator, not of the app, which writes to its own storage either way.
- **File open and save are not wired on Android.** The editor's dialogs go
  through a method channel implemented in Swift for macOS; on Android the call
  fails and the app reports it as a failed file action. The core's own
  `plan_save` and `plan_open` are exercised by the self-test against real paths
  and work, so this is a missing platform channel and not a boundary question.

## Reproducing

```sh
export ANDROID_HOME=/private/tmp/domowik-android-0024/sdk
export JAVA_HOME=/private/tmp/domowik-android-0024/jdk/jdk-21.0.12.1+1/Contents/Home
export PATH="$JAVA_HOME/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"

emulator -avd domowik_0024_arm64 -no-window -no-audio -no-boot-anim &
./scripts/android-probe.sh                # build, install, every probe
./scripts/android-probe.sh --mutate-abi   # prove the APK cannot go stale
```
