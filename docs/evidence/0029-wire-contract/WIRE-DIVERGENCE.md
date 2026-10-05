# The Rust container does not write the format the spec describes

**Issue:** 0029
**Date:** 2026-09-12
**Host:** darwin 25.6.0, arm64
**Reproduced on:** `crates/domowik-container/src/lib.rs`, worktree `finalize/0029` at `06ce1f9`

## What was found

`crates/domowik-container` reads and writes a wire format of its own. It is not
the v0.1 package of FORMAT-SPEC §7, and the two have no entry name in common
except `ops/` and `blob/`. The reader therefore refuses every conforming
package in this repository, and accepts nothing any other tool produces.

Nothing about this is subtle once the reader is pointed at the tracked corpus.
It had not been pointed at it for the manifest: `tests/hostile_corpus.rs`
asserts only that hostile fixtures are refused, and they are — for the wrong
reason, before any check it means to exercise.

## The divergence, item by item

| | FORMAT-SPEC | `domowik-container` | Where |
|---|---|---|---|
| W1 | manifest at `domowik.json` (§7.3 closed namespace, §7.4) | `manifest.json` | `lib.rs:159` |
| W2 | snapshot at `snapshot/state.json` (§7.5) | `document.json` | `lib.rs:160` |
| W3 | `formatVersion` is the `MAJOR.MINOR` string; v0.1 emits `"0.1"` (§8.1 V1) | `format_version: u32`, emitting `1` | `lib.rs:109,115` |
| W4 | `profile` required, `"thin"` or `"portable"` (§7.4, §7.9) | absent | `lib.rs:114-146` |
| W5 | `assetRoot` required, ascending-by-digest array of objects (§7.4) | `assets: Vec<String>`, omitted entirely when empty | `lib.rs:137` |
| W6 | `entries` required: per-entry `{sha256, byteLength}`, "the authoritative integrity layer" (§7.4) | absent | — |
| W7 | the snapshot frontier of record is S2 inside `snapshot/state.json`, and "no other location may carry or shadow it" (§7.5) | `Manifest::snapshot_frontier`, in the manifest | `lib.rs:128` |

W1 and W2 are the wire names the dispatch names. W7 is the one that cannot be
fixed by renaming an entry: §7.5 forbids the location, not the spelling, so the
frontier has to move into the snapshot section whatever the manifest is called.

W3 matters more than a type mismatch looks. §8.2 V3 makes the reader's
acceptance gate a comparison of schema minors. A `u32` has no minor to compare,
so the gate in `read_package_with_limits` (`lib.rs:867`) is comparing a build
counter and calling the result `unsupported-version`.

## Reproduction

A throwaway probe (`tests/zz_wire_probe.rs`, not committed) ran
`domowik_container::read_package` over every tracked `.domo` fixture and printed
the error. Command:

```text
cargo test -p domowik-container --test zz_wire_probe -- --nocapture
```

Result: **40 fixtures, 0 accepted.**

- **Both golden conforming packages are refused.**
  `golden/packages/thin-valid.domo` and `golden/packages/portable-valid.domo`
  both fail with ``is missing the required `manifest.json` entry``. AC1 asks for
  a round trip "across all golden fixtures"; the Rust reader cannot open one.
- **30 of 36 hostile fixtures fail the same way**, on the missing manifest,
  before reaching the check each fixture exists to provoke. The remaining 6
  (`bad-signature`, `empty-file`, `overlapping-regions`, `spanned-disk-number`,
  `truncated-eocd`, `truncated-header`) are refused by the `zip` crate at
  archive level, which is a real rejection but not one this crate performs.

## The corpus suite is vacuous, proven by mutation

`every_hostile_fixture_is_refused_without_panicking` cannot fail for any defect
in the code it is supposed to guard, because the fixtures never reach that code.
Two guards were deleted at once and the suite was re-run:

- **M1** — the row-L6 entry-count guard on the reader path, `lib.rs:857-864`,
  deleted outright.
- **M2** — blob digest verification on read, `lib.rs:938-944`, deleted so any
  bytes are accepted under any `blob/sha256/...` name.

```text
$ cargo test -p domowik-container --test hostile_corpus
running 3 tests
test the_corpus_is_present_and_not_empty ... ok
test every_hostile_fixture_is_refused_without_panicking ... ok
test a_well_formed_package_is_accepted_by_the_same_check ... ok
test result: ok. 3 passed; 0 failed
```

Green with both guards gone. The positive control passes too, because it writes
its own package with the writer and reads it back, so it never touches the
corpus either. The mutations were reverted and the suite re-run green before
anything else was done.

This is the ED-0089 shape: a test named after the threat model, certifying
conformance it never tested. It is worse than ED-0089's case in one respect —
that one restated a literal, this one runs real code against real fixtures and
still discriminates nothing.

## Compatibility inventory, done before proposing any change

The question the dispatch asks is whether renaming the entries strands files
that already exist. Evidence says no, and each part of that was checked rather
than assumed.

1. **No released schema exists.** FORMAT-SPEC §8.2 V6 records v0.1 as not
   publicly released. ADR 0007 states the evidenced predecessor set is empty.
   ED-0051 (via issue 0031) deleted `LatestReleasedSchema` and made the
   migration chain unfillable, because §8.2 V4 lets a step originate only at a
   released schema. So no migration is owed to anything, and none could be
   written if it were.
2. **No release exists to have shipped a writer.** `git tag` is empty. There is
   no tag, no published binary, and no distribution record.
3. **No tracked artifact uses the runtime names.** Every tracked `.domo` is
   either corpus (`domowik.json`) or Go-era size evidence
   (`docs/evidence/0035-size-corpus/packages/*.domo`, also `domowik.json`).
   Neither evidence package is read by any Rust test: `size_corpus.rs` writes
   fresh packages and measures those.
4. **The names are not spread through the tree.** `MANIFEST_ENTRY` and
   `DOCUMENT_ENTRY` are private constants used only inside
   `crates/domowik-container/src/lib.rs`. No other crate names either string.
   The 40-odd call sites in `domowik-cli` go through `read_package`,
   `write_package` and `Plan`, none of which mention an entry name.

**The only producer of `manifest.json` packages is this build.** Files written
by a developer running the current CLI are the entire population at risk, and
they are not a released format, a supported schema, or user data anyone was
promised would keep opening.

## What this does not settle

The inventory establishes that nothing is owed a migration. It does not
establish what the reader should do when it meets one of those files, and that
is an owner call rather than a worker's, because every available answer trades
something:

- **Refuse.** One wire format, no legacy branch. A developer's working file
  stops opening with a diagnostic naming the cause.
- **Read the old names, write only the new.** A one-release read-side shim.
  It contradicts §7.3's closed namespace, under which `manifest.json` is
  `invalid-entry-name` and the package has no manifest, so the shim would be a
  documented deviation from a normative rule rather than a courtesy.
- **Accept both, indefinitely.** Rejected here without waiting for a ruling.
  The dispatch forbids silently accepting both, and §7.3's namespace is closed
  precisely so a reader cannot acquire dialects.

The second option is the only one that needs a decision, and it needs one from
whoever owns §7 — issue 0005 — not from this issue.

## Scope note

W1 and W2 are this issue's to fix. W3 through W7 are the same defect seen from
further back: the manifest is not a §7.4 manifest and the snapshot is not a §7.5
snapshot, so renaming the entries alone produces a package that is conforming in
its file names and nothing else. Recorded here so the next round does not
mistake a rename for conformance.
