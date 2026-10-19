import 'package:flutter/material.dart';

import 'user_config.dart';
import 'measurement_units.dart';

/// User/device preferences; persistence must succeed before closing with Save.
class PreferencesDialog extends StatefulWidget {
  const PreferencesDialog({super.key, required this.store});

  final UserConfigStore store;

  @override
  State<PreferencesDialog> createState() => _PreferencesDialogState();
}

class _PreferencesDialogState extends State<PreferencesDialog> {
  late SplitOrientation _orientation = widget.store.splitOrientation;
  late UnitSystem _units = widget.store.unitSystem;
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
              'Saved for all plans on this device. Imperial fields use decimal inches; rulers use feet. Stored plan measurements are unchanged.',
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
