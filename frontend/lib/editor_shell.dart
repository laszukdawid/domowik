import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

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
    required this.onSaveAs,
    this.onExportUsdz,
    this.onAssets,
    this.onPreferences,
    this.account,
    this.onViewModeChanged,
    this.viewMode = '2d',
    required this.onUndo,
    required this.onRedo,
    required this.tools,
    required this.inspector,
    required this.canvas,
    required this.status,
    required this.statusIsError,
  });

  final String fileName;

  /// The sign-in control, when a server is configured.
  final Widget? account;
  final VoidCallback onNew;
  final VoidCallback onOpen;
  final VoidCallback onSave;

  /// Writes the plan where the user chooses, leaving the current file
  /// behind. Reached from the save control's dropdown; a plan the plain
  /// [onSave] cannot update silently (never saved, or a browser download)
  /// reaches the same dialog through [onSave].
  final VoidCallback onSaveAs;

  /// Writes the whole plan as a `.usdz` 3D scene; absent where the platform
  /// cannot choose an export location.
  final VoidCallback? onExportUsdz;
  final VoidCallback? onAssets;
  final VoidCallback? onPreferences;
  final ValueChanged<String>? onViewModeChanged;
  final String viewMode;
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

  /// Status messages read as sentences: the first letter is capitalised for
  /// display only, so callers keep composing them from lower-case parts.
  static String _sentence(String status) =>
      status.isEmpty ? status : status[0].toUpperCase() + status.substring(1);

  /// A labelled header action; icon-only when the window is narrow.
  static Widget _headerButton({
    Key? key,
    required bool compact,
    required VoidCallback? onPressed,
    required String tooltip,
    required IconData icon,
    required String label,
  }) => compact
      ? IconButton(
          key: key,
          onPressed: onPressed,
          tooltip: tooltip,
          icon: Icon(icon),
        )
      : Tooltip(
          message: tooltip,
          child: TextButton.icon(
            key: key,
            onPressed: onPressed,
            icon: Icon(icon, size: 20),
            label: Text(label),
          ),
        );

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
                      Flexible(
                        flex: 3,
                        child: Wrap(
                          alignment: WrapAlignment.end,
                          crossAxisAlignment: WrapCrossAlignment.center,
                          children: [
                            _headerButton(
                              compact: compact,
                              onPressed: onNew,
                              tooltip: 'New plan',
                              icon: Icons.note_add_outlined,
                              label: 'New',
                            ),
                            _headerButton(
                              compact: compact,
                              onPressed: onOpen,
                              tooltip: 'Open plan',
                              icon: Icons.folder_open_outlined,
                              label: 'Open',
                            ),
                            const SizedBox(width: 8),
                            if (onAssets != null)
                              _headerButton(
                                key: const ValueKey('asset-library'),
                                compact: compact,
                                onPressed: onAssets,
                                tooltip: 'Furniture and model library',
                                icon: Icons.chair_outlined,
                                label: 'Furniture',
                              ),
                            if (onViewModeChanged != null)
                              PopupMenuButton<String>(
                                key: const ValueKey('view-mode-menu'),
                                tooltip: 'Choose 2D / 3D view',
                                initialValue: viewMode,
                                onSelected: onViewModeChanged,
                                icon: compact
                                    ? const Icon(Icons.view_in_ar_outlined)
                                    : null,
                                child: compact
                                    ? null
                                    : _ViewModeLabel(viewMode: viewMode),
                                itemBuilder: (_) => const [
                                  PopupMenuItem(
                                    value: '2d',
                                    child: Text('2D plan'),
                                  ),
                                  PopupMenuItem(
                                    value: '3d',
                                    child: Text('3D view'),
                                  ),
                                  PopupMenuItem(
                                    value: 'split',
                                    child: Text('2D + 3D split view'),
                                  ),
                                ],
                              ),
                            if (account != null) account!,
                            if (onPreferences != null)
                              IconButton(
                                key: const ValueKey('preferences'),
                                onPressed: onPreferences,
                                tooltip: 'Preferences',
                                icon: const Icon(Icons.settings_outlined),
                              ),
                            const SizedBox(width: 4),
                            _SaveSplitButton(
                              onSave: onSave,
                              onSaveAs: onSaveAs,
                              onExportUsdz: onExportUsdz,
                            ),
                          ],
                        ),
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
                          key: const ValueKey('drawing-toolbar-scroll'),
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
                            tooltip: 'Levels and plan details',
                            icon: const Icon(Icons.layers_outlined),
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
                    void copyStatus() {
                      Clipboard.setData(ClipboardData(text: status));
                    }

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
                                message: _sentence(status),
                                child: Text(
                                  _sentence(status),
                                  maxLines: 2,
                                  overflow: TextOverflow.ellipsis,
                                  style: statusStyle,
                                  strutStyle: statusStrut,
                                  locale: Localizations.maybeLocaleOf(context),
                                ),
                              ),
                            ),
                            IconButton(
                              tooltip: 'Copy status message',
                              constraints: const BoxConstraints.tightFor(
                                width: 32,
                                height: 32,
                              ),
                              padding: EdgeInsets.zero,
                              visualDensity: VisualDensity.compact,
                              onPressed: copyStatus,
                              icon: Icon(
                                Icons.copy_outlined,
                                size: 16,
                                color: statusColor,
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

/// The header's save control: one filled pill whose wide lead area runs
/// [onSave] and whose fixed-width chevron area opens the save menu.
///
/// A press anywhere left of the divider saves — everything but the last
/// chevron-wide strip, roughly three quarters of the pill at the default
/// text scale, since the chevron strip does not grow with the label. A
/// press on that strip opens the menu items below it: Save As ([onSaveAs])
/// and, where available, a 3D export ([onExportUsdz]). The control holds no state and is
/// always enabled: the file-action gate covers the whole header while a
/// file workflow runs, and a failed save or open menu changes nothing.
class _SaveSplitButton extends StatelessWidget {
  const _SaveSplitButton({
    required this.onSave,
    required this.onSaveAs,
    this.onExportUsdz,
  });

  final VoidCallback onSave;
  final VoidCallback onSaveAs;
  final VoidCallback? onExportUsdz;

  @override
  Widget build(BuildContext context) {
    final colors = Theme.of(context).colorScheme;
    return Material(
      // The FilledButton face the header previously showed for Save.
      color: colors.primary,
      shape: const StadiumBorder(),
      clipBehavior: Clip.antiAlias,
      // The stretch keeps both press zones the pill's full height at any
      // text scale, one intrinsic height above the label deciding the pill.
      child: IntrinsicHeight(
        child: Row(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            InkWell(
              key: const ValueKey('save-action'),
              onTap: onSave,
              child: Container(
                padding: const EdgeInsets.symmetric(
                  horizontal: 12,
                  vertical: 10,
                ),
                alignment: Alignment.center,
                child: Row(
                  mainAxisSize: MainAxisSize.min,
                  children: [
                    Icon(
                      Icons.save_outlined,
                      size: 18,
                      color: colors.onPrimary,
                    ),
                    const SizedBox(width: 6),
                    Text(
                      'Save',
                      style: Theme.of(context).textTheme.labelLarge
                          ?.copyWith(color: colors.onPrimary),
                    ),
                  ],
                ),
              ),
            ),
            VerticalDivider(
              width: 1,
              thickness: 1,
              indent: 9,
              endIndent: 9,
              color: colors.onPrimary.withValues(alpha: 0.35),
            ),
            PopupMenuButton<String>(
              key: const ValueKey('save-menu'),
              tooltip: 'More ways to save',
              position: PopupMenuPosition.under,
              itemBuilder: (_) => [
                const PopupMenuItem<String>(
                  value: 'save-as',
                  child: Row(
                    children: [
                      Icon(Icons.save_as_outlined, size: 18),
                      SizedBox(width: 8),
                      Text('Save as'),
                    ],
                  ),
                ),
                if (onExportUsdz != null)
                  const PopupMenuItem<String>(
                    key: ValueKey('export-usdz'),
                    value: 'export-usdz',
                    child: Row(
                      children: [
                        Icon(Icons.view_in_ar_outlined, size: 18),
                        SizedBox(width: 8),
                        Flexible(
                          child: Text(
                            'Export 3D (.usdz)',
                            overflow: TextOverflow.ellipsis,
                          ),
                        ),
                      ],
                    ),
                  ),
              ],
              onSelected: (action) {
                if (action == 'save-as') onSaveAs();
                if (action == 'export-usdz') onExportUsdz?.call();
              },
              child: SizedBox(
                width: 28,
                child: Center(
                  child: Icon(
                    Icons.arrow_drop_down,
                    size: 26,
                    color: colors.onPrimary,
                  ),
                ),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The view menu's face when there is room for words: the current mode.
class _ViewModeLabel extends StatelessWidget {
  const _ViewModeLabel({required this.viewMode});

  final String viewMode;

  @override
  Widget build(BuildContext context) {
    final color = Theme.of(context).colorScheme.primary;
    return Padding(
      padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.view_in_ar_outlined, size: 20, color: color),
          const SizedBox(width: 8),
          Text(switch (viewMode) {
            '3d' => '3D',
            'split' => '2D + 3D',
            _ => '2D',
          }, style: TextStyle(color: color, fontWeight: FontWeight.w500)),
          Icon(Icons.arrow_drop_down, color: color),
        ],
      ),
    );
  }
}
