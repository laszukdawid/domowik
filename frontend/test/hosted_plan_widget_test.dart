import 'dart:async';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
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
  int _nextId = 1;
  int signOutCalls = 0;

  @override
  ServerEndpoint get server => ServerEndpoint('https://planner.example');

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

  @override
  Future<AccountSession?> restore() async => _accountSession;

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
  PlanByteTarget? nextSave;
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

  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1200, 850));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => FfiCore(bridge),
          accountClient: client,
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
    await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
    await tap(tester, find.byKey(const ValueKey('hosted-create')));
    await tester.enterText(
      find.byKey(const ValueKey('hosted-plan-title')),
      title,
    );
    await tap(tester, find.byKey(const ValueKey('hosted-title-confirm')));
    await tester.pumpAndSettle();
    return client.plans.values.single;
  }

  testWidgets('create, open, edit during save, local copy and hosted resave', (
    tester,
  ) async {
    await start(tester);
    final plan = await createHosted(tester, 'Project');
    expect(client.calls.take(3), [
      'list',
      'create:Project',
      'acquire:${plan.id}:false',
    ]);
    expect(
      find.textContaining('Hosted · Project · revision 1'),
      findsOneWidget,
    );

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
    expect(
      find.textContaining('Hosted · Project · revision 2 • Unsaved'),
      findsOneWidget,
    );

    Uint8List? localCopy;
    dialogs.nextSave = PlanByteTarget('Project.domo', (bytes) async {
      localCopy = Uint8List.fromList(bytes);
      return true;
    });
    await tap(tester, find.byKey(const ValueKey('save-local-copy')));
    expect(dialogs.suggestedName, 'Project.domo');
    expect(localCopy, isNotNull);
    expect(localCopy, isNot(client.savedBytes.single));
    expect(
      find.textContaining('Hosted · Project · revision 2 • Unsaved'),
      findsOneWidget,
    );

    await tap(tester, find.byKey(const ValueKey('save-action')));
    expect(client.savedRevisions, ['1', '2']);
    expect(
      find.textContaining('Hosted · Project · revision 3'),
      findsOneWidget,
    );
    expect(find.textContaining('Unsaved'), findsNothing);
    await tap(tester, find.byTooltip('New plan'));
    expect(find.text('Save your changes?'), findsNothing);
    expect(find.textContaining('Local · Untitled plan'), findsOneWidget);
  });

  testWidgets(
    'dirty cancel and failed local open preserve hosted editor state',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Existing', seedBytes);
      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(ValueKey('hosted-open-${plan.id}')));
      await tester.pumpAndSettle();
      await drawWall(tester);

      await tap(tester, find.byTooltip('New plan'));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Cancel'));
      expect(find.textContaining('Hosted · Existing'), findsOneWidget);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );

      dialogs.nextOpen = PlanBytes('bad.domo', Uint8List.fromList([1, 2, 3]));
      await tap(tester, find.byTooltip('Open plan'));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Leave unsaved'));
      await tester.pumpAndSettle();
      expect(find.textContaining('Hosted · Existing'), findsOneWidget);
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
    await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
    await tap(tester, find.byKey(ValueKey('hosted-open-${current.id}')));
    await tester.pumpAndSettle();
    expect(find.textContaining('Hosted · Current'), findsOneWidget);

    final held = client._newPlan('Held elsewhere', seedBytes);
    client.failAcquireCode = 'lease-held';
    await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
    await tap(tester, find.byKey(ValueKey('hosted-open-${held.id}')));
    await tester.pumpAndSettle();

    final error = find.byKey(const ValueKey('hosted-library-action-error'));
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
    expect(find.textContaining('Hosted · Current'), findsOneWidget);
    expect(find.byKey(const ValueKey('hosted-library-dialog')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'local plan upload is deliberate and does not clear local dirty state',
    (tester) async {
      await start(tester);
      await drawWall(tester);
      expect(client.calls.where((call) => call.startsWith('import:')), isEmpty);
      expect(
        find.textContaining('Local · Untitled plan • Unsaved'),
        findsOneWidget,
      );

      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(const ValueKey('hosted-import-current')));
      await tester.enterText(
        find.byKey(const ValueKey('hosted-plan-title')),
        'Explicit copy',
      );
      await tap(tester, find.byKey(const ValueKey('hosted-title-confirm')));
      await tester.pumpAndSettle();

      expect(client.calls, contains('import:Explicit copy'));
      expect(client.plans.values.single.title, 'Explicit copy');
      expect(
        find.textContaining('Local · Untitled plan • Unsaved'),
        findsOneWidget,
      );
      expect(find.textContaining('Hosted ·'), findsNothing);
      await tap(tester, find.text('Close'));
    },
  );

  testWidgets(
    'unconfirmed local recovery download cannot authorize replacement',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Keep me', seedBytes);
      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(ValueKey('hosted-open-${plan.id}')));
      await tester.pumpAndSettle();
      await drawWall(tester);
      dialogs.nextSave = PlanByteTarget('recovery.domo', (_) async => false);

      await tap(tester, find.byTooltip('New plan'));
      await tap(tester, find.text('Download local copy'));
      await tester.pumpAndSettle();
      expect(
        find.textContaining('Hosted · Keep me · revision 1 • Unsaved'),
        findsOneWidget,
      );
      expect(find.textContaining('Local · Untitled plan'), findsNothing);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        isEmpty,
      );
    },
  );

  testWidgets('large hosted library remains scrollable inside its bound', (
    tester,
  ) async {
    await start(tester);
    for (var i = 0; i < 35; i++) {
      client._newPlan('Plan $i', seedBytes);
    }
    await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
    expect(find.byKey(const ValueKey('hosted-library-dialog')), findsOneWidget);
    expect(tester.takeException(), isNull);
    final list = find.descendant(
      of: find.byKey(const ValueKey('hosted-library-dialog')),
      matching: find.byType(ListView),
    );
    await tester.scrollUntilVisible(
      find.byKey(const ValueKey('hosted-open-hosted-35')),
      250,
      scrollable: find.descendant(of: list, matching: find.byType(Scrollable)),
    );
    expect(find.byKey(const ValueKey('hosted-open-hosted-35')), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'ambiguous save blocks overwrite and same-plan reopen is explicit',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Recover', seedBytes);
      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(ValueKey('hosted-open-${plan.id}')));
      await tester.pumpAndSettle();
      await drawWall(tester);
      client.failSaveCode = 'service-busy';
      await tap(tester, find.byKey(const ValueKey('save-action')));
      expect(client.savedRevisions, ['1']);
      expect(find.textContaining('Save blocked'), findsOneWidget);

      await tap(tester, find.byKey(const ValueKey('save-action')));
      expect(client.savedRevisions, [
        '1',
      ], reason: 'ambiguous retry is blocked');

      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(ValueKey('hosted-open-${plan.id}')));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Leave unsaved'));
      await tester.pumpAndSettle();
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        hasLength(1),
      );
      expect(
        client.calls.where((call) => call == 'acquire:${plan.id}:false'),
        hasLength(2),
      );
      expect(
        find.textContaining('Hosted · Recover · revision 1'),
        findsOneWidget,
      );
      expect(find.textContaining('Unsaved'), findsNothing);
    },
  );

  testWidgets(
    'sign-out is guarded by dirty confirmation and releases the lease',
    (tester) async {
      await start(tester);
      final plan = client._newPlan('Logout', seedBytes);
      await tap(tester, find.byKey(const ValueKey('hosted-plan-library')));
      await tap(tester, find.byKey(ValueKey('hosted-open-${plan.id}')));
      await tester.pumpAndSettle();
      await drawWall(tester);

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      expect(find.text('Save your changes?'), findsOneWidget);
      await tap(tester, find.text('Cancel'));
      expect(client.signOutCalls, 0);
      expect(find.textContaining('Hosted · Logout'), findsOneWidget);

      await tap(tester, find.byKey(const ValueKey('account')));
      await tap(tester, find.byKey(const ValueKey('sign-out')));
      await tap(tester, find.text('Leave unsaved'));
      await tester.pumpAndSettle();
      expect(client.signOutCalls, 1);
      expect(
        client.calls.where((call) => call == 'release:${plan.id}'),
        hasLength(1),
      );
      expect(find.textContaining('Local · Logout • Unsaved'), findsOneWidget);
    },
  );
}
