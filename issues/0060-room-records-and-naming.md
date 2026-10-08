# 0060: Deliver room records and room naming

- **Phase:** 2
- **Priority:** High
- **Status:** Blocked
- **Depends on:** 0010, 0013

## Current disposition — 2026-10-05

Keep blocked for the explicitly recorded owner/specification questions, not
for missing room naming.

Room records, commands, labels and editor naming are delivered. Decide
R5.1.7's manual-geometry/opaque-field boundary and disposition the remaining
diagnostic/project_against/interior_point findings, then recheck closure.
F27's corrected geometry/test name is already integrated; text shaping and
numerical guarantees remain with 0017/0030. Do not silently undo the recorded
owner-directed reopening.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Remaining acceptance work — 2026-10-08

The original checked criteria below describe delivered scope and dated
evidence. These later residuals explain the current nonterminal status.

- [ ] Record the owner/specification disposition of R5.1.7's manual-geometry/
      opaque-field boundary and the remaining diagnostic, projection and
      interior-point findings; then re-evaluate closure without reopening
      delivered naming work.

## Original problem

A room cannot be named. `grep -rn 'R5\.2' crates/` returns nothing: there is
no room record entity anywhere in the Rust core, so rooms exist only as
derived faces with an area and a ring.

BEHAVIOR-SPEC section 5.2 is normative. R5.2.1 fixes the authoritative field
set — a command-supplied stable UUID, `levelId`, `seedMm`, an optional display
name, an optional floor-style reference, an optional ceiling-style reference,
and nothing else. R5.2.2 fixes exactly three command kinds, each one atomic
batch and one undo boundary with no incidental effects.

The consequences are already recorded elsewhere and are not hypothetical. The
Phase 1 closure record names room metadata reconciliation as undelivered and
notes that the size corpus therefore measures 960 of its 1,000 entities, so
`unfurnished-1000-v1` stands as a lower bound rather than a measurement.

## Why this is a separate issue

The underlying work is already gated in two places and this issue does not
restate either:

- The typed schema for a room record belongs to **issue 0010**, whose first
  criterion — typed schemas matching the normative v0.1 logical model — the
  Phase 1 closure record reports unmet for `roomRecord` and the full opening
  field set.
- Split, merge and disappearance reconciliation belongs to **issue 0013**,
  criterion three.

Neither owns the user-facing capability: creating, naming, restyling and
deleting a room record from the CLI and the editor, and rendering the name in
2D output. That capability fits no existing acceptance criteria, which is why
it is filed rather than appended.

Per the partial-scope semantics in `README.md`, this issue consumes issue
0010's and issue 0013's delivered material as a labeled provisional handoff.
Their later outputs flow back through their own gates; this is not an upstream
edge that re-opens them.

## Scope

Belongs here: the room record as an authoritative entity end to end — create,
update and delete commands with the R5.2.2 atomicity and undo boundaries; CLI
commands to name and inspect rooms; editor affordances to name a room and see
the name; name rendering in SVG and PNG export; the association between a
record and the derived face it seeds.

Explicitly excluded: floor and ceiling style definitions themselves beyond
holding an optional reference (section 5.3); furniture and asset instances
(BEHAVIOR-SPEC section 10, still "To define"); the renderer-neutral paint API
gap that issue 0016 owns, which this must not work around by adding a third
independent renderer path.

## Acceptance criteria

- [x] Room records are authoritative entities with exactly the R5.2.1 field
      set — no more — and derived data stays derived and rebuildable per
      R3.0.7. `domowik_format::RoomRecord`; a test asserts the SERIALIZED key
      set, because the wire is where "no more" is observable. The `unknown`
      map is not a field of the entity: it is §8 U2's preserve-don't-strip
      mechanism, carrying a NEWER version's optional data verbatim, and every
      Tier A kind has one. Candidacy, association, status, areas, label
      anchors and surfaces are computed in `domowik_geometry::rooms` and
      persisted nowhere.
- [x] Create, update and delete are exactly three command kinds, each one
      atomic batch and one undo boundary with no incidental effects; a
      supplied identifier already in use rejects deterministically with an
      empty batch, no state change and no undo entry.
      `domowik_edit::{create,update,delete}_room_operations`; the collision
      path is asserted at the builder, through the CLI and through the bridge.
- [x] The CLI can create, name, restyle, list and delete room records, and
      `info` reports names alongside the areas it already reports.
      `domowik room add|list|set|remove`, addressable by listing number or by
      record UUID. `info` gained the label and a room-record count; `--json`
      gained `roomRecords` beside `rooms`, and `label`/`displayAreaM2` on each
      face.
- [x] The editor can name a room and shows the name; the editor holds no room
      identity rules of its own (ED-0101). A row per face with a naming
      button, `addRoom`/`setRoom` gestures, and the label drawn from the
      core's own answer. Two rules the painter used to hold are gone: it
      labelled every face, where R5.3.3 shows no label on a face with no
      candidates, and it rounded areas with `toStringAsFixed(2)`, where
      R5.3.2 resolves exact halves to even. Even the seed for a new record
      comes back from the core as `seedCandidateMm`.
- [x] SVG and PNG export render room names deterministically, through whatever
      shared path issue 0016 leaves in place rather than a third renderer.
      `crates/domowik-cli/src/label.rs` produces placed glyph CONTOURS in plan
      millimetres and each exporter draws them with the path primitive it
      already had. See "What this leaves open" below: this is a shared
      semantic source for labels, and it is not the renderer-neutral paint API
      issue 0016 still owes.
- [x] Records survive save, reopen, undo, redo, compaction and external batch
      admission, with tests for each.
      `crates/domowik-cli/tests/room_records.rs`, one test per mechanism.
- [x] The size corpus is re-measured with records present, and the
      `unfurnished-1000-v1` lower-bound caveat in the Phase 1 closure record
      and `docs/evidence/size-corpus/` is either discharged or restated with
      its new reason. **Discharged.** All 1,000 entities are built and
      measured: 20.0 KiB against a 256 KiB target, 193 B/entity uncompressed,
      9.4x compression. The published figures became RANGES over six runs,
      because identifiers are UUIDv7 and the previous single number was one
      sample presented as exact.
- [x] Full repository verification passes, with remaining scope stated
      honestly. `./scripts/verify.sh` exited 0 with
      `=== VERIFICATION PASSED === all stages` on 2026-09-12 at commit
      `bdb2006`, with NO stage skipped — the Flutter editor stages, the macOS
      application build, the C harness under asan/ubsan and the Dart FFI
      harness all ran. The remaining scope is the section below.

## What this leaves open

Recorded rather than waived, and each against the issue that owns it:

- **Text shaping (issue 0017).** Room names outside `label::SUPPORTED_RANGES`
  — anything needing `GSUB`/`GPOS`, mark attachment or bidirectional
  reordering, so CJK, Arabic, Hebrew, the Indic scripts, and any name written
  with combining diacritics — are not DRAWN. The export succeeds, keeps the
  area line, and names each refusal by record and code point; the stored
  string is never altered. Refusing is deliberate: a wrongly shaped name looks
  like a name, and a missing one is visibly missing.
- **The renderer-neutral paint API (issue 0016).** Labels are the first thing
  both exporters take from one semantic source. The occupied region, apertures
  and background images still go from the `Document` into each exporter
  separately.
- **R5.1.7's `manual-room-geometry-unsupported` diagnostic.** Not implemented,
  and nothing in v0.1 can currently trigger it: no entity kind carries a room
  polygon, an unknown entity `type` is refused by the tagged enum, and an
  unknown FIELD on a `roomRecord` is preserved opaquely because §8 U2 requires
  exactly that. Implementing the rejection means deciding which unknown field
  names count as persisted face geometry, which is a specification question
  rather than an implementation one, so it is left to the spec owner.
- **F26 and F27 disagree about join-patch geometry**, found while building the
  room projection and routed to issue 0030. F27 states a four-vertex courtyard
  hole and 38,200,000 mm²; the arrangement produces chamfered corners and
  38,220,000 mm², which is what F26's own derivation requires — four island
  corners times the same 5,000 mm² triangle F26 adds to its own room. The room
  work changed no geometry; the test
  `f27_courtyard_hole_is_chamfered_as_the_f26_patch_rule_requires`
  (named `f27_courtyard_area_follows_f26_patch_geometry_not_f27_stated_rectangle`
  when this was written) pins current behaviour with the derivation in full, so
  a correction to either side is visible rather than silent.

## Provenance

Original Domowik specification (BEHAVIOR-SPEC section 5.2) and a repository
review on 2026-09-06. No comparable-application source, fixtures or assets
consulted.

## Not closed: owner decisions outstanding

Reopened 2026-09-12 on operator instruction, before any owner decision had
been answered. **Nothing below is withdrawn** — every criterion stays met,
the verification banner stays recorded, and no evidence or criterion is
deleted. Only the STATUS changes, because a terminal status asserts that
nobody is waiting on anybody, and that is not true here.

What is waiting, and on whom:

- **R5.1.7's `manual-room-geometry-unsupported` diagnostic** is a
  specification question for the owner, recorded under "What this leaves
  open" and not implementable without deciding which unknown field names
  count as persisted face geometry — which §8 U2's preserve-don't-strip rule
  cuts against.
- The three findings of the completed adversarial review (the
  `nonfinite-coordinate` diagnostic naming a finite value, the
  `project_against` union precondition, and `interior_point`'s sub-ulp
  limit) have a proposed fix awaiting a land-or-defer.

Re-closing needs those answered, not more work: the acceptance criteria are
met and the evidence stands.

## Closure

- **Implementation/spec commits:** `d573085` (record entity, projection,
  R5.2.2 commands), `29ad99c` (CLI commands, size-corpus remeasurement),
  `da03bc2` (SVG/PNG name rendering, pinned font), `5021a19` (bridge ABI 13,
  editor naming), `4f48554` (format-spec conformance tests), `a305d47`
  (self-supplied adversarial scrutiny), `6bbb7f3` (README), `bdb2006`
  (display-rule wording per issue 0030).
- **Follow-up issues:** none opened. Every open thread is recorded against an
  issue that already owns it — text shaping and the pinned-font criterion on
  **0017**, the renderer-neutral paint API and tolerant rasterized comparison
  on **0016**, the F26/F27 fixture contradiction and the R5.3.2 boundary
  wording on **0030**. R5.1.7's `manual-room-geometry-unsupported` diagnostic
  is noted above as a specification question rather than filed as work, since
  nothing in v0.1 can currently trigger it.

## Note on the advisory round

The required Codex adversarial review (`gpt-5.6-sol`, high reasoning effort)
was run as INSTRUCTIONS.md specifies and produced NO adversary: its startup
banner confirmed the model and effort, and it then returned only "Your
workspace is out of credits" — with exit status 0, so a caller checking only
the status would have recorded a review that never happened.

That round is not counted. Its side was not written for it. The scrutiny it
was asked for was done instead, as `a305d47`: one test per attack question,
including the display rounding compared differentially against an exact
integer reference over a thousand values rather than against the four worked
examples, and `interior_point` round-tripped through the projection across
five face shapes. The prompt is saved and re-runnable if credits return.
