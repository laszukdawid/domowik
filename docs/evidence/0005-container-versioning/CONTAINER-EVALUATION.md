# Issue 0005 evidence: container candidate evaluation and selection

Status: **Evidence record for FORMAT-SPEC §7.** The normative decision text
lives in [FORMAT-SPEC §7](../../../FORMAT-SPEC.md); this file records the
evaluation required by issue 0005's acceptance criteria ("do not assume ZIP
until evaluated") and the disposition of each §7.1 requirement. Provenance
for every consulted source is in
[PROVENANCE.md](PROVENANCE.md) (records E1–E6).

## 1. Method

Four container candidates were scored against one identical criterion set.
The criterion set, method, scope, exclusions, and provenance plan were
agreed with the paired adversarial reviewer before any matrix drafting;
the completed matrix and the selection it produced are submitted to that
reviewer for full-diff adversarial review at handoff, whose findings are
addressed before this record is treated as final. Criteria are
the binding requirements of FORMAT-SPEC §7.1 (C1–C8), the issue 0005
acceptance-criteria surfaces (streaming, large files, duplicate entries,
checksums, compression methods, atomic complete-package replacement),
thin/portable sameness, operation-batch framing expressibility, and the
project constraint that all tooling be implementable with the Go standard
library and zero new dependencies.

## 2. Criteria

| # | Criterion (source) |
|---|---|
| A | Bounded streaming extraction of any single entry with incremental digest verification; compression outside the hash domain (§7.1 C1/C2; ASSET-STRATEGY §10.1/§10.6) |
| B | Declared lengths validated against observed growth; no allocation proportional to unverified declarations (§7.1 C3) |
| C | Manifest/index located and processed under a bounded reserve, never trusting an unbounded directory region (§7.1 C4; row L7 strategy) |
| D | Entries beyond any legacy 32-bit size/offset field carry a defined large-record mechanism (§7.1 C5) |
| E | Streaming respects rows L1–L5, L15, L16 with atomic cache admission on cancellation (§7.1 C6) |
| F | Random access only as a bounded capability where required (§7.1 C7) |
| G | Physical, logical (expanded), and decoded bytes kept distinct (§7.1 C8; §9.2) |
| H | One unambiguous manifest; deterministic logical entry names; duplicate logical names rejected (issue ACs; §9.4) |
| I | Checksum/digest coverage per entry class (issue ACs; `checksum-error`, digest-mismatch classes of §9.4) |
| J | Compression-method allowlist feasible per entry, with writer compression settings non-observable (issue ACs; reviewer amendment) |
| K | Atomic complete-package replacement via FORMAT-SPEC §9.7 S0–S5 only; no in-place append reliance (issue ACs; BEHAVIOR-SPEC §9.1) |
| L | Thin and portable packages serialize the same logical plan under one manifest schema (issue ACs; ASSET-STRATEGY §10.4) |
| M | Operation-batch framing: deterministic naming/order/framing with rejects surfacing `malformed-framing` before replay (ED-0017 transfer; §4.3 row 4) |
| N | Go-standard-library implementability, zero new dependencies for tooling (binding constraint) |
| O | Hostile-input surface mappable onto the already-ratified §9 schedule without new limit categories |

## 3. Matrix

Legend: **+** satisfies, **±** satisfies with added nonstandard constraints,
**−** fails or requires weakening a ratified rule.

| Criterion | ZIP / ZIP64 family | tar / POSIX pax | Custom sequential container | SQLite database |
|---|---|---|---|---|
| A streaming + digest-as-stream | + central directory locates entries; inflate streams (RFC 1951 bounded memory, E2) | + sequential-native | + by construction | ± blob streaming exists but through SQL engine buffers |
| B declared-length validation | + sizes declared in local+central headers, checkable both places | + size field per block | + self-defined | − page-level structures trust internal pointers |
| C bounded index processing | + EOCD located at tail; central-directory size capped by L7 strategy | + no global index needed | + self-defined | ± schema pages must be walked to enumerate content |
| D large records | + Zip64 extra field 0x0001 and Zip64 EOCD (APPNOTE 6.3.10, E1; §7.1 items 1–4) | + pax `size` extended record above 8,589,934,591 octets (E3) | + self-defined | + sizes far beyond needs |
| E ratio/limit guards | + container-neutral counters bind identically (L15/L16 are entry-based; ZIP has per-entry physical sizes) | − whole-archive compression would erase the per-entry physical quantity L15 binds; stored-only defeats compression purpose | ± same problem if compressed; otherwise fine | − per-entry expansion ratios not expressible over SQL storage |
| F random access | + native, bounded via central directory | − sequential only; a custom index would have to be invented | − unless invented | + native |
| G size-class separation | + physical = stored bytes, logical = inflated bytes, distinct fields | − single stream conflates stored/logical when compressed | + self-defined | ± page/storage vs payload distinction differs from §9.2 classes |
| H unique manifest/names | + name uniqueness enforceable; duplicates already classified hostile (§9.4, L29 boundary preserved) | − repeated names are legal sequences; uniqueness is a nonstandard overlay | + self-defined | ± uniqueness requires app-level tables |
| I checksums/digests | ± CRC-32 per entry (corruption detection) plus manifest sha256 inventory (authoritative integrity) — see §7 | − header checksum is weak additive; strong digests need invented vendor pax records | + self-defined | ± optional newer checksum feature, not inherent |
| J method allowlist, settings non-observable | + method field per entry; allowlist {store, deflate} trivially enforced (E2/E6) | − no standard per-entry compression | + self-defined | n/a engine-owned |
| K atomic full replacement | + single-file write + rename fits §9.7 exactly; append never relied upon | + single-file write + rename | + self-designed | − rollback journal/WAL sidecars are part of state (E5); multi-artifact model conflicts with §9.7 S0–S5 single-candidate replace and RS-state recovery |
| L thin/portable sameness | + profile is manifest data; carrier identical | + | + | + |
| M batch framing | + one JSON document per deterministic entry name; enumeration order derivable from names | ± order derivable but framing checks weaker (no index) | + self-defined | ± via tables |
| N Go stdlib, zero deps | + archive/zip built-in incl. Store/Deflate and ZIP64 fields (E6) | + archive/tar built-in | + trivially | − requires cgo or a third-party port ⇒ new dependency |
| O maps onto ratified §9 schedule | + §9.3 rows L6/L15/L16 and the §7.1 conditional schedule were ratified around ZIP quantities (P7-grounded); checklist discharges item-by-item | ± several rows lose their referent | − entire hostile-input schedule would need re-derivation for novel structures, burdening issues 0028/0029 | − new hostile surface (SQL parser, b-tree corruption classes) outside the ratified taxonomy |

## 4. Selection

**Selected: ZIP-family container under a strict Domowik profile** — a ZIP64-
capable ZIP archive restricted by the normative profile rules of FORMAT-SPEC
§7. The decisive rows are D+E+J+N+O: it is the only candidate that carries a
standardized large-record mechanism, keeps the ratified per-entry expansion
limits meaningful, admits a two-method compression allowlist, implements in
the Go standard library with zero dependencies, and maps onto the already
ratified §9 hostile-input schedule instead of forcing its re-derivation.

## 5. Rejection rationales

- **tar/pax:** fails E and J structurally (no per-entry compression; a whole-
  stream compressor erases the per-entry physical quantity that ratified row
  L15 binds), fails F (no index; random access would require inventing one),
  weakens I (no standard strong per-entry digest; vendor pax extensions would
  be self-authored anyway), and turns duplicate-name rejection into a
  nonstandard overlay on formats where repeated names are legal (E3).
- **Custom sequential container:** scores well on purity (minimal surface)
  but loses O decisively — every structure would be novel, so the entire
  ratified §9 schedule and the §7.1 capability review would need fresh
  fixtures (issues 0028/0029) against structures with no independent
  precedent, while offering no user-visible benefit over the profiled-ZIP
  outcome. Recorded as the strongest rejected alternative.
- **SQLite:** fails N (not Go stdlib; any use adds a dependency), fails K
  (journal/WAL artifacts are part of database state during recovery (E5),
  conflicting with the ratified single-candidate replace/recovery contract of
  FORMAT-SPEC §9.7), and imports a general-purpose engine's hostile-input
  surface that the ratified rejection taxonomy does not cover.

## 6. Disposition of §7.1 conditional items

The eight-item conditional ZIP64 schedule of FORMAT-SPEC §7.1 becomes
binding through the selection and is discharged clause-by-clause in
FORMAT-SPEC §§7.2–7.9 (items 1–4 → §7.8 Zip64 usage; item 5 → §7.7 method
allowlist; item 6 → §§7.2–7.3 duplicate/encryption rejection; item 7 → §7.3
path normalization; item 8 → §7.7 archive-limit mapping). No §7.1
capability requirement C1–C8 is weakened; each is realized or exceeded in
the selected-container sections.
