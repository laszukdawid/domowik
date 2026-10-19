// The browser editor draws its text with the bundled Roboto and Noto Sans
// Symbols 2 subset and loads no other font (ADR 0012 §6), so a character the
// editor's own source uses and neither has would draw as a box. Coverage alone
// does not prove a style names the covering font; the release probe draws
// these characters through both a painter and a widget for that. The release probe catches that only
// for text it happens to draw; this test checks every character in lib/.
import 'dart:io';
import 'dart:typed_data';

import 'package:domowik_editor/editor_fonts.dart';
import 'package:flutter_test/flutter_test.dart';

/// The code points an OpenType font's Unicode cmap maps to a glyph, read from
/// the format 4 (BMP) and format 12 (full range) subtables.
Set<int> _cmap(Uint8List bytes) {
  final data = ByteData.sublistView(bytes);
  final tables = data.getUint16(4);
  int? cmap;
  for (var i = 0; i < tables; i++) {
    final record = 12 + 16 * i;
    if (String.fromCharCodes(bytes, record, record + 4) == 'cmap') {
      cmap = data.getUint32(record + 8);
    }
  }
  if (cmap == null) throw const FormatException('no cmap table');
  final covered = <int>{};
  final subtables = data.getUint16(cmap + 2);
  for (var i = 0; i < subtables; i++) {
    final at = cmap + data.getUint32(cmap + 4 + 8 * i + 4);
    switch (data.getUint16(at)) {
      case 4:
        final segments = data.getUint16(at + 6) ~/ 2;
        final ends = at + 14;
        final starts = ends + 2 * segments + 2;
        final deltas = starts + 2 * segments;
        final offsets = deltas + 2 * segments;
        for (var s = 0; s < segments; s++) {
          final end = data.getUint16(ends + 2 * s);
          final start = data.getUint16(starts + 2 * s);
          final delta = data.getUint16(deltas + 2 * s);
          final offset = data.getUint16(offsets + 2 * s);
          for (var c = start; c <= end && c != 0xFFFF; c++) {
            var glyph = 0;
            if (offset == 0) {
              glyph = (c + delta) & 0xFFFF;
            } else {
              final index = offsets + 2 * s + offset + 2 * (c - start);
              final raw = data.getUint16(index);
              if (raw != 0) glyph = (raw + delta) & 0xFFFF;
            }
            if (glyph != 0) covered.add(c);
          }
        }
      case 12:
        final groups = data.getUint32(at + 12);
        for (var g = 0; g < groups; g++) {
          final group = at + 16 + 12 * g;
          final start = data.getUint32(group);
          final end = data.getUint32(group + 4);
          if (data.getUint32(group + 8) == 0) continue;
          for (var c = start; c <= end; c++) {
            covered.add(c);
          }
        }
    }
  }
  return covered;
}

void main() {
  test('desktop builds name no family and no fallback', () {
    expect(editorFontFamily, isNull);
    expect(editorFontFallback, isNull);
  });

  test('pubspec declares only the symbols subset, with its licence', () {
    final pubspec = File('pubspec.yaml').readAsStringSync();
    expect(
      pubspec,
      contains(
        '    - family: Noto Sans Symbols 2 Subset\n'
        '      fonts:\n'
        '        - asset: assets/fonts/NotoSansSymbols2-Subset.ttf\n'
        '  licenses:\n'
        '    - assets/fonts/NotoSansSymbols2-NOTICES.txt',
      ),
    );
    // A native bundle registering a family named Roboto would change Linux
    // typography, whose default theme names Roboto.
    expect(pubspec, isNot(contains('family: Roboto')));
    // Flutter's NOTICES entry format: package name, a blank line, the text.
    expect(
      File('assets/fonts/NotoSansSymbols2-NOTICES.txt').readAsStringSync(),
      'Noto Sans Symbols 2 Subset\n\n'
      '${File('assets/fonts/NotoSansSymbols2-OFL.txt').readAsStringSync()}',
    );
  });

  test('Roboto is placed where the installed web engine loads it from', () {
    final root = Platform.environment['FLUTTER_ROOT'];
    final engine = File(
      '$root/bin/cache/flutter_web_sdk/lib/_engine/engine/canvaskit/fonts.dart',
    );
    if (root == null || !engine.existsSync()) {
      markTestSkipped('the Flutter web SDK is not installed');
      return;
    }
    final url = RegExp(
      r"_robotoUrl =\s*'\$\{configuration\.fontFallbackBaseUrl\}([^']+)'",
    ).firstMatch(engine.readAsStringSync());
    expect(url, isNotNull, reason: 'the engine no longer loads Roboto so');
    final path = url!.group(1)!;
    final slash = path.lastIndexOf('/');
    final modules = File('scripts/build-web-modules.sh').readAsStringSync();
    expect(
      modules,
      contains('roboto_dir=fallback-fonts/${path.substring(0, slash)}\n'),
    );
    expect(modules, contains('roboto_file=${path.substring(slash + 1)}\n'));
    // CanvasKit fetches the path as given; skwasm under assets/.
    final probe = File('scripts/web-probe.sh').readAsStringSync();
    expect(probe, contains('  fallback-fonts/$path'));
    expect(probe, contains('  assets/fallback-fonts/$path'));
    expect(modules, contains(r'for dir in "$roboto_dir" "assets/$roboto_dir"'));
    expect(
      File('web/flutter_bootstrap.js').readAsStringSync(),
      contains('fontFallbackBaseUrl: "fallback-fonts/",'),
    );
  });

  test('the bundled fonts have every character the editor source uses', () {
    final roboto = _cmap(
      File('assets/fonts/Roboto-Regular.ttf').readAsBytesSync(),
    );
    final symbols = _cmap(
      File('assets/fonts/NotoSansSymbols2-Subset.ttf').readAsBytesSync(),
    );
    // A check of the reader itself: Polish letters, Greek and Cyrillic are in
    // Roboto, ⌘ is not, and the subset holds ⌘ alone.
    expect(roboto, containsAll('AzĄłŻΩЖ²—…'.runes));
    expect(roboto, isNot(contains(0x2318)));
    expect(symbols, {0x2318});
    final covered = {...roboto, ...symbols};

    final missing = <String>[];
    for (final file
        in Directory('lib')
            .listSync(recursive: true)
            .whereType<File>()
            .where((f) => f.path.endsWith('.dart'))) {
      final lines = file.readAsLinesSync();
      for (var i = 0; i < lines.length; i++) {
        for (final rune in lines[i].runes) {
          if (rune < 0x80 || covered.contains(rune)) continue;
          missing.add(
            '${file.path}:${i + 1}: U+${rune.toRadixString(16).toUpperCase()}',
          );
        }
      }
    }
    expect(missing, isEmpty);
  });
}
