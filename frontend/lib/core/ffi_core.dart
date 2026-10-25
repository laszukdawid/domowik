// [DomowikCore] over `dart:ffi`: the native desktop and Android builds.
//
// Every request is copied into libc memory this file owns, every response
// buffer is released through `domowik_buffer_free` before a method returns,
// and every scratch allocation is freed in a `finally`. Callers therefore never
// see a pointer or a token.

import 'dart:convert';
import 'dart:ffi';
import 'dart:isolate';
import 'dart:typed_data';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart' as native;

import '../scene_packet_data.dart';
import 'domowik_core.dart';

/// Opens the core packaged with this application.
Future<DomowikCore> loadPlatformCore() async => FfiCore(native.openBridge());

class FfiCore implements DomowikCore {
  FfiCore(this.bridge);

  /// The raw bindings, for native-only probes and tests.
  final native.Bridge bridge;

  @override
  int abiVersion() => bridge.abiVersion();

  @override
  String? loadedImagePath() => bridge.loadedImagePath();

  @override
  bool get supportsPaths => true;

  @override
  String? get failure => null;

  CoreHandle _register(int Function(Pointer<Uint64>) call) {
    final out = native.alloc<Uint64>(sizeOf<Uint64>());
    try {
      out.value = 0;
      final code = call(out);
      return CoreHandle(code, code == ok ? out.value : 0);
    } finally {
      native.release(out);
    }
  }

  @override
  CoreHandle createDefault() => _register(bridge.planCreateDefault);

  @override
  CoreHandle openPath(String path) {
    final input = native.toNative('$path\u0000');
    try {
      return _register((out) => bridge.planOpen(input, out));
    } finally {
      native.release(input);
    }
  }

  @override
  CoreHandle openBytes(Uint8List bytes) {
    final input = native.bytesToNative(bytes);
    try {
      return _register((out) => bridge.planOpenBytes(input, bytes.length, out));
    } finally {
      native.release(input);
    }
  }

  @override
  int savePath(int handle, String path) {
    final input = native.toNative('$path\u0000');
    try {
      return bridge.planSave(handle, input);
    } finally {
      native.release(input);
    }
  }

  @override
  CoreEncoded encode(int handle) {
    final revision = native.alloc<Uint64>(sizeOf<Uint64>());
    final out = native.alloc<native.DomowikBuf>(sizeOf<native.DomowikBuf>());
    try {
      final code = bridge.planEncode(handle, revision, out);
      if (code != ok) return CoreEncoded(code, null, 0);
      return CoreEncoded(code, bridge.takeBytes(out), revision.value);
    } finally {
      native.release(revision);
      native.release(out);
    }
  }

  @override
  int markSaved(int handle, int revision) =>
      bridge.planMarkSaved(handle, revision);

  @override
  int close(int handle) => bridge.planClose(handle);

  /// One call with a borrowed request and a token-owned response.
  CoreReply _request(
    int Function(int, Pointer<Uint8>, int, Pointer<native.DomowikBuf>) call,
    int handle,
    String request, {
    bool binary = false,
  }) {
    final bytes = utf8.encode(request);
    final input = native.bytesToNative(bytes);
    try {
      return _reply((out) => call(handle, input, bytes.length, out), binary);
    } finally {
      native.release(input);
    }
  }

  CoreReply _reply(int Function(Pointer<native.DomowikBuf>) call, bool binary) {
    final out = native.alloc<native.DomowikBuf>(sizeOf<native.DomowikBuf>());
    try {
      final code = call(out);
      if (code != ok) return CoreReply.failed(code);
      return binary
          ? CoreReply(code, bytes: bridge.takeBytes(out))
          : CoreReply(code, text: bridge.takeString(out));
    } finally {
      native.release(out);
    }
  }

  @override
  CoreReply execute(int handle, String request) =>
      _request(bridge.planExecute, handle, request);

  @override
  CoreReply edit(int handle, String request) =>
      _request(bridge.planEdit, handle, request);

  @override
  CoreReply assets(int handle, String request) =>
      _request(bridge.planAssets, handle, request);

  @override
  CoreReply hitTest(int handle, String request) =>
      _request(bridge.planHitTest, handle, request);

  @override
  CoreReply marquee(int handle, String request) =>
      _request(bridge.planMarquee, handle, request);

  @override
  CoreReply snap(int handle, String request) =>
      _request(bridge.planSnap, handle, request);

  @override
  CoreReply stairPreview(int handle, String request) =>
      _request(bridge.planStairPreview, handle, request);

  @override
  CoreReply sceneFrame(int handle, String request) =>
      _request(bridge.planSceneFrame, handle, request, binary: true);

  @override
  CoreReply undo(int handle) =>
      _reply((out) => bridge.planUndo(handle, out), false);

  @override
  CoreReply redo(int handle) =>
      _reply((out) => bridge.planRedo(handle, out), false);

  @override
  CoreReply state(int handle) =>
      _reply((out) => bridge.planState(handle, out), false);

  @override
  CoreReply snapshot(int handle) =>
      _reply((out) => bridge.planSnapshot(handle, out), false);

  @override
  CoreReply derived(int handle, String levelId) {
    final id = native.toNative('$levelId\u0000');
    try {
      return _reply((out) => bridge.planDerived(handle, id, out), false);
    } finally {
      native.release(id);
    }
  }

  @override
  CoreReply importModel(int handle, String name, Uint8List bytes) {
    final nameBytes = utf8.encode(name);
    final namePtr = native.bytesToNative(nameBytes);
    final input = native.bytesToNative(bytes);
    try {
      return _reply(
        (out) => bridge.planImportModel(
          handle,
          namePtr,
          nameBytes.length,
          input,
          bytes.length,
          out,
        ),
        false,
      );
    } finally {
      native.release(namePtr);
      native.release(input);
    }
  }

  @override
  Future<Object?> assetsInBackground(int handle, Map<String, Object?> request) {
    // The isolate binds its own copy of the bindings to the SAME loaded image,
    // so it reaches this process's plan registry rather than a second core.
    final image = bridge.loadedImagePath();
    if (image == null) {
      return Future.value(_assetsNow(handle, request));
    }
    return Isolate.run(() {
      try {
        final result = native.assetRequest(
          native.Bridge(image),
          handle,
          request,
        );
        // Return compact arrays from this existing worker, rather than copying
        // decoded scene-number lists back to convert on the UI isolate.
        return request['kind'] == 'scene'
            ? prepareSceneArrays(result as Map<String, Object?>)
            : result;
      } on native.BridgeException catch (e) {
        throw CoreException(e.code, e.detail);
      }
    });
  }

  Object? _assetsNow(int handle, Map<String, Object?> request) {
    final reply = assets(handle, jsonEncode(request));
    if (!reply.succeeded) throw CoreException(reply.code, lastError(handle));
    return jsonDecode(reply.text!);
  }

  @override
  String? lastError(int handle) {
    final reply = _reply((out) => bridge.lastError(handle, out), false);
    return reply.succeeded ? reply.text : null;
  }

  @override
  int outstandingBuffers() => bridge.outstandingBuffers();

  @override
  int openPlans() => bridge.openPlans();

  @override
  int debugPanic() => bridge.debugPanic();
}
