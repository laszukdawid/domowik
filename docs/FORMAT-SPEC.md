# Domowik Document Format: Requirements and Draft Logical Model

Status: **Experimental contract, not a stable public wire specification.**
Sections 2–5, §7, §8 and §9 carry section-scoped normativity. A reader/writer
exists, but not every specified behavior or byte-layout requirement is a
complete conformance claim. See [Compatibility](COMPATIBILITY.md) for current
support and conversion limits.

The plan file extension is `.domo` and the identity values are ratified
provisionally by [ADR 0006](decisions/0006-naming-and-identity.md); all
values are tracked and replaceable via the
[naming registry](NAMING.md).

**Evolution direction accepted — 2026-10-08:** a defined core with namespaced
feature extensions, selected by the existing manifest `formatVersion`.
[ADR 0011](decisions/0011-extensible-versioned-document-format.md) records the owner decision and
§8.5 distinguishes it from pending extension wire design. Current readers
support `"0.1"` and `"0.2"`; new architectural entities require `"0.2"`, while
ordinary `"0.1"` plans retain their label. The normative
[architectural supplement](ARCHITECTURAL-PRIMITIVES.md) defines that addition;
no namespaced-extension wire capability is activated here.

## 1. What the format must provide

- Compact local-first plan storage.
- Deterministic projection of document state.
- Atomic updates and recoverable saves.
- Scriptable edits and validation through the CLI.
- Stable entity identity and sufficient operation metadata for future sync.
- Thin and portable asset packaging, with content deduplication.
- Safe handling of unknown, malformed, and hostile input.
- Explicit migration behavior between released schema versions.

The format covers the logical document and package. It does not define UI
selection, hover state, viewport zoom, or other per-device ephemeral state.

## 2. Logical document model

Status: **Normative for v0.1 within this section's scope (issue 0004):
entity identity, entity schemas, and collection shapes.** The document
header above still marks the overall format pre-implementation: the
physical container, operation-batch framing, compression, and the
compatibility/migration policy remain owned by issue 0005 (§7, §8).
Behavioral semantics — which commands exist, what they accept, and what
users observe — are owned by [BEHAVIOR-SPEC.md](BEHAVIOR-SPEC.md) §§1–5
and 8 (normative); this section specifies only the wire realization those
semantics require, never new behavior. Asset-descriptor material is
consumed ONLY from ASSET-STRATEGY §10 ("Provisional handoff — 0021 partial
scope"). Numeric magnitudes appear only as the named provisional
parameters of §3; none are ratified here.

### 2.1 Entity identity (format decisions, issue 0004)

- **E1 UUID form.** Every entity id is an RFC 9562 UUID in canonical
  lowercase hyphenated textual form, exactly 36 characters (parser bound:
  §9.3 row L19; provenance P8d). Malformed ids reject
  (`invalid-field-syntax`).
- **E2 Version opacity.** Readers validate syntactic validity — well-formed
  version and variant bits — and nothing else. No rule may depend on the
  UUID version; substituting any RFC-valid version yields an equally valid
  document. This is the forward/backward-compatibility contract for all
  identifier-minting choices below.
- **E3 Minting default.** Writers mint **UUIDv7** for interactively created
  entities. Rationale (format decision): millisecond time-ordering gives
  lexicographic locality under the total order of E5 while remaining
  collision-free across devices. This is a writer default, not a validity
  requirement (E2).
- **E4 Derived-tag layout.** Deterministic generators derive a 16-byte tag
  by a formula owned by their own issue (for the v0.1 size corpus:
  ASSET-STRATEGY §10.9, which left the version/variant layout unspecified
  "until issue 0004 ratifies an identifier format"). That layout is hereby
  fixed: interpret the tag as the 128-bit big-endian UUID value; overwrite
  the four-bit **version** field — the high nibble of octet 6 (bits 48–51,
  numbering the most significant bit as bit 0) — with `1000` (UUIDv8), and
  the two-bit **variant** field — the two most significant bits of octet 8
  (bits 64–65) — with `10`. The remaining 122 bits carry the tag unchanged.
  Overwriting six bits is the explicit, documented cost of RFC 9562
  conformance; the derivation semantics (hash choice, input encoding) stay
  owned by the generator issue, which must account for the overwrite when
  pinning expected values. A generator requiring bit-exact tag recovery
  would have to store the raw tag outside the UUID; none currently does.
- **E5 Total order.** Wherever this specification compares stable UUIDs
  (for example the room-record association tie-break of BEHAVIOR-SPEC
  R5.2.4, whose physical realization it delegates to the format), the
  comparison is ascending order of the UUID interpreted as an unsigned
  128-bit integer, most significant octet first. This realizes the
  semantic total order independently of any textual or case-normalized
  form (which R5.2.4 explicitly declines to define).
- **E6 Stability and reuse.** An entity id is immutable for the lifetime of
  its entity and is never reassigned to a different entity. Deletion
  tombstones the id (§4.5). The sole reuse exception is authorized
  restoration of a tombstoned identity by the same device's own undo
  (BEHAVIOR-SPEC R8.1.3); tombstones created by sync merge are never
  resurrected by redo.

### 2.2 Collections and ordering

- **C1 Keyed collections.** Every collection of entities serializes as a
  JSON object keyed by entity id. Collection order does not exist
  semantically: readers MUST NOT attach meaning to key order, writers may
  emit keys in any order (ascending id recommended), and no future rule may
  infer semantics from serialization order.
- **C2 Intrinsic-order arrays.** Arrays are permitted exactly where order
  is intrinsic and specified: coordinate tuples (§3), polygon rings
  (derived data, §2.4), ordered operations inside a batch (§4.2), and the
  canonical-order set encodings of C3. Any other array in authoritative
  state is invalid input.
- **C3 Unordered-set encodings.** Fields whose semantics are sets serialize
  as arrays under a declared canonical order, and validators MUST treat
  permutations as equivalent while rejecting duplicates:
  - Elements carrying a member UUID (wall-join membership, wall
    style-reference tokens) sort by ascending member UUID (E5), ties by
    role/token byte order.
  - Elements without a member UUID (`level.backgroundImages`) sort by the
    lexicographic tuple `(digest, stackOrdinal, translationMm[0],
    translationMm[1], scaleUniform, rotationDeg)` — digest as an ascending
    byte string, `stackOrdinal` numerically, numeric anchor components
    under the IEEE-754 total order for binary64 after the §3.2 negative-
    zero canonicalization.
  In both cases the sort key covers EVERY serialized field of the element,
  so key equality coincides with element equality and duplicate elements
  reject deterministically. Canonical order makes diffing and any future
  hashing deterministic without making position semantic.
- **C4 Parser ceilings.** Generic array-element and object-member count caps
  are ratified reader envelopes (§9.3 rows L17/L18). v0.1 defines no
  additional semantic cap on the SIZE of these collections: §4.1 B7 fixes
  that, and the caps that constrain what a collection may CONTAIN are the
  schema constraints of §2.3. Magnitude bounds on numeric fields are the
  separate matter of §3.1.

### 2.3 Entity schemas

v0.1 defines eight entity kinds in two tiers. Gaussian Splats are excluded
entirely (ASSET-STRATEGY §10.9); no limit row implies a splat entity
exists.

Entity objects use FLAT top-level fields with two reserved keys, `id` and
`type`; `levelId` appears where the table says so. Field names never
collide with reserved keys. The flat shape is what `setFields` addressing
(§4.2 O3) means by "top-level".

All numbers in authoritative fields are finite binary64 values under §3.2;
all magnitude bounds are references to the named provisional parameters of
§3.1 — inline numeric bounds are forbidden in schemas.

**Tier A — behavior-normative kinds.** Constraints restate the wire shape
of rules that are normative in BEHAVIOR-SPEC; the cited rule owns the
semantics. Validation beyond structure happens at command time under those
rules, not at parse time.

| Kind | Fields | Wire notes / semantics owner |
|---|---|---|
| `level` | Req: `name` string; `elevationMm` finite with absolute value ≤ `coordBoundMm`; `floorThicknessMm` ∈ [0, `dimensionCapMm`]; `defaultWallHeightMm`, `defaultCeilingHeightMm` ∈ (0, `dimensionCapMm`]. Opt: `backgroundImages`, an unordered set of `{digest, anchor{translationMm:[x,y], scaleUniform > 0 finite, rotationDeg}, stackOrdinal ≥ 0}` entries; `displayOrder`, a whole number in [0, 2^53−1] (issue 0115; absent means no stored ordinal — §2.1's whole-list writers emit it for every level). | BEHAVIOR-SPEC §2.1–§2.3 (fields, constraints, warnings including the `displayOrder` bound), §2.2 (list order), §2.5 (anchor transform: p = t + s·R(θ)·q, degrees CCW), §2.7 (duplication). |
| `wall` | Req: `startMm`, `endMm` coordinate pairs whose components each have absolute value ≤ `coordBoundMm`, with `startMm ≠ endMm`; `thicknessMm`, `heightMm` ∈ (0, `dimensionCapMm`]. Opt: `styleRefs`, set of non-empty opaque tokens. | BEHAVIOR-SPEC §3 (R3.0.x conventions; style resolution discipline R3.0.6). No stored derived geometry. |
| `wallJoin` | Req: `members`, set of `{wallId, role ∈ {Start, End}}` with ≥ 2 unique entries; both endpoints of one wall prohibited. Stores NO coordinates and NO `levelId` (derives from members). | BEHAVIOR-SPEC §3.3 (R3.3.1–R3.3.7). Membership is an unordered set (C3). |
| `opening` | Req: `kind` ∈ {door, window}; `hostWallId`; `u` ∈ [0,1]; `widthMm`, `heightMm`, `depthMm` ∈ (0, `dimensionCapMm`]; `sillMm` ∈ [0, `dimensionCapMm`]; `side`, `hinge`, `opensToward` enums; `swingDeg` ∈ [0°,180°]. Opt: `mechanism` ∈ {doorHinged, doorSliding} for doors or {windowSideHung, windowDoubleSwing, windowLift} for windows; absent means doorHinged/windowSideHung respectively. Opt: `styleRefs`. Stores NO `levelId`: the effective level resolves through `hostWallId` at read time (BEHAVIOR-SPEC R4.0.2) and never changes during the opening's lifetime. | BEHAVIOR-SPEC §4 (R4.0.x–R4.3.x). Openings hosted by one wall form an unordered set (R4.0.6); nothing here may introduce order. |
| `roomRecord` | Req: `seedMm` coordinate pair whose components each have absolute value ≤ `coordBoundMm`. Opt: `name` string (absent and empty-string are distinct authoritatively and identical in display); `floorStyleRef`, `ceilingStyleRef` non-empty opaque tokens. | BEHAVIOR-SPEC §5.2 (R5.2.1–R5.2.8). See §2.5 for the reconciliation representation. |

The examples below form ONE coherent bundle: every id they reference is
defined inside the bundle, so each example is referentially valid under
O4, not merely structurally well-formed. Illustrative values; whitespace
and key order insignificant per §3.3.

```json
{"id":"019534c7-0000-7000-8000-000000000001","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500,"displayOrder":1,"backgroundImages":[{"digest":"sha256:6bcb425196e349a259d7d4861e76edf76bcb425196e349a259d7d4861e76edf7","anchor":{"translationMm":[0,0],"scaleUniform":10,"rotationDeg":90},"stackOrdinal":0}]}
```

```json
{"id":"019534c7-0000-7000-8000-000000000002","type":"wall","levelId":"019534c7-0000-7000-8000-000000000001","startMm":[0,2500],"endMm":[4000,2500],"thicknessMm":120,"heightMm":2500,"styleRefs":["S0"]}
```

```json
{"id":"019534c7-0000-7000-8000-000000000004","type":"wall","levelId":"019534c7-0000-7000-8000-000000000001","startMm":[4000,2500],"endMm":[8000,2500],"thicknessMm":120,"heightMm":2500}
```

```json
{"id":"019534c7-0000-7000-8000-000000000003","type":"wallJoin","members":[{"wallId":"019534c7-0000-7000-8000-000000000002","role":"End"},{"wallId":"019534c7-0000-7000-8000-000000000004","role":"Start"}]}
```

```json
{"id":"019534c7-0000-7000-8000-000000000005","type":"opening","kind":"door","hostWallId":"019534c7-0000-7000-8000-000000000002","u":0.5,"widthMm":900,"heightMm":2100,"sillMm":0,"depthMm":120,"side":"Left","hinge":"nearStart","opensToward":"Left","swingDeg":90,"styleRefs":["S0"]}
```

```json
{"id":"019534c7-0000-7000-8000-000000000006","type":"roomRecord","levelId":"019534c7-0000-7000-8000-000000000001","seedMm":[6000,4500],"floorStyleRef":"F0"}
```

**Tier B — format-reserved provisional envelopes.** The three kinds below
have complete typed wire schemas so that every v0.1 entity kind carries
required/optional fields, units, parameter-referenced bounds, reference
lifecycle, and a valid example. **Every non-reserved field of a Tier B
entity is a format decision of issue 0004**: no normative behavior section
yet defines creation, editing, validation beyond structure, undo grouping,
or synchronization for these kinds, and none is claimed here. Readers
enforce structural/type checks and the unknown-data rules of §8; all
further semantics are deferred and tracked as open questions. Nothing in
this table constitutes behavioral, numerical, or API ratification.

| Kind | Fields (all format decisions, issue 0004) | Open questions (draft TODOs, not decisions) |
|---|---|---|
| `label` | Req: `levelId`; `anchorMm` coordinate pair; `name` string (absent vs empty-string distinct authoritatively, identical in display — follows the R5.3.3/F37d pattern of BEHAVIOR-SPEC §5). | Font packaging and text shaping (§2.2 row); anchoring behavior when nearby entities move. |
| `dimension` | Req: `levelId`; `startMm`, `endMm` coordinate pairs — the STORED measured extent. Opt: `labelOverride` string ≤ 1,024 B (L19 parser bound only). | Attachment to moving entities via entity-referencing anchors; measurement-display semantics (§2.2 row). |
| `assetInstance` | Req: `levelId`; `digest` `sha256:<64 lowercase hex>` (identity per ASSET-STRATEGY §10.1); `transform` object `{translationMm:[x,y], scaleUniform (> 0, finite), rotationDeg}` — structurally analogous to, but NOT derived from or owned by, the BEHAVIOR-SPEC §2.5 background-anchor pattern. Opt: `displayName` string ≤ 1,024 B (L19). | Furniture/catalog scope (§2.2 row); placement interaction semantics. Descriptor-level behavior of `digest` (requirement class, placeholders, dangling-digest treatment) is owned by ASSET-STRATEGY §10.2/§10.6 and BEHAVIOR-SPEC §9.6 — cited, not restated. |

Tier B structural bounds only: finiteness, L19 string caps, §3.1 parameter
references. Examples:

The experimental editor's issue-0074 extension adds optional
`editorFootprintMm: [width, depth]` metadata to an asset instance, and
`editorAssetDirectories: [absolute directory path, ...]` to the manifest.
BEHAVIOR-SPEC §10.1 owns their meaning. They introduce no new entity kind,
required feature or container version. Existing readers preserve them via §8 U2
but may not draw the editor footprints. Instance footprint edits use ordinary
`setFields` operations and therefore replay and undo with the existing envelope.
Directory locations are package metadata, not retrieval hints or asset identity;
portable use of placed instances relies on embedded bytes, never those paths.

Issue 0082 adds optional `editorHeightMm` instance metadata: finite positive
unscaled height in millimetres, at most 1,000,000 mm. BEHAVIOR-SPEC §10.4 defines
initial sizing, resizing and legacy inferred height. It shares the existing
unknown-field preservation and `setFields` replay/undo rules. No container
version or required feature changes; older editors preserve height but may
infer it from width rather than display a non-proportional resize.

Issue 0109 adds optional instance metadata `editorElevationMm` (finite, within
the coordinate bound), `editorRotationXDeg` and `editorRotationYDeg` (finite
degrees), each defaulting to zero. BEHAVIOR-SPEC §10.4 defines the center pivot,
axis order and projected footprint. Z rotation remains `transform.rotationDeg`.
These use the same optional editor-metadata and ordinary `setFields` contract:
no new required capability or container version. Older editors preserve them
but display only the floor-plane transform, so are not accurate 3D-pose viewers.

```json
{"id":"019534c7-0000-7000-8000-000000000007","type":"label","levelId":"019534c7-0000-7000-8000-000000000001","anchorMm":[3000,1250],"name":"Kitchen"}
```

```json
{"id":"019534c7-0000-7000-8000-000000000008","type":"dimension","levelId":"019534c7-0000-7000-8000-000000000001","startMm":[0,0],"endMm":[4000,0]}
```

```json
{"id":"019534c7-0000-7000-8000-000000000009","type":"assetInstance","levelId":"019534c7-0000-7000-8000-000000000001","digest":"sha256:9a1f2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f9","transform":{"translationMm":[5200,3100],"scaleUniform":1,"rotationDeg":0},"displayName":"Armchair"}
```

Unknown entity kinds and unknown fields follow §8 exactly: refuse safely
when required data is unrecognized; preserve opaquely (bounded by L32) or
refuse write-back otherwise; silent stripping is forbidden.

**Optional solid appearance fields.** A `wall` may carry `color`, a seven-byte
string `#RRGGBB` representing opaque 8-bit sRGB (hex input is case-insensitive;
authoring commands emit uppercase). A `wall` may also carry `colorLeft` and
`colorRight` in the same representation, naming its `Left` (`+n̂`) and `Right`
(`−n̂`) faces as for openings. A face's effective color is its side field, else
`color`, else the view default. Authoring commands write a canonical form:
`color` alone when both faces agree, otherwise only the side fields. An
`assetInstance` may carry
`materialColors`, an object mapping nonempty, case-sensitive material keys of at
most 1024 UTF-8 bytes to the same color strings, with at most 256 entries. These
are recognized optional presentation fields, carried through the existing flat
field map; malformed values produce `invalid-appearance`. Absence means original
appearance; an empty palette has the same visual meaning. `setFields` writes the
whole palette; null removes either field under the ordinary clear-field rule.
BEHAVIOR-SPEC's solid-color section owns material identity and rendering.

Compatibility assessment: no entity kind, asset-root reference, required feature
or schema label changes (`0.1` and `0.2` remain supported). Older readers preserve
the optional fields under §8 U2 but do not display the overrides; a reader
that knows `color` but not the side fields shows a wall with differing faces in
its default finish. Color cannot
change geometry, references or editing safety; it does not reinterpret opaque
`styleRefs`. Reversing a wall swaps `colorLeft` and `colorRight`. Synthetic
tests cover opaque preservation, replay, undo/redo, split inheritance, reversal
and package roundtrips. User plans are not format fixtures.

For a door `opening`, optional `color` uses the same `#RRGGBB` representation,
and optional `doorStyle` is the string `solid` or `glass`. Absence means the
legacy solid finish; null in `setFields` clears the optional field. Invalid
door colors/styles produce `invalid-appearance`. These fields use the existing
flat field map and do not reinterpret `styleRefs`; they remain opaque on window
openings. The schema labels and required features are unchanged. Older readers
preserve these optional fields but display the legacy appearance. Synthetic
tests cover validation, atomic undo/redo, replay and package compaction.

### 2.4 Authoritative, derived, and cached data

The classification of BEHAVIOR-SPEC (R3.0.7, R4.0.5, §2.4, R5.2.1) binds:

- **Authoritative:** exactly the entity fields of §2.3 plus the operation
  metadata of §4. Nothing else is authoritative.
- **Derived:** wall/room polygons, occupied regions and join patches,
  measured areas, aperture footprints and cut geometry, room candidacy/
  association/status/labels/surfaces, triangulations, meshes, glyphs.
  Derived values are NEVER authoritative fields. They may be persisted
  only in a clearly non-authoritative cache namespace that old readers can
  ignore entirely, guarded by digest/version checks against stale reuse;
  a cache hit that fails its check is discarded, never trusted.
- **View-local:** previews, hover, tool state, active level, visibility
  flags. Never serialized as document state.

Polygon rings, where persisted at all, live in the cache namespace as
derived data; their arrays are intrinsic-order values (C2).

Splats remain excluded from v0.1 entirely (ASSET-STRATEGY §10.9); chunked
representation work precedes any splat entity.

### 2.5 Room reconciliation representation

Room identity is metadata-plus-seed reconciliation, exactly as normative
in BEHAVIOR-SPEC §5.2; the wire realization is:

- a `roomRecord` entity (Tier A) carrying only its authoritative fields;
- the seed's strict-interior candidacy test, effective association by the
  lowest-stable-UUID tie-break (realized physically by E5), and the four
  mutually exclusive statuses (`associated`, `superseded`,
  `seed-on-boundary`, `seed-in-wall`, `seed-in-exterior`) are DERIVED
  projection: recomputable from walls plus records, persistable only in
  the cache namespace, never authored by operations (R5.2.5);
- split/merge/disappear/reappear transitions follow R5.2.6 deterministically
  and emit zero operations; deletion prevents resurrection (R5.2.7);
- no wall or level mutation batch ever contains room effects because of
  reconciliation (R3.4.6/R5.2.5).

## 3. Coordinates and numbers

Status: **Normative for v0.1 within this section's scope:
units, orientation conventions, number representation, and the named
geometric magnitude parameters. Their VALUES are RATIFIED and
recorded with their reasoning in [NUMERICAL-MODEL.md](NUMERICAL-MODEL.md) §3;
see §3.1.**

- Logical unit: millimetres throughout authoritative fields; suffix `Mm`
  marks millimetre-valued fields.
- Plan plane: right-handed X/Y; screen transformation is a view concern
  (BEHAVIOR-SPEC §1.2) and never enters document state.
- Angles: degrees; zero direction is the +X axis of the plan plane;
  positive rotation is counter-clockwise. Field-specific ranges live in
  the schemas of §2.3 (for example `swingDeg` ∈ [0°,180°]).
- Non-finite values reject everywhere (`nonfinite-value`,
  `nonfinite-coordinate` — ratified diagnostics, BEHAVIOR-SPEC R3.0.4).

### 3.1 Named geometric magnitude parameters (single definition site)

These parameters are the ONLY spelling of geometric magnitude limits in
this specification. Their values come from NUMERICAL-MODEL §3 and are
**Ratified**. Schemas and rules
reference the names; no section may inline the values.

| Parameter | Meaning (single definition) | Value — RATIFIED |
|---|---|---|
| `coordBoundMm` | Maximum coordinate magnitude for every authoritative plan coordinate, including room-record seeds and dimension/label anchors. | **10⁷ mm.** ±10 km, a 20 km span. Coordinates are PLAN-LOCAL; no CRS or georeferencing transform is defined, so a survey import rebases into local coordinates and carries its datum separately. |
| `dimensionCapMm` | Inclusive upper cap for strictly-positive dimension fields (wall thickness/height, opening width/height/depth, level defaults) and for the non-negative `sillMm`. | **10⁶ mm.** |
| `derivedCoordBoundMm` | Upper bound for ideal derived vertices; a breach signals an implementation defect, never user-input rejection. | **`coordBoundMm` + 10⁶ mm = 1.1×10⁷ mm.** |

Violations above a parameter take the PROPOSED diagnostics
`coordinate-out-of-range` / `dimension-out-of-range`; diagnostic naming is
owned by later ratification and these tokens are recorded as proposals,
not commitments.

Limit row L14 and the named bounds are ratified. Split-point persistence uses
nearest binary64, not rational coordinate storage. Representable bounds,
precision, topology quantization, overflow, construction rounding, exact
predicate decisions and area display are defined by
[Numerical model](NUMERICAL-MODEL.md) §§3–7 and §12. The topology grid applies
to the arrangement, not to authoritative stored coordinates. Geometry APIs
must state these distinctions rather than infer exact constructions from an
exact predicate.

### 3.2 Number representation

- Stored values are IEEE-754 binary64. Decimal serialization MUST round-
  trip the stored binary64 exactly; the shortest round-trip form is
  RECOMMENDED but byte identity is not required (§3.3).
- Integer-typed fields (counts, ordinals) must fit signed 64-bit (ratified
  parser ceiling L13).
- **Negative zero:** validation maps −0 to +0, and conforming writers emit
  `0`, never `-0`, so equality comparisons on stored values are
  simultaneously numerical and bit-safe (format decision, issue 0004,
  aligned with NUMERICAL-MODEL §3; revisitable under ED-0014's validation
  gate if 0030 changes the underlying model).

### 3.3 Canonical JSON: explicitly NOT adopted for v0.1

Byte identity of JSON serialization is NOT required anywhere in v0.1.
Content digests apply ONLY to asset blob bytes (ASSET-STRATEGY §10.1);
package-level integrity artifacts belong to the container decision
(issue 0005). Two consequences bind:

1. No identifier — entity, batch, or operation — may be DERIVED from
   serialized JSON bytes (see §4.2 O1).
2. Any future rule that hashes logical document JSON MUST first adopt an
   explicit canonicalization standard by amending this section; until
   then, hash-over-JSON rules are invalid.

Parser/security ceilings ratified in §9.3 (L11 nesting depth 128, L12
number-token length, L13 non-finite/overflow, L17/L18 counts, L19 field
syntax) bind parsing independently and imply nothing about geometric
bounds.

## 4. Operations and batches

Status: **Normative for v0.1 within this section's scope (issue 0004):
logical operation, batch, counter, frontier, idempotency, tombstone, and
conflict-metadata semantics.** Physical framing and byte layout are owned
by issue 0005; framing violations surface as `malformed-framing` before
replay begins (§9.4).

**The operation log is not a CRDT.** It provides replay and synchronization
metadata only. The v0.1 conflict rule (§4.6) is deterministic per-field
last-writer-wins and is LOSSY: it may discard a concurrent value. Nothing
here provides convergence, and no wording in this specification may be
read as claiming CRDT or lossless-collaboration semantics. Real-time or
multi-user merge design would require separate mechanisms and conformance
vectors and is out of v0.1 scope.

### 4.1 Batches

- **B1 Atomicity.** A batch is the unit of atomic change: all of its
  operations apply or none do. No observer or replica ever projects a
  partial batch.
- **B2 Identity.** `batchId`: UUID (rules E1–E2). `actorId`: UUID naming
  the single-writer replica epoch that produced the batch — distinct from
  user identity; one user may operate several actors/devices (GLOSSARY
  Actor). Actor identity is stable for the epoch's lifetime.
- **B3 Counters (format decision, issue 0004).** Each actor maintains one
  monotonic counter starting at 1. A batch declares `counterBase` — the
  counter of its first operation — and thereby reserves the contiguous
  range `[counterBase, counterBase + len(operations) − 1]`. Operation
  `i`, counted from zero within `operations`, carries the OPERATION
  COUNTER `counterBase + i`; the operation counter is the operation's
  entry in that reserved range. A batch contains at least one operation:
  an empty batch is a malformed-framing reject (§4.3 row 4) because it
  would define an empty range and no frontier position. Within one
  actor's stream, reserved ranges are disjoint and strictly increasing,
  with no gaps between successive batches. Rationale: contiguous
  reservation lets causal metadata advance only at batch ends (B5), so a
  frontier can never observe part of an atomic batch, and gives every
  operation a globally unique dedup key `(actorId, operationCounter)`
  without a per-operation counter field.
- **B4 Ordered operations.** `operations` is an array; its order is
  intrinsic (C2) and is the application order within the batch.
- **B5 Frontier atomicity.** Causal metadata advances past a batch only to
  its END: after applying, `frontier[actorId] ≥ counterBase +
  len(operations) − 1`. Intermediate counters of a batch are never exposed
  in any frontier.
- **B6 No wall-clock dependence.** Ordering, conflict resolution, and
  deduplication use ONLY logical identities and counters. Timestamps, if
  present at all, are display-only metadata in RFC 3339 UTC fixed form
  (L19) and NEVER feed ordering, conflicts, or deduplication.
- **B7 Envelope limits, and the semantic caps v0.1 does not define
  (FIXED — ED-0105).** Reader-side resource caps are RATIFIED §9.3 row
  L23 (batches per package, operations per batch, total retained
  operations, snapshot-metadata bytes). L23 implies nothing about
  meaning: it bounds what a reader must be willing to allocate, not what
  a document may say.
  **v0.1 defines NO semantic cap on batch contents beyond the §2.3 schema
  constraints and the ratified L17/L18/L23 envelopes.** The reason is that
  no candidate cap carries semantic content. A batch of fifty thousand
  legitimate operations means exactly what a batch of five means, and
  atomicity (B1) is what makes its size a user's editing decision rather
  than the format's; a cap on it would reject a conforming document for
  being large, which is what L23 is already for and is already sized for.
  The caps that DO carry meaning are in §2.3 and stay there —
  `wallJoin.members` needing at least two unique entries, both endpoints
  of one wall prohibited, `u` in [0,1], the enum domains.
  Two caps previously deferred here close as DERIVED rather than chosen,
  in the shape row L33 uses: neither picks a quantity.
  - **`wallJoin` membership.** Members are unique `{wallId, role}` pairs,
    and §2.3 prohibits both endpoints of one wall in the same join, so a
    wall contributes at most ONE member and a join on a document holding
    `w` wall entities can hold at most `w` members. L17 bounds the
    serialized array independently; whichever is smaller binds. No third
    number is needed, and one would have to be wrong somewhere: a cap
    below `w` rejects a join a user can legitimately build.
    The `2w` a reader might expect from the two-valued role is NOT the
    bound, and getting that wrong in the permissive direction is the
    dangerous mistake — the endpoint prohibition is what halves it.
  - **Frontier actor count.** The semantic cap EQUALS L18. An actor names
    a replica epoch (B2), so the count is a function of how many epochs
    have ever written the document, which is history rather than state,
    and any tighter cap rejects a legitimate long-lived document for
    having been edited on enough devices. Row L33's revisit trigger asks
    what this issue fixes the cap to; the answer is L18, so L33's derived
    maximum stands unchanged and that trigger is DISCHARGED rather than
    left stale.
    The reversal gate is B2 rather than a number: if an implementation
    ever mints a fresh actor epoch per session instead of keeping epoch
    identity stable, L18 becomes a ceiling a document can reach by being
    opened often, and the defect to fix is the minting, not the cap.
  Physical batch framing and batch byte size are deferred to issue 0005.

### 4.2 Operations

- **O1 Fields.** Each operation carries `operationId` (UUID, E1–E2),
  `kind`, `entityId`, and a kind-specific payload. `operationId` is an
  EXPLICIT field (format decision): deriving identifiers from serialized
  bytes would violate §3.3 consequence 1.
- **O2 Kinds (v0.1).** `createEntity` — payload is the complete entity
  object per §2.3. `setFields` — payload object mapping top-level entity
  field names to new values; absent payload fields leave the entity
  unchanged; `null` clears an optional field to absent. `deleteEntity` —
  no payload. Unknown kinds refuse safely per §8: the containing batch
  rejects before any effect.
- **O3 Field-addressing granularity (format decision).** `setFields`
  addresses TOP-LEVEL entity fields by serialized name. Set-valued and
  array-valued fields (C3 membership/style sets, `backgroundImages`) are
  replaced WHOLESALE, never merged element-wise; concurrency therefore
  resolves at field granularity (§4.6), never at element granularity.
- **O4 Referential integrity at commit.** Commit-time validation (owned by
  the BEHAVIOR-SPEC sections per kind; malformed-log classes per §9.4)
  rejects, before any effect: unknown `levelId`/`hostWallId`/membership
  targets, duplicate ids, self-references where prohibited, and asset
  digests malformed in syntax. Rejection is batch-atomic (B1) with zero
  effects and no undo entry (the R3.1.6/R4.1.1 pattern). O4 governs the
  AUTHORING replica at command time; resolution of targets that are
  absent at REPLAY time despite a committed batch is the separate,
  deterministic stale-batch policy of I5.

### 4.3 Failure taxonomy (distinct classes, never conflated)

| Class | Examples | Observable outcome |
|---|---|---|
| User-input validation failure | `nonfinite-value`, `zero-length-wall`, `invalid-opening-kind`, unknown targets (O4) | Whole batch rejects atomically BEFORE any effect; state unchanged; no undo entry; diagnostic names the user-fixable cause. |
| Warning-class condition | duplicate level elevations; ceiling height below wall height; parallel area overlap | Batch COMMITS; warning surfaces alongside the effects (semantics owned by BEHAVIOR-SPEC §§2–4). |
| Derived-geometry failure | `geometry-indeterminate` (proposed name; NUMERICAL-MODEL §4.6) | The derivation aborts and THE WHOLE BATCH REJECTS ATOMICALLY with zero effects — an implementation-facing nonconformance signal, expected unreachable for valid inputs, categorically DISTINCT from user-input rejection and from warnings. Never a silent substitution, tolerance, or partial output. |
| Framing/log anomaly | duplicate batch ids, overlapping counter ranges, torn/partial batch, inconsistent frontier | Reject BEFORE replay begins (`malformed-framing`, §9.4; physical framing classes owned by issue 0005). |

### 4.4 Causal frontier

- **F1 Shape.** `frontier` is an object mapping actorId → highest counter
  of that actor observed. Absent actor: nothing observed. Entries advance
  only at batch ends (B5). Because B3 makes an actor's ranges contiguous
  from 1, `frontier[a]` is exactly the NUMBER of that actor's operations
  observed, so the sum over all entries is the number of operations in the
  batch's causal past — F3's `causalPastSize`.
- **F1 Zero prohibition.** A frontier member whose counter is zero is
  malformed input. The PRIMARY rejection diagnostic is
  `invalid-field-syntax` at the schema-validation stage (§9.5 outermost-
  stage precedence). "Schema validation" denotes the pipeline stage where
  field-level value constraints are enforced—`StageSchema` for snapshots
  and `StageFraming` for batches in v0.1's implementation, already used for
  field syntax—not a requirement for JSON Schema documents. A reader that
  cannot detect zero at this stage emits
  `malformed-framing` at frontier construction as a fallback. F1 defines
  absence as nothing observed; zero observed is a structural contradiction:
  under B3, an actor's counter
  stream starts at 1 and advances only at batch ends, so zero cannot be a
  valid watermark for any stream state. Readers MUST reject a frontier
  carrying a zero-valued member before constructing watermark state,
  performing closure validation, or storing the frontier in any position
  table (§5.1 S3). Absence (actor omitted from the frontier map) is the
  sole representation of "nothing observed"; zero is not a permitted
  alternative spelling.
- **F2 Dominance.** Frontier A dominates B iff A[a] ≥ B[a] for every actor
  a in either frontier.
- **F3 Delivery order independence.** Batches may arrive in any order. A
  batch whose causal prerequisites — every batch its frontier points to —
  are not yet present is held until they arrive. Projection is DEFINED
  over the causal closure of the delivered batch set as follows: batches
  apply in THE CANONICAL ORDER, the ascending sort of the delivered
  batches by ORDERING POSITION P = (causalPastSize, counterBase,
  actorId), compared lexicographically, where `causalPastSize` is the
  number of operations in the batch's causal past — the sum of its
  frontier's counters (F1), exact because of B3's contiguous ranges and
  F4's closure requirement. P is a SORT KEY, not a set of pairwise
  precedence constraints, and that distinction is why this order exists at
  all: a rule of the form "causality, with incomparable pairs ordered by
  ascending counter" states constraints that can form a CYCLE and then
  define no order, whereas a sort by one key with unique values cannot.
  Two properties make the sort correct. P is unique per batch, because B3
  makes (counterBase, actorId) unique, so exactly ONE canonical order
  exists. And a batch's causal past strictly contains the past of every
  batch it observes, so `causalPastSize` strictly increases along
  causality and the sort is always a linear extension of it. The canonical
  order is therefore a function of the delivered SET, never of arrival
  sequence, and arrival order cannot change the projected result (vectors
  V2, V7). Holding is an implementation strategy for respecting causality;
  it does not make application order semantic. P is DERIVED — no batch
  field carries it. `causalPastSize` is bounded by L18 members of L13
  counters, so it may exceed int64: implementations compare it EXACTLY,
  and wrapping or saturating arithmetic is FORBIDDEN because it would
  change the order. No serialized member carries the sum (§5.1 S3).

- **F4 Frontier closure (authoring requirement, with a stated verification
  limit).** A conforming writer MUST emit TRANSITIVELY CLOSED frontiers: a
  batch's frontier dominates (F2) the frontier of every batch it observes,
  includes that batch's end counter, AND includes the writing actor's own
  prior end counter. Every entry is required on the same terms; omitting
  one is a closure violation. Closure is what makes F3 sound: an omitted
  entry under-reports `causalPastSize`, which can invert a comparison
  against a CONCURRENT batch that a complete frontier would have won — a
  batch whose closed past is 45 reports 5 with its own entry dropped and
  loses to a concurrent batch at 30 it should beat. Omitting the OWN-actor
  entry preserves one thing only, that actor's own STREAM ORDER, because
  `counterBase` is P's second component; it is not otherwise harmless.
  The obligations are:
  1. **Authoring.** The writer validates closure when it mints a batch.
     Emitting a non-closed frontier is a writer defect, not a reader's
     problem to absorb.
  2. **Reading, ancestry retained.** A reader MUST validate closure for
     every retained batch whose observed ancestors are also retained. A
     violation is an inconsistent frontier and rejects `malformed-framing`
     before replay (§4.3 row 4, §9.4).
  3. **Reading, ancestry compacted.** Closure is NOT provable for a batch
     whose observed ancestors are no longer retained, and a snapshot-only
     package admits no closure proof at all. No reader may claim
     otherwise. This limit is recorded deliberately: F3's ordering rests
     on an authoring guarantee that a compacted reader verifies only in
     part.

### 4.5 Idempotency, duplicates, tombstones

- **I1 Dedup key.** `(actorId, operationCounter)` uniquely identifies an
  application slot (B3). `operationId` is identity for humans and
  diagnostics; dedup uses the pair.
- **I2 Duplicates.** A delivered batch whose entire counter range lies at
  or below the applying replica's per-actor watermark is already applied:
  ignore it silently and deterministically — no effects, no error. I2
  suppresses EFFECTS only and is not a structural exemption: watermark
  classification never precedes, gates, or replaces the pre-replay
  structural checks §4.3 row 4 applies to the retained log, and a
  retained range at or below the watermark is exempt from none of them.
  WHICH checks those are, and over which entries, is owned by §7.6
  (issue 0005).
- **I3 Tombstones.** `deleteEntity` writes a TOMBSTONE:
  `{entityId, deletingPosition, creationPosition}` where
  `deletingPosition` is the delete operation's own write position (LWW1)
  and `creationPosition` is that of the create whose interval it closes,
  so the entity's live interval survives deletion. Tombstones persist into
  snapshots (§5.1 S4) and block resurrection: a write whose causal
  position follows a visible tombstone targets a nonexistent entity and
  is resolved by the stale rule of I5 — deterministically, without
  recreating anything (the R5.2.7 discipline generalized to all kinds).
- **I4/I5 Missing targets during replay — exact policy.** Commit-time
  validation (O4) runs on the authoring replica; a committed batch can
  therefore still meet an absent or tombstoned target at its canonical
  application position, through concurrency only. The policy is:
  - **I5 Stale-batch discard.** When a batch applies at its canonical
    position, any operation targeting an entity that is absent or
    tombstoned at that position makes the ENTIRE BATCH discarded as
    STALE — atomicity (B1) forbids partial application of the remaining
    operations. Discard is deterministic, silent to end users (it is a
    projection outcome, not an error), recorded in projection
    diagnostics, and never resurrects anything. Presence is evaluated at
    the OPERATION's write position (LWW1), not at its batch's position:
    an entity created by an earlier operation of the same batch is
    present for the later ones (B4). After compaction the same test runs
    against the LIVE INTERVAL a snapshot retains for each entity —
    `[creationPosition, deletingPosition)` from S1 and S4. The interval
    test governs operations that REQUIRE A PRE-EXISTING TARGET
    (`setFields`, and any later kind carrying that requirement); it
    classifies each such operation's target independently, at that
    operation's own write position, in exactly THREE ways rather than
    two:
    1. **Before creation** — the target's creation position is not
       earlier than the operation's write position, so the target is
       absent at it;
    2. **Within the live interval** — the target exists at that
       position;
    3. **After deletion** — the target's deleting position is earlier
       than the operation's write position, so the target is tombstoned
       at it.
    The two kinds that DEFINE an interval are not classified by it.
    `createEntity` ESTABLISHES existence at its own position: it is the
    creation rather than something preceding one, so it is never stale
    for want of its own target; a CONCURRENT collision with another
    create on the same id — neither create observing the other — resolves
    under LWW4, whose scope is exactly that case. A causally ordered
    same-id create is not a LWW4 collision: the later author observed the
    first create, so O4 local validation rejects it at authoring time.
    A create positioned after a tombstone for that id does not resurrect
    it, with the single exception E6 already names: authorized
    restoration of a tombstoned identity by the same device's own undo
    (BEHAVIOR-SPEC R8.1.3, §4.8). Tombstones
    created by sync merge are never resurrected (I3, E6).
    `deleteEntity` against a live target CLOSES the interval at its own
    position; against a target absent or already tombstoned at that
    position it is stale on the same terms as branch 1 or branch 3.
    The BATCH outcome then follows from its operations, never from a
    single classification of the batch: if ANY operation is stale under
    the rules above, the WHOLE batch is discarded — including operations
    whose own targets were live — because atomicity (B1) forbids partial
    application, and a branch-3 discard carries I3's no-resurrection
    rule. Only when NO operation is stale does the batch apply, and then
    its writes to targets that a canonically LATER delete has since
    tombstoned are superseded by that delete (LWW3) and produce no field
    state, while its writes to other targets take effect normally. The
    two-way reading — present-or-absent in the CURRENT projection —
    makes branch 2 indistinguishable from branch 3 and silently changes
    an unrelated field's value; it is not conforming.
  - Consequences, all deterministic: concurrent create-versus-modify on
    one id resolves by canonical order (V7); concurrent modify-versus-
    delete resolves to deleted when the delete canonically precedes
    (I3/LWW3), and to modified-then-deleted = deleted when it follows.
  - Structurally malformed logs (overlapping ranges, torn batches,
    unknown kinds) are framing rejects under §4.3 row 4 BEFORE replay,
    never partially applied.

### 4.6 Conflict model: deterministic per-field LWW (lossy)

- **LWW1 Winner position.** For each top-level mergeable field, the
  winning write is the one with the greatest WRITE POSITION: F3's batch
  position refined by the operation's index within its batch,
  (causalPastSize, counterBase, actorId, operationIndex), compared
  lexicographically, with actorId as an unsigned 128-bit integer (E5).
  The leading three components are exactly F3's P, so write positions
  never interleave two batches' operations (B1) and the winner is exactly
  the LAST write to that field in canonical order, ordered inside a batch
  by `operations` array order (B4). CAUSALITY GOVERNS: a write whose
  batch observes another write's batch has a strictly greater
  `causalPastSize` and wins regardless of counters. The remaining
  components decide ONLY between writes of EQUAL `causalPastSize`. For
  writes in DISTINCT batches, equal past sizes imply the two writes are
  CONCURRENT — had either batch observed the other, its past would be
  strictly larger. Two writes in the SAME batch also share that batch's
  past size, but they are ORDERED rather than concurrent (B4): the
  `operationIndex` component orders them and no concurrency claim
  attaches. So the counter tie-break separates CONCURRENT CANDIDATES only
  across batches, B4's array order governs within one, and neither ever
  overrides causal order; for a single-operation batch the cross-batch
  tie-break is exactly I1's pair K = (operationCounter, actorId). Two
  ORDERED writes by the SAME actor to one field therefore never tie: a
  later batch has a strictly greater past size, and a later index within
  one batch a greater position (B4).
  Across actors the write position totally orders writes WITHOUT wall
  clocks, and the tie-break among equal-past-size writes is
  arbitrary-but-deterministic by construction.
- **LWW2 Honest limitation.** The losing concurrent value is DISCARDED.
  Documentation and UI must not describe this as lossless collaboration;
  surfacing conflicts to users is a behavior-layer concern outside this
  specification.
- **LWW3 Deletes dominate.** Delete-versus-set resolves to DELETED: a
  tombstone beats any concurrent field write on the same entity.
- **LWW4 Create collisions.** Two concurrent `createEntity` operations for
  the SAME id from different actors (each locally valid under O4) resolve
  by the same total order: the greater write position is kept; the loser
  is recorded as an anomalous-discarded create. Within one actor stream
  the situation is impossible (local validation).
- **LWW5 Snapshot cooperation.** Snapshots retain winner positions (§5.1
  S3) precisely so that late-concurrent operations resolve identically to
  a never-compacted replica (vector V5).

### 4.7 Deterministic projection vectors

Conformance FIXTURES belong to issue 0028; the EXPECTED projections are
fixed here and are deterministic in all cases:

| Vector | Setup | Required deterministic projection |
|---|---|---|
| V1 Duplicate delivery | same batch delivered twice | Second occurrence ignored (I2); state and frontier identical to single delivery. |
| V2 Reordered delivery | batch B causally after A arrives first | B held until A arrives (F3); final projection equals in-order delivery exactly. |
| V3 Concurrent set/set | actors X, Y write the same field; neither observes the other | Winner per LWW1; loser value discarded; outcome independent of arrival order. |
| V4 Delete versus set | X deletes the entity; Y concurrently sets a field | Deleted (LWW3); Y's write discarded; tombstone retained. |
| V4b Stale set after observed delete | Y's write is causally AFTER X's delete reached Y | Discarded as stale (I3); no resurrection. |
| V5 Late-concurrent after compaction | an operation arrives whose batch predates snapshot coverage but whose write position loses to a snapshot-retained winner position | Resolved via retained winner positions (LWW5/S3): identical outcome to the uncompacted replica. Operations at or below snapshot watermark counters are skipped (I2). |
| V6 Snapshot replay | rebuild projection from snapshot + subsequent log | Projection equals full-history replay for all authoritative state; derived data rebuilds identically; no byte-identity claim about serialization (§3.3). |
| V7 Concurrent create versus modify | X creates entity E; Y, not observing X's create, sets a field on E | Resolved by canonical order (F3/I5): if Y's batch canonically precedes the create, it is discarded STALE in its entirety (target absent); if it follows, it applies. Outcome is a function of the delivered SET only, never of arrival order. |

V3's stated precondition is that neither write observes the other; it is
silent on causal-past size. Its required projection "Winner per LWW1" is
therefore unchanged and resolves through LWW1 as written above, so where
two concurrent writes have UNEQUAL past sizes the past size decides. The
ROW is unchanged; a conformance fixture built on the earlier reading of
LWW1 needs its expected value re-derived (issue 0028, ED-0037).

Evidence discipline for these vectors: a hand-built adversarial witness is
what DISCRIMINATES a correct rule from a plausible one, while a large
random aggregate is soundness evidence for the rule actually adopted and
never proof that a rejected alternative fails. A conformance report showing
only aggregate agreement has not exercised these vectors, and neither has a
fixture whose checker does not read the projection it replays, however
complete that fixture's recorded expectation looks.

### 4.8 Undo/redo boundary (separation, not restatement)

Local undo/redo stacks are session-local device state (BEHAVIOR-SPEC §8,
normative) and are NOT part of the durable format. Executed undos/redos
append NEW compensating batches with freshly minted batch/operation ids
and a fresh actor-counter advance (R8.1.2); history itself is never mutated
or reordered. External/replayed batches clear local stacks (R8.3.1) and
never enter them. Authorized redo restoration of tombstoned entity ids
follows R8.1.3 (rule E6).

### 4.9 Batch example (normative shape; bytes insignificant per §3.3)

```json
{
  "batchId": "01948f5b-c4e4-7e42-b996-f552a18df77f",
  "actorId": "01948f5a-ca77-7a09-9e76-5ace11b12fd2",
  "counterBase": 42,
  "frontier": {
    "01948f5a-ca77-7a09-9e76-5ace11b12fd2": 41
  },
  "operations": [
    {
      "operationId": "01948f5d-6e18-7c01-8f0d-77f974bd8036",
      "kind": "setFields",
      "entityId": "01948f55-6b18-7d3a-a153-1ee2f33cc402",
      "fields": {"endMm": [4000.0, 2500.0]}
    }
  ]
}
```

The example batch reserves the counter range starting at 42; its single
operation (index 0) carries operation counter 42 + 0 = 42, so I1's dedup
pair is K = (42, actorId); the batch's ordering position is
P = (41, 42, actorId) and that operation's write position is
(41, 42, actorId, 0), because the frontier's counters sum to 41 — 41
operations precede the batch causally (F1/F3/LWW1). The frontier records
observation of that actor through counter 41.

## 5. Snapshots, retention, and compaction

Status: **Normative for v0.1 within this section's scope (issue 0004):
snapshot CONTENTS and the SEPARATION of retention concerns.** Compaction
thresholds and schedules are implementation policy owned by issues
0005/0027/0029; only the observable guarantees below belong to the format.

### 5.1 Snapshot contents

A snapshot contains ALL of:

- **S0 Document identity (issue 0116)** — the package's stable `documentId`,
  a UUID v7 absent in documents created before this member existed. It is a
  FACT about the projected document, not per-entity metadata: it is minted
  ONLY by new-plan entry points (BEHAVIOR-SPEC §2.2 lists them), no user
  command may set or edit it, and every save, package conversion and
  compaction carries it verbatim. A reader MUST accept a snapshot without
  one (the pre-0116 shape) and MUST NOT treat identity by any other
  numeral invented for a nameless document; per-device consumers
  (BEHAVIOR-SPEC §2.6) that find none fall back to their elevation-derived
  defaults instead.
- **S1 Projected authoritative state** — every live entity per §2.3, plus
  `createdPositions`: for each entity within the horizon, the write
  position (LWW1) of the `createEntity` operation that currently owns the
  id. It is retained for TOMBSTONED entities as well as live ones,
  because a tombstone alone cannot distinguish a late batch positioned
  BEFORE the creation from one positioned inside the live interval (I5
  branches 1 and 2); treating creation metadata as live-entity-only is
  exactly what collapses those branches. Also required so LWW4's create
  collisions can be re-decided against a late concurrent create. This
  resolves the `CreatedKeys` question routed here from issue 0011's
  review: the member is enumerated, and the retained datum is a POSITION,
  not a bare `(operationCounter, actorId)` pair.
- **S2 Frontier** — the causal frontier (§4.4) of ALL operations the
  snapshot covers, serving simultaneously as coverage record and as the
  per-actor duplicate watermark for I2.
- **S3 Winner positions** — for every mergeable top-level field written
  within the retained competing-write horizon (named parameter
  `lwwHorizon`, **PROPOSED at COMPLETE COVERAGE — ED-0105, awaiting an
  explicit owner ruling**: the horizon is
  exactly what the snapshot's S2 frontier covers, with no count, age, or
  byte window inside it. Complete retention is the SAFE MINIMUM: it loses no
  data, and no smaller value is available without changing §4.7 V5. It is a
  PROPOSED CONTRACT, not a description of any existing reader — no
  implementation retains S1 creation positions or S3 winner positions at all
  today, so nothing currently implements this horizon or any other. What is
  unsettled is fixing it as v0.1 product policy, which remains an owner
  decision), the winning WRITE POSITION of LWW1, keyed by the
  written field. ALL FOUR components are required, the leading
  `causalPastSize` included: an (operationCounter, actorId) pair alone
  cannot place a later-arriving concurrent write relative to the retained
  winner, and it is not recoverable after compaction — for a
  multi-operation batch it does not even determine `counterBase`. Because
  the write position is intrinsic to each write and totally ordered, the
  greatest position over a set equals the greatest over its parts, so ONE
  retained position per field is sufficient, no losing value is ever
  needed again, and LWW2's discarding stands.
  One economy, derived rather than granted (ED-0105): a field whose
  current value was written by the `createEntity` operation that owns the
  id, and never written since, needs NO entry here, because S1 already
  retains exactly the position a late write to that field must be
  compared against. A reader MUST fall back to the S1 creation position
  for a mergeable field absent from `winnerPositions`, and MUST NOT read
  that absence as "no winner". This is not a narrowing of the horizon —
  the position is retained, in the member that already had to carry it —
  and it keeps a snapshot of an unedited document from restating one
  position once per field.
  Exactness, checked in both directions: a late write positioned AFTER
  the creation wins under LWW1, which is what full replay gives, since
  the create is the only prior write to that field; and one positioned
  BEFORE it targets an entity that does not yet exist, so I5 branch 1
  discards its whole batch rather than consulting any winner at all.
  CLEARING COUNTS AS WRITING. A `setFields` that sets a field to `null`
  is a write and takes an entry, so an optional field that is absent from
  the entity because it was cleared is NOT eligible for this economy —
  only a field still holding the value its create gave it. Reading a
  cleared field's absence as eligibility would compare a late write
  against the creation rather than against the clear, and could restore a
  value the clear had already beaten.
  Representation, exactly one shape: the snapshot carries
  `batchPositions`, mapping each referenced batch's §7.6 entry stem
  (`<counterBase padded to 19 digits>-<actorId>`) to that batch's
  `frontier` (the S2 shape, per-actor counters within L13). Positions
  elsewhere — `winnerPositions` keyed `"<entityId>.<fieldName>"`,
  `createdPositions` keyed by entity (S1), and S4's tombstones — carry
  only `{"batch": "<stem>", "operationIndex": n}`. `counterBase` and
  `actorId` are read from the stem, and `causalPastSize` is the sum of the
  referenced frontier's counters, so no member carries an integer outside
  L13/§3.2. `batchPositions` contains EXACTLY the batches those maps
  reference: no unreferenced entries, and a reference to an absent stem is
  absent required data and rejects (§9.4). Sharing through this table is
  the ONLY permitted encoding; a second one would fork readers, which is
  the divergent-reader risk §9.4's Rule T1 rejects. Row L23's
  snapshot-metadata ceiling binds the total, and the bytes this adds are
  an input to issue 0035's re-measurement — and, under ED-0105's horizon,
  the only thing that bounds retention at all, which is why L23's refusal
  path in §5.2 is load-bearing rather than theoretical.
  LEGACY (ED-0038): a snapshot carrying the pre-ED-0037 `winnerKeys`,
  `createdKeys`, or `deletingKey` members instead is a legacy snapshot.
  The two encodings are EXCLUSIVE. A snapshot carrying members of both —
  any position-bearing member alongside any pre-ED-0037 member — is
  neither a conforming snapshot nor a legacy one, and REJECTS: it is
  ambiguous required data, handled as §9.4 handles every other ambiguous
  structure, and no reader may convert it partly.
  Its ACTOR UNIVERSE is the union of the actors named in its frontier and
  those named by its retained winner, creation, and tombstone keys: a key
  actor need not appear in the frontier, so the frontier alone does not
  determine the set. With exactly one actor in that union it is read under
  ONE documented
  convention — each retained winner is treated as its own one-operation
  batch, giving `causalPastSize = operationCounter − 1`,
  `counterBase = operationCounter`, and `operationIndex = 0`. V5 exactness
  is explicitly NOT claimed under that convention, and the first save
  REWRITES exact positions, so the ambiguity cannot outlive one read. With
  two or more actors in that union the winners cannot be placed at all:
  the snapshot is fail-closed, exactly migratable only for a field whose
  full relevant write history is retained or is attested, and otherwise
  rejected. Two cases the actor universe does not settle. An EMPTY legacy
  snapshot — empty entities, empty frontier, and no winner, creation, or
  tombstone entries, which is §7.5's brand-new document — has nothing to
  place: it reads as the empty projection and no convention applies. A
  legacy snapshot carrying ANY tombstone is fail-closed unless the
  deleted id's creating batch is still retained: the pre-ED-0037 shape
  records a `deletingKey` but, because creation metadata was
  live-entity-only, no creation key for a deleted id, so I5's live
  interval cannot be reconstructed. No convention may invent it, because
  guessing the creation side would apply a late batch positioned before
  the true creation where full replay discards it whole — a silently
  wrong projection, which is worse than a refusal. These are what make
  late-concurrent operations resolve identically after compaction (V5).
- **S4 Tombstones** — `entityId` + the DELETING POSITION (the write
  position, LWW1, of the `deleteEntity` operation) for deletes within the
  same horizon, AND the creation position of the same id (S1), so the
  live interval `[creationPosition, deletingPosition)` survives deletion.
  Together they prevent resurrection by late or reordered deliveries and
  let I5 place a late batch in one of its three branches. A bare
  `deletingKey` of the pre-ED-0037 shape is insufficient for exactly the
  reason S3's winner key is, and a tombstone without the creation
  position collapses I5 branches 1 and 2.
- **S5 Batch/dedup metadata** — recently retained batch identities
  sufficient to detect duplicates among operations still arriving (I2),
  bounded by the ratified snapshot-metadata envelope (L23).

**The horizon, and why complete coverage is the safe minimum (ED-0105,
PROPOSED).** The horizon is a DERIVED extent rather than a chosen quantity
under this proposal: it equals the snapshot's
own coverage, so S1 retains a creation position for every id the snapshot
carries live or tombstoned, S3 a winner position for every mergeable field it
carries, and S4 a tombstone for every delete it absorbs. Three things forced
this rather than recommending it. A window smaller than coverage knowingly
breaks §4.7's V5 for inputs that fall outside it, and V5 is a fixed
conformance vector rather than a tunable — a horizon parameter that can be set
to a conforming value and a nonconforming one is not a parameter. Tombstones
cannot be dropped on age at all without S6's drop precondition, which v0.1
supplies no protocol to attest, so a window over them would be unsound
independently of its value. And the metadata a complete horizon costs is
bounded by row L23 with an already-specified observable when it does not fit,
so the growth is bounded by a rule that is already observable rather than by a
window. What it COSTS is not established: no implementation retains S1/S3
positions yet, so the metadata a complete horizon requires has never been
measured, and row L23's snapshot-metadata cap names no byte class for any
measurement to be compared against — §9.2 requires every row to name the
quantity it binds, and that clause does not. Both are open, and neither is a
reason to retain less in the meantime.

**Retaining MORE than the horizon requires is never an error.** A reader MUST
accept a snapshot carrying winner, creation or tombstone entries it would not
itself have written, and MUST NOT infer anything from their presence. This is
stated so the horizon can be narrowed later without breaking readers built
against this text: a narrowing would then be a writer-side change. It does not
relax S3's separate, referential exactness — `batchPositions` still contains
exactly the batches those maps reference, no more.

What that costs, stated without rounding it down. The NUMBER of retained
positions is bounded by the projected state for live entities — one creation
position per entity, one winner position per mergeable field — and by
DELETION HISTORY for tombstones, which accumulate one per absorbed delete and
are not a function of current state. The BYTES are not a constant factor on
either: each referenced batch carries a frontier whose member count grows with
the number of actors that have ever written the document, up to row L18, and
that count is history rather than state as well. Row L23's snapshot-metadata
ceiling bounds the total, so a document whose required metadata will not fit
within L23 cannot compact, and that refusal is observable under §5.2 — S6 for
a synced document, S7 for a local-only one. Retention of the operation LOG is
a separate matter and is governed by §5.2.

A snapshot is NOT interchangeable with a full audit history: winner positions
for writes the snapshot no longer covers, discarded loser values, and
historical states are recoverable from it alone by no reader.

Example (normative shape, and complete under the ED-0105 horizon rather
than eliding anything). One actor has written 41 operations; the level was
created by the batch reserving `[41,41]`, whose own frontier records the 40
operations that precede it, and no field has been written since. Every
mergeable field therefore takes its winner from the S1 creation position and
`winnerPositions` is legitimately empty:

```json
{
  "entities": {
    "019534c7-0000-7000-8000-000000000001": {"id":"019534c7-0000-7000-8000-000000000001","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}
  },
  "frontier": {"01948f5a-ca77-7a09-9e76-5ace11b12fd2": 41},
  "batchPositions": {"0000000000000000041-01948f5a-ca77-7a09-9e76-5ace11b12fd2": {"01948f5a-ca77-7a09-9e76-5ace11b12fd2": 40}},
  "winnerPositions": {},
  "createdPositions": {"019534c7-0000-7000-8000-000000000001": {"batch":"0000000000000000041-01948f5a-ca77-7a09-9e76-5ace11b12fd2","operationIndex":0}},
  "tombstones": {}
}
```

Read back through S3: the stem gives `counterBase` 41 and the actor, the
referenced frontier sums to `causalPastSize` 40, and `operationIndex` is 0,
so the creation position is `(40, 41, 01948f5a-…, 0)` — the four components
LWW1 compares, none of them stored as a number outside L13.

### 5.2 Retention concerns — four, never conflated

| Concern | Owner | Persisted in the format? |
|---|---|---|
| Local session undo/redo stacks | BEHAVIOR-SPEC §8 (device-local state) | NO — never persisted; cleared on reopen (R8.4.2) |
| Sync retention — operations peers may still need | THIS SECTION + future protocol policy | Yes; deletion gated by S6 |
| Audit retention — full permanent history | NONE in v0.1 | Explicitly out of scope |
| Compaction thresholds — when and what to drop | Implementation policy (issues 0005/0027/0029) | Observable guarantees only (S7/S8) |

- **S6 Synced-document compaction.** A batch may be dropped from a synced
  document ONLY when no undelivered batch can still be positioned before
  it (owner ruling ED-0039). This is a PRECONDITION on dropping, not a
  recommendation: a writer that cannot establish it MUST NOT drop the
  batch. Receipt of a batch by a peer does NOT by itself establish the
  precondition, and it may not be inferred from receipt: an acknowledging
  peer may still hold a batch it created BEFORE acknowledging and has not
  delivered, and that batch has a smaller causal past than the
  acknowledged range, so it sorts before it. An acknowledgement
  establishes the precondition only where the sync protocol attests all
  three of: every peer capable of producing batches has acknowledged the
  range; each acknowledging peer has delivered every batch it created
  before its acknowledgement; and every actor created later is bound to
  observe the range before producing batches. Transfer of the successor
  snapshot to peers does not establish it either, for the same reason,
  so that branch is not sufficient by itself. v0.1 defines no sync
  protocol (§4), so v0.1 supplies no such attestation and a v0.1 writer
  cannot drop from a synced document on acknowledgement alone; local-only
  documents are unaffected and compact under S7. Dropping still
  additionally requires retention of the S1 and S3–S5 metadata needed to
  prevent resurrection (creation positions, tombstones with their live
  intervals, winner positions, watermarks). The writer MUST NOT truncate
  blindly.
  The condition is on DROPPING rather than on retention because one I5
  class cannot be decided from any bounded metadata: a late batch
  positioned before an already-dropped batch can flip that batch's
  whole-batch I5 verdict, and re-deciding it needs the dropped batch's
  OPERATIONS, which no retained key, position, or live interval replaces.
  The condition AVOIDS that class by restricting when history may be
  dropped; it does not satisfy it by retaining more, and no claim of V6
  completeness rests on the condition alone.
  Observable behaviour when a drop is refused: when a compaction request
  or a retention policy would otherwise drop a batch and this
  precondition is not established, the COMPACTION OPERATION MUST report a
  NON-BLOCKING diagnostic naming (a) that the operation log is being
  retained rather than dropped, (b) which condition is unestablished, and
  (c) the retained range that could not be dropped. The diagnostic is
  part of that operation's observable result, not an implementation log:
  a writer that records the refusal only in debug output does NOT satisfy
  this. Silent retention is forbidden here exactly as silent undo/redo
  breakage after compaction is forbidden under S7 (the R8.4.3
  non-blocking pattern). Threshold values and schedules remain
  implementation policy (S8).
  Cost, accepted by ED-0039 as amended: a synced document retains its
  operation log until the drop PRECONDITION is established — which
  requires the attestation above, not acknowledgement alone — so a peer
  that never returns, or a protocol that cannot attest delivery
  completeness, means the log does not shrink. Real-time and multi-user
  merge design is out of v0.1 scope (§4), and local-only documents
  compact unaffected under S7.
- **S7 Local-only compaction.** For a local-only document, compaction may
  write a fresh complete package and retain operations according to a
  documented undo/recovery policy. Where the retention policy cannot
  preserve what a live session stack needs, affected stacks truncate to
  the retained prefix with a NON-BLOCKING diagnostic (R8.4.3); silent
  undo/redo breakage after compaction is forbidden. A local-only
  compaction that cannot proceed at all — including one whose required
  snapshot metadata (S3) will not fit row L23 — reports a NON-BLOCKING
  diagnostic on the same terms, naming the retained state and the
  blocking reason; silent refusal is forbidden here exactly as silent
  undo/redo breakage is.
- **S8 Guarantee boundary.** Snapshot/framing semantics and the observable
  retention guarantees of this section belong to the format; threshold
  values, schedules, and writer policies do not (see also L31, Deferred —
  issues 0005/0029).
## 6. Assets and package profiles

See [ASSET-STRATEGY.md](ASSET-STRATEGY.md) §10 (normative v0.1 asset,
package, retrieval, and corpus material, partial scope). The format must
support:

- a thin package referencing content-addressed blobs in a cache/store;
- a portable package embedding every referenced blob — every member of the
  package's referenced-blob universe `U(P)` — digest-verified, or failing
  with a complete omission report;
- digest identity over logical blob bytes (`sha256:<lowercase hex>`),
  descriptor schema, and logical blob-path mapping per ASSET-STRATEGY
  §10.1–10.3;
- streaming verification, deduplication, and dangling-reference diagnostics;
- safe behavior when optional assets are missing, including deterministic
  handling of well-formed but unresolved references;
- a stable package-level asset-root table enumerating `U(P)` so readers
  preserve blobs referenced by unknown future data (invariants I1–I4);
  wire shape fixed in §7.4;
- cache pin/lease garbage collection that is fail-closed under metadata
  anomalies and can never remove blobs referenced by retained snapshots or
  operations;
- policy-gated remote retrieval with consent, scheme allowlist, bounded
  redirects, private-destination rejection, fail-closed resolver-to-dial
  binding, TLS/authority origin binding, and digest-before-decode/admission
  (values ratified by issue 0023; see §9.3).

UUIDs and content digests serve different purposes and must not be
conflated.

## 7. Physical container

Status: **Normative for v0.1 within this section's scope (issue 0005): the
selected physical container, entry-name serialization, manifest and
asset-root wire shapes, operation-batch naming/order/framing, snapshot
placement, compression-method allowlist, physical entry ordering, and the
writer-policy separation.** Logical semantics remain owned by §§2–5 and
observable save/recovery behavior by BEHAVIOR-SPEC §9; security limits and
rejection classes of §9 are consumed here, never weakened. Numeric limit
values always live in §9.3; this section fixes only which quantity binds
which container structure.

**Selected container: a ZIP-family package under the strict Domowik profile
of §7.2** — a ZIP64-capable ZIP archive whose conforming subset, forbidden
features, logical entry names, and integrity rules are fixed below. The §7.1
requirements therefore bind as selected-container requirements and are
discharged clause-by-clause in §§7.2–7.9.

The container provides:

- one unambiguous manifest with deterministic logical entry names (§7.4);
- atomic save exclusively by writing a complete temporary package and
  replacing the old file through the §9.7 save state machine — no in-place
  append exists in the profile (§7.8);
- checksums and content digests at two layers (§7.4);
- a normative operation-batch naming/order/framing scheme (§7.6);
- duplicate-entry and duplicate-logical-name rejection (§7.3);
- streaming access for large assets (§7.2, §7.7);
- failure recovery through §9.7 and BEHAVIOR-SPEC §9 unchanged.

### 7.1 Binding requirements on the container decision (issue 0023)

Status: **Normative requirements stated container-neutrally.** This
subsection records capability requirements C1–C8 and a conditional schedule
without selecting a technology; technology selection is issue 0005's own
decision, exercised in the §7 introduction. The physical container,
entry-name serialization, operation-batch framing, and compression settings
are owned by issue 0005; because that selection chose a ZIP-family
container, the conditional schedule below binds and is discharged by
§§7.2–7.9 item by item, without weakening any C1–C8 requirement.

Capability requirements (container-neutral):

- **C1 Bounded sequential access:** any single blob can be extracted and
  digest-verified as a stream without loading the whole package (required by
  §6 lookup order and BEHAVIOR-SPEC §9.6 degraded inspection on large
  portable packages).
- **C2 Digest-as-stream:** the hash domain of ASSET-STRATEGY §10.1 (logical
  blob bytes `B`) is verifiable incrementally without double buffering;
  container-level compression stays outside the hash domain.
- **C3 Declared-length validation:** declared sizes are validated against
  observed growth; no allocation may be proportional to an unverified
  declaration (see §9.3 rows L7, L3–L5).
- **C4 No whole-archive allocation:** manifests/tables are located and
  processed under the memory bound of §9.3 row L7, with an explicit strategy
  that does not trust an unbounded directory/table region.
- **C5 Large-entry support:** individual blobs may exceed any legacy 32-bit
  size/offset field of the underlying technology; the selected container must
  carry a defined large-record mechanism.
- **C6 Streaming respects limits and cancellation safety:** streaming never
  bypasses §9.3 rows L1–L5, L15, L16; cancelling a mid-stream load leaves no
  partial cache admission (admission is atomic with verified digest,
  ASSET-STRATEGY §10.6/§10.7).
- **C7 Random access only where required:** if the format needs random access
  (for example future chunked level-of-detail classes), the requirement is
  stated as a bounded capability, not as a container choice.
- **C8 Threshold separation:** physical bytes, logical (expanded) bytes, and
  decoded bytes have distinct limits and distinct fixture vectors (§9.2).

Conditional ZIP64 schedule — applies **only if** issue 0005 selects a
ZIP-family container, in which case the ratifying specification MUST fix all
of the following, citing APPNOTE.TXT (.ZIP File Format Specification)
version 6.3.10, PKWARE Inc., revised Nov 01 2022 (provenance record P7 in
§9.8):

1. 64-bit uncompressed/compressed sizes via the Zip64 extended information
   extra field (0x0001): the per-entry trigger is APPNOTE §4.5.3 — an entry
   whose size fields are too large for the original 4-byte fields carries the
   extra field, and when present in a local header it MUST contain BOTH
   original and compressed sizes; the related archive-level sentinel behavior
   (fields set to −1/0xFFFFFFFF) is documented separately in APPNOTE
   §4.4.1.4;
2. Zip64 end-of-central-directory record and locator usage with 8-byte entry
   counts and central-directory size/offset (APPNOTE §§4.3.14, 4.3.15);
3. Entry counts beyond the legacy 65,535 ceiling (APPNOTE §§4.4.21–4.4.22):
   Domowik's own limit (§9.3 row L6) makes Zip64 entry counts unreachable in
   v0.1, but a reader must still classify their structural presence as
   hostile input;
4. Malformed, truncated, or contradictory Zip64 locator or Zip64 EOCD records
   are rejected per §9.4 (fail-closed rule T1);
5. A compression-method allowlist (expected candidates: deflate and store;
   final list fixed with the 0005 ratification); every other method is
   rejected as `unsupported-method`;
6. Duplicate names are rejected and encrypted entries are rejected exactly
   per §9.4;
7. Path normalization: names are normalized per ASSET-STRATEGY §10.3 and
   §9.4 before comparison; case-folding and Unicode-normalization collisions
   are rejected;
8. Archive limits: every applicable row of §9.3 restated against ZIP
   quantities (central-directory size folds into row L7; per-entry caps bind
   local-header declared sizes).

If another container is selected instead, it receives an equivalent capability
review against C1–C8 and an equivalent hostile-input schedule mapped onto its
own structures.

### 7.2 Container profile (selected-container requirements)

Every `.domo` package is a single ZIP archive conforming to the following
profile. APPNOTE.TXT version 6.3.10 (provenance P7, §9.8) is the underlying
authority for ZIP structures; RFC 1951 (E2 in the issue 0005 provenance
record) for the deflate stream. Deviations are hostile input under §9.

- **Single-disk, non-spanned archive.** Any nonzero disk-number or
  span-marker field rejects (`malformed-container`).
- **No encryption.** Entries with the general-purpose encryption flag set,
  or carrying any encryption-related extra field, reject exactly per §9.4
  (`encrypted-entry`).
- **No data descriptors.** The general-purpose bit-3 (data-descriptor) flag
  MUST be clear; a set bit rejects (`malformed-container`). Conforming
  writers know every size before writing because packages are written as
  complete candidates (§7.8).
- **UTF-8 names only.** Writers MUST set the language-encoding flag (bit 11)
  on every entry; a clear bit rejects (`invalid-entry-name`).
- **Local/central consistency.** Local file headers and central-directory
  records MUST agree on name, method, CRC-32, and sizes; contradiction
  rejects (`malformed-container`).
- **CRC-32 verified.** A stored-data CRC-32 mismatch rejects
  (`checksum-error`); it detects corruption but confers no authenticity.
- **Extra fields restricted.** Only the Zip64 extended information extra
  field (0x0001) is permitted; any other extra field rejects
  (`unsupported-extension`).
- **No comments.** Entry comments and the archive comment MUST be zero-
  length; otherwise `unsupported-extension`.
- **Version bound.** A version-needed-to-extract value above 45 rejects
  (`unsupported-version`). Other header metadata is either validated or
  ignored by explicit rule: timestamps are never interpreted (B6 discipline
  generalized to the carrier), while file-type attributes are enforced —
  see the next bullet.
- **Regular-file entries only.** The internal-attributes field MUST be
  zero. External attributes MUST NOT designate anything but a regular file:
  the MS-DOS directory (0x0010) and volume-label (0x0008) bits MUST be
  clear, and when the creator-system field (high byte of version-made-by)
  indicates a mode-bearing system such as Unix (3), the high 16 bits must be
  clear or carry the regular-file type; any entry whose attributes designate
  a directory, symlink, hardlink, device, or other special type rejects as
  hostile input (`invalid-entry-name` nearest cause). This realizes §9.4's
  symlink/hardlink/special-file rejection at the carrier level. Conforming
  writers emit regular-file (or zeroed) attributes only.
- **Physical entry ordering (writer obligation).** Conforming writers emit
  entries in ascending UTF-8 byte order of logical entry names. Readers
  NEVER depend on physical order — the central directory is the sole
  authoritative index — so no reader-side rejection class arises from
  physical order (resolving the owner-defined §9.4 row by making the
  contract writer-normative only).
- **Streaming capability (C1/C2/C6).** Each entry's stored bytes are
  extractable as a stream located via the central directory; decompressed
  bytes equal logical blob bytes `B` verbatim for blob entries
  (ASSET-STRATEGY §10.1), keeping container compression outside the hash
  domain. All streaming passes the L1–L5/L15/L16 guards with cancellation
  safety per C6; admission stays atomic with digest verification.

### 7.3 Logical entry names and namespaces

Logical names are `/`-separated relative paths of ASCII or UTF-8 components,
validated before any use under rows L8/L9 and the §9.4 name rules (no
absolute paths, drive forms, dot segments, backslashes, NUL/control
characters, invalid UTF-8, case-folding or Unicode-normalization collisions;
symlink/special designations cannot arise in this profile and any designation
attempt rejects). Names are compared after normalization exactly per §9.4;
duplicate logical names always reject (`duplicate-logical-name`). L29's
tolerance for "repeated physical occurrences of identical blobs" is
unreachable under v0.1: because the blob mapping is injective (§10.3),
identical blobs have identical logical paths, so `duplicate-logical-name`
rejection covers all duplicate-blob cases at name validation (outermost per
§9.5) before L29's enumeration stage.

The namespace set is CLOSED within one accepted schema version:

| Prefix / name | Contents |
|---|---|
| `domowik.json` | The manifest — exactly one per package (§7.4) |
| `snapshot/state.json` | The snapshot section — exactly one per package (§7.5) |
| `assets/descriptors.json` | Asset descriptor documents keyed by digest (§7.4) |
| `ops/<counterBase>-<actorId>.json` | Operation-batch documents (§7.6) |
| `blob/sha256/<2-hex>/<62-hex>` | Content-addressed asset blobs, mapping per ASSET-STRATEGY §10.3 |
| `preview/data/<2-hex>/<62-hex>`, `preview/index.json` | Derived preview cache namespace, entirely optional (§7.5) |

Any other entry name, or a name malformed for its namespace, rejects
(`invalid-entry-name`). New namespaces can arrive only with a new schema
identifier under §8's version gate; they are never smuggled in under an old
version label.

### 7.4 Manifest and integrity

The manifest `domowik.json` is a JSON object. Exactly one manifest exists
per the §7.3 closed namespace; zero manifests reject `missing-manifest`.
The `ambiguous-manifest` token (§9.4) is unreachable under v0.1: multiple
entries named `domowik.json` reject as `duplicate-logical-name` at name
validation (§7.3, outermost per §9.5), and multiple manifests with
different names cannot exist under the closed namespace. Fields:

| Field | Req | Meaning |
|---|---|---|
| `formatVersion` | yes | Schema identifier string per §8 (v0.1 emits `"0.1"`) |
| `profile` | yes | `"thin"` \| `"portable"` (§7.9) |
| `assetRoot` | yes | Ascending-by-digest array enumerating U(P) (invariants I1–I4 below) |
| `entries` | yes | Object mapping EVERY other logical entry name to `{sha256, byteLength}` |

The `entries` inventory is the authoritative integrity layer: each listed
value carries the SHA-256 of the entry's logical bytes and its byte length.
A mismatching digest maps to the §9.4 classes — blob entries follow
digest-mismatch ≡ missing-blob handling; structural entries (snapshot,
descriptors, batches, preview index) reject `checksum-error`. A byte-length
contradiction between inventory and observed growth rejects
`malformed-length` under C3: declared sizes are validated against observed
growth and never drive allocation. The manifest itself is protected by the
container CRC-32, the structural checks of §7.2, and the fail-closed
trailing-data rule T1; a forged manifest surfaces through the digests it
mislists. Readers locate and process the manifest and central directory
under the row-L7 computed-reserve strategy (C4): reservation derives from
entry counts times minimum record overhead, never from declared lengths.

Example (normative shape; values illustrative, whitespace insignificant):

```json
{
  "formatVersion": "0.1",
  "profile": "thin",
  "assetRoot": [
    {"digest": "sha256:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90"}
  ],
  "entries": {
    "assets/descriptors.json": {"sha256": "sha256:c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1", "byteLength": 512},
    "blob/sha256/a1/b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90": {"sha256": "sha256:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90", "byteLength": 2048},
    "snapshot/state.json": {"sha256": "sha256:b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90a1", "byteLength": 256}
  }
}
```

`assets/descriptors.json` holds the descriptor documents of ASSET-STRATEGY
§10.2, serialized as ONE object keyed by digest (key order insignificant);
descriptor field syntax follows §10.2 and rows L19/L32. Example:

```json
{
  "sha256:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90": {
    "digest": "sha256:a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c6d7e8f90",
    "byteLength": 2048,
    "mediaType": "image/png",
    "purpose": "background-image",
    "encoding": "identity",
    "requirement": "required",
    "provenance": {
      "displayName": "Wood floor tile",
      "sourceDescription": "user import",
      "author": "",
      "acquiredAt": "2026-08-24T00:00:00Z"
    }
  }
}
```

The asset-root table discharges ASSET-STRATEGY §10.5 invariants I1–I4:
(I1) `assetRoot` is enumerable without understanding any entity schema;
(I2) a reader understanding only the table preserves every listed blob on
write-back or refuses write — silent stripping forbidden; (I3) membership
implies nothing about schema comprehension; (I4) full-knowledge writers
regenerate the table completely, partial-knowledge writers never rewrite
selectively. Because the §10.3 mapping is injective, a root entry needs only
the digest; the physical path is derived, never stored.

`assetRoot` is REQUIRED and its required JSON type is array. A JSON `null`
is not an array: it is a wrong-typed required manifest field and rejects
`invalid-field-syntax` (§9.4 manifest and schema classes; §8.4 U1) before
the table is used for anything. `null` is not a spelling of an absent or
empty table — an empty table is the empty array value `[]`. The distinction
is load-bearing rather than stylistic: `[]` ASSERTS that U(P) is empty,
which licenses a write-back preserving no blobs, whereas `null` asserts
nothing, so reading it as `[]` would license exactly the silent stripping
§7.4 I2 above and §8.4 U4 forbid. A parser's tolerance cannot make a
package conforming.

### 7.5 Snapshot placement

Every package contains exactly one snapshot section at
`snapshot/state.json`, whose content is the §5.1 snapshot (S1 projected
authoritative state, S2 frontier, S3 winner keys, S4 tombstones, S5
batch/dedup metadata). **The snapshot frontier of record is S2 inside
`snapshot/state.json`: it is simultaneously the coverage record and the
per-actor duplicate watermark (I2), and no other location may carry or
shadow it.** A brand-new document serializes the empty projection (empty
entities, empty frontier) rather than omitting the section. The member
shapes of S1–S5 are owned by §5.1; this section fixes only the placement.
Row L23's snapshot-metadata ceiling binds this section; the descriptor and
batch entries are ordinary entries bounded by rows L3/L4 and their own
envelopes. Preview payloads and index live in the optional `preview/`
namespace as class-4 derived caches (ASSET-STRATEGY §10.5); absent previews
never fail anything.

### 7.6 Operation-batch framing

Each batch serializes as ONE JSON document — the §4 batch shape (B1–B6,
§4.9 example) — at a deterministic logical name:

```text
ops/<counterBase>-<actorId>.json
```

where `<counterBase>` is the decimal counter base zero-padded to 19 digits
(signed-64-bit ceiling L13 makes 19 digits total-order-preserving under byte
comparison) and `<actorId>` is the canonical 36-character UUID form (E1).

Normative framing rules:

- **Name/content binding.** The entry name carries exactly two identity
  components, `<counterBase>` and `<actorId>`. A name malformed for the
  `ops/` namespace — a counter component that is not the 19-digit
  zero-padded decimal, or an actor component that is not the canonical
  36-character UUID (E1) — is already rejected as `invalid-entry-name` under
  §7.3 at name validation, before this check is reached; such a name is not
  a name/content-binding case. Over a name that passed §7.3, a batch whose
  embedded `counterBase` or `actorId` differs from the corresponding parsed
  name component rejects `malformed-framing` before replay begins (§4.3
  row 4). The name carries NO `batchId`, so no name component binds it, and
  `batchId`'s obligations elsewhere — B2's UUID syntax under row L19, the
  duplicate-batch-id check below, and §5.1 S5's dedup metadata — are
  unchanged by this clause. A reader states as checked only what it can
  compare.
- **Deterministic enumeration order.** Physical enumeration sorts by entry
  name ascending; because of the fixed-width padding this equals ascending
  `(counterBase, actorId)`. Enumeration is possible from names alone,
  before parsing contents.
- **Structural log checks precede replay.** After enumeration and parse,
  and BEFORE any replay effect: duplicate batch ids, duplicate reserved
  ranges, overlapping or gapped ranges within one actor's stream (violating
  B3), torn or partial batch documents, count envelopes beyond row L23, and
  inconsistent frontiers all reject `malformed-framing` atomically — the
  §4.5 rule that structural anomalies are pre-replay framing rejects, now
  physically grounded.
- **Domain of those checks.** They are evaluated over EVERY `ops/` entry the
  package retains, taken as one set, after enumeration and parse and BEFORE
  any replay effect. No entry is removed from this domain and no entry in it
  is exempt from any check in the list. §4.5 I2 classification runs only
  after these checks have passed and suppresses replay EFFECTS; it never
  selects what was checked. This answers the referral §4.5 I2 makes here.
- **Falsifiability obligation.** An implementation satisfies the domain rule
  only if an entry lying WHOLLY at or below a coverage boundary can still
  FAIL these checks. An implementation that filters covered entries before
  evaluating them does not satisfy it, however its diagnostics read.
  Conformance evidence MUST include at least one input whose only defect
  lies wholly within covered counters, and MUST show that input rejecting.
- **Parity, scoped to these checks.** Because the domain and the
  classification below are defined without reference to replay, a replaying
  reader and a parse-only inspection seam MUST reach the same outcome with
  the same §9.4 token FOR THE CHECKS IN THIS SUBSECTION. Nothing here
  constrains later stages: a seam is expected to accept structurally framed
  packages that a replaying reader rejects during operation construction or
  replay, and that difference is intentional.
- **Per-actor range classification.** Let `C(a)` be actor `a`'s
  covered-prefix bound: the counter recorded for `a` in the snapshot
  frontier of record (§7.5, S2). Let `r1 < … < rn` be `a`'s retained
  reserved ranges in ascending order. Where the snapshot frontier NAMES `a`:
  1. **Disjointness.** The ranges are pairwise disjoint. Any overlap rejects
     `malformed-framing` (B3), WHETHER OR NOT the overlapping counters lie
     at or below `C(a)`.
  2. **No straddle.** `ri.base ≤ C(a) < ri.end` rejects `malformed-framing`
     as a torn batch: B5 advances a frontier only to a batch END, so no
     conforming coverage boundary falls strictly inside a reserved range.
  3. **Compacted prefix.** Counters at or below `C(a)` appearing in no
     retained range were reserved by B3 and dropped by retention (§5.2).
     They are not a gap and never reject. Retention drops batches
     individually (S6/S7), so the entries retained at or below `C(a)` need
     NOT form one contiguous block, and a hole among them is not a defect.
  4. **Subsequent log.** Every counter above `C(a)` and at or below the
     greatest retained end for `a` MUST appear in a retained range: the
     ranges above `C(a)` are the subsequent log of vector V6 — the first
     begins at exactly `C(a) + 1`, each later one at its predecessor's end
     plus one. A missing counter above `C(a)` is a genuine gap and rejects
     `malformed-framing`, because nothing covers it and nothing retains it.
- **Actor ABSENT from the snapshot frontier: the PREFIX is UNRULED in this
  revision, and deliberately so.** Where S2 does not name `a`, `C(a)` has no
  determined value, and exactly one thing is left unruled: whether the
  counters BELOW `a`'s earliest retained base are a compacted prefix or a
  gap. **Unruled is not unchecked.** Every other structural check in this
  subsection binds for `a` exactly as it does for a named actor — in
  particular the ranges remain pairwise DISJOINT, and every counter between
  `a`'s earliest retained base and its greatest retained end MUST appear in
  a retained range, so a hole BETWEEN two retained ranges rejects
  `malformed-framing` here as anywhere else. A reader may not read this
  paragraph as suspending the checks it does not mention; the unruled
  boundary is one boundary, not a region in which nothing is verified.
  Two readings of that boundary are available and neither is adopted here.
  Reading one takes an absent entry as zero coverage (F1: nothing observed),
  so the first retained range must begin at counter 1. Reading two takes the
  compacted prefix as implied by the earliest retained base, which is
  satisfied by construction and therefore checks nothing. The first
  invalidates committed packages that carry a retained entry for an actor
  absent from the snapshot frontier; the second admits a clause that cannot
  fail, which §4.7's evidence discipline rejects. The underlying question —
  whether this format accepts a PARTIAL-HISTORY package at all, one
  asserting operations that are neither covered by its snapshot nor present
  in its log — is not settled here, and its eventual answer may move a
  `malformed-framing` boundary. What this revision settles is only that it
  does not answer it. The limit is recorded deliberately, on the same terms
  as F4's unprovable-closure limit: a reader is told exactly where the rule
  stops rather than given a guarantee this section cannot support.
- **Physical order is never semantic.** Replay order remains the canonical
  order of F3/LWW1 over the delivered set; the deterministic enumeration
  exists solely so diagnostics and framing checks are reproducible.
  Arrival, delivery, and storage order cannot influence projection (V2/V7).
- **Envelope.** Batches per package, operations per batch, and total
  retained operations bind through row L23; each batch document is an entry
  bounded by rows L3/L4.

### 7.7 Compression

- **Allowlist.** Per-entry compression methods are exactly: `0` (store) and
  `8` (deflate, the raw DEFLATE stream of RFC 1951 as referenced by APPNOTE
  method 8, E1/E2). Every other method rejects (`unsupported-method`),
  discharging §7.1 item 5.
- **Writer settings are not observable format rules.** Deflate level,
  strategy, block heuristics, and per-entry-class choices are writer policy
  (issues 0027/0029 measurement-driven); readers accept either allowed
  method on any entry and infer nothing normative from the choice. No
  compression setting is validated beyond membership in the allowlist —
  except where required to decode (the method ID itself).
- **Limit mapping (§7.1 item 8, C8).** Declared sizes bind local-header
  fields against central-directory records and observed growth; per-entry
  expanded/physical caps are rows L3/L4; expansion ratios L15/L16 apply per
  entry and in aggregate; the aggregate manifest/table reserve is row L7;
  entry-count ceiling L6 applies at manifest enumeration. Central-directory
  size folds into L7 exactly as §7.1 item 8 states.

### 7.8 Large files and Zip64

§7.1 items 1–4 bind verbatim as selected-container requirements: Zip64
extended information extra field (0x0001) usage per APPNOTE §§4.4.x/4.5.3
with BOTH sizes present when carried in a local header; Zip64 EOCD record
and locator per §§4.3.14/4.3.15; entry counts beyond the legacy 65,535
ceiling classified hostile input (unreachable for conforming v0.1 writers
because of row L6, still rejected structurally); and malformed, truncated,
or contradictory Zip64 locator/EOCD records rejected per §9.4 under fail-
closed rule T1. Individual entries may exceed all legacy 32-bit fields (C5)
within rows L1–L5.

### 7.9 Thin and portable profiles

Thin and portable packages serialize THE SAME logical plan under the same
manifest schema; they differ only in the `profile` value and in which blobs
are embedded (ASSET-STRATEGY §10.4):

- **Thin:** embeds only preview-class blobs in the `preview/` namespace;
  every blob of U(P) resolves through the lookup order of §10.6 (embedded →
  cache → policy-gated remote → missing).
- **Portable:** embeds every blob of U(P) at its §10.3-derived path and
  digest-verifies each; any unverifiable reference aborts packing completely
  with the omission report of BEHAVIOR-SPEC §9.6. A portable package whose
  required embedded blob fails verification has its portability claim
  treated as broken, never presented as holding.
- **An EMPTY `U(P)` is PORTABLE (issue 0064).** A plan referencing no assets
  satisfies the portable rule vacuously — there is no blob that could fail to
  embed, and none that could fail to verify — so such a package is
  self-contained in the only sense the word has here, and no move or copy can
  break it. This is stated rather than left to a writer's guard, because the
  profile is a FACT ABOUT THE PACKAGE rather than a preference, and a writer
  that answered `thin` would be adding a condition this section does not state.
  The observable consequence it removes: `pack --portable` on an asset-free
  plan previously wrote a manifest saying `thin`, so a user's stated intention
  and the recorded fact disagreed with nothing wrong in between them.
  The alternative — "thin by convention, because a package with no assets is
  arguably neither" — was considered and rejected. It is defensible, but it
  requires a rule the text does not contain, and it makes the profile of an
  empty package a matter of convention while every other package's profile is
  derived. Nothing about the NON-EMPTY derivation changes.
- **Conversion** thin↔portable is explicit, all-or-nothing, reports
  embedded-vs-external totals before packing, and copies `B` verbatim —
  embedding is a byte copy, transcoding creates a new asset with a new
  digest (§10.1). Conversion rewrites the complete package through §7.8;
  nothing mutates in place.

### 7.10 Atomic replacement and writer-policy separation

**Atomic complete-package replacement.** Saving replaces the whole package
exclusively through the §9.7 save state machine (complete temporary sibling,
full reader-equivalent validation, content sync, atomic same-filesystem
replace, directory sync where supported). The profile has no append, no
partial overwrite, and no in-place mutation semantics; trailing, prepended,
or concatenated material rejects under T1. Failure recovery observability is
§9.7/BEHAVIOR-SPEC §9 unchanged.

**Writer thresholds versus format-observable retention (issue 0005
criterion).** Two concerns stay separated:

1. Format-OBSERVABLE retention rules: the guarantees of §5 (S6 synced-
   compaction gating, S7 local-compaction stack truncation diagnostics, S8
   guarantee boundary), tombstone/winner-key presence per S3/S4, and the
   unknown-data preservation duty of §8/L32. These bind every conforming
   writer and reader.
2. Writer THRESHOLD policy: compaction trigger values, retained-operation
   targets, schedules, autosave intervals, and size budgets. These are
   implementation policy owned by issues 0027/0029 (measurements via
   0035→0018); they are never serialized as normative constraints, never
   validated by readers, and no number is ratified here. Row L31 remains
   Deferred to those owners untouched.

## 8. Versioning and unknown data

Status: **Normative for v0.1 within this section's scope (issue 0005): the
schema identifier, reader/writer version rules, pre-1.0 migration policy,
post-v1 compatibility pointer, and the unknown-data rules.** Logical schema
content remains owned by §§2–5; security ceilings of §9 apply unchanged.

### 8.1 Schema identifier

- **V1 Single statement of emission.** The manifest field `formatVersion`
  (§7.4) is the sole schema identifier. Its value is a decimal
  `MAJOR.MINOR` string, no leading zeros; v0.1 emits `"0.1"`. A writer
  states exactly which schema it emits; readers never infer a version from
  content heuristics.
- **V2 Honest terminology.** The pre-1.0 series is called the **experimental
  `0.N` schema series**, defined by THIS section — not by SemVer. Semantic
  Versioning 2.0.0 rule 4 says major-version-zero anything MAY change with
  an unstable API (provenance E4 in the issue 0005 evidence record); Domowik
  instead ADDS explicit migration guarantees on top of an experimental
  series, so partially borrowing SemVer vocabulary without its semantics is
  prohibited. The term "SemVer" may appear only where SemVer rules are
  actually adopted (§8.3).

### 8.2 Pre-1.0 policy: released schemas and migration

- **V3 Reader acceptance gate.** A reader accepts exactly the experimental
  minors it claims to support. A package whose minor exceeds the reader's
  supported minor rejects (`unsupported-version`) with an actionable
  diagnostic naming both versions; guessing, best-effort parsing, or silent
  degradation across minors is forbidden. This is the safe refusal path for
  unknown-required constructs arriving under a newer label.
- **V4 Migration duty.** Every publicly released `0.N` schema MUST have a
  tested migration path available in any later release that claims to open
  it: migration produces a COMPLETE new package via the §9.7 save state
  machine (never an in-place edit), preserves unknown data per U2 below,
  and runs only on explicit user save/export — opening stays read-only.
  Implementation ownership: issue 0031 (migrations); conformance fixtures:
  issue 0028.
- **V5 Breaking-change discipline.** Any change that a previously released
  reader could not fully understand — including any new entry namespace
  (§7.3), required field, entity kind, or operation kind — requires a new
  incompatible schema identifier: the MINOR increments (`0.N` → `0.N+1`)
  together with the V4 migration. Additive changes may ship within an
  existing minor ONLY when older readers demonstrably handle them through
  the U2 unknown-optional rules; otherwise they bump the minor. No patch-
  level compatibility claims exist pre-1.0; a PATCH component is unused.
- **V6 Release state of the experimental series (recorded fact, dated).** As
  of 2026-08-29 no `0.N` schema has been publicly released and the
  predecessor set is EMPTY: issue 0018's release-candidate audit found no
  independently evidenced predecessor release. v0.1 is UNRELEASED. Three
  consequences follow, and none weakens a rule above:
  1. **V4's migration duty runs FROM a released `0.N` schema TO a later
     release claiming to open it.** With an empty predecessor set it
     presently binds nothing, and no migration path may be claimed,
     fabricated, or tested against a predecessor that does not exist. Once
     v0.1 is released it becomes such a predecessor and V4 binds any later
     release claiming to open a v0.1 package; V4 never requires a migration
     of v0.1 to itself.
  2. **V5 is scoped to changes a PREVIOUSLY RELEASED reader could not
     understand.** While v0.1 is unreleased no such reader exists, so
     no released-schema migration duty is created by a development revision.
     Under the owner's 2026-10-08 direction (ADR 0011), incompatible development
     schemas nevertheless receive distinct `formatVersion` labels when
     introduced; readers accept only labels they implement. Compatible additions
     may retain a label under V5's safe optional-data rule. No support/migration
     promise covers every unpublished revision. After release, V5 binds without
     exception.
  3. **The §5.1 legacy snapshot members are an unreleased development
     shape, not a released schema.** Reading them under §5.1's documented
     convention is an accommodation for this project's own artifacts; it
     confers no released-schema status, no V4 migration duty, and no
     conformance claim.
  This bullet is a dated fact about the world, not a promise about the
  format, and it stops being true at the moment of a release. Whoever
  releases a `0.N` schema updates this bullet in the same change.

### 8.3 Post-v1 compatibility promise (ADR-backed)

The compatibility contract for v1.0 and beyond is recorded in
[ADR 0007](decisions/0007-format-compatibility-promise.md) (**Proposed**;
ratification is a v1.0 release gate). In outline: post-v1 identifiers adopt
MAJOR.MINOR.PATCH with actual SemVer semantics — breaking changes require a
MAJOR bump plus a migration; backward-compatible additions are MINOR bumps
readable by earlier v1.x readers precisely because additions ride the
unknown-optional rules; corrections are PATCH bumps. The compatibility
promise lives in that ADR as an explicit contract, never implied by version
labels alone.

### 8.4 Unknown data

- **U1 Unknown required refuses safely.** Unrecognized entity kinds,
  operation kinds (whole-batch reject per O2), required manifest/snapshot/
  descriptor fields, unrecognized framing structures, and packages under
  unsupported schema labels all reject BEFORE any effect, using the nearest
  §9.4 class (`unsupported-version`, `invalid-field-syntax`,
  `malformed-framing`, schema classes).
- **U2 Unknown optional is preserved or write-back is refused.** A writer
  unable to interpret optional-but-well-formed data — unknown descriptor
  fields, asset-root members, Tier-B semantics, or any optional construct
  this schema tolerates on read — MUST either carry it into the successor
  package with identical meaning (package-level entries byte-identical;
  JSON values value-preserving round-trips; byte identity of serialization
  still not required per §3.3) or refuse write-back entirely with a safe-
  write-refusal diagnostic naming what cannot be preserved. Silent stripping
  is forbidden everywhere; cumulative preservation volume is bounded by row
  L32 within L1.
- **U3 Closed namespaces.** Within one accepted schema version, entry
  namespaces are closed (§7.3): foreign entries are hostile input, not
  preservable unknowns. Future namespaces arrive only through V5's new
  schema identifier and meet the V3 gate.
- **U4 Asset-root preservation.** The I2 invariant (§7.4) binds every
  reader regardless of schema comprehension: listed blobs are preserved on
  write-back or write-back is refused.

### 8.5 Extensible format direction (ADR 0011)

**Accepted architecture and development policy; extension wire specification
pending.** [ADR 0011](decisions/0011-extensible-versioned-document-format.md) adopts a defined
core with namespaced, versioned feature extensions. The existing manifest
`formatVersion` remains the sole overall schema identifier. The same `.domo`
file type and ZIP-based container are the baseline across schema revisions;
core structure and semantics can evolve under a different supported label.
Changing a label does not convert incompatible contents: a reader uses the
rules for that label, performs a defined explicit conversion, or refuses.

Issue 0004 owns the extension logical contract; issue 0005 owns package and
capability/version gates. Exact extension field names, payloads, namespaces
and operation behavior are pending reviewed wire design and issue 0028 tests.
Until that design is normative and implemented, U1–U4 and §7.3's closed
namespace rules remain unchanged. Accepting the direction does not make
unknown entity/operation kinds or arbitrary archive paths permissible.

The design must support safe preservation of unknown optional extensions and
explicit requirements for correct viewing or editing. Byte/value preservation
alone does not prove that editing a referenced core entity preserves unknown
extension meaning. Required capabilities must be recognized before effects;
an older reader that ignores an optional-looking declaration must not be
allowed to edit a document requiring the declared capability. Development
versioning follows V6; released-schema duties follow V4/V5. ADR 0007's post-v1
promise and the proposed snapshot horizon remain separate decisions.

## 9. Security requirements

Status: **Normative for v0.1 within this section's scope (issue 0023;
ratified 2026-08-23).** The document header above still marks the overall
format pre-implementation; this section is the consolidated security and
recovery material designated by ASSET-STRATEGY §10.8. Observable
save/recovery behavior remains owned by BEHAVIOR-SPEC §9 (normative,
issue 0020) and is referenced here, never redefined. Where a rule's final
value belongs to another owning issue, the owner is named inline.

### 9.1 Threat model and trust boundary

Every `.domo` package, cache blob, preview record, recovery artifact, stale
temporary file, and remote retrieval response is **untrusted input**.

Protected resources: CPU, memory, disk space, file descriptors/handles,
decompression work, decoder work, worker count, network time, and cache
metadata. Failure results are safe, deterministic, and actionable — never raw
library errors. Quota exhaustion, permission failures, power loss, process
crash, and hostile or unreliable filesystems are failure environments handled
by §9.7 and BEHAVIOR-SPEC §9; crafted bytes are handled by this section.

Trust boundary: no entry name, declared length, digest, media type,
compression setting, manifest value, preview record, model metadata, or
remote URL is trusted before validation. Digest verification proves integrity
against a fixed expected digest only; it never authenticates origin or
proves authorship (ASSET-STRATEGY §10.1).

Ownership boundary: issue 0023 owns threat-model behavior and resource
ceilings (this section); issue 0005 owns physical syntax; issue 0028 authors
fixtures; issue 0029 enforces the contract; issue 0004 owns logical operation
and batch semantics; issues 0022/0030 own geometry numerics.

Remote asset hints are untrusted network input. Automatic retrieval must be
opt-in or policy-controlled, prohibit ambient credentials, restrict schemes
and redirects, address private/link-local destination risks, enforce time and
byte limits, and verify the content digest before decoding or cache admission.
The normative configuration schema, normalization rules, fail-closed
resolver-to-dial binding, and TLS/authority origin binding live in
[ASSET-STRATEGY.md](ASSET-STRATEGY.md) §10.8; their numeric values are
ratified in §9.3 below (rows L24–L28).

### 9.2 Size classes

Declared sizes, physical (stored/compressed) bytes, expanded/logical bytes,
decoded bytes, and retained package bytes are separate quantities. Every
limit row below names which quantity it binds; fixture vectors must
distinguish them.

Exact limits and hostile fixtures belong in the format conformance corpus
(issue 0028); see §9.6 for the per-row fixture obligations.

### 9.3 Consolidated resource limits

Status values: **Ratified** = final for v0.1 (rows from outcome-contract
blocks FINAL-ON-MERGE plus the countersigned CANDIDATE block L1–L5, L7, L15,
L16, L21, L23–L25, L27–L28 — countersigned 2026-08-23, contract v4);
**Deferred** = no value deliverable by issue 0023; owner named inline with an
interim parser-level guard where applicable. Measurement feedback published
through issue 0035 → issue 0018 feeds revisit triggers only; it never owns a
value. Row **L33** belongs to neither block: it is DERIVED — its value is
arithmetic over rows L18 and L13 below, and no quantity in it is chosen. It
was placed by issue 0005 under ED-0037 comment (g) and ED-0040 queue item 1.
It is Ratified exactly to the extent L13 and L18 are, and it moves only if
they move.

| ID | Limit / quantity | Value | Status | Boundary & unit | Enforcement stage | Error category | Rationale (grounding) | Revisit trigger |
|---|---|---|---|---|---|---|---|---|
| L1 | Total expanded (logical output) bytes, cumulative per open | 2,147,483,648 B (2 GiB) | Ratified | Bytes; reject when counter would exceed N (**> N rejected; == N succeeds**) | Streaming cumulative counter, checked before yielding each chunk to any decoder/extractor | `limit-total-expanded` | Orders of magnitude above largest legitimate scenario (furnished thin target < 1 MiB; portable scales with user assets yet stays far below mobile-disk scale); bounds bomb damage | Post-v0.1 measurement feedback via 0035→0018 |
| L2 | Total physical input bytes per open | 2,147,483,648 B (2 GiB) | Ratified | Bytes; > N rejected | Byte counter on raw input stream, before parsing | `limit-total-physical` | Matches L1; prevents pure-storage exhaustion independent of expansion | Same as L1 |
| L3 | Per-entry expanded bytes | 536,870,912 B (512 MiB) | Ratified | Bytes; > N rejected | Checked against declared-and-observed size before extraction proceeds past cap | `limit-entry-expanded` | Largest dormant-corpus blob is 196,608 B (non-normative parameters, ASSET-STRATEGY §10.9); 2600× headroom admits real textures/models | Same as L1 |
| L4 | Per-entry physical (stored) bytes | 536,870,912 B (512 MiB) | Ratified | Bytes; > N rejected | On raw entry stream | `limit-entry-physical` | ≥ L3 so incompressible/stored entries are never falsely rejected | Same as L1 |
| L5 | Per-blob logical bytes `len(B)` (asset admission, hash domain of ASSET-STRATEGY §10.1) | 536,870,912 B (512 MiB) | Ratified | Bytes; > N rejected | During streaming digest verification, before cache admission | `limit-blob-bytes` | Aligns asset identity layer with L3; one number across cache/embedded/retrieval paths | Same as L1 |
| L6 | Entry/member count per package | 65,535 | Ratified | Count; > N rejected (**== 65,535 succeeds**) | Manifest/index enumeration, before per-entry processing | `limit-entry-count` | Legacy ZIP end-of-central-directory entry field is 16-bit, max 65,535 (APPNOTE 6.3.10 §§4.3.16, 4.4.21); same ceiling guarantees any ZIP-family selection meets it without Zip64 counts; container-neutral statement | If 0005 selects a container with a different native ceiling, the lower applies |
| L7 | Manifest/asset-root table combined allocation cap | 67,108,864 B (64 MiB) | Ratified | Bytes reserved; > N rejected | Enforced against computed minimum structure size **before** any proportional allocation; declared lengths never drive reservation | `limit-manifest-reserve` | Header/table DoS guard; 64 MiB admits ≈9×10⁵ bare 71-character digest identifiers before per-record overhead (order 10⁵–10⁶ entries) | Same as L1 |
| L8 | Logical name length (full path string) | 1,024 UTF-8 bytes | Ratified | Bytes; > N rejected | Name validation before any use | `limit-name-length` | Far above the fixed 77-byte blob-path form of ASSET-STRATEGY §10.3; matches common filesystem maxima so serialization stays lossless | — |
| L9 | Individual logical-name component length | 255 UTF-8 bytes | Ratified | Bytes; > N rejected | With L8 | `limit-name-component` | Common per-component filesystem maximum | — |
| L10 | Container nesting depth | 1 (the package itself; embedded archive-class payloads are opaque assets) | Ratified | Depth; interpreting a nested archive as such ⇒ reject | Structural scan during streaming | `limit-archive-depth` | Nested archives are unbounded recursion surfaces; v0.1 has no nested-package entity | Future encoding profiles may define bounded decode via the promotion gate (ASSET-STRATEGY §10.9) |
| L11 | JSON/object serialization nesting depth. **Counting contract, fixed by ED-0105 and stated in full below the table** | 128 | Ratified | Levels; reject when depth would exceed 128 (**depth == 128 parses; depth 129 rejects**) | Parser depth counter, before the offending container is entered | `limit-nesting-depth` | Precedent: serde_json default recursion limit 128; RFC 8259 §9 expressly permits implementation depth limits (P8c, P8e) | — |
| L12 | Number literal token length | 64 characters | Ratified | Characters; > N rejected | Lexer | `limit-number-token` | Catches exponent/padding abuse; far above legitimate millimetre-coordinate forms | — |
| L13 | Numeric magnitude — parser security ceilings | Non-finite rejected everywhere; float-range overflow rejected (magnitude beyond IEEE-754 binary64 max); integer-typed fields must fit signed 64-bit | Ratified | n/a | Parse/validation, before semantic use | `nonfinite-value`; new: `numeric-overflow` | FORMAT-SPEC §3 requires rejecting non-finite values; these are parser ceilings, not geometry semantics | Geometry bounds are Deferred (L14) |
| L14 | Coordinate/magnitude geometric bounds | `coordBoundMm` = 10⁷ mm; `dimensionCapMm` = 10⁶ mm; `derivedCoordBoundMm` = 1.1×10⁷ mm; topology quantum 2⁻¹⁰ mm (ED-0110; was 10⁻³ mm) | **Ratified — ED-0100 (2026-09-04)** | Bounds, dimension caps and the coincidence/weld quantum are fixed and enforced. The items ED-0100 left open are decided by ED-0103 (NUMERICAL-MODEL §12): stored coordinates are NOT quantized and sub-quantum geometry is topologically absent (N-1); split points persist as the nearest binary64 (N-3); the exact-area and half-even display contract STANDS and the implementation owes conformance to it (N-4). None of these changes a stored representation or a limit value | Parser ceilings of L13 enforce independently | ED-0100 | Compatibility: v0.1 is unreleased, so no released contract breaks. The bound ROSE from the previously enforced 10⁶ mm, which only widens the set of accepted documents | Resolved |
| L15 | Decompression ratio — per entry | 1,000 : 1 | Ratified | Ratio expanded/physical; reject when ratio > 1,000 (**exactly 1,000 succeeds**); inputs whose physical size < 64 B use an absolute expanded bound of 65,536 B instead of the ratio | Streaming, checked incrementally with saturating arithmetic (counter overflow cannot bypass) | `limit-expansion-ratio` | Deliberately just below DEFLATE's theoretical single-stream maximum ≈1032:1 (P8j) — an extreme-but-legal stream may be rejected, accepted trade-off; no mainstream standard-library ZIP reader ships a ratio guard (P8f, P8k), so this is Domowik policy grounded in attack literature with CERT IDS04-J (≈100 MB total cumulative uncompressed output, never-per-entry-reset accumulator, plus 1,024-entry cap) and Info-ZIP overlap detection as precedents (P8h, P8i) | Same as L1 |
| L16 | Decompression ratio — aggregate | Reject when totalExpanded > 100 × totalPhysical **and** totalExpanded > 64 MiB | Ratified | Ratio with absolute floor; both conditions required | Cumulative, saturating arithmetic | `limit-aggregate-expansion` | Floor prevents ratio trips on legitimately highly-compressible small documents (empty/unfurnished corpus scenarios) | Same as L1 |
| L17 | Generic array element count (any serialized array) | 65,536 | Ratified | Elements; > N rejected | Parser, before element allocation | `limit-array-count` | Uniform parser guard; RFC 8259 §9 permits limits (P8c) | Semantic caps on specific arrays (join membership, frontier actors) are Deferred — issue 0004 cross-check |
| L18 | Object member count per serialized object | 65,536 | Ratified | Members; > N rejected | Parser | `limit-member-count` | As L17 | — |
| L19 | String/URI field lengths | mediaType ≤ 127 B; URI/hint URL ≤ 2,048 B; display strings ≤ 1,024 B; digest identifier exactly 71 characters (`sha256:` + 64 lowercase hex, FIPS 180-4); UUID textual form exactly 36 characters (RFC 9562 ABNF); timestamps RFC 3339 UTC fixed form | Ratified | Bytes/characters, exact-or-maximum as listed | Field-level validation before semantic use | `invalid-field-syntax` | Digest and UUID forms are externally fixed standards (P8d); URI bound is a conventional interoperability ceiling | — |
| L20 | Image dimensions (admission, pre-decode) | width ≤ 16,384 px AND height ≤ 16,384 px AND width×height ≤ 268,435,456 px (2²⁸) | Ratified | Pixels; > N rejected on any clause | Declared-header check before decode allocation; decoded-byte bound derives from pixel cap × profile bytes-per-pixel | `limit-image-dimensions` | WebP hard maximum 16,383×16,383 (P8a); D3D12 texture dimension constant 16384 (P8b); JPEG format maximum 65,535² (P8g) — the 16,384 cap admits plausible texture classes while bounding worst-case decode | Encoding promotion gate may add per-profile refinement |
| L21 | Model complexity (admission, codec-independent) | vertices ≤ 10,000,000; primitive instances ≤ 10,000,000; nodes/meshes ≤ 65,536; textures per model ≤ 64; decoded bytes ≤ L3 | Ratified | Counts; > N rejected | Metadata pre-scan before GPU/CPU resource construction | `limit-model-complexity` | glTF 2.0 imposes no format-level complexity limits beyond its 32-bit index encodings, so admission caps must be policy; sized far above plausible furniture models | Encoding benchmark record via promotion gate may refine; owner stays issue 0023 |
| L22 | Chunk counts, generic | 65,536 chunks per blob | Ratified | Count; > N rejected | Streaming enumeration | `limit-chunk-count` | Uniform guard for future chunked classes | Splat-specific numeric limits are **Deferred — future issue, to be filed before any splat work begins**; splats are excluded from v0.1 entirely (ASSET-STRATEGY §10.9 exclusions); no limit implies a splat entity exists |
| L23 | Operation/batch envelope (reader-side resource caps only) | batches per package ≤ 65,535; operations per batch ≤ 65,535; total retained operations per package ≤ 1,048,576 (2²⁰); snapshot metadata ≤ 16,777,216 B (16 MiB) of UNCOMPRESSED SERIALIZED bytes, counted per the L23 metadata accounting below (ED-0105) | Ratified | Counts/bytes; > N rejected | Framing enumeration before replay | `limit-operation-count`, `limit-snapshot-metadata` | Reader envelope distinct from semantics; sized far above unfurnished-1000 corpus scale (ASSET-STRATEGY §10.9 Part A) | Logical batch semantics/identity/counters are **Deferred — issue 0004**; physical framing and batch byte size are **Deferred — issue 0005**, bounded by the applicable per-entry and total caps once framing is fixed |
| L24 | Retrieval transfer byte cap (per retrieval, post-transfer-decoding bytes) | 536,870,912 B (512 MiB) | Ratified | Bytes; > N aborted mid-stream | During receipt, before decode/admission (ASSET-STRATEGY §10.8 ordering unchanged) | `limit-transfer-bytes` ⇒ missing-blob handling | ≥ L5 so retrieval never fails where local admission would succeed; consent-gated feature | Same as L1 |
| L25 | Retrieval time caps | connect ≤ 10 s; stalled transfer (no progress) ≤ 15 s; total per retrieval ≤ 120 s | Ratified | Seconds; exceeding aborts | Connection/transfer timers | `limit-transfer-time` ⇒ missing-blob handling | Bounds network-time resource; generous relative to broadband transfers of ≤512 MiB | Same as L1 |
| L26 | Redirect hop bound | 4 | Ratified | Hops; the 5th hop is rejected (**exactly 4 hops succeed**) | Per-hop, full revalidation per hop (structure per ASSET-STRATEGY §10.8 unchanged) | `limit-redirect-hops` ⇒ missing-blob handling | Ratifies ASSET-STRATEGY §10.8's proposed "≤4"; distinct from L10/L11, never conflated | — |
| L27 | Hints-per-session retrieval attempts | 64 per document per session | Ratified | Attempts; excess skipped with summary diagnostic | Session scheduler | `limit-hints-session` | Prevents hint-list amplification; offline summary UX unaffected (BEHAVIOR-SPEC §9.6) | Same as L1 |
| L28 | Concurrent retrieval requests | 4 per document | Ratified | Requests; excess queued then capped by L27 | Scheduler | `limit-concurrent-retrieval` | Modest parallelism protecting the descriptor/handle budget | Same as L1 |
| L29 | Duplicate physical occurrence tolerance | Readers tolerate repeated physical occurrences of identical blobs up to L6 | Ratified | Count bounded by L6 | Enumeration | n/a (tolerance rule) | Implements "readers tolerate duplicate physical occurrences defensively" (ASSET-STRATEGY §10.6) without weakening duplicate logical-name rejection (§9.4) | — |
| L30 | Temporary/recovery-candidate file count and size | — | **Deferred — issue 0029** | Persistence-policy quantities; artifact naming/mechanism intentionally unspecified by BEHAVIOR-SPEC §9.3 | Writer/persistence layer | owner-defined | Belongs to the persistence contract issue 0029 implements; §9.7/F-matrix require leftovers be ignorable and cleanable | Resolves in 0029's contract/tests |
| L31 | Retained package size / writer thresholds | — | **Deferred — issues 0005/0029** | Writer policy, separated from format-observable retention rules (FORMAT-SPEC §5; issue 0005 acceptance criteria) | Writer compaction policy | n/a | Writer thresholds vs observable retention split is 0005's stated criterion | 0005 decision |
| L32 | Unknown optional data retention size | Bounded by L1 cumulatively; silent stripping forbidden (§8) | Ratified | Bytes within L1 | Write-back path | `unknown-data-preservation` (refusal) | Preservation obligation with a resource ceiling | — |
| L33 | `causalPastSize` — the derived leading component of ordering position P (§4.4 F3, §4.6 LWW1). It is the sum of the counters of ONE frontier, evaluated separately for each: a retained batch's own `frontier` member (§4.9), and each single frontier stored in the snapshot's `batchPositions` table for a referenced position (§5.1 S3). It is never a sum across several frontiers, and the bound below is a per-frontier bound | Maximum 604,462,909,807,314,587,287,552 = 65,536 × (2⁶³ − 1) = 2⁷⁹ − 2¹⁶ (79 bits; 2⁷⁹ itself is not attainable). **DERIVED, not chosen:** arithmetic over rows L18 and L13, both Ratified above | Ratified | Non-negative integer. **Reader representation obligation, not an input threshold:** a conforming reader represents and COMPARES every value in [0, 2⁷⁹ − 2¹⁶] exactly. Wrapping and saturating arithmetic are FORBIDDEN (§4.4 F3) because either changes canonical order. Exactness over that interval is achievable in fixed width; a 64-bit accumulator cannot provide it | Canonical-order construction, and any comparison of two write positions (§4.6 LWW1) | n/a (reader representation obligation) | Follows from two rows already in force: at most L18 frontier members, each an L13 non-negative signed-64-bit counter, and F1/B3 make frontier counters non-negative. It is not a new ceiling on input and no reject clause attaches: input exceeding it has already been rejected by L18 or L13 at parse under §9.5 precedence, so such a clause could never fire. The violable obligation is the reader's — a 64-bit accumulator mis-orders concurrent batches whose past sizes cross a 64-bit boundary | Narrows if L13 or L18 changes, or if issue 0004 fixes the frontier actor-count semantic cap the L17 note defers to it, in which case the maximum is re-derived rather than left stale |

Boundary probes owed to issue 0028 (machine-readable): for every row above,
fixtures cover zero, minimum, exactly-at-limit (success/failure exactly as
specified), one-over-limit (rejected **before** excessive allocation),
maximum representable, arithmetic-overflow attempt, absent field, negative
where prohibited, malformed encodings, and ratio/nested-limit interactions
(many entries each just under L15). Expected results are machine-readable and
distinguish reject, warning, degraded read-only inspection, missing-asset
equivalence, and safe write refusal.

Row L33 is NOT discharged by that one-over-limit pattern: a frontier
exceeding L33 cannot be presented, because L18 or L13 rejects it first, so
the one-over probe would never reach the accumulator. Its probes are instead
frontiers VALID under L13 and L18 whose sums cross the signed and unsigned
64-bit boundaries — comparing sums of 2⁶³ − 1 against 2⁶³, and 2⁶⁴ − 1
against 2⁶⁴, each of which reverses under a 64-bit accumulator while the
exact order does not — and each such check is mutation-proven against a
64-bit accumulator.

**Row L23's snapshot-metadata accounting (ED-0105).** The row gave a number
and a name, and §9.2 requires every row to name the quantity it binds. This one
did not, so no measurement could be compared against it. The number is
unchanged at 16,777,216 B; what follows is the missing domain and accounting.

- **The domain is UNCOMPRESSED SERIALIZED bytes**, measured after
  decompression, never the stored length. A reader-resource cap bounds what a
  reader must hold, and compressed length cannot bound that: metadata is
  repetitive by construction — the same actor UUIDs and batch stems recur in
  every frontier — so it compresses hard, and a cap on stored bytes would admit
  a section that expands far past any bound the reader can honour. Rows L15 and
  L16 bound the RATIO; this bounds the result.
- **What is counted:** the serialized bytes of the snapshot's metadata members
  — S2 `frontier`, S3 `batchPositions` and `winnerPositions`, S1
  `createdPositions`, S4 `tombstones`, and S5's batch/dedup metadata —
  including their keys, their structural punctuation, and the whitespace
  actually present between them. §3.3 does not require canonical JSON, so two
  writers may serialize identical metadata to different byte counts; the cap
  binds the bytes PRESENT rather than a normalized form, and a reader never
  re-serializes in order to measure. A writer near the cap may reduce
  whitespace, and that is a legitimate way to fit.
- **What is NOT counted:** S1's projected authoritative state, the `entities`
  member. It is document content rather than metadata, it is bounded as an
  ordinary entry by rows L3/L4, and folding it in here would silently convert a
  metadata cap into a maximum document size — a product decision this row does
  not make.
- **Unknown members ARE counted.** A member this version does not understand,
  carried under §8 U2's preservation duty, occupies the reader's memory exactly
  as a known one does, and excluding it would leave the only unbounded thing in
  the section being the thing the reader understands least.
- **Boundary:** ≤ 16,777,216 B succeeds; greater rejects, with
  `limit-snapshot-metadata`.
- **Enforcement stage, stated precisely because it differs from the pre-parse
  rows.** Member boundaries are not knowable before parsing, so this cap is
  evaluated during structural traversal of the decompressed entry, before the
  metadata is RETAINED — before position tables are built. Allocation ahead of
  that point is bounded by L3/L4 on the entry and by L15/L16 on expansion, so
  nothing here asks a reader to trust an unbounded input first.
- **Consequence for §7.5, which is inconsistent with this and must be
  amended.** §7.5 says "Row L23's snapshot-metadata ceiling binds this
  section", meaning the whole `snapshot/state.json` entry including S1. That is
  a different quantity from the one defined here and cannot both be right. The
  reading above governs; §7.5's sentence is a drafting error to be repaired
  under issue 0005, and it is named here rather than silently overridden.

**Row L11's counting contract (ED-0105).** A limit stated as a number with no
definition of what it counts is not a contract, and L11 was one: the row gave
128 and a boundary but never said what a "level" is, so two conforming readers
could disagree about the same bytes. What follows is the missing half, and it
is DERIVED from the corpus twins issue 0028 already authored rather than
imposed over them.

- A LEVEL is an enclosing JSON array or object. The outermost array or object
  of a parsed text is at depth 1; each array or object nested directly inside
  a value at depth `d` is at depth `d + 1`.
- Scalars are not levels. A string, number, `true`, `false` or `null` is at
  the depth of the container holding it, and a JSON text that is a bare scalar
  has depth 0. Object KEYS are not levels either; a member's depth is its
  value's.
- A text whose maximum depth is 128 parses. A text containing any value at
  depth 129 rejects with `limit-nesting-depth`, and the rejection happens
  BEFORE that container is entered, so the 129th level is never allocated.
- The bound applies to every JSON text a reader parses from a package or from
  untrusted input, entry by entry. It is not cumulative across entries and
  container nesting is the separate matter of L10.
- **The budget belongs to the ENTRY, not to the document content inside it, and
  a wrapper costs a level (ED-0105, clarified 2026-09-12).** Depth is counted
  from the root of the text being parsed, so content carried as a MEMBER of
  some enclosing structure starts deeper than the same content would alone.
  §5.1 makes S1's projected state a member of the snapshot rather than the
  whole of it, so a document reaching depth `d` on its own reaches `d + 1`
  inside `snapshot/state.json`; an operation payload inside a batch envelope
  sits deeper still. This is a property of every CONFORMING package, not an
  artefact of any particular writer.
  It is stated rather than compensated for. Granting an entry extra levels to
  offset its wrapper would make the bound differ per entry, and a reader would
  then need to know which entry it was parsing before it could know the limit —
  which is the divergent-reader risk this counting contract exists to remove.
  The consequence for authors and for issue 0028's fixtures: the nesting
  available to document CONTENT is 128 MINUS the wrapper depth of the entry
  carrying it, and that subtrahend is a property of the entry's shape. A
  boundary fixture must therefore name the entry it targets; "a document at
  depth 128" is ambiguous until it does.

The golden twin `golden/parser-twin-nesting-depth-at-L11.json` is 128 nested
arrays and MUST parse; `hostile/json/nesting-depth-over-L11.json` is 129 and
MUST reject with that token. They are the executable statement of the three
rules above.

**A general-purpose JSON library's own recursion limit does not discharge
this**, and assuming it does is how the obligation was missed once already.
Two independent reasons, both measured or checkable rather than argued: such
a limit may sit at a different boundary — `serde_json` 1.0.151, the pinned
implementation and the row's own cited precedent, accepts 127 levels and
rejects 128, which is a CONFORMING document refused at exactly the boundary
this row ratifies — and it may be switched off by a build feature enabled
anywhere in a dependency graph, which makes a normative bound depend on
something no reader of this specification can see. A conforming reader
therefore counts depth itself and reports `limit-nesting-depth`; inheriting
a library's behaviour is permitted only where that behaviour is pinned,
tested against the twins above, and produces this token.

### 9.4 Rejection taxonomy

Malformed and ambiguous input classes. Diagnostic precedence follows §9.5.
Each class states primary diagnostic, precedence position, reject-versus-
degraded outcome, and whether a safe rewrite is permitted.

**A third disposition: RESERVED — UNREACHABLE UNDER THE v0.1 PROFILE
(ED-0105).** A class carries this marker when the v0.1 profile makes it
impossible to present: some earlier stage always rejects the input first under
§9.5 precedence. Issue 0005's rulings identified two, and routed the keep-or-
remove question here. **Both tokens are RETAINED**, and the rules for a
reserved-unreachable class are:

- The token stays in this taxonomy and in reader code. "This rule cannot fire"
  and "this rule should not exist" are different claims (ED-0079): the class
  is unreachable because of the profile's closed namespace and injective blob
  mapping, not because the condition is meaningless, and a later profile that
  opens either one makes it reachable again with no taxonomy change needed.
- The class NAMES the diagnostic that fires instead, so the marker is a
  statement about precedence rather than a shrug.
- **Conformance coverage is discharged by a fixture asserting the PRECEDING
  token plus the recorded unreachability rationale** — not by a gap record and
  not by a fixture expecting a token no conforming reader can emit. This is
  what §9.6 requires of issue 0028 for such a class, and it closes
  `gaps/ambiguous-manifest-deterministic-trigger`, which has stood as
  blocked-pending-owner-decision.
- A reader is NOT required to be able to produce the token, and MUST NOT
  reorder its stages to make it producible.

Retention is also the disposition that survives being wrong about the
analysis. If some input nobody has thought of does reach one of these classes,
a retained token simply fires and the marker was merely inaccurate; a removed
one leaves a reader with a real condition and no way to name it. An
unreachability argument is a claim about every possible input, which is the
kind of claim that is cheap to state and expensive to be wrong about, so the
decision is deliberately the one that does not depend on it.

Physical bytes and framing:

| Class | Observable behavior | Error token |
|---|---|---|
| Empty input; too-short or invalid signature; truncated header/payload/index/footer | Reject; no degraded mode (nothing trustworthy to degrade to) | `malformed-container` |
| Malformed length; integer overflow; impossible offset; inconsistent declared-vs-observed lengths | Reject; declared sizes never drive allocation (§9.1); never repaired silently | `malformed-length` |
| Invalid compression method; unsupported extension; unsupported version | Required-entry scope: reject; asset-blob scope maps to BEHAVIOR-SPEC §9.6 undecodable classes where the profile says so | `unsupported-method`, `unsupported-extension` |
| Encrypted entry | Unconditional reject in v0.1 (no supported encryption profile exists) | `encrypted-entry` |
| Malformed checksum | Reject | `checksum-error` |
| Digest mismatch | Treated exactly like a missing blob per BEHAVIOR-SPEC §9.6; a portable package's portability claim is treated as broken (ASSET-STRATEGY §10.4) | missing-blob class |
| Duplicate physical entries with identical bytes | Tolerated defensively up to L6 (row L29); duplicate **logical** names always reject. **RESERVED — UNREACHABLE under the v0.1 profile:** ASSET-STRATEGY §10.3's blob mapping is injective, so identical blobs necessarily carry identical logical names, and `duplicate-logical-name` fires at name validation before enumeration reaches the tolerance. L29 is retained unchanged for the same reason its token is | — (`duplicate-logical-name` fires first) |
| Duplicate logical names; names colliding after normalization; multiple manifests; missing manifest; manifest ambiguity | Reject; writers must reject duplicates at write time (§7). **`ambiguous-manifest` is RESERVED — UNREACHABLE under the v0.1 profile:** §7.3 names exactly one manifest entry, so two manifests means two entries of that one name and `duplicate-logical-name` fires at name validation, which §9.5 places before §7.4 manifest parsing. The token is retained | `duplicate-logical-name`, `ambiguous-manifest` (reserved), `missing-manifest` |
| **Rule T1 (accepted fail-closed trailing/ambiguous-data rule):** trailing bytes after the container terminator; concatenated packages; trailing partial records; second central directory; prepended data; overlapping regions | Reject the entire package. Nothing is ignored silently; there is no ambiguous-data interpretation. Rationale: tolerate-with-report creates divergent-reader risk for zero legitimate-content benefit, since conforming writers produce exact packages (BEHAVIOR-SPEC §9.1 complete-candidate save); fail-closed matches the GC precedent of ASSET-STRATEGY §10.7 | `trailing-data`, `overlapping-regions` |
| Physical ordering violating the selected container contract | Rejected or normalized only if issue 0005 makes that behavior normative; until then unspecified | owner-defined |

Names and paths: absolute paths; drive/UNC forms; `.` and `..` segments;
mixed separators; empty components; NUL/control characters; invalid UTF-8;
overlong names (beyond L8/L9); case-folding collisions; Unicode-
normalization collisions; symlink/hardlink/special-file designations — all
reject (`path-traversal` or `invalid-entry-name`, nearest cause). Logical
names are never interpreted through host filesystem semantics; the mapping of
ASSET-STRATEGY §10.3 (`blob/<algorithm>/<2-hex>/<62-hex>`) is injective and
traversal-free by construction, so any deviation is hostile input. Nothing is
extracted to user-chosen paths in v0.1.

Manifest and schema: invalid encoding; malformed JSON; duplicate object keys;
wrong types; missing required fields; invalid enums; invalid UUID/digest/
URI/timestamp syntax (bounds per L19); unknown required data refusing safely
(§8); unknown optional data preserved opaquely or write-back refused, silent
stripping forbidden (L32). NaN/infinity: reject (`nonfinite-value`).
Negative where prohibited: reject. Negative zero: accepted on input and
canonicalized, per the §3.2 rule that is IN FORCE rather than pending —
validation maps −0 to +0 and conforming writers emit `0`. This sentence
previously deferred that canonical form to "issue 0004's schema … once
ratified" and was stale: §3.2 had already ratified it. Excessive
exponent, decimal precision loss, integer overflow: covered by L12/L13.
Invalid references, dangling references, cycles, duplicate IDs,
self-references where prohibited, invalid asset-root members, descriptor
length inconsistencies: semantic-validation rejects (`invalid-reference`
family). A descriptorless well-formed reference follows the required-missing
treatment of ASSET-STRATEGY §10.6 and BEHAVIOR-SPEC §9.6.

Operations and assets: malformed operation/batch identity, counter/frontier
anomalies, duplicate or reordered operations, partial atomic batches,
tombstone anomalies, missing targets, invalid dependencies — owned by issues
0004/0005; issue 0023 specifies only that framing violations surface as
`malformed-framing` before replay begins. Asset-side classes follow
BEHAVIOR-SPEC §9.6 and ASSET-STRATEGY §10.6 exactly: missing optional;
missing required; digest mismatch ≡ missing; descriptorless well-formed
reference ⇒ read-only degraded inspection with placeholder; undecodable-but-
opaque blob; malformed digest syntax ⇒ hostile-input rejection. Decoder
refusal, complexity-limit exceedance (L20/L21), and digest-before-decode
failures are distinct categories from package framing failures.

### 9.5 Enforcement precedence

Where layers overlap: malformed physical bytes are rejected **before** schema
parsing; schema and limit violations before decoding; digest verification
before decode and cache admission (ASSET-STRATEGY §10.8 ordering preserved);
semantic unknown-optional preservation at write-back. Cross-layer maxima name
which layer rejects first: the outermost failing stage wins and diagnostics
name that stage.

### 9.6 Conformance-fixture obligations (handoff to issue 0028)

Issue 0028 authors every hostile and boundary fixture listed in §9.3 and
§9.4 from this normative text, before persistence implementation; each
fixture carries a machine-readable expected category (reject / warning /
degraded read-only inspection / missing-asset equivalence / safe write
refusal). Prose completion alone confers no conformance claim; conformance
waits for those fixtures and implementation evidence (issues 0028/0029).

### 9.7 Save, replacement, and recovery observability

Observable save/recovery behavior is owned by BEHAVIOR-SPEC §9 (normative,
issue 0020). This subsection codifies format-level mechanics that implement
it; nothing here modifies a BEHAVIOR-SPEC rule.

Three properties are distinguished and never conflated:

1. **Namespace atomicity:** readers see either old or new content, never a
   mixture (rename/ReplaceFile semantics).
2. **Crash durability:** whether a completed replace survives power loss
   (requires content sync plus directory-metadata sync; platform-profiled).
3. **Readability:** the surviving package passes full reader-equivalent
   validation (threat-model pass, not parse-only).

Save state machine (implements BEHAVIOR-SPEC §9.1):

| Step | Action | Establishes | On failure |
|---|---|---|---|
| S0 | Create temporary sibling on the same target filesystem; no in-place append or partial overwrite; collision-safe unique name; regular-file target only — symlink/reparse targets are rejected, not followed | Work area; old target untouched | Abort; target untouched; leftover ignorable/cleanable (matrix F1/F2) |
| S1 | Stream-write the complete candidate under §9.3 limits | Complete byte sequence | Abort; target untouched |
| S2 | Validate candidate with full reader-equivalent checks: manifest consistency, limits, checksums/digests, semantic-projection reopen at documented level | Evidence the new package is readable | Abort `candidate-invalid`; target untouched (F3) |
| S3 | Flush/sync candidate contents (POSIX file `fsync`; Windows `FlushFileBuffers` on a GENERIC_WRITE handle) | Content bytes on stable storage | Abort; target untouched (F4) |
| S4 | Atomic replace within the same filesystem (POSIX `rename`; Windows `ReplaceFile`) | Namespace atomicity: old-or-new for readers | Report error; target is whatever the filesystem left — old or new under the rename assumption (F5). Cross-device EXDEV or equivalent is unreachable by construction because of the S0 same-directory mandate; any copy-fallback path must report the weaker contract and must not claim old-or-new atomicity |
| S5 | Sync parent directory where supported (POSIX directory-descriptor `fsync`; NTFS metadata via available mechanisms) | Directory-entry durability (power-loss class) | Completed replace stands; caveat acknowledged; no rollback (F6). POSIX fsync(2): "Calling fsync() does not necessarily ensure that the entry in the directory containing the file has also reached disk. For that an explicit fsync() on a file descriptor for the directory is also needed" (P2) |

Permissions: replacement preserves the target's existing permissions where
the primitive does so (Windows ReplaceFile preserves attributes of the
replaced file per P3; POSIX rename carries the *new* file's permissions), so
the writer sets intended permissions on the candidate before S4 rather than
relying on inheritance. Concurrent writers unsupported: last completed
replace wins, each outcome individually valid (BEHAVIOR-SPEC §9.1).

Platform/filesystem assumption profiles. Power-loss claims hold **only** for
these profiles and only after their fault-injection suites pass
(BEHAVIOR-SPEC §9.5):

| Platform/filesystem class | Profile | Power-loss claim | Process-crash claim | Basis |
|---|---|---|---|---|
| Linux, ext4, local | atomic same-filesystem rename + file fsync + parent-directory fsync | Yes, after suite passes | Yes | P1/P2/P5/P9 |
| Windows, NTFS, local | `ReplaceFile` + `FlushFileBuffers` | Yes, after suite passes | Yes | P3/P4 |
| macOS (APFS/HFS+), local | none validated | **No — process-crash-only** | Yes, under declared capabilities | P6: Apple documents F_FULLFSYNC as best-effort ("data can still be lost in the case of sudden power loss"); no official Apple document establishes an atomic-replace durability profile; adding one requires a normative BEHAVIOR-SPEC §9.5 amendment with its own provenance review and fault suite |
| Network, FUSE, removable, encrypted, copy-on-write, unusual | explicitly unsupported for the strong contract | No | weaker contract only if separately declared per filesystem class; local evidence is never generalized | — |

Power-loss/kernel-crash assumptions are stated separately from process-crash
and ordinary I/O-error assumptions throughout.

Guarantee table (per injection point × validated profile; OLD = previous
package readable, NEW = new package readable, EITHER = old-or-new readable):

| Injection point | Linux/ext4 profile | Windows/NTFS profile | Crash-only platforms |
|---|---|---|---|
| Before S4 completes (F2, F3, F4: target untouched ⇒ OLD; F1: no prior target exists ⇒ leftover candidate ignorable and cleanable) | OLD (F1 case: no target; leftover cleanable) | OLD (F1 case: no target; leftover cleanable) | OLD (F1 case: no target; leftover cleanable) |
| Interrupted inside replace window (F5) | EITHER | EITHER | EITHER (rename atomicity holds across process crashes; no power-loss claim) |
| After replace, failed/absent directory sync (F6) | NEW readable; power-loss durability caveated | NEW readable; durability caveated | NEW readable; no durability claim |
| Recovery-artifact write/sync failure (F7) | Artifact discarded/marked invalid and excluded; target unaffected | same | same |
| Cleanup interrupted (F8) | Artifacts valid-or-excluded; never partially promoted | same | same |
| Autosave failure (F9) | Non-blocking notification; retry next trigger; target untouched | same | same |

Recovery-state observability. Ordering authority follows BEHAVIOR-SPEC
§9.3: same-device artifacts order solely by greatest monotonic per-device
sequence number (ties impossible locally); timestamps are display-only and
never determine order; cross-device or same-sequence collisions require
explicit user choice (BEHAVIOR-SPEC §9.4).

| State | Meaning | Machine-readable category | Deterministic action |
|---|---|---|---|
| RS1 | Valid old package (target passes full semantic validation) | `state-valid-target` | Open target; artifacts untouched; stale-artifact notice permitted |
| RS2 | Valid new candidate — target replaced, crash before acknowledgment; byte-level indistinguishable from RS1 and needing no distinction | `state-valid-target` | Same as RS1; no claim about which generation survived, none needed |
| RS3 | Partial candidate (leftover temporary fails validation) | `state-invalid-candidate` | Excluded from precedence; reported; auto-cleaned only if provably not a package (BEHAVIOR-SPEC §9.3) |
| RS4 | Stale candidate (older valid artifact behind a newer sequence) | `state-stale-candidate` | Listed; never auto-promoted; cleanup only by explicit user action |
| RS5 | Corrupt old package | `state-corrupt-target` | Fall to artifact precedence (BEHAVIOR-SPEC §9.4 row 2); target never overwritten silently |
| RS6 | Corrupt candidate | `state-invalid-candidate` | Skipped by precedence, never auto-promoted, reported (BEHAVIOR-SPEC §9.4 row 4) |
| RS7 | No package validates anywhere | `state-no-recovery` | Opening fails with diagnostics naming the target and every artifact attempted |
| RS8 | Ambiguous multiple candidates (cross-device artifacts or same-sequence collision) | `state-ambiguous-recovery` | No auto-selection; user chooses explicitly among validated candidates; opening a candidate requires consent; target replacement only explicit or via save-as |

Binding recovery rules: idempotent and bounded; cleanup failure leaves
artifacts valid-or-excluded, never partially promoted (F8); if a replacement
outcome is genuinely unknowable after a crash, ambiguity is reported and the
deterministic safe actions above offered — the application never claims which
version was durable without evidence. Diagnostics identify path, phase,
category, affected generation, and remediation; untrusted metadata (names,
URLs) is rendered inert, never treated as control text. Users may inspect,
list, quarantine, or export-as-copy validated candidates; promoting an
invalid artifact or automatically deleting valid candidates is prohibited.
All states observable through CLI/API output with these categories, not logs
alone. Discovery/admission treats recovery artifacts as untrusted input
under §9.1.

Fault-injection matrix. The complete normative matrix is BEHAVIOR-SPEC §9.5;
its injection points and guaranteed outcomes are reproduced verbatim for
cross-reference, together with format-artifact state, implementing issue, and
fixture duties:

| # | Injection point (verbatim, BEHAVIOR-SPEC §9.5) | Guaranteed observable outcome (verbatim) | Artifact state (this spec) | Implementing issue | Fixture duty |
|---|---|---|---|---|---|
| F1 | First save, no prior target; failure before replace | No target file exists; leftover candidate ignorable and cleanable | RS3 | 0029 | 0028 |
| F2 | Candidate write interrupted | Target untouched: previous package readable; leftover candidates ignorable and cleanable | RS1 + RS3 | 0029 | 0028 |
| F3 | Candidate validation fails | Save aborts with an actionable error; target untouched | S2 abort path | 0029 | 0028 |
| F4 | File-content sync fails | Save aborts with an error; target untouched | S3 abort path | 0029 | 0028 |
| F5 | Atomic replacement interrupted | Old-or-new readable, per the rename-atomicity assumption | EITHER row above | 0029 | 0028 |
| F6 | Replacement completed; directory sync fails or unsupported | New package readable but durability uncertain until the platform flushes; the result acknowledges the caveat; no rollback | S5 caveat path | 0029 | 0028 |
| F7 | Recovery-artifact write or sync fails | Artifact discarded or marked invalid and excluded from precedence; target unaffected | RS6 exclusion | 0029 | 0028 |
| F8 | Cleanup interrupted | Artifacts remain valid or excluded per discovery rules; never partially promoted | RS3/RS4/RS6 invariants | 0029 | 0028 |
| F9 | Autosave failure | Non-blocking notification; retry at next trigger; unsaved-changes state unchanged; target untouched | autosave path (BEHAVIOR-SPEC §9.3) | 0029 | 0028 |

Validation-profile qualifier (verbatim meaning preserved): old-or-new
readability including power-loss behavior is claimed only for the Linux ext4
and Windows NTFS profiles and only after their fault-injection suites pass;
all other platforms are process-crash-only under explicitly declared
capabilities.

Extended injection points — **Proposed**, routed through the BEHAVIOR-SPEC
§9.5 amendment process or into issue 0029's test plan; not normative until
adopted:

| ID | Injection point | Expected outcome |
|---|---|---|
| X1 | Temporary creation failure / name collision / permission denied | Save aborts; target untouched; actionable diagnostic |
| X2 | Short write, zero-progress write, ENOSPC, quota exhaustion, EIO, descriptor exhaustion, source-read failure during S1 | Abort; target untouched; leftover cleanable |
| X3 | Interruption at every write boundary: header, manifest, each entry, large streaming chunk boundary, index/footer, close, final size accounting | F2 class |
| X4 | Sync success/failure/unsupported/close-failure at S3; crash before vs after sync | F4 / F2 classes respectively |
| X5 | Validation/reopen failure after seemingly successful write; digest/checksum failure post-write | F3 class |
| X6 | Replace refusal: target lock/sharing violation, permission failure, cross-device attempt, crash before/during/after replace, before/after directory sync | F5/F6 classes; sharing-violation specifics are Deferred — issue 0029 empirical characterization (public Microsoft pages do not enumerate them, P3) |
| X7 | Startup with each old/new/temporary/candidate combination; stale-cleanup failure; ambiguous generations; corrupt marker; corrupt package; both invalid | RS1–RS8 outcomes |
| X8 | Concurrent reader during replace; concurrent writer; cancellation mid-save; repeated retry; restart mid-phase | Concurrent-writer non-guarantee applies; readers see old-or-new under namespace atomicity |
| X9 | Cache-GC anomaly coinciding with recovery (metadata checksum/generation anomaly) | Fail-closed collector mode per ASSET-STRATEGY §10.7 — zero collection; recovery proceeds on pins alone (G4) |

Every injection asserts: readability per guarantee table, semantic projection
integrity, opaque-data preservation, no partial-success report, diagnostics
emitted, cleanup performed or safely deferred, bounded resource use. Tests run
on the documented platform matrix; mocked errors alone never establish crash
durability. Injection is deterministic; assertions avoid timing dependence
(BEHAVIOR-SPEC §1.7).

External modification detection: per normative BEHAVIOR-SPEC §9.2, including
its declared race boundary (a modification between baseline comparison and
replacement is outside detection capability and falls under the concurrent-
writer non-guarantee).

### 9.8 Provenance records

Verbatim-observation records following the ASSET-STRATEGY §10.10 pattern:
no clearance claims, no GPL source consultation, access date 2026-08-23 for
all entries. "Fetched" means loaded and checked during issue 0023 research;
independently re-verified items are marked.

- **P1 — rename(2), Linux man-pages.** "rename(2) — Linux manual page";
  Linux man-pages project; page from man-pages-6.18 (2026-02-08);
  https://man7.org/linux/man-pages/man2/rename.2.html (fetched). Facts used:
  atomic replacement of an existing target ("no point at which another
  process … will find it missing"); EXDEV across different mounted
  filesystems; NFS bug note — rename failure does not prove the rename did
  not happen (grounds honest F5/EITHER treatment).
- **P2 — fsync(2), Linux man-pages.** Same project/version;
  https://man7.org/linux/man-pages/man2/fsync.2.html (fetched). Facts used:
  data+metadata synchronization; verbatim directory-durability requirement
  quoted in §9.7 step S5; ENOSPC/EIO/EINTR error classes. Flag honored: the
  open-directory-read-only detail is not attributed to this page.
- **P3 — ReplaceFileW, Microsoft Learn.** "ReplaceFileW function
  (winbase.h)", ms.date 2018-12-05, updated 2025-07-01;
  https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew
  (fetched). Facts used: preserves attributes/streams/security of the
  replaced file; backup/replaced/replacement files must share one volume;
  documented partial-failure modes; REPLACEFILE_WRITE_THROUGH unsupported.
  Flag honored: sharing-violation codes are not enumerated publicly ⇒ X6
  deferred to issue 0029 empirics.
- **P4 — FlushFileBuffers, Microsoft Learn.** "FlushFileBuffers function
  (fileapi.h)"; https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-flushfilebuffers
  (fetched). Facts used: GENERIC_WRITE handle requirement; buffered-data
  flush semantics; FILE_FLAG_NO_BUFFERING/WRITE_THROUGH alternatives.
- **P5 — POSIX.1-2017 rename/fsync.** IEEE Std 1003.1-2017, The Open Group
  Base Specifications Issue 7, 2018 edition;
  https://pubs.opengroup.org/onlinepubs/9699919799/functions/rename.html and
  https://pubs.opengroup.org/onlinepubs/9699919799/functions/fsync.html
  (both fetched). Facts used: ISO-C-required rename atomicity; mandatory
  EXDEV across filesystems; fsync null implementation permitted when
  `_POSIX_SYNCHRONIZED_IO` is undefined — why POSIX conformance alone
  grounds no power-loss claim.
- **P6 — macOS.** Archived Apple fcntl(2) man page (BSD System Calls
  Manual), https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/fcntl.2.html
  (fetched): F_FULLFSYNC asks the drive to flush buffered data. Apple
  article "Reducing disk writes",
  https://developer.apple.com/documentation/xcode/reducing-disk-writes
  (fetched): F_FULLFSYNC is best-effort; "data can still be lost in the case
  of sudden power loss". No official Apple document establishing rename()
  atomicity was found; macOS therefore remains process-crash-only. Flags
  honored: Apple's archived fsync(2) page was not directly fetched and is
  not quoted or relied upon.
- **P7 — APPNOTE.TXT .ZIP File Format Specification, PKWARE Inc., version
  6.3.10, Status FINAL, revised Nov 01, 2022** (load-bearing artifact).
  https://pkware.cachefly.net/webdocs/APPNOTE/APPNOTE-6.3.10.TXT
  (downloaded and section-verified 2026-08-23; reviewer independently
  re-downloaded and re-verified cited sections). Freely downloadable per its
  §1.5.2; copyright PKWARE Inc.; permitted use covers implementing ZIP
  readers/writers; reproduction of the document itself needs written
  permission; some components marked patented — recorded per
  THIRD-PARTY-POLICY; no APPNOTE text reproduced beyond short section
  references. Whether pkware.com's unversioned "latest" pointer serves a
  different minor version could not be reproduced this session
  (unable-to-verify-this-session) and is relied upon nowhere; the versioned
  artifact is the sole cited authority.
- **P8 — Limit-value groundings** (access date 2026-08-23):
  - **P8a — WebP dimension ceiling.** "WebP FAQ", Google Developers, page
    last updated 2026-02-03; https://developers.google.com/speed/webp/faq
    (fetched; WebP uses 14 bits for width/height ⇒ maximum
    16,383 × 16,383). Grounds L20. Reviewer re-verified.
  - **P8b — GPU texture ceiling precedent.** "Constants (D3D12.h)",
    Microsoft Learn, ms.date 2018-05-31, updated 2021-12-30;
    https://learn.microsoft.com/en-us/windows/win32/direct3d12/constants
    (search-index verified; `D3D12_REQ_TEXTURE2D_U_OR_V_DIMENSION = 16384`).
    Grounds L20. Reviewer re-verified.
  - **P8c — JSON parser-limits permission.** RFC 8259, T. Bray ed., IETF,
    December 2017; https://www.rfc-editor.org/rfc/rfc8259.txt (fetched).
    §9 permits implementations to set limits on text size, nesting depth,
    and number range/precision. Grounds L11, L12, L17, L18. Reviewer
    confirms accurate.
  - **P8d — UUID textual form.** RFC 9562, Davis, Peabody, Leach, IETF,
    May 2024 (obsoletes RFC 4122);
    https://www.rfc-editor.org/rfc/rfc9562.txt (fetched). §4 ABNF canonical
    form = 36 characters. Grounds L19. Reviewer confirms accurate.
  - **P8e — Nesting-depth precedents.** (i) serde_json documentation/source
    (`remaining_depth: 128`;
    https://docs.rs/serde_json/latest/serde_json/de/struct.Deserializer.html,
    version noted 1.0.151). (ii) Go encoding/json CL 199837 "limit max
    nesting depth" (merged Go 1.15.x; `maxNestingDepth = 10000`;
    https://go.dev/cl/199837; motivating issue
    https://github.com/golang/go/issues/31789). Grounds L11's conservative
    128 choice.
  - **P8f — Absence of ZIP ratio guards in Go.** golang/go#33036,
    "archive/zip: provide API for resource limits", filed 2019-07-10, still
    open; https://github.com/golang/go/issues/33036 (search-verified).
    Grounds recording L15/L16 as Domowik policy rather than industry
    default. Reviewer re-verified.
  - **P8g — JPEG/libjpeg dimension facts.** ITU-T Recommendation T.81,
    16-bit SOF sample fields ⇒ 65,535 × 65,535, consulted via W3C mirror
    https://www.w3.org/Graphics/JPEG/itu-t81.pdf; Independent JPEG Group
    jmorecfg.h constant `JPEG_MAX_DIMENSION = 65500L`, header consulted in
    the official IJG source distribution (jpegsrc, mirrored publicly at
    https://www.ijg.org/files/). Context for L20's stricter cap.
  - **P8h — Info-ZIP overlap-based bomb detection.** Info-ZIP unzip(1) man
    page (https://man.archlinux.org/man/unzip.1.en.html) documenting the
    CVE-2019-13232 fix (default-on overlapped-components detection,
    `UNZIP_DISABLE_ZIPBOMB_DETECTION=TRUE` escape); upstream discussion
    https://github.com/madler/unzip/issues/13. Guard precedent alongside
    L15/L16.
  - **P8i — CERT IDS04-J.** "IDS04-J. Safely extract files from
    ZipInputStream", SEI CERT Oracle Coding Standard for Java, Carnegie
    Mellon University Software Engineering Institute. Full-title wiki URL
    https://wiki.sei.cmu.edu/confluence/display/java/IDS04-J.+Safely+extract+files+from+ZipInputStream
    (the truncated display/java/IDS04-J form does not resolve); stable
    rendering https://cmu-sei.github.io/secure-coding-standards/sei-cert-oracle-coding-standard-for-java/rules/input-validation-and-data-sanitization-ids/ids04-j/
    (fetched; reviewer re-verified). Facts used: `TOOBIG = 0x6400000`
    (≈100 MB) enforced against a cumulative total uncompressed-output
    accumulator never reset per entry, alongside `TOOMANY` = 1,024 entries —
    an absolute-cap + count-cap precedent, cited for no per-entry ratio
    value.
  - **P8j — DEFLATE theoretical maximum ratio ≈1032:1.** Attack-literature
    figure for worst-case single-stream expansion; consulted via public
    zip-bomb analyses (e.g., https://en.wikipedia.org/wiki/Zip_bomb,
    accessed 2026-08-23). Secondary corroboration of a mathematical property
    of DEFLATE, independently derivable from the format's structure. Grounds
    L15's placement below the ceiling.
  - **P8k — Negative findings** (recorded, never cited as positive
    authority): Python zipfile contains no expansion-ratio/bomb guard
    (Lib/zipfile/__init__.py, CPython main branch, source-inspected); JDK
    system properties `jdk.util.zip.maxExpansionBytes` /
    `maxExpansionFactor` do not exist (JDK master and jdk21 sources, Jira
    full-text search, release notes — zero hits). Grounds the refused-claims
    list of the issue record.
  - PNG IHDR 2³¹−1 grounding is covered by the existing PNG Specification
    v1.2 §4.1.1 record in ASSET-STRATEGY §10.10 and not duplicated here.
- **P9 — ext4 auto_da_alloc corroboration.** Kernel administrator guide,
  https://docs.kernel.org/admin-guide/ext4.html (fetched): the heuristic
  detects replace-via-rename patterns and forces data blocks out before the
  rename commits; explicitly best-effort, never a guarantee. Supporting
  context for step S5; secondary mailing-list corroboration recorded in the
  issue handoff, nothing normative rests on it.

Every platform claim cites public documentation; a POSIX or Windows API name
is nowhere treated as proof of identical behavior across concrete filesystems
— hence the per-filesystem-class matrix in §9.7. Any reference whose
provenance is uncertain blocks adoption and is quarantined rather than
normalized into the specification.
