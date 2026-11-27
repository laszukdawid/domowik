import 'dart:async';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/property_planner.dart';
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
  String? failDeleteCode;
  String? failSignOutMessage;
  String? failSignOutCode;
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
    if (failSignOutMessage != null) {
      throw AccountException(failSignOutMessage!, code: failSignOutCode);
    }
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
  Future<HostedPlan> renamePlan(
    AccountSession session,
    String planId,
    String title,
  ) async {
    calls.add('rename:$planId:$title');
    final existing = plans[planId]!;
    final renamed = HostedPlan(
      id: existing.id,
      title: title,
      revision: existing.revision,
      updatedAt: DateTime.utc(2026),
    );
    plans[planId] = renamed;
    return renamed;
  }

  @override
  Future<void> deletePlan(AccountSession session, String planId) async {
    calls.add('delete:$planId');
    final code = failDeleteCode;
    failDeleteCode = null;
    if (code != null) {
      throw AccountException('could not delete plan', code: code);
    }
    if (!plans.containsKey(planId)) {
      throw const AccountException('missing plan', code: 'plan-not-found');
    }
    plans.remove(planId);
    packages.remove(planId);
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

class _PropertyHostedClient extends _HostedClient
    implements PropertyPlannerClient {
  _PropertyHostedClient(super.packageBytes);

  final linked = <PlannerPlanSummary>[];
  String propertyName = 'Maple House';
  String propertyId = 'property-1';
  final propertyHints = <String?>[];
  Completer<PlannerPropertyOverview>? pendingOverview;

  @override
  Future<PlannerPropertyOverview> propertyOverview(
    AccountSession session, {
    String? propertyHint,
  }) async {
    propertyHints.add(propertyHint);
    if (pendingOverview != null) return pendingOverview!.future;
    return PlannerPropertyOverview(
      property: PlannerNamedId(id: propertyId, name: propertyName),
      locations: const [PlannerNamedId(id: 'loc-1', name: 'Kitchen')],
      plans: List.of(linked),
    );
  }

  @override
  Future<PlannerPlanContext> planPropertyContext(
    AccountSession session,
    String planId,
  ) async {
    final summary = linked
        .where((candidate) => candidate.id == planId)
        .firstOrNull;
    if (summary == null) {
      throw const AccountException(
        'this plan has no property link',
        statusCode: 409,
        code: 'property-unlinked',
      );
    }
    return PlannerPlanContext(
      plan: summary,
      property: PlannerNamedId(id: propertyId, name: propertyName),
      locations: const [PlannerNamedId(id: 'loc-1', name: 'Kitchen')],
      roomLinks: const [],
    );
  }

  @override
  Future<void> deletePlan(AccountSession session, String planId) async {
    await super.deletePlan(session, planId);
    linked.removeWhere((summary) => summary.id == planId);
  }

  @override
  Future<void> linkPlanProperty(
    AccountSession session,
    String planId,
    String propertyId,
  ) async {
    calls.add('linkProperty:$planId:$propertyId');
    final plan = plans[planId];
    if (plan != null && !linked.any((summary) => summary.id == planId)) {
      linked.add(PlannerPlanSummary(id: plan.id, title: plan.title));
    }
  }

  @override
  Future<void> unlinkPlanProperty(AccountSession session, String planId) async {
    calls.add('unlinkProperty:$planId');
    linked.removeWhere((summary) => summary.id == planId);
  }

  @override
  Future<void> linkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
    String locationId,
  ) async {}

  @override
  Future<void> unlinkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
  ) async {}

  @override
  Future<PlannerRoomInventory> roomInventory(
    AccountSession session,
    String planId,
    String roomId,
  ) => throw UnimplementedError();

  @override
  Future<bool> openItemUri(Uri uri) async => true;
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
    AccountClient? accountClient,
    AccountClient? Function(String? savedServerUrl)? accountClients,
    UserConfigStore? configStore,
    String? propertyHint,
    Uri? startupUri,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1200, 850));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => FfiCore(bridge),
          accountClient: accountClients == null
              ? (accountClient ?? client)
              : null,
          accountClients: accountClients,
          configStore: configStore,
          fileDialogs: dialogs,
          propertyHint: propertyHint,
          startupUri: startupUri,
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
    await tap(tester, find.byKey(const ValueKey('new-action')));
    await tester.enterText(
      find.byKey(const ValueKey('online-plan-title')),
      title,
    );
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();
    return client.plans.values.single;
  }

  Future<void> showOnlinePlans(WidgetTester tester) async {
    await tap(tester, find.byKey(const ValueKey('open-action')));
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
    await tap(tester, find.text('New local plan'));
  }

  void expectOnlineTitle(String title) {
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('plan-storage-badge')),
        matching: find.text('Online'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('plan-title')),
        matching: find.text(title),
        matchRoot: true,
      ),
      findsOneWidget,
    );
  }

  void expectOfflineTitle(String title) {
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('plan-storage-badge')),
        matching: find.text('Offline'),
      ),
      findsOneWidget,
    );
    expect(
      find.descendant(
        of: find.byKey(const ValueKey('plan-title')),
        matching: find.text(title),
        matchRoot: true,
      ),
      findsOneWidget,
    );
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
    expectOnlineTitle('Project');

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
    expectOnlineTitle('Project • Unsaved');

    Uint8List? localCopy;
    dialogs.nextSave = PlanByteTarget('Project.domo', (bytes) async {
      localCopy = Uint8List.fromList(bytes);
      return true;
    });
    await saveMenu(tester, 'save-as');
    expect(dialogs.suggestedName, 'Project.domo');
    expect(localCopy, isNotNull);
    expect(localCopy, isNot(client.savedBytes.single));
    expectOnlineTitle('Project • Unsaved');

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1', '2']);
    expectOnlineTitle('Project');
    expect(find.textContaining('Unsaved'), findsNothing);
    await newLocalPlan(tester);
    expect(find.text('Save your changes?'), findsNothing);
    expectOfflineTitle('Untitled plan');
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
      expectOnlineTitle('Existing • Unsaved');
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );

      dialogs.nextOpen = PlanBytes('bad.domo', Uint8List.fromList([1, 2, 3]));
      await tap(tester, find.byKey(const ValueKey('open-menu')));
      await tap(tester, find.text('Open local file…'));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text("Don't save"));
      await tester.pumpAndSettle();
      expectOnlineTitle('Existing • Unsaved');
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
    expectOnlineTitle('Current');

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
    expectOnlineTitle('Current');
    expect(find.byKey(const ValueKey('online-plans-dialog')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets('signed-in Save defaults to online storage and adopts the plan', (
    tester,
  ) async {
    await start(tester);
    await drawWall(tester);
    expect(client.calls.where((call) => call.startsWith('import:')), isEmpty);
    expectOfflineTitle('Untitled plan • Unsaved');

    await tap(tester, find.byKey(const ValueKey('save-action')));
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
    expectOnlineTitle('Explicit copy');

    await drawWall(tester, y: 260);
    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1']);
    expect(dialogs.saveCalls, 0);
    expectOnlineTitle('Explicit copy');
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
    expectOfflineTitle('Untitled plan • Unsaved');
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
      expectOnlineTitle('Keep me • Unsaved');
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
    expectOnlineTitle('Recover • Not saved online');

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1', '1']);
    expect(client.calls, contains('renew:${plan.id}'));
    expectOnlineTitle('Recover');
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
    expectOnlineTitle('Shared • Not saved online');

    await saveMenu(tester, 'save-as-online');
    await tester.enterText(
      find.byKey(const ValueKey('online-plan-title')),
      'Shared (mine)',
    );
    await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
    await tester.pumpAndSettle();
    expectOnlineTitle('Shared (mine)');
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
    expectOnlineTitle('Idle tab • Unsaved');

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(
      client.calls.where((call) => call == 'acquire:${plan.id}:false'),
      hasLength(2),
    );
    expect(client.savedRevisions, ['1']);
    expectOnlineTitle('Idle tab');
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
    expectOnlineTitle('Two tabs • Not saved online');
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
    expectOnlineTitle('Reopen');
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
        expectOnlineTitle(
          moved ? 'Broken • Not saved online' : 'Broken • Unsaved',
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
      expectOnlineTitle('Logout • Unsaved');

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      await tap(tester, find.text("Don't save"));
      await tester.pumpAndSettle();
      expect(client.signOutCalls, 1);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        hasLength(1),
      );
      expectOfflineTitle('Untitled plan • Unsaved');

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
    expectOnlineTitle('Copy me • Unsaved');
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
    expectOnlineTitle('Moving • Unsaved');

    await changeServer("Don't save");
    expect(store.serverUrl, 'https://other.example');
    expect(requested, [null, 'https://other.example']);
    expect(client.calls.last, 'release:${plan.id}');
    expect(client.signOutCalls, 1);
    expect(other.calls, isEmpty, reason: 'nothing moves to the new server');
    expectOfflineTitle('Untitled plan • Unsaved');
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

  testWidgets(
    'visiting with a property hint automatically opens its linked plan',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes);
      final plan = propertyClient._newPlan('Ground Floor', seedBytes);
      propertyClient.linked.add(
        PlannerPlanSummary(id: plan.id, title: plan.title),
      );

      await start(
        tester,
        accountClient: propertyClient,
        propertyHint: 'property-1',
      );

      expect(propertyClient.propertyHints, ['property-1']);
      expectOnlineTitle('Ground Floor');
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsNothing,
      );
    },
  );

  testWidgets(
    'visiting with a property hint opens Property plans when none is linked yet',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes);

      await start(
        tester,
        accountClient: propertyClient,
        propertyHint: 'property-1',
      );

      expect(propertyClient.propertyHints, ['property-1']);
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsOneWidget,
      );
      expect(find.text('HomeHero property: Maple House'), findsOneWidget);
      expect(
        find.text('Sign-in and property access are checked separately.'),
        findsNothing,
      );
    },
  );

  testWidgets('slow property lookup does not clobber a renamed blank plan', (
    tester,
  ) async {
    final overviewCompleter = Completer<PlannerPropertyOverview>();
    final propertyClient = _PropertyHostedClient(seedBytes)
      ..pendingOverview = overviewCompleter;
    final plan = propertyClient._newPlan('Ground Floor', seedBytes);
    propertyClient.linked.add(
      PlannerPlanSummary(id: plan.id, title: plan.title),
    );

    await start(
      tester,
      accountClient: propertyClient,
      propertyHint: 'property-1',
    );

    // User renames the blank plan while the property lookup is still pending.
    await tap(tester, find.byKey(const ValueKey('plan-title')));
    await tester.enterText(
      find.byKey(const ValueKey('plan-title-input')),
      'My Local Sketch',
    );
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pumpAndSettle();
    expectOfflineTitle('My Local Sketch');

    overviewCompleter.complete(
      PlannerPropertyOverview(
        property: const PlannerNamedId(id: 'property-1', name: 'Maple House'),
        locations: const [PlannerNamedId(id: 'loc-1', name: 'Kitchen')],
        plans: List.of(propertyClient.linked),
      ),
    );
    await tester.pumpAndSettle();

    expectOfflineTitle('My Local Sketch');
    expect(find.byKey(const ValueKey('property-planner-dialog')), findsNothing);
  });

  testWidgets(
    'failed auto-open checkout falls back to Property plans with alternatives',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes)
        ..failAcquireCode = 'lease-held';
      final first = propertyClient._newPlan('Ground Floor', seedBytes);
      final second = propertyClient._newPlan('Renovation', seedBytes);
      propertyClient.linked.addAll([
        PlannerPlanSummary(id: first.id, title: first.title),
        PlannerPlanSummary(id: second.id, title: second.title),
      ]);

      await start(
        tester,
        accountClient: propertyClient,
        propertyHint: 'property-1',
      );

      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsOneWidget,
      );
      expect(
        find.byKey(const ValueKey('property-planner-error')),
        findsOneWidget,
      );
      expect(find.text('Ground Floor'), findsOneWidget);
      expect(find.text('Renovation'), findsOneWidget);

      // Clicking the alternative plan card opens it directly.
      await tap(tester, find.byKey(ValueKey('property-plan-${second.id}')));
      expectOnlineTitle('Renovation');
    },
  );

  testWidgets(
    'deletes unwanted online plans after confirmation and detaches cleanly when deleting the open plan',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes);
      final openPlan = propertyClient._newPlan('Active Layout', seedBytes);
      final sparePlan = propertyClient._newPlan('Old Draft', seedBytes);
      propertyClient.linked.add(
        PlannerPlanSummary(id: openPlan.id, title: openPlan.title),
      );

      await start(tester, accountClient: propertyClient);
      await openHosted(tester, openPlan);
      await drawWall(tester);
      expectOnlineTitle('Active Layout • Unsaved');

      // Open the hosted library and cancel deleting the spare plan first.
      await showOnlinePlans(tester);
      await tap(tester, find.byKey(ValueKey('online-delete-${sparePlan.id}')));
      expect(
        find.byKey(const ValueKey('online-delete-confirm-dialog')),
        findsOneWidget,
      );
      expect(find.textContaining('currently open'), findsNothing);
      await tap(tester, find.byKey(const ValueKey('online-delete-cancel')));
      expect(propertyClient.plans.containsKey(sparePlan.id), isTrue);

      // Confirm deleting the spare plan: removes it and keeps Active Layout open.
      await tap(tester, find.byKey(ValueKey('online-delete-${sparePlan.id}')));
      await tap(tester, find.byKey(const ValueKey('online-delete-confirm')));
      await tester.pumpAndSettle();
      expect(propertyClient.calls, contains('delete:${sparePlan.id}'));
      expect(propertyClient.plans.containsKey(sparePlan.id), isFalse);
      expect(find.text('Old Draft'), findsNothing);
      expectOnlineTitle('Active Layout • Unsaved');

      // Delete the currently open, property-linked plan: confirmation warns about
      // both open state and property linkage, then detaches cleanly without
      // losing the drawn wall on the canvas.
      await tap(tester, find.byKey(ValueKey('online-delete-${openPlan.id}')));
      expect(
        find.byKey(const ValueKey('online-delete-confirm-dialog')),
        findsOneWidget,
      );
      expect(find.textContaining('currently open'), findsOneWidget);
      expect(
        find.textContaining('linked to property "Maple House"'),
        findsOneWidget,
      );
      await tap(tester, find.byKey(const ValueKey('online-delete-confirm')));
      await tester.pumpAndSettle();

      expect(propertyClient.calls, contains('delete:${openPlan.id}'));
      expect(
        propertyClient.calls.where((call) => call == 'release:${openPlan.id}'),
        isEmpty,
        reason: 'deleted plan needs no redundant lease release',
      );
      expect(find.text('No online plans yet'), findsOneWidget);
      await tap(tester, find.text('Close'));

      expectOfflineTitle('Untitled plan • Unsaved');
      expect(
        find.textContaining(
          'Deleted online plan "Active Layout"; current plan remains open locally',
        ),
        findsOneWidget,
      );

      // The drawn wall is still in memory: Undo is enabled and Save uploads it
      // as a fresh online plan.
      expect(
        tester
            .widget<IconButton>(find.widgetWithIcon(IconButton, Icons.undo))
            .onPressed,
        isNotNull,
      );
      await tap(tester, find.byKey(const ValueKey('save-action')));
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Recovered Layout',
      );
      await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();
      expectOnlineTitle('Recovered Layout');
      expect(propertyClient.plans.values.single.title, 'Recovered Layout');
    },
  );

  testWidgets(
    'visiting with a property hint, closing the empty modal, drawing, and saving links the plan so reopening auto-opens it',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes);

      await start(
        tester,
        accountClient: propertyClient,
        propertyHint: 'property-1',
      );
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsOneWidget,
      );
      await tap(tester, find.text('Close'));

      await drawWall(tester);
      await tap(tester, find.byKey(const ValueKey('save-action')));
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Main Residence',
      );
      await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      final savedPlan = propertyClient.plans.values.single;
      expect(
        propertyClient.calls,
        contains('linkProperty:${savedPlan.id}:property-1'),
      );
      expectOnlineTitle('Main Residence');
      expect(
        find.textContaining('Saved as new property plan "Main Residence"'),
        findsOneWidget,
      );

      // Simulate reopening the editor with the same ?property=property-1 hint:
      // it now auto-opens "Main Residence" instead of showing the empty dialog.
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pumpWidget(
        MaterialApp(
          home: PlanPage(
            bridgeLoader: () => FfiCore(bridge),
            accountClient: propertyClient,
            fileDialogs: dialogs,
            propertyHint: 'property-1',
          ),
        ),
      );
      await tester.pumpAndSettle();

      expectOnlineTitle('Main Residence');
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsNothing,
      );
    },
  );

  testWidgets(
    'drawing before desktop sign-in preserves property hint on Save, while opening an unlinked plan clears it',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes)
        ..restored = null
        ..leavesEditor = false;
      final unlinked = propertyClient._newPlan('Separate Cabin', seedBytes);

      await start(
        tester,
        accountClient: propertyClient,
        propertyHint: 'property-1',
      );
      await drawWall(tester);
      await tap(tester, find.byKey(const ValueKey('account')));
      expect(find.text('person@example.test'), findsOneWidget);

      await tap(tester, find.byKey(const ValueKey('save-action')));
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Drawn Before Sign-In',
      );
      await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      final linkedPlan = propertyClient.plans.values.firstWhere(
        (plan) => plan.title == 'Drawn Before Sign-In',
      );
      expect(
        propertyClient.calls,
        contains('linkProperty:${linkedPlan.id}:property-1'),
      );

      // Opening an unlinked plan from Open online plan and saving a copy does
      // NOT link the unrelated copy to property-1.
      propertyClient.calls.clear();
      await openHosted(tester, unlinked);
      await saveMenu(tester, 'save-as-online');
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Cabin Copy',
      );
      await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();
      expect(
        propertyClient.calls.where((call) => call.startsWith('linkProperty:')),
        isEmpty,
      );

      // Desktop offline sign-out still detaches and clears the local session.
      propertyClient.failSignOutMessage = 'Could not reach server';
      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      expect(propertyClient.signOutCalls, 1);
      expect(find.text('Sign in'), findsOneWidget);
      expect(find.textContaining('Signed out on this device'), findsOneWidget);
    },
  );

  testWidgets(
    'opening a property-linked plan via Open online plan preserves its property link on Save as new online plan',
    (tester) async {
      final propertyClient = _PropertyHostedClient(seedBytes);
      final linkedOriginal = propertyClient._newPlan('Main House', seedBytes);
      propertyClient.linked.add(
        PlannerPlanSummary(id: linkedOriginal.id, title: linkedOriginal.title),
      );

      // Open without any ?property= URL hint, via Open online plan.
      await start(tester, accountClient: propertyClient);
      await openHosted(tester, linkedOriginal);
      expectOnlineTitle('Main House');

      await saveMenu(tester, 'save-as-online');
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Main House Option B',
      );
      await tap(tester, find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      final copied = propertyClient.plans.values.firstWhere(
        (plan) => plan.title == 'Main House Option B',
      );
      expect(
        propertyClient.calls,
        contains('linkProperty:${copied.id}:property-1'),
      );
      expectOnlineTitle('Main House Option B');
    },
  );

  testWidgets(
    'desktop local-signout-failed keeps session active and refuses to claim device sign-out',
    (tester) async {
      final desktopClient = _HostedClient(seedBytes)
        ..leavesEditor = false
        ..failSignOutMessage =
            'Could not remove saved credentials from this device'
        ..failSignOutCode = 'local-signout-failed';

      await start(tester, accountClient: desktopClient);
      expect(find.text('person@example.test'), findsOneWidget);

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));

      expect(desktopClient.signOutCalls, 1);
      expect(find.text('person@example.test'), findsOneWidget);
      expect(find.text('Sign in'), findsNothing);
      expect(find.textContaining('Signed out on this device'), findsNothing);
      expect(
        find.textContaining('Could not sign out on this device'),
        findsWidgets,
      );

      // Once local storage is writable again, retrying sign-out succeeds.
      desktopClient
        ..failSignOutMessage = null
        ..failSignOutCode = null;
      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      expect(desktopClient.signOutCalls, 2);
      expect(find.text('Sign in'), findsOneWidget);
    },
  );
}
