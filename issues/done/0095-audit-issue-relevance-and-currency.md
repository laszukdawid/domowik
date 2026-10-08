# 0095: Audit issue relevance and currency

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0094

## Scope

Owner-requested 2026-10-08 audit of all 94 preceding issue records, including
terminal history and nonterminal work. Check current status, dependencies,
acceptance criteria and named evidence against the Rust/frontend tree. Correct
stale current summaries and tracking contradictions without erasing historical
records or implementing unrelated product work.

## Acceptance criteria

- [x] Every preceding issue has an explicit audit disposition and named basis.
- [x] Stale current claims, duplicate ownership and dependency/status conflicts
      are reconciled; unfinished obligations retain an owner and next action.
- [x] Historical terminal records are distinguished from current work and from
      renewed correctness or platform certification.
- [x] Tracker consistency, local links and documentation checks pass; validation
      limits and the canonical verification result are recorded accurately.

## Provenance

Domowik repository specifications, source, tests and checked-in evidence only.
No third-party application implementation, external publication or workflow
execution accompanies this audit.

## Review outcome

All 94 preceding issues have an explicit disposition and named repository
basis in [the audit ledger](../AUDIT-2026-10-08.md). All remain relevant either
as historical completion or as an unfinished product/release/decision
obligation. No whole remaining issue was shown obsolete and no Done issue was
shown to need reopening by this relevance review.

Corrected the current 0032 room-record gate/criterion explanation, clarified
0021/0035 furnished-corpus prerequisites now that experimental §10 exists,
and added explicit residual checklists to 0010/0036/0058/0060/0078. Updated the
index, current backlog, roadmap and asset-strategy clarification. Original
acceptance criteria and dated evidence remain. With this audit closed:
95 issues, 62 Done, 24 partial scope, 4 Blocked and 5 Open.

## Verification

Final checks passed on the audit working tree:

- `cargo run --locked -q -p domowik-devtools --bin domowik-tracker-check`: exit 0.
- `cargo run --locked -q -p domowik-devtools --bin domowik-jsoncheck -- docs`:
  15 JSON fences checked, syntax only, exit 0.
- Structural/link audit: 95 unique issues; exactly 94 preceding audit rows;
  correct status totals; all dependency targets present; no upstream cycles;
  363 local Markdown targets across 104 tracker/documentation files resolve.
- `git diff --check`: exit 0.

No application or test source changed during this audit. The existing 0059 canonical run ended with
`=== VERIFICATION FAILED ===`, stage `P13 clean tree`, exit 1, after its
functional stages passed. It does not establish an exit-0 audit result.
Product tests and platform probes are not repeated for tracker-only changes;
tracker, documentation, graph/coverage, local links and whitespace are checked
on the final audit tree. No remote CI observation is refreshed here.

## Closure

- **Implementation:** owner-requested working-tree tracker/documentation
  changes; no commit requested.
- **Follow-up issues:** existing nonterminal issues retain their criteria and
  next actions in the audit ledger; no product obligation was silently closed.
