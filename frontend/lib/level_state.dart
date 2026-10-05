import 'package:flutter/foundation.dart' show setEquals;

/// R2.4's four per-device level states. `visible`/`ghosted` are both
/// rendered (reduced emphasis for `ghosted`); `hidden` is not rendered.
/// The active level is never stored as any of these — it is always
/// implicitly visible (R2.4: "no separate editable flag exists").
enum LevelVisibility { visible, ghosted, hidden }

/// The authoritative R2.1 level fields this app reads: existence, name,
/// elevation, floor thickness, default wall height and default ceiling
/// height. Only wall height has a specified copying consumer today
/// (interactive/CLI wall creation with no explicit height override resolves
/// the level's CURRENT `defaultWallHeightMm` at commit time, R2.1); floor
/// thickness and ceiling height are stored and editable here but no entity
/// kind is specified to copy them yet — this class does not assume one.
/// Background anchors are not read here; nothing in this class is view
/// state, and none of it is ever mutated in place — an edit always replaces
/// the whole [LevelInfo] via a fresh snapshot read (issue 0054's properties
/// form reads from this, never from its own prior form state, so a metadata
/// edit can never go stale here).
class LevelInfo {
  const LevelInfo({
    required this.id,
    required this.name,
    required this.elevationMm,
    required this.floorThicknessMm,
    required this.defaultWallHeightMm,
    required this.defaultCeilingHeightMm,
  });

  final String id;
  final String name;
  final double elevationMm;
  final double floorThicknessMm;
  final double defaultWallHeightMm;
  final double defaultCeilingHeightMm;

  @override
  bool operator ==(Object other) =>
      other is LevelInfo &&
      other.id == id &&
      other.name == name &&
      other.elevationMm == elevationMm &&
      other.floorThicknessMm == floorThicknessMm &&
      other.defaultWallHeightMm == defaultWallHeightMm &&
      other.defaultCeilingHeightMm == defaultCeilingHeightMm;

  @override
  int get hashCode => Object.hash(
    id,
    name,
    elevationMm,
    floorThicknessMm,
    defaultWallHeightMm,
    defaultCeilingHeightMm,
  );

  @override
  String toString() => 'LevelInfo($id, $name, ${elevationMm}mm)';
}

/// The level-view subset of R2.2–R2.6 and R8.7: active level and
/// visibility/ghost flags. Camera, units and grid defaults belong to the
/// editor. No document, FFI or
/// widget dependency, and nothing here is ever persisted (R2.6, R8.7) —
/// every open re-derives it via [reset].
class LevelViewState {
  List<LevelInfo> _levels = const [];
  String? _activeId;
  final Map<String, LevelVisibility> _flags = {};
  int _revision = 0;

  /// All levels, ascending elevation with stable-ID tie-break (R2.2).
  List<LevelInfo> get levels => List.unmodifiable(_levels);

  /// The exactly-one active level id, or `null` only for the defensive
  /// zero-level case (not a normatively valid document, R2.2).
  String? get activeId => _activeId;

  /// Bumped on every OBSERVABLE view-state change: an active transition, an
  /// applied visibility change, or a level added/removed by [reconcile].
  /// Metadata-only changes (name/elevation) on an unchanged level set do
  /// not bump it, and neither does any rejected or no-op call.
  int get revision => _revision;

  /// Every level's flag; the active level always reads `visible`,
  /// regardless of what (if anything) is stored for it internally.
  Map<String, LevelVisibility> get visibility => Map.unmodifiable({
    for (final l in _levels)
      l.id: l.id == _activeId
          ? LevelVisibility.visible
          : (_flags[l.id] ?? LevelVisibility.ghosted),
  });

  /// Render/z-order (R2.4): ghost tier, then visible tier, then the active
  /// level last (highest); hidden levels are excluded entirely. Ties within
  /// a tier keep [levels]' ascending-elevation/stable-ID order.
  List<LevelInfo> get paintOrder {
    final ghosted = <LevelInfo>[];
    final visible = <LevelInfo>[];
    LevelInfo? active;
    for (final l in _levels) {
      if (l.id == _activeId) {
        active = l;
        continue;
      }
      switch (_flags[l.id] ?? LevelVisibility.ghosted) {
        case LevelVisibility.hidden:
          break;
        case LevelVisibility.ghosted:
          ghosted.add(l);
        case LevelVisibility.visible:
          visible.add(l);
      }
    }
    return List.unmodifiable([
      ...ghosted,
      ...visible,
      if (active != null) active,
    ]);
  }

  /// Deterministic reopen (R2.6): active is the lowest-elevation level
  /// (stable-ID tie-break), every other level starts ghosted. Always a real
  /// transition — a fresh open is never a no-op — so this always advances
  /// [revision], including reopening the SAME document.
  void reset(Iterable<LevelInfo> incoming) {
    final sorted = _validatedSorted(incoming);
    _levels = sorted;
    _activeId = sorted.isEmpty ? null : sorted.first.id;
    _flags
      ..clear()
      ..addEntries(
        sorted
            .where((l) => l.id != _activeId)
            .map((l) => MapEntry(l.id, LevelVisibility.ghosted)),
      );
    _revision++;
  }

  /// Reconciles against a fresh authoritative level list (e.g. after an
  /// accepted document change): surviving levels keep their stored flag and
  /// the active level if it survives; new levels start ghosted; a removed
  /// active level falls back to the new lowest-elevation level (R2.3/R2.4's
  /// delete-active-level row). An empty incoming list is a DEFENSIVE state
  /// (`activeId` becomes `null`), not a claim that zero levels is a valid
  /// document (R2.2) — issue 0051's own boundary note.
  ///
  /// Returns whether anything observable changed (the level set itself, or
  /// which level is active); a reconcile that only refreshes metadata
  /// (name/elevation) for the SAME set of ids, with the same active level,
  /// does not advance [revision] and returns `false`.
  bool reconcile(Iterable<LevelInfo> incoming) {
    final sorted = _validatedSorted(incoming);
    final previousIds = _levels.map((l) => l.id).toSet();
    final newIds = sorted.map((l) => l.id).toSet();
    final changed = !setEquals(previousIds, newIds);

    _flags.removeWhere((id, _) => !newIds.contains(id));
    for (final l in sorted) {
      if (l.id != _activeId && !_flags.containsKey(l.id)) {
        _flags[l.id] = LevelVisibility.ghosted;
      }
    }

    if (sorted.isEmpty) {
      _activeId = null;
    } else if (_activeId == null || !newIds.contains(_activeId)) {
      _activeId = sorted.first.id;
      _flags.remove(_activeId);
    }

    _levels = sorted;
    if (changed) _revision++;
    return changed;
  }

  /// R2.4: activating an unknown or already-active id is a no-op (`false`).
  /// Otherwise the prior active level becomes visible-ghosted, `id` becomes
  /// active (forced visible), and [revision] advances.
  bool activate(String id) {
    if (id == _activeId) return false;
    if (!_levels.any((l) => l.id == id)) return false;
    final priorActive = _activeId;
    if (priorActive != null) _flags[priorActive] = LevelVisibility.ghosted;
    _activeId = id;
    _flags.remove(id);
    _revision++;
    return true;
  }

  /// R2.4: rejected (`false`, no state change) for an unknown id or for the
  /// active level (hide/ghost requests on it are rejected outright, and it
  /// is always already visible). Setting a non-active level to the flag it
  /// already has is a no-op (`false`). Otherwise applies the flag and
  /// advances [revision].
  bool setVisibility(String id, LevelVisibility value) {
    if (!_levels.any((l) => l.id == id)) return false;
    if (id == _activeId) return false;
    final current = _flags[id] ?? LevelVisibility.ghosted;
    if (current == value) return false;
    _flags[id] = value;
    _revision++;
    return true;
  }

  static int _compare(LevelInfo a, LevelInfo b) {
    final byElevation = a.elevationMm.compareTo(b.elevationMm);
    return byElevation != 0 ? byElevation : a.id.compareTo(b.id);
  }

  /// Validates finite elevations and unique ids ATOMICALLY: a violation
  /// throws before any field of this controller is touched, so a rejected
  /// call never leaves a partial mutation behind. No UUID parsing — ids are
  /// opaque strings.
  static List<LevelInfo> _validatedSorted(Iterable<LevelInfo> incoming) {
    final list = incoming.toList(growable: false);
    final seen = <String>{};
    for (final level in list) {
      if (!level.elevationMm.isFinite) {
        throw ArgumentError.value(
          level.elevationMm,
          'elevationMm',
          'must be finite (level ${level.id})',
        );
      }
      if (!seen.add(level.id)) {
        throw ArgumentError.value(level.id, 'id', 'duplicate level id');
      }
    }
    final sorted = List<LevelInfo>.of(list)..sort(_compare);
    return sorted;
  }
}
