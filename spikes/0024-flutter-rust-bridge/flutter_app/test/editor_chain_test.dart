import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_spike_app/main.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late Bridge bridge;

  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('../target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
  });

  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(home: PlanPage(bridgeLoader: () => bridge)),
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

  Future<void> click(WidgetTester tester, Offset point) async {
    await tester.tapAt(tester.getTopLeft(canvas()) + point);
    await tester.pumpAndSettle();
  }

  PlanPainter painter(WidgetTester tester) =>
      tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;

  for (final chaining in [false, true]) {
    testWidgets('inverse-overflow input is discarded whole (chain=$chaining)', (
      tester,
    ) async {
      await start(tester);
      if (chaining) {
        await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
        await tester.pumpAndSettle();
      }
      final input = tester.widget<Listener>(
        find.byKey(const ValueKey('plan-input')),
      );
      // Finite logical pixels can overflow when divided by the view scale.
      input.onPointerDown!(
        const PointerDownEvent(
          pointer: 94,
          position: Offset(1e308, 200),
          buttons: kPrimaryButton,
        ),
      );
      final top = tester.getTopLeft(canvas());
      final drag = await tester.startGesture(
        top + const Offset(183, 223),
        pointer: 93,
      );
      await drag.moveTo(top + const Offset(420, 223));
      await tester.pump();
      final before = painter(tester).draft;
      expect(before, isNotNull);
      input.onPointerMove!(
        const PointerMoveEvent(
          pointer: 93,
          position: Offset(1e308, 200),
          delta: Offset(1, 0),
        ),
      );
      input.onPointerUp!(
        const PointerUpEvent(pointer: 93, position: Offset(1e308, 200)),
      );
      await tester.pump();
      expect(painter(tester).draft, before);
      expect(painter(tester).walls, isEmpty);
      expect(tester.takeException(), isNull);
      await drag.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls, hasLength(1));
    });
  }

  testWidgets(
    'first drag supplies the accepted relative angle to hover and click',
    (tester) async {
      await start(tester);
      for (final key in ['snap-grid', 'snap-object', 'snap-angle']) {
        await tester.tap(find.byKey(ValueKey(key)));
      }
      await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
      await tester.pumpAndSettle();
      final top = tester.getTopLeft(canvas());
      final drag = await tester.startGesture(top + const Offset(180, 500));
      await drag.moveTo(top + const Offset(380, 400));
      await drag.up();
      await tester.pumpAndSettle();
      final first = painter(tester).walls.single;
      tester
          .widget<IconButton>(find.byKey(const ValueKey('snap-angle')))
          .onPressed!();
      final hover = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await hover.addPointer(location: top + const Offset(380, 400));
      // The previous direction is (2000,1000) mm. Its +45-degree ray is
      // (1000,3000); (30,-10) is perpendicular, so this cursor projects
      // exactly to local (480,100), far from any global 45-degree ray.
      const cursor = Offset(483, 101), target = Offset(480, 100);
      await hover.moveTo(top + cursor);
      await tester.pumpAndSettle();
      expect(painter(tester).draft!.$1, first.end);
      expect(painter(tester).draft!.$2, painter(tester).view.toPlan(target));
      expect(painter(tester).snapPoint, painter(tester).draft!.$2);
      expect(painter(tester).snapGuide, isNotNull);
      await click(tester, cursor);
      expect(painter(tester).walls, hasLength(2));
      expect(painter(tester).walls.last.start, first.end);
      expect(
        painter(tester).walls.last.end,
        painter(tester).view.toPlan(target),
      );
      await hover.removePointer();
    },
  );

  testWidgets('panning with Walls armed never creates a segment', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final before = painter(tester).view;
    final pan = await tester.startGesture(
      tester.getCenter(canvas()),
      kind: PointerDeviceKind.mouse,
      buttons: kMiddleMouseButton,
    );
    await pan.moveBy(const Offset(90, 40));
    await pan.up();
    await tester.pumpAndSettle();
    expect(painter(tester).view.origin, before.origin + const Offset(90, 40));
    expect(painter(tester).walls, isEmpty);
    expect(find.textContaining('• Unsaved'), findsNothing);
  });

  testWidgets('a refused native snap never commits a raw chain drag', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final top = tester.getTopLeft(canvas());
    final drag = await tester.startGesture(top + const Offset(183, 223));
    await drag.moveTo(top + const Offset(1e24, 223));
    await drag.up();
    await tester.pumpAndSettle();
    expect(painter(tester).walls, isEmpty);
    await click(tester, const Offset(550, 350));
    expect(
      painter(tester).walls,
      isEmpty,
      reason: 'undo must terminate the stale chain before another click',
    );
    expect(painter(tester).draft, isNull);
    expect(find.textContaining('snap failed:'), findsOneWidget);
    expect(find.textContaining('• Unsaved'), findsNothing);
  });

  testWidgets('wall chain commits continuous segments one batch each', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    await click(tester, const Offset(180, 220));
    await click(tester, const Offset(420, 220));
    await click(tester, const Offset(420, 420));

    final walls = painter(tester).walls;
    expect(walls, hasLength(2));
    expect(walls[1].start, walls[0].end);
    // Two accepted segments produce two undo entries, not one synthetic batch.
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(painter(tester).walls, isEmpty);
  });

  testWidgets('Enter finishes and Escape or tool switch cancels only preview', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    await click(tester, const Offset(180, 220));
    await click(tester, const Offset(420, 220));
    final top = tester.getTopLeft(canvas());
    final preview = await tester.startGesture(top + const Offset(420, 220));
    await preview.moveTo(top + const Offset(420, 420));
    await tester.pump();
    expect(painter(tester).draft, isNotNull);
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();
    await preview.cancel();
    expect(painter(tester).walls, hasLength(1));
    expect(painter(tester).draft, isNull);
    expect(painter(tester).snapPoint, isNull);
    expect(painter(tester).snapGuide, isNull);

    final select = tester.widget<ChoiceChip>(
      find.widgetWithText(ChoiceChip, 'Select / draw'),
    );
    select.onSelected!(true);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final canceled = await tester.startGesture(top + const Offset(420, 220));
    await canceled.moveTo(top + const Offset(500, 300));
    await tester.pump();
    expect(painter(tester).draft, isNotNull);
    final selectAgain = tester.widget<ChoiceChip>(
      find.widgetWithText(ChoiceChip, 'Select / draw'),
    );
    selectAgain.onSelected!(true);
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
    expect(painter(tester).draft, isNull);
    expect(painter(tester).snapPoint, isNull);
    expect(painter(tester).snapGuide, isNull);
    await canceled.cancel();

    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final escaped = await tester.startGesture(top + const Offset(420, 220));
    await escaped.moveTo(top + const Offset(500, 300));
    await tester.pump();
    expect(painter(tester).draft, isNotNull);
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
    expect(painter(tester).draft, isNull);
    await escaped.cancel();
  });

  testWidgets(
    'double-click and secondary click finish without a trailing wall',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
      await tester.pumpAndSettle();
      await click(tester, const Offset(180, 220));
      final doublePoint = tester.getTopLeft(canvas()) + const Offset(420, 220);
      await tester.tapAt(doublePoint);
      await tester.pump(const Duration(milliseconds: 80));
      await tester.tapAt(doublePoint);
      await tester.pumpAndSettle();
      expect(painter(tester).walls, hasLength(1));
      expect(painter(tester).snapPoint, isNull);
      expect(painter(tester).snapGuide, isNull);
      await click(tester, const Offset(500, 300));
      expect(
        painter(tester).walls,
        hasLength(1),
        reason: 'double-click must finish, so this is a new anchor',
      );
      final secondary = await tester.startGesture(
        tester.getTopLeft(canvas()) + const Offset(650, 300),
        kind: PointerDeviceKind.mouse,
        buttons: kSecondaryMouseButton,
      );
      await secondary.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls, hasLength(1));
      expect(painter(tester).snapPoint, isNull);
      expect(painter(tester).snapGuide, isNull);
      await click(tester, const Offset(650, 400));
      expect(
        painter(tester).walls,
        hasLength(1),
        reason: 'right-click must finish, so this is another new anchor',
      );
    },
  );

  testWidgets('rejected reverse segment keeps the accepted chain anchor', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    await click(tester, const Offset(180, 220));
    await click(tester, const Offset(420, 220));
    expect(painter(tester).walls, hasLength(1));
    final anchor = painter(tester).walls.single.end;

    // The reverse segment overlaps the accepted wall and is refused by the
    // core. The next accepted segment must still start at the old endpoint.
    await click(tester, const Offset(180, 220));
    expect(painter(tester).walls, hasLength(1));
    expect(find.textContaining('refused:'), findsOneWidget);
    await click(tester, const Offset(420, 420));
    expect(painter(tester).walls, hasLength(2));
    expect(painter(tester).walls.last.start, anchor);
  });

  testWidgets(
    'Alt latched at chain start disables snapping for later segments',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
      await tester.pumpAndSettle();
      final view = painter(tester).view;
      const first = Offset(183, 223);
      const end = Offset(427, 241);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await click(tester, first);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await click(tester, end);
      final wall = painter(tester).walls.single;
      expect(wall.start, view.toPlan(first));
      expect(wall.end, view.toPlan(end));
      const next = Offset(543, 347);
      await click(tester, next);
      expect(painter(tester).walls, hasLength(2));
      expect(painter(tester).walls.last.start, wall.end);
      expect(painter(tester).walls.last.end, view.toPlan(next));
    },
  );
}
