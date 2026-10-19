#!/usr/bin/env bash
# Builds the browser editor (issue 0088, ADR 0012) into build/web.
#
# 1. The core and 3D renderer modules (scripts/build-web-modules.sh), copied
#    into web/ (generated, ignored) and the build output.
# 2. The Flutter web application, with CanvasKit served from the build
#    itself (--no-web-resources-cdn) and the bundled Roboto as its text
#    font. web/flutter_bootstrap.js points fallback fonts at the page's own
#    origin, so the page contacts no other (ADR 0012 §6).
#
# Extra arguments go to `flutter build web` (for example --wasm).
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
app=$(cd -P "$here/.." && pwd -P)
cd "$app"

for tool in flutter cargo; do
  command -v "$tool" >/dev/null 2>&1 || { echo "error: $tool is not on PATH" >&2; exit 1; }
done

# web/ keeps a copy for `flutter run -d chrome`.
"$here/build-web-modules.sh" web
flutter build web --release --no-web-resources-cdn "$@"
# `flutter build web` does not carry .wasm files over from web/, so the
# modules are placed beside the page here.
"$here/build-web-modules.sh" build/web
echo "browser editor built: $app/build/web"
