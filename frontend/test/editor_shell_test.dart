import 'package:domowik_editor/editor_shell.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Widget shell(
    List<String> calls, {
    Widget? canvas,
    String status = 'Ready to draw',
    bool statusIsError = false,
    double textScale = 1.0,
  }) => MediaQuery(
    data: MediaQueryData(textScaler: TextScaler.linear(textScale)),
    child: MaterialApp(
      home: EditorShell(
        fileName: 'Our house.domo',
        onNew: () => calls.add('new'),
        onOpen: () => calls.add('open'),
        onSave: () => calls.add('save'),
        onSaveAs: () => calls.add('save as'),
        onUndo: () => calls.add('undo'),
        onRedo: () => calls.add('redo'),
        tools: Row(
          children: [
            for (final name in ['Walls', 'Door', 'Window'])
              TextButton(onPressed: () => calls.add(name), child: Text(name)),
          ],
        ),
        inspector: ListView(children: const [Text('Level details')]),
        canvas: canvas ?? const ColoredBox(color: Colors.white),
        status: status,
        statusIsError: statusIsError,
      ),
    ),
  );

  testWidgets('save shortcuts work from the canvas and a focused text field', (
    tester,
  ) async {
    final calls = <String>[];
    final focus = FocusNode();
    addTearDown(focus.dispose);
    await tester.pumpWidget(
      shell(
        calls,
        canvas: Focus(
          focusNode: focus,
          autofocus: true,
          child: const TextField(),
        ),
      ),
    );
    await tester.pumpAndSettle();
    for (final textFocused in [false, true]) {
      if (textFocused) {
        await tester.tap(find.byType(TextField));
        await tester.pumpAndSettle();
      }
      for (final modifier in [
        LogicalKeyboardKey.controlLeft,
        LogicalKeyboardKey.metaLeft,
      ]) {
        await tester.sendKeyDownEvent(modifier);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.keyS);
        await tester.sendKeyRepeatEvent(LogicalKeyboardKey.keyS);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.keyS);
        await tester.sendKeyUpEvent(modifier);
        await tester.pump();
      }
    }
    expect(calls, ['save', 'save', 'save', 'save']);
    focus.requestFocus();
    await tester.pump();
    await tester.sendKeyEvent(LogicalKeyboardKey.keyS);
    expect(calls, hasLength(4), reason: 'bare S does not save');
    expect(tester.takeException(), isNull);
  });

  testWidgets('desktop file and tool actions reach their own callbacks', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final calls = <String>[];
    await tester.pumpWidget(shell(calls));
    expect(find.text('Our house.domo'), findsOneWidget);
    expect(find.text('Level details'), findsOneWidget);
    expect(find.byTooltip('Levels and plan details'), findsNothing);
    for (final tooltip in ['New plan', 'Open plan', 'Undo']) {
      await tester.tap(find.byTooltip(tooltip));
    }
    await tester.tap(find.text('Save'));
    await tester.tap(find.byKey(const ValueKey('save-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save as…'));
    await tester.pumpAndSettle();
    for (final label in ['Walls', 'Door', 'Window']) {
      await tester.tap(find.text(label));
    }
    expect(calls, [
      'new',
      'open',
      'undo',
      'save',
      'save as',
      'Walls',
      'Door',
      'Window',
    ]);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'wide windows label the header actions; narrow ones keep icons only, '
    'and status messages read as sentences',
    (tester) async {
      Widget withExtras(List<String> calls) => MaterialApp(
        home: EditorShell(
          fileName: 'Our house.domo',
          onNew: () => calls.add('new'),
          onOpen: () => calls.add('open'),
          onSave: () => calls.add('save'),
          onSaveAs: () => calls.add('save as'),
          onAssets: () => calls.add('assets'),
          onViewModeChanged: calls.add,
          viewMode: 'split',
          onUndo: null,
          onRedo: null,
          tools: const SizedBox(),
          inspector: const SizedBox(),
          canvas: const SizedBox(),
          status: 'redo applied',
          statusIsError: false,
        ),
      );
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      final calls = <String>[];
      await tester.pumpWidget(withExtras(calls));
      for (final label in ['New', 'Open', 'Furniture', '2D + 3D']) {
        expect(find.text(label), findsOneWidget, reason: label);
      }
      // The save control is one split pill: the Save face in both modes, the
      // chevron menu in both modes, and the Save As item only when open.
      expect(find.text('Save'), findsOneWidget);
      expect(find.byKey(const ValueKey('save-menu')), findsOneWidget);
      expect(find.text('Save as…'), findsNothing);
      expect(find.text('Redo applied'), findsOneWidget);
      await tester.tap(find.text('Furniture'));
      expect(calls, ['assets']);

      await tester.binding.setSurfaceSize(const Size(600, 800));
      await tester.pumpWidget(withExtras(calls));
      for (final label in ['New', 'Open', 'Furniture', '2D + 3D']) {
        expect(find.text(label), findsNothing, reason: label);
      }
      expect(find.byTooltip('Furniture and model library'), findsOneWidget);
      expect(find.text('Save'), findsOneWidget);
      expect(find.byKey(const ValueKey('save-menu')), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('small windows keep the canvas and expose details in a drawer', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(shell([]));
    expect(find.text('Level details'), findsNothing);
    expect(tester.getSize(find.byType(ClipRect).last).width, 390);
    await tester.tap(find.byTooltip('Levels and plan details'));
    await tester.pumpAndSettle();
    expect(find.text('Level details'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('resizing across the drawer breakpoint preserves canvas state', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(shell([], canvas: const _CanvasCounter()));
    await tester.tap(find.text('Canvas taps: 0'));
    await tester.pump();
    await tester.binding.setSurfaceSize(const Size(600, 700));
    await tester.pumpAndSettle();
    expect(find.text('Canvas taps: 1'), findsOneWidget);
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    await tester.pumpAndSettle();
    expect(find.text('Canvas taps: 1'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  // Issue 0048: a status message must never resize the canvas above it,
  // because that recentres the camera under an unchanged pointer and makes
  // a repeated click miss its target.
  const shortStatus = 'Ready to draw';
  const longStatus =
      'Draw wall refused: rejected by the core — the resulting wall would '
      'overlap an existing opening on this host, which is not permitted';
  const errorStatus = 'delete wall refused: unknown handle';
  // Explicitly two lines, without depending on platform glyph widths.
  const twoLineStatus = 'Wall cannot move.\nOpening blocks it.';

  for (final size in [Size(1200, 800), Size(390, 700)]) {
    testWidgets(
      'status length and error state do not move the canvas (${size.width}w)',
      (tester) async {
        await tester.binding.setSurfaceSize(size);
        addTearDown(() => tester.binding.setSurfaceSize(null));

        await tester.pumpWidget(shell([], status: shortStatus));
        final baseline = tester.getSize(find.byType(ClipRect).last);

        await tester.pumpWidget(shell([], status: longStatus));
        expect(
          tester.getSize(find.byType(ClipRect).last),
          baseline,
          reason: 'a long, wrapping message must not resize the canvas',
        );

        await tester.pumpWidget(
          shell([], status: errorStatus, statusIsError: true),
        );
        expect(
          tester.getSize(find.byType(ClipRect).last),
          baseline,
          reason: 'switching to the error status must not resize the canvas',
        );

        await tester.pumpWidget(shell([], status: shortStatus));
        expect(tester.getSize(find.byType(ClipRect).last), baseline);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'a larger accessibility text scale keeps the canvas stable across messages',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(shell([], status: shortStatus, textScale: 2.0));
      final baseline = tester.getSize(find.byType(ClipRect).last);

      await tester.pumpWidget(shell([], status: longStatus, textScale: 2.0));
      expect(
        tester.getSize(find.byType(ClipRect).last),
        baseline,
        reason:
            'a taller reserved status bar must still hold steady across '
            'messages at the same text scale',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a larger text scale reserves a taller bar than the default scale',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(shell([], status: shortStatus));
      final defaultScaleCanvas = tester.getSize(find.byType(ClipRect).last);

      await tester.pumpWidget(shell([], status: shortStatus, textScale: 2.0));
      final largeScaleCanvas = tester.getSize(find.byType(ClipRect).last);

      expect(
        largeScaleCanvas.height,
        lessThan(defaultScaleCanvas.height),
        reason:
            'a larger text scale must reserve more height for the status '
            'bar, shrinking the canvas rather than clipping scaled text',
      );
    },
  );

  testWidgets(
    'a genuinely two-line message renders both lines without clipping',
    (tester) async {
      // Both explicit lines fit at the compact width.
      await tester.binding.setSurfaceSize(const Size(390, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(shell([], status: twoLineStatus));
      final paragraph = tester.renderObject<RenderParagraph>(
        find.descendant(
          of: find.text(twoLineStatus),
          matching: find.byType(RichText),
        ),
      );
      final boxes = paragraph.getBoxesForSelection(
        const TextSelection(baseOffset: 0, extentOffset: twoLineStatus.length),
      );
      expect(boxes.map((box) => box.top).toSet(), hasLength(2));
      for (final box in boxes) {
        expect(box.top, greaterThanOrEqualTo(0));
        expect(
          box.bottom,
          lessThanOrEqualTo(paragraph.size.height + 1e-6),
          reason: 'actual rendered line boxes must fit the allocated height',
        );
      }
      expect(
        paragraph.didExceedMaxLines,
        isFalse,
        reason:
            'the reserved status-bar height must actually fit two real '
            'lines of this message, not merely hold canvas bounds steady '
            'while clipping content',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'mixed-Unicode content uses the same forced line height as plain ASCII',
    (tester) async {
      // Emoji, Latin and Japanese in one message: without a forced strut,
      // glyphs outside the primary font can fall back to a font with
      // different metrics and change the line height this content alone
      // needs, independent of message length.
      const unicodeStatus = '⚠️ wall drag refused: 壁が開口部と重なっています';
      await tester.binding.setSurfaceSize(const Size(390, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));

      await tester.pumpWidget(shell([], status: shortStatus));
      final baseline = tester.getSize(find.byType(ClipRect).last);

      await tester.pumpWidget(shell([], status: unicodeStatus));
      expect(
        tester.getSize(find.byType(ClipRect).last),
        baseline,
        reason:
            'mixed-script content must reserve the same height as '
            'plain ASCII, not one derived from its own glyph metrics',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('the full status message stays reachable through its tooltip', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(shell([], status: longStatus));
    expect(find.byTooltip(longStatus), findsOneWidget);
    expect(
      find.text(longStatus),
      findsOneWidget,
      reason:
          'the Text widget carries the untruncated message even though '
          'maxLines/ellipsis clip it visually, so semantics see the full '
          'text',
    );
  });

  testWidgets('the footer copy action copies the full untruncated status', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    var copied = '';
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      if (call.method == 'Clipboard.setData') {
        copied = (call.arguments as Map<Object?, Object?>)['text'] as String;
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );
    await tester.pumpWidget(shell([], status: longStatus, statusIsError: true));

    await tester.tap(find.byTooltip('Copy status message'));
    await tester.pump();

    expect(copied, longStatus);
    expect(tester.takeException(), isNull);
  });

  testWidgets('tiny text scaling still leaves room for the status icon', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(390, 700));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(shell([], status: shortStatus, textScale: 0.1));
    final icon = find.byIcon(Icons.info_outline);
    expect(tester.getSize(icon).height, 16);
    expect(tester.takeException(), isNull);
    final baseline = tester.getRect(find.byType(ClipRect).last);
    await tester.pumpWidget(shell([], status: longStatus, textScale: 0.1));
    expect(tester.getRect(find.byType(ClipRect).last), baseline);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'two-row header places identity + settings/avatar on top and in-app actions + right-aligned save below',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final calls = <String>[];
      await tester.pumpWidget(
        MaterialApp(
          home: EditorShell(
            fileName: 'Main house',
            isOnline: true,
            onNew: () => calls.add('new-local'),
            onNewOnline: () => calls.add('new-online'),
            newOnlineTooltip: 'New property plan',
            onOpen: () => calls.add('open-local'),
            onOpenOnline: () => calls.add('open-online'),
            onSave: () => calls.add('save'),
            onSaveAs: () => calls.add('save-as'),
            onAssets: () => calls.add('furniture'),
            onPreferences: () => calls.add('preferences'),
            onViewModeChanged: calls.add,
            account: TextButton(
              key: const ValueKey('account'),
              onPressed: () => calls.add('account'),
              child: const Text('person@example.test'),
            ),
            propertyPlans: TextButton(
              key: const ValueKey('property-plans-button'),
              onPressed: () => calls.add('property-plans'),
              child: const Text('Property plans'),
            ),
            onUndo: null,
            onRedo: null,
            tools: const SizedBox(),
            inspector: const SizedBox(),
            canvas: const SizedBox(),
            status: 'Ready',
            statusIsError: false,
          ),
        ),
      );

      final topRow = tester.getRect(
        find.byKey(const ValueKey('header-top-row')),
      );
      final actionsRow = tester.getRect(
        find.byKey(const ValueKey('header-actions-row')),
      );
      expect(topRow.bottom, lessThanOrEqualTo(actionsRow.top + 1.0));

      // Top row left: Domowik icon, name, Online badge, plan title.
      final brandIcon = tester.getRect(find.byIcon(Icons.cottage_outlined));
      final badge = tester.getRect(
        find.byKey(const ValueKey('plan-storage-badge')),
      );
      final title = tester.getRect(find.byKey(const ValueKey('plan-title')));
      expect(find.text('Online'), findsOneWidget);
      expect(find.text('Main house'), findsOneWidget);
      expect(brandIcon.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(badge.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(title.center.dy, inInclusiveRange(topRow.top, topRow.bottom));

      // Top row right: Settings, then Account/Avatar in the top-right corner.
      final prefs = tester.getRect(find.byKey(const ValueKey('preferences')));
      final account = tester.getRect(find.byKey(const ValueKey('account')));
      expect(prefs.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(account.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(account.left, greaterThanOrEqualTo(prefs.right));
      expect(account.right, greaterThan(1000));

      // Second row left: New, Open, Furniture, View, Property plans under the icon.
      final newBtn = tester.getRect(find.byKey(const ValueKey('new-action')));
      final openBtn = tester.getRect(find.byKey(const ValueKey('open-action')));
      final furnitureBtn = tester.getRect(
        find.byKey(const ValueKey('asset-library')),
      );
      final saveBtn = tester.getRect(find.byKey(const ValueKey('save-action')));
      expect(
        newBtn.center.dy,
        inInclusiveRange(actionsRow.top, actionsRow.bottom),
      );
      expect(newBtn.left, lessThan(200));
      expect(openBtn.left, greaterThan(newBtn.right));
      expect(furnitureBtn.left, greaterThan(openBtn.right));
      // Save is right-aligned on the second row.
      expect(
        saveBtn.center.dy,
        inInclusiveRange(actionsRow.top, actionsRow.bottom),
      );
      expect(saveBtn.right, greaterThan(1050));

      // Signed-in split New & Open default to online, with local options in dropdown.
      await tester.tap(find.byKey(const ValueKey('new-action')));
      await tester.tap(find.byKey(const ValueKey('open-action')));
      await tester.tap(find.byKey(const ValueKey('new-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('New local plan'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('open-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open local file…'));
      await tester.pumpAndSettle();
      expect(calls, ['new-online', 'open-online', 'new-local', 'open-local']);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'offline plan shows Offline badge and narrow header fits cleanly',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(390, 700));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: EditorShell(
            fileName: 'Untitled plan',
            isOnline: false,
            onNew: () {},
            onNewOnline: () {},
            onOpen: () {},
            onOpenOnline: () {},
            onSave: () {},
            onSaveAs: () {},
            onAssets: () {},
            onPreferences: () {},
            account: TextButton.icon(
              key: const ValueKey('account'),
              onPressed: () {},
              icon: const Icon(Icons.account_circle_outlined),
              label: const Text('Sign in'),
            ),
            onViewModeChanged: (_) {},
            onUndo: null,
            onRedo: null,
            tools: const SizedBox(),
            inspector: const SizedBox(),
            canvas: const SizedBox(),
            status: 'Ready',
            statusIsError: false,
          ),
        ),
      );
      expect(find.text('Offline'), findsOneWidget);
      expect(find.text('Untitled plan'), findsOneWidget);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('clicking the plan title edits it inline and Escape cancels', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    final renamed = <String>[];
    await tester.pumpWidget(
      MaterialApp(
        home: EditorShell(
          fileName: 'Untitled plan • Unsaved',
          editableTitle: 'Untitled plan',
          onRenamePlan: renamed.add,
          onNew: () {},
          onOpen: () {},
          onSave: () {},
          onSaveAs: () {},
          onUndo: null,
          onRedo: null,
          tools: const SizedBox(),
          inspector: const SizedBox(),
          canvas: const SizedBox(),
          status: 'Ready',
          statusIsError: false,
        ),
      ),
    );

    // Click to edit, type a new name, press Escape -> cancelled.
    await tester.tap(find.byKey(const ValueKey('plan-title')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('plan-title-input')), findsOneWidget);
    await tester.enterText(
      find.byKey(const ValueKey('plan-title-input')),
      'Discarded name',
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('plan-title-input')), findsNothing);
    expect(renamed, isEmpty);

    // Click to edit, enter a new name, submit -> commits.
    await tester.tap(find.byKey(const ValueKey('plan-title')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('plan-title-input')),
      '  Main floor  ',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('plan-title-input')), findsNothing);
    expect(renamed, ['Main floor']);
    expect(tester.takeException(), isNull);
  });
}

class _CanvasCounter extends StatefulWidget {
  const _CanvasCounter();

  @override
  State<_CanvasCounter> createState() => _CanvasCounterState();
}

class _CanvasCounterState extends State<_CanvasCounter> {
  int taps = 0;

  @override
  Widget build(BuildContext context) => Center(
    child: TextButton(
      onPressed: () => setState(() => taps++),
      child: Text('Canvas taps: $taps'),
    ),
  );
}
