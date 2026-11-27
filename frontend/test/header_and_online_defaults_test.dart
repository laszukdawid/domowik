import 'dart:convert';
import 'dart:typed_data';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/main.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/property_planner.dart';
import 'package:domowik_editor/web_graphics_policy.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _session = AccountSession(token: 'csrf');

/// Minimal in-memory [DomowikCore] so [PlanPage] header and online/local file
/// workflows can be verified end-to-end without requiring the native library.
class _FakeCore implements DomowikCore {
  @override
  bool get supportsPaths => false;

  int _nextHandle = 1;
  final Map<int, _FakeDoc> _docs = {};

  @override
  int abiVersion() => expectedAbi;

  @override
  String? loadedImagePath() => null;

  @override
  String? get failure => null;

  @override
  CoreHandle createDefault() {
    final handle = _nextHandle++;
    _docs[handle] = _FakeDoc();
    return CoreHandle(ok, handle);
  }

  @override
  CoreHandle openPath(String path) => createDefault();

  @override
  CoreHandle openBytes(Uint8List bytes) => createDefault();

  @override
  int savePath(int handle, String path) {
    final doc = _docs[handle];
    if (doc == null) return unknownHandle;
    doc.savedRevision = doc.revision;
    return ok;
  }

  @override
  CoreEncoded encode(int handle) {
    final doc = _docs[handle];
    if (doc == null) return const CoreEncoded(unknownHandle, null, 0);
    return CoreEncoded(
      ok,
      Uint8List.fromList(utf8.encode('fake-domo-rev-${doc.revision}')),
      doc.revision,
    );
  }

  @override
  int markSaved(int handle, int revision) {
    final doc = _docs[handle];
    if (doc == null) return unknownHandle;
    doc.savedRevision = revision;
    return ok;
  }

  @override
  int close(int handle) {
    _docs.remove(handle);
    return ok;
  }

  @override
  CoreReply execute(int handle, String request) {
    final doc = _docs[handle];
    if (doc == null) return const CoreReply.failed(unknownHandle);
    doc.revision++;
    return const CoreReply(ok, text: '{"batch_id":"b1"}');
  }

  @override
  CoreReply edit(int handle, String request) => execute(handle, request);

  @override
  CoreReply assets(int handle, String request) => const CoreReply(
    ok,
    text: '{"directories":[],"models":[],"instances":[],"clipboardCount":0}',
  );

  @override
  CoreReply hitTest(int handle, String request) =>
      const CoreReply(ok, text: '{"hits":[]}');

  @override
  CoreReply marquee(int handle, String request) =>
      const CoreReply(ok, text: '{"ids":[]}');

  @override
  CoreReply snap(int handle, String request) =>
      const CoreReply(ok, text: '{"snapped":false}');

  @override
  CoreReply stairPreview(int handle, String request) =>
      const CoreReply(ok, text: '{"lines":[]}');

  @override
  CoreReply undo(int handle) => const CoreReply(ok, text: '{"batch_id":""}');

  @override
  CoreReply redo(int handle) => const CoreReply(ok, text: '{"batch_id":""}');

  @override
  CoreReply state(int handle) {
    final doc = _docs[handle];
    if (doc == null) return const CoreReply.failed(unknownHandle);
    return CoreReply(
      ok,
      text: jsonEncode({
        'dirty': doc.dirty,
        'undo_depth': 0,
        'redo_depth': 0,
        'document_revision': doc.revision,
        'warnings': <Object?>[],
      }),
    );
  }

  @override
  CoreReply snapshot(int handle) => CoreReply(
    ok,
    text: jsonEncode({
      'documentId': 'doc-$handle',
      'levels': [
        {
          'id': 'level-1',
          'name': 'Ground Floor',
          'elevationMm': 0.0,
          'floorThicknessMm': 200.0,
          'defaultWallHeightMm': 2500.0,
          'defaultCeilingHeightMm': 2500.0,
        },
      ],
      'walls': <Object?>[],
      'openings': <Object?>[],
      'rooms': <Object?>[],
      'stairs': <Object?>[],
      'assets': <Object?>[],
    }),
  );

  @override
  CoreReply derived(int handle, String levelId) => CoreReply(
    ok,
    text: jsonEncode({
      'occupied': <Object?>[],
      'rooms': <Object?>[],
      'openings': <Object?>[],
      'floorAreaMm2': 0.0,
    }),
  );

  @override
  CoreReply importModel(int handle, String name, Uint8List bytes) =>
      const CoreReply.failed(rejected);

  @override
  CoreReply sceneFrame(int handle, String request) =>
      const CoreReply.failed(rejected);

  @override
  CoreReply exportUsdz(int handle) => const CoreReply.failed(rejected);

  @override
  Future<Object?> assetsInBackground(
    int handle,
    Map<String, Object?> request,
  ) async => {
    'directories': <Object?>[],
    'models': <Object?>[],
    'instances': <Object?>[],
    'clipboardCount': 0,
  };

  @override
  String? lastError(int handle) => null;

  @override
  int outstandingBuffers() => 0;

  @override
  int openPlans() => _docs.length;

  @override
  int debugPanic() => ok;
}

class _FakeDoc {
  int revision = 0;
  int savedRevision = 0;
  bool get dirty => revision != savedRevision;
}

class _WebPropertyAccountClient
    implements AccountClient, HostedPlanClient, PropertyPlannerClient {
  _WebPropertyAccountClient({this.restoredSession});

  AccountSession? restoredSession;
  bool failPropertyLink = false;
  final calls = <String>[];
  final plans = <String, HostedPlan>{};
  final linkedPlanIds = <String>{};
  int _nextId = 1;

  @override
  ServerEndpoint get server => ServerEndpoint('https://planner.homehero.pro');

  @override
  bool get signInLeavesEditor => true;

  @override
  Future<AccountSession?> restore() async => restoredSession;

  @override
  Future<AccountSession?> signIn() async => _session;

  @override
  Future<void> signOut(AccountSession session) async {}

  @override
  Future<AccountProfile> profile(AccountSession session) async =>
      AccountProfile(
        email: 'user@homehero.pro',
        plans: plans.values.toList(),
        planLimit: 5,
      );

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) async {
    calls.add('list');
    return plans.values.toList();
  }

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) async {
    calls.add('create:$title');
    final id = 'plan-${_nextId++}';
    final plan = HostedPlan(
      id: id,
      title: title,
      revision: '1',
      updatedAt: DateTime.utc(2026),
    );
    plans[id] = plan;
    return plan;
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
    plans.remove(planId);
    linkedPlanIds.remove(planId);
  }

  @override
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  ) async {
    calls.add('import:$title');
    final id = 'plan-${_nextId++}';
    final plan = HostedPlan(
      id: id,
      title: title,
      revision: '1',
      updatedAt: DateTime.utc(2026),
    );
    plans[id] = plan;
    return plan;
  }

  @override
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  }) async {
    calls.add('acquire:$planId');
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
    return HostedPlanDownload(
      bytes: Uint8List.fromList([1, 2, 3]),
      revision: plans[planId]!.revision,
    );
  }

  @override
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  ) async {
    calls.add('save:$planId:$expectedRevision');
    final updated = HostedPlan(
      id: planId,
      title: plans[planId]!.title,
      revision: '${int.parse(expectedRevision) + 1}',
      updatedAt: DateTime.utc(2026),
    );
    plans[planId] = updated;
    return updated;
  }

  @override
  Future<PlannerPropertyOverview> propertyOverview(
    AccountSession session, {
    String? propertyHint,
  }) async {
    calls.add('propertyOverview:$propertyHint');
    if (failPropertyLink) {
      throw const AccountException('No synced property found');
    }
    return PlannerPropertyOverview(
      property: const PlannerNamedId(id: 'prop-1', name: 'My Home'),
      locations: const [],
      plans: [
        for (final id in linkedPlanIds)
          PlannerPlanSummary(id: id, title: plans[id]!.title),
      ],
    );
  }

  @override
  Future<void> linkPlanProperty(
    AccountSession session,
    String planId,
    String propertyId,
  ) async {
    calls.add('linkPlanProperty:$planId:$propertyId');
    if (failPropertyLink) {
      throw const AccountException('Could not link property');
    }
    linkedPlanIds.add(planId);
  }

  @override
  Future<void> unlinkPlanProperty(AccountSession session, String planId) async {
    linkedPlanIds.remove(planId);
  }

  @override
  Future<PlannerPlanContext> planPropertyContext(
    AccountSession session,
    String planId,
  ) => throw UnimplementedError();

  @override
  Future<void> linkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
    String locationId,
  ) => throw UnimplementedError();

  @override
  Future<void> unlinkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
  ) => throw UnimplementedError();

  @override
  Future<PlannerRoomInventory> roomInventory(
    AccountSession session,
    String planId,
    String roomId,
  ) => throw UnimplementedError();

  @override
  Future<bool> openItemUri(Uri uri) async => true;
}

class _FakeFileDialogs implements FileDialogs {
  int openCalls = 0;
  int saveCalls = 0;
  String? lastSuggested;
  PlanSource? nextOpen;
  bool confirmLocalWrite = true;
  String localSaveName = 'local-house.domo';

  @override
  Future<PlanSource?> openPlan() async {
    openCalls++;
    return nextOpen;
  }

  @override
  Future<PlanTarget?> savePlan(String suggestedName) async {
    saveCalls++;
    lastSuggested = suggestedName;
    return PlanByteTarget(localSaveName, (_) async => confirmLocalWrite);
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) {}
}

void main() {
  Future<void> pumpEditor(
    WidgetTester tester, {
    required _FakeCore core,
    required _WebPropertyAccountClient client,
    required _FakeFileDialogs dialogs,
    String? propertyHint,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1280, 820));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => core,
          accountClient: client,
          fileDialogs: dialogs,
          propertyHint: propertyHint,
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'signed-out user sees two-row header with Offline badge, top-right settings+login, and local Save/New defaults',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: null);
      final dialogs = _FakeFileDialogs();
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      // Row 1 vs Row 2 placement.
      final topRow = tester.getRect(
        find.byKey(const ValueKey('header-top-row')),
      );
      final actionsRow = tester.getRect(
        find.byKey(const ValueKey('header-actions-row')),
      );
      expect(topRow.bottom, lessThanOrEqualTo(actionsRow.top + 1));

      // Top-left identity: Offline badge + Untitled plan.
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
          matching: find.text('Untitled plan'),
          matchRoot: true,
        ),
        findsOneWidget,
      );

      // Top-right corner: Settings then Sign in button.
      final prefs = tester.getRect(find.byKey(const ValueKey('preferences')));
      final account = tester.getRect(find.byKey(const ValueKey('account')));
      expect(prefs.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(account.center.dy, inInclusiveRange(topRow.top, topRow.bottom));
      expect(account.left, greaterThanOrEqualTo(prefs.right));

      // Second row: New/Open/Furniture left-aligned under Domowik; Save right-aligned.
      final newBtn = tester.getRect(find.byKey(const ValueKey('new-action')));
      final saveBtn = tester.getRect(find.byKey(const ValueKey('save-action')));
      expect(
        newBtn.center.dy,
        inInclusiveRange(actionsRow.top, actionsRow.bottom),
      );
      expect(
        saveBtn.center.dy,
        inInclusiveRange(actionsRow.top, actionsRow.bottom),
      );
      expect(saveBtn.right, greaterThan(1100));

      // Default Save when logged out saves to a local file.
      await tester.tap(find.byKey(const ValueKey('save-action')));
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 1);
      expect(dialogs.lastSuggested, 'plan.domo');
      expect(client.calls, isEmpty);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('local-house.domo'),
          matchRoot: true,
        ),
        findsOneWidget,
      );

      // Default New when logged out creates a new local plan.
      await tester.tap(find.byKey(const ValueKey('new-action')));
      await tester.pumpAndSettle();
      expect(client.calls, isEmpty);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('Untitled plan'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'signed-in Save defaults to online storage while Save as allows local storing',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session);
      final dialogs = _FakeFileDialogs();
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      // Initially Offline untitled plan.
      expect(find.text('Offline'), findsOneWidget);
      expect(find.text('user@homehero.pro'), findsOneWidget);

      // Save as local file from dropdown writes locally without touching online storage.
      await tester.tap(find.byKey(const ValueKey('save-menu')));
      await tester.pumpAndSettle();
      expect(find.text('Save as local file (.domo)…'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('save-as')));
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 1);
      expect(client.calls, isEmpty);

      // Reset to a fresh local untitled plan (unbound) via New dropdown.
      await tester.tap(find.byKey(const ValueKey('new-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('New local plan'));
      await tester.pumpAndSettle();

      // Default Save on an unbound plan while signed in saves to online storage.
      await tester.tap(find.byKey(const ValueKey('save-action')));
      await tester.pumpAndSettle();
      expect(find.text('Save online plan'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Cloud Home',
      );
      await tester.tap(find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      expect(client.calls, ['import:Cloud Home', 'acquire:plan-1']);
      expect(dialogs.saveCalls, 1, reason: 'did not trigger local save dialog');
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
          matching: find.text('Cloud Home'),
          matchRoot: true,
        ),
        findsOneWidget,
      );

      // Subsequent Save updates the online plan directly.
      await tester.tap(find.byKey(const ValueKey('save-action')));
      await tester.pumpAndSettle();
      expect(client.calls.last, 'save:plan-1:1');

      // Save as (Download a copy) still allows local storing while staying online.
      await tester.tap(find.byKey(const ValueKey('save-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('save-as')));
      await tester.pumpAndSettle();
      expect(dialogs.saveCalls, 2);
      expect(dialogs.lastSuggested, 'Cloud Home.domo');
      expect(find.text('Online'), findsOneWidget);
    },
  );

  testWidgets(
    'signed-in New defaults to starting a new property plan linked to the user property',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session);
      final dialogs = _FakeFileDialogs();
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      await tester.tap(find.byKey(const ValueKey('new-action')));
      await tester.pumpAndSettle();
      expect(find.text('New property plan'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Renovation layout',
      );
      await tester.tap(find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      expect(client.calls, [
        'create:Renovation layout',
        'propertyOverview:null',
        'linkPlanProperty:plan-1:prop-1',
        'acquire:plan-1',
        'download:plan-1',
      ]);
      expect(client.linkedPlanIds, contains('plan-1'));
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
          matching: find.text('Renovation layout'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
      expect(
        find.textContaining('Opened property plan "Renovation layout"'),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'signed-in New still opens the created online plan when property linking fails',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session)
        ..failPropertyLink = true;
      final dialogs = _FakeFileDialogs();
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      await tester.tap(find.byKey(const ValueKey('new-action')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Fallback layout',
      );
      await tester.tap(find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      expect(client.linkedPlanIds, isEmpty);
      expect(find.text('Online'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('Fallback layout'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
      expect(find.textContaining('could not link to property'), findsOneWidget);
    },
  );

  testWidgets(
    'signed-in Open defaults to online plans and its dropdown opens local files',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session);
      await client.createPlan(_session, 'Existing Home');
      client.calls.clear();
      final dialogs = _FakeFileDialogs()
        ..nextOpen = PlanBytes('disk.domo', Uint8List.fromList([1, 2, 3]));
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      // Primary Open action opens online plans dialog directly.
      await tester.tap(find.byKey(const ValueKey('open-action')));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('online-plans-dialog')), findsOneWidget);
      expect(find.text('Existing Home'), findsOneWidget);
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();

      // Secondary Open menu opens a local file.
      await tester.tap(find.byKey(const ValueKey('open-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Open local file…'));
      await tester.pumpAndSettle();
      expect(dialogs.openCalls, 1);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('disk.domo'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets(
    'clicking the plan title inline renames offline plans and online plans',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session);
      final dialogs = _FakeFileDialogs();
      await pumpEditor(tester, core: core, client: client, dialogs: dialogs);

      // Inline rename of an untitled offline plan.
      await tester.tap(find.byKey(const ValueKey('plan-title')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('plan-title-input')),
        'Kitchen Remodel',
      );
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('Kitchen Remodel'),
          matchRoot: true,
        ),
        findsOneWidget,
      );

      // Local Save as suggests the renamed title.
      await tester.tap(find.byKey(const ValueKey('save-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('save-as')));
      await tester.pumpAndSettle();
      expect(dialogs.lastSuggested, 'Kitchen Remodel.domo');

      // Reset to fresh offline plan, rename inline, then click Save -> saves
      // directly online under that name without prompting a second time.
      await tester.tap(find.byKey(const ValueKey('new-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('New local plan'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('plan-title')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('plan-title-input')),
        'Ground Floor',
      );
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('save-action')));
      await tester.pumpAndSettle();
      expect(client.calls, ['import:Ground Floor', 'acquire:plan-1']);
      expect(find.text('Online'), findsOneWidget);

      // Inline rename of the active online plan calls PATCH /plans/{id}.
      await tester.tap(find.byKey(const ValueKey('plan-title')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('plan-title-input')),
        'First Floor',
      );
      await tester.testTextInput.receiveAction(TextInputAction.done);
      await tester.pumpAndSettle();
      expect(client.calls.last, 'rename:plan-1:First Floor');
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('First Floor'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
    },
  );

  testWidgets('Safe 2D info banner can be dismissed while keeping 2D active', (
    tester,
  ) async {
    final core = _FakeCore();
    final client = _WebPropertyAccountClient(restoredSession: null);
    final dialogs = _FakeFileDialogs();
    await tester.binding.setSurfaceSize(const Size(1280, 820));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: PlanPage(
          bridgeLoader: () => core,
          accountClient: client,
          fileDialogs: dialogs,
          graphicsPolicy: const WebGraphicsPolicy.safe2d(
            'Safe 2D: 3D could not start.',
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('safe-2d-banner')), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('dismiss-safe-2d-banner')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('safe-2d-banner')), findsNothing);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'Save as new online plan links pending propertyHint and keeps online save when property link fails',
    (tester) async {
      final core = _FakeCore();
      final client = _WebPropertyAccountClient(restoredSession: _session);
      final dialogs = _FakeFileDialogs();
      await pumpEditor(
        tester,
        core: core,
        client: client,
        dialogs: dialogs,
        propertyHint: 'prop-1',
      );

      // Dismiss initial empty Property plans dialog.
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsOneWidget,
      );
      await tester.tap(find.text('Close'));
      await tester.pumpAndSettle();

      await tester.tap(find.byKey(const ValueKey('save-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('save-as-online')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Linked Upload',
      );
      await tester.tap(find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      expect(client.linkedPlanIds, contains('plan-1'));
      expect(client.calls, contains('linkPlanProperty:plan-1:prop-1'));
      expect(
        find.textContaining('Saved as new property plan "Linked Upload"'),
        findsOneWidget,
      );

      // Saving an alternative copy when property linking fails still adopts the
      // uploaded online plan and surfaces the non-fatal link error.
      client.failPropertyLink = true;
      await tester.tap(find.byKey(const ValueKey('save-menu')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('save-as-online')));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('online-plan-title')),
        'Fallback Alternative',
      );
      await tester.tap(find.byKey(const ValueKey('online-title-confirm')));
      await tester.pumpAndSettle();

      expect(find.text('Online'), findsOneWidget);
      expect(
        find.descendant(
          of: find.byKey(const ValueKey('plan-title')),
          matching: find.text('Fallback Alternative'),
          matchRoot: true,
        ),
        findsOneWidget,
      );
      expect(find.textContaining('could not link to property'), findsOneWidget);
    },
  );
}
