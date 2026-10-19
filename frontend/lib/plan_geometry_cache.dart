import 'plan_painter.dart';

/// Presentation-only cache of core-derived level geometry, never an authority.
///
/// Scoped to a borrowed native session identity and retained visible levels.
/// Content keys come from the core. Null keys/failed reads are not cached, so
/// later refreshes retry and never display stale geometry after a failure.
/// This owns no native handles, images, camera state or GPU resources.
class PlanGeometryCache {
  int? _session;
  final Map<String, (String, Derived)> _levels = {};
  int? get session => _session;
  bool contains(String level) => _levels.containsKey(level);

  /// Returns true if the session changed, allowing other session-owned caches
  /// to release resources. Visibility eviction is independent of content keys.
  bool retain(int session, Set<String> visibleLevels) {
    final changed = _session != session;
    if (changed) _levels.clear();
    _session = session;
    _levels.removeWhere((id, _) => !visibleLevels.contains(id));
    return changed;
  }

  Derived resolve(String level, String? key, Derived? Function() load) {
    final cached = _levels[level];
    if (key != null && cached != null && cached.$1 == key) return cached.$2;
    final geometry = load();
    if (geometry != null && key != null) {
      _levels[level] = (key, geometry);
    } else {
      _levels.remove(level);
    }
    return geometry ?? Derived.empty;
  }

  void clear() {
    _levels.clear();
    _session = null;
  }
}
