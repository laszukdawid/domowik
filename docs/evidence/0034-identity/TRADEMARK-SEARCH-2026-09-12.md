# Trademark search attempt — 2026-09-12

- **Authorised as:** read-only feasibility, official public interfaces only.
- **Not done:** no paid access, no registration, no filing, no reservation, no
  access control bypassed.
- **These are observations at a moment in time. They are NOT a clearance
  opinion, and nothing here establishes availability, rights, freedom to
  operate, or absence of conflict.** A clearance position requires qualified
  review the owner arranges.

## Feasibility: answered

An earlier note assumed these searches were human-only. **That was wrong.** One
of the four jurisdictions' data is reachable programmatically, and it carries
all four.

## What was queried

**TMview** (`https://www.tmdn.org/tmview/api/search/results`), the official
public multi-office interface operated through the EUIPN. POST, read-only, no
credentials, no challenge.

Coverage was **proved per office with positive controls**, not assumed:

| Office | Control query | Result | Coverage |
|---|---|---|---|
| `EM` EUIPO | `apple` | 763 | reachable |
| `US` USPTO | `apple` | 7,161 | reachable |
| `WO` WIPO / Madrid | `apple` | 491 | reachable |
| `PL` UPRP | `domownik` | returns `DOMOWNIK`, `tmOffice: PL`, `uprp.gov.pl` detail URL | reachable |

## Results for "domowik"

| Query variant | Offices | Result |
|---|---|---|
| `criteria: C` | all | **0** |
| `criteria: E` | all | **0** |
| `criteria: W` | all | **0** |
| `criteria: F` (fuzzy) | all | **41** |
| `criteria: C` | `EM` | 0 |
| `criteria: C` | `PL` | 0 |
| `criteria: C` | `US` | 0 |
| `criteria: C` | `WO` | 0 |

`criteria: S` returns HTTP 400 `PARAMETER_INCORRECT_FORMAT` and was not used.

**No exact match for "domowik" in any of the four target offices.**

### The fuzzy set is the part worth a reviewer's attention

41 hits, 33 distinct marks. The nearest neighbours are one letter away and sit
in the target jurisdictions:

- **`DOMOVIK`** — registered at **US**, **EM** (EUIPO), `ES`, `CN`, and as
  `domovik` at `UA`, plus Cyrillic **`домовик`** at `UA`. Same Slavic
  household-spirit figure as "Domowik", differing by one character.
- **`DOMOWNIK`** — several at **PL**, including `DOMOWNIK PODŁOGI I DRZWI`,
  `LEW DOMOWNIK`, `domownik BUDOWNICTWO MIESZKANIA WNĘTRZA`.
- `DOMOTIK` — many, across `AR DE EM ES FR IT MX PE BR`.
- `DOMOWIN` — `CN`, `FR`. `DOMÓWKI` — `PL`.

**No assessment of similarity, conflict or risk is offered here.** Whether
`DOMOVIK` at USPTO and EUIPO matters is exactly the judgement that needs
qualified review.

## Native interfaces: attempted, and what each returned

None of the four registries' own interfaces produced a result. Recorded so the
next pass does not repeat the attempts.

| Interface | Outcome | Why unperformed |
|---|---|---|
| USPTO TSDR API | **HTTP 401** — "you'll need to register for an API key" | Registration not authorised |
| USPTO trademark search UI | HTTP 200, Angular shell; no public query endpoint found | Not pursued |
| WIPO Global Brand Database | HTTP 200, **altcha proof-of-work challenge page** | An access control; not bypassed |
| UPRP e-wyszukiwarka | HTTP 200, **Incapsula bot-protection interstitial** | An access control; not bypassed |
| EUIPO eSearch plus | HTTP 200, JavaScript application shell | Not pursued; `EM` data covered via TMview |

## What was NOT checked

- **Nice classes.** No class filter was applied, so results span all classes.
  Classes 9 and 42 were not searched separately.
- **Status.** No status filter; results are not restricted to live registrations
  and may include lapsed or pending marks.
- **Transliterations and scripts** beyond what fuzzy matching surfaced.
- **Unregistered and common-law rights**, which appear in no registry.
- **Company registers, domain disputes, app-store listings** — outside this pass.
- **The four native interfaces**, per the table above. TMview aggregates their
  data but is not a substitute for an official register extract.
