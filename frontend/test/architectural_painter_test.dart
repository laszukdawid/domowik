import 'dart:ui' as ui;

import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'resize preview replaces the old stair on active and passive floors',
    (tester) async {
      PlanConstruction graphic(List<List<int>> outline) =>
          PlanConstruction.fromJson({
            'id': 'resizing-stair',
            'kind': 'staircase',
            'name': 'Synthetic resize',
            'outlineMm': outline,
            'holesMm': <Object?>[],
            'segmentsMm': <Object?>[],
            'labelMm': [500, 250],
            'direction': '',
          });
      final original = graphic([
        [0, 0],
        [1000, 0],
        [1000, 500],
        [0, 500],
      ]);
      final resized = graphic([
        [0, 200],
        [600, 200],
        [600, 500],
        [0, 500],
      ]);
      final derived = Derived(
        const [],
        const [],
        const [],
        0,
        construction: [original],
      );
      final data = await tester.runAsync(() async {
        final recorder = ui.PictureRecorder();
        final canvas = Canvas(recorder)..drawColor(Colors.white, BlendMode.src);
        PlanPainter(
          const [],
          derived,
          original.id,
          null,
          selectedIds: {original.id},
          resizingStairId: original.id,
          constructionPreview: [resized],
          backgroundLevels: [
            PlanLayer(
              levelId: 'other-floor',
              walls: const [],
              derived: derived,
            ),
          ],
          view: PlanViewport(zoom: 0.1, origin: const Offset(40, 140)),
          showGrid: false,
        ).paint(canvas, const Size(300, 200));
        final picture = recorder.endRecording();
        final image = await picture.toImage(300, 200);
        final bytes = await image.toByteData();
        image.dispose();
        picture.dispose();
        return bytes!;
      });
      List<int> pixel(int x, int y) => [
        for (var c = 0; c < 3; c++) data!.getUint8((y * 300 + x) * 4 + c),
      ];
      expect(pixel(125, 120), [255, 255, 255]);
      expect(pixel(80, 100), isNot([255, 255, 255]));
      expect(tester.takeException(), isNull);
    },
  );
  testWidgets(
    'selected stair preview follows plan Y-up and leaves no passive copy behind',
    (tester) async {
      final stair = PlanConstruction.fromJson({
        'id': 'synthetic-stair',
        'kind': 'staircase',
        'name': 'Synthetic stair',
        'outlineMm': [
          [0, 0],
          [1000, 0],
          [1000, 500],
          [0, 500],
        ],
        'holesMm': <Object?>[],
        'segmentsMm': <Object?>[],
        'labelMm': [500, 250],
        'direction': '',
      });
      final derived = Derived(
        const [],
        const [],
        const [],
        0,
        construction: [stair],
      );
      final data = await tester.runAsync(() async {
        final recorder = ui.PictureRecorder();
        final canvas = Canvas(recorder)..drawColor(Colors.white, BlendMode.src);
        PlanPainter(
          const [],
          derived,
          stair.id,
          null,
          selectedIds: {stair.id},
          backgroundLevels: [
            PlanLayer(
              levelId: 'other-floor',
              walls: const [],
              derived: derived,
              ghosted: false,
            ),
          ],
          groupPreview: ({stair.id}, const Offset(500, 400)),
          view: PlanViewport(zoom: 0.1, origin: const Offset(40, 140)),
          showGrid: false,
        ).paint(canvas, const Size(300, 200));
        final picture = recorder.endRecording();
        final image = await picture.toImage(300, 200);
        final bytes = await image.toByteData();
        image.dispose();
        picture.dispose();
        return bytes!;
      });
      List<int> pixel(int x, int y) => [
        for (var c = 0; c < 3; c++) data!.getUint8((y * 300 + x) * 4 + c),
      ];
      expect(pixel(50, 120), [
        255,
        255,
        255,
      ]); // Old footprint, including passive floor.
      final highlighted = pixel(120, 75); // +X right and +Y upward.
      expect(highlighted[0], greaterThan(highlighted[1]));
      expect(highlighted, isNot([255, 255, 255]));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'paints supplied floor cuts and architectural outlines without deriving them',
    (tester) async {
      final body = PlanConstruction.fromJson({
        'id': 'synthetic',
        'kind': 'body',
        'name': 'Original test platform',
        'outlineMm': [
          [0, 0],
          [2000, 0],
          [2000, 1000],
          [0, 1000],
        ],
        'holesMm': [
          [
            [500, 250],
            [500, 750],
            [1500, 750],
            [1500, 250],
          ],
        ],
        'segmentsMm': <Object?>[],
        'labelMm': [1000, 500],
        'direction': '',
      });
      expect(body.outline[1], const Offset(2000, 0));
      final data = await tester.runAsync(() async {
        final recorder = ui.PictureRecorder();
        final canvas = Canvas(recorder);
        canvas.drawColor(Colors.white, BlendMode.src);
        PlanPainter(
          const [],
          Derived(
            const [],
            const [],
            const [],
            0,
            construction: [body],
            floorSurfaces: const [],
          ),
          null,
          null,
          view: PlanViewport(zoom: 0.1, origin: const Offset(40, 140)),
          showGrid: false,
        ).paint(canvas, const Size(300, 200));
        final picture = recorder.endRecording();
        final image = await picture.toImage(300, 200);
        final bytes = await image.toByteData();
        image.dispose();
        picture.dispose();
        return bytes!;
      });
      List<int> pixel(int x, int y) => [
        for (var c = 0; c < 3; c++) data!.getUint8((y * 300 + x) * 4 + c),
      ];
      expect(pixel(60, 90), [0xe6, 0xdf, 0xd2]);
      expect(pixel(140, 90), [255, 255, 255]);
      expect(tester.takeException(), isNull);
    },
  );
}
