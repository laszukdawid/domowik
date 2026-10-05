import 'dart:ffi';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';

/// Native status retained verbatim; presentation belongs to the caller.
class PlanFileResult {
  const PlanFileResult(this.code, {this.detail});
  final int code;
  final String? detail;
  bool get succeeded => code == ok;
}

/// Owns one native plan handle on the calling isolate. The bridge is borrowed.
///
/// Operations are synchronous, matching the current C ABI. Native allocations
/// are always released locally. A replacement opens before closing the old
/// handle; failed opens/saves preserve handle/path. [handle] is borrowed by
/// editing/rendering clients, which must not close it. Native close drains
/// admitted work; disposal is idempotent and rejects subsequent file calls.
class PlanSession {
  PlanSession(this.bridge);
  final Bridge bridge;
  int _handle = 0;
  String? _path;
  bool _disposed = false;
  int get handle => _handle;
  String? get path => _path;

  PlanFileResult create() => _replace(null);
  PlanFileResult open(String path) => _replace(path);

  PlanFileResult _replace(String? path) {
    _checkLive();
    final output = alloc<Uint64>(sizeOf<Uint64>());
    Pointer<Uint8>? input;
    try {
      final int code;
      if (path == null) {
        code = bridge.planCreateDefault(output);
      } else {
        input = toNative('$path\u0000');
        code = bridge.planOpen(input, output);
      }
      if (code != ok) return PlanFileResult(code);
      if (_handle != 0) bridge.planClose(_handle);
      _handle = output.value;
      _path = path;
      return PlanFileResult(code);
    } finally {
      if (input != null) release(input);
      release(output);
    }
  }

  PlanFileResult save(String path) {
    _checkLive();
    if (_handle == 0) return const PlanFileResult(invalidArgument);
    final input = toNative('$path\u0000');
    try {
      final code = bridge.planSave(_handle, input);
      if (code == ok) {
        _path = path;
        return PlanFileResult(code);
      }
      final output = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      try {
        final errorCode = bridge.lastError(_handle, output);
        final detail = errorCode == ok
            ? bridge.takeString(output)
            : 'no detail (last_error returned $errorCode)';
        return PlanFileResult(
          code,
          detail: detail.isEmpty ? 'no detail' : detail,
        );
      } finally {
        release(output);
      }
    } finally {
      release(input);
    }
  }

  void _checkLive() {
    if (_disposed) throw StateError('Plan session is disposed');
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    if (_handle != 0) bridge.planClose(_handle);
    _handle = 0;
    _path = null;
  }
}
