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
