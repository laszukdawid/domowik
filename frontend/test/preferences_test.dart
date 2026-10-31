import 'dart:convert';
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/preferences_dialog.dart';
import 'package:domowik_editor/measurement_units.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:domowik_editor/platform/config_storage_native.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

Future<void> chooseHorizontal(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('split-orientation')));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Horizontal (top and bottom)').last);
  await tester.pumpAndSettle();
}

Future<void> chooseImperial(WidgetTester tester) async {
  await tester.tap(find.byKey(const ValueKey('unit-system')));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Imperial (in, ft²)').last);
  await tester.pumpAndSettle();
}

Future<UserConfigStore> showPreferences(
  WidgetTester tester, {
  String? serverOverride,
  Future<bool> Function()? beforeServerChange,
}) async {
  final store = UserConfigStore.inMemory();
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: PreferencesDialog(
          store: store,
          showServer: true,
          serverOverride: serverOverride,
          beforeServerChange: beforeServerChange,
        ),
      ),
    ),
  );
  return store;
}

void main() {
  testWidgets('an invalid server is shown and nothing is saved', (
    tester,
  ) async {
    var asked = 0;
    final store = await showPreferences(
      tester,
      beforeServerChange: () async {
        asked++;
        return true;
      },
    );
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('server-url')))
          .controller!
          .text,
      'https://planner.homehero.pro',
    );
    await tester.enterText(
      find.byKey(const ValueKey('server-url')),
      'http://planner.example/path',
    );
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(find.textContaining('Enter an https:// address'), findsOneWidget);
    expect(asked, 0);
    expect(store.serverUrl, isNull);

    // Equivalent spelling of the current server is not a change.
    await tester.enterText(
      find.byKey(const ValueKey('server-url')),
      'https://Planner.HomeHero.pro/',
    );
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(asked, 0);
    expect(store.serverUrl, isNull);
  });

  testWidgets('an invalid stored server can be replaced or cleared', (
    tester,
  ) async {
    for (final (entered, stored) in [
      ('https://planner.example', 'https://planner.example'),
      ('', ''),
    ]) {
      final store = UserConfigStore(
        _MemoryStorage('{"version":1,"serverUrl":"http://remote.example"}'),
      );
      await store.load();
      expect(store.serverUrl, 'http://remote.example');
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () => showDialog<bool>(
                context: context,
                builder: (_) =>
                    PreferencesDialog(store: store, showServer: true),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('server-url')), entered);
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(find.byType(PreferencesDialog), findsNothing, reason: 'saved');
      expect(store.serverUrl, stored);
    }
  });

  testWidgets('a declined server change saves nothing', (tester) async {
    final store = await showPreferences(
      tester,
      beforeServerChange: () async => false,
    );
    await tester.enterText(
      find.byKey(const ValueKey('server-url')),
      'https://other.example',
    );
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(store.serverUrl, isNull);
    expect(find.text('Save'), findsOneWidget, reason: 'the dialog stays');
  });

  testWidgets('an environment override is shown and cannot be edited', (
    tester,
  ) async {
    await showPreferences(tester, serverOverride: 'http://127.0.0.1:8080');
    final field = tester.widget<TextField>(
      find.byKey(const ValueKey('server-url')),
    );
    expect(field.enabled, isFalse);
    expect(field.controller!.text, 'http://127.0.0.1:8080');
    expect(find.textContaining('Set by DOMOWIK_SERVER_URL'), findsOneWidget);
  });

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
          home: PlanPage(
            bridgeLoader: () => FfiCore(bridge),
            configStore: store,
          ),
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
      await chooseImperial(tester);
      await tester.tap(find.text('Cancel'));
      await tester.pumpAndSettle();
      expect(pane('workspace-2d'), plan);
      expect(store.splitOrientation, SplitOrientation.vertical);
      expect(store.unitSystem, UnitSystem.metric);
      await tester.tap(find.byTooltip('Preferences'));
      await tester.pumpAndSettle();
      await chooseHorizontal(tester);
      await chooseImperial(tester);
      await tester.tap(find.widgetWithText(FilledButton, 'Save').last);
      await tester.pumpAndSettle();
      expect(pane('workspace-2d').left, pane('workspace-3d').left);
      expect(pane('workspace-2d').bottom + 4, pane('workspace-3d').top);
      expect(pane('workspace-2d').height, pane('workspace-3d').height);
      expect(tester.state(find.byType(ThreeDView)), same(state));
      expect(state.camera, same(camera));
      expect(store.unitSystem, UnitSystem.imperial);
      final paint = tester.widget<CustomPaint>(
        find.byWidgetPredicate(
          (w) => w is CustomPaint && w.painter is PlanPainter,
        ),
      );
      expect((paint.painter! as PlanPainter).units, UnitSystem.imperial);
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
    final store = UserConfigStore(
      FileConfigStorage(File('${blocker.path}/config.json')),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(body: PreferencesDialog(store: store)),
      ),
    );
    await chooseHorizontal(tester);
    await chooseImperial(tester);
    await tester.runAsync(() async {
      await tester.tap(find.widgetWithText(FilledButton, 'Save'));
      await Future<void>.delayed(const Duration(milliseconds: 100));
    });
    await tester.pumpAndSettle();
    expect(find.textContaining('Could not save preferences:'), findsOneWidget);
    expect(store.splitOrientation, SplitOrientation.vertical);
    expect(store.unitSystem, UnitSystem.metric);
    expect(blocker.readAsStringSync(), 'original');
  });
}

/// Preferences kept in memory, starting from [text].
class _MemoryStorage implements ConfigStorage {
  _MemoryStorage(this.text);

  String text;

  @override
  Future<List<int>?> read(int limit) async => utf8.encode(text);

  @override
  Future<void> write(String text) async => this.text = text;
}
