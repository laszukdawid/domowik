// Command-line probe entry points. Native builds can be launched as a
// self-test, a first-frame probe or an Android lifecycle/benchmark probe; the
// browser has no command line, so its variant runs none (issue 0088).
export 'entry_native.dart' if (dart.library.js_interop) 'entry_web.dart';
