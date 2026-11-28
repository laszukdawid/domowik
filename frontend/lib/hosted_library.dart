import 'package:flutter/material.dart';

import 'account.dart';
import 'property_planner.dart';

/// Lists the account's online plans, opens one, or deletes an unwanted plan.
/// It receives the same session as AccountButton; it never restores or stores
/// tokens.
Future<void> showOnlinePlans(
  BuildContext context, {
  required HostedPlanClient client,
  required AccountSession session,
  required Future<bool> Function(HostedPlan plan) onOpen,
  required String? Function() lastActionError,
  String? activePlanId,
  PropertyPlannerClient? propertyClient,
  String? propertyHint,
  Future<void> Function(HostedPlan plan)? onDeleted,
}) => showDialog<void>(
  context: context,
  builder: (context) => _OnlinePlansDialog(
    client: client,
    session: session,
    onOpen: onOpen,
    lastActionError: lastActionError,
    activePlanId: activePlanId,
    propertyClient: propertyClient,
    propertyHint: propertyHint,
    onDeleted: onDeleted,
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
    this.activePlanId,
    this.propertyClient,
    this.propertyHint,
    this.onDeleted,
  });

  final HostedPlanClient client;
  final AccountSession session;
  final Future<bool> Function(HostedPlan plan) onOpen;
  final String? Function() lastActionError;
  final String? activePlanId;
  final PropertyPlannerClient? propertyClient;
  final String? propertyHint;
  final Future<void> Function(HostedPlan plan)? onDeleted;

  @override
  State<_OnlinePlansDialog> createState() => _OnlinePlansDialogState();
}

class _OnlinePlansDialogState extends State<_OnlinePlansDialog> {
  List<HostedPlan> _plans = const [];
  String? _error;
  bool _loading = true;
  bool _working = false;
  String? _actionError;
  late String? _activePlanId = widget.activePlanId;

  @override
  void initState() {
    super.initState();
    _refresh();
  }

  @override
  void didUpdateWidget(covariant _OnlinePlansDialog oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.activePlanId != widget.activePlanId) {
      _activePlanId = widget.activePlanId;
    }
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

  Future<String?> _linkedPropertyName(HostedPlan plan) async {
    final propertyClient = widget.propertyClient;
    if (propertyClient == null) return null;
    try {
      final context = await propertyClient.planPropertyContext(
        widget.session,
        plan.id,
      );
      return context.property.name;
    } on UnimplementedError {
      try {
        final overview = await propertyClient.propertyOverview(
          widget.session,
          propertyHint: widget.propertyHint,
        );
        if (overview.plans.any((linked) => linked.id == plan.id)) {
          return overview.property.name;
        }
      } on Object {
        return null;
      }
      return null;
    } on Object {
      return null;
    }
  }

  Future<bool?> _confirmDelete(
    HostedPlan plan, {
    required bool isCurrentlyOpen,
    required String? linkedPropertyName,
  }) {
    final notes = <String>[
      'Delete "${plan.title}" permanently? This cannot be undone.',
      if (isCurrentlyOpen)
        'This plan is currently open in the editor. Your canvas will remain '
            'open locally, and its online copy will be removed.',
      if (linkedPropertyName != null)
        'It is linked to property "$linkedPropertyName" and will be unlinked.',
    ];
    return showDialog<bool>(
      context: context,
      builder: (context) {
        final colors = Theme.of(context).colorScheme;
        return AlertDialog(
          key: const ValueKey('online-delete-confirm-dialog'),
          title: const Text('Delete online plan?'),
          content: Text(notes.join('\n\n')),
          actions: [
            TextButton(
              key: const ValueKey('online-delete-cancel'),
              onPressed: () => Navigator.pop(context, false),
              child: const Text('Cancel'),
            ),
            FilledButton(
              key: const ValueKey('online-delete-confirm'),
              style: FilledButton.styleFrom(
                backgroundColor: colors.error,
                foregroundColor: colors.onError,
              ),
              onPressed: () => Navigator.pop(context, true),
              child: const Text('Delete'),
            ),
          ],
        );
      },
    );
  }

  Future<void> _delete(HostedPlan plan) async {
    setState(() {
      _working = true;
      _actionError = null;
    });
    final isCurrentlyOpen = plan.id == _activePlanId;
    final linkedPropertyName = await _linkedPropertyName(plan);
    if (!mounted) return;
    setState(() => _working = false);

    final confirmed = await _confirmDelete(
      plan,
      isCurrentlyOpen: isCurrentlyOpen,
      linkedPropertyName: linkedPropertyName,
    );
    if (confirmed != true || !mounted) return;

    setState(() {
      _working = true;
      _actionError = null;
    });
    try {
      await widget.client.deletePlan(widget.session, plan.id);
      if (isCurrentlyOpen) _activePlanId = null;
      await widget.onDeleted?.call(plan);
      if (!mounted) return;
      setState(() {
        _plans = _plans.where((candidate) => candidate.id != plan.id).toList();
        _working = false;
      });
      await _refresh();
    } on Object catch (error) {
      if (!mounted) return;
      final detail = error is AccountException ? error.message : '$error';
      setState(() {
        _working = false;
        _actionError = 'Could not delete "${plan.title}": $detail';
      });
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
                          subtitle: Text(
                            plan.id == _activePlanId
                                ? 'Updated ${_date(plan.updatedAt)} · Currently open'
                                : 'Updated ${_date(plan.updatedAt)}',
                          ),
                          trailing: IconButton(
                            key: ValueKey('online-delete-${plan.id}'),
                            tooltip: 'Delete online plan',
                            onPressed: _working ? null : () => _delete(plan),
                            icon: const Icon(Icons.delete_outline),
                          ),
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
