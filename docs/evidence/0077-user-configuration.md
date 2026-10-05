# User configuration — issue 0077

Asset directories added through **Assets** are now user-level preferences and
appear after restart and when creating/opening another plan. Existing plan-local
locations also appear in the browser. Placed models remain embedded in the
plan; their portability does not depend on the user config or source directory.

Configuration paths:

- Linux: `$XDG_CONFIG_HOME/domowik/config.json`, or
  `~/.config/domowik/config.json` when XDG_CONFIG_HOME is absent/not absolute.
- macOS: `~/Library/Application Support/domowik/config.json`.
- Android: the app-private persistent files directory, supplied by the launcher
  as XDG_CONFIG_HOME; provider-URI asset access remains separate.
- Windows path convention: `%APPDATA%/domowik/config.json` (the editor is not
  currently packaged for Windows).

Versioned JSON stores `version: 1` and `assetDirectories`. Unknown keys are
preserved. Reads and writes are bounded to 1 MiB/64 paths. Corrupt/future-version
files are reported and protected from silent overwrite. Saves flush a temporary
file alongside the config and replace it by rename; this does not claim
power-loss durability or a transaction spanning preferences and plan metadata.

Tests use in-memory config or injected temporary config files and never mutate
the real user's preferences. Regression coverage checks persistence after
restart, deduplication, unknown keys, rejected corrupt/future/oversized files,
write failure and the real editor's reuse of libraries in a new clean session.

Complete Flutter suite: **271 passed**; Linux release bundle built. Canonical
verification passed all executed checks preceding its final clean-tree
assertion, then printed **`=== VERIFICATION FAILED ===`**,
**`STAGE: P13 clean tree`**, exit **1**, because the worktree is uncommitted.
