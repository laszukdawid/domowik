import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

enum HistoryShortcut { undo, redo }

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
      'Edits act only on the primary object.';

  String get marqueeHint =>
      'Hold Shift or $primaryModifier and drag from empty space to add '
      'everything in the rectangle to the selection. Escape cancels.';

  String get historyHint =>
      'Undo: $primaryModifier+Z. Redo: $primaryModifier+Shift+Z.';

  static bool commandModifier({required bool control, required bool meta}) =>
      control || meta;

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
        ]) ...[
          const SizedBox(height: 8),
          Text(hint, style: theme.textTheme.bodySmall),
        ],
      ],
    );
  }
}
