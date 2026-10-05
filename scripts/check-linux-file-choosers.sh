#!/usr/bin/env bash
# Native GTK regression check; requires existing GTK development tools and Xvfb.
set -euo pipefail
repo_root="$(cd "$(dirname "$0")/.." && pwd)"
test_dir="$(mktemp -d /tmp/domowik-file-choosers.XXXXXX)"
trap 'rm -f "$test_dir/file_chooser_test"; rmdir "$test_dir"' EXIT
source_dir="$repo_root/frontend/linux/runner"
# pkg-config supplies compiler flags for the same GTK dependency as the runner.
c++ -std=c++17 -Wall -Wextra -Werror \
  "$source_dir/file_chooser_test.cc" -o "$test_dir/file_chooser_test" \
  $(pkg-config --cflags --libs gtk+-3.0)
xvfb-run -a "$test_dir/file_chooser_test"
