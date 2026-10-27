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
}

class _FakeClient implements AccountClient {
  bool fail = false;
  bool signedOut = false;

  @override
  ServerEndpoint get server => ServerEndpoint('https://planner.example');

  @override
  Future<AccountSession> signIn() async {
    if (fail) throw const AccountException('Signing in failed');
    return const AccountSession(token: 'dmk_secret', tokenId: 'token-id');
  }

  @override
  Future<void> signOut(AccountSession session) async => signedOut = true;
}
