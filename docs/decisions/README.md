# Architecture Decision Records

ADRs capture durable project decisions and their validation state.

| ADR | Status | Decision |
|---|---|---|
| [0001](0001-standalone-permissive-oss.md) | Accepted | Standalone Apache-2.0 project |
| [0002](0002-go-core-cli-first.md) | Superseded by 0008 | Go core with CLI-first delivery |
| [0003](0003-flutter-gui-wgpu-native.md) | Proposed — bridge-spike evidence added 2026-09-01; gate NOT passed | Flutter GUI and wgpu-native 3D renderer |
| [0004](0004-identified-operation-model.md) | Proposed | Identified operation batches and snapshots |
| [0005](0005-defer-external-reference-schema.md) | Accepted | Defer external integration fields from v0.1 |
| [0006](0006-naming-and-identity.md) | Accepted | Provisional naming and identity with canonical registry |
| [0007](0007-format-compatibility-promise.md) | Proposed | Post-v1 format compatibility contract (SemVer-backed, ADR normative) |
| [0008](0008-rust-core-implementation.md) | Accepted (gates 1-4 open) | Rust core implementation, superseding 0002 |

Statuses:

- **Proposed:** not binding; validation criteria or design work remains.
- **Accepted:** in force until superseded.
- **Rejected:** considered and not selected.
- **Superseded:** replaced by a later ADR.

ADRs describe decisions, not legal conclusions. Material licensing decisions
should cite the third-party and clean-room policies and receive appropriate
review.
