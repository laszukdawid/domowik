// dart:ffi bindings for the Domowik bridge spike (issue 0024).
//
// ONE binding set, imported by both the plain-Dart harness and the Flutter
// application. Two copies of an ABI declaration is exactly the drift this
// spike exists to catch: a `Uint64` that becomes a `Uint32` in one copy is a
// silent stack corruption, not a compile error, and it would be found by
// whichever of the two happened to be run.
//
// NOTHING here is Flutter-specific. `DynamicLibrary`, `Pointer` and the
// calling convention are the Dart VM's, so this library loads and works
// identically under `dart run` and inside a Flutter app.

import 'dart:convert';
import 'dart:ffi';
import 'dart:typed_data';

// NO third-party Dart package is adopted. `package:ffi` would supply `malloc`
// and `calloc`, and would also be a dependency needing a licence, provenance
// and notice record (docs/THIRD-PARTY-POLICY.md) for two functions that libc
// already exports into this process. The Go spike made the same call for the
// same reason; keeping it means the spike's dependency ledger stays empty.
final _process = DynamicLibrary.process();

/// `Dl_info` from `<dlfcn.h>`. Only the first field is read here.
final class _DlInfo extends Struct {
  external Pointer<Uint8> fileName;
  external Pointer<Void> fileBase;
  external Pointer<Uint8> symbolName;
  external Pointer<Void> symbolAddress;
}

final _dladdr = _process.lookupFunction<
    Int32 Function(Pointer<Void>, Pointer<_DlInfo>),
    int Function(Pointer<Void>, Pointer<_DlInfo>)>('dladdr');
final _malloc = _process.lookupFunction<Pointer<Uint8> Function(Uint64),
    Pointer<Uint8> Function(int)>('malloc');
final _free = _process.lookupFunction<Void Function(Pointer<Uint8>),
    void Function(Pointer<Uint8>)>('free');

/// libc-allocated scratch memory owned by the CALLER. Never passed to
/// `domowik_buffer_free`, which would refuse it anyway — the harness's
/// ownership probes assert exactly that.
Pointer<T> alloc<T extends NativeType>(int bytes) => _malloc(bytes).cast<T>();

void release(Pointer<NativeType> p) => _free(p.cast<Uint8>());

// ---------------------------------------------------------------------------
// ABI
// ---------------------------------------------------------------------------

const int ok = 0;
const int invalidArgument = 1;
const int unknownHandle = 2;
const int unknownBuffer = 3;
const int malformedRequest = 4;
const int rejected = 5;
const int panicContained = 6;
const int canceled = 7;

/// A native query failed.  [code] is one of the bridge status constants and
/// [detail] is the handle's sticky diagnostic, when the core supplied one.
class BridgeException implements Exception {
  const BridgeException(this.code, [this.detail]);

  final int code;
  final String? detail;

  @override
  String toString() =>
      'BridgeException($code${detail == null || detail!.isEmpty ? '' : ', $detail'})';
}

final class DomowikBuf extends Struct {
  external Pointer<Uint8> data;
  @Uint64()
  external int len;

  /// The release key. Pass THIS to `domowik_buffer_free`, never `data`.
  @Uint64()
  external int token;
}

typedef _AbiVersionC = Int32 Function();
typedef _AbiVersion = int Function();
typedef _CreateC = Int32 Function(Pointer<Uint64>);
typedef _Create = int Function(Pointer<Uint64>);
typedef _ExecuteC = Int32 Function(
    Uint64, Pointer<Uint8>, Uint64, Pointer<DomowikBuf>);
typedef _Execute = int Function(int, Pointer<Uint8>, int, Pointer<DomowikBuf>);
typedef _HandleBufC = Int32 Function(Uint64, Pointer<DomowikBuf>);
typedef _HandleBuf = int Function(int, Pointer<DomowikBuf>);
typedef _FreeC = Int32 Function(Uint64);
typedef _Free = int Function(int);
typedef _HandleC = Int32 Function(Uint64);
typedef _Handle = int Function(int);
typedef _NullaryC = Int32 Function();
typedef _Nullary = int Function();
typedef _BusyC = Int32 Function(Uint64, Uint64);
typedef _Busy = int Function(int, int);
typedef _CountC = Int64 Function();
typedef _Count = int Function();

class Bridge {
  /// Opens the library at `path` — what the standalone harnesses do.
  Bridge(String path) : _lib = DynamicLibrary.open(path);

  /// Binds to an already-open library.
  ///
  /// The Flutter application needs this: on macOS the Rust code is LINKED into
  /// the plugin framework rather than shipped as a loose dylib, so the caller
  /// resolves `domowik_bridge_plugin.framework/domowik_bridge_plugin` and the
  /// bindings attach to whatever that produced. Keeping one `Bridge` for both
  /// is the point — a second copy of these declarations could disagree with
  /// the ABI in a way no compiler would catch.
  Bridge.fromLibrary(this._lib);

  final DynamicLibrary _lib;

  late final abiVersion =
      _lib.lookupFunction<_AbiVersionC, _AbiVersion>('domowik_abi_version');
  late final planCreate =
      _lib.lookupFunction<_CreateC, _Create>('domowik_plan_create');
  late final planCreateDefault =
      _lib.lookupFunction<_CreateC, _Create>('domowik_plan_create_default');
  late final planExecute =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_execute');

  /// ABI 15: plan-local libraries, footprint geometry and asset edits. Uses the
  /// same borrowed JSON / token-owned response buffers as planExecute.
  late final planAssets =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_assets');
  late final planUndo =
      _lib.lookupFunction<_HandleBufC, _HandleBuf>('domowik_plan_undo');
  late final planRedo =
      _lib.lookupFunction<_HandleBufC, _HandleBuf>('domowik_plan_redo');
  late final planState =
      _lib.lookupFunction<_HandleBufC, _HandleBuf>('domowik_plan_state');
  late final planSnapshot =
      _lib.lookupFunction<_HandleBufC, _HandleBuf>('domowik_plan_snapshot');

  /// Performs one editing GESTURE, with every consequence the specification
  /// attaches to it — auto-join, induced splits, opening remap-or-reject.
  ///
  /// Building operations by hand instead gets none of those, and quietly
  /// produces a different document from the one the CLI would.
  late final planEdit =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_edit');

  /// Opens a `.domo` package and registers it as a plan.
  late final planOpen = _lib.lookupFunction<
      Int32 Function(Pointer<Uint8>, Pointer<Uint64>),
      int Function(Pointer<Uint8>, Pointer<Uint64>)>('domowik_plan_open');

  /// Writes the plan to a path, adopting it.
  late final planSave = _lib.lookupFunction<
      Int32 Function(Uint64, Pointer<Uint8>),
      int Function(int, Pointer<Uint8>)>('domowik_plan_save');

  /// ABI 16: the byte boundary the browser build needs (issue 0088). Native
  /// builds export the same symbols, so the two hosts share one tested path.
  late final planOpenBytes = _lib.lookupFunction<
      Int32 Function(Pointer<Uint8>, Uint64, Pointer<Uint64>),
      int Function(Pointer<Uint8>, int, Pointer<Uint64>)>(
    'domowik_plan_open_bytes',
  );
  late final planEncode = _lib.lookupFunction<
      Int32 Function(Uint64, Pointer<Uint64>, Pointer<DomowikBuf>),
      int Function(int, Pointer<Uint64>, Pointer<DomowikBuf>)>(
    'domowik_plan_encode',
  );
  late final planMarkSaved = _lib.lookupFunction<Int32 Function(Uint64, Uint64),
      int Function(int, int)>('domowik_plan_mark_saved');
  late final planImportModel = _lib.lookupFunction<
      Int32 Function(Uint64, Pointer<Uint8>, Uint64, Pointer<Uint8>, Uint64,
          Pointer<DomowikBuf>),
      int Function(int, Pointer<Uint8>, int, Pointer<Uint8>, int,
          Pointer<DomowikBuf>)>('domowik_plan_import_model');

  /// ABI 22: the whole plan as a `.usdz` 3D scene.
  late final planExportUsdz = _lib.lookupFunction<
      Int32 Function(Uint64, Pointer<DomowikBuf>, Pointer<DomowikBuf>),
      int Function(int, Pointer<DomowikBuf>, Pointer<DomowikBuf>)>(
    'domowik_plan_export_usdz',
  );
  late final planSceneFrame =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_scene_frame');

  /// One level's DERIVED geometry: occupied region, rooms, floor area.
  ///
  /// The snapshot returns wall centre lines. Rooms and the occupied region are
  /// COMPUTED by the core and never stored, and recomputing them in Dart would
  /// put a second geometry implementation in the view.
  late final planDerived = _lib.lookupFunction<
      Int32 Function(Uint64, Pointer<Uint8>, Pointer<DomowikBuf>),
      int Function(
          int, Pointer<Uint8>, Pointer<DomowikBuf>)>('domowik_plan_derived');

  /// Ranked hit candidates at one point on one level (ABI 9).
  ///
  /// Request: `{"levelId":"…","pointMm":[x,y],"radiusMm":120.0}`. The radius
  /// is the logical-pixel pick radius (12 mouse/stylus, 24 touch) ALREADY
  /// converted to plan millimetres through the inverse view transform — the
  /// core has no view transform and this call does not invent one. It must be
  /// finite and within `[0, 240000]`, that upper bound being 24 px at the
  /// 0.0001 px/mm zoom floor.
  ///
  /// Response: an ORDERED array, top-ranked first, of
  /// `{"class":…,"entityId":…,"closestMm":[x,y],"distanceSquaredMm2":…}`,
  /// where `class` is `join-node`, `free-endpoint` or `region`. Distances are
  /// SQUARED and never rooted; they are binary64 rather than exact reals, and
  /// issue 0012 owns that gap.
  ///
  /// No selection policy is applied here: click cycling, the selection set and
  /// the primary item are the CALLER's, and answering them behind this call
  /// would put one policy in two places.
  ///
  /// Empty is not a refusal. A finite point far outside the plan domain
  /// answers `[]` — panning a long way is ordinary — and so does an unknown
  /// level. A malformed request, a non-finite coordinate or an out-of-range
  /// radius returns `malformedRequest` and writes nothing.
  late final planHitTest =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_hit_test');
  late final planMarquee =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_marquee');

  /// Read-only snapping query. The ABI 12 symbol is resolved when this
  /// property is first used, consistently with the other bridge bindings.
  late final planSnap =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_snap');
  late final planStairPreview =
      _lib.lookupFunction<_ExecuteC, _Execute>('domowik_plan_stair_preview');
  late final lastError =
      _lib.lookupFunction<_HandleBufC, _HandleBuf>('domowik_last_error');
  late final bufferFree =
      _lib.lookupFunction<_FreeC, _Free>('domowik_buffer_free');
  late final planClose =
      _lib.lookupFunction<_HandleC, _Handle>('domowik_plan_close');
  late final debugPanic =
      _lib.lookupFunction<_NullaryC, _Nullary>('domowik_debug_panic');
  late final busy = _lib.lookupFunction<_BusyC, _Busy>('domowik_busy');
  late final cancel = _lib.lookupFunction<_HandleC, _Handle>('domowik_cancel');
  late final debugCancelEpoch =
      _lib.lookupFunction<Int64 Function(Uint64), int Function(int)>(
          'domowik_debug_cancel_epoch');

  /// How many times this handle has BUILT a hit index; `-1` if unknown.
  ///
  /// A DEBUG probe. A cache that rebuilt on every query would return
  /// byte-identical answers, so a test written against the RESULTS passes on
  /// the broken implementation. This is the only way to assert the actual
  /// contract: reused across reads and saves, dropped by an edit.
  late final debugHitIndexBuilds =
      _lib.lookupFunction<Int64 Function(Uint64), int Function(int)>(
          'domowik_debug_hit_index_builds');

  /// How many times this handle's warning cache has actually RECOMPUTED (a
  /// revision miss); `-1` if unknown.
  ///
  /// A DEBUG probe, for the same reason as [debugHitIndexBuilds]: a mutation
  /// and the `planState` read immediately following it would return
  /// byte-identical `warnings` whether or not the second call recomputed, so
  /// that is not otherwise assertable.
  late final debugWarningsComputations =
      _lib.lookupFunction<Int64 Function(Uint64), int Function(int)>(
          'domowik_debug_warnings_computations');
  late final outstandingBuffers =
      _lib.lookupFunction<_CountC, _Count>('domowik_outstanding_buffers');
  late final openPlans =
      _lib.lookupFunction<_CountC, _Count>('domowik_open_plans');

  /// Where the code these bindings call actually lives on disk.
  ///
  /// Resolved with `dladdr` on the address of a REAL exported symbol, so the
  /// answer is where the loader found the image, not where the caller hoped
  /// it would. A bundle self-test that merely observes `DynamicLibrary.open`
  /// succeeding proves nothing about packaging: an identically named library
  /// somewhere else on the machine satisfies it just as well, and the check
  /// would pass on precisely the bug it exists to catch.
  ///
  /// Returns `null` when `dladdr` cannot name the image.
  String? loadedImagePath() {
    final symbol = _lib.lookup<Void>('domowik_abi_version');
    final info = alloc<_DlInfo>(sizeOf<_DlInfo>());
    try {
      if (_dladdr(symbol, info) == 0) {
        return null;
      }
      final name = info.ref.fileName;
      if (name == nullptr) {
        return null;
      }
      final bytes = <int>[];
      for (var i = 0;; i++) {
        final b = name[i];
        if (b == 0) break;
        bytes.add(b);
      }
      return utf8.decode(bytes);
    } finally {
      release(info);
    }
  }

  /// Reads a buffer the bridge issued and releases it through the ABI.
  ///
  /// The read is LENGTH-DELIMITED. `Pointer<Utf8>.toDartString()` would stop
  /// at a NUL and read past the end looking for one — the same mistake the C
  /// harness made and AddressSanitizer caught (finding F1).
  String takeString(Pointer<DomowikBuf> buf) => utf8.decode(takeBytes(buf));

  /// Reads a buffer the bridge issued as bytes and releases it through the ABI.
  ///
  /// The buffer is released however the copy ends, so a failed copy cannot
  /// strand it.
  Uint8List takeBytes(Pointer<DomowikBuf> buf) {
    final Uint8List bytes;
    try {
      bytes = Uint8List.fromList(buf.ref.data.asTypedList(buf.ref.len));
    } finally {
      final code = bufferFree(buf.ref.token);
      buf.ref.data = nullptr;
      buf.ref.token = 0;
      if (code != ok) {
        // ignore: control_flow_in_finally
        throw StateError('buffer_free returned $code');
      }
    }
    return bytes;
  }

  /// Runs one read-only ABI 12 snap query and decodes its response object.
  /// The request map is JSON-owned by the caller; the native side borrows it
  /// only for this call.  Native response buffers are always released here.
  Map<String, Object?> snap(int handle, Map<String, Object?> request) {
    final text = jsonEncode(request);
    final bytes = utf8.encode(text);
    final ptr = toNative(text);
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = planSnap(handle, ptr, bytes.length, out);
      if (code != ok) {
        String? detail;
        final errorOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
        try {
          if (lastError(handle, errorOut) == ok) {
            detail = takeString(errorOut);
          }
        } finally {
          release(errorOut);
        }
        throw BridgeException(code, detail);
      }
      return jsonDecode(takeString(out)) as Map<String, Object?>;
    } finally {
      release(ptr);
      release(out);
    }
  }
}

/// Runs an ABI 15 asset request using plain data and explicit buffer ownership.
Object? assetRequest(Bridge bridge, int handle, Map<String, Object?> request) {
  final text = jsonEncode(request);
  final ptr = toNative(text);
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    final code = bridge.planAssets(handle, ptr, utf8.encode(text).length, out);
    if (code != ok) {
      final error = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
      try {
        throw BridgeException(
            code,
            bridge.lastError(handle, error) == ok
                ? bridge.takeString(error)
                : null);
      } finally {
        release(error);
      }
    }
    return jsonDecode(bridge.takeString(out));
  } finally {
    release(ptr);
    release(out);
  }
}

/// Copies `bytes` into native memory the CALLER owns (at least one byte, so
/// an empty input still has a valid address).
Pointer<Uint8> bytesToNative(Uint8List bytes) {
  final ptr = alloc<Uint8>(bytes.isEmpty ? 1 : bytes.length);
  ptr.asTypedList(bytes.length).setAll(0, bytes);
  return ptr;
}

/// Copies `text` into native memory the CALLER owns. The bridge borrows it
/// read-only for the call and never retains it.
Pointer<Uint8> toNative(String text) {
  final bytes = utf8.encode(text);
  final ptr = alloc<Uint8>(bytes.length);
  ptr.asTypedList(bytes.length).setAll(0, bytes);
  return ptr;
}
