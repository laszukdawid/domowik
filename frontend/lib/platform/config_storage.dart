// Where user preferences persist: a JSON file on native hosts, the page's
// origin storage in a browser (issue 0088).
export 'config_storage_types.dart';
export 'config_storage_native.dart'
    if (dart.library.js_interop) 'config_storage_web.dart';
