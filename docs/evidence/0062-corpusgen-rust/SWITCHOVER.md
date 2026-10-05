# Removing the Go toolchain: the exact change (issue 0062)

The Rust corpus generator is in the tree and proven byte-identical
([PARITY.md](PARITY.md)). This is the second half — everything that has to
happen in ONE commit, because a repository with `go.mod` and no Go package is
as broken as one with Go stages and no Go toolchain.

Not applied here. `scripts/verify.sh` and `.github/workflows/` belong to issue
0061 while it is in flight, so this is handed over rather than taken.

## Order of operations

1. Apply the patch below.
2. `git rm go.mod internal/corpusgen/generate.go internal/corpusgen/zipwriter.go internal/corpusgen/corpus_test.go`
   (this empties `internal/`, so the directory goes with it).
3. Remove the `github-actions/setup-go` row from `docs/third-party/tools.json`
   — see the conflict warning below.
4. Run `./scripts/verify.sh` on a machine with NO Go on `PATH`, and keep the
   `=== VERIFICATION PASSED ===` banner. That is issue 0062's last open
   acceptance criterion and nothing else can close it.

## What the patch does

**`scripts/verify.sh`** loses `export GOTOOLCHAIN=local` and eight stages:
`go build`, `go test`, `gofmt`, `go vet`, `corpusgen gates`, `dependency
drift`, `module graph size`, `module graph identity`. A `corpus gates` stage
appears after `cargo test`, running the same ten gates a second time. That
looks redundant and is on purpose: `cargo test --workspace` already ran them,
but ED-0077 exists because an unnamed failure is indistinguishable from a pass,
and a corpus gate that fails inside a workspace-wide summary has no stage name
to print. It costs hundredths of a second.

**`.github/workflows/ci.yml`** loses the `GOTOOLCHAIN` env block and BOTH
`Set up Go` steps — the ubuntu `verify` job and the `verify-macos` job. The
ubuntu job is renamed to `verify (rust core)`.

**`AGENTS.md`** stops saying a Go package survives and stops describing Go
stages.

**`testdata/corpus/README.md`** and **`testdata/corpus/EXPECTATIONS.md`** point
at the Rust generator and the new `--update` command.

**ADR 0008** gets a second amendment recording that the open question is
answered, in the same append-only style as the first. Nothing earlier is
rewritten.

## Conflicts to expect

- `docs/third-party/tools.json`: issue 0024's branch also touches this file.
  The `setup-go` row (lines 15-25 as of `06ce1f9`; find it by name, not by line — issue 0024 has already shifted it once) must go when the CI step does. Sequence the
  two so neither drops the other's edit. The row is not in the patch below for
  exactly this reason.
- `docs/NAMING.md` row 28 registers the Go module path `domowik.dev/domowik` as
  provisional. Once `go.mod` is gone there is no Go module to name. The row is
  issue 0034's to retire or annotate, not this issue's, and leaving it stale is
  the smaller error than editing a naming registry across an ownership
  boundary. Flagged, not patched.
- `_reference/go/README.md` lines 41-53 say `internal/corpusgen` is "still Go".
  That tree is deliberately out of scope and preserved, so the sentence is left
  alone here; it describes the active tree rather than the frozen one, so it is
  worth a one-line correction in whatever change next has a reason to touch
  that file.

## The complete Go surface, so nothing is missed

A sweep of the tree outside `_reference/` finds exactly this, and the list
above covers all of it:

| Item | Disposition |
|---|---|
| `internal/corpusgen/{generate,zipwriter,corpus_test}.go` | `git rm` |
| `go.mod` | `git rm` (there is no `go.sum`) |
| `scripts/verify.sh` | patched |
| `.github/workflows/ci.yml` | patched, both jobs |
| `AGENTS.md`, `testdata/corpus/README.md`, `testdata/corpus/EXPECTATIONS.md`, ADR 0008 | patched |
| `docs/third-party/tools.json` | flagged above, sequenced with issue 0024 |

`.gitignore` keeps its two Go sections (`*.test`, `*.out`, `/domowik`,
`coverage.out`, `*.prof`). Nothing breaks either way, and `_reference/go/` can
still be built by hand, so the rules still have a subject. Worth a look if the
reference tree is ever retired.

Everything else that matches `go.mod`, `GOTOOLCHAIN`, `setup-go` or `gofmt`
lives under `docs/evidence/`, `EXECUTIVE-DECISIONS.md` or closed issues. Those
are records of runs that happened and are not edited.

## What is NOT removed, and why

`_reference/go/` stays exactly as it is. It is the only worked implementation
of migrations (0031), the `ops` flag set (0027), asset `pin`/`unpin`/`gc`
(0026) and release/SBOM (0036). The leading underscore keeps it out of the Go
toolchain's default patterns, which is why no verifier stage ever reached it
and why removing the toolchain does not touch it. Retiring it is a separate
decision and a cheaper one to defer than to reverse.

The historical evidence documents under `docs/evidence/` that mention `go
test`, `gofmt` or `GOTOOLCHAIN` are records of runs that happened. They are not
edited.

## The patch

Re-verified with `git apply --check` against `main` at `06ce1f9`, through merge
`b626116`. None of the six target files changed between `55cf559` and
`06ce1f9`, so the patch below is unchanged from that regeneration.

It was regenerated once already, against `55cf559`, after issue 0061's verifier and CI rework landed: the
original applied cleanly to `38b0024` and did NOT apply to `55cf559`, because
0061 inserted a `REPO_ROOT` resolution and a marked failure-reporting preamble
either side of the Go stages and rewrote much of `ci.yml`.

**Expect it to need regenerating once more.** Issue 0027's ops branch has a
two-line `verify.sh` change pending (`--assert-local-only` on the P14 stage).
That lands before this removal does, and it will move the context again. A
handoff patch that no longer applies is worse than none, so this is kept
current with `main` rather than with the commit that wrote it — re-check it
with `git apply --check` immediately before applying, every time.

```diff
--- a/scripts/verify.sh
+++ b/scripts/verify.sh
@@ -4,7 +4,6 @@
 # hard check locally; ci.yml invokes the same script. All issue-0008 baseline
 # steps are preserved verbatim; the 0036 checks are additive.
 set -euo pipefail
-export GOTOOLCHAIN=local
 
 # Resolved from this script's own location, not from $PWD: several stages `cd`
 # into subdirectories inside a subshell, and one of them needs to invoke a
@@ -58,40 +57,19 @@
   "$CURRENT_STAGE" "$LINENO" "$rc" >&2; exit "$rc"' ERR
 # END FAILURE-REPORTING PREAMBLE
 
-stage "go build"
-go build ./...
-stage "go test"
-go test ./...
-stage "gofmt"
-test -z "$(gofmt -l .)"
-stage "go vet"
-go vet ./...
-
-# Issue 0028: the corpus's own gates — deterministic artifact regeneration,
-# machine-readable expectation-manifest completeness, and the inert-artifact
-# size cap (all implemented inside internal/corpusgen as dev-only checks;
-# no reader/writer/validator behavior).
+# WITHDRAWN WITH THE GO TOOLCHAIN (issue 0062, ED-0104).
 #
-# The two `jsoncheck` stages that used to sit here moved BELOW the Rust build
-# (issue 0062): the checker is now a Rust binary, so it cannot run before the
-# workspace compiles. Nothing else about them changed.
-stage "corpusgen gates"
-go test ./internal/corpusgen
-
-# P5: dependency-drift guard
-stage "dependency drift"
-go mod tidy && git diff --exit-code -- go.mod go.sum
-
-# F1/P6: license guard - the module graph must contain ONLY the canonical
-# module. Empirically (go1.27.0, 'go 1.27', GOTOOLCHAIN=local),
-# `go list -m all` prints exactly one line; go@/toolchain@ edges appear only
-# in `go mod graph`. The expected value is derived via `go list -m` rather
-# than hardcoded so this script stays free of the provisional URL token
-# (contract sections 3 and 6).
-stage "module graph size"
-test "$(go list -m all | wc -l)" -eq 1
-stage "module graph identity"
-test "$(go list -m all)" = "$(go list -m)"
+# `go build`, `go test`, `gofmt`, `go vet`, `corpusgen gates`, `dependency
+# drift`, `module graph size` and `module graph identity` all existed to cover
+# ONE package, `internal/corpusgen`. It is Rust now, so there is no Go module,
+# no `go.mod`, and nothing for those stages to assert. Two of them guarded an
+# empty set even before that: `dependency drift` ran `go mod tidy` against a
+# module with no `go.sum` and no requires, and `module graph size` asserted
+# that `go list -m all` prints exactly one line.
+#
+# `_reference/go/` is untouched and stays invisible to any Go toolchain: the
+# leading underscore keeps it out of the toolchain's default patterns, which is
+# why no stage ever reached it.
 
 # WITHDRAWN: the Go release-inventory verifier (0036).
 #
@@ -138,6 +116,18 @@
 stage "cargo test"
 cargo test --workspace --locked
 
+# Issue 0028: the corpus's own gates — deterministic artifact regeneration,
+# machine-readable expectation-manifest completeness, the honest-coverage audit
+# and the inert-artifact size cap (all dev-only checks in
+# `crates/domowik-devtools`; no reader/writer/validator behavior).
+#
+# `cargo test --workspace` above already ran these. Naming them again costs
+# hundredths of a second and buys the thing ED-0077 exists for: a failing corpus
+# gate prints STAGE "corpus gates" instead of being one line inside a test
+# summary.
+stage "corpus gates"
+cargo test --locked -p domowik-devtools --test corpus_gates
+
 # Issue 0004: syntax-validate every fenced ```json block in committed docs,
 # then the same parse-only guard over the conformance corpus manifest.
 # Parse-only by design; schema/conformance checking belongs to issues
--- a/.github/workflows/ci.yml
+++ b/.github/workflows/ci.yml
@@ -9,12 +9,9 @@
 permissions:
   contents: read
 
-env:
-  GOTOOLCHAIN: local
-
 jobs:
   verify:
-    name: verify (rust core + go corpus generator)
+    name: verify (rust core)
     runs-on: ubuntu-24.04
     steps:
       # Issue 0061 acceptance: runner cost and wall-clock impact are RECORDED,
@@ -27,14 +24,6 @@
 
       - name: Check out repository
         uses: actions/checkout@08c6903cd8c0fde910a37f88322edcfb5dd907a8 # v5.0.0
-      # verify.sh needs BOTH toolchains: the core is Rust, and the surviving
-      # issue-0028 corpus generator (internal/corpusgen) is still Go. The other
-      # hygiene tools moved to Rust under issue 0062.
-      - name: Set up Go
-        uses: actions/setup-go@44694675825211faa026b3c33043df3e48a5fa00 # v6.0.0
-        with:
-          go-version: '1.27.0'
-          check-latest: false
       # No toolchain version here: rust-toolchain.toml pins it, and naming a
       # version in both places lets them drift apart silently. rustup honours
       # the file, so this installs exactly what the repository asks for.
@@ -161,12 +150,6 @@
       - name: Check out repository
         uses: actions/checkout@08c6903cd8c0fde910a37f88322edcfb5dd907a8 # v5.0.0
 
-      # Same pin as the verify job; setup-go supports macOS runners directly.
-      - name: Set up Go
-        uses: actions/setup-go@44694675825211faa026b3c33043df3e48a5fa00 # v6.0.0
-        with:
-          go-version: '1.27.0'
-          check-latest: false
 
       # Same pin/rationale as the verify job; macos-15 already ships
       # rustup/cargo/rustc 1.98.0 matching rust-toolchain.toml (confirmed
--- a/AGENTS.md
+++ b/AGENTS.md
@@ -106,13 +106,14 @@
   `domowik-jsoncheck`, `domowik-tracker-check`). It is not product code and is
   not distributed; it lives in the workspace so `cargo fmt`, `cargo clippy`
   and `cargo test` cover it.
-- **One Go package survives**: `internal/corpusgen`, the issue-0028 fixture
-  generator, still formatted with `gofmt`. Its hand-rolled ZIP writer emits
-  deliberately malformed archives, which the `zip` crate cannot do; whether to
-  port or freeze it is issue 0062's open question. The Go module path remains
-  `domowik.dev/domowik` per
-  [ADR 0006](docs/decisions/0006-naming-and-identity.md) and the
-  [naming registry](docs/NAMING.md).
+- **No Go package survives in the active tree.** The last one,
+  `internal/corpusgen`, was ported to Rust under issue 0062 (ED-0104) and now
+  lives in `crates/domowik-devtools/src/corpusgen`. Its hand-rolled ZIP writer
+  emits deliberately malformed archives, which the `zip` crate cannot do, so the
+  byte-level assembly came across as-is; every one of the 39 committed artifacts
+  is byte-identical to what the Go generator produced. The generator does NOT
+  compress: the two DEFLATE streams the corpus ships are pinned inputs with
+  recorded provenance, checked by inflating them.
 - Proposed GUI: Dart/Flutter, subject to the FFI feasibility gate.
 - Document exported APIs, ownership, concurrency, errors, numerical
   guarantees, and FFI safety. Avoid comments that merely restate code.
@@ -140,12 +141,11 @@
 ([THIRD-PARTY-POLICY](docs/THIRD-PARTY-POLICY.md)). It reports declared
 licences only; it is not a licence audit and does not build `NOTICE`.
 
-Go stages cover `internal/corpusgen` only: `go build`, `go test`, `gofmt -l`,
-`go vet`, the corpus gates, dependency-drift and module-graph guards. The JSON
-fence checks and the `tracker consistency` stage are Rust binaries from
-`domowik-devtools` and therefore run AFTER `cargo test`; tracker consistency
-still runs just before the clean-tree check, so a tracker repair stays visible
-as uncommitted work rather than masked by it.
+There are no Go stages. The JSON fence checks, the `corpus gates` stage and
+the `tracker consistency` stage are Rust binaries and tests from
+`domowik-devtools`, so they run AFTER `cargo test`; tracker consistency still
+runs just before the clean-tree check, so a tracker repair stays visible as
+uncommitted work rather than masked by it.
 
 **Withdrawn, and owed back:** the Go release-inventory verifier froze with the
 Go product it describes, and CLI stream/exit-code probes P1-P3 plus
--- a/testdata/corpus/README.md
+++ b/testdata/corpus/README.md
@@ -38,12 +38,12 @@
 ```
 
 `.domo` packages are real ZIP-family containers built deterministically by the
-dev-only generator in `internal/corpusgen` (Go stdlib `archive/zip` plus
-targeted byte patches for defect variants). Only these generated artifacts —
+dev-only generator in `crates/domowik-devtools/src/corpusgen` (a hand-rolled
+byte-level ZIP writer plus targeted patches for defect variants). Only these generated artifacts —
 the `.domo` files and the ratio-twin sidecar `meta.json` — carry a
 byte-reproducibility guarantee: the test suite regenerates them on every run
-and asserts byte equality (`go test ./internal/corpusgen -update` rewrites
-them). All OTHER committed fixtures are integrity-checked instead: every
+and asserts byte equality (`cargo run -p domowik-devtools --bin
+domowik-corpusgen -- --update` rewrites them). All OTHER committed fixtures are integrity-checked instead: every
 artifact record in `EXPECTATIONS.md` carries a `sha256` that the checker
 recomputes from the referenced file bytes on every run and never trusts from
 a sidecar.
@@ -140,7 +140,7 @@
 no fixture expands to resource-exhausting sizes on any machine. Traversal
 fixtures use synthetic entry names inside package metadata only; nothing is
 extracted to host paths by this corpus, and no nested archive is ever
-interpreted. The independent safety test in `internal/corpusgen` asserts the
+interpreted. The independent safety test in `domowik-devtools` asserts the
 size cap over all artifacts and recipe bounds on every run.
 
 ## Provenance and clean-room statement
--- a/testdata/corpus/EXPECTATIONS.md
+++ b/testdata/corpus/EXPECTATIONS.md
@@ -1,8 +1,8 @@
 # Corpus expectation manifest (issue 0028)
 
 Machine-readable index of every fixture. Records live in the fenced JSON
-arrays below and are validated by `internal/corpusgen`
-(`go test ./internal/corpusgen`), which enforces: unique ids, closed
+arrays below and are validated by the corpus gates in `domowik-devtools`
+(`cargo test -p domowik-devtools --test corpus_gates`), which enforces: unique ids, closed
 vocabularies, clause citations, artifact/recipe exclusivity, artifact
 existence and the 256 KiB inert cap, recipe bound expressions, and full
 two-way coverage between this file and the committed fixture tree.
@@ -32,7 +32,7 @@
    normative v0.1 profile.
 
 The `residualBlockers` array in the coverage-audit fence lists every record in
-state 2 or 3 with its route. `internal/corpusgen` enforces the honest-coverage
+state 2 or 3 with its route. The corpus gates enforce the honest-coverage
 contract mechanically: the marker's named assertions must exist in the
 record's own `assert` block; `residualBlockers` must match the blocked record
 set exactly in both directions; and no `categoryAudit` or `rowAudit` entry may
--- a/docs/decisions/0008-rust-core-implementation.md
+++ b/docs/decisions/0008-rust-core-implementation.md
@@ -115,6 +115,24 @@
   archives, which the `zip` crate cannot; whether to port or freeze it is
   issue 0062's open question and the reason `go.mod` still exists.
 
+**Amendment 2 (issue 0062, ED-0104).** The open question above is answered:
+OPTION A, port. `internal/corpusgen` is Rust, in
+`crates/domowik-devtools/src/corpusgen`, and `go.mod` is gone with it. No Go
+package remains in the active tree and no Go toolchain is needed to verify the
+repository.
+
+Ported rather than frozen because the corpus is not finished — issues 0028 and
+0018 both name outstanding corpus work — and freezing would have cost the
+deterministic regeneration guarantee, the coverage audit and the
+`EXPECTATIONS.md` digest recomputation that runs on every check. All 39
+generated artifacts are byte-identical to the Go generator's output.
+
+One guarantee changed rather than survived, and ED-0104 names the trade: the
+generator no longer compresses, because no Rust compressor reproduces Go's
+DEFLATE bytes, so the two streams the corpus ships are pinned inputs proven by
+inflating them. Payload-to-compressor regeneration is lost; a semantic
+expansion check plus a recorded digest stands in its place.
+
 Nothing about the frozen `_reference/go/` tree changed. `internal/release`,
 `cmd/domowik-sbom` and `release/` stay frozen with it, and the Rust
 release/SBOM obligation recorded below is unaffected.
```
