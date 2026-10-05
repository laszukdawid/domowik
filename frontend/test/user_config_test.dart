import 'dart:convert';
import 'dart:io';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'split defaults and saved choices preserve libraries and unknown keys',
    () async {
      final root = await Directory.systemTemp.createTemp('domowik-split-');
      addTearDown(() => root.delete(recursive: true));
      final file = File('${root.path}/config.json');
      final store = UserConfigStore(file);
      expect(store.splitOrientation, SplitOrientation.vertical);
      await file.writeAsString(
        jsonEncode({
          'version': 1,
          'assetDirectories': ['/library'],
          'future': 42,
        }),
      );
      await store.load();
      expect(store.splitOrientation, SplitOrientation.vertical);
      await store.saveSplitOrientation(SplitOrientation.horizontal);
      await store.saveDirectories(['/other']);
      final restarted = UserConfigStore(file);
      await restarted.load();
      expect(restarted.splitOrientation, SplitOrientation.horizontal);
      expect(restarted.assetDirectories, ['/other']);
      expect((jsonDecode(await file.readAsString()) as Map)['future'], 42);
      await restarted.saveSplitOrientation(SplitOrientation.vertical);
      await store.load();
      expect(store.splitOrientation, SplitOrientation.vertical);
    },
  );
  test(
    'user libraries survive restart, replacements and unknown preferences',
    () async {
      final root = await Directory.systemTemp.createTemp('domowik-config-');
      addTearDown(() => root.delete(recursive: true));
      final file = File('${root.path}/config.json');
      await file.writeAsString(
        jsonEncode({
          'version': 1,
          'assetDirectories': ['/original'],
          'futurePreference': {'value': 7},
        }),
      );
      final store = UserConfigStore(file);
      await store.load();
      await store.saveDirectories(['/new library', '/new library']);
      final restarted = UserConfigStore(file);
      await restarted.load();
      expect(restarted.assetDirectories, ['/new library']);
      expect(
        (jsonDecode(await file.readAsString()) as Map)['futurePreference'],
        {'value': 7},
      );
      expect(
        await root.list().length,
        1,
        reason: 'atomic-save staging is removed',
      );
    },
  );
  test('corrupt or future configuration is reported and preserved', () async {
    final root = await Directory.systemTemp.createTemp('domowik-config-bad-');
    addTearDown(() => root.delete(recursive: true));
    final file = File('${root.path}/config.json');
    for (final content in [
      'broken JSON',
      '{"version":99,"assetDirectories":[]}',
      '{"version":1,"assetDirectories":["relative"]}',
      '{"version":1,"splitOrientation":"diagonal"}',
    ]) {
      await file.writeAsString(content);
      final store = UserConfigStore(file);
      await expectLater(store.load(), throwsA(anything));
      await expectLater(store.saveDirectories(['/new']), throwsFormatException);
      await expectLater(
        store.saveSplitOrientation(SplitOrientation.horizontal),
        throwsFormatException,
      );
      expect(await file.readAsString(), content);
    }
  });
  test(
    'failed writes retain preferences and oversized input is rejected',
    () async {
      final root = await Directory.systemTemp.createTemp(
        'domowik-config-write-',
      );
      addTearDown(() => root.delete(recursive: true));
      final blocker = File('${root.path}/blocked')
        ..writeAsStringSync('original');
      final store = UserConfigStore(File('${blocker.path}/config.json'));
      await expectLater(
        store.saveDirectories(['/new']),
        throwsA(isA<FileSystemException>()),
      );
      expect(store.assetDirectories, isEmpty);
      final file = File('${root.path}/big.json');
      await file.writeAsString(' ' * (1024 * 1024 + 1));
      await expectLater(UserConfigStore(file).load(), throwsFormatException);
    },
  );
}
