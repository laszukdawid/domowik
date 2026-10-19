import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/asset_thumbnails.dart';
import 'package:domowik_editor/user_config.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// Original synthetic 2x2 solid-colour PNG; no catalog artwork.
Uint8List get _png => base64Decode(
  'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGNImXbiPwgzwBgAXzALBcb8yfgAAAAASUVORK5CYII=',
);
const _models = [
  LibraryModel(name: 'Original chair', digest: 'sha256:chair'),
  LibraryModel(name: 'Original table', digest: 'sha256:table'),
];

void main() {
  Future<void> start(
    WidgetTester tester, {
    List<LibraryModel> models = _models,
    Future<Uint8List> Function(LibraryModel)? preview,
    AssetLibraryView initialView = AssetLibraryView.list,
    ValueChanged<AssetLibraryView>? onViewChanged,
    Size size = const Size(1100, 850),
  }) async {
    await tester.binding.setSurfaceSize(size);
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AssetLibraryDialog(
            directories: const [],
            embedded: models,
            directoriesAvailable: false,
            onAttach: (_) =>
                throw StateError('No directory mutations expected'),
            onReplace: (_, _) =>
                throw StateError('No directory mutations expected'),
            onPreview: preview ?? (_) async => _png,
            initialView: initialView,
            onViewChanged: onViewChanged,
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  testWidgets(
    'list prefix thumbnails and grid preserve search, selection and cached art',
    (tester) async {
      final calls = <String>[];
      final views = <AssetLibraryView>[];
      await start(
        tester,
        preview: (m) async {
          calls.add(m.digest!);
          return _png;
        },
        onViewChanged: views.add,
      );
      expect(find.byKey(const ValueKey('asset-list')), findsOneWidget);
      for (final tile in tester.widgetList<ListTile>(find.byType(ListTile))) {
        expect(tile.leading, isA<SizedBox>());
      }
      expect(find.byType(Image), findsNWidgets(2));
      await tester.enterText(
        find.byKey(const ValueKey('asset-search')),
        'table',
      );
      await tester.pumpAndSettle();
      await tester.tap(find.text('Original table'));
      await tester.tap(find.byTooltip('Grid view'));
      await tester.pumpAndSettle();
      expect(find.byKey(const ValueKey('asset-grid')), findsOneWidget);
      expect(find.byType(ListTile), findsNothing);
      expect(find.text('Original chair'), findsNothing);
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('asset-search')))
            .controller!
            .text,
        'table',
      );
      expect(
        tester
            .widget<FilledButton>(find.byKey(const ValueKey('choose-asset')))
            .onPressed,
        isNotNull,
      );
      expect(views, [AssetLibraryView.grid]);
      expect(calls, ['sha256:chair', 'sha256:table']);
      await tester.tap(find.byTooltip('List view'));
      await tester.pumpAndSettle();
      expect(tester.widget<ListTile>(find.byType(ListTile)).selected, isTrue);
      expect(
        calls,
        hasLength(2),
        reason: 'Switching views must reuse previews',
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'initial grid supports a narrow screen and unavailable preview selection',
    (tester) async {
      await start(
        tester,
        initialView: AssetLibraryView.grid,
        size: const Size(430, 740),
        models: const [
          LibraryModel(
            name:
                'An original model with a deliberately very long descriptive name',
            digest: 'sha256:bad',
          ),
        ],
        preview: (_) async => throw const FormatException('Unsupported model'),
      );
      expect(find.byKey(const ValueKey('asset-grid')), findsOneWidget);
      expect(find.byIcon(Icons.image_not_supported_outlined), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('sha256:bad')));
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<FilledButton>(find.byKey(const ValueKey('choose-asset')))
            .onPressed,
        isNotNull,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('a large grid requests only displayed thumbnails', (
    tester,
  ) async {
    var calls = 0;
    await start(
      tester,
      initialView: AssetLibraryView.grid,
      models: [
        for (var i = 0; i < 1000; i++)
          LibraryModel(name: 'Model $i', digest: 'sha256:$i'),
      ],
      preview: (_) async {
        calls++;
        return _png;
      },
    );
    expect(calls, greaterThan(0));
    expect(calls, lessThan(25));
    await tester.enterText(
      find.byKey(const ValueKey('asset-search')),
      'does not exist',
    );
    await tester.pumpAndSettle();
    expect(find.text('No models match your search.'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'grid selection returns the model only after explicit placement',
    (tester) async {
      AssetChoice? choice;
      await tester.binding.setSurfaceSize(const Size(1000, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () async {
                  choice = await showDialog<AssetChoice>(
                    context: context,
                    builder: (_) => AssetLibraryDialog(
                      directories: const [],
                      embedded: _models,
                      onAttach: (_) => null,
                      onReplace: (_, _) => null,
                      initialView: AssetLibraryView.grid,
                      onPreview: (_) async => _png,
                    ),
                  );
                },
                child: const Text('Open library'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open library'));
      await tester.pumpAndSettle();
      await tester.tap(find.text('Original chair'));
      await tester.pumpAndSettle();
      expect(choice, isNull);
      await tester.tap(find.byKey(const ValueKey('choose-asset')));
      await tester.pumpAndSettle();
      expect(choice!.model.digest, 'sha256:chair');
    },
  );

  test(
    'thumbnail queue shares work, drops invisible jobs and closes safely',
    () async {
      final cache = AssetThumbnailCache();
      final active = Completer<Uint8List>();
      var calls = 0;
      final first = cache.request('a', () {
        calls++;
        return active.future;
      });
      final same = cache.request('a', () async {
        throw StateError('Duplicate decode');
      });
      final invisible = cache.request('b', () async {
        throw StateError('Cancelled job ran');
      });
      await Future<void>.delayed(Duration.zero);
      expect(calls, 1);
      invisible.release();
      expect(await invisible.result, isNull);
      active.complete(_png);
      expect((await first.result)!.png, isNotNull);
      expect(identical(await first.result, await same.result), isTrue);
      final cached = cache.request('a', () async {
        throw StateError('Cache miss');
      });
      expect((await cached.result)!.png, isNotNull);
      first.release();
      same.release();
      cached.release();
      final pending = Completer<Uint8List>();
      final inFlight = cache.request('c', () => pending.future);
      final queued = cache.request('d', () async {
        throw StateError('Closed queue ran');
      });
      await Future<void>.delayed(Duration.zero);
      cache.dispose();
      expect(await queued.result, isNull);
      pending.complete(_png);
      expect(await inFlight.result, isNull);
      inFlight.release();
      queued.release();
    },
  );

  test(
    'thumbnail cache bounds completed entries and caches failures',
    () async {
      final cache = AssetThumbnailCache();
      addTearDown(cache.dispose);
      var failures = 0;
      Future<Uint8List> broken() async {
        failures++;
        throw StateError('Bad model');
      }

      final bad = cache.request('broken', broken);
      expect((await bad.result)!.error, contains('Bad model'));
      bad.release();
      final again = cache.request('broken', broken);
      await again.result;
      again.release();
      expect(failures, 1);
      for (var i = 0; i < AssetThumbnailCache.maxEntries; i++) {
        final r = cache.request('$i', () async => _png);
        await r.result;
        r.release();
      }
      final evicted = cache.request('broken', broken);
      await evicted.result;
      evicted.release();
      expect(failures, 2);
    },
  );
}
