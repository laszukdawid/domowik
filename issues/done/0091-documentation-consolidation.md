# 0091: Consolidate documentation and retire archive references

- **Phase:** 0
- **Priority:** Medium
- **Status:** Done
- **Depends on:** 0062, 0086

## Scope

The owner removed `_reference/` and requested updated documentation and a
single commit containing all pending documentation. Application/build changes
and source deletions are excluded. Dated evidence and ADR decisions remain
historical records; current guidance must not depend on the deleted archive.

## Acceptance criteria

- [x] Current guides and contributor policy no longer point to a live archive.
- [x] Historical records explain where the retired implementation can be found.
- [x] Local documentation and tracker checks pass.
- [x] All pending documentation is included in one local commit, without
      application/build changes or a push.

## Closure

Updated current onboarding/contributor guidance, roadmap, release history and
ADR status notes for the owner's archive removal on 2026-10-05. Earlier
decisions and dated evidence remain historical, with Git history named as the
source of retired implementation material. All previously pending docs,
evidence, tracker records and license/inventory documentation are consolidated
in the same docs-only commit; application/build changes and `_reference/`
source deletions are deliberately left unstaged.

Local checks: documentation JSON fences (15), corpus JSON fences (17), tracker
consistency and whitespace checks. No full application verification is claimed
for this documentation-only increment; the preceding Go cleanup's full Linux
verification is recorded separately in issue 0062.
