# Issue 0028 — mutation probes for the corpus precondition assertions

Status: **test-harness evidence** under ED-0027. This is not a release, tag,
publication, compatibility approval, or Done claim, and it asserts nothing
about reader, writer, or validator behavior. No product code was modified to
produce it.

Scope note: this file is committed under an explicit orchestrator scope
addition superseding the issue-0028 pre-edit exclusion on `docs/evidence/` for
this file alone. It is the probe record only; every other exclusion in the
agreed matrix stands.

## Why this record exists

Issue 0018's release audit ruled that fixture counts and prose do not
substitute for semantic assertions. The same rule applies one level up: an
assertion that cannot fail is itself prose, and that is not visible by reading
the test source. Each rule below was therefore verified by breaking what it
guards and confirming the intended failure.

The mutations are deliberately **not** committed. Only their outcomes are
evidence, so this file records, for each rule, the mutation applied, the exact
resulting message, and the commit the rule lives in. Every probe was run
against the committed tree with `GOTOOLCHAIN=local` and the Go 1.27
module-cache toolchain first on `PATH` (ED-0030 item 1), and the working tree
was byte-restored from a copy afterwards; `git status --porcelain` was empty
before and after.

## P1 — V2's causal edge is load-bearing

Rule: `requireCausalEdge` in `internal/persist/corpus_semantic_test.go`.
Commit: `e09210e`.

Mutation: delete actor X's entry from `B-later`'s `frontier` in
`testdata/corpus/adversarial/vectors/v2-reordered-delivery.json`, making the
two batches concurrent instead of causally ordered.

Result: `TestProjectionVectors/adversarial/vectors/v2-reordered-delivery`
fails before any subtest runs.

```
corpus_semantic_test.go:172: B-later's frontier does not name A-earlier's
actor 01948f5a-ca77-7a09-9e76-5ace11b12fd2: the two batches are CONCURRENT,
so this fixture is not the causally-ordered vector it claims to be
```

What this establishes: the causal premise is now checked from the fixture
bytes. It was verified separately that **without** this rule all three V2
subtests pass under the same mutation — holding passes via stale discard,
order-equality passes, and the witness still observes the same value — so a
concurrent V7-shaped case could have stood in for V2 undetected.

## P2 — V7's non-observation setup is load-bearing

Rule: `requireNotObserving` in `internal/persist/corpus_semantic_test.go`,
applied to both modify alternatives. Commit: `e09210e`.

Mutation: add `"01948f5a-ca77-7a09-9e76-5ace11b12fd2": 110` to
`modifyBatchFollowing`'s `frontier` in
`testdata/corpus/adversarial/vectors/v7-create-vs-modify-both-orderings.json`,
so Y observes X's create. FORMAT-SPEC §4.7 V7's setup is "Y, **not
observing** X's create".

Result: `TestProjectionVectors/.../v7-create-vs-modify/modify-canonically-follows`
fails.

```
corpus_semantic_test.go:646: modifyBatchFollowing's frontier observes
createBatch's actor 01948f5a-ca77-7a09-9e76-5ace11b12fd2 through counter 110:
the vector requires that it does NOT observe it, so this is a causally-ordered
case, not a concurrent one
```

What this establishes: the vector's own setup is checked rather than assumed.
This mutation preserves **every outcome the harness asserts** — the modify
still applies, `thicknessMm` is still 150, no stale diagnostic appears, and
both arrival orders still agree — while replacing the vector under test. It
was the adversarial reviewer's explicitly stated attack on the first version
of this work, which had no such check.

## P3 — V2's blocked witness pins a value, not merely a difference

Rule: the `executionBlocked.observed` map in
`testdata/corpus/EXPECTATIONS.md`, asserted by the
`blocked-implementation-nonconformance` subtest and shape-validated by
`internal/corpusgen`. Commit: `e09210e`.

Mutation: change the V2 record's
`executionBlocked.observed.finalThicknessMm` from 120 to 130.

Result: `.../v2-reordered-delivery/blocked-implementation-nonconformance`
fails rather than accepting any value that merely differs from the oracle.

```
corpus_semantic_test.go:405: finalThicknessMm observed 120, but the recorded
non-conformant value is 130 and the oracle is 150: this is a THIRD behavior,
not the routed defect — reassess ED-0032 before touching the marker
```

Corroboration from an accident in the same session: reverting an earlier probe
with `git checkout` also discarded the then-uncommitted `observed` block, and
the manifest-side gate failed immediately with

```
corpus_test.go:930: adversarial/vectors/v2-reordered-delivery:
executionBlocked names assertion "finalThicknessMm" but records no observed
value for it
```

so both halves of the rule are enforced — the harness cannot silently accept a
drifted value, and the manifest cannot silently omit the pin.

## P4 — the duplicate-key fixture's byte shape is load-bearing

Rule: `requireRepeatedTopLevelMember` in
`internal/persist/corpus_framing_test.go`. Commit: `9d2a264`.

Mutation: delete the second `"name"` member from
`testdata/corpus/hostile/json/duplicate-object-keys.json`, restoring it to a
document with no duplicate key.

Result: `TestJSONLevelFixtures/hostile/json/duplicate-object-keys` fails.

```
corpus_framing_test.go:110: fixture contains no repeated top-level member
name, so its duplicate-key expectation is not what the reader classifies;
scanned 7 distinct names
```

What this establishes, narrowly: the committed bytes really do contain a
repeated top-level member, checked independently of the decoder under test,
and removing that repetition fails the guard. It does **not** establish that
`invalid-field-syntax` could not also be supplied by some coexisting
unrelated syntax defect — the guard checks that the repetition is present, not
that no other defect is. What it closes is the specific hole issue 0018
routed: the superseded fixture was a *valid* level document with eight
distinct keys, so the reader classified it as no rejection at all and the
record had to be parked as an open question rather than asserting anything.

## P5 — the spanned-disk fixture probes the field it claims

Rule: `TestSpannedDiskFixtureDiffersOnlyInTheDiskField` in
`internal/persist/hostile_container_test.go`. Commit: `a9e6b77`.

Mutation: reproduce the original generator defect — restore EOCD
`thisDiskNumber` to zero and corrupt a `cdOffset` byte instead, at EOCD+18,
which is where the superseded generator patched.

Result: the test fails.

```
hostile_container_test.go:133: the differing byte is at EOCD+18, outside
thisDiskNumber [4,6): FS 7.2 is not what this fixture probes
```

What this establishes: the assertion attributes the reject to
`thisDiskNumber` directly, and the probe confirms it fires on the historical
offset.

It does **not** rest on the claim that the exact-token check was blind to
those bytes — it was not. The historical mutation rejects as
`malformed-length` ("impossible offset", FS §9.4) while the record pins
`malformed-container`, so the token check detected the mismatch correctly and
the record was parked as an unresolved corpus defect rather than passing. The
gap this assertion closes is a different one: the token check cannot
distinguish *among* malformed-container causes. Had the superseded patch
landed on `cdStartDisk`, `entriesOnThisDisk`, or another EOCD field that also
yields `malformed-container`, the record would have passed for the wrong
reason with nothing to catch it. Pinning the field removes that possibility.

## P6 — generated manifests carry a conforming `assetRoot`

Rule: `TestGeneratedManifestsAreConforming` in
`internal/corpusgen/corpus_test.go`. Commit: `48aecaa`.

Mutation: revert the dev-only generator's `buildManifest` to leave
`AssetRoot` nil, so an empty root serializes as `null` again.

Result: the gate fails for every thin package.

```
corpus_test.go:115: golden/packages/thin-valid.domo: manifest assetRoot is
null, but FS 7.4 requires an ascending-by-digest ARRAY; an empty root
serializes as []
```

Background: FORMAT-SPEC §7.4 lists `assetRoot` as a required
ascending-by-digest array. Thin packages emitted `null` until this gate
existed, which meant `golden/packages/thin-valid.domo` — a SUCCESS oracle —
rested on non-conforming bytes, and every hostile base derived from it carried
a second latent defect alongside the one it meant to isolate. The reader
tolerates `null` (`internal/persist/wire.go`: "generators may serialize an
absent table as null"), but that is parser laxity and cannot make a fixture
conforming; the reasoning is the same one that put the complete S1–S5 member
set into the ratio twin's snapshot payload. The parser's tolerance is routed
under ED-0034 — the `wire.go` decision to issue 0029, the acceptance category
to issue 0005 — and is not changed here: no product code was touched.

**The first version of this gate was vacuous and its own probe caught it.**
It checked only that `assetRoot` unmarshalled into a slice, and
`encoding/json` unmarshals a literal `null` into a slice *without error*,
leaving it nil — so the gate passed under the mutation it was written to
catch. The raw token is now rejected explicitly. This is recorded because it
is the clearest available demonstration of why these probes exist: the gate
looked correct, reviewed correct, and asserted nothing.

## What these probes do NOT establish

Stated explicitly so this record is not read as more than it is.

- They prove that five specific assertions fail when their guarded property is
  broken. They do not prove the assertions are complete, nor that no other
  fixture in the corpus rests on an unchecked premise.
- They are point mutations, not exhaustive. P1 and P2 each break one frontier
  entry; other ways of violating the same preconditions were not enumerated.
- **No coverage is claimed for `adversarial/vectors/v2-reordered-delivery`.**
  Its `finalThicknessMm` assertion remains blocked under ED-0032 as an
  issue-0011 implementation nonconformance, and the record contributes nothing
  to the `categoryAudit` or `rowAudit` coverage claims in
  `testdata/corpus/EXPECTATIONS.md`; the corpus gates enforce that
  mechanically. P3 shows only that the *divergence* is pinned exactly — it is
  evidence about the harness, not evidence that the vector conforms. The
  normative outcome remains 150 and the implementation still produces 120.
- The other blocked records listed in that manifest's `residualBlockers` array
  are untouched by these probes and remain routed to their owners.
- P6 checks the SHAPE of `assetRoot` and the presence of `entries`. It does not
  check the §7.4 I1–I4 root invariants, ascending-by-digest ordering of a
  non-empty root, or any other manifest field, and it makes no claim about the
  reader's own manifest validation.
- The six corrected fixtures from `a9e6b77` are regressed primarily by their
  consumers' exact-token outcomes; only the two carrying an accidental-pass
  risk (P4 and P5) have targeted structural assertions, by agreement with the
  adversarial reviewer, to avoid duplicative brittle tests.
- P4 and P5 are described above as retrospective in the limited sense that
  each mutation reproduces a historical defect and now fails. Neither shows
  that the exact-token oracle was insufficient to notice that defect: for P5
  the token check did notice it, and for P4 the record was parked because the
  bytes asserted nothing. What both add is attribution — evidence that the
  fixture exercises the clause it names, rather than merely producing an
  acceptable-looking outcome.
