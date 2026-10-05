# 0062: Retire the Go toolchain

- **Phase:** 2
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0037

## Problem

Nothing in the Rust core, the CLI or the GUI depends on Go. ADR 0008 moved the
core to Rust, ADR 0002 is superseded, and no open issue across Phases 2 to 6
plans work that touches Go. What remained was four repo-hygiene packages and a
second toolchain that every contributor and every CI job had to install.

A review on 2026-09-06 established the facts this issue acts on:

- The Rust tests read `testdata/corpus/` fixtures directly, and **all 39
  generated `.domo` artifacts are committed**. Removing the generator breaks no
  Rust test.
- `internal/evidence/phase0` was referenced by nothing but ADR 0008's own list
  of surviving tools. Only `go test ./...` ever ran it.
- `spikes/0024-flutter-go-bridge/` carries its own `go.mod`, so it was outside
  `go build ./...` and verified by nothing — the superseded Go bridge spike,
  in neither the verified tree nor the frozen reference tree.
- Two of the Go stages guard an empty set: `dependency drift` runs
  `go mod tidy` against a module with no `go.sum` and no requires, and
  `module graph size` asserts `go list -m all` prints exactly one line.

## Delivered — 2026-09-06

**`internal/evidence/phase0` deleted.** Dead. Its output already lives under
`docs/evidence/`; the parser produced no measurements itself.

**`spikes/0024-flutter-go-bridge/` deleted.** Superseded by the Rust bridge
spike, built and tested by nothing, and recoverable from history.

**`domowik-jsoncheck` and `domowik-tracker-check` ported to Rust** as
`crates/domowik-devtools`, with no new dependency: `serde_json` was already a
workspace dependency, and the tracker checker's three patterns are prefix and
separator tests that plain string operations express without `regex`.

Rust rather than a shell or Python script because both tools carry real logic
and the tracker checker had a real test suite. The workspace already gates
`cargo fmt`, `cargo clippy -- -D warnings` and `cargo test`; a script in
`scripts/` would have been the only unlinted, untested code in the verifier.
Twenty tests now cover them, up from the Go originals' eight.

**Parity was PROVEN, not assumed**, against the Go binaries before they were
deleted — identical stdout, stderr and exit status on:

| Case | Result |
|---|---|
| `jsoncheck docs` | `checked 15 json fence(s)`, exit 0, both |
| `jsoncheck testdata/corpus` | `checked 17 json fence(s)`, exit 0, both |
| `jsoncheck` on a tree with no fences | exit 1, both — "nothing to check" must not read as success |
| `tracker-check`, real tracker | exit 0, both |
| Missing status line | byte-identical diagnostic, exit 1 |
| Status mismatch + orphan row | byte-identical, including line numbers and quoting |
| Duplicate id across `issues/` and `done/` | byte-identical, exit 1 |

Two divergences were found by that comparison and one was corrected.
`root.join(".")` rendered as `issues/./0077-x.md` where Go's `filepath.Join`
normalises it away, and the reported fence line was off by one against the Go
original; both now match. The remaining divergence is deliberate: the Rust
error appends `serde_json`'s own message, which names the line and column
within the fence, where Go printed a bare `invalid JSON`.

**`scripts/verify.sh` rewired.** The two `jsoncheck` stages moved below
`cargo test`, because a Rust binary cannot run before the workspace compiles;
`tracker consistency` already sat after the build and only changed its command.
Both use `cargo run --locked -q` rather than a path into `target/`, so a stale
binary from another profile cannot be picked up silently. No stage was removed
and none changed what it asserts.

`AGENTS.md` and `.github/workflows/ci.yml` were corrected to describe what
survives.

## Open question — `internal/corpusgen`, for the owner

**Not decided. Nothing in this issue's delivered work depends on the answer**,
and the Go toolchain stays until it is settled.

`internal/corpusgen` is 681 source lines and 1,432 test lines. Its
`zipwriter.go` is a hand-rolled byte-level ZIP writer — signature constants and
a `patchEOCD func([]byte) []byte` hook — built specifically to emit INVALID
archives: truncated EOCD, malformed zip64 locators, overlapping regions,
spanned disk numbers, encrypted entries, NUL bytes in names. The `zip` crate in
the workspace cannot produce these; it exists to write correct archives.

**Option A — port it.** Roughly 850 lines of Rust, no new dependency. Keeps
every issue-0028 gate: deterministic artifact regeneration, `TestCoverageAudit`,
`TestExpectationsManifest`, `TestRatioTwinMeta`, and the artifact-safety cap.

**Option B — freeze the fixtures, drop the generator.** The 39 `.domo` files
and 120 corpus files stay committed and the Rust tests keep consuming them.
What is lost is the guarantee that they are REPRODUCIBLE, plus the coverage
audit and the `EXPECTATIONS.md` sha256 recomputation that runs on every check.
That is a real downgrade to a fail-closed gate issue 0028 owns.

The trade turns on whether the corpus is finished. It is not obviously so:
issues 0028 and 0018 both name corpus work — golden, adversarial and migration
corpora — as outstanding. A frozen generator would have to be revived, or the
new fixtures hand-built, the first time that work starts.

Recommendation: **Option A if the corpus will grow, Option B if it is
effectively frozen.** The `EXPECTATIONS.md` digest recomputation is the single
part hardest to justify losing.

## Decided 2026-09-12 — OPTION A, port. Delivered standalone.

**The owner chose to port.** The corpus is not finished: issues 0028 and 0018
both name outstanding corpus work, and the finish-all-issues direction makes
more fixtures likely rather than less. Option B's cost, stated rather than
dropped, is what settled it — freezing would have given up deterministic
artifact regeneration, `TestCoverageAudit`, `TestExpectationsManifest` and the
`EXPECTATIONS.md` sha256 recomputation that runs on every check, and the first
new fixture would have had to revive the generator or be hand-built.

Recorded as **ED-0104**, which covers both halves: Option A (port) and the A2
sub-decision (pinned DEFLATE streams instead of a ported Go compressor).

**Delivered.** `crates/domowik-devtools/src/corpusgen/` — `zipwriter.rs`,
`generate.rs`, `expectations.rs` — plus `domowik-corpusgen` for regeneration
and the ten gates in `crates/domowik-devtools/tests/corpus_gates.rs`. No new
third-party crate: `sha2`, `crc32fast`, `zip` and `flate2` were already in the
workspace graph and already recorded as runtime crates, and
`docs/RUST-DEPENDENCIES.md` and `NOTICE` regenerate byte-identically.

**All 39 generated artifacts are byte-identical**, proven twice: against a
sha256 oracle taken from the Go generator's `AllArtifacts()` in memory BEFORE
any Rust existed and written into an empty directory, and against the committed
tree. Nothing under `testdata/corpus/` was rewritten to reach that.

Two silent-divergence traps were found and closed rather than discovered later.
Go's `encoding/json` HTML-escapes `<`, `>` and `&` where `serde_json` does not,
so the generator now refuses to emit a document containing one. Go decodes every
JSON number into `float64`, so an oracle written `150` and an observation
written `150.0` are equal there; without that, the `executionBlocked` check
would have stopped firing on a record that merely spelled its number
differently.

Nine mutants, each killed and reverted, including the L33 coverage mutant that
reproduces ED-0069's exact diagnostic — `row L33 covered without records` — so
the coverage audit demonstrably came across intact. Evidence, commands and
outputs: [docs/evidence/0062-corpusgen-rust/PARITY.md](../../docs/evidence/0062-corpusgen-rust/PARITY.md).

### The one deliberate difference: the generator no longer compresses

Two hostile fixtures ship a raw DEFLATE stream produced by Go's
`compress/flate` at `BestCompression`. `flate2`/`miniz_oxide` was measured at
every level from 0 to 9 against both payloads and matched neither, differing in
LENGTH as well as content — 369 bytes against Go's 372 for the twin, 55 against
51 for the snapshot. Byte-identity and a Rust compressor are therefore
incompatible.

The two streams are PINNED generator inputs, carrying their provenance and
sha256 in the source. Recompressing would have changed two committed artifacts
and their recorded digests; porting Go's level-9 encoder would have meant
translating roughly two thousand lines of `deflate.go`, `token.go`,
`huffman_code.go` and `huffman_bit_writer.go` into a repo-hygiene crate, with
the differential oracle disappearing the moment Go was removed.

**`TestRatioTwinMeta` changed mechanism, and with it the guarantee.**
Recompressing asserted that the stored stream is REGENERABLE from its payload by
a named compressor. The Rust gate inflates the stored stream and requires it to
equal the payload the package declares, which asserts that it EXPANDS to a fully
conforming snapshot document. Two mutants prove the inflate half bites on its
own.

**Neither subsumes the other, and the first draft of this section wrongly called
the new gate "strictly stronger". ED-0104 corrects that.** What is lost is real:
payload-to-compressor regeneration for those two streams is gone, not replaced.
What stands in its place is a semantic expansion check plus a recorded sha256
and provenance per stream. The trade was accepted because the alternatives were
changing committed fixture bytes or vendoring roughly two thousand lines of
translated Go compressor into a repo-hygiene crate.

A future compressed fixture carries its stream the same way and is not blocked,
but it takes one manual step the Go generator did automatically.

### Adversarial review — 2026-09-12

Codex `gpt-5.6-sol` at high effort, scoped to four questions: gate parity,
divergence on FUTURE inputs, whether the pinned-DEFLATE loss was fully stated,
and latent defects in the ported writer. It confirmed all nine Go gates have
counterparts, and recorded the pinned-stream disclosure and the "neither
subsumes the other" framing as non-issues.

Four findings fixed, each with a test:

- **A top-level `null` could claim `rawByDesign`.** Go's `json.Unmarshal`
  accepts `null` into a map without error, so the Go gate caught it as a false
  raw claim; `serde_json::from_slice::<Map>` refuses `null`, which would have
  read as "genuinely not an object" and let the record skip its assert contract.
  A real weakening, now matched by `go_unmarshal_object` and mutation-proven.
- **Entry names could no longer be arbitrary bytes.** `Entry::name` was a
  `String`, so the port could not author a hostile fixture with a non-UTF-8
  entry name — a §7.6 vector the Go generator could express. Now `Vec<u8>`;
  all 39 artifacts re-verified byte-identical after the change.
- **Two numeric checks accepted values Go's `int(...)` cannot represent.**
  `1e300` passed both `operationIndex` and `warnings`. Go's out-of-range
  float-to-int conversion is undefined, so `as_go_integer` rejects instead:
  well defined, and stricter than either side.
- **The corpus walk followed symlinked directories**, where Go's `WalkDir`
  treats them as leaves, and collapsed non-UTF-8 filenames lossily.

One finding accepted as known: the manifest gate delegates container tolerance
to a ZIP reader, so a future generated package one reader accepts and the other
rejects would be inspected by one and not the other. Mitigated by the
anti-vacuity check the port added, which requires both conforming golden
packages to contribute a manifest check.

**One finding ROUTED TO ISSUE 0028, not fixed here.** `base[1..]` means
"everything but the manifest" but, sorted, index 0 is
`assets/descriptors.json` — so eight traversal/name fixtures and
`ambiguous-manifest` ship a manifest inventorying an entry the archive does not
contain, on top of the defect they mean to isolate. It is the same class as the
`missing-manifest` slice-position defect issue 0028 already fixed, surviving one
function away from the comment that records it. INHERITED, not introduced: the
port reproduces it exactly, which is what makes the artifacts byte-identical.
Repairing it changes nine artifacts and their recorded digests, which is 0028's
call. Recorded in that issue with the byte-level evidence.

### Historical handoff — 2026-09-12

The Go package, `go.mod` and the Go stages are still here. `scripts/verify.sh`
and `.github/workflows/` belong to issue 0061 while it is in flight, so the
removal is prepared and handed over rather than taken:
[docs/evidence/0062-corpusgen-rust/SWITCHOVER.md](../../docs/evidence/0062-corpusgen-rust/SWITCHOVER.md)
carries the exact patch, verified with `git apply --check`, plus the two
ownership conflicts it must be sequenced around.

Both generators running side by side is worth something while it lasts:
`go test ./internal/corpusgen` and `cargo test -p domowik-devtools` assert byte
equality against the same committed artifacts, so the implementations
cross-check each other on every run rather than only at the moment of the port.

### Final switchover — 2026-10-04

The owner requested completion of the Go cleanup. The prepared handoff is
applied to the current verifier and CI configuration, preserving the
mandatory Linux editor gate and unrelated workspace changes. The last active
Go package, root module, CI setup and version-manager entry are retired
together; the Rust corpus gates replace the Go stage. The frozen reference
tree and historical evidence remain intact. Task commands, contributor setup,
the naming registry and ADR 0008 now agree that Go is retired from active work.

Full local verification passed in the clean snapshot
`/tmp/domowik-no-go-check.16uA9W`, with neither `go` nor `gofmt` on PATH:

```sh
export PATH=/opt/dev/flutter/bin:/home/dawid/.cargo/bin:/usr/bin:/bin
if command -v go || command -v gofmt; then exit 1; fi
DOMOWIK_REQUIRE_LINUX_EDITOR=1 ./scripts/verify.sh
```

Exit 0, ending `=== VERIFICATION PASSED === all stages`. The run covered
Rust formatting/build/clippy/tests, all 11 named corpus gates, dependency and
NOTICE checks, benchmark compilation/smoke, CLI probes, Rust/C/Dart bridge
checks, native Linux file dialogs, Flutter analysis, 305 widget tests, all
three CPU harnesses, Linux release compilation and both Xvfb bundle probes.
The full log is `/tmp/domowik-no-go-verify.log`. Flutter 3.47.5 / Dart 3.13.4
matched the pre-existing workspace lockfile; CI's 3.41.6 pin is unchanged.
macOS application/Bash 3.2 checks were not exercised on this Linux host.

Local CI preflight passed 27 summary cases, shell lint over eight files,
missing-prerequisite rejection proofs and 27 inventory checks; the inventory
log is `/tmp/domowik-no-go-preflight.log`. One optional cached libloading
0.8.9 text probe announced a skip; the independent ISC notice proofs passed.
`task --dry test` resolves only to `cargo test --workspace --locked`.

The first scratch build reused stale source timestamps and was rerun after
cache invalidation. A subsequent benchmark build exhausted the temporary
filesystem quota; relocating the disposable cache to local disk resolved it.
Neither failure caused product edits or remote CI runs. Existing workspace
changes and installed Go toolchains are preserved; no commit or push is part
of this closure.

## Acceptance criteria

- [x] Dead Go packages are deleted rather than ported.
- [x] `domowik-jsoncheck` and `domowik-tracker-check` run from Rust with no
      new dependency, and their behaviour is proven identical to the Go
      originals — stdout, stderr and exit status — on success and on every
      failure path, before the originals are removed.
- [x] The ported tools are covered by `cargo fmt`, `cargo clippy -- -D
      warnings` and `cargo test`, with at least the test coverage the Go
      versions had.
- [x] `scripts/verify.sh` keeps every stage it had, asserting the same things,
      with stage ordering corrected for the new build dependency.
- [x] Documentation naming the retired tooling is corrected in the same
      change.
- [x] The `internal/corpusgen` question above is answered by the owner and
      recorded, with the losing option's cost stated rather than dropped.
- [x] If corpusgen is ported or frozen: `go.mod`, the `go build`, `go test`,
      `gofmt`, `go vet`, `dependency drift`, `module graph size` and
      `module graph identity` stages, and the CI `setup-go` step and
      `GOTOOLCHAIN` variable are removed together, in one change.
      *Applied to the current Linux/macOS workflows on 2026-10-04; the
      mandatory Linux editor gate is preserved.*
- [x] `./scripts/verify.sh` passes with no Go toolchain available in the
      verification environment (neither `go` nor `gofmt` on PATH).

## Boundaries

`_reference/go/` is NOT in scope and is deliberately retained. It is the only
worked implementation of migrations (issue 0031), the `ops` flag set (0027),
asset `pin`/`unpin`/`gc` (0026) and release/SBOM (0036), and it costs nothing:
the leading underscore keeps it invisible to the Go toolchain, so no verifier
stage touches it. AGENTS.md already governs how it may be used — read, do not
port mechanically, the specification wins.

Retiring it is a separate decision, and a cheaper one to defer than to reverse.

## Provenance

Original repository review and local reproduction on 2026-09-06. The ported
tools are behaviour-for-behaviour translations of this repository's own Go
code; no third-party source was consulted or adapted.

## Closure

- **Implementation/spec commits:**
- **Follow-up issues:**
