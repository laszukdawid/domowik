import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/main.dart';
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
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
  });

  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(home: PlanPage(bridgeLoader: () => FfiCore(bridge))),
    );
    await tester.pumpAndSettle();
    expect(bridge.openPlans(), 1);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(bridge.openPlans(), 0);
      expect(bridge.outstandingBuffers(), 0);
    });
  }

  Finder getCanvas() => find.byWidgetPredicate(
    (widget) => widget is CustomPaint && widget.painter is PlanPainter,
  );

  PlanPainter painter(WidgetTester tester) =>
      tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter;

  IconButton snapButton(WidgetTester tester, String key) =>
      tester.widget<IconButton>(find.byKey(ValueKey(key)));

  Future<void> draw(WidgetTester tester, Offset start, Offset end) async {
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final gesture = await tester.startGesture(start);
    await gesture.moveTo(end);
    await gesture.up();
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ChoiceChip, 'Select'));
    await tester.pumpAndSettle();
  }

  testWidgets('creation uses default native grid and endpoint snapping', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    // Deliberately off the lattice; the native grid query supplies the start
    // and end points before the first wall is committed.
    await draw(
      tester,
      top + const Offset(103, 153),
      top + const Offset(329, 171),
    );
    final first = painter(tester).walls.single;
    final rawStart = painter(tester).view.toPlan(const Offset(103, 153));
    expect(first.start, isNot(rawStart));

    final endpoint = top + painter(tester).toScreen(first.end);
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final gesture = await tester.startGesture(top + const Offset(600, 600));
    await gesture.moveTo(endpoint + const Offset(2, 1));
    await tester.pump();
    expect(
      painter(tester).draft!.$2,
      first.end,
      reason: 'object snapping is on by default and outranks grid',
    );
    await gesture.cancel();
  });

  testWidgets(
    'snap preference toggles are session-only and do not dirty history',
    (tester) async {
      await start(tester);
      for (final key in ['snap-grid', 'snap-object', 'snap-angle']) {
        expect(snapButton(tester, key).isSelected, isTrue);
        await tester.ensureVisible(find.byKey(ValueKey(key)));
        await tester.tap(find.byKey(ValueKey(key)));
        await tester.pumpAndSettle();
        expect(snapButton(tester, key).isSelected, isFalse);
      }
      expect(find.textContaining('• Unsaved'), findsNothing);
      expect(
        tester
            .widget<IconButton>(
              find.byWidgetPredicate(
                (widget) => widget is IconButton && widget.tooltip == 'Undo',
              ),
            )
            .onPressed,
        isNull,
        reason: 'preferences must not enter undo history',
      );
      await tester.tap(find.byTooltip('New plan'));
      await tester.pumpAndSettle();
      for (final key in ['snap-grid', 'snap-object', 'snap-angle']) {
        expect(snapButton(tester, key).isSelected, isTrue);
      }
    },
  );

  testWidgets('Alt is latched at down: releasing it mid-drag stays raw', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    const localEnd = Offset(333, 177);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    final gesture = await tester.startGesture(top + const Offset(103, 153));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await gesture.moveTo(top + localEnd);
    await tester.pump();
    expect(painter(tester).draft!.$2, painter(tester).view.toPlan(localEnd));
    await gesture.up();
    await tester.pumpAndSettle();
    expect(
      painter(tester).walls.single.end,
      painter(tester).view.toPlan(localEnd),
    );
  });

  testWidgets('release without a move snaps both wall endpoints', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    final view = painter(tester).view;
    await tester.startGesture(top + const Offset(103, 153), pointer: 78);
    await tester.sendEventToBinding(
      PointerUpEvent(pointer: 78, position: top + const Offset(333, 177)),
    );
    await tester.pumpAndSettle();
    final wall = painter(tester).walls.single;
    expect(wall.start, isNot(view.toPlan(const Offset(103, 153))));
    expect(wall.end, isNot(view.toPlan(const Offset(333, 177))));
    for (final point in [wall.start, wall.end]) {
      expect(point.dx % view.gridStepMm, 0);
      expect(point.dy % view.gridStepMm, 0);
    }
    expect(painter(tester).snapPoint, isNull);
  });

  testWidgets('live preferences change the cursor but preserve its anchor', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    const from = Offset(103, 153), to = Offset(333, 177);
    final gesture = await tester.startGesture(top + from);
    await gesture.moveTo(top + to);
    await tester.pump();
    final anchor = painter(tester).draft!.$1;
    expect(painter(tester).snapPoint, isNotNull);
    // Invoke controls without injecting a second touch, which cancels drawing.
    for (final key in ['snap-grid', 'snap-object', 'snap-angle']) {
      snapButton(tester, key).onPressed!();
      await tester.pump();
    }
    expect(painter(tester).draft, (anchor, painter(tester).view.toPlan(to)));
    expect(painter(tester).snapPoint, isNull);
    expect(painter(tester).snapGuide, isNull);
    expect(find.textContaining('• Unsaved'), findsNothing);
    await gesture.up();
    await tester.pumpAndSettle();
    expect(painter(tester).walls.single.start, anchor);
    expect(painter(tester).walls.single.end, painter(tester).view.toPlan(to));
  });

  testWidgets('camera changes reevaluate the native preview before release', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    const to = Offset(333, 177);
    final gesture = await tester.startGesture(top + const Offset(103, 153));
    await gesture.moveTo(top + to);
    await tester.pump();
    final before = painter(tester).draft!.$2;
    await tester.sendEventToBinding(
      PointerScrollEvent(position: top + to, scrollDelta: const Offset(0, 103)),
    );
    await tester.pumpAndSettle();
    final preview = painter(tester).draft!.$2;
    expect(preview, isNot(before));
    expect(painter(tester).snapPoint, preview);
    expect(preview.dx % painter(tester).view.gridStepMm, 0);
    expect(preview.dy % painter(tester).view.gridStepMm, 0);
    await gesture.up();
    await tester.pumpAndSettle();
    expect(painter(tester).walls.single.end, preview);
    expect(painter(tester).snapPoint, isNull);
  });

  testWidgets(
    'native grid refusal suppresses commit and preserves the anchor on retry',
    (tester) async {
      await start(tester);
      final top = tester.getTopLeft(getCanvas());
      final gesture = await tester.startGesture(top + const Offset(103, 153));
      await gesture.moveTo(top + const Offset(333, 177));
      await tester.pump();
      final anchor = painter(tester).draft!.$1;
      // All input coordinates are finite. The native lattice index, however,
      // cannot represent this cursor, so this is a refusal rather than a miss.
      await gesture.moveTo(top + const Offset(1e24, 177));
      await tester.pump();
      expect(painter(tester).draft, isNull);
      expect(painter(tester).snapPoint, isNull);
      expect(find.textContaining('Snap failed:'), findsOneWidget);
      expect(painter(tester).walls, isEmpty);
      expect(find.textContaining('• Unsaved'), findsNothing);
      for (final key in ['snap-grid', 'snap-object', 'snap-angle']) {
        snapButton(tester, key).onPressed!();
      }
      await gesture.moveTo(top + const Offset(333, 177));
      await tester.pump();
      expect(painter(tester).draft!.$1, anchor);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls.single.start, anchor);
      expect(
        painter(tester).walls.single.end,
        painter(tester).view.toPlan(const Offset(333, 177)),
      );
    },
  );

  testWidgets(
    'release at an unrepresentable grid index never commits a raw wall',
    (tester) async {
      await start(tester);
      final top = tester.getTopLeft(getCanvas());
      final gesture = await tester.startGesture(top + const Offset(103, 153));
      await gesture.moveTo(top + const Offset(1e24, 177));
      await gesture.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls, isEmpty);
      expect(painter(tester).draft, isNull);
      expect(painter(tester).snapPoint, isNull);
      expect(painter(tester).snapGuide, isNull);
      expect(find.textContaining('• Unsaved'), findsNothing);
      expect(find.textContaining('Snap failed:'), findsOneWidget);
    },
  );

  testWidgets('Alt pressed after down does not disable that creation snap', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    const localEnd = Offset(333, 177);
    final gesture = await tester.startGesture(top + const Offset(103, 153));
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await gesture.moveTo(top + localEnd);
    await tester.pump();
    expect(
      painter(tester).draft!.$2,
      isNot(painter(tester).view.toPlan(localEnd)),
    );
    await gesture.up();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pumpAndSettle();
  });

  testWidgets(
    'snap guide clears on cancellation and up uses its actual position',
    (tester) async {
      await start(tester);
      final top = tester.getTopLeft(getCanvas());
      await draw(
        tester,
        top + const Offset(103, 153),
        top + const Offset(329, 171),
      );
      final wall = painter(tester).walls.single;
      await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
      await tester.pumpAndSettle();
      final gesture = await tester.startGesture(top + const Offset(600, 600));
      await gesture.moveTo(
        top +
            painter(tester).toScreen((wall.start + wall.end) / 2) +
            const Offset(0, 2),
      );
      await tester.pump();
      expect(painter(tester).snapGuide, isNotNull);
      await gesture.cancel();
      await tester.pumpAndSettle();
      expect(painter(tester).snapGuide, isNull);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      final release = top + const Offset(401, 233);
      final captured = await tester.startGesture(
        top + const Offset(600, 600),
        pointer: 77,
      );
      await captured.moveBy(const Offset(100, 0));
      await tester.sendEventToBinding(
        PointerUpEvent(pointer: 77, position: release),
      );
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();
      expect(
        painter(tester).toScreen(painter(tester).walls.last.end),
        release - top,
      );
    },
  );

  // ------------------------------------------------------------------
  // Editing drags (issue 0058): the legacy `_snap` helper is gone and
  // endpoint/body drags go through the same native query creation uses.
  // ------------------------------------------------------------------

  Future<void> setToggle(WidgetTester tester, String key, bool on) async {
    if (snapButton(tester, key).isSelected == on) return;
    await tester.tap(find.byKey(ValueKey(key)));
    await tester.pumpAndSettle();
  }

  testWidgets('an endpoint drag lands exactly on another wall endpoint', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    await draw(
      tester,
      top + const Offset(103, 153),
      top + const Offset(329, 171),
    );
    final target = painter(tester).walls.single.end;
    await draw(
      tester,
      top + const Offset(600, 400),
      top + const Offset(800, 500),
    );
    final moving = painter(tester).walls.last;

    // Grab the second wall's start and drop it a few pixels from the first
    // wall's end. Object snapping outranks the grid, so the commit must carry
    // the endpoint itself and not the pointer's own lattice neighbour.
    final grab = top + painter(tester).toScreen(moving.start);
    final gesture = await tester.startGesture(grab);
    await gesture.moveTo(
      top + painter(tester).toScreen(target) + const Offset(3, -2),
    );
    await gesture.up();
    await tester.pumpAndSettle();

    expect(painter(tester).walls.length, 2);
    expect(
      painter(tester).walls.map((wall) => wall.start).contains(target),
      isTrue,
      reason: painter(tester).walls.map((w) => '${w.start}-${w.end}').join(' '),
    );
  });

  testWidgets(
    'endpoint and body drags preview before committing and clear on cancel',
    (tester) async {
      await start(tester);
      final top = tester.getTopLeft(getCanvas());
      await draw(
        tester,
        top + const Offset(200, 400),
        top + const Offset(600, 400),
      );
      final wall = painter(tester).walls.single;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);

      final endpoint = await tester.startGesture(
        top + painter(tester).toScreen(wall.end),
      );
      final end = top + const Offset(700, 300);
      await endpoint.moveTo(end);
      await tester.pump();
      expect(painter(tester).walls.single.end, wall.end);
      expect(painter(tester).editPreview, [
        (wall.start, painter(tester).view.toPlan(end - top), wall.thicknessMm),
      ]);
      await endpoint.cancel();
      await tester.pump();
      expect(painter(tester).editPreview, isNull);
      expect(painter(tester).walls.single.end, wall.end);

      final body = await tester.startGesture(top + const Offset(400, 400));
      await body.moveBy(const Offset(0, -80));
      await tester.pump();
      final delta = const Offset(0, 80 / 0.1);
      expect(painter(tester).walls.single.start, wall.start);
      expect(painter(tester).editPreview, [
        (wall.start + delta, wall.end + delta, wall.thicknessMm),
      ]);
      await body.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();
      expect(painter(tester).editPreview, isNull);
      expect(painter(tester).walls.single.start, wall.start + delta);
    },
  );

  testWidgets('a dragged wall is not offered its own axis to snap to', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    await draw(
      tester,
      top + const Offset(200, 400),
      top + const Offset(600, 400),
    );
    // Grid off, so the only candidates left in the document belong to the
    // wall being dragged — and R6.1.3 removes every one of them by identity.
    await setToggle(tester, 'snap-grid', false);
    final wall = painter(tester).walls.single;

    // Two pixels above the wall's own axis line, well beyond its end: without
    // self-exclusion the axis extension wins at a perpendicular distance of
    // two pixels and the commit would carry the projection instead.
    final release = top + const Offset(900, 398);
    final raw = painter(tester).view.toPlan(release - top);
    final gesture = await tester.startGesture(
      top + painter(tester).toScreen(wall.end),
    );
    await gesture.moveTo(release);
    await gesture.up();
    await tester.pumpAndSettle();

    final moved = painter(tester).walls.single;
    expect(moved.end, raw);
    expect(moved.end.dy, isNot(wall.end.dy));
    expect(moved.start, wall.start);
  });

  testWidgets('Alt held from before an endpoint drag commits raw coordinates', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    await draw(
      tester,
      top + const Offset(200, 400),
      top + const Offset(600, 400),
    );
    final wall = painter(tester).walls.single;
    expect(snapButton(tester, 'snap-grid').isSelected, isTrue);

    final release = top + const Offset(747, 311);
    final raw = painter(tester).view.toPlan(release - top);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    final gesture = await tester.startGesture(
      top + painter(tester).toScreen(wall.end),
    );
    await gesture.moveTo(release);
    await gesture.up();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pumpAndSettle();

    expect(painter(tester).walls.single.end, raw);
  });

  // ------------------------------------------------------------------
  // Named section 6.7 lifecycle fixtures whose substance is event
  // handling rather than coordinates.
  // ------------------------------------------------------------------

  testWidgets('S11: five logical pixels is a click however long it is held', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    // S11 states raw device (200,200) -> (206,208) at DPR 2, which is a
    // LOGICAL displacement of (3,4). Widget-test coordinates are already
    // logical, so the fixture's own displacement is used directly.
    const below = Offset(3, 4);
    expect(below.distance, 5.0);

    final held = await tester.startGesture(top + const Offset(100, 100));
    await held.moveBy(below);
    // "HOLD DURATION participates in NO classification": three seconds of it.
    await tester.pump(const Duration(seconds: 3));
    await held.up();
    await tester.pumpAndSettle();
    expect(
      painter(tester).walls,
      isEmpty,
      reason: 'exactly 5 px does not exceed the threshold, so this is a click',
    );

    // "A later (6,8)-logical-px move (squared 100 px^2) classifies drag."
    const above = Offset(6, 8);
    expect(above.distanceSquared, 100.0);
    final dragged = await tester.startGesture(top + const Offset(100, 100));
    await dragged.moveBy(above);
    await dragged.up();
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
  });

  testWidgets('S1: a snapped start auto-joins in one batch and one undo', (
    tester,
  ) async {
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    await draw(
      tester,
      top + const Offset(103, 153),
      top + const Offset(329, 171),
    );
    final first = painter(tester).walls.single;

    // S1 arms the WALL TOOL, where a press always draws: under the hybrid
    // Select / draw tool a press on an endpoint grabs it instead.
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();

    // Begin the second wall a few pixels from the first wall's end. Object
    // snapping is on by default, so the committed start is the endpoint
    // itself rather than the pointer's own position.
    await tester.tapAt(
      top + painter(tester).toScreen(first.end) + const Offset(4, -3),
    );
    await tester.pumpAndSettle();
    await tester.tapAt(top + const Offset(700, 520));
    await tester.pumpAndSettle();
    await tester.sendKeyEvent(LogicalKeyboardKey.enter);
    await tester.pumpAndSettle();

    expect(painter(tester).walls, hasLength(2));
    final second = painter(tester).walls.last;
    expect(second.start, first.end);

    // ONE batch, ONE undo entry: a single undo takes the wall AND the join
    // it induced, restoring the one-wall state.
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(painter(tester).walls, hasLength(1));
    expect(painter(tester).walls.single.start, first.start);
    expect(painter(tester).walls.single.end, first.end);
  });

  testWidgets('Alt pressed after an edit drag starts does not disable it', (
    tester,
  ) async {
    // R6.4.2's third row, on an editing gesture: the disable decision is
    // sampled at gesture START, so pressing Alt mid-drag affects only later
    // gestures.
    await start(tester);
    final top = tester.getTopLeft(getCanvas());
    await draw(
      tester,
      top + const Offset(200, 400),
      top + const Offset(600, 400),
    );
    final wall = painter(tester).walls.single;

    final release = top + const Offset(747, 311);
    final raw = painter(tester).view.toPlan(release - top);
    final gesture = await tester.startGesture(
      top + painter(tester).toScreen(wall.end),
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await gesture.moveTo(release);
    await gesture.up();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    await tester.pumpAndSettle();

    expect(
      painter(tester).walls.single.end,
      isNot(raw),
      reason: 'Alt was not held at gesture start, so snapping still applies',
    );
  });
}
