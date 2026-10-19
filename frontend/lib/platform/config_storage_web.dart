import 'dart:convert';
import 'dart:js_interop';

import 'config_storage_types.dart';

/// `window.localStorage`, bound by hand: the editor adopts no third-party
/// Dart package (docs/THIRD-PARTY-POLICY.md), and two methods do not need one.
extension type _Storage(JSObject _) implements JSObject {
  external String? getItem(String key);
  external void setItem(String key, String value);
}

@JS('localStorage')
external _Storage? get _localStorage;

/// Preferences in the page origin's local storage, under one key.
///
/// Storage can be absent or refuse writes (private browsing, quota, a
/// blocked origin); a write failure surfaces to the caller like a failed
/// file write, and an absent store reads as no preferences.
class LocalConfigStorage implements ConfigStorage {
  const LocalConfigStorage([this.key = 'domowik.config']);
  final String key;

  @override
  Future<List<int>?> read(int limit) async {
    final text = _localStorage?.getItem(key);
    if (text == null) return null;
    final bytes = utf8.encode(text);
    return bytes.length > limit ? bytes.sublist(0, limit + 1) : bytes;
  }

  @override
  Future<void> write(String text) async {
    final storage = _localStorage;
    if (storage == null) {
      throw StateError('This browser offers no local storage for preferences');
    }
    storage.setItem(key, text);
  }
}

/// The browser's per-origin preference store.
ConfigStorage standardConfigStorage() => const LocalConfigStorage();
