import '../asset_library.dart' show DirectoryModels;

/// Browsers expose no directories by path; models arrive by upload instead.
Future<DirectoryModels> scanAssetDirectories(List<String> directories) async =>
    const DirectoryModels([], []);
