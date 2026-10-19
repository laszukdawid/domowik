import 'package:flutter/services.dart';

import 'plan_session.dart';

enum LeaveDecision { cancel, discard, save }

/// Platform dialogs return null on cancellation; platform errors propagate.
/// Implementations own their native close callback until it is cleared.
///
/// A desktop dialog answers with a path; a browser picker with bytes to open
/// or a writer to save through (issue 0088).
abstract interface class FileDialogs {
  Future<PlanSource?> openPlan();
  Future<PlanTarget?> savePlan(String suggestedName);
  void setCloseHandler(Future<bool> Function()? handler);
}

/// Dialogs that can also guard unsaved changes on their own, for a host that
/// cannot wait for the editor's asynchronous close handler (a browser tab).
abstract interface class UnsavedChangesGuard {
  void setUnsavedChanges(bool unsaved);
}

class NativeFileDialogs implements FileDialogs {
  const NativeFileDialogs();
  static const _channel = MethodChannel('invalid.domowik-spike/files');

  @override
  Future<PlanSource?> openPlan() async {
    final path = await _channel.invokeMethod<String>('openPlan');
    return path == null ? null : PlanPath(path);
  }

  @override
  Future<PlanTarget?> savePlan(String suggestedName) async {
    final path = await _channel.invokeMethod<String>('savePlan', suggestedName);
    return path == null ? null : PlanPathTarget(path);
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) {
    _channel.setMethodCallHandler(
      handler == null
          ? null
          : (call) async {
              if (call.method == 'requestClose') return await handler();
              throw MissingPluginException(
                'Unknown editor method: ${call.method}',
              );
            },
    );
  }
}

/// Serializes file/close workflows without owning widgets or document handles.
///
/// Dependencies are borrowed callbacks on the UI isolate. Callers supply the
/// approval dialog and core actions. Exceptions are reported once; the busy
/// gate always releases. Disposing suppresses later callbacks and actions,
/// but cannot cancel an already-entered native save/open call.
class FileWorkflowController {
  FileWorkflowController({
    required this.isDirty,
    required this.askToLeave,
    required this.save,
    required this.beforeAction,
    required this.onBusyChanged,
    required this.onError,
  });

  final bool Function() isDirty;
  final Future<LeaveDecision> Function() askToLeave;
  final Future<bool> Function() save;
  final void Function() beforeAction;
  final void Function(bool) onBusyChanged;
  final void Function(Object) onError;
  bool _busy = false, _disposed = false;
  bool get busy => _busy;

  Future<bool> run(Future<bool> Function() action) async {
    if (_busy || _disposed) return false;
    _busy = true;
    try {
      beforeAction();
      onBusyChanged(true);
      final result = await action();
      return !_disposed && result;
    } on Object catch (error) {
      if (!_disposed) onError(error);
      return false;
    } finally {
      _busy = false;
      if (!_disposed) onBusyChanged(false);
    }
  }

  Future<bool> confirmLeaving() async {
    if (_disposed) return false;
    if (!isDirty()) return true;
    final decision = await askToLeave();
    if (_disposed) return false;
    return switch (decision) {
      LeaveDecision.discard => true,
      LeaveDecision.save => await save(),
      LeaveDecision.cancel => false,
    };
  }

  Future<bool> replace(Future<bool> Function() action) => run(() async {
    if (!await confirmLeaving() || _disposed) return false;
    return await action();
  });

  void dispose() => _disposed = true;
}
