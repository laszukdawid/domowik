import 'package:flutter/material.dart';

Color solidColor(String hex) =>
    Color(0xFF000000 | int.parse(hex.substring(1), radix: 16));

class SolidColorMaterial {
  const SolidColorMaterial(this.key, this.name, this.source);
  final String key, name, source;
}

/// Presentation only. Blank resets a material to its original appearance; the
/// Rust core owns validation, persistence and applying the resulting palette.
class SolidColorsDialog extends StatefulWidget {
  const SolidColorsDialog({
    super.key,
    required this.title,
    required this.materials,
    required this.colors,
  });
  final String title;
  final List<SolidColorMaterial> materials;
  final Map<String, String> colors;

  @override
  State<SolidColorsDialog> createState() => _SolidColorsDialogState();
}

class _SolidColorsDialogState extends State<SolidColorsDialog> {
  static const palette = [
    '#F2F0E8',
    '#D8CDB7',
    '#B8A78B',
    '#8B7358',
    '#6B5A48',
    '#55514B',
    '#303234',
    '#FFFFFF',
    '#90A4AE',
    '#779383',
    '#B77964',
    '#D1B66E',
  ];
  late final fields = {
    for (final m in widget.materials)
      m.key: TextEditingController(text: widget.colors[m.key] ?? ''),
  };
  bool valid(String text) =>
      text.trim().isEmpty || RegExp(r'^#[0-9a-fA-F]{6}$').hasMatch(text.trim());

  @override
  void dispose() {
    for (final c in fields.values) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: Text(widget.title),
    content: SizedBox(
      width: 440,
      height: widget.materials.length == 1 ? 190 : 420,
      child: Column(
        children: [
          const Text(
            'Solid colors · blank uses the original. Changes affect only this selection.',
          ),
          const SizedBox(height: 12),
          Expanded(
            child: ListView.builder(
              itemCount: widget.materials.length,
              itemBuilder: (context, i) {
                final m = widget.materials[i];
                final c = fields[m.key]!;
                final text = c.text.trim();
                return Padding(
                  padding: const EdgeInsets.only(bottom: 16),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      TextField(
                        key: ValueKey('solid-color-${m.key}'),
                        controller: c,
                        decoration: InputDecoration(
                          labelText: m.name,
                          hintText: '${m.source} (original)',
                          errorText: valid(text) ? null : 'Use #RRGGBB',
                          prefixIcon: Padding(
                            padding: const EdgeInsets.all(12),
                            child: Container(
                              width: 20,
                              height: 20,
                              decoration: BoxDecoration(
                                color: solidColor(
                                  valid(text) && text.isNotEmpty
                                      ? text
                                      : m.source,
                                ),
                                border: Border.all(
                                  color: Theme.of(context).colorScheme.outline,
                                ),
                              ),
                            ),
                          ),
                          suffixIcon: IconButton(
                            tooltip: 'Reset to original',
                            icon: const Icon(Icons.restart_alt),
                            onPressed: () => setState(c.clear),
                          ),
                        ),
                        onChanged: (_) => setState(() {}),
                      ),
                      const SizedBox(height: 6),
                      Wrap(
                        spacing: 6,
                        runSpacing: 6,
                        children: [
                          for (final hex in palette)
                            Tooltip(
                              message: hex,
                              child: InkWell(
                                onTap: () => setState(() => c.text = hex),
                                child: Container(
                                  width: 24,
                                  height: 24,
                                  decoration: BoxDecoration(
                                    color: solidColor(hex),
                                    borderRadius: BorderRadius.circular(4),
                                    border: Border.all(
                                      color: Theme.of(context)
                                          .colorScheme
                                          .outline,
                                    ),
                                  ),
                                ),
                              ),
                            ),
                        ],
                      ),
                    ],
                  ),
                );
              },
            ),
          ),
        ],
      ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('apply-solid-colors'),
        onPressed: fields.values.every((c) => valid(c.text))
            ? () => Navigator.pop(context, <String, String>{
                // Preserve overrides for unavailable materials, too.
                for (final entry in widget.colors.entries)
                  if (!fields.containsKey(entry.key)) entry.key: entry.value,
                for (final entry in fields.entries)
                  if (entry.value.text.trim().isNotEmpty)
                    entry.key: entry.value.text.trim().toUpperCase(),
              })
            : null,
        child: const Text('Apply'),
      ),
    ],
  );
}
