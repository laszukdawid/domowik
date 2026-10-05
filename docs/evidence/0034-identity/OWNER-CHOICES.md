# Issue 0034 — owner choice sheet

- **Date:** 2026-09-12
- **For:** the project owner. Six choices, and nobody else may make them.
- **Evidence:** [PREPARATION-2026-09-12.md](PREPARATION-2026-09-12.md)

Nothing here is decided, recommended as settled, reserved, registered,
published or tagged. No claim of clearance, availability or ownership is made.

## The six choices

### 1. Does `laszukdawid/domowik` become public?

It is **private** today (verified 2026-09-12, created 2026-08-26).

**Narrower than an earlier draft of this sheet claimed.** Visibility does not
decide the canonical URL: `https://github.com/laszukdawid/domowik` is stable and
usable by authorised collaborators whether or not the repository is public. Nor
does it decide the documentation links — relative links render correctly in
public and private GitHub repositories alike. The link question belongs to the
project's own gate in `issues/README.md`: relative links are replaced with
stable URLs **before the tracker is published independently**, which is a
different event from this repository becoming public.

What visibility actually governs is public distribution.

| Option | Consequence |
|---|---|
| Public now | Satisfies the access half of a public release. Discloses the clean-room record, the specifications, this tracker and every audit in it — a disclosure decision in its own right |
| Public later, at release | Everything below can still be decided and built; only public distribution waits. Costs nothing today |
| Stay private | No public release. Internal development, CI and authorised collaboration are unaffected |

### 2. What is the canonical URL?

| Option | Consequence |
|---|---|
| `https://github.com/laszukdawid/domowik` | The only URL the project controls today. Stable now; public access is choice 1. Ties identity to a forge path — see choice 4 |
| A domain the project registers | Only option that yields a clean app ID and a stable identity across forges. Costs a registration (choice 5) |
| Keep `https://domowik.dev` | **Not available.** Registered to a third party and serving a live site (HTTP 200, 2026-09-12). ED-0008 adopted it as explicitly temporary; that has come due |

Whatever is chosen, `docs/NAMING.md` remains the single derivation point, so a
later change stays cheap — which is what ED-0008 asked for.

### 3. Does Domowik publish to crates.io?

`domowik` is unpublished there as of 2026-09-12 (crates.io answered `crate
"domowik" does not exist`). That is a time-of-access observation, not
availability.

| Option | Consequence |
|---|---|
| Publish crates | Needs a reserved name and an ongoing publication process. Makes the core consumable as a library |
| Ship binaries only | Nothing to reserve, nothing to maintain. 0018 does not require crates.io |
| Defer | Costs nothing now; the name may not be free later |

### 4. Application IDs — confirm continued deferral?

Deferred-by-rule under ADR 0006, derived "at store-publication time from
whichever URL/domain is canonical then".

**Correcting an overreach in an earlier draft.** It said a forge-derived
reverse-DNS ID is discouraged by both major stores. **No current platform rule
was checked, and none is cited here.** Application IDs are opaque unique
identifiers; store uniqueness is enforced by the developer account, not by DNS
ownership. Treat the following as a **recommendation with a reason, not a
technical constraint**:

> Prefer an ID derived from a namespace the project controls. A
> `com.github.<account>.*` ID reads as belonging to GitHub and to one personal
> account, which makes it awkward to transfer and impossible to defend if the
> account name changes. That is a durability argument, not a rule.

The dossier separately records a third-party Android app already published as
`pl.domowik.app` — an observation, establishing nothing about rights.

| Option | Consequence |
|---|---|
| Keep deferred | Nothing is forced. ED-0099 makes macOS a local-development target, so no store publication is imminent |
| Keep deferred, and add the controlled-namespace preference as a recorded recommendation | Same, with the reasoning captured so it is not re-derived later |
| Select IDs now | Possible with any canonical URL, including a forge path. Verify the then-current store rules first — this sheet did not |

### 5. Register a project-controlled domain?

| Option | Consequence |
|---|---|
| Yes | Resolves choices 2 and 4 cleanly and permanently. Costs money and a registration only the owner can make |
| No | Identity stays tied to a forge path; app IDs stay deferred indefinitely |

`domowik.dev` and `domowik.pl` both serve live third-party sites.
`domowik.com` did not respond to one request, which is **not** evidence about
its registration.

### 6. When are the four trademark checks performed, and by whom?

EUIPO eSearch Plus, Polish Patent Office e-search, WIPO Global Brand Database,
and USPTO if applicable.

**What was actually attempted, stated exactly.** **None of the four.** This
pass attempted package registries and domains only — crates.io, PyPI, npm, the
GitHub API, pkg.go.dev and three domain endpoints, all recorded in
[PREPARATION-2026-09-12.md](PREPARATION-2026-09-12.md). An earlier draft called
the trademark checks "human-only"; that was an assumption, not a finding, and
it is withdrawn. Whether any of the four can be queried programmatically is
**untested**.

Two separable things, which that assumption ran together:

1. **Running a search.** Feasibility unknown until attempted. If the owner
   wants it attempted, say so and it will be, read-only.
2. **Deciding what a result means.** A clearance determination is a legal
   judgement requiring qualified review the owner arranges. No agent output,
   and no search result, is a clearance opinion. `docs/NAMING-DUE-DILIGENCE.md`
   already says this in terms.

| Option | Consequence |
|---|---|
| Attempt the searches now, read-only | Produces dated observations for criterion 1. Does **not** produce clearance |
| Defer to the release gate | The order ADR 0006's release gate assumes |
| Arrange qualified review | The only route to a clearance position, whenever the owner wants one |

## What can proceed with no publication decision

These need no URL, no visibility change and no reservation. They are the
majority of the remaining release work.

| Task | Issue | Note |
|---|---|---|
| SBOM generation over the source tree and cross-built binaries | 0036 | Buildable and testable now; only inclusion in a public release is gated |
| Licence scanning, fail-closed | 0036 | Already enforced; extending it needs nothing external |
| `NOTICE` generation | 0036 | Already generated across the dependency graph |
| A Rust release process and cross-platform artefacts | 0018 | Build, sign locally, verify reproducibility. Do not publish |
| Restating 0036's and 0024's Go-era criteria against the Rust product | 0036, 0024 | Paperwork, no identity input |
| Retiring the Go toolchain | 0062 | Removes `go.mod`, which settles the `domowik.dev/domowik` module path by deletion |
| CI cross-build and packaging coverage | 0061 | Independent |
| A migration framework, tested against synthetic predecessors | 0031 | The *obligation* starts at first release; the code need not wait |

## What cannot proceed without a decision

| Blocked | Needs |
|---|---|
| Public distribution of a release | Choice 1, plus ADR 0006's release gate |
| Any crates.io reservation or publish | Choice 3 |
| Store publication | Choice 4, and a developer account the owner holds |
| 0034 acceptance criterion 1 | The four searches performed and dated, then qualified review for any clearance claim |
| 0034 acceptance criterion 4 | Choice 3, then reservations only the owner may make |

**Not blocked, contrary to an earlier draft:** replacing relative documentation
links. That is governed by `issues/README.md`'s own gate — before the tracker is
published independently — and not by this repository's visibility. The original
gate is unchanged.

## The `go.mod` hazard, flagged rather than decided

`go.mod` still declares `module domowik.dev/domowik`, and Go resolves module
paths over HTTP against a domain the project does not control. Issue 0062
removes `go.mod` entirely, which settles this by deletion. If 0062 lands first,
`docs/NAMING.md` should record the module path as **retired** rather than let
the row quietly disappear. No decision needed; a sequencing note for root.
