import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/services.dart';

import '../asset_library.dart' show scanAssetDirectories;
import 'model_import_types.dart';

const _files = MethodChannel('invalid.domowik-spike/files');

/// Native folder selection is implemented by the Linux and macOS desktop
/// runners. Android's document provider is not a filesystem directory picker.
bool get modelDirectoryImportAvailable => Platform.isLinux || Platform.isMacOS;

/// Native hosts use attached directories for single-model placement.
Future<(String, Uint8List)?> pickModelFile() async => null;

/// Selects a folder and lazily reads supported model files from its tree.
Future<PickedModelDirectory?> pickModelDirectory() async {
  if (!modelDirectoryImportAvailable) return null;
  final path = await _files.invokeMethod<String>('pickAssetDirectory');
  if (path == null) return null;

  final scan = await scanAssetDirectories([path]);
  final files = scan.models.map((model) {
    final path = model.file!;
    return PickedModelFile(
      path.split(Platform.pathSeparator).last,
      () => _readModel(path),
      sourcePath: path,
    );
  });
  return PickedModelDirectory(
    Stream<PickedModelFile>.fromIterable(files),
    diagnostics: scan.diagnostics,
  );
}

Future<Uint8List> _readModel(String path) async {
  final file = File(path);
  final length = await file.length();
  if (length == 0 || length > maxModelImportBytes) {
    throw FileSystemException(
      'Model is empty or exceeds the per-file 256 MiB limit',
      path,
    );
  }

  final builder = BytesBuilder(copy: false);
  await for (final chunk in file.openRead(0, maxModelImportBytes + 1)) {
    builder.add(chunk);
    if (builder.length > maxModelImportBytes) {
      throw FileSystemException(
        'Model exceeds the per-file 256 MiB limit',
        path,
      );
    }
  }
  final bytes = builder.takeBytes();
  if (bytes.isEmpty) {
    throw FileSystemException('Model is empty', path);
  }
  return bytes;
}
