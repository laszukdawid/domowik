import 'dart:convert';

import 'package:domowik_editor/asset_library.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final png = base64Decode(
    'iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0kAAAAEUlEQVR4nGNImXbiPwgzwBgAXzALBcb8yfgAAAAASUVORK5CYII=',
  );

  testWidgets(
    'opening discovers once, only visible previews load, category search remains local',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      var discoveries = 0;
      final previews = <String>[];
      AssetChoice? chosen;
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Scaffold(
              body: TextButton(
                onPressed: () async {
                  chosen = await showDialog<AssetChoice>(
                    context: context,
                    builder: (_) => AssetLibraryDialog(
                      directories: const [],
                      embedded: const [],
                      directoriesAvailable: false,
                      onAttach: (_) => null,
                      onReplace: (_, _) => null,
                      onDiscover: () async {
                        discoveries++;
                        return [
                          for (var i = 0; i < 100; i++)
                            LibraryModel(
                              name: 'Catalog chair $i',
                              digest: 'sha256:catalog-$i',
                              sourceDescription: i == 99 ? 'Office' : 'Seating',
                            ),
                        ];
                      },
                      onPreview: (m) async {
                        previews.add(m.digest!);
                        return png;
                      },
                    ),
                  );
                },
                child: const Text('Furniture'),
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(discoveries, 0, reason: 'No metadata or art at startup');
      expect(previews, isEmpty);
      await tester.tap(find.text('Furniture'));
      await tester.pumpAndSettle();
      expect(discoveries, 1);
      expect(previews, isNotEmpty);
      expect(
        previews.length,
        lessThan(30),
        reason: 'Never decode/fetch all 100 entries to browse',
      );
      await tester.enterText(
        find.byKey(const ValueKey('asset-search')),
        'Office',
      );
      await tester.pumpAndSettle();
      expect(find.text('Catalog chair 99'), findsOneWidget);
      expect(discoveries, 1, reason: 'Search never rescans the network');
      await tester.tap(find.text('Catalog chair 99'));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const ValueKey('choose-asset')));
      await tester.pumpAndSettle();
      expect(chosen!.model.name, 'Catalog chair 99');
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets(
    'catalog failure is retryable and does not block embedded models or uploads',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1100, 850));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      var calls = 0;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: AssetLibraryDialog(
              directories: const [],
              embedded: const [
                LibraryModel(name: 'Saved sofa', digest: 'sha256:saved'),
              ],
              directoriesAvailable: false,
              onAttach: (_) => null,
              onReplace: (_, _) => null,
              onDiscover: () async {
                if (++calls == 1)
                  throw StateError('Synthetic offline condition');
                return const [
                  LibraryModel(name: 'Catalog table', digest: 'sha256:table'),
                ];
              },
              onUpload: () async => const [
                LibraryModel(name: 'Saved sofa', digest: 'sha256:saved'),
                LibraryModel(name: 'Uploaded chair', digest: 'sha256:uploaded'),
              ],
              onPreview: (_) async => png,
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Saved sofa'), findsOneWidget);
      expect(
        find.byKey(const ValueKey('retry-furniture-catalog')),
        findsOneWidget,
      );
      await tester.tap(find.byKey(const ValueKey('retry-furniture-catalog')));
      await tester.pumpAndSettle();
      expect(calls, 2);
      expect(find.text('Catalog table'), findsOneWidget);
      await tester.tap(find.byKey(const ValueKey('upload-asset-model')));
      await tester.pumpAndSettle();
      expect(find.text('Uploaded chair'), findsOneWidget);
      expect(find.text('Saved sofa'), findsOneWidget);
      expect(
        find.text('Catalog table'),
        findsOneWidget,
        reason: 'Uploading must not discard the hosted catalog',
      );
      expect(tester.takeException(), isNull);
    },
  );
}
