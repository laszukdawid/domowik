import 'package:domowik_spike_app/level_properties_form.dart';
import 'package:domowik_spike_app/level_state.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const level = LevelInfo(
    id: 'L1',
    name: 'Ground',
    elevationMm: 0,
    floorThicknessMm: 200,
    defaultWallHeightMm: 2500,
    defaultCeilingHeightMm: 2500,
  );

  testWidgets('fields are seeded from the current level snapshot', (
    tester,
  ) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () => LevelPropertiesDialog.show(context, level),
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(
      tester
          .widget<TextField>(find.byKey(const ValueKey('level-name-field')))
          .controller!
          .text,
      'Ground',
    );
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('level-elevation-field')),
          )
          .controller!
          .text,
      '0',
    );
    expect(
      tester
          .widget<TextField>(
            find.byKey(const ValueKey('level-floor-thickness-field')),
          )
          .controller!
          .text,
      '200',
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('Cancel returns null without submitting any change', (
    tester,
  ) async {
    LevelPropertiesChanges? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await LevelPropertiesDialog.show(context, level);
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
      find.byKey(const ValueKey('level-name-field')),
      'Renamed',
    );
    await tester.tap(find.byKey(const ValueKey('level-properties-cancel')));
    await tester.pumpAndSettle();
    expect(result, isNull);
  });

  testWidgets('an empty name is valid and does not block Apply', (
    tester,
  ) async {
    LevelPropertiesChanges? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await LevelPropertiesDialog.show(context, level);
              },
              child: const Text('open'),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    await tester.enterText(find.byKey(const ValueKey('level-name-field')), '');
    await tester.tap(find.byKey(const ValueKey('level-properties-apply')));
    await tester.pumpAndSettle();
    expect(result, isNotNull);
    expect(result!.name, '');
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'an out-of-bounds elevation disables Apply with an inline error',
    (tester) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                onPressed: () => LevelPropertiesDialog.show(context, level),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-elevation-field')),
        '20000000',
      );
      await tester.pumpAndSettle();
      final apply = tester.widget<FilledButton>(
        find.byKey(const ValueKey('level-properties-apply')),
      );
      expect(apply.onPressed, isNull, reason: 'out of ±10,000,000mm bounds');
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('level-elevation-field')),
            )
            .decoration!
            .errorText,
        isNotNull,
      );
    },
  );

  for (final bad in ['0', '-1']) {
    testWidgets('a non-positive wall height ($bad) disables Apply', (
      tester,
    ) async {
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                onPressed: () => LevelPropertiesDialog.show(context, level),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('level-wall-height-field')),
        bad,
      );
      await tester.pumpAndSettle();
      final apply = tester.widget<FilledButton>(
        find.byKey(const ValueKey('level-properties-apply')),
      );
      expect(apply.onPressed, isNull);
    });
  }

  testWidgets('only the changed field is reported', (tester) async {
    LevelPropertiesChanges? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await LevelPropertiesDialog.show(context, level);
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
      find.byKey(const ValueKey('level-elevation-field')),
      '3000',
    );
    await tester.tap(find.byKey(const ValueKey('level-properties-apply')));
    await tester.pumpAndSettle();
    expect(result!.elevationMm, 3000.0);
    expect(result!.name, isNull);
    expect(result!.floorThicknessMm, isNull);
    expect(result!.defaultWallHeightMm, isNull);
    expect(result!.defaultCeilingHeightMm, isNull);
    expect(result!.isEmpty, isFalse);
    expect(result!.toJson(), {'elevationMm': 3000.0});
  });

  testWidgets('narrow width does not overflow', (tester) async {
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            width: 320,
            child: Builder(
              builder: (context) => TextButton(
                onPressed: () => LevelPropertiesDialog.show(context, level),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets('a large text scale does not overflow', (tester) async {
    await tester.pumpWidget(
      MediaQuery(
        data: const MediaQueryData(textScaler: TextScaler.linear(3.0)),
        child: MaterialApp(
          home: Scaffold(
            body: Builder(
              builder: (context) => TextButton(
                onPressed: () => LevelPropertiesDialog.show(context, level),
                child: const Text('open'),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.tap(find.text('open'));
    await tester.pumpAndSettle();
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'a large text scale with an active field error still does not overflow',
    (tester) async {
      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(textScaler: TextScaler.linear(3.0)),
          child: MaterialApp(
            home: Scaffold(
              body: Builder(
                builder: (context) => TextButton(
                  onPressed: () => LevelPropertiesDialog.show(context, level),
                  child: const Text('open'),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('open'));
      await tester.pumpAndSettle();
      // A long label plus a long error string together are the combination
      // most likely to overflow a fixed-width field at 3x text scale.
      await tester.enterText(
        find.byKey(const ValueKey('level-floor-thickness-field')),
        '-1',
      );
      await tester.pumpAndSettle();
      expect(
        tester
            .widget<TextField>(
              find.byKey(const ValueKey('level-floor-thickness-field')),
            )
            .decoration!
            .errorText,
        isNotNull,
      );
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('Escape cancels the dialog like the Cancel button', (
    tester,
  ) async {
    LevelPropertiesChanges? result;
    var completed = false;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await LevelPropertiesDialog.show(context, level);
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
      find.byKey(const ValueKey('level-name-field')),
      'Renamed',
    );

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();

    expect(completed, isTrue, reason: 'Escape must resolve the dialog');
    expect(result, isNull, reason: 'Escape discards the edit, like Cancel');
  });
}
