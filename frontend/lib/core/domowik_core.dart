// The editor's view of the Rust core, independent of how it is reached.
//
// Native builds reach the core through `dart:ffi` (ffi_core.dart) and the
// browser through a WebAssembly instance (web_core.dart). Both implement this
// interface over the SAME exported C ABI, so the document, editing and file
// semantics live once, in Rust (issue 0088, ADR 0012). Nothing here imports
// `dart:ffi`, `dart:io` or `dart:isolate`, so the browser build can compile it.
//
// Requests and responses are strings and bytes: pointers, buffer tokens and
// out-parameters stay inside the implementations, which release every core
// buffer before returning.

import 'dart:typed_data';

// Bridge status codes, mirroring `status` in frontend/native/src/lib.rs.
const int ok = 0;
const int invalidArgument = 1;
const int unknownHandle = 2;
const int unknownBuffer = 3;
const int malformedRequest = 4;
const int rejected = 5;
const int panicContained = 6;
const int canceled = 7;

/// The status and, on success, the body of one core call.
///
/// [text] is the UTF-8 response of a call that answers in JSON; [bytes] the
/// response of one that answers in binary. Both are null on failure, when the
/// handle's sticky diagnostic is available through [DomowikCore.lastError].
class CoreReply {
  const CoreReply(this.code, {this.text, this.bytes});
  const CoreReply.failed(this.code) : text = null, bytes = null;

  final int code;
  final String? text;
  final Uint8List? bytes;

  bool get succeeded => code == ok;
}

/// A status and the handle a create/open call registered (0 on failure).
class CoreHandle {
  const CoreHandle(this.code, this.handle);
  final int code;
  final int handle;
}

/// The package bytes of an encode and the save revision they describe.
class CoreEncoded {
  const CoreEncoded(this.code, this.bytes, this.revision);
  final int code;
  final Uint8List? bytes;
  final int revision;
}

/// One loaded core. Calls are synchronous, as the C ABI is, and must come
/// from the isolate that created the core. Handles are opaque and belong to
/// this core; they are not portable between cores.
abstract interface class DomowikCore {
  /// The ABI revision of the loaded core.
  int abiVersion();

  /// Where the native image was loaded from, or null when that has no
  /// meaning (the browser) or cannot be determined.
  String? loadedImagePath();

  /// Whether plans can be opened and saved by filesystem path.
  bool get supportsPaths;

  /// Why the core stopped working, or null while it works. Native builds
  /// contain a Rust panic and never set this; the browser build cannot
  /// contain one, so the first panic ends the core and every later call
  /// fails with [panicContained].
  String? get failure;

  CoreHandle createDefault();
  CoreHandle openPath(String path);
  CoreHandle openBytes(Uint8List bytes);
  int savePath(int handle, String path);
  CoreEncoded encode(int handle);
  int markSaved(int handle, int revision);
  int close(int handle);

  CoreReply execute(int handle, String request);
  CoreReply edit(int handle, String request);
  CoreReply assets(int handle, String request);
  CoreReply hitTest(int handle, String request);
  CoreReply marquee(int handle, String request);
  CoreReply snap(int handle, String request);
  CoreReply undo(int handle);
  CoreReply redo(int handle);
  CoreReply state(int handle);
  CoreReply snapshot(int handle);
  CoreReply derived(int handle, String levelId);

  /// Embeds model bytes. A bare filename records an upload description; a
  /// relative or absolute path is also retained in asset source provenance.
  CoreReply importModel(int handle, String name, Uint8List bytes);
  CoreReply sceneFrame(int handle, String request);

  /// Runs an asset request away from the UI where the platform can (a native
  /// isolate) and inline where it cannot (the browser's single thread).
  /// Completes with the decoded JSON, or throws [CoreException].
  Future<Object?> assetsInBackground(int handle, Map<String, Object?> request);

  /// The handle's sticky diagnostic, or null when the core has none.
  String? lastError(int handle);

  int outstandingBuffers();
  int openPlans();
  int debugPanic();
}

/// A failed core call with its status and the core's diagnostic, if any.
class CoreException implements Exception {
  const CoreException(this.code, [this.detail]);

  final int code;
  final String? detail;

  @override
  String toString() =>
      'CoreException($code${detail == null || detail!.isEmpty ? '' : ', $detail'})';
}
