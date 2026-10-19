#!/usr/bin/env bash
# Runs the browser editor's release probe against a built bundle (issue 0088).
#
# Serves frontend/build/web (build it first with scripts/build-web.sh), opens
# it in a pinned headless Chrome for Testing with ?DOMOWIK_WEB_PROBE=1, and
# reads the app's verdict from the browser's console log. The probe draws a
# room, undoes and redoes, saves to bytes and reopens them, uploads and places
# a model, and renders the plan in 3D on WebGPU (lib/main.dart _runWebProbe).
#
# The browser resolves no host but 127.0.0.1, so the probe also proves the
# page loads nothing from a third-party origin: a font the engine fails to
# load, or a font path the server answers 404, fails the probe (ADR 0012 §6).
#
# --base-href PATH serves the bundle under that URL path, which must match the
# build's own `build-web.sh --base-href PATH`. verify.sh probes the --wasm
# build under a path with percent-encoded characters, because skwasm encodes
# font URLs itself (web/flutter_bootstrap.js).
#
# Needs python3 (the static server) and a WebGPU adapter: the GPU on macOS,
# Chrome's software adapter on Linux (chrome-for-testing.sh --webgpu-flags). Prints the
# probe's lines and exits 0 only on "WEB PROBE PASSED".
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
app=$(cd -P "$here/.." && pwd -P)
bundle="$app/build/web"
base=/
case "${1:-}" in
  --base-href) base=${2:?--base-href needs a path}; shift 2 ;;
esac
case "$base" in /*/) ;; /) ;; *) echo "error: --base-href must start and end with /" >&2; exit 1 ;; esac
for f in index.html main.dart.js domowik_bridge.wasm domowik_renderer.js domowik_renderer_bg.wasm \
  fallback-fonts/roboto/v32/KFOmCnqEu92Fr1Me4GZLCzYlKw.woff2 \
  assets/fallback-fonts/roboto/v32/KFOmCnqEu92Fr1Me4GZLCzYlKw.woff2; do
  test -f "$bundle/$f" || { echo "error: $bundle/$f missing; run scripts/build-web.sh" >&2; exit 1; }
done
chrome=$("$here/chrome-for-testing.sh")
read -r -a webgpu <<<"$("$here/chrome-for-testing.sh" --webgpu-flags)"
# Ubuntu 24.04 restricts the unprivileged user namespaces Chrome's sandbox
# uses; Flutter's own headless launcher passes --no-sandbox for the same
# reason. This browser loads only the local build.
if [ "$(uname -s)" = Linux ]; then webgpu+=(--no-sandbox); fi
work=$(mktemp -d "${TMPDIR:-/tmp}/domowik-web-probe.XXXXXX")
server=""
browser=""
cleanup() {
  # Chrome and its helper processes keep writing the profile briefly after
  # the browser exits, so wait for it and retry the removal.
  [ -n "$browser" ] && { kill "$browser" 2>/dev/null; wait "$browser" 2>/dev/null; } || true
  [ -n "$server" ] && kill "$server" 2>/dev/null || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do
    rm -rf "$work" 2>/dev/null && break
    sleep 0.2
  done
}
trap cleanup EXIT

# Under a base path the server root is a directory holding the bundle at the
# decoded path, so /my%20editor/ is served from "<root>/my editor".
root=$bundle
if [ "$base" != / ]; then
  root="$work/site"
  dir=$(python3 -c 'import sys, urllib.parse; print(urllib.parse.unquote(sys.argv[1]).strip("/"))' "$base")
  mkdir -p "$root/$(dirname "$dir")"
  ln -s "$bundle" "$root/$dir"
fi
port=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
python3 -m http.server "$port" --bind 127.0.0.1 -d "$root" >"$work/server.log" 2>&1 &
server=$!
for _ in $(seq 1 50); do
  python3 -c "import urllib.request; urllib.request.urlopen('http://127.0.0.1:$port/')" 2>/dev/null && break
  sleep 0.1
done

"$chrome" --headless=new --use-mock-keychain --no-first-run --no-default-browser-check \
  --user-data-dir="$work/profile" --window-size=1400,900 \
  --host-resolver-rules="MAP * ~NOTFOUND, EXCLUDE 127.0.0.1" \
  --enable-logging=stderr --v=0 ${webgpu[@]+"${webgpu[@]}"} \
  "http://127.0.0.1:$port$base?DOMOWIK_WEB_PROBE=1" >"$work/chrome.log" 2>&1 &
browser=$!

verdict=""
for _ in $(seq 1 600); do
  if grep -q "WEB PROBE PASSED\|WEB PROBE FAILED" "$work/chrome.log" 2>/dev/null; then
    verdict=$(grep -o "WEB PROBE [A-Z]*[^\"]*" "$work/chrome.log" | tail -1)
    break
  fi
  sleep 0.1
done
sed -n 's/.*CONSOLE[^"]*"\(web probe.*\)", source:.*/\1/p' "$work/chrome.log" || true
if [ -z "$verdict" ]; then
  echo "WEB PROBE FAILED: no verdict within 60 s; console tail:" >&2
  grep CONSOLE "$work/chrome.log" | tail -20 >&2 || tail -20 "$work/chrome.log" >&2
  exit 1
fi
echo "$verdict"
# Engine warnings reach the console; the static server logs each request.
fonts=$(grep -o "Failed to load font[^\"]*\|Could not find a set of Noto fonts" "$work/chrome.log" || true)
missing=$(grep -i '"GET [^"]*font[^"]*" 404' "$work/server.log" || true)
if [ -n "$fonts$missing" ]; then
  echo "WEB PROBE FAILED: a font did not load from the bundle:" >&2
  printf '%s\n' "$fonts" "$missing" | sed '/^$/d' >&2
  exit 1
fi
case "$verdict" in "WEB PROBE PASSED"*) exit 0 ;; *) exit 1 ;; esac
