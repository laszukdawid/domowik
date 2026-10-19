// A model restored by upload gets its 2D preview without reopening the plan
// (issue 0088 review round 2): the editor's failed-preview cache follows the
// core's asset revision.
@TestOn('browser')
library;

import 'dart:convert';
import 'dart:typed_data';

import 'package:domowik_editor/core/web_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/platform/model_import.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class _OpenDialogs implements FileDialogs {
  _OpenDialogs(this.opening);
  final PlanBytes opening;
  @override
  Future<PlanSource?> openPlan() async => opening;
  @override
  Future<PlanTarget?> savePlan(String name) async => null;
  @override
  void setCloseHandler(Future<bool> Function()? handler) {}
}

void main() {
  testWidgets('uploading a missing model brings its 2D preview back', (
    tester,
  ) async {
    late WebCore core;
    late Uint8List fixture;
    await tester.runAsync(() async {
      core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
      fixture = await fetchBytes('fixtures/optional-missing-model.domo');
    });
    final model = Uint8List.fromList(
      utf8.encode('v 0 0 0\nv 2 0 0\nv 0 1 2\nf 1 2 3\n'),
    );
    await tester.binding.setSurfaceSize(const Size(1400, 900));
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.binding.setSurfaceSize(null);
      expect(core.openPlans(), 0);
      expect(core.outstandingBuffers(), 0);
    });
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => core,
          fileDialogs: _OpenDialogs(PlanBytes('missing.domo', fixture)),
          configStore: UserConfigStore.inMemory(),
          modelUploads: true,
          modelPicker: () async => ('chair.obj', model),
          modelDirectoryPicker: () async => PickedModelDirectory(
            Stream.fromIterable([
              PickedModelFile(
                'chair-copy.obj',
                () async => model,
                sourcePath: 'Furniture/seating/chair-copy.obj',
              ),
              PickedModelFile(
                'table.zip',
                () async => Uint8List.fromList([1, 2]),
                sourcePath: 'Furniture/kitchen/table.zip',
              ),
              PickedModelFile(
                'unreadable.glb',
                () async => throw StateError('permission denied'),
              ),
            ]),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pumpAndSettle();
    PlanPainter painter() =>
        tester
                .widget<CustomPaint>(
                  find.byWidgetPredicate(
                    (w) => w is CustomPaint && w.painter is PlanPainter,
                  ),
                )
                .painter!
            as PlanPainter;
    int handle() => tester
        .widget<ThreeDView>(find.byType(ThreeDView, skipOffstage: false))
        .handle;
    Future<void> settle() async {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 100)),
      );
      await tester.pumpAndSettle();
    }

    // The missing model's preview fails and that failure is cached.
    await settle();
    expect(painter().assets, hasLength(1));
    expect(painter().modelPreviews, isEmpty);
    final before =
        (jsonDecode(core.state(handle()).text!) as Map)['asset_revision'];

    await tester.tap(find.byKey(const ValueKey('asset-library')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('upload-asset-model')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Close').last);
    await tester.pumpAndSettle();
    for (var i = 0; i < 30 && painter().modelPreviews.isEmpty; i++) {
      await settle();
    }
    final after =
        (jsonDecode(core.state(handle()).text!) as Map)['asset_revision'];
    expect(after, greaterThan(before as int));
    expect(
      painter().modelPreviews,
      hasLength(1),
      reason: 'the restored model is previewed without reopening the plan',
    );

    await tester.tap(find.byKey(const ValueKey('asset-library')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('upload-asset-directory')));
    await settle();
    expect(find.text('chair'), findsOneWidget);
    expect(find.text('table'), findsOneWidget);
    expect(find.textContaining('Furniture/kitchen/table.zip'), findsOneWidget);
    expect(find.textContaining('unreadable.glb'), findsOneWidget);
    final models =
        (jsonDecode(core.assets(handle(), '{"kind":"list"}').text!)
                as Map)['models']
            as List;
    expect(
      models.singleWhere((model) => model['name'] == 'table')['source'],
      'Furniture/kitchen/table.zip',
    );
    expect(
      painter().assets,
      hasLength(1),
      reason: 'import does not place models',
    );
    expect(tester.takeException(), isNull);
  });
}
