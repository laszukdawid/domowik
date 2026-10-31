# ADR 0012: The editor in a web browser

- **Status:** Accepted
- **Date:** 2026-10-12
- **Related:** ADR 0003 (Flutter frontend, Proposed), ADR 0008 (Rust core)

## Context

The owner requires the editor to run in a browser. The native editor reaches
the Rust core through `dart:ffi` and a C ABI, opens and saves plans by
filesystem path, keeps preferences in a file, imports models from directories,
runs asset work on background isolates, and shows 3D through platform
textures (Metal on macOS, GL on Linux). None of that exists in a browser.
The decision is how to keep **one** implementation of document, editing,
file and rendering semantics while replacing every native-only seam.

## Decision

### 1. The core is the same C ABI, compiled to `wasm32-wasip1`

`frontend/native` (`domowik-frontend-bridge`) builds unchanged for
`wasm32-wasip1`; its 36 exports are the native ABI plus two allocation calls.
The Dart host (`lib/core/web_core.dart`) instantiates it with
`WebAssembly.instantiate` and calls the exports directly. There is no
JavaScript binding generator in the core.

WASI is used only as the standard library's portable system interface: the
host implements `clock_time_get` (`Date.now`, `performance.now`),
`random_get` (`crypto.getRandomValues`), `fd_write` for stdout/stderr (console)
and environment queries answering "empty". Every filesystem call returns
`EBADF`/`ENOSYS`, so no code path can reach a file. Rejected alternatives:
`wasm32-unknown-unknown` with `wasm-bindgen`, which panics on
`SystemTime::now` (used by `Uuid::now_v7` across the core) unless every caller
changes or `uuid`'s `js` feature pulls JavaScript bindings into the core; and
one combined core-plus-renderer module, which would ship GPU code to a page
that never opens 3D.

The editor reaches either host through `DomowikCore`
(`lib/core/domowik_core.dart`): strings and bytes in, status and strings or
bytes out. `FfiCore` and `WebCore` implement it over the same exports, and
conditional imports choose one, so `dart:ffi`, `dart:io` and `dart:isolate`
never enter the browser compile.

### 2. Ownership, memory and numbers

The host cannot allocate inside the module, so ABI 16 adds
`domowik_web_alloc(len)`/`domowik_web_free(ptr, len)` (8-byte aligned,
browser build only). Requests are copied in, borrowed for one call, and freed;
responses are the usual token-owned `domowik_buf`, copied out and released
with `domowik_buffer_free` before a `WebCore` method returns, exactly as
`FfiCore` does. `u64` parameters cross as JavaScript `BigInt`; `u64` fields
are read as two `u32` halves because dart2js has no 64-bit `ByteData`.
Handles, tokens and revisions stay far below 2^53. Memory views are
re-derived on every access because growing memory detaches the old buffer.

### 3. Errors and panics

Status codes and the sticky `last_error` are unchanged. A Rust panic cannot
unwind on this target: it prints to stderr and traps. `WebCore` records the
message as `failure`, refuses every later call with `panicContained`, and the
editor replaces the workspace with a "core stopped; reload" screen. Unsaved
work in that tab is lost. Native builds still contain panics; the browser
build says plainly that it does not.

### 4. Threads, workers and cancellation

The browser core runs on the page's one thread. There is no Web Worker and
no WebAssembly threading: shared memory needs cross-origin isolation headers
and a nightly-built standard library, and a worker-hosted core would turn
every synchronous ABI call into a message round trip. `domowik_cancel` exists
but has nothing to cancel. Work the native editor runs on an isolate (model
import, sizing, previews, scene derivation) runs inline after yielding one
event-loop turn. Decoding a large model can block the page for its duration.
The latency harness lives at `test/core_latency_harness.dart`; its host-specific
results are not a responsiveness guarantee.

### 5. Persistence

Plans open from bytes (`domowik_plan_open_bytes`), through the same container
reader and checks as a file: `read_package_bytes` and `read_package_with_limits`
share one generic reader, and every fixture of the malformed-package corpus is
asserted to fail identically both ways. Saving encodes
(`domowik_plan_encode`) with the same writer and §9.7 S2 validation as a file
save, byte-identical to it, and hands the bytes to the browser: the File
System Access API where available, whose write reports success or failure
(a failed write is aborted), otherwise a download, which reports nothing. The
plan becomes clean only through `domowik_plan_mark_saved(revision)`, which
refuses if anything changed while the bytes were being stored, and only after
a confirmed write: a download leaves the plan unsaved, so choosing Save while
leaving does not authorize replacing the plan. A tab cannot wait for the editor's close dialog,
so unsaved changes raise the browser's own leave-page prompt.

There is no autosave or offline hosted-plan merge. When a same-origin hosted
service is configured, the browser also offers an explicit hosted-plan library
with single-writer leases and revision-conditional whole-package saves. Hosted
plans are labelled separately from local files. Uploading a local plan requires
an explicit copy action; local open/save remains account-free and works without
the service. A confirmed local recovery download does not mark a hosted edit
saved. Preferences live in the origin's `localStorage` under one key with the
same schema and limits as the native file. Model directories do not exist in
a browser: an uploaded model is embedded in the plan's library
(`domowik_plan_import_model`) under the same media-type and size rules as a
directory placement, and is placed by digest like any embedded model. An
upload that supplies bytes a placed instance lacked changes no document
batch, so the plan state carries a separate `asset_revision`: the hit,
footprint and resolution caches key on it, and the editor and 3D view reload
on it.

The owner-approved [first-party furniture catalog](../../catalog/README.md)
adds a separate optional host service (2026-10-26). Its HTTPS manifest URL is
configurable in deployment HTML or at build time, can be disabled, and defaults
to the public HomeHero catalog. Credential-free CORS permits other deployments
to use it without knowing private backend internals. Opening
Furniture discovers bounded metadata and visible preview PNGs, never all model
bytes; explicit Place in plan verifies and imports the chosen GLB through the
same core API. No catalog request occurs on startup, and catalog files are not
part of the Flutter bundle. Embedded bytes keep saved plans self-contained.
This is not automatic retrieval of URLs or missing references from a plan.

### 6. Rendering and input

2D is the same Flutter painting on CanvasKit. CanvasKit, the core and the
renderer are served from the bundle (`--no-web-resources-cdn`), and so are
the text fonts; startup contacts no other origin. Explicit Furniture browsing
may contact the configured catalog origin as described above.

- **Roboto 2.138 Regular** (Apache-2.0). Flutter's web engine loads Roboto
  from `fontFallbackBaseUrl` plus a fixed path unless the application
  declares a family named `Roboto`. `web/flutter_bootstrap.js` sets that
  base to the page-relative `fallback-fonts/`, so the browser resolves it
  against whatever path the page is served from, and
  `scripts/build-web-modules.sh` copies the bundled file, with its licence,
  to the engine's path beneath it. skwasm (`--wasm`) loads it through the
  asset manager, which prefixes `assets/`, so the script copies it there as
  well; each renderer fetches only its own copy. (An absolute base breaks
  skwasm, which percent-encodes it again.) Roboto is deliberately not a pubspec
  font: a native bundle registering a `Roboto` family would change Linux
  desktop typography, whose default theme names Roboto.
- **A 2.2 KB subset of Noto Sans Symbols 2** (OFL-1.1) holding only ⌘, a
  pubspec font with its licence entry in the bundle's `NOTICES`, for the
  macOS modifier name "Command (⌘)" (BEHAVIOR-SPEC §11.2) and user text.

The engine checks a text style's characters from U+00A0 up only against the
families that style names, so a style naming none treats them all as missing
and requests Noto fallback fonts even where Roboto has the glyph. The web
build therefore names Roboto and the symbols fallback in the theme and in
every style a painter builds (`lib/editor_fonts.dart`); desktop builds name
neither. A glyph no bundled font has (CJK, Arabic, Hebrew, emoji, most
symbols, →) is requested from `fallback-fonts/`, which has no Noto fonts, and
draws as a box: each attempt is a failed same-origin request, never a
third-party one. A VM test checks that the two fonts have every character in
`lib/` and that Roboto sits where the installed engine looks for it. The
release probe draws those characters through a painter and a widget on every
platform, with every host but `127.0.0.1` unresolvable, and fails on any font
the engine cannot load; `verify.sh` runs it on both the default (CanvasKit)
build and a `--wasm` (skwasm) build served under a path with an encoded space
and an encoded `#`, since the two font loaders handle URLs differently. 3D is the shared `domowik-renderer` on WebGPU, built for
`wasm32-unknown-unknown` as `frontend/web-renderer` with `wasm-bindgen`, in a
second module the page loads only when a 3D view opens. The core derives each
scene and hands it over as one `domowik-scene-wire` frame (a small, validated
binary format whose encoder and decoder live in one crate), so geometry never
passes through Dart collections. The presenter (`scene_presenter_web.dart`)
speaks the same protocol as the native presenters, draws into a canvas shown
as a platform view with pointer events passed through, and leaves all camera
input to the shared Dart code.

**WebGPU only.** There is no WebGL2 fallback: without WebGPU the 3D view says
so and 2D keeps working. A GPU-free admission policy defaults desktop Firefox
on Linux to Safe 2D, preventing 3D/module initialization even when saved
preferences request 3D. The presenter independently enforces that policy. URL
parameters select Safe 2D in any browser or explicitly bypass the Linux Firefox
gate for experimental testing; see [the user guide](../USER-GUIDE.md). No automatic
GPU probe is attempted on protected pages, since such a probe can itself crash
the native browser. This is a conservative compatibility gate, not a driver
capability test or a guarantee of crash containment. Firefox on macOS and native
presenters are unchanged. A reported WebGPU startup refusal also latches Safe 2D
until reload, with selectable diagnostics and no automatic retries or preference
rewrites. This is ordinary exception handling, not native crash recovery.
Browser keyboard and pointer input reaches the
editor through Flutter; the page's context menu is disabled so secondary
clicks reach the editor's menus. Modifier-scroll zooms the plan, not the page.
Shortcuts a browser reserves for itself are outside the page's reach; the
editor's own shortcuts (undo, redo) are not among them.

### 7. Dependencies and licences

No Dart package is added: browser APIs are bound by hand with
`dart:js_interop`. The renderer module adds `wasm-bindgen` 0.2.127,
`wasm-bindgen-futures`, `web-sys` and `js-sys` (all MIT OR Apache-2.0, already
resolved by `wgpu` 30.0.1's WebGPU backend). `release/rust-manifest.json`
declares the core module and the renderer module as two staged artifacts, and
`scripts/release-validate.py` audits their 50 and 60 linked third-party crates
against the reviewed allow-list. The `rust-std` packages for both wasm targets
are recorded with their channel-manifest hashes. The Flutter web engine
(CanvasKit) ships in the bundle and, like the macOS engine, is listed from a
built bundle rather than licence-resolved by scan. `wasm-bindgen-cli` (build)
and Chrome for Testing (tests) are dev-only tools, recorded in
`docs/third-party/tools.json` and never distributed; the glue `wasm-bindgen`
emits is distributed under its MIT/Apache-2.0 terms.

### 8. Verification

`frontend/scripts/test-web.sh` runs the browser tests (real widgets, real
WebAssembly core and WebGPU renderer) in a pinned, checksum-verified headless
Chrome for Testing; `build-web.sh` builds the release bundle;
`web-probe.sh` loads that bundle and runs the in-app release probe (draw, room
derivation, undo/redo, save and reopen, model upload, 3D). All three are
`verify.sh` stages wherever Flutter runs, and so run in `ci.yml`. A managed
desktop Chrome is not used because a forced-sign-in policy can open the
profile picker and hang headless runs. On Linux, where a headless runner has
no GPU, the tests pass Chrome's SwiftShader software-WebGPU switches.

## Consequences

- One implementation of document, editing, file and rendering semantics; the
  browser adds hosts, not rules. ABI 16's byte exports are shared with native
  builds and tested there too.
- The browser bundle is about 40 MB on disk, dominated by CanvasKit variants;
  the core module is 4.0 MB (1.25 MB gzipped) and the renderer 313 KB, both
  stripped by the `web` Cargo profile.
- A browser panic loses the tab's unsaved work.
- Large model imports block the page while they decode.

## Revisit triggers

- A Web Worker or threaded core becomes affordable (cross-origin isolation is
  acceptable for deployment and threads stabilise on the target).
- WebGL2 support is required for browsers without WebGPU.
- Saving needs to be durable inside the browser (autosave, offline library).
- Plans need text in scripts Roboto lacks (CJK, Arabic, Hebrew, emoji):
  bundle reviewed Noto fallback files under `fallback-fonts/` (the engine
  requests its own Google Fonts paths beneath that base), or a font that
  covers them as an explicit fallback family.
- dart2wasm becomes the default web compiler. `build-web.sh --wasm` already
  works: the browser loads `main.dart.wasm` on skwasm and `verify.sh` runs
  the release probe on it; the default build stays dart2js until that is the
  default.
