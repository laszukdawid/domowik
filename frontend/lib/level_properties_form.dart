import 'package:flutter/material.dart';

import 'level_state.dart';
import 'measurement_units.dart';

/// R2.1 field bounds this form enforces locally before ever asking the core.
/// The core is still the authority — these exist so a plainly-out-of-range
/// value is rejected at the field, not after a round trip — and duplicate
/// elevations and empty names are NOT bounds: both are explicitly legal
/// (R2.1/R2.2) and produce, at most, a core warning, never a form error.
class LevelFieldBounds {
  static const elevationAbsMaxMm = 10000000.0;
  static const floorThicknessMinMm = 0.0;
  static const floorThicknessMaxMm = 1000000.0;
  static const heightMaxMm = 1000000.0;

  static bool elevationValid(double v) =>
      v.isFinite && v.abs() <= elevationAbsMaxMm;
  static bool floorThicknessValid(double v) =>
      v.isFinite && v >= floorThicknessMinMm && v <= floorThicknessMaxMm;
  static bool heightValid(double v) => v.isFinite && v > 0 && v <= heightMaxMm;
}

/// Only the fields the user actually changed, ready for the `setLevel`
/// gesture (`{kind: 'setLevel', levelId, ...changes.toJson()}`). Never sent
/// at all when [isEmpty] — an Apply with nothing changed must not mint a
/// no-op batch.
class LevelPropertiesChanges {
  const LevelPropertiesChanges({
    this.name,
    this.elevationMm,
    this.floorThicknessMm,
    this.defaultWallHeightMm,
    this.defaultCeilingHeightMm,
  });

  final String? name;
  final double? elevationMm;
  final double? floorThicknessMm;
  final double? defaultWallHeightMm;
  final double? defaultCeilingHeightMm;

  bool get isEmpty =>
      name == null &&
      elevationMm == null &&
      floorThicknessMm == null &&
      defaultWallHeightMm == null &&
      defaultCeilingHeightMm == null;

  Map<String, Object?> toJson() => {
    if (name != null) 'name': name,
    if (elevationMm != null) 'elevationMm': elevationMm,
    if (floorThicknessMm != null) 'floorThicknessMm': floorThicknessMm,
    if (defaultWallHeightMm != null) 'defaultWallHeightMm': defaultWallHeightMm,
    if (defaultCeilingHeightMm != null)
      'defaultCeilingHeightMm': defaultCeilingHeightMm,
  };
}

/// Compact level-properties dialog (issue 0054): name, elevation, floor
/// thickness, default wall height, default ceiling height, applied as one
/// atomic edit. Explicit Apply/Cancel — nothing here submits on every
/// keystroke.
///
/// Seeded ONCE from `level` in [initState] and never resynced from a later
/// rebuild: a dialog is its own route with its own [State], so an unrelated
/// ambient refresh in the caller (another gesture, a background poll)
/// cannot clobber an in-progress edit here. Reading the CURRENT snapshot at
/// open time (not a stale cached copy) is the caller's job — pass the
/// [LevelInfo] straight from the live level list.
class LevelPropertiesDialog extends StatefulWidget {
  const LevelPropertiesDialog({
    required this.level,
    this.units = UnitSystem.metric,
    super.key,
  });

  final LevelInfo level;
  final UnitSystem units;

  /// Shows the dialog and returns the changes to apply, or `null` on
  /// Cancel/dismiss.
  static Future<LevelPropertiesChanges?> show(
    BuildContext context,
    LevelInfo level, {
    UnitSystem units = UnitSystem.metric,
  }) => showDialog<LevelPropertiesChanges>(
    context: context,
    builder: (_) => LevelPropertiesDialog(level: level, units: units),
  );

  @override
  State<LevelPropertiesDialog> createState() => _LevelPropertiesDialogState();
}

class _LevelPropertiesDialogState extends State<LevelPropertiesDialog> {
  late final _name = TextEditingController(text: widget.level.name);
  late final _elevation = TextEditingController(
    text: _formatMm(widget.level.elevationMm),
  );
  late final _floorThickness = TextEditingController(
    text: _formatMm(widget.level.floorThicknessMm),
  );
  late final _wallHeight = TextEditingController(
    text: _formatMm(widget.level.defaultWallHeightMm),
  );
  late final _ceilingHeight = TextEditingController(
    text: _formatMm(widget.level.defaultCeilingHeightMm),
  );

  String _formatMm(double v) => widget.units.editLength(v);

  /// `null` means the field is blank/unparsable, which the caller treats as
  /// invalid — never as "no change".
  double? _parse(String text, {double? original}) =>
      widget.units.parseLength(text, originalMm: original);

  String? _numericError(
    String text,
    bool Function(double) valid,
    String hint,
    double original,
  ) {
    final v = _parse(text, original: original);
    if (v == null) return 'Enter a number';
    if (!valid(v)) return hint;
    return null;
  }

  String? get _elevationError => _numericError(
    _elevation.text,
    LevelFieldBounds.elevationValid,
    'Must be within ±${widget.units.length(LevelFieldBounds.elevationAbsMaxMm)}',
    widget.level.elevationMm,
  );

  String? get _floorThicknessError => _numericError(
    _floorThickness.text,
    LevelFieldBounds.floorThicknessValid,
    '0 to ${widget.units.length(LevelFieldBounds.floorThicknessMaxMm)}',
    widget.level.floorThicknessMm,
  );

  String? get _wallHeightError => _numericError(
    _wallHeight.text,
    LevelFieldBounds.heightValid,
    'Above 0, up to ${widget.units.length(LevelFieldBounds.heightMaxMm)}',
    widget.level.defaultWallHeightMm,
  );

  String? get _ceilingHeightError => _numericError(
    _ceilingHeight.text,
    LevelFieldBounds.heightValid,
    'Above 0, up to ${widget.units.length(LevelFieldBounds.heightMaxMm)}',
    widget.level.defaultCeilingHeightMm,
  );

  bool get _allValid =>
      _elevationError == null &&
      _floorThicknessError == null &&
      _wallHeightError == null &&
      _ceilingHeightError == null;

  LevelPropertiesChanges get _changes {
    final name = _name.text != widget.level.name ? _name.text : null;
    double? changed(TextEditingController c, double original) {
      final v = _parse(c.text, original: original);
      return (v != null && v != original) ? v : null;
    }

    return LevelPropertiesChanges(
      name: name,
      elevationMm: changed(_elevation, widget.level.elevationMm),
      floorThicknessMm: changed(_floorThickness, widget.level.floorThicknessMm),
      defaultWallHeightMm: changed(
        _wallHeight,
        widget.level.defaultWallHeightMm,
      ),
      defaultCeilingHeightMm: changed(
        _ceilingHeight,
        widget.level.defaultCeilingHeightMm,
      ),
    );
  }

  @override
  void dispose() {
    _name.dispose();
    _elevation.dispose();
    _floorThickness.dispose();
    _wallHeight.dispose();
    _ceilingHeight.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // The WHOLE dialog — content AND actions — listens together: the Apply
    // button's enabled state depends on every field's validity, so it must
    // rebuild on the same change that revalidates them. Scoping the listener
    // to `content` alone left `actions` built once and stale.
    return ListenableBuilder(
      listenable: Listenable.merge([
        _name,
        _elevation,
        _floorThickness,
        _wallHeight,
        _ceilingHeight,
      ]),
      builder: (context, _) => AlertDialog(
        title: Text('${widget.level.name} properties'),
        content: SizedBox(
          width: 360,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  key: const ValueKey('level-name-field'),
                  controller: _name,
                  decoration: const InputDecoration(labelText: 'Name'),
                  // Whatever is typed is used verbatim, including empty or
                  // whitespace-only: R2.2 permits an empty name, and
                  // trimming here would silently rewrite what the user typed
                  // into something they never entered.
                ),
                const SizedBox(height: 8),
                _numberField(
                  key: 'level-elevation-field',
                  label: 'Elevation (${widget.units.lengthSymbol})',
                  controller: _elevation,
                  error: _elevationError,
                ),
                const SizedBox(height: 8),
                _numberField(
                  key: 'level-floor-thickness-field',
                  label: 'Floor thickness (${widget.units.lengthSymbol})',
                  controller: _floorThickness,
                  error: _floorThicknessError,
                ),
                const SizedBox(height: 8),
                _numberField(
                  key: 'level-wall-height-field',
                  label: 'Default wall height (${widget.units.lengthSymbol})',
                  controller: _wallHeight,
                  error: _wallHeightError,
                ),
                const SizedBox(height: 8),
                _numberField(
                  key: 'level-ceiling-height-field',
                  label:
                      'Default ceiling height (${widget.units.lengthSymbol})',
                  controller: _ceilingHeight,
                  error: _ceilingHeightError,
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            key: const ValueKey('level-properties-cancel'),
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('level-properties-apply'),
            onPressed: _allValid
                ? () => Navigator.of(context).pop(_changes)
                : null,
            child: const Text('Apply'),
          ),
        ],
      ),
    );
  }

  Widget _numberField({
    required String key,
    required String label,
    required TextEditingController controller,
    required String? error,
  }) => TextField(
    key: ValueKey(key),
    controller: controller,
    keyboardType: const TextInputType.numberWithOptions(
      signed: true,
      decimal: true,
    ),
    decoration: InputDecoration(
      labelText: label,
      errorText: error,
      errorMaxLines: 3,
    ),
  );
}
