# ADR 0003: Flutter GUI and portable GPU rendering

- **Status:** Proposed

## Direction and current implementation

Flutter owns the application shell, interaction and 2D canvas. Rust owns the
shared document/editing core and renderer-neutral scene. Platform presenters
own GPU surfaces, textures and lifecycle; rendering must not create a second
geometry model.

The shared Rust `wgpu` renderer currently runs on macOS through Metal and in
browsers through WebGPU. Linux uses a separate native OpenGL texture presenter.
The shared renderer calls the Rust crate directly, not a `wgpu-native` C wrapper.
Android 3D is unvalidated; native iOS and Windows editor support is not promised.
See [Architecture](../ARCHITECTURE.md) and
[Compatibility](../COMPATIBILITY.md) for current support boundaries.

## Remaining acceptance gates

This direction remains Proposed until the relevant target-platform bridge and
whole-editor validation is complete:

- Surface/texture integration, resize, teardown and resource ownership.
- Native packaging and callback/thread scheduling on the intended platforms.
- Representative instanced geometry and the proposed splat workload.
- Startup, memory, presented frame time and input latency on named devices.

The native ABI must document and test handle/buffer lifetimes, allocation,
serialization, close/drain behavior, cancellation and panic transport. Rust's
lack of a managed runtime does not remove those obligations. Browser panic
behavior and single-thread limitations are separately specified in
[ADR 0012](0012-browser-editor.md).

Software-renderer probes establish functional behavior, not hardware frame-time
budgets. Existing implementation does not automatically satisfy every acceptance
gate. A replacement backend requires an explicit architecture and licence review;
Linux GL remains a bounded experiment under [ADR 0010](0010-native-3d-texture-experiment.md).
