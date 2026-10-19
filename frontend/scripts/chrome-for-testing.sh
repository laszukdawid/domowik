#!/usr/bin/env bash
# Prints the path of a pinned Chrome for Testing executable, downloading and
# verifying it on first use (issue 0088).
#
# The browser tests need a Chrome that starts straight onto a page. A managed
# desktop Chrome may not: a policy that forces sign-in opens the profile
# picker, and Flutter's headless test runner then waits forever. Chrome for
# Testing is Google's separate build for automation; it reads no
# com.google.Chrome policy and is pinned here, so every run tests the same
# browser. It is a test tool, never shipped (docs/THIRD-PARTY-POLICY.md).
#
# Override with CHROME_EXECUTABLE to use another Chrome.
#
# `chrome-for-testing.sh --webgpu-flags` prints the extra switches a headless
# run needs for WebGPU on this host: none where the GPU serves it (macOS), and
# Chrome's SwiftShader software adapter on Linux, where a headless runner has
# no GPU. Those switches are unsafe for browsing and are for tests only.
set -euo pipefail

if [ "${1:-}" = "--webgpu-flags" ]; then
  if [ "$(uname -s)" = Linux ]; then
    echo "--enable-unsafe-webgpu --enable-features=Vulkan --use-vulkan=swiftshader --use-webgpu-adapter=swiftshader --enable-unsafe-swiftshader"
  fi
  exit 0
fi

if [ -n "${CHROME_EXECUTABLE:-}" ]; then
  echo "$CHROME_EXECUTABLE"
  exit 0
fi

version=155.0.8059.39
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) platform=mac-arm64
    sha=529a71bd61aaa2ef266a4d4bd300ae9572ba6a3468a8d55c3023ffeffb5b6b4e
    exe="chrome-$platform/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" ;;
  Darwin-x86_64) platform=mac-x64
    sha=9e4da7961d4a426078f02e89ecfa569a7926ada2dc88aa53c0bd9ed0d46c3bb7
    exe="chrome-$platform/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing" ;;
  Linux-x86_64) platform=linux64
    sha=55672d1f392fd3e7b7a08621b6e804e6bcb39d40cf155504abb74b3a021ea8ea
    exe="chrome-$platform/chrome" ;;
  *) echo "error: no pinned Chrome for Testing for $(uname -s)-$(uname -m);" \
       "set CHROME_EXECUTABLE" >&2; exit 1 ;;
esac

here=$(cd -P "$(dirname "$0")" && pwd -P)
dir="$here/../.tools/chrome-for-testing/$version/$platform"
if [ ! -x "$dir/$exe" ]; then
  for tool in curl unzip shasum; do
    command -v "$tool" >/dev/null 2>&1 || {
      echo "error: $tool is not on PATH" >&2; exit 1; }
  done
  mkdir -p "$dir"
  zip="$dir/chrome.zip"
  url="https://storage.googleapis.com/chrome-for-testing-public/$version/$platform/chrome-$platform.zip"
  echo "downloading Chrome for Testing $version ($platform)" >&2
  curl -sSfL "$url" -o "$zip"
  echo "$sha  $zip" | shasum -a 256 -c - >/dev/null || {
    echo "error: checksum mismatch for $url" >&2; rm -f "$zip"; exit 1; }
  unzip -q -o "$zip" -d "$dir"
  rm -f "$zip"
  # A downloaded app carries the quarantine flag; Gatekeeper would prompt.
  if [ "$(uname -s)" = Darwin ]; then xattr -cr "$dir" 2>/dev/null || true; fi
fi
echo "$dir/$exe"
