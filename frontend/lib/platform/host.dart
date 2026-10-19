// Host services the editor needs outside Flutter: environment probes, process
// exit, logging and filesystem directories. Native builds use `dart:io`; the
// browser build gets inert or console-backed equivalents, so no `dart:io`
// import reaches it (issue 0088).
export 'host_native.dart' if (dart.library.js_interop) 'host_web.dart';
