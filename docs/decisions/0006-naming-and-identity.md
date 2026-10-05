# ADR 0006: Naming and Identity Ratification

- **Status:** Accepted
- **Date:** 2026-08-23
- **Depends on:** [Naming due-diligence dossier](../NAMING-DUE-DILIGENCE.md),
  issue 0009, executive decisions ED-0006 and ED-0008

## Context

Issue 0009 asks this project to ratify the `.domo` extension, the `domowik`
binary name, application IDs, the Go module path, and a MIME registration
strategy. Public black-box observation (see the dossier) records an existing
Android home-organizer app published as "Domowik: Home Organizer" with
application ID `pl.domowik.app` and the domain `domowik.dev`, plus other
public users of the word. The dossier establishes no relationship, rights,
ownership, or clearance between any observed party and this project.

On 2026-08-23 this ADR was Proposed with both naming paths gated behind an
unmet authorization-or-rename condition (ED-0006), and issue 0009 was marked
Blocked. The same day, executive input superseded that gate. It is recorded
verbatim as ED-0008 in `EXECUTIVE-DECISIONS.md`, which explicitly supersedes
ED-0006:

> "For the url, domowik.dev sounds good. We'll definitely change this in the
> future so make sure that it's easy to find and modify. This shouldn't be a
> blocker."

This decision establishes no legal clearance, availability, ownership,
affiliation, non-affiliation, or non-infringement for any identifier or any
party.

## Decision

Adopt the identity values below as **provisional**, centrally tracked,
expected to change where marked, and replaceable via the canonical registry
[NAMING.md](../NAMING.md). Each artifact has an independent status and
independent future-change treatment; changing one changes nothing else by
implication.

| Artifact | Value | Status | Future-change treatment |
|---|---|---|---|
| Product name | "Domowik" | Provisional | Registry sweep + this ADR |
| Project URL | `https://domowik.dev` | **Temporary** per ED-0008; expected to change | Registry sweep + this ADR |
| Go module path | `domowik.dev/domowik` | Provisional | Changes with URL; single derivation recorded in NAMING.md |
| Binary name | `domowik` | Provisional | Registry sweep + this ADR |
| File extension | `.domo` | Provisional | Registry sweep + FORMAT-SPEC + this ADR |
| Application IDs | None selected (reconfirmed 2026-08-24: no application ID has been selected, registered, or published by this project); derive at store publication from then-canonical URL/domain | Deferred-by-rule | Record here + NAMING.md at selection |
| MIME type | No Domowik media type is selected or registered by this project; decision of 2026-08-24 is continued deferral — files described by extension and container signature; revisit at first public binary release | Deferred-by-rule (deferral decision recorded) | Same as application IDs; revisit trigger recorded in NAMING.md |
| Repository/package namespaces | Unreserved on every platform; planned per-platform inventory in NAMING.md §Namespace observations; reserve consistently across platforms at first publication subject to each registry's access policies and separately established authority | Deferred-by-rule | Record selections in NAMING.md at publication |

Required old→new migration mappings have a defined storage location: the
"Migration alias mappings" table in NAMING.md, which must be filled per
affected artifact before any post-publication rename is executed.

The RDAP observation that `domowik.dev` is registered by another party
(dossier O4) stands. Adoption of the string is internal and provisional: no
hosting, registration attempt, or claim of control is made.

## Superseded context (history)

The Proposed version of this ADR defined Gate G0 — retain-if-authorized
(Branch A) versus executive-approved rename (Branch B), under ED-0006 — and
held all artifacts unratified while issue 0009 was Blocked. ED-0008 retired
that gate: the blocker was superseded by explicit executive acceptance of
provisional adoption plus a mandated replaceability mechanism. Branches A
and B remain recorded here as historical alternatives; if pre-publication
checks (release gate below) or future circumstances warrant revisiting
naming, they describe the still-valid evidence standard: confirmed
relationship/explicit authorization scoped per the dossier, or an exact
approved migration mapping. Ownership alone never sufficed under either
branch.

## Release gate (issue 0009 AC 4)

Before the first public binary release:

1. Complete the dossier's outstanding manual registry checks (EUIPO eSearch
   Plus, Polish Patent Office e-search, WIPO Global Brand Database, USPTO if
   applicable) using timestamped text-only public result references filed
   outside the repository.
2. Re-confirm repository/package namespace availability on each target
   platform, without inferring absence from restricted or failed queries.
3. Select application IDs from the then-canonical URL/domain and record them.
4. If any finding warrants a change, execute the NAMING.md change procedure
   including migration alias mappings.

These checks are risk-reduction inputs. Their outcome may trigger the change
procedure; it never produces clearance, ownership, affiliation,
non-affiliation, or infringement statements.

### Release-gate progress record (2026-08-24, issue 0034)

A first read-only, text-only public due-diligence pass was performed on
2026-08-24 (UTC) by the issue 0034 pair. Raw evidence with URLs, queries,
verbatim result text where retrievable, timestamps, limitations, and manual
completion procedures is filed outside this repository at
`/home/dlaszuk/projects/domowik-observatory/evidence/0034/` (sha256 manifest
`MANIFEST.txt` therein); a status index summarizing each check with outcome
classifications is in NAMING.md §Pre-publication due diligence status index.

Outcome per gate item:

1. **Manual registry checks — outstanding.** All four registries
   (EUIPO/TMview, Polish Patent Office, WIPO Global Brand Database, USPTO)
   require interactive browser sessions or human verification that the
   text-only method cannot provide; each check is recorded as attempted but
   incomplete/restricted/unavailable. No absence or availability conclusion
   is drawn from any response.
2. **Namespace reconfirmation — partially observed, outstanding.**
   Observation-level checks were recorded for GitHub and pkg.go.dev; PyPI,
   npm, and crates.io public search pages were inaccessible to this method
   and remain unperformed as valid checks. No namespace was reserved
   anywhere; reservation remains forbidden pending separately established
   authority.
3. **Application-ID selection — decision recorded: deferral retained.** No
   application ID is selected. The derivation rule (store-publication time,
   then-canonical URL/domain) stands unchanged in this ADR and NAMING.md;
   selection will be recorded there when it happens. Platform manifests do
   not yet exist and receive IDs only at creation.
4. **Change procedure — not triggered.** No finding of this pass warrants a
   value change; migration alias mappings remain defined and empty.

**This pass does not satisfy the release gate.** Items 1 and 2 must be
completed through the registries' interactive public interfaces, and all
observations reconfirmed, before the first public binary release. The MIME
strategy was decided within this pass as continued deferral with rationale
and a concrete revisit trigger (NAMING.md §MIME strategy decision).

### Release-gate progress appendix — corrective second pass (2026-08-24)

A corrective second read-only, text-only public due-diligence pass was
performed on 2026-08-24 (UTC), after an interrupted out-of-scope batch was
fully discarded (process incident at 2026-08-24T13:05:49Z, recorded in the
evidence directory as explicitly non-evidence). Every external request in
this pass was an individually issued, non-scripted ordinary public web page
request; no API/index/simple/JSON endpoint was contacted. Raw evidence is
filed with the first pass at
`/home/dlaszuk/projects/domowik-observatory/evidence/0034/` as
`07-reattempt-registries.txt` (trademark registries, attempts 1–5) and
`08-reattempt-namespaces.txt` (package registries and namespaces,
attempts 6–14); files `01`–`06` are byte-identical to the original capture
and the sha256 manifest `MANIFEST.txt` was regenerated (all entries verify
OK). Each attempt carries exactly one permitted class token (SUCCESS,
NO-RESULT, BLOCKED, RATE-LIMITED, MANUAL-ONLY).

Outcome per gate item — unchanged by this pass:

1. **Manual registry checks — still outstanding.** EUIPO landing page fetch
   succeeded (SUCCESS) but TMview, the Polish Patent Office e-search, WIPO
   Global Brand Database, and USPTO were all BLOCKED for lookup purposes
   (JavaScript-only application, bot-protection interstitial,
   human-verification gate, JavaScript-only shell respectively; attempts at
   13:09:09Z–13:10:05Z). All four resulting lookups remain MANUAL-ONLY and
   unperformed; no absence or availability conclusion is drawn.
2. **Namespace reconfirmation — still partially observed, outstanding.**
   pkg.go.dev returned NO-RESULT ("Showing 0 modules with matching
   packages." at 13:11:32Z) and the direct GitHub namespace page returned
   HTTP 404 (NO-RESULT at 13:11:44Z), both time-of-access observations
   only; the repository search (SUCCESS at 13:17:02Z) again observed four
   third-party "domowik" repositories, consistent with dossier O8. PyPI,
   npm, and crates.io remained BLOCKED on both search and direct package
   pages (13:10:15Z–13:16:26Z); their lookups remain MANUAL-ONLY and
   unperformed. No namespace was reserved anywhere by this project;
   reservation remains forbidden pending separately established authority.

Items 3 and 4 are unaffected: application-ID selection remains deferred by
rule and the change procedure remains untriggered, with migration alias
mappings defined and empty. **This corrective pass also does not satisfy the
release gate.** No availability, absence, clearance, ownership,
affiliation, non-affiliation, or non-infringement conclusion is drawn from
any outcome recorded in either pass; every response is a time-of-access
observation or access restriction only.

## Verification

Any change to identity values must pass, and the results be recorded:

- Deterministic location check: run the NAMING.md §Deterministic location
  check command; every hit must be in NAMING.md (canonical definition),
  `EXECUTIVE-DECISIONS.md` (only within the preserved decision/reason text
  of entry ED-0006 — whose status metadata alone reads "Superseded by
  ED-0008" — and within entry ED-0008, per NAMING.md's entry-attribution
  rule), this ADR (decision record),
  `docs/NAMING-DUE-DILIGENCE.md` (excluded immutable history), the issue
  0009 blocker-resolution history section (exempt preserved history), or
  text that marks the value temporary/provisional and points to the
  registry. Any other hit is a defect. No ordering property is claimed.
- Sweep preservation: for any future-token change sweep, NAMING.md's sweep
  exclusions apply, including the preserved decision/reason text of
  superseded executive decisions (currently ED-0006). ED-0006's
  decision/reason text stays verbatim — only its status metadata line was
  intentionally changed to "Superseded by ED-0008", and that line is not
  covered by the exemption; a token change never rewrites superseded
  history.
- Canonical values: every artifact row above matches its row in NAMING.md.
- Artifact status: every artifact carries exactly one of Provisional /
  Temporary / Deferred-by-rule / Empty.
- Alias storage: NAMING.md contains a defined "Migration alias mappings"
  table, referenced here, even when empty.
- Stale-blocker: active documents retain no operative blocker for issue
  0009. Preserved history is exempt and must be unambiguously labeled as
  such: the dossier's historical status header and Gate-G0-era dispositions
  carry a supersession annotation pointing to this ADR and ED-0008; tracker
  `done/` records and quoted historical sections are likewise exempt.
  Tracker index agrees with the issue file.
- ADR/index sync: `docs/decisions/README.md` shows this ADR Accepted.
- Prohibited claims: no document states clearance, availability, ownership,
  affiliation, non-affiliation, or non-infringement for any party; only
  negations/disclaimers of such claims appear.

## Consequences

- Issue 0009's four acceptance criteria can now be satisfied on the current
  integrated tree: names are ratified provisionally in an Accepted ADR
  (AC 1), MIME is explicitly deferred (AC 2), glossary consistency was
  audited against the current normative specifications (AC 3), and the
  release gate accounts for naming/trademark checks before public release
  (AC 4).
- The temporary URL guarantees future churn; NAMING.md exists so that churn
  is mechanical rather than archaeology.
- The due-diligence dossier's observations, URLs, timestamps, limitations,
  and original matrix data are preserved as the risk record supporting the
  release gate; only clearly labeled supersession annotations and an
  appended addendum were added to it.
