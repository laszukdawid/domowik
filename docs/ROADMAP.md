# Roadmap

**Domowik is in Phase 2 prototyping** (2026-09-05). Each phase starts with
unresolved feasibility work and has an exit gate. Dates are intentionally
omitted until those gates are sized.

| Phase | State |
|---|---|
| 0 — specification and risk reduction | Exited, with named gates still open (below) |
| **1 — core and headless CLI** | **Exit criteria met; see the closure record** |
| 2 — interactive 2D GUI | Working macOS Flutter prototype; platform and measured performance gates remain open |
| 3 — 3D | Not started |

The implementation language changed during Phase 1: the core is Rust
([ADR 0008](decisions/0008-rust-core-implementation.md), superseding ADR 0002),
and the Go tree is frozen reference at `_reference/go/`. Phase bullets below
that say "Go" are the original text, kept because rewriting history is worse
than a dated note — read them as the work item, not the language.

## Phase 0: specification and risk reduction

- Complete the black-box behavior specification for walls/joins, openings,
  levels, snapping/selection/undo, room topology, and room identity.
- Decide the numerical geometry model: floating/adaptive predicates, integer
  quantization, coordinate bounds, clipping, and planar face extraction.
- Complete the draft logical schemas and operation metadata, including
  identity, atomic batches, frontiers, tombstones, and conflict requirements.
- Specify asset descriptors, thin/portable packages, representative size
  corpus, and initial budgets.
- Select the physical container and define atomic-save and hostile-input rules.
- Ratify project names and the canonical Go module path.

**Exit:** normative behavior examples exist for Phase 1; format/security
requirements are implementable; numerical spikes support a Go implementation
or ADR 0002 is revised.

## Phase 1: core and headless CLI

- Scaffold the Go module and release/license CI checks.
- Implement entity projection, operation batches/replay, validation, local
  commands/undo, and container persistence.
- Implement asset cache/embedding, verification, deduplication, and CLI asset
  commands.
- Implement geometry, room reconciliation, and spatial queries.
- Provide `new`, `info`, `validate`, `apply`, `ops`, `compact`, and 2D SVG/PNG
  export commands.
- Build hostile/corrupt input and compatibility corpora.
- Release experimental format v0.1 with tested migrations, not a claim of
  permanent stability.

**Exit:** representative plans can be created, scripted, validated, packed,
reopened after injected save failures, and exported headlessly. Size targets
are measured and published.

### Phase 1 closure — 2026-09-01

**The exit criteria are met.** Each clause, with what discharges it:

| Clause | Discharged by |
|---|---|
| created, scripted | 20 CLI commands; probes P5–P14 |
| validated | `validate`, human and `--json`; probe P11 asserts the invalid path |
| packed | `pack --portable`, all-or-nothing with a complete omission report |
| reopened after injected save failures | `crates/domowik-container/tests/injected_save_failures.rs` — the §9.7 guarantee table's F1, F2, F3, F5 and F6 states, plus a torn package and a sweep. It REPRODUCES each filesystem state rather than killing a process mid-write, which tests the recovery contract and not the kernel's ordering; §9.7 requires a fault-injecting filesystem before any power-loss claim, and this project makes none |
| exported headlessly | `export-svg`, `export-png`, both with background images |
| size targets measured and published | `docs/evidence/size-corpus/PART-A-MEASURED.md` |

**What Phase 1 did NOT deliver**, listed here rather than left to be discovered:

- **Room metadata reconciliation** (BEHAVIOR-SPEC §5.2). Rooms are derived
  geometry with no persisted record, so the size corpus measures 960 of its
  1,000 entities and `unfurnished-1000-v1` is a lower bound.
  **Discharged 2026-09-12 by issue 0060**, which delivered the `roomRecord`
  entity, the three R5.2.2 commands, candidacy and effective association. The
  corpus now builds all 1,000 entities and measures 20.0 KiB against a 256 KiB
  target; see [the Part A evidence](evidence/size-corpus/PART-A-MEASURED.md).
  The bullet stays because this table records what Phase 1 shipped, not what
  the repository holds today.
- **A blob cache and `pack --thin`.** Every reference is embedded or dangling;
  there is no third state, and `--thin` is refused rather than approximated.
- **Remote asset retrieval**, and everything §6 says about consenting to it.
- **Cross-platform CLI builds and a release/SBOM process.** The dependency
  inventory and `NOTICE` are enforced fail-closed, but neither is a release
  process, and issue 0036 is marked Done on the strength of the Go one.
- **Migrations** (issue 0031). No `0.N` schema has been publicly released, so
  the predecessor set is empty and there is nothing to migrate FROM — but the
  machinery to migrate does not exist either, and the first release creates the
  obligation.
- **Full §5.1 snapshots.** `compact` retains the frontier (S2) and tombstones
  (S4) and makes S1/S3 unnecessary by refusing any batch concurrent with the
  snapshot. That is a narrowing, not the specified snapshot.
- **The numerical model ratification** (issues 0022/0030), which several
  provisional constants still name as their gate.
- **An experimental v0.1 release.** Nothing is tagged or published, ADR 0007
  is still Proposed, and issue 0034 has not settled publication identity. The
  Phase 1 bullet list names a release; there is none.
- **A real fault-injection suite**, recovery artefacts and the RS1–RS5 states,
  and §9.2 external-modification detection (issue 0029).
- **Validation warnings, a compatibility-aware mode, digests in the report, and
  the six-way exit-code taxonomy** (issue 0014). Three exit codes exist:
  unsupported version, I/O and internal failure all collapse into 1.
- **Golden, adversarial and migration corpora.** The hostile corpus runs, but a
  fixture rejected for the WRONG diagnostic still passes (issue 0028).
- **`ops --json` and tombstones in the `ops` report** (issue 0027).
- **`roomRecord` and the full opening field set** — `depthMm`, `side`, `hinge`,
  `opensToward`, `swingDeg` (issue 0010). The typed schema is a subset of the
  normative logical model, not a match for it.
- **A renderer-neutral paint API.** `svg.rs` and `png.rs` each render straight
  from the document, so the two exporters can drift and there is nothing to
  write serialization-independent conformance tests against. Both are tested
  structurally; neither is tested by tolerant rasterized comparison. This is
  why issue 0016 closes as partial scope rather than Done.

Phase 1 is closed as **"the engine and the CLI work, and here is exactly where
they stop"** — not as "everything in the bullet list above exists".

**The tracker was made to agree with that.** Twenty-two issues carried
`In Progress` at once, which described what had been touched rather than what
was in flight. [ED-0095](../EXECUTIVE-DECISIONS.md) re-scoped the ED-0018 gates
and re-stated every issue: implementation issues whose criteria are met are
`Done`, issues that delivered with named gaps are `In Progress (partial
scope)`, issues awaiting an owner decision or an unstarted phase are `Blocked`,
and unstarted ones are `Open`. Nothing is `In Progress`, because nothing is
being worked on between phases. Every re-stated issue carries a section saying
what it did and did not deliver. The result is **10 Done, 19 partial scope,
5 Blocked, 3 Open**.

**The pass was then audited adversarially against every issue's verbatim
acceptance criteria, and the audit is the reason those numbers are what they
are.** [ED-0096](../EXECUTIVE-DECISIONS.md) records it in full. In summary:

- **Two Done marks were unearned and were EARNED rather than argued away.**
  Issue 0015 was Done with `apply --dry-run` — a stated criterion — not
  implemented at all, and with the untrusted batch file bounded only at save.
  Issue 0037 was Done with ADR 0003 still saying "the Go core" six days after
  ADR 0008 moved the core to Rust. Both are now implemented.
- **Four issues were downgraded** — 0010, 0011, 0016 and 0027 — with their
  gaps named above. Issue 0036 was restored from `done/`: a release process
  describing the frozen Go product does not discharge criteria about "every
  distributed binary artifact" of a Rust one.
- **Two `Blocked` marks were not earned.** Issue 0030 was hiding three
  criteria's worth of unstarted work behind a decision nobody had been asked
  for; it is partial scope. Issues 0018 and 0031 are genuinely blocked on one
  thing each and were claiming to be blocked on everything; their notes now
  separate the two.
- **Three defects in shipped code were found behind the paperwork**, each now
  fixed with a mutation-proven regression test: `apply` accepted and SAVED a
  batch creating a zero-length wall of negative thickness, producing a file
  the same binary's `validate` then rejected; `causalPastSize` accumulated in
  `u64`, which §9.3 row L33 forbids in terms; and a session opened on a
  compacted plan dropped the retained tombstones, so the live document and the
  saved file disagreed.

That last group is the argument for auditing a closure instead of announcing
one. Two of the three were invisible from the tracker and from the test suite;
they surfaced only by reading each criterion against the code that claimed to
satisfy it.

### After Phase 1 — the first thing measured

The Go bridge spike recorded finding **F1** on 2026-08-25: repeated
`Session.Execute` slowed superlinearly with command count, cause undiagnosed.
The Rust core inherited it. Measured on 2026-09-01, one commit at 2000
operations cost **48.6 ms** — three interactive frames, for one keystroke —
and was still climbing quadratically. No amount of GUI work survives that, so
it was diagnosed and fixed before Phase 2 was started rather than discovered
inside it. It now costs **197 µs** (247x), and building a 2000-operation
document went from 32.9 s to 112 ms (292x) on the same host. The Go spike's
numbers are not compared against — different hardware, different boundary, and
that spike disclaimed comparative use — but they established the SHAPE, and the
shape was reproduced before it was fixed. Issue 0038 and
[the evidence](evidence/0038-session-commit-cost/MEASURED.md) carry the
numbers, the correctness argument, and the one residual left open.

## Phase 2: interactive 2D GUI

**Current handoff (2026-09-05):** issue 0024 delivers a macOS editor backed by
the Rust gesture API (`domowik-edit`, ED-0101), including wall/opening edits,
derived room areas and native open/save. Issue 0039 improves the workspace
and its small-window layout. This is still the experimental spike; Linux and
Android execution and issue 0033's measured interaction budgets remain owed.
Issues 0040 and 0041 add unsaved-session protection, stable specification-based
viewport navigation, and an adaptive grid/ruler display. Camera/grid CPU
measurements are published separately from the pending whole-app frame goals.
Issues 0042–0044 add core-owned Undo/Redo, zoom-independent gesture thresholds,
cancel-safe previews, and continuous trackpad/two-touch navigation. Injected
input tests pass; physical-device validation is not inferred from them.
Issue 0046 bounds grid work even for extreme finite camera origins. Issues
0045/0047 share cached native ordered targets across press, selection and
opening placement, with true joined-corner selection. Issue 0048 prevents
status messages from moving the canvas. Issues 0049/0050 add deterministic
entity cycling, modifier selection and immediate reset observations; 0052
keeps inspector values readable at narrow widths and larger text scales.
Issue 0051 supplies visible/ghosted/hidden level controls and passive rendering,
with per-level native payload reuse and active-only editing. Issue 0053 aligns
new-plan baselines and adds core-owned level deletion guards shared by CLI,
GUI and history/external admission. Issue 0054 adds atomic authoritative level
properties, explicit default-height resolution for future walls and separate
non-rejecting validation warnings. The scoped suite now passes 151 Flutter
tests; full repository/macOS verification for this
increment is recorded in its evidence report. Native exact-distance predicates
(0012), wall-contact refusals and overlap warnings (0055), marquee and full
snapping remain open; these GUI increments do not close those conformance or
platform gates.

- First validate the **Rust**-to-Flutter FFI on Linux and Android: ABI
  packaging, memory ownership, errors, threading, callbacks, cancellation, and
  runtime lifecycle. Revise the architecture if it fails. Issue 0024 now
  targets Rust under [ADR 0008](decisions/0008-rust-core-implementation.md);
  the FFI obligations in AGENTS.md apply to that boundary.
- Build the Flutter shell and 2D editor against the same core behavior.
- Implement levels, wall/opening tools, room metadata, selection, snapping,
  undo/redo, and asset-instance placement.
- Define benchmark corpus, reference devices, frame-time percentiles, and
  memory limits before claiming performance.

**Exit:** a real apartment can be edited on reference desktop/mobile devices;
GUI edits replay through the CLI; 2D output is semantically equivalent within
documented rendering tolerances.

## Phase 3: full 3D plan view

- First validate Flutter surface integration and packaging for wgpu-native on
  target desktop/mobile ABIs. Revise ADR 0003 if it fails.
- Generate render geometry from the document; add floors, walls, openings,
  instanced furniture, materials, lighting, and orbit/first-person cameras.
- Define GPU memory/resource lifetime and level-of-detail budgets.

**Exit:** walk through Phase 2 plans in 3D on named reference devices.

## Phase 4: optional synchronization

- Design an explicit sync protocol around operation IDs, frontiers, snapshots,
  idempotency, tombstone retention, and untrusted peers.
- Start with offline multi-device sync and visible conflict handling; do not
  market LWW as lossless concurrent collaboration.
- Add real-time/multi-user semantics only after domain-specific merge or CRDT
  rules have convergence tests.

**Exit:** supported offline/edit/interleaving scenarios converge or surface a
recoverable conflict according to published tests.

## Phase 5: Gaussian Splat viewing

- Select or design a licensed compact, quantized, spatially chunked, multi-LOD
  splat representation.
- Stream a useful preview before loading full quality; anchor/register splats
  in plan coordinates.
- Publish representative indoor-scene size, frame-time, and quality metrics.

**Exit:** view a splat progressively and align it with a floor plan on target
desktop/mobile hardware.

## Phase 6: capture and reconstruction

- Guided mobile capture and privacy/retention controls.
- Optional hosted or self-hosted GPU pipeline for camera poses and splat
  training, selected after license/deployment review.
- Later research: infer candidate planes/walls and present them as user-reviewed
  proposals, never silent authoritative geometry.

**Exit:** create a splat from a room photo set and import/anchor it without
embedding source photographs unless the user opts in.

## Deferred: HomeHero integration

Domowik must be useful independently first. Any adapter is API-level and is
designed from real integration requirements, not speculative fields in format
v0.1. See [INTEGRATION-HOMEHERO.md](INTEGRATION-HOMEHERO.md).
