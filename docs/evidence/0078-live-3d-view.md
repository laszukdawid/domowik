# Live GPU 3D view — issue 0078

## Performance architecture

Owner direction: performance first, rendering approximations permitted.

- Rust owns scene derivation from the existing plan; no second document.
- Imported meshes are decoded/LOD-clustered once per content digest.
- Repeated instances share GPU geometry and are submitted as instanced draws.
- Camera movement updates a view/projection matrix, not vertex buffers.
- Flutter receives a registered native GPU texture; no per-frame CPU readback.
- Scene work runs asynchronously after revisions. Camera events are coalesced
  with at most one platform update in flight.
- Rendering LOD, flat/diffuse shading and approximate transparency are derived
  visual choices; original assets and exact 2D geometry remain unchanged.

Fast mode caps meshes at 5,000 triangles (20,000 remains the public high-budget
ceiling). A separate camera mutex prevents input updates waiting on rendering.
Scene generation copies no cached furniture vertex arrays; asset uploads are
requested independently and occur once per digest. GPU resources are created,
uploaded and retired on the populate GL context. Closed texture resources are
reaped on the next populate in that context or released with the engine context;
no GL deletion is attempted on the platform method-call thread. Surface session
tokens reject late/stale requests. Resize reallocates only the color/depth target.

Interop state includes restored framebuffer/VAO/program/blend/color/depth state.
The functional probe caught Impeller's reversed-depth clear state hiding all
geometry; the renderer now explicitly establishes its own depth convention and
restores the embedder's. Floor-cap winding and front-face state are explicit.

## Reviewed components and provenance

`earcutr 0.5.0` (ISC), crates.io package / https://github.com/frewsxcv/earcutr/:
public triangulator API used for derived floor/slab polygons with holes. Its
packaged LICENSE records Mapbox (2016) and Tree Cricket (2018), permits use and
redistribution with retained copyright/permission; generated NOTICE carries
those texts. Normal dependencies: itertools 0.14 (MIT OR Apache-2.0) and
num-traits 0.2 (MIT OR Apache-2.0), with their permissive transitive closure.
The existing exact 2D core determines the polygons; triangulation is only for
rendering. No source or fixtures copied/adapted into Domowik.

Linux SDK path: Flutter `FlTextureGL`/texture registrar public headers (Flutter
SDK BSD-style license) and GTK's existing GL context; `libepoxy 1.5.10` system
SDK dispatcher, Expat/MIT (Intel 2013–2014 / Khronos registry 2013). This adds an
explicit link to an existing GTK graphics dependency, not a bundled driver or
foreign shader implementation. Shaders, scene assembly and camera controls are
original code. No comparable application's source or private format consulted.

ADR 0010 records this as a native-texture experiment. ADR 0003 and issue 0025's
cross-platform/splat/hardware gates do not close merely because a 3D frame is
displayed. Evidence must name the real graphics driver and measurement scope.

## Use

The header **Choose 2D / 3D view** menu selects plan, perspective 3D, or a split
workspace. Drag to orbit; right/middle or Shift-drag to pan; scroll/pinch to zoom.
**Walk through** uses an eye-level camera: drag to look, WASD forward/strafe,
Q/E vertical movement, Shift faster. **Fit** / F / Home resets the aerial pose.
Edit in the 2D pane/inspector; accepted edits, history and visible-level changes
update the same 3D plan. View changes never edit document state.

This backend is implemented/validated on Linux. It is not a claim of complete
textured/PBR fidelity, cross-platform 3D support or external-app feature parity.

## Recorded evidence — Linux, 2026-10-03

- Complete Flutter suite: **275 passed**. Rust/native tests, formatting/clippy,
  dependency/notice checks, existing C/Dart probes and Linux release build pass.
- Original scene tests verify shared instances, rebasing, elevations, stored
  wall heights, slab surfaces, opening holes/leaf orientation and ghosting.
- Native model-cache test verifies repeated scene reads decode one shared model
  once and leave the document unchanged. Widget tests verify navigation changes
  only camera calls, authoritative edits update scene transforms and teardown
  releases the surface session.
- Dart JIT benchmark: **2,184 ns** per orbit + view/projection update (100,000
  updates). This is arithmetic/allocation only, not a GPU/presentation time.
- Rust scene benchmark: **184–211 µs** for 1,000 shared furniture instances.
  This is scene construction after edits, not navigation work.
- Actual built application under **llvmpipe (LLVM 21.1.8, 256 bits)** / Impeller
  OpenGLESSDF / Xvfb X11 passed `3D GPU PROBE PASSED`, with displayed geometry
  confirmed by an explicit one-time diagnostic capture. Normal frames never
  read back pixels. Original 25-instance fixture: three draw calls, one scene
  request, one mesh request; 120 camera gestures leave mesh/instance upload and
  scene counters unchanged.
- Imported MARKUS GLB, 25 shared instances: **116,585 drawn triangles**, three
  draw calls, one scene request and one mesh request; navigation still causes
  no geometry uploads. Last recorded native command submission was **17.8 ms**
  on llvmpipe. This is software-renderer submission work, not a hardware GPU
  frame budget or a claimed 60 FPS target. Camera updates are coalesced rather
  than queued when the renderer is slower.
- `./scripts/verify.sh` passed all executed stages preceding its final assertion,
  then printed **`=== VERIFICATION FAILED ===`**, **`STAGE: P13 clean tree`**,
  exit **1**, because pre-existing/new work is uncommitted. macOS bundle stages
  were explicitly skipped on Linux; no full verification pass is claimed.

Probe from `spikes/0024-flutter-rust-bridge/flutter_app`:

```sh
flutter build linux --release
xvfb-run -a env GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1 DOMOWIK_3D_PROBE=1 \
  build/linux/x64/release/bundle/domowik_spike_app
```

Set `DOMOWIK_3D_PROBE_MODEL` to a local model path for the denser instancing
probe. Remove software/Xvfb overrides for a separately labelled hardware run;
record its actual graphics driver and frame scope rather than relabelling the
software numbers above. No screenshot or external model fixture is committed.

## Texture-orientation regression — issue 0079

OpenGL framebuffer rows were presented upside down through Flutter's texture
convention. The vertex shader now reflects clip-space Y; the render pass uses
clockwise front-face selection so reflection does not invert face lighting.
This is presentation normalization only, not a change to camera matrices,
world axes or model/plan coordinates. It adds no copy, pass or normal-frame
readback and leaves all cached geometry and camera-update counts unchanged.

The explicitly requested orientation probe renders original red/blue quads at
world +Y/-Y using the actual GPU shader/instancing/FBO. It asserts red in the
presented top half and blue in the bottom half. Run the normal command above
with `DOMOWIK_3D_ORIENTATION_PROBE=1` as well. Recorded llvmpipe/Impeller/X11
result: **9,867 top red pixels**, **9,867 bottom blue pixels**,
**`3D ORIENTATION PROBE PASSED`** and **`3D GPU PROBE PASSED`**. An actual
temporary desktop capture confirms upright room/furniture display too.

Linux release build/analysis and 275 Flutter tests passed. Canonical verification
again reached its final `P13 clean tree` assertion after all preceding executed
checks passed, then printed `=== VERIFICATION FAILED ===`, exit 1 because work
is uncommitted. No image fixture was added to the repository.
