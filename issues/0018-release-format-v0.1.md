# 0018: Release experimental format v0.1

- **Phase:** 1
- **Priority:** High
- **Status:** Blocked
- **Depends on:** 0005, 0011, 0026, 0027, 0028, 0029, 0031, 0035, 0036

## Problem

Publish the first experimental `.domo` schema with an honest migration and
compatibility promise. This is a release milestone, not a permanent freeze.

## Acceptance criteria

- [ ] Golden corpus covers every v0.1 entity/op, thin and portable assets,
      duplicates, corruption, missing/orphan blobs, unknown optional data, and
      unsupported required data.
- [ ] Current application migrates every earlier public schema fixture it
      claims to support without silent loss and keeps backups.
- [ ] Unknown fields/entities/operations and newer schema behavior are tested,
      including opaque preservation or safe write refusal.
- [ ] Size corpus and measured thin-package budgets are published.
- [ ] ADR records the exact v0.1 compatibility and migration promise.

## Release-audit reconciliation accepted 2026-08-30 (ED-0052 pre-authorisation)

Branch `issue/0018-release-audit-reconciliation` (`6a4c63b`..`972786b`, four
commits) merged to main by `11ceaee` with integration SBOM refresh `b5d0d7f`.
Full main `./scripts/verify.sh` exits 0 with a clean tree.

This work existed because issue 0031 split one conflated versioning table into
three spec-anchored concepts, leaving the release-audit documentation asserting
the old fused model. Delivered: the private glossary in `AUDIT.md` §10 that
redefined "released schema" to mean "accepted by this reader as its sole
supported label" is DELETED — it was the root defect, and every use of
"released" above it depended on it; `AUDIT.md` and ADR 0007 now name exactly
one of emitted (§8.1 V1), supported (§8.2 V3) or released (§8.2 V4/V5/V6) per
sentence; the machine-readable summary's `releasedSchemaLabels` becomes `[]`
with `emittedSchemaLabel` added, while `isRelease` and `predecessorSchemaSet`
are deliberately untouched because both were already true; and the V3 corpus
fixture's rationale moves from a release claim to the support gate. No
conclusion was weakened — §2's finding is STRENGTHENED, an empty released set
being a stronger statement of an honestly empty migration chain than a
single-entry one. ADR 0007 remains `Proposed`; nothing tags, publishes, or
calls v0.1 released.

Scope was expanded during review by the reviewer, not the worker, to
`AUDIT.md` 136-138, 250-252 and 263 and ADR 0007 17-19 and 46-49, on the
ground that leaving them preserves the defects this issue exists to remove.

**Adversarial review found three defects post-edit and they are recorded
because two were in text that had already passed two review rounds.** A false
`§8.2 V2` anchor (V1 and V2 are under §8.1; §8.2 opens at V3), repeated twice
in `fa4d741`'s commit body; "currently EMPTY, v0.1 included", which places
v0.1 IN the set it declares empty — the fused-release defect reintroduced in
the section that deletes the glossary; and a false rationale in `fa4d741`'s
body, "because tooling consumes it", when no consumer exists and
`cmd/domowik-jsoncheck/main.go:5-8` declares itself parsing-only. **That third
one originated as an orchestrator authorisation rationale and was adopted over
the worker's own contrary grep evidence — recorded as ED-0059**, with
amendments (b) and (c) correcting an attribution error and stating the
same-medium rule's scope.

The repair preserved `fa4d741` and `8b1d53d` unrewritten and added one
correction commit whose message corrects both false durable claims, rather
than rewriting history — which would have erased the claims instead of
correcting them and invalidated the completed independent review.

**Declared imprecision, non-blocking under ED-0059 amendment (c):** commit
`0666421`'s F2 paragraph cites `AUDIT.md:278` where the quoted phrase is
physically line 279; 278 is the clause start of the parenthetical quoted. The
accurate number is 279. It stands uncorrected deliberately, because the commit
reproduces the phrase verbatim and uniquely, so the pointer is redundant with
the quote — and because each further correction commit is itself a durable
artifact capable of carrying its own imprecision, which this issue has already
demonstrated twice.

Reviewer verdict: **ACCEPT** of `6a4c63b..972786b`, delivered after the
contracted-tier post-edit inspection and a fresh independent sweep for a fifth
defect that found none. Reviewer capacity moved three times during this issue
(ED-0058, ED-0061); no verdict was accepted at reduced tier and no work was
integrated on a satisfied precondition.

Status stays **In Progress**: eight of nine dependencies are In Progress with
only 0036 Done, `AUDIT.md` §8 finding 5 stands, and ADR 0007 remains
`Proposed`. This work removed false claims from an audit; it closed no
acceptance criterion.

## Activation (2026-08-25)

Activated under ED-0026 on branch `issue/0018-release-v0.1-audit` as a gated
release-candidate audit against reviewed provisional handoffs. Dependency 0035
is now explicit, completing its documented publication handoff. This work may
audit corpus coverage, compatibility/migration language, reproducibility, and
remaining blockers; it may not tag, publish, call v0.1 released, mark a
compatibility ADR Accepted, or reach Done while any dependency remains
unclosed. The predecessor-schema set must be reported honestly; no positive
migration or public history may be fabricated.

## Partial audit handoff accepted (2026-08-25)

Branch `issue/0018-release-v0.1-audit` (head `6c6243b`) merged to main by
`ccfe953` with SBOM refresh `2c2e3c4`; full main `./scripts/verify.sh` exits 0.
Delivered an executable-evidence inventory, machine-readable blocker routing,
direct 0035 measured-budget publication references, and exact experimental
compatibility/migration wording in ADR 0007, which remains `Proposed`.
Reviewer verdict: **NOT RELEASE-QUALIFYING** partial handoff.

This issue remains `In Progress`. Corpus semantic paths and hostile records
are skipped/partial, asset duplicate/orphan coverage is incomplete, recovery
does not cover the full platform matrix, no independently evidenced predecessor
release exists, dependencies remain unclosed, and ADR approval is open. No tag,
publication, release, positive migration, or Done claim was made.

## Status corrected by ED-0096 — 2026-09-01

**Blocked.** Publication is genuinely blocked on issue 0034's identity decision, which is the owner's. But the rest of this issue is UNSTARTED WORK, not blocked work: there is no Rust release process, no cross-platform artefact, and no SBOM for a Rust binary. The dependency inventory and NOTICE are enforced fail-closed and are neither of those things.
