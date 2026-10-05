# ADR 0008: Rust core implementation

- **Status:** Accepted
- **Date:** 2026-08-31
- **Supersedes:** ADR 0002

## Context

ADR 0002 proposed a Go core with CLI-first delivery, subject to validation gates. Its revision trigger 3 fired on issue 0030's Phase 0 performance criterion: two predicates missed the 10,000 ns/op bound. On 2026-08-31, the project owner decided the core moves to Rust, full-core scope.

## Evidence the decision was taken against

Per ED-0073 and its amendments, measured on Apple M4 Max, darwin/arm64, Go 1.27:

**Evidence FOR the trigger:**
- `BenchmarkIntersectSegmentsRobust`: 12,134 ns/op, 14,765 B/op, 426 allocs/op
- `BenchmarkOffsetCertificateAndVertex`: 20,458 ns/op, 21,396 B/op, 538 allocs/op
- Both miss the 10,000 ns/op bound by 2×
- math/big is 72-95% of allocated objects
- The failing predicates have NO adaptive filter; they call big.Rat unconditionally

**Counter-evidence (recorded honestly per kickoff requirement):**
- `BenchmarkOrientationFiltered/easy`: **5.479 ns/op, 0 B/op, 0 allocs/op**
- The adaptive filter escalates 0% on non-adversarial inputs, 90-100% on adversarial
- The filtered and unfiltered paths are ~2,000× apart, same package, same run
- The orchestrator's assessment: the bound was likely reachable in Go via applying the existing filter pattern to the two failing predicates
- No filtered `IntersectSegments` was measured; the expectation is strong inference, not evidence

**The owner decided to move to Rust knowing the counter-evidence.** An ADR presenting only the case for its own decision is advocacy, not a decision record.

## Decision

Implement the core in Rust: document model, persistence, geometry, and CLI. The Go implementation is retired when Rust reaches feature parity.

## Rationale

Rust offers:
- No garbage collection
- SIMD intrinsics and precise layout control
- Mature geometry ecosystem (geo, parry, robust, spade — each subject to clean-room and licensing review)
- Const generics and custom allocators

The trade-offs:
- Longer compile times (accepted)
- Loss of Go team experience (accepted)
- Flutter bridge story changes materially (ADR 0003 re-assessed separately)

The 5.479 ns filtered path demonstrates Go CAN be fast with adaptive predicates. The decision reflects a judgment that Rust's ecosystem and control are worth the migration cost, not that Go cannot meet the bound.

## Validation gates

This decision becomes final only if:

1. **Rust workspace builds and verifies** with substantive tests per crate, reproducible builds, and named verification-stage failures.
2. **Third-party audit complete**: every adopted crate has licence, provenance, and notice recorded per docs/THIRD-PARTY-POLICY.md. Geometry crates receive explicit clean-room + license review before use.
3. **Flutter bridge spike** (separate from ADR 0002's Go spike): demonstrates Rust cdylib on Linux and Android with panic containment, callback delivery, cancellation, ownership, and measured binary size/startup. iOS remains unsupported until separately validated.
4. **Format conformance preserved**: the 38-package corpus with machine-readable expectations continues to pass. Ported implementation built AGAINST FORMAT-SPEC, BEHAVIOR-SPEC, not against Go code.

## Failure triggers

- Rust geometry ecosystem proves inadequate (licensing, clean-room violations, missing primitives), forcing custom stack maintenance comparable to Go's burden.
- Build times or IDE experience degrade developer productivity beyond accepted trade-off.
- Flutter bridge requires duplicating domain logic in Dart (same trigger as ADR 0002).
- Team cannot achieve proficiency within reasonable ramp-up period.

If triggered, reassess: continue with Rust and accept the constraint, split only the geometry kernel behind a C ABI, or revert to filtered-Go implementation with the 5.479 ns evidence as the baseline.

## Consequences

**What survives the migration:**
- FORMAT-SPEC, BEHAVIOR-SPEC, ASSET-STRATEGY
- §9.3 limits table, §9.4 diagnostic taxonomy, §9.5 precedence
- 38-package conformance corpus with machine-readable expectations
- Every ruling made (F1 zero prohibition, StageSchema placement, L3 unreachability, partial-history labelling)
- Executive decision record
- Four carried-forward corpus defects (L12, L19, L4, L10 — documented separately)

**What does not survive:**
- Go implementation: internal/document, internal/persist, internal/geometry, internal/cli, cmd/*
- Findings about Go's allocation behaviour (not problem properties)
- Go-specific tooling (domowik-sbom, domowik-jsoncheck, domowik-tracker-check) — retained as build utilities during migration, reimplementation is a separate decision

**Go tree disposition (SUPERSEDES orchestrator ruling R1):**

**Archive-retirement amendment, 2026-10-05.** The owner removed `_reference/`
from the source tree. Earlier committed Go sources and release records remain
in Git history. The freeze and retention decisions below are historical,
not instructions to restore or maintain that directory. Current implementation
work uses the reviewed specifications; historical code is not normative.

R1 held that the Go tree would keep verifying on `main` until Rust reached
parity. The owner superseded that on 2026-08-30, directing a full Rust rewrite
with the Go code kept "in some separate dir for references". The Go core is
therefore FROZEN, not co-maintained:

- Moved wholesale to `_reference/go/` with history preserved. The leading
  underscore is load-bearing — the Go toolchain ignores `_`-prefixed paths, so
  the frozen tree is invisible to `go build ./...` and `gofmt -l .`.
- `internal/release`, `cmd/domowik-sbom` and `release/` were frozen WITH it.
  They describe a Go product — two linux binaries from `./cmd/domowik` — and
  the manifest enforces `artifacts must not be empty`. With no Go artifact to
  describe, the alternatives were to point the manifest at a build tool (a lie)
  or relax a fail-closed guard. Freezing the machinery with its subject was the
  honest third option.
- Still live in Go: `cmd/domowik-jsoncheck`, `cmd/domowik-tracker-check`,
  `internal/corpusgen`, `internal/evidence/phase0` — repo-hygiene tools with no
  dependency on the product.

**Amendment, 2026-09-06 (issue 0062).** The last bullet above is no longer
true and is kept rather than rewritten, because it records what was decided on
2026-08-31. The disposition it left open — "reimplementation is a separate
decision" — has now been taken for three of the four:

- `cmd/domowik-jsoncheck` and `cmd/domowik-tracker-check` were PORTED to
  `crates/domowik-devtools`, with no new dependency, after their behaviour was
  proven identical to the Go originals on every success and failure path.
- `internal/evidence/phase0` was DELETED. Nothing referenced it but this ADR's
  own list; its output already lives under `docs/evidence/`.
- `internal/corpusgen` REMAINS in Go, and is now the only Go package in the
  active tree. Its hand-rolled ZIP writer emits deliberately malformed
  archives, which the `zip` crate cannot; whether to port or freeze it is
  issue 0062's open question and the reason `go.mod` still exists.

**Amendment 2, 2026-10-04 (issue 0062, ED-0104).** The corpus generator was
ported to `crates/domowik-devtools/src/corpusgen`, preserving byte identity
for all 39 generated artifacts and the corpus gates. The root Go module,
active Go package, verification stages and CI setup are retired together;
Go is no longer a development requirement. `_reference/go/` remains frozen.

The generator uses two pinned DEFLATE streams with recorded provenance and
digests, checked by inflation. It does not recompress: payload-to-compressor
regeneration was intentionally traded for semantic expansion checks, as
recorded in ED-0104 and the issue-0062 parity evidence.

Nothing about the frozen `_reference/go/` tree changed. `internal/release`,
`cmd/domowik-sbom` and `release/` stay frozen with it, and the Rust
release/SBOM obligation recorded below is unaffected.

**Obligations this creates, recorded so they are not lost:**

1. **A Rust release/SBOM process is owed.** Apache-2.0 distribution and
   `docs/THIRD-PARTY-POLICY.md` still bind. The interim guard is
   `scripts/rust-deps-inventory.py`, regenerated and diffed by `verify.sh`, so
   a new dependency fails the build until recorded. It reports DECLARED
   licences only — it is not a licence audit, does not read LICENSE files, and
   does not build `NOTICE`.
2. **CLI stream/exit-code probes P1-P3 are withdrawn, not retired.** They
   asserted exit-2 discipline and stream separation against `./cmd/domowik`.
   Reinstating them against the Rust binary is an acceptance criterion of the
   CLI port.
3. **Cross-compilation coverage lapsed** with those probes and must return.

## Performance posture

Performance is the owner's stated key requirement, so it is enforced rather
than asserted:

- `[profile.release]` sets `lto = "fat"` and `codegen-units = 1`. This is not
  cargo-culting: the filter path is a few floating-point operations that must
  inline ACROSS crate boundaries to reach single-digit nanoseconds.
- `panic = "unwind"` is deliberately retained. `abort` would be smaller and
  faster, but the FFI boundary must transport panics rather than kill the host
  process. Revisit only with the FFI decision, never as a lone perf tweak.
- A criterion harness exists in `domowik-geometry` from its first commit,
  before any predicate, and `verify.sh` COMPILES benchmarks every run so the
  measurement path cannot rot. Timings are not a pass/fail gate — they are
  host-dependent (ED-0070).
- The budgets the harness defends: filtered path < 50 ns/op with 0 allocations
  (Go managed 5.479 ns/op), any predicate < 10,000 ns/op on adversarial input
  (the criterion Go failed).

**The lesson that must survive:** the win is the ADAPTIVE FILTER, not the
language. A Rust port that calls exact arithmetic unconditionally will be just
as slow as the Go it replaced.

## Status

**Accepted** as the direction; the owner's decision is not in question.

**Validation gates are OPEN — including gate 1.** An earlier draft recorded
gate 1 as satisfied "with substantive tests per crate". That claim was false:
all five crate tests were tautological, each asserting a literal equalled the
same literal declared above it, and one of them certified an off-by-one against
FORMAT-SPEC row L6 while being named for that row (ED-0089).

Current honest state:

- `domowik-container` has a real, mutation-proven L6 boundary test: it fails
  when the ceiling is set back to 65,536, naming the spec row.
- The other four crates are **declared scaffolds**. Their tests prove nothing
  about Domowik and say so in their own module documentation.

Gate 1 closes when each crate has behaviour worth testing and a test that fails
without it. Gates 2-4 remain open.
