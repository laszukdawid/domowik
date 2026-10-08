# 0034: Pre-publication identity finalization

- **Phase:** 0
- **Priority:** High
- **Status:** Blocked
- **Depends on:** 0009
- **Blocks:** 0018 (release format v0.1)

## Current disposition — 2026-10-05

Keep blocked on release identity/namespace decisions; do not re-run
time-sensitive checks from old observations.

Current local Git remote is configuration, not proof of public availability or
trademark clearance. Separate source-preview readiness from
application/package publication, finalize IDs/reservations only with owner
authority, and refresh registry observations when actually preparing that
release. Go module-path removal is discharged by 0062/0093. Relative
repository documentation links need no replacement for a normal GitHub source
publication.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

ADR 0006's release gate and `../docs/NAMING.md` defer several identity
decisions to first publication. They must be completed before any public
binary release, using timestamped text-only public result references filed
outside the repository.

## Scope

- Complete the manual registry checks from `../docs/NAMING-DUE-DILIGENCE.md`
  (EUIPO eSearch Plus, Polish Patent Office e-search, WIPO Global Brand
  Database, USPTO if applicable).
- Re-confirm repository/package namespace availability per platform without
  inferring absence from restricted or failed queries.
- Select application IDs derived from the then-canonical URL/domain; record
  in ADR 0006, NAMING.md, and platform manifests.
- Decide the MIME registration strategy (register or continue describing
  files by extension and container signature); record the decision.
- Reserve repository/package namespaces consistently across platforms.

Excluded: asserting legal clearance, availability, ownership, affiliation,
non-affiliation, or non-infringement for any identifier or party. Check
outcomes may only trigger ADR 0006's change procedure.

## Acceptance criteria

- [ ] Dated text-only results for each registry check exist outside the
      repository and are summarized with URLs and access timestamps.
- [ ] Application IDs are recorded in ADR 0006 and `../docs/NAMING.md`.
- [x] MIME strategy is recorded as registered or as continued deferral with
      rationale.
- [ ] Namespace reservations are listed per platform in `../docs/NAMING.md`.

## Integrated partial handoff — 2026-08-24

Commit `efc13cc` was integrated into the main repository by merge commit
`f900d96`. The ED-0009-limited repository changes and the external evidence
manifest under `domowik-observatory/evidence/0034/` passed paired review and
verification.

The issue is not Done. Several manual registry/package checks remain
incomplete or restricted, application IDs remain deliberately unselected, and
external namespace reservations were not authorized. The repository records
those limits without claiming legal clearance or availability.

## Corrective evidence continuation — 2026-08-24

The corrective, individually issued public-page pass is indexed in
`../docs/NAMING.md` and ADR 0006 by substantive commit `5d2e0c2`. After
reconciling current main and regenerating exact-tree release inventory, the
handoff was integrated by main merge commit `e5dfa4d` and verified in full;
the integration-tree SBOM refresh is `0a42314`.

The external evidence manifest contains nine verified entries, including
separate trademark-registry attempts 1–5 and package/namespace attempts
6–14. These are time-of-access observations and access restrictions only.
The manual interactive trademark checks and namespace reconfirmations remain
outstanding, so the first acceptance criterion is not complete. Application
IDs remain deferred by rule, MIME remains deliberately deferred under the
recorded strategy, and no namespace was reserved. Status therefore remains
`In Progress (partial scope)` and the release gate remains unsatisfied.

## Status set by ED-0095 — 2026-09-01

**Blocked.** Blocked on an owner decision about publication identity. docs/NAMING.md records the repository URL as unreserved, and nothing in the code may claim it.

## Preparation round — 2026-09-12

Evidence and a concrete proposal, prepared under root's assignment:
[docs/evidence/0034-identity/PREPARATION-2026-09-12.md](../docs/evidence/0034-identity/PREPARATION-2026-09-12.md).
**No status change, no criterion ticked, nothing reserved or registered.**

**The repository exists and is PRIVATE** — `laszukdawid/domowik`, created
2026-08-26, verified through an authenticated `gh repo view`. That reframes
this issue: the blocker is not "decide whether to publish under some name"
but "decide whether this repository becomes public", because the stable URLs
the tracker and issue 0018 need are unreachable until it does.

**A 404 does not mean absent.** The unauthenticated GitHub API returns 404 for
this repository precisely because it is private. The same ambiguity applies to
every `NO-RESULT` row in the 2026-08-24 pass.

**Three registry checks that were BLOCKED on 2026-08-24 are now unblocked**, via
machine-readable APIs rather than bot-protected search pages: crates.io returns
``crate `domowik` does not exist``, PyPI and npm return their own not-found
bodies. These are time-of-access observations that the name is unpublished —
not availability, and not clearance.

**`domowik.dev` is live third-party infrastructure** (HTTP 200 today), which
makes the Go module path `domowik.dev/domowik` a hazard rather than a
placeholder, since Go resolves module paths over HTTP. Issue 0062 removes
`go.mod` and would settle it by deletion; that interaction is a sequencing
note for root.

Five owner decisions and four interactive trademark checks remain, listed in
the proposal. The first acceptance criterion cannot close until the four are
performed by a person.

## Owner choice sheet — 2026-09-12

[docs/evidence/0034-identity/OWNER-CHOICES.md](../docs/evidence/0034-identity/OWNER-CHOICES.md)
states the six choices only the owner may make, each with its consequences, and
separates the release work that needs none of them from the work that is
genuinely blocked.

The short version: **most of the remaining release work is not blocked.** SBOM
generation, licence scanning, `NOTICE`, a Rust release process, cross-platform
artefacts, CI packaging coverage, the Go retirement and a migration framework
can all be built and tested with no URL, no visibility change and no
reservation. What is blocked is publication itself, stable documentation links,
crates.io, application IDs, and this issue's acceptance criteria 1 and 4.

Still true, and still nobody else's to settle: the repository is private, the
four trademark checks are interactive and human-only, and no namespace has been
or may be reserved. Status stays `Blocked`; no criterion is ticked.

## Corrections after root review — 2026-09-12

Three claims in the two notes above exceeded their evidence and are withdrawn in
place:

- **Visibility does not gate the canonical URL or the documentation links.** A
  private repository still has a stable URL for authorised users, and relative
  links render the same either way. The link replacement keeps its original
  trigger in `issues/README.md` — before the tracker is published independently.
  What visibility governs is public distribution.
- **No store rule was checked**, so the claim that forge-derived application IDs
  are discouraged is withdrawn. A controlled namespace is a recommendation with
  a durability reason, not a technical constraint.
- **The four trademark searches were not attempted**, and calling them
  human-only was an assumption. Their feasibility is untested. Distinct and
  unchanged: a clearance determination needs qualified review the owner
  arranges, which no search result supplies.

The domain non-response conclusion stands: one failed request proves nothing
about registration. Original project gates are unchanged; status stays
`Blocked`; no criterion is ticked.

## Trademark searches performed — 2026-09-12

[docs/evidence/0034-identity/TRADEMARK-SEARCH-2026-09-12.md](../docs/evidence/0034-identity/TRADEMARK-SEARCH-2026-09-12.md).
Read-only, official public interfaces, nothing registered or bypassed.

**Feasibility answered: the searches are not human-only.** TMview's public API
carries EUIPO, UPRP, USPTO and WIPO data, and per-office coverage was proved
with positive controls rather than assumed.

**No exact match for "domowik" in any of the four offices.** Fuzzy matching
returns 41 hits, and the nearest are one character away in the target
jurisdictions — `DOMOVIK` at USPTO and EUIPO, `DOMOWNIK` several times at UPRP.

**This is evidence, not clearance.** No similarity or risk assessment is
offered; that judgement needs qualified review the owner arranges. The four
registries' own interfaces returned an API-key requirement (USPTO), two
anti-bot access controls (WIPO, UPRP) and a JavaScript shell (EUIPO), so none
was queried directly and no control was bypassed.

Criterion 1 is closer but not met: TMview is an aggregator, not an official
register extract. Status stays `Blocked`; no criterion ticked.
