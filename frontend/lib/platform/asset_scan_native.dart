import 'dart:io';

import '../asset_library.dart' show DirectoryModels, LibraryModel;

/// Filesystem enumeration only: no model reads, extraction or decoding. Symlinks
/// are skipped. Work is capped at 10,000 entries and 64 directory levels; a
/// truncated or inaccessible library has an explicit diagnostic.
Future<DirectoryModels> scanAssetDirectories(List<String> directories) async {
  final models = <LibraryModel>[];
  final diagnostics = <String>[];
  final seen = <String>{};
  var visited = 0;
  Future<void> walk(String path, int depth, String sourceDirectory) async {
    if (!seen.add(path)) return;
    if (depth > 64) {
      diagnostics.add('Directory nesting limit reached: $path');
      return;
    }
    try {
      await for (final entry in Directory(path).list(followLinks: false)) {
        if (++visited > 10000) {
          if (!diagnostics.contains(
            'Library scan limited to 10,000 entries.',
          )) {
            diagnostics.add('Library scan limited to 10,000 entries.');
          }
          return;
        }
        if (entry is Directory) {
          await walk(entry.path, depth + 1, sourceDirectory);
        } else if (entry is File) {
          final name = entry.uri.pathSegments.last;
          final dot = name.lastIndexOf('.');
          if (dot > 0 &&
              {
                'glb',
                'obj',
                'zip',
              }.contains(name.substring(dot + 1).toLowerCase())) {
            models.add(
              LibraryModel(
                name: name.substring(0, dot),
                file: entry.absolute.path,
                sourceDirectory: sourceDirectory,
              ),
            );
          }
        }
      }
    } on FileSystemException catch (e) {
      diagnostics.add('Cannot read $path: ${e.message}');
    }
  }

  for (final directory in directories) {
    if (visited > 10000) break;
    await walk(directory, 0, directory);
  }
  models.sort((a, b) => a.file!.compareTo(b.file!));
  return DirectoryModels(models, diagnostics);
}

