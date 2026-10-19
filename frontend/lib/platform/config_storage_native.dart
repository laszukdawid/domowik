import 'dart:io';

import 'config_storage_types.dart';

/// Preferences in one JSON file, replaced atomically through a temporary
/// sibling so a crash never leaves a truncated document.
class FileConfigStorage implements ConfigStorage {
  FileConfigStorage(this.file);
  final File file;

  @override
  Future<List<int>?> read(int limit) async {
    if (!await file.exists()) return null;
    if (await file.length() > limit) return List.filled(limit + 1, 0);
    final handle = await file.open();
    try {
      return await handle.read(limit + 1);
    } finally {
      await handle.close();
    }
  }

  @override
  Future<void> write(String text) async {
    await file.parent.create(recursive: true);
    final temporary = File(
      '${file.path}.${DateTime.now().microsecondsSinceEpoch}.tmp',
    );
    try {
      await temporary.writeAsString(text, flush: true);
      await temporary.rename(file.path);
    } finally {
      if (await temporary.exists()) await temporary.delete();
    }
  }
}

/// The per-user configuration file for this operating system.
ConfigStorage standardConfigStorage() {
  final env = Platform.environment;
  // Android's launcher supplies its private persistent files directory here.
  // A valid XDG location also works on Linux without relying on HOME.
  if (!Platform.isMacOS &&
      !Platform.isWindows &&
      env['XDG_CONFIG_HOME']?.startsWith('/') == true) {
    return FileConfigStorage(
      File('${env['XDG_CONFIG_HOME']}/domowik/config.json'),
    );
  }
  final home = env['HOME'] ?? env['USERPROFILE'];
  if (home == null)
    throw FileSystemException(
      'Cannot locate the user configuration directory',
    );
  final base = Platform.isMacOS
      ? '$home/Library/Application Support'
      : Platform.isWindows
      ? env['APPDATA'] ?? '$home/AppData/Roaming'
      : env['XDG_CONFIG_HOME']?.startsWith('/') == true
      ? env['XDG_CONFIG_HOME']!
      : '$home/.config';
  return FileConfigStorage(File('$base/domowik/config.json'));
}
