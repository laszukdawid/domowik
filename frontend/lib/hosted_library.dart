import 'package:flutter/material.dart';

import 'account.dart';

/// The hosted plan library entry point. It deliberately receives the same
/// in-memory session as AccountButton; it never restores or stores tokens.
class HostedPlanLibraryButton extends StatelessWidget {
  const HostedPlanLibraryButton({
    super.key,
    required this.client,
    required this.session,
    required this.currentPlanTitle,
    required this.onOpen,
    required this.onCreate,
    required this.onImportCurrent,
    required this.lastActionError,
  });

  final HostedPlanClient client;
  final AccountSession? session;
  final String currentPlanTitle;
  final Future<bool> Function(HostedPlan plan) onOpen;
  final Future<bool> Function(String title) onCreate;
  final Future<bool> Function(String title) onImportCurrent;
  final String? Function() lastActionError;

  @override
  Widget build(BuildContext context) => TextButton.icon(
    key: const ValueKey('hosted-plan-library'),
    onPressed: session == null
        ? null
        : () => showDialog<void>(
            context: context,
            builder: (context) => _HostedPlanLibraryDialog(
              client: client,
              session: session!,
              currentPlanTitle: currentPlanTitle,
              onOpen: onOpen,
              onCreate: onCreate,
              onImportCurrent: onImportCurrent,
              lastActionError: lastActionError,
            ),
          ),
    icon: const Icon(Icons.cloud_outlined),
    label: const Text('Hosted plans'),
  );
}

class _HostedPlanLibraryDialog extends StatefulWidget {
  const _HostedPlanLibraryDialog({
    required this.client,
    required this.session,
    required this.currentPlanTitle,
    required this.onOpen,
    required this.onCreate,
    required this.onImportCurrent,
    required this.lastActionError,
  });

  final HostedPlanClient client;
  final AccountSession session;
  final String currentPlanTitle;
  final Future<bool> Function(HostedPlan plan) onOpen;
  final Future<bool> Function(String title) onCreate;
  final Future<bool> Function(String title) onImportCurrent;
  final String? Function() lastActionError;

  @override
  State<_HostedPlanLibraryDialog> createState() =>
      _HostedPlanLibraryDialogState();
}

class _HostedPlanLibraryDialogState extends State<_HostedPlanLibraryDialog> {
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

  Future<void> _refresh({bool clearActionError = true}) async {
    setState(() {
      _loading = true;
      _error = null;
      if (clearActionError) _actionError = null;
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

  Future<String?> _titleDialog({required String action}) async {
    final title = await showDialog<String>(
      context: context,
      builder: (context) => _HostedPlanTitleDialog(
        title: action,
        initialValue: widget.currentPlanTitle,
      ),
    );
    if (title == null || title.isEmpty) return null;
    return title;
  }

  Future<void> _create() async {
    final title = await _titleDialog(action: 'Create hosted plan');
    if (title == null || !mounted) return;
    setState(() {
      _working = true;
      _actionError = null;
    });
    final opened = await widget.onCreate(title);
    if (!mounted) return;
    setState(() => _working = false);
    if (opened) {
      Navigator.pop(context);
    } else {
      setState(
        () => _actionError = widget.lastActionError() ?? 'Could not create and open the hosted plan. Your current plan remains open.',
      );
      await _refresh(clearActionError: false);
    }
  }

  Future<void> _import() async {
    final title = await _titleDialog(action: 'Upload current plan');
    if (title == null || !mounted) return;
    setState(() {
      _working = true;
      _actionError = null;
    });
    final imported = await widget.onImportCurrent(title);
    if (!mounted) return;
    setState(() => _working = false);
    if (!imported) {
      setState(
        () => _actionError =
            widget.lastActionError() ??
            'Could not upload the current plan as a hosted copy.',
      );
    }
    await _refresh(clearActionError: imported);
    if (imported && mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(content: Text('Uploaded "$title" as a hosted copy')),
      );
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
    key: const ValueKey('hosted-library-dialog'),
    title: const Text('Hosted plan library'),
    content: SizedBox(
      width: 440,
      height: 420,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          if (_error != null)
            Text(
              'Could not load hosted plans: $_error',
              key: const ValueKey('hosted-library-error'),
            ),
          if (_actionError != null)
            Semantics(
              key: const ValueKey('hosted-library-action-error'),
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
                      key: ValueKey('hosted-library-loading'),
                    ),
                  )
                : _plans.isEmpty && _error == null
                ? const Center(child: Text('No hosted plans yet'))
                : ListView(
                    children: [
                      for (final plan in _plans)
                        ListTile(
                          key: ValueKey('hosted-open-${plan.id}'),
                          title: Text(plan.title),
                          subtitle: Text('Revision ${plan.revision}'),
                          trailing: const Icon(Icons.open_in_new),
                          enabled: !_working,
                          onTap: () => _open(plan),
                        ),
                    ],
                  ),
          ),
          const SizedBox(height: 12),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              OutlinedButton.icon(
                key: const ValueKey('hosted-refresh'),
                onPressed: _working ? null : _refresh,
                icon: const Icon(Icons.refresh),
                label: const Text('Refresh'),
              ),
              OutlinedButton.icon(
                key: const ValueKey('hosted-create'),
                onPressed: _working ? null : _create,
                icon: const Icon(Icons.add),
                label: const Text('Create'),
              ),
              FilledButton.tonalIcon(
                key: const ValueKey('hosted-import-current'),
                onPressed: _working ? null : _import,
                icon: const Icon(Icons.upload_file),
                label: const Text('Upload current as copy'),
              ),
            ],
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: _working ? null : () => Navigator.pop(context),
        child: const Text('Close'),
      ),
    ],
  );
}

class _HostedPlanTitleDialog extends StatefulWidget {
  const _HostedPlanTitleDialog({
    required this.title,
    required this.initialValue,
  });

  final String title;
  final String initialValue;

  @override
  State<_HostedPlanTitleDialog> createState() => _HostedPlanTitleDialogState();
}

class _HostedPlanTitleDialogState extends State<_HostedPlanTitleDialog> {
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
      key: const ValueKey('hosted-plan-title'),
      controller: _controller,
      autofocus: true,
      decoration: const InputDecoration(labelText: 'Plan title'),
      onSubmitted: (value) => Navigator.pop(context, value.trim()),
    ),
    actions: [
      TextButton(
        key: const ValueKey('hosted-title-cancel'),
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('hosted-title-confirm'),
        onPressed: () => Navigator.pop(context, _controller.text.trim()),
        child: Text(widget.title),
      ),
    ],
  );
}
