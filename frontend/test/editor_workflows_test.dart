import 'package:domowik_editor/editor_workflows.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  for (final platform in TargetPlatform.values) {
    testWidgets('$platform shows only its runtime shortcut vocabulary', (
      tester,
    ) async {
      // Deliberately disagree with the runtime platform: a visual theme must
      // not choose which keyboard the user is told to use.
      await tester.pumpWidget(
        MaterialApp(
          theme: ThemeData(
            platform: platform == TargetPlatform.macOS
                ? TargetPlatform.windows
                : TargetPlatform.macOS,
          ),
          home: const Scaffold(body: EditorWorkflowHints()),
        ),
      );
      final modifier = platform == TargetPlatform.macOS
          ? 'Command (⌘)'
          : 'Ctrl';
      expect(EditorWorkflows.current().primaryModifier, modifier);
      expect(
        find.textContaining('Hold $modifier while scrolling'),
        findsOneWidget,
      );
      expect(find.textContaining('Hold Shift or $modifier'), findsNWidgets(2));
      expect(
        find.text('Undo: $modifier+Z. Redo: $modifier+Shift+Z.'),
        findsOneWidget,
      );
      expect(
        find.textContaining(platform == TargetPlatform.macOS ? 'Ctrl' : '⌘'),
        findsNothing,
      );
      expect(tester.takeException(), isNull);
    }, variant: TargetPlatformVariant({platform}));
  }

  test('history and wheel share command aliases without claiming Ctrl+Y', () {
    for (final control in [false, true]) {
      for (final meta in [false, true]) {
        for (final shift in [false, true]) {
          final command = control || meta;
          expect(
            EditorWorkflows.commandModifier(control: control, meta: meta),
            command,
          );
          expect(
            EditorWorkflows.historyShortcut(
              LogicalKeyboardKey.keyZ,
              control: control,
              meta: meta,
              shift: shift,
            ),
            !command
                ? null
                : shift
                ? HistoryShortcut.redo
                : HistoryShortcut.undo,
          );
          expect(
            EditorWorkflows.historyShortcut(
              LogicalKeyboardKey.keyY,
              control: control,
              meta: meta,
              shift: shift,
            ),
            isNull,
          );
        }
      }
    }
  });

  testWidgets('held keys resolve the same history command in either view', (
    tester,
  ) async {
    for (final modifier in [
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.metaLeft,
    ]) {
      await tester.sendKeyDownEvent(modifier);
      expect(
        EditorWorkflows.heldHistoryShortcut(LogicalKeyboardKey.keyZ),
        HistoryShortcut.undo,
      );
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      expect(
        EditorWorkflows.heldHistoryShortcut(LogicalKeyboardKey.keyZ),
        HistoryShortcut.redo,
      );
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyUpEvent(modifier);
    }
    expect(
      EditorWorkflows.heldHistoryShortcut(LogicalKeyboardKey.keyZ),
      isNull,
    );
  });
}
