# What the developer layout carries, and what a canonical reader would reject

**Issue:** 0029
**Decision:** ED-0108, step 1 — assess the legacy envelope and unknown-data
preservation before converting anything
**Date:** 2026-09-12
**Worktree:** `finalize/0029` at `287053b`

ED-0108 requires this assessment before the reader moves to the §7 names, and
requires that no field be dropped silently. The second requirement is already
violated, by the code that exists now, in three places.

## Unknown data is dropped at every level the container owns

A package was written by the real writer, every entry was then decorated with an
unknown field the way a later writer would leave one, and the result was read
and written back. The probe is in the scratchpad, not committed; it built its
shapes from the real types rather than hand-rolled JSON, so nothing here depends
on guessing a serialization.

| Envelope | Owner | Unknown field survives? |
|---|---|---|
| `manifest.json` | `Manifest`, this crate | **dropped** |
| `document.json`, top level | `Document`, `domowik-format` | **dropped** |
| `ops/<base>-<actor>.json` | `Batch`, `domowik-document` | **dropped** |
| a level inside the document | `Level`, `domowik-format` | preserved |

The nested entity types carry `#[serde(flatten)] unknown` maps and round-trip
correctly. The three envelopes do not have them, so a field arriving at the top
of a manifest, a document or a batch is read, discarded, and written back as
absent. The manifest came back as `{"formatVersion":1,"generator":"domowik
0.1.0"}` with the decoration gone and no error raised.

FORMAT-SPEC §8.4 U2 requires unknown optional data preserved with identical
meaning, or write-back refused. Neither happens. This is a §8 defect on its own
terms, and it is independent of the entry names.

**It also blocks the converter.** A conversion tool built on today's types would
read a developer's plan and write a canonical package with the unknown fields
missing, which is precisely the silent drop ED-0108 prohibits. The flatten maps
have to land before the converter, not after it.

## What the canonical reader rejects beyond the two names

Renaming the constants produces a reader that still cannot open a conforming
package, because the manifest and snapshot it would then look for are not the
§7.4 and §7.5 documents. Measured against `golden/packages/thin-valid.domo`:

| | Conforming package | Today's types | Consequence for a renamed-only reader |
|---|---|---|---|
| `formatVersion` | `"0.1"` | `u32` | parse error: a string is not a `u32` |
| `profile` | `"thin"` required | absent | unknown field, dropped |
| `assetRoot` | array of `{digest}` objects | `assets`, array of strings | parse error on a `Vec<String>` |
| `entries` | required per-entry `{sha256, byteLength}` | absent | the §7.4 integrity layer stays unimplemented |
| snapshot body | S1–S5 (`entities`, `frontier`, `winnerKeys`, `tombstones`) | a bare `Document`, S1 only | parse error: none of the §5.1 members are `Document` fields |
| frontier location | S2 inside `snapshot/state.json` | `Manifest::snapshot_frontier` | §7.5 forbids the location, so no rename fixes it |

So the golden fixtures do not become supported by a rename, and reporting them
as supported on that basis would be false. The snapshot section in particular is
not the document: §7.5 puts the §5.1 snapshot there, of which the projection is
one member. A `SnapshotSection` type has to exist, with the frontier inside it.

The frontier move is also a dependency for someone else. Issue 0004's ED-0105
horizon proposal defines the retained horizon as exactly what S2 covers, which
is only implementable once S2 is the single frontier of record, so this is a
precondition for that work rather than a conflict with it.

## What a developer's existing file actually is

Worth stating plainly, because it decides what the diagnostic should say. A file
written by the current build is a ZIP holding `manifest.json` and
`document.json`, with correct `ops/` and `blob/` names, an integer
`formatVersion` of `1`, and no `profile`, `assetRoot` or `entries`. It declares
no schema identifier in the §8.1 sense, so there is nothing for the §8.2 V3
acceptance gate to compare and nothing a migration could originate from.

That is why ED-0108's "unpublished-layout repair" is the right label and
"migration" is not. A migration under §8.2 V4 originates at a released schema;
this originates at a build artifact. Calling it a migration would fabricate a
released-schema relationship the evidence does not support, and would also make
the §8.2 chain look fillable when ED-0051 established it is not.

## Order of work this implies

1. Unknown-preservation maps on the three envelopes. Without them every later
   step drops data.
2. The canonical types: §7.4 manifest, §7.5 snapshot section with S2 inside it.
3. The reader on the §7.3 names, with `unsupported-development-layout` for a
   package carrying the developer names — actionable, and distinct from the
   `missing-manifest` a genuinely manifest-less package gets.
4. Conversion to a NEW path, sharing the parser and validation, never
   overwriting the source and never running on open.
5. Diagnostic-specific corpus tests, each proven to fail when its guard is
   removed.

Step 5 is where the existing suite's problem gets fixed. Asserting the
diagnostic class per fixture is what makes a corpus test discriminate; asserting
only that something was refused is what let the current one stay green with two
security guards deleted.
