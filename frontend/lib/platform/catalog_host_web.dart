import 'dart:async';
import 'dart:js_interop';
import 'dart:typed_data';

import '../catalog.dart';

/// Deployment HTML overrides the build default without an extra request. An
/// empty setting disables discovery. Invalid host configuration fails closed.
CatalogClient? createCatalogClient({String? manifestUrl}) {
  final configured =
      manifestUrl ??
      _document
          .querySelector('meta[name="domowik-catalog-url"]')
          ?.getAttribute('content') ??
      const String.fromEnvironment(
        'DOMOWIK_CATALOG_URL',
        defaultValue: defaultCatalogManifestUrl,
      );
  if (configured.trim().isEmpty) return null;
  try {
    final endpoint = CatalogEndpoint(Uri.base.resolve(configured).toString());
    return CatalogClient(
      _BrowserCatalogTransport(endpoint),
      endpoint: endpoint,
    );
  } on FormatException {
    return null;
  }
}

@JS('document')
external _Document get _document;

extension type _Document(JSObject _) implements JSObject {
  external _Element? querySelector(String selector);
}

extension type _Element(JSObject _) implements JSObject {
  external String? getAttribute(String name);
}

@JS('fetch')
external JSPromise<_Response> _fetch(String url, _Options options);

extension type _Options._(JSObject _) implements JSObject {
  external factory _Options({
    String credentials,
    String redirect,
    String mode,
    JSObject signal,
  });
}

@JS('AbortController')
extension type _AbortController._(JSObject _) implements JSObject {
  external factory _AbortController();
  external JSObject get signal;
  external void abort();
}

extension type _Response(JSObject _) implements JSObject {
  external bool get ok;
  external bool get redirected;
  external _Stream? get body;
}

extension type _Stream(JSObject _) implements JSObject {
  external _Reader getReader();
}

extension type _Reader(JSObject _) implements JSObject {
  external JSPromise<_Chunk> read();
  external JSPromise<JSAny?> cancel();
}

extension type _Chunk(JSObject _) implements JSObject {
  external bool get done;
  external JSUint8Array get value;
}

@JS('crypto.subtle.digest')
external JSPromise<JSArrayBuffer> _digest(String algorithm, JSUint8Array bytes);

class _BrowserCatalogTransport implements CatalogTransport {
  _BrowserCatalogTransport(this.endpoint);
  final CatalogEndpoint endpoint;
  final _active = <_AbortController>{};
  bool _closed = false;

  @override
  Future<Uint8List> fetch(String path, int maxBytes) async {
    if (_closed || !endpoint.permits(path)) {
      throw StateError('Catalog request refused');
    }
    final controller = _AbortController();
    _active.add(controller);
    final timer = Timer(const Duration(seconds: 15), () => controller.abort());
    _Reader? reader;
    try {
      final response = await _fetch(
        path,
        _Options(
          credentials: 'omit',
          redirect: 'error',
          mode: 'cors',
          signal: controller.signal,
        ),
      ).toDart;
      if (!response.ok || response.redirected || response.body == null) {
        throw StateError('Furniture catalog is unavailable');
      }
      reader = response.body!.getReader();
      final bytes = BytesBuilder(copy: false);
      while (true) {
        final chunk = await reader.read().toDart;
        if (chunk.done) break;
        final part = chunk.value.toDart;
        if (part.length > maxBytes - bytes.length)
          throw const FormatException('Catalog response is too large');
        bytes.add(part);
      }
      return bytes.takeBytes();
    } finally {
      timer.cancel();
      controller.abort();
      if (reader != null) {
        try {
          await reader.cancel().toDart;
        } catch (_) {
          /* Already aborted. */
        }
      }
      _active.remove(controller);
    }
  }

  @override
  Future<String> sha256(Uint8List bytes) async {
    final result = await _digest('SHA-256', bytes.toJS).toDart;
    return result.toDart
        .asUint8List()
        .map((b) => b.toRadixString(16).padLeft(2, '0'))
        .join();
  }

  @override
  void close() {
    _closed = true;
    for (final controller in _active) {
      controller.abort();
    }
    _active.clear();
  }
}
