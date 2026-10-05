import 'dart:ui';

/// Anchor and previous-direction bookkeeping for the wall chain tool.
///
/// R3.1.1/R6.5.10: chaining continues only from the last ACCEPTED endpoint,
/// and `previousDirection` is that accepted segment's raw vector. A rejected
/// segment must never move [anchor] or [previousDirection] — callers only
/// call [accept] after the native gesture actually returns success.
class WallChainState {
  Offset? anchor;
  Offset? previousDirection;

  /// Fixes a new start anchor with zero document operations.
  void start(Offset at) {
    anchor = at;
    previousDirection = null;
  }

  /// Records an accepted segment's endpoint as the next anchor.
  void accept(Offset acceptedEnd) {
    final from = anchor;
    if (from != null) previousDirection = acceptedEnd - from;
    anchor = acceptedEnd;
  }

  /// Ends the chain session: finish events and document-lifecycle changes
  /// (undo/redo/open/new/level switch, tool switch) terminate stale state.
  void clear() {
    anchor = null;
    previousDirection = null;
  }
}
