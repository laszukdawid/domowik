import 'package:flutter/material.dart';

import 'account.dart';

/// The header's account control, shown only when a server is configured.
/// Signed out it signs in; signed in it names the account and opens a menu
/// with its plans on the server and sign-out. The session lasts until
/// sign-out or the app closes.
class AccountButton extends StatefulWidget {
  const AccountButton({super.key, required this.client});

  final AccountClient client;

  @override
  State<AccountButton> createState() => _AccountButtonState();
}

class _AccountButtonState extends State<AccountButton> {
  AccountSession? _session;
  AccountProfile? _profile;
  String? _profileError;
  bool _busy = true;

  @override
  void initState() {
    super.initState();
    widget.client
        .restore()
        .then<AccountSession?>((session) => session, onError: (_) => null)
        .then((session) {
          if (mounted) {
            setState(() {
              _session = session;
              _busy = false;
            });
            _loadProfile();
          }
        });
  }

  /// Reads the account and its plans; a failure leaves sign-out available.
  Future<void> _loadProfile() async {
    final session = _session;
    if (session == null) return;
    try {
      final profile = await widget.client.profile(session);
      if (mounted && identical(_session, session)) {
        setState(() {
          _profile = profile;
          _profileError = null;
        });
      }
    } on AccountException catch (error) {
      if (mounted && identical(_session, session)) {
        setState(() => _profileError = error.message);
      }
    }
  }

  Future<void> _signIn() async {
    setState(() => _busy = true);
    try {
      final signedIn = await widget.client.signIn();
      if (mounted) setState(() => _session = signedIn);
    } on AccountException catch (error) {
      _report(error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
    // In the background, so a slow server never holds up sign-out.
    _loadProfile();
  }

  Future<void> _signOut() async {
    final session = _session;
    if (session == null) return;
    setState(() => _busy = true);
    try {
      // Forgotten only once the server has ended it, so a failed sign-out
      // can be retried.
      await widget.client.signOut(session);
      if (mounted) {
        setState(() {
          _session = null;
          _profile = null;
          _profileError = null;
        });
      }
    } on AccountException catch (error) {
      _report(error);
    } finally {
      if (mounted) setState(() => _busy = false);
    }
  }

  void _report(AccountException error) {
    if (mounted) {
      ScaffoldMessenger.of(context)
          .showSnackBar(SnackBar(content: Text(error.message)));
    }
  }

  @override
  Widget build(BuildContext context) {
    if (_busy || _session == null) {
      return TextButton.icon(
        key: const ValueKey('account'),
        onPressed: _busy ? null : _signIn,
        icon: const Icon(Icons.account_circle_outlined),
        label: Text(_busy ? 'Working…' : 'Sign in'),
      );
    }
    return MenuAnchor(
      menuChildren: [
        _AccountDetails(
          server: widget.client.server,
          profile: _profile,
          error: _profileError,
        ),
        const Divider(height: 1),
        MenuItemButton(
          key: const ValueKey('sign-out'),
          leadingIcon: const Icon(Icons.logout),
          onPressed: _signOut,
          child: const Text('Sign out'),
        ),
      ],
      builder: (context, controller, _) => TextButton.icon(
        key: const ValueKey('account'),
        onPressed: () {
          if (controller.isOpen) {
            controller.close();
          } else {
            controller.open();
            _loadProfile();
          }
        },
        icon: const Icon(Icons.account_circle),
        label: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 180),
          child: Text(
            _profile?.email ?? 'Signed in',
            overflow: TextOverflow.ellipsis,
          ),
        ),
      ),
    );
  }
}

/// The menu's account summary: who is signed in, where, and their plans.
class _AccountDetails extends StatelessWidget {
  const _AccountDetails({
    required this.server,
    required this.profile,
    required this.error,
  });

  final ServerEndpoint server;
  final AccountProfile? profile;
  final String? error;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final muted = theme.textTheme.bodySmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );
    final profile = this.profile;
    final plans = profile?.plans ?? const <HostedPlan>[];
    final limit = profile?.planLimit;
    return ConstrainedBox(
      constraints: const BoxConstraints(minWidth: 240, maxWidth: 320),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(16, 12, 16, 12),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text(
              profile?.email ?? 'Signed in',
              key: const ValueKey('account-email'),
              style: theme.textTheme.titleSmall,
              overflow: TextOverflow.ellipsis,
            ),
            Text(
              'on ${server.origin.host}',
              style: muted,
              overflow: TextOverflow.ellipsis,
            ),
            if (error != null) ...[
              const SizedBox(height: 12),
              Text(
                'Could not load account details: $error',
                style: muted?.copyWith(color: theme.colorScheme.error),
              ),
            ] else if (profile != null) ...[
              const SizedBox(height: 12),
              Text(
                limit == null
                    ? _count(plans.length, 'plan')
                    : '${plans.length} of ${_count(limit, 'plan')}',
                key: const ValueKey('account-plan-usage'),
                style: theme.textTheme.labelMedium,
              ),
              if (plans.isEmpty)
                Text('No plans on the server yet', style: muted)
              else
                for (final plan in plans)
                  Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Row(
                      children: [
                        const Icon(Icons.description_outlined, size: 18),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Column(
                            crossAxisAlignment: CrossAxisAlignment.start,
                            children: [
                              Text(plan.title, overflow: TextOverflow.ellipsis),
                              Text(
                                'Updated ${_date(plan.updatedAt)}',
                                style: muted,
                              ),
                            ],
                          ),
                        ),
                      ],
                    ),
                  ),
            ],
          ],
        ),
      ),
    );
  }

  static String _count(int n, String noun) => '$n $noun${n == 1 ? '' : 's'}';

  static String _date(DateTime at) {
    final local = at.toLocal();
    String two(int n) => n.toString().padLeft(2, '0');
    return '${local.year}-${two(local.month)}-${two(local.day)}';
  }
}
