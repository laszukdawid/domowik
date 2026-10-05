# Issue 0034 — identity preparation and proposal

- **Date:** 2026-09-12
- **Author:** `domo-tracker`, preparation only
- **Status:** proposal for the owner. **Nothing here is a decision.**

This gathers current public evidence and proposes concrete values so the owner
can settle identity in one pass. **No namespace was reserved, no registration
attempted, nothing published, no tag created, and no claim of legal clearance,
availability, ownership, affiliation or non-infringement is made or implied.**
Every check below is a public read-only observation at the stated time.

## The finding that changes the framing

**The repository exists and is PRIVATE.**

```
$ gh repo view laszukdawid/domowik --json nameWithOwner,visibility,isPrivate,createdAt,defaultBranchRef
{"createdAt":"2026-08-26T14:20:48Z","defaultBranchRef":{"name":"main"},
 "isPrivate":true,"nameWithOwner":"laszukdawid/domowik",
 "url":"https://github.com/laszukdawid/domowik","visibility":"PRIVATE"}
```

**Corrected 2026-09-12 after root review.** An earlier version of this section
drew more from the private status than the evidence supports. Withdrawn: that a
private repository cannot serve as a canonical URL, and that documentation links
depend on visibility. Neither holds. The URL is stable and usable by authorised
collaborators regardless of visibility, and relative links render the same in
public and private GitHub repositories.

What the private status actually bears on, narrowly:

- **Public distribution.** A public release needs public access. That is ADR
  0006's existing release gate, not a new dependency.
- **Nothing in the code may claim the URL**, which is what the ED-0095 note on
  0034 already says, and is unchanged.

The `issues/README.md` link gate is unchanged and unrelated: relative links are
replaced with stable URLs **before the tracker is published independently**.

**Repository visibility remains a decision only the owner can take.**

## A methodological warning, proven in this pass

**An unauthenticated 404 cannot distinguish "does not exist" from "exists but
is private."** This is not theoretical. The GitHub API returned 404 for
`laszukdawid/domowik` — a repository that demonstrably exists and that this
very worktree is checked out from:

```
$ curl -s -o /dev/null -w "%{http_code}" https://api.github.com/repos/laszukdawid/domowik
404
```

`docs/NAMING-DUE-DILIGENCE.md` already forbids inferring absence from
restricted or failed queries. The same caution applies to every `NO-RESULT`
row in the 2026-08-24 pass, and to the GitHub organisation observation below.

## Public read-only observations — 2026-09-12

All timestamps UTC. Commands are reproducible; bodies are quoted verbatim.

| Target | URL | Time | HTTP | Body / result |
|---|---|---|---:|---|
| crates.io | `https://crates.io/api/v1/crates/domowik` | 18:46:00Z | 404 | ``{"errors":[{"detail":"crate `domowik` does not exist"}]}`` |
| PyPI | `https://pypi.org/pypi/domowik/json` | 18:46:00Z | 404 | `{"message": "Not Found"}` |
| npm | `https://registry.npmjs.org/domowik` | 18:46:00Z | 404 | `{"error":"Not found"}` |
| GitHub repo | `gh repo view laszukdawid/domowik` | 18:46:11Z | 200 | exists, **PRIVATE**, created 2026-08-26 |
| GitHub account `domowik` | `https://api.github.com/users/domowik` | 18:46:40Z | 404 | `{"message":"Not Found"}` |
| pkg.go.dev | `https://pkg.go.dev/domowik.dev/domowik` | 18:46:40Z | 404 | module not published |
| `domowik.dev` | `https://domowik.dev` | 18:46:11Z | **200** | live third-party site |
| `domowik.pl` | `https://domowik.pl` | 18:46:11Z | **200** | live third-party site |
| `domowik.com` | `https://domowik.com` | 18:46:11Z | — | connection timed out after 15 s |

**What these do and do not establish.** The three package-registry rows are
materially better evidence than the 2026-08-24 pass obtained, which was BLOCKED
on all three by bot protection and recorded as MANUAL-ONLY and unperformed.
These are unblocked machine-readable API responses, and crates.io states
non-existence in words rather than by status code alone. They remain
**time-of-access observations that the name is not currently published**. They
are not availability, not clearance, and not a guarantee the name will still be
unpublished tomorrow. A registry may also hold a name in reserve without
serving it.

`domowik.com` timing out is **not** evidence of non-registration. It is one
failed request.

## Proposal

Six artifacts. Each is independent; deciding one decides nothing else.

### 1. Repository visibility — the gating decision

| Option | Consequence |
|---|---|
| **Make `laszukdawid/domowik` public** | Unblocks stable links, 0018's release, and the URL rows. Publishes the clean-room record, the specifications and this tracker, which is a disclosure decision in itself |
| Keep private | 0018 and the tracker link replacement stay blocked. Everything else below can still be recorded as intent |

**Recommendation: owner's call, and it should be taken first**, because five of
the six artifacts below derive from it.

### 2. Canonical URL

| Candidate | Assessment |
|---|---|
| `https://github.com/laszukdawid/domowik` | The only URL the project actually controls. Requires option 1. **Recommended for v0.1** |
| `https://domowik.dev` | **Not viable.** Registered to a third party and serving a live site today (HTTP 200). ED-0008 adopted it as explicitly temporary and expected to change; that expectation has now come due |
| A newly registered domain | Cleanest long-term and the only path that yields a good application-ID base (see 4). Costs money and a registration the owner must make |

ED-0008's requirement — "make sure that it's easy to find and modify" — is
already satisfied structurally: `docs/NAMING.md` is the single derivation point
and ADR 0006 records the change procedure.

### 3. Go module path — `domowik.dev/domowik`

This is the most concrete hazard on the list. Go resolves a module path **by
fetching it over HTTP**, and `domowik.dev` is a live site under someone else's
control. The path should not survive in its current form.

Two routes, and they interact with issue 0062:

- **0062 removes `go.mod` entirely** as part of retiring the Go toolchain. If
  that lands first, the hazard resolves by deletion — but the NAMING.md row
  must then record the module path as RETIRED, not silently vanish.
- **If `go.mod` outlives 0062 for any reason**, change the path to derive from
  whichever canonical URL option 2 selects.

Either way this is a sequencing note for root rather than a separate decision.

### 4. Application IDs — recommend continued deferral, with a reason that is new

ADR 0006 defers these by rule, deriving them "at store-publication time from
whichever URL/domain is canonical then".

**Corrected 2026-09-12.** An earlier version said both major stores discourage a
forge-derived reverse-DNS ID. **No current platform rule was checked and none is
cited**, so that claim is withdrawn. Application IDs are opaque unique
identifiers; store uniqueness is enforced through the developer account, not DNS
ownership.

What stands is a preference with a reason, not a constraint: an ID under a
namespace the project controls is easier to transfer and does not depend on a
personal account name. The dossier separately observes a third-party Android app
published as `pl.domowik.app`, which establishes nothing about rights.

ED-0099 makes macOS a local-development target, so no store publication is
imminent. **Recommendation: keep Deferred-by-rule.**

### 5. Package namespaces

`docs/NAMING.md` predates the Rust rewrite and carries no crates.io intent row.

- **crates.io** is now the registry that matters. Decide whether Domowik
  publishes crates at all — a CLI can ship as binary artifacts without ever
  publishing to crates.io, and issue 0018 does not require it.
- **PyPI and npm** have no intended distribution channel. Recommend recording
  "none intended" rather than reserving defensively.

### 6. MIME strategy — no change

Already recorded as continued deferral with rationale and a revisit trigger at
first public binary release. This is the one acceptance criterion already
ticked, and nothing observed today changes it.

## Exact remaining actions

**Owner decisions** (five, and only the first is blocking):

1. Repository public or private.
2. Canonical URL.
3. Whether Domowik publishes to crates.io.
4. Confirm application IDs stay deferred, with the controlled-domain condition.
5. Whether to register a domain the project controls.

**Not attempted in this pass, and feasibility untested.** An earlier version
called these human-only; that was an assumption and is withdrawn. **None of the
four was attempted here** — this pass covered package registries and domains
only. Whether they can be queried programmatically is unknown until tried.
Separately, and regardless of who runs a search: a clearance determination is a
legal judgement requiring qualified review the owner arranges, and no search
result is a clearance opinion. All four remain outstanding:

6. EUIPO eSearch Plus.
7. Polish Patent Office e-search.
8. WIPO Global Brand Database.
9. USPTO, if applicable.

Each needs a dated text-only result filed outside the repository, per the
existing evidence-manifest convention.

**Repository corrections** this project can make once 1 and 2 are answered:

10. `docs/NAMING.md` rows 31 and 33 say no repository namespace is reserved on
    any platform. A repository now exists, privately. The registry describes
    itself as canonical, so it should say what is true — including that the
    observation is of a private repository.
11. Record the 2026-09-12 registry observations in the NAMING.md namespace
    table, replacing three BLOCKED/unperformed rows with unblocked results.
12. Retire or re-derive the Go module path per 3.
13. Replace relative tracker links with stable URLs, on the existing
    `issues/README.md` trigger — before the tracker is published independently.
    Not gated on this repository's visibility.

## Acceptance criteria status

| # | Criterion | State |
|---|---|---|
| 1 | Dated text-only results for each registry check, outside the repository | **Not met.** The four trademark searches were not attempted. The package-registry half improved materially today |
| 2 | Application IDs recorded in ADR 0006 and NAMING.md | **Not met**, and deliberately so — Deferred-by-rule |
| 3 | MIME strategy recorded | **Met**, unchanged |
| 4 | Namespace reservations listed per platform in NAMING.md | **Not met.** Nothing is reserved, and nothing may be reserved without the owner |

No criterion is ticked by this document.
