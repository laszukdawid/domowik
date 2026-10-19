// macOS presenter for the shared Rust 3D renderer (issue 0101).
#import <FlutterMacOS/FlutterMacOS.h>

// Registers `invalid.domowik-spike/three-d`, the same channel the Linux
// runner implements, so the Dart 3D view is one client of both platforms.
@interface DomowikScene3DPlugin : NSObject <FlutterPlugin>
@end
