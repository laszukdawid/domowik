import 'package:flutter/material.dart';

/// One label/value diagnostics row (issue 0052).
///
/// A `Row` with the label and value pinned to opposite ends overflows the
/// moment either string, at the current width or text scale, is wider than
/// the space left for it — exactly the 37px overflow this replaces. `Wrap`
/// instead lets the value drop to its own line when there is no room beside
/// the label, and constrains each `Text` to the available width so a long
/// label or value soft-wraps onto further lines rather than clipping or
/// running off the edge. Nothing here truncates: there is no `maxLines` or
/// `TextOverflow.ellipsis` anywhere in this widget.
class InspectorMetric extends StatelessWidget {
  const InspectorMetric({required this.label, required this.value, super.key});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final style = DefaultTextStyle.of(context).style;
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 4),
      child: Wrap(
        alignment: WrapAlignment.spaceBetween,
        crossAxisAlignment: WrapCrossAlignment.center,
        spacing: 8,
        runSpacing: 2,
        children: [
          Text(label, style: style),
          Text(
            value,
            style: style.copyWith(
              fontFeatures: const [FontFeature.tabularFigures()],
            ),
          ),
        ],
      ),
    );
  }
}
