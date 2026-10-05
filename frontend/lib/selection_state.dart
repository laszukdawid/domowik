import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter/painting.dart' show Offset;

/// R7.1.1: an id set plus one primary (the most recent addition). `primary`,
/// when set, MUST be a member of `ids` (R7.4: "No implementation may invent
/// a primary").
///
/// Enforced with [ArgumentError], not `assert`: this type is public and can
/// be constructed by any caller, including in release builds where asserts
/// are stripped. An impossible snapshot must never be constructible at all,
/// not merely rejected in debug mode.
class SelectionSnapshot {
  SelectionSnapshot(Set<String> ids, this.primary)
    : ids = Set.unmodifiable(ids) {
    if (primary != null && !this.ids.contains(primary)) {
      throw ArgumentError.value(
        primary,
        'primary',
        'must be a member of ids ($ids)',
      );
    }
  }

  static final SelectionSnapshot empty = SelectionSnapshot(const {}, null);

  final Set<String> ids;
  final String? primary;

  @override
  bool operator ==(Object other) =>
      other is SelectionSnapshot &&
      other.primary == primary &&
      other.ids.length == ids.length &&
      other.ids.containsAll(ids);

  @override
  int get hashCode =>
      Object.hash(primary, ids.fold<int>(0, (h, id) => h ^ id.hashCode));

  @override
  String toString() => 'SelectionSnapshot($ids, primary: $primary)';
}

/// R7.2.3/R7.2.3a's discrete reset inputs, sampled at one instant. `viewKey`
/// is the caller's own immutable, structurally-equatable stand-in for the
/// view transform (e.g. a record of zoom/offset) — this class only compares
/// it with `==`. Equality is by value across every field.
class SelectionContext {
  const SelectionContext({
    required this.viewKey,
    required this.levelId,
    required this.documentRevision,
    required this.visibilityRevision,
    required this.modifierMask,
    required this.pointerKind,
  });

  final Object viewKey;
  final String? levelId;
  final int documentRevision;
  final int visibilityRevision;
  final int modifierMask;
  final PointerDeviceKind pointerKind;

  @override
  bool operator ==(Object other) =>
      other is SelectionContext &&
      other.viewKey == viewKey &&
      other.levelId == levelId &&
      other.documentRevision == documentRevision &&
      other.visibilityRevision == visibilityRevision &&
      other.modifierMask == modifierMask &&
      other.pointerKind == pointerKind;

  @override
  int get hashCode => Object.hash(
    viewKey,
    levelId,
    documentRevision,
    visibilityRevision,
    modifierMask,
    pointerKind,
  );
}

/// R7.2.3a modifier family bits. Ctrl/Meta/Shift enable toggling (mask
/// [toggleModifierMask]); Alt alone does not.
const int controlModifier = 1;
const int metaModifier = 2;
const int shiftModifier = 4;
const int altModifier = 8;
const int toggleModifierMask = controlModifier | metaModifier | shiftModifier;

/// Pure selection/cycle state machine for R7.1/R7.2/R7.4 (issue 0049).
///
/// No document, FFI or widget dependency: the caller supplies native hit
/// results, raw pointer positions and context snapshots at event time, and
/// owns everything geometric (view transform, hit testing, painting).
///
/// # Why observation is a separate channel from `click`
///
/// R7.2.3a's resets fire on the OBSERVED transition itself, even when it
/// reverts before the next click (CR1/CR3/CR5; L10). A cycle that was only
/// ever compared click-to-click would miss a round trip entirely. The
/// caller MUST therefore call [observeContext]/[observePointer] at the time
/// of each real transition (each pan tick, each modifier key edge, each
/// level/visibility change, each pointer-kind change) — not once, lazily,
/// right before the next click. [click] runs the same two checks itself
/// against its own `position`/`context`, so a click is never less strict
/// than an explicit observation would have been.
class SelectionController {
  Set<String> _ids = const {};
  String? _primary;

  List<String>? _cycleList;
  int? _cycleIndex;
  Offset? _anchorPosition;
  SelectionContext? _anchorContext;

  SelectionSnapshot get snapshot => SelectionSnapshot(_ids, _primary);
  Set<String> get ids => Set.unmodifiable(_ids);
  String? get primary => _primary;

  /// R7.2.3/R7.2.3a: view, level, visibility, modifier and pointer-kind
  /// changes reset the cycle when OBSERVED, whether or not a later value
  /// matches the anchor's again. A no-op call (context unchanged, or no
  /// cycle active) changes nothing.
  void observeContext(SelectionContext context) => _applyContext(context);

  /// R7.2.3a: the anchor is the FIRST click's position, in logical pixels.
  /// A finite position observed strictly beyond the applicable radius (12px
  /// mouse/stylus, 24px touch; the boundary itself is inclusive) resets the
  /// cycle immediately, even if the pointer returns before the next click.
  /// A non-finite position is rejected whole: no state change.
  void observePointer(Offset position) => _applyPointer(position);

  /// R7.2.1/R7.2.2/R7.2.2a/R7.2.4/R7.2.4a/R7.2.6: one click or tap.
  ///
  /// `rawOrderedIds` is the native hit order (R7.1.3/R6.3.1), possibly
  /// containing one entity more than once under different feature classes
  /// (R6.3.2); this method derives the click's own entity list by keeping
  /// each id's FIRST occurrence (R7.2.2a) before doing anything else.
  ///
  /// A non-finite `position` is rejected whole: no state change, including
  /// no observation side effect. Otherwise this observes `context` and
  /// `position` exactly as [observeContext]/[observePointer] would, then:
  /// an empty entity list clears selection and ends the cycle (also true
  /// under a held toggle modifier); a held Ctrl/Meta/Shift toggles the
  /// TOP-RANKED entity's membership and ends any cycle rather than
  /// participating in one — R7.2.4a (issue 0049, fixture L12): cycling
  /// (R7.2.2/R7.2.2a) is a plain-click-only concept; Alt alone stays on the
  /// plain-click path below, never this branch, since [toggleModifierMask]
  /// excludes it. A held toggle modifier repeated at the same top target
  /// therefore toggles it on and off rather than cycling to the next
  /// overlapping entity, and the next PLAIN click afterward always
  /// restarts at the top-ranked entity, because every toggle click ends the
  /// cycle (L12).
  ///
  /// Otherwise this is a plain click: if the entity list is unchanged from
  /// the active cycle (after observation), advance to the next entity and
  /// wrap; otherwise (first click, or any reset already fired) select the
  /// top-ranked entity and start a fresh cycle anchored at this
  /// `position`/`context`.
  void click(
    Iterable<String> rawOrderedIds,
    Offset position,
    SelectionContext context,
  ) {
    if (!position.dx.isFinite || !position.dy.isFinite) return;
    final entities = _dedupeFirstOccurrence(rawOrderedIds);

    _applyContext(context);
    _applyPointer(position);

    if (entities.isEmpty) {
      _ids = const {};
      _primary = null;
      _endCycle();
      return;
    }

    final toggling = (context.modifierMask & toggleModifierMask) != 0;
    if (toggling) {
      final target = entities.first;
      final ids = Set<String>.of(_ids);
      var primary = _primary;
      if (ids.remove(target)) {
        if (primary == target) primary = null;
      } else {
        ids.add(target);
        primary = target;
      }
      _ids = ids;
      _primary = primary;
      _endCycle();
      return;
    }

    final continuing = _cycleList != null && listEquals(_cycleList, entities);
    if (continuing) {
      _cycleIndex = (_cycleIndex! + 1) % entities.length;
    } else {
      _cycleList = List.unmodifiable(entities);
      _cycleIndex = 0;
      _anchorPosition = position;
      _anchorContext = context;
    }
    final chosen = entities[_cycleIndex!];
    _ids = {chosen};
    _primary = chosen;
  }

  /// R7.3.2: applies an already native-key-ordered marquee result.
  ///
  /// Geometry, corner validity and candidate ordering belong to the native
  /// query; this method only projects feature hits to distinct entity IDs.
  /// Every accepted marquee ends click cycling, including an additive marquee
  /// that adds no entity.
  void marquee(Iterable<String> orderedIds, {required bool additive}) {
    final ids = _dedupeFirstOccurrence(orderedIds);
    if (!additive) {
      _ids = ids.toSet();
      _primary = ids.isEmpty ? null : ids.last;
      _endCycle();
      return;
    }

    final next = Set<String>.of(_ids);
    String? lastAdded;
    for (final id in ids) {
      if (next.add(id)) lastAdded = id;
    }
    _ids = next;
    if (lastAdded != null) _primary = lastAdded;
    _endCycle();
  }

  /// R7.2.6: a plain press's tentative drag-primary selection. A no-op when
  /// `entity` is already primary (a press on the current primary preserves
  /// the whole selection for a potential drag); otherwise selection becomes
  /// `{entity}` alone. Cycle state is untouched either way — this is
  /// transient press-time state, never a click.
  void selectForDrag(String entity) {
    if (entity == _primary) return;
    _ids = {entity};
    _primary = entity;
  }

  /// Sets selection to `snapshot` and nothing else: cycle state is left
  /// exactly as it was, never revived. Used both for R6.5.9 cancellation
  /// (pre-gesture selection restored exactly) and for R7.2.6 (a release
  /// that classifies as a click evaluates R7.2.2 against the PRE-PRESS
  /// selection — the caller restores it before calling [click]).
  void restoreSelection(SelectionSnapshot snapshot) {
    _ids = Set<String>.of(snapshot.ids);
    _primary = snapshot.primary;
  }

  /// R7.4: keeps only ids present in `survivors`; unsets primary if it was
  /// dropped. Resets the cycle ONLY when this actually removes a selected
  /// id — a no-op refresh (identical survivors) is not a candidate-set
  /// change (R7.2.3) and must not disturb an active cycle.
  void retainSurvivors(Set<String> survivors) {
    final next = _ids.where(survivors.contains).toSet();
    final removed = next.length != _ids.length;
    _ids = next;
    if (_primary != null && !next.contains(_primary)) _primary = null;
    if (removed) _endCycle();
  }

  /// R7.4: clears selection and ends the cycle (Escape while idle, active
  /// level switch, clicking empty space is handled inside [click] itself).
  void clear() {
    _ids = const {};
    _primary = null;
    _endCycle();
  }

  /// Ends the active cycle without touching selection.
  void resetCycle() => _endCycle();

  void _applyContext(SelectionContext context) {
    if (_anchorContext == null) return;
    if (context != _anchorContext) _endCycle();
  }

  void _applyPointer(Offset position) {
    if (!position.dx.isFinite || !position.dy.isFinite) return;
    final anchor = _anchorPosition;
    final anchorContext = _anchorContext;
    if (anchor == null || anchorContext == null) return;
    final radius = _radiusFor(anchorContext.pointerKind);
    if ((position - anchor).distanceSquared > radius * radius) _endCycle();
  }

  void _endCycle() {
    _cycleList = null;
    _cycleIndex = null;
    _anchorPosition = null;
    _anchorContext = null;
  }

  static double _radiusFor(PointerDeviceKind kind) =>
      kind == PointerDeviceKind.touch ? 24.0 : 12.0;

  static List<String> _dedupeFirstOccurrence(Iterable<String> ids) {
    final seen = <String>{};
    final out = <String>[];
    for (final id in ids) {
      if (seen.add(id)) out.add(id);
    }
    return out;
  }
}
