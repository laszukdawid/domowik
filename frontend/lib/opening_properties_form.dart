import 'package:flutter/material.dart';

import 'plan_painter.dart';

/// Edits opening dimensions as one command. The native document validator
/// checks fit, overlap and host height together before admitting the batch.
class OpeningPropertiesDialog extends StatefulWidget {
  const OpeningPropertiesDialog({required this.opening, super.key});

  final Aperture opening;

  static Future<Map<String, double>?> show(
    BuildContext context,
    Aperture opening,
  ) => showDialog<Map<String, double>>(
    context: context,
    builder: (_) => OpeningPropertiesDialog(opening: opening),
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

  static String _format(double? value) => value == null
      ? ''
      : value == value.roundToDouble()
      ? value.toStringAsFixed(0)
      : value.toString();

  bool _valid(String key) {
    final value = double.tryParse(fields[key]!.text.trim());
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
      for (final entry in fields.entries)
        entry.key: double.parse(entry.value.text.trim()),
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
                    'widthMm' => 'Width (mm)',
                    'heightMm' => 'Height (mm)',
                    'sillMm' => 'Sill height (mm)',
                    _ => 'Depth (mm)',
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
