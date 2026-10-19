import 'package:domowik_editor/inspector_metric.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  Future<void> pump(
    WidgetTester tester, {
    required double width,
    double textScale = 1.0,
    TextDirection direction = TextDirection.ltr,
    required String label,
    required String value,
  }) => tester.pumpWidget(
    MediaQuery(
      data: MediaQueryData(textScaler: TextScaler.linear(textScale)),
      child: Directionality(
        textDirection: direction,
        child: Material(
          // The real call site is a row in a `ListView`/`Column`, which
          // gives this widget UNBOUNDED height — never a fixed viewport.
          // A bounded ancestor here would clamp `InspectorMetric`'s own
          // reported height at an arbitrary cutoff while `Wrap` kept
          // painting further wrapped lines past it, silently (unlike
          // `Row`/`Column`, `Wrap` throws no overflow exception). A
          // scroll view gives the same unbounded-height contract as the
          // real `ListView` ancestor, rather than papering over a
          // bounded-height test artifact with a taller fixed number.
          child: SingleChildScrollView(
            child: Align(
              alignment: Alignment.topLeft,
              child: SizedBox(
                width: width,
                child: InspectorMetric(label: label, value: value),
              ),
            ),
          ),
        ),
      ),
    ),
  );

  /// No layout-overflow exception, and every actual painted, NON-WHITESPACE
  /// text-run box (selecting whole runs preserves ligatures and surrogate
  /// pairs, while a wrapped line's
  /// trailing space — whose reported box can legitimately extend past the
  /// visible line without painting any ink — is never mistaken for
  /// escaping content) sits inside [InspectorMetric]'s own painted bounds,
  /// in one shared (global) coordinate space.
  void expectContained(WidgetTester tester) {
    expect(tester.takeException(), isNull);
    final parentBox = tester.renderObject<RenderBox>(
      find.byType(InspectorMetric),
    );
    final parentRect = parentBox.localToGlobal(Offset.zero) & parentBox.size;

    for (final element
        in find
            .descendant(
              of: find.byType(InspectorMetric),
              matching: find.byType(RichText),
            )
            .evaluate()) {
      final paragraph = element.renderObject! as RenderParagraph;
      final text = paragraph.text.toPlainText();
      if (text.isEmpty) continue;
      final origin = paragraph.localToGlobal(Offset.zero);

      final runs = RegExp(r'\S+', unicode: true).allMatches(text).toList();
      expect(runs, isNotEmpty);
      for (final run in runs) {
        final boxes = paragraph.getBoxesForSelection(
          TextSelection(baseOffset: run.start, extentOffset: run.end),
        );
        expect(boxes, isNotEmpty, reason: 'visible run ${run.group(0)}');
        for (final box in boxes) {
          final global = box.toRect().shift(origin);
          expect(global.left, greaterThanOrEqualTo(parentRect.left));
          expect(global.top, greaterThanOrEqualTo(parentRect.top));
          expect(
            global.right,
            lessThanOrEqualTo(parentRect.right),
            reason:
                '"${run.group(0)}" (index ${run.start} of "$text") box $global '
                'escapes parent $parentRect',
          );
          expect(global.bottom, lessThanOrEqualTo(parentRect.bottom));
        }
      }
    }
  }

  const longValue = '1234567890123456 outstanding buffers currently open';
  const nonAsciiRtl =
      'المخازن المؤقتة المعلقة'; // "outstanding buffers", Arabic
  const unbroken40 =
      'abcdefghijklmnopqrstuvwxyzABCDEFGHIJ0123'; // 40 chars, no spaces

  for (final width in [248.0, 120.0]) {
    testWidgets('label and value fit at ${width}px with no overflow', (
      tester,
    ) async {
      await pump(
        tester,
        width: width,
        label: 'Outstanding buffers',
        value: '0',
      );
      expectContained(tester);
    });
  }

  testWidgets('a long numeric value wraps instead of overflowing', (
    tester,
  ) async {
    await pump(
      tester,
      width: 248,
      label: 'Outstanding buffers',
      value: longValue,
    );
    expectContained(tester);
  });

  for (final scale in [2.0, 3.0]) {
    testWidgets('text scale ${scale}x at 248px does not overflow', (
      tester,
    ) async {
      await pump(
        tester,
        width: 248,
        textScale: scale,
        label: 'Outstanding buffers',
        value: '128',
      );
      expectContained(tester);
    });
  }

  testWidgets('120px width combined with 3x text scale does not overflow', (
    tester,
  ) async {
    await pump(
      tester,
      width: 120,
      textScale: 3.0,
      label: 'Outstanding buffers',
      value: '128',
    );
    expectContained(tester);
  });

  testWidgets(
    'a 40-character unbroken token in label or value does not overflow at 120px',
    (tester) async {
      await pump(tester, width: 120, label: unbroken40, value: '0');
      expectContained(tester);
      await pump(tester, width: 120, label: 'Value', value: unbroken40);
      expectContained(tester);
    },
  );

  testWidgets('RTL layout does not overflow', (tester) async {
    await pump(
      tester,
      width: 248,
      direction: TextDirection.rtl,
      label: 'Outstanding buffers',
      value: longValue,
    );
    expectContained(tester);
  });

  testWidgets('non-ASCII RTL content does not overflow', (tester) async {
    await pump(
      tester,
      width: 248,
      direction: TextDirection.rtl,
      label: nonAsciiRtl,
      value: '١٢٣٤٥٦٧٨٩٠', // Arabic-Indic digits, a long RTL numeric value
    );
    expectContained(tester);
  });

  testWidgets('the value keeps tabular figures; content is never elided', (
    tester,
  ) async {
    await pump(tester, width: 248, label: 'Outstanding buffers', value: '42');
    final text = tester.widget<Text>(find.text('42'));
    expect(
      text.style?.fontFeatures,
      contains(const FontFeature.tabularFigures()),
    );
    expect(text.maxLines, isNull);
    expect(text.overflow, isNot(TextOverflow.ellipsis));
    expect(find.text('Outstanding buffers'), findsOneWidget);
    expect(find.text('42'), findsOneWidget);
  });

  testWidgets('vertical padding is 4 logical pixels on each side', (
    tester,
  ) async {
    await pump(tester, width: 248, label: 'Label', value: '1');
    final padding = tester.widget<Padding>(
      find.descendant(
        of: find.byType(InspectorMetric),
        matching: find.byType(Padding),
      ),
    );
    expect(padding.padding, const EdgeInsets.symmetric(vertical: 4));
  });
}
