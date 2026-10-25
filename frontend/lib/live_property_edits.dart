import 'package:flutter/foundation.dart';

/// Coordinates mounted live inspectors before history/file/context actions.
/// Callbacks are synchronous UI-isolate work; this owns no document or timers.
class LivePropertyEdits {
  final _editors = <Object, ({VoidCallback flush, VoidCallback cancel})>{};

  VoidCallback attach({
    required VoidCallback flush,
    required VoidCallback cancel,
  }) {
    final token = Object();
    _editors[token] = (flush: flush, cancel: cancel);
    return () => _editors.remove(token);
  }

  void flush() {
    for (final editor in _editors.values.toList()) {
      editor.flush();
    }
  }

  void cancel() {
    for (final editor in _editors.values.toList()) {
      editor.cancel();
    }
  }
}
