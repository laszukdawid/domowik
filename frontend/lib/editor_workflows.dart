import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

enum HistoryShortcut { undo, redo }

enum ClipboardShortcut { copy, cut, paste }

/// Shared keyboard vocabulary for editor workflows, independent of theme.
///
/// Platform affects guidance only: both existing Control/Meta aliases remain
/// accepted. Pure input resolution has no document, focus or event ownership.
class EditorWorkflows {
  const EditorWorkflows(this.platform);

  factory EditorWorkflows.current() => EditorWorkflows(defaultTargetPlatform);

  final TargetPlatform platform;

  String get primaryModifier =>
      platform == TargetPlatform.macOS ? 'Command (⌘)' : 'Ctrl';

  String get navigationHint =>
      'Scroll to pan. Hold $primaryModifier while scrolling to zoom. '
      'Middle-drag or hold Space and drag to pan; Escape cancels. '
      'Drag a wall to move it, or an endpoint to move its corner.';

  String get selectionHint =>
      'Click again to cycle overlapping objects. Hold Shift or '
      '$primaryModifier and click to toggle selection. '
      'Select multiple walls to edit their height and thickness together; '
      'other property edits and deletion act on the primary object. '
      'Press R to rotate selected furniture; click elsewhere in the plan to '
      'accept, then Undo to revert.';

  String get marqueeHint =>
      'Hold Shift or $primaryModifier and drag from empty space to add '
      'everything in the rectangle to the selection. Escape cancels.';

  String get historyHint =>
      'Undo: $primaryModifier+Z. Redo: $primaryModifier+Shift+Z.';

  String get clipboardHint =>
      'Copy: $primaryModifier+C. Cut: $primaryModifier+X. '
      'Paste: $primaryModifier+V, placing the copied models at the pointer '
      'on the active level.';

  static bool commandModifier({required bool control, required bool meta}) =>
      control || meta;

  static ClipboardShortcut? clipboardShortcut(
    LogicalKeyboardKey key, {
    required bool control,
    required bool meta,
    required bool shift,
  }) {
    if (shift || !commandModifier(control: control, meta: meta)) {
      return null;
    }
    if (key == LogicalKeyboardKey.keyC) return ClipboardShortcut.copy;
    if (key == LogicalKeyboardKey.keyX) return ClipboardShortcut.cut;
    if (key == LogicalKeyboardKey.keyV) return ClipboardShortcut.paste;
    return null;
  }

  static ClipboardShortcut? heldClipboardShortcut(LogicalKeyboardKey key) {
    final keyboard = HardwareKeyboard.instance;
    return clipboardShortcut(
      key,
      control: keyboard.isControlPressed,
      meta: keyboard.isMetaPressed,
      shift: keyboard.isShiftPressed,
    );
  }

  static HistoryShortcut? historyShortcut(
    LogicalKeyboardKey key, {
    required bool control,
    required bool meta,
    required bool shift,
  }) {
    if (key != LogicalKeyboardKey.keyZ ||
        !commandModifier(control: control, meta: meta)) {
      return null;
    }
    return shift ? HistoryShortcut.redo : HistoryShortcut.undo;
  }

  static HistoryShortcut? heldHistoryShortcut(LogicalKeyboardKey key) {
    final keyboard = HardwareKeyboard.instance;
    return historyShortcut(
      key,
      control: keyboard.isControlPressed,
      meta: keyboard.isMetaPressed,
      shift: keyboard.isShiftPressed,
    );
  }
}

/// Shared inspector guidance; platform is resolved at build time so tests and
/// runtime platform changes do not leave stale labels. Owns no editing state.
class EditorWorkflowHints extends StatelessWidget {
  const EditorWorkflowHints({super.key});

  @override
  Widget build(BuildContext context) {
    final workflows = EditorWorkflows.current();
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text('Move around', style: theme.textTheme.titleSmall),
        for (final hint in [
          workflows.navigationHint,
          workflows.selectionHint,
          workflows.marqueeHint,
          workflows.historyHint,
          workflows.clipboardHint,
        ]) ...[
          const SizedBox(height: 8),
          Text(hint, style: theme.textTheme.bodySmall),
        ],
      ],
    );
  }
}
