import 'package:flutter/material.dart';

import 'account.dart';

/// The header's sign-in control, shown only when a server is configured.
/// Signed out it offers "Sign in"; signed in it names the account and offers
/// "Sign out". The session lasts until sign-out or the app closes.
class AccountButton extends StatefulWidget {
  const AccountButton({super.key, required this.client, this.compact = false});

  final AccountClient client;
  final bool compact;

  @override
  State<AccountButton> createState() => _AccountButtonState();
}

class _AccountButtonState extends State<AccountButton> {
  AccountSession? _session;
  bool _busy = false;

  Future<void> _run(Future<void> Function() action) async {
    setState(() => _busy = true);
    try {
      await action();
    } on AccountException catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(error.message)));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  Future<void> _signIn() => _run(() async {
    final session = await widget.client.signIn();
    if (mounted) setState(() => _session = session);
  });

  Future<void> _signOut(AccountSession session) => _run(() async {
    // Forget the token first: a failed revocation must not keep it in use.
    setState(() => _session = null);
    await widget.client.signOut(session);
  });

  @override
  Widget build(BuildContext context) {
    final host = widget.client.server.origin.host;
    if (_busy) {
      return const Padding(
        padding: EdgeInsets.symmetric(horizontal: 12),
        child: SizedBox.square(
          dimension: 18,
          child: CircularProgressIndicator(strokeWidth: 2),
        ),
      );
    }
    final session = _session;
    if (session == null) {
      return widget.compact
          ? IconButton(
              key: const ValueKey('account-sign-in'),
              onPressed: _signIn,
              tooltip: 'Sign in to $host',
              icon: const Icon(Icons.account_circle_outlined),
            )
          : TextButton.icon(
              key: const ValueKey('account-sign-in'),
              onPressed: _signIn,
              icon: const Icon(Icons.account_circle_outlined),
              label: const Text('Sign in'),
            );
    }
    return PopupMenuButton<String>(
      key: const ValueKey('account-menu'),
      tooltip: 'Signed in to $host as ${session.username}',
      onSelected: (_) => _signOut(session),
      icon: widget.compact ? const Icon(Icons.account_circle) : null,
      itemBuilder: (_) => const [
        PopupMenuItem(value: 'sign-out', child: Text('Sign out')),
      ],
      child: widget.compact
          ? null
          : Padding(
              padding: const EdgeInsets.symmetric(horizontal: 8),
              child: Row(
                mainAxisSize: MainAxisSize.min,
                children: [
                  const Icon(Icons.account_circle),
                  const SizedBox(width: 6),
                  Text(session.username),
                ],
              ),
            ),
    );
  }
}
