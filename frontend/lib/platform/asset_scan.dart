// Model-library directory enumeration: the filesystem on native hosts, none
// in the browser, where models are uploaded into the plan (issue 0088).
export 'asset_scan_native.dart' if (dart.library.js_interop) 'asset_scan_web.dart';
