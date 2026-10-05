# 0009: Ratify names, module path, and glossary

- **Phase:** 0
- **Priority:** Medium
- **Status:** Done
- **Depends on:** None

## Blocker resolution (history preserved)

On 2026-08-23 this issue was Blocked under ED-0006: a public Android
home-organizer app ("Domowik: Home Organizer", application ID
`pl.domowik.app`, website `domowik.dev`) uses the same name and derived
identifiers, and unblocking required either a confirmed relationship with,
or explicit authorization from, the relevant rights holder(s), or an
executive-approved rename. The same day, executive input recorded verbatim
as ED-0008 (`../../EXECUTIVE-DECISIONS.md`) — explicitly superseding ED-0006 —
adopted `https://domowik.dev` as an explicitly temporary project URL,
expected to change, with all identity values provisional or rule-deferred
and centrally replaceable via `../../docs/NAMING.md`. No relationship, rights,
clearance, ownership, affiliation, non-affiliation, or non-infringement is
established for any party; the due-diligence record is
`../../docs/NAMING-DUE-DILIGENCE.md`, and ADR 0006 (Accepted) converts its
outstanding manual checks into a pre-publication release gate tracked in
issue 0034.

## Problem

Ratify or change the proposed `.domo` extension and `domowik` binary name;
select the fully qualified Go module path; audit `../../docs/GLOSSARY.md` for
consistent terms. Existing uses are proposals until this issue closes.

## Acceptance criteria

- [x] Extension, binary, application IDs, and importable Go module path are
      recorded in an ADR or accepted specification section.
      (ADR 0006 Accepted; app IDs deferred-by-rule per ADR 0006 and
      `../../docs/NAMING.md`; module path `domowik.dev/domowik` provisional.)
- [x] MIME registration strategy is either defined or explicitly deferred.
      (Explicitly deferred-by-rule in ADR 0006 and `../../docs/NAMING.md`.)
- [x] README, specs, ADRs, and issue titles use glossary terms consistently.
      (Audit re-run 2026-08-23 on the current integrated tree at main
      `1d1cda1`, covering the normative BEHAVIOR-SPEC sections: probes for
      op-log shorthand, frontier/snapshot, thin/portable package, actor,
      tombstone, and entity terminology found no inconsistencies with
      `../../docs/GLOSSARY.md`; no fixes required.)
- [x] The decision accounts for naming/trademark checks before public release.
      (ADR 0006 release gate; manual registry checks assigned to issue 0034.)

## Closure record

Completed under PRE-MERGE WORK APPROVED with explicit completion agreement.
Implementation commits: main repository `4851db9`
("docs(issue-0009): ratify provisional naming identity") and nested tracker
`9a56640` ("docs(issue-0009): record naming follow-up"). Follow-up issue 0034
remains Open. This file moved to `done/` and the index was updated in this
closure commit.
