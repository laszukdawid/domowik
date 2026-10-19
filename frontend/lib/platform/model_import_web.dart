import 'dart:typed_data';

import 'browser_files.dart';
import 'model_import_types.dart';

bool get modelDirectoryImportAvailable => true;

/// A browser cannot browse arbitrary filesystem paths, but it can upload the
/// selected directory tree as a set of files.
Future<PickedModelDirectory?> pickModelDirectory() async {
  final selection = await pickDirectoryFiles();
  if (selection == null) return null;
  return PickedModelDirectory(
    Stream<PickedModelFile>.fromIterable(
      selection.files.map(
        (file) => PickedModelFile(
          file.name,
          file.readBytes,
          sourcePath: file.sourcePath,
        ),
      ),
    ),
    diagnostics: selection.diagnostics,
  );
}

/// The browser's single-file upload remains available in the Furniture dialog.
Future<(String, Uint8List)?> pickModelFile() async {
  final file = await pickFile('.glb,.obj,.zip');
  return file == null ? null : (file.name, file.bytes);
}
