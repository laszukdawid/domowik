import 'package:domowik_editor/live_property_edits.dart';
import 'package:domowik_editor/measurement_units.dart';
import 'package:domowik_editor/staircase_properties_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const properties = <String, Object?>{
  'widthMm': 900.0,
  'risers': 15,
  'guardHeightMm': 1000.0,
  'startElevationMm': 0.0,
  'endElevationMm': -2600.0,
};
Finder field(String key) => find.byKey(PageStorageKey('stair-$key'));
Future<void> settleEdit(WidgetTester tester) async {
  await tester.pump(StaircasePropertiesForm.editDelay);
  await tester.pump();
}

Future<void> mount(
  WidgetTester tester,
  bool Function(Map<String, Object?>) onChanged, {
  LivePropertyEdits? liveEdits,
  UnitSystem units = UnitSystem.metric,
}) => tester.pumpWidget(
  MaterialApp(
    home: Scaffold(
      body: SingleChildScrollView(
        child: StaircasePropertiesForm(
          name: 'Test stairs',
          properties: properties,
          onChanged: onChanged,
          liveEdits: liveEdits,
          units: units,
        ),
      ),
    ),
  ),
);

void main() {
  for (final units in UnitSystem.values) {
    testWidgets(
      'rounded ${units.name} elevation echoes preserve pending typing',
      (tester) async {
        var current = <String, Object?>{
          ...properties,
          'endElevationMm': -6000.0,
        };
        var name = 'Test stairs', revision = 0;
        final calls = <Map<String, Object?>>[];
        late StateSetter rebuild;
        await tester.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: StatefulBuilder(
                builder: (context, setState) {
                  rebuild = setState;
                  return StaircasePropertiesForm(
                    name: name,
                    properties: current,
                    units: units,
                    revision: revision,
                    onChanged: (changes) {
                      calls.add(changes);
                      return true;
                    },
                  );
                },
              ),
            ),
          ),
        );
        final text = units == UnitSystem.metric ? '-4095.9' : '-161.2558';
        await tester.enterText(field('startElevationMm'), text);
        await settleEdit(tester);
        final typed = calls.single['startElevationMm'] as double;
        final echo = (typed - 2600.0) + 2600.0;
        expect(
          echo,
          isNot(typed),
          reason: 'exercise a non-exact datum round trip',
        );
        await tester.enterText(field('startElevationMm'), '${text}1');
        final controller = tester
            .widget<TextFormField>(field('startElevationMm'))
            .controller!;
        controller.selection = const TextSelection.collapsed(offset: 3);
        rebuild(() {
          current = {...current, 'startElevationMm': echo};
          revision++;
        });
        await tester.pump();
        expect(controller.text, '${text}1');
        expect(controller.selection.baseOffset, 3);
        await settleEdit(tester);
        expect(calls, hasLength(2));
        final next = calls.last['startElevationMm'] as double;
        rebuild(() {
          current = {...current, 'startElevationMm': (next - 2600.0) + 2600.0};
          revision++;
        });
        await tester.pump();
        await tester.testTextInput.receiveAction(TextInputAction.done);
        await settleEdit(tester);
        expect(
          calls,
          hasLength(2),
          reason: 'rounding must not add another undo batch',
        );
        rebuild(() {
          name = 'External edit';
          revision++;
        });
        await tester.pump();
        expect(
          tester.widget<TextFormField>(field('name')).controller!.text,
          'External edit',
        );
      },
    );
  }

  testWidgets('typing coalesces to one live edit without an Apply button', (
    tester,
  ) async {
    final calls = <Map<String, Object?>>[];
    await mount(tester, (changes) {
      calls.add(changes);
      return true;
    });
    expect(find.text('Apply stair changes'), findsNothing);
    expect(find.byType(FilledButton), findsNothing);
    await tester.enterText(field('risers'), '1');
    await tester.pump(const Duration(milliseconds: 40));
    expect(find.text('Use 2–256 whole risers'), findsOneWidget);
    await tester.enterText(field('risers'), '16');
    await tester.pump(const Duration(milliseconds: 40));
    await tester.enterText(field('risers'), '160');
    await tester.pump(const Duration(milliseconds: 119));
    expect(calls, isEmpty);
    await tester.pump(const Duration(milliseconds: 1));
    expect(calls, [
      {'risers': 160},
    ]);
    await tester.pump(const Duration(seconds: 1));
    expect(calls, hasLength(1));
  });

  testWidgets('Enter and focus loss flush once without waiting', (
    tester,
  ) async {
    final calls = <Map<String, Object?>>[];
    await mount(tester, (changes) {
      calls.add(changes);
      return true;
    });
    await tester.enterText(field('name'), 'Garden steps');
    await tester.testTextInput.receiveAction(TextInputAction.done);
    await tester.pump();
    expect(calls, [
      {'name': 'Garden steps'},
    ]);
    await tester.enterText(field('startElevationMm'), '-150');
    await tester.tap(field('endElevationMm'));
    await tester.pump();
    expect(calls.last, {'startElevationMm': -150.0});
    await settleEdit(tester);
    expect(calls, hasLength(2));
  });

  testWidgets(
    'invalid fields stay local without blocking valid unrelated edits',
    (tester) async {
      final calls = <Map<String, Object?>>[];
      await mount(tester, (changes) {
        calls.add(changes);
        return true;
      });
      await tester.enterText(field('risers'), '15.0');
      await settleEdit(tester);
      expect(find.text('Use 2–256 whole risers'), findsOneWidget);
      expect(calls, isEmpty);
      await tester.enterText(field('name'), 'Garden');
      await settleEdit(tester);
      expect(calls, [
        {'name': 'Garden'},
      ]);
      await tester.enterText(field('startElevationMm'), '-3000');
      await settleEdit(tester);
      expect(find.text('Start must be above end'), findsWidgets);
      expect(calls, hasLength(1));
      await tester.enterText(field('startElevationMm'), '-150');
      await settleEdit(tester);
      expect(calls.last, {'startElevationMm': -150.0});
      await tester.enterText(field('widthMm'), 'NaN');
      await settleEdit(tester);
      expect(calls, hasLength(2));
    },
  );

  testWidgets(
    'core refusals show inline errors and do not retry unchanged text',
    (tester) async {
      var calls = 0;
      final edits = LivePropertyEdits();
      await mount(tester, (_) {
        calls++;
        return false;
      }, liveEdits: edits);
      await tester.enterText(field('widthMm'), '1200');
      await settleEdit(tester);
      expect(calls, 1);
      expect(find.text('Value refused; see status'), findsOneWidget);
      edits.flush();
      await settleEdit(tester);
      expect(calls, 1);
      await tester.enterText(field('widthMm'), '1000');
      await settleEdit(tester);
      expect(calls, 2);
    },
  );

  testWidgets(
    'acknowledgements retain focus and caret; history cancels pending edits',
    (tester) async {
      var name = 'Test stairs', revision = 0, calls = 0;
      late StateSetter rebuild;
      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: StatefulBuilder(
              builder: (context, setState) {
                rebuild = setState;
                return StaircasePropertiesForm(
                  name: name,
                  properties: properties,
                  revision: revision,
                  onChanged: (changes) {
                    calls++;
                    rebuild(() {
                      name = changes['name'] as String;
                      revision++;
                    });
                    return true;
                  },
                );
              },
            ),
          ),
        ),
      );
      await tester.enterText(field('name'), 'Garden');
      final controller = tester
          .widget<TextFormField>(field('name'))
          .controller!;
      controller.selection = const TextSelection.collapsed(offset: 3);
      await settleEdit(tester);
      expect(calls, 1);
      final input = tester.widget<TextFormField>(field('name'));
      expect(input.controller, same(controller));
      expect(
        tester
            .widget<TextField>(
              find.descendant(
                of: field('name'),
                matching: find.byType(TextField),
              ),
            )
            .focusNode!
            .hasFocus,
        isTrue,
      );
      expect(controller.selection.baseOffset, 3);
      await tester.enterText(field('name'), 'Pending');
      rebuild(() {
        name = 'Test stairs';
        revision++;
      });
      await tester.pump();
      await settleEdit(tester);
      expect(controller.text, 'Test stairs');
      expect(calls, 1);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('cancellation and disposal never emit delayed edits', (
    tester,
  ) async {
    var calls = 0;
    final edits = LivePropertyEdits();
    await mount(tester, (_) {
      calls++;
      return true;
    }, liveEdits: edits);
    await tester.enterText(field('name'), 'Cancelled');
    edits.cancel();
    await settleEdit(tester);
    edits.flush();
    expect(calls, 0);
    await tester.enterText(field('name'), 'Unmounted');
    await tester.pumpWidget(const SizedBox.shrink());
    await settleEdit(tester);
    edits.flush();
    expect(calls, 0);
    expect(tester.takeException(), isNull);
  });

  testWidgets(
    'unit formatting preserves unchanged values without conversion drift',
    (tester) async {
      final calls = <Map<String, Object?>>[];
      await mount(tester, (changes) {
        calls.add(changes);
        return true;
      }, units: UnitSystem.imperial);
      final original = tester
          .widget<TextFormField>(field('widthMm'))
          .controller!
          .text;
      await tester.enterText(field('widthMm'), original);
      await settleEdit(tester);
      expect(calls, isEmpty);
      await tester.enterText(field('widthMm'), '40');
      await settleEdit(tester);
      expect(calls, [
        {'widthMm': 1016.0},
      ]);
      expect(
        tester
            .widget<TextField>(
              find.descendant(
                of: field('widthMm'),
                matching: find.byType(TextField),
              ),
            )
            .decoration!
            .suffixText,
        'in',
      );
    },
  );

  testWidgets('IME composition and caret-only movement do not submit work', (
    tester,
  ) async {
    final calls = <Map<String, Object?>>[];
    await mount(tester, (changes) {
      calls.add(changes);
      return true;
    });
    final c = tester.widget<TextFormField>(field('name')).controller!;
    c.value = const TextEditingValue(
      text: 'Garden',
      composing: TextRange(start: 0, end: 6),
    );
    await settleEdit(tester);
    expect(calls, isEmpty);
    c.value = c.value.copyWith(composing: TextRange.empty);
    await settleEdit(tester);
    expect(calls, [
      {'name': 'Garden'},
    ]);
    c.selection = const TextSelection.collapsed(offset: 2);
    await settleEdit(tester);
    expect(calls, hasLength(1));
  });

  testWidgets('compact fields remain usable in a narrow inspector', (
    tester,
  ) async {
    await tester.binding.setSurfaceSize(const Size(240, 750));
    addTearDown(() => tester.binding.setSurfaceSize(null));
    await mount(tester, (_) => true);
    expect(find.text('Name'), findsOneWidget);
    expect(find.text('Start elevation'), findsOneWidget);
    expect(tester.takeException(), isNull);
  });
}
