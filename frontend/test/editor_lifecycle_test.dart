import 'dart:async';
import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/level_state.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

const files = MethodChannel('invalid.domowik-spike/files');

void main() {
  late Bridge bridge;
  late Directory scratch;
  late List<String> dialogs;
  late Future<Object?> Function(MethodCall) dialog;

  setUp(() {
    // Only the native file picker is replaced. Editing, saving, opening and
    // ownership use the very same Rust library as the built application.
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
    scratch = Directory.systemTemp.createTempSync('domowik-lifecycle-');
    dialogs = [];
    dialog = (_) async => null;
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(files, (call) async {
          dialogs.add(call.method);
          return dialog(call);
        });
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(files, null);
    scratch.deleteSync(recursive: true);
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
      expect(
        bridge.openPlans(),
        0,
        reason: 'every replaced or closed handle is released',
      );
      expect(bridge.outstandingBuffers(), 0);
    });
  }

  Finder getCanvas() => find.byWidgetPredicate(
    (widget) => widget is CustomPaint && widget.painter is PlanPainter,
  );

  int walls(WidgetTester tester) =>
      (tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter)
          .walls
          .length;

  IconButton historyButton(WidgetTester tester, String label) =>
      tester.widget<IconButton>(
        find.byWidgetPredicate(
          (widget) => widget is IconButton && widget.tooltip == label,
        ),
      );

  bool dirty() => find.textContaining('• Unsaved').evaluate().isNotEmpty;

  PlanPainter painter(WidgetTester tester) =>
      tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter;

  PlanViewport camera(WidgetTester tester) =>
      (tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter).view;

  Future<void> draw(WidgetTester tester) async {
    await tester.dragFrom(
      tester.getTopLeft(getCanvas()) + const Offset(100, 150),
      const Offset(250, 0),
    );
    await tester.pumpAndSettle();
    expect(walls(tester), 1);
    expect(dirty(), isTrue);
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.tap(finder);
    await tester.pumpAndSettle();
  }

  Future<void> clickAt(
    WidgetTester tester,
    Offset position,
    PointerDeviceKind kind,
  ) async {
    final gesture = await tester.startGesture(position, kind: kind);
    await gesture.up();
    await tester.pumpAndSettle();
  }

  Future<void> zoomAt(WidgetTester tester, Offset position, bool zoomIn) async {
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.sendEventToBinding(
      PointerScrollEvent(
        position: position,
        scrollDelta: Offset(0, zoomIn ? -10000 : 10000),
      ),
    );
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
  }

  // Build fixtures through the real native operation and file APIs. There is
  // no synthetic hit list or substituted selection/edit backend in these tests.
  Future<List<String>> openWalls(
    WidgetTester tester,
    List<(Offset, Offset)> segments,
  ) async {
    final handle = alloc<Uint64>(sizeOf<Uint64>());
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    final path = '${scratch.path}/selection.domo';
    final pathPtr = toNative('$path\u0000');
    final ids = <String>[];
    var opened = 0;
    try {
      expect(bridge.planCreateDefault(handle), ok);
      opened = handle.value;
      expect(bridge.planSnapshot(opened, out), ok);
      final document = jsonDecode(bridge.takeString(out)) as Map;
      final level = (document['levels'] as List).single['id'];
      for (var i = 0; i < segments.length; i++) {
        final id =
            '01950000-0000-7000-8000-${(i + 1).toString().padLeft(12, '0')}';
        ids.add(id);
        final segment = segments[i];
        final request = jsonEncode({
          'operations': [
            {
              'operationId':
                  '01950001-0000-7000-8000-${(i + 1).toString().padLeft(12, '0')}',
              'entityId': id,
              'kind': 'createEntity',
              'entity': {
                'type': 'wall',
                'id': id,
                'levelId': level,
                'startMm': [segment.$1.dx, segment.$1.dy],
                'endMm': [segment.$2.dx, segment.$2.dy],
                'thicknessMm': 20.0,
                'heightMm': 2500.0,
              },
            },
          ],
        });
        final ptr = toNative(request);
        try {
          expect(
            bridge.planExecute(opened, ptr, utf8.encode(request).length, out),
            ok,
          );
          bridge.takeString(out);
        } finally {
          release(ptr);
        }
      }
      expect(bridge.planSave(opened, pathPtr), ok);
    } finally {
      if (opened != 0) expect(bridge.planClose(opened), ok);
      release(pathPtr);
      release(out);
      release(handle);
    }
    dialog = (_) async => path;
    await tap(tester, find.byTooltip('Open plan'));
    expect(walls(tester), segments.length);
    expect(dirty(), isFalse);
    return ids;
  }

  Offset screenPoint(WidgetTester tester, Offset planPoint) =>
      tester.getTopLeft(getCanvas()) + camera(tester).toScreen(planPoint);

  Future<({List<String> levels, List<List<String>> walls})> openLevelScene(
    WidgetTester tester, {
    bool overlappingActiveWalls = false,
  }) async {
    String id(int value) =>
        '01960000-0000-7000-8000-${value.toString().padLeft(12, '0')}';
    final levels = [
      for (final n in [30, 20, 10]) id(n),
    ];
    final wallIds = <List<String>>[];
    final operations = <Map<String, Object?>>[];
    var sequence = 1000;
    void create(Map<String, Object?> entity) => operations.add({
      'operationId': id(sequence++),
      'entityId': entity['id'],
      'kind': 'createEntity',
      'entity': entity,
    });
    final segments = <List<(Offset, Offset)>>[
      [(const Offset(-1000, -1500), const Offset(1000, -1500))],
      [(const Offset(-1000, 1500), const Offset(1000, 1500))],
      overlappingActiveWalls
          ? [
              (const Offset(-100, 0), const Offset(100, 0)),
              (const Offset(0, -1000), const Offset(0, 1000)),
            ]
          : [(const Offset(-1000, 0), const Offset(1000, 0))],
    ];
    for (var i = 0; i < levels.length; i++) {
      // Document order opposes elevation, and tied elevations oppose UUID.
      create({
        'type': 'level',
        'id': levels[i],
        'name': ['Upper', 'Lower B', 'Lower A'][i],
        'elevationMm': [3000.0, -3000.0, -3000.0][i],
        'floorThicknessMm': 200.0,
        'defaultWallHeightMm': 2500.0,
        'defaultCeilingHeightMm': 2500.0,
      });
      final ids = <String>[];
      for (var j = 0; j < segments[i].length; j++) {
        final wall = id(100 + i * 10 + j);
        final segment = segments[i][j];
        ids.add(wall);
        create({
          'type': 'wall',
          'id': wall,
          'levelId': levels[i],
          'startMm': [segment.$1.dx, segment.$1.dy],
          'endMm': [segment.$2.dx, segment.$2.dy],
          'thicknessMm': 200.0,
          'heightMm': 2500.0,
        });
      }
      wallIds.add(ids);
    }
    final handle = alloc<Uint64>(sizeOf<Uint64>());
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    final request = jsonEncode({'operations': operations});
    final ptr = toNative(request);
    final path = '${scratch.path}/levels.domo';
    final pathPtr = toNative('$path\u0000');
    var opened = 0;
    try {
      expect(bridge.planCreate(handle), ok);
      opened = handle.value;
      expect(
        bridge.planExecute(opened, ptr, utf8.encode(request).length, out),
        ok,
      );
      bridge.takeString(out);
      expect(bridge.planSave(opened, pathPtr), ok);
    } finally {
      if (opened != 0) expect(bridge.planClose(opened), ok);
      release(pathPtr);
      release(ptr);
      release(out);
      release(handle);
    }
    dialog = (_) async => path;
    await tap(tester, find.byTooltip('Open plan'));
    expect(painter(tester).walls.map((wall) => wall.id), wallIds[2]);
    expect(dirty(), isFalse);
    return (levels: levels, walls: wallIds);
  }

  String fieldText(WidgetTester tester, String key) =>
      tester.widget<TextField>(find.byKey(ValueKey(key))).controller!.text;

  PopupMenuButton<LevelVisibility> visibilityControl(
    WidgetTester tester,
    String id,
  ) => tester.widget<PopupMenuButton<LevelVisibility>>(
    find.byKey(ValueKey('level-visibility-$id')),
  );

  Future<void> chooseVisibility(
    WidgetTester tester,
    String id,
    LevelVisibility value,
  ) async {
    await tap(tester, find.byKey(ValueKey('level-visibility-$id')));
    await tap(
      tester,
      find.byWidgetPredicate(
        (widget) =>
            widget is PopupMenuItem<LevelVisibility> && widget.value == value,
      ),
    );
  }

  testWidgets(
    'levels reopen sorted, render in tiers, and remain visual-only when inactive',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final [upper, lowerB, lowerA] = scene.levels;
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerB,
        upper,
      ]);
      expect(
        painter(tester).backgroundLevels.every((level) => level.ghosted),
        isTrue,
      );
      expect(visibilityControl(tester, lowerA).enabled, isFalse);
      expect(visibilityControl(tester, lowerB).enabled, isTrue);
      final view = camera(tester);
      final upperDerived = painter(tester).backgroundLevels.last.derived;
      await chooseVisibility(tester, upper, LevelVisibility.visible);
      await chooseVisibility(tester, lowerB, LevelVisibility.visible);
      expect(
        painter(tester).backgroundLevels.every((level) => !level.ghosted),
        isTrue,
      );
      await chooseVisibility(tester, upper, LevelVisibility.hidden);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerB,
      ]);
      await tap(tester, find.byKey(ValueKey('level-$upper')));
      expect(
        painter(tester).derived,
        isNot(same(upperDerived)),
        reason: 'hidden payloads are evicted, not retained indefinitely',
      );
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[0]);
      expect(visibilityControl(tester, upper).enabled, isFalse);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerA,
        lowerB,
      ]);
      expect(painter(tester).backgroundLevels.map((level) => level.ghosted), [
        true,
        false,
      ]);
      expect(camera(tester).zoom, view.zoom);
      expect(camera(tester).origin, view.origin);
      // The lower-B wall is visible and isolated from every active-level hit.
      await clickAt(
        tester,
        screenPoint(tester, const Offset(0, 1500)),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).selectedIds, isEmpty);
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[0]);
      await tap(tester, find.text('Door'));
      await clickAt(
        tester,
        screenPoint(tester, const Offset(0, 1500)),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).derived.openings, isEmpty);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'drawing at a visible inactive endpoint never snaps or joins across levels',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final upper = scene.levels[0];
      await chooseVisibility(tester, upper, LevelVisibility.visible);
      final passive = painter(
        tester,
      ).backgroundLevels.firstWhere((layer) => layer.levelId == upper);
      final oldWall = passive.walls.single;
      final drawStart = screenPoint(tester, const Offset(-500, -800));
      final end = screenPoint(tester, oldWall.end);
      final draw = await tester.startGesture(
        drawStart,
        kind: PointerDeviceKind.mouse,
      );
      await draw.moveTo(end);
      await draw.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls, hasLength(2));
      final after = painter(
        tester,
      ).backgroundLevels.firstWhere((layer) => layer.levelId == upper);
      expect(after.derived, same(passive.derived));
      expect(after.walls.single.start, oldWall.start);
      expect(after.walls.single.end, oldWall.end);
      await clickAt(tester, end, PointerDeviceKind.mouse);
      expect(painter(tester).selectedJoin, isNull);
      expect(painter(tester).selected, isNot(oldWall.id));
      expect(
        painter(tester).walls.map((wall) => wall.id),
        contains(painter(tester).selected),
      );
    },
  );

  testWidgets('activating the current level does not clear selection', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final wall = painter(tester).walls.single;
    await clickAt(
      tester,
      screenPoint(tester, (wall.start + wall.end) / 2),
      PointerDeviceKind.mouse,
    );
    await tap(tester, find.text('Level 1'));
    expect(painter(tester).selectedIds, {wall.id});
    expect(painter(tester).selected, wall.id);
  });

  testWidgets(
    'visibility round trips reset cycling immediately and no-op flags do not',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester, overlappingActiveWalls: true);
      final upper = scene.levels[0];
      await zoomAt(tester, screenPoint(tester, Offset.zero), false);
      final point = screenPoint(tester, Offset.zero);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, scene.walls[2][0]);
      final onVisibility = visibilityControl(tester, upper).onSelected!;
      // Both real application callbacks occur before another frame/click. No
      // pointer excursion or key transition may accidentally prove this reset.
      onVisibility(LevelVisibility.hidden);
      onVisibility(LevelVisibility.ghosted);
      await tester.pumpAndSettle();
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, scene.walls[2][0]);
      onVisibility(LevelVisibility.ghosted); // actual no-op
      await tester.pumpAndSettle();
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, scene.walls[2][1]);
      expect(dirty(), isFalse);
    },
  );

  testWidgets(
    'native geometry cache preserves unrelated floors and invalidates wall and opening edits',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final initial = painter(tester);
      final passive = {
        for (final layer in initial.backgroundLevels)
          layer.levelId: layer.derived,
      };
      await chooseVisibility(tester, scene.levels[0], LevelVisibility.visible);
      for (final layer in painter(tester).backgroundLevels) {
        expect(layer.derived, same(passive[layer.levelId]));
      }
      expect(painter(tester).derived, same(initial.derived));
      final drag = await tester.startGesture(
        screenPoint(tester, Offset.zero),
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(40, 0));
      await drag.up();
      await tester.pumpAndSettle();
      final moved = painter(tester);
      expect(moved.derived, isNot(same(initial.derived)));
      for (final layer in moved.backgroundLevels) {
        expect(
          layer.derived,
          same(passive[layer.levelId]),
          reason: 'an edit on another floor must not rederive this one',
        );
      }
      await tap(tester, find.text('Door'));
      final wall = moved.walls.single;
      await clickAt(
        tester,
        screenPoint(tester, (wall.start + wall.end) / 2),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).derived.openings, hasLength(1));
      expect(painter(tester).derived, isNot(same(moved.derived)));
      for (final layer in painter(tester).backgroundLevels) {
        expect(layer.derived, same(passive[layer.levelId]));
      }
      final withOpening = painter(tester).derived;
      await tap(tester, find.text('Save'));
      expect(painter(tester).derived, same(withOpening));
      expect(dirty(), isFalse);
      historyButton(tester, 'Undo').onPressed!();
      await tester.pumpAndSettle();
      expect(painter(tester).derived.openings, isEmpty);
      expect(painter(tester).derived, isNot(same(withOpening)));
      for (final layer in painter(tester).backgroundLevels) {
        expect(layer.derived, same(passive[layer.levelId]));
      }
    },
  );

  testWidgets(
    'save keeps local level flags but reopen resets all specified view defaults',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final [upper, lowerB, lowerA] = scene.levels;
      await chooseVisibility(tester, upper, LevelVisibility.hidden);
      await tap(tester, find.byKey(ValueKey('level-$lowerB')));
      await tap(tester, find.byTooltip('Hide grid and rulers'));
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: tester.getCenter(getCanvas()),
          scrollDelta: const Offset(60, 40),
        ),
      );
      await tester.pumpAndSettle();
      final view = camera(tester);
      await tap(tester, find.text('Save'));
      expect(painter(tester).showGrid, isFalse);
      expect(camera(tester).origin, view.origin);
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[1]);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerA,
      ]);
      await tap(tester, find.byTooltip('New plan'));
      expect(painter(tester).showGrid, isTrue);
      await tap(tester, find.byTooltip('Open plan'));
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[2]);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerB,
        upper,
      ]);
      expect(
        painter(tester).backgroundLevels.every((level) => level.ghosted),
        isTrue,
      );
      expect(painter(tester).showGrid, isTrue);
      expect(painter(tester).selectedIds, isEmpty);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'undo of an active added level chooses the lowest survivor and keeps other flags',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final [upper, lowerB, lowerA] = scene.levels;
      await chooseVisibility(tester, upper, LevelVisibility.hidden);
      await chooseVisibility(tester, lowerB, LevelVisibility.visible);
      await tap(tester, find.byTooltip('Add a storey above'));
      expect(painter(tester).walls, isEmpty);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerA,
        lowerB,
      ]);
      historyButton(tester, 'Undo').onPressed!();
      await tester.pumpAndSettle();
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[2]);
      expect(painter(tester).backgroundLevels.map((level) => level.levelId), [
        lowerB,
      ]);
      expect(painter(tester).backgroundLevels.single.ghosted, isFalse);
      historyButton(tester, 'Redo').onPressed!();
      await tester.pumpAndSettle();
      expect(painter(tester).walls.map((wall) => wall.id), scene.walls[2]);
      expect(painter(tester).backgroundLevels, hasLength(2));
      expect(painter(tester).backgroundLevels.first.ghosted, isTrue);
      expect(painter(tester).backgroundLevels.last.levelId, lowerB);
    },
  );

  Future<List<String>> parallelWalls(WidgetTester tester) => openWalls(tester, [
    (const Offset(-1000, 0), const Offset(1000, 0)),
    (const Offset(-1000, 1000), const Offset(1000, 1000)),
  ]);

  Future<void> selectBoth(WidgetTester tester) async {
    await clickAt(
      tester,
      screenPoint(tester, Offset.zero),
      PointerDeviceKind.mouse,
    );
    await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
    await clickAt(
      tester,
      screenPoint(tester, const Offset(0, 1000)),
      PointerDeviceKind.mouse,
    );
    await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
    await tester.pumpAndSettle();
  }

  testWidgets('native features project to one entity per click-cycle stop', (
    tester,
  ) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-100, 0), const Offset(100, 0)),
      (const Offset(0, -1000), const Offset(0, 1000)),
    ]);
    await zoomAt(tester, screenPoint(tester, Offset.zero), false);
    final point = screenPoint(tester, Offset.zero);
    for (final id in [ids[0], ids[1], ids[0], ids[1]]) {
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, id);
      expect(painter(tester).selectedIds, {id});
    }
    expect(dirty(), isFalse);
    expect(walls(tester), 2);
  });

  testWidgets('modifier selection deletes only primary and undo invents none', (
    tester,
  ) async {
    await start(tester);
    final ids = await parallelWalls(tester);
    await selectBoth(tester);
    expect(painter(tester).selectedIds, ids.toSet());
    expect(painter(tester).selected, ids[1]);
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    expect(painter(tester).walls.map((w) => w.id), [ids[0]]);
    expect(painter(tester).selectedIds, {ids[0]});
    expect(painter(tester).selected, isNull);
    expect(find.text('No primary selected'), findsOneWidget);
    historyButton(tester, 'Undo').onPressed!();
    await tester.pumpAndSettle();
    expect(walls(tester), 2);
    expect(painter(tester).selectedIds, {ids[0]});
    expect(painter(tester).selected, isNull);
    expect(dirty(), isTrue, reason: 'undo appends a new unsaved batch');
  });

  testWidgets('modifier removal leaves survivors without choosing a primary', (
    tester,
  ) async {
    await start(tester);
    final ids = await parallelWalls(tester);
    await selectBoth(tester);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await clickAt(
      tester,
      screenPoint(tester, const Offset(0, 1000)),
      PointerDeviceKind.mouse,
    );
    await tester.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    expect(painter(tester).selectedIds, {ids[0]});
    expect(painter(tester).selected, isNull);
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    expect(walls(tester), 2);
    expect(dirty(), isFalse);
  });

  testWidgets(
    'primary drag preserves other selections and cancellation restores pre-press',
    (tester) async {
      await start(tester);
      final ids = await parallelWalls(tester);
      await selectBoth(tester);
      final before = {for (final wall in painter(tester).walls) wall.id: wall};
      final drag = await tester.startGesture(
        screenPoint(tester, const Offset(0, 1000)),
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(40, 30));
      await drag.up();
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, ids.toSet());
      expect(painter(tester).selected, ids[1]);
      final after = {for (final wall in painter(tester).walls) wall.id: wall};
      expect(after[ids[0]]!.start, before[ids[0]]!.start);
      expect(after[ids[0]]!.end, before[ids[0]]!.end);
      expect(after[ids[1]]!.start, isNot(before[ids[1]]!.start));
      final cancel = await tester.startGesture(
        screenPoint(tester, Offset.zero),
        kind: PointerDeviceKind.mouse,
      );
      await cancel.moveBy(const Offset(40, 30));
      await tester.pump();
      expect(painter(tester).selectedIds, {ids[0]});
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await cancel.up();
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, ids.toSet());
      expect(painter(tester).selected, ids[1]);
      expect(
        painter(tester).walls.firstWhere((w) => w.id == ids[0]).start,
        before[ids[0]]!.start,
      );
    },
  );

  testWidgets('modifier drags over non-primary and empty space emit no edits', (
    tester,
  ) async {
    await start(tester);
    final ids = await parallelWalls(tester);
    await selectBoth(tester);
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    for (final point in [
      screenPoint(tester, Offset.zero),
      tester.getTopLeft(getCanvas()) + const Offset(20, 20),
    ]) {
      final drag = await tester.startGesture(
        point,
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(45, 25));
      await drag.up();
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, ids.toSet());
      expect(painter(tester).selected, ids[1]);
      expect(dirty(), isFalse);
      expect(walls(tester), 2);
    }
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
  });

  testWidgets(
    'observed pointer, view and modifier round trips reset click cycling',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-100, 0), const Offset(100, 0)),
        (const Offset(0, -1000), const Offset(0, 1000)),
      ]);
      await zoomAt(tester, screenPoint(tester, Offset.zero), false);
      final point = screenPoint(tester, Offset.zero);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[1]);
      // Move outside the canvas entirely, then return before the next click.
      await tester.sendEventToBinding(
        const PointerHoverEvent(position: Offset(1, 1)),
      );
      await tester.sendEventToBinding(PointerHoverEvent(position: point));
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      final view = camera(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.pumpAndSettle();
      expect(camera(tester).zoom, view.zoom);
      expect(camera(tester).origin, view.origin);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[1]);
      expect(dirty(), isFalse);
    },
  );

  testWidgets(
    'save and refusal preserve cycling but a distant accepted edit resets it',
    (tester) async {
      await start(tester);
      const centre = Offset(1000000, 1000000);
      final ids = await openWalls(tester, [
        (centre + const Offset(-100, 0), centre + const Offset(100, 0)),
        (centre + const Offset(0, -1000), centre + const Offset(0, 1000)),
      ]);
      final controlsScroll = find.descendant(
        of: find.byType(ListView),
        matching: find.byType(Scrollable),
      );
      await tester.scrollUntilVisible(
        find.text('Developer diagnostics'),
        200,
        scrollable: controlsScroll,
      );
      await tap(tester, find.text('Developer diagnostics'));
      await tester.scrollUntilVisible(
        find.text('Add demo rectangle wall'),
        150,
        scrollable: controlsScroll,
      );
      await zoomAt(tester, screenPoint(tester, centre), false);
      final point = screenPoint(tester, centre);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      // Invoke actual application handlers without introducing an unrelated
      // pointer excursion or modifier transition to reach their toolbar buttons.
      tester
          .widget<FilledButton>(find.widgetWithText(FilledButton, 'Save'))
          .onPressed!();
      await tester.pumpAndSettle();
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(
        painter(tester).selected,
        ids[1],
        reason: 'save is not an accepted batch',
      );
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Send invalid wall'),
          )
          .onPressed!();
      await tester.pumpAndSettle();
      expect(dirty(), isFalse);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(
        painter(tester).selected,
        ids[1],
        reason: 'refused batch leaves revision and cycle intact',
      );
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[0]);
      tester
          .widget<OutlinedButton>(
            find.widgetWithText(OutlinedButton, 'Add demo rectangle wall'),
          )
          .onPressed!();
      await tester.pumpAndSettle();
      expect(walls(tester), 3);
      expect(dirty(), isTrue);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(
        painter(tester).selected,
        ids[0],
        reason: 'distant accepted batch resets even with identical local hits',
      );
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, ids[1]);
      historyButton(tester, 'Undo').onPressed!();
      await tester.pumpAndSettle();
      expect(walls(tester), 2);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(
        painter(tester).selected,
        ids[0],
        reason: 'undo appends a compensating accepted batch',
      );
    },
  );

  testWidgets(
    'joined corner and its member walls occupy separate cycle stops',
    (tester) async {
      await start(tester);
      await draw(tester);
      final origin = tester.getTopLeft(getCanvas());
      await tester.dragFrom(
        origin + const Offset(350, 300),
        const Offset(0, -150),
      );
      await tester.pumpAndSettle();
      final wallIds = painter(tester).walls.map((w) => w.id).toSet();
      final point = origin + const Offset(350, 150);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      final join = painter(tester).selected;
      expect(join, isNotNull);
      expect(wallIds, isNot(contains(join)));
      final selectedWalls = <String>{};
      for (var i = 0; i < 2; i++) {
        await clickAt(tester, point, PointerDeviceKind.mouse);
        selectedWalls.add(painter(tester).selected!);
      }
      expect(selectedWalls, wallIds);
      await clickAt(tester, point, PointerDeviceKind.mouse);
      expect(painter(tester).selected, join);
      expect(painter(tester).selectedJoin, isNotNull);
    },
  );

  testWidgets('classified drag returning to its start is not a cycle click', (
    tester,
  ) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-1000, 0), const Offset(1000, 0)),
      (const Offset(0, -1000), const Offset(0, 1000)),
    ]);
    final point = screenPoint(tester, Offset.zero);
    await clickAt(tester, point, PointerDeviceKind.mouse);
    final primary = painter(tester).selected;
    final drag = await tester.startGesture(
      point,
      kind: PointerDeviceKind.mouse,
    );
    await drag.moveBy(
      const Offset(8, 0),
    ); // >5px classifies; <12px preserves anchor.
    await drag.moveTo(point);
    await drag.up();
    await tester.pumpAndSettle();
    expect(painter(tester).selected, primary);
    expect(dirty(), isFalse);
    await clickAt(tester, point, PointerDeviceKind.mouse);
    expect(painter(tester).selected, ids.singleWhere((id) => id != primary));
  });

  testWidgets(
    'file replacement and level changes clear transient multi-selection',
    (tester) async {
      await start(tester);
      final ids = await parallelWalls(tester);
      await selectBoth(tester);
      expect(painter(tester).selectedIds, ids.toSet());
      await tap(tester, find.text('Save'));
      expect(painter(tester).selectedIds, ids.toSet());
      await tap(tester, find.byTooltip('New plan'));
      expect(painter(tester).selectedIds, isEmpty);
      expect(painter(tester).selected, isNull);
      await tap(tester, find.byTooltip('Open plan'));
      expect(walls(tester), 2);
      expect(painter(tester).selectedIds, isEmpty);
      await selectBoth(tester);
      await tap(tester, find.byTooltip('Add a storey above'));
      expect(walls(tester), 0);
      expect(painter(tester).selectedIds, isEmpty);
      expect(painter(tester).selected, isNull);
      await tap(tester, find.text('Level 1'));
      expect(walls(tester), 2);
      expect(painter(tester).selectedIds, isEmpty);
    },
  );

  testWidgets('native target reach is screen-space at both zoom clamps', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    dialog = (_) async => '${scratch.path}/targeting.domo';
    await tap(tester, find.text('Save'));
    final original = painter(tester).walls.single;
    final edge =
        (original.start + original.end) / 2 +
        Offset(0, original.thicknessMm / 2);
    for (final zoomIn in [false, true]) {
      var edgeScreen =
          tester.getTopLeft(getCanvas()) + camera(tester).toScreen(edge);
      await zoomAt(tester, edgeScreen, zoomIn);
      expect(camera(tester).zoom, zoomIn ? 100 : 0.0001);
      edgeScreen =
          tester.getTopLeft(getCanvas()) + camera(tester).toScreen(edge);
      for (final kind in [
        PointerDeviceKind.mouse,
        PointerDeviceKind.stylus,
        PointerDeviceKind.touch,
      ]) {
        final reach = kind == PointerDeviceKind.touch ? 24.0 : 12.0;
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
        await tester.pumpAndSettle();
        final inside = edgeScreen - Offset(0, reach - 0.25);
        final press = await tester.startGesture(inside, kind: kind);
        await tester.pump();
        expect(
          painter(tester).selected,
          original.id,
          reason: 'press at zoom ${camera(tester).zoom}, $kind',
        );
        await press.up();
        await tester.pumpAndSettle();
        expect(
          painter(tester).selected,
          original.id,
          reason: 'release must agree with the same press',
        );
        await clickAt(tester, edgeScreen - Offset(0, reach + 0.25), kind);
        expect(painter(tester).selected, isNull);
        expect(walls(tester), 1);
        expect(painter(tester).walls.single.start, original.start);
        expect(painter(tester).walls.single.end, original.end);
        expect(dirty(), isFalse);
      }
    }
  });

  testWidgets('inclusive hit boundary uses butt caps and modality reach', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final wall = painter(tester).walls.single;
    final origin = tester.getTopLeft(getCanvas());
    final edge = (wall.start + wall.end) / 2 + Offset(0, wall.thicknessMm / 2);
    for (final kind in [PointerDeviceKind.mouse, PointerDeviceKind.touch]) {
      final reach = kind == PointerDeviceKind.touch ? 24.0 : 12.0;
      await clickAt(
        tester,
        origin + camera(tester).toScreen(edge) - Offset(0, reach),
        kind,
      );
      expect(
        painter(tester).selected,
        wall.id,
        reason: 'the outer boundary of the tolerance is included',
      );
    }
    // Beyond the butt cap: adding half thickness to a centre-line capsule
    // used to select this even though it is outside every native shape.
    await clickAt(
      tester,
      origin + camera(tester).toScreen(wall.start) - const Offset(18, 0),
      PointerDeviceKind.mouse,
    );
    expect(painter(tester).selected, isNull);
    await clickAt(
      tester,
      origin + camera(tester).toScreen(wall.start) - const Offset(18, 0),
      PointerDeviceKind.touch,
    );
    expect(painter(tester).selected, wall.id);
    expect(walls(tester), 1);
  });

  Future<bool> nativeClose(WidgetTester tester) {
    final result = Completer<bool>();
    tester.binding.channelBuffers.push(
      files.name,
      const StandardMethodCodec().encodeMethodCall(
        const MethodCall('requestClose'),
      ),
      (reply) => result.complete(
        const StandardMethodCodec().decodeEnvelope(reply!) as bool,
      ),
    );
    return result.future;
  }

  testWidgets('opening placement uses the same modality radius as selection', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    dialog = (_) async => '${scratch.path}/opening-target.domo';
    await tap(tester, find.text('Save'));
    final wall = painter(tester).walls.single;
    final edge = (wall.start + wall.end) / 2 + Offset(0, wall.thicknessMm / 2);
    final position =
        tester.getTopLeft(getCanvas()) +
        camera(tester).toScreen(edge) -
        const Offset(0, 18);
    await tap(tester, find.text('Door'));
    await clickAt(tester, position, PointerDeviceKind.mouse);
    expect(painter(tester).derived.openings, isEmpty);
    expect(dirty(), isFalse);
    expect(
      find.textContaining('click on a wall to place a door'),
      findsOneWidget,
    );
    await clickAt(tester, position, PointerDeviceKind.touch);
    expect(painter(tester).derived.openings, hasLength(1));
    expect(dirty(), isTrue);
    expect(painter(tester).walls.single.start, wall.start);
    expect(painter(tester).walls.single.end, wall.end);
  });

  testWidgets('joined corner keeps its identity through movement and undo', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final origin = tester.getTopLeft(getCanvas());
    await tester.dragFrom(
      origin + const Offset(350, 300),
      const Offset(0, -150),
    );
    await tester.pumpAndSettle();
    expect(walls(tester), 2);
    final originalWalls = painter(tester).walls.toList();
    final position = origin + const Offset(350, 150);
    await clickAt(tester, position, PointerDeviceKind.mouse);
    final join = painter(tester).selected;
    expect(join, isNotNull);
    expect(originalWalls.map((wall) => wall.id), isNot(contains(join)));
    expect(painter(tester).selectedJoin, originalWalls.first.end);
    expect(find.text('Joined corner'), findsOneWidget);
    expect(find.text('Delete wall'), findsNothing);
    // Delete must never reinterpret a join UUID as a member wall command.
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    expect(walls(tester), 2);
    expect(painter(tester).selected, join);
    var gesture = await tester.startGesture(
      position,
      kind: PointerDeviceKind.mouse,
    );
    await gesture.moveBy(const Offset(30, -30));
    await tester.pump();
    final preview = painter(tester).editPreview;
    expect(preview, hasLength(2));
    expect(preview![0].$2, preview[1].$2);
    expect(painter(tester).walls.first.end, originalWalls.first.end);
    await gesture.cancel();
    await tester.pumpAndSettle();
    expect(painter(tester).editPreview, isNull);
    expect(painter(tester).selected, join);
    expect(painter(tester).walls.first.end, originalWalls.first.end);
    gesture = await tester.startGesture(
      position,
      kind: PointerDeviceKind.mouse,
    );
    await gesture.moveBy(const Offset(30, -30));
    await gesture.up();
    await tester.pumpAndSettle();
    expect(painter(tester).selected, join);
    final moved = painter(tester).walls;
    expect(moved[0].end, moved[1].end);
    expect(moved[0].end, isNot(originalWalls[0].end));
    expect(moved[0].start, originalWalls[0].start);
    expect(moved[1].start, originalWalls[1].start);
    await tap(tester, find.byTooltip('Undo'));
    expect(painter(tester).selected, join);
    expect(painter(tester).walls[0].end, originalWalls[0].end);
    expect(painter(tester).walls[1].end, originalWalls[1].end);
  });

  testWidgets('opening-tool drags never reshape walls or create stray walls', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    dialog = (_) async => '${scratch.path}/opening-drag.domo';
    await tap(tester, find.text('Save'));
    final wall = painter(tester).walls.single;
    await tap(tester, find.text('Door'));
    final origin = tester.getTopLeft(getCanvas());
    for (final from in [
      camera(tester).toScreen(wall.start),
      const Offset(100, 350),
    ]) {
      await tester.dragFrom(origin + from, const Offset(50, 40));
      await tester.pumpAndSettle();
      expect(walls(tester), 1);
      expect(painter(tester).derived.openings, isEmpty);
      expect(painter(tester).walls.single.start, wall.start);
      expect(painter(tester).walls.single.end, wall.end);
      expect(dirty(), isFalse);
    }
  });

  for (final kind in ['Window', 'Door'])
    testWidgets('$kind click selects the opening before its host wall', (
      tester,
    ) async {
      await start(tester);
      await draw(tester);
      final top = tester.getTopLeft(getCanvas());
      final at = top + const Offset(225, 150);
      final wall = painter(tester).walls.single;
      await tap(tester, find.text(kind));
      await clickAt(tester, at, PointerDeviceKind.mouse);
      final opening = painter(tester).derived.openings.single;
      expect(opening.isDoor, kind == 'Door');
      await clickAt(tester, at, PointerDeviceKind.mouse);
      expect(painter(tester).selected, opening.id);
      expect(find.text(kind), findsWidgets);
      expect(painter(tester).walls.single.start, wall.start);
      final drag = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await drag.moveBy(const Offset(30, -30));
      await drag.cancel();
      await tester.pumpAndSettle();
      expect(painter(tester).walls.single.start, wall.start);
      expect(painter(tester).walls.single.end, wall.end);
      await tester.sendKeyEvent(LogicalKeyboardKey.delete);
      await tester.pumpAndSettle();
      expect(painter(tester).derived.openings, isEmpty);
      expect(painter(tester).walls.single.id, wall.id);
      expect(painter(tester).selected, isNull);
      await tap(tester, find.byTooltip('Undo'));
      expect(painter(tester).derived.openings.single.id, opening.id);
      await clickAt(tester, at, PointerDeviceKind.mouse);
      await clickAt(tester, at, PointerDeviceKind.mouse);
      expect(painter(tester).selected, wall.id);
    });

  testWidgets('Fit plan includes the fully opened door leaf', (tester) async {
    await start(tester);
    await draw(tester);
    final top = tester.getTopLeft(getCanvas());
    await tap(tester, find.text('Door'));
    await clickAt(
      tester,
      top + const Offset(225, 150),
      PointerDeviceKind.mouse,
    );
    final door = painter(tester).derived.openings.single;
    final bounds = PlanPainter.boundsOf(
      painter(tester).walls,
      painter(tester).derived,
    )!;
    expect(door.tip.dx, inInclusiveRange(bounds.left, bounds.right));
    expect(door.tip.dy, inInclusiveRange(bounds.top, bounds.bottom));
    await tap(tester, find.byTooltip('Fit plan'));
    final view = camera(tester);
    expect(
      view.visibleBounds(tester.getSize(getCanvas())).contains(door.tip),
      isTrue,
    );
  });

  for (final (menu, choice, expected, isDoor) in [
    ('door-type-menu', 'Sliding door', 'doorSliding', true),
    ('window-type-menu', 'Double-swing window', 'windowDoubleSwing', false),
    ('window-type-menu', 'Lift window', 'windowLift', false),
    ('window-type-menu', 'Side-hung window', 'windowSideHung', false),
  ])
    testWidgets('choosing $choice creates the correct type and glyph', (
      tester,
    ) async {
      await start(tester);
      await draw(tester);
      await tap(tester, find.byKey(ValueKey(menu)));
      await tap(tester, find.text(choice).last);
      final top = tester.getTopLeft(getCanvas());
      await clickAt(
        tester,
        top + const Offset(225, 150),
        PointerDeviceKind.mouse,
      );
      final opening = painter(tester).derived.openings.single;
      expect(opening.isDoor, isDoor);
      expect(opening.mechanism, expected);
      expect(opening.glyphSegments, isNotEmpty);
      await tap(tester, find.byTooltip('Undo'));
      expect(painter(tester).derived.openings, isEmpty);
      await tap(tester, find.byTooltip('Redo'));
      expect(painter(tester).derived.openings.single.mechanism, expected);
      if (expected == 'doorSliding') {
        final path = '${scratch.path}/sliding-door.domo';
        dialog = (_) async => path;
        await tap(tester, find.text('Save'));
        await tap(tester, find.byTooltip('Open plan'));
        expect(painter(tester).derived.openings.single.mechanism, expected);
      }
    });

  testWidgets('a hinged door changes hinge end and swing side independently', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final top = tester.getTopLeft(getCanvas());
    await tap(tester, find.text('Door'));
    await clickAt(
      tester,
      top + const Offset(225, 150),
      PointerDeviceKind.mouse,
    );
    await clickAt(
      tester,
      top + const Offset(225, 150),
      PointerDeviceKind.mouse,
    );
    final original = painter(tester).derived.openings.single;
    final scroll = find.descendant(
      of: find.byType(ListView),
      matching: find.byType(Scrollable),
    );
    await tester.scrollUntilVisible(
      find.byKey(const ValueKey('opening-hinge')),
      160,
      scrollable: scroll,
    );
    await tap(tester, find.byKey(const ValueKey('opening-hinge')));
    await tap(tester, find.text('At wall end').last);
    expect(painter(tester).derived.openings.single.hingeEnd, 'nearEnd');
    expect(
      painter(tester).derived.openings.single.opensToward,
      original.opensToward,
    );
    await tester.scrollUntilVisible(
      find.byKey(const ValueKey('opening-swing-side')),
      160,
      scrollable: scroll,
    );
    await tap(tester, find.byKey(const ValueKey('opening-swing-side')));
    await tap(tester, find.text('Right side').last);
    expect(painter(tester).derived.openings.single.opensToward, 'Right');
    expect(painter(tester).derived.openings.single.hingeEnd, 'nearEnd');
    await tap(tester, find.byTooltip('Undo'));
    expect(
      painter(tester).derived.openings.single.opensToward,
      original.opensToward,
    );
    expect(painter(tester).derived.openings.single.hingeEnd, 'nearEnd');
  });

  for (final kind in ['Window', 'Door'])
    testWidgets(
      '$kind drag moves along host; inspector edits dimensions atomically',
      (tester) async {
        await start(tester);
        await draw(tester);
        final at = tester.getTopLeft(getCanvas()) + const Offset(225, 150);
        await tap(tester, find.text(kind));
        await clickAt(tester, at, PointerDeviceKind.mouse);
        await clickAt(tester, at, PointerDeviceKind.mouse);
        final opening = painter(tester).derived.openings.single;
        final wall = painter(tester).walls.single;
        final drag = await tester.startGesture(
          at,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveBy(const Offset(30, 40));
        await tester.pump();
        expect(painter(tester).derived.openings.single.u, opening.u);
        expect(painter(tester).openingPreview?.$1, opening.id);
        await drag.up();
        await tester.pumpAndSettle();
        final moved = painter(tester).derived.openings.single;
        expect(moved.id, opening.id);
        expect(moved.u, closeTo(opening.u! + 0.12, 1e-8));
        expect(painter(tester).openingPreview, isNull);
        expect(painter(tester).walls.single.start, wall.start);
        await tap(tester, find.byTooltip('Undo'));
        expect(painter(tester).derived.openings.single.u, opening.u);
        await tap(tester, find.byTooltip('Redo'));
        expect(painter(tester).derived.openings.single.u, moved.u);

        final edit = find.byKey(const ValueKey('edit-opening-size'));
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
        await tester.enterText(
          find.byKey(const ValueKey('opening-widthMm-field')),
          '800',
        );
        await tester.enterText(
          find.byKey(const ValueKey('opening-heightMm-field')),
          '1500',
        );
        await tap(
          tester,
          find.byKey(const ValueKey('opening-properties-apply')),
        );
        expect(painter(tester).derived.openings.single.widthMm, 800);
        expect(painter(tester).derived.openings.single.heightMm, 1500);
        await tap(tester, find.byTooltip('Undo'));
        expect(
          painter(tester).derived.openings.single.widthMm,
          opening.widthMm,
        );
        expect(
          painter(tester).derived.openings.single.heightMm,
          opening.heightMm,
        );
        await tap(tester, find.byTooltip('Redo'));
        expect(painter(tester).derived.openings.single.widthMm, 800);
        expect(painter(tester).derived.openings.single.heightMm, 1500);
      },
    );

  testWidgets(
    'opening size refusal and cancelled drag preserve the last accepted state',
    (tester) async {
      await start(tester);
      await draw(tester);
      final at = tester.getTopLeft(getCanvas()) + const Offset(225, 150);
      await tap(tester, find.text('Window'));
      await clickAt(tester, at, PointerDeviceKind.mouse);
      await clickAt(tester, at, PointerDeviceKind.mouse);
      final opening = painter(tester).derived.openings.single;
      final drag = await tester.startGesture(at, kind: PointerDeviceKind.mouse);
      await drag.moveBy(const Offset(30, 0));
      await tester.pump();
      expect(painter(tester).openingPreview, isNotNull);
      await drag.cancel();
      await tester.pumpAndSettle();
      expect(painter(tester).openingPreview, isNull);
      expect(painter(tester).derived.openings.single.u, opening.u);
      final perpendicular = await tester.startGesture(
        at,
        kind: PointerDeviceKind.mouse,
      );
      await perpendicular.moveBy(const Offset(0, 30));
      await tester.pump();
      expect(painter(tester).openingPreview, isNull);
      await perpendicular.up();
      await tester.pumpAndSettle();
      expect(painter(tester).derived.openings.single.u, opening.u);

      final edit = find.byKey(const ValueKey('edit-opening-size'));
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
      await tap(
        tester,
        find.byKey(const ValueKey('opening-properties-cancel')),
      );
      await tap(tester, edit);
      await tap(tester, find.byKey(const ValueKey('opening-properties-apply')));
      await tap(tester, edit);
      await tester.enterText(
        find.byKey(const ValueKey('opening-widthMm-field')),
        '4000',
      );
      await tap(tester, find.byKey(const ValueKey('opening-properties-apply')));
      expect(painter(tester).derived.openings.single.widthMm, opening.widthMm);
      expect(painter(tester).selected, opening.id);
      await tap(tester, find.byTooltip('Undo'));
      expect(
        painter(tester).derived.openings,
        isEmpty,
        reason: 'refused size and cancelled drag add no undo entry',
      );
    },
  );

  for (final kind in ['Window', 'Door'])
    testWidgets(
      'dragging a $kind to another wall is one undoable replacement',
      (tester) async {
        await start(tester);
        await draw(tester);
        final top = tester.getTopLeft(getCanvas());
        await tester.dragFrom(
          top + const Offset(100, 350),
          const Offset(250, 0),
        );
        await tester.pumpAndSettle();
        final initialWalls = painter(tester).walls.toList();
        await tap(tester, find.text(kind));
        final from = top + const Offset(225, 150);
        await clickAt(tester, from, PointerDeviceKind.mouse);
        await clickAt(tester, from, PointerDeviceKind.mouse);
        final original = painter(tester).derived.openings.single;
        final drag = await tester.startGesture(
          from,
          kind: PointerDeviceKind.mouse,
        );
        await drag.moveTo(top + const Offset(225, 350));
        await tester.pump();
        expect(painter(tester).openingPreview, isNotNull);
        await drag.up();
        await tester.pumpAndSettle();
        final moved = painter(tester).derived.openings.single;
        expect(moved.hostWallId, initialWalls.last.id);
        expect(moved.id, isNot(original.id));
        expect(moved.widthMm, original.widthMm);
        expect(painter(tester).selected, moved.id);
        await tap(tester, find.byTooltip('Undo'));
        expect(painter(tester).derived.openings.single.id, original.id);
        expect(
          painter(tester).derived.openings.single.hostWallId,
          original.hostWallId,
        );
        await tap(tester, find.byTooltip('Redo'));
        expect(painter(tester).derived.openings.single.id, moved.id);
      },
    );

  testWidgets('a refused cross-wall move retains the source and undo history', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final top = tester.getTopLeft(getCanvas());
    await tester.dragFrom(top + const Offset(200, 350), const Offset(50, 0));
    await tester.pumpAndSettle();
    await tap(tester, find.text('Window'));
    final from = top + const Offset(225, 150);
    await clickAt(tester, from, PointerDeviceKind.mouse);
    await clickAt(tester, from, PointerDeviceKind.mouse);
    final opening = painter(tester).derived.openings.single;
    final drag = await tester.startGesture(from, kind: PointerDeviceKind.mouse);
    await drag.moveTo(top + const Offset(225, 350));
    await drag.up();
    await tester.pumpAndSettle();
    expect(painter(tester).derived.openings.single.id, opening.id);
    expect(
      painter(tester).derived.openings.single.hostWallId,
      opening.hostWallId,
    );
    expect(painter(tester).selected, opening.id);
    await tap(tester, find.byTooltip('Undo'));
    expect(painter(tester).derived.openings, isEmpty);
  });

  testWidgets('dragging a window end resizes with its other end fixed', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final top = tester.getTopLeft(getCanvas());
    await tap(tester, find.text('Window'));
    await clickAt(
      tester,
      top + const Offset(225, 150),
      PointerDeviceKind.mouse,
    );
    final before = painter(tester).derived.openings.single;
    final drag = await tester.startGesture(
      top + const Offset(285, 150),
      kind: PointerDeviceKind.mouse,
    );
    await drag.moveBy(const Offset(20, 0));
    await tester.pump();
    expect(painter(tester).openingPreview, isNotNull);
    expect(painter(tester).derived.openings.single.widthMm, before.widthMm);
    await drag.up();
    await tester.pumpAndSettle();
    final after = painter(tester).derived.openings.single;
    expect(after.widthMm, closeTo(before.widthMm! + 200, 1e-6));
    expect(after.u, closeTo(before.u! + 0.04, 1e-8));
    await tap(tester, find.byTooltip('Undo'));
    expect(painter(tester).derived.openings.single.widthMm, before.widthMm);
    final otherEnd = await tester.startGesture(
      top + const Offset(165, 150),
      kind: PointerDeviceKind.mouse,
    );
    await otherEnd.moveBy(const Offset(-10, 0));
    await otherEnd.up();
    await tester.pumpAndSettle();
    final other = painter(tester).derived.openings.single;
    expect(other.widthMm, closeTo(before.widthMm! + 100, 1e-6));
    expect(other.u, closeTo(before.u! - 0.02, 1e-8));
  });

  testWidgets('hovering an opening end shows the resize cursor', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final top = tester.getTopLeft(getCanvas());
    await tap(tester, find.text('Window'));
    await clickAt(
      tester,
      top + const Offset(225, 150),
      PointerDeviceKind.mouse,
    );
    final mouse = await tester.createGesture(
      kind: PointerDeviceKind.mouse,
      pointer: 777,
    );
    await mouse.moveTo(top + const Offset(285, 150));
    await tester.pumpAndSettle();
    MouseCursor cursor() => tester
        .widget<MouseRegion>(find.byKey(const ValueKey('plan-cursor-region')))
        .cursor;
    expect(cursor(), SystemMouseCursors.resizeLeftRight);
    await mouse.moveTo(top + const Offset(225, 150));
    await tester.pumpAndSettle();
    expect(cursor(), SystemMouseCursors.basic);
    await mouse.removePointer();
  });

  testWidgets(
    'dragging a selected door leaf across the wall reverses its swing',
    (tester) async {
      await start(tester);
      await draw(tester);
      final top = tester.getTopLeft(getCanvas());
      await tap(tester, find.text('Door'));
      await clickAt(
        tester,
        top + const Offset(225, 150),
        PointerDeviceKind.mouse,
      );
      await clickAt(
        tester,
        top + const Offset(225, 150),
        PointerDeviceKind.mouse,
      );
      final before = painter(tester).derived.openings.single;
      final hinge = top + painter(tester).toScreen(before.hinge);
      final tip = top + painter(tester).toScreen(before.tip);
      final drag = await tester.startGesture(
        tip,
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveTo(Offset(tip.dx, 2 * hinge.dy - tip.dy));
      await tester.pump();
      expect(painter(tester).doorSwingPreview?.$1, before.id);
      await drag.up();
      await tester.pumpAndSettle();
      final after = painter(tester).derived.openings.single;
      expect(after.opensToward, isNot(before.opensToward));
      expect(after.id, before.id);
      await tap(tester, find.byTooltip('Undo'));
      expect(
        painter(tester).derived.openings.single.opensToward,
        before.opensToward,
      );
    },
  );

  testWidgets('switching levels cancels a grabbed wall before release', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final original = painter(tester).walls.single;
    await tap(tester, find.byTooltip('Add a storey above'));
    await tap(tester, find.text('Level 1'));
    dialog = (_) async => '${scratch.path}/level-target.domo';
    await tap(tester, find.text('Save'));
    final position =
        tester.getTopLeft(getCanvas()) +
        camera(tester).toScreen(original.start);
    final gesture = await tester.startGesture(
      position,
      kind: PointerDeviceKind.mouse,
    );
    await gesture.moveBy(const Offset(30, -30));
    await tester.pump();
    // Drive the actual level control while the first pointer is captured.
    final level = tester.widget<ListTile>(
      find.widgetWithText(ListTile, 'Level 2'),
    );
    level.onTap!();
    await tester.pumpAndSettle();
    await gesture.up();
    await tester.pumpAndSettle();
    expect(walls(tester), 0);
    expect(painter(tester).selected, isNull);
    expect(dirty(), isFalse);
    await tap(tester, find.text('Level 1'));
    expect(painter(tester).walls.single.start, original.start);
    expect(painter(tester).walls.single.end, original.end);
    expect(painter(tester).selected, isNull);
    expect(dirty(), isFalse);
  });

  testWidgets(
    'explicit Select / draw leaves the Walls tool in selection-only mode',
    (tester) async {
      await start(tester);
      await tap(tester, find.byKey(const ValueKey('wall-chain-tool')));
      final top = tester.getTopLeft(getCanvas());
      await tester.dragFrom(top + const Offset(100, 250), const Offset(250, 0));
      await tester.pumpAndSettle();
      final wall = painter(tester).walls.single;
      await tap(tester, find.widgetWithText(ChoiceChip, 'Select / draw'));
      expect(find.widgetWithText(ChoiceChip, 'Select'), findsOneWidget);
      await tester.dragFrom(top + const Offset(100, 450), const Offset(80, 0));
      await tester.pumpAndSettle();
      expect(painter(tester).walls.single.id, wall.id);
      await clickAt(
        tester,
        top + const Offset(225, 250),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).selected, wall.id);
    },
  );

  testWidgets(
    'New cancel preserves the real document; discard adopts a clean replacement',
    (tester) async {
      await start(tester);
      expect(dirty(), isFalse);
      await draw(tester);
      await tap(tester, find.byTooltip('New plan'));
      await tap(tester, find.text('Cancel'));
      expect(walls(tester), 1);
      expect(dirty(), isTrue);
      await tap(tester, find.byTooltip('New plan'));
      await tap(tester, find.text('Discard'));
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      expect(bridge.openPlans(), 1);
    },
  );

  testWidgets(
    'cancelled and failed saves prevent Open; retry saves real content',
    (tester) async {
      await start(tester);
      await draw(tester);
      await tap(tester, find.byTooltip('Open plan'));
      await tap(tester, find.text('Save changes'));
      expect(dialogs, ['savePlan']);
      expect(walls(tester), 1);
      expect(dirty(), isTrue);

      dialog = (_) async => '${scratch.path}/missing/plan.domo';
      await tap(tester, find.byTooltip('Open plan'));
      await tap(tester, find.text('Save changes'));
      expect(dialogs, ['savePlan', 'savePlan']);
      expect(find.textContaining('could not save'), findsOneWidget);
      expect(dirty(), isTrue);

      final saved = '${scratch.path}/plan.domo';
      dialog = (call) async => call.method == 'savePlan' ? saved : null;
      await tap(tester, find.byTooltip('Open plan'));
      await tap(tester, find.text('Save changes'));
      expect(File(saved).existsSync(), isTrue);
      expect(dialogs.last, 'openPlan');
      expect(walls(tester), 1);
      expect(dirty(), isFalse);
      await tap(tester, find.byTooltip('New plan'));
      expect(walls(tester), 0);
      dialog = (_) async => saved;
      await tap(tester, find.byTooltip('Open plan'));
      expect(walls(tester), 1);
      expect(dirty(), isFalse);
    },
  );

  testWidgets(
    'discard followed by cancelled or failed Open keeps the dirty session',
    (tester) async {
      await start(tester);
      await draw(tester);
      await tap(tester, find.byTooltip('Open plan'));
      await tap(tester, find.text('Discard'));
      expect(walls(tester), 1);
      expect(dirty(), isTrue);
      dialog = (_) async => '${scratch.path}/absent.domo';
      await tap(tester, find.byTooltip('Open plan'));
      await tap(tester, find.text('Discard'));
      expect(find.textContaining('could not open'), findsOneWidget);
      expect(walls(tester), 1);
      expect(dirty(), isTrue);
    },
  );

  testWidgets(
    'view changes stay clean; saved edits followed by undo become dirty',
    (tester) async {
      await start(tester);
      await tap(tester, find.text('Door'));
      await tap(tester, find.widgetWithText(ChoiceChip, 'Walls'));
      expect(dirty(), isFalse);
      await draw(tester);
      dialog = (_) async => '${scratch.path}/plan.domo';
      await tap(tester, find.text('Save'));
      expect(dirty(), isFalse);
      await tap(tester, find.byTooltip('Undo'));
      expect(walls(tester), 0);
      expect(dirty(), isTrue);
    },
  );

  testWidgets('native close uses cancel, failed save, and discard paths', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    var closing = nativeClose(tester);
    await tester.pumpAndSettle();
    await tap(tester, find.text('Cancel'));
    expect(await closing, isFalse);
    dialog = (_) async => '${scratch.path}/missing/plan.domo';
    closing = nativeClose(tester);
    await tester.pumpAndSettle();
    await tap(tester, find.text('Save changes'));
    expect(await closing, isFalse);
    expect(dirty(), isTrue);
    closing = nativeClose(tester);
    await tester.pumpAndSettle();
    await tap(tester, find.text('Discard'));
    expect(await closing, isTrue);
  });

  testWidgets('pending file dialog blocks edits and overlapping native close', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final painter =
        tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter;
    final wall = painter.walls.single;
    await tester.tapAt(
      tester.getTopLeft(getCanvas()) +
          painter.toScreen((wall.start + wall.end) / 2),
    );
    await tester.pumpAndSettle();
    // The wall inspector is showing, so selection landed. "Delete wall" now
    // sits below the lazily-built viewport of the compact test surface; the
    // first measurement counter is the stable above-the-fold marker.
    expect(find.text('Length'), findsOneWidget);
    final picker = Completer<Object?>();
    dialog = (_) => picker.future;
    await tap(tester, find.text('Save'));
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
    final closing = nativeClose(tester);
    await tester.pumpAndSettle();
    expect(await closing, isFalse);
    expect(find.text('Save your changes?'), findsNothing);
    expect(walls(tester), 1);
    picker.complete(null);
    await tester.pumpAndSettle();
    expect(dirty(), isTrue);
    expect(tester.takeException(), isNull);
  });

  testWidgets('saving for native close writes the plan before approval', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final saved = '${scratch.path}/closing.domo';
    dialog = (_) async => saved;
    final closing = nativeClose(tester);
    await tester.pumpAndSettle();
    await tap(tester, find.text('Save changes'));
    expect(await closing, isTrue);
    expect(File(saved).existsSync(), isTrue);
    expect(dirty(), isFalse);
  });

  testWidgets('the clean Level 1 baseline cannot be undone', (tester) async {
    await start(tester);
    expect(historyButton(tester, 'Undo').onPressed, isNull);
    expect(historyButton(tester, 'Redo').onPressed, isNull);
    dialog = (_) async => '${scratch.path}/empty.domo';
    await tap(tester, find.text('Save'));
    expect(dirty(), isFalse);
    expect(historyButton(tester, 'Undo').onPressed, isNull);
    expect(dirty(), isFalse);
    await draw(tester);
    await tap(tester, find.byTooltip('Undo'));
    expect(walls(tester), 0);
    expect(historyButton(tester, 'Undo').onPressed, isNull);
    await draw(tester);
  });

  testWidgets(
    'level deletion refuses the last level and supports undo and reopen',
    (tester) async {
      await start(tester);
      final delete = find.byKey(const ValueKey('delete-active-level'));
      final view = camera(tester);
      await tap(tester, delete);
      expect(
        find.textContaining('Cannot delete level: Level 1 is the only level'),
        findsOneWidget,
      );
      expect(
        find.textContaining(RegExp(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-')),
        findsNothing,
      );
      expect(find.text('Level 1'), findsOneWidget);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
      expect(camera(tester), view);

      await tap(tester, find.byTooltip('Add a storey above'));
      expect(find.text('Level 2'), findsOneWidget);
      await tap(tester, delete);
      expect(find.text('Level 2'), findsNothing);
      expect(painter(tester).backgroundLevels, isEmpty);
      await tap(tester, find.byTooltip('Undo'));
      expect(find.text('Level 2'), findsOneWidget);
      expect(painter(tester).backgroundLevels.single.ghosted, isTrue);
      await tap(tester, find.byTooltip('Redo'));
      expect(find.text('Level 2'), findsNothing);
      dialog = (_) async => '${scratch.path}/deleted-level.domo';
      await tap(tester, find.text('Save'));
      await tap(tester, find.byTooltip('Open plan'));
      expect(find.text('Level 1'), findsOneWidget);
      expect(find.text('Level 2'), findsNothing);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'a full property edit preserves active level, other flags and the '
    'camera; undo restores every original field and redo reapplies them, '
    'surviving save and reopen',
    (tester) async {
      await start(tester);
      // A second, non-active level with a NON-default flag: proves a
      // metadata edit to Level 1 never touches it (R2.4).
      await tap(tester, find.byTooltip('Add a storey above'));
      await tap(tester, find.text('Level 1')); // reactivate Level 1
      final level2Id = painter(tester).backgroundLevels.single.levelId;
      await chooseVisibility(tester, level2Id, LevelVisibility.visible);
      expect(painter(tester).backgroundLevels.single.ghosted, isFalse);
      final view = camera(tester);

      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-name-field')),
        'Ground Floor',
      );
      // Above Level 2's 3000mm: proves reordering, not just field storage.
      await tester.enterText(
        find.byKey(const ValueKey('level-elevation-field')),
        '5000',
      );
      await tester.enterText(
        find.byKey(const ValueKey('level-floor-thickness-field')),
        '300',
      );
      await tester.enterText(
        find.byKey(const ValueKey('level-wall-height-field')),
        '3000',
      );
      await tester.enterText(
        find.byKey(const ValueKey('level-ceiling-height-field')),
        '3200',
      );
      await tap(tester, find.byKey(const ValueKey('level-properties-apply')));

      expect(find.text('Ground Floor'), findsOneWidget);
      expect(find.text('Level 1'), findsNothing);
      expect(dirty(), isTrue);
      expect(
        camera(tester),
        view,
        reason: 'an authoritative-field edit never touches the camera',
      );
      expect(
        painter(tester).backgroundLevels.single.levelId,
        level2Id,
        reason: 'the other level survives, still non-active',
      );
      expect(
        painter(tester).backgroundLevels.single.ghosted,
        isFalse,
        reason: 'its explicitly-set flag is untouched by editing Level 1',
      );
      expect(
        tester.getTopLeft(find.text('Level 2')).dy,
        lessThan(tester.getTopLeft(find.text('Ground Floor')).dy),
        reason:
            'elevation 5000 > Level 2\'s 3000: the row order actually swaps',
      );

      // Undo restores every original field, not just the rendered name —
      // verified by reopening the dialog, which always seeds from the
      // live snapshot, and reading every field back.
      await tap(tester, find.byTooltip('Undo'));
      expect(find.text('Level 1'), findsOneWidget);
      expect(find.text('Ground Floor'), findsNothing);
      expect(camera(tester), view);
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      expect(fieldText(tester, 'level-name-field'), 'Level 1');
      expect(fieldText(tester, 'level-elevation-field'), '0');
      expect(fieldText(tester, 'level-floor-thickness-field'), '200');
      expect(fieldText(tester, 'level-wall-height-field'), '2500');
      expect(fieldText(tester, 'level-ceiling-height-field'), '2500');
      await tap(tester, find.byKey(const ValueKey('level-properties-cancel')));

      // Redo reapplies the full edit in one step.
      await tap(tester, find.byTooltip('Redo'));
      expect(find.text('Ground Floor'), findsOneWidget);

      // Persistence: save, reopen, and the dialog still seeds every edited
      // value from the reopened document, not merely the visible name.
      dialog = (_) async => '${scratch.path}/level-properties.domo';
      await tap(tester, find.text('Save'));
      expect(dirty(), isFalse);
      await tap(tester, find.byTooltip('Open plan'));
      expect(find.text('Ground Floor'), findsOneWidget);
      // R2.6: reopen activates the LOWEST elevation — now Level 2 (3000),
      // not the former active Ground Floor (5000) — and every non-active
      // flag resets to ghosted; neither is persisted view state.
      expect(
        tester
            .widget<ListTile>(
              find.ancestor(
                of: find.text('Level 2'),
                matching: find.byType(ListTile),
              ),
            )
            .subtitle,
        isA<Text>().having((t) => t.data, 'data', 'Active'),
      );
      expect(
        tester
            .widget<ListTile>(
              find.ancestor(
                of: find.text('Ground Floor'),
                matching: find.byType(ListTile),
              ),
            )
            .subtitle,
        isA<Text>().having((t) => t.data, 'data', 'Ghosted'),
      );
      // Still targeted by name for editing/persistence, despite no longer
      // being active.
      await tap(tester, find.byTooltip('Edit Ground Floor'));
      await tester.pumpAndSettle();
      expect(fieldText(tester, 'level-elevation-field'), '5000');
      expect(fieldText(tester, 'level-floor-thickness-field'), '300');
      expect(fieldText(tester, 'level-wall-height-field'), '3000');
      expect(fieldText(tester, 'level-ceiling-height-field'), '3200');
      await tap(tester, find.byKey(const ValueKey('level-properties-cancel')));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'an invalid, cancelled or unchanged edit creates no batch; the modal '
    'blocks native close while showing and can unmount safely mid-edit',
    (tester) async {
      await start(tester);
      await draw(tester);
      expect(walls(tester), 1);
      dialog = (_) async => '${scratch.path}/no-op-edit.domo';
      await tap(tester, find.text('Save'));
      expect(dirty(), isFalse);

      // An out-of-bounds value, then Cancel: no batch.
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-elevation-field')),
        '99999999999',
      );
      await tap(tester, find.byKey(const ValueKey('level-properties-cancel')));
      await tester.pumpAndSettle();
      expect(dirty(), isFalse);

      // Every field left as seeded, then Apply: valid, but nothing changed,
      // so no gesture is sent at all.
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tap(tester, find.byKey(const ValueKey('level-properties-apply')));
      await tester.pumpAndSettle();
      expect(dirty(), isFalse);
      expect(find.text('Level 1'), findsOneWidget);

      // The modal blocks a native close request while it is showing —
      // stronger proof than tapping through the barrier, reusing the SAME
      // `_fileActionPending` gate `nativeClose` already exercises for the
      // pending-Save-dialog case elsewhere in this file.
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-name-field')),
        'Mid-edit',
      );
      final closing = nativeClose(tester);
      await tester.pumpAndSettle();
      expect(await closing, isFalse);
      expect(
        find.byKey(const ValueKey('level-properties-apply')),
        findsOneWidget,
      );
      await tap(tester, find.byKey(const ValueKey('level-properties-cancel')));
      expect(find.text('Mid-edit'), findsNothing);
      expect(dirty(), isFalse);

      // None of the above (invalid+cancel, unchanged+apply, blocked close)
      // pushed anything onto the undo stack: a single Undo still removes
      // exactly the original drawn wall, nothing more.
      await tap(tester, find.byTooltip('Undo'));
      expect(walls(tester), 0);
      expect(historyButton(tester, 'Undo').onPressed, isNull);

      // Unmounting mid-edit (the dialog's `await` still pending) must not
      // throw — the SAME teardown `start` always runs, exercised early.
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a drawn wall resolves the active level\'s CURRENT default height; an '
    'existing wall keeps its own; the height-affecting edit is warning-only',
    (tester) async {
      await start(tester);
      await draw(tester); // wall #1, at the original 2500mm default
      final firstWallId = painter(tester).walls.single.id;

      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-wall-height-field')),
        '3000',
      );
      await tap(tester, find.byKey(const ValueKey('level-properties-apply')));
      // Accepted despite the ceiling (still 2500) now sitting below the new
      // wall height (3000): R2.1's ceiling-below-wall-height rule is a
      // WARNING, never an error.
      expect(dirty(), isTrue);
      expect(
        find.byKey(const ValueKey('level-warnings-banner')),
        findsOneWidget,
      );

      // wall #2, drawn after the level's default changed.
      await tester.dragFrom(
        tester.getTopLeft(getCanvas()) + const Offset(100, 250),
        const Offset(250, 0),
      );
      await tester.pumpAndSettle();
      expect(walls(tester), 2);
      final secondWallId = painter(
        tester,
      ).walls.firstWhere((wall) => wall.id != firstWallId).id;

      final path = '${scratch.path}/wall-height-resolution.domo';
      dialog = (_) async => path;
      await tap(tester, find.text('Save'));

      final handle = alloc<Uint64>(sizeOf<Uint64>());
      final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      final pathPtr = toNative('$path\u0000');
      try {
        expect(bridge.planOpen(pathPtr, handle), ok);
        expect(bridge.planSnapshot(handle.value, out), ok);
        final document = jsonDecode(bridge.takeString(out)) as Map;
        final wallsJson = (document['walls'] as List).cast<Map>();
        double heightOf(String id) =>
            (wallsJson.firstWhere((w) => w['id'] == id)['heightMm'] as num)
                .toDouble();
        expect(
          heightOf(firstWallId),
          2500.0,
          reason: 'drawn before the change, keeps its own resolved height',
        );
        expect(
          heightOf(secondWallId),
          3000.0,
          reason: 'drawn after the change, at the new current default',
        );
      } finally {
        expect(bridge.planClose(handle.value), ok);
        release(pathPtr);
        release(handle);
        release(out);
      }
    },
  );

  testWidgets(
    'many simultaneous level warnings stay a bounded summary at a small '
    'canvas size, expandable to the full scrollable list',
    (tester) async {
      await start(tester);
      await tester.binding.setSurfaceSize(const Size(400, 700));
      await tester.pumpAndSettle();
      // Below the 900px compact breakpoint, level controls live in the
      // drawer, not inline.
      await tap(tester, find.byTooltip('Plan details'));

      const extraLevels = 5;
      for (var i = 0; i < extraLevels; i++) {
        await tap(tester, find.byTooltip('Add a storey above'));
      }
      // Every level (including the original) gets its ceiling dropped below
      // its wall height, so each one independently triggers R2.1's
      // ceiling-below-wall-height warning.
      for (final name in [
        'Level 1',
        for (var i = 0; i < extraLevels; i++) 'Level ${i + 2}',
      ]) {
        await tap(tester, find.byTooltip('Edit $name'));
        await tester.pumpAndSettle();
        await tester.enterText(
          find.byKey(const ValueKey('level-ceiling-height-field')),
          '2000',
        );
        await tap(tester, find.byKey(const ValueKey('level-properties-apply')));
      }

      // The drawer overlays the canvas while open; close it so the banner
      // underneath is the thing actually hit-testable and checked.
      await tester.tapAt(const Offset(10, 10));
      await tester.pumpAndSettle();

      final canvasRect = tester.getRect(getCanvas());
      final bannerFinder = find.byKey(const ValueKey('level-warnings-banner'));
      expect(bannerFinder, findsOneWidget);
      final bannerRect = tester.getRect(bannerFinder);
      expect(
        bannerRect.top,
        greaterThanOrEqualTo(
          tester
              .getRect(find.text('Drag to draw · Click a wall to select'))
              .bottom,
        ),
        reason: 'the warning summary must not cover the tool instructions',
      );
      expect(
        bannerRect.height,
        lessThan(80),
        reason: 'a bounded summary, not one row per warning',
      );
      expect(
        bannerRect.left,
        greaterThanOrEqualTo(canvasRect.left),
        reason: 'must not spill past the left edge of the canvas itself',
      );
      expect(bannerRect.right, lessThanOrEqualTo(canvasRect.right));
      expect(tester.takeException(), isNull);

      await tap(tester, find.byKey(const ValueKey('level-warnings-summary')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('level-warnings-list')), findsOneWidget);
      expect(find.textContaining('ceiling'), findsWidgets);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a positive-area wall overlap warns immediately and keeps the canvas '
    'editable',
    (tester) async {
      await start(tester);
      // Two parallel, disjoint walls: no overlap, no warning.
      await openWalls(tester, [
        (const Offset(-1000, 0), const Offset(1000, 0)),
        (const Offset(-1000, 1000), const Offset(1000, 1000)),
      ]);
      expect(find.byKey(const ValueKey('level-warnings-banner')), findsNothing);

      // The SAME plan file, now with a second wall 10mm from the first
      // (t=20 each, from `openWalls`): their bands overlap by 10mm along the
      // whole span, with no axis contact at all — R3.1.2's last row,
      // `wall-area-overlap`. Built through raw operations rather than a
      // canvas drag, so the geometry is exact.
      final overlapIds = await openWalls(tester, [
        (const Offset(-1000, 0), const Offset(1000, 0)),
        (const Offset(-1000, 10), const Offset(1000, 10)),
      ]);
      expect(
        find.byKey(const ValueKey('level-warnings-banner')),
        findsOneWidget,
        reason: 'the overlap must be surfaced as soon as the plan is open',
      );
      expect(
        find.textContaining('1 overlapping wall-segment pair'),
        findsOneWidget,
      );
      await tap(tester, find.byKey(const ValueKey('level-warnings-summary')));
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('level-warnings-list')),
          matching: find.text(
            'Wall segment 1 and Wall segment 2 on Level 1 '
            'overlap in thickness, but their axes do not meet',
          ),
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining('Crossing wall axes are split'),
        findsOneWidget,
      );
      expect(find.textContaining(overlapIds[0]), findsNothing);
      expect(find.textContaining(overlapIds[1]), findsNothing);
      await tap(tester, find.byKey(const ValueKey('warning-highlight-0')));
      expect(painter(tester).warningIds, containsAll(overlapIds));
      await tap(tester, find.byKey(const ValueKey('level-warnings-close')));
      expect(painter(tester).warningIds, isEmpty);

      // The canvas must remain editable: a warning never blocks a command
      // (R3.1.7). An unrelated edit — changing the level's elevation —
      // must still land while the warning is showing.
      await tap(tester, find.byTooltip('Edit Level 1'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-elevation-field')),
        '500',
      );
      await tap(tester, find.byKey(const ValueKey('level-properties-apply')));
      expect(
        dirty(),
        isTrue,
        reason:
            'an unrelated edit must not be blocked by the outstanding warning',
      );
      expect(
        find.byKey(const ValueKey('level-warnings-banner')),
        findsOneWidget,
        reason: 'an unrelated edit does not clear the existing overlap',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('wall-area-overlap warning tracks live delete undo and redo', (
    tester,
  ) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-1000, 0), const Offset(1000, 0)),
      (const Offset(-1000, 10), const Offset(1000, 10)),
    ]);
    expect(find.byKey(const ValueKey('level-warnings-banner')), findsOneWidget);
    expect(dirty(), isFalse);

    // Both swept bands contain this point, so use the real click-cycle to
    // select the second wall rather than relying on painter ordering.
    final point = screenPoint(tester, Offset.zero);
    await clickAt(tester, point, PointerDeviceKind.mouse);
    await clickAt(tester, point, PointerDeviceKind.mouse);
    expect(painter(tester).selected, ids[1]);
    expect(find.text('Length'), findsOneWidget);

    // The inspector action is below the compact test surface; invoke the
    // same real primary-wall delete command through its keyboard binding.
    await tester.sendKeyEvent(LogicalKeyboardKey.delete);
    await tester.pumpAndSettle();
    expect(walls(tester), 1);
    expect(find.byKey(const ValueKey('level-warnings-banner')), findsNothing);
    expect(dirty(), isTrue);
    expect(historyButton(tester, 'Undo').onPressed, isNotNull);

    await tap(tester, find.byTooltip('Undo'));
    expect(walls(tester), 2);
    expect(find.byKey(const ValueKey('level-warnings-banner')), findsOneWidget);
    expect(dirty(), isTrue);
    expect(historyButton(tester, 'Redo').onPressed, isNotNull);

    await tap(tester, find.byTooltip('Redo'));
    expect(walls(tester), 1);
    expect(find.byKey(const ValueKey('level-warnings-banner')), findsNothing);
    expect(dirty(), isTrue);
    expect(historyButton(tester, 'Undo').onPressed, isNotNull);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'nonempty level deletion preserves content, selection and history',
    (tester) async {
      await start(tester);
      await openLevelScene(tester);
      final wall = painter(tester).walls.single;
      await clickAt(
        tester,
        screenPoint(tester, (wall.start + wall.end) / 2),
        PointerDeviceKind.mouse,
      );
      final before = painter(tester);
      await tap(tester, find.byKey(const ValueKey('delete-active-level')));
      expect(
        find.textContaining(
          'Cannot delete level: Lower A still holds 1 wall(s)',
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining(RegExp(r'[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-')),
        findsNothing,
      );
      expect(painter(tester).walls.single.id, wall.id);
      expect(painter(tester).derived, same(before.derived));
      expect(painter(tester).selectedIds, before.selectedIds);
      expect(painter(tester).backgroundLevels, before.backgroundLevels);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'save and navigation preserve redo; reopen resets session history',
    (tester) async {
      await start(tester);
      await draw(tester);
      await tap(tester, find.byTooltip('Undo'));
      final before = camera(tester);
      final saved = '${scratch.path}/history.domo';
      dialog = (_) async => saved;
      await tap(tester, find.text('Save'));
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Redo').onPressed, isNotNull);
      await tap(tester, find.byTooltip('Hide grid and rulers'));
      expect(dirty(), isFalse);
      await tap(tester, find.byTooltip('Redo'));
      expect(walls(tester), 1);
      expect(dirty(), isTrue);
      expect(camera(tester), before);
      await tap(tester, find.text('Save'));
      await tap(tester, find.byTooltip('Open plan'));
      expect(walls(tester), 1);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
      expect(historyButton(tester, 'Redo').onPressed, isNull);
    },
  );

  testWidgets('keyboard undo and redo work and a new edit invalidates redo', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    for (final modifier in [
      LogicalKeyboardKey.controlLeft,
      LogicalKeyboardKey.metaLeft,
    ]) {
      await tester.sendKeyDownEvent(modifier);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
      await tester.pumpAndSettle();
      expect(walls(tester), 0);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyEvent(LogicalKeyboardKey.keyZ);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendKeyUpEvent(modifier);
      await tester.pumpAndSettle();
      expect(walls(tester), 1);
    }
    await tap(tester, find.byTooltip('Undo'));
    await draw(tester);
    expect(historyButton(tester, 'Redo').onPressed, isNull);
  });

  testWidgets('drawing preserves the camera and committed screen coordinates', (
    tester,
  ) async {
    await start(tester);
    final before = camera(tester);
    // Isolate the camera transform from snapping's intentional displacement.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
    await draw(tester);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
    expect(camera(tester), before);
    final painter =
        tester.widget<CustomPaint>(getCanvas()).painter! as PlanPainter;
    expect(
      painter.toScreen(painter.walls.single.start),
      const Offset(100, 150),
    );
    expect(painter.toScreen(painter.walls.single.end), const Offset(350, 150));
  });

  testWidgets(
    'wheel and keyboard navigation never dirty or edit the document',
    (tester) async {
      await start(tester);
      final initial = camera(tester);
      const local = Offset(300, 200);
      final position = tester.getTopLeft(getCanvas()) + local;
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: position,
          scrollDelta: const Offset(0, 100),
        ),
      );
      await tester.pumpAndSettle();
      expect(camera(tester).origin, initial.origin + const Offset(0, 100));
      await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: position,
          scrollDelta: const Offset(0, 100),
        ),
      );
      await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await tester.pumpAndSettle();
      expect(camera(tester).origin, initial.origin + const Offset(100, 100));
      final point = camera(tester).toPlan(local);
      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: position,
          scrollDelta: const Offset(0, -100),
        ),
      );
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(camera(tester).zoom, 0.125);
      expect(camera(tester).toPlan(local), point);
      await tester.sendKeyEvent(LogicalKeyboardKey.arrowRight);
      await tester.sendKeyEvent(LogicalKeyboardKey.minus);
      await tester.pumpAndSettle();
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
    },
  );

  testWidgets(
    'middle drag cancellation restores the camera without drawing a wall',
    (tester) async {
      await start(tester);
      final initial = camera(tester);
      final gesture = await tester.startGesture(
        tester.getCenter(getCanvas()),
        kind: PointerDeviceKind.mouse,
        buttons: kMiddleMouseButton,
      );
      await gesture.moveBy(const Offset(80, 60));
      await tester.pump();
      expect(camera(tester).origin, initial.origin + const Offset(80, 60));
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(camera(tester), initial);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
    },
  );

  testWidgets('Fit is explicit and resizing keeps the same plan-space centre', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    final initial = camera(tester);
    await tap(tester, find.byTooltip('Fit plan'));
    expect(camera(tester).zoom, isNot(initial.zoom));
    var size = tester.getSize(getCanvas());
    final centre = camera(tester).toPlan(size.center(Offset.zero));
    await tester.binding.setSurfaceSize(const Size(700, 700));
    await tester.pumpAndSettle();
    size = tester.getSize(getCanvas());
    expect(camera(tester).toPlan(size.center(Offset.zero)), centre);
    expect(walls(tester), 1);
  });

  testWidgets(
    'release commits at its own cursor, including outside the canvas',
    (tester) async {
      await start(tester);
      // This checks pointer capture, independent of snap target selection.
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      final top = tester.getTopLeft(getCanvas());
      final end = Offset(tester.getSize(getCanvas()).width + 30, 150);
      final gesture = await tester.startGesture(
        top + const Offset(100, 150),
        pointer: 42,
      );
      await gesture.moveTo(top + const Offset(350, 150));
      await tester.sendEventToBinding(
        PointerUpEvent(pointer: 42, position: top + end),
      );
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();
      expect(walls(tester), 1);
      expect(painter(tester).toScreen(painter(tester).walls.single.end), end);
      expect(dirty(), isTrue);
    },
  );

  testWidgets(
    'wheel during a drag reprojects preview and release consistently',
    (tester) async {
      await start(tester);
      // Preserve raw cursor coordinates while testing camera reprojection.
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      final top = tester.getTopLeft(getCanvas());
      const end = Offset(350, 150);
      final gesture = await tester.startGesture(top + const Offset(100, 150));
      await gesture.moveTo(top + end);
      await tester.sendEventToBinding(
        PointerScrollEvent(
          position: top + end,
          scrollDelta: const Offset(0, 100),
        ),
      );
      await tester.pumpAndSettle();
      expect(painter(tester).toScreen(painter(tester).draft!.$2), end);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      await gesture.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();
      expect(painter(tester).toScreen(painter(tester).walls.single.end), end);
    },
  );

  testWidgets(
    'overlapping wall drags refuse without dirtying or changing history',
    (tester) async {
      await start(tester);
      // Fit at exactly 0.5 px/mm, so screen-to-plan rounding does not turn
      // an intended coincident axis into a distinct, sub-pixel parallel axis.
      final halfSpan = tester.getSize(getCanvas()).width * 0.9;
      final ids = await openWalls(tester, [
        (Offset(-halfSpan, 0), Offset(halfSpan, 0)),
        (Offset(-halfSpan, 500), Offset(halfSpan, 500)),
      ]);
      expect(camera(tester).zoom, 0.5);
      final saved = File('${scratch.path}/selection.domo').readAsBytesSync();
      final before = painter(tester).walls.toList();
      expect(historyButton(tester, 'Undo').onPressed, isNull);
      for (final attempt in [
        (const Offset(0, 0), 'wall-coincidence'),
        (const Offset(500, 0), 'collinear-overlap'),
      ]) {
        final from = screenPoint(tester, const Offset(0, 500));
        final to = screenPoint(tester, attempt.$1);
        // Alt held from before the press disables snapping for the whole
        // gesture (R6.4.2), which is what lets the drag land on an EXACTLY
        // coincident axis. With snapping on, R6.3.1's class-major order sends
        // a pointer inside a wall to that wall's FACE rather than its
        // centre line, so these two refusals are unreachable by dragging —
        // the refusals themselves are what this test is about.
        await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
        await tester.dragFrom(from, to - from);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
        await tester.pumpAndSettle();
        expect(
          find.textContaining(attempt.$2),
          findsOneWidget,
          reason: tester
              .widgetList<Text>(find.byType(Text))
              .map((text) => text.data)
              .join('\n'),
        );
        expect(walls(tester), 2);
        for (var i = 0; i < before.length; i++) {
          expect(painter(tester).walls[i].id, ids[i]);
          expect(painter(tester).walls[i].start, before[i].start);
          expect(painter(tester).walls[i].end, before[i].end);
        }
        expect(dirty(), isFalse);
        expect(historyButton(tester, 'Undo').onPressed, isNull);
        expect(historyButton(tester, 'Redo').onPressed, isNull);
        expect(File('${scratch.path}/selection.domo').readAsBytesSync(), saved);
        expect(tester.takeException(), isNull);
      }
      // The same handle remains editable; one accepted move creates one undo.
      final from = screenPoint(tester, const Offset(0, 500));
      final to = screenPoint(tester, const Offset(0, 750));
      await tester.sendKeyDownEvent(LogicalKeyboardKey.altLeft);
      await tester.dragFrom(from, to - from);
      await tester.sendKeyUpEvent(LogicalKeyboardKey.altLeft);
      await tester.pumpAndSettle();
      expect(dirty(), isTrue);
      await tap(tester, find.byTooltip('Undo'));
      expect(painter(tester).walls[1].start, before[1].start);
      expect(painter(tester).walls[1].end, before[1].end);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets('click jitter up to five pixels never moves a saved wall', (
    tester,
  ) async {
    await start(tester);
    await draw(tester);
    dialog = (_) async => '${scratch.path}/jitter.domo';
    await tap(tester, find.text('Save'));
    final before = painter(tester).walls.single;
    for (final displacement in [1.0, 5.0]) {
      final point =
          tester.getTopLeft(getCanvas()) +
          painter(tester).toScreen(before.start);
      final gesture = await tester.startGesture(point);
      await gesture.moveBy(Offset(displacement, 0));
      await gesture.up();
      await tester.pumpAndSettle();
      expect(painter(tester).walls.single.start, before.start);
      expect(dirty(), isFalse);
    }
  });

  testWidgets(
    'cancellation restores selection and interruption rolls back pan',
    (tester) async {
      await start(tester);
      await draw(tester);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(painter(tester).selected, isNull);
      dialog = (_) async => '${scratch.path}/cancel.domo';
      await tap(tester, find.text('Save'));
      final point =
          tester.getTopLeft(getCanvas()) +
          painter(tester).toScreen(painter(tester).walls.single.start);
      var gesture = await tester.startGesture(point);
      await gesture.moveBy(const Offset(50, 0));
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(painter(tester).selected, isNull);
      expect(dirty(), isFalse);
      final before = camera(tester);
      gesture = await tester.startGesture(
        point,
        kind: PointerDeviceKind.mouse,
        buttons: kMiddleMouseButton,
      );
      await gesture.moveBy(const Offset(40, 30));
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.inactive);
      await gesture.up();
      tester.binding.handleAppLifecycleStateChanged(AppLifecycleState.resumed);
      await tester.pumpAndSettle();
      expect(camera(tester), before);
      expect(dirty(), isFalse);
    },
  );

  testWidgets('Fit waits for a nonzero canvas size', (tester) async {
    await start(tester);
    await draw(tester);
    final chromeHeight = 800 - tester.getSize(getCanvas()).height;
    await tester.binding.setSurfaceSize(Size(1200, chromeHeight));
    await tester.pumpAndSettle();
    expect(tester.getSize(getCanvas()).height, 0);
    await tap(tester, find.byTooltip('Fit plan'));
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    await tester.pumpAndSettle();
    final expected = PlanViewport.fit(
      tester.getSize(getCanvas()),
      PlanPainter.boundsOf(painter(tester).walls, painter(tester).derived),
    );
    expect(camera(tester), expected);
    expect(walls(tester), 1);
  });

  testWidgets(
    'tool switch and pointer cancel discard previews without history',
    (tester) async {
      await start(tester);
      final point = tester.getTopLeft(getCanvas()) + const Offset(100, 150);
      var gesture = await tester.startGesture(point);
      await gesture.moveBy(const Offset(200, 0));
      await gesture.cancel();
      await tester.pumpAndSettle();
      expect(painter(tester).draft, isNull);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
      gesture = await tester.startGesture(point);
      await gesture.moveBy(const Offset(200, 0));
      await tap(tester, find.text('Door'));
      await gesture.up();
      await tester.pumpAndSettle();
      expect(painter(tester).draft, isNull);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'trackpad cumulative pan and scale compose without document edits',
    (tester) async {
      await start(tester);
      final before = camera(tester);
      final top = tester.getTopLeft(getCanvas());
      const anchor = Offset(300, 200);
      await tester.sendEventToBinding(
        PointerPanZoomStartEvent(pointer: 77, position: top + anchor),
      );
      await tester.sendEventToBinding(
        PointerPanZoomUpdateEvent(
          pointer: 77,
          position: top + anchor,
          pan: const Offset(20, 10),
          panDelta: const Offset(20, 10),
          scale: 1.25,
        ),
      );
      await tester.pumpAndSettle();
      var expected = before.panZoom(
        previousCentroid: anchor,
        currentCentroid: anchor + const Offset(20, 10),
        scale: 1.25,
      );
      expect(camera(tester).origin, expected.origin);
      expect(camera(tester).zoom, expected.zoom);
      await tester.sendEventToBinding(
        PointerPanZoomUpdateEvent(
          pointer: 77,
          position: top + anchor,
          pan: const Offset(40, 20),
          panDelta: const Offset(20, 10),
          scale: 1.5,
        ),
      );
      await tester.pumpAndSettle();
      expected = expected.panZoom(
        previousCentroid: anchor + const Offset(20, 10),
        currentCentroid: anchor + const Offset(40, 20),
        scale: 1.5 / 1.25,
      );
      expect(camera(tester), expected);
      expect(camera(tester).zoom, closeTo(before.zoom * 1.5, 1e-12));
      await tester.sendEventToBinding(
        PointerPanZoomEndEvent(pointer: 77, position: top + anchor),
      );
      await tester.pumpAndSettle();
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
    },
  );

  testWidgets(
    'invalid trackpad steps are discarded and Escape restores the camera',
    (tester) async {
      await start(tester);
      final before = camera(tester);
      final position = tester.getCenter(getCanvas());
      await tester.sendEventToBinding(
        PointerPanZoomStartEvent(pointer: 78, position: position),
      );
      await tester.sendEventToBinding(
        PointerPanZoomUpdateEvent(
          pointer: 78,
          position: position,
          pan: const Offset(40, 20),
          scale: 0,
        ),
      );
      await tester.pumpAndSettle();
      expect(camera(tester), before);
      await tester.sendEventToBinding(
        PointerPanZoomUpdateEvent(
          pointer: 78,
          position: position,
          pan: const Offset(40, 20),
          scale: 2,
        ),
      );
      await tester.pumpAndSettle();
      expect(camera(tester).zoom, before.zoom * 2);
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.sendEventToBinding(
        PointerPanZoomEndEvent(pointer: 78, position: position),
      );
      await tester.pumpAndSettle();
      expect(camera(tester), before);
      expect(dirty(), isFalse);
    },
  );

  testWidgets(
    'second touch cancels drawing before pinch and cancel restores its start',
    (tester) async {
      await start(tester);
      final before = camera(tester);
      final top = tester.getTopLeft(getCanvas());
      final first = await tester.startGesture(
        top + const Offset(100, 200),
        pointer: 81,
      );
      await first.moveTo(top + const Offset(200, 200));
      await tester.pumpAndSettle();
      expect(painter(tester).draft, isNotNull);
      final second = await tester.startGesture(
        top + const Offset(400, 200),
        pointer: 82,
      );
      await tester.pumpAndSettle();
      expect(painter(tester).draft, isNull);
      await second.moveTo(top + const Offset(450, 200));
      await tester.pumpAndSettle();
      expect(
        camera(tester),
        before.panZoom(
          previousCentroid: const Offset(300, 200),
          currentCentroid: const Offset(325, 200),
          scale: 1.25,
        ),
      );
      final third = await tester.startGesture(
        top + const Offset(600, 300),
        pointer: 83,
      );
      await third.moveBy(const Offset(100, 20));
      await third.up();
      await tester.pumpAndSettle();
      expect(camera(tester).zoom, before.zoom * 1.25);
      await second.cancel();
      await first.up();
      await tester.pumpAndSettle();
      expect(camera(tester), before);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      expect(historyButton(tester, 'Undo').onPressed, isNull);
      await draw(tester);
    },
  );

  testWidgets(
    'pinch release keeps navigation and never turns remaining finger into an edit',
    (tester) async {
      await start(tester);
      final before = camera(tester);
      final top = tester.getTopLeft(getCanvas());
      final first = await tester.startGesture(
        top + const Offset(200, 200),
        pointer: 91,
      );
      final second = await tester.startGesture(
        top + const Offset(400, 200),
        pointer: 92,
      );
      await second.moveTo(top + const Offset(500, 200));
      await second.up();
      await first.moveBy(const Offset(200, 100));
      await first.up();
      await tester.pumpAndSettle();
      expect(camera(tester).zoom, before.zoom * 1.5);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      await draw(tester);
    },
  );

  testWidgets(
    'stale and nonfinite releases do not commit or cancel an active gesture',
    (tester) async {
      await start(tester);
      final top = tester.getTopLeft(getCanvas());
      final gesture = await tester.startGesture(
        top + const Offset(100, 150),
        pointer: 101,
      );
      await gesture.moveTo(top + const Offset(350, 150));
      await tester.pumpAndSettle();
      final input = tester.widget<Listener>(
        find.byKey(const ValueKey('plan-input')),
      );
      final before = painter(tester).draft;
      // Drive the local Listener contract directly: the engine's hit tester
      // itself rejects NaN positions before they can reach a widget callback.
      input.onPointerUp!(
        const PointerUpEvent(pointer: 999, position: Offset(350, 150)),
      );
      input.onPointerUp!(
        const PointerUpEvent(pointer: 101, position: Offset(double.nan, 150)),
      );
      await tester.pumpAndSettle();
      expect(painter(tester).draft, before);
      expect(walls(tester), 0);
      expect(dirty(), isFalse);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(walls(tester), 1);
    },
  );

  testWidgets(
    'Escape during trackpad navigation preserves pre-gesture selection',
    (tester) async {
      await start(tester);
      await draw(tester);
      final wall = painter(tester).walls.single;
      final position =
          tester.getTopLeft(getCanvas()) +
          painter(tester).toScreen((wall.start + wall.end) / 2);
      await tester.tapAt(position);
      await tester.pumpAndSettle();
      expect(painter(tester).selected, wall.id);
      final before = camera(tester);
      await tester.sendEventToBinding(
        PointerPanZoomStartEvent(pointer: 102, position: position),
      );
      await tester.sendEventToBinding(
        PointerPanZoomUpdateEvent(pointer: 102, position: position, scale: 2),
      );
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.sendEventToBinding(
        PointerPanZoomEndEvent(pointer: 102, position: position),
      );
      await tester.pumpAndSettle();
      expect(camera(tester), before);
      expect(painter(tester).selected, wall.id);
      expect(walls(tester), 1);
    },
  );

  // ---- marquee selection (issue 0059) ----

  /// Holds Ctrl for one drag, mirroring `zoomAt`'s modifier pattern.
  Future<void> modifierDragFrom(
    WidgetTester tester,
    Offset from,
    Offset delta,
  ) async {
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.dragFrom(from, delta);
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
  }

  // Original PH1/PH2 snapshot, built from a native-written baseline package.
  // Python only constructs the fixture and rebuilds its integrity inventory;
  // all inspection, selection and deletion below run through the real bridge.
  Future<String> openPlaceholder(
    WidgetTester tester, {
    required bool requiredAsset,
  }) async {
    const instance = '01950000-0000-7000-8000-000000000009';
    final path = '${scratch.path}/placeholder.domo';
    final ptr = toNative('$path\u0000');
    final handle = alloc<Uint64>(sizeOf<Uint64>());
    try {
      expect(bridge.planCreateDefault(handle), ok);
      expect(bridge.planSave(handle.value, ptr.cast()), ok);
      expect(bridge.planClose(handle.value), ok);
    } finally {
      release(ptr);
      release(handle);
    }
    final result = Process.runSync('python3', [
      '-c',
      r'''import hashlib, json, sys, zipfile
path, instance, requirement = sys.argv[1:]
with zipfile.ZipFile(path) as z:
    blobs = {n: z.read(n) for n in z.namelist() if not n.startswith("ops/")}
manifest = json.loads(blobs["domowik.json"])
section = json.loads(blobs["snapshot/state.json"])
doc = section["document"]
digest = "sha256:" + "0" * 64
doc["assetInstances"] = [{"id": instance, "levelId": doc["levels"][0]["id"],
    "digest": digest, "displayName": "Missing model",
    "transform": {"translationMm": [2000, 0], "scaleUniform": 3, "rotationDeg": 45},
    "editorFootprintMm": [600, 400]}]
blobs["snapshot/state.json"] = json.dumps(section).encode()
blobs["assets/descriptors.json"] = json.dumps({digest: {"digest": digest,
    "byteLength": 1, "mediaType": "model/obj", "purpose": "furniture-model",
    "encoding": "original", "requirement": requirement, "provenance": {}}}).encode()
manifest["assetRoot"] = [{"digest": digest}]
manifest["profile"] = "thin"
manifest["entries"] = {n: {"sha256": "sha256:" + hashlib.sha256(b).hexdigest(),
    "byteLength": len(b)} for n, b in blobs.items() if n != "domowik.json"}
blobs["domowik.json"] = json.dumps(manifest).encode()
with zipfile.ZipFile(path, "w") as z:
    for n, b in blobs.items(): z.writestr(n, b)
''',
      path,
      instance,
      requiredAsset ? 'required' : 'optional',
    ]);
    expect(result.exitCode, 0, reason: '${result.stderr}');
    dialog = (call) async => call.method == 'openPlan' ? path : null;
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pumpAndSettle();
    expect(painter(tester).assets.single.id, instance);
    return instance;
  }

  for (final requiredAsset in [false, true]) {
    testWidgets(
      'PH1 native-backed marquee and deletion with required=$requiredAsset',
      (tester) async {
        await start(tester);
        final instance = await openPlaceholder(
          tester,
          requiredAsset: requiredAsset,
        );
        await zoomAt(tester, screenPoint(tester, const Offset(2000, 0)), false);
        await zoomAt(tester, screenPoint(tester, const Offset(2000, 0)), false);
        final from = screenPoint(tester, const Offset(1400, -600));
        await modifierDragFrom(
          tester,
          from,
          screenPoint(tester, const Offset(2600, 600)) - from,
        );
        expect(painter(tester).selectedIds, {instance});
        expect(painter(tester).selected, instance);
        expect(dirty(), isFalse);
        // PH2 ignores both the non-identity transform and editor footprint.
        expect(painter(tester).assets.single.corners, [
          const Offset(1500, -500),
          const Offset(2500, -500),
          const Offset(2500, 500),
          const Offset(1500, 500),
        ]);
        await tester.sendKeyEvent(LogicalKeyboardKey.delete);
        await tester.pumpAndSettle();
        if (requiredAsset) {
          expect(painter(tester).assets.single.id, instance);
          expect(painter(tester).selectedIds, {instance});
          expect(dirty(), isFalse);
        } else {
          expect(painter(tester).assets, isEmpty);
          expect(painter(tester).selectedIds, isEmpty);
          expect(painter(tester).selected, isNull);
          await tester.tap(find.byTooltip('Undo'));
          await tester.pumpAndSettle();
          expect(painter(tester).assets.single.id, instance);
          expect(painter(tester).selectedIds, isEmpty);
        }
        expect(tester.takeException(), isNull);
      },
    );
  }

  testWidgets(
    'modifier-drag from empty canvas performs additive marquee selection; '
    'plain drag still draws',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-1000, -1000), const Offset(1000, -1000)),
        (const Offset(-1000, 1000), const Offset(1000, 1000)),
      ]);
      expect(painter(tester).selectedIds, isEmpty);

      // Both walls' occupied rectangles are well inside this rectangle,
      // whose corners are clear of every wall (R7.2.4: additive marquee).
      await modifierDragFrom(
        tester,
        screenPoint(tester, const Offset(-1100, -1100)),
        screenPoint(tester, const Offset(1100, 1100)) -
            screenPoint(tester, const Offset(-1100, -1100)),
      );

      expect(
        walls(tester),
        2,
        reason: 'a modifier-drag over empty canvas must never draw a wall',
      );
      expect(painter(tester).selectedIds, {ids[0], ids[1]});
      // Ascending marquee key orders by min-corner y here (both walls share
      // min-corner x): the SECOND wall (higher y) sorts last, so it becomes
      // primary (R7.3.2).
      expect(painter(tester).selected, ids[1]);
      expect(
        dirty(),
        isFalse,
        reason: 'selection is transient, never a document edit',
      );
      expect(tester.takeException(), isNull);

      // Plain drag from empty canvas is UNCHANGED: it still draws a wall,
      // never a marquee. `draw()` assumes a blank canvas, so this repeats
      // its drag directly, at (0,0)-(300,0) — inside the auto-fit view (the
      // walls sit at y=+/-1000, so this stays clear of both) rather than far
      // outside it, where a drag never reaches the canvas at all.
      final gapFrom = screenPoint(tester, Offset.zero);
      final gapTo = screenPoint(tester, const Offset(300, 0));
      expect(tester.getRect(getCanvas()).contains(gapFrom), isTrue);
      expect(tester.getRect(getCanvas()).contains(gapTo), isTrue);
      await tester.dragFrom(gapFrom, gapTo - gapFrom);
      await tester.pumpAndSettle();
      expect(walls(tester), 3);
    },
  );

  testWidgets('an additive marquee that finds nothing new leaves selection and '
      'primary unchanged', (tester) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-1000, -1000), const Offset(1000, -1000)),
      (const Offset(-1000, 1000), const Offset(1000, 1000)),
    ]);
    await modifierDragFrom(
      tester,
      screenPoint(tester, const Offset(-1100, -1100)),
      screenPoint(tester, const Offset(1100, 1100)) -
          screenPoint(tester, const Offset(-1100, -1100)),
    );
    expect(painter(tester).selectedIds, {ids[0], ids[1]});
    expect(painter(tester).selected, ids[1]);

    // A second additive marquee over the SAME (already fully selected)
    // rectangle adds nothing new: R7.3.2 says selection AND primary stay
    // exactly as they were, not merely equal in content.
    await modifierDragFrom(
      tester,
      screenPoint(tester, const Offset(-1100, -1100)),
      screenPoint(tester, const Offset(1100, 1100)) -
          screenPoint(tester, const Offset(-1100, -1100)),
    );
    expect(painter(tester).selectedIds, {ids[0], ids[1]});
    expect(painter(tester).selected, ids[1]);
    expect(tester.takeException(), isNull);
  });

  testWidgets('a modifier-click on empty canvas still clears the selection', (
    tester,
  ) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-1000, -1000), const Offset(1000, -1000)),
    ]);
    await modifierDragFrom(
      tester,
      screenPoint(tester, const Offset(-1100, -1100)),
      screenPoint(tester, const Offset(1100, 1100)) -
          screenPoint(tester, const Offset(-1100, -1100)),
    );
    expect(painter(tester).selectedIds, {ids[0]});

    // R7.2.4: modifier-click on empty space clears the selection — a
    // below-threshold release of the same gesture this test's marquee
    // uses, so this also proves the click/drag disambiguation still
    // works with the new mode in place.
    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    await tester.tapAt(screenPoint(tester, const Offset(-1100, -1100)));
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();
    expect(painter(tester).selectedIds, isEmpty);
    expect(painter(tester).selected, isNull);
  });

  testWidgets('Escape during a marquee drag restores the pre-drag selection', (
    tester,
  ) async {
    await start(tester);
    final ids = await openWalls(tester, [
      (const Offset(-1000, -1000), const Offset(1000, -1000)),
      (const Offset(-1000, 1000), const Offset(1000, 1000)),
    ]);
    // A prior selection the cancelled marquee must restore exactly.
    await clickAt(
      tester,
      screenPoint(tester, const Offset(0, -1000)),
      PointerDeviceKind.mouse,
    );
    expect(painter(tester).selectedIds, {ids[0]});

    await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
    final gesture = await tester.startGesture(
      screenPoint(tester, const Offset(-1100, -1100)),
    );
    await gesture.moveTo(screenPoint(tester, const Offset(1100, 1100)));
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await gesture.up();
    await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
    await tester.pumpAndSettle();

    expect(painter(tester).selectedIds, {
      ids[0],
    }, reason: 'cancellation must restore the pre-drag selection exactly');
    expect(
      walls(tester),
      2,
      reason: 'a cancelled marquee draws no wall either',
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('marquee never selects a wall on a different level', (
    tester,
  ) async {
    await start(tester);
    final scene = await openLevelScene(tester, overlappingActiveWalls: false);
    // openLevelScene's fixture places walls at DIFFERENT y per level
    // (upper: -1500, lower B: 1500, active lower A: 0), so the rectangle
    // below deliberately encloses ALL three (x in [-1100,1100], y in
    // [-1600,1600]) rather than just the active one. Only if cross-level
    // exclusion (R7.5.4) actually holds does this rectangle still yield
    // exactly the active level's walls.
    final activeWallIds = painter(tester).walls.map((w) => w.id).toSet();
    expect(activeWallIds, scene.walls[2].toSet());
    final otherLevelWallIds = {...scene.walls[0], ...scene.walls[1]};
    expect(otherLevelWallIds, isNotEmpty);

    final from = screenPoint(tester, const Offset(-1100, -1600));
    final to = screenPoint(tester, const Offset(1100, 1600));
    final canvasRect = tester.getRect(getCanvas());
    expect(
      canvasRect.contains(from),
      isTrue,
      reason: 'the rectangle must actually reach the canvas, not miss it',
    );
    expect(
      canvasRect.contains(to),
      isTrue,
      reason: 'the rectangle must actually reach the canvas, not miss it',
    );

    await modifierDragFrom(tester, from, to - from);

    expect(
      painter(tester).selectedIds,
      activeWallIds,
      reason: "only the ACTIVE level's walls are eligible (R7.5.4)",
    );
    for (final id in otherLevelWallIds) {
      expect(
        painter(tester).selectedIds.contains(id),
        isFalse,
        reason:
            'a rectangle spanning every level must still exclude the '
            'inactive ones, not merely happen to miss them',
      );
    }
  });

  testWidgets(
    'the marquee rubber-band is visible mid-drag and cleared on release',
    (tester) async {
      await start(tester);
      await openWalls(tester, [
        (const Offset(-1000, -1000), const Offset(1000, -1000)),
      ]);
      expect(painter(tester).marqueeRect, isNull);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      final gesture = await tester.startGesture(
        screenPoint(tester, const Offset(-1100, -1100)),
      );
      await gesture.moveTo(screenPoint(tester, const Offset(1100, 1100)));
      await tester.pump();
      expect(
        painter(tester).marqueeRect,
        isNotNull,
        reason: 'the rubber-band preview must be visible mid-drag',
      );

      await gesture.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(
        painter(tester).marqueeRect,
        isNull,
        reason: 'the preview is release-transient, not left behind',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a second touch cancels an in-progress marquee drag, restoring its start',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-1000, -1000), const Offset(1000, -1000)),
        (const Offset(-1000, 1000), const Offset(1000, 1000)),
      ]);
      await clickAt(
        tester,
        screenPoint(tester, const Offset(0, -1000)),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).selectedIds, {ids[0]});

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      final first = await tester.startGesture(
        screenPoint(tester, const Offset(-1100, -1100)),
        pointer: 1,
      );
      await first.moveTo(screenPoint(tester, const Offset(1100, 1100)));
      await tester.pumpAndSettle();
      expect(painter(tester).marqueeRect, isNotNull);

      // A second touch landing mid-gesture cancels the first, generically —
      // the same mechanism "second touch cancels drawing..." exercises for
      // wall-drawing, now exercised for a marquee drag instead.
      final second = await tester.startGesture(
        screenPoint(tester, const Offset(0, 0)),
        pointer: 2,
      );
      await tester.pumpAndSettle();
      expect(
        painter(tester).marqueeRect,
        isNull,
        reason: 'the second touch must cancel the marquee, not extend it',
      );
      expect(painter(tester).selectedIds, {
        ids[0],
      }, reason: 'cancellation restores the pre-drag selection exactly');

      await second.cancel();
      await first.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, {ids[0]});
      expect(walls(tester), 2);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a pointer cancel on the marquee gesture itself restores the pre-press '
    'selection and primary',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-1000, -1000), const Offset(1000, -1000)),
        (const Offset(-1000, 1000), const Offset(1000, 1000)),
      ]);
      await clickAt(
        tester,
        screenPoint(tester, const Offset(0, -1000)),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).selectedIds, {ids[0]});
      expect(painter(tester).selected, ids[0]);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      final gesture = await tester.startGesture(
        screenPoint(tester, const Offset(-1100, -1100)),
      );
      await gesture.moveTo(screenPoint(tester, const Offset(1100, 1100)));
      await tester.pumpAndSettle();
      expect(painter(tester).marqueeRect, isNotNull);

      // A PointerCancelEvent on the marquee's OWN pointer — not a second
      // touch — must cancel it the same way Escape does.
      await gesture.cancel();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(
        painter(tester).marqueeRect,
        isNull,
        reason: 'a pointer cancel must clear the rubber-band preview',
      );
      expect(painter(tester).selectedIds, {
        ids[0],
      }, reason: 'cancellation restores the pre-press selection exactly');
      expect(
        painter(tester).selected,
        ids[0],
        reason: 'cancellation restores the pre-press primary exactly',
      );
      expect(walls(tester), 2);
      expect(dirty(), isFalse);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a modifier-drag starting on a non-primary entity stays inert, unlike '
    'one starting on empty canvas',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-1000, -1000), const Offset(1000, -1000)),
        (const Offset(-1000, 1000), const Offset(1000, 1000)),
      ]);
      // Select the FIRST wall as primary, so the second wall's midpoint is a
      // non-primary hit target (R7.2.6).
      await clickAt(
        tester,
        screenPoint(tester, const Offset(0, -1000)),
        PointerDeviceKind.mouse,
      );
      expect(painter(tester).selected, ids[0]);

      await modifierDragFrom(
        tester,
        screenPoint(tester, const Offset(0, 1000)),
        screenPoint(tester, const Offset(300, 1000)) -
            screenPoint(tester, const Offset(0, 1000)),
      );

      expect(
        painter(tester).selectedIds,
        {ids[0]},
        reason:
            'a modifier-drag whose hit target is not the primary emits no '
            'edit at all (R7.2.6); it must not be reclassified as a marquee',
      );
      expect(painter(tester).selected, ids[0]);
      expect(walls(tester), 2);
      expect(dirty(), isFalse);
      expect(tester.takeException(), isNull);
    },
  );

  // ---- R7.3.2a, R7.4 and R7.5 completion (issue 0059) ----

  /// Fixture L13's Result B, through the real gesture. Result A (coincident
  /// corners select the join alone) is NOT reachable here and is deliberately
  /// not faked: `_marqueeRect` requires a classified drag, the classification
  /// threshold is 5 logical pixels, and `toPlan` is affine with a positive
  /// zoom, so two screen positions far enough apart to classify can never
  /// produce one plan point. Result A is covered in the core, by
  /// `a_line_rectangle_keeps_rectangle_semantics`, which asserts both halves
  /// of L13 against the F2 fixture.
  testWidgets(
    'L13 Result B — a modifier-drag along a line selects every wall it '
    'crosses, not one entity at its corner',
    (tester) async {
      await start(tester);
      final ids = await openWalls(tester, [
        (const Offset(-500, -1000), const Offset(-500, 1000)),
        (const Offset(500, -1000), const Offset(500, 1000)),
      ]);
      final from = screenPoint(tester, const Offset(-900, 0));
      final to = screenPoint(tester, const Offset(900, 0));
      final canvasRect = tester.getRect(getCanvas());
      expect(canvasRect.contains(from), isTrue);
      expect(canvasRect.contains(to), isTrue);
      expect(
        from.dy,
        to.dy,
        reason: 'the band is flat in SCREEN space, so it is flat in plan too',
      );

      // The band's own min corner is empty canvas. Point-hit semantics there
      // would select NOTHING, so this fixture separates R7.3.2a's reading
      // from the one that treats any zero-AREA rectangle as a point.
      await clickAt(tester, from, PointerDeviceKind.mouse);
      expect(painter(tester).selectedIds, isEmpty);

      await tester.sendKeyDownEvent(LogicalKeyboardKey.controlLeft);
      final drag = await tester.startGesture(
        from,
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveTo(to);
      await tester.pump();
      final band = painter(tester).marqueeRect!;
      expect(
        band.$1.dy,
        band.$2.dy,
        reason: 'zero height in plan millimetres: a line, and zero area',
      );
      expect(band.$1.dx, isNot(band.$2.dx), reason: 'but two distinct corners');
      await drag.up();
      await tester.sendKeyUpEvent(LogicalKeyboardKey.controlLeft);
      await tester.pumpAndSettle();

      expect(
        painter(tester).selectedIds,
        ids.toSet(),
        reason:
            'R7.3.2a: a line rectangle keeps R7.3.1 intersection semantics '
            'and selects both walls it crosses',
      );
      // Ascending marquee key orders by min-corner x here: the wall at
      // x = +500 sorts last and becomes primary (R7.3.2).
      expect(painter(tester).selected, ids[1]);
      expect(walls(tester), 2, reason: 'a marquee never edits the document');
      expect(dirty(), isFalse);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'R7.4 after a marquee: deleting the primary unsets it, undo restores the '
    'wall but not the selection, and idle Escape clears',
    (tester) async {
      await start(tester);
      final ids = await parallelWalls(tester);
      await modifierDragFrom(
        tester,
        screenPoint(tester, const Offset(-1100, -100)),
        screenPoint(tester, const Offset(1100, 1100)) -
            screenPoint(tester, const Offset(-1100, -100)),
      );
      expect(painter(tester).selectedIds, ids.toSet());
      expect(
        painter(tester).selected,
        ids[1],
        reason: 'both boxes share a min-corner x, so the higher y sorts last',
      );

      // R7.4: deletion targets the PRIMARY only; the remaining member stays
      // selected and primary goes UNSET rather than moving to it (L5).
      await tester.sendKeyEvent(LogicalKeyboardKey.delete);
      await tester.pumpAndSettle();
      expect(painter(tester).walls.map((wall) => wall.id), [ids[0]]);
      expect(painter(tester).selectedIds, {ids[0]});
      expect(painter(tester).selected, isNull);

      // R7.4: undo re-creates the wall; selection is untouched by stack
      // execution, so the restored wall is NOT reselected and primary stays
      // unset (L5 again, reached from a marquee rather than from clicks).
      historyButton(tester, 'Undo').onPressed!();
      await tester.pumpAndSettle();
      expect(walls(tester), 2);
      expect(painter(tester).selectedIds, {ids[0]});
      expect(painter(tester).selected, isNull);

      // R7.4: Escape while idle clears the selection entirely.
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, isEmpty);
      expect(painter(tester).selected, isNull);
      expect(walls(tester), 2, reason: 'clearing a selection edits nothing');
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'a primary-targeted drag after a marquee keeps the whole selection and '
    'moves only the primary',
    (tester) async {
      await start(tester);
      final ids = await parallelWalls(tester);
      await modifierDragFrom(
        tester,
        screenPoint(tester, const Offset(-1100, -100)),
        screenPoint(tester, const Offset(1100, 1100)) -
            screenPoint(tester, const Offset(-1100, -100)),
      );
      expect(painter(tester).selectedIds, ids.toSet());
      expect(painter(tester).selected, ids[1]);
      final before = {for (final wall in painter(tester).walls) wall.id: wall};

      // R7.4, first row: the command targets the marquee's primary. The other
      // member is an unaffected witness, and v0.1 has no group command that
      // would move it too.
      final drag = await tester.startGesture(
        screenPoint(tester, const Offset(0, 1000)),
        kind: PointerDeviceKind.mouse,
      );
      await drag.moveBy(const Offset(40, 30));
      await drag.up();
      await tester.pumpAndSettle();

      expect(
        painter(tester).selectedIds,
        ids.toSet(),
        reason: 'selection is unchanged by its own primary-targeted command',
      );
      expect(painter(tester).selected, ids[1], reason: 'and primary is kept');
      final after = {for (final wall in painter(tester).walls) wall.id: wall};
      expect(after[ids[1]]!.start, isNot(before[ids[1]]!.start));
      expect(after[ids[0]]!.start, before[ids[0]]!.start);
      expect(after[ids[0]]!.end, before[ids[0]]!.end);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'marquee eligibility ignores level visibility flags, and a level switch '
    'clears what a marquee selected',
    (tester) async {
      await start(tester);
      final scene = await openLevelScene(tester);
      final [upper, lowerB, lowerA] = scene.levels;
      final activeWallIds = scene.walls[2].toSet();
      final from = screenPoint(tester, const Offset(-1100, -1600));
      final to = screenPoint(tester, const Offset(1100, 1600));

      // R7.5.1: the inactive levels are rendered at full strength here, not
      // ghosted, and the answer must not change for it.
      await chooseVisibility(tester, upper, LevelVisibility.visible);
      await chooseVisibility(tester, lowerB, LevelVisibility.visible);
      expect(
        painter(tester).backgroundLevels.every((level) => !level.ghosted),
        isTrue,
      );
      await modifierDragFrom(tester, from, to - from);
      expect(painter(tester).selectedIds, activeWallIds);

      // ... nor when they are hidden outright. Escape first, so the second
      // marquee is answered afresh rather than inheriting the first.
      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(painter(tester).selectedIds, isEmpty);
      await chooseVisibility(tester, upper, LevelVisibility.hidden);
      await chooseVisibility(tester, lowerB, LevelVisibility.hidden);
      expect(painter(tester).backgroundLevels, isEmpty);
      await modifierDragFrom(tester, from, to - from);
      expect(
        painter(tester).selectedIds,
        activeWallIds,
        reason:
            'R7.5.1: hiddenness is the level-state model; eligibility is the '
            'ACTIVE level, whatever any flag says',
      );
      expect(painter(tester).selected, isNotNull);

      // R7.4: activating another level clears selection entirely (L7).
      await tap(tester, find.byKey(ValueKey('level-$upper')));
      expect(painter(tester).selectedIds, isEmpty);
      expect(painter(tester).selected, isNull);
      expect(lowerA, isNotEmpty);
      expect(dirty(), isFalse, reason: 'no document operation in any of this');
      expect(tester.takeException(), isNull);
    },
  );
}
