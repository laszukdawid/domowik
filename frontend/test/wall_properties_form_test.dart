import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/wall_properties_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const wall = Wall('w1', Offset.zero, Offset(5000, 0), 200, 2500);

  Future<void> open(
    WidgetTester tester,
    void Function(WallPropertiesChanges?) capture,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async =>
                  capture(await WallPropertiesDialog.show(context, wall)),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
  }

  testWidgets('fields are seeded from the current wall snapshot', (
    tester,
  ) async {
    await open(tester, (_) {});
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('wall-lengthMm-field')))
          .controller!
          .text,
      '5000',
    );
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('wall-thicknessMm-field')),
          )
          .controller!
          .text,
      '200',
    );
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('wall-heightMm-field')))
          .controller!
          .text,
      '2500',
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('Cancel returns null without submitting any change', (
    tester,
  ) async {
    WallPropertiesChanges? result;
    await open(tester, (value) => result = value);
    await tester.enterText(
      find.byKey(const ValueKey('wall-heightMm-field')),
      '2700',
    );
    await tester.tap(find.byKey(const ValueKey('wall-properties-cancel')));
    await tester.pumpAndSettle();
    expect(result, isNull);
  });

  testWidgets('only the changed field is reported', (tester) async {
    WallPropertiesChanges? result;
    await open(tester, (value) => result = value);
    await tester.enterText(
      find.byKey(const ValueKey('wall-heightMm-field')),
      '2700',
    );
    await tester.tap(find.byKey(const ValueKey('wall-properties-apply')));
    await tester.pumpAndSettle();
    expect(result, isNotNull);
    expect(result!.lengthMm, isNull);
    expect(result!.thicknessMm, isNull);
    expect(result!.heightMm, 2700.0);
    expect(result!.isEmpty, isFalse);
    expect(result!.toJson(), {'heightMm': 2700.0});
  });

  for (final field in [
    'wall-lengthMm-field',
    'wall-thicknessMm-field',
    'wall-heightMm-field',
  ]) {
    for (final bad in ['0', '-1', '']) {
      final shown = bad.isEmpty ? 'blank' : bad;
      testWidgets('$shown in $field disables Apply with an inline error', (
        tester,
      ) async {
        await open(tester, (_) {});
        await tester.enterText(find.byKey(ValueKey(field)), bad);
        await tester.pumpAndSettle();
        final apply = tester.widget<FilledButton>(
          find.byKey(const ValueKey('wall-properties-apply')),
        );
        expect(apply.onPressed, isNull);
        expect(
          tester
              .widget<TextField>(find.byKey(ValueKey(field)))
              .decoration!
              .errorText,
          isNotNull,
        );
      });
    }
  }

  testWidgets('Escape cancels the dialog like the Cancel button', (
    tester,
  ) async {
    WallPropertiesChanges? result;
    var completed = false;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await WallPropertiesDialog.show(context, wall);
                completed = true;
              },
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('wall-heightMm-field')),
      '2700',
    );
    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(completed, isTrue, reason: 'Escape must resolve the dialog');
    expect(result, isNull, reason: 'Escape discards the edit, like Cancel');
  });

  testWidgets(
    'a large text scale with an active field error does not overflow',
    (tester) async {
      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(textScaler: TextScaler.linear(3.0)),
          child: MaterialApp(
            home: Scaffold(
              body: Builder(
                builder: (context) => TextButton(
                  onPressed: () => WallPropertiesDialog.show(context, wall),
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('wall-thicknessMm-field')),
        '-1',
      );
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('wall-thicknessMm-field')),
            )
            .decoration!
            .errorText,
        isNotNull,
      );
      expect(tester.takeException(), isNull);
    },
  );
}
