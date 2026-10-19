import 'dart:async';
import 'dart:io';

import 'package:domowik_editor/asset_library.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const files = MethodChannel('invalid.domowik-spike/files');
  late Directory root;
  late String oldPath, newPath;
  late List<String> directories;
  late List<(String?, String)> changes;

  setUp(() {
    root = Directory.systemTemp.createTempSync('domowik-folder-picker-');
    final old = Directory('${root.path}/old')..createSync();
    final replacement = Directory('${root.path}/new')..createSync();
    oldPath = old.resolveSymbolicLinksSync();
    newPath = replacement.resolveSymbolicLinksSync();
    File('$oldPath/chair.glb').writeAsStringSync('original test model');
    File('$newPath/table.obj').writeAsStringSync('original test model');
    directories = [];
    changes = [];
  });
  tearDown(() => root.deleteSync(recursive: true));

  List<String>? update(String? old, String path) {
    if (!Directory(path).existsSync()) return null;
    changes.add((old, path));
    if (old != null) directories.remove(old);
    if (!directories.contains(path)) directories.add(path);
    return List.of(directories);
  }

  Future<void> start(WidgetTester tester) async {
    await tester.binding.setSurfaceSize(const Size(1000, 800));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    addTearDown(
      () => tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        files,
        null,
      ),
    );
    await tester.runAsync(() async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: AssetLibraryDialog(
              directories: List.of(directories),
              embedded: const [],
              onAttach: (path) => update(null, path),
              onReplace: update,
            ),
          ),
        ),
      );
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pumpAndSettle();
  }

  Future<void> tap(WidgetTester tester, Finder finder) async {
    await tester.pump();
    await tester.runAsync(() async {
      await tester.tap(finder);
      await tester.pump();
      await Future<void>.delayed(const Duration(milliseconds: 150));
    });
    await tester.pumpAndSettle();
  }

  testWidgets(
    'Add directory opens picker; pending and cancelled picks change nothing',
    (tester) async {
      final pending = Completer<String?>();
      var calls = 0;
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(files, (
        call,
      ) async {
        expect(call.method, 'pickAssetDirectory');
        expect(call.arguments, isNull);
        calls++;
        return pending.future;
      });
      await start(tester);
      expect(find.byKey(const ValueKey('asset-directory')), findsNothing);
      final add = find.byKey(const ValueKey('attach-asset-directory'));
      await tap(tester, add);
      expect(calls, 1);
      expect(tester.widget<OutlinedButton>(add).onPressed, isNull);
      expect(changes, isEmpty);
      pending.complete(null);
      await tester.pumpAndSettle();
      expect(changes, isEmpty);
      expect(directories, isEmpty);
      expect(find.textContaining('Could not'), findsNothing);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        files,
        (_) async => oldPath,
      );
      await tap(tester, add);
      expect(changes, [(null, oldPath)]);
      expect(find.text('chair'), findsOneWidget);
      await tap(tester, add);
      expect(directories, [
        oldPath,
      ], reason: 'duplicate picks retain one directory');
    },
  );

  testWidgets(
    'manual paths add and replace; invalid replacement keeps the old library',
    (tester) async {
      await start(tester);
      await tap(tester, find.byKey(const ValueKey('enter-asset-path')));
      await tester.enterText(
        find.byKey(const ValueKey('asset-directory')),
        oldPath,
      );
      await tap(tester, find.byKey(const ValueKey('apply-asset-path')));
      expect(directories, [oldPath]);
      await tap(tester, find.byKey(ValueKey('edit-directory:$oldPath')));
      expect(
        tester
            .widget<TextField>(find.byKey(const ValueKey('asset-directory')))
            .controller!
            .text,
        oldPath,
      );
      await tester.enterText(
        find.byKey(const ValueKey('asset-directory')),
        '${root.path}/missing',
      );
      await tap(tester, find.byKey(const ValueKey('apply-asset-path')));
      expect(directories, [oldPath]);
      expect(find.textContaining('Could not attach'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('asset-directory')),
        newPath,
      );
      await tap(tester, find.byKey(const ValueKey('apply-asset-path')));
      expect(directories, [newPath]);
      expect(changes.last, (oldPath, newPath));
      expect(find.text('table'), findsOneWidget);
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(files, (
        call,
      ) async {
        expect(
          call.arguments,
          newPath,
          reason: 'replacement picker starts at the current path',
        );
        return oldPath;
      });
      await tap(tester, find.byKey(ValueKey('browse-directory:$newPath')));
      expect(directories, [oldPath]);
      expect(changes.last, (newPath, oldPath));
    },
  );

  testWidgets(
    'picker errors retain manual entry; late picker response cannot attach after unmount',
    (tester) async {
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(files, (
        _,
      ) async {
        throw PlatformException(
          code: 'picker-error',
          message: 'Original test error',
        );
      });
      await start(tester);
      final add = find.byKey(const ValueKey('attach-asset-directory'));
      await tap(tester, add);
      expect(find.textContaining('Original test error'), findsOneWidget);
      expect(changes, isEmpty);
      await tap(tester, find.byKey(const ValueKey('enter-asset-path')));
      expect(find.byKey(const ValueKey('asset-directory')), findsOneWidget);
      final pending = Completer<String?>();
      tester.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        files,
        (_) async => pending.future,
      );
      await tap(tester, add);
      await tester.pumpWidget(const SizedBox.shrink());
      pending.complete(oldPath);
      await tester.pumpAndSettle();
      expect(changes, isEmpty);
      expect(tester.takeException(), isNull);
    },
  );
}
