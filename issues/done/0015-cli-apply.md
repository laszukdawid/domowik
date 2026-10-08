# 0015: Implement CLI `apply`

- **Phase:** 1
- **Priority:** High
- **Status:** Done
- **Depends on:** 0011, 0029

## Current disposition — 2026-10-05

Close: the last recorded defect, L11 depth enforcement, is implemented and
tested.

`read_external_batch` uses Limits.max_nesting_depth, check_nesting_depth and
the guarded parser. Current tests admit depths 127/128, reject 129 before
mutation, and preserve unknown fields and file bytes. CLI/container tests
passed locally on 2026-10-05. Earlier reopening notes are historical;
batch-from-stdin/multi-batch remain optional future scope, not closure gates.

This disposition supersedes older progress/status claims below; original
criteria and dated evidence are retained, not waived.

## Original problem

Apply operation scripts from a file or stdin. Define whether scripts contain
commands or trusted stored operations; do not accept spoofed actor/counter
metadata accidentally.

## Acceptance criteria

Reconciled against the Rust implementation on 2026-10-05. The September
sections below retain the earlier closure/reopening history. All criteria
are now satisfied for the documented single-batch invocation.

- [x] External script schema and stored operation schema are separated or
      their trust/assignment rules are explicit. Not separated — `cmd_apply`
      reads the FORMAT-SPEC §4.9 stored shape — so this is met on the
      trust-rules arm, and those rules are in code rather than in prose.
      `LogStore::check_framing` rejects an empty batch, a `counterBase` below
      1, a zero frontier member, and any `setFields` naming `id` or `type`,
      "because a hostile external batch never passes through the command
      layer". `Session::accept_external` then projects and validates on a
      scratch log before anything lands (§4.3 row 1).
- [x] Actor identity, counters, batch IDs, duplicates, and idempotency are
      defined. `LogStore::deliver`: a per-actor watermark, a range wholly at or
      below it returning `Delivery::Duplicate` (I2), a `batch_id` set, and a
      batch whose prerequisites are absent returning `Delivery::Held` rather
      than being applied or refused.
- [x] Transaction scope is explicit (per batch or whole invocation), including
      later-batch failure behavior. One batch per invocation, so the two
      scopes coincide, and the §9.7 save path makes the write atomic. **The
      later-batch clause has no subject**: multi-batch invocation is not
      delivered, and was recorded as a convenience rather than held open. If
      it is ever added, this criterion acquires real content again.
- [x] `--dry-run` produces no write and reports projected changes/warnings.
      Reports the projected entity-count change and the document's warnings.
      `a_dry_run_apply_reports_the_projection_and_writes_nothing` asserts the
      plan is byte-identical afterwards, not merely that it re-projects the
      same.
- [x] Input size, operation count, depth, and numeric bounds are enforced.
      Rechecked 2026-10-05: declared/read byte limits and L23 checks remain;
      validation enforces numeric bounds through `accept_external`. The L11
      guard now uses `Limits.max_nesting_depth`, admits 127/128 and rejects
      129 before parsing or mutation. Corresponding CLI tests pass. The dated
      reopening below records the defect before this fix.


Activated 2026-08-25 under ED-0022 on branch `issue/0015-cli-apply`. Consumes normative
FORMAT-SPEC sections and the delivered internal/document + internal/persist
material as labeled provisional handoffs (upstreams: 0011, 0029); numeric values
only via named parameters or §9.3 quantities. No Done while upstream issues
remain unclosed under their recorded gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0015-cli-apply` (head `75e33d8`) merged to main by `39ddbd1`
with SBOM refresh `f7ccbf1`; full main `./scripts/verify.sh` exits 0.
Delivered: a command-only external script schema with system-assigned actor,
counters, and frontier; canonical batch-id idempotency; contiguous counters
after duplicate classification; whole-invocation transactional delivery;
write-free dry-run; per-batch outcome diagnostics; and named §9.3 input,
operation, depth, and numeric limits. Reviewer verdict: ACCEPT after resolving
counter gaps, dry-run baseline access, report completeness, and rejection-code
preservation.

This issue remains `In Progress` under ED-0022 while 0011 and 0029 remain
unclosed under their recorded gates. Dependents may consume the delivered CLI
apply implementation as a labeled provisional handoff.

## Progress — 2026-08-31 (Rust core)

`domowik apply <plan> <batch.json>` delivers an external operation batch.

The batch is read as the FORMAT-SPEC §4.9 shape. Framing anomalies reject
before any effect; a batch whose causal prerequisites are absent is HELD, and
the command SAYS SO rather than reporting success — F3 makes holding a normal
outcome, and reporting it as applied or as rejected would both be wrong. A
second delivery of the same batch is ignored per I2/V1.

BEHAVIOR-SPEC R8.3.1 makes an accepted external batch clear both local undo
stacks and R8.3.3 makes it dirty the document; both are `Session`'s behaviour
and are tested there. This command exists partly so those rules are exercised
by something a user can run.

Not delivered: applying a batch from stdin, applying several batches in one
invocation, and any `--dry-run` preview. Status stays In Progress.

## Status set by ED-0095 — 2026-09-01

**Done.** `apply` delivers external batches and reports accepted, duplicate or held. `--dry-run` reports the projected entity-count change and writes nothing (proved byte-identically, not merely by re-projection). The untrusted batch file is bounded three ways before it can cost anything — declared size before the read, actual bytes read, and operation count before the batch reaches the session — because the `apply` path does not go through the package reader that enforces §9.3 elsewhere. Batch-from-stdin and multi-batch invocation are conveniences, recorded as such rather than held open.

## Checklist reconciled, and one criterion is not met — 2026-09-12

**Location.** The status line has read `Done` since ED-0095 (2026-09-01) but
the file sat in `issues/` for eleven days, and `issues/README.md` linked it
there. Moved to `issues/done/` with the index updated in the same commit.
`domowik-tracker-check` now fails a terminal status whose file is in the wrong
directory, and fails an index row whose link does not name the file's actual
location, so this cannot recur silently. Neither check existed before: the old
one compared the file's status against the index's and nothing else, which both
agreed on throughout.

**The boxes were never ticked.** All five stayed `- [ ]` through the ED-0095
closure and the ED-0096 audit, because both wrote their verdict in prose and
left the checklist alone. Four are now ticked against named code.

**The fifth is genuinely unmet, and ED-0096 slightly overstates it.** ED-0096
says issues 0015 and 0037 "had their unmet criteria **implemented** rather than
argued away". That is true of the bounds it was looking at — declared size,
bytes read, operation count — and not true of depth, which nobody chose a value
for on this path. A hostile batch file is protected today by a library default,
which is a different thing from a bound the format picked.

**The `Done` status predates this discovery, and that is not authority to
waive it.** ED-0095 marked this issue Done on 2026-09-01; the depth gap was
found on 2026-09-12. A status set before a defect was known says nothing about
the defect. The criterion is not discharged by being routed somewhere else, and
this issue is not closed on the strength of a status that was set without it.

The order of operations, so this resolves rather than circulates:

1. **Issue 0005 names a normative nesting-depth bound** in §9.3, as it owns the
   row. `serde_json`'s 128-level default is a library's choice, not the
   format's, and a conformance criterion cannot be met by an implementation
   detail nobody chose.
2. **`Limits` gains the field and `apply` enforces it**, the way it already
   enforces declared size, bytes read and operation count.
3. **This issue is then re-checked against the criterion.** If it is met, the
   box is ticked and `Done` stands on evidence rather than on inheritance. **If
   it is still unmet, this issue reopens.**

Issue 0028 additionally owns the hostile fixture: no deeply nested batch has an
expectation today, which is why nothing caught this.

Recorded at this length because an unmet criterion under a `Done` status is
precisely what ED-0096 was written about, and repeating it quietly would be
worse than the original.

## Reopened — 2026-09-12

**Status changed from `Done` to `In Progress (partial scope)`, and the file
moved back to `issues/`.** The historical closure notes above are preserved
unchanged, including ED-0095's `Done` verdict and the ED-0096 record, because
they are the account of what was believed when.

The reason is the fifth acceptance criterion, and it is decided on today's
evidence rather than deferred.

**Correction to an earlier claim in this file.** An earlier note said no
FORMAT-SPEC row names a depth bound. That was wrong. **§9.3 row L11 is
Ratified** — "JSON/object serialization nesting depth | 128 | ... reject when
depth would exceed 128 (**depth == 128 parses**)" — and §2 line 320 lists it
among the parser ceilings. The contract exists and always did.

The criterion is unmet for two narrower reasons, both worse than a missing
contract:

1. **Nothing enforces L11 on this path.** `Limits` in
   `crates/domowik-container/src/lib.rs` has four fields — entry count, entry
   bytes, total operations, operations per batch — and no depth field. A
   ratified ceiling is left to whatever `serde_json` defaults to.
2. **The library's limit is off by one against the row, in the rejecting
   direction.** L11 requires depth 128 to PARSE. Issue 0004's owner measured
   `serde_json`'s default as REJECTING depth 128, so `apply` refuses a document
   the format requires it to accept. L11's notes cite "serde_json default
   recursion limit 128" as the precedent for the value, which is how an
   inclusive spec bound and an exclusive library limit came to be treated as one
   number.

So this is a conformance defect, not only a gap — which makes the reopening
firmer, not weaker.

The earlier reconciliation said this would be re-checked after a contract
existed, and left `Done` standing meanwhile. That was the wrong order twice
over: the contract already existed, and a status is a claim about the issue as
it stands today.

**The guard and the off-by-one belong to issue 0004** (`format0004`, `w63:p2`).
Issue 0028 owns the hostile fixture: no deeply nested batch has an expectation
today, which is why nothing caught either.

Everything else this issue delivered is unaffected and still stands — the four
other criteria remain ticked against named code.
