import 'dart:convert';
import 'dart:typed_data';

/// Configured catalog discovery, not a resolver for URLs in user plans.
/// No request is made by construction. Network work starts on explicit browse;
/// models are downloaded only on an explicit import action. All sizes are bytes.
abstract interface class CatalogTransport {
  Future<Uint8List> fetch(String path, int maxBytes);
  Future<String> sha256(Uint8List bytes);
  void close();
}

const defaultCatalogManifestUrl =
    'https://planner.homehero.pro/catalog/v1/index.json';

/// A host-selected HTTPS manifest and hash-addressed files beside it. HTTP is
/// accepted only on loopback for local development. No credentials, queries,
/// fragments, encoded paths or directory traversal are accepted.
class CatalogEndpoint {
  CatalogEndpoint(String url) : manifest = Uri.parse(url) {
    final local = const [
      'localhost',
      '127.0.0.1',
      '::1',
    ].contains(manifest.host);
    if (!(manifest.scheme == 'https' || (manifest.scheme == 'http' && local)) ||
        manifest.host.isEmpty ||
        manifest.userInfo.isNotEmpty ||
        manifest.hasQuery ||
        manifest.hasFragment ||
        !RegExp(r'^/(?:[a-zA-Z0-9_-]+/)*[a-zA-Z0-9_.-]+\.json$')
            .hasMatch(manifest.path) ||
        url.contains('%') ||
        url.contains('\\') ||
        url.contains('/../') ||
        url.contains('/./')) {
      throw const FormatException('Invalid catalog manifest URL');
    }
  }

  final Uri manifest;
  static final _asset = RegExp(
    r'^(?:models/[a-z0-9-]+\.[0-9a-f]{64}\.glb|thumbnails/[a-z0-9-]+\.[0-9a-f]{64}\.png)$',
  );

  String asset(String path) {
    if (!_asset.hasMatch(path))
      throw const FormatException('Invalid catalog asset path');
    return manifest.resolve(path).toString();
  }

  bool permits(String url) {
    if (url == manifest.toString()) return true;
    final root = manifest.resolve('.').toString();
    return url.startsWith(root) && _asset.hasMatch(url.substring(root.length));
  }
}

class CatalogFile {
  const CatalogFile(this.path, this.sha256, this.bytes);
  final String path, sha256;
  final int bytes;
}

class CatalogModel {
  const CatalogModel({
    required this.id,
    required this.name,
    required this.category,
    required this.dimensionsMm,
    required this.model,
    required this.thumbnail,
  });
  final String id, name, category;
  final List<int> dimensionsMm;
  final CatalogFile model, thumbnail;
  String get digest => 'sha256:${model.sha256}';
  String get description =>
      'Domowik catalog / $category / ${dimensionsMm.join(' x ')} mm / Apache-2.0';
}

class CatalogClient {
  /// Takes ownership of [transport]. Concurrent discovery calls share one
  /// result; failed discovery is retryable. [close] aborts host network work.
  CatalogClient(this.transport, {CatalogEndpoint? endpoint})
    : endpoint = endpoint ?? CatalogEndpoint(defaultCatalogManifestUrl);
  final CatalogTransport transport;
  final CatalogEndpoint endpoint;
  static const maxIndexBytes = 128 * 1024;
  static const maxModelBytes = 256 * 1024;
  static const maxThumbnailBytes = 64 * 1024;
  Future<List<CatalogModel>>? _index;
  bool _closed = false;

  Future<List<CatalogModel>> discover() => _index ??= _discover();

  Future<List<CatalogModel>> _discover() async {
    try {
      _checkOpen();
      final bytes = await transport.fetch(
        endpoint.manifest.toString(),
        maxIndexBytes,
      );
      _checkOpen();
      if (bytes.length > maxIndexBytes)
        throw const FormatException('Catalog index is too large');
      return parse(bytes);
    } catch (_) {
      _index = null; // A failed request must not poison later retries.
      rethrow;
    }
  }

  /// Validates the entire bounded index before exposing entries to the UI.
  static List<CatalogModel> parse(Uint8List bytes) {
    if (bytes.length > maxIndexBytes)
      throw const FormatException('Catalog index is too large');
    final value = jsonDecode(utf8.decode(bytes));
    if (value is! Map ||
        value['catalogVersion'] != 1 ||
        value['id'] is! String ||
        !RegExp(r'^[a-z0-9]+(?:-[a-z0-9]+)*$')
            .hasMatch(value['id'] as String) ||
        (value['id'] as String).length > 80) {
      throw const FormatException('Unsupported furniture catalog');
    }
    final entries = value['models'];
    if (entries is! List || entries.length > 500)
      throw const FormatException('Invalid catalog entry count');
    final ids = <String>{};
    String text(Object? value, int max) {
      if (value is! String ||
          value.isEmpty ||
          value.length > max ||
          value.contains(RegExp(r'[\x00-\x1f\x7f]'))) {
        throw const FormatException('Invalid catalog text');
      }
      return value;
    }

    CatalogFile file(
      Object? value,
      String id,
      String directory,
      String extension,
      int limit,
    ) {
      if (value is! Map) throw const FormatException('Invalid catalog file');
      final hash = value['sha256'], size = value['bytes'];
      if (hash is! String ||
          !RegExp(r'^[0-9a-f]{64}$').hasMatch(hash) ||
          size is! int ||
          size <= 0 ||
          size > limit ||
          value['path'] != '$directory/$id.$hash.$extension') {
        throw const FormatException('Invalid catalog path, digest or size');
      }
      return CatalogFile(value['path'] as String, hash, size);
    }

    return List.unmodifiable(
      entries.map((entry) {
        if (entry is! Map || entry['license'] != 'Apache-2.0')
          throw const FormatException('Unreviewed catalog license');
        final id = text(entry['id'], 80);
        if (!RegExp(r'^[a-z0-9]+(?:-[a-z0-9]+)*$').hasMatch(id) ||
            !ids.add(id)) {
          throw const FormatException('Invalid or duplicate catalog identity');
        }
        final dimensions = entry['dimensionsMm'];
        if (dimensions is! List ||
            dimensions.length != 3 ||
            dimensions.any((v) => v is! int || v <= 0 || v > 1000000)) {
          throw const FormatException('Invalid catalog dimensions');
        }
        return CatalogModel(
          id: id,
          name: text(entry['name'], 120),
          category: text(entry['category'], 80),
          dimensionsMm: List<int>.unmodifiable(dimensions.cast<int>()),
          model: file(entry['model'], id, 'models', 'glb', maxModelBytes),
          thumbnail: file(
            entry['thumbnail'],
            id,
            'thumbnails',
            'png',
            maxThumbnailBytes,
          ),
        );
      }),
    );
  }

  Future<Uint8List> _read(CatalogFile file, int limit) async {
    _checkOpen();
    final bytes = await transport.fetch(endpoint.asset(file.path), limit);
    _checkOpen();
    if (bytes.length != file.bytes ||
        bytes.length > limit ||
        await transport.sha256(bytes) != file.sha256) {
      throw const FormatException(
        'Catalog content failed its size or SHA-256 check',
      );
    }
    _checkOpen();
    return bytes;
  }

  Future<Uint8List> download(CatalogModel entry) =>
      _read(entry.model, maxModelBytes);

  Future<Uint8List> preview(CatalogModel entry) async {
    final png = await _read(entry.thumbnail, maxThumbnailBytes);
    // Bound image allocation as well as compressed/network bytes.
    const signature = [137, 80, 78, 71, 13, 10, 26, 10];
    if (png.length < 24 ||
        !List.generate(8, (i) => png[i] == signature[i]).every((v) => v) ||
        ascii.decode(png.sublist(12, 16), allowInvalid: true) != 'IHDR') {
      throw const FormatException('Invalid catalog thumbnail');
    }
    final data = ByteData.sublistView(png);
    final width = data.getUint32(16), height = data.getUint32(20);
    if (width == 0 || height == 0 || width > 256 || height > 256) {
      throw const FormatException(
        'Catalog thumbnail dimensions exceed 256 pixels',
      );
    }
    return png;
  }

  void _checkOpen() {
    if (_closed) throw StateError('Catalog client is closed');
  }

  void close() {
    _closed = true;
    _index = null;
    transport.close();
  }
}
