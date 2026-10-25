import 'dart:ui' as ui;
import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
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
