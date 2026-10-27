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
    expect(find.text('Sign out'), findsOneWidget);

    await tester.tap(button);
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
    expect(find.text('Signing out failed'), findsOneWidget);
    expect(find.text('Sign out'), findsOneWidget);
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
    expect(find.text('Sign out'), findsOneWidget);
  });
}

class _FakeClient implements AccountClient {
  _FakeClient({this.existing});

  final AccountSession? existing;
  bool fail = false;
  bool failSignOut = false;
  bool navigates = false;
  bool signedOut = false;

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
}
