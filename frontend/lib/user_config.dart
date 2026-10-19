import 'dart:convert';

import 'measurement_units.dart';
import 'platform/config_storage.dart';

export 'platform/config_storage.dart' show ConfigStorage;

/// Orientation of the divider: vertical places 2D left and 3D right.
enum SplitOrientation { vertical, horizontal }

/// Local user/device preferences. Unknown JSON keys survive updates. Reads are
/// bounded and unsupported/corrupt files are never silently overwritten.
class UserConfigStore {
  UserConfigStore(this.storage);
  UserConfigStore.inMemory() : storage = null;

  /// Where preferences persist; null keeps them in memory only.
  final ConfigStorage? storage;

  /// The largest preference document read or written, in bytes.
  static const int maxBytes = 1024 * 1024;

  /// The most documents' remembered active levels kept at once (issue 0117).
  /// The cap bounds the plan-key-to-level-ID map no matter how often the
  /// user switches between plans; once full, the least recently touched
  /// document's hint is dropped.
  static const int maxLastActiveLevels = 64;
  Map<String, Object?> _data = {'version': 1, 'assetDirectories': <String>[]};
  Map<String, String> _lastActiveLevels = const {};
  bool _writable = true;
  Future<void>? _pendingSave;
  UnitSystem get unitSystem => _data['unitSystem'] == 'imperial'
      ? UnitSystem.imperial
      : UnitSystem.metric;
  SplitOrientation get splitOrientation =>
      _data['splitOrientation'] == 'horizontal'
      ? SplitOrientation.horizontal
      : SplitOrientation.vertical;
  List<String> get assetDirectories =>
      List.unmodifiable((_data['assetDirectories'] as List).cast<String>());

  /// The last active level per document identity or legacy-file key, never
  /// editable by any document command — it
  /// is device-local state that emerges nowhere in the document. Unmodifiable
  /// in insertion order, which is most-recent-update last.
  Map<String, String> get lastActiveLevels =>
      Map.unmodifiable(_lastActiveLevels);

  /// The platform's per-user store: a file on desktop, browser storage on
  /// the web (issue 0088).
  static UserConfigStore standard() => UserConfigStore(standardConfigStorage());

  /// Wait for already queued writes before a normal application close. Callers
  /// of each saver receive its errors; a failed optional hint never blocks exit.
  Future<void> flush() => _pendingSave ?? Future<void>.value();

  Future<void> load() async {
    final source = storage;
    if (source == null) return;
    try {
      final text = await source.read(maxBytes);
      if (text == null) return;
      if (text.length > maxBytes)
        throw const FormatException('User config exceeds 1 MiB');
      final data = jsonDecode(utf8.decode(text)) as Map<String, Object?>;
      if (data['version'] != 1)
        throw const FormatException('Unsupported user config version');
      final dirs = data['assetDirectories'] ?? <String>[];
      if (dirs is! List ||
          dirs.length > 64 ||
          dirs.any(
            (p) =>
                p is! String ||
                !p.startsWith('/') && !RegExp(r'^[A-Za-z]:[\\/]').hasMatch(p),
          )) {
        throw const FormatException('Invalid configured asset directories');
      }
      final split = data['splitOrientation'];
      if (data.containsKey('unitSystem') &&
          data['unitSystem'] != 'metric' &&
          data['unitSystem'] != 'imperial') {
        throw const FormatException('Invalid unit system');
      }
      if (data.containsKey('splitOrientation') &&
          split != 'vertical' &&
          split != 'horizontal') {
        throw const FormatException('Invalid split orientation');
      }
      // The remembered levels are a device hint, not user-authored data: any
      // entry that does not validate is DROPPED rather than rejected, so it
      // can never block the other preferences' saving — only this member is
      // pruned away. Kept entries are non-empty strings, at most
      // [maxLastActiveLevels] of them in most-recent-first order.
      final remembered = data.remove('lastActiveLevels');
      final valid = <String, String>{};
      if (remembered is Map) {
        final entries = remembered.entries.toList().reversed;
        for (final entry in entries) {
          final key = entry.key;
          if (key is! String || key.isEmpty) continue;
          final value = entry.value;
          if (value is! String || value.isEmpty) continue;
          valid[key] = value;
          if (valid.length == maxLastActiveLevels) break;
        }
      }
      final oldestFirst = Map<String, String>.fromEntries(
        valid.entries.toList().reversed,
      );
      if (oldestFirst.isNotEmpty) data['lastActiveLevels'] = oldestFirst;
      _lastActiveLevels = oldestFirst;
      _data = {...data, 'assetDirectories': dirs};
    } catch (_) {
      _writable = false;
      rethrow;
    }
  }

  Future<void> saveDirectories(List<String> directories) async {
    if (!_writable)
      throw const FormatException(
        'Repair the existing user config before saving preferences',
      );
    final unique = directories.toSet().toList();
    if (unique.any(
      (p) => !p.startsWith('/') && !RegExp(r'^[A-Za-z]:[\\/]').hasMatch(p),
    )) {
      throw const FormatException('Asset directory paths must be absolute');
    }
    if (unique.length > 64)
      throw const FormatException(
        'At most 64 global asset directories are supported',
      );
    await _save(() => {..._data, 'version': 1, 'assetDirectories': unique});
  }

  Future<void> saveSplitOrientation(SplitOrientation orientation) =>
      _save(() => {..._data, 'splitOrientation': orientation.name});

  /// Save the dialog's choices together; failed persistence applies neither.
  Future<void> savePreferences({
    required SplitOrientation splitOrientation,
    required UnitSystem unitSystem,
  }) => _save(
    () => {
      ..._data,
      'splitOrientation': splitOrientation.name,
      'unitSystem': unitSystem.name,
    },
  );

  /// Remembers one document's last active level. The key is its document ID
  /// or the editor's local legacy-file key; null records nothing.
  /// Saving the same value again skips the write, so the hint must
  /// not churn the config file on every activation.
  ///
  /// Overwriting an entry moves it to the last (most recent) position; at
  /// [maxLastActiveLevels] entries the first (least recently touched)
  /// document's hint is dropped. This may throw exactly as the other savers
  /// do when the stored document is unwritable; the editor treats a failed
  /// hint as device-local and never fails the activation that triggered it.
  Future<void> saveLastActiveLevel(String? documentKey, String levelId) async {
    if (documentKey == null || documentKey.isEmpty || levelId.isEmpty) {
      return;
    }
    await _save(() {
      if (_lastActiveLevels[documentKey] == levelId) return null;
      final next = Map<String, String>.of(_lastActiveLevels)
        ..remove(documentKey)
        ..[documentKey] = levelId;
      while (next.length > maxLastActiveLevels) {
        next.remove(next.keys.first);
      }
      return {..._data, 'lastActiveLevels': next};
    });
  }

  // Build each update only after previous writes settle. Otherwise rapid
  // A -> B -> A activation or a simultaneous preference save loses updates.
  Future<void> _save(Map<String, Object?>? Function() update) {
    Future<void> writeUpdate() async {
      final data = update();
      if (data != null) await _write(data);
    }

    final previous = _pendingSave;
    final result = previous == null
        ? writeUpdate()
        : previous.then((_) => writeUpdate());
    late final Future<void> settled;
    void clearPending() {
      if (identical(_pendingSave, settled)) _pendingSave = null;
    }

    settled = result.then<void>(
      (_) => clearPending(),
      onError: (Object _) => clearPending(),
    );
    _pendingSave = settled;
    return result;
  }

  Future<void> _write(Map<String, Object?> data) async {
    if (!_writable) {
      throw const FormatException(
        'Repair the existing user config before saving preferences',
      );
    }
    final text = '${const JsonEncoder.withIndent('  ').convert(data)}\n';
    if (utf8.encode(text).length > 1024 * 1024)
      throw const FormatException('User config exceeds 1 MiB');
    await storage?.write(text);
    _data = data;
    final remembered = data['lastActiveLevels'];
    _lastActiveLevels = remembered is Map<String, String>
        ? Map<String, String>.of(remembered)
        : const {};
  }
}
