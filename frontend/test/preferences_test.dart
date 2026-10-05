import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/preferences_dialog.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> chooseHorizontal(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('split-orientation')));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Horizontal (top and bottom)').last);
  await tester.pumpAndSettle();
}

void main() {
  testWidgets(
    'editor applies saved split, cancel preserves layout and view state',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1200, 800));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      final suffix = Platform.isMacOS ? 'dylib' : 'so';
      final bridge = Bridge(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      );
      final store = UserConfigStore.inMemory();
      await tester.pumpWidget(
        MaterialApp(
          home: PlanPage(bridgeLoader: () => bridge, configStore: store),
        ),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('view-mode-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('2D + 3D split view'));
      await tester.pumpAndSettle();
      final state = tester.state<ThreeDViewState>(find.byType(ThreeDView));
      final camera = state.camera;
      Rect pane(String name) => tester.getRect(find.byKey(ValueKey(name)));
      final plan = pane('workspace-2d');
      final scene = pane('workspace-3d');
      expect(plan.top, scene.top);
      expect(plan.right + 4, scene.left);
      expect(plan.width, scene.width);
      await tester.tap(find.byTooltip('Preferences'));
      await tester.pumpAndSettle();
      await chooseHorizontal(tester);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(pane('workspace-2d'), plan);
      expect(store.splitOrientation, SplitOrientation.vertical);
      await tester.tap(find.byTooltip('Preferences'));
      await tester.pumpAndSettle();
      await chooseHorizontal(tester);
      await tester.tap(find.widgetWithText(FilledButton, 'Save').last);
      await tester.pumpAndSettle();
      expect(pane('workspace-2d').left, pane('workspace-3d').left);
      expect(pane('workspace-2d').bottom + 4, pane('workspace-3d').top);
      expect(pane('workspace-2d').height, pane('workspace-3d').height);
      expect(tester.state(find.byType(ThreeDView)), same(state));
      expect(state.camera, same(camera));
      expect(find.textContaining('• Unsaved'), findsNothing);
      expect(tester.takeException(), isNull);
      await tester.binding.setSurfaceSize(const Size(390, 700));
      await tester.pumpAndSettle();
      expect(find.byTooltip('Preferences'), findsOneWidget);
      expect(tester.takeException(), isNull);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
      expect(bridge.openPlans(), 0);
      expect(bridge.outstandingBuffers(), 0);
    },
  );

  testWidgets('failed save stays visible and retains the applied preference', (
    tester,
  ) async {
    final root = Directory.systemTemp.createTempSync('domowik-preferences-');
    addTearDown(() => root.deleteSync(recursive: true));
    final blocker = File('${root.path}/blocked')..writeAsStringSync('original');
    final store = UserConfigStore(File('${blocker.path}/config.json'));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: PreferencesDialog(store: store)),
      ),
    );
    await chooseHorizontal(tester);
    await tester.runAsync(() async {
      await tester.tap(find.widgetWithText(FilledButton, 'Save'));
      await Future<void>.delayed(const Duration(milliseconds: 100));
    });
    await tester.pumpAndSettle();
    expect(find.textContaining('Could not save preferences:'), findsOneWidget);
    expect(store.splitOrientation, SplitOrientation.vertical);
    expect(blocker.readAsStringSync(), 'original');
  });
}
