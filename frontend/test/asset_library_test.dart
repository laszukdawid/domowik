import 'dart:io';
import 'dart:convert';
import 'dart:ui' as ui;

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/core/domowik_core.dart' show CoreReply;
import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/platform/config_storage_native.dart';
import 'package:domowik_editor/platform/model_import.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'directory import lazily finds nested model files and cancellation is inert',
    () async {
      if (!modelDirectoryImportAvailable) return;
      final root = await Directory.systemTemp.createTemp(
        'domowik-import-models-',
      );
      addTearDown(() => root.delete(recursive: true));
      final nested = await Directory(
        '${root.path}/room/chairs',
      ).create(recursive: true);
      await File('${root.path}/chair.GLB').writeAsBytes([1, 2, 3]);
      await File('${nested.path}/table with spaces.zip').writeAsBytes([4, 5]);
      await File('${nested.path}/notes.txt').writeAsString('not a model');

      const channel = MethodChannel('invalid.domowik-spike/files');
      String? selected;
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(channel, (call) async {
            expect(call.method, 'pickAssetDirectory');
            return selected;
          });
      addTearDown(
        () => TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
            .setMockMethodCallHandler(channel, null),
      );

      expect(await pickModelDirectory(), isNull, reason: 'cancel is unchanged');
      selected = root.path;
      final selection = (await pickModelDirectory())!;
      final imported = <String, List<int>>{};
      final sources = <String, String?>{};
      await for (final file in selection.files) {
        imported[file.name] = (await file.readBytes()).toList();
        sources[file.name] = file.sourcePath;
      }
      expect(imported, {
        'chair.GLB': [1, 2, 3],
        'table with spaces.zip': [4, 5],
      });
      expect(sources, {
        'chair.GLB': File('${root.path}/chair.GLB').absolute.path,
        'table with spaces.zip': File(
          '${nested.path}/table with spaces.zip',
        ).absolute.path,
      });
      expect(selection.diagnostics, isEmpty);
    },
  );

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
      final core = _CountingCore(bridge);
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
              bridgeLoader: () => core,
              configStore: UserConfigStore(FileConfigStorage(configFile)),
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
        await tester.ensureVisible(f);
        await tester.pumpAndSettle();
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
      // Placement resolves to the whole millimetres the panel will show,
      // never 3199.999…97-style screen-division debris.
      expect(
        initial.position.dx,
        initial.position.dx.roundToDouble(),
        reason: 'X placement commits at whole-millimetre resolution',
      );
      expect(
        initial.position.dy,
        initial.position.dy.roundToDouble(),
        reason: 'Y placement commits at whole-millimetre resolution',
      );
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

      await contextMenu();
      await tap(find.text('Rotate…'));
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('asset-pose-5')))
            .controller!
            .text,
        '0.0',
      );
      expect(
        num.tryParse(
          tester
              .widget<TextField>(find.byKey(const ValueKey('asset-pose-0')))
              .controller!
              .text,
        ),
        isA<int>(),
        reason: 'the X field shows whole millimetres, never float digits',
      );
      await inspector(find.byKey(const ValueKey('asset-pose-5')));
      final angleField = tester.widget<TextField>(
        find.byKey(const ValueKey('asset-pose-5')),
      );
      final previews = core.previews,
          reads = core.stateReads,
          lists = core.listReads;
      for (final angle in ['10', '20', '30']) {
        angleField.controller!.text = angle;
        angleField.onChanged!(angle);
      }
      expect(
        core.previews,
        previews,
        reason: 'input is queued until the next frame',
      );
      await tester.pump();
      expect(
        core.previews,
        previews + 1,
        reason: 'one frame sends only the latest angle',
      );
      expect(core.stateReads, reads);
      expect(
        core.listReads,
        lists,
        reason: 'preview response already supplies the 2D footprint',
      );
      expect(scene().assets.single.rotationDeg, 30);
      // Cancel before the next scheduled preview flush: stale input must not
      // resurrect an overlay after cancellation or add any history.
      angleField.controller!.text = '99';
      angleField.onChanged!('99');
      tester
          .widget<AssetPoseControls>(find.byType(AssetPoseControls))
          .onCancel();
      await tester.pumpAndSettle();
      expect(core.previews, previews + 1);
      expect(scene().assets.single.rotationDeg, 0);
      await contextMenu();
      await tap(find.text('Rotate…'));
      for (final invalid in ['', 'NaN', 'Infinity', 'not an angle']) {
        await inspector(find.byKey(const ValueKey('asset-pose-5')));
        await tester.enterText(
          find.byKey(const ValueKey('asset-pose-5')),
          invalid,
        );
        await tester.pumpAndSettle();
        expect(
          tester
              .widget<FilledButton>(
                find.byKey(const ValueKey('apply-asset-pose')),
              )
              .onPressed,
          isNull,
        );
      }
      await inspector(find.byKey(const ValueKey('cancel-asset-pose')));
      expect(scene().assets.single.rotationDeg, 0);
      await contextMenu();
      await tap(find.text('Rotate…'));
      await inspector(find.byKey(const ValueKey('asset-pose-5')));
      await tester.enterText(find.byKey(const ValueKey('asset-pose-5')), '90');
      await tester.pumpAndSettle();
      expect(
        scene().assets.single.rotationDeg,
        90,
        reason: 'live preview updates 2D before Apply',
      );
      await inspector(find.byKey(const ValueKey('asset-pose-3')));
      await tester.enterText(find.byKey(const ValueKey('asset-pose-3')), '100');
      await inspector(find.byKey(const ValueKey('asset-pose-2')));
      await tester.enterText(
        find.byKey(const ValueKey('asset-pose-2')),
        '1500',
      );
      await tap(find.byKey(const ValueKey('inspector-floor-tab')));
      expect(find.byKey(const ValueKey('asset-pose-3')), findsNothing);
      expect(scene().assets.single.rotationXDeg, 100);
      await tap(find.byKey(const ValueKey('inspector-selection-tab')));
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('asset-pose-3')))
            .controller!
            .text,
        '100',
      );
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('asset-pose-2')))
            .controller!
            .text,
        '1500',
      );
      await inspector(find.byKey(const ValueKey('apply-asset-pose')));
      expect(scene().assets.single.rotationDeg, 90);
      expect(scene().assets.single.position, placed.position);
      expect(scene().assets.single.widthMm, placed.widthMm);
      expect(scene().assets.single.depthMm, placed.depthMm);
      expect(scene().assets.single.heightMm, 600);
      expect(scene().assets.single.rotationXDeg, 100);
      expect(scene().assets.single.elevationMm, 1500);
      expect(
        identical(scene().modelPreviews[placed.digest], modelImage),
        isTrue,
        reason: 'rotation reuses decoded art',
      );
      // Applying the current angle must not add a second undo entry.
      await contextMenu();
      await tap(find.text('Rotate…'));
      await inspector(find.byKey(const ValueKey('apply-asset-pose')));
      await tap(find.byTooltip('Undo'));
      expect(scene().assets.single.rotationDeg, 0);
      expect(scene().assets.single.rotationXDeg, 0);
      expect(scene().assets.single.elevationMm, 0);
      await tap(find.byTooltip('Redo'));
      expect(scene().assets.single.rotationDeg, 90);
      expect(scene().assets.single.rotationXDeg, 100);
      expect(scene().assets.single.widthMm, 2000);
      expect(scene().assets.single.depthMm, 800);
      expect(scene().assets.single.heightMm, 600);

      final before = scene().assets.single.position;
      await tester.dragFrom(
        tester.getTopLeft(canvas()) + scene().toScreen(before),
        const Offset(40, 20),
      );
      await tester.pumpAndSettle();
      // The drag moves by the pixel displacement resolved to the whole
      // millimetres the panel would show, never 399.999…-style debris.
      // Plan +Y is up, so the downward screen component commits negative.
      final after = scene().assets.single.position;
      expect(
        after,
        before +
            Offset(
              (40 / scene().view.zoom).roundToDouble(),
              -(20 / scene().view.zoom).roundToDouble(),
            ),
        reason: 'a single-item drag commits whole millimetres',
      );
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
      expect(scene().assets.single.rotationXDeg, 100);
      expect(scene().assets.single.elevationMm, 1500);
      await tap(find.byKey(const ValueKey('asset-library')));
      await io();
      expect(find.textContaining('original sofa.obj'), findsOneWidget);
      expect(find.textContaining('Cannot read'), findsOneWidget);
      await tap(find.text('Close'));
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(() async {
        await tester.pumpWidget(
          MaterialApp(
            home: PlanPage(
              bridgeLoader: () => FfiCore(bridge),
              configStore: UserConfigStore(FileConfigStorage(configFile)),
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

  testWidgets(
    'marquee selects several models and one drag moves them all equally',
    (tester) async {
      final root = Directory.systemTemp.createTempSync('domowik-marquee-');
      File('${root.path}/pair sofa.obj').writeAsStringSync(
        'v 0 1 0\nv 3 1 0\nv 3 1 1\nv 1 1 1\nv 1 1 3\nv 0 1 3\nf 1 2 3 4 5 6\n',
      );
      final output = File('${root.path}.domo');
      addTearDown(() {
        if (root.existsSync()) root.deleteSync(recursive: true);
        if (output.existsSync()) output.delete();
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
          MaterialApp(home: PlanPage(bridgeLoader: () => FfiCore(bridge))),
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
      Future<void> io() async {
        await tester.runAsync(
          () async => Future<void>.delayed(const Duration(milliseconds: 150)),
        );
        await tester.pumpAndSettle();
      }

      Future<void> tap(Finder f) async {
        await tester.runAsync(() async {
          await tester.tap(f);
          await Future<void>.delayed(const Duration(milliseconds: 150));
        });
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

      // Attach the directory once; the dialog stays open, so the first
      // placement taps the model inside it without reopening.
      await tap(find.byKey(const ValueKey('asset-library')));
      await io();
      await tap(find.byKey(const ValueKey('attach-asset-directory')));
      await io();

      // `choose-asset` pops the dialog, so a later placement reopens it and
      // WAITS for the reopened dialog's directory scan: its progress spinner
      // schedules frames forever, so a blind pumpAndSettle would time out
      // while the scan is still running.
      // After the first placement the plan EMBEDS the model, so a reopened
      // dialog lists it twice — the directory copy and the embedded copy.
      // Either places the same digest; take the first deterministically.
      Finder modelRow() => find
          .descendant(
            of: find.byType(AssetLibraryDialog),
            matching: find.widgetWithText(ListTile, 'pair sofa'),
          )
          .first;
      Future<void> placeOne(Offset at, {bool reopen = false}) async {
        if (reopen) {
          // The reopened dialog's directory scan needs BOTH fake-clock pumps
          // (the open handler only continues past its first await on one,
          // and the scan's setState lands on another) and real time for the
          // file IO — a settle before the scan finishes spins forever on the
          // progress indicator.
          await tester.runAsync(() async {
            await tester.pump();
            await tester.tap(find.byKey(const ValueKey('asset-library')));
            await tester.pump();
            for (var i = 0; i < 40; i++) {
              await Future<void>.delayed(const Duration(milliseconds: 50));
              await tester.pump(const Duration(milliseconds: 100));
              if (modelRow().evaluate().isNotEmpty) break;
            }
          });
          await tester.pumpAndSettle();
        }
        await tap(modelRow());
        await tap(find.byKey(const ValueKey('choose-asset')));
        await click(tester.getTopLeft(canvas()) + at);
      }

      await placeOne(const Offset(320, 360));
      await placeOne(const Offset(620, 360), reopen: true);
      expect(scene().assets, hasLength(2));
      final before = {
        for (final asset in scene().assets) asset.id: asset.position,
      };

      // A marquee from EMPTY canvas, clear of both footprints, covering both.
      final planA = before.values.reduce(
        (a, b) => Offset(a.dx < b.dx ? a.dx : b.dx, a.dy < b.dy ? a.dy : b.dy),
      );
      final planB = before.values.reduce(
        (a, b) => Offset(a.dx > b.dx ? a.dx : b.dx, a.dy > b.dy ? a.dy : b.dy),
      );
      final topLeft = tester.getTopLeft(canvas());
      final from = topLeft + scene().toScreen(planA - const Offset(2000, 2000));
      final to = topLeft + scene().toScreen(planB + const Offset(2000, 2000));
      expect(tester.getRect(canvas()).contains(from), isTrue);
      expect(tester.getRect(canvas()).contains(to), isTrue);
      await tester.dragFrom(from, to - from);
      await tester.pumpAndSettle();
      expect(scene().selectedIds, before.keys.toSet());
      expect(scene().selected, isNotNull);

      // One drag on EITHER member moves every member by the same plan delta.
      final grab =
          topLeft +
          scene().toScreen(scene().assets.first.position) +
          const Offset(30, 10);
      final gesture = await tester.startGesture(
        grab,
        kind: PointerDeviceKind.mouse,
      );
      await gesture.moveBy(const Offset(45, 25));
      await tester.pump();
      expect(scene().groupPreview?.$1, before.keys.toSet());
      await gesture.up();
      await tester.pumpAndSettle();
      final after = {
        for (final asset in scene().assets) asset.id: asset.position,
      };
      final deltas = [for (final id in before.keys) after[id]! - before[id]!];
      expect(deltas[0], isNot(Offset.zero));
      for (final delta in deltas) {
        expect((delta - deltas[0]).distance, lessThan(1e-6));
      }
      expect(scene().selectedIds, before.keys.toSet());

      // ONE undo entry: both models return together.
      await tap(find.byTooltip('Undo'));
      expect({
        for (final asset in scene().assets) asset.id: asset.position,
      }, before);
    },
  );

  test('position formatting owns the whole-millimetre resolution', () {
    expect(formatPositionMm(10494.436709540969), '10494');
    expect(
      formatPositionMm(2093.5),
      '2094',
      reason: 'half rounds away from zero',
    );
    expect(formatPositionMm(-2093.5), '-2094');
    expect(formatPositionMm(-0.4), '0', reason: 'a rounded -0 prints unsigned');
    expect(formatPositionMm(0), '0');
    expect(formatPositionMm(10000000), '10000000');
  });

  testWidgets('pose fields preview and commit whole millimetres only', (
    tester,
  ) async {
    final previews = <List<double>>[];
    final applies = <List<double>>[];
    await tester.binding.setSurfaceSize(const Size(900, 1200));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AssetPoseControls(
            initial: [10494.436709540969, 2093.5, 0.5, 0, 0, 0],
            onPreview: previews.add,
            onApply: applies.add,
            onCancel: () {},
          ),
        ),
      ),
    );
    String text(int i) => tester
        .widget<TextField>(find.byKey(ValueKey('asset-pose-$i')))
        .controller!
        .text;
    expect(text(0), '10494');
    expect(text(1), '2094');
    expect(text(2), '1');
    expect(text(3), '0.0', reason: 'angle fields keep their own precision');
    expect(text(5), '0.0');

    await tester.enterText(
      find.byKey(const ValueKey('asset-pose-0')),
      '10494.436709540969',
    );
    await tester.pump();
    expect(previews.last[0], 10494, reason: 'preview sees whole millimetres');

    await tester.enterText(
      find.byKey(const ValueKey('asset-pose-0')),
      '10494.6',
    );
    await tester.pump();
    expect(
      previews.last[0],
      10495,
      reason: 'a typed fraction previews rounded',
    );

    await tester.tap(find.byKey(const ValueKey('apply-asset-pose')));
    await tester.pump();
    expect(applies.last[0], 10495, reason: 'Apply commits what preview showed');
    expect(applies.last[1], 2094);
    expect(applies.last[2], 1);

    await tester.enterText(find.byKey(const ValueKey('asset-pose-1')), '2094');
    await tester.pump();
    await tester.tap(find.byTooltip('Increase 10 mm').at(1));
    await tester.pump();
    expect(text(1), '2104', reason: 'stepping stays on the millimetre lattice');
    expect(previews.last[1], 2104);
  });

  testWidgets('model property positions seed whole millimetres', (
    tester,
  ) async {
    final asset = AssetFootprint.fromJson({
      'id': 'a',
      'levelId': 'l',
      'digest':
          'sha256:72a08d5b1e5ac4d3a91b8a5c45ee1f9f72a08d5b1e5ac4d3a91b8a5c45ee1f9f',
      'name': 'Chair',
      'positionMm': [10494.436709540969, 2093.5],
      'dimensionsMm': [800, 600],
      'rotationDeg': 0,
      'pose3d': [0.5, 0, 0],
      'cornersMm': [
        [0, 0],
        [800, 0],
        [800, 600],
        [0, 600],
      ],
    });
    List<double>? returned;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                returned = await showDialog<List<double>>(
                  context: context,
                  builder: (_) =>
                      AssetPropertiesDialog(asset: asset, heightMm: 730.0),
                );
              },
              child: const Text('open model properties'),
            ),
          ),
        ),
      ),
    );
    String text(int i) => tester
        .widget<TextField>(find.byKey(ValueKey('asset-property-$i')))
        .controller!
        .text;
    await tester.tap(find.text('open model properties'));
    await tester.pumpAndSettle();
    expect(text(0), '10494');
    expect(text(1), '2094');
    expect(
      text(2),
      '800.0',
      reason: 'dimensions are not placement coordinates',
    );
    expect(text(5), '730.0');

    await tester.enterText(
      find.byKey(const ValueKey('asset-property-0')),
      '10494.9',
    );
    await tester.pump();
    await tester.tap(find.byKey(const ValueKey('apply-asset-properties')));
    await tester.pumpAndSettle();
    expect(
      returned![0],
      10494.9,
      reason:
          'the dialog hands back the typed value; the commit funnel '
          '(_setAsset) resolves it to whole millimetres',
    );
  });
}

/// Counts calls while retaining the real native document and mutation APIs.
class _CountingCore extends FfiCore {
  _CountingCore(super.bridge);
  int previews = 0, stateReads = 0, listReads = 0;
  @override
  CoreReply assets(int handle, String request) {
    final message = jsonDecode(request) as Map;
    if (message['kind'] == 'pose' && message['preview'] == true) previews++;
    if (message['kind'] == 'list') listReads++;
    return super.assets(handle, request);
  }

  @override
  CoreReply state(int handle) {
    stateReads++;
    return super.state(handle);
  }
}
