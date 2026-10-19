{{flutter_js}}
{{flutter_build_config}}
// Fonts the engine downloads load from the page's own origin, not Google
// Fonts (ADR 0012 §6). fallback-fonts/ holds only Roboto, placed by
// scripts/build-web-modules.sh; a glyph no bundled font has draws as a box.
// The base stays relative, so the browser resolves it against the page's own
// path, whatever characters that path encodes. CanvasKit fetches Roboto at
// fallback-fonts/...; skwasm (--wasm) fetches it through the asset manager,
// which adds assets/ in front, so the build script places it at both. (An
// absolute base breaks skwasm, which percent-encodes it again: %20 and %23 in
// the page's path would become %2520 and %2523.)
_flutter.loader.load({
  serviceWorkerSettings: {
    serviceWorkerVersion: {{flutter_service_worker_version}},
  },
  config: {
    fontFallbackBaseUrl: "fallback-fonts/",
  },
});
