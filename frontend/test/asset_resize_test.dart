import 'dart:io';
import 'dart:ui' as ui;

import 'package:domowik_editor/asset_library.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'compact rounded dialog previews changes and preserves untouched precision',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final name =
          'TOLKNING bench with storage - handmade rattan ${'long descriptive name ' * 3}';
      final asset = AssetFootprint.fromJson({
        'id': 'original',
        'levelId': 'level',
        'digest': 'digest',
        'name': name,
        'positionMm': [10, 20],
        'dimensionsMm': [1200.0000476837158, 369.96886134147644],
        'heightMm': 450.02747102582805,
        'rotationDeg': 30,
        'cornersMm': <List<num>>[],
      });
      final recorder = ui.PictureRecorder();
      final canvas = Canvas(recorder);
      canvas.drawRect(
        const Rect.fromLTWH(4, 8, 56, 48),
        Paint()..color = Colors.brown,
      );
      final picture = recorder.endRecording();
      final image = await tester.runAsync(() => picture.toImage(64, 64));
      picture.dispose();
      final capture = GlobalKey();
      final fontPath = Platform.environment['DOMOWIK_RESIZE_TEST_FONT'];
      if (fontPath != null) {
        await tester.runAsync(() async {
          final bytes = await File(fontPath).readAsBytes();
          final loader = FontLoader('ResizeTestFont')
            ..addFont(Future.value(ByteData.sublistView(bytes)));
          await loader.load();
        });
      }
      List<double>? applied;
      await tester.pumpWidget(
        RepaintBoundary(
          key: capture,
          child: MaterialApp(
            theme: ThemeData(
              fontFamily: fontPath == null ? null : 'ResizeTestFont',
            ),
            home: Scaffold(
              body: Builder(
                builder: (context) => TextButton(
                  onPressed: () async {
                    applied = await showDialog<List<double>>(
                      context: context,
                      builder: (_) => ResizeAssetDialog(
                        asset: asset,
                        heightMm: asset.heightMm!,
                        preview: image,
                      ),
                    );
                  },
                  child: const Text('Resize'),
                ),
              ),
            ),
          ),
        ),
      );
      Future<void> open() async {
        await tester.tap(find.text('Resize'));
        await tester.pumpAndSettle();
      }

      Finder field(int i) => find.byKey(ValueKey('resize-asset-$i'));
      ResizePreviewPainter painter() =>
          tester
                  .widget<CustomPaint>(
                    find.descendant(
                      of: find.byKey(const ValueKey('resize-preview')),
                      matching: find.byType(CustomPaint),
                    ),
                  )
                  .painter!
              as ResizePreviewPainter;
      await open();
      final surface = find
          .descendant(
            of: find.byType(AlertDialog),
            matching: find.byType(Material),
          )
          .first;
      expect(tester.getSize(surface).width, lessThanOrEqualTo(420));
      expect(
        find.text('Resize ${String.fromCharCodes(name.runes.take(49))}…'),
        findsOneWidget,
      );
      expect(tester.widget<TextField>(field(0)).controller!.text, '1200.0');
      expect(tester.widget<TextField>(field(1)).controller!.text, '370.0');
      expect(tester.widget<TextField>(field(2)).controller!.text, '450.0');
      expect(painter().image, same(image));
      final original = painter();
      final size = tester.getSize(find.byKey(const ValueKey('resize-preview')));
      Future<List<int>> pixels() async {
        final boundary = tester.renderObject<RenderRepaintBoundary>(
          find.byKey(const ValueKey('resize-preview-pixels')),
        );
        return (await tester.runAsync(() async {
          final snapshot = await boundary.toImage();
          final data = await snapshot.toByteData(
            format: ui.ImageByteFormat.rawRgba,
          );
          final result = data!.buffer.asUint8List().toList();
          snapshot.dispose();
          return result;
        }))!;
      }

      final originalPixels = await pixels();
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: Offset.zero);
      await mouse.moveTo(
        tester.getCenter(
          find.text('Resize ${String.fromCharCodes(name.runes.take(49))}…'),
        ),
      );
      await tester.pump(const Duration(seconds: 1));
      expect(find.text(name), findsOneWidget);
      await mouse.removePointer();
      await tester.pumpAndSettle();
      final output = Platform.environment['DOMOWIK_RESIZE_SCREENSHOT'];
      if (output != null) {
        final boundary =
            capture.currentContext!.findRenderObject() as RenderRepaintBoundary;
        await tester.runAsync(() async {
          final rendered = await boundary.toImage();
          final bytes = await rendered.toByteData(
            format: ui.ImageByteFormat.png,
          );
          await File(output).writeAsBytes(bytes!.buffer.asUint8List());
          rendered.dispose();
        });
      }
      await tester.enterText(field(0), '1800');
      await tester.pump();
      expect(
        await pixels(),
        isNot(originalPixels),
        reason: 'the rendered preview changes, not only its values',
      );
      expect(
        painter().footprint(size, painter().dimensions).width,
        greaterThan(original.footprint(size, original.dimensions).width),
      );
      expect(
        painter().heightPixels(size, painter().dimensions[2]),
        greaterThan(original.heightPixels(size, original.dimensions[2])),
      );
      expect(
        tester.widget<TextField>(field(1)).controller!.text,
        (asset.depthMm * 1800 / asset.widthMm).toStringAsFixed(1),
      );
      final validPreview = List.of(painter().dimensions);
      await tester.enterText(field(0), '1800.1234');
      await tester.pump();
      expect(tester.widget<TextField>(field(0)).controller!.text, '1800.1');
      final roundedPreview = List.of(painter().dimensions);
      await tester.enterText(field(0), 'invalid');
      await tester.pump();
      expect(painter().dimensions, roundedPreview);
      expect(validPreview[0], 1800);
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('apply-asset-resize')),
            )
            .onPressed,
        isNull,
      );
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(applied, isNull);
      await open();
      await tester.tap(find.text('Apply'));
      await tester.pumpAndSettle();
      expect(applied, [asset.widthMm, asset.depthMm, asset.heightMm]);
      await tester.binding.setSurfaceSize(const Size(390, 700));
      await tester.pump();
      await open();
      expect(tester.getSize(surface).width, lessThanOrEqualTo(350));
      expect(tester.takeException(), isNull);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      await tester.pumpWidget(const SizedBox.shrink());
      image!.dispose();
    },
  );
  testWidgets(
    'resize locks all three axes, supports independent size and rejects invalid values',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1000, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final asset = AssetFootprint.fromJson({
        'id': 'original',
        'levelId': 'level',
        'digest': 'digest',
        'name': 'Object',
        'positionMm': [10, 20],
        'dimensionsMm': [2000, 500],
        'heightMm': 1000,
        'rotationDeg': 30,
        'cornersMm': <List<num>>[],
      });
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(body: ResizeAssetDialog(asset: asset, heightMm: 1000)),
        ),
      );
      Finder field(int i) => find.byKey(ValueKey('resize-asset-$i'));
      double value(int i) =>
          double.parse(tester.widget<TextField>(field(i)).controller!.text);
      await tester.enterText(field(2), '1500');
      await tester.pump();
      expect([value(0), value(1), value(2)], [3000, 750, 1500]);
      await tester.tap(find.byKey(const ValueKey('resize-lock-proportions')));
      await tester.pump();
      await tester.enterText(field(1), '1000');
      await tester.pump();
      expect([value(0), value(1), value(2)], [3000, 1000, 1500]);
      await tester.tap(find.byKey(const ValueKey('resize-lock-proportions')));
      await tester.pump();
      await tester.enterText(field(0), '6000');
      await tester.pump();
      expect([value(0), value(1), value(2)], [6000, 2000, 3000]);
      for (final invalid in ['0', '-1', 'NaN', 'Infinity', '1000001']) {
        await tester.enterText(field(0), invalid);
        await tester.pump();
        expect(
          tester
              .widget<FilledButton>(
                find.byKey(const ValueKey('apply-asset-resize')),
              )
              .onPressed,
          isNull,
        );
      }
      await tester.enterText(field(0), '6000');
      await tester.pump();
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('apply-asset-resize')),
            )
            .onPressed,
        isNotNull,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
