import 'package:flutter/material.dart';

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
  });

  final UserConfigStore store;
  final String? documentKey;
  final String? layoutName;
  final String currentViewMode;

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
  bool _saving = false;
  String? _error;

  Future<void> _save() async {
    setState(() {
      _saving = true;
      _error = null;
    });
    try {
      await widget.store.savePreferences(
        splitOrientation: _orientation,
        unitSystem: _units,
        defaultViewMode: _defaultView,
        documentKey: widget.documentKey,
        rememberedViewMode: _rememberView ? widget.currentViewMode : null,
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
