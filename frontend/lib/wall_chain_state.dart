import 'dart:ui';

/// Anchor and previous-direction bookkeeping for the wall chain tool.
///
/// R3.1.1/R6.5.10: chaining continues only from the last ACCEPTED endpoint,
/// and `previousDirection` is that accepted segment's raw vector. A rejected
/// segment must never move [anchor] or [previousDirection] — callers only
/// call [accept] after the native gesture actually returns success.
///
/// [previousSegment] keeps the accepted segment's two STORED endpoints. The
/// native snap query prefers them to [previousDirection]: the relative angle
/// family is then exact over stored coordinates, not over a rounded Dart
/// subtraction (ED-0112).
class WallChainState {
  Offset? anchor;
  Offset? previousDirection;
  (Offset, Offset)? previousSegment;

  /// Fixes a new start anchor with zero document operations.
  void start(Offset at) {
    anchor = at;
    previousDirection = null;
    previousSegment = null;
  }

  /// Records an accepted segment's endpoint as the next anchor.
  void accept(Offset acceptedEnd) {
    final from = anchor;
    if (from != null) {
      previousDirection = acceptedEnd - from;
      previousSegment = (from, acceptedEnd);
    }
    anchor = acceptedEnd;
  }

  /// Ends the chain session: finish events and document-lifecycle changes
  /// (undo/redo/open/new/level switch, tool switch) terminate stale state.
  void clear() {
    anchor = null;
    previousDirection = null;
    previousSegment = null;
  }
}
