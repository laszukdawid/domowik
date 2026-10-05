# Naming Due Diligence Dossier

Status (historical record): **Blocked — no clearance established.** This
blocked status and every Gate-G0-era disposition below were superseded on
2026-08-23 by executive decision ED-0008 (`EXECUTIVE-DECISIONS.md`) and
[ADR 0006](decisions/0006-naming-and-identity.md) (Accepted); the operative
disposition is now [NAMING.md](NAMING.md). The observations, URLs,
timestamps, and limitations in this dossier are preserved unchanged as
immutable risk history. See also the Post-dossier disposition addendum at
the end of this file.

This dossier records public, black-box observations about the project name
"Domowik" and related identifiers. It establishes **no** trademark clearance,
availability, ownership, affiliation, or non-affiliation. It is an engineering
risk-reduction input for issue 0009 and draft ADR 0006, not a legal opinion.

## Method and restrictions

- Sources: public website content, public app-store listings, public registry
  metadata (RDAP/WHOIS), public package-registry lookups, and web search
  result pages only.
- Prohibited and not performed: screenshots, APK download or inspection,
  decompilation, source-code access of third-party projects, automated bulk
  queries, or any non-public data.
- Every entry separates **observation** (what the source displayed) from
  **inference** (what we might tentatively conclude). Inferences are labeled
  and carry no decision weight.
- Access timestamps are UTC.

## Observation log

| # | Accessed (UTC) | Source / query | Observation | Type |
|---|---|---|---|---|
| O1 | 2026-08-23T04:00Z | Google Play listing `https://play.google.com/store/apps/details?id=pl.domowik.app` | App titled "Domowik: Home Organizer", developer listed as "Domowik Apps" / "ALEKS MICHAL ZROBCZYNSKI", Zabrze, Poland; support email domowik-app@proton.me; version 1.45.28, updated 2026-08-13; category Productivity. Description markets home inventory, warranties, budgets, lists. | Observation |
| O2 | 2026-08-23T04:00Z | Same listing | Android application ID shown in the listing URL is `pl.domowik.app`. | Observation |
| O3 | 2026-08-23T04:00Z | `https://domowik.dev` | Live marketing website for the same home-organizer app ("Domowik — cały dom w jednym miejscu"), multilingual (PL/EN/DE/FR/ES/UK/CS), links to Google Play, footer "© 2026 Domowik". | Observation |
| O4 | 2026-08-23T04:00Z | RDAP `https://rdap.org/domain/domowik.dev` | Domain registered 2026-07-29 via Porkbun LLC (.dev registry); registrant contact data redacted for privacy; client delete/transfer prohibited. | Observation |
| O5 | 2026-08-23T04:00Z | RDAP `https://rdap.verisign.com/com/v1/domain/domowik.com` | `domowik.com` registered 2004-10-06, registrar NameBright (TurnCommerce); current use not determined from this query. | Observation |
| O6 | 2026-08-23T04:00Z | RDAP `https://rdap.dns.pl/domain/domowik.pl` | `domowik.pl` registered 2024-03-04 to registrant organization "DOMOWIK SPÓŁKA Z OGRANICZONĄ ODPOWIEDZIALNOŚCIĄ", Wrocław, Poland. | Observation |
| O7 | 2026-08-23T04:01Z | Web search `"Domowik" spółka Wrocław domowik.pl`; results incl. `https://domowik.pl/`, `https://www.firma.egospodarka.pl/raporty-o-firmach/firma,19574884,domowik-sp-z-o-o.html`, `https://krs-pobierz.pl/domowik-sp-z-o-o-i0001172139` | Public business directories show (a) Domowik Sp. z o.o., Wrocław, KRS 0000618938, active since 2016, property rental management, operating `domowik.pl`; and (b) a separate DOMOWIK SP. Z O.O., Łódź, KRS 0001172139, registered 2025-05-12, accommodation activities. Two distinct Polish companies use "Domowik" in their registered names. | Observation |
| O8 | 2026-08-23T04:00Z | GitHub repository search `https://github.com/search?q=domowik&type=repositories` | Four repositories matching "domowik" by accounts with no established relationship to this project (kamilwolanski/Domowik, wxn0brP/domowik, dreadangel/domowik, Kraxio/Domowik-app). No relationship, rights, or clearance between these projects or their owners and this project is established, claimed, or excluded. | Observation |
| O9 | 2026-08-23T04:00Z | PyPI `https://pypi.org/pypi/domowik/json` → HTTP 404 | No PyPI package named `domowik` at access time. | Observation |
| O10 | 2026-08-23T04:00Z | npm `https://registry.npmjs.org/domowik` → HTTP 404 | No npm package named `domowik` at access time. | Observation |
| O11 | 2026-08-23T04:00Z | crates.io API `https://crates.io/api/v1/crates/domowik` | The initial automated API query was rejected under crates.io's data-access policy before any record lookup; a subsequent query returned "crate does not exist". Because automated access is policy-restricted here, **no absence may be inferred from either response**. Manual verification via `https://crates.io/search?q=domowik` is outstanding. | Observation + limitation |
| O12 | 2026-08-23T04:00Z | pkg.go.dev search `https://pkg.go.dev/search?q=domowik` | Zero published Go modules matched at access time. | Observation |
| O13 | 2026-08-23T04:00Z | IANA media-types CSV `https://www.iana.org/assignments/media-types/media-types.csv`, case-insensitive grep for `domo` | The CSV export was fetched and the grep produced no output at access time. This method is brittle (format changes, truncation) and **no absence of a registered media type may be inferred from it**. Manual verification against the registry's published assignment tables is outstanding. | Observation + limitation |
| O14 | 2026-08-23T04:02Z | Web search `".domo" file extension format` | No widely documented `.domo` file format found in the queries run. Results were dominated by Domo Inc. (a BI platform) whose tooling uses a `domo` CLI name and `domo/` directory convention, not an observed `.domo` file extension. Absence within these queries is not proof of absence elsewhere. | Observation + limitation |
| O15 | 2026-08-23T04:02Z | Web search `Domowik Android home organizer app`; result incl. `https://pl.linkedin.com/posts/mdmat_wspania%C5%82y-to-by%C5%82-weekend-nie-zapomn%C4%99-go-activity-7400951816705716224-xLt-` | Additional public uses of "Domowik": a LinkedIn post (2025-11-30) describing a hackathon elder-care assistant named "Domowik"; apps named "Domownik" (a different word) also appear in results and are **not** counted as matches. | Observation |

### Summary inference (non-decisional)

The word "Domowik" derives from Slavic folklore (a household spirit), which
this project's README also cites as its naming inspiration. Multiple third
parties visibly use the word, including an Android home-organizer app in the
same consumer-home domain. **Inference:** coexistence risk for consumer-facing
use of the bare word "Domowik" is real and cannot be dismissed. No
relationship, rights, priority, ownership, affiliation, or non-affiliation is
established between any observed party and this project, and none may be
inferred from this dossier. This inference does not determine rights,
priority, or confusion in any jurisdiction.

## Nine-artifact collision matrix

Each artifact is assessed independently. "Exposure" is an engineering risk
estimate, not a legal conclusion.

| Artifact | Proposed value | Known collisions | Exposure while unblocked | Disposition until gate G0 |
|---|---|---|---|---|
| A1 Product name | "Domowik" | Android app "Domowik: Home Organizer"; ≥2 Polish companies; hobby repos | High: direct category adjacency (consumer home software) | Proposal only; no ratification, marketing, store publication, or domain claims |
| A2 Binary name | `domowik` | Domo Inc.'s `domo` CLI is similar but distinct; no observed `domowik` binary conflict | Low–medium | Proposal only; rename mapping must cover it if Branch B is taken |
| A3 File extension | `.domo` | None found in queries run (O13, O14); similarity to Domo branding noted | Low | Proposal only; extension choice may survive independently of product-name outcome |
| A4 Application IDs | e.g. reverse-DNS derived from a Domowik domain | `pl.domowik.app` used by an existing Android organizer | High: ID conventions would embed domain names whose control by this project is not established | No app-ID may be selected while derived from contested names/domains |
| A5 Domains | none held by project (to be confirmed by orchestrator) | RDAP records list registrations of `domowik.dev` (2026), `domowik.pl` (2024), and `domowik.com` (2004) by registrants other than this project; no relationship, rights, or clearance is established for any party | High | Do not register, reference as ours, or build on any `domowik.*` domain |
| A6 MIME type | undefined; subname would likely derive from product/domain name | O13's limited query observed no `domo` match; registry absence is **not** established; manual verification outstanding | Low now; becomes coupled to A1/A5 at registration time | Defer per issue 0009 AC; strategy documented in ADR 0006 |
| A7 Go module path | undecided; conventional form needs a controlled domain | No modules matching "domowik" (O12) | Medium if domain-based; low if neutral | Domain-based path blocked; neutral paths remain candidates |
| A8 Repository/package namespaces | Git host org, store publisher accounts, registries | GitHub name variants in use by accounts with no established relationship to this project (O8); PyPI/npm/crates statuses per O9–O11, subject to registry access policies | Medium | Do not squat or reserve names under contested brand during block |
| A9 Migration aliases | n/a yet | n/a | n/a | If Branch B (rename) is approved, every row above needs an explicit old→new mapping including alias handling for early adopters |

> **Historical annotation:** the "Disposition until gate G0" column above is
> superseded and non-operative. Gate G0 was retired by ED-0008; the operative
> per-artifact disposition is [ADR 0006](decisions/0006-naming-and-identity.md)
> and [NAMING.md](NAMING.md). The observation columns remain valid history.

## Candidate disposition criteria

Any candidate identifier set (retained or renamed) must satisfy all of:

1. **Gate G0 satisfied:** see "Evidence needed to unblock".
2. **Per-artifact independence:** each of A1–A7 decided on its own merits; no
   bundled verdict.
3. **Domain derivability:** application IDs, module paths, and MIME subnames
   may only be derived from a domain/name whose control or authorization is
   documented.
4. **Namespace consistency:** repository, package, and publisher namespaces
   consistent across platforms before first public release.
5. **Migration completeness:** if any artifact changes post-publication, an
   exact old→new mapping exists covering binaries, extensions, IDs, MIME,
   module path, and documentation aliases.
6. **No legal conclusions embedded:** decisions record engineering facts and
   cite this dossier. No party may state that any identifier is legally
   clear, cleared, or free of conflict — not even when Branch A
   authorization exists. Authorization establishes only its documented
   permitted scope (relationship, artifacts, territories, channels, wording,
   expiry) and nothing beyond it.

## Evidence needed to unblock

Exactly one branch must produce evidence meeting these requirements:

### Branch A: retain identifiers

Written confirmation of either a confirmed relationship with, or explicit
authorization from, the relevant rights holder(s). The evidence must specify:

- **Authorization source:** who grants it (legal name/entity) and on whose
  behalf;
- **Exact artifacts covered:** which of A1–A8 are authorized (e.g., product
  name, domains, app-ID namespace);
- **Scope and territories:** geographic and market scope;
- **Channels:** permitted distribution/marketing channels;
- **Wording:** what the project may publicly state about the relationship
  (if any);
- **Expiry:** duration or renewal conditions;
- plus completion of the manual registry checks below where applicable.

Ownership alone — e.g., proof that someone owns a trademark or domain — never
unblocks this project without one of the above.

### Branch B: executive-approved rename

An executive decision record containing the exact migration mapping for every
artifact row A1–A8 (new values chosen per the criteria above), plus updated
issue 0009 acceptance criteria.

### Manual checks outstanding (both branches)

These could not be completed programmatically and must be done by a human via
their public web interfaces, recorded as timestamped text-only public result
references (URL, query, exact result text or counts, access date and
timezone) filed outside the repository. No screenshots are required or
expected:

- EUIPO eSearch Plus (`https://www.euipo.europa.eu/en/trade-marks`) — word
  marks containing "domowik", EU;
- Polish Patent Office e-search (`https://ewyszukiwarka.pue.uprp.gov.pl`) —
  PL trademarks;
- WIPO Global Brand Database (`https://branddb.wipo.int`);
- USPTO trademark search (`https://tmsearch.uspto.gov`) if US release is
  contemplated.

An attempted programmatic TMview query returned no usable response on
2026-08-23T04:03Z and was abandoned per the method restrictions.

## Limitations

- Registry RDAP data shows registrars and redacted contacts; it proves
  registration events, not rights or use.
- Package-registry and IANA responses (including 404s, policy-restricted
  API responses, and empty grep output) are time-of-access signals only;
  absence is not inferred from any of them where the method was restricted
  or brittle (O11, O13).
- Search-engine absence of a `.domo` extension is weak negative evidence.
- Store listings can change or be removed after the access timestamp.
- No trademark, priority, likelihood-of-confusion, ownership, affiliation,
  or infringement analysis has been attempted, and none may be inferred
  from this document. No relationship between any observed party and this
  project is established.

## Post-dossier disposition (2026-08-23 addendum)

Executive input recorded verbatim as ED-0008 (`EXECUTIVE-DECISIONS.md`),
explicitly superseding ED-0006, superseded the naming blocker on the day
this dossier was written: `https://domowik.dev` was adopted as an explicitly
temporary project URL — expected to change — and derived identifiers were
ratified provisionally via [ADR 0006](decisions/0006-naming-and-identity.md),
with all values tracked in [NAMING.md](NAMING.md) for mechanical future
replacement.

The original observations, URLs, access timestamps, limitations, and matrix
data above are preserved unaltered. Two clearly labeled supersession
annotations were added after this dossier was written — the historical status
note at the top and the historical annotation beneath the artifact matrix —
and this addendum was appended. They alter no recorded observation. In
particular, O4 (the
`domowik.dev` RDAP registration by a registrant other than this project)
stands; adoption of the string is internal and provisional only. This
addendum, the historical status annotation at the top of this file, and the
decisions they reference establish no legal clearance, availability,
ownership, affiliation, non-affiliation, or non-infringement. The manual
registry checks in "Evidence needed to unblock" were converted by ADR 0006
into pre-publication release-gate requirements rather than executed.
