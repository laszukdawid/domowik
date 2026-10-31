import 'dart:async';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/property_planner.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _session = AccountSession(token: 'csrf');

class _Plans extends Fake implements HostedPlanClient {
  final plans = <HostedPlan>[
    HostedPlan(
      id: 'plan-1',
      title: 'Current',
      revision: '1',
      updatedAt: DateTime.utc(2026),
    ),
    HostedPlan(
      id: 'plan-2',
      title: 'Cabin',
      revision: '2',
      updatedAt: DateTime.utc(2026),
    ),
  ];
  var nextId = 3;

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) async =>
      List.of(plans);

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) async {
    final plan = HostedPlan(
      id: 'plan-${nextId++}',
      title: title,
      revision: '1',
      updatedAt: DateTime.utc(2026),
    );
    plans.add(plan);
    return plan;
  }
}

class _PropertyClient extends Fake implements PropertyPlannerClient {
  final linked = <PlannerPlanSummary>[
    const PlannerPlanSummary(id: 'plan-1', title: 'Current'),
  ];
  final links = <String, String>{};
  final calls = <String>[];
  Completer<PlannerPropertyOverview>? pendingOverview;
  Completer<PlannerPlanContext>? pendingPlanContext;

  PlannerPropertyOverview overview() => PlannerPropertyOverview(
    property: const PlannerNamedId(id: 'property-1', name: 'Home'),
    locations: const [PlannerNamedId(id: 'location-1', name: 'Kitchen')],
    plans: List.of(linked),
  );

  @override
  Future<PlannerPropertyOverview> propertyOverview(
    AccountSession session, {
    String? propertyHint,
  }) {
    calls.add('overview:$propertyHint');
    return pendingOverview?.future ?? Future.value(overview());
  }

  @override
  Future<void> linkPlanProperty(
    AccountSession session,
    String planId,
    String propertyId,
  ) async {
    calls.add('link-plan:$planId:$propertyId');
    final planTitle = switch (planId) {
      'plan-2' => 'Cabin',
      _ => 'Alternative layout',
    };
    if (!linked.any((plan) => plan.id == planId)) {
      linked.add(PlannerPlanSummary(id: planId, title: planTitle));
    }
  }

  @override
  Future<void> unlinkPlanProperty(AccountSession session, String planId) async {
    calls.add('unlink-plan:$planId');
    linked.removeWhere((plan) => plan.id == planId);
  }

  @override
  Future<PlannerPlanContext> planPropertyContext(
    AccountSession session,
    String planId,
  ) {
    calls.add('context:$planId');
    return pendingPlanContext?.future ??
        Future.value(
          PlannerPlanContext(
            plan: PlannerPlanSummary(id: planId, title: 'Current'),
            property: const PlannerNamedId(id: 'property-1', name: 'Home'),
            locations: const [
              PlannerNamedId(id: 'location-1', name: 'Kitchen'),
            ],
            roomLinks: [
              if (links['room-1'] case final locationId?)
                PlannerRoomLink(roomId: 'room-1', locationId: locationId),
            ],
          ),
        );
  }

  @override
  Future<void> linkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
    String locationId,
  ) async {
    calls.add('link-room:$planId:$roomId:$locationId');
    links[roomId] = locationId;
  }

  @override
  Future<void> unlinkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
  ) async {
    calls.add('unlink-room:$planId:$roomId');
    links.remove(roomId);
  }

  @override
  Future<PlannerRoomInventory> roomInventory(
    AccountSession session,
    String planId,
    String roomId,
  ) async {
    calls.add('inventory:$planId:$roomId');
    return const PlannerRoomInventory(
      property: PlannerNamedId(id: 'property-1', name: 'Home'),
      location: PlannerNamedId(id: 'location-1', name: 'Kitchen'),
      roomId: 'room-1',
      items: [
        PlannerInventoryItem(
          id: 'item/1',
          name: 'Mixer',
          detailsUrl: 'https://console.homehero.pro/view/item%2F1',
        ),
      ],
    );
  }

  @override
  Future<bool> openItemUri(Uri uri) async {
    calls.add('open:${uri.toString()}');
    return true;
  }
}

Widget _app(Widget child) => MaterialApp(home: Scaffold(body: child));

void main() {
  test('validates a property hint and fixed HomeHero item URLs', () {
    expect(validatedPropertyHint('property-1'), 'property-1');
    expect(validatedPropertyHint('bad/id'), isNull);
    expect(validatedPropertyHint('x' * 129), isNull);
    expect(
      validatedHomeHeroItemUri('https://console.homehero.pro/view/item%2F1')
          ?.host,
      'console.homehero.pro',
    );
    expect(
      validatedHomeHeroItemUri(
        'https://console.homehero.pro.evil.test/view/item',
      ),
      isNull,
    );
    expect(
      validatedHomeHeroItemUri('http://console.homehero.pro/view/item'),
      isNull,
    );
    expect(
      validatedHomeHeroItemUri(
        'https://console.homehero.pro/view/item?next=https://evil.test',
      ),
      isNull,
    );
  });

  testWidgets(
    'lists, links, creates, opens and unlinks property plan alternatives',
    (tester) async {
      final propertyClient = _PropertyClient();
      final planClient = _Plans();
      final sessionChanges = ValueNotifier<int>(0);
      addTearDown(sessionChanges.dispose);
      final documentChanges = ValueNotifier<int>(0);
      addTearDown(documentChanges.dispose);
      final opened = <String>[];
      await tester.pumpWidget(
        _app(
          PropertyPlannerManagerButton(
            client: propertyClient,
            planClient: planClient,
            session: _session,
            propertyHint: 'property-1',
            isSessionCurrent: (_) => true,
            sessionChanges: sessionChanges,
            onOpen: (plan) async {
              opened.add(plan.id);
              documentChanges.value++;
              return true;
            },
          ),
        ),
      );

      await tester.tap(find.byKey(const ValueKey('property-plans-button')));
      await tester.pumpAndSettle();
      expect(find.text('HomeHero property: Home'), findsOneWidget);
      expect(find.text('Current'), findsOneWidget);

      await tester.tap(
        find.byKey(const ValueKey('property-planner-link-existing')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('property-link-plan-plan-2')));
      await tester.pumpAndSettle();
      expect(find.text('Cabin'), findsOneWidget);
      expect(propertyClient.calls, contains('link-plan:plan-2:property-1'));

      await tester.tap(find.byKey(const ValueKey('property-planner-create')));
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('property-planner-title-confirm')),
      );
      await tester.pumpAndSettle();
      expect(propertyClient.linked.map((plan) => plan.id), contains('plan-3'));
      expect(find.text('Alternative layout'), findsOneWidget);

      await tester.tap(find.byKey(const ValueKey('property-plan-open-plan-2')));
      await tester.pumpAndSettle();
      expect(opened, ['plan-2']);
      expect(
        find.byKey(const ValueKey('property-planner-dialog')),
        findsNothing,
      );

      await tester.tap(find.byKey(const ValueKey('property-plans-button')));
      await tester.pumpAndSettle();
      await tester.tap(
        find.byKey(const ValueKey('property-plan-unlink-plan-2')),
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Unlink').last);
      await tester.pumpAndSettle();
      expect(propertyClient.linked.any((plan) => plan.id == 'plan-2'), isFalse);
    },
  );

  testWidgets(
    'links a named room to a chosen location and reads transient inventory',
    (tester) async {
      final client = _PropertyClient();
      final sessionChanges = ValueNotifier<int>(0);
      addTearDown(sessionChanges.dispose);
      final contextChanges = ValueNotifier<int>(0);
      addTearDown(contextChanges.dispose);
      await tester.pumpWidget(
        _app(
          PropertyRoomInventoryDialog(
            client: client,
            session: _session,
            planId: 'plan-1',
            roomId: 'room-1',
            roomName: 'Kitchen room',
            isContextCurrent: () => true,
            sessionChanges: sessionChanges,
            contextChanges: contextChanges,
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(
        find.text('Choose a location to link this room and view inventory.'),
        findsOneWidget,
      );

      await tester.tap(find.byKey(const ValueKey('room-location-picker')));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Kitchen').last);
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('room-location-link')));
      await tester.pumpAndSettle();
      expect(find.text('Mixer'), findsOneWidget);
      expect(client.calls, contains('inventory:plan-1:room-1'));

      await tester.tap(find.byTooltip('Open item in HomeHero'));
      await tester.pump();
      expect(
        client.calls,
        contains('open:https://console.homehero.pro/view/item%2F1'),
      );

      await tester.tap(find.byKey(const ValueKey('room-location-unlink')));
      await tester.pumpAndSettle();
      expect(client.links, isEmpty);
      expect(
        find.text('Choose a location to link this room and view inventory.'),
        findsOneWidget,
      );
    },
  );

  testWidgets('discards a delayed property result after the account changes', (
    tester,
  ) async {
    final client = _PropertyClient()
      ..pendingOverview = Completer<PlannerPropertyOverview>();
    final planClient = _Plans();
    final sessionChanges = ValueNotifier<int>(0);
    addTearDown(sessionChanges.dispose);
    var current = true;
    await tester.pumpWidget(
      _app(
        PropertyPlannerManagerButton(
          client: client,
          planClient: planClient,
          session: _session,
          propertyHint: null,
          isSessionCurrent: (_) => current,
          sessionChanges: sessionChanges,
          onOpen: (_) async => true,
        ),
      ),
    );
    await tester.tap(find.byKey(const ValueKey('property-plans-button')));
    await tester.pump();
    current = false;
    client.pendingOverview!.complete(
      const PlannerPropertyOverview(
        property: PlannerNamedId(
          id: 'stale',
          name: 'Old account private property',
        ),
        locations: [],
        plans: [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('Old account private property'), findsNothing);
  });

  testWidgets('session notification clears an already loaded room inventory', (
    tester,
  ) async {
    final client = _PropertyClient()..links['room-1'] = 'location-1';
    final sessionChanges = ValueNotifier<int>(0);
    addTearDown(sessionChanges.dispose);
    final contextChanges = ValueNotifier<int>(0);
    addTearDown(contextChanges.dispose);
    await tester.pumpWidget(
      _app(
        PropertyRoomInventoryDialog(
          client: client,
          session: _session,
          planId: 'plan-1',
          roomId: 'room-1',
          roomName: 'Kitchen room',
          isContextCurrent: () => sessionChanges.value == 0,
          sessionChanges: sessionChanges,
          contextChanges: contextChanges,
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Mixer'), findsOneWidget);

    sessionChanges.value++;
    await tester.pump();

    expect(find.text('Mixer'), findsNothing);
    expect(
      find.textContaining('account, plan or room changed'),
      findsOneWidget,
    );
    expect(client.calls.where((call) => call.startsWith('open:')), isEmpty);
  });

  testWidgets(
    'stale item tap is refused even before the context notifier rebuilds',
    (tester) async {
      final client = _PropertyClient()..links['room-1'] = 'location-1';
      final sessionChanges = ValueNotifier<int>(0);
      addTearDown(sessionChanges.dispose);
      final contextChanges = ValueNotifier<int>(0);
      addTearDown(contextChanges.dispose);
      var current = true;
      await tester.pumpWidget(
        _app(
          PropertyRoomInventoryDialog(
            client: client,
            session: _session,
            planId: 'plan-1',
            roomId: 'room-1',
            roomName: 'Kitchen room',
            isContextCurrent: () => current,
            sessionChanges: sessionChanges,
            contextChanges: contextChanges,
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Mixer'), findsOneWidget);

      current = false;
      await tester.tap(find.byTooltip('Open item in HomeHero'));
      await tester.pump();

      expect(client.calls.where((call) => call.startsWith('open:')), isEmpty);
      expect(find.text('Mixer'), findsNothing);
    },
  );

  testWidgets('discards delayed room context after the current plan changes', (
    tester,
  ) async {
    final client = _PropertyClient()
      ..pendingPlanContext = Completer<PlannerPlanContext>();
    final sessionChanges = ValueNotifier<int>(0);
    addTearDown(sessionChanges.dispose);
    final contextChanges = ValueNotifier<int>(0);
    addTearDown(contextChanges.dispose);
    var current = true;
    await tester.pumpWidget(
      _app(
        PropertyRoomInventoryDialog(
          client: client,
          session: _session,
          planId: 'plan-1',
          roomId: 'room-1',
          roomName: 'Kitchen room',
          isContextCurrent: () => current,
          sessionChanges: sessionChanges,
          contextChanges: contextChanges,
        ),
      ),
    );
    await tester.pump();
    current = false;
    client.pendingPlanContext!.complete(
      const PlannerPlanContext(
        plan: PlannerPlanSummary(id: 'old-plan', title: 'Old plan'),
        property: PlannerNamedId(id: 'old-property', name: 'Stale property'),
        locations: [],
        roomLinks: [],
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 100));

    expect(find.text('Property: Stale property'), findsNothing);
    expect(
      find.text('Choose a location to link this room and view inventory.'),
      findsNothing,
    );
  });
}
