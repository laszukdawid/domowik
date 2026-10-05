import 'dart:convert';
import 'dart:io';

/// Orientation of the divider: vertical places 2D left and 3D right.
enum SplitOrientation { vertical, horizontal }

/// Local user/device preferences. Unknown JSON keys survive updates. Reads are
/// bounded and unsupported/corrupt files are never silently overwritten.
class UserConfigStore {
  UserConfigStore(this.file);
  UserConfigStore.inMemory() : file = null;
  final File? file;
  Map<String, Object?> _data = {'version': 1, 'assetDirectories': <String>[]};
  bool _writable = true;
  SplitOrientation get splitOrientation =>
      _data['splitOrientation'] == 'horizontal'
      ? SplitOrientation.horizontal
      : SplitOrientation.vertical;
  List<String> get assetDirectories =>
      List.unmodifiable((_data['assetDirectories'] as List).cast<String>());

  static UserConfigStore standard() {
    final env = Platform.environment;
    // Android's launcher supplies its private persistent files directory here.
    // A valid XDG location also works on Linux without relying on HOME.
    if (!Platform.isMacOS &&
        !Platform.isWindows &&
        env['XDG_CONFIG_HOME']?.startsWith('/') == true) {
      return UserConfigStore(
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
    return UserConfigStore(File('$base/domowik/config.json'));
  }

  Future<void> load() async {
    final source = file;
    if (source == null || !await source.exists()) return;
    try {
      if (await source.length() > 1024 * 1024)
        throw const FormatException('User config exceeds 1 MiB');
      final handle = await source.open();
      final List<int> text;
      try {
        text = await handle.read(1024 * 1024 + 1);
      } finally {
        await handle.close();
      }
      if (text.length > 1024 * 1024)
        throw const FormatException('User config exceeds 1 MiB');
      final data = jsonDecode(utf8.decode(text)) as Map<String, Object?>;
      if (data['version'] != 1)
        throw const FormatException('Unsupported user config version');
      final dirs = data['assetDirectories'] ?? <String>[];
      if (dirs is! List ||
          dirs.length > 64 ||
          dirs.any(
            (p) =>
                p is! String ||
                !p.startsWith('/') && !RegExp(r'^[A-Za-z]:[\\/]').hasMatch(p),
          )) {
        throw const FormatException('Invalid configured asset directories');
      }
      final split = data['splitOrientation'];
      if (data.containsKey('splitOrientation') &&
          split != 'vertical' &&
          split != 'horizontal') {
        throw const FormatException('Invalid split orientation');
      }
      _data = {...data, 'assetDirectories': dirs};
    } catch (_) {
      _writable = false;
      rethrow;
    }
  }

  Future<void> saveDirectories(List<String> directories) async {
    if (!_writable)
      throw const FormatException(
        'Repair the existing user config before saving preferences',
      );
    final unique = directories.toSet().toList();
    if (unique.any(
      (p) => !p.startsWith('/') && !RegExp(r'^[A-Za-z]:[\\/]').hasMatch(p),
    )) {
      throw const FormatException('Asset directory paths must be absolute');
    }
    if (unique.length > 64)
      throw const FormatException(
        'At most 64 global asset directories are supported',
      );
    await _save({..._data, 'version': 1, 'assetDirectories': unique});
  }

  Future<void> saveSplitOrientation(SplitOrientation orientation) =>
      _save({..._data, 'splitOrientation': orientation.name});

  Future<void> _save(Map<String, Object?> data) async {
    if (!_writable) {
      throw const FormatException(
        'Repair the existing user config before saving preferences',
      );
    }
    final text = '${const JsonEncoder.withIndent('  ').convert(data)}\n';
    if (utf8.encode(text).length > 1024 * 1024)
      throw const FormatException('User config exceeds 1 MiB');
    final target = file;
    if (target != null) {
      await target.parent.create(recursive: true);
      final temporary = File(
        '${target.path}.${DateTime.now().microsecondsSinceEpoch}.tmp',
      );
      try {
        await temporary.writeAsString(text, flush: true);
        await temporary.rename(target.path);
      } finally {
        if (await temporary.exists()) await temporary.delete();
      }
    }
    _data = data;
  }
}
