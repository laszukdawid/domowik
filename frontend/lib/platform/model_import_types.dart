import 'dart:typed_data';

/// Must match the container's per-entry asset byte limit.
const maxModelImportBytes = 256 * 1024 * 1024;

/// One model selected for import. Reads happen lazily so directory imports
/// hold at most one model's bytes at a time.
class PickedModelFile {
  const PickedModelFile(this.name, this.readBytes, {this.sourcePath});

  final String name;

  /// Native absolute path or browser-relative path within the selected folder.
  /// Null for test/custom sources that only have a file name.
  final String? sourcePath;
  final Future<Uint8List> Function() readBytes;
}

/// A selected directory's supported models and any scan-limit diagnostics.
class PickedModelDirectory {
  const PickedModelDirectory(this.files, {this.diagnostics = const []});

  final Stream<PickedModelFile> files;
  final List<String> diagnostics;
}
