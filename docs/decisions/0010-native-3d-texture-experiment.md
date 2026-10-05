# ADR 0010: Native texture experiment for the live 3D editor

- **Status:** Proposed
- **Date:** 2026-10-03

## Context and experiment

Issue 0025's wgpu dependency inventory contains no surface implementation.
Issue 0078 requests a usable live 3D view now. The Linux GTK embedder publicly
provides a current GL context inside `FlTextureGL.populate`, allowing rendering
directly into a registered texture with no CPU framebuffer readback or new
graphics runtime dependency. This is a bounded native-GL alternative experiment
under ADR 0003's stated fallback; it does not supersede ED-0109's wgpu package
choice or automatically accept a cross-platform final renderer.

Rust builds renderer-neutral scene data from the existing authoritative plan.
The GUI/platform layer owns GPU resources, shaders, texture registration and
camera state. Scene uploads occur after document changes; camera updates upload
only matrices. No per-frame model decoding, scene serialization or image copy.

Owner direction: performance is paramount; visual rendering approximations are
allowed. The initial scene budget is 20,000 triangles per imported mesh at fast
quality, digest-keyed shared GPU buffers, one coalesced camera update per frame
and one in-flight platform update. These are rendering budgets, never changes
to stored model bytes or exact 2D geometry. Tests/probes count uploads and decode
work independently from camera updates, and report native render timing with
the actual graphics driver rather than conflating it with scene-load time.

The experiment must prove displayed 3D content, depth/lighting, resize and
teardown under the actual Flutter texture API. Software-renderer evidence is
explicitly functional, not proof of a hardware frame budget. ADR 0003 and issue
0025's platform/splat/hardware gates remain open. A backend may be replaced
behind the same scene/camera interface after those gates are measured.
