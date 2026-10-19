import 'dart:convert';
import 'dart:io';
import 'package:domowik_editor/platform/config_storage_native.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:domowik_editor/measurement_units.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'split defaults and saved choices preserve libraries and unknown keys',
    () async {
      final root = await Directory.systemTemp.createTemp('domowik-split-');
      addTearDown(() => root.delete(recursive: true));
      final file = File('${root.path}/config.json');
      final store = UserConfigStore(FileConfigStorage(file));
      expect(store.splitOrientation, SplitOrientation.vertical);
      expect(store.unitSystem, UnitSystem.metric);
      await file.writeAsString(
        jsonEncode({
          'version': 1,
          'assetDirectories': ['/library'],
          'future': 42,
        }),
      );
      await store.load();
      expect(store.splitOrientation, SplitOrientation.vertical);
      await store.savePreferences(
        splitOrientation: SplitOrientation.horizontal,
        unitSystem: UnitSystem.imperial,
      );
      await store.saveDirectories(['/other']);
      final restarted = UserConfigStore(FileConfigStorage(file));
      await restarted.load();
      expect(restarted.splitOrientation, SplitOrientation.horizontal);
      expect(restarted.unitSystem, UnitSystem.imperial);
      expect(restarted.assetDirectories, ['/other']);
      expect((jsonDecode(await file.readAsString()) as Map)['future'], 42);
      await restarted.saveSplitOrientation(SplitOrientation.vertical);
      await store.load();
      expect(store.splitOrientation, SplitOrientation.vertical);
      expect(store.unitSystem, UnitSystem.imperial);
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
      final store = UserConfigStore(FileConfigStorage(file));
      await store.load();
      await store.saveDirectories(['/new library', '/new library']);
      final restarted = UserConfigStore(FileConfigStorage(file));
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
      '{"version":1,"unitSystem":"unknown"}',
      '{"version":1,"unitSystem":null}',
    ]) {
      await file.writeAsString(content);
      final store = UserConfigStore(FileConfigStorage(file));
      await expectLater(store.load(), throwsA(anything));
      await expectLater(store.saveDirectories(['/new']), throwsFormatException);
      await expectLater(
        store.saveSplitOrientation(SplitOrientation.horizontal),
        throwsFormatException,
      );
      await expectLater(
        store.savePreferences(
          splitOrientation: SplitOrientation.horizontal,
          unitSystem: UnitSystem.imperial,
        ),
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
      final store = UserConfigStore(
        FileConfigStorage(File('${blocker.path}/config.json')),
      );
      await expectLater(
        store.saveDirectories(['/new']),
        throwsA(isA<FileSystemException>()),
      );
      expect(store.assetDirectories, isEmpty);
      final file = File('${root.path}/big.json');
      await file.writeAsString(' ' * (1024 * 1024 + 1));
      await expectLater(
        UserConfigStore(FileConfigStorage(file)).load(),
        throwsFormatException,
      );
    },
  );

  group('remembered active levels (issue 0117)', () {
    test(
      'invalid entries are dropped, never block saving, and unknown keys survive',
      () async {
        final root = await Directory.systemTemp.createTemp('domowik-hint-');
        addTearDown(() => root.delete(recursive: true));
        final file = File('${root.path}/config.json');
        // A non-map member, empty keys, non-string values, an empty value:
        // every BAD shape is pruned; the valid entry and the unknown keys stay.
        await file.writeAsString(
          jsonEncode({
            'version': 1,
            'assetDirectories': ['/library'],
            'lastActiveLevels': {
              'document-A': 'level-1',
              '': 'level-1',
              'document-B': 5,
              'document-C': '',
            },
            'futurePreference': 42,
          }),
        );
        final store = UserConfigStore(FileConfigStorage(file));
        await store.load();
        expect(store.lastActiveLevels, {'document-A': 'level-1'});

        // Pruning the device hint did not poison the store: the other
        // preferences still save, carrying the valid hint along, and none of
        // the bad entries return.
        await store.saveSplitOrientation(SplitOrientation.horizontal);
        final restarted = UserConfigStore(FileConfigStorage(file));
        await restarted.load();
        expect(restarted.splitOrientation, SplitOrientation.horizontal);
        expect(restarted.assetDirectories, ['/library']);
        expect(restarted.lastActiveLevels, {'document-A': 'level-1'});
        expect(
          (jsonDecode(await file.readAsString()) as Map)['futurePreference'],
          42,
        );
      },
    );

    test(
      'overrides record per document, skip identical values, and keep the cap',
      () async {
        final root = await Directory.systemTemp.createTemp('domowik-hint-');
        addTearDown(() => root.delete(recursive: true));
        final file = File('${root.path}/config.json');
        final store = UserConfigStore(FileConfigStorage(file));
        const first = 'document-1';
        await store.saveLastActiveLevel(first, 'level-1');
        expect(store.lastActiveLevels, {first: 'level-1'});
        final bytes = await file.readAsString();
        // The same value for the same document writes nothing; the store
        // must not churn over every activation.
        await store.saveLastActiveLevel(first, 'level-1');
        expect(await file.readAsString(), bytes);

        // A plan without identity never gains a hint: there is nothing to
        // attribute the value to (§2.6).
        await store.saveLastActiveLevel(null, 'level-2');
        await store.saveLastActiveLevel('', 'level-2');
        expect(store.lastActiveLevels[''], isNull);
        expect(store.lastActiveLevels.length, 1);

        // Entries move to most-recent on update; the least recently touched
        // document is dropped first once the cap is full.
        for (var i = 2; i <= 70; i++) {
          await store.saveLastActiveLevel('document-$i', 'level-1');
        }
        expect(
          store.lastActiveLevels.length,
          UserConfigStore.maxLastActiveLevels,
        );
        expect(store.lastActiveLevels['document-70'], 'level-1');
        expect(store.lastActiveLevels['document-1'], isNull);
        await store.saveLastActiveLevel(
          'document-2',
          'level-9',
        ); // touches it again
        await store.saveLastActiveLevel('document-71', 'level-1');
        expect(store.lastActiveLevels['document-2'], 'level-9');
        expect(store.lastActiveLevels['document-71'], 'level-1');
        expect(store.lastActiveLevels['document-3'], isNull);
        // The value on disk outlives the instance: a fresh store reads it.
        final restarted = UserConfigStore(FileConfigStorage(file));
        await restarted.load();
        expect(restarted.lastActiveLevels['document-2'], 'level-9');
      },
    );

    test('a hint referencing a nonexistent level is read back harmlessly', () {
      // Covered behaviourally through LevelsViewState.reset's stale-id case
      // and, with a real store file, in the editor lifecycle suite
      // (`remembering_a_deleted_level_no_longer_matches_anything`): the stale
      // value simply stops matching a live level, and reopening falls back to
      // the elevation default (BEHAVIOR-SPEC §2.6).
    });
  });
}
