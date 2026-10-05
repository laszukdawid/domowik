# 0025: Spike Flutter-to-wgpu surface integration

- **Phase:** 3
- **Priority:** High
- **Status:** Open
- **Depends on:** 0024

## Problem

Validate a real Flutter rendering surface and native packaging; Dart FFI calls
to wgpu-native alone do not display a frame.

### Current integration assessment — 2026-10-05

Issue 0092 promotes the working application to `frontend/`; it is the place
to integrate a future GPU backend, not a second Flutter application.
The former `spikes/0025-flutter-wgpu-surface` held a Cargo manifest/lockfile
and placeholder `src/lib.rs`, with no rendering implementation to merge.
Issue 0093 removes that unused workspace, not the requirements below. This
issue returns to Open because implementation is not currently underway; no
criterion is completed by removing scaffolding.
The dated feasibility inventory below is a design reference, not execution
evidence on the current Linux host. The interface already has a working Linux
native GL texture backend; renderer-neutral scene payloads live in
`crates/domowik-scene`. A wgpu backend could reuse those payloads and the
frontend's view/control separation, but still needs its own texture interop,
lifecycle, ownership, licensing and platform tests. No new runtime dependency
is adopted by promoting the application.

## Acceptance criteria

- [ ] Linux and Android render into a Flutter-owned surface/texture with
      resize, pause/resume, device loss, and teardown handling.
- [ ] Native binaries are reproducibly built/packaged for target ABIs without
      assuming upstream mobile prebuilts.
- [ ] Callbacks, UI/render thread scheduling, shader errors, and GPU resource
      ownership are documented.
- [ ] Triangle, representative instanced room geometry, and sample splat shader
      have startup/memory/frame measurements on named devices.
- [ ] Exact tools/libraries/generated bindings, versions, provenance, direct
      and transitive licenses, notices, and distribution obligations are
      recorded.
- [ ] Evidence is added to ADR 0003; the Flutter choice remains Proposed until
      the measured editor-prototype gate also passes.


## Feasibility inventory — 2026-09-12

Inventory and plan only. **No implementation, no dependency adopted and
nothing downloaded**, pending review of the proposal below. Issue 0024's
dependency is satisfied at ABI 12; its ABI 13 re-run is owed but does not
block this inventory.

### The finding that shapes everything else: neither gate platform has a GPU here

0025's acceptance criteria name Linux and Android, and require
*startup/memory/frame measurements on named devices*. Both platforms are
reachable on this machine only through software rasterisers:

| Platform | How 0024 reached it | Graphics |
|---|---|---|
| Linux | Docker Desktop LinuxKit VM | **No `/dev/dri`**, no GPU passthrough. Mesa software rendering; the 0024 probes set `LIBGL_ALWAYS_SOFTWARE=1` |
| Android | AVD `domowik_0024_arm64` | **`-gpu swiftshader_indirect`**, SwiftShader software rasteriser |
| macOS | native | Real GPU, Apple M4 Max, Metal 4 — but macOS is a local-development target (ED-0099) and is **not** a platform ADR 0003's gate names |

So the platform with a usable GPU is the one that does not count, and the two
that count have no GPU. A triangle and a splat shader will very likely *render*
under software rasterisation, which means correctness, lifecycle, resize,
device loss and teardown are all testable here. **Frame-time and memory
measurements are not**, and a number measured on llvmpipe or SwiftShader is not
a weaker version of a GPU measurement, it is a measurement of something else.

This is an owner/hardware question, not something the spike can engineer
around, and it is raised now rather than after the work: 0024 spent its effort
discovering that evidence quality is where this project gets hurt, and a
measurement criterion that cannot be met should be renegotiated before code is
written, not after.

**Options, for the owner to choose between:**

1. Split the criterion: correctness, lifecycle and packaging validated here;
   measurement deferred to named hardware, recorded as owed. Cheapest, and
   honest, but leaves the ADR 0003 Phase 3 gate open on the same axis.
2. Provide a Linux machine with a GPU, and a physical Android device. Closes
   the criterion properly. Needs owner authority and hardware.
3. Re-scope the criterion to what a software rasteriser can support — a
   correctness and API-conformance gate rather than a performance one — and
   record the change as an executive decision.

### What is already on this machine, and costs nothing

- Rust 1.98.0 with `aarch64-linux-android` already installed (added by the
  Android work stream), plus both darwin targets.
- Android SDK, **NDK 28.2.13676358**, platform-tools, emulator and the AVD
  `domowik_0024_arm64`, preserved at `/private/tmp/domowik-android-0024/sdk`
  after the Android work stream ended. 6.6 GB already on disk.
- Flutter 3.41.6 with engine artifacts for `android-arm64` and the desktop
  targets.
- The Linux container images from 0024, including the clean-machine image.
- `spikes/0024-flutter-rust-bridge/` itself: a working Flutter application,
  plugin packaging for macOS, Linux and Android, and probe harnesses that
  already assert bundle provenance. 0025 should extend this rather than start a
  second application.

### The real technical risk is texture interop, not wgpu

`wgpu-native` is Rust, so cross-compiling it for `aarch64-linux-android` and
for Linux is the same cargo problem 0024 already solved, and the criterion
"without assuming upstream mobile prebuilts" is satisfied by construction.
That part is not where this spike will get into trouble.

The difficulty is that **Flutter owns the surface and wgpu wants to own a
texture**, and the two use different graphics APIs on both gate platforms:

- **Android.** Flutter renders with Impeller (Vulkan, GL fallback); wgpu on
  Android targets Vulkan or GLES. Handing a wgpu-rendered image to Flutter
  means either a `SurfaceTexture` through `TextureRegistry`, or an
  `AHardwareBuffer` shared between the two. Both are real mechanisms; neither
  is a function call.
- **Linux.** The GTK embedder is EGL/GL; wgpu targets Vulkan. Sharing needs
  dmabuf or an `EGLImage`, or a readback path that copies every frame, which
  works and is slow and would poison exactly the measurements above.

Two honest possibilities have to be held open: that the interop path is
different enough per platform that the "surface bridge" is two bridges, and
that a readback fallback is the only thing that works everywhere. ADR 0003's
failure clause already says a failed wgpu spike requires a new ADR rather than
a hidden platform-specific divergence, and this is the shape such a divergence
would take.

### Dependency and licensing, for review before anything is fetched

Nothing below is adopted yet.

| Candidate | Licence | Distributed? | Note |
|---|---|---|---|
| `wgpu` / `wgpu-native` | MIT OR Apache-2.0 | **Yes**, linked into the GUI | Permissive, compatible with Apache-2.0. Its transitive tree is large and each licence needs recording, which is the real ledger cost |
| `raw-window-handle`, `ash`, `naga`, `gpu-allocator` etc. | mostly MIT/Apache-2.0 | Yes, transitively | Pulled in by wgpu; the count is the issue, not any one licence |
| Android NDK | already provisioned | No | Build-time only, already in the 0024 ledger |

**Correction (root, 2026-09-12).** An earlier draft of this section claimed
wgpu would be the first distributed third-party dependency tree the project
takes on. That is wrong, and checking it takes one look: `docs/RUST-
DEPENDENCIES.md` records 127 lines of cargo dependencies that ship in the
binaries, `docs/third-party/assets.json` records a distributed Noto Sans font
whose OFL text `scripts/rust-notice.py` reproduces into NOTICE with
`verify.sh` diffing the result, and `licenses.json` carries an
`allowedLicenses` registry. The Rust runtime libraries and the Flutter engine
already ship too. `tools.json` is a **dev-tool ledger, not the dependency
inventory**, and reading it as the latter is what produced the error.

So the machinery exists and wgpu goes through it rather than inventing it.
What is genuinely new is scale: wgpu pulls a large transitive tree into the
DISTRIBUTED graph, and the per-crate licence recording that `verify.sh`
already enforces is the real cost. The runtime inventory and NOTICE work is
issue 0036's, and is to be coordinated with that owner rather than duplicated
here.

**Download estimate, not yet incurred:** the `wgpu` crate tree is roughly
200-400 MB of sources and registry cache, and several GB of build artefacts
per target once compiled for three platforms. Nothing will be fetched without
a scoped estimate approved first.

### Proposed order of work, once reviewed

1. Settle the measurement criterion (options 1-3 above). It changes what the
   spike is for.
2. Record the wgpu dependency decision, including the distributed-dependency
   question, before adding a single crate.
3. Prove the interop path on ONE platform end to end, with a triangle, and
   pick the platform by which interop mechanism is riskiest rather than
   easiest.
4. Only then the second platform, the instanced geometry, and the splat
   shader.

Steps 1 and 2 are owner or root decisions. Nothing in 3 or 4 starts before
they are answered.

## Direction from root — 2026-09-12

- **Functional and lifecycle work proceeds under the software renderer.**
  Correctness, resize, pause/resume, device loss and teardown do not need a
  GPU and are not gated on one.
- **The hardware timing gate stays OPEN.** Software-rasteriser timings are
  never relabelled as GPU evidence, and no frame or memory number from
  llvmpipe or SwiftShader is presented as satisfying the measurement
  criterion.
- **Per-frame readback is not acceptable as the final architecture** without a
  measured budget. It may exist only as an explicitly labelled diagnostic
  path, so that the interop question stays answered rather than bypassed.
- **Before anything is fetched**, a proposal goes to root carrying exact
  candidate versions, licences and provenance, plus the minimal surface-API
  experiment: who owns the texture, how synchronisation works, and how many
  copies a frame makes. The 200-400 MB sources and several-GB artefacts
  estimate is within feasible disk scope but is **not** blanket licence
  approval.
- **Open question put back to the owner:** whether a macOS proof is worth
  doing first to reduce interop risk cheaply, on the explicit understanding
  that it fulfils NEITHER gate platform. macOS is the only real GPU here, so
  it is the only place the interop mechanism can be exercised against actual
  hardware at all — but ED-0099 makes it a local-development target and ADR
  0003's gate does not name it, so it can inform the design and can never
  close the criterion.


## Dependency and licence proposal — 2026-09-12

For review before anything is fetched. Versions and licences below are from
registry and repository METADATA only; no crate has been downloaded.

### The headline: ADR 0003 names a package that should not be used

ADR 0003 says wgpu-native, "consumed through its C API". Two facts make that
wrong now, and neither is a matter of taste.

| Candidate | Version | Licence | Verdict |
|---|---|---|---|
| `wgpu` (crates.io) | **30.0.1**, published 2026-08-22 | **MIT OR Apache-2.0** | **Proposed.** Current, permissive, both arms on the allowed list |
| `wgpu-native` (crates.io) | 0.5.1, published **2020-07-10** | **MPL-2.0** | **Rejected.** Six years stale, and MPL-2.0 is NOT in `licenses.json` `allowedLicenses` (`Apache-2.0, BSD-2-Clause, BSD-3-Clause, CC0-1.0, ISC, MIT, Zlib`) |
| `gfx-rs/wgpu-native` (GitHub) | active, pushed 2026-09-08 | Apache-2.0 | Allowed, but it is a C-API wrapper around `wgpu` |

The crates.io package named `wgpu-native` is not the thing ADR 0003 means. The
thing it means is the GitHub project, which is a **C API wrapper around the
`wgpu` crate** — and the reason that wrapper exists is to let non-Rust callers
use wgpu.

**Our caller is Rust.** The GUI plugin is already a Rust `cdylib`/`staticlib`
built by cargo, proved on three platforms by issue 0024. Going Rust → C API →
Rust would add a second FFI boundary inside our own process, with its own
ownership and lifetime rules, purely to satisfy a sentence written when the
core was Go and the plugin was assumed not to be Rust. ADR 0003's C-API
phrasing predates ADR 0008 exactly as its "Flutter-to-Go" phrasing did.

**Proposal: depend on the `wgpu` crate directly and record ADR 0003 as
re-assessed on this point.** Not a rejection of wgpu; a rejection of an
indirection the ADR assumed was necessary. If the owner wants the C API kept
for a future non-Rust consumer, that is a product-direction call and the
reason should be written down, because it is not free.

### What still has to be recorded, and with whom

`wgpu` pulls a large transitive tree into the DISTRIBUTED graph. `verify.sh`
already fails the build when a dependency is added without a
`docs/RUST-DEPENDENCIES.md` record, so the enforcement exists; the cost is the
per-crate licence review, and the count is the work. Any crate whose licence
is outside `allowedLicenses` is a stop-and-review, not a judgement call.

This is coordinated with **issue 0036**, which owns the runtime inventory and
NOTICE machinery. I do not duplicate that generator or its fixtures.

### The minimal surface experiment, before any of it

Three questions decide the architecture, and they are answerable with a
triangle:

1. **Who owns the texture?** Flutter's `TextureRegistry` handing wgpu a
   surface, or wgpu rendering to its own texture that Flutter samples.
2. **How is it synchronised?** A GPU fence or semaphore shared across the two
   APIs, or a CPU round trip. The answer differs per platform and is the part
   most likely to force two bridges instead of one.
3. **How many copies does a frame make?** Zero-copy import, one blit, or a
   full readback. Per root, a per-frame readback is a labelled DIAGNOSTIC
   only, never the architecture, and it may not be adopted without a measured
   budget.

Per root, the macOS interop diagnostic is authorised as bounded risk reduction
once this proposal is reviewed. It answers 1-3 against real hardware, which is
otherwise impossible here, and it closes NEITHER gate platform.

### Download estimate, still not incurred

Roughly 200-400 MB of crate sources and registry cache; several GB of build
artefacts once compiled per target. Root has confirmed this is within disk
scope and that it is **not** blanket licence approval.


## Transitive licence audit — 2026-09-12 (ED-0109 condition, before linking)

Resolved with `cargo metadata --filter-platform aarch64-apple-darwin` against
`wgpu = { version = "30.0.1", default-features = false, features = ["wgsl"] }`.
**Sources fetched; nothing built and nothing linked**, which is the order
ED-0109 requires.

**The tree is 50 crates, not the hundreds the estimate feared.** Turning
default features off and naming only `wgsl` is most of why. The 200-400 MB
figure was an upper guess made before resolution and should not be quoted
again now that a real number exists.

**Licence result: one crate fails the allowed list, and it is already shipping.**

| Crate | Licence | Status |
|---|---|---|
| 49 of 50 | `MIT OR Apache-2.0` and similar, plus one each of `Zlib`, `ISC`, `Apache-2.0`, `MIT` | Satisfied by `allowedLicenses` |
| `unicode-ident` 1.0.24 | `(MIT OR Apache-2.0) AND Unicode-3.0` | **Not satisfied** — `Unicode-3.0` is absent from `allowedLicenses` |

`unicode-ident` is **not something wgpu drags in for the first time**. It is
already recorded in `docs/RUST-DEPENDENCIES.md` line 108 as `runtime`, meaning
already distributed, at the same version and the same licence, and its licence
text is already reproduced in `NOTICE` at line 13846.

So **issue 0025 adds no new licence exposure whatsoever**. What the audit found
is a pre-existing inconsistency that belongs to nobody in this issue: a crate
requiring `Unicode-3.0` ships today, while `licenses.json` `allowedLicenses`
does not list `Unicode-3.0`. Either the allowed list is incomplete or that
crate should not be shipping, and both readings are decisions for issue 0036
and root rather than something this spike resolves or silences.

### The audit tool was wrong first, and the fix is the interesting part

The first pass split on `OR` before handling parentheses, so
`(MIT OR Apache-2.0) AND Unicode-3.0` matched its first arm `(MIT` and was
reported as SATISFIED. It also reported `Apache-2.0/MIT` as a failure, because
it did not know the legacy cargo slash syntax means `OR`.

One false pass and one false stop, in the one tool whose entire job is to stop
the work. The replacement is a real expression parser with a self-test of eight
cases carried in the script, **including both cases the first version got
wrong**, and it refuses to run if any of them regress. An auditor that is not
itself tested is the same defect this project has now found in its evidence
four separate times.


## The licence audit above was against the WRONG REGISTRY — corrected 2026-09-12

Issue 0036's owner caught it and they are right. Verified before accepting:

- The list the audit used, `allowedLicenses` in
  `docs/third-party/licenses.json`, is the **Go-era registry**. Nothing in the
  Rust path reads it — `grep -rln licenses.json scripts/*.py scripts/*.sh`
  returns nothing.
- The list actually **enforced** is `ALLOWED` in
  `scripts/rust-deps-inventory.py`, 17 entries, and `verify.sh` fails the
  build on any crate outside it.

Two consequences, and the first retracts a claim made above.

**`unicode-ident` is NOT a problem and never was.** Its licence,
`(MIT OR Apache-2.0) AND Unicode-3.0`, is literally in the enforced list. The
"pre-existing inconsistency" reported earlier does not exist; it was an
artefact of comparing against a registry no longer in force. That claim is
withdrawn rather than edited away.

**The enforced gate is EXACT STRING MEMBERSHIP, not SPDX evaluation.** The
check is `if lic not in ALLOWED`. So the careful expression parser written
above models the wrong thing entirely: semantics never enter into it, and two
licence strings that mean the same thing are different values.

### The real result, across all four ship triples

Resolved with `cargo metadata --locked --filter-platform <triple>` for
`aarch64`/`x86_64` × `apple-darwin`/`unknown-linux-gnu`. 50 crates on macOS, 51
on Linux. **Three are outside the enforced list, the same three on all four:**

| Crate | Licence string | Why it fails |
|---|---|---|
| `libloading` 0.8.9 | `ISC` | ISC is genuinely absent from the enforced list. A permissive, BSD-style licence, but absent |
| `raw-window-handle` 0.6.2 | `MIT OR Apache-2.0 OR Zlib` | The list has `MIT OR Zlib OR Apache-2.0`. **Same three licences, different word order** |
| `rustc-hash` 1.1.0 | `Apache-2.0/MIT` | The list has `MIT/Apache-2.0`. **Same two licences, different word order** |

Only ONE of the three, `libloading`, raises a licence question at all. The
other two are the exact-string gate rejecting spellings of expressions it
already permits, which is worth reporting to issue 0036 as a brittleness in
the guard rather than a finding about wgpu: a dependency can fail the build
because an upstream author wrote the same dual licence in a different order.

None of this is decided here. Admitting `ISC`, or normalising the comparison,
is issue 0036's registry and root's call. The filtered metadata JSON for all
four triples has been handed to 0036 in the shape they asked for.
