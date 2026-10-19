/// Device-local presentation only. Document and bridge values remain in mm.
enum UnitSystem {
  metric,
  imperial;

  String get lengthSymbol => this == metric ? 'mm' : 'in';
  String get areaSymbol => this == metric ? 'm²' : 'ft²';
  double get millimetresPerUnit => this == metric ? 1 : 25.4;

  double fromMm(double mm) => mm / millimetresPerUnit;
  double toMm(double value) => value * millimetresPerUnit;

  /// Editable decimal inches (four decimals), or lossless metric values.
  String editLength(double mm) {
    final value = fromMm(mm);
    if (this == metric) {
      return value == value.roundToDouble()
          ? value.toStringAsFixed(0)
          : value.toString();
    }
    final text = value.toStringAsFixed(4).replaceFirst(RegExp(r'\.?0+$'), '');
    return text == '-0' ? '0' : text;
  }

  /// Retain the exact original when its displayed value is confirmed unchanged.
  /// Display rounding must never create an edit or accumulate conversion drift.
  double? parseLength(String text, {double? originalMm}) {
    final value = double.tryParse(text.trim());
    if (value == null) return null;
    if (originalMm != null &&
        value == double.tryParse(editLength(originalMm))) {
      return originalMm;
    }
    return toMm(value);
  }

  String length(double mm) =>
      '${this == metric ? mm.toStringAsFixed(0) : editLength(mm)} $lengthSymbol';

  /// Metric room text is supplied by the core, preserving its exact half-even
  /// rounding. Imperial is a view-only conversion of the unrounded core area.
  String area(double mm2, {String? metricText}) =>
      '${this == metric ? metricText ?? (mm2 / 1e6).toStringAsFixed(2) : (mm2 / (304.8 * 304.8)).toStringAsFixed(2)} $areaSymbol';
}
