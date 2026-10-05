# Naming and Identity Registry

Status: **Canonical registry.** This is the single place where Domowik's
identity values, their provisional/permanent status, their migration-alias
record, and their change procedure live. Executive decision ED-0008
(2026-08-23, superseding ED-0006) adopted these values as explicitly
temporary — "We'll definitely change this in the future" — and required that
a future change be easy to find and execute. **Change procedure starts here:
no identifier is renamed by editing scattered files directly.**

This registry makes no legal clearance, availability, ownership,
affiliation, non-affiliation, or non-infringement claims. See
[NAMING-DUE-DILIGENCE.md](NAMING-DUE-DILIGENCE.md) for recorded public
observations (immutable risk history) and their limitations.

## Authority

- [ADR 0006](decisions/0006-naming-and-identity.md) (Accepted) — decision record.
- `EXECUTIVE-DECISIONS.md`, entry ED-0008 — verbatim executive input;
  supersedes ED-0006.

## Registry

| Artifact | Canonical value | Status | Future-change treatment |
|---|---|---|---|
| Product name | `Domowik` | Provisional | Sweep via checklist below; update ADR 0006 and this row |
| Project URL | `https://domowik.dev` | **Temporary** (executive: expected to change) | Same sweep; see caution below |
| Historical Go module path | `domowik.dev/domowik` | Retired from active code (issue 0062) | No active Go module; retained for historical reference, not a publication target |
| Binary name | `domowik` | Provisional | Sweep + ADR 0006 update |
| File extension | `.domo` | Provisional | Sweep + FORMAT-SPEC + ADR 0006 update |
| Application IDs | None selected (reconfirmed 2026-08-24: no application ID has been selected, registered, or published by this project). Rule: derive at store-publication time from whichever URL/domain is canonical then | Deferred-by-rule | Record selection in ADR 0006 and this row at publication |
| MIME type | No Domowik media type is selected or registered by this project. Decision (2026-08-24): continue describing files by extension and container signature; see §MIME strategy decision | Deferred-by-rule (deferral decision recorded) | Revisit trigger: preparation of the first public binary release (issue 0018); record any registration decision here and in ADR 0006 |
| Repository/package namespaces | Not reserved on any platform. Planned per-platform inventory: §Pre-publication due diligence status index → Namespace observations, below | Deferred-by-rule | Record selections here and in that section at publication |

**Caution on the URL:** public RDAP records (dossier O4) show `domowik.dev`
registered by a registrant other than this project. Adoption of the string is
internal and provisional only: no hosting, registration attempt, or claim of
control is made or implied.

## MIME strategy decision (2026-08-24)

Decision: **continued deferral.** No Domowik media type is selected,
registered, or applied for by this project, and no registry-absence
conclusion of any kind is drawn. Files continue to be described by the
`.domo` extension and the container signature defined in the format
specification.

Rationale:

- Registration requires stable, published identifiers; the project URL is
  explicitly temporary (ED-0008) and any derived type/subname would embed a
  name expected to change.
- No public binary release or distributed `.domo` corpus exists yet, so no
  external consumer depends on a media type today.
- Deferral is reversible: the change procedure below and the migration alias
  table cover a later registration without rework.

Revisit trigger: preparation of the first public binary release (issue
0018). At that point decide between IANA registration of a derived type and
permanent extension-plus-signature description, and record the outcome here
and in ADR 0006.

## Pre-publication due diligence status index (2026-08-24)

This section is an auditable evidence/status index for ADR 0006's release
gate. It is not a second registry, does not replace the immutable dossier
([NAMING-DUE-DILIGENCE.md](NAMING-DUE-DILIGENCE.md)), and **does not satisfy
the release gate**: every registry check below must be re-executed through
its interactive public interface and every namespace observation reconfirmed
before the first public binary release.

Raw text-only evidence (URLs, queries, verbatim result text where retrievable,
UTC timestamps, limitations, and manual completion procedures) is filed
outside this repository at:

`/home/dlaszuk/projects/domowik-observatory/evidence/0034/`
(sha256 manifest: `MANIFEST.txt` in that directory; verified with
`sha256sum -c MANIFEST.txt`.)

### Registry checks

| Check | Public URL / query | Accessed (UTC) | Outcome | Evidence |
|---|---|---|---|---|
| EUIPO eSearch Plus / TMview | `https://www.euipo.europa.eu/en/trade-marks`; `https://www.tmdn.org/tmview/` | 2026-08-24T04:31–04:35Z | Attempted — incomplete: landing page reachable; search app is JavaScript-only; one out-of-scope REST endpoint request returned HTTP 405 and is recorded as a restricted-access event, not a lookup result | `01-euipo-tmview.txt` |
| Polish Patent Office e-search | `https://ewyszukiwarka.pue.uprp.gov.pl/` (+ query URL variant) | 2026-08-24T04:31–04:32Z | Attempted — unavailable: application shell only; query URL returned no usable content; no result text obtainable | `02-pupr-pl-trademarks.txt` |
| WIPO Global Brand Database | `https://branddb.wipo.int/` | 2026-08-24T04:32Z | Attempted — restricted: entry gated behind a human-verification challenge; not bypassed; no query performed | `03-wipo-branddb.txt` |
| USPTO trademark search | `https://tmsearch.uspto.gov/` | 2026-08-24T04:33–04:35Z | Attempted — incomplete: search app is JavaScript-only; supplementary web-search engine query surfaced no USPTO record for "domowik" (weak non-decisional signal only) | `04-uspto-tmsearch.txt` |

No absence, availability, clearance, ownership, affiliation,
non-affiliation, or non-infringement conclusion is drawn from any of these
outcomes. Each evidence file contains the exact reproducible manual procedure
required to complete its check via the registry's own interactive interface.

### Corrective second pass — trademark registries (2026-08-24)

A corrective pass was performed after an interrupted out-of-scope batch was
fully discarded (process incident at 2026-08-24T13:05:49Z, recorded in the
evidence directory as explicitly non-evidence). Every external request in
this pass was an individually issued, non-scripted ordinary public web page
request; no API/index/simple/JSON endpoint was contacted. Files `01`–`06`
are byte-identical to the original capture; new evidence files are
`07-reattempt-registries.txt` and `08-reattempt-namespaces.txt` (sha256
manifest regenerated; all entries verify OK). Each attempt carries exactly
one permitted class token (SUCCESS, NO-RESULT, BLOCKED, RATE-LIMITED,
MANUAL-ONLY); no availability or absence conclusion is drawn from any token.

| Check | Second-pass attempts (UTC) | Permitted outcome tokens | Evidence |
|---|---|---|---|
| EUIPO eSearch Plus / TMview | landing page 2026-08-24T13:09:09Z; TMview app page 2026-08-24T13:09:28Z | SUCCESS (landing fetch); BLOCKED (JS-only app); resulting lookup MANUAL-ONLY (unperformed) | `07-reattempt-registries.txt` |
| Polish Patent Office e-search | 2026-08-24T13:09:41Z | BLOCKED (bot-protection interstitial); resulting lookup MANUAL-ONLY (unperformed) | `07-reattempt-registries.txt` |
| WIPO Global Brand Database | 2026-08-24T13:09:51Z | BLOCKED (human-verification gate); resulting lookup MANUAL-ONLY (unperformed) | `07-reattempt-registries.txt` |
| USPTO trademark search | 2026-08-24T13:10:05Z | BLOCKED (JS-only shell); resulting lookup MANUAL-ONLY (unperformed) | `07-reattempt-registries.txt` |

### Corrective second pass — package registries and namespaces (2026-08-24)

Recorded in the same corrective pass as §Corrective second pass — trademark
registries above (individually issued, non-scripted ordinary public web page
requests only; no API/index/simple/JSON endpoint contacted). Search pages and
direct package/crate/project pages were both attempted where applicable.
Each attempt carries exactly one permitted class token; no availability or
absence conclusion is drawn from any token, including NO-RESULT.

| Platform / check | Second-pass attempts (UTC, public URL queried) | Permitted outcome tokens | Evidence |
|---|---|---|---|
| PyPI | search 2026-08-24T13:10:15Z (`https://pypi.org/search/?q=domowik`); direct project page 2026-08-24T13:14:14Z (`https://pypi.org/project/domowik/`) | BLOCKED ×2 (bot-protection client challenge); resulting lookup MANUAL-ONLY (unperformed) | `08-reattempt-namespaces.txt` |
| npm | search 2026-08-24T13:11:13Z (`https://www.npmjs.com/search?q=domowik`); direct package page 2026-08-24T13:15:25Z (`https://www.npmjs.com/package/domowik`) | BLOCKED ×2 (HTTP 403 bot protection); resulting lookup MANUAL-ONLY (unperformed) | `08-reattempt-namespaces.txt` |
| crates.io | search 2026-08-24T13:11:24Z (`https://crates.io/search?q=domowik`); direct crate page 2026-08-24T13:16:26Z (`https://crates.io/crates/domowik`) | BLOCKED ×2 (JavaScript-only application shell); resulting lookup MANUAL-ONLY (unperformed) | `08-reattempt-namespaces.txt` |
| Go modules / pkg.go.dev | 2026-08-24T13:11:32Z (`https://pkg.go.dev/search?q=domowik`) | NO-RESULT ("Showing 0 modules with matching packages." at access time; time-of-access observation only) | `08-reattempt-namespaces.txt` |
| Git host organization/account (GitHub) | direct namespace page 2026-08-24T13:11:44Z (`https://github.com/domowik`); repository search 2026-08-24T13:17:02Z (`https://github.com/search?q=domowik&type=repositories`) | NO-RESULT (direct page HTTP 404); SUCCESS (repository search observed the same four third-party repositories as dossier O8; no relationship, rights, or clearance established, claimed, or excluded) | `08-reattempt-namespaces.txt` |

No reservation, account creation, ID selection, or domain registration was
made by this project in this pass. Human reconfirmation of every namespace
observation through each platform's own interactive interface remains
outstanding before any release decision.

### Namespace observations

| Platform | Intended namespace / candidate | Check / evidence reference | Status |
|---|---|---|---|
| Git host organization/account (GitHub) | Candidate name not committed; to be chosen at first publication | Repository web search observed four third-party "domowik" repositories (matching dossier O8); direct `github.com/domowik` page fetch returned HTTP 404 — time-of-access observation only (`05-github-namespaces.txt`) | Not reserved; checked (observation-level); reservation forbidden pending separately established authority |
| Go modules / pkg.go.dev | Current provisional module path per Registry row above | Public search page observed "Showing 0 modules with matching packages." at access time (`06-package-registries.txt`) | Not published/reserved; checked (observation-level); module path changes with URL |
| PyPI | None intended today; evaluate only if a Python distribution channel is ever selected | Public search page blocked by client challenge; JSON API event recorded as restricted-access event only (`06-package-registries.txt`) | Not reserved; attempted — restricted; unperformed as a valid check |
| npm | None intended today; evaluate only if a JavaScript distribution channel is ever selected | Public search page returned HTTP 403 (bot protection); JSON API event recorded as restricted-access event only (`06-package-registries.txt`) | Not reserved; attempted — restricted; unperformed as a valid check |
| crates.io | None intended today; evaluate only if a Rust distribution channel is ever selected | Public search page rendered client-side with no result text retrievable (`06-package-registries.txt`) | Not reserved; attempted — unavailable; unperformed as a valid check |
| App-store publisher accounts and package/application IDs (Google Play et al.) | None; IDs remain Deferred-by-rule per Registry row above | No account created; no store query performed beyond dossier O1/O2 history | Not reserved; unperformed (forbidden pending authority); deferred-by-rule |
| Domains | No domain is recorded in the repository record as held, registered, or applied for by this project, and no registration attempt is recorded; dossier RDAP observations O4–O6 record third-party registrations of `domowik.dev`, `domowik.pl`, and `domowik.com` | Dossier observations O4–O6 stand unchanged | Not reserved; registration not attempted |

"No result", HTTP 404, HTTP 403/405, challenge pages, empty output, and
client-side-rendered pages are all recorded as time-of-access observations
or access restrictions. None is treated as proof that any name, identifier,
or namespace is absent, free, available, or safe to reserve, anywhere in
this file or the evidence directory.

## Migration alias mappings

Storage location for the explicit old→new mappings required before or when
any identity value changes. The table is empty today because nothing has
changed since adoption; it is defined now so a future migration has a
mandated home.

| Artifact | Old value | New value | Effective date | Notes (compatibility behavior, affected artifacts) |
|---|---|---|---|---|
| *(none yet)* | | | | |

Any post-publication change must add one row per affected artifact here
**before** executing the sweep below, and define compatibility behavior in
the format specification rather than assuming it.

## Change procedure (mechanical)

1. Update the affected row(s) in the Registry table; add migration alias
   mapping rows if anything was published.
2. Run the location sweep for every changed token across every token-bearing
   file type present in the tree (`*.md`, `*.go`, `go.mod`, `*.yml`, `*.sh`
   at minimum), excluding immutable historical records (see exclusions):
   - `grep -rn "domowik.dev" . \
       --exclude=NAMING-DUE-DILIGENCE.md --exclude-dir=.git \
       --exclude-dir=done --exclude-dir=dropped --exclude-dir=build`
   - `grep -rniE "\bdomowik\b" . \
       --exclude=NAMING-DUE-DILIGENCE.md --exclude-dir=.git \
       --exclude-dir=done --exclude-dir=dropped --exclude-dir=build`
   - `grep -rn "\.domo\b" . \
       --exclude=NAMING-DUE-DILIGENCE.md --exclude-dir=.git \
       --exclude-dir=done --exclude-dir=dropped --exclude-dir=build`
3. **Sweep exclusions (testable):** the following are exempt from update
   because they are preserved history, not active disposition: all of
   `docs/NAMING-DUE-DILIGENCE.md` including its dated observations and
   addendum; everything under tracker `done/` and `dropped/`; quoted
   historical text inside ADR 0006 §Superseded context; the issue 0009
   blocker-resolution history section; and, inside
   `EXECUTIVE-DECISIONS.md`, the preserved decision/reason text of superseded
   executive decisions — currently ED-0006 — which must remain verbatim.
   Only that entry's status metadata line is intentionally updated (to
   "Superseded by ED-0008"); the exemption does not cover it.
   Test this exemption with the entry-attribution pattern from
   §Deterministic location check, generalized to any token:

   ```sh
   awk '/^### ED-/{ed=$2" "$3} /'"$TOKEN"'/{print FILENAME" line "FNR" -> "ed}' \
     EXECUTIVE-DECISIONS.md
   ```

   Hits attributed to an entry whose status is `Superseded` are exempt and
   must be left verbatim; so are verbatim quoted executive inputs inside
   `Active` entries (they record what was said, not the current value).
   Every other hit — including pre-ledger preamble text and non-quoted
   decision text under `Active` entries — must be updated to match the new
   canonical value or point to this registry; ADR 0006, FORMAT-SPEC, and
   the README docs index must be updated in the same pass.
4. Re-run the deterministic verification in ADR 0006 §Verification and
   record results in the issue handoff.

## Deterministic location check

Instead of an order-dependent "first hit" claim, correctness is defined as a
set condition, testable after any change:

```sh
grep -rn "domowik.dev" . \
  --exclude=NAMING-DUE-DILIGENCE.md --exclude-dir=.git \
  --exclude-dir=done --exclude-dir=dropped --exclude-dir=build
```

Every hit must be in one of: this file (canonical definition),
`EXECUTIVE-DECISIONS.md` (see entry-attribution rule below),
`docs/decisions/0006-naming-and-identity.md` (decision record), `AGENTS.md`
(where the module-path clause marks the value provisional and points here),
`go.mod`, Go source files importing the module, and build/verification
scripts that reference the module path only as a derived comparison target —
these are registered derivation points of the provisional module-path row
("single derivation point in this row plus code scaffold"),
`docs/NAMING-DUE-DILIGENCE.md` (excluded immutable history), the issue 0009
blocker-resolution history section in the nested tracker (exempt preserved
history), or text that explicitly marks the value as temporary/provisional
and points to this registry. Any other hit is a defect.

**Entry-attribution rule for `EXECUTIVE-DECISIONS.md`:** hits there are valid
only inside the preserved decision/reason text of entry ED-0006 (whose status
metadata alone was deliberately changed to "Superseded by ED-0008"; that
one-line change is not history rewriting) and inside entry ED-0008. Attribute
each hit to its entry and test it mechanically:

```sh
awk '/^### ED-/{ed=$2} /domowik\.dev/{print FILENAME" line "FNR" -> "ed}' \
  EXECUTIVE-DECISIONS.md
```

Every output line must attribute to `ED-0006:` or `ED-0008:`. A hit under any
other entry is a defect.

Snapshot at integration (2026-08-23): the command above hits exactly this
file, `EXECUTIVE-DECISIONS.md`, `docs/decisions/0006-naming-and-identity.md`,
and — outside its exclusion — the nested tracker's
`issues/0009-naming-glossary.md` (exempt blocker-resolution history);
`docs/NAMING-DUE-DILIGENCE.md` is excluded by flag. Within
`EXECUTIVE-DECISIONS.md` the attribution rule reports 1 hit under ED-0006
(its preserved superseded text) and 3 under ED-0008. This snapshot is
indicative; re-run both commands to verify. Product-name occurrences span
README and most of `docs/`; the product name is not scheduled to change, and
the sweep commands are the authoritative enumeration if that ever happens.
