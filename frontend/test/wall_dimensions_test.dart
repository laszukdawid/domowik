import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/main.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Wall measurements from the editor (BEHAVIOR-SPEC R7.1.6, issue 0071).
///
/// Against the REAL library, because the point is the boundary: the view
/// chooses "this wall is longer, thicker and taller now", and the core
/// resolves the length about the stored Start, validates, and commits the
/// whole form as one undoable batch.
void main() {
  late Bridge bridge;

  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        Platform.environment['DOMOWIK_TEST_LIBRARY'] ??
            File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
  });

  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(home: PlanPage(bridgeLoader: () => FfiCore(bridge))),
    );
    await tester.pumpAndSettle();
    expect(bridge.openPlans(), 1);
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(bridge.openPlans(), 0);
      expect(bridge.outstandingBuffers(), 0);
    });
  }

  Finder canvas() => find.byWidgetPredicate(
    (w) => w is CustomPaint && w.painter is PlanPainter,
  );

  PlanPainter painter(WidgetTester tester) =>
      tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.tap(finder);
    await tester.pumpAndSettle();
  }

  Future<void> clickAt(WidgetTester tester, Offset position) async {
    final gesture = await tester.startGesture(
      position,
      kind: PointerDeviceKind.mouse,
    );
    await gesture.up();
    await tester.pumpAndSettle();
  }

  /// Draws one horizontal wall, then selects it by its body so the wall
  /// inspector — not the level list — is showing.
  Future<Wall> drawAndSelect(WidgetTester tester) async {
    await tap(tester, find.byKey(const ValueKey('wall-chain-tool')));
    await tester.dragFrom(
      tester.getTopLeft(canvas()) + const Offset(150, 300),
      const Offset(250, 0),
    );
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
    final wall = painter(tester).walls.single;
    await tap(tester, find.widgetWithText(ChoiceChip, 'Select'));
    await clickAt(
      tester,
      tester.getTopLeft(canvas()) +
          painter(tester).toScreen((wall.start + wall.end) / 2),
    );
    expect(painter(tester).selected, wall.id);
    return wall;
  }

  Future<void> openMeasurements(WidgetTester tester) async {
    final edit = find.byKey(const ValueKey('edit-wall-dimensions'));
    await tester.scrollUntilVisible(
      edit,
      300,
      scrollable: find.descendant(
        of: find.byType(ListView),
        matching: find.byType(Scrollable),
      ),
    );
    await tester.ensureVisible(edit);
    await tap(tester, edit);
  }

  testWidgets('one form submits length, thickness and height as one batch', (
    tester,
  ) async {
    await start(tester);
    final wall = await drawAndSelect(tester);
    await openMeasurements(tester);

    final longer = (wall.lengthMm + 1000).round();
    await tester.enterText(
      find.byKey(const ValueKey('wall-lengthMm-field')),
      '$longer',
    );
    await tester.enterText(
      find.byKey(const ValueKey('wall-thicknessMm-field')),
      '350',
    );
    await tester.enterText(
      find.byKey(const ValueKey('wall-heightMm-field')),
      '2700',
    );
    await tap(tester, find.byKey(const ValueKey('wall-properties-apply')));

    final changed = painter(tester).walls.single;
    expect(changed.start, wall.start, reason: 'length anchors at Start');
    expect(changed.lengthMm, closeTo(longer, 1));
    expect(
      (changed.end - changed.start).direction,
      closeTo((wall.end - wall.start).direction, 1e-9),
      reason: 'End travels along the existing direction',
    );
    expect(changed.thicknessMm, 350);
    expect(changed.heightMm, 2700);

    // ONE form, ONE undo entry: everything it changed comes back together.
    await tap(tester, find.byTooltip('Undo'));
    final restored = painter(tester).walls.single;
    expect(restored.start, wall.start);
    expect(restored.lengthMm, closeTo(wall.lengthMm, 1e-6));
    expect(restored.thicknessMm, wall.thicknessMm);
    expect(restored.heightMm, wall.heightMm);
  });

  testWidgets('a nonpositive measurement cannot be submitted or land', (
    tester,
  ) async {
    await start(tester);
    final wall = await drawAndSelect(tester);
    await openMeasurements(tester);
    await tester.enterText(
      find.byKey(const ValueKey('wall-heightMm-field')),
      '0',
    );
    await tester.pumpAndSettle();
    final apply = tester.widget<FilledButton>(
      find.byKey(const ValueKey('wall-properties-apply')),
    );
    expect(apply.onPressed, isNull);
    await tap(tester, find.byKey(const ValueKey('wall-properties-cancel')));
    expect(painter(tester).walls.single.heightMm, wall.heightMm);
  });

  testWidgets(
    'wall color Apply updates derived paint and one undo restores it',
    (tester) async {
      await start(tester);
      final original = await drawAndSelect(tester);
      final edit = find.byKey(const ValueKey('edit-wall-color'));
      await tester.scrollUntilVisible(
        edit,
        300,
        scrollable: find.descendant(
          of: find.byType(ListView),
          matching: find.byType(Scrollable),
        ),
      );
      await tap(tester, edit);
      for (final side in ['left', 'right']) {
        await tester.enterText(
          find.byKey(ValueKey('solid-color-$side')),
          '#d8cdb7',
        );
      }
      await tap(tester, find.byKey(const ValueKey('apply-solid-colors')));
      final changed = painter(tester).walls.single;
      expect(changed.color, '#D8CDB7');
      expect(changed.start, original.start);
      expect(changed.end, original.end);
      expect(painter(tester).derived.wallPaints.single.color, '#D8CDB7');
      await tap(tester, find.byTooltip('Undo'));
      expect(painter(tester).walls.single.color, isNull);
      expect(painter(tester).derived.wallPaints, isEmpty);
      await tap(tester, find.byTooltip('Redo'));
      expect(painter(tester).walls.single.color, '#D8CDB7');
    },
  );

  testWidgets('each wall face takes its own color and paints its half', (
    tester,
  ) async {
    await start(tester);
    final wall = await drawAndSelect(tester);
    final edit = find.byKey(const ValueKey('edit-wall-color'));
    await tester.scrollUntilVisible(
      edit,
      300,
      scrollable: find.descendant(
        of: find.byType(ListView),
        matching: find.byType(Scrollable),
      ),
    );
    await tap(tester, edit);
    expect(find.text('Left side'), findsOneWidget);
    await tester.enterText(
      find.byKey(const ValueKey('solid-color-left')),
      '#b77964',
    );
    await tap(tester, find.byKey(const ValueKey('apply-solid-colors')));
    final derived = painter(tester).derived;
    expect(derived.wallSides[wall.id]!.colors, ['#B77964', null]);
    expect(derived.wallPaints.map((p) => p.color).toSet(), {'#B77964', null});
    await tap(tester, find.byTooltip('Undo'));
    expect(painter(tester).derived.wallPaints, isEmpty);
  });
}
