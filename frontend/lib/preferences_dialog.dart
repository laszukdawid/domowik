import 'package:flutter/material.dart';

import 'account.dart';
import 'user_config.dart';
import 'measurement_units.dart';

/// User/device preferences; persistence must succeed before closing with Save.
class PreferencesDialog extends StatefulWidget {
  const PreferencesDialog({
    super.key,
    required this.store,
    this.documentKey,
    this.layoutName,
    this.currentViewMode = '2d',
    this.showServer = false,
    this.serverOverride,
    this.beforeServerChange,
  });

  final UserConfigStore store;
  final String? documentKey;
  final String? layoutName;
  final String currentViewMode;

  /// Offers the Server preference (desktop only).
  final bool showServer;

  /// A `DOMOWIK_SERVER_URL` value that overrides the preference; the field
  /// then shows it and cannot be edited.
  final String? serverOverride;

  /// Runs before a changed server is saved, while the old server is still in
  /// use; returning false keeps every preference unchanged.
  final Future<bool> Function()? beforeServerChange;

  @override
  State<PreferencesDialog> createState() => _PreferencesDialogState();
}

class _PreferencesDialogState extends State<PreferencesDialog> {
  late SplitOrientation _orientation = widget.store.splitOrientation;
  late UnitSystem _units = widget.store.unitSystem;
  late String _defaultView = widget.store.defaultViewMode;
  late bool _rememberView = widget.store.rememberedViewModes.containsKey(
    widget.documentKey,
  );
  late final TextEditingController _server = TextEditingController(
    text: widget.serverOverride ?? widget.store.serverUrl ?? defaultServerUrl,
  );
  bool _saving = false;
  String? _error;
  String? _serverError;

  @override
  void dispose() {
    _server.dispose();
    super.dispose();
  }

  /// The canonical server origin, or empty for none; throws when invalid.
  static String _canonical(String url) =>
      url.trim().isEmpty ? '' : ServerEndpoint(url).canonical;

  /// The server to save, or null when it is unchanged or overridden.
  String? _changedServer() {
    if (!widget.showServer || widget.serverOverride != null) return null;
    final next = _canonical(_server.text);
    final stored = widget.store.serverUrl ?? defaultServerUrl;
    final String previous;
    try {
      previous = _canonical(stored);
    } on FormatException {
      // A hand-edited invalid value can always be replaced or cleared.
      return next;
    }
    return next == previous ? null : next;
  }

  Future<void> _save() async {
    final String? server;
    try {
      server = _changedServer();
    } on FormatException {
      setState(
        () => _serverError =
            'Enter an https:// address with no path, or leave it empty',
      );
      return;
    }
    setState(() {
      _saving = true;
      _error = null;
      _serverError = null;
    });
    try {
      if (server != null &&
          widget.beforeServerChange != null &&
          !await widget.beforeServerChange!()) {
        if (mounted) setState(() => _saving = false);
        return;
      }
      await widget.store.savePreferences(
        splitOrientation: _orientation,
        unitSystem: _units,
        defaultViewMode: _defaultView,
        documentKey: widget.documentKey,
        rememberedViewMode: _rememberView ? widget.currentViewMode : null,
        serverUrl: server,
      );
      if (mounted) Navigator.of(context).pop(true);
    } catch (error) {
      if (mounted)
        setState(() {
          _saving = false;
          _error = '$error';
        });
    }
  }

  @override
  Widget build(BuildContext context) => PopScope(
    canPop: !_saving,
    child: AlertDialog(
      title: const Text('Preferences'),
      scrollable: true,
      content: SizedBox(
        width: 360,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            DropdownButtonFormField<String>(
              key: const ValueKey('default-view-mode'),
              initialValue: _defaultView,
              isExpanded: true,
              decoration: const InputDecoration(labelText: 'Default view'),
              items: [
                for (final entry in planViewModes.entries)
                  DropdownMenuItem(value: entry.key, child: Text(entry.value)),
              ],
              onChanged: _saving
                  ? null
                  : (value) => setState(() => _defaultView = value!),
            ),
            if (widget.documentKey != null) ...[
              CheckboxListTile(
                key: const ValueKey('remember-layout-view'),
                contentPadding: EdgeInsets.zero,
                title: const Text('Remember last used view for this layout'),
                subtitle: Text(widget.layoutName ?? 'Untitled plan'),
                value: _rememberView,
                onChanged: _saving
                    ? null
                    : (value) => setState(() => _rememberView = value!),
              ),
              const Text(
                'When enabled, this layout reopens in its last used view. '
                'Otherwise it uses the default view. Stored on this device only.',
              ),
            ],
            const SizedBox(height: 12),
            DropdownButtonFormField<UnitSystem>(
              key: const ValueKey('unit-system'),
              initialValue: _units,
              isExpanded: true,
              decoration: const InputDecoration(labelText: 'Measurement units'),
              items: const [
                DropdownMenuItem(
                  value: UnitSystem.metric,
                  child: Text('Metric (mm, m²)'),
                ),
                DropdownMenuItem(
                  value: UnitSystem.imperial,
                  child: Text('Imperial (in, ft²)'),
                ),
              ],
              onChanged: _saving
                  ? null
                  : (value) => setState(() => _units = value!),
            ),
            const SizedBox(height: 12),
            DropdownButtonFormField<SplitOrientation>(
              key: const ValueKey('split-orientation'),
              initialValue: _orientation,
              isExpanded: true,
              decoration: const InputDecoration(labelText: '2D / 3D split'),
              items: const [
                DropdownMenuItem(
                  value: SplitOrientation.vertical,
                  child: Text('Vertical (left and right)'),
                ),
                DropdownMenuItem(
                  value: SplitOrientation.horizontal,
                  child: Text('Horizontal (top and bottom)'),
                ),
              ],
              onChanged: _saving
                  ? null
                  : (value) => setState(() => _orientation = value!),
            ),
            const SizedBox(height: 12),
            const Text(
              'Units and split orientation apply to all plans on this device. Imperial fields use decimal inches; rulers use feet. Stored plan measurements are unchanged.',
            ),
            if (widget.showServer) ...[
              const SizedBox(height: 12),
              TextField(
                key: const ValueKey('server-url'),
                controller: _server,
                enabled: !_saving && widget.serverOverride == null,
                keyboardType: TextInputType.url,
                decoration: InputDecoration(
                  labelText: 'Server',
                  hintText: defaultServerUrl,
                  errorText: _serverError,
                  helperMaxLines: 3,
                  helperText: widget.serverOverride != null
                      ? 'Set by $serverUrlSetting for this run.'
                      : 'Stores online plans; sign in with your HomeHero '
                            'account. Leave empty to keep plans only on '
                            'this device.',
                ),
              ),
            ],
            if (_error != null) ...[
              const SizedBox(height: 12),
              Text(
                'Could not save preferences: $_error',
                style: TextStyle(color: Theme.of(context).colorScheme.error),
              ),
            ],
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _saving ? null : () => Navigator.of(context).pop(),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: _saving ? null : _save,
          child: Text(_saving ? 'Saving…' : 'Save'),
        ),
      ],
    ),
  );
}
