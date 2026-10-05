import 'dart:io';
import 'dart:ui' as ui;

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'recursive model discovery handles spaces, mixed case, missing paths and symlinks',
    () async {
      final root = await Directory.systemTemp.createTemp('domowik-model-scan-');
      addTearDown(() => root.delete(recursive: true));
      final nested = await Directory('${root.path}/furniture').create();
      await File('${root.path}/chair.GLB').writeAsString('original model');
      await File(
        '${nested.path}/table with spaces.zip',
      ).writeAsString('original bundle');
      await File('${nested.path}/sofa.obj').writeAsString('original mesh');
      await File('${nested.path}/notes.txt').writeAsString('not a model');
      await Link('${nested.path}/loop').create(root.path);
      final result = await scanAssetDirectories([
        root.path,
        '${root.path}/missing',
      ]);
      expect(
        result.models.map((m) => m.name),
        containsAll(['chair', 'table with spaces', 'sofa']),
      );
      expect(result.models, hasLength(3));
      expect(result.diagnostics.single, contains('missing'));
    },
  );

  testWidgets(
    'directory library places, edits, moves, saves and reopens a model without its source',
    (tester) async {
      final root = Directory.systemTemp.createTempSync(
        'domowik-editor-assets-',
      );
      final libraryPath = root.resolveSymbolicLinksSync();
      File('${root.path}/original sofa.obj').writeAsStringSync(
        'v 0 1 0\nv 3 1 0\nv 3 1 1\nv 1 1 1\nv 1 1 3\nv 0 1 3\nf 1 2 3 4 5 6\n',
      );
      final output = File('${root.path}.domo');
      final configFile = File('${root.path}.config.json');
      addTearDown(() {
        if (root.existsSync()) root.deleteSync(recursive: true);
        if (output.existsSync()) output.deleteSync();
        if (configFile.existsSync()) configFile.deleteSync();
      });
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final bridge = Bridge(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      );
      const files = MethodChannel('invalid.domowik-spike/files');
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(files, (
        call,
      ) async {
        if (call.method == 'savePlan' || call.method == 'openPlan')
          return output.path;
        if (call.method == 'pickAssetDirectory') return root.path;
        return null;
      });
      addTearDown(
        () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          files,
          null,
        ),
      );
      await tester.binding.setSurfaceSize(const Size(1400, 900));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.runAsync(() async {
        await tester.pumpWidget(
          MaterialApp(
            home: PlanPage(
              bridgeLoader: () => bridge,
              configStore: UserConfigStore(configFile),
            ),
          ),
        );
        await Future<void>.delayed(const Duration(milliseconds: 150));
      });
      await tester.pumpAndSettle();
      addTearDown(() async {
        await tester.pumpWidget(const SizedBox.shrink());
        expect(bridge.openPlans(), 0);
        expect(bridge.outstandingBuffers(), 0);
      });

      Finder canvas() => find.byWidgetPredicate(
        (w) => w is CustomPaint && w.painter is PlanPainter,
      );
      PlanPainter scene() =>
          tester.widget<CustomPaint>(canvas()).painter! as PlanPainter;
      Future<void> tap(Finder f) async {
        await tester.pump();
        await tester.runAsync(() async {
          await tester.tap(f);
          await tester.pump();
          await Future<void>.delayed(const Duration(milliseconds: 150));
        });
        await tester.pumpAndSettle();
      }

      Future<void> io() async {
        await tester.runAsync(
          () async => Future<void>.delayed(const Duration(milliseconds: 150)),
        );
        await tester.pumpAndSettle();
      }

      Future<void> click(Offset at) async {
        await tester.runAsync(() async {
          final g = await tester.startGesture(
            at,
            kind: PointerDeviceKind.mouse,
          );
          await g.up();
          await Future<void>.delayed(const Duration(milliseconds: 150));
        });
        await io();
      }

      Future<void> inspector(Finder f) async {
        await tester.scrollUntilVisible(
          f,
          300,
          scrollable: find
              .descendant(
                of: find.byType(ListView),
                matching: find.byType(Scrollable),
              )
              .last,
        );
        await tap(f);
      }

      await tap(find.byKey(const ValueKey('asset-library')));
      await io();
      await tap(find.byKey(const ValueKey('attach-asset-directory')));
      await io();
      expect(find.text('original sofa'), findsOneWidget);
      expect(configFile.existsSync(), isTrue);
      expect(
        find.textContaining('Unsaved'),
        findsNothing,
        reason: 'global library preferences do not dirty the plan',
      );
      await tap(find.text('original sofa'));
      expect(find.byKey(const ValueKey('asset-width')), findsNothing);
      expect(find.byKey(const ValueKey('asset-depth')), findsNothing);
      await tap(find.byKey(const ValueKey('choose-asset')));
      await click(tester.getTopLeft(canvas()) + const Offset(320, 360));
      expect(scene().assets, hasLength(1));
      final initial = scene().assets.single;
      expect(initial.widthMm, 1000);
      expect(initial.depthMm, 1000);
      expect(initial.heightMm, 1);
      Future<void> contextMenu() async {
        final gesture = await tester.startGesture(
          tester.getTopLeft(canvas()) + scene().toScreen(initial.position),
          kind: PointerDeviceKind.mouse,
          buttons: kSecondaryMouseButton,
        );
        await gesture.up();
        await tester.pumpAndSettle();
      }

      await contextMenu();
      await tap(find.text('Select').last);
      expect(scene().selected, initial.id);
      await contextMenu();
      await tap(find.text('Resize…'));
      await tester.enterText(
        find.byKey(const ValueKey('resize-asset-0')),
        '2000',
      );
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('resize-asset-1')))
            .controller!
            .text,
        '2000.0',
      );
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('resize-asset-2')))
            .controller!
            .text,
        '2.0',
      );
      await tap(find.text('Cancel'));
      expect(scene().assets.single.widthMm, 1000);
      await contextMenu();
      await tap(find.text('Resize…'));
      await tap(find.byKey(const ValueKey('resize-lock-proportions')));
      await tester.enterText(
        find.byKey(const ValueKey('resize-asset-0')),
        '2000',
      );
      await tester.enterText(
        find.byKey(const ValueKey('resize-asset-1')),
        '800',
      );
      await tester.enterText(
        find.byKey(const ValueKey('resize-asset-2')),
        '600',
      );
      await tap(find.byKey(const ValueKey('apply-asset-resize')));
      expect(scene().assets.single.heightMm, 600);
      expect(scene().assets.single.position, initial.position);
      await tap(find.byTooltip('Undo'));
      expect(scene().assets.single.widthMm, 1000);
      expect(scene().assets.single.heightMm, 1);
      await tap(find.byTooltip('Redo'));
      final placed = scene().assets.single;
      expect(placed.widthMm, 2000);
      expect(placed.depthMm, 800);
      expect(scene().selected, placed.id);
      expect(find.text('Make room for your ideas'), findsNothing);
      await io();
      final modelImage = scene().modelPreviews[placed.digest];
      expect(
        modelImage,
        isNotNull,
        reason: 'decoded model art reaches the painter',
      );
      await tester.runAsync(() async {
        final recorder = ui.PictureRecorder();
        final size = tester.getSize(canvas());
        final paint = PlanPainter(
          const [],
          Derived.empty,
          null,
          null,
          view: scene().view,
          showGrid: false,
          assets: scene().assets,
          modelPreviews: scene().modelPreviews,
        );
        paint.paint(Canvas(recorder), size);
        final picture = recorder.endRecording();
        final raster = await picture.toImage(
          size.width.ceil(),
          size.height.ceil(),
        );
        final rgba = (await raster.toByteData(
          format: ui.ImageByteFormat.rawRgba,
        ))!;
        final gap = scene().toScreen(
          placed.position + Offset(placed.widthMm * 0.3, -placed.depthMm * 0.3),
        );
        final filled = scene().toScreen(
          placed.position +
              Offset(-placed.widthMm * 0.3, -placed.depthMm * 0.3),
        );
        int alpha(Offset p) =>
            rgba.getUint8((p.dy.floor() * raster.width + p.dx.floor()) * 4 + 3);
        expect(
          alpha(gap),
          0,
          reason: 'the model gap is not covered by a footprint rectangle',
        );
        expect(
          alpha(filled),
          greaterThan(0),
          reason: 'actual object pixels are drawn',
        );
        raster.dispose();
        picture.dispose();
      });

      await inspector(find.byKey(const ValueKey('edit-asset')));
      await tester.enterText(
        find.byKey(const ValueKey('asset-property-4')),
        '90',
      );
      await tap(find.byKey(const ValueKey('apply-asset-properties')));
      expect(scene().assets.single.rotationDeg, 90);
      expect(scene().assets.single.heightMm, 600);
      expect(
        identical(scene().modelPreviews[placed.digest], modelImage),
        isTrue,
        reason: 'rotation reuses decoded art',
      );
      await tap(find.byTooltip('Undo'));
      expect(scene().assets.single.rotationDeg, 0);
      await tap(find.byTooltip('Redo'));
      expect(scene().assets.single.rotationDeg, 90);
      expect(scene().assets.single.widthMm, 2000);
      expect(scene().assets.single.depthMm, 800);
      expect(scene().assets.single.heightMm, 600);

      final before = scene().assets.single.position;
      await tester.dragFrom(
        tester.getTopLeft(canvas()) + scene().toScreen(before),
        const Offset(40, 20),
      );
      await tester.pumpAndSettle();
      expect(scene().assets.single.position, isNot(before));
      await inspector(find.byKey(const ValueKey('delete-asset')));
      expect(scene().assets, isEmpty);
      await tap(find.byTooltip('Undo'));
      expect(scene().assets, hasLength(1));

      await tap(find.text('Save'));
      expect(output.existsSync(), isTrue);
      root.deleteSync(recursive: true);
      await tap(find.byTooltip('Open plan'));
      expect(scene().assets, hasLength(1));
      expect(scene().assets.single.rotationDeg, 90);
      await tap(find.byKey(const ValueKey('asset-library')));
      await io();
      expect(find.text('Embedded in this plan'), findsOneWidget);
      expect(find.textContaining('Cannot read'), findsOneWidget);
      await tap(find.text('Close'));
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(() async {
        await tester.pumpWidget(
          MaterialApp(
            home: PlanPage(
              bridgeLoader: () => bridge,
              configStore: UserConfigStore(configFile),
            ),
          ),
        );
        await Future<void>.delayed(const Duration(milliseconds: 150));
      });
      await tester.pumpAndSettle();
      expect(
        scene().assets,
        isEmpty,
        reason: 'new session does not reuse plan content',
      );
      await tap(find.byKey(const ValueKey('asset-library')));
      await io();
      expect(
        tester
            .widget<AssetLibraryDialog>(find.byType(AssetLibraryDialog))
            .directories,
        [libraryPath],
        reason: 'global library survives application restart',
      );
      await tap(find.text('Close'));
    },
  );
}
