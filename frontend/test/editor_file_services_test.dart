import 'dart:async';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class _Dialogs implements FileDialogs {
  Future<bool> Function()? close;
  Future<String?> Function() opening = () async => null;
  String? saving, suggested;
  @override
  Future<String?> openPlan() => opening();
  @override
  Future<String?> savePlan(String name) async {
    suggested = name;
    return saving;
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) => close = handler;
}

void main() {
  late Bridge bridge;
  late Directory scratch;
  late _Dialogs dialogs;
  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
    scratch = Directory.systemTemp.createTempSync('domowik-file-service-');
    dialogs = _Dialogs();
  });
  tearDown(() {
    expect(bridge.openPlans(), 0);
    expect(bridge.outstandingBuffers(), 0);
    scratch.deleteSync(recursive: true);
  });
  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(bridgeLoader: () => bridge, fileDialogs: dialogs),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'injected picker saves/reopens real plans and owns close callback',
    (tester) async {
      await start(tester);
      expect(await dialogs.close!(), isTrue);
      expect(bridge.openPlans(), 1);
      final path = '${scratch.path}/service plan.domo';
      dialogs.saving = path;
      await tester.tap(find.text('Save'));
      await tester.pumpAndSettle();
      expect(dialogs.suggested, 'plan.domo');
      expect(File(path).existsSync(), isTrue);
      dialogs.opening = () async => path;
      await tester.tap(find.byTooltip('Open plan'));
      await tester.pumpAndSettle();
      expect(find.text('service plan.domo'), findsOneWidget);
      expect(bridge.openPlans(), 1);
      await tester.pumpWidget(const SizedBox.shrink());
      expect(dialogs.close, isNull);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('unmount during an injected picker cannot adopt a late plan', (
    tester,
  ) async {
    await start(tester);
    final pending = Completer<String?>();
    dialogs.opening = () => pending.future;
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pump();
    expect(await dialogs.close!(), isFalse);
    await tester.pumpWidget(const SizedBox.shrink());
    pending.complete('${scratch.path}/late.domo');
    await tester.pumpAndSettle();
    expect(dialogs.close, isNull);
    expect(bridge.openPlans(), 0);
    expect(tester.takeException(), isNull);
  });
}
