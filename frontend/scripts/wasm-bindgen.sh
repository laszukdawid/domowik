#!/usr/bin/env bash
# Runs the pinned wasm-bindgen CLI, installing it into frontend/.tools on
# first use (issue 0088). The CLI must match the wasm-bindgen crate version
# exactly; both are pinned to 0.2.127, the version wgpu 30.0.1 resolves.
# It is a build tool: its generated JavaScript glue ships with the browser
# editor under the same MIT/Apache-2.0 terms (docs/RUST-DEPENDENCIES.md).
set -euo pipefail
version=0.2.127
here=$(cd -P "$(dirname "$0")" && pwd -P)
root="$here/../.tools/wasm-bindgen-$version"
if [ ! -x "$root/bin/wasm-bindgen" ]; then
  echo "installing wasm-bindgen-cli $version into $root" >&2
  cargo install wasm-bindgen-cli --version "=$version" --locked --root "$root" >&2
fi
exec "$root/bin/wasm-bindgen" "$@"
