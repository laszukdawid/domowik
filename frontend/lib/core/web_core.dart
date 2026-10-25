// [DomowikCore] over a WebAssembly instance: the browser build (issue 0088,
// ADR 0012).
//
// The module is the same Rust bridge the native builds link, compiled for
// `wasm32-wasip1`, exporting the same C ABI. This file is its host:
//
// - It supplies the few WASI imports the Rust standard library uses — the
//   clock, random bytes, stderr — from browser APIs, and refuses every
//   filesystem call, so no code path can reach a file.
// - It allocates request memory inside the module (`domowik_web_alloc`),
//   copies requests in and responses out, and releases every core buffer by
//   its token before returning, exactly as the FFI host does.
// - Everything runs on the page's one thread. Calls are synchronous; there is
//   nothing to cancel, and "background" asset work runs inline.
//
// A Rust panic cannot unwind on this target: it prints its message to stderr
// and traps. The instance is then unusable, so the core records the message
// as [failure] and refuses every later call. Native builds contain panics;
// the browser build does not, and says so.

import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'domowik_core.dart';

/// Fetches and instantiates the core shipped beside the page.
Future<DomowikCore> loadPlatformCore() async =>
    WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));

/// The bytes at [url], resolved against the page; throws if not found.
Future<Uint8List> fetchBytes(String url) async {
  final response = await _fetch(url.toJS).toDart;
  if (!response.ok) {
    throw StateError('Could not fetch $url: HTTP ${response.status}');
  }
  return (await response.arrayBuffer().toDart).toDart.asUint8List();
}

@JS('fetch')
external JSPromise<_Response> _fetch(JSString url);

extension type _Response(JSObject _) implements JSObject {
  external bool get ok;
  external int get status;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

@JS('WebAssembly.instantiate')
external JSPromise<_Instantiated> _instantiate(
  JSUint8Array bytes,
  JSObject imports,
);

extension type _Instantiated(JSObject _) implements JSObject {
  external JSObject get instance;
}

extension type _Memory(JSObject _) implements JSObject {
  external JSArrayBuffer get buffer;
}

@JS('BigInt')
external JSBigInt _bigInt(JSNumber value);

@JS('Number')
external JSNumber _number(JSBigInt value);

@JS('crypto.getRandomValues')
external void _getRandomValues(JSUint8Array view);

@JS('Date.now')
external double _dateNow();

@JS('performance.now')
external double _performanceNow();

@JS('console.error')
external void _consoleError(JSString text);

// WASI errno values (wasi_snapshot_preview1).
const int _success = 0;
const int _badf = 8;
const int _nosys = 52;

/// Thrown by `proc_exit`, which the bridge never calls on purpose.
class _ProcExit {
  const _ProcExit(this.code);
  final int code;
}

class WebCore implements DomowikCore {
  WebCore._();

  /// Compiles and instantiates [wasm], a `wasm32-wasip1` build of the bridge.
  static Future<WebCore> instantiate(Uint8List wasm) async {
    final core = WebCore._();
    final result = await _instantiate(wasm.toJS, core._imports()).toDart;
    core._exports = result.instance.getProperty<JSObject>('exports'.toJS);
    core._memory = core._exports.getProperty<_Memory>('memory'.toJS);
    return core;
  }

  late final JSObject _exports;
  late final _Memory _memory;
  final StringBuffer _stderr = StringBuffer();
  String? _failure;

  @override
  String? get failure => _failure;

  // ---------------------------------------------------------------------------
  // Memory
  // ---------------------------------------------------------------------------

  /// A fresh view: growing memory detaches the previous buffer.
  ByteData get _view => _memory.buffer.toDart.asByteData();
  Uint8List get _bytes => _memory.buffer.toDart.asUint8List();

  int _u64(int address) =>
      _view.getUint32(address, Endian.little) +
      _view.getUint32(address + 4, Endian.little) * 0x100000000;

  void _setU64(int address, int value) {
    _view.setUint32(address, value % 0x100000000, Endian.little);
    _view.setUint32(address + 4, value ~/ 0x100000000, Endian.little);
  }

  JSBigInt _big(int value) => _bigInt(value.toJS);
  int _fromBig(JSAny? value) => _number(value as JSBigInt).toDartInt;

  int _alloc(int length) {
    final ptr = (_exports.callMethod<JSNumber>(
      'domowik_web_alloc'.toJS,
      length.toJS,
    )).toDartInt;
    if (ptr == 0) throw StateError('the core is out of memory');
    return ptr;
  }

  void _free(int ptr, int length) {
    _exports.callMethod<JSAny?>('domowik_web_free'.toJS, ptr.toJS, length.toJS);
  }

  /// Copies [data] into a fresh block; the caller frees it with its length.
  int _copyIn(Uint8List data) {
    final ptr = _alloc(data.length);
    _bytes.setRange(ptr, ptr + data.length, data);
    return ptr;
  }

  // ---------------------------------------------------------------------------
  // Calls
  // ---------------------------------------------------------------------------

  /// Calls an export, turning a trap into a recorded [failure].
  T _call<T extends JSAny?>(String name, List<JSAny?> args) {
    final failed = _failure;
    if (failed != null) throw _Lost(failed);
    try {
      return _exports.callMethodVarArgs<T>(name.toJS, args);
    } catch (error) {
      if (error is _Lost) rethrow;
      final panic = _stderr.toString().trim();
      _failure = panic.isEmpty ? 'the core stopped: $error' : panic;
      throw _Lost(_failure!);
    }
  }

  int _status(String name, List<JSAny?> args) {
    try {
      return _call<JSNumber>(name, args).toDartInt;
    } on _Lost {
      return panicContained;
    }
  }

  /// A call whose last argument is a `domowik_buf*`; reads and frees it.
  CoreReply _withBuf(String name, List<JSAny?> args, {bool binary = false}) {
    if (_failure != null) return const CoreReply.failed(panicContained);
    const size = 24; // data (u32, padded), len (u64), token (u64)
    final buf = _alloc(size);
    try {
      _bytes.fillRange(buf, buf + size, 0);
      final code = _status(name, [...args, buf.toJS]);
      if (code != ok) return CoreReply.failed(code);
      final data = _view.getUint32(buf, Endian.little);
      final length = _u64(buf + 8);
      final token = _u64(buf + 16);
      // The response is released however copying or decoding it ends.
      final Uint8List body;
      try {
        body = _bytes.sublist(data, data + length);
      } finally {
        _status('domowik_buffer_free', [_big(token)]);
      }
      return binary
          ? CoreReply(code, bytes: body)
          : CoreReply(code, text: utf8.decode(body));
    } finally {
      if (_failure == null) _free(buf, size);
    }
  }

  /// A call taking `(handle, req, req_len, out)`.
  CoreReply _request(
    String name,
    int handle,
    String request, {
    bool binary = false,
  }) {
    if (_failure != null) return const CoreReply.failed(panicContained);
    final data = utf8.encode(request);
    final ptr = _copyIn(data);
    try {
      return _withBuf(name, [
        _big(handle),
        ptr.toJS,
        _big(data.length),
      ], binary: binary);
    } finally {
      if (_failure == null) _free(ptr, data.length);
    }
  }

  CoreHandle _register(String name, List<JSAny?> args) {
    if (_failure != null) return const CoreHandle(panicContained, 0);
    final out = _alloc(8);
    try {
      _setU64(out, 0);
      final code = _status(name, [...args, out.toJS]);
      return CoreHandle(code, code == ok ? _u64(out) : 0);
    } finally {
      if (_failure == null) _free(out, 8);
    }
  }

  // ---------------------------------------------------------------------------
  // DomowikCore
  // ---------------------------------------------------------------------------

  @override
  int abiVersion() => _status('domowik_abi_version', const []);

  @override
  String? loadedImagePath() => null;

  @override
  bool get supportsPaths => false;

  @override
  CoreHandle createDefault() => _register('domowik_plan_create_default', []);

  @override
  CoreHandle openPath(String path) => const CoreHandle(invalidArgument, 0);

  @override
  CoreHandle openBytes(Uint8List bytes) {
    if (_failure != null) return const CoreHandle(panicContained, 0);
    final ptr = _copyIn(bytes);
    try {
      return _register('domowik_plan_open_bytes', [
        ptr.toJS,
        _big(bytes.length),
      ]);
    } finally {
      if (_failure == null) _free(ptr, bytes.length);
    }
  }

  @override
  int savePath(int handle, String path) => invalidArgument;

  @override
  CoreEncoded encode(int handle) {
    if (_failure != null) return CoreEncoded(panicContained, null, 0);
    final revision = _alloc(8);
    try {
      final reply = _withBuf('domowik_plan_encode', [
        _big(handle),
        revision.toJS,
      ], binary: true);
      return reply.succeeded
          ? CoreEncoded(ok, reply.bytes, _u64(revision))
          : CoreEncoded(reply.code, null, 0);
    } finally {
      if (_failure == null) _free(revision, 8);
    }
  }

  @override
  int markSaved(int handle, int revision) =>
      _status('domowik_plan_mark_saved', [_big(handle), _big(revision)]);

  @override
  int close(int handle) => _status('domowik_plan_close', [_big(handle)]);

  @override
  CoreReply execute(int handle, String request) =>
      _request('domowik_plan_execute', handle, request);

  @override
  CoreReply edit(int handle, String request) =>
      _request('domowik_plan_edit', handle, request);

  @override
  CoreReply assets(int handle, String request) =>
      _request('domowik_plan_assets', handle, request);

  @override
  CoreReply hitTest(int handle, String request) =>
      _request('domowik_plan_hit_test', handle, request);

  @override
  CoreReply marquee(int handle, String request) =>
      _request('domowik_plan_marquee', handle, request);

  @override
  CoreReply snap(int handle, String request) =>
      _request('domowik_plan_snap', handle, request);

  @override
  CoreReply stairPreview(int handle, String request) =>
      _request('domowik_plan_stair_preview', handle, request);

  @override
  CoreReply sceneFrame(int handle, String request) =>
      _request('domowik_plan_scene_frame', handle, request, binary: true);

  @override
  CoreReply undo(int handle) => _withBuf('domowik_plan_undo', [_big(handle)]);

  @override
  CoreReply redo(int handle) => _withBuf('domowik_plan_redo', [_big(handle)]);

  @override
  CoreReply state(int handle) => _withBuf('domowik_plan_state', [_big(handle)]);

  @override
  CoreReply snapshot(int handle) =>
      _withBuf('domowik_plan_snapshot', [_big(handle)]);

  @override
  CoreReply derived(int handle, String levelId) {
    if (_failure != null) return const CoreReply.failed(panicContained);
    final id = Uint8List.fromList([...utf8.encode(levelId), 0]);
    final ptr = _copyIn(id);
    try {
      return _withBuf('domowik_plan_derived', [_big(handle), ptr.toJS]);
    } finally {
      if (_failure == null) _free(ptr, id.length);
    }
  }

  @override
  CoreReply importModel(int handle, String name, Uint8List bytes) {
    if (_failure != null) return const CoreReply.failed(panicContained);
    final nameBytes = utf8.encode(name);
    // Each block enters its own clean-up scope as soon as it exists, so a
    // failed second allocation still releases the first.
    final namePtr = _copyIn(nameBytes);
    try {
      final dataPtr = _copyIn(bytes);
      try {
        return _withBuf('domowik_plan_import_model', [
          _big(handle),
          namePtr.toJS,
          _big(nameBytes.length),
          dataPtr.toJS,
          _big(bytes.length),
        ]);
      } finally {
        if (_failure == null) _free(dataPtr, bytes.length);
      }
    } finally {
      if (_failure == null) _free(namePtr, nameBytes.length);
    }
  }

  @override
  Future<Object?> assetsInBackground(
    int handle,
    Map<String, Object?> request,
  ) async {
    // One thread: yield to the event loop first so the UI can paint the
    // "working" state, then run inline.
    await Future<void>.delayed(Duration.zero);
    final reply = assets(handle, jsonEncode(request));
    if (!reply.succeeded) throw CoreException(reply.code, lastError(handle));
    return jsonDecode(reply.text!);
  }

  @override
  String? lastError(int handle) {
    if (_failure != null) return _failure;
    final reply = _withBuf('domowik_last_error', [_big(handle)]);
    return reply.succeeded ? reply.text : null;
  }

  @override
  int outstandingBuffers() => _counter('domowik_outstanding_buffers');

  @override
  int openPlans() => _counter('domowik_open_plans');

  int _counter(String name) {
    try {
      return _fromBig(_call<JSAny?>(name, const []));
    } on _Lost {
      return -1;
    }
  }

  @override
  int debugPanic() => _status('domowik_debug_panic', const []);

  // ---------------------------------------------------------------------------
  // WASI
  // ---------------------------------------------------------------------------

  JSObject _imports() {
    final wasi = JSObject();
    void define(String name, JSFunction function) =>
        wasi.setProperty(name.toJS, function);
    int unsupported() => _nosys;

    define(
      'random_get',
      ((int ptr, int length) {
        // getRandomValues fills at most 65,536 bytes per call.
        for (var at = 0; at < length; at += 65536) {
          final n = length - at < 65536 ? length - at : 65536;
          _getRandomValues(_memory.buffer.toDart.asUint8List(ptr + at, n).toJS);
        }
        return _success;
      }).toJS,
    );
    define(
      'clock_time_get',
      ((int id, JSBigInt precision, int out) {
        // 0 is the realtime clock; the others are monotonic for our purposes.
        final ms = id == 0 ? _dateNow() : _performanceNow();
        final ns = (ms * 1e6).floorToDouble();
        final high = (ns / 4294967296).floorToDouble();
        _view.setUint32(out, (ns - high * 4294967296).toInt(), Endian.little);
        _view.setUint32(out + 4, high.toInt(), Endian.little);
        return _success;
      }).toJS,
    );
    define(
      'environ_sizes_get',
      ((int count, int size) {
        _view.setUint32(count, 0, Endian.little);
        _view.setUint32(size, 0, Endian.little);
        return _success;
      }).toJS,
    );
    define('environ_get', ((int environ, int buffer) => _success).toJS);
    define(
      'fd_write',
      ((int fd, int iovs, int count, int written) {
        var total = 0;
        final text = StringBuffer();
        for (var i = 0; i < count; i++) {
          final ptr = _view.getUint32(iovs + 8 * i, Endian.little);
          final length = _view.getUint32(iovs + 8 * i + 4, Endian.little);
          text.write(
            utf8.decode(
              _bytes.sublist(ptr, ptr + length),
              allowMalformed: true,
            ),
          );
          total += length;
        }
        if (fd == 2) {
          _stderr.write(text);
          _consoleError(text.toString().toJS);
        } else if (fd != 1) {
          return _badf;
        }
        _view.setUint32(written, total, Endian.little);
        return _success;
      }).toJS,
    );
    define('fd_prestat_get', ((int fd, int out) => _badf).toJS);
    define('sched_yield', (() => _success).toJS);
    int procExit(int code) => throw _ProcExit(code);
    define('proc_exit', procExit.toJS);
    for (final name in const [
      'fd_close',
      'fd_fdstat_get',
      'fd_filestat_get',
      'fd_prestat_dir_name',
      'fd_read',
      'fd_seek',
      'fd_sync',
      'path_filestat_get',
      'path_open',
      'path_readlink',
      'path_rename',
      'path_unlink_file',
    ]) {
      define(name, unsupported.toJS);
    }
    final imports = JSObject();
    imports.setProperty('wasi_snapshot_preview1'.toJS, wasi);
    return imports;
  }
}

/// The instance trapped earlier; carried internally to fail the call.
class _Lost implements Exception {
  const _Lost(this.reason);
  final String reason;
}
