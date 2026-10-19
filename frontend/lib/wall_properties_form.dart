import 'package:flutter/material.dart';

import 'plan_painter.dart';
import 'measurement_units.dart';

/// R3.0.4/R7.1.6 bounds this form enforces locally before ever asking the
/// core. The core remains the authority; these exist so a plainly invalid
/// measurement is refused at the field rather than after a round trip.
class WallFieldBounds {
  static const maxMm = 1000000.0;

  /// Length, thickness and height share one rule: finite and strictly
  /// positive (R3.0.4), with the same practical ceiling the other forms use.
  static bool measurementValid(double v) => v.isFinite && v > 0 && v <= maxMm;
}

/// Only the fields the user actually changed, ready for the `setWall` gesture
/// (`{kind: 'setWall', wallId, ...changes.toJson()}`). `lengthMm` is an
/// interaction-level measurement: the bridge resolves it about the stored
/// Start (R7.1.6), so this form never derives an endpoint itself. Never sent
/// at all when [isEmpty] — an Apply with nothing changed must not mint a
/// no-op batch.
class WallPropertiesChanges {
  const WallPropertiesChanges({this.lengthMm, this.thicknessMm, this.heightMm});

  final double? lengthMm;
  final double? thicknessMm;
  final double? heightMm;

  bool get isEmpty =>
      lengthMm == null && thicknessMm == null && heightMm == null;

  Map<String, Object?> toJson() => {
    if (lengthMm != null) 'lengthMm': lengthMm,
    if (thicknessMm != null) 'thicknessMm': thicknessMm,
    if (heightMm != null) 'heightMm': heightMm,
  };
}

/// Wall-measurements dialog (issue 0071): centreline length, thickness and
/// height applied as ONE atomic `setWall` batch, so a single undo restores
/// every changed value together.
/// The multiple-wall form (R7.1.7) edits only height/thickness via `setWalls`;
/// mixed values start blank, and any blank field means leave unchanged.
///
/// Seeded ONCE from `wall` in the field initialisers and never resynced from
/// a later rebuild: a dialog is its own route with its own [State], so an
/// unrelated ambient refresh in the caller cannot clobber an in-progress
/// edit. Reading the CURRENT snapshot at open time is the caller's job — pass
/// the live [Wall] straight from the scene, never a stale copy.
class WallPropertiesDialog extends StatefulWidget {
  const WallPropertiesDialog({
    required Wall this.wall,
    this.units = UnitSystem.metric,
    super.key,
  }) : walls = null;

  const WallPropertiesDialog.multiple({
    required List<Wall> this.walls,
    this.units = UnitSystem.metric,
    super.key,
  }) : assert(walls.length > 1),
       wall = null;

  final Wall? wall;
  final List<Wall>? walls;
  final UnitSystem units;

  /// Shows the dialog and returns the changes to apply, or `null` on
  /// Cancel/dismiss.
  static Future<WallPropertiesChanges?> show(
    BuildContext context,
    Wall wall, {
    UnitSystem units = UnitSystem.metric,
  }) => showDialog<WallPropertiesChanges>(
    context: context,
    builder: (_) => WallPropertiesDialog(wall: wall, units: units),
  );

  static Future<WallPropertiesChanges?> showMultiple(
    BuildContext context,
    List<Wall> walls, {
    UnitSystem units = UnitSystem.metric,
  }) => showDialog<WallPropertiesChanges>(
    context: context,
    builder: (_) => WallPropertiesDialog.multiple(walls: walls, units: units),
  );

  @override
  State<WallPropertiesDialog> createState() => _WallPropertiesDialogState();
}

class _WallPropertiesDialogState extends State<WallPropertiesDialog> {
  bool get _bulk => widget.walls != null;

  double? _common(double Function(Wall) value) {
    final walls = widget.walls ?? [widget.wall!];
    final first = value(walls.first);
    return walls.every((wall) => value(wall) == first) ? first : null;
  }

  late final _originalThickness = _common((wall) => wall.thicknessMm);
  late final _originalHeight = _common((wall) => wall.heightMm);
  late final _length = TextEditingController(
    text: _bulk ? '' : _formatMm(widget.wall!.lengthMm),
  );
  late final _thickness = TextEditingController(
    text: _originalThickness == null ? '' : _formatMm(_originalThickness),
  );
  late final _height = TextEditingController(
    text: _originalHeight == null ? '' : _formatMm(_originalHeight),
  );

  String _formatMm(double v) => widget.units.editLength(v);

  /// Blank means no change only in the multiple-wall form.
  double? _parse(String text, {double? original}) =>
      widget.units.parseLength(text, originalMm: original);

  double? _original(TextEditingController c) => identical(c, _length)
      ? widget.wall?.lengthMm
      : identical(c, _thickness)
      ? _originalThickness
      : _originalHeight;

  String? _error(TextEditingController controller) {
    if (_bulk && controller.text.trim().isEmpty) return null;
    final v = _parse(controller.text, original: _original(controller));
    if (v == null) return 'Enter a number';
    if (!WallFieldBounds.measurementValid(v)) {
      return 'Above 0, up to ${widget.units.length(WallFieldBounds.maxMm)}';
    }
    return null;
  }

  bool get _allValid =>
      (_bulk || _error(_length) == null) &&
      _error(_thickness) == null &&
      _error(_height) == null;

  WallPropertiesChanges get _changes {
    double? changed(TextEditingController c, double? original) {
      final v = _parse(c.text, original: original);
      return (v != null && v != original) ? v : null;
    }

    return WallPropertiesChanges(
      lengthMm: _bulk ? null : changed(_length, widget.wall!.lengthMm),
      thicknessMm: changed(_thickness, _originalThickness),
      heightMm: changed(_height, _originalHeight),
    );
  }

  @override
  void dispose() {
    _length.dispose();
    _thickness.dispose();
    _height.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    // The WHOLE dialog — content AND actions — listens together: the Apply
    // button's enabled state depends on every field's validity, so it must
    // rebuild on the same change that revalidates them.
    return ListenableBuilder(
      listenable: Listenable.merge([_length, _thickness, _height]),
      builder: (context, _) => AlertDialog(
        title: Text(
          _bulk ? 'Edit ${widget.walls!.length} walls' : 'Wall measurements',
        ),
        content: SizedBox(
          width: 360,
          child: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                if (_bulk) ...[
                  const Text('Blank fields leave existing values unchanged.'),
                  const SizedBox(height: 8),
                ] else ...[
                  _numberField(
                    key: 'wall-lengthMm-field',
                    label: 'Length (${widget.units.lengthSymbol})',
                    controller: _length,
                  ),
                  const SizedBox(height: 8),
                ],
                _numberField(
                  key: 'wall-thicknessMm-field',
                  label: 'Thickness (${widget.units.lengthSymbol})',
                  controller: _thickness,
                ),
                const SizedBox(height: 8),
                _numberField(
                  key: 'wall-heightMm-field',
                  label: 'Height (${widget.units.lengthSymbol})',
                  controller: _height,
                ),
              ],
            ),
          ),
        ),
        actions: [
          TextButton(
            key: const ValueKey('wall-properties-cancel'),
            onPressed: () => Navigator.of(context).pop(),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('wall-properties-apply'),
            onPressed: _allValid && (!_bulk || !_changes.isEmpty)
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
  }) => TextField(
    key: ValueKey(key),
    controller: controller,
    keyboardType: const TextInputType.numberWithOptions(decimal: true),
    decoration: InputDecoration(
      labelText: label,
      hintText: _bulk && _original(controller) == null ? 'Mixed' : null,
      errorText: _error(controller),
      errorMaxLines: 3,
    ),
  );
}
