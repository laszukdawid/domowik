# Asset and Size Strategy

Status: **Requirements accepted; encodings and budgets provisional.
Section 10 is normative v0.1 asset/package/retrieval/corpus material
authored under issue 0021 (partial scope).** Asset size is a primary
product constraint, not an afterthought.

> **Provisional handoff — 0021 partial scope.** Section 10 is the
> agreed deliverable of issue 0021 while its deferred criteria remain open
> (logical operation/batch metadata via issue 0004 / ADR 0004; physical
> batch framing and container compression via issue 0005; furnished corpus
> via BEHAVIOR-SPEC §10 acceptance + 0004; measurements, preview records,
> and encoding benchmarks via the follow-up measurement issue 0035,
> published through issue 0018's existing criterion; conformance fixtures
> via issue 0028). The former "retrieval-limit values via issue 0023" gate
> is resolved: issue 0023 ratified the numeric retrieval limits in
> FORMAT-SPEC §9.3 (rows L24–L28, 2026-08-23), and §10.8 records the
> resolved mapping below. AC-4's remaining gate is issue 0028 conformance
> fixtures only. Downstream issues may consume only this labeled material;
> no part of it is a completed v0.1 asset contract while the partial scope
> holds.

**Gate clarification — 2026-10-08:** BEHAVIOR-SPEC §10 now contains normative
experimental-editor behavior. The furnished-corpus gate still requires 0004
to reconcile that behavior with provisional Tier B format envelopes and the
corpus contract. The corpus remains dormant; no measurements or encoding
criteria are discharged by the existence of editor import. See the
[issue audit](../issues/AUDIT-2026-10-08.md).

## Goals

- Keep ordinary plan documents small enough to sync and version cheaply.
- Avoid storing the same model or texture more than once.
- Open plans before every optional asset is available.
- Support both a small working file and a self-contained file for sharing.
- Make very large splat datasets progressively streamable.
- Never require generated wall/floor meshes to be persisted.

## Data classes

1. **Authoritative plan data:** compact procedural entities such as wall axes,
   thicknesses, levels, opening parameters, labels, and asset instances.
2. **Asset descriptors:** digest, media type, byte length, encoding, purpose,
   provenance, and optional license metadata.
3. **Content blobs:** images, textures, models, fonts, and splat chunks,
   identified by digest of their stored bytes.
4. **Derived caches:** thumbnails, triangulations, GPU textures, render meshes,
   and room polygons. They are disposable and must never be the sole copy of
   user-authored data.

## Package profiles

### Thin package (default working form)

The `.domo` file contains the plan, operation/snapshot metadata, asset
descriptors, and small essential previews. Large blobs live in a shared local
content-addressed cache or optional remote store.

Identical content used by many plans is stored once. Missing optional assets
must produce placeholders and actionable diagnostics, not make the plan
unreadable.

### Portable package

A portable `.domo` embeds every referenced asset blob, including optional
assets, and therefore has no external asset dependency. It is larger but works
as a single transferable file. Packing and thinning are explicit operations;
the logical plan does not change. Packing must report anything that cannot be
embedded and fail rather than silently produce an incomplete portable file.

The CLI should eventually expose:

```text
domowik asset add|list|verify|remove
domowik pack --portable
domowik pack --thin
```

## Content addressing and deduplication

The candidate identifier is `sha256:<lowercase-hex>`, calculated over the
exact stored blob bytes. The final format issue must define the algorithm,
path mapping, and whether any canonical decoding is involved. Asset UUIDs are
not content addresses.

Every descriptor records at least:

- digest and byte length;
- media type and encoding/profile;
- original file name only as optional display metadata;
- whether the asset is required or optional;
- provenance and optional SPDX license identifier/license URI.

Import verifies the digest while streaming. Blobs are deduplicated before
write. A stable package-level asset-root table lists all reachable digests,
including those used by opaque future entities, so older readers can preserve
them without understanding future schemas. Garbage collection also needs a
defined cache pin/lease policy and must not remove blobs referenced by retained
snapshots or operations.

## Encoding guidance

These are candidates to benchmark, not fixed format requirements:

| Asset | Candidate approach |
|---|---|
| Source images | WebP or AVIF with explicit dimension/quality profiles; originals retained only on request |
| Runtime GPU textures | KTX2/Basis Universal when platform support and tool licensing are validated |
| Furniture models | glTF/GLB with instancing and meshoptimizer compression; avoid repeated geometry |
| Fonts | Reference system/bundled fonts deliberately; subset embedded fonts when license permits |
| Thumbnails | Small, regenerable previews with fixed maximum dimensions |
| Gaussian Splats | Quantized, spatially chunked, multi-level-of-detail representation; raw PLY is interchange only, not the working format |

Procedural plan geometry is stored once and meshed at runtime. Repeated
furniture uses one content blob plus many transforms.

## Provisional size budgets

Budgets must be verified against a published representative corpus before
format v0.1. Initial targets, excluding external cache contents:

| Scenario (compacted snapshot plus a defined short history window) | Thin package target |
|---|---:|
| Empty plan | under 16 KiB |
| Unfurnished apartment (about 1,000 entities) | under 256 KiB |
| Furnished plan referencing cached assets | under 1 MiB |

The corpus definition must record retained operation count/window, compression
settings, and included previews; history overhead is reported separately.
Portable-package limits depend on user-selected asset quality. The tool must
show embedded and external byte totals before packing.

For splats, define two separate measured budgets after selecting a corpus:

- bytes needed for a first useful preview;
- total bytes for each quality level.

No numerical splat budget should be promised before representative indoor
scans are benchmarked.

## Large assets and streaming

- Splat and high-resolution assets must be chunked so the viewer can request
  visible/appropriate levels of detail without reading the whole dataset.
- Phone reconstruction source photographs are not embedded in a plan by
  default. Retention is an explicit user choice.
- Thin files may contain remote retrieval hints, but identity is always the
  content digest; URLs are replaceable locations, not identity. Retrieval is
  disabled or user-approved by policy, allows only configured schemes, follows
  bounded redirects, rejects private/link-local destinations unless explicitly
  trusted, sends no ambient credentials, enforces time/byte limits, and
  verifies the digest before decoding or cache admission.
- Portable containers need ZIP64 or another streaming-capable container if
  large files are supported. The physical container remains an open decision.

## Security and limits

All imports are untrusted. Implementations must cap total expanded bytes,
individual asset bytes, image dimensions, chunk counts, path length, archive
depth, JSON nesting, and decompression ratio. Reject duplicate logical paths,
path traversal, encrypted entries unless explicitly supported, digest
mismatches, and inconsistent declared lengths. Decode images/models/splats in
bounded worker contexts where practical.

Exact limits become part of the format/security conformance corpus.

---

## 10. Normative v0.1 assets, packages, retrieval, and corpus (issue 0021, partial scope)

Status: **Normative for v0.1 within the partial scope labeled above.**
Where a rule's final value belongs to another owning issue, that ownership
is stated inline. This section supersedes the provisional wording of
sections "Package profiles"–"Security and limits" above wherever the two
differ; behavioral consequences remain owned by BEHAVIOR-SPEC §9.6 and are
referenced, not restated.

### 10.1 Digest identity and hash domain

- Asset identity is `sha256:<64 lowercase hex>` — SHA-256 per FIPS 180-4
  over the **logical blob byte sequence** `B`: the exact stored bytes of
  the asset in its stored form, independent of any physical carrier.
  Uppercase hex or unprefixed forms are invalid identifiers.
- Container-level compression applies outside the hash domain:
  decompressing a package entry yields `B` verbatim.
- Transport decoding order is fixed: HTTP transfer coding (e.g., chunked)
  is undone first; a non-identity `Content-Encoding` is rejected, not
  transparently decoded. After transfer-decoding, received bytes must
  equal `B`.
- A descriptor's `byteLength` is `len(B)`; declared length ≠ actual length
  ⇒ rejection.
- Thin↔portable conversion copies `B` verbatim (embedding is a byte copy,
  never a re-encode); transcoding creates a new asset with a new digest.
  Writers must fail rather than re-encode during pack/thin. Conversion
  therefore cannot change identity.
- Digest verification proves integrity against a fixed expected digest
  only. It does not authenticate origin, prove authorship, or substitute
  for signatures.

### 10.2 Descriptor schema (v0.1)

| Field | Req | Type/notes |
|---|---|---|
| digest | yes | `sha256:<lowercase hex>` per §10.1 |
| byteLength | yes | non-negative integer = len(B); mismatch ⇒ reject |
| mediaType | yes | `type/subtype` token; unregistered types use an `application/x-…` convention until MIME strategy is revisited (NAMING.md Deferred-by-rule) |
| purpose | yes | token naming the descriptor's intended role (e.g., `background-image`, `furniture-model`, `texture`); drives diagnostics ("which entities or levels use it", BEHAVIOR-SPEC §9.6) and lookup policy |
| encoding / profile | yes | exact-comparison string identifying compression/container profile; required by the accepted strategy requirements |
| requirement | yes | `required` \| `optional`; behavior per BEHAVIOR-SPEC §9.6 |
| provenance | yes | structured block: original display name (never functional), source description/URI, author string, acquisition date — attribution metadata only; required by the accepted strategy requirements |
| license | no | SPDX expression **or** license URI, optional copyright string; absence permitted for user-imported content and surfaced informationally by `validate`; anything the project distributes requires a recorded per-asset entry |
| placeholderDimensions | no | width/height/depth in mm; consumed by the BEHAVIOR-SPEC §9.6 placeholder rule |
| hints | no | list of `{url, addedAt, lastVerifiedAt?, expiresAt?}`; semantics in §10.8/§10.5; physical placement is issue 0005's decision |

The required set matches the accepted strategy requirement that every
descriptor records at least digest, byte length, media type, encoding/
profile, purpose, provenance, and the required/optional flag; license
metadata remains optional per the same accepted text. `purpose`,
`placeholderDimensions`, and `hints` are additive v0.1 fields — they do
not regress any accepted requirement.

Asset UUIDs/instance identities and content digests are distinct
namespaces; interpretation lives in the descriptor, never in the blob.

### 10.3 Blob path mapping (logical)

Package and cache blob namespaces are keyed by digest with the logical
mapping `blob/<algorithm>/<2-hex>/<62-hex>`. Constraints (binding on the
container decision, issue 0005): deterministic; injective per digest;
no dependence on filesystem path semantics (no traversal surface);
stable across thin/portable conversion. Physical entry-name serialization
remains 0005's decision.

### 10.4 Package profiles

Thin and portable packages serialize the same logical plan; conversions
are explicit, all-or-nothing, and report embedded-vs-external byte totals
before packing.

- **Thin:** embeds only preview-class blobs (§10.5). Every other blob in
  `U(P)` lives in the cache or a policy-gated remote store/hint.
- **Portable:** embeds every blob in `U(P)` and digest-verifies each. Any
  unverifiable reference aborts packing completely with an omission report
  naming every such asset (BEHAVIOR-SPEC §9.6). Blobs that verify but are
  locally undecodable embed opaquely with a warning.
- A portable file whose required embedded blob fails digest verification
  has its portability claim treated as broken — never presented as holding
  (BEHAVIOR-SPEC §9.6).

### 10.5 Referenced-blob universe, previews, hints

- **`U(P) = {descriptor digests} ∪ {package asset-root table entries} ∪
  {instance/reference digests}`** — previews are *not* members: they are
  class-4 derived caches. Portable completeness, omission reports,
  root-table enumeration, old-reader obligations, and GC roots quantify
  over `U(P)`.
- **Asset-root table:** stable package-level enumeration of `U(P)`.
  Invariants (binding; wire shape is issue 0005's to accept/adapt):
  (I1) enumerable without understanding any entity schema; (I2) a reader
  understanding only the table preserves listed blobs on write-back or
  refuses to write — silent stripping forbidden; (I3) membership implies
  nothing about schema comprehension; (I4) writers regenerate from full
  knowledge; partial-knowledge writers never rewrite selectively. Unknown
  descriptor fields/entities follow FORMAT-SPEC §8.
- **Preview records:** document thumbnails live in a dedicated derived-
  cache namespace storing `{digest, byteLength, sourceKey}`. Rendered at
  exactly 512×512 px; stored size ≤4,096 B enforced at every reduction-
  ladder step (halve edge 512→256→128→64 on violation); at 64 px still
  over cap ⇒ preview not stored — document remains valid, warning-class
  derived-cache absence (regenerable from authoritative data alone).
  Absent previews never fail portability and never enter omission reports.
- **Hints:** field semantics normative here wherever located: absolute URI,
  scheme passing the configured allowlist, no userinfo component;
  timestamps RFC 3339 UTC and advisory only (display/diagnostics freshness
  — expiry neither enables nor blocks retrieval; consent governs); set
  semantics, duplicate URLs rejected; add/remove is metadata-only, never
  affecting digest identity or portability truth; invalid hint entries ⇒
  parse rejection with actionable error; when retrieval is disabled hints
  are ignored entirely beyond the offline summary.

### 10.6 Cache lookup, deduplication, dangling references

- Lookup order: embedded blob → local content cache → configured remote
  stores/hints (policy-gated) → missing per BEHAVIOR-SPEC §9.6. Cache
  admission requires streaming digest verification of `B`.
- Writers deduplicate before write; readers tolerate duplicate physical
  occurrences defensively.
- Diagnostic classes: missing blob; digest mismatch ≡ missing (§9.6);
  dangling instance→digest reference with no descriptor. For the latter:
  - malformed input (digest syntax/type invalid) ⇒ hostile-input rejection
    per FORMAT-SPEC §9;
  - well-formed but unresolved ⇒ treated as **required-missing** under the
    §9.6 matrix ⇒ read-only degraded inspection; placeholder per §9.6
    (fixed 1000 mm box absent declared dimensions); distinct diagnostic
    category naming referencing element, digest, remediation. Portable
    packing fails completely. References persist through save cycles with
    unchanged meaning and resolvable identity. This default is owned by
    issue 0021; issue 0004 holds an informational cross-check and may amend
    it through the normal process.

### 10.7 Cache garbage collection — pins and leases, fail-closed

- **Pin** `{digest, ownerId, rootKind, createdAt}`, rootKind ∈
  `{snapshot, operation-frontier, root-table, manual}`: durable protection;
  persists across restarts; created when a document registers with the
  cache; released only by durable root-set diff or explicit counterpart
  action (`manual` pins survive document deregistration; surface: issue
  0026 `asset` commands).
- **Lease** `{digest, holderId, expiresAtUtc}` plus durable monotonic
  anchor `(bootID, monoDeadline)` where available: **advisory**, time-bound
  permission. Durability-critical protection uses pins only.
- Expiry decision (factored as a pure function for fixture testing):
  same-boot collection requires `now_mono > monoDeadline`; cross-boot (or
  UTC-only platforms) requires persisted UTC past `expiresAtUtc + M`,
  M = 24 h default. Residual risk declared: forward wall-clock jumps
  combined with reboot can collect an active lease early; harm is bounded
  by digest-keyed re-admission (identical bytes) and holder re-acquire
  duties. Platform modes: Linux uses `/proc/sys/kernel/random/boot_id` +
  `CLOCK_MONOTONIC` (suspend does not advance it — lease life extends, the
  safe direction); Windows/macOS default to UTC-only mode unless a verified
  monotonic mechanism is documented. Active mode is observable at collector
  startup.
- Atomicity guarantees: (G1) a digest becomes collectable only after its
  last protecting record is durably removed — crashes yield over-retention,
  never premature deletion; (G2) admission and collection are mutually
  atomic against the metadata store; admitting a digest atomically restores
  protection for every registered document whose current root set includes
  it; (G3) collection never inspects, decodes, or rewrites content; (G4)
  GC correctness is never load-bearing for data safety because pins cover
  every root.
- **Fail-closed under metadata anomalies:** records carry checksums and a
  monotonic store-generation counter; startup verifies both. Any anomaly
  (checksum failure, generation gap/regression, truncation, unparsable
  record) ⇒ collector enters fail-closed mode: **zero collection** until
  repair. Rebuild recomputes reachable root sets from registered documents'
  retained snapshots/operations/frontier data and available package asset-
  root tables, then re-pins the conservative superset; collection resumes
  only after a clean validated rebuild; unreadable documents' previously
  pinned digests stay protected (retained-until-repair). This implements
  the accepted rule that GC must not remove blobs referenced by retained
  snapshots or operations, without exception.

### 10.8 Remote retrieval policy

Structure normative here; numeric values and the redirect bound were
**provisional until ratification by issue 0023 — now ratified: see
FORMAT-SPEC §9.3 (2026-08-23)**; adversarial fixtures are
owned by issue 0028 and gate conformance claims.

**Ratified-value ledger (AC-4).** Authority split is binding: policy
*structure* below is owned by this section (issue 0021) until issue 0023
formally amends it; *numeric values* resolve toward FORMAT-SPEC §9.3 as
ratified by issue 0023. "Structural" rows have no numeric row of their own;
their numeric-row cell is N/A by design, not a deferral.

| Obligation | Normative structure | Value / semantics | Numeric row / status / owner | Error / boundary behavior | Fixture gate |
|---|---|---|---|---|---|
| Consent gate; retrieval disabled by default | §10.8 `RetrievalPolicy.consent` | default `false` | N/A — Structural — 0021 | Fetching without consent never occurs; offline summary per BEHAVIOR-SPEC §9.6 | 0028 |
| Scheme allowlist + normalization | §10.8 normalization rules | default `["https"]`; NFKC → lowercase → ASCII exact match; empty entries invalid | N/A — Structural — 0021 | Non-allowlisted scheme ⇒ reject before dial | 0028 |
| Hint host form; IP-literal rejection | §10.8 normalization rules | lowercase IDNA A-label, trailing dot stripped; IP-literal hosts rejected outright in v0.1 | N/A — Structural — 0021 | Invalid host form ⇒ parse rejection | 0028 |
| Trusted destinations; private/link-local rejection | §10.8 destination classes + `trustedDestinations` | exactly-one-of FQDN/CIDR; network addresses with host bits set invalid; loopback, RFC 1918, link-local incl. 169.254.169.254, ULA, link-local multicast, IPv4-mapped IPv6 rejected unless user-configured | N/A — Structural — 0021 | Never document-supplied, never hint-inferred; uncovered destination ⇒ reject | 0028 |
| No ambient credentials; userinfo prohibition | §10.8 credentials rule | no cookies, auth headers, proxy credentials; userinfo URLs invalid | N/A — Structural — 0021 | Userinfo URL ⇒ parse rejection; nothing credential-bearing transmitted | 0028 |
| Redirect re-validation | §10.8 redirects rule | full per-hop revalidation: scheme, destination classes, dial binding; no downgrade; no credential forwarding | N/A — Structural — 0021 | Any failed hop validation aborts before payload bytes | 0028 |
| Resolver-to-dial binding (fail-closed) | §10.8 resolver-to-dial rule | candidate set A validated to V ⊆ A; V = ∅ ⇒ reject; dial-by-address with post-connect peer ∈ V verification; Happy-Eyeballs only within V | N/A — Structural — 0021 | Unbindable stack ⇒ fetch rejected, no hostname-dial fallback | 0028 |
| Origin identity through TLS/HTTP | §10.8 origin-identity rule | SNI = hop's origin hostname; PKIX name checking against origin hostname; Host/:authority in authority-form `host[:port]` | N/A — Structural — 0021 | Any mismatch aborts before payload byte ⇒ missing-blob handling | 0028 |
| Digest-before-decode/admission | §10.8 verification rule | digest over transfer-decoded bytes before any decode and before cache admission | N/A — Structural — 0021 (numeric admission ceiling on all paths is L5, FORMAT-SPEC §9.3) | Mismatch ⇒ missing-blob handling | 0028 |
| Transfer byte cap | §10.8 limits bullet | **L24 = 536,870,912 B (512 MiB)** per retrieval, post-transfer-decoding bytes; > N aborted mid-stream during receipt, before decode/admission | L24 — Ratified — 0023 (FORMAT-SPEC §9.3 L24) | `limit-transfer-bytes` ⇒ missing-blob handling | 0028 |
| Retrieval time caps | §10.8 limits bullet | **L25: connect ≤ 10 s; stalled transfer ≤ 15 s without progress; total ≤ 120 s per retrieval**; exceeding aborts | L25 — Ratified — 0023 (FORMAT-SPEC §9.3 L25) | `limit-transfer-time` ⇒ missing-blob handling | 0028 |
| Redirect hop bound | §10.8 redirects rule | **L26 = 4 hops** maximum; the 5th hop is rejected (exactly 4 succeed); per-hop full revalidation | L26 — Ratified — 0023 (FORMAT-SPEC §9.3 L26) | `limit-redirect-hops` ⇒ missing-blob handling | 0028 |
| Hints-per-session cap | §10.8 hints-per-session category | **L27 = 64 retrieval attempts per document per session**; excess skipped with summary diagnostic | L27 — Ratified — 0023 (FORMAT-SPEC §9.3 L27) | `limit-hints-session`; skipped attempts surface in the session summary | 0028 |
| Concurrent-retrieval bound | §10.8 concurrency category | **L28 = 4 concurrent requests per document**; excess queued, then capped by L27 | L28 — Ratified — 0023 (FORMAT-SPEC §9.3 L28) | `limit-concurrent-retrieval` via scheduler queuing | 0028 |

Configuration schema:

```text
RetrievalPolicy {
  consent             : boolean          ; default false — retrieval disabled by default
  schemes             : list<string>     ; default ["https"]
  trustedDestinations : list<Destination>; default []
}
Destination { host? : FQDN ; cidr? : IPNetwork }   ; exactly one set
```

Normalization/validation (normative):

- Scheme: NFKC → lowercase → ASCII; exact match against the allowlist;
  empty entries invalid.
- Host: lowercase IDNA A-label (punycode), trailing dot stripped; match =
  exact equality with the normalized connection host. IP-literal hint
  hosts are rejected outright in v0.1.
- CIDR: strict dotted-quad/prefix or IPv6/prefix parse; network addresses
  with host bits set are invalid (reject, don't mask).
- Consent gate: fetching reveals client IP/time to the hint host; consent
  covers this. Offline behavior per BEHAVIOR-SPEC §9.6 summary UX.
- Redirects: bounded count (ratified by issue 0023: maximum 4 hops; see
  FORMAT-SPEC §9.3 row L26), full re-validation
  per hop (scheme, destination classes, dial binding), no downgrade, no
  credential forwarding.
- Private/link-local destinations (loopback, RFC 1918, link-local incl.
  169.254.169.254, ULA, link-local multicast, IPv4-mapped IPv6) are
  rejected unless covered by a user-configured `trustedDestinations`
  entry — never document-supplied, never inferred from hints.
- Credentials: none ambient — no cookies, auth headers, proxy credentials;
  userinfo URLs invalid.
- Limits: transfer byte/time caps and hints-per-session cap — categories
  normative here, values ratified by issue 0023 in FORMAT-SPEC §9.3
  (rows L24–L25, L27–L28); structure below remains owned by issue 0021.
- **Resolver-to-dial binding (fail-closed):** resolve → candidate set A;
  validate → subset V ⊆ A; V = ∅ ⇒ reject. The connection must be dialed
  to one specific address from V via a dial-by-address API (or equivalent
  binding making the peer address provably ∈ V); stacks that cannot
  guarantee this ⇒ fetch rejected outright, no hostname-dial fallback.
  Happy-eyeballs only among V. Post-connect, before any payload byte, the
  peer address is verified ∈ V; mismatch ⇒ abort ⇒ missing-blob handling.
- **Origin identity through TLS/HTTP:** hint hosts must be DNS names.
  TLS SNI = the current hop's origin hostname (never an address);
  certificates validated against the origin hostname per PKIX name
  checking — validation against the dialed IP or skipping name checks is
  prohibited. The HTTP `Host` header / HTTP/2 `:authority` value is the
  current hop's origin in authority-form `host[:port]` (port omitted when
  the scheme default, included otherwise); scheme is conveyed by TLS and
  negotiated protocol, not by Host. Any mismatch — certificate name ≠
  origin hostname, wrong SNI emission, authority-form mismatch, scheme
  downgrade — aborts before any payload byte ⇒ missing-blob handling per
  §9.6. Each redirect hop re-establishes scheme, SNI/certificate, and
  authority bindings for its own origin after full revalidation.
- Verification: digest over received (transfer-decoded) bytes before any
  decode and before cache admission; mismatch ⇒ missing handling.
  Integrity against a fixed expected digest only — no authenticity claim.

Limit-category ownership (values → ratified by issue 0023 in the
consolidated FORMAT-SPEC §9.3 table, 2026-08-23, unless noted; fixtures →
issue 0028): total expanded bytes; per-entry/individual asset bytes; entry
count; path length; archive depth; nesting depth; numeric magnitude
(bounds with issues 0022/0030 — still owner-deferred there); decompression
ratio; image dimensions;
model complexity; chunk counts (future splat classes); operation count
(operation-count limits: logical batch metadata owned by issue 0004,
physical framing owned by issue 0005 — still owner-deferred there);
transfer byte/time caps; redirect hop bound;
hints-per-session cap. FORMAT-SPEC §9 is the designated home for the
consolidated limit table; that table now exists (FORMAT-SPEC §9.3) and was
ratified there by issue 0023. The structure in this policy remains owned by
issue 0021 until issue 0023 formally amends it; conflicts resolve toward
0023 for numbers, 0021 for structure until 0023 amends it.

### 10.9 Size corpus (Part A normative; Part B open)

**Part A (this issue):** static authoritative content of two scenarios.

Scenario `empty-v1`: exactly 1 level — name `"Level 1"`, elevationMm 0,
floorThicknessMm 200, defaultWallHeightMm 2500, defaultCeilingHeightMm
2500, background anchor absent; zero entities of every other type.
(Part B history framing: open — see below.)

Scenario `unfurnished-1000-v1`: exactly 1,000 entities — 4 levels
(names `"Level 1"`–`"Level 4"`, elevations 3000·ℓ mm, defaults 200/2500/
2500), 480 walls, 320 wall joins, 156 openings, 40 room metadata records.
Zero descriptors; zero instances; labels/dimensions/asset instances are
out of scope (draft BEHAVIOR-SPEC sections; exclusion list in the issue).

Geometry construction (exact): rings r∈{0…9}, origin
((r mod 5)·15000, (r div 5)·12000) mm, size 12000×9000 mm; four base
segments per ring (bottom/right/top/left edges); bottom/top split at
4000/8000 mm, sides at 3000/6000 mm ⇒ three sub-walls each ⇒ 480 walls;
join at every interior split point ⇒ 320 joins; one room-metadata record
per ring with seed at ring centre `(15000·(r mod 5)+6000, 12000·(r div 5)
+4500)` ⇒ 40 rooms (optional display name omitted). Openings (per
ED-0025): number ring instances globally g ∈ {0…39} in level-major/
ring-major order (levels ℓ ∈ {1…4} ascending, rings r ∈ {0…9} within
each level; global instance g = 10·(ℓ-1) + r). Instances g ∈ {0…38}
(the first 39 of 40 total) each receive exactly four doors, one on the
middle sub-wall of each edge in order bottom, right, top, left ⇒ 156
non-overlapping openings (39 instances × 4 edges). Instance g=39
(ring 9 on Level 4) receives no doors.

Authoritative payload values (all literal):

- Walls: `{levelId, startMm, endMm, thicknessMm=200, heightMm=2500,
  style=["S0"]}`; traversal direction fixes Start: edge order
  bottom→right→top→left, sub-walls start→end.
- Openings (R4.0.5 set): `kind=door`, `u=0.5`, `widthMm=900`,
  `heightMm=2100`, `sillMm=0`, `depthMm=120`, `side=Left`, `hinge=
  nearStart`, `opensToward=Left`, `swingDeg=90°`, `style=["S0"]`.
- Rooms: `{levelId, seedMm}`; optional name/style omitted.
- Style vocabulary (per ED-0025, generator-local): exactly one token
  `"S0"` for corpus self-check only. Walls and openings serialize
  `styleRefs:["S0"]`; room optional style references stay absent. The
  generator validates every serialized style token against this local
  vocabulary. No wire-level registry field, manifest entry, or cache
  object is added (FORMAT-SPEC §2.3–2.4 permit no authoritative
  document-level registry).
- Entity-path ordering (identity-tag derivation): levels ℓ ascending;
  walls by ℓ, ring r, edge order, sub-walls start→end; joins by segment
  then split point nearest-start-first; openings by ℓ, ring; rooms by ℓ,
  ring. Identity tags = first 16 bytes of `SHA-256(seedBytes ‖
  ASCII(path))`; version/variant bit layout unspecified until issue 0004
  ratifies an identifier format. Timestamps fixed literal
  `2026-01-01T00:00:00Z` (display-only).

Generator self-check (normative): validate every entity against R3.\*/,
R4.\*/, R5.\*/§2 rules; any violation aborts generation naming the entity;
openings additionally abort with `missing-opening-field:<field>` if any
R4.0.5-authoritative field is absent.

Deterministic primitives (literals verified this session via Python
hashlib, coreutils sha256sum, and OpenSSL 3.6.3 — three-way agreement):

- Seeds: `SHA-256("domowik-corpus/empty-v1")` first 16 bytes =
  `6bcb425196e349a259d7d4861e76edf7`; `SHA-256("domowik-corpus/unfurnished-1000-v1")`
  first 16 bytes = `d2ef08ed140beebf797b8a2ddf34bf1c`. Inputs are the exact
  ASCII strings shown, no trailing newline; verify mechanically:
  `printf '%s' '<input>' | sha256sum`.
- RNG: `next(domain, counter)` = first 8 bytes of
  `SHA-256(ASCII(domain) ‖ LE64(counter))`, little-endian. Vector:
  domain `"domowik-corpus/rng/selfcheck"`, counter 0 →
  `0x88DBCD98B68422D4`. Geometry above is fully formula-determined; the RNG
  serves extension slots only.
- Reference thumbnail encoder (evidence vector only; scenario thumbnails
  are measurement-time derived output, not pinned corpus content): PNG
  signature; `IHDR` (512,512,8,6,0,0,0); single `IDAT` =
  DEFLATE(zlib level 9, default strategy, memLevel 8, windowBits 15) over
  512 scanlines each `00` + 2048 × `FF`; `IEND`. With zlib 1.3.2 the
  output is 1,809 bytes, SHA-256
  `49c8c56d509296669f01cf18365984d4b4a3920b9da04b137eb0443d6eefe29f`.
  Environments whose output differs fail the check; no silent acceptance.

**Part B (OPEN — not part of this issue's closure):** history variants
(generated change log; compaction window; batch grouping), the logical
change→operation/batch mapping (identity, counters, frontiers — owned by
issue 0004 / ADR 0004), and physical operation-batch framing plus
container compression settings (owned by issue 0005). Part-B records
close only when both owners deliver. Both variants will always be
reported separately; history cost is never folded into headline budgets.

Reproducibility tiers: Tier R (same generator version ⇒ identical
authoritative intermediate representation + reference-encoder vector) is
claimed only upon verbatim merge of this section including the
recomputation command; Tier W (two runs through one writer ⇒
byte-identical packages) applies once a writer exists; Tier C (logical
equality via 0004's ratified comparator) thereafter. No logical-comparison
or byte-identity claim is made across writers/containers.

Estimates (ESTIMATES—UNMEASURED; KiB = 1,024 B; strict targets
`measured < target`; assumptions: 150–250 B/entity uncompressed,
compression factor assumed 2.0–2.5×, overhead 5 KiB, included preview
≤4 KiB counted in package size):

| Scope | Estimate | Target |
|---|---:|---|
| Part A `empty-v1` | ≤10 KiB | < 16 KiB |
| Part A `unfurnished-1000-v1` | 67.6–131.1 KiB | < 256 KiB |
| Full compacted variant (informational, Part-B-contingent) | 77.6–156.1 KiB | < 256 KiB |
| `furnished-cached-v1` (dormant) | ≈71–136 KiB | < 1024 KiB |

Only Part-A rows support Part-A closure. Measured budgets are established
exclusively by the follow-up measurement issue feeding issue 0018; the
furnished scenario stays dormant until BEHAVIOR-SPEC §10 acceptance + 0004
(non-normative dormant parameters: 24 synthetic blobs — sizes 12×49,152 B,
8×98,304 B, 4×196,608 B, total external 2,162,688 B reported separately;
60 instances; seeds `SHA-256("domowik-corpus/furnished-cached-v1/blob/" ‖
class ‖ "/" ‖ %02d(i))`, bytes = first L bytes of concatenated
`SHA-256(s ‖ BE64(j))` blocks).

Encoding promotion gate: WebP, AVIF, KTX2/Basis Universal, glTF/GLB +
meshoptimizer, and font subsetting remain **candidates — not format
requirements**. No candidate may become a requirement without a completed
benchmark record in the follow-up measurement issue containing: tool
name/version/source, license identifier and notice obligations,
adapted-vs-independent provenance, patent-risk review flag (mandatory for
Basis Universal), named reference environment, encoded+decoded sizes per
fixed quality tier, decode-cost percentiles and peak memory, corpus input
digests, committed raw outputs, and pre-registered failure thresholds.

Exclusions reaffirmed: Gaussian Splats (no v0.1 entity/blob class/budget;
future quantized/chunked/multi-LOD requirements documented above remain
non-normative) and reconstruction source photographs (never embedded by
default; retention is an explicit user choice) stay out of v0.1.

### 10.10 Provenance records (verbatim observations; no clearance claims)

**zlib 1.3.2** — artifact
`https://github.com/madler/zlib/releases/download/v1.3.2/zlib-1.3.2.tar.gz`,
SHA-256 `bb329a0a2cd0274d05519d61c667c062e06990d72e125ee2dfa8de64f0119d16`
(downloaded 2026-08-23); dev-only tool, not distributed. Complete LICENSE
file, verbatim:

> Copyright notice:
>
>  (C) 1995-2026 Jean-loup Gailly and Mark Adler
>
>   This software is provided 'as-is', without any express or implied
>   warranty.  In no event will the authors be held liable for any damages
>   arising from the use of this software.
>
>   Permission is granted to anyone to use this software for any purpose,
>   including commercial applications, and to alter it and redistribute it
>   freely, subject to the following restrictions:
>
>   1. The origin of this software must not be misrepresented; you must not
>      claim that you wrote the original software. If you use this software
>      in a product, an acknowledgment in the product documentation would be
>      appreciated but is not required.
>   2. Altered source versions must be plainly marked as such, and must not be
>      misrepresented as being the original software.
>   3. This notice may not be removed or altered from any source distribution.
>
>   Jean-loup Gailly        Mark Adler
>   jloup@gzip.org          madler@alumni.caltech.edu

Obligations relevant here: condition 1's acknowledgment is optional for
product documentation; conditions 2–3 bind redistribution only (none
today). Implementation status: external tool used **unmodified** as a
dev-only dependency; no zlib source adapted or translated into Domowik.
Test-vector provenance: the reference-PNG determinism vector in §10.9 was
produced with this exact artifact's compression routine and is
recomputable from the command embedded in §10.9; no other vectors are
attributed to zlib.

**PNG Specification, Version 1.2** — consulted artifact
`http://www.libpng.org/pub/png/spec/1.2/PNG-Credits.html` (fetched
2026-08-23; built from `png-master-19990714`, 14 July 1999). Section 19
"Credits", transcribed verbatim and in full:

> 19. Credits
>
> ### Editors
>
> Glenn Randers-Pehrson (PNG 1.1 and PNG 1.2),
> randeg @ alum.rpi.edu
> Thomas Boutell (PNG 1.0),
> boutell @ boutell.com
>
> ### Contributing Editors
>
> Adam M. Costello (PNG 1.1 and PNG 1.2),
> png-spec.amc @ nicemice.net
> Tom Lane (PNG 1.0),
> tgl @ sss.pgh.pa.us
>
> ### Authors
>
> Authors' names are presented in alphabetical order.  Spaces have
> been added to e-mail addresses (around the @ symbols) to
> minimize the amount of unwanted e-mail via automated harvesters.
>
> Mark Adler,
> madler @ alumni.caltech.edu
> Thomas Boutell,
> boutell @ boutell.com
> John Bowler,
> jbowler @ acm.org
> Christian Brunschen,
> christian @ brunschen.com
> Adam M. Costello,
> png-spec.amc @ nicemice.net
> Lee Daniel Crocker,
> lee @ piclab.com
> Andreas Dilger,
> adilger @ enel.ucalgary.ca
> Oliver Fromme,
> oliver @ fromme.com
> Jean-loup Gailly,
> jloup @ gzip.org
> Chris Herborth,
> chrish @ qnx.com
> Aleks Jakulin,
> jakulin @ acm.org
> Neal Kettler,
> neal @ westwood.com
> Tom Lane,
> tgl @ sss.pgh.pa.us
> Alexander Lehmann,
> alex @ hal.rhein-main.de
> Chris Lilley,
> chris @ w3.org
> Dave Martindale,
> davem @ cs.ubc.ca
> Owen Mortensen,
> ojm @ acm.org
> Keith S. Pickens,
> ksp @ swri.edu
> Robert P. Poole,
> tarquin @ alum.mit.edu
> Glenn Randers-Pehrson,
> randeg @ alum.rpi.edu
> Greg Roelofs,
> newt @ pobox.com
> Willem van Schaik,
> willem @ schaik.com
> Guy Schalnat,
> gschal @ infinet.com
> Paul Schmidt,
> pschmidt @ photodex.com
> Michael Stokes,
> Michael_Stokes @ hp.com
> Tim Wegner,
> twegner @ phoenix.net
> Jeremy Wohl,
> jeremyw @ evantide.com
>
> The authors wish to acknowledge the contributions of the Portable
> Network Graphics mailing list, the readers of comp.graphics, and the
> members of the World Wide Web Consortium (W3C).
>
> The Adam7 interlacing scheme is not patented and it is not the
> intention of the originator of that scheme to patent it.  The scheme
> may be freely used by all PNG implementations.  The name "Adam7"
> may be freely used to describe interlace method 1 of the PNG specification.
>
> ### Trademarks
>
> GIF is a service mark of CompuServe Incorporated.  IBM PC is a
> trademark of International Business Machines Corporation.  Macintosh is
> a trademark of Apple Computer, Inc.  Microsoft, Windows, and MS-DOS are
> trademarks of Microsoft Corporation.  PhotoCD is a trademark of Eastman
> Kodak Company.  PostScript and TIFF are trademarks of Adobe Systems
> Incorporated.  SGI is a trademark of Silicon Graphics, Inc.  X Window
> System is a trademark of the Massachusetts Institute of Technology.
>
> ### Document source
>
> This document was built from the file png-master-19990714
> on 14 July 1999.
>
> ### Copyright notice
>
> Copyright © 1998, 1999 by:  Glenn Randers-Pehrson
>
> This specification is a modification of the PNG 1.0 specification.
> It is being provided by the copyright holder under the provisions of
> the 1996 MIT copyright and license:
>
> Copyright © 1996 by:  Massachusetts Institute of
> Technology (MIT)
>
> This W3C specification is being provided by the copyright holders
> under the following license.  By obtaining, using and/or copying this
> specification, you agree that you have read, understood, and will comply
> with the following terms and conditions:
>
> Permission to use, copy, and distribute this specification for any
> purpose and without fee or royalty is hereby granted, provided that the
> full text of this NOTICE appears on ALL copies
> of the specification or portions thereof, including modifications, that
> you make.
>
> THIS SPECIFICATION IS PROVIDED "AS IS," AND
> COPYRIGHT HOLDERS
> MAKE NO REPRESENTATIONS OR WARRANTIES, EXPRESS OR IMPLIED.  BY WAY OF
> EXAMPLE, BUT NOT LIMITATION, COPYRIGHT HOLDERS MAKE NO REPRESENTATIONS
> OR WARRANTIES OF MERCHANTABILITY OR FITNESS FOR ANY PARTICULAR PURPOSE
> OR THAT THE USE OF THE SPECIFICATION WILL NOT INFRINGE ANY THIRD PARTY
> PATENTS, COPYRIGHTS, TRADEMARKS OR OTHER RIGHTS.  COPYRIGHT HOLDERS WILL
> BEAR NO LIABILITY FOR ANY USE OF THIS SPECIFICATION.
>
> The name and trademarks of copyright holders may NOT be
> used in advertising or publicity pertaining to the specification
> without specific, written prior permission.  Title to copyright in this
> specification and any associated documentation will at all times remain
> with copyright holders.
>
> End of PNG Specification

Consequences (observations only): Domowik consults the specification
read-only and reproduces none of its text; were any portion ever
reproduced, the full NOTICE text above must accompany it per the license
grant. The Adam7 statement is the artifact's own declaration and is not
relied upon as clearance. The artifact's trademark paragraph lists the
designations GIF, IBM PC, Macintosh, Microsoft, Windows, MS-DOS, PhotoCD,
PostScript, TIFF, SGI, and X Window System together with the company
names it associates with them; this record makes no ownership, clearance,
availability, affiliation, non-affiliation, or non-infringement claim
regarding any designation or party. No such claim is made anywhere in
these records.

**FIPS 180-4** — title "Secure Hash Standard (SHS)", FIPS 180-4; author
National Institute of Standards and Technology (U.S. Department of
Commerce); August 2015;
`https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf`; consulted
artifact: the published PDF. License/notice obligations: the artifact's
own terms were not transcribed here; no FIPS text is reproduced in Domowik
materials — consult the published document for its terms.
Implementation status: **no Domowik SHA-256 implementation exists yet**;
the pinned literals in §10.9 were independently recomputed using the
external verification tools already recorded there (Python hashlib,
coreutils sha256sum, OpenSSL 3.6.3). Any future implementation must use an
approved dependency or an independent implementation, with provenance
recorded at that time; no implementation or adaptation is claimed here.
Test-vector provenance: the corpus seed/RNG literals derive from SHA-256
of the ASCII input strings quoted in §10.9; correctness vectors cite the
standard's own example messages and hashes. Publication status: U.S.
Government publication; recorded as observed, no legal conclusion.

**Corpus constructions** (ring geometry, hash-based RNG, seeded blob
expansion): original Domowik work; independently implemented; no external
artifact consulted.
