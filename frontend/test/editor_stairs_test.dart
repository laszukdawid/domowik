import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';
import 'dart:math' as math;

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/staircase_properties_form.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class _StairCore extends FfiCore {
  _StairCore(super.bridge);
  int handle = 0;
  int stairEdits = 0, derivedReads = 0;
  String? renderDiagnostic;
  @override
  CoreReply edit(int handle, String request) {
    if ((jsonDecode(request) as Map)['kind'] == 'setStaircase') stairEdits++;
    return super.edit(handle, request);
  }

  @override
  CoreReply derived(int handle, String levelId) {
    derivedReads++;
    final reply = super.derived(handle, levelId);
    if (!reply.succeeded || renderDiagnostic == null) return reply;
    final payload = jsonDecode(reply.text!) as Map<String, Object?>;
    payload['constructionDiagnostics'] = [renderDiagnostic];
    return CoreReply(reply.code, text: jsonEncode(payload));
  }

  @override
  CoreHandle createDefault() {
    final h = super.createDefault();
    handle = h.handle;
    final reply = execute(
      handle,
      jsonEncode({
        'operations': [
          {
            'operationId': '01950000-0000-7000-8000-000000000101',
            'entityId': '01950000-0000-7000-8000-000000000102',
            'kind': 'createEntity',
            'entity': {
              'type': 'level',
              'id': '01950000-0000-7000-8000-000000000102',
              'name': 'Upper',
              'elevationMm': 2600,
              'floorThicknessMm': 200,
              'defaultWallHeightMm': 2500,
              'defaultCeilingHeightMm': 2500,
            },
          },
        ],
      }),
    );
    expect(reply.succeeded, isTrue);
    return h;
  }

  Map<String, Object?> get document =>
      jsonDecode(snapshot(handle).text!) as Map<String, Object?>;
  List<Object?> get stairs =>
      (document['staircases'] as List<Object?>?) ?? const [];
}

class _StairDialogs implements FileDialogs {
  _StairDialogs(this.path);
  final String path;
  @override
  Future<PlanSource?> openPlan() async => null;
  @override
  Future<PlanTarget?> savePlan(String suggestedName) async =>
      PlanPathTarget(path);
  @override
  void setCloseHandler(Future<bool> Function()? handler) {}
}

void main() {
  late _StairCore core;
  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    final override = Platform.environment['DOMOWIK_TEST_LIBRARY'];
    core = _StairCore(
      Bridge.fromLibrary(
        DynamicLibrary.open(
          override ??
              File('target/release/libdomowik_bridge.$suffix').absolute.path,
        ),
      ),
    );
  });
  Finder canvas() => find.byWidgetPredicate(
    (w) => w is CustomPaint && w.painter is PlanPainter,
  );
  PlanPainter painter(WidgetTester tester) =>
      tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;
  Future<void> start(
    WidgetTester tester, {
    bool upper = true,
    FileDialogs? dialogs,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1280, 1000));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(bridgeLoader: () => core, fileDialogs: dialogs),
      ),
    );
    await tester.pumpAndSettle();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(core.openPlans(), 0);
      expect(core.outstandingBuffers(), 0);
    });
    await tester.tap(find.byKey(const ValueKey('inspector-floor-tab')));
    await tester.pumpAndSettle();
    if (upper) {
      await tester.tap(find.text('Upper'));
      await tester.pumpAndSettle();
    }
  }

  Future<void> addStair(WidgetTester tester) async {
    await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
    await tester.pumpAndSettle();
    await tester.dragFrom(
      tester.getTopLeft(canvas()) + const Offset(180, 300),
      const Offset(300, 0),
    );
    await tester.pumpAndSettle();
  }

  for (final scenario in <(String, Offset, double)>[
    ('up', const Offset(15, -280), 90),
    ('diagonal', const Offset(230, -250), 45),
    ('sideways', const Offset(280, -15), 0),
    ('15 degrees', const Offset(280, -80), 15),
  ]) {
    testWidgets(
      'Ctrl stair drag constrains ${scenario.$1} and commits the preview',
      (tester) async {
        await start(tester);
        await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
        await tester.pumpAndSettle();
        final before = core.document;
        final view = painter(tester).view;
        final top = tester.getTopLeft(canvas());
        const localStart = Offset(200, 350);
        final upperStart = view.toPlan(localStart);
        await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
        final drag = await tester.startGesture(
          top + localStart,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(top + localStart + scenario.$2);
        await tester.pumpAndSettle();
        final preview = painter(tester).constructionPreview.single;
        expect(core.document, before);
        await drag.up();
        await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
        await tester.pumpAndSettle();
        expect(
          painter(tester).derived.construction.single.outline,
          preview.outline,
        );
        final connection =
            (core.document['levelConnections'] as List).single as Map;
        final upper = (connection['to'] as Map)['positionMm'] as List;
        final lower = (connection['from'] as Map)['positionMm'] as List;
        expect(upper, [upperStart.dx, upperStart.dy]);
        final direction =
            Offset((lower[0] as num).toDouble(), (lower[1] as num).toDouble()) -
            upperStart;
        expect(
          math.atan2(direction.dy, direction.dx) * 180 / math.pi,
          closeTo(scenario.$3, 1e-9),
        );
        await tester.tap(find.byTooltip('Undo'));
        await tester.pumpAndSettle();
        expect(core.stairs, isEmpty);
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'Ctrl changes a stationary stair preview and releases without drift',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
      await tester.pumpAndSettle();
      final before = core.document;
      final top = tester.getTopLeft(canvas());
      final drag = await tester.startGesture(
        top + const Offset(200, 350),
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveTo(top + const Offset(480, 330));
      await tester.pumpAndSettle();
      final free = painter(tester).constructionPreview.single.outline;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      final constrained = painter(tester).constructionPreview.single.outline;
      expect(constrained, isNot(free));
      expect(core.document, before);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview.single.outline, free);
      await drag.up();
      await tester.pumpAndSettle();
      expect(painter(tester).derived.construction.single.outline, free);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'click-click stair drawing responds to Ctrl without another hover',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
      await tester.pumpAndSettle();
      final top = tester.getTopLeft(canvas());
      await tester.tapAt(top + const Offset(200, 350));
      final hover = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await hover.addPointer(location: top + const Offset(200, 350));
      await hover.moveTo(top + const Offset(480, 330));
      await tester.pumpAndSettle();
      final free = painter(tester).constructionPreview.single.outline;
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlRight);
      await tester.pumpAndSettle();
      final constrained = painter(tester).constructionPreview.single.outline;
      expect(constrained, isNot(free));
      expect(core.stairs, isEmpty);
      await tester.tapAt(top + const Offset(480, 330));
      await tester.pumpAndSettle();
      expect(painter(tester).derived.construction.single.outline, constrained);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlRight);
      await hover.removePointer();
      expect(tester.takeException(), isNull);
    },
  );

  Future<void> selectTool(WidgetTester tester) async {
    await tester.tap(
      find.descendant(
        of: find.byType(ChoiceChip),
        matching: find.text('Select'),
      ),
    );
    await tester.pumpAndSettle();
  }

  Offset stairCenter(WidgetTester tester) {
    final p = painter(tester);
    final outline = p.derived.construction.single.outline;
    final center = outline.reduce((a, b) => a + b) / outline.length.toDouble();
    return tester.getTopLeft(canvas()) + p.view.toScreen(center);
  }

  for (final lower in [false, true]) {
    for (final key in [
      LogicalKeyboardKey.delete,
      LogicalKeyboardKey.backspace,
    ]) {
      testWidgets(
        '${key.keyLabel} deletes selected stairs from ${lower ? 'lower' : 'upper'} floor and undoes atomically',
        (tester) async {
          await start(tester);
          await addStair(tester);
          if (lower) {
            await tester.tap(find.text('Level 1'));
            await tester.pumpAndSettle();
          }
          await selectTool(tester);
          await tester.tapAt(stairCenter(tester));
          await tester.pumpAndSettle();
          final before = core.document;
          expect(painter(tester).selectedIds, hasLength(1));
          await tester.sendKeyEvent(key);
          await tester.pumpAndSettle();
          final deleted = core.document;
          expect(core.stairs, isEmpty);
          expect(deleted['levelConnections'] ?? const [], isEmpty);
          expect(deleted['hostedOpenings'], before['hostedOpenings']);
          expect(painter(tester).selectedIds, isEmpty);
          expect(painter(tester).stairHandles, isEmpty);
          await tester.tap(find.byTooltip('Undo'));
          await tester.pumpAndSettle();
          expect(core.document, before);
          expect(painter(tester).selectedIds, isEmpty);
          await tester.tap(find.byTooltip('Redo'));
          await tester.pumpAndSettle();
          expect(core.document, deleted);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }

  testWidgets('Selection panel deletes stairs with the same undoable command', (
    tester,
  ) async {
    await start(tester);
    await addStair(tester);
    await selectTool(tester);
    await tester.tapAt(stairCenter(tester));
    await tester.pumpAndSettle();
    final before = core.document;
    await tester.tap(find.byKey(const ValueKey('delete-staircase')));
    await tester.pumpAndSettle();
    expect(core.stairs, isEmpty);
    expect(core.document['levelConnections'] ?? const [], isEmpty);
    expect(core.document['hostedOpenings'], before['hostedOpenings']);
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(core.document, before);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Delete and Backspace inside stair name fields never remove the assembly',
    (tester) async {
      await start(tester);
      await addStair(tester);
      final input = find.byKey(const PageStorageKey('stair-name'));
      await tester.ensureVisible(input);
      await tester.enterText(input, 'Synthetic stairs');
      for (final key in [
        LogicalKeyboardKey.delete,
        LogicalKeyboardKey.backspace,
      ]) {
        await tester.sendKeyEvent(key);
        await tester.pumpAndSettle();
        expect(core.stairs, hasLength(1));
        expect(core.document['levelConnections'], hasLength(1));
        expect(core.document['hostedOpenings'], hasLength(1));
      }
      expect(tester.takeException(), isNull);
    },
  );

  for (final lower in [false, true]) {
    for (final edge in ['left', 'right', 'lower', 'upper']) {
      testWidgets(
        '$edge handle resizes from ${lower ? 'lower' : 'upper'} floor with one undoable edit',
        (tester) async {
          await start(tester);
          await addStair(tester);
          if (lower) {
            await tester.tap(find.text('Level 1'));
            await tester.pumpAndSettle();
          }
          await selectTool(tester);
          await tester.tapAt(stairCenter(tester));
          await tester.pumpAndSettle();
          expect(painter(tester).stairHandles, hasLength(4));
          final before = core.document;
          final outline = painter(tester).derived.construction.single.outline;
          final handle = painter(tester).stairHandles
              .singleWhere((h) => h.edge == edge);
          final at = tester.getTopLeft(canvas()) + handle.position;
          final inward = handle.direction / handle.direction.distance * -30;
          final drag = await tester.startGesture(
            at,
            kind: lower && edge == 'left'
                ? PointerDeviceKind.touch
                : PointerDeviceKind.mouse,
          );
          await drag.moveTo(at + inward);
          await tester.pumpAndSettle();
          expect(core.document, before);
          expect(painter(tester).resizingStairId, painter(tester).selected);
          final preview = painter(tester).constructionPreview.single;
          final fixed = switch (edge) {
            'left' => [0, 1],
            'right' => [2, 3],
            'lower' => [1, 2],
            _ => [0, 3],
          };
          for (final i in fixed) {
            expect((outline[i] - preview.outline[i]).distance, lessThan(1e-6));
          }
          await drag.up();
          await tester.pumpAndSettle();
          final resized = core.document;
          expect(resized, isNot(before));
          expect(
            painter(tester).derived.construction.single.outline,
            preview.outline,
          );
          expect(painter(tester).constructionPreview, isEmpty);
          expect(painter(tester).resizingStairId, isNull);
          expect(painter(tester).stairHandles, hasLength(4));
          final oldStair = (before['staircases'] as List).single as Map;
          final newStair = (resized['staircases'] as List).single as Map;
          expect(newStair['risers'], oldStair['risers']);
          expect(newStair['guardHeightMm'], oldStair['guardHeightMm']);
          if (edge == 'left' || edge == 'right') {
            expect(newStair['widthMm'], lessThan(oldStair['widthMm'] as num));
          } else {
            expect(newStair['widthMm'], oldStair['widthMm']);
          }
          final connection =
              (resized['levelConnections'] as List).single as Map;
          final cut =
              ((resized['hostedOpenings'] as List).single as Map)['footprint']
                  as Map;
          expect(cut['startMm'], (connection['from'] as Map)['positionMm']);
          expect(cut['endMm'], (connection['to'] as Map)['positionMm']);
          expect(cut['widthMm'], newStair['widthMm']);
          await tester.tap(find.byTooltip('Undo'));
          await tester.pumpAndSettle();
          expect(core.document, before);
          await tester.tap(find.byTooltip('Redo'));
          await tester.pumpAndSettle();
          expect(core.document, resized);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }

  testWidgets(
    'cancelled, invalid and return-to-grab stair resizes never add history',
    (tester) async {
      await start(tester);
      await addStair(tester);
      await selectTool(tester);
      await tester.tapAt(stairCenter(tester));
      await tester.pumpAndSettle();
      final before = core.document;
      final selected = painter(tester).selected;
      final handle = painter(tester).stairHandles.first;
      final at = tester.getTopLeft(canvas()) + handle.position;
      final inward = handle.direction / handle.direction.distance * -30;
      for (final cancel in ['escape', 'pointer', 'focus']) {
        final drag = await tester.startGesture(
          at,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(at + inward);
        await tester.pumpAndSettle();
        expect(painter(tester).constructionPreview, isNotEmpty);
        if (cancel == 'escape') {
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
          await drag.up();
        } else if (cancel == 'pointer') {
          await drag.cancel();
        } else {
          FocusManager.instance.primaryFocus?.unfocus();
          await tester.pump();
          await drag.up();
        }
        await tester.pumpAndSettle();
        expect(core.document, before);
        expect(painter(tester).constructionPreview, isEmpty);
        expect(painter(tester).resizingStairId, isNull);
        expect(painter(tester).selected, selected);
      }
      final invalid = await tester.startGesture(
        at,
        kind: PointerDeviceKind.mouse,
      );
      await invalid.moveTo(at + inward * 10);
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview, isEmpty);
      await invalid.up();
      await tester.pumpAndSettle();
      expect(core.document, before);
      final noOp = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await noOp.moveTo(at + inward);
      await noOp.moveTo(at);
      await noOp.up();
      await tester.pumpAndSettle();
      expect(core.document, before);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      expect(core.stairs, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  for (final lower in [false, true]) {
    for (final edge in ['lower', 'upper']) {
      testWidgets(
        '$edge end changes length and angle from ${lower ? 'lower' : 'upper'} floor with live Ctrl snapping',
        (tester) async {
          await start(tester);
          await addStair(tester);
          if (lower) {
            await tester.tap(find.text('Level 1'));
            await tester.pumpAndSettle();
          }
          await selectTool(tester);
          await tester.tapAt(stairCenter(tester));
          await tester.pumpAndSettle();
          final before = core.document;
          final oldConnection =
              (before['levelConnections'] as List).single as Map;
          final pivot = edge == 'lower' ? 'to' : 'from';
          final moving = edge == 'lower' ? 'from' : 'to';
          final handle = painter(tester).stairHandles
              .singleWhere((h) => h.edge == edge);
          // Grab slightly off-centre: the edit must use displacement, not jump
          // the endpoint to the precise cursor location.
          final at =
              tester.getTopLeft(canvas()) +
              handle.position +
              const Offset(3, 2);
          final screenDelta = Offset(edge == 'lower' ? 60 : -60, -100);
          final zoom = painter(tester).view.zoom;
          final drag = await tester.startGesture(
            at,
            kind: PointerDeviceKind.mouse,
          );
          await drag.moveTo(at + screenDelta);
          await tester.pumpAndSettle();
          final free = painter(tester).constructionPreview.single.outline;
          expect(core.document, before);
          await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
          await tester.pumpAndSettle();
          final snapped = painter(tester).constructionPreview.single.outline;
          expect(snapped, isNot(free));
          await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
          await tester.pumpAndSettle();
          expect(painter(tester).constructionPreview.single.outline, free);
          expect(core.document, before);
          await drag.up();
          await tester.pumpAndSettle();
          final freelyChanged = core.document;
          final freeConnection =
              (freelyChanged['levelConnections'] as List).single as Map;
          final oldPosition =
              (oldConnection[moving] as Map)['positionMm'] as List;
          final newPosition =
              (freeConnection[moving] as Map)['positionMm'] as List;
          expect(
            newPosition[0],
            closeTo((oldPosition[0] as num) + screenDelta.dx / zoom, 1e-6),
          );
          expect(
            newPosition[1],
            closeTo((oldPosition[1] as num) - screenDelta.dy / zoom, 1e-6),
          );
          expect(freeConnection[pivot], oldConnection[pivot]);
          expect(painter(tester).derived.construction.single.outline, free);
          await tester.tap(find.byTooltip('Undo'));
          await tester.pumpAndSettle();
          expect(core.document, before);
          // Repeat the same grab and commit its constrained preview.
          await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
          final constrainedDrag = await tester.startGesture(
            at,
            kind: PointerDeviceKind.mouse,
          );
          await constrainedDrag.moveTo(at + screenDelta);
          await tester.pumpAndSettle();
          expect(painter(tester).constructionPreview.single.outline, snapped);
          await constrainedDrag.up();
          await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
          await tester.pumpAndSettle();
          final changed = core.document;
          final connection =
              (changed['levelConnections'] as List).single as Map;
          expect(connection[pivot], oldConnection[pivot]);
          expect(changed['staircases'], before['staircases']);
          final from = (connection['from'] as Map)['positionMm'] as List;
          final to = (connection['to'] as Map)['positionMm'] as List;
          final steps =
              math.atan2(
                (to[1] as num).toDouble() - (from[1] as num).toDouble(),
                (to[0] as num).toDouble() - (from[0] as num).toDouble(),
              ) *
              180 /
              math.pi /
              15;
          expect(steps, closeTo(steps.roundToDouble(), 1e-10));
          final cut =
              ((changed['hostedOpenings'] as List).single as Map)['footprint']
                  as Map;
          expect(cut['startMm'], from);
          expect(cut['endMm'], to);
          expect(painter(tester).derived.construction.single.outline, snapped);
          await tester.tap(find.byTooltip('Undo'));
          await tester.pumpAndSettle();
          expect(core.document, before);
          await tester.tap(find.byTooltip('Redo'));
          await tester.pumpAndSettle();
          expect(core.document, changed);
          expect(tester.takeException(), isNull);
        },
      );
    }
  }

  testWidgets(
    'navigation reprojects a captured stair resize and commit matches it',
    (tester) async {
      await start(tester);
      await addStair(tester);
      await selectTool(tester);
      await tester.tapAt(stairCenter(tester));
      await tester.pumpAndSettle();
      final before = core.document;
      final handle = painter(tester).stairHandles.first;
      final at = tester.getTopLeft(canvas()) + handle.position;
      final drag = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await drag.moveTo(at - handle.direction / handle.direction.distance * 20);
      await tester.pumpAndSettle();
      final first = painter(tester).constructionPreview.single.outline;
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowUp);
      await tester.pumpAndSettle();
      final reprojected = painter(tester).constructionPreview.single.outline;
      expect(reprojected, isNot(first));
      expect(core.document, before);
      await drag.up();
      await tester.pumpAndSettle();
      expect(painter(tester).derived.construction.single.outline, reprojected);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'Escape cancels a Ctrl-constrained end rotation without delayed edits or history',
    (tester) async {
      await start(tester);
      await addStair(tester);
      await selectTool(tester);
      await tester.tapAt(stairCenter(tester));
      await tester.pumpAndSettle();
      final before = core.document;
      final selected = painter(tester).selected;
      final handle = painter(tester).stairHandles
          .singleWhere((h) => h.edge == 'upper');
      final at = tester.getTopLeft(canvas()) + handle.position;
      final drag = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await drag.moveTo(at + const Offset(-60, -100));
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview, isNotEmpty);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await drag.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(core.document, before);
      expect(painter(tester).constructionPreview, isEmpty);
      expect(painter(tester).resizingStairId, isNull);
      expect(painter(tester).selected, selected);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      expect(core.stairs, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );

  for (final lower in [false, true]) {
    testWidgets(
      'stairs select and drag with their cut from the ${lower ? 'lower' : 'upper'} floor',
      (tester) async {
        await start(tester);
        await addStair(tester);
        if (lower) {
          await tester.tap(find.text('Level 1'));
          await tester.pumpAndSettle();
        }
        await selectTool(tester);
        final before = core.document;
        final id = (core.stairs.single as Map)['id'];
        final at = stairCenter(tester);
        await tester.tapAt(at);
        await tester.pumpAndSettle();
        expect(painter(tester).selectedIds, {id});
        expect(painter(tester).selected, id);
        expect(core.document, before);
        final zoom = painter(tester).view.zoom;
        const screenDelta = Offset(60, 30);
        final drag = await tester.startGesture(
          at,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(at + screenDelta);
        await tester.pumpAndSettle();
        expect(core.document, before);
        expect(painter(tester).groupPreview!.$1, {id});
        expect(
          painter(tester).groupPreview!.$2.dx,
          closeTo(screenDelta.dx / zoom, 1e-6),
        );
        await drag.up();
        await tester.pumpAndSettle();
        final moved = core.document;
        expect(moved, isNot(before));
        expect(painter(tester).selectedIds, {id});
        expect(painter(tester).groupPreview, isNull);
        final oldConnection =
            (before['levelConnections'] as List).single as Map;
        final newConnection = (moved['levelConnections'] as List).single as Map;
        for (final end in ['from', 'to']) {
          final a = oldConnection[end] as Map, b = newConnection[end] as Map;
          expect(b['levelId'], a['levelId']);
          expect(b['offsetMm'], a['offsetMm']);
          final oldPosition = a['positionMm'] as List,
              newPosition = b['positionMm'] as List;
          expect(
            newPosition[0],
            closeTo((oldPosition[0] as num) + screenDelta.dx / zoom, 1e-6),
          );
          expect(
            newPosition[1],
            closeTo((oldPosition[1] as num) - screenDelta.dy / zoom, 1e-6),
          );
        }
        final cut =
            ((moved['hostedOpenings'] as List).single as Map)['footprint']
                as Map;
        expect(cut['startMm'], (newConnection['from'] as Map)['positionMm']);
        expect(cut['endMm'], (newConnection['to'] as Map)['positionMm']);
        expect(moved['staircases'], before['staircases']);
        await tester.tap(find.byTooltip('Undo'));
        await tester.pumpAndSettle();
        expect(core.document, before);
        await tester.tap(find.byTooltip('Redo'));
        await tester.pumpAndSettle();
        expect(core.document, moved);
        await tester.tap(find.byKey(const ValueKey('inspector-floor-tab')));
        await tester.pumpAndSettle();
        await tester.tap(find.text(lower ? 'Upper' : 'Level 1'));
        await tester.pumpAndSettle();
        await selectTool(tester);
        await tester.tapAt(stairCenter(tester));
        await tester.pumpAndSettle();
        expect(painter(tester).selectedIds, {id});
        expect(
          painter(tester).derived.construction.single.direction,
          lower ? 'DOWN' : 'UP',
        );
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'cancelling a stair drag restores selection and leaves history untouched',
    (tester) async {
      await start(tester);
      await addStair(tester);
      await selectTool(tester);
      final before = core.document;
      final at = stairCenter(tester);
      for (final escape in [true, false]) {
        final drag = await tester.startGesture(
          at,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(at + const Offset(50, 30));
        await tester.pumpAndSettle();
        expect(painter(tester).selectedIds, isNotEmpty);
        expect(painter(tester).groupPreview, isNotNull);
        if (escape) {
          await tester.sendKeyEvent(LogicalKeyboardKey.escape);
          await drag.up();
        } else {
          await drag.cancel();
        }
        await tester.pumpAndSettle();
        expect(core.document, before);
        expect(painter(tester).selectedIds, isEmpty);
        expect(painter(tester).groupPreview, isNull);
      }
      // Dragging out and back is also history-neutral.
      final drag = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await drag.moveTo(at + const Offset(50, 30));
      await drag.moveTo(at);
      await drag.up();
      await tester.pumpAndSettle();
      expect(core.document, before);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      expect(
        core.stairs,
        isEmpty,
      ); // Undo creation, not a cancelled/no-op move.
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'moved resized and rotated stairs and their opening survive saving and reopening',
    (tester) async {
      final directory = Directory(
        Platform.environment['DOMOWIK_TEST_TMPDIR'] ??
            Directory.systemTemp.path,
      ).createTempSync('stair-move-save-');
      addTearDown(() => directory.deleteSync(recursive: true));
      final path = '${directory.path}/synthetic.domo';
      await start(tester, dialogs: _StairDialogs(path));
      await addStair(tester);
      await selectTool(tester);
      await tester.dragFrom(stairCenter(tester), const Offset(50, 30));
      await tester.pumpAndSettle();
      final handle = painter(tester).stairHandles.first;
      await tester.dragFrom(
        tester.getTopLeft(canvas()) + handle.position,
        handle.direction / handle.direction.distance * -20,
      );
      await tester.pumpAndSettle();
      final endHandle = painter(tester).stairHandles
          .singleWhere((h) => h.edge == 'upper');
      await tester.dragFrom(
        tester.getTopLeft(canvas()) + endHandle.position,
        const Offset(-40, -25),
      );
      await tester.pumpAndSettle();
      final moved = core.document;
      await tester.tap(find.byKey(const ValueKey('save-action')));
      await tester.pumpAndSettle();
      final saved = core.openPath(path);
      expect(saved.code, 0);
      final snapshot = jsonDecode(core.snapshot(saved.handle).text!) as Map;
      for (final collection in [
        'levelConnections',
        'staircases',
        'hostedOpenings',
      ]) {
        expect(snapshot[collection], moved[collection]);
      }
      expect(core.close(saved.handle), 0);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'omitted stair diagnostics stay visible beside the valid drawing',
    (tester) async {
      core.renderDiagnostic =
          'Skipped staircase synthetic-invalid: invalid stair geometry';
      await start(tester);
      final banner = find.byKey(const ValueKey('level-warnings-banner'));
      expect(banner, findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('level-warnings-summary')));
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byType(AlertDialog),
          matching: find.text(core.renderDiagnostic!),
        ),
        findsOneWidget,
      );
      await tester.tap(find.byKey(const ValueKey('level-warnings-close')));
      await tester.pumpAndSettle();
      expect(canvas(), findsOneWidget);
    },
  );

  testWidgets(
    'rapid stair typing sends one command and preserves the focused editor',
    (tester) async {
      await start(tester);
      await addStair(tester);
      final input = find.byKey(const PageStorageKey('stair-risers'));
      await tester.ensureVisible(input);
      final controller = tester.widget<TextFormField>(input).controller!;
      final reads = core.derivedReads;
      for (final text in ['1', '16', '160']) {
        await tester.enterText(input, text);
        await tester.pump(const Duration(milliseconds: 30));
      }
      expect(core.stairEdits, 0);
      expect(core.derivedReads, reads);
      await tester.pump(StaircasePropertiesForm.editDelay);
      await tester.pumpAndSettle();
      expect(core.stairEdits, 1);
      expect(core.derivedReads - reads, inInclusiveRange(1, 2));
      expect(
        painter(tester).derived.construction.single.staircase!['risers'],
        160,
      );
      expect(tester.widget<TextFormField>(input).controller, same(controller));
      final editable = tester.widget<EditableText>(
        find.descendant(of: input, matching: find.byType(EditableText)),
      );
      expect(editable.focusNode.hasFocus, isTrue);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      expect(
        painter(tester).derived.construction.single.staircase!['risers'],
        15,
      );
      await tester.pump(const Duration(seconds: 1));
      expect(core.stairEdits, 1);
    },
  );

  testWidgets(
    'Undo immediately after typing flushes once and never reapplies a delayed edit',
    (tester) async {
      await start(tester);
      await addStair(tester);
      final input = find.byKey(const PageStorageKey('stair-name'));
      final original = painter(tester).derived.construction.single.name;
      await tester.ensureVisible(input);
      await tester.enterText(input, 'Pending');
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      expect(core.stairEdits, 1);
      expect(core.stairs, hasLength(1));
      expect(painter(tester).derived.construction.single.name, original);
      await tester.pump(const Duration(seconds: 1));
      expect(core.stairEdits, 1);
      expect(tester.widget<TextFormField>(input).controller!.text, original);
    },
  );

  testWidgets('Save includes a live edit still inside its typing delay', (
    tester,
  ) async {
    final directory = Directory(
      Platform.environment['DOMOWIK_TEST_TMPDIR'] ?? Directory.systemTemp.path,
    ).createTempSync('stair-live-save-');
    addTearDown(() => directory.deleteSync(recursive: true));
    final path = '${directory.path}/synthetic.domo';
    await start(tester, dialogs: _StairDialogs(path));
    await addStair(tester);
    final input = find.byKey(const PageStorageKey('stair-name'));
    await tester.ensureVisible(input);
    await tester.enterText(input, 'Saved stairs');
    await tester.tap(find.byKey(const ValueKey('save-action')));
    await tester.pumpAndSettle();
    final saved = core.openPath(path);
    expect(saved.code, 0);
    final snapshot = jsonDecode(core.snapshot(saved.handle).text!) as Map;
    expect(snapshot['staircases'][0]['name'], 'Saved stairs');
    expect(core.close(saved.handle), 0);
    expect(core.stairEdits, 1);
    await tester.pump(const Duration(seconds: 1));
    expect(core.stairEdits, 1);
  });

  testWidgets('deleting stairs cancels pending property work', (tester) async {
    await start(tester);
    await addStair(tester);
    final input = find.byKey(const PageStorageKey('stair-name'));
    await tester.ensureVisible(input);
    await tester.enterText(input, 'Pending');
    final remove = find.text('Remove stairs');
    await tester.ensureVisible(remove);
    await tester.tap(remove);
    await tester.pumpAndSettle();
    expect(core.stairs, isEmpty);
    expect(core.stairEdits, 0);
    await tester.pump(const Duration(seconds: 1));
    expect(core.stairEdits, 0);
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(core.stairs, hasLength(1));
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Stairs draws directly downwards, previews without mutation and edits in Floor',
    (tester) async {
      await start(tester);
      for (final label in ['Walls', 'Stairs', 'Doors', 'Windows']) {
        expect(
          find.descendant(
            of: find.byType(ChoiceChip),
            matching: find.text(label),
          ),
          findsOneWidget,
        );
      }
      expect(find.text('Add Stairs'), findsNothing);
      await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Convert room to staircase'), findsNothing);
      final top = tester.getTopLeft(canvas());
      final before = core.document;
      final drag = await tester.startGesture(
        top + const Offset(180, 300),
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveTo(top + const Offset(480, 300));
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview, hasLength(1));
      expect(core.document, before);
      final preview = painter(tester).constructionPreview.single;
      await drag.up();
      await tester.pumpAndSettle();
      expect(core.stairs, hasLength(1));
      expect(find.text('Make room for your ideas'), findsNothing);
      expect(painter(tester).constructionPreview, isEmpty);
      final item = painter(tester).derived.construction.single;
      expect(item.direction, 'DOWN');
      expect(item.outline, preview.outline);
      expect(item.staircase!['startElevationMm'], 2600.0);
      expect(item.staircase!['endElevationMm'], 0.0);
      final doc = core.document;
      expect(doc['walls'], isEmpty);
      expect(doc['roomRecords'], isEmpty);
      expect(doc['hostedOpenings'], hasLength(1));
      final input = find.byKey(const PageStorageKey('stair-name'));
      await tester.ensureVisible(input);
      await tester.enterText(input, 'Garden stairs');
      await tester.enterText(
        find.byKey(const PageStorageKey('stair-startElevationMm')),
        '2450',
      );
      await tester.enterText(
        find.byKey(const PageStorageKey('stair-endElevationMm')),
        '200',
      );
      await tester.enterText(
        find.byKey(const PageStorageKey('stair-risers')),
        '13',
      );
      expect(find.text('Apply stair changes'), findsNothing);
      await tester.pump(StaircasePropertiesForm.editDelay);
      await tester.pumpAndSettle();
      final changed = painter(tester).derived.construction.single;
      expect(changed.name, 'Garden stairs');
      expect(changed.staircase!['startElevationMm'], 2450.0);
      expect(changed.staircase!['endElevationMm'], 200.0);
      expect(changed.staircase!['risers'], 13);
      await tester.tap(find.byTooltip('Undo'));
      await tester.pumpAndSettle();
      final undone = painter(tester).derived.construction.single;
      expect(undone.name, 'Garden stairs');
      expect(undone.staircase!['startElevationMm'], 2450.0);
      expect(undone.staircase!['endElevationMm'], 200.0);
      expect(undone.staircase!['risers'], 15);
      await tester.tap(find.byTooltip('Redo'));
      await tester.pumpAndSettle();
      expect(painter(tester).derived.construction.single.name, 'Garden stairs');
      expect(
        painter(tester).derived.construction.single.staircase!['risers'],
        13,
      );
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'click-click drawing and Escape cancel work without placeholder rooms',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
      await tester.pumpAndSettle();
      final top = tester.getTopLeft(canvas());
      await tester.tapAt(top + const Offset(150, 300));
      await tester.pumpAndSettle();
      final hover = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await hover.addPointer(location: top + const Offset(150, 300));
      await hover.moveTo(top + const Offset(400, 300));
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview, hasLength(1));
      expect(core.stairs, isEmpty);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(painter(tester).constructionPreview, isEmpty);
      expect(core.stairs, isEmpty);
      await tester.tapAt(top + const Offset(180, 300));
      await tester.pumpAndSettle();
      await tester.tapAt(top + const Offset(450, 300));
      await tester.pumpAndSettle();
      expect(core.stairs, hasLength(1));
      await hover.removePointer();
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets('stair properties survive repeated collapse and expansion', (
    tester,
  ) async {
    await start(tester);
    await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
    await tester.pumpAndSettle();
    final top = tester.getTopLeft(canvas());
    await tester.dragFrom(top + const Offset(180, 300), const Offset(300, 0));
    await tester.pumpAndSettle();
    final before = core.document;
    final card = find.byType(ExpansionTile);
    final name = find.byKey(const PageStorageKey('stair-name'));
    await tester.ensureVisible(name);
    await tester.enterText(name, 'Garden stairs');
    await tester.pump(StaircasePropertiesForm.editDelay);
    await tester.pumpAndSettle();
    final after = core.document;
    expect(after, isNot(before));
    for (var i = 0; i < 3; i++) {
      final title = find
          .descendant(of: card, matching: find.byType(ListTile))
          .first;
      await tester.ensureVisible(title);
      await tester.tap(title);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      await tester.tap(title);
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
      expect(
        tester.widget<TextFormField>(name).controller!.text,
        'Garden stairs',
      );
    }
    expect(core.document, after);
  });
  testWidgets('lowest floor has a disabled Stairs tool', (tester) async {
    await start(tester, upper: false);
    expect(
      tester
          .widget<ChoiceChip>(find.byKey(const ValueKey('add-stairs-tool')))
          .onSelected,
      isNull,
    );
    expect(core.stairs, isEmpty);
  });

  testWidgets('narrow toolbar scrolls between snap controls and Stairs', (
    tester,
  ) async {
    await start(tester);
    await tester.binding.setSurfaceSize(const Size(600, 1000));
    await tester.pumpAndSettle();
    final toolbar = find.byKey(const ValueKey('drawing-toolbar-scroll'));
    final before = core.document;
    await tester.drag(toolbar, const Offset(-1100, 0));
    await tester.pumpAndSettle();
    final snap = find.byKey(const ValueKey('snap-angle'));
    expect(tester.getRect(toolbar).contains(tester.getCenter(snap)), isTrue);
    await tester.tap(snap);
    await tester.pumpAndSettle();
    expect(tester.widget<IconButton>(snap).isSelected, isFalse);
    expect(core.document, before);
    await tester.drag(toolbar, const Offset(1100, 0));
    await tester.pumpAndSettle();
    final stairs = find.byKey(const ValueKey('add-stairs-tool'));
    expect(tester.getRect(toolbar).contains(tester.getCenter(stairs)), isTrue);
    await tester.tap(stairs);
    await tester.pumpAndSettle();
    expect(tester.widget<ChoiceChip>(stairs).selected, isTrue);
    expect(core.stairs, isEmpty);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'switching to the lowest level cancels drawing and disarms stairs',
    (tester) async {
      await start(tester);
      await tester.tap(find.byKey(const ValueKey('add-stairs-tool')));
      await tester.pumpAndSettle();
      final top = tester.getTopLeft(canvas());
      await tester.tapAt(top + const Offset(180, 300));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Level 1'));
      await tester.pumpAndSettle();
      final tool = tester.widget<ChoiceChip>(
        find.byKey(const ValueKey('add-stairs-tool')),
      );
      expect(tool.selected, isFalse);
      expect(tool.onSelected, isNull);
      expect(painter(tester).constructionPreview, isEmpty);
      expect(core.stairs, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );
}
