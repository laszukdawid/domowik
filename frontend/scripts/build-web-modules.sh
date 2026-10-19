#!/usr/bin/env bash
# Builds the two WebAssembly modules the browser editor loads (issue 0088,
# ADR 0012) and copies them into each directory given as an argument:
#
# - domowik_bridge.wasm: the Rust core, the same bridge crate the native
#   builds link, for wasm32-wasip1.
#
# Both use the `web` profile (frontend/Cargo.toml): release, stripped.
# - domowik_renderer.js + domowik_renderer_bg.wasm: the shared 3D renderer on
#   WebGPU, for wasm32-unknown-unknown, with wasm-bindgen glue.
#
# It also places the browser editor's text font (ADR 0012 §6). Flutter's web
# engine loads Roboto from `fontFallbackBaseUrl` + the path below
# (web/flutter_bootstrap.js sets the base to the page-relative fallback-fonts/),
# so the bundled Roboto 2.138 is copied to that path, unmodified, with its
# licence beside it. skwasm (--wasm) requests the same path under assets/,
# so it is copied there too; each renderer fetches only its own copy. The file is TrueType despite the engine's .woff2 name;
# CanvasKit reads either. test/editor_fonts_test.dart checks the path against
# the installed engine. Roboto is not a pubspec font, so native bundles, whose
# Linux theme names Roboto, keep the system font.
set -euo pipefail
here=$(cd -P "$(dirname "$0")" && pwd -P)
app=$(cd -P "$here/.." && pwd -P)
cd "$app"
roboto_dir=fallback-fonts/roboto/v32
roboto_file=KFOmCnqEu92Fr1Me4GZLCzYlKw.woff2

cargo build --profile web --locked -p domowik-frontend-bridge --target wasm32-wasip1
cargo build --profile web --locked -p domowik-web-renderer --target wasm32-unknown-unknown
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
"$here/wasm-bindgen.sh" --target web --no-typescript --out-name domowik_renderer \
  --out-dir "$out" target/wasm32-unknown-unknown/web/domowik_renderer.wasm
for dest in "$@"; do
  mkdir -p "$dest"
  cp target/wasm32-wasip1/web/domowik_bridge.wasm "$dest/"
  cp "$out/domowik_renderer.js" "$out/domowik_renderer_bg.wasm" "$dest/"
  for dir in "$roboto_dir" "assets/$roboto_dir"; do
    mkdir -p "$dest/$dir"
    cp assets/fonts/Roboto-Regular.ttf "$dest/$dir/$roboto_file"
    cp assets/fonts/Roboto-LICENSE.txt "$dest/$dir/LICENSE.txt"
  done
done
