import 'dart:convert';
import 'dart:typed_data';

import 'core/domowik_core.dart';

/// Native status retained verbatim; presentation belongs to the caller.
class PlanFileResult {
  const PlanFileResult(this.code, {this.detail, this.stored = true});
  final int code;
  final String? detail;

  /// False when the bytes were handed over but storage could not be
  /// confirmed (a browser download): the plan then stays unsaved.
  final bool stored;

  /// Whether the plan is safely stored. An unconfirmed download is not.
  bool get succeeded => code == ok && stored;

  /// The bytes left the editor but their storage is unknown.
  bool get unconfirmed => code == ok && !stored;
}

/// Where a plan comes from: a filesystem path (desktop) or bytes a browser
/// picker delivered. [name] is what the editor shows as the plan's file.
sealed class PlanSource {
  const PlanSource();
  String get name;
}

class PlanPath extends PlanSource {
  const PlanPath(this.path);
  final String path;
  @override
  String get name => path;
}

class PlanBytes extends PlanSource {
  const PlanBytes(this.name, this.bytes);
  @override
  final String name;
  final Uint8List bytes;
}

/// Where a save goes: a path the core writes atomically (§9.7), or a writer
/// that stores encoded bytes for a host without a filesystem. A writer
/// completes with true once the bytes are known to be stored, with false
/// when it handed them over without confirmation (a browser download), and
/// throws if storing failed.
sealed class PlanTarget {
  const PlanTarget();
  String get name;
}

class PlanPathTarget extends PlanTarget {
  const PlanPathTarget(this.path);
  final String path;
  @override
  String get name => path;
}

class PlanByteTarget extends PlanTarget {
  const PlanByteTarget(this.name, this.write);
  @override
  final String name;
  final Future<bool> Function(Uint8List bytes) write;
}

/// Owns one plan handle on the calling isolate. The core is borrowed.
///
/// Core operations are synchronous, matching the C ABI; only a byte target's
/// write is asynchronous. A replacement opens before closing the old handle;
/// failed opens/saves preserve handle and name. [handle] is borrowed by
/// editing/rendering clients, which must not close it. Native close drains
/// admitted work; disposal is idempotent and rejects subsequent file calls.
class PlanSession {
  PlanSession(this.core);
  final DomowikCore core;
  int _handle = 0;
  String? _name;

  /// The writer of a browser save whose storage the host confirmed once.
  /// Only such a writer can be reused for another silent save; a download
  /// cannot be rewritten, and no writer exists at all before the first save.
  PlanByteTarget? _confirmedWriter;
  bool _disposed = false;
  int get handle => _handle;

  /// The plan's file: a path on desktop, a file name in the browser, or null
  /// while it has never been opened from or saved to one.
  String? get path => _name;

  /// Device-local floor memory uses the document identity when available.
  /// Legacy desktop plans use their absolute file URI. Browsers expose no
  /// path, so legacy byte files use their name plus sorted stable level IDs
  /// to distinguish unrelated files with the same name. No identity is minted
  /// or written into the plan. Unsaved legacy plans have no persistent key.
  String? levelPreferencesKey(String? documentId, Iterable<String> levelIds) {
    if (documentId != null) return documentId;
    final name = _name;
    if (name == null) return null;
    if (core.supportsPaths) {
      return 'legacy-path:${Uri.base.resolveUri(Uri.file(name)).normalizePath()}';
    }
    return 'legacy-bytes:${jsonEncode([name, levelIds.toList()..sort()])}';
  }

  /// Where a plain Save writes: the plan's own file, overwritable without
  /// asking again. On a filesystem build that is its path, including a plan
  /// opened from a file it has never been saved to; in the browser it is the
  /// writer of a save the host confirmed. Null for a new plan and for one
  /// this browser could only download (an unconfirmed write cannot be
  /// repeated silently), which makes Save ask where to write, like Save As.
  PlanTarget? get boundTarget {
    if (core.supportsPaths) {
      final path = _name;
      return path == null ? null : PlanPathTarget(path);
    }
    return _confirmedWriter;
  }

  PlanFileResult create() {
    _checkLive();
    return _replace(core.createDefault(), null);
  }

  PlanFileResult open(PlanSource source) {
    _checkLive();
    return _replace(switch (source) {
      PlanPath(:final path) => core.openPath(path),
      PlanBytes(:final bytes) => core.openBytes(bytes),
    }, source.name);
  }

  PlanFileResult _replace(CoreHandle opened, String? name) {
    _checkLive();
    if (opened.code != ok) return PlanFileResult(opened.code);
    if (_handle != 0) core.close(_handle);
    _handle = opened.handle;
    _name = name;
    // A replaced plan carries nothing over: the previous file belongs to the
    // previous plan, and a plan opened from received bytes has no file.
    _confirmedWriter = null;
    return PlanFileResult(opened.code);
  }

  /// Saves to [target]. A byte target stays dirty if the plan changed while
  /// its bytes were being stored, and reports that as a refusal; one that
  /// cannot confirm storage leaves the plan dirty and reports
  /// [PlanFileResult.unconfirmed], which does not count as saved. Only a
  /// fully confirmed byte save is retained for [boundTarget]; a failed save
  /// leaves the previous binding untouched.
  Future<PlanFileResult> save(PlanTarget target) async {
    _checkLive();
    if (_handle == 0) return const PlanFileResult(invalidArgument);
    final handle = _handle;
    switch (target) {
      case PlanPathTarget(:final path):
        final code = core.savePath(handle, path);
        if (code == ok) _name = path;
        return code == ok ? const PlanFileResult(ok) : _failure(code);
      case PlanByteTarget(:final name, :final write):
        final encoded = core.encode(handle);
        if (encoded.code != ok) return _failure(encoded.code);
        final stored = await write(encoded.bytes!);
        if (_disposed || _handle != handle) {
          return const PlanFileResult(
            rejected,
            detail: 'the plan was replaced while it was being saved',
          );
        }
        if (!stored) {
          _name = name;
          return const PlanFileResult(
            ok,
            stored: false,
            detail: 'the browser cannot confirm the download was kept',
          );
        }
        final code = core.markSaved(handle, encoded.revision);
        if (code != ok) return _failure(code);
        _name = name;
        _confirmedWriter = PlanByteTarget(name, write);
        return const PlanFileResult(ok);
    }
  }

  PlanFileResult _failure(int code) {
    final detail = core.lastError(_handle);
    return PlanFileResult(
      code,
      detail: detail == null || detail.isEmpty ? 'no detail' : detail,
    );
  }

  void _checkLive() {
    if (_disposed) throw StateError('Plan session is disposed');
  }

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    if (_handle != 0) core.close(_handle);
    _handle = 0;
    _name = null;
    _confirmedWriter = null;
  }
}
