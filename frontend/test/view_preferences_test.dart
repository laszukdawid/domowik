import 'dart:async';
import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/editor_shell.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/three_d_view.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:domowik_editor/web_graphics_policy.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

class _Storage implements ConfigStorage {
  List<int>? bytes;
  bool fail = false;
  Completer<void>? pending;

  @override
  Future<List<int>?> read(int limit) async => bytes;
  @override
  Future<void> write(String text) async {
    await pending?.future;
    if (fail) throw StateError('storage unavailable');
    bytes = utf8.encode(text);
  }
}

class _Dialogs implements FileDialogs {
  String? path;
  String? savingPath;
  Future<bool> Function()? close;
  @override
  Future<PlanSource?> openPlan() async => path == null ? null : PlanPath(path!);
  @override
  Future<PlanTarget?> savePlan(String suggestedName) async =>
      savingPath == null ? null : PlanPathTarget(savingPath!);
  @override
  void setCloseHandler(Future<bool> Function()? handler) => close = handler;
}

void main() {
  late Bridge bridge;
  late FfiCore core;
  late Directory scratch;
  late _Storage storage;
  late _Dialogs dialogs;
  late String first, second, firstKey;
  late UserConfigStore store;
  var surfaceCreates = 0;

  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge(
      File('target/release/libdomowik_bridge.$suffix').absolute.path,
    );
    core = FfiCore(bridge);
    scratch = Directory.systemTemp.createTempSync('domowik-view-preferences-');
    storage = _Storage();
    dialogs = _Dialogs();
    store = UserConfigStore(storage);
    surfaceCreates = 0;
    String fixture(String name) {
      final opened = core.createDefault();
      try {
        final path = '${scratch.path}/$name.domo';
        expect(core.savePath(opened.handle, path), ok);
        return path;
      } finally {
        core.close(opened.handle);
      }
    }

    first = fixture('first');
    second = fixture('second');
    final opened = core.openPath(first);
    firstKey =
        (jsonDecode(core.snapshot(opened.handle).text!) as Map)['documentId']
            as String;
    core.close(opened.handle);
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(ThreeDViewState.channel, (call) async {
          if (call.method != 'create') return null;
          surfaceCreates++;
          return {'textureId': 71, 'session': 1};
        });
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(ThreeDViewState.channel, null);
    expect(bridge.openPlans(), 0);
    expect(bridge.outstandingBuffers(), 0);
    scratch.deleteSync(recursive: true);
  });

  Future<void> start(
    WidgetTester tester, {
    WebGraphicsPolicy? graphicsPolicy,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1200, 900));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => core,
          configStore: store,
          fileDialogs: dialogs,
          graphicsPolicy: graphicsPolicy,
        ),
      ),
    );
    await tester.pumpAndSettle();
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();
    });
  }

  String mode(WidgetTester tester) =>
      tester.widget<EditorShell>(find.byType(EditorShell)).viewMode;
  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.tap(finder);
    await tester.pumpAndSettle();
  }

  Future<void> open(WidgetTester tester, String path) async {
    dialogs.path = path;
    await tap(tester, find.byTooltip('Open plan'));
  }

  Future<void> chooseView(WidgetTester tester, String label) async {
    await tap(tester, find.byKey(const ValueKey('view-mode-menu')));
    await tap(tester, find.text(label).last);
  }

  Future<void> preferences(WidgetTester tester) =>
      tap(tester, find.byTooltip('Preferences'));
  Future<void> savePreferences(WidgetTester tester) =>
      tap(tester, find.widgetWithText(FilledButton, 'Save').last);
  Future<void> defaultSplit(WidgetTester tester) async {
    await tap(tester, find.byKey(const ValueKey('default-view-mode')));
    await tap(tester, find.text('2D + 3D split view').last);
  }

  testWidgets(
    'safe 2D overrides defaults and remembered modes without GPU work',
    (tester) async {
      await store.savePreferences(
        splitOrientation: SplitOrientation.vertical,
        unitSystem: store.unitSystem,
        defaultViewMode: '3d',
      );
      await store.saveRememberedViewMode(firstKey, 'split');
      final savedConfig = List<int>.of(storage.bytes!);
      final savedPlan = File(first).readAsBytesSync();
      await start(
        tester,
        graphicsPolicy: WebGraphicsPolicy.forBrowser(
          userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Firefox/159.0',
          page: Uri.parse('https://editor.invalid/'),
        ),
      );
      expect(mode(tester), '2d');
      expect(find.byKey(const ValueKey('safe-2d-banner')), findsOneWidget);
      expect(find.byType(ThreeDView), findsNothing);
      await open(tester, first);
      for (final label in ['3D view', '2D + 3D split view', '2D plan']) {
        await chooseView(tester, label);
        expect(mode(tester), '2d');
        expect(find.byType(ThreeDView), findsNothing);
      }
      await open(tester, second);
      expect(mode(tester), '2d');
      expect(surfaceCreates, 0);
      expect(store.defaultViewMode, '3d');
      expect(store.rememberedViewModes[firstKey], 'split');
      // Opening files may update the independent remembered-floor hint.
      final before = jsonDecode(utf8.decode(savedConfig)) as Map;
      final after = jsonDecode(utf8.decode(storage.bytes!)) as Map;
      expect(after['defaultViewMode'], before['defaultViewMode']);
      expect(after['rememberedViewModes'], before['rememberedViewModes']);
      expect(File(first).readAsBytesSync(), savedPlan);
      expect(find.textContaining('• Unsaved'), findsNothing);
      final canvas = find.byWidgetPredicate(
        (w) => w is CustomPaint && w.painter is PlanPainter,
      );
      int wallCount() =>
          (tester.widget<CustomPaint>(canvas).painter! as PlanPainter)
              .walls
              .length;
      await tap(tester, find.byKey(const ValueKey('wall-chain-tool')));
      await tester.dragFrom(
        tester.getTopLeft(canvas) + const Offset(100, 150),
        const Offset(250, 0),
      );
      await tester.pumpAndSettle();
      expect(wallCount(), 1);
      await tap(tester, find.widgetWithText(ChoiceChip, 'Select'));
      await tap(tester, find.byTooltip('Undo'));
      expect(wallCount(), 0);
      await tap(tester, find.byTooltip('Redo'));
      expect(wallCount(), 1);
      await tap(tester, find.byKey(const ValueKey('save-action')));
      expect(find.textContaining('• Unsaved'), findsNothing);
      await open(tester, second);
      expect(wallCount(), 1);
      expect(mode(tester), '2d');
      await tester.binding.setSurfaceSize(const Size(560, 720));
      await tester.pumpAndSettle();
      await chooseView(tester, '3D view');
      expect(mode(tester), '2d');
      expect(find.byKey(const ValueKey('safe-2d-banner')), findsOneWidget);
      expect(surfaceCreates, 0);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'an unavailable WebGPU adapter falls back for the page without retries',
    (tester) async {
      await store.savePreferences(
        splitOrientation: SplitOrientation.vertical,
        unitSystem: store.unitSystem,
        defaultViewMode: '3d',
      );
      await store.saveRememberedViewMode(firstKey, 'split');
      final savedPlan = File(first).readAsBytesSync();
      const diagnostic = 'No suitable graphics adapter found';
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
          .setMockMethodCallHandler(ThreeDViewState.channel, (call) async {
            if (call.method == 'create') {
              surfaceCreates++;
              throw PlatformException(
                code: 'no-webgpu',
                message: List.filled(30, diagnostic).join('\n'),
              );
            }
            fail('a failed surface must not load or render a scene');
          });
      await start(tester);
      expect(mode(tester), '2d');
      expect(find.byType(ThreeDView), findsNothing);
      expect(find.byKey(const ValueKey('safe-2d-banner')), findsOneWidget);
      expect(find.textContaining('current graphics setup'), findsOneWidget);
      expect(surfaceCreates, 1);
      expect(find.textContaining(diagnostic), findsNothing);
      expect(find.textContaining('PlatformException'), findsNothing);
      await tap(tester, find.text('3D diagnostics'));
      expect(find.textContaining(diagnostic), findsOneWidget);
      await tap(tester, find.text('3D diagnostics'));
      await open(tester, first);
      for (final label in ['3D view', '2D + 3D split view', '2D plan']) {
        await chooseView(tester, label);
        expect(mode(tester), '2d');
        expect(find.byType(ThreeDView), findsNothing);
      }
      await open(tester, second);
      await tester.binding.setSurfaceSize(const Size(560, 720));
      await tester.pumpAndSettle();
      await chooseView(tester, '3D view');
      await tap(tester, find.text('3D diagnostics'));
      expect(find.textContaining(diagnostic), findsOneWidget);
      expect(mode(tester), '2d');
      expect(surfaceCreates, 1);
      expect(store.defaultViewMode, '3d');
      expect(store.rememberedViewModes[firstKey], 'split');
      expect(File(first).readAsBytesSync(), savedPlan);
      expect(find.textContaining('• Unsaved'), findsNothing);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'opt-in remembers last mode per layout across restart without saving the plan',
    (tester) async {
      final original = File(first).readAsBytesSync();
      await start(tester);
      await open(tester, first);
      await chooseView(tester, '2D + 3D split view');
      await open(tester, first);
      expect(mode(tester), '2d', reason: 'remembering is opt-in');
      await chooseView(tester, '2D + 3D split view');
      await preferences(tester);
      await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
      await savePreferences(tester);
      expect(store.rememberedViewModes[firstKey], 'split');
      await chooseView(tester, '3D view');
      expect(store.rememberedViewModes[firstKey], '3d');
      expect(find.textContaining('• Unsaved'), findsNothing);
      expect(File(first).readAsBytesSync(), original);
      expect(
        tester.widget<EditorShell>(find.byType(EditorShell)).onUndo,
        isNull,
      );
      expect(await dialogs.close!(), isTrue);
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpAndSettle();

      store = UserConfigStore(storage);
      await start(tester);
      expect(mode(tester), '2d');
      await open(tester, first);
      expect(mode(tester), '3d');
      await open(tester, second);
      expect(mode(tester), '2d');
      await open(tester, first);
      expect(mode(tester), '3d');
      await preferences(tester);
      await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
      await savePreferences(tester);
      expect(
        mode(tester),
        '2d',
        reason: 'forgetting reapplies the global default',
      );
      await open(tester, first);
      expect(mode(tester), '2d');
    },
  );

  testWidgets(
    'global default applies to new and unconfigured layouts; overrides win',
    (tester) async {
      await store.saveRememberedViewMode(firstKey, '3d');
      await start(tester);
      await open(tester, first);
      expect(mode(tester), '3d');
      await preferences(tester);
      await defaultSplit(tester);
      await savePreferences(tester);
      expect(store.defaultViewMode, 'split');
      expect(mode(tester), '3d');
      await open(tester, second);
      expect(mode(tester), 'split');
      await tap(tester, find.byTooltip('New plan'));
      expect(mode(tester), 'split');
      await open(tester, first);
      expect(mode(tester), '3d');
      expect(find.textContaining('• Unsaved'), findsNothing);
    },
  );

  testWidgets('cancel and failed preference saves apply neither choice', (
    tester,
  ) async {
    await start(tester);
    await open(tester, first);
    await preferences(tester);
    expect(
      await dialogs.close!(),
      isFalse,
      reason: 'layout preferences exclude close',
    );
    await defaultSplit(tester);
    await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
    await tap(tester, find.text('Cancel'));
    expect(store.defaultViewMode, '2d');
    expect(store.rememberedViewModes, isEmpty);
    expect(mode(tester), '2d');

    await preferences(tester);
    await defaultSplit(tester);
    await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
    storage.fail = true;
    await savePreferences(tester);
    expect(find.textContaining('Could not save preferences:'), findsOneWidget);
    expect(store.defaultViewMode, '2d');
    expect(store.rememberedViewModes, isEmpty);
    expect(mode(tester), '2d');
    storage.fail = false;
    await savePreferences(tester);
    expect(store.defaultViewMode, 'split');
    expect(store.rememberedViewModes[firstKey], '2d');
    expect(
      mode(tester),
      '2d',
      reason: 'enabling remembers the actual current view',
    );
  });

  testWidgets(
    'legacy layouts remember by path and Save As carries the override',
    (tester) async {
      final output = alloc<Uint64>(sizeOf<Uint64>());
      expect(bridge.planCreate(output), ok);
      final handle = output.value;
      release(output);
      final path = '${scratch.path}/legacy.domo';
      try {
        const level = '01950000-0000-7000-8000-000000000001';
        expect(
          core
              .execute(
                handle,
                jsonEncode({
                  'operations': [
                    {
                      'operationId': '01950000-0000-7000-8000-000000000002',
                      'entityId': level,
                      'kind': 'createEntity',
                      'entity': {
                        'type': 'level',
                        'id': level,
                        'name': 'Ground floor',
                        'elevationMm': 0,
                        'floorThicknessMm': 200,
                        'defaultWallHeightMm': 2500,
                        'defaultCeilingHeightMm': 2500,
                      },
                    },
                  ],
                }),
              )
              .succeeded,
          isTrue,
        );
        expect(core.savePath(handle, path), ok);
      } finally {
        core.close(handle);
      }
      final original = File(path).readAsBytesSync();
      await start(tester);
      await open(tester, path);
      await chooseView(tester, '2D + 3D split view');
      await preferences(tester);
      await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
      await savePreferences(tester);
      expect(store.rememberedViewModes.keys.single, startsWith('legacy-path:'));
      final saved = '${scratch.path}/saved-as.domo';
      dialogs.savingPath = saved;
      await tap(tester, find.byKey(const ValueKey('save-menu')));
      await tap(tester, find.text('Save as'));
      expect(store.rememberedViewModes, hasLength(2));
      await open(tester, path);
      expect(mode(tester), 'split');
      await open(tester, saved);
      expect(mode(tester), 'split');
      expect(File(path).readAsBytesSync(), original);
      final unrelated = '${scratch.path}/other-legacy.domo';
      File(unrelated).writeAsBytesSync(original);
      await open(tester, unrelated);
      expect(
        mode(tester),
        '2d',
        reason: 'legacy paths distinguish identical files',
      );
    },
  );

  testWidgets(
    'failed automatic writes keep the view usable and allow opting out',
    (tester) async {
      await store.saveRememberedViewMode(firstKey, '2d');
      await start(tester);
      await open(tester, first);
      storage.fail = true;
      await chooseView(tester, '3D view');
      expect(mode(tester), '3d');
      expect(store.viewModeFor(firstKey), '2d');
      expect(find.textContaining('Could not remember view:'), findsOneWidget);
      expect(find.textContaining('• Unsaved'), findsNothing);
      storage.fail = false;
      await preferences(tester);
      await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
      await savePreferences(tester);
      expect(mode(tester), '2d');
      expect(store.rememberedViewModes, isEmpty);
    },
  );

  testWidgets('reopening waits for a queued mode choice', (tester) async {
    await store.saveRememberedViewMode(firstKey, '2d');
    await start(tester);
    await open(tester, first);
    storage.pending = Completer<void>();
    await chooseView(tester, '2D + 3D split view');
    await open(tester, first);
    expect(mode(tester), 'split');
    storage.pending!.complete();
    await tester.pumpAndSettle();
    expect(mode(tester), 'split');
    expect(store.viewModeFor(firstKey), 'split');
    expect(find.textContaining('• Unsaved'), findsNothing);
  });

  testWidgets('view preferences remain usable in a narrow window', (
    tester,
  ) async {
    await start(tester);
    await open(tester, first);
    await tester.binding.setSurfaceSize(const Size(390, 700));
    await tester.pumpAndSettle();
    await preferences(tester);
    await defaultSplit(tester);
    await tap(tester, find.byKey(const ValueKey('remember-layout-view')));
    await savePreferences(tester);
    expect(store.defaultViewMode, 'split');
    expect(store.viewModeFor(firstKey), '2d');
    expect(tester.takeException(), isNull);
  });

  testWidgets('normal close waits for the last remembered mode write', (
    tester,
  ) async {
    await store.saveRememberedViewMode(firstKey, '2d');
    await start(tester);
    await open(tester, first);
    storage.pending = Completer<void>();
    await chooseView(tester, '2D + 3D split view');
    var closed = false;
    final close = dialogs.close!().then((result) => closed = result);
    await tester.pumpAndSettle();
    expect(closed, isFalse);
    storage.pending!.complete();
    await tester.pumpAndSettle();
    expect(await close, isTrue);
    final restarted = UserConfigStore(storage);
    await restarted.load();
    expect(restarted.viewModeFor(firstKey), 'split');
    expect(find.textContaining('• Unsaved'), findsNothing);
  });
}
