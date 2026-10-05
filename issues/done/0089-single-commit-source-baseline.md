# 0089: Collapse committed history and prune branches

- **Phase:** 0
- **Priority:** High
- **Status:** Done
- **Depends on:** 0086

## Scope and owner decisions

Owner request, 2026-10-04: squash existing committed history to one commit and
prune branches. Follow-up answers explicitly exclude all current uncommitted
work and authorize rewriting GitHub main and removing other GitHub branches.
Repository visibility, releases and current source changes are not published.

## Acceptance criteria

- [x] Save a recoverable backup before changing references.
- [x] Main has one parentless commit with the exact previous committed tree.
- [x] Existing dirty files/index are preserved, including the second worktree.
- [x] Local non-main branches and stale remote-tracking branches are pruned.
- [x] GitHub main is rewritten using an exact old-tip lease; remaining remote
      branch/tag references are checked without changing visibility.

## Execution record — 2026-10-04

Complete committed-history bundle verified at
`/tmp/domowik-history-backup.rsWIzz/history.bundle`; tracked dirty patches and
a non-ignored primary-workspace archive are in the same directory. These are
local recovery artifacts, not durable off-machine backups or publication files.

Old main: `669d843e6c8d35625e8c14ab9cb70c8a0ce5de07`.
New main: `ccaa5bb23e586f732e4aec78e61189171f1daa8a`.
Both tree IDs: `82b5025ba462dd0ed8840cc24d6f9b2b63afb021`.
The new root was constructed with `git commit-tree`; compare-and-swap
`git update-ref` changed main without resetting/staging/checking out files.
The tracked primary dirty patch compared identical before/after the rewrite.

The `domowik-poi-highlighting` worktree was detached at its existing commit
`9cd8011d3193622d8654413973fd778bd3b5df5e`; its dirty binary patch compared
identical before/after. No files or worktree directories were deleted.
Local `claude/add-or-filter-groups-4lahY` and `feat/poi-highlighting` branches
were deleted after the bundle backup. Three stale origin branches were pruned.

Live `git ls-remote --heads --tags origin` showed only main even before the
rewrite, so there were no actual GitHub branches or tags to delete. Push used
`--force-with-lease=refs/heads/main:<exact old tip>` and succeeded; subsequent
remote inspection returned only the new main. Old objects/reflogs and the
detached worktree remain recoverable; no garbage collection or server-side
erasure claim is made. Existing Actions logs are not erased by this operation.

Current editor/docs/CI/license work deliberately remains uncommitted and was
not included in the GitHub push. The ignored user models were not included.

## Provenance

Original owner instructions and explicit follow-up choices. Git-native
reference/tree operations only; no external code, content or dependencies.
