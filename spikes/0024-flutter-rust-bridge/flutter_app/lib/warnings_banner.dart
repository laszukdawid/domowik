import 'package:flutter/material.dart';

/// A bounded, tappable summary of non-blocking validation warnings (issue
/// 0054: duplicate elevation, slab intrusion, ceiling below wall height).
///
/// Renders NOTHING when `warnings` is empty. Otherwise a single fixed-height
/// line — the count plus the first message, ellipsized, never one row per
/// warning — so an unbounded number of warnings can never grow this past a
/// compact summary or push into the canvas. It is deliberately NOT wrapped
/// in `IgnorePointer`: tapping it is how the rest are read, via a scrollable
/// dialog that bounds itself instead of the canvas.
class WarningsBanner extends StatelessWidget {
  const WarningsBanner({required this.warnings, super.key});

  final List<String> warnings;

  @override
  Widget build(BuildContext context) {
    if (warnings.isEmpty) return const SizedBox.shrink();
    final theme = Theme.of(context);
    final summary = warnings.length == 1
        ? warnings.first
        : '${warnings.length} warnings: ${warnings.first}';
    return Card(
      key: const ValueKey('level-warnings-banner'),
      color: Colors.amber.shade100,
      elevation: 0,
      child: InkWell(
        key: const ValueKey('level-warnings-summary'),
        onTap: () => _showDetails(context),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            children: [
              const Icon(Icons.warning_amber_outlined, size: 16),
              const SizedBox(width: 6),
              Flexible(
                child: Text(
                  summary,
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: theme.textTheme.bodySmall,
                ),
              ),
              if (warnings.length > 1) ...[
                const SizedBox(width: 4),
                const Icon(Icons.expand_more, size: 16),
              ],
            ],
          ),
        ),
      ),
    );
  }

  void _showDetails(BuildContext context) {
    showDialog<void>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text('${warnings.length} warning(s)'),
        content: SizedBox(
          width: 360,
          // Fixed, not `warnings.length`-dependent: the LIST scrolls inside
          // this bound, so a document with many warnings never grows the
          // dialog (and never the canvas behind it) without limit.
          height: 320,
          child: ListView.separated(
            key: const ValueKey('level-warnings-list'),
            itemCount: warnings.length,
            separatorBuilder: (context, index) => const Divider(height: 1),
            itemBuilder: (context, index) => Padding(
              padding: const EdgeInsets.symmetric(vertical: 4),
              child: Text(warnings[index]),
            ),
          ),
        ),
        actions: [
          TextButton(
            key: const ValueKey('level-warnings-close'),
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Close'),
          ),
        ],
      ),
    );
  }
}
