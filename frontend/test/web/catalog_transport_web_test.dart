@TestOn('browser')
library;

import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'package:domowik_editor/platform/catalog_host_web.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('globalThis.fetch')
external JSFunction get _fetch;
@JS('globalThis.fetch')
external set _fetch(JSFunction value);

@JS('Response')
extension type _Response._(JSObject _) implements JSObject {
  external factory _Response(JSUint8Array bytes);
}

void main() {
  test('uses WebCrypto SHA-256 on the exact bytes', () async {
    final client = createCatalogClient();
    addTearDown(client.close);
    expect(
      await client.transport.sha256(Uint8List.fromList(ascii.encode('abc'))),
      'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
  });

  test('browser requests omit credentials, reject redirects and enforce streamed byte limits', () async {
    final original = _fetch;
    addTearDown(() => _fetch = original);
    final calls = <String>[];
    var content = Uint8List.fromList([1, 2, 3]);
    _fetch = ((JSString url, JSObject options) {
      calls.add(url.toDart);
      expect(options.getProperty<JSString>('credentials'.toJS).toDart, 'omit');
      expect(options.getProperty<JSString>('redirect'.toJS).toDart, 'error');
      expect(options.getProperty<JSString>('mode'.toJS).toDart, 'same-origin');
      return Future<JSAny?>.value(_Response(content.toJS)).toJS;
    }).toJS;
    final client = createCatalogClient();
    addTearDown(client.close);
    expect(calls, isEmpty);
    expect(await client.transport.fetch('/catalog/v1/index.json', 3), [
      1,
      2,
      3,
    ]);
    content = Uint8List(4);
    await expectLater(
      client.transport.fetch('/catalog/v1/index.json', 3),
      throwsFormatException,
    );
    await expectLater(
      client.transport.fetch('/api/v1/plans', 3),
      throwsStateError,
    );
    expect(
      calls,
      hasLength(2),
      reason: 'Non-catalog paths must not reach fetch',
    );
  });
}
