# 0031: Implement format migrations and backups

- **Phase:** 1
- **Priority:** High
- **Status:** Blocked
- **Depends on:** 0005, 0028, 0029

## Released-schema reconciliation accepted 2026-08-29 (ED-0051)

Branch `issue/0031-released-schema-honesty` (`ae759bb`..`a139681`, six
commits) merged to main by `5ab9948` with integration SBOM refresh `7456539`.
Full main `./scripts/verify.sh` exits 0 with a clean tree, run independently
by the executor, the reviewer, and the orchestrator.

This work did not exist before 2026-08-29: issue 0005's §8 text recorded that
v0.1 is UNRELEASED, and `internal/migration` asserted the opposite in three
places. The root cause was ONE table answering THREE questions that §8 answers
differently, so the fix separates three spec-anchored concepts rather than
rewording comments: **EMITTED** (§8.1 V1) as `const emittedSchema = "0.1"`;
**SUPPORTED** (§8.2 V3) as `supportedSchemas = {"0.1"}`, the concept
`internal/persist` already had and already called reader-supported; and
**RELEASED** (§8.2 V4/V5/V6), now EMPTY. `LatestReleasedSchema` is deleted —
with an empty released set it had no true answer, and its one caller wanted
the emitted label — and no `LatestSupportedSchema` replaced it, because
nothing normative says the last supported label is the emitted one.

A migration `Step` must now ORIGINATE at a released schema, so the shipped
chain is **unfillable** rather than merely empty: no migration can be added
without first recording a release. That is strictly stronger than the
`len(chain) == 0` assertion it replaces, which was a loop over zero entries —
a vacuous protection replaced by a real one, in an honesty test the brief
explicitly warned must not be weakened. A latent panic was also fixed in
scope: `validateRegistry` indexed its own error-formatting path out of bounds,
proved by execution rather than analysis.

Six files, 758 insertions and 158 deletions, confined to
`internal/migration/{migration.go,migration_test.go}`,
`internal/cli/migrate.go` and the three SBOMs. Thirteen mutation proofs, each
with a control and a stated alternate cause under ED-0053. The executor
reported EIGHT verification runs, seven green and one red — the red being a
pre-commit run correctly aborted by the release-inventory gate — on the
principle that a ledger listing only green runs is not a ledger.

Adversarial review (r0031, `openai/gpt-5.6-sol` at max effort): **ACCEPT** of
this dispatch scope, after fourteen findings, all resolved. It replayed all
thirteen mutations in an isolated detached worktree and confirmed each
produced the claimed discriminating red, with witness-fail/control-pass shown
in the same mutant run for two of them. It withheld agreement across four
pre-edit rounds, rejecting an incomplete two-way split, a non-discriminating
unfillability mutation, an incoherent report seam, and an overcorrection to
"latest supported" — a second false claim replacing the first.

Recorded by the executor without being asked: findings R11, R12, R13 and a
self-caught verification-count error were all cases where the CODE was right
and a SENTENCE about the code was wrong, and R13 was introduced by the
sentence written to fix R11 — ED-0041 instance 4 recurring four times inside
one issue, and the part of the change with no test behind it.

Status stays **In Progress** under ED-0022: issues 0028 and 0029 remain
unclosed, V4 positive-migration coverage still requires a specified successor
schema and its corpus, and §8.2 V6's empty predecessor set means no positive
migration exists to test.

Routed, not fixed: `AUDIT.md` lines 24-47, whose citation of the old released
set is now stale though its conclusion strengthens; ADR 0007's
"while 0.1 is the sole publicly released schema" opening; corpus wording in
`migration/older-minor-unknown-label.json`; and a pre-existing LOW
path-traversal Snyk finding in `internal/cli/apply.go` dating to `40492c5`,
outside this diff. Withdrawn under orchestrator correction and recorded as
withdrawn: `persist/writer.go:346` is NOT a defect but the most faithful
implementation of §8.1 V1; what was found is a testability limitation, now
recorded beside the harness's non-claims.

## Problem

Implement migrations for every public pre-v0.1 fixture Domowik claims to
support and the backup/rollback behavior promised by the versioning policy.

## Acceptance criteria

- [ ] Migrations are explicit, ordered, idempotent where practical, and tested
      against the compatibility corpus.
- [ ] Original files are preserved until migrated output validates and atomic
      replacement succeeds.
- [ ] Unsupported or lossy migrations stop with actionable diagnostics; no
      unknown data is silently discarded.
- [ ] CLI can report the source/target schema and perform dry-run validation.


Activated 2026-08-25 under ED-0022 on branch `issue/0031-migrations`. Consumes normative
FORMAT-SPEC sections and the delivered internal/document + internal/persist
material as labeled provisional handoffs (upstreams: 0005, 0028, 0029); numeric values
only via named parameters or §9.3 quantities. No Done while upstream issues
remain unclosed under their recorded gates, except by explicit re-scope.

## Partial handoff accepted (2026-08-25)

Branch `issue/0031-migrations` (head `0dfa387`) merged to main by `cb0b9a6`
with SBOM refresh `1596574`; full main `./scripts/verify.sh` exits 0.
Delivered: explicit ordered migration registry with whole-registry integrity
validation, corpus-backed rejection/unknown-data checks, validate-before-
replace backup/rollback through the persistence save state machine,
fault-injection tests, and a write-free CLI dry-run reporting source/target
schema. Reviewer verdict: Conditional ACCEPT after fixing registry validation
before path selection.

This issue remains `In Progress`: no successor schema exists, so no positive
migration is fabricated or claimed. V4 stays a machine-readable blocked open
question pending a specified successor schema and corresponding corpus. ED-0022
also prevents Done while 0005/0028/0029 remain unclosed. Dependents may consume
the delivered migration framework as a labeled provisional handoff.

Issue 0018's release audit (`ccfe953`) confirms that the production migration
chain is honestly empty and that the repository has no independently evidenced
pre-v0.1 public release artifact. This is not positive migration coverage: a
future predecessor claim still requires both that release evidence and a
passing migration corpus before V4 or this issue can close.

## Status corrected by ED-0096 — 2026-09-01

**Blocked.** Completion is genuinely blocked: FORMAT-SPEC V6 records that no 0.N schema has been publicly released, so the predecessor set is EMPTY and there is nothing to migrate from. The migration FRAMEWORK, however, is unstarted work rather than blocked work, and the first release under issue 0018 creates the obligation.
