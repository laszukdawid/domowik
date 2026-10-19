import 'package:domowik_editor/asset_library.dart';
import 'package:domowik_editor/level_properties_form.dart';
import 'package:domowik_editor/level_state.dart';
import 'package:domowik_editor/measurement_units.dart';
import 'package:domowik_editor/opening_properties_form.dart';
import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/wall_properties_form.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const units = UnitSystem.imperial;

void main() {
  test('unit conversion, area and unchanged precision', () {
    expect(units.length(25.4), '1 in');
    expect(units.length(304.8), '12 in');
    expect(units.length(0), '0 in');
    expect(units.length(-0.00001), '0 in');
    expect(units.parseLength('-2.5'), -63.5);
    expect(units.parseLength('no'), isNull);
    expect(units.area(304.8 * 304.8), '1.00 ft²');
    expect(UnitSystem.metric.area(30245000, metricText: '30.24'), '30.24 m²');
    for (final mm in [0.00001, 123.456789, -1234.56789, 1000000.0]) {
      expect(units.parseLength(units.editLength(mm), originalMm: mm), mm);
    }
  });

  testWidgets('imperial wall fields keep unchanged values and convert edits', (
    tester,
  ) async {
    const wall = Wall(
      'wall',
      Offset.zero,
      Offset(1234.56789, 0),
      200.123,
      2500,
    );
    WallPropertiesChanges? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async {
              result = await WallPropertiesDialog.show(
                context,
                wall,
                units: units,
              );
            },
            child: const Text('Open'),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    expect(find.text('Length (in)'), findsOneWidget);
    await tester.tap(find.byKey(const ValueKey('wall-properties-apply')));
    await tester.pumpAndSettle();
    expect(result!.isEmpty, isTrue);
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('wall-lengthMm-field')),
      '100',
    );
    await tester.tap(find.byKey(const ValueKey('wall-properties-apply')));
    await tester.pumpAndSettle();
    expect(result!.lengthMm, 2540);
    expect(result!.thicknessMm, isNull);
    expect(result!.heightMm, isNull);
  });

  testWidgets('imperial level fields parse negative elevation', (tester) async {
    const level = LevelInfo(
      id: 'level',
      name: 'Floor',
      elevationMm: -123.456,
      floorThicknessMm: 200,
      defaultWallHeightMm: 2500,
      defaultCeilingHeightMm: 2600,
    );
    LevelPropertiesChanges? result;
    await tester.pumpWidget(
      MaterialApp(
        home: Builder(
          builder: (context) => TextButton(
            onPressed: () async {
              result = await LevelPropertiesDialog.show(
                context,
                level,
                units: units,
              );
            },
            child: const Text('Open'),
          ),
        ),
      ),
    );
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const ValueKey('level-properties-apply')));
    await tester.pumpAndSettle();
    expect(result!.isEmpty, isTrue);
    await tester.tap(find.text('Open'));
    await tester.pumpAndSettle();
    await tester.enterText(
      find.byKey(const ValueKey('level-elevation-field')),
      '-12',
    );
    await tester.tap(find.byKey(const ValueKey('level-properties-apply')));
    await tester.pumpAndSettle();
    expect(result!.elevationMm, closeTo(-304.8, 1e-10));
    expect(result!.floorThicknessMm, isNull);
  });

  testWidgets(
    'imperial opening fields convert sizes and reject non-finite input',
    (tester) async {
      const opening = Aperture(
        'opening',
        true,
        [],
        Offset.zero,
        Offset.zero,
        90,
        widthMm: 900.123,
        heightMm: 2100,
        sillMm: 0,
        depthMm: 200,
      );
      Map<String, double>? result;
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => TextButton(
              onPressed: () async {
                result = await OpeningPropertiesDialog.show(
                  context,
                  opening,
                  units: units,
                );
              },
              child: const Text('Open'),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
      await tester.enterText(
        find.byKey(const ValueKey('opening-widthMm-field')),
        'Infinity',
      );
      await tester.pump();
      expect(
        tester
            .widget<FilledButton>(
              find.byKey(const ValueKey('opening-properties-apply')),
            )
            .onPressed,
        isNull,
      );
      await tester.enterText(
        find.byKey(const ValueKey('opening-widthMm-field')),
        '36',
      );
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('opening-properties-apply')));
      await tester.pumpAndSettle();
      expect(result!['widthMm'], closeTo(914.4, 1e-10));
      expect(result!['heightMm'], 2100);
      expect(result!['sillMm'], 0);
    },
  );

  testWidgets('imperial live pose callbacks still receive mm and degrees', (
    tester,
  ) async {
    List<double>? preview;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SingleChildScrollView(
            child: AssetPoseControls(
              units: units,
              initial: const [0, 0, 0, 0, 0, 0],
              onPreview: (v) => preview = v,
              onApply: (_) {},
              onCancel: () {},
            ),
          ),
        ),
      ),
    );
    await tester.pumpAndSettle();
    await tester.enterText(find.byKey(const ValueKey('asset-pose-0')), '10');
    expect(preview![0], 254);
    await tester.enterText(find.byKey(const ValueKey('asset-pose-3')), '45');
    expect(preview![3], 45);
    await tester.tap(find.byTooltip('Increase 0.3937 in').first);
    expect(preview![0], 264);
  });

  testWidgets(
    'imperial furniture properties preserve precision and resize in mm',
    (tester) async {
      final asset = AssetFootprint.fromJson({
        'id': 'asset',
        'levelId': 'level',
        'digest': 'synthetic',
        'name': 'Box',
        'positionMm': [254.0, -254.0],
        'dimensionsMm': [25.4, 50.8],
        'rotationDeg': 45.0,
        'cornersMm': <Object?>[],
      });
      List<double>? properties;
      List<double>? size;
      await tester.pumpWidget(
        MaterialApp(
          home: Builder(
            builder: (context) => Column(
              children: [
                TextButton(
                  onPressed: () async {
                    properties = await showDialog<List<double>>(
                      context: context,
                      builder: (_) => AssetPropertiesDialog(
                        asset: asset,
                        heightMm: 123.456789,
                        units: units,
                      ),
                    );
                  },
                  child: const Text('Properties'),
                ),
                TextButton(
                  onPressed: () async {
                    size = await showDialog<List<double>>(
                      context: context,
                      builder: (_) => ResizeAssetDialog(
                        asset: asset,
                        heightMm: 76.2,
                        units: units,
                      ),
                    );
                  },
                  child: const Text('Resize'),
                ),
              ],
            ),
          ),
        ),
      );
      await tester.tap(find.text('Properties'));
      await tester.pumpAndSettle();
      expect(find.text('X (in)'), findsOneWidget);
      await tester.enterText(
        find.byKey(const ValueKey('asset-property-2')),
        '2',
      );
      await tester.pump();
      await tester.tap(find.byKey(const ValueKey('apply-asset-properties')));
      await tester.pumpAndSettle();
      expect(properties, [254, -254, 50.8, 50.8, 45, 123.456789]);
      await tester.tap(find.text('Resize'));
      await tester.pumpAndSettle();
      await tester.enterText(find.byKey(const ValueKey('resize-asset-0')), '2');
      await tester.pump();
      await tester.ensureVisible(
        find.byKey(const ValueKey('apply-asset-resize')),
      );
      await tester.tap(find.byKey(const ValueKey('apply-asset-resize')));
      await tester.pumpAndSettle();
      expect(size![0], closeTo(50.8, 1e-10));
      expect(size![1], closeTo(101.6, 1e-10));
      expect(size![2], closeTo(152.4, 1e-10));
    },
  );
}
