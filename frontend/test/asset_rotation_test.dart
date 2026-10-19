import 'dart:math' as math;

import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/asset_rotation.dart';
import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('pointer yaw has the plan sign and no grab offset jump', () {
    final drag = AssetRotationDrag(
      center: const Offset(100, 200),
      start: const Offset(100, 1200),
      angleDeg: 30,
    );
    expect(drag.update(const Offset(100, 1200), zoom: 0.1), 30);
    expect(drag.update(const Offset(-900, 200), zoom: 0.1), 120);
    expect(drag.update(const Offset(100, -800), zoom: 0.1), 210);
    expect(drag.update(const Offset(1100, 200), zoom: 0.1), 300);
    expect(drag.update(const Offset(100, 1200), zoom: 0.1), 390);
  });

  test('clockwise and counter-clockwise branch crossings stay continuous', () {
    Offset at(double degrees) => Offset(
      math.cos(degrees * math.pi / 180) * 1000,
      math.sin(degrees * math.pi / 180) * 1000,
    );
    final drag = AssetRotationDrag(
      center: Offset.zero,
      start: at(179),
      angleDeg: 20,
    );
    expect(drag.update(at(-179), zoom: 0.1), 22);
    expect(drag.update(at(179), zoom: 0.1), 20);
    expect(drag.update(Offset.zero, zoom: 0.1), 20);
    expect(drag.update(const Offset(0.01, 0.01), zoom: 0.1), 20);
    expect(drag.update(at(178), zoom: 0.1), 19);
  });

  test('Shift snaps absolute angles, including negatives and half steps', () {
    for (final (raw, snapped) in [
      (44.0, 45.0),
      (88.0, 90.0),
      (178.0, 180.0),
      (-44.0, -45.0),
      (-88.0, -90.0),
      (-178.0, -180.0),
      (359.0, 360.0),
      (7.5, 15.0),
      (-7.5, -15.0),
    ]) {
      final drag = AssetRotationDrag(
        center: Offset.zero,
        start: const Offset(1000, 0),
        angleDeg: raw,
      );
      for (var i = 0; i < 3; i++) {
        expect(
          drag.update(const Offset(1000, 0), zoom: 0.1, snapTo15Degrees: true),
          snapped,
        );
        expect(drag.update(const Offset(1000, 0), zoom: 0.1), raw);
      }
    }
  });

  test(
    'snapped motion preserves raw angle across steps and the branch cut',
    () {
      Offset at(double degrees) => Offset(
        math.cos(degrees * math.pi / 180) * 1000,
        math.sin(degrees * math.pi / 180) * 1000,
      );
      final drag = AssetRotationDrag(
        center: Offset.zero,
        start: at(0),
        angleDeg: 13,
      );
      for (final (heading, expected) in [
        (31.0, 45.0),
        (75.0, 90.0),
        (165.0, 180.0),
        (179.0, 195.0),
        (-179.0, 195.0),
      ]) {
        expect(
          drag.update(at(heading), zoom: 0.1, snapTo15Degrees: true),
          expected,
        );
      }
      expect(drag.update(at(-179), zoom: 0.1), 194);
      expect(drag.update(at(-178), zoom: 0.1), 195);
    },
  );

  test('handle stays outside native footprint with a logical-pixel gap', () {
    final asset = AssetFootprint.fromJson({
      'id': 'object',
      'levelId': 'level',
      'digest': 'digest',
      'name': 'Synthetic object',
      'positionMm': [100, 200],
      'dimensionsMm': [600, 800],
      'rotationDeg': 90,
      'cornersMm': [
        [500, -100],
        [500, 500],
        [-300, 500],
        [-300, -100],
      ],
    });
    for (final zoom in [0.001, 0.1, 2.0]) {
      final view = PlanViewport(zoom: zoom, origin: const Offset(400, 300));
      final handle = assetRotationHandlePosition(asset, view);
      final delta = handle - view.toScreen(asset.position);
      expect(delta.dx, closeTo(-math.max(40, 400 * zoom + 28), 1e-8));
      expect(delta.dy, closeTo(0, 1e-8));
      final before = PlanPainter(
        const [],
        Derived.empty,
        asset.id,
        null,
        view: view,
        assets: [asset],
      );
      final active = PlanPainter(
        const [],
        Derived.empty,
        asset.id,
        null,
        view: view,
        assets: before.assets,
        rotatingAssetId: asset.id,
      );
      expect(before.assetRotationHandle, isNull);
      expect(active.assetRotationHandle, handle);
      expect(active.shouldRepaint(before), isTrue);
      expect(before.shouldRepaint(active), isTrue);
    }
  });

  testWidgets(
    'direct yaw updates controls without replacing other draft text',
    (tester) async {
      List<double>? applied;
      var previews = 0;
      Widget controls(double yaw) => MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: AssetPoseControls(
              initial: const [100, 200, 300, 10, 20, 30],
              rotationZDeg: yaw,
              onPreview: (_) => previews++,
              onApply: (values) => applied = values,
              onCancel: () {},
            ),
          ),
        ),
      );
      await tester.pumpWidget(controls(30));
      await tester.enterText(find.byKey(const ValueKey('asset-pose-0')), '125');
      await tester.pumpWidget(controls(120));
      final yaw = tester.widget<TextField>(
        find.byKey(const ValueKey('asset-pose-5')),
      );
      expect(yaw.controller!.text, '120.0');
      expect(previews, 1, reason: 'external yaw synchronization does not echo');
      final apply = find.byKey(const ValueKey('apply-asset-pose'));
      await tester.ensureVisible(apply);
      await tester.tap(apply);
      expect(applied, [125, 200, 300, 10, 20, 120]);
    },
  );
}
