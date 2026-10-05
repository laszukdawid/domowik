# 0027: Implement CLI operation inspection and compaction

- **Phase:** 1
- **Priority:** High
- **Status:** In Progress (partial scope)
- **Depends on:** 0011, 0029

## Problem

Provide `domowik ops` inspection and `domowik compact` without implying that
local truncation is safe for synchronized peers.

## Acceptance criteria

- [x] `ops` reports batches, actor/counters, frontier, tombstones, and machine-
      readable output without executing untrusted content.
- [ ] Local compaction produces equivalent projection and records snapshot
      coverage/retention metadata.
- [x] Undo/recovery retention effects are explicit before execution.
- [x] Synced-document compaction is refused unless a defined peer-acknowledged
      or snapshot-transfer protocol is active.
- [x] Interrupted compaction leaves old or new readable package.


Activated 2026-08-25 under ED-0022 on branch `issue/0027-ops-compact`. Consumes normative
FORMAT-SPEC sections and the delivered internal/document + internal/persist
material as labeled provisional handoffs (upstreams: 0011, 0029); numeric values
only via named parameters or §9.3 quantities. No Done while upstream issues
remain unclosed under their recorded gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0027-ops-compact` (head `a3204d0`) merged to main by `c4515ef`.
Generated artifacts were refreshed by `46c5216`; integration-only test-helper
collision fix `68e4fc7` and final SBOM refresh `8f3b25e` leave full main
`./scripts/verify.sh` green. Delivered: ED-0023 parse-only persistence
inspection with non-execution/mutation and framing-parity proofs; stable
text/JSON `ops`; local-only plan/execute compaction with explicit pre-write
retention consequences, snapshot/frontier coverage, suffix retention,
projection equivalence, sync refusal, baseline protection, and old-or-new
atomic fault tests. Reviewer verdict: functionally accepted, with the former
SBOM verification gate closed by integration.

This issue remains `In Progress` under ED-0022 while 0011 and 0029 remain
unclosed under their recorded gates. Dependents may consume the delivered
inspection and local compaction implementation as a labeled provisional
handoff.

## Observation routed here 2026-08-26 (ED-0037 amendment)

Issue 0011's pair found that the two package write paths disagree on what
constitutes the retained log: `internal/cli/apply.go:415` writes
`RetainedBatches: proj.Log()` while `internal/persist/smoke_test.go:62`
rebuilds from `view.Retained`. Found while measuring, not repaired.

Related and more consequential: one ordinary save-after-load cycle discards
the entire operation history. `Save` writes a State whose frontier covers
every retained batch, so on reload `ReplayFromSnapshot` skips them all under
§4.5 I2, none enters `Projection.log`, and the write-back then persists an
empty retained set. Measured on a two-actor V2-shaped history:
`view.Projection.Log()` returned 0 with `view.Retained` 2.

Whether that is the intended retention outcome is this issue's question to
answer with issues 0029 and 0031, alongside the write-path disagreement.

## Progress — 2026-08-31 (Rust core) — inspection only

`domowik ops <plan>` lists what a plan retains: batch and operation counts, the
snapshot frontier when one exists, every batch in CANONICAL order with its
position and reserved counter range, each operation described, and any
operations replay discarded as stale.

Canonical order rather than file order is the point. That is the order replay
applies batches in, and listing arrival order would misrepresent what the log
means — two plans with identical content and different arrival histories would
print differently.

**COMPACTION IS NOT DELIVERED**, and it is the larger half of this issue. The
container already carries the §5.1 S2 snapshot frontier and
`replay::project_from` already replays a tail on top of a baseline, so the
mechanism a compaction would write into exists and is tested (vector V6). What
does not exist is §5.1 S4 tombstone retention, without which a compaction
would silently permit resurrection of anything deleted before the snapshot —
and BEHAVIOR-SPEC R8.4.3 forbids silent breakage of undo after compaction.
Compaction must not land before S4. Status stays In Progress.

## Status corrected by ED-0096 — 2026-09-01

**In Progress (partial scope).** `ops` lists batches, actors, counters and the frontier; `compact` folds the log, retains S2 and S4, refuses any compaction that would move the document, and `--dry-run` now states the retention effects BEFORE execution rather than after it. The interrupted-compaction criterion is inherited from the atomic §9.7 write path. NOT delivered: `ops` has no machine-readable output and does not report the retained tombstones, and `compacted()` does not determine whether the document is synced before discarding the log — the delivery-side dominance guard makes that safe for this version, but it is not the precondition the criterion states.

## Delivered 2026-09-12 — ops JSON, tombstone reporting, and the S6 gate

Branch `finalize/0027`, commit `21e31d3`. Closes the two gaps ED-0096 named:
`ops` had no machine-readable output and did not report the retained
tombstones, and `compact` did not determine whether the document was synced
before discarding the log.

**`ops --json`.** Batches, operations, the §5.1 S2 snapshot frontier, every
actor with its watermark, the retained tombstones, the ids the retained log
deletes, and the batches in CANONICAL order — each by batch id and by its §7.6
entry name, with its counter range and its own frontier. `causalPastSize` is a
decimal STRING: §9.3 row L33's range has no IEEE-754 representation, and a JSON
number would be silently rounded by many parsers. Operations are reported by
kind, target and field NAMES; the values an untrusted payload carries are not
echoed into a report a script consumes. The human form gained the actors and
both tombstone lists, and is rendered from the same structure, so the two
cannot drift.

**The compaction gate.** §5.1 S6 forbids dropping a batch a peer might still
need and says in terms that v0.1 attests nothing; §5.2 S7 lets a local-only
document compact. Nothing in a package decides which it is — sharing leaves no
trace in a file — so execution requires `--assert-local-only`, and the answer is
recorded in the plan. Without it the compaction is refused. The flag is not a
protocol and cannot replace one.

The refusal reports S6's three parts — the log is RETAINED rather than dropped,
which condition is unestablished, and the retained range per actor — in text and
in the JSON plan, and exits non-zero. S6 forbids SILENT retention; the exit code
is what stops a script from reading a refusal as a drop.

**The actors are reported, not gated on, and that is a correction.** The first
cut of this work refused every multi-actor history unless each actor was named
with an `--acknowledge-actor` flag, following the frozen Go CLI
(`_reference/go/internal/cli/ops.go:554-558`). The tracker owner rejected it and
was right: S6's condition is about undelivered batches held by PEERS, and an
actor count does not measure that. B2 defines `actorId` as "the single-writer
replica epoch that produced the batch — distinct from user identity; one user
may operate several actors/devices", so a hand-applied batch produces a second
actor on a file that never left the machine. A second assertion about each epoch
demanded more ceremony without establishing anything §5.1 asks for. The actors
are listed in the plan instead, and a multi-actor fold ends with a non-blocking
note naming whose operations were discarded.

Measured while arguing it: `a_routine_local_plan_never_acquires_a_second_actor`
shows the Rust `Plan::open` CONTINUES the highest-watermark actor rather than
minting one per invocation, so an ordinary plan stays single-actor across edits,
undo, compaction and further edits. That is the evidence the first design leaned
on, and it survives as a fact about the writer even though the gate it justified
does not.

**`ops` does not apply the log by default, which is the other correction.**
ED-0023 authorised a parse-only inspection path for this command and states the
reason: reusing the replaying load path "would violate issue 0027 AC1". The Rust
port had quietly dropped that — `cmd_ops` called `project()` to report stale
discards, which applies untrusted operations to a document. Found by adversarial
review. The default report is now built from the manifest and from parsed,
framing-checked, canonically ordered batch headers; `--replay` opts into
application and is the only path to the stale discards and the log's own
deletes. The Rust core splits what the Go tree's `Deliver` did:
`OperationLog::deliver` validates framing and computes causal order — the
structural checking ED-0023's seam explicitly permits — while `replay::project`
applies operations to entities, which is what it forbids. The text form says the
log was not applied rather than printing an empty section.

**The plan is reported before the write.** `--dry-run`, and `--json` in any
mode, emit the phase, the entries the fold would discard, the S2 coverage it
would record, the retained tombstones, the actor accounting, and the R8.4.2/
R8.4.3 undo effect and §9.7 recovery effect. A report produced afterwards
describes a decision the caller no longer has.

**Tests** (`crates/domowik-cli/src/lib.rs`, 184 passing): the single-actor
measurement above; a multi-actor history is reported and compacts on the
assertion; the refusal carries S6's three parts in text and in JSON and still
fails; compaction records the coverage it absorbed; `foldedEntries` really are
the §7.6 names of the batches that go, re-derived in the test from each batch's
own `counterBase` and actor rather than through the helper the report calls, so
a padding mix-up cannot agree with itself; a dry run writes nothing even when
the assertion is present, which is what an ordering bug in the gates would
break; `ops` neither writes nor applies a malformed payload in either mode —
proven under a READ-ONLY DIRECTORY, because a byte comparison passes for a
rewrite of the same package, and the first version of that test did; a
compaction that cannot write leaves the old package byte-identical; and an
interrupted compaction leaves the old package or the new one, reproducing the
§9.7 F2 and F5 filesystem states.

Twenty-nine mutants were executed, each reddening the witness it targeted. Three were rewritten after failing to apply or compile rather than
counted as passes, and one was WITHDRAWN as non-discriminating — it showed that
the read-only test could not see an identical rewrite, which is why the
directory check replaced the byte comparison rather than joining it.

### Found by review, and by asking what review would ask

Two adversarial rounds ran (Codex, `gpt-5.6-sol`, high effort, banner verified).
The first died mid-review when the advisor's workspace ran out of credits, after
producing two findings; the second completed against the full branch diff and
falsified three of five claims. Between them, and a self-directed pass written
while the advisor was down, seven defects were found:

1. **`ops` was applying untrusted operations.** `cmd_ops` called `project()` to
   report stale discards. ED-0023 states in terms that reusing the replaying
   load path "would violate issue 0027 AC1". Now parse-only by default, with
   `--replay` as the opt-in.
2. **The text refusal omitted S6's part (c).** Only the JSON form carried the
   retained range — the one thing a caller needs if the answer turns out to be
   that a peer is waiting for it.
3. **The compaction-defect refusal named the reason but not the retained
   state**, which §5.2 S7 requires of a compaction that cannot proceed at all.
4. **A retained entry the snapshot already covers was counted but not listed**
   by `ops`. §4.5 I2 ignores such an entry silently, so `batches` and
   `canonicalOrder` disagreed with nothing to explain them.
5. **`foldedEntries` omitted entries compaction actually deletes.** Built from
   canonical order, which omits an I2-covered batch, while compaction discards
   the whole `ops/` namespace — so an entry about to be deleted could be absent
   from the list of entries about to be deleted, in the one report a caller
   reads before an irreversible operation. The reviewer found it against the
   fixture this branch had just built for defect 4, which is the sharper half
   of the finding: the same gap had been seen from one side and not the other.
6. **The execution gate ran after the projection.** A package that parses but
   cannot project would have answered with a projection error rather than the
   §5.1 S6 refusal the command owes. The gate needs only the batch headers and
   now runs first.
7. **A write failure named the reason but not the retained state**, and under
   `--json` was not JSON.

Each is fixed with a mutation-proven witness, except the one below.

### ED-0023's boundary: ruled as ED-0107 — 2026-09-12

The second review round's HIGH finding was that ED-0023 forbids its seam to
"construct document operations, replay a snapshot, call Deliver, or mutate
state", while the default `ops` path calls `Package::log()`, which calls
`OperationLog::deliver`. This issue routed it up rather than deciding it, and
the ROOT ORCHESTRATOR ruled after reading both functions, under delegated
bounded authority. It is not an owner ruling and the owner has not reviewed
this code.

[ED-0107](../EXECUTIVE-DECISIONS.md#ed-0107-owner-direction--a-bounded-rust-amendment-to-ed-0023s-inspection-seam):
on the default path, materialising typed INERT operations and private
framing/ordering metadata is allowed; document execution, entity projection,
validation callbacks, shared or session mutation and file writes stay
forbidden. It supersedes the Go-symbol "call Deliver" wording for this seam
only, leaves the no-execution requirement untouched, and keeps one security
parser rather than forking `read_package`.

What the amendment asked for is delivered here:

- `application_invalid_data_inspects_but_replay_rejects_it` is ED-0023's
  differential witness — a structurally valid, application-invalid batch that
  the default path reports by kind and field name with no verdict, and that
  `--replay` rejects as a malformed payload. Same package, two paths, an
  observable difference rather than a claim about call graphs.
- `default_path_never_applies_the_log` guards EVERY default output, text and
  JSON, against carrying a replay verdict.
- Both redden when the application call is hoisted out of its `--replay`
  branch, which is the regression they exist for.

No counterexample was found while implementing it. The one place the default
path could reach application is the single `pkg.project()` call inside the
`replay` branch, and the comment there says so.

**The assertion is not attestation.** `--assert-local-only` records an
operator's answer about locality. It is not peer attestation, §5.1 S6 supplies
none in v0.1, and nothing in the code or the messages describes it as one.

### §9.6 degraded compaction, found by audit and fixed — 2026-09-12

`cmd_compact` reads a package, folds it and writes it without ever building a
`Plan`, so it never reached `Plan::save`'s `require_editable()` — the gate that
enforces BEHAVIOR-SPEC §9.6. A package with an unresolvable REQUIRED asset is
read-only degraded, where "interactive editing and normal saving are
unavailable", and a compaction is a normal save that discards the log
irreversibly. The audit probe folded six batches out of such a package and left
it degraded.

Execution is now gated on `Package::open_mode_from_embedded`, the one place
§9.6's matrix is interpreted; re-reading the descriptors here would have been a
second answer to "does this asset block editing". Inspection is untouched, which
§9.6 requires in terms: `ops` in both modes and `compact --dry-run` still work
on a degraded package, and an already-compact one is still a no-op because
nothing is written. The refusal names identity, requirement level, the levels
using the asset and the remediation, and carries S7's retained state and range.

The gate runs BEFORE the §5.1 S6 locality gate and before any projection: no
flag clears it, so advising `--assert-local-only` first would be advice that
cannot work.

Five witnesses and four mutants, including an optional-missing positive control
that a gate looking for absent bytes rather than reading the matrix would fail.

### Why criterion 2 is NOT ticked

The projection-equivalence half is delivered and proven; the metadata half is
not. `compacted()` records §5.1 S2 coverage and S4 tombstone ids, and nothing
else — S1's `createdPositions` and S3's `winnerPositions`/`batchPositions` are
absent, and S4's entries are bare ids rather than the deleting and creation
positions S4 specifies.

The criterion says "records snapshot coverage/retention metadata" without
enumerating members, so it could be read as satisfied by what is stored. That
reading is available and is not taken: it would narrow the acceptance text to
whatever the code already does, which is how a requirement disappears without
anyone deciding to drop it. The criterion stands as written and stays unticked
until the snapshot metadata is independently satisfied.

Nothing in this branch turns on that. The CLI reports and gates against the
encoding that exists, and the gap is upstream, below.

### Upstream, and NOT implementable here

`Package::compacted` retains §5.1 S2 and S4 only. **S1 `createdPositions` and
S3 `winnerPositions`/`batchPositions` are not INTERPRETED** — since
persist0029's §7.5 reshape a conforming package's S3 and S5 survive a
write-back in `Package::snapshot_unknown` (§8.4 U2) rather than being dropped,
but nothing reads them, nothing writes them, and a compaction this version
performs produces neither. The operation log instead REFUSES any later batch
concurrent with the snapshot
(`OperationLog::check_dominates_snapshot`) — precisely the case those positions
exist to resolve. S4's tombstones are a bare id list rather than the
`entityId` + deleting position + creation position S4 specifies, so I5 branches
1 and 2 cannot be told apart after a compaction.

This is not deferred CLI work. Writing a conforming S3 needs the named
parameter `lwwHorizon`, which FORMAT-SPEC marks Provisional with no value
asserted, and ED-0096 states that it "decides what §5.1 S1/S3 metadata a
snapshot must retain, so it is not separable from whether `compact` conforms".
Inventing the constant is forbidden. The work therefore sits with the snapshot
encoding in `domowik-format`/`domowik-container` and the replay that consumes
it (issue 0011), behind the fixation owned by issues 0004/0005. The CLI's
reporting and gating are complete against the encoding that exists, and the
`ops --json` shape has room for the positions when they land.

Two other items stay upstream: R8.4.3's live-stack truncation diagnostic has
nothing to fire on in a CLI, whose session lives for one command (issue 0011
owns the `Session` side), and the multi-actor case reaches `compact` only
through `apply`, so any future sync protocol replaces the assertion flags
rather than extending them.

### Verification owed

`scripts/verify.sh` probe P14 needs one flag: `"$CLI" compact
build/ci/probe.domo` becomes `"$CLI" compact build/ci/probe.domo
--assert-local-only` at both execution sites. The `--dry-run` lines and the
`already compact` line are unaffected — a package with no batches returns
before the gate. CI owns that file; the change was sent to the orchestrator
rather than made here at first; the orchestrator then authorised it and it is
committed as `d359ed9`, with the whole patched P14 body executed against the
built binary before it landed.
