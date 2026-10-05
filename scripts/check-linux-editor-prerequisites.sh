#!/usr/bin/env bash
# CI's fail-closed counterpart to optional local GUI verification stages.
set -euo pipefail
if [ "$(uname -s)" != Linux ]; then
  echo "Linux editor checks require a Linux host" >&2
  exit 1
fi
for tool in flutter dart clang c++ cmake ninja pkg-config xvfb-run timeout; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "Linux editor prerequisite missing: $tool" >&2
    exit 1
  fi
done
if ! pkg-config --exists gtk+-3.0 epoxy; then
  echo "Linux editor prerequisites require gtk+-3.0 and epoxy development packages" >&2
  exit 1
fi
echo "Linux editor prerequisites present"
