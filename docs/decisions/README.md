# Architecture Decision Records

ADRs describe current architectural choices and their validation state.
They retain the operative decision and remaining gates, not internal issue or
review histories.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-standalone-permissive-oss.md) | Accepted | Standalone Apache-2.0 project |
| [0003](0003-flutter-gui-wgpu-native.md) | Proposed — portability and performance gates remain open | Flutter GUI and portable GPU rendering |
| [0004](0004-identified-operation-model.md) | Proposed | Identified operation batches and snapshots |
| [0005](0005-defer-external-reference-schema.md) | Accepted | Defer external integration fields from v0.1 |
| [0006](0006-naming-and-identity.md) | Accepted | Provisional naming and identity with canonical registry |
| [0007](0007-format-compatibility-promise.md) | Proposed | Post-v1 format compatibility contract (SemVer-backed, ADR normative) |
| [0008](0008-rust-core-implementation.md) | Accepted (release coverage staged) | Rust core implementation |
| [0010](0010-native-3d-texture-experiment.md) | Proposed | Linux native texture experiment; portable renderer gates remain open |
| [0011](0011-extensible-versioned-document-format.md) | Accepted (extension wire design pending) | Versioned core with namespaced feature extensions |
| [0012](0012-browser-editor.md) | Accepted | The editor in a browser: the core as WASI WebAssembly, the shared renderer on WebGPU |
| [0013](0013-architectural-primitives.md) | Accepted (initial straight-stair slice) | Construction bodies, hosted cuts and level connections underpin architectural assemblies |

Statuses:

- **Proposed:** not binding; validation criteria or design work remains.
- **Accepted:** in force until superseded.
- **Rejected:** considered and not selected.
- **Superseded:** replaced by a later ADR.

ADRs describe decisions, not legal conclusions. Material licensing decisions
should cite the third-party and clean-room policies and receive appropriate
review.
