import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';

import 'live_property_edits.dart';
import 'measurement_units.dart';

/// Live Floor-panel properties. Rust remains authoritative for stair geometry.
/// Typing is coalesced; Enter/focus loss flushes immediately. Acknowledgements
/// preserve the focused editor/caret; history/context changes cancel stale work.
class StaircasePropertiesForm extends StatefulWidget {
  const StaircasePropertiesForm({
    super.key,
    required this.name,
    required this.properties,
    required this.onChanged,
    this.revision = 0,
    this.enabled = true,
    this.units = UnitSystem.metric,
    this.liveEdits,
  });

  static const editDelay = Duration(milliseconds: 120);
  final String name;
  final Map<String, Object?> properties;
  final bool Function(Map<String, Object?> changes) onChanged;
  final int revision;
  final bool enabled;
  final UnitSystem units;
  final LivePropertyEdits? liveEdits;

  @override
  State<StaircasePropertiesForm> createState() =>
      _StaircasePropertiesFormState();
}

class _StaircasePropertiesFormState extends State<StaircasePropertiesForm> {
  static const _labels = {
    'name': 'Name',
    'startElevationMm': 'Start elevation',
    'endElevationMm': 'End elevation',
    'risers': 'Steps (risers)',
    'widthMm': 'Width',
    'guardHeightMm': 'Guard height',
  };
  static const _help = {
    'startElevationMm': 'Upper end, relative to the plan datum.',
    'endElevationMm': 'Lower end, relative to the plan datum.',
    'risers': 'N risers generate N−1 treads.',
    'guardHeightMm': 'Set to zero for no guards.',
  };
  final _form = GlobalKey<FormState>();
  late Map<String, Object?> _baseline = _source;
  late final _controllers = {
    for (final key in _labels.keys)
      key: TextEditingController(text: _display(key, _baseline[key])),
  };
  late final _focus = {for (final key in _labels.keys) key: FocusNode()};
  final _observed = <String, TextEditingValue>{};
  final _rejected = <String>{};
  final _acceptedText = <String, String>{};
  Timer? _timer;
  VoidCallback? _detach;
  bool _synchronizing = false, _cancelled = false, _awaitingEcho = false;

  Map<String, Object?> get _source => {
    'name': widget.name,
    for (final key in _labels.keys.where((key) => key != 'name'))
      key: widget.properties[key],
  };

  String _display(String key, Object? value) => key == 'name' || key == 'risers'
      ? '$value'
      : widget.units.editLength((value as num).toDouble());

  @override
  void initState() {
    super.initState();
    for (final key in _labels.keys) {
      _observed[key] = _controllers[key]!.value;
      _controllers[key]!.addListener(() => _edited(key));
      _focus[key]!.addListener(() {
        if (!_focus[key]!.hasFocus) _flush();
      });
    }
    _attach();
  }

  void _attach() {
    _detach = widget.liveEdits?.attach(flush: _flush, cancel: _cancel);
  }

  @override
  void didUpdateWidget(StaircasePropertiesForm oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.liveEdits != oldWidget.liveEdits) {
      _detach?.call();
      _attach();
    }
    final source = _source;
    final ownEcho = _awaitingEcho && _matchesAcknowledgement(source);
    if (widget.units != oldWidget.units ||
        widget.enabled != oldWidget.enabled ||
        (!mapEquals(source, _baseline) && !ownEcho) ||
        (widget.revision != oldWidget.revision && !ownEcho)) {
      _cancel();
      _baseline = source;
      _acceptedText.clear();
      _rejected.clear();
      _synchronizing = true;
      for (final key in _labels.keys) {
        final controller = _controllers[key]!,
            text = _display(key, source[key]);
        if (controller.text != text)
          controller.value = TextEditingValue(
            text: text,
            selection: TextSelection.collapsed(offset: text.length),
          );
        _observed[key] = controller.value;
      }
      _synchronizing = false;
    } else if (ownEcho) {
      // Adopt Rust's precise result without rewriting a focused controller or
      // cancelling edits made since submission. Retain accepted text so the
      // unchanged decimal input cannot submit the rounding difference again.
      _baseline = source;
    }
    _awaitingEcho = false;
  }

  bool _matchesAcknowledgement(Map<String, Object?> source) {
    for (final key in _labels.keys) {
      final a = source[key], b = _baseline[key];
      if (a == b) continue;
      // Only elevations take the subtract-datum/add-datum round trip. Four
      // binary64 epsilons at the format's 10^7 mm coordinate bound cover its
      // arithmetic error (< 10^-8 mm), not a meaningful external edit.
      const toleranceMm = 4 * 2.220446049250313e-16 * 10000000;
      if (!key.endsWith('ElevationMm') ||
          a is! num ||
          b is! num ||
          !a.isFinite ||
          !b.isFinite ||
          (a - b).abs() > toleranceMm) {
        return false;
      }
    }
    return true;
  }

  Object? _parse(String key) {
    final text = _controllers[key]!.text;
    if (key == 'name') return text.trim().isEmpty ? null : text;
    if (key == 'risers') {
      final value = int.tryParse(text.trim());
      return value != null && value >= 2 && value <= 256 ? value : null;
    }
    if (_acceptedText[key] == text) return _baseline[key];
    final value = widget.units.parseLength(
      text,
      originalMm: (_baseline[key] as num).toDouble(),
    );
    if (value == null || !value.isFinite) return null;
    if (key.endsWith('ElevationMm'))
      return value.abs() <= 10000000 ? value : null;
    return value <= 1000000 && (key == 'guardHeightMm' ? value >= 0 : value > 0)
        ? value
        : null;
  }

  String? _error(String key) {
    if (_rejected.contains(key)) return 'Value refused; see status';
    if (_parse(key) == null) {
      if (key == 'name') return 'Enter a name';
      if (key == 'risers') return 'Use 2–256 whole risers';
      return 'Enter a valid ${widget.units.lengthSymbol} value';
    }
    if (key.endsWith('ElevationMm')) {
      final start = _parse('startElevationMm') as num?,
          end = _parse('endElevationMm') as num?;
      if (start != null && end != null && start <= end)
        return 'Start must be above end';
    }
    return null;
  }

  void _edited(String key) {
    if (_synchronizing) return;
    final value = _controllers[key]!.value, previous = _observed[key]!;
    _observed[key] = value;
    // Caret/selection changes must not enqueue geometry work or extend a burst.
    if (value.text == previous.text && value.composing == previous.composing)
      return;
    _cancelled = false;
    _rejected.remove(key);
    _form.currentState?.validate();
    _timer?.cancel();
    if (widget.enabled)
      _timer = Timer(StaircasePropertiesForm.editDelay, _flush);
  }

  void _cancel() {
    _timer?.cancel();
    _timer = null;
    _cancelled = true;
    _awaitingEcho = false;
  }

  void _flush() {
    _timer?.cancel();
    _timer = null;
    if (!mounted || !widget.enabled || _cancelled) return;
    final changes = <String, Object?>{
      for (final key in _labels.keys)
        if (_controllers[key]!.value.composing.isCollapsed &&
            _error(key) == null &&
            _parse(key) != _baseline[key])
          key: _parse(key),
    };
    if (changes.isEmpty) return;
    if (widget.onChanged(changes)) {
      _baseline.addAll(changes);
      for (final key in changes.keys) {
        _acceptedText[key] = _controllers[key]!.text;
      }
      _awaitingEcho = true;
    } else if (mounted) {
      _rejected.addAll(changes.keys);
      _form.currentState?.validate();
    }
  }

  @override
  void dispose() {
    _cancel();
    _detach?.call();
    for (final node in _focus.values) {
      node.dispose();
    }
    for (final controller in _controllers.values) {
      controller.dispose();
    }
    super.dispose();
  }

  Widget _field(String key) {
    final field = TextFormField(
      key: PageStorageKey('stair-$key'),
      controller: _controllers[key],
      focusNode: _focus[key],
      enabled: widget.enabled,
      decoration: InputDecoration(
        labelText: _labels[key],
        suffixText: key == 'name' || key == 'risers'
            ? null
            : widget.units.lengthSymbol,
        isDense: true,
        border: const OutlineInputBorder(),
        errorMaxLines: 2,
      ),
      keyboardType: key == 'name'
          ? TextInputType.text
          : TextInputType.numberWithOptions(
              decimal: key != 'risers',
              signed: key.endsWith('ElevationMm'),
            ),
      autovalidateMode: AutovalidateMode.onUserInteraction,
      validator: (_) => _error(key),
      onFieldSubmitted: (_) => _flush(),
    );
    return _help[key] == null
        ? field
        : Tooltip(message: _help[key]!, child: field);
  }

  @override
  Widget build(BuildContext context) => Form(
    key: _form,
    child: LayoutBuilder(
      builder: (context, constraints) {
        Widget pair(String a, String b) => constraints.maxWidth >= 260
            ? Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(child: _field(a)),
                  const SizedBox(width: 12),
                  Expanded(child: _field(b)),
                ],
              )
            : Column(
                children: [_field(a), const SizedBox(height: 12), _field(b)],
              );
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: [
            _field('name'),
            const SizedBox(height: 12),
            pair('startElevationMm', 'endElevationMm'),
            const SizedBox(height: 12),
            pair('risers', 'widthMm'),
            const SizedBox(height: 12),
            _field('guardHeightMm'),
          ],
        );
      },
    ),
  );
}
