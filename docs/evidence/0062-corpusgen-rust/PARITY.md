# The corpus generator in Rust, against the Go original (issue 0062)

`internal/corpusgen` was the last Go package in the active tree. Issue 0062
left its disposition open — port it or freeze the fixtures — and the owner
chose to port. This is the evidence that the port did not change anything.

Both generators exist in the tree while this lands. That is deliberate: until
the Go package is removed, `go test ./internal/corpusgen` and
`cargo test -p domowik-devtools` assert byte equality against the SAME
committed artifacts, so the two implementations cross-check each other on every
run rather than only at the moment of the port.

## What moved

| Go | Rust |
|---|---|
| `internal/corpusgen/zipwriter.go` | `crates/domowik-devtools/src/corpusgen/zipwriter.rs` |
| `internal/corpusgen/generate.go` | `crates/domowik-devtools/src/corpusgen/generate.rs` |
| `internal/corpusgen/corpus_test.go` | `crates/domowik-devtools/src/corpusgen/expectations.rs` and `crates/domowik-devtools/tests/corpus_gates.rs` |
| `go test ./internal/corpusgen -update` | `cargo run -p domowik-devtools --bin domowik-corpusgen -- --update` |

Splitting the test file was not cosmetic. The Go original mixed the record
types, the closed vocabularies and the pure validators in with the gates that
walk the tree, which is why its negative invariants had to reach into test-file
internals. The validators now live in the library and the gates call them, so
the invariant tests exercise the same code the gates do.

No new third-party crate. `sha2`, `crc32fast`, `zip` and `flate2` were already
in the workspace graph and already recorded as runtime crates;
`scripts/rust-deps-inventory.py` regenerates `docs/RUST-DEPENDENCIES.md`
byte-identically after the change, and `NOTICE` is unchanged.

## Byte equality: 39 artifacts, both directions

The Go generator's in-memory output was captured before any Rust existed — one
sha256 and byte length per artifact, taken from `AllArtifacts()` rather than
from the committed files, so the comparison cannot be satisfied by both sides
reading the same tree. The Rust generator then wrote all 39 artifacts into an
EMPTY directory and the two lists were diffed.

```text
$ diff -u go-oracle-sorted.txt rust-oracle-sorted.txt
(no output)
IDENTICAL: 39 artifacts, same sha256 and byte length from both generators
```

And against the committed tree:

```text
$ GOTOOLCHAIN=local go test ./internal/corpusgen
ok  	domowik.dev/domowik/internal/corpusgen	0.335s
EXIT 0

$ cargo run --locked -q -p domowik-devtools --bin domowik-corpusgen -- --check testdata/corpus
checked 39 corpus artifact(s)
EXIT 0
```

Nothing under `testdata/corpus/` was rewritten to reach this. No fixture byte,
no `EXPECTATIONS.md` digest and no sidecar number changed.

## Two places where Go and Rust could have diverged silently

**Map key order and string escaping.** Go's `encoding/json` sorts map keys and
HTML-escapes `<`, `>` and `&`; `serde_json` sorts `BTreeMap` keys the same way
but escapes nothing. No current fixture contains one of those characters, so
the divergence would have appeared for the first time in some future fixture
and looked like a generator bug rather than an encoder difference. The
generator now REFUSES to emit a document containing one, with a test that
proves the refusal fires.

**Number spelling in the blocked-assertion check.** Go decodes every JSON
number into `float64`, so an oracle written `150` and an observation written
`150.0` are equal there and the `executionBlocked` check fires. `serde_json`
keeps the distinction and would have let that record through. Equality is
compared through `go_json_equal`, and the enforcement test carries a case that
fails without it.

## The one deliberate difference: the generator no longer compresses

Two hostile fixtures ship a raw DEFLATE stream, both produced by Go's
`compress/flate` at `BestCompression`.

`flate2`/`miniz_oxide` was measured at every level from 0 to 9 against both
payloads. It matched neither, and differed in LENGTH as well as content: 369
bytes against Go's 372 for the twin, 55 against 51 for the snapshot. No level
is close, so there is no Rust compressor setting that reproduces those bytes.

That left three options. Recompressing with `flate2` would have changed two
committed artifacts, their recorded digests and the ratio sidecar. Porting Go's
level-9 encoder would have meant translating roughly two thousand lines of
`deflate.go`, `token.go`, `huffman_code.go` and `huffman_bit_writer.go` — a
vendored compressor inside a repo-hygiene crate, with the differential oracle
disappearing the moment Go was removed. The third option was taken: the two
streams are PINNED generator inputs, carrying their provenance and sha256 in
the source, and the property the fixtures rest on is checked the other way
round.

**The gate changed guarantee, and calling that an upgrade would be wrong.** An
earlier draft of this document said the gate got "strictly stronger". It did
not, and ED-0104 corrects the claim.

| | Go: recompress | Rust: inflate |
|---|---|---|
| Asserts | the stored stream is REGENERABLE from its payload by a named compressor | the stored stream EXPANDS to the payload the package declares |
| Catches | a payload edited after the stream was built | a stream that expands to the wrong document |
| Misses | whether the bytes expand to anything conforming at all | a coordinated edit to both stream and payload, caught instead by the pinned sha256 |

Neither is a superset of the other. **What is lost is real:** those two stored
streams are no longer recomputable from their payload by the generator's own
compressor, and that property is gone rather than replaced. What stands in its
place is a semantic expansion check plus a recorded digest and provenance for
each stream — different coverage, deliberately accepted (ED-0104) because the
alternatives were changing committed fixture bytes or vendoring roughly two
thousand lines of translated compressor into a repo-hygiene crate.

A future compressed fixture is not blocked: it carries its stream the same way,
generated once by any deterministic encoder and pinned with the same inflate
check. It does mean one manual step the Go generator did automatically.

## Every gate, and the mutant that proves it bites

Eleven gates run on `cargo test --workspace`. The Go original had nine. The two
additions both exist to stand behind something the port changed: the
pinned-stream check behind the pinned DEFLATE inputs, and the null-document
check behind the JSON parse that replaced Go's. Every other gate asserts what
its Go original asserted.

| Gate | Go original | Mutant | Result |
|---|---|---|---|
| `generate_corpus_artifacts` | `TestGenerateCorpusArtifacts` | one byte flipped in a committed `.domo` | killed |
| `generated_manifests_are_conforming` | `TestGeneratedManifestsAreConforming` | generator emits `assetRoot` as `null` | killed |
| `corpus_artifact_safety` | `TestCorpusArtifactSafety` | a 300 KiB file added under `hostile/container/` | killed |
| `coverage_audit` | `TestCoverageAudit` | L33 flipped from `blocked-upstream-spec` to `covered` | killed — `row L33 covered without records` |
| `expectations_manifest` | `TestExpectationsManifest` | one recorded artifact sha256 corrupted | killed |
| `ratio_twin_meta` | `TestRatioTwinMeta` | sidecar `storedBytes` drifted by one | killed |
| `ratio_twin_meta` (inflate half) | replaces the recompression half | twin payload document altered at equal length | killed — `the twin's deflate entry does not expand to the declared payload` |
| `pinned_deflate_streams_expand_to_their_payloads` | none | one nibble changed in the pinned stream | killed |
| the same, digest guard disabled | none | as above, so only the inflate check can catch it | killed |
| `a_null_document_cannot_claim_raw_by_design` | part of `TestValidateAssertInvariants`'s parse path | `go_unmarshal_object` reverted to strict `serde_json` | killed |
| `validate_assert_invariants` | `TestValidateAssertInvariants` | self-proving: one compliant control plus five wrong records that must each produce a violation | — |
| `coverage_audit_invariants` | `TestCoverageAuditInvariants` | self-proving: seven wrong claims that must each be flagged, plus three compliant controls that must not be | — |
| `execution_blocked_marker_is_enforced` | `TestExecutionBlockedMarkerIsEnforced` | self-proving: one well-formed marker plus ten malformed ones that must each be rejected | — |

The L33 mutant reproduces the exact failure ED-0069 recorded for the Go gate,
which is the strongest single signal that the coverage audit came across
intact: the same wrong claim produces the same diagnostic.

Every mutant was reverted and the suite re-run green.

## Counts

- 39 generated artifacts, byte-identical.
- 175 expectation records, 115 artifact-backed, parsed and validated.
- 11 corpus gates plus 14 new unit tests for the generator and the validators;
  `domowik-devtools` goes from 20 tests to 45.
- `cargo test --workspace --locked`: all suites pass.
- `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets --locked -- -D warnings`: clean.

## Without Go on PATH

No toolchain was uninstalled. The commands below ran in a stripped environment
whose `PATH` is `~/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin` — `bash -c`, not
`bash -lc`, because a login shell re-sources the profile and puts the mise-
managed Go back.

```text
go: not found on PATH
gofmt: not found on PATH
checked 39 corpus artifact(s)
test result: ok. 34 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
EXIT 0
```

That covers the corpus generator and every `domowik-devtools` test, the eleven
gates included. It does NOT cover the whole verifier.

## What this does NOT yet show

`./scripts/verify.sh` has not been run end to end with the Go toolchain absent,
and no `=== VERIFICATION PASSED ===` banner is claimed here. That is issue
0062's last acceptance criterion; it needs the Go removal to land first, and
the removal is sequenced behind the CI and ops-P14 integrations. The exact
change is in [SWITCHOVER.md](SWITCHOVER.md). Until it lands the Go stages still
run and still pass, which is the cross-check described at the top of this
document.

## Adversarial review, and what it changed

Codex `gpt-5.6-sol` at high effort, scoped to four questions: gate parity,
silent divergence on future inputs, whether the pinned-DEFLATE loss was fully
stated, and latent defects in the ported writer. It confirmed all nine Go gates
have counterparts, accepted the pinned-stream disclosure and the "neither
subsumes the other" framing as non-issues, and found five things worth acting
on. Four were fixed here; one is routed.

**Fixed — a top-level `null` could claim `rawByDesign`.** Go's
`json.Unmarshal` accepts `null` into a map without error, so the Go gate caught
a `null` fixture as a false raw claim. `serde_json::from_slice::<Map>` refuses
`null`, which would have read as "genuinely not an object" and let such a record
skip its assert contract entirely. `go_unmarshal_object` now matches Go, with a
gate and a mutant.

**Fixed — entry names could no longer be arbitrary bytes.** Go strings carry
any bytes, so the Go generator could author a hostile fixture whose entry name
is not valid UTF-8, which is a real §7.6 name vector. `Entry::name` was a
`String` and could not. It is `Vec<u8>` now; the manifest key goes through
`from_utf8_lossy`, which is what Go's `encoding/json` does with invalid UTF-8 in
a key. All 39 artifacts re-verified byte-identical after the change.

**Fixed — two numeric checks accepted values Go's `int(...)` cannot
represent.** `operationIndex` used a saturating `as i64` and `warnings` used a
bare `fract() == 0.0`, so `1e300` passed both. Go converts with `int(...)`,
which is UNDEFINED outside the integer range, so there is no behaviour to match
out there; `as_go_integer` rejects instead, which is well defined and stricter
than either side.

**Fixed — the corpus walk followed symlinked directories.** Go's
`filepath.WalkDir` treats a symlinked directory as a leaf; `Path::is_dir`
follows it, so a symlink planted in the tree could walk outside the corpus or
loop. Recursion is decided by `file_type()` now. A non-UTF-8 filename is a hard
error rather than a lossy string, because two files must not collapse onto one
indexed name when the two-way coverage check rests on those names.

**Accepted as known — the manifest gate delegates container tolerance to a ZIP
reader.** Both ports skip an archive their reader refuses, so a future
generated package that one reader accepts and the other rejects would be
inspected by one and not the other. The mitigation is the anti-vacuity check
the port added: the two conforming golden packages MUST each contribute a
manifest check, so the gate cannot quietly fall back to inspecting a handful of
stragglers.

**Routed to issue 0028, NOT fixed here — `base[1..]` drops the wrong entry.**
The Go generator slices the sorted base at index 1 to mean "everything but the
manifest". Sorted, index 0 is `assets/descriptors.json`, so the slice drops the
DESCRIPTORS and keeps the manifest. Every traversal and name fixture therefore
ships a manifest inventorying an entry the archive does not contain, on top of
the hostile name it means to isolate. Confirmed against the bytes:

```text
traversal-dotdot.domo entries:
  "domowik.json"
  "ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json"
  "snapshot/state.json"
  "../domowik.json"
  manifest inventories:
    "assets/descriptors.json"     <- absent from the archive
    "ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json"
    "snapshot/state.json"
```

This is the same class as the defect that produced `withoutName` — "the entry
is removed BY NAME; a positional drop removes the wrong entry" — surviving one
function away from the comment that describes it. It is INHERITED, not
introduced: the Rust port reproduces it exactly, which is why the bytes match.

Fixing it would change committed artifact bytes, their recorded digests and the
expected-outcome reasoning for eight fixtures. That is issue 0028's call, not
this port's, and making it here would have destroyed the byte-parity proof this
document exists to give. Routed with the evidence above.

## Provenance

The Rust generator is a behaviour-for-behaviour translation of this
repository's own Go code, under the clean-room policy's rule for first-party
material. No third-party source was consulted or adapted, and the pinned
DEFLATE streams are output bytes already committed to this repository, not
code.
