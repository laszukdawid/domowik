import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

/// Naming a room from the editor (BEHAVIOR-SPEC §5.2/§5.3, issue 0060).
///
/// Against the REAL library, because the point of these is the boundary:
/// ED-0101 says the editor chooses what the user is trying to do and the core
/// decides what it means, and a mocked bridge would let the editor decide
/// anything it liked.
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

  Future<void> click(WidgetTester tester, Offset point) async {
    await tester.tapAt(tester.getTopLeft(canvas()) + point);
    await tester.pumpAndSettle();
  }

  /// Draws a closed rectangle with the chain tool, which is how a person makes
  /// a room. The last click lands back on the first corner and closes it.
  Future<void> drawARoom(WidgetTester tester) async {
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    for (final corner in const [
      Offset(200, 240),
      Offset(500, 240),
      Offset(500, 480),
      Offset(200, 480),
      Offset(200, 240),
    ]) {
      await click(tester, corner);
    }
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    expect(
      painter(tester).derived.rooms,
      hasLength(1),
      reason: 'four joined walls enclose one face',
    );
    await tester.tap(find.byKey(const ValueKey('inspector-floor-tab')));
    await tester.pumpAndSettle();
  }

  Future<void> nameIt(WidgetTester tester, String name) async {
    await tester.tap(find.byKey(const ValueKey('name-room-0')));
    await tester.pumpAndSettle();
    await tester.enterText(find.byKey(const ValueKey('room-name-field')), name);
    await tester.tap(find.byKey(const ValueKey('room-name-save')));
    await tester.pumpAndSettle();
  }

  testWidgets('an unnamed face offers naming and shows no label', (
    tester,
  ) async {
    await start(tester);
    await drawARoom(tester);
    final room = painter(tester).derived.rooms.single;
    // R5.3.3 absence: no record, no label, and no area either — the area
    // ACCOMPANIES the label rather than standing alone.
    expect(room.recordId, isNull);
    expect(room.label, isNull);
    expect(room.labelAnchorMm, isNull);
    // But the core has supplied somewhere to put one.
    expect(room.seedCandidateMm, isNotNull);
    expect(find.text('Unnamed room'), findsOneWidget);
    expect(find.byKey(const ValueKey('name-room-0')), findsOneWidget);
  });

  testWidgets('naming a room shows the name and the core-rounded area', (
    tester,
  ) async {
    await start(tester);
    await drawARoom(tester);
    await nameIt(tester, 'Salon');

    final room = painter(tester).derived.rooms.single;
    expect(room.label, 'Salon');
    expect(room.recordId, isNotNull);
    expect(room.displayAreaM2, isNotNull);
    expect(find.text('Salon'), findsWidgets);

    // The record exists on the core side with the status the CORE decided.
    final records = painter(tester).derived.roomRecords;
    expect(records, hasLength(1));
    expect(records.single.name, 'Salon');
    expect(
      records.single.status,
      'associated',
      reason: 'status is derived there, not computed here',
    );

    // And the label sits where the core put it: at the record's seed.
    expect(room.labelAnchorMm, records.single.seedMm);
  });

  testWidgets('the seed comes from the core, not from the editor', (
    tester,
  ) async {
    await start(tester);
    await drawARoom(tester);
    final candidate = painter(tester).derived.rooms.single.seedCandidateMm!;
    await nameIt(tester, 'Salon');
    expect(
      painter(tester).derived.roomRecords.single.seedMm,
      candidate,
      reason:
          'the editor sent the point the core offered; picking one here would '
          'be a containment test in the view (ED-0101)',
    );
  });

  testWidgets('renaming edits the existing record rather than adding one', (
    tester,
  ) async {
    await start(tester);
    await drawARoom(tester);
    await nameIt(tester, 'Salon');
    final id = painter(tester).derived.roomRecords.single.id;

    await nameIt(tester, 'Kuchnia');
    final records = painter(tester).derived.roomRecords;
    expect(records, hasLength(1), reason: 'one record, renamed');
    expect(records.single.id, id);
    expect(records.single.name, 'Kuchnia');
    expect(painter(tester).derived.rooms.single.label, 'Kuchnia');
  });

  testWidgets('an empty name is a legal name and displays as the fallback', (
    tester,
  ) async {
    await start(tester);
    await drawARoom(tester);
    await nameIt(tester, '');
    final records = painter(tester).derived.roomRecords;
    expect(records, hasLength(1));
    // F37d: the empty string is a VALUE, distinct from absent, and both show
    // as `Room`. The editor must not turn one into the other.
    expect(records.single.name, '');
    expect(painter(tester).derived.rooms.single.label, 'Room');
  });

  testWidgets('cancelling the dialog changes nothing', (tester) async {
    await start(tester);
    await drawARoom(tester);
    await tester.tap(find.byKey(const ValueKey('name-room-0')));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('room-name-field')),
      'Salon',
    );
    await tester.tap(find.byKey(const ValueKey('room-name-cancel')));
    await tester.pumpAndSettle();
    expect(painter(tester).derived.roomRecords, isEmpty);
    expect(painter(tester).derived.rooms.single.label, isNull);
  });

  testWidgets('undo removes the record the naming created', (tester) async {
    await start(tester);
    await drawARoom(tester);
    await nameIt(tester, 'Salon');
    await tester.tap(find.byTooltip('Undo'));
    await tester.pumpAndSettle();
    expect(painter(tester).derived.roomRecords, isEmpty);
    expect(
      painter(tester).derived.rooms,
      hasLength(1),
      reason: 'the FACE survives; only the record went (R5.1.8)',
    );
  });
}
