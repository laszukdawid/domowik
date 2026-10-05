# Issue 0005 evidence: container and versioning provenance records

Verbatim-observation records following the FORMAT-SPEC §9.8 and
ASSET-STRATEGY §10.10 pattern: title, version/date, URL, exactly what was
consulted, license status, access date. No clearance claims. **No GPL-
licensed source and no floor-plan-application source or artifacts were
consulted for this issue.** Access date for all entries: 2026-08-24.
"Fetched" means loaded and checked by the issue 0005 worker during this
issue's research.

## Records

- **E1 — APPNOTE.TXT .ZIP File Format Specification (relied-upon authority
  for the selected container).** PKWARE Inc., version 6.3.10, Status FINAL,
  revised Nov 01, 2022. NOT re-downloaded by issue 0005; consumed solely
  through provenance record P7 of [FORMAT-SPEC §9.8](../../../FORMAT-SPEC.md)
  (downloaded and section-verified 2026-08-23 by issue 0023, independently
  re-verified by its reviewer), which records free downloadability,
  copyright, permitted-use scope (implementing ZIP readers/writers), and the
  patented-components caveat. Issue 0005 reproduces no APPNOTE text beyond
  short section references already carried in FORMAT-SPEC §7.1.
- **E2 — RFC 1951, DEFLATE Compressed Data Format Specification version 1.3.**
  P. Deutsch, Aladdin Enterprises, IETF, May 1996;
  https://www.rfc-editor.org/rfc/rfc1951.txt (fetched). Facts used: deflate
  is a lossless byte-level compressed data format producible and consumable
  as a stream with a priori bounded intermediate storage; implementable
  readily in a manner not covered by patents; worst-case expansion bound
  stated in §1.1. License status: copyright 1996 L. Peter Deutsch with
  express permission to copy and distribute for any purpose without charge
  provided the notice is preserved; freely implementable. Grounds the
  method-8 (deflate) allowlist entry and its streaming-decompression
  properties in FORMAT-SPEC §7.
- **E3 — IEEE Std 1003.1-2017 `pax` utility / pax interchange format.**
  The Open Group Base Specifications Issue 7, 2018 edition;
  https://pubs.opengroup.org/onlinepubs/9699919799/utilities/pax.html
  (fetched). Facts used, solely for the candidate-evaluation matrix
  ([CONTAINER-EVALUATION.md](CONTAINER-EVALUATION.md)): the pax interchange
  format shares the ustar block layout with optional extended header records;
  the `size` extended-header keyword carries file sizes above 8,589,934,591
  octets (large-file mechanism); archives terminate with two 512-byte
  zero blocks; multiple members with the same name are representable as a
  sequence (no native uniqueness rule); no per-entry compression or
  strong per-entry checksum exists in the standard formats. License status:
  IEEE/Open Group standard documents; limited quotation of short factual
  references only, no reproduction of standard text.
- **E4 — Semantic Versioning 2.0.0.** Tom Preston-Werner;
  https://semver.org/ (fetched; the root page renders the Semantic
  Versioning 2.0.0 specification text and its grammar, and the page's own
  License section links Creative Commons — CC BY 3.0). Facts used: rule 4 —
  major version zero (0.y.z) is for initial development, anything MAY change
  at any time, the public API SHOULD NOT be considered stable; rules 6–8
  define PATCH/MINOR/MAJOR increment meanings; rule 1 requires declaring a
  public API. License status: CC BY 3.0 per the fetched page's license
  section. Grounds FORMAT-SPEC §8's terminology rule: partial adoption of
  SemVer vocabulary without SemVer semantics would conflict with rule 4's
  no-guarantees reading of 0.y.z, so the pre-1.0 scheme is defined
  independently and labeled as such.
- **E5 — SQLite Database File Format.** SQLite documentation page
  "Database File Format" (fileformat2.html);
  https://www.sqlite.org/fileformat2.html (fetched). Facts used, solely for
  the candidate matrix: the complete database state is usually one main file
  but rollback-journal/WAL sidecar files are part of the state during
  transactions and crash recovery ("hot journals … are part of the state of
  an SQLite database and so cannot be ignored"). Grounds the atomicity
  criterion finding that a single-file rename-based replace contract
  (FORMAT-SPEC §9.7) does not map onto SQLite's multi-artifact recovery
  model without weakening §9.7. License status: SQLite documentation is
  public domain (SQLite copyright page); short factual references only.
- **E6 — Go standard library `archive/zip` documentation, go1.27.0.**
  https://pkg.go.dev/archive/zip (fetched). Facts used, solely for the
  candidate matrix: package provides ZIP read/write support with built-in
  Store (method 0) and Deflate (method 8) compression methods, 64-bit
  CompressedSize64/UncompressedSize64 header fields used for the ZIP64
  format when 32-bit fields overflow, random access via io.ReaderAt, and no
  disk-spanning support; BSD-3-Clause license. Grounds the zero-new-
  dependency implementability criterion; implementation itself belongs to
  later issues.

## Negative findings

- No comparison against GPL-licensed floor-plan applications' storage
  implementations was performed or needed; the evaluation compares published
  general-purpose container formats only (clean-room policy).
- Whether any mainstream tar/pax tooling exposes per-entry cryptographic
  digests natively: not researched beyond E3, because the standard formats
  themselves lack them, which suffices for the matrix row. Recorded as a
  negative observation, not cited as authority for anything normative.

Any reference whose provenance were uncertain would be quarantined per
FORMAT-SPEC §9.8's closing rule; none arose here.
