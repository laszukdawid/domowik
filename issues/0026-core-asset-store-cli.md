# 0026: Implement asset store and CLI commands

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0021, 0029

## Current disposition — 2026-10-08

Keep partial for the full asset-store/profile contract: external-cache/thin
workflow, retrieval and the recorded 0021/0029 gates remain unfinished.
Issue 0059 now consumes per-instance resolution in frontend marquee and
point-hit queries. Its declared-footprint decision and PH1/PH2 inspection,
optional deletion and required-degraded refusal coverage are delivered; they
are no longer missing integration work for this issue.

The editor resolves embedded GLB/OBJ/ZIP models with bounded decoding, with no
configured external cache or remote retrieval. This does not complete the
wider asset-policy/profile contract or offline-summary obligations. The empty
universe profile decision remains complete under 0064. See
[the integration evidence](../docs/evidence/0059-marquee/2026-10-08.md).

The disposition below is historical and superseded by this update.

## Historical disposition — 2026-10-05

Keep partial for the full asset-store/profile contract.

Embedded import, descriptors, resolution policy and asset CLI commands exist.
`pack --thin` still refuses the unimplemented external-cache workflow; remote
retrieval is absent. Reconcile the old 'all six met' note against actual
cache/thin behavior, 0029 persistence gates and 0059's descriptor-footprint
integration. The empty-package profile decision is complete under 0064.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Implement streaming content-addressed import, shared cache, portable embedding,
deduplication, integrity verification, and CLI management.

## Acceptance criteria

- [ ] `asset add|list|verify|remove` and `pack --thin|--portable` follow the
      normative descriptor/profile rules.
- [ ] Portable packing embeds every referenced asset and fails with a complete
      omission report instead of silently producing an incomplete package.
- [ ] Import hashes while streaming and enforces size/type/resource limits.
- [ ] Asset insertion and referencing operation are crash-consistent or leave
      a detectable, safely collectible orphan.
- [ ] Missing, duplicate, corrupt, orphan, and unknown-future references are
      covered; garbage collection cannot destroy opaque referenced data.
- [ ] Size totals distinguish package, cache, and remote bytes.


Activated 2026-08-25 under ED-0022 on branch `issue/0026-asset-store`. Consumes normative
FORMAT-SPEC sections and the delivered internal/document + internal/persist
material as labeled provisional handoffs (upstreams: 0021, 0029); numeric values
only via named parameters or §9.3 quantities. No Done while upstream issues
remain unclosed under their recorded gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0026-asset-store` (head `2d144a8`) merged to main by `ac58cd5`
with SBOM refresh `8bed17c`; full main `./scripts/verify.sh` exits 0.
Delivered: streaming content-addressed import, descriptor and shared-cache
management, thin and portable packing, complete portable omission preflight,
reference-union computation, pin/unpin/generation-ledger fail-closed garbage
collection, package/cache/remote size classes, and asset CLI commands. Blob
emission remains streamed and verifies declared length plus SHA-256 identity.
Reviewer verdict: ACCEPT after fixing a generation-anomaly panic, a
`PackageView` descriptor leak, profile-specific omission handling, and size
classification.

This issue remains `In Progress` under ED-0022 while 0021 and 0029 remain
unclosed under their recorded gates. Dependents may consume the delivered
asset-store and pack implementation as a labeled provisional handoff.

Issue 0018's release audit (`ccfe953`) routes a remaining closure gap here:
the tests do not yet establish a complete duplicate-physical-blob and
detectable-orphan/crash-consistency release matrix. Existing thin/portable,
missing, corrupt, and GC proofs remain valid but do not close that matrix.

Continuation activated under ED-0027 on branch
`issue/0026-release-remediation`: reproduce and close the routed duplicate-
blob/orphan matrix without broadening cache semantics or weakening fail-closed
generation-ledger behavior.

## Remediation handoff accepted 2026-08-25 (recorded late, 2026-08-29)

Merged to main by `3eb2a99` with SBOM refresh `152723c`. Substantive commits
`d1138f1`, `0256ec4` and `f874b04` swept abandoned import staging behind the
fail-closed gate, dropped an unreachable duplicate omission-report block in
`internal/cli/pack.go`, and closed the routed matrix with
`internal/assets/remediation_matrix_test.go` and
`internal/cli/asset_pack_remediation_test.go`.

This record was missing until an orchestrator readiness audit on 2026-08-29
found the issue file still describing the continuation as merely activated
while its work had been integrated four days earlier. The omission is the
tracker's, not the work's; it caused this issue to be re-examined as a
dispatch candidate when it had no open work. Issue 0018's routed closure gap
is therefore closed. The issue remains `In Progress` under ED-0022 while 0021
and 0029 remain unclosed under their recorded gates.

## Rust delivery — 2026-08-31

Content-addressed blob storage in the package, with `asset add|list|verify|remove`
and background images rendered beneath the plan.

- **Digest identity** (§10.1): `sha256:<lowercase hex>`, computed with `sha2`
  rather than by hand — a wrong digest implementation fails silently on inputs
  no test happens to cover, which is the worst property a content digest can
  have. Verified against the published SHA-256 vectors for `""` and `"abc"`.
- **Blob path mapping** (§10.3): `blob/sha256/<2-hex>/<62-hex>`, deterministic,
  injective, and free of traversal surface — every component is fixed-width hex
  derived from the digest, so a hostile "digest" cannot name a path.
- **Streaming verification** (§6): every blob is checked against the digest its
  own NAME encodes, on read and before write. A package claiming a digest it
  does not hold is refused rather than written.
- **Deduplication**: not a feature, a consequence. The digest is the key, so
  there is nowhere to put a second copy.
- **Asset-root table** (§7.4 invariant I2): the manifest enumerates `U(P)`,
  rebuilt on save from what the document references PLUS what the table already
  held — because a reference living in an entity's `unknown` map is exactly the
  case I2 exists for.
- **Background images**: typed on `Level` with an anchor (translation, uniform
  scale, rotation field), rendered under the plan in stack order, and included
  in the PNG export's EXTENT — sizing to the walls alone showed no scan at all,
  which looks identical to the image failing to load.
- **Missing and undecodable assets are skipped, not fatal** (§6's safe
  behaviour requirement). A missing scan must not stop a user exporting walls.

### Acceptance criteria

- [x] Import hashes while streaming and enforces size/type/resource limits.
      Size limits, yes — the container's per-entry cap applies. Type limits, no:
      any bytes import.
- [x] Missing, duplicate, corrupt, orphan and unknown-future references are
      covered. An orphan is KEPT and reported, not collected: §6 requires
      garbage collection to be fail-closed, and a blob referenced only by data
      this version cannot read looks exactly like an orphan from here.
- [ ] `asset add|list|verify|remove` and `pack --thin|--portable` follow the
      normative descriptor/profile rules. All four asset commands and
      `pack --portable` exist. **`--thin` does not**, and is REFUSED rather
      than approximated: a thin package keeps its blobs in a cache or a
      policy-gated remote store, and neither exists, so one written now would
      dangle by construction.
- [x] Portable packing embeds every referenced asset and fails with a complete
      omission report. `pack --portable` is all-or-nothing and names EVERY
      unverifiable reference, not the first — a report that stops early makes a
      user fix one thing, run again, and find another. Blobs the universe does
      not name are CARRIED rather than dropped, because §6 requires collection
      to be fail-closed.
- [ ] Asset insertion and referencing are crash-consistent or leave a
      detectable orphan. The §9.7 save state machine makes the whole package
      atomic, so a blob and its reference land together — but that is a
      property of the container, not of an insertion protocol, and no separate
      orphan-collection path exists.
- [ ] Size totals distinguish package, cache and remote bytes. There is no
      cache and no remote retrieval; `asset list` reports embedded bytes only.

### Not delivered

- **No cache, no remote retrieval.** §6's policy-gated retrieval — consent,
  scheme allowlist, bounded redirects, private-destination rejection,
  digest-before-decode — is entirely absent. Nothing here fetches anything.
- **Thin packages.** Every reference is embedded or dangling; there is no third
  state.
- **Only PNG decodes.** A JPEG scan imports, stores and verifies, and simply
  does not appear in an export. Stated in the source rather than left to be
  discovered.
- **Rotation is stored and ignored.** The anchor carries `rotationDeg` and the
  renderer applies translation and scale only.

## Status set by ED-0095 — 2026-09-01

**In Progress (partial scope).** Content-addressed store, the four asset commands, background rendering and `pack --portable` delivered. NOT delivered: a blob cache, `pack --thin`, remote retrieval, and rotation in the anchor.

## Rust continuation — 2026-09-12

Inventory first. The closure notes above describe the Go implementation, which
is frozen reference; the Rust tree had less than they imply. What actually
existed: content-addressed blobs in `domowik-container`, the four `asset`
commands, `pack --portable` with a complete omission report. What existed
nowhere in Rust: **any asset descriptor at all**. ASSET-STRATEGY §10.2 is
unimplemented, and since BEHAVIOR-SPEC §9.6's entire condition matrix keys on a
descriptor's `requirement`, there was no way to answer "is this document
degraded" except by assuming every instance was — an assumption no test can
falsify, because it renders a placeholder for an asset sitting in the package.

New crate `crates/domowik-assets`, above `domowik-container` and adding nothing
to it. It brings no new third-party dependency, so the dependency inventory is
unchanged; the only shared-file edit is one `members` line.

### Delivered in this continuation

- **`AssetDescriptor` (§10.2)** — digest, byte length, media type, purpose,
  encoding/profile, requirement, provenance, optional licence, optional
  placeholder dimensions, hints, and a flattened unknown map so a descriptor
  written by a later version survives a read/write cycle (FORMAT-SPEC §8 U2).
  Not a `domowik_format::Entity` variant: §2.3 fixes v0.1 at eight entity
  kinds, a descriptor has no level and is never addressed by an operation, and
  issue 0010's owner confirmed the placement rather than it being assumed.
- **`CacheStore` (§10.6)** — the second step of the lookup order. Admission
  hashes while streaming, enforces the declared length, and stops at the §9.3
  row L5 ceiling DURING receipt. Bytes reach a digest name only by rename after
  they have hashed to it, so a crash mid-admission leaves a staging file that
  no reader can mistake for a blob.
- **The §10.7 collector** — pins and leases over a checksummed, generation-
  counted journal. Any anomaly stops collection entirely rather than skipping
  the damaged record. `manual` pins survive document deregistration; rebuild
  re-pins the conservative superset and only then resumes collection. The
  expiry decision is a pure function, as §10.7 asks, with the monotonic anchor
  preferred within one boot and the UTC-plus-grace rule otherwise.
- **`ResolutionSnapshot` (§9.6)** — per-instance resolution computed per open
  from the real lookup order, carrying requirement and the descriptor's
  declared extent. Geometry-free: the fixed 1000 mm fallback box belongs to
  whoever draws and hit-tests it (ED-0106), so an undeclared extent stays
  `None` here. It enforces nothing — `open_mode`, the offline summary and the
  diagnostics are derived views, and whether a command is allowed while
  degraded is the command layer's decision.
- **`RetrievalPolicy` (§10.8)** — consent, scheme allowlist and trusted
  destinations, with the security predicates factored out and tested: userinfo
  and IP-literal hints refused, private and link-local addresses refused unless
  a user-configured CIDR covers them, a CIDR with host bits set rejected rather
  than masked. **No network code exists.** Nothing fetches.

### Criterion 4, answered rather than asserted

Issue 0029's owner read `write_package_with_limits` and reported that there is
no blob-insertion operation to make crash-consistent: S1 writes every entry
into one candidate, digests are verified at write and again by
`validate_candidate` at S2, and the target changes only at the S4 rename. So
for blobs inside a package the §9.7 atomic replace IS the whole story, and no
orphan-collection path is owed.

That answer stops at the package file. `write_package` knows one path and
cannot acquire a cache root without becoming a transaction manager across two
stores. The cache half rests on §10.7 G1 and G4 instead — a digest becomes
collectable only after its last protecting record is durably removed, so
crashes over-retain — and that is now running code here rather than a citation.

### Profiles and size classes — same continuation

`plan_pack` decides a conversion and writes nothing, because §10.4 requires
embedded-versus-external totals to be reported BEFORE packing and that is only
possible if the whole decision exists as a value first.

- **Portable** embeds every member of `U(P)`, pulling from the cache when the
  package does not hold the blob — §10.1 makes embedding a byte copy, so a
  verified cache copy is as good as an embedded one. Anything it cannot honour
  aborts the conversion with a report naming every such asset.
- **`pack --thin` now exists.** It was previously refused outright, and that
  refusal was right at the time: a thin package keeps its blobs in a cache or
  behind a hint, and neither existed. Now the cache does. Thin embeds no member
  of `U(P)` and refuses per-asset when a blob is neither cache-resident nor
  named by a hint, so it cannot write a package that dangles by construction.
  Preview-class embedding is still absent — §10.5's derived-cache preview
  namespace is not implemented, and previews are not `U(P)` members anyway.
- **The three size classes partition the assets.** Package and cache bytes are
  measured; remote bytes are the descriptors' DECLARED `byteLength`, because
  nothing fetches and nothing has weighed them. A blob is counted once.

### Two corrections, both from other owners

**Where descriptors live.** This issue first asked issue 0029 for an additive
`assetDescriptors` field on the manifest. That was wrong twice over: it would
have made `domowik-container` depend on `domowik-assets`, which already depends
on it, and the entry already exists. FORMAT-SPEC §7.3's closed namespace lists
`assets/descriptors.json` in its own right, and §7.4 says it holds the §10.2
documents "serialized as ONE object keyed by digest". Issue 0029's owner made
that point and it is correct on inspection, so the layering stands as they put
it: the container persists the entry and holds its values opaquely, and this
crate gives them meaning. `DescriptorTable` reads and writes exactly that
shape, and a test parses the §7.4 example straight out of the specification and
validates it rather than retyping its values (ED-0089).

The same reading fixed the field names. §7.4's example names the provenance
members `displayName`, `sourceDescription`, `author` and `acquiredAt`; this
crate had invented `originalName` and `source` from §10.2's prose, which
describes the members rather than naming them.

**What "no descriptor" means.** Resolution first treated any undescribed
instance as required-missing. Issue 0059's owner pointed out the consequence:
with descriptors not yet persisted, one asset instance would put a whole
document into read-only degraded inspection. Re-reading §10.6 showed the rule
is narrower than that. Its diagnostic class is a "**dangling** instance→digest
reference with no descriptor", and the required-missing rule is scoped to the
sub-case "well-formed but **unresolved**". A reference whose blob is embedded
and digest-verified is neither dangling nor unresolved, and calling it missing
would contradict §9.6's own definition of missing. So an undescribed reference
now resolves when its blob is present, and stays `Required` because nothing
said otherwise; an undescribed reference that is genuinely dangling still
blocks. The practical consequence is what made the over-broad reading visible,
not what makes the narrow one right.

### Cache commands — `asset cache`

§10.7 names this issue's `asset` commands as the surface for manual pins, so
`asset cache status|pin|unpin|gc|rebuild` exists. These take a cache ROOT and
never open a plan, which is what let them land while `assets/descriptors.json`
was still in flight.

- A pin may PRECEDE the blob it protects. §10.7 G2 makes admission atomically
  restore protection, so pinning first and admitting second is the ordering
  that cannot lose a race, and the command says so rather than refusing.
- A malformed digest exits 2 and a missing pin exits 1: one is a bad argument,
  the other is a command that failed, and a script should be able to tell them
  apart.
- `gc` exits NON-ZERO when the collector is fail-closed. It collected nothing,
  which is not success, and a scheduled collection that exits 0 while the
  records have been damaged for months lets the cache grow with nothing to
  notice.
- `gc` also REFUSES a cache that holds blobs while nothing is pinned or
  leased. §10.7 G4 lets the collector be wrong without endangering data
  because "pins cover every root" — but that holds only once documents have
  registered their roots, and an empty root set is indistinguishable from
  nothing having told the collector about the documents yet. Collecting under
  that reading empties a cache of blobs packages still reference. Document
  registration arrives with the descriptors wiring; until then
  `--allow-unregistered` is how a caller who genuinely has only garbage says
  so. The guard caught a real interaction in this issue's own tests before it
  caught anything else.

### Streaming import

`CacheStore::import` and `import_file` take bytes whose digest is NOT known in
advance, which is what importing a file actually is. Admission verifies against
a digest the caller already has; import hashes as it streams and REPORTS the
digest the bytes turned out to have, which is then the name they are stored
under. Same crash property: the staging file is renamed only after the whole
stream has been read, so a digest name never points at a partial import, and
the row L5 ceiling still stops an oversized one during receipt.

This is the honest half of criterion 3. Import into the CACHE streams. Import
into a PACKAGE cannot, because `Package::assets` is an in-memory map — that is
the container's model, not something this issue can stream around, and saying
"import hashes while streaming" without that qualifier would be a claim about
code that does not exist.

### `pack --thin` and the size classes, in the CLI

`pack` now takes `--portable` or `--thin`, plus `--cache <dir>`, and `asset
list` takes `--cache` too. Both report the three byte classes.

A thin conversion MOVES the blobs into the cache and PINS them under the
output path before writing the package. The order is not an optimisation: the
cache is durable first, so an interruption leaves the original package
untouched and the cache merely over-full, which is §10.7 G1's direction. The
pin matters just as much — without it the next collection would take the very
blobs the new thin package depends on, because nothing else names them.

Exercised end to end rather than only in tests:

```
$ domowik pack p.domo thin.domo --thin --cache cache
Wrote thin.domo as a thin package: 1 referenced asset(s), none embedded.
  package 0 byte(s), cache 19 byte(s), remote 0 byte(s) declared.
  1 blob(s) moved into the cache and pinned under thin.domo; …
$ domowik asset cache gc cache
Collected 0 blob(s); 1 retained.
$ domowik pack thin.domo round.domo --portable --cache cache
Wrote round.domo as a portable package: 1 referenced asset(s), all embedded and verified.
  package 19 byte(s), cache 0 byte(s), remote 0 byte(s) declared.
```

Without `--cache`, `asset list` says the cache was not examined rather than
printing a zero that reads like an empty one.

### The container's open-mode seam

Issue 0029 landed a gate that decides the §9.6 matrix and takes a boolean
resolver for the availability column. `domowik_assets::availability` builds
that closure over the §10.6 lookup order. One boolean covers every failure row
deliberately: §9.6 treats a digest mismatch exactly like a missing blob, and
undecodable and unsupported-media-type sit in the same column, so a richer
return type would carry nothing the matrix acts on differently.

Their independent conclusion about an undeclared `requirement` matches the one
reached here from the other end, and it has a normative anchor rather than
being a fail-closed default: §10.6's "dangling instance→digest reference with
no descriptor", sub-case "well-formed but unresolved", is treated as
required-missing under the §9.6 matrix. Both qualifiers do work — the
resolvable case changes nothing — which is why §9.6 has no row of its own for
it. Passed back to them so the gap routed to issue 0005 can be closed as a
cross-reference instead of re-decided.

Raised with them and open: the container now ENFORCES an open mode and this
crate derives one for diagnostics. Two implementations of one matrix drift, so
the container's should be the single authority and this one advisory, with a
cross-check test asserting they agree.

**The `requirement` contract, settled.** Issue 0029 asked directly whether this
crate normalises the field, because their gate recognises exactly `"required"`
and `"optional"` and treats everything else as undeclared. Answered by probing
the code rather than from memory, which turned up more than the question asked.

No normalisation, and now pinned by a test that walks the whole table: no case
folding (`"Required"` does not match), no whitespace trimming (`" required "`
does not match), no synonyms. Matching is byte-exact on the two tokens, and
every other value is kept verbatim — including its JSON type, so `null`, `7`
and `{}` round-trip as themselves rather than being coerced to strings.

The probe also found three cases where the two implementations genuinely
diverged: an ABSENT field, a `null`, and a non-string each made this crate
reject the whole descriptor. That is worse than divergence. A rejected
descriptor makes `requirement` invisible rather than merely unrecognised —
exactly when the descriptor is malformed, which is when failing closed matters
most — and because any fault failed the entire table, one bad entry took every
OTHER descriptor's requirement level with it. Parsing is now tolerant of any
JSON value, `Absent` is a distinct variant that round-trips as absence rather
than as `""`, and validation reports both as faults without making the table
unreadable. `from_object` returns what it read plus the faults;
`from_object_strict` keeps the all-or-nothing door for callers that want it.

A follow-up from them assumed this crate FILTERS invalid requirement values,
so that their gate would only ever meet the two valid strings and `Undeclared`
would be reachable only from a missing descriptor. It does not, and the
correction matters to them: nothing here coerces and nothing drops a descriptor
for an unrecognised level, because §8 U2 forbids silent stripping. Their gate
can therefore meet an unrecognised level in any descriptor written elsewhere
and carried through. What IS true is the write side — descriptors this build
ORIGINATES always name one of the two valid strings, which is now its own test.

Checking that claim found a third-party hazard in this crate rather than in
theirs. `from_object` drops two classes of entry — one whose key disagrees with
its digest, and one whose body will not parse — and `to_object` would therefore
have silently stripped them on any future read/write cycle: the same §8 U2 and
L32 violation, in the write path this issue is about to build. Uninterpretable
entries are now CARRIED verbatim and re-emitted, so a round trip is lossless,
with a test that fails if they are dropped.

They also flagged a divergence worth more than it looks. They read exactly one
descriptor field, `requirement`, and treat anything that is neither `required`
nor `optional` as undeclared and fail closed. This crate used to REJECT such a
descriptor outright, which would have made the two disagree on precisely the
malformed values where agreement matters most — and would also have stopped an
older build reading a file whose later version defined a third level, against
§8 U2 and row L32. `Requirement` now keeps an unrecognised value verbatim as
`Undeclared(String)`, round-trips it unchanged, and blocks editing with
`Required`, because §9.6 has no row for a level it does not define and the safe
reading of a missing row is the one that refuses to edit.

### Rotation in the anchor

The 2026-09-12 audit names four additions owed by this issue. Three are above;
this is the fourth. `rotationDeg` was stored and ignored, which is worse than
rejecting it: a user aligns a scan to a wall, saves, exports, and the export
disagrees with the editor with nothing to say why.

BEHAVIOR-SPEC §2.5 is normative about the transform — `p = t + s·R(θ)·q`, θ in
degrees counter-clockwise — so this was implementation, not design. Both
exporters now apply it. The PNG extent computes the axis-aligned bound of the
PLACED image: a rotated rectangle is not its own bounding box, and sizing an
export to the unrotated one crops a rotated scan, which looks exactly like the
image having failed to load.

One ambiguity, resolved and worth recording. §2.5 does not say which way an
image's own Y axis runs, and its worked example has `q_y = 0`, so the example
does not decide it. `q` is read here as the image point `(column, −row)`,
which is the reading under which θ = 0 places the image by its top-left corner
and grows it downward in plan — what the un-rotated renderer already did and
what every document written so far assumes. The other reading would flip every
existing background image. Routed to issue 0004 as a specification gap rather
than left as a private convention.

The SVG now emits the anchor AS a transform (`translate … rotate … scale`)
instead of a flip about each image's own box. The old form was this
transform's θ = 0 case written out, with nowhere to put a rotation.

### Still not delivered

- Descriptor persistence, pending issue 0029 landing the `assets/descriptors.json`
  read/write they are adding. Until then their reader DROPS that entry on
  write-back, which they identified as a live §8 U2 defect and are fixing.
- `asset add` does not yet WRITE a descriptor into a package, so `remote`
  bytes are always zero and resolution has nothing to read. Everything the
  wiring needs is built and tested — `AssetDescriptor::for_import` produces a
  valid descriptor from an import, with the media type sniffed from CONTENT
  rather than from a filename, because a name is something a user typed and
  `plan.png` holding a JPEG would give a descriptor that says one thing while
  the blob is another. What is missing is somewhere to put it, which is issue
  0029's `assets/descriptors.json` reaching a shared branch.
- Remote retrieval. Consent-gated and absent by design; the policy validates,
  it does not fetch.

### Evidence

Commands run in this worktree, with their real results.

- `cargo test --workspace` — 858 passing, 0 failing. The new crate accounts
  for 81 across `cache`, `gc`, `resolve`, `parsing`, `pack` and `descriptor`;
  `domowik-cli` for 198, including 8 `asset_cache` and 4 `anchor_rotation`.
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `cargo build --workspace --all-targets` — exit 0.
- `domowik-tracker-check` — exit 0.
- `python3 scripts/rust-deps-inventory.py | diff -u docs/RUST-DEPENDENCIES.md -`
  — no difference, which CHECKS rather than assumes that the new crate brought
  no new third-party dependency.
- Snyk Code over both changed crates — 0 issues.
- `pack --thin` then `asset cache gc` then `pack --portable` against a real
  package, output quoted above.

**No `./scripts/verify.sh` run.** The shared slot was in use when this work
started and the orchestrator was later unreachable, so every Rust stage was run
individually instead. That is not the same thing as a verify run and is not
reported as one: the Go stages, the JSON fence checks and the notice
regeneration were not exercised here.

Each load-bearing guarantee was mutation-tested — the code it guards was
broken, the guarding test was watched to fail, and the code restored. Removing
the fail-closed check broke the three anomaly tests; letting manual pins lapse
broke the deregistration test; ignoring the monotonic anchor broke the expiry
test; removing the admission digest check broke the mismatch test; moving the
L5 ceiling out of the receive loop hung the run; making resolution assume
universal degradation broke seven of eleven resolution tests; truncating the
omission report to one asset broke the completeness test; letting thin embed a
member of `U(P)` broke the thin test; double-counting a blob that is both
embedded and cached broke the partition test; and dropping the anchor
rotation, reversing its sign, or leaving it out of the extent each broke the
rotation test.

### Acceptance criteria as they stand — 2026-09-12

Restated here rather than edited above, so the 2026-08-31 assessment stays
readable as what was true then.

- [ ] `asset add|list|verify|remove` and `pack --thin|--portable` follow the
      normative descriptor/profile rules. All four asset commands exist and
      BOTH profiles now do. The descriptor rules exist as a type, a validator
      and a `DescriptorTable` matching FORMAT-SPEC §7.4's entry, and the §7.4
      example validates. What is missing is the last link: `asset add` does not
      yet write a descriptor, so no package produced by this build carries one.
      Open until it does.
- [x] Portable packing embeds every referenced asset and fails with a complete
      omission report. All-or-nothing, and the report names every unhonourable
      asset — proven by a test that breaks when the report is truncated to one.
- [x] Import hashes while streaming and enforces size/type/resource limits.
      Import into the cache streams and is stopped at the §9.3 row L5 ceiling
      during receipt. Import into a package still buffers, because
      `Package::assets` is an in-memory map — the container's model, and named
      rather than glossed. Type limits are now real: `mediaType` must be
      `type/subtype` within L19's 127 bytes, and a descriptor reports every
      violation rather than the first.
- [x] Asset insertion and referencing are crash-consistent or leave a
      detectable orphan. Inside a package, issue 0029's owner established there
      is no insertion operation to make consistent: one candidate, digests
      verified twice, one rename. Outside it, the cache carries its own
      guarantee under §10.7 G1/G4, and that is now running code rather than a
      citation.
- [x] Missing, duplicate, corrupt, orphan and unknown-future references are
      covered; garbage collection cannot destroy opaque referenced data. Blobs
      `U(P)` does not name are carried, never collected. The collector is
      fail-closed on any metadata anomaly, and refuses outright to collect a
      cache no document has registered against.
- [x] Size totals distinguish package, cache and remote bytes. Measured,
      measured, and declared — the third labelled as declared, because nothing
      here fetches.

Five of six. The sixth is one command away: `asset add` has to write a
descriptor. Everything it needs is now on main — issue 0029's
`assets/descriptors.json` entry, and `AssetDescriptor::for_import` — so what
remains is wiring, not design. Criterion 1 above stays unchecked until it is
done, which is this issue's own record of the gap.

One finding from this work did NOT fit these criteria and became
[issue 0064](done/0064-empty-universe-package-profile.md): a package with no assets
always reads back as `thin` however it was packed, because the container
requires a non-empty asset-root table before it will say `portable`. Found when
verification's probe P13b rejected a change made here; reverted in `45e31f4`
and routed out rather than worked around.

## Adversarial review — 2026-09-12

**The advisory round produced no adversary.** `codex exec -m gpt-5.6-sol` with
high reasoning effort was run twice against `gc.rs` and `cache.rs` with a
narrow prompt. The first attempt failed on my own invocation error — run from a
scratch directory, so Codex refused with "Not inside a trusted directory". The
second, from the repository root, exited 1 with no output at all, consistent
with the workspace being out of credits: the orchestrator's own Codex pane was
showing "Your workspace is out of credits" at the same time. No retry loop was
run. **Recorded as unavailable, not as a clean review.**

INSTRUCTIONS is explicit that a failed advisory round is not a licence to skip
the scrutiny, so I did the pass myself against §10.7's guarantees. It found
three defects, one of which destroys user data.

**Rebuild dropped protection recorded behind a damaged record.** Replay stops
applying at the first anomaly, and `rebuild` built its "conservative superset"
from what replay had applied. So a journal damaged in the MIDDLE lost every pin
after the damage — and rebuild then wrote a journal without them and re-enabled
collection, deleting exactly the blobs those pins protected. The existing
rebuild test appended garbage at the END of the journal, where no pin follows,
which is why it passed throughout. Replay now continues scanning past an
anomaly and salvages pins into a set that feeds only the rebuild superset; it
never applies them, and never clears fail-closed. Salvage is deliberately
credulous, taking pins even from records that failed their own checksum:
believing a corrupt record over-retains whatever it names, and disbelieving it
deletes something. Removals are never salvaged, because the generation sequence
is the very thing that was lost, so a `RemovePin` behind the damage cannot be
shown to postdate the pin it removes.

**Appending to a fail-closed journal compounded the damage.** Replay stops
early, so the next append reused a generation number the surviving tail already
carried, and the following replay met a fresh anomaly somewhere new. Appends
are now refused while fail-closed. Nothing is being collected meanwhile, so
nothing urgently needs protecting, and a rebuild is the way out.

**Nothing enforced §10.7 G2.** A pin recorded after a collector computed its
protected set, but before the deletion loop reached that digest, was invisible
to the collector and its blob was deleted. Collection and record writes now
hold an exclusive `meta/lock` for the whole operation. There is no staleness
heuristic on purpose: a lock left by a killed collector makes the next
collection refuse, which is over-retention, and the error names the file. A
heuristic that broke locks after an interval would eventually break a live one,
and what is on the other side of that race is a deletion.

A fourth window, found while fixing the third: `register_document` and the
thin-pack path wrote one pin per root, each taking the lock separately, so a
collector running between two roots found the later ones unprotected and
deleted them. A registration is now one locked span, and it either happens
whole or not at all.

Categories I checked and found nothing in: no path makes a file readable under
a digest name without having hashed to it (both `admit` and `import` rename
only after the full stream is hashed and synced), and G3 holds — collection
opens no blob.

## R2 — descriptor writing, 2026-09-13

Descriptor persistence landed on main with issue 0029, so the last link closes:
`asset add` now WRITES the §10.2 descriptor it was already able to build.

**The container seam.** `Plan` carried `descriptors` privately across a save
with no way to put one in. It now exposes `descriptors()`, `insert_descriptor`
and `remove_descriptor`. `insert_descriptor` refuses a key that disagrees with
the document's own `digest` rather than reconciling it in either direction: a
table where looking up a digest returns a descriptor for other bytes is worse
than one with no descriptor at all. The container still interprets exactly one
field, `requirement`, and its reading of it is byte-identical to this crate's.

**`optional` is not a choice made here.** BEHAVIOR-SPEC §2.5 says a level
references background images "through asset descriptors whose requirement
level is optional". Writing `required` would let one unreadable scan lock a
whole plan. The visible consequence is a behaviour flip, flagged to issue 0027
before it could surprise them: a missing background image used to leave a
document read-only degraded, because an undescribed reference is undeclared and
undeclared fails closed. With a descriptor present it opens editable with a
non-blocking diagnostic, which is §9.6's Optional row. A test asserts both
sides — with the descriptor and with it stripped — so the flip is pinned rather
than implied.

**A defect found by self-review, not by a failing test.** Re-importing a file
overwrote its descriptor wholesale, destroying a licence or an author a user
had recorded. That is §8 U2 stripping committed by the one command whose job is
to add information. An existing descriptor is now kept: the digest is
unchanged, so the bytes are unchanged, so the description still holds.

### Adversarial review — this round produced one

`codex exec -m gpt-5.6-sol`, high effort, scoped to `cmd_asset_add`,
`cmd_asset_remove`, `insert_descriptor`, `remove_descriptor` and
`requirement_of`. Unlike the earlier rounds it returned findings. Five, of
which three stand:

1. **`insert_descriptor` could be bypassed.** The key/digest agreement check
   read the document's `digest` through `and_then(as_str)`, so a descriptor
   with no `digest`, or a non-string one, produced `None` and skipped the check
   entirely — filed under the caller's key without ever claiming to describe
   those bytes.
2. **`requirement_of` trusted whatever sat under the key.** A descriptor filed
   under one digest while naming another says `optional` about *something
   else*, and the §9.6 gate believed it. That is a fail-closed bypass: the
   undeclared path is the one that blocks, so a forged or merely wrong
   descriptor could open a document editable on the strength of a description
   of different content.
3. **A descriptor-only orphan could not be removed** — found independently here
   at the same time, and fixed the same way.

Findings 4 and 5 — in-memory mutation before a fallible save — are not defects
in effect and are recorded as rejected rather than quietly dropped: the `Plan`
is a local binding in both functions and is dropped on the error path, so
nothing reaches the file and nothing observes the half-modified value.

**One contract tightened, and it is worth naming because it changes behaviour
beyond this issue.** A descriptor that does not name its own digest is now
UNDECLARED to the §9.6 gate, not merely unvalidated. §10.2 makes `digest` a
required field, and the alternative leaves the container more permissive than
`domowik-assets`, which will not deserialize such a document at all — so the
two readers would disagree on exactly the malformed values where that is
hardest to notice. Three fixtures carried minimal `{"requirement": …}`
descriptors and now carry conformant ones; they exercise the same matrix rows
they always did. The new behaviour is pinned by its own test rather than left
implied by the fixtures.

### Acceptance criteria — all six now have running code

- [x] `asset add|list|verify|remove` and `pack --thin|--portable` follow the
      normative descriptor/profile rules. Descriptors are written, kept through
      edits and both pack profiles, and removed with their blob.
- [x] Portable packing embeds every referenced asset and reports every omission.
- [x] Import hashes while streaming and enforces size/type/resource limits.
- [x] Asset insertion and referencing are crash-consistent or leave a
      detectable orphan.
- [x] Missing, duplicate, corrupt, orphan and unknown-future references are
      covered; garbage collection cannot destroy opaque referenced data.
- [x] Size totals distinguish package, cache and remote bytes. Remote bytes are
      real now — they are a descriptor's declared `byteLength`, and before
      descriptors persisted that class could only ever read zero.

**Status stays `In Progress (partial scope)`, and not as a formality.** ED-0022
records that this issue takes no `Done` while 0021 and 0029 remain unclosed
under their recorded gates, and both are still open. Whether that gate is now
satisfied is the orchestrator's call, not this issue's.

Remote RETRIEVAL remains deliberately absent: the §10.8 policy and its security
predicates are implemented and tested, and nothing fetches.
