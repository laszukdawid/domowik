import 'dart:async';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

const _accountSession = AccountSession(token: 'csrf');

class _HostedClient implements AccountClient, HostedPlanClient {
  _HostedClient(this.packageBytes);

  final Uint8List packageBytes;
  final plans = <String, HostedPlan>{};
  final packages = <String, Uint8List>{};
  final calls = <String>[];
  final savedRevisions = <String>[];
  final savedBytes = <Uint8List>[];
  Completer<HostedPlan>? pendingSave;
  String? failSaveCode;
  String? failAcquireCode;
  String? failRenewCode;
  int _nextId = 1;
  int signOutCalls = 0;
  bool leavesEditor = true;

  @override
  ServerEndpoint get server => ServerEndpoint('https://planner.example');

  @override
  bool get signInLeavesEditor => leavesEditor;

  HostedPlan _newPlan(String title, Uint8List bytes) {
    final id = 'hosted-${_nextId++}';
    final plan = HostedPlan(
      id: id,
      title: title,
      revision: '1',
      updatedAt: DateTime.utc(2026),
    );
    plans[id] = plan;
    packages[id] = Uint8List.fromList(bytes);
    return plan;
  }

  AccountSession? restored = _accountSession;

  @override
  Future<AccountSession?> restore() async => restored;

  @override
  Future<AccountSession?> signIn() async => _accountSession;

  @override
  Future<void> signOut(AccountSession session) async {
    signOutCalls++;
  }

  @override
  Future<AccountProfile> profile(AccountSession session) async =>
      AccountProfile(
        email: 'person@example.test',
        plans: plans.values.toList(),
      );

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) async {
    calls.add('list');
    return plans.values.toList();
  }

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) async {
    calls.add('create:$title');
    return _newPlan(title, packageBytes);
  }

  @override
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  ) async {
    calls.add('import:$title');
    return _newPlan(title, bytes);
  }

  @override
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  }) async {
    calls.add('acquire:$planId:$force');
    final code = failAcquireCode;
    failAcquireCode = null;
    if (code != null) {
      throw AccountException('another writer holds this plan', code: code);
    }
    return HostedPlanLease(
      leaseId: 'lease-$planId',
      revision: plans[planId]!.revision,
      expiresInSeconds: 90,
    );
  }

  @override
  Future<HostedPlanLease> renewLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) async {
    calls.add('renew:$planId');
    // Sticky, like a lease that has really expired on the server.
    final code = failRenewCode;
    if (code != null) throw AccountException('lease expired', code: code);
    return HostedPlanLease(
      leaseId: leaseId,
      revision: plans[planId]!.revision,
      expiresInSeconds: 90,
    );
  }

  @override
  Future<void> releaseLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) async {
    calls.add('release:$planId');
  }

  @override
  Future<HostedPlanDownload> downloadPlan(
    AccountSession session,
    String planId,
  ) async {
    calls.add('download:$planId');
    final plan = plans[planId]!;
    return HostedPlanDownload(
      bytes: Uint8List.fromList(packages[planId]!),
      revision: plan.revision,
    );
  }

  @override
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  ) {
    calls.add('save:$planId:$leaseId');
    savedRevisions.add(expectedRevision);
    savedBytes.add(Uint8List.fromList(bytes));
    final code = failSaveCode;
    failSaveCode = null;
    if (code != null) {
      return Future.error(AccountException('write failed', code: code));
    }
    final completer = pendingSave;
    pendingSave = null;
    Future<HostedPlan> acknowledge(HostedPlan response) async {
      plans[planId] = response;
      packages[planId] = Uint8List.fromList(bytes);
      return response;
    }

    if (completer != null) return completer.future.then(acknowledge);
    return acknowledge(
      HostedPlan(
        id: planId,
        title: plans[planId]!.title,
        revision: '${int.parse(expectedRevision) + 1}',
        updatedAt: DateTime.utc(2026),
      ),
    );
  }
}

class _Dialogs implements FileDialogs {
  PlanSource? nextOpen;
  PlanTarget? nextSave;
  Future<bool> Function()? closeHandler;
  int saveCalls = 0;
  String? suggestedName;

  @override
  Future<PlanSource?> openPlan() async => nextOpen;

  @override
  Future<PlanTarget?> savePlan(String suggestedName) async {
    saveCalls++;
    this.suggestedName = suggestedName;
    return nextSave;
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) {
    closeHandler = handler;
  }
}

void main() {
  late Bridge bridge;
  late _HostedClient client;
  late _Dialogs dialogs;
  late Uint8List seedBytes;

  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
    final core = FfiCore(bridge);
    final opened = core.createDefault();
    seedBytes = core.encode(opened.handle).bytes!;
    core.close(opened.handle);
    client = _HostedClient(seedBytes);
    dialogs = _Dialogs();
  });

  Future<void> start(
    WidgetTester tester, {
    AccountClient? Function(String? savedServerUrl)? accountClients,
    UserConfigStore? configStore,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1200, 850));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => FfiCore(bridge),
          accountClient: accountClients == null ? client : null,
          accountClients: accountClients,
          configStore: configStore,
          fileDialogs: dialogs,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(bridge.openPlans(), 1);
    addTearDown(() async {
      await tester.pumpWidget(const SizedBox.shrink());
      expect(bridge.openPlans(), 0);
      expect(bridge.outstandingBuffers(), 0);
    });
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.tap(finder);
    await tester.pumpAndSettle();
  }

  Future<void> drawWall(WidgetTester tester, {double y = 150}) async {
    await tester.tap(find.byKey(const ValueKey('wall-chain-tool')));
    await tester.pumpAndSettle();
    final canvas = find.byWidgetPredicate(
      (widget) => widget is CustomPaint && widget.painter is PlanPainter,
    );
    await tester.dragFrom(
      tester.getTopLeft(canvas) + Offset(100, y),
      const Offset(250, 0),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.widgetWithText(ChoiceChip, 'Select'));
    await tester.pumpAndSettle();
  }

  Future<HostedPlan> createHosted(WidgetTester tester, String title) async {
    await tap(tester, find.byKey(const ValueKey('new-menu')));
    await tap(tester, find.text('New online plan…'));
    await tester.enterText(
      find.byKey(const ValueKey('online-plan-title')),
      title,
    );
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();
    return client.plans.values.single;
  }

  Future<void> showOnlinePlans(WidgetTester tester) async {
    await tap(tester, find.byKey(const ValueKey('open-menu')));
    await tap(tester, find.text('Open online plan…'));
  }

  Future<void> openHosted(WidgetTester tester, HostedPlan plan) async {
    await showOnlinePlans(tester);
    await tap(tester, find.byKey(ValueKey('online-open-${plan.id}')));
    await tester.pumpAndSettle();
  }

  Future<void> saveMenu(WidgetTester tester, String key) async {
    await tap(tester, find.byKey(const ValueKey('save-menu')));
    await tap(tester, find.byKey(ValueKey(key)));
  }

  Future<void> newLocalPlan(WidgetTester tester) async {
    await tap(tester, find.byKey(const ValueKey('new-menu')));
    await tap(tester, find.text('New plan'));
  }

  testWidgets('create, open, edit during save, local copy and hosted resave', (
    tester,
  ) async {
    await start(tester);
    final plan = await createHosted(tester, 'Project');
    expect(client.calls.take(2), [
      'create:Project',
      'acquire:${plan.id}:false',
    ]);
    expect(find.textContaining('Online · Project'), findsOneWidget);

    await drawWall(tester);
    final pending = Completer<HostedPlan>();
    client.pendingSave = pending;
    tester.widget<InkWell>(find.byKey(const ValueKey('save-action'))).onTap!();
    await tester.pump();
    await drawWall(tester, y: 260);
    pending.complete(
      HostedPlan(
        id: plan.id,
        title: plan.title,
        revision: '2',
        updatedAt: DateTime.utc(2026),
      ),
    );
    await tester.pumpAndSettle();
    expect(client.savedRevisions, ['1']);
    expect(find.textContaining('Online · Project • Unsaved'), findsOneWidget);

    Uint8List? localCopy;
    dialogs.nextSave = PlanByteTarget('Project.domo', (bytes) async {
      localCopy = Uint8List.fromList(bytes);
      return true;
    });
    await saveMenu(tester, 'save-as');
    expect(dialogs.suggestedName, 'Project.domo');
    expect(localCopy, isNotNull);
    expect(localCopy, isNot(client.savedBytes.single));
    expect(find.textContaining('Online · Project • Unsaved'), findsOneWidget);

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1', '2']);
    expect(find.textContaining('Online · Project'), findsOneWidget);
    expect(find.textContaining('Unsaved'), findsNothing);
    await newLocalPlan(tester);
    expect(find.text('Save your changes?'), findsNothing);
    expect(find.text('Untitled plan'), findsOneWidget);
  });

  testWidgets(
    'dirty cancel and failed local open preserve hosted editor state',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Existing', seedBytes);
      await openHosted(tester, plan);
      await drawWall(tester);

      await newLocalPlan(tester);
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Cancel'));
      expect(find.textContaining('Online · Existing'), findsOneWidget);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );

      dialogs.nextOpen = PlanBytes('bad.domo', Uint8List.fromList([1, 2, 3]));
      await tap(tester, find.byKey(const ValueKey('open-menu')));
      await tap(tester, find.text('Open file…'));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text("Don't save"));
      await tester.pumpAndSettle();
      expect(find.textContaining('Online · Existing'), findsOneWidget);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );
    },
  );

  testWidgets('lease-held open failure is visible in the library', (
    tester,
  ) async {
    await start(tester);
    final current = client._newPlan('Current', seedBytes);
    await openHosted(tester, current);
    expect(find.textContaining('Online · Current'), findsOneWidget);

    final held = client._newPlan('Held elsewhere', seedBytes);
    client.failAcquireCode = 'lease-held';
    await openHosted(tester, held);

    final error = find.byKey(const ValueKey('online-plans-action-error'));
    expect(error, findsOneWidget);
    expect(
      find.descendant(
        of: error,
        matching: find.textContaining('Close it there'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: error,
        matching: find.textContaining('Your current plan remains open.'),
      ),
      findsOneWidget,
    );
    expect(find.textContaining('Online · Current'), findsOneWidget);
    expect(find.byKey(const ValueKey('online-plans-dialog')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('save as new online plan switches Save to the new plan', (
    tester,
  ) async {
    await start(tester);
    await drawWall(tester);
    expect(client.calls.where((call) => call.startsWith('import:')), isEmpty);
    expect(find.text('Untitled plan • Unsaved'), findsOneWidget);

    await saveMenu(tester, 'save-as-online');
    await tester.enterText(
      find.byKey(const ValueKey('online-plan-title')),
      'Explicit copy',
    );
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();

    final plan = client.plans.values.single;
    expect(plan.title, 'Explicit copy');
    expect(client.calls, [
      'import:Explicit copy',
      'acquire:${plan.id}:false',
    ], reason: 'adopts the upload without downloading it again');
    expect(find.text('Online · Explicit copy'), findsOneWidget);

    await drawWall(tester, y: 260);
    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1']);
    expect(dialogs.saveCalls, 0);
    expect(find.text('Online · Explicit copy'), findsOneWidget);
  });

  testWidgets('a failed switch after upload keeps the current plan', (
    tester,
  ) async {
    await start(tester);
    await drawWall(tester);
    client.failAcquireCode = 'lease-held';
    await saveMenu(tester, 'save-as-online');
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();

    expect(client.plans, hasLength(1), reason: 'the upload itself succeeded');
    expect(find.text('Untitled plan • Unsaved'), findsOneWidget);
    expect(find.textContaining('Open online plan'), findsOneWidget);
  });

  testWidgets(
    'unconfirmed local recovery download cannot authorize replacement',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Keep me', seedBytes);
      await openHosted(tester, plan);
      await drawWall(tester);
      dialogs.nextSave = PlanByteTarget('recovery.domo', (_) async => false);

      await newLocalPlan(tester);
      await tap(tester, find.byKey(const ValueKey('leave-download-copy')));
      await tester.pumpAndSettle();
      expect(find.textContaining('Online · Keep me • Unsaved'), findsOneWidget);
      expect(find.text('Untitled plan'), findsNothing);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );
    },
  );

  testWidgets('large online plan list remains scrollable inside its bound', (
    tester,
  ) async {
    await start(tester);
    for (var i = 0; i < 35; i++) {
      client._newPlan('Plan $i', seedBytes);
    }
    await showOnlinePlans(tester);
    expect(find.byKey(const ValueKey('online-plans-dialog')), findsOneWidget);
    expect(tester.takeException(), isNull);
    final list = find.descendant(
      of: find.byKey(const ValueKey('online-plans-dialog')),
      matching: find.byType(ListView),
    );
    await tester.scrollUntilVisible(
      find.byKey(const ValueKey('online-open-hosted-35')),
      250,
      scrollable: find.descendant(of: list, matching: find.byType(Scrollable)),
    );
    expect(find.byKey(const ValueKey('online-open-hosted-35')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('Save again retries an unconfirmed save when nobody else saved', (
    tester,
  ) async {
    await start(tester);
    final plan = client._newPlan('Recover', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);
    client.failSaveCode = 'service-busy';
    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1']);
    expect(find.text('Online · Recover • Not saved online'), findsOneWidget);

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1', '1']);
    expect(client.calls, contains('renew:${plan.id}'));
    expect(find.text('Online · Recover'), findsOneWidget);
    expect(find.textContaining('Unsaved'), findsNothing);
  });

  testWidgets('Save refuses to overwrite a newer online version', (
    tester,
  ) async {
    await start(tester);
    final plan = client._newPlan('Shared', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);
    client.failSaveCode = 'service-busy';
    await tap(tester, find.byKey(const ValueKey('save-action')));
    // Another session saved meanwhile.
    client.plans[plan.id] = HostedPlan(
      id: plan.id,
      title: plan.title,
      revision: '2',
      updatedAt: DateTime.utc(2026),
    );

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1'], reason: 'no blind overwrite');
    expect(
      find.textContaining('online version changed since your last confirmed'),
      findsWidgets,
    );
    expect(find.text('Online · Shared • Not saved online'), findsOneWidget);

    await saveMenu(tester, 'save-as-online');
    await tester.enterText(
      find.byKey(const ValueKey('online-plan-title')),
      'Shared (mine)',
    );
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();
    expect(find.text('Online · Shared (mine)'), findsOneWidget);
    expect(client.calls, contains('release:${plan.id}'));
  });

  testWidgets('a lost lease is re-acquired by the next Save', (tester) async {
    await start(tester);
    final plan = client._newPlan('Idle tab', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);
    client.failRenewCode = 'lease-not-held';
    await tester.pump(const Duration(seconds: 50));
    await tester.pumpAndSettle();
    expect(find.text('Online · Idle tab • Unsaved'), findsOneWidget);

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(
      client.calls.where((call) => call == 'acquire:${plan.id}:false'),
      hasLength(2),
    );
    expect(client.savedRevisions, ['1']);
    expect(find.text('Online · Idle tab'), findsOneWidget);
  });

  testWidgets('a lease held by another session is explained on Save', (
    tester,
  ) async {
    await start(tester);
    final plan = client._newPlan('Two tabs', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);
    client.failRenewCode = 'lease-not-held';
    await tester.pump(const Duration(seconds: 50));
    await tester.pumpAndSettle();
    client.failAcquireCode = 'lease-held';

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, isEmpty);
    expect(find.textContaining('open in another tab'), findsWidgets);
    expect(find.text('Online · Two tabs • Not saved online'), findsOneWidget);
  });

  testWidgets('reopening the same plan stays explicit', (tester) async {
    await start(tester);
    final plan = client._newPlan('Reopen', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);

    await openHosted(tester, plan);
    expect(find.text('Save your changes?'), findsOneWidget);
    await tap(tester, find.text("Don't save"));
    await tester.pumpAndSettle();
    expect(
      client.calls.where((call) => call == 'release:${plan.id}'),
      hasLength(1),
    );
    expect(
      client.calls.where((call) => call == 'acquire:${plan.id}:false'),
      hasLength(2),
    );
    expect(find.text('Online · Reopen'), findsOneWidget);
  });

  for (final moved in [false, true]) {
    testWidgets(
      'a failed reopen keeps the edits and ${moved ? 'refuses' : 'allows'} Save',
      (tester) async {
        await start(tester);
        final plan = client._newPlan('Broken', seedBytes);
        await openHosted(tester, plan);
        await drawWall(tester);
        client.packages[plan.id] = Uint8List.fromList([1, 2, 3]);
        if (moved) {
          client.plans[plan.id] = HostedPlan(
            id: plan.id,
            title: plan.title,
            revision: '2',
            updatedAt: DateTime.utc(2026),
          );
        }

        await openHosted(tester, plan);
        await tap(tester, find.text("Don't save"));
        await tester.pumpAndSettle();
        await tap(tester, find.text('Close'));
        expect(
          find.text(
            moved
                ? 'Online · Broken • Not saved online'
                : 'Online · Broken • Unsaved',
          ),
          findsOneWidget,
        );

        await tap(tester, find.byKey(const ValueKey('save-action')));
        expect(client.savedRevisions, moved ? isEmpty : ['1']);
      },
    );
  }

  testWidgets(
    'sign-out is guarded by dirty confirmation and releases the lease',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Logout', seedBytes);
      await openHosted(tester, plan);
      await drawWall(tester);

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Cancel'));
      expect(client.signOutCalls, 0);
      expect(find.textContaining('Online · Logout'), findsOneWidget);

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      await tap(tester, find.text("Don't save"));
      await tester.pumpAndSettle();
      expect(client.signOutCalls, 1);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        hasLength(1),
      );
      expect(find.textContaining('Online · Logout'), findsNothing);
      expect(find.textContaining('• Unsaved'), findsOneWidget);

      // The online title never became a local file: Save asks where.
      dialogs.nextSave = null;
      tester
          .widget<InkWell>(find.byKey(const ValueKey('save-action')))
          .onTap!();
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 1);
      expect(File('Logout').existsSync(), isFalse);
    },
  );

  testWidgets('a desktop recovery copy is a file and keeps the plan online', (
    tester,
  ) async {
    final root = Directory.systemTemp.createTempSync('domowik-copy-');
    addTearDown(() => root.deleteSync(recursive: true));
    await start(tester);
    final plan = client._newPlan('Copy me', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);
    final copy = '${root.path}/copy.domo';
    dialogs.nextSave = PlanPathTarget(copy);

    await tap(tester, find.byKey(const ValueKey('save-menu')));
    await tester.tap(find.byKey(const ValueKey('save-as')));
    // The copy is real file I/O, which needs the real event loop.
    for (
      var i = 0;
      i < 50 && find.textContaining('saved a copy').evaluate().isEmpty;
      i++
    ) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 20)),
      );
      await tester.pump();
    }
    await tester.pumpAndSettle();
    expect(dialogs.suggestedName, 'Copy me.domo');
    expect(File(copy).lengthSync(), greaterThan(0));
    expect(root.listSync(), hasLength(1), reason: 'no temporary file left');
    expect(find.textContaining('Online · Copy me • Unsaved'), findsOneWidget);
    expect(client.calls.where((call) => call.startsWith('release')), isEmpty);

    // Save still goes online, never to the copy.
    tester.widget<InkWell>(find.byKey(const ValueKey('save-action'))).onTap!();
    await tester.pumpAndSettle();
    expect(client.calls.last, startsWith('save:${plan.id}'));
  });

  testWidgets('desktop sign-in keeps unsaved work open without asking', (
    tester,
  ) async {
    client
      ..restored = null
      ..leavesEditor = false;
    await start(tester);
    await drawWall(tester);
    await tap(tester, find.byKey(const ValueKey('account')));
    expect(find.text('Save your changes?'), findsNothing);
    expect(find.textContaining('• Unsaved'), findsOneWidget);
    expect(find.text('person@example.test'), findsOneWidget);
    expect(client.calls, isEmpty, reason: 'signing in uploads nothing');
  });

  testWidgets('a close releases the online lease after approval', (
    tester,
  ) async {
    await start(tester);
    final plan = client._newPlan('Closing', seedBytes);
    await openHosted(tester, plan);
    expect(await dialogs.closeHandler!(), isTrue);
    expect(client.calls.last, 'release:${plan.id}');
  });

  testWidgets('changing the server settles online work and starts signed out', (
    tester,
  ) async {
    final other = _HostedClient(seedBytes)..restored = null;
    final requested = <String?>[];
    final store = UserConfigStore.inMemory();
    await start(
      tester,
      configStore: store,
      accountClients: (saved) {
        requested.add(saved);
        return saved == 'https://other.example' ? other : client;
      },
    );
    expect(requested, [null]);
    final plan = client._newPlan('Moving', seedBytes);
    await openHosted(tester, plan);
    await drawWall(tester);

    Future<void> changeServer(String choice) async {
      await tap(tester, find.byTooltip('Preferences'));
      await tester.enterText(
        find.byKey(const ValueKey('server-url')),
        'https://Other.example/',
      );
      await tap(tester, find.widgetWithText(FilledButton, 'Save').last);
      expect(find.text('Save your changes?'), findsOneWidget);
      // The leave dialog sits above Preferences.
      await tap(tester, find.text(choice).last);
    }

    await changeServer('Cancel');
    expect(find.byType(AlertDialog), findsOneWidget, reason: 'still open');
    await tap(tester, find.text('Cancel'));
    expect(store.serverUrl, isNull);
    expect(client.signOutCalls, 0);
    expect(find.textContaining('Online · Moving • Unsaved'), findsOneWidget);

    await changeServer("Don't save");
    expect(store.serverUrl, 'https://other.example');
    expect(requested, [null, 'https://other.example']);
    expect(client.calls.last, 'release:${plan.id}');
    expect(client.signOutCalls, 1);
    expect(other.calls, isEmpty, reason: 'nothing moves to the new server');
    expect(find.textContaining('Online ·'), findsNothing);
    expect(find.textContaining('• Unsaved'), findsOneWidget);
    expect(find.text('Sign in'), findsOneWidget);
    expect(find.text('Server changed to planner.example'), findsOneWidget);
  });

  testWidgets('turning the server off removes accounts', (tester) async {
    final store = UserConfigStore.inMemory();
    await start(
      tester,
      configStore: store,
      accountClients: (saved) => saved == '' ? null : client,
    );
    await tap(tester, find.byTooltip('Preferences'));
    await tester.enterText(find.byKey(const ValueKey('server-url')), '');
    await tap(tester, find.widgetWithText(FilledButton, 'Save'));
    expect(store.serverUrl, '');
    expect(find.byKey(const ValueKey('account')), findsNothing);
    expect(client.signOutCalls, 1);
  });
}
