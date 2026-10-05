# 0077: Remember asset libraries in user-level configuration

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0075

## Acceptance criteria

- [x] Asset library additions/replacements persist in user-level config and are
      available after restart and in new or different plans.
- [x] Merely reading global libraries does not dirty a plan; placed content is
      embedded and remains portable with the plan.
- [x] OS-specific config paths, bounded/versioned JSON, unknown-key preservation
      and atomic replacement are documented and tested; failures are surfaced.
- [x] Tests isolate configuration from the real user's preferences.

## Scope

Initial global configuration owns reusable asset directory locations. It is
local user/device state, independent of plan content, with room for later
preferences. No remote retrieval or bundled catalog.

## Closure — 2026-10-03

Implemented in working tree, no commit requested. Adding/replacing libraries
persists version-1 user config; the browser merges it with legacy plan-local
paths. Selected global model files are validated and embedded on placement.
Preferences alone leave the plan clean. Config has a bounded read, unknown-key
preservation and flushed same-directory replacement. Tests use temporary files
and in-memory stores, including an actual editor restart into a new empty plan.

See [config paths/evidence](../../docs/evidence/0077-user-configuration.md).
Complete Flutter suite: 271 passed; native checks and Linux release build passed.
Canonical verification printed `=== VERIFICATION FAILED ===`,
`STAGE: P13 clean tree`, exit 1 after all prior executed stages passed, because
the worktree contains uncommitted work. Android's app-private config location
is supplied by the launcher, but Android execution was not performed here.
