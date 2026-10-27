import 'package:flutter/material.dart';

import 'account.dart';

/// The header's sign-in control, shown only when a server is configured. The
/// session lasts until sign-out or the app closes.
class AccountButton extends StatefulWidget {
  const AccountButton({super.key, required this.client});

  final AccountClient client;

  @override
  State<AccountButton> createState() => _AccountButtonState();
}

class _AccountButtonState extends State<AccountButton> {
  AccountSession? _session;
  bool _busy = false;

  Future<void> _toggle() async {
    final session = _session;
    setState(() {
      _busy = true;
      // Forget the token first: a failed revocation must not keep it in use.
      _session = null;
    });
    try {
      if (session == null) {
        final signedIn = await widget.client.signIn();
        if (mounted) setState(() => _session = signedIn);
      } else {
        await widget.client.signOut(session);
      }
    } on AccountException catch (error) {
      if (mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(error.message)));
      }
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  @override
  Widget build(BuildContext context) {
    final signedIn = _session != null;
    return TextButton.icon(
      key: const ValueKey('account'),
      onPressed: _busy ? null : _toggle,
      icon: Icon(
        signedIn ? Icons.account_circle : Icons.account_circle_outlined,
      ),
      label: Text(
        _busy
            ? 'Working…'
            : signedIn
            ? 'Sign out'
            : 'Sign in',
      ),
    );
  }
}
