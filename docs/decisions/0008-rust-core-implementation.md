# ADR 0008: Rust core implementation

- **Status:** Accepted

## Decision

Implement the authoritative document model, geometry, editing, persistence and
CLI in Rust. Flutter consumes this core through the native C ABI or its
WebAssembly host. It must not duplicate domain semantics.

Rust provides explicit layout and ownership, no garbage collector, and one
toolchain across the core and shared renderer. Longer compilation and explicit
FFI ownership/error handling are accepted trade-offs. This is not a claim that
one language is inherently faster.

## Performance and safety

- Release builds enable full LTO and one code-generation unit for cross-crate
  optimization.
- Predicates first use a proven floating-point filter and escalate ambiguous
  cases to exact arithmetic. Geometry changes include benchmarks.
- Benchmarks compile during verification; measured timings depend on host and
  workload and do not gate correctness automatically.
- Native builds retain unwinding so the FFI can contain panics. Changing to
  abort requires reviewing that boundary, not an isolated size optimization.
- WebAssembly panic behavior differs and is documented in
  [ADR 0012](0012-browser-editor.md).

## Validation and remaining boundaries

The root workspace contains substantive behavior and conformance tests,
malformed-package fixtures, geometry oracles and named verification stages.
Dependencies require the clean-room and third-party reviews, generated inventory
and notices. A source build is not a reproducible-binary claim.

Platform packaging, end-to-end performance and renderer acceptance remain
bounded by [ADR 0003](0003-flutter-gui-wgpu-native.md) and
[Compatibility](../COMPATIBILITY.md). Release/SBOM coverage and remaining GUI
licensing obligations are maintained in [Release inventory](../RELEASE-INVENTORY.md).
