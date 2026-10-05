# The Rust reader against the hostile corpus (issue 0028)

`testdata/corpus/hostile/` was built for the frozen Go reader. Running the
RUST reader against it tests the FORMAT's threat model rather than one
implementation's habits — and a fresh reader that has never met these fixtures
is exactly the reader that should.

Two harnesses, both tracked and both run by `cargo test --workspace`:

| Harness | Corpus | Fixtures |
|---|---|---|
| `domowik-container/tests/hostile_corpus.rs` | `hostile/container` and below | 37 packages |
| `domowik-document/tests/hostile_framing.rs` | `hostile/framing` | 8 batch and snapshot documents |

## What is asserted, and what is not

**Asserted: every hostile fixture is REFUSED, and none panics.** That is §9's
threat model reduced to the one property checkable over a whole corpus without
reproducing each fixture's expected diagnostic.

**Not asserted: the diagnostic.** `EXPECTATIONS.md` records the exact
diagnostic each fixture should produce; these harnesses check the CLASS. A
fixture rejected for the wrong reason passes here, and saying so is the point —
what the harnesses cannot do is pass by accident, because accepting a hostile
package and dying on one both fail.

**The panic check is not decoration.** `read_package` is the whole public entry
point, so a panic beneath it is a panic a user can trigger with a file.
Catching it turns "the test binary died" into "this fixture panicked".

## The defect this found

**A batch could declare a frontier reaching past its own reserved range.**
`inconsistent-frontier.json` reserves counter 85 and declares 86 for its own
actor — a batch claiming to have observed itself. The F4 closure check enforced
only the LOWER bound ("includes the writing actor's own prior end counter"), so
a frontier running AHEAD of its own batch passed and the batch was merely held.

A frontier is a batch's causal PAST (F1/F3); it cannot contain the batch or
anything after it, and B5 forbids a frontier reaching inside a batch at all.
Both bounds are now enforced, so for a contiguous stream the own-actor entry
must equal `counterBase - 1` exactly. Mutation-proven by
`a_batch_may_not_observe_its_own_counters_or_beyond`.

## Two fixtures the log is not the right reader for

Listed rather than silently skipped, because an exclusion without a stated
reason is indistinguishable from an oversight:

- `name-content-binding-mismatch.json` — its defect is the §7.6 ENTRY NAME
  disagreeing with the content. The log never sees an entry name;
  `domowik-container` covers it.
- `duplicate-batch-id-a.json` — well formed on its own. It is the partner that
  makes `-b` a duplicate, and the harness delivers it first for that reason.

## Why there is a positive control

`every_hostile_fixture_is_refused_without_panicking` asserts that NOTHING was
accepted. An assertion of that shape passes just as happily if `read_package`
rejects everything, valid plans included, and **no mutation can catch that** —
the property it asserts is genuinely true either way.

`a_well_formed_package_is_accepted_by_the_same_check` writes a valid package
and requires it to be read back. Only a case that SHOULD be accepted can show
the check discriminates at all.

The same reasoning gives both harnesses a minimum fixture count: a corpus test
that silently covers nothing is worse than no test, because it reports green
for a tree that is not there. Those guards ARE mutation-proven, by pointing the
harness at a directory with no fixtures in it.

## CORRECTION 2026-09-12 — the container harness is vacuous, and this document overclaimed

Found while adding the §7.2 archive-comment fixture (issue 0028). Everything
above about `domowik-container/tests/hostile_corpus.rs` must be read with this.

**`domowik-container` looks for a manifest entry named `manifest.json`.** The
format's manifest is `domowik.json` — FORMAT-SPEC §7.3's closed-namespace table
and §7.4's opening sentence both say so, and every committed `.domo` package
carries that name.

`crates/domowik-container/src/lib.rs:159`:

```text
const MANIFEST_ENTRY: &str = "manifest.json";
```

In the Rust workspace, `domowik.json` appears only in the corpus generator.
Reader and writer both use `manifest.json`, self-consistently.

**The consequence is that the reader refuses the SUCCESS fixtures too.**
Probed directly:

```text
GOLDEN golden/packages/thin-valid.domo:     REFUSED -> MissingEntry { entry: "manifest.json" }
GOLDEN golden/packages/portable-valid.domo: REFUSED -> MissingEntry { entry: "manifest.json" }
PROBE  hostile/container/nonzero-comment.domo:         MissingEntry { entry: "manifest.json" }
PROBE  hostile/container/nonzero-archive-comment.domo: MissingEntry { entry: "manifest.json" }
PROBE  hostile/container/trailing-data.domo:           MissingEntry { entry: "manifest.json" }
```

Reproduce: a two-line test calling `domowik_container::read_package` on those
paths. `trailing-data.domo` is a conforming package plus sixteen bytes; Go's
`archive/zip` opens it without complaint.

**So `every_hostile_fixture_is_refused_without_panicking` cannot fail.** It
asserts that no hostile fixture is accepted. A reader that accepts NOTHING —
including the two packages whose recorded outcome is `success` — satisfies that
assertion trivially. Its green says the reader did not panic. It says nothing
whatever about §9's threat model, and this document's "37 hostile packages"
line invites exactly the wrong reading.

**Why nothing caught it.** `a_well_formed_package_is_accepted_by_the_same_check`
builds its own package with `MANIFEST_ENTRY` and reads it back with
`MANIFEST_ENTRY`. It is self-consistent by construction and cannot detect that
the constant disagrees with the format. That is ED-0089's shape again: a test
named after a spec row that cannot fail for the reason the row exists.

**What still stands.** The `domowik-document` framing harness reads documents
directly, never through the container, so it is unaffected — including the
`inconsistent-frontier` defect recorded above, which remains a real find.

**Routed, NOT fixed here.** `MANIFEST_ENTRY` is reader code, owned by the Rust
core; issue 0028 owns fixtures. This issue has already had one out-of-scope
reader change accepted with the breach recorded, and the lesson was to route.
AGENTS.md settles the direction if it is asked: the specifications are
normative and the Go tree is not, so the reader is the side that moves. Whether
`manifest.json` was a deliberate departure is not for this document to decide.

**Consequence for the new §7.2 fixture.** `nonzero-archive-comment.domo` is
correct by construction and verified against `archive/zip`, but NO Rust reader
currently exercises it, and neither does `nonzero-comment.domo`. The corpus gap
0029 identified is closed in the corpus; it is not closed in any reader, and
this record refuses to let the fixture's existence be mistaken for that.

