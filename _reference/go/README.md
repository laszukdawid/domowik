# Go reference implementation (frozen)

This directory holds the **frozen Go core** of Domowik. It is reference
material for the Rust rewrite, not a live build target.

## Why the leading underscore

The parent directory is named `_reference`, and **the underscore is
load-bearing**. The Go toolchain ignores any path element beginning with `_`
or `.`, so this tree is invisible to `go build ./...`, `go test ./...`,
`go vet ./...` and `gofmt -l .`. Renaming the directory without an underscore
would silently pull ~32k lines of frozen code back into every verification
run. Do not rename it.

## What is here

Moved wholesale from the module root on 2026-08-30, with full history
preserved (`git mv`, so `git log --follow` works):

| Package | Lines | Rewritten as |
|---------|-------|--------------|
| `internal/persist` | ~6,200 | `crates/domowik-container` |
| `internal/document` | ~6,000 | `crates/domowik-document` |
| `internal/geometry` (+ `predicates`) | ~6,100 | `crates/domowik-geometry` |
| `internal/cli` | ~2,700 | `crates/domowik-cli` |
| `internal/assets` | ~1,900 | `crates/domowik-container` (asset store) |
| `internal/migration` | — | `crates/domowik-format` |
| `internal/sizecorpus` | — | port TBD |
| `cmd/domowik` | — | `crates/domowik-cli` binary |

Roughly 32k lines of implementation and 26k lines of tests.

## What is NOT here, and why

These stayed live in Go because they are **build and release infrastructure**,
not core, and ED-0087 R2 keeps them in Go until a separate issue ports them:

- `cmd/domowik-sbom` + `internal/release` — release evidence, SPDX, `NOTICE`
- `cmd/domowik-jsoncheck` — JSON fence validation in docs
- `cmd/domowik-tracker-check` — issue tracker consistency
- `internal/corpusgen` — conformance corpus generation
- `internal/evidence/phase0` — recorded evidence

**Update, 2026-09-06 (issue 0062).** The list above records the position at
the 2026-08-31 freeze and is kept as written. Four of those five have since
moved, and only `internal/corpusgen` is still live Go:

| Then | Now |
|---|---|
| `cmd/domowik-sbom` + `internal/release` | frozen here, in this tree, with `release/` |
| `cmd/domowik-jsoncheck` | ported to `crates/domowik-devtools` |
| `cmd/domowik-tracker-check` | ported to `crates/domowik-devtools` |
| `internal/corpusgen` | **still Go** — the only Go package left in the active tree |
| `internal/evidence/phase0` | deleted; nothing referenced it |

The freeze on THIS tree is unchanged: nothing here is co-maintained, and the
"read it, do not copy it mechanically" rule below still governs every use of
it.

## How to use it

**Read it; do not copy it mechanically.** The Rust core is built against the
SPECIFICATIONS (`docs/FORMAT-SPEC.md`, `docs/BEHAVIOR-SPEC.md`, the §9.3
limits, the §9.4 diagnostic taxonomy) — the specs are normative, this code is
not. Where the Go code and the spec disagree, **the spec wins** and the
disagreement is a bug worth recording.

This code carries four known conformance defects found on 2026-08-30 that must
NOT be reproduced in the port: the L12 fixture sits on the boundary rather than
over it, L19 fixtures test format rather than overlength, L4 has corpus
defects, and L10 has no guard at all.

It also carries the performance defect that motivated the rewrite: the failing
predicates call `big.Rat` unconditionally instead of using an adaptive
floating-point filter. See `docs/decisions/0008-rust-core-implementation.md`.

To build or test it, enter the directory and use its own package paths — it is
still valid Go, just not part of the module's default package pattern.
