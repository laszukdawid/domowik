# 0079: Present the native 3D texture upright

- **Phase:** 3
- **Priority:** High
- **Status:** Done
- **Depends on:** 0078

## Problem

The owner reports the 3D scene is vertically inverted. The renderer's OpenGL
framebuffer orientation is not normalized for Flutter texture presentation.

## Acceptance criteria

- [x] World +Y appears above world -Y in the displayed texture; camera/world
      coordinates and plan data remain unchanged.
- [x] An asymmetric red-above-blue native rendering probe detects inversion.
- [x] The correction preserves face lighting, depth and the cached GPU camera
      path, without readback/copies or a per-frame geometry rebuild.
- [x] Linux build, relevant tests, native probe and canonical checks are run.

## Closure — 2026-10-03

The original vertex shader left OpenGL's bottom-origin framebuffer orientation
unnormalized for Flutter's top-origin texture presentation. Clip-space Y is now
reflected in the shader, with clockwise front-face selection for the reflected
projection. Camera/world coordinates, normals and source geometry are unchanged;
no extra rendering pass, image copy or normal-frame readback is introduced.

The original native red-above-blue fixture passed with 9,867 red top-half and
9,867 blue bottom-half pixels under llvmpipe/Impeller/X11. A temporary capture
of the actual application also confirms upright room/furniture presentation.
The normal GPU probe still passes its no-redecode/no-geometry-upload camera
checks. Linux release build, analysis and all 275 Flutter tests passed. Canonical
verification passed preceding executed checks and failed only at `P13 clean
tree`, printing `=== VERIFICATION FAILED ===`, exit 1 for uncommitted work.
No commits or screenshot fixtures added.
