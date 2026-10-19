import 'package:flutter/foundation.dart';

/// The text family the browser editor draws with (ADR 0012 §6), or null on
/// desktop, where the theme's default typography is left alone.
///
/// The web build serves Roboto 2.138 itself: scripts/build-web-modules.sh
/// places it where Flutter's web engine looks for Roboto under
/// `fontFallbackBaseUrl`, which web/flutter_bootstrap.js points at the page's
/// own `fallback-fonts/`. Every text style, including those a
/// [CustomPainter] builds without a theme, names the family explicitly: the
/// engine checks a style's characters from U+00A0 up only against the
/// families it names, so a style naming none treats them all as missing and
/// requests a Noto fallback font even when Roboto has the glyph. Glyphs no
/// named family has are requested from `fallback-fonts/`, which has no Noto
/// fonts, and are drawn as boxes.
const String? editorFontFamily = kIsWeb ? 'Roboto' : null;

/// The browser editor's fallback for characters Roboto lacks, or null on
/// desktop: a subset of Noto Sans Symbols 2 holding only ⌘, declared in
/// pubspec.yaml, for the macOS modifier name (BEHAVIOR-SPEC §11.2) and for
/// user text such as a room named with it. Named wherever
/// [editorFontFamily] is.
const List<String>? editorFontFallback = kIsWeb
    ? ['Noto Sans Symbols 2 Subset']
    : null;
