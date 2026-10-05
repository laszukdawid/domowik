# 0040: Protect unsaved edits when replacing an editor session

- **Phase:** 2
- **Priority:** High
- **Status:** Done
- **Depends on:** 0024

## Problem

The prototype's New and Open actions replace the current document without
checking for unsaved changes. Closing the window also discards the session.
A person can lose a plan despite the core's safe file-writing behavior.
This existing limitation surfaced while improving the workspace in issue 0039.

## Scope

Specify and implement the GUI's dirty-document lifecycle and save/discard/cancel
flow. Consume issue 0024's existing macOS file-dialog and persistence handoff;
do not change document geometry or the file format.

## Acceptance criteria

- [x] Record the original product choice and review it in BEHAVIOR-SPEC before
      implementing the session-replacement behavior.
- [x] New, Open and window close preserve unsaved work unless saving succeeds
      or the user explicitly chooses to discard it.
- [x] Cancelling the confirmation or file dialog keeps the current session.
- [x] Failed saves keep the document dirty and prevent the pending replacement.
- [x] View-only changes do not dirty the document; accepted edits and undo are
      covered by the dirty-state contract.
- [x] Tests exercise save/discard/cancel, failed saves and cancelled dialogs
      through the actual session lifecycle.

## Closure

- **Implementation/spec commits:** `fix(editor): protect unsaved sessions`
  (the commit containing this closure).
- **Follow-up issues:** None.

Delivered 2026-09-05. Eight lifecycle widget tests load the real Rust library,
exercise the actual editor handlers, and check released plan/buffer ownership.
Only native file-picker responses are substituted. Close requests are delivered
through the actual Dart method-channel handler; these tests do not automate
AppKit's menu or title-bar controls. The Swift close/quit integration compiles
in the macOS release app. Both packaged-app probes pass: 33 persistence/ABI
checks and the first-frame raster probe. No new dependencies or format changes.

## Implementation notes

The app now tracks dirty state from accepted batch IDs and successful saves,
blocks editing during lifecycle dialogs, and adopts a replacement native
handle only after initialising it successfully. The blank plan's default
level establishes the clean initial state. No-op undo now interprets the
bridge's JSON response instead of treating the nonempty response string as
an executed undo.

Native close and quit use the same Dart guard. Apple documents that quitting
does not call [windowShouldClose](https://developer.apple.com/documentation/appkit/nswindowdelegate/windowshouldclose(_:)),
so the app delegate separately uses
[terminateLater](https://developer.apple.com/documentation/appkit/nsapplication/terminatereply/terminatelater)
and [reply(toApplicationShouldTerminate:)](https://developer.apple.com/documentation/appkit/nsapplication/reply(toapplicationshouldterminate:)).
After approval, [NSWindow.close](https://developer.apple.com/documentation/appkit/nswindow/close())
closes without recursively asking the delegate again. References accessed
2026-09-05; implementation is original and no Apple source was copied.
