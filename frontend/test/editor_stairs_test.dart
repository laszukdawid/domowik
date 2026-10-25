import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';

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
  @override
  CoreReply edit(int handle, String request) {
    if ((jsonDecode(request) as Map)['kind'] == 'setStaircase') stairEdits++;
    return super.edit(handle, request);
  }

  @override
  CoreReply derived(int handle, String levelId) {
    derivedReads++;
    return super.derived(handle, levelId);
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
