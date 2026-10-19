import 'package:domowik_editor/measurement_units.dart';
import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/wall_properties_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const a = Wall('a', Offset.zero, Offset(5000, 0), 200, 2500);
  const b = Wall('b', Offset(0, 1000), Offset(5000, 1000), 120, 2800);
  final height = find.byKey(const ValueKey('wall-heightMm-field'));
  final thickness = find.byKey(const ValueKey('wall-thicknessMm-field'));
  final apply = find.byKey(const ValueKey('wall-properties-apply'));

  Future<void> open(
    WidgetTester tester,
    void Function(WallPropertiesChanges?) capture, {
    List<Wall> walls = const [a, b],
    UnitSystem units = UnitSystem.metric,
  }) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async => capture(
                await WallPropertiesDialog.showMultiple(
                  context,
                  walls,
                  units: units,
                ),
              ),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
  }

  testWidgets('mixed fields are blank, optional and have no length edit', (
    tester,
  ) async {
    WallPropertiesChanges? result;
    await open(tester, (value) => result = value);
    expect(find.text('Edit 2 walls'), findsOneWidget);
    expect(find.byKey(const ValueKey('wall-lengthMm-field')), findsNothing);
    for (final field in [height, thickness]) {
      final widget = tester.widget<TextField>(field);
      expect(widget.controller!.text, isEmpty);
      expect(widget.decoration!.hintText, 'Mixed');
      expect(widget.decoration!.errorText, isNull);
    }
    expect(tester.widget<FilledButton>(apply).onPressed, isNull);
    await tester.enterText(height, '2700');
    await tester.pumpAndSettle();
    await tester.tap(apply);
    await tester.pumpAndSettle();
    expect(result!.toJson(), {'heightMm': 2700.0});
  });

  testWidgets(
    'thickness-only and both-field updates leave no implicit values',
    (tester) async {
      WallPropertiesChanges? result;
      await open(tester, (value) => result = value);
      await tester.enterText(thickness, '180');
      await tester.pumpAndSettle();
      await tester.tap(apply);
      await tester.pumpAndSettle();
      expect(result!.toJson(), {'thicknessMm': 180.0});
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.enterText(thickness, '220');
      await tester.enterText(height, '3000');
      await tester.pumpAndSettle();
      await tester.tap(apply);
      await tester.pumpAndSettle();
      expect(result!.toJson(), {'thicknessMm': 220.0, 'heightMm': 3000.0});
    },
  );

  testWidgets('a common thickness is not submitted when mixed height changes', (
    tester,
  ) async {
    const c = Wall('c', Offset(0, 2000), Offset(5000, 2000), 200, 2800);
    WallPropertiesChanges? result;
    await open(tester, (value) => result = value, walls: const [a, c]);
    expect(tester.widget<TextField>(thickness).controller!.text, '200');
    expect(tester.widget<TextField>(height).decoration!.hintText, 'Mixed');
    await tester.enterText(height, '2500');
    await tester.pumpAndSettle();
    await tester.tap(apply);
    await tester.pumpAndSettle();
    expect(result!.toJson(), {'heightMm': 2500.0});
  });

  for (final units in UnitSystem.values) {
    testWidgets('common values stay unchanged in $units, clearing means keep', (
      tester,
    ) async {
      WallPropertiesChanges? result;
      await open(
        tester,
        (value) => result = value,
        walls: const [a, a],
        units: units,
      );
      expect(
        tester.widget<TextField>(height).controller!.text,
        units.editLength(2500),
      );
      expect(
        tester.widget<TextField>(thickness).controller!.text,
        units.editLength(200),
      );
      expect(tester.widget<FilledButton>(apply).onPressed, isNull);
      await tester.enterText(thickness, '');
      await tester.enterText(height, units.editLength(3000));
      await tester.pumpAndSettle();
      await tester.tap(apply);
      await tester.pumpAndSettle();
      expect(result!.thicknessMm, isNull);
      expect(result!.heightMm, isNotNull);
      expect(result!.lengthMm, isNull);
    });
  }

  for (final bad in ['0', '-1', 'NaN', 'Infinity', '1000001', 'abc']) {
    testWidgets('$bad disables Apply rather than skipping the invalid field', (
      tester,
    ) async {
      await open(tester, (_) {});
      await tester.enterText(height, '3000');
      await tester.enterText(thickness, bad);
      await tester.pumpAndSettle();
      expect(tester.widget<FilledButton>(apply).onPressed, isNull);
      expect(
        tester.widget<TextField>(thickness).decoration!.errorText,
        isNotNull,
      );
    });
  }

  for (final escape in [false, true]) {
    testWidgets('${escape ? 'Escape' : 'Cancel'} discards all changes', (
      tester,
    ) async {
      var completed = false;
      WallPropertiesChanges? result;
      await open(tester, (value) {
        completed = true;
        result = value;
      });
      await tester.enterText(height, '3000');
      if (escape) {
        await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      } else {
        await tester.tap(find.byKey(const ValueKey('wall-properties-cancel')));
      }
      await tester.pumpAndSettle();
      expect(completed, isTrue);
      expect(result, isNull);
    });
  }
}
