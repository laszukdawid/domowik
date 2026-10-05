#!/usr/bin/env bash
# Synthetic tools prove each absent prerequisite is refused, without a GUI SDK.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/../.." && pwd)
fixture=$(mktemp -d /tmp/domowik-linux-prerequisites.XXXXXX)
tools=(flutter dart clang c++ cmake ninja pkg-config xvfb-run timeout)
cleanup() {
  for tool in "${tools[@]}" uname; do rm -f "$fixture/$tool"; done
  rmdir "$fixture"
}
trap cleanup EXIT
ln -s /bin/uname "$fixture/uname"
for tool in "${tools[@]}"; do ln -s /bin/true "$fixture/$tool"; done
checker="$repo_root/scripts/check-linux-editor-prerequisites.sh"
PATH="$fixture" /bin/bash "$checker" >/dev/null
for tool in "${tools[@]}"; do
  rm "$fixture/$tool"
  if output=$(PATH="$fixture" /bin/bash "$checker" 2>&1); then
    echo "FAIL: missing $tool was accepted" >&2
    exit 1
  fi
  [[ "$output" == *"prerequisite missing: $tool"* ]]
  ln -s /bin/true "$fixture/$tool"
done
rm "$fixture/pkg-config"
ln -s /bin/false "$fixture/pkg-config"
if output=$(PATH="$fixture" /bin/bash "$checker" 2>&1); then
  echo "FAIL: missing native packages were accepted" >&2
  exit 1
fi
[[ "$output" == *"gtk+-3.0 and epoxy"* ]]
echo "Linux prerequisites: complete fixture passes; nine missing tools and missing packages rejected"
