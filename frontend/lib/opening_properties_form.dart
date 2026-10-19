import 'package:flutter/material.dart';

import 'plan_painter.dart';
import 'measurement_units.dart';

/// Edits opening dimensions as one command. The native document validator
/// checks fit, overlap and host height together before admitting the batch.
class OpeningPropertiesDialog extends StatefulWidget {
  const OpeningPropertiesDialog({
    required this.opening,
    this.units = UnitSystem.metric,
    super.key,
  });

  final Aperture opening;
  final UnitSystem units;

  static Future<Map<String, double>?> show(
    BuildContext context,
    Aperture opening, {
    UnitSystem units = UnitSystem.metric,
  }) => showDialog<Map<String, double>>(
    context: context,
    builder: (_) => OpeningPropertiesDialog(opening: opening, units: units),
  );

  @override
  State<OpeningPropertiesDialog> createState() =>
      _OpeningPropertiesDialogState();
}

class _OpeningPropertiesDialogState extends State<OpeningPropertiesDialog> {
  late final fields = <String, TextEditingController>{
    'widthMm': TextEditingController(text: _format(widget.opening.widthMm)),
    'heightMm': TextEditingController(text: _format(widget.opening.heightMm)),
    'sillMm': TextEditingController(text: _format(widget.opening.sillMm)),
    'depthMm': TextEditingController(text: _format(widget.opening.depthMm)),
  };

  String _format(double? value) =>
      value == null ? '' : widget.units.editLength(value);

  double? _value(String key) => widget.units.parseLength(
    fields[key]!.text,
    originalMm: switch (key) {
      'widthMm' => widget.opening.widthMm,
      'heightMm' => widget.opening.heightMm,
      'sillMm' => widget.opening.sillMm,
      _ => widget.opening.depthMm,
    },
  );

  bool _valid(String key) {
    final value = _value(key);
    return value != null &&
        value.isFinite &&
        (key == 'sillMm' ? value >= 0 : value > 0) &&
        value <= 1000000;
  }

  void _apply() {
    if (!fields.keys.every(_valid)) {
      setState(() {});
      return;
    }
    Navigator.pop(context, {
      for (final entry in fields.entries) entry.key: _value(entry.key)!,
    });
  }

  @override
  void dispose() {
    for (final controller in fields.values) {
      controller.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.opening.isDoor ? 'Door size' : 'Window size'),
    content: SizedBox(
      width: 320,
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          for (final entry in fields.entries)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: TextField(
                key: ValueKey('opening-${entry.key}-field'),
                controller: entry.value,
                keyboardType: const TextInputType.numberWithOptions(
                  decimal: true,
                ),
                onChanged: (_) => setState(() {}),
                decoration: InputDecoration(
                  labelText: switch (entry.key) {
                    'widthMm' => 'Width (${widget.units.lengthSymbol})',
                    'heightMm' => 'Height (${widget.units.lengthSymbol})',
                    'sillMm' => 'Sill height (${widget.units.lengthSymbol})',
                    _ => 'Depth (${widget.units.lengthSymbol})',
                  },
                  errorText: _valid(entry.key)
                      ? null
                      : 'Enter a valid dimension',
                ),
              ),
            ),
        ],
      ),
    ),
    actions: [
      TextButton(
        key: const ValueKey('opening-properties-cancel'),
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('opening-properties-apply'),
        onPressed: fields.keys.every(_valid) ? _apply : null,
        child: const Text('Apply'),
      ),
    ],
  );
}
