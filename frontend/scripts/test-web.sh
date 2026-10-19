#!/usr/bin/env bash
# Runs the browser-backed editor tests (issue 0088): builds the WebAssembly
# core and renderer, places them beside test/web, and runs those tests in a pinned headless
# Chrome for Testing (scripts/chrome-for-testing.sh).
set -euo pipefail

here=$(cd -P "$(dirname "$0")" && pwd -P)
app=$(cd -P "$here/.." && pwd -P)
cd "$app"

"$here/build-web-modules.sh" test/web
# Resolved once (downloading if needed) before Flutter starts the browser.
DOMOWIK_TEST_CHROME=$("$here/chrome-for-testing.sh")
export DOMOWIK_TEST_CHROME
export CHROME_EXECUTABLE="$here/chrome-with-gpu.sh"
flutter test --platform chrome test/web "$@"
