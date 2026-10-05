import 'package:domowik_spike_app/warnings_banner.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> pump(
    WidgetTester tester,
    List<String> warnings, {
    double width = 800,
  }) => tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: SizedBox(
          width: width,
          child: Align(
            alignment: Alignment.topRight,
            child: WarningsBanner(warnings: warnings),
          ),
        ),
      ),
    ),
  );

  testWidgets('no warnings renders nothing', (tester) async {
    await pump(tester, const []);
    expect(find.byKey(const ValueKey('level-warnings-banner')), findsNothing);
  });

  testWidgets(
    'many warnings at a narrow width stay a single bounded summary line',
    (tester) async {
      final many = [
        for (var i = 0; i < 40; i++)
          'level $i: the upper level\'s slab intrudes below the preceding '
              'level\'s finished-floor plane',
      ];
      await pump(tester, many, width: 240);

      final banner = tester.getSize(
        find.byKey(const ValueKey('level-warnings-banner')),
      );
      // A bounded compact card, not one row per warning: with 40 entries a
      // per-row layout would be hundreds of logical pixels tall.
      expect(banner.height, lessThan(80));
      expect(tester.takeException(), isNull);

      final text = tester.widget<Text>(
        find
            .descendant(
              of: find.byKey(const ValueKey('level-warnings-summary')),
              matching: find.byType(Text),
            )
            .first,
      );
      expect(text.maxLines, 1);
      expect(text.overflow, TextOverflow.ellipsis);
      expect(text.data, contains('40 warnings'));
    },
  );

  testWidgets('tapping the summary opens a scrollable details dialog', (
    tester,
  ) async {
    final many = ['first warning', 'second warning', 'third warning'];
    await pump(tester, many);
    await tester.tap(find.byKey(const ValueKey('level-warnings-summary')));
    await tester.pumpAndSettle();

    expect(find.byKey(const ValueKey('level-warnings-list')), findsOneWidget);
    for (final warning in many) {
      expect(find.text(warning), findsOneWidget);
    }
    expect(tester.takeException(), isNull);

    await tester.tap(find.byKey(const ValueKey('level-warnings-close')));
    await tester.pumpAndSettle();
    expect(find.byKey(const ValueKey('level-warnings-list')), findsNothing);
  });

  testWidgets(
    'the details dialog stays a fixed size regardless of warning count',
    (tester) async {
      final few = ['only one warning'];
      final many = [for (var i = 0; i < 50; i++) 'warning number $i'];

      await pump(tester, few);
      await tester.tap(find.byKey(const ValueKey('level-warnings-summary')));
      await tester.pumpAndSettle();
      final smallSize = tester.getSize(find.byType(AlertDialog));
      await tester.tap(find.byKey(const ValueKey('level-warnings-close')));
      await tester.pumpAndSettle();

      await pump(tester, many);
      await tester.tap(find.byKey(const ValueKey('level-warnings-summary')));
      await tester.pumpAndSettle();
      final largeSize = tester.getSize(find.byType(AlertDialog));

      expect(largeSize.height, smallSize.height);
      expect(tester.takeException(), isNull);
    },
  );

  testWidgets('Escape dismisses the details dialog like Cancel', (
    tester,
  ) async {
    await pump(tester, const ['a warning']);
    await tester.tap(find.byKey(const ValueKey('level-warnings-summary')));
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsOneWidget);

    await tester.sendKeyEvent(LogicalKeyboardKey.escape);
    await tester.pumpAndSettle();
    expect(find.byType(AlertDialog), findsNothing);
  });
}
