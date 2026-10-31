import 'package:flutter/material.dart';

import 'account.dart';

/// Lists the account's online plans and opens one. It receives the same
/// in-memory session as AccountButton; it never restores or stores tokens.
Future<void> showOnlinePlans(
  BuildContext context, {
  required HostedPlanClient client,
  required AccountSession session,
  required Future<bool> Function(HostedPlan plan) onOpen,
  required String? Function() lastActionError,
}) => showDialog<void>(
  context: context,
  builder: (context) => _OnlinePlansDialog(
    client: client,
    session: session,
    onOpen: onOpen,
    lastActionError: lastActionError,
  ),
);

/// Asks for an online plan's title; null when cancelled or left empty.
Future<String?> askOnlinePlanTitle(
  BuildContext context, {
  required String action,
  required String confirm,
  required String initialValue,
}) async {
  final title = await showDialog<String>(
    context: context,
    builder: (context) => _OnlinePlanTitleDialog(
      title: action,
      confirm: confirm,
      initialValue: initialValue,
    ),
  );
  return title == null || title.isEmpty ? null : title;
}

class _OnlinePlansDialog extends StatefulWidget {
  const _OnlinePlansDialog({
    required this.client,
    required this.session,
    required this.onOpen,
    required this.lastActionError,
  });

  final HostedPlanClient client;
  final AccountSession session;
  final Future<bool> Function(HostedPlan plan) onOpen;
  final String? Function() lastActionError;

  @override
  State<_OnlinePlansDialog> createState() => _OnlinePlansDialogState();
}

class _OnlinePlansDialogState extends State<_OnlinePlansDialog> {
  List<HostedPlan> _plans = const [];
  String? _error;
  bool _loading = true;
  bool _working = false;
  String? _actionError;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  Future<void> _refresh() async {
    setState(() {
      _loading = true;
      _error = null;
      _actionError = null;
    });
    try {
      final plans = await widget.client.listPlans(widget.session);
      if (!mounted) return;
      setState(() {
        _plans = plans;
        _loading = false;
      });
    } on Object catch (error) {
      if (!mounted) return;
      setState(() {
        _error = '$error';
        _loading = false;
      });
    }
  }

  Future<void> _open(HostedPlan plan) async {
    setState(() {
      _working = true;
      _actionError = null;
    });
    final opened = await widget.onOpen(plan);
    if (!mounted) return;
    setState(() => _working = false);
    if (opened) Navigator.pop(context);
    if (!opened && mounted) {
      setState(
        () => _actionError =
            widget.lastActionError() ??
            'Could not open "${plan.title}". Your current plan remains open.',
      );
    }
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('online-plans-dialog'),
    title: const Text('Open online plan'),
    content: SizedBox(
      width: 440,
      height: 420,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_error != null)
            Text(
              'Could not load online plans: $_error',
              key: const ValueKey('online-plans-error'),
            ),
          if (_actionError != null)
            Semantics(
              key: const ValueKey('online-plans-action-error'),
              liveRegion: true,
              container: true,
              child: Padding(
                padding: const EdgeInsets.only(bottom: 8),
                child: Text(
                  _actionError!,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            ),
          Expanded(
            child: _loading
                ? const Center(
                    child: CircularProgressIndicator(
                      key: ValueKey('online-plans-loading'),
                    ),
                  )
                : _plans.isEmpty && _error == null
                ? const Center(child: Text('No online plans yet'))
                : ListView(
                    children: [
                      for (final plan in _plans)
                        ListTile(
                          key: ValueKey('online-open-${plan.id}'),
                          title: Text(plan.title),
                          subtitle: Text('Updated ${_date(plan.updatedAt)}'),
                          trailing: const Icon(Icons.open_in_new),
                          enabled: !_working,
                          onTap: () => _open(plan),
                        ),
                    ],
                  ),
          ),
        ],
      ),
    ),
    actions: [
      TextButton.icon(
        key: const ValueKey('online-plans-refresh'),
        onPressed: _working ? null : _refresh,
        icon: const Icon(Icons.refresh),
        label: const Text('Refresh'),
      ),
      TextButton(
        onPressed: _working ? null : () => Navigator.pop(context),
        child: const Text('Close'),
      ),
    ],
  );

  static String _date(DateTime at) {
    final local = at.toLocal();
    String two(int n) => n.toString().padLeft(2, '0');
    return '${local.year}-${two(local.month)}-${two(local.day)} '
        '${two(local.hour)}:${two(local.minute)}';
  }
}

class _OnlinePlanTitleDialog extends StatefulWidget {
  const _OnlinePlanTitleDialog({
    required this.title,
    required this.confirm,
    required this.initialValue,
  });

  final String title;
  final String confirm;
  final String initialValue;

  @override
  State<_OnlinePlanTitleDialog> createState() => _OnlinePlanTitleDialogState();
}

class _OnlinePlanTitleDialogState extends State<_OnlinePlanTitleDialog> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.initialValue,
  );

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.title),
    content: TextField(
      key: const ValueKey('online-plan-title'),
      controller: _controller,
      autofocus: true,
      decoration: const InputDecoration(labelText: 'Plan title'),
      onSubmitted: (value) => Navigator.pop(context, value.trim()),
    ),
    actions: [
      TextButton(
        key: const ValueKey('online-title-cancel'),
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('online-title-confirm'),
        onPressed: () => Navigator.pop(context, _controller.text.trim()),
        child: Text(widget.confirm),
      ),
    ],
  );
}
