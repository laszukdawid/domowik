import 'dart:async';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

class _Dialogs implements FileDialogs {
  Future<bool> Function()? close;
  Future<String?> Function() opening = () async => null;
  String? saving, suggested;
  int saveCalls = 0;
  @override
  Future<PlanSource?> openPlan() async {
    final path = await opening();
    return path == null ? null : PlanPath(path);
  }

  @override
  Future<PlanTarget?> savePlan(String name) async {
    suggested = name;
    saveCalls++;
    final path = saving;
    return path == null ? null : PlanPathTarget(path);
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
        home: PlanPage(
          bridgeLoader: () => FfiCore(bridge),
          fileDialogs: dialogs,
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  /// Opens the save control's chevron menu and picks Save As from it.
  Future<void> tapSaveAs(WidgetTester tester) async {
    await tester.tap(find.byKey(const ValueKey('save-menu')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Save as'));
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

  testWidgets('an opened plan saves again silently; Save as always asks', (
    tester,
  ) async {
    await start(tester);
    expect(await dialogs.close!(), isTrue);
    final first = '${scratch.path}/service plan.domo';
    dialogs.saving = first;
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.saveCalls, 1);
    expect(File(first).existsSync(), isTrue);

    // Save as asks and rebinds: the next plain Save rewrites the new file.
    final second = '${scratch.path}/moved.domo';
    dialogs.saving = second;
    await tapSaveAs(tester);
    expect(dialogs.saveCalls, 2);
    expect(File(second).existsSync(), isTrue);

    dialogs.saving = null;
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.saveCalls, 2, reason: 'plain Save does not ask');
    expect(find.textContaining('Saved to', findRichText: true), findsOneWidget);

    // Reopening binds the plan to the moved file, still saving silently.
    dialogs.opening = () async => second;
    await tester.tap(find.byTooltip('Open plan'));
    await tester.pumpAndSettle();
    expect(find.text('moved.domo'), findsOneWidget);
    await tester.tap(find.text('Save'));
    await tester.pumpAndSettle();
    expect(dialogs.saveCalls, 2);

    await tester.pumpWidget(const SizedBox.shrink());
    expect(dialogs.close, isNull);
    expect(tester.takeException(), isNull);
  });

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
