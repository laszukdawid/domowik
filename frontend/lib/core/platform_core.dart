// The core this build ships: the native library through `dart:ffi`, or the
// WebAssembly module in a browser (issue 0088).
export 'ffi_core.dart' if (dart.library.js_interop) 'web_core.dart'
    show loadPlatformCore;
