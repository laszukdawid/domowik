# domowik_bridge_plugin

Spike plugin for issue 0024. **Not a product component.**

Answers one question the earlier rounds of the spike left open: where does the
native core live once a Flutter application is a signed bundle rather than a
`dart run` invocation?

On macOS it links the Rust bridge crate into this pod's framework as a static
archive, so Xcode embeds and signs `domowik_bridge_plugin.framework` inside
`MyApp.app/Contents/Frameworks` and gives it an `@rpath` id. See
`macos/domowik_bridge_plugin.podspec` for why a `staticlib` rather than the
`cdylib` the standalone harnesses load, and the spike README for what was
measured.

The FFI declarations are NOT here. They live in `package:domowik_bridge_harness`
and are shared verbatim with the standalone harness, because two copies of an
ABI declaration is the drift this spike exists to catch.
