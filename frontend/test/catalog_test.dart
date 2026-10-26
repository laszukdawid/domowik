import 'dart:convert';
import 'dart:typed_data';

import 'package:domowik_editor/catalog.dart';
import 'package:flutter_test/flutter_test.dart';

final _hash = List.filled(64, 'a').join();
final _otherHash = List.filled(64, 'b').join();
Map<String, Object?> entry() => {
  'id': 'original-chair',
  'name': 'Original chair',
  'category': 'Seating',
  'dimensionsMm': [450, 500, 850],
  'license': 'Apache-2.0',
  'model': {
    'path': 'models/original-chair.$_hash.glb',
    'sha256': _hash,
    'bytes': 3,
  },
  'thumbnail': {
    'path': 'thumbnails/original-chair.$_hash.png',
    'sha256': _hash,
    'bytes': 24,
  },
};
Uint8List index(List<Object?> entries) => Uint8List.fromList(
  utf8.encode(
    jsonEncode({
      'catalogVersion': 1,
      'id': 'domowik-common-furniture-v1',
      'models': entries,
    }),
  ),
);

class _Transport implements CatalogTransport {
  final paths = <String>[];
  Uint8List metadata = index([entry()]);
  Uint8List content = Uint8List.fromList([1, 2, 3]);
  String hash = _hash;
  bool fail = false, closed = false;
  @override
  Future<Uint8List> fetch(String path, int limit) async {
    paths.add(path);
    if (fail) throw StateError('Synthetic network failure');
    return path.endsWith('index.json') ? metadata : content;
  }

  @override
  Future<String> sha256(Uint8List bytes) async => hash;
  @override
  void close() {
    closed = true;
  }
}

void main() {
  test('construction is lazy, discovery is shared, models only on explicit download', () async {
    final transport = _Transport();
    final client = CatalogClient(transport);
    expect(transport.paths, isEmpty);
    final results = await Future.wait([client.discover(), client.discover()]);
    expect(transport.paths, ['/catalog/v1/index.json']);
    expect(identical(results[0], results[1]), isTrue);
    final model = results[0].single;
    expect(model.description, contains('Seating'));
    expect(model.digest, 'sha256:$_hash');
    expect(await client.download(model), [1, 2, 3]);
    expect(
      transport.paths.last,
      '/catalog/v1/models/original-chair.$_hash.glb',
    );
  });

  test('a network failure is retryable, not cached forever', () async {
    final transport = _Transport()..fail = true;
    final client = CatalogClient(transport);
    await expectLater(client.discover(), throwsStateError);
    transport.fail = false;
    expect(await client.discover(), hasLength(1));
    expect(transport.paths, hasLength(2));
  });

  test(
    'rejects incorrect digest and incorrect actual byte count before import',
    () async {
      final transport = _Transport();
      final client = CatalogClient(transport);
      final model = (await client.discover()).single;
      transport.hash = _otherHash;
      await expectLater(client.download(model), throwsFormatException);
      transport.hash = _hash;
      transport.content = Uint8List(4);
      await expectLater(client.download(model), throwsFormatException);
    },
  );

  test('closed clients cannot download or discover', () async {
    final transport = _Transport();
    final client = CatalogClient(transport);
    final model = (await client.discover()).single;
    client.close();
    expect(transport.closed, isTrue);
    await expectLater(client.download(model), throwsStateError);
    await expectLater(client.discover(), throwsStateError);
  });

  test('thumbnail reads never request a model and reject oversized decoded dimensions', () async {
    final transport = _Transport();
    final client = CatalogClient(transport);
    final model = (await client.discover()).single;
    final png = Uint8List(24)
      ..setAll(0, [137, 80, 78, 71, 13, 10, 26, 10])
      ..setAll(12, ascii.encode('IHDR'));
    ByteData.sublistView(png)
      ..setUint32(16, 256)
      ..setUint32(20, 256);
    transport.content = png;
    expect(await client.preview(model), png);
    expect(transport.paths.last, contains('/thumbnails/'));
    expect(transport.paths.any((p) => p.endsWith('.glb')), isFalse);
    ByteData.sublistView(png).setUint32(16, 100000);
    await expectLater(client.preview(model), throwsFormatException);
  });

  test('index and entry count have hard byte/count bounds', () {
    expect(
      () => CatalogClient.parse(Uint8List(CatalogClient.maxIndexBytes + 1)),
      throwsFormatException,
    );
    expect(
      () => CatalogClient.parse(index(List.filled(501, entry()))),
      throwsFormatException,
    );
  });

  final invalid = <String, void Function(Map<String, Object?>)>{
    'unreviewed license': (e) => e['license'] = 'unknown',
    'oversized model': (e) =>
        (e['model'] as Map)['bytes'] = CatalogClient.maxModelBytes + 1,
    'oversized thumbnail': (e) =>
        (e['thumbnail'] as Map)['bytes'] = CatalogClient.maxThumbnailBytes + 1,
    'non-integer bytes': (e) => (e['model'] as Map)['bytes'] = 1.5,
    'external origin': (e) =>
        (e['model'] as Map)['path'] = 'https://example.invalid/model.glb',
    'network path': (e) =>
        (e['model'] as Map)['path'] = '//localhost/private.glb',
    'traversal': (e) => (e['model'] as Map)['path'] = '../private.glb',
    'encoded traversal': (e) =>
        (e['model'] as Map)['path'] = 'models/%2e%2e/private.glb',
    'query string': (e) =>
        (e['model'] as Map)['path'] = 'models/original-chair.$_hash.glb?x=1',
    'digest not in path': (e) => (e['model'] as Map)['sha256'] = _otherHash,
    'invalid identifier': (e) => e['id'] = '../chair',
    'invalid dimensions': (e) => e['dimensionsMm'] = [0, 10, 10],
    'control characters': (e) => e['name'] = 'Chair\nInjected',
  };
  for (final check in invalid.entries) {
    test('refuses ${check.key}', () {
      final value = entry();
      check.value(value);
      expect(() => CatalogClient.parse(index([value])), throwsFormatException);
    });
  }
  test('refuses duplicate entries', () {
    expect(
      () => CatalogClient.parse(index([entry(), entry()])),
      throwsFormatException,
    );
  });
}
