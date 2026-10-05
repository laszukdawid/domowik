# Linux integration for the Domowik bridge spike (issue 0024).
#
# WHAT THIS ANSWERS. ADR 0003's gate names Linux as a target Phase 2 platform.
# The macOS slice answered "where does the native core live once the app is a
# bundle" for macOS; this answers it for Linux, and the answer is a different
# mechanism rather than the same one with a different suffix.
#
# WHY THE CDYLIB IS FINE HERE AND WAS NOT ON macOS. A Mach-O dylib carries an
# `LC_ID_DYLIB` install name, and cargo sets it to the absolute path of the
# build directory, so the macOS slike had to link a `staticlib` into a
# framework instead. ELF has no such field: the loader resolves a bare soname
# against the RPATH, and Flutter's Linux runner sets `CMAKE_INSTALL_RPATH` to
# `$ORIGIN/lib`. So the `.so` cargo already produces is shippable as-is, and
# everything this file does is get it into `<bundle>/lib`.
cmake_minimum_required(VERSION 3.13)
set(PROJECT_NAME "domowik_bridge_plugin")
project(${PROJECT_NAME} LANGUAGES C)

set(DOMOWIK_BRIDGE_SO "${CMAKE_CURRENT_BINARY_DIR}/libdomowik_bridge.so")

# ALWAYS out of date, deliberately, and this is the one decision in this file
# that has already cost a defect elsewhere.
#
# `add_custom_command(OUTPUT ...)` would make cargo a dependency edge that
# CMake considers satisfied whenever the file exists. That is exactly the shape
# the macOS podspec had before issue 0024's third round: Xcode saw a declared
# output, skipped the script, and linked LAST BUILD'S archive. Both bundle
# probes passed on that stale build. `add_custom_target(... ALL)` has no such
# up-to-date check -- it runs every build -- and cargo's own incremental logic
# is what keeps that cheap.
add_custom_target(domowik_bridge_rust ALL
  BYPRODUCTS "${DOMOWIK_BRIDGE_SO}"
  COMMAND "${CMAKE_CURRENT_LIST_DIR}/build_rust.sh" "${DOMOWIK_BRIDGE_SO}"
  COMMENT "Building the Domowik Rust core for Linux"
  VERBATIM
)

# The contract `flutter/generated_plugins.cmake` reads: every path listed here
# is installed into the bundle's `lib/` directory, which is what `$ORIGIN/lib`
# on the runner resolves against.
set(domowik_bridge_plugin_bundled_libraries
  "${DOMOWIK_BRIDGE_SO}"
  PARENT_SCOPE
)
