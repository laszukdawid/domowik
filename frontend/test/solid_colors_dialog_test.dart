import 'package:domowik_editor/solid_colors_dialog.dart';
import 'package:domowik_editor/asset_library.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> open(
    WidgetTester tester,
    void Function(Map<String, String>?) result,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result(
                  await showDialog<Map<String, String>>(
                    context: context,
                    builder: (_) => const SolidColorsDialog(
                      title: 'Colors',
                      materials: [
                        SolidColorMaterial('first', 'Cabinet', '#FFFFFF'),
                        SolidColorMaterial('second', 'Counter', '#777777'),
                      ],
                      colors: {'first': '#F2F0E8', 'unavailable': '#123456'},
                    ),
                  ),
                );
              },
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
  }

  testWidgets(
    'invalid hex blocks Apply; editing one material preserves the rest',
    (tester) async {
      Map<String, String>? result;
      await open(tester, (v) => result = v);
      final field = find.byKey(const ValueKey('solid-color-second'));
      await tester.enterText(field, '#bad');
      await tester.pump();
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('apply-solid-colors')),
            )
            .onPressed,
        isNull,
      );
      await tester.enterText(field, '#ab12cd');
      await tester.pump();
      await tester.tap(find.text('Apply'));
      await tester.pumpAndSettle();
      expect(result, {
        'first': '#F2F0E8',
        'second': '#AB12CD',
        'unavailable': '#123456',
      });
    },
  );

  testWidgets('blank resets one material; cancel produces no palette', (
    tester,
  ) async {
    Map<String, String>? result;
    await open(tester, (v) => result = v);
    await tester.enterText(find.byKey(const ValueKey('solid-color-first')), '');
    await tester.tap(find.text('Apply'));
    await tester.pumpAndSettle();
    expect(result, {'unavailable': '#123456'});
    result = null;
    await open(tester, (v) => result = v);
    await tester.tap(find.text('Cancel'));
    await tester.pumpAndSettle();
    expect(result, isNull);
  });

  test('preview cache identity includes a canonical per-instance palette', () {
    Map<String, Object?> data(Map<String, String> colors) => {
      'id': 'a',
      'levelId': 'l',
      'digest': 'model',
      'name': 'Model',
      'positionMm': [0, 0],
      'dimensionsMm': [100, 100],
      'rotationDeg': 0,
      'cornersMm': [],
      'materialColors': colors,
    };
    expect(AssetFootprint.fromJson(data({})).previewKey, 'model');
    expect(
      AssetFootprint.fromJson(data({'a': '#FFFFFF', 'b': '#000000'}))
          .previewKey,
      AssetFootprint.fromJson(data({'b': '#000000', 'a': '#FFFFFF'}))
          .previewKey,
    );
    expect(
      AssetFootprint.fromJson(data({'a': '#FFFFFF'})).previewKey,
      isNot(AssetFootprint.fromJson(data({'a': '#000000'})).previewKey),
    );
  });
}
