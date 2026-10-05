// CocoaPods needs at least one source file to build a framework target; the
// Rust core arrives through `-force_load` on a static archive, not through
// this file. It is deliberately empty of logic — anything written here would
// be a second implementation of a boundary that already has one.
//
// An empty translation unit is not valid C, so this declares one unused
// symbol rather than nothing at all.
int domowik_bridge_plugin_placeholder(void);
int domowik_bridge_plugin_placeholder(void) { return 0; }
