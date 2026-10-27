import 'dart:async';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/account_button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets('the header button signs in and out, and reports failures', (
    tester,
  ) async {
    final client = _FakeClient();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    final button = find.byKey(const ValueKey('account'));
    expect(find.text('Sign in'), findsOneWidget);
    await tester.tap(button);
    await tester.pumpAndSettle();
    expect(find.text('ann@example.com'), findsOneWidget);

    await tester.tap(button);
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('sign-out')));
    await tester.pumpAndSettle();
    expect(client.signedOut, isTrue);
    expect(find.text('Sign in'), findsOneWidget);

    client.fail = true;
    await tester.tap(button);
    await tester.pumpAndSettle();
    expect(find.text('Signing in failed'), findsOneWidget);
    expect(find.text('Sign in'), findsOneWidget);
  });

  testWidgets('a failed sign-out keeps the session to retry', (tester) async {
    final client = _FakeClient(existing: const AccountSession(token: 'csrf'))
      ..failSignOut = true;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('sign-out')));
    await tester.pumpAndSettle();
    expect(find.text('Signing out failed'), findsOneWidget);
    expect(find.text('ann@example.com'), findsOneWidget);
  });

  testWidgets('the menu names the account, the server and its plans', (
    tester,
  ) async {
    final client = _FakeClient(existing: const AccountSession(token: 'csrf'));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    expect(find.text('ann@example.com'), findsNWidgets(2));
    expect(find.text('on planner.example'), findsOneWidget);
    expect(find.text('1 of 1 plan'), findsOneWidget);
    expect(find.text('Flat'), findsOneWidget);
    expect(find.textContaining('Updated '), findsOneWidget);
    expect(client.profileReads, 2, reason: 'opening the menu refreshes it');
  });

  testWidgets('sign-out does not wait for the account details', (tester) async {
    final client = _FakeClient()..pendingProfile = Completer();
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    expect(find.text('Signed in'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('sign-out')));
    await tester.pumpAndSettle();
    expect(client.signedOut, isTrue);

    // Details arriving after sign-out are dropped.
    client.pendingProfile!.complete(
      const AccountProfile(email: 'ann@example.com', plans: []),
    );
    await tester.pumpAndSettle();
    expect(find.text('Sign in'), findsOneWidget);
    expect(find.text('ann@example.com'), findsNothing);
  });

  testWidgets('an unreadable account still offers sign-out', (tester) async {
    final client = _FakeClient(existing: const AccountSession(token: 'csrf'))
      ..failProfile = true;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('Signed in'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    expect(
      find.text('Could not load account details: Server unavailable'),
      findsOneWidget,
    );
    await tester.tap(find.byKey(const ValueKey('sign-out')));
    await tester.pumpAndSettle();
    expect(client.signedOut, isTrue);
    expect(find.text('Sign in'), findsOneWidget);
  });

  testWidgets('a sign-in that leaves the page frees the button if it stays', (
    tester,
  ) async {
    final client = _FakeClient()..navigates = true;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(child: AccountButton(client: client)),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('account')));
    await tester.pumpAndSettle();
    expect(find.text('Sign in'), findsOneWidget);
    expect(
      tester
          .widget<TextButton>(find.byKey(const ValueKey('account')))
          .onPressed,
      isNotNull,
    );
  });

  testWidgets('a session already in place shows as signed in', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Center(
            child: AccountButton(
              client: _FakeClient(
                existing: const AccountSession(token: 'csrf'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    expect(find.text('ann@example.com'), findsOneWidget);
  });
}

class _FakeClient implements AccountClient {
  _FakeClient({this.existing});

  final AccountSession? existing;
  bool fail = false;
  bool failSignOut = false;
  bool navigates = false;
  bool failProfile = false;
  Completer<AccountProfile>? pendingProfile;
  bool signedOut = false;
  int profileReads = 0;

  @override
  Future<AccountSession?> restore() async => existing;

  @override
  ServerEndpoint get server => ServerEndpoint('https://planner.example');

  @override
  Future<AccountSession?> signIn() async {
    if (fail) throw const AccountException('Signing in failed');
    if (navigates) return null;
    return const AccountSession(token: 'dmk_secret', tokenId: 'token-id');
  }

  @override
  Future<void> signOut(AccountSession session) async {
    if (failSignOut) throw const AccountException('Signing out failed');
    signedOut = true;
  }

  @override
  Future<AccountProfile> profile(AccountSession session) async {
    profileReads++;
    if (failProfile) throw const AccountException('Server unavailable');
    if (pendingProfile != null) return pendingProfile!.future;
    return AccountProfile(
      email: 'ann@example.com',
      planLimit: 1,
      plans: [HostedPlan(title: 'Flat', updatedAt: DateTime(2026, 10, 27))],
    );
  }
}
