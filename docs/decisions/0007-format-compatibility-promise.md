# ADR 0007: Experimental and post-v1 format compatibility promise

- **Status:** Proposed
- **Date:** 2026-08-24
- **Owner:** issue 0005 (FORMAT-SPEC §8.3); ratification is a v1.0 release gate

## Context

FORMAT-SPEC §8 fixes the pre-1.0 versioning policy: an experimental `0.N`
schema series where each publicly released minor carries a tested migration
path and breaking changes increment the minor. That policy is deliberately
not Semantic Versioning: SemVer 2.0.0 rule 4 gives major version zero no
stability meaning, whereas Domowik's pre-1.0 policy adds explicit migration
guarantees on top of an experimental series. Borrowing SemVer vocabulary for
that hybrid would misdescribe both.

Format v0.1 is the first schema of the experimental `0.N` series, not a
permanent freeze (FORMAT-SPEC §8.1 V2); it is not yet publicly released
(§8.2 V6). Users and future tooling need to know, before v1.0 ships,
exactly what stability they may rely on afterwards. A compatibility
promise implied only by version labels would be unverifiable; it must be
an explicit contract with testable obligations.

## Decision (proposed)

### v0.1 boundary (proposed audit wording)

The exact v0.1 promise is deliberately narrow:

> Domowik v0.1 is the first schema of the experimental `0.N` series, not a
> stability freeze. This implementation reads and writes schema `0.1` only. The
> evidenced predecessor set is empty: no earlier Domowik schema is claimed as
> publicly released or supported, so no positive predecessor migration is
> promised. A later release may claim support for a predecessor only after an
> independently evidenced public release record and a passing migration corpus
> exist. Unsupported required data, unknown operation/entity kinds, foreign
> namespaces, and newer or otherwise unsupported schema labels are rejected
> before effects. Unknown optional data is preserved with identical meaning
> where the reader can do so; otherwise write-back is refused without silent
> stripping. Explicit migration, when a tested path exists, writes a complete
> validated package through the save state machine and preserves the source
> until atomic replacement succeeds; opening remains read-only. Backups and
> recovery claims are limited to the tested save-state outcomes and do not
> claim power-loss durability beyond the documented platform evidence.

This wording is an experimental compatibility boundary, not a claim that v0.1
is released, tagged, published, or permanently stable. The production
migration registry is empty because, as of 2026-08-29, no `0.N` schema has
been publicly released at all (FORMAT-SPEC §8.2 V6) — not because `0.1` is
the only released one. It is in fact unfillable: §8.2 V4 lets a migration
step originate only at a released schema. No migration is inferred from a
fixture or from an implementation test helper.

Upon ratification — before any file claiming format v1.0.0 is released —
Domowik adopts the following compatibility contract:

1. **Version form.** Post-v1 schema identifiers take the form
   MAJOR.MINOR.PATCH with actual Semantic Versioning 2.0.0 semantics
   (semver.org; provenance E4 in docs/evidence/0005-container-versioning/).
   The declared "public API" for SemVer purposes is the FORMAT-SPEC itself:
   logical schemas of §§2–3, operation/batch semantics of §4, snapshot rules
   of §5, container profile of §7, and the reader/writer obligations of §8–§9.
2. **MAJOR bump** for any change that breaks a conforming reader or writer
   of the previous major line: removing or narrowing fields, changing
   semantics of existing data, altering the container profile incompatibly,
   or tightening acceptance beyond ratified limits. Every MAJOR transition
   ships migrations for all supported predecessor schemas per the §8.2
   discipline, promoted from pre-1.0 practice.
3. **MINOR bump** for backward-compatible additions: new optional fields,
   new entity kinds, new entry namespaces, new operation kinds. Such
   additions MUST be readable by earlier v1.x readers through the
   unknown-optional rules (FORMAT-SPEC §8.4 U2): older readers either
   preserve them opaquely or refuse write-back — never strip silently.
   This makes every MINOR-compatibility claim conformance-testable by
   issues 0028/0029 fixtures rather than aspirational.
4. **PATCH bump** for compatible corrections: clarifications, diagnostic
   improvements, and bug fixes that change no accepted/rejected boundary
   of the specification. Any correction that moves a parse or validation
   outcome is at least MINOR.
5. **Unknown-data floor never weakens.** No later version may reduce the
   U2 preservation duty or the §9 security posture retroactively for data
   written under earlier same-major schemas.
6. **Contract location.** This ADR is the normative home of the post-v1
   promise; FORMAT-SPEC §8.3 cites it. Version labels alone imply nothing.

### Experimental-series statement (recorded by the issue 0018 audit, 2026-08-25)

This statement records the exact v0.1 experimental promise required by
issue 0018's acceptance criteria; it adds no obligation and changes no
gate above. Three facts hold distinctly, and none of them is evidence for
another: `"0.1"` is the sole schema this implementation EMITS
(FORMAT-SPEC §8.1 V1); `"0.1"` is the sole schema its reader SUPPORTS
(§8.2 V3); and no `0.N` schema of the experimental series has been
publicly RELEASED (§8.2 V6). On that basis:

- Domowik claims **no earlier public schema**: the predecessor-schema
  set is empty, and no positive migration exists or may be fabricated.
  FORMAT-SPEC §8.2 V4 is recorded as a blocked open question behind two
  gates: no `0.N` schema is publicly released, so V4 presently binds
  nothing (§8.2 V6 consequence 1); and a positive path would further
  require a specified later target schema that **that later release**
  emits and supports, plus a passing migration corpus.
- Every request whose source and target schema labels differ refuses
  with an actionable `unsupported-version` diagnostic naming both
  versions (V3); opening stays read-only.
- The V4 tested-migration duty attaches **prospectively** to each future
  released minor of the series; nothing retroactive is promised for the
  experimental series, which remains governed solely by FORMAT-SPEC §8.
- Unknown-data duties during this period are exactly those of
  FORMAT-SPEC §8.4: unknown optional data is preserved opaquely or
  write-back is refused — never stripped silently.

Evidence for every clause:
[docs/evidence/0018-release-v0.1/AUDIT.md](../evidence/0018-release-v0.1/AUDIT.md).

## Consequences

- Pre-1.0 files remain governed solely by FORMAT-SPEC §8.2; nothing here
  retroactively constrains the experimental series.
- Ratifying this ADR becomes a checklist item of the v1.0 release (issue
  0018 lineage); until then it binds nobody.
- Conformance corpus work (issue 0028) gains forward-compatible-reader
  vectors once ratified: older-major/minor readers encountering newer MINOR
  additions must preserve-or-refuse, never corrupt.
- If the project later abandons SemVer adoption, this ADR is superseded by
  its own amendment procedure and FORMAT-SPEC §8.3 must be revised with it;
  the two texts are coupled deliberately.
