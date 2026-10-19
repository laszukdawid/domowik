import 'package:domowik_editor/asset_library.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

// Metadata-only fixtures: source paths are deliberately unavailable. Searching
// them must not fetch a file, rescan a directory or need a native bridge.
const _models = [
  LibraryModel(
    name: 'BILLY white',
    file: '/home/owner/assets/direct_import/billy-30616563.glb',
    sourceDirectory: '/home/owner/assets',
  ),
  LibraryModel(
    name: 'KALLAX black',
    digest: 'sha256:kallax',
    sourceDescription: r'C:\Furniture\Living room\kallax-black.glb',
  ),
  LibraryModel(
    name: 'Desk',
    digest: 'sha256:desk',
    sourceDescription: 'Collection/Office/standing.glb',
  ),
  LibraryModel(
    name: 'Bookcase',
    digest: 'sha256:bookcase',
    sourceDirectory: '/Libraries/Favourites',
  ),
  LibraryModel(name: 'Old chair', digest: 'sha256:chair'),
];

void main() {
  Future<void> start(
    WidgetTester tester, {
    List<LibraryModel> models = _models,
  }) async {
    await tester.binding.setSurfaceSize(const Size(1000, 850));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: AssetLibraryDialog(
            directories: const [],
            embedded: models,
            directoriesAvailable: false,
            onAttach: (_) =>
                throw StateError('search must not attach directories'),
            onReplace: (_, _) =>
                throw StateError('search must not replace directories'),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
  }

  for (final (query, name) in [
    ('BiLlY', 'BILLY white'),
    ('DIRECT_IMPORT', 'BILLY white'),
    ('/home/owner/assets/direct_import/billy-30616563.glb', 'BILLY white'),
    ('30616563.glb', 'BILLY white'),
    ('Living ROOM', 'KALLAX black'),
    (r'C:\FURNITURE\Living room', 'KALLAX black'),
    ('c:/furniture/living room', 'KALLAX black'),
    ('collection/office', 'Desk'),
    (r'Collection\Office', 'Desk'),
    ('libraries/favourites', 'Bookcase'),
    ('old CHAIR', 'Old chair'),
  ]) {
    testWidgets('asset search finds $name by $query', (tester) async {
      await start(tester);
      await tester.enterText(find.byKey(const ValueKey('asset-search')), query);
      await tester.pumpAndSettle();
      expect(find.byType(ListTile), findsOneWidget);
      expect(find.text(name), findsOneWidget);
      expect(find.text('Search by name or path'), findsOneWidget);
      expect(tester.takeException(), isNull);
    });
  }

  testWidgets(
    'clearing a no-match search restores entries without provenance',
    (tester) async {
      await start(tester);
      final search = find.byKey(const ValueKey('asset-search'));
      // A literal substring, not a regex that would match any name.
      await tester.enterText(search, '.*');
      await tester.pumpAndSettle();
      expect(find.byType(ListTile), findsNothing);
      expect(find.text('No models match your search.'), findsOneWidget);
      expect(
        find.textContaining('Upload a model to get started'),
        findsNothing,
      );
      await tester.enterText(search, '');
      await tester.pumpAndSettle();
      expect(find.byType(ListTile), findsAtLeastNWidgets(1));
      // Rows are now lazy thumbnail subscribers, not a prebuilt whole library.
      await tester.scrollUntilVisible(
        find.text('Old chair'),
        100,
        scrollable: find.descendant(
          of: find.byKey(const ValueKey('asset-list')),
          matching: find.byType(Scrollable),
        ),
      );
      await tester.pumpAndSettle();
      expect(find.text('Old chair'), findsOneWidget);
      expect(find.text('Embedded in this plan'), findsNWidgets(2));
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('an empty library still offers its existing import guidance', (
    tester,
  ) async {
    await start(tester, models: const []);
    expect(
      find.text('No models yet. Upload a model to get started.'),
      findsOneWidget,
    );
    expect(find.text('No models match your search.'), findsNothing);
  });
}
