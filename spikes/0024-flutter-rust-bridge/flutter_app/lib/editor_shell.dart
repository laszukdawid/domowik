import 'dart:math' as math;

import 'package:flutter/material.dart';

/// Presentation only: callers own document state, editing and native handles.
///
/// The inspector becomes a drawer below 900 logical pixels. The canvas stays
/// mounted when the window crosses that breakpoint, preserving active input.
class EditorShell extends StatelessWidget {
  const EditorShell({
    super.key,
    required this.fileName,
    required this.onNew,
    required this.onOpen,
    required this.onSave,
    required this.onUndo,
    required this.onRedo,
    required this.tools,
    required this.inspector,
    required this.canvas,
    required this.status,
    required this.statusIsError,
  });

  final String fileName;
  final VoidCallback onNew;
  final VoidCallback onOpen;
  final VoidCallback onSave;
  final VoidCallback? onUndo;
  final VoidCallback? onRedo;
  final Widget tools;
  final Widget inspector;
  final Widget canvas;
  final String status;
  final bool statusIsError;

  /// Vertical padding on each side of the status bar's content. Read by both
  /// [_statusBarHeight] (sizing) and the bar's own `Container` (layout) —
  /// one constant, so the two can never drift apart.
  static const _statusVerticalPadding = 8.0;

  /// Matches the `Icon(size: ...)` used in the status bar below, so the
  /// reserved height never comes out shorter than the icon it must contain.
  static const _statusIconSize = 16.0;

  /// Resolves the status text's FULLY EXPLICIT style once per build: the
  /// same object is measured by [_statusBarHeight] and handed to the
  /// rendered `Text`, so the two can never disagree about line metrics.
  ///
  /// Non-inheriting: [DefaultTextStyle.of] is read once, merged with our
  /// size override, and frozen (`inherit: false`). A bare `TextStyle(fontSize:
  /// 12)` measured on its own does NOT see the font family, weight or line
  /// height the ambient theme merges into the actually-rendered `Text` — so
  /// measuring that instead of the resolved style could reserve a height for
  /// different metrics than the ones on screen.
  static TextStyle _resolvedStatusStyle(BuildContext context, Color color) {
    return DefaultTextStyle.of(context).style
        .merge(const TextStyle(fontSize: 12))
        .copyWith(color: color, inherit: false);
  }

  /// A strut matching `style`, forced onto every line regardless of which
  /// glyphs actually appear on it.
  ///
  /// Without a forced strut, dart:ui derives each line's height from the
  /// ACTUAL glyphs on it, so a status message containing a character the
  /// primary font lacks (an emoji, a symbol, non-Latin script) can fall back
  /// to a font with different metrics and render a taller or shorter line
  /// than the plain-ASCII line this class would otherwise measure —
  /// reintroducing exactly the per-message height drift issue 0048 is about,
  /// just triggered by content instead of length. `forceStrutHeight: true`
  /// makes every line exactly this height no matter what it contains, and
  /// the SAME [StrutStyle] object is used for both [_statusBarHeight]'s
  /// measurement and the rendered `Text` below.
  static StrutStyle _statusStrutStyle(TextStyle style) => StrutStyle(
    fontFamily: style.fontFamily,
    fontFamilyFallback: style.fontFamilyFallback,
    fontSize: style.fontSize,
    height: style.height,
    forceStrutHeight: true,
  );

  /// Height that reserves exactly two lines of `style`/`strut` at the
  /// current accessibility text scale, text direction and locale, plus the
  /// bar's own vertical padding — floored so the status icon itself is
  /// never squeezed shorter than its own size at a very small text scale.
  ///
  /// `style` and `strut` MUST be the same resolved objects passed to the
  /// rendered `Text` (see [_resolvedStatusStyle]/[_statusStrutStyle]) —
  /// measuring differently-resolved ones is exactly the mismatch this exists
  /// to avoid. Measured as a genuine two-line paragraph (`maxLines: 2`, an
  /// explicit line break), not one line's height doubled: multi-line layout
  /// (leading distribution, line-spacing rounding) is not required to be
  /// exactly linear in the line count, so doubling a single measured line
  /// can undershoot a real two-line paragraph by a sub-pixel amount — and
  /// `Text(maxLines: 2)` truncates on ANY shortfall, however small — which is
  /// also why the strut is forced rather than left to vary by content.
  static double _statusBarHeight(
    BuildContext context,
    TextStyle style,
    StrutStyle strut,
  ) {
    final painter = TextPainter(
      text: TextSpan(text: 'Ay\nAy', style: style),
      textDirection: Directionality.of(context),
      textScaler: MediaQuery.textScalerOf(context),
      locale: Localizations.maybeLocaleOf(context),
      strutStyle: strut,
      maxLines: 2,
    )..layout();
    final textHeight = painter.height;
    painter.dispose();
    return math.max(textHeight, _statusIconSize) + _statusVerticalPadding * 2;
  }

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return LayoutBuilder(
      builder: (context, constraints) {
        final compact = constraints.maxWidth < 900;
        return Scaffold(
          endDrawer: compact ? Drawer(child: SafeArea(child: inspector)) : null,
          body: SafeArea(
            child: Column(
              children: [
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 16,
                    vertical: 10,
                  ),
                  child: Row(
                    children: [
                      Icon(
                        Icons.cottage_outlined,
                        color: colors.primary,
                        size: 28,
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: [
                            const Text(
                              'Domowik',
                              style: TextStyle(
                                fontSize: 18,
                                fontWeight: FontWeight.w700,
                              ),
                            ),
                            Text(
                              fileName,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                              style: TextStyle(
                                fontSize: 12,
                                color: colors.onSurfaceVariant,
                              ),
                            ),
                          ],
                        ),
                      ),
                      IconButton(
                        onPressed: onNew,
                        tooltip: 'New plan',
                        icon: const Icon(Icons.note_add_outlined),
                      ),
                      IconButton(
                        onPressed: onOpen,
                        tooltip: 'Open plan',
                        icon: const Icon(Icons.folder_open_outlined),
                      ),
                      const SizedBox(width: 8),
                      FilledButton.icon(
                        onPressed: onSave,
                        icon: const Icon(Icons.save_outlined, size: 18),
                        label: const Text('Save'),
                      ),
                    ],
                  ),
                ),
                const Divider(height: 1),
                Padding(
                  padding: const EdgeInsets.symmetric(
                    horizontal: 12,
                    vertical: 8,
                  ),
                  child: Row(
                    children: [
                      Expanded(
                        child: SingleChildScrollView(
                          scrollDirection: Axis.horizontal,
                          child: tools,
                        ),
                      ),
                      const SizedBox(width: 8),
                      IconButton(
                        onPressed: onUndo,
                        tooltip: 'Undo',
                        icon: const Icon(Icons.undo),
                      ),
                      IconButton(
                        onPressed: onRedo,
                        tooltip: 'Redo',
                        icon: const Icon(Icons.redo),
                      ),
                      if (compact)
                        Builder(
                          builder: (context) => IconButton(
                            onPressed: () =>
                                Scaffold.of(context).openEndDrawer(),
                            tooltip: 'Plan details',
                            icon: const Icon(Icons.tune),
                          ),
                        ),
                    ],
                  ),
                ),
                const Divider(height: 1),
                Expanded(
                  child: Row(
                    crossAxisAlignment: CrossAxisAlignment.stretch,
                    children: [
                      Expanded(child: ClipRect(child: canvas)),
                      if (!compact) ...[
                        const VerticalDivider(width: 1),
                        SizedBox(width: 280, child: inspector),
                      ],
                    ],
                  ),
                ),
                const Divider(height: 1),
                // Fixed height, reserved for two lines at the CURRENT text
                // scale (see `_statusBarHeight`): a message changing length
                // or error state must never resize this bar, because that
                // resizes the canvas above it and moves geometry beneath an
                // unchanged pointer (issue 0048). `statusStyle` is resolved
                // ONCE and used for both the measurement and the rendered
                // `Text` below, so they cannot disagree about line metrics.
                Builder(
                  builder: (context) {
                    final statusColor = statusIsError
                        ? colors.onErrorContainer
                        : colors.onSurfaceVariant;
                    final statusStyle = _resolvedStatusStyle(
                      context,
                      statusColor,
                    );
                    final statusStrut = _statusStrutStyle(statusStyle);
                    return SizedBox(
                      height: _statusBarHeight(
                        context,
                        statusStyle,
                        statusStrut,
                      ),
                      child: Container(
                        width: double.infinity,
                        color: statusIsError
                            ? colors.errorContainer
                            : colors.surface,
                        padding: const EdgeInsets.symmetric(
                          horizontal: 16,
                          vertical: _statusVerticalPadding,
                        ),
                        child: Row(
                          children: [
                            Icon(
                              statusIsError
                                  ? Icons.error_outline
                                  : Icons.info_outline,
                              size: _statusIconSize,
                              color: statusColor,
                            ),
                            const SizedBox(width: 8),
                            Expanded(
                              child: Tooltip(
                                message: status,
                                child: Text(
                                  status,
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: statusStyle,
                                  strutStyle: statusStrut,
                                  locale: Localizations.maybeLocaleOf(context),
                                ),
                              ),
                            ),
                          ],
                        ),
                      ),
                    );
                  },
                ),
              ],
            ),
          ),
        );
      },
    );
  }
}
