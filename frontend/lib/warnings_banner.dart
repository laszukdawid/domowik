import 'package:flutter/material.dart';

/// A bounded, tappable summary of non-blocking validation warnings (issues
/// 0054/0055: level advisories and wall-area overlap).
///
/// Renders NOTHING when `warnings` is empty. Otherwise a single fixed-height
/// line — a count and short summary, ellipsized, never one row per
/// warning — so an unbounded number of warnings can never grow this past a
/// compact summary or push into the canvas. It is deliberately NOT wrapped
/// in `IgnorePointer`: tapping it is how the rest are read, via a scrollable
/// dialog that bounds itself instead of the canvas.
class WarningsBanner extends StatelessWidget {
  const WarningsBanner({
    required this.warnings,
    this.wallOverlapCount = 0,
    this.warningTargets = const [],
    this.onHighlightChanged,
    super.key,
  });

  final List<String> warnings;

  /// Number of warnings about overlapping *pairs of wall segments*, not the
  /// number of drawn walls or crossings. Derived from the native warning codes.
  final int wallOverlapCount;
  final List<Set<String>> warningTargets;
  final ValueChanged<Set<String>>? onHighlightChanged;

  @override
  Widget build(BuildContext context) {
    if (warnings.isEmpty) return const SizedBox.shrink();
    final theme = Theme.of(context);
    final otherCount = warnings.length - wallOverlapCount;
    final summary = wallOverlapCount > 0
        ? '$wallOverlapCount overlapping wall-segment pair${wallOverlapCount == 1 ? '' : 's'}'
              '${otherCount > 0 ? ' · $otherCount other ${otherCount == 1 ? 'warning' : 'warnings'}' : ''} — tap for details'
        : warnings.length == 1
        ? warnings.first
        : '${warnings.length} warnings — tap for details';
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
    int? pinned;
    void highlight(int? index) => onHighlightChanged?.call(
      index == null || index >= warningTargets.length
          ? const {}
          : warningTargets[index],
    );
    showDialog<void>(
      context: context,
      builder: (context) => StatefulBuilder(
        builder: (context, update) => AlertDialog(
          title: Text('${warnings.length} warning(s)'),
          content: SizedBox(
            width: 360,
            // Fixed, not `warnings.length`-dependent: the LIST scrolls inside
            // this bound, so a document with many warnings never grows the
            // dialog (and never the canvas behind it) without limit.
            height: 320,
            child: Column(
              children: [
                if (wallOverlapCount > 0) ...[
                  const SelectableText(
                    'Crossing wall axes are split and joined automatically. '
                    'These warnings are pairs of segments whose thickness overlaps '
                    'without their axes meeting.',
                  ),
                  const SizedBox(height: 12),
                ],
                Expanded(
                  child: ListView.separated(
                    key: const ValueKey('level-warnings-list'),
                    itemCount: warnings.length,
                    separatorBuilder: (context, index) =>
                        const Divider(height: 1),
                    itemBuilder: (context, index) => MouseRegion(
                      onEnter: (_) => highlight(index),
                      onExit: (_) => highlight(pinned),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(vertical: 4),
                        child: Row(
                          children: [
                            Expanded(child: SelectableText(warnings[index])),
                            IconButton(
                              key: ValueKey('warning-highlight-$index'),
                              tooltip: 'Highlight affected items',
                              icon: Icon(
                                pinned == index
                                    ? Icons.visibility
                                    : Icons.visibility_outlined,
                              ),
                              onPressed: () {
                                update(
                                  () => pinned = pinned == index ? null : index,
                                );
                                highlight(pinned);
                              },
                            ),
                          ],
                        ),
                      ),
                    ),
                  ),
                ),
              ],
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
      ),
    ).whenComplete(() => highlight(null));
  }
}
