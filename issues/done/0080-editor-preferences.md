# 0080: Add editor preferences and split orientation

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0077, 0078

## Acceptance criteria

- [x] Preferences opens from the editor and offers vertical (left/right) and
  horizontal (top/bottom) 2D/3D split layouts.
- [x] Vertical is the default, including for existing configurations without this
  setting. Saved choices survive restart without modifying a plan.
- [x] Save applies the layout; Cancel discards changes. Configuration write errors
  remain visible and do not apply an unsaved choice.
- [x] Switching orientation preserves the mounted view state; tests cover layout,
  persistence, cancellation and write failure.

## Provenance

Original product choice requested by the user on 2026-10-04. No third-party
application source, assets or dependencies consulted.

## Closure — 2026-10-04

Implemented in the working tree. The header's Preferences button opens a
Save/Cancel dialog. The version-1 user configuration stores `splitOrientation`
as `vertical` or `horizontal`; an absent key defaults to vertical. Invalid
values are reported and protected against overwrite, just like corrupt config.
Unknown keys and library directories survive preference changes. The workspace
retains its mounted plan and 3D view when changing orientation. Header actions
wrap to remain usable at narrow widths.

Native-backed widget tests verify pane positions, cancellation, clean document
state, retained camera/view identity and a 390-pixel window. Config tests verify
restart, old configuration defaults and unknown-key preservation; a blocked-file
test verifies save failure. Focused preference/config tests and editor-shell
tests pass. Flutter analysis and the Linux release build pass.

Full Flutter suite: 278 tests passed. Canonical verification outside the sandbox
completed all executed checks through tracker consistency, then printed
`=== VERIFICATION FAILED ===`, `STAGE: P13 clean tree`, exit 1 because the
workspace contains uncommitted changes. macOS bundle checks were explicitly
skipped on Linux. The first sandboxed run failed at the C harness because
LeakSanitizer cannot run under process tracing; the unrestricted run passed
that stage without changing its checks.
