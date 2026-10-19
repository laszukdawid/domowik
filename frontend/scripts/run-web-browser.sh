#!/usr/bin/env bash
# Linux Intel/NVIDIA workaround, validated with Chromium 155 (issue 0105).
# The only backend is a loopback static-file server; document work stays in WASM.
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
bundle="$here/../build/web"
chrome=${DOMOWIK_WEB_BROWSER:-chromium}
port=${DOMOWIK_WEB_PORT:-8080}
profile=${DOMOWIK_WEB_PROFILE:-"$HOME/snap/chromium/common/domowik-webgpu"}

fail() { echo "error: $*" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || fail "this Intel/Vulkan/X11 launcher is for Linux; use task run:gui:web elsewhere"
[ -n "${DISPLAY:-}" ] || fail "DISPLAY is unset; this launcher needs X11 or XWayland"
for tool in python3 "$chrome"; do
  command -v "$tool" >/dev/null 2>&1 || fail "$tool not found on PATH"
done
for file in index.html domowik_bridge.wasm domowik_renderer.js domowik_renderer_bg.wasm; do
  [ -f "$bundle/$file" ] || fail "$bundle/$file missing; run task run:gui:web-browser to build first"
done
python3 -c 'import sys; p=int(sys.argv[1]); assert 0 <= p <= 65535' "$port" \
  2>/dev/null || fail "DOMOWIK_WEB_PORT must be an integer from 0 to 65535 (0 selects a free port)"
mkdir -p "$profile"
profile=$(cd -P "$profile" && pwd -P)
# Otherwise Chromium forwards the URL to an existing process and ignores the
# new GPU flags, then exits immediately, leaving no browser for us to supervise.
if [ -e "$profile/SingletonLock" ] || [ -L "$profile/SingletonLock" ]; then
  fail "browser profile already locked: $profile; close that profile's browser or choose another DOMOWIK_WEB_PROFILE"
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/domowik-web-browser.XXXXXX")
server=''
browser=''
cleanup() {
  trap - EXIT INT TERM
  if [ -n "$browser" ]; then
    kill "$browser" 2>/dev/null || true
    wait "$browser" 2>/dev/null || true
  fi
  if [ -n "$server" ]; then
    kill "$server" 2>/dev/null || true
    wait "$server" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# Bind before publishing the ready file: an unrelated server on the chosen
# port must never be mistaken for this launcher's successfully started server.
python3 - "$bundle" "$port" "$work/ready" >"$work/server.log" 2>&1 <<'PY' &
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import sys

handler = partial(SimpleHTTPRequestHandler, directory=sys.argv[1])
with ThreadingHTTPServer(('127.0.0.1', int(sys.argv[2])), handler) as server:
    Path(sys.argv[3]).write_text(f'http://127.0.0.1:{server.server_port}/')
    server.serve_forever()
PY
server=$!
for ((i=0; i<100; i++)); do
  [ -s "$work/ready" ] && break
  if ! kill -0 "$server" 2>/dev/null; then
    tail -20 "$work/server.log" >&2
    fail "could not start localhost server; stop the existing server or set DOMOWIK_WEB_PORT=8081"
  fi
  sleep 0.1
done
[ -s "$work/ready" ] || fail "localhost server did not become ready within 10 seconds"
url=$(<"$work/ready")
printf 'Domowik: %s\nBrowser profile: %s\nClose Chromium or press Ctrl+C to stop this server.\n' "$url" "$profile"

# Process-local driver selection, not a system or profile setting. Keep normal
# Chromium sandboxing and GPU blocklists; no unsafe-WebGPU/SwiftShader switches.
VK_LOADER_DRIVERS_SELECT="${VK_LOADER_DRIVERS_SELECT:-*intel*}" \
  "$chrome" --user-data-dir="$profile" --no-first-run --no-default-browser-check \
  --disable-background-mode --enable-features=Vulkan --ozone-platform=x11 "$url" &
browser=$!
while kill -0 "$browser" 2>/dev/null; do
  kill -0 "$server" 2>/dev/null || fail "localhost server stopped unexpectedly"
  sleep 0.2
done
wait "$browser"
