// The plan file dialogs this host offers: native pickers through the platform
// channel, or browser pickers and downloads (issue 0088).
export 'file_dialogs_native.dart'
    if (dart.library.js_interop) 'file_dialogs_web.dart';
