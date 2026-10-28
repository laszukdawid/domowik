import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:domowik_editor/platform/config_storage_native.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:domowik_editor/measurement_units.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('asset library view defaults to list and survives unrelated preference saves', () async {
    final root = await Directory.systemTemp.createTemp('domowik-library-view-');
    addTearDown(() => root.delete(recursive: true));
    final file = File('${root.path}/config.json');
    final store = UserConfigStore(FileConfigStorage(file));
    expect(store.assetLibraryView, AssetLibraryView.list);
    await store.saveAssetLibraryView(AssetLibraryView.grid);
    await store.saveDirectories(['/library']);
    final restarted = UserConfigStore(FileConfigStorage(file));
    await restarted.load();
    expect(restarted.assetLibraryView, AssetLibraryView.grid);
    await restarted.saveAssetLibraryView(AssetLibraryView.list);
    await store.load();
    expect(store.assetLibraryView, AssetLibraryView.list);
    expect(store.assetDirectories, ['/library']);
  });
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
      '{"version":1,"assetLibraryView":"tiles"}',
      '{"version":1,"defaultViewMode":"unknown"}',
      '{"version":1,"defaultViewMode":null}',
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

  group('layout view preferences', () {
    test(
      'global fallback and layout overrides survive restart independently',
      () async {
        final storage = _DelayedStorage();
        final store = UserConfigStore(storage);
        expect(store.viewModeFor('A'), '2d');
        await store.savePreferences(
          splitOrientation: SplitOrientation.horizontal,
          unitSystem: UnitSystem.imperial,
          defaultViewMode: 'split',
          documentKey: 'A',
          rememberedViewMode: '3d',
        );
        await store.saveLastActiveLevel('A', 'floor');
        await store.saveDirectories(['/library']);
        final restarted = UserConfigStore(storage);
        await restarted.load();
        expect(restarted.viewModeFor('A'), '3d');
        expect(restarted.viewModeFor('B'), 'split');
        expect(restarted.viewModeFor(null), 'split');
        expect(restarted.lastActiveLevels, {'A': 'floor'});
        await restarted.savePreferences(
          splitOrientation: SplitOrientation.vertical,
          unitSystem: UnitSystem.metric,
          documentKey: 'A',
        );
        expect(restarted.rememberedViewModes, isEmpty);
        expect(restarted.viewModeFor('A'), 'split');
        expect(restarted.assetDirectories, ['/library']);
      },
    );

    test('queued view changes, forgetting and other settings keep invocation order', () async {
      final storage = _DelayedStorage();
      final store = UserConfigStore(storage);
      storage.delay = Completer<void>();
      final updates = [
        store.saveRememberedViewMode('A', 'split'),
        store.saveRememberedViewMode('A', '3d'),
        store.saveRememberedViewMode('B', 'split'),
        store.saveRememberedViewMode('A', null),
        store.savePreferences(
          splitOrientation: SplitOrientation.horizontal,
          unitSystem: UnitSystem.imperial,
          defaultViewMode: '3d',
        ),
      ];
      final flushed = store.flush();
      storage.delay!.complete();
      await Future.wait([...updates, flushed]);
      final restarted = UserConfigStore(storage);
      await restarted.load();
      expect(restarted.rememberedViewModes, {'B': 'split'});
      expect(restarted.viewModeFor('A'), '3d');
      expect(restarted.splitOrientation, SplitOrientation.horizontal);
      expect(restarted.unitSystem, UnitSystem.imperial);
    });

    test('invalid hints are pruned, newest entries retained and unknown keys preserved', () async {
      final storage = _DelayedStorage()
        ..bytes = utf8.encode(
          jsonEncode({
            'version': 1,
            'futurePreference': 42,
            'rememberedViewModes': {
              for (var i = 0; i < 70; i++) 'plan-$i': 'split',
              '': '2d',
              'invalid': 'future-mode',
              'wrong-type': 5,
            },
          }),
        );
      final store = UserConfigStore(storage);
      await store.load();
      expect(
        store.rememberedViewModes.length,
        UserConfigStore.maxRememberedViewModes,
      );
      expect(store.rememberedViewModes.keys.first, 'plan-6');
      await store.saveRememberedViewMode('plan-6', '3d');
      await store.saveRememberedViewMode('new', '2d');
      expect(store.rememberedViewModes['plan-6'], '3d');
      expect(store.rememberedViewModes.containsKey('plan-7'), isFalse);
      final restarted = UserConfigStore(storage);
      await restarted.load();
      expect(restarted.rememberedViewModes, store.rememberedViewModes);
      expect(
        (jsonDecode(utf8.decode(storage.bytes!)) as Map)['futurePreference'],
        42,
      );
      storage.bytes = utf8.encode('{"version":1,"rememberedViewModes":false}');
      await restarted.load();
      expect(restarted.rememberedViewModes, isEmpty);
    });

    test(
      'no-op hints skip storage; failed or invalid updates apply nothing',
      () async {
        final storage = _DelayedStorage();
        final store = UserConfigStore(storage);
        await store.saveRememberedViewMode('A', 'split');
        storage.fail = true;
        await store.saveRememberedViewMode('A', 'split');
        await store.saveRememberedViewMode(null, '3d');
        await expectLater(
          store.savePreferences(
            splitOrientation: SplitOrientation.horizontal,
            unitSystem: UnitSystem.imperial,
            defaultViewMode: '3d',
            documentKey: 'A',
          ),
          throwsStateError,
        );
        expect(store.viewModeFor('A'), 'split');
        expect(store.defaultViewMode, '2d');
        expect(store.splitOrientation, SplitOrientation.vertical);
        await expectLater(
          store.saveRememberedViewMode('A', 'invalid'),
          throwsFormatException,
        );
        storage.fail = false;
        await store.saveRememberedViewMode('A', '2d');
        expect(store.viewModeFor('A'), '2d');
      },
    );
  });

  group('remembered active levels (issue 0117)', () {
    test(
      'rapid activations and preference saves preserve the final choice',
      () async {
        final storage = _DelayedStorage();
        final store = UserConfigStore(storage);
        await store.saveLastActiveLevel('plan', 'A');
        storage.delay = Completer<void>();
        final writes = [
          store.saveLastActiveLevel('plan', 'B'),
          store.saveLastActiveLevel('plan', 'A'),
          store.saveLastActiveLevel('other', 'C'),
          store.savePreferences(
            splitOrientation: SplitOrientation.horizontal,
            unitSystem: UnitSystem.imperial,
          ),
        ];
        // Let the first write reach storage while subsequent updates queue.
        await Future<void>.delayed(Duration.zero);
        storage.delay!.complete();
        await Future.wait(writes);
        final restarted = UserConfigStore(storage);
        await restarted.load();
        expect(restarted.lastActiveLevels, {'plan': 'A', 'other': 'C'});
        expect(restarted.unitSystem, UnitSystem.imperial);
        expect(restarted.splitOrientation, SplitOrientation.horizontal);
      },
    );

    test('eviction retains recency across restart', () async {
      final storage = _DelayedStorage();
      final store = UserConfigStore(storage);
      for (var i = 0; i < UserConfigStore.maxLastActiveLevels; i++) {
        await store.saveLastActiveLevel('plan-$i', 'A');
      }
      final restarted = UserConfigStore(storage);
      await restarted.load();
      await restarted.saveLastActiveLevel('new', 'B');
      expect(restarted.lastActiveLevels['plan-0'], isNull);
      expect(restarted.lastActiveLevels['plan-63'], 'A');
    });

    test('failed hint writes do not block later saves', () async {
      final storage = _DelayedStorage()..fail = true;
      final store = UserConfigStore(storage);
      await expectLater(
        store.saveLastActiveLevel('plan', 'A'),
        throwsStateError,
      );
      storage.fail = false;
      await store.saveLastActiveLevel('plan', 'B');
      expect(store.lastActiveLevels, {'plan': 'B'});
    });

    test('invalid entries are dropped, never block saving, and unknown keys survive', () async {
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
    });

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

class _DelayedStorage implements ConfigStorage {
  List<int>? bytes;
  Completer<void>? delay;
  bool fail = false;

  @override
  Future<List<int>?> read(int limit) async => bytes;

  @override
  Future<void> write(String text) async {
    await delay?.future;
    if (fail) throw StateError('write failed');
    bytes = utf8.encode(text);
  }
}
