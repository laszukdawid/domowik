/// Loads the Domowik Rust core from inside a Flutter application bundle.
///
/// This package exists for ONE reason: to answer where the native code lives
/// once the app is a bundle rather than a `dart run` invocation. The FFI
/// declarations are not here — they are in `package:domowik_bridge_harness`,
/// shared verbatim with the standalone harness, because two copies of an ABI
/// declaration is the drift this spike exists to catch.
library;

import 'dart:ffi';
import 'dart:io';

import 'package:domowik_bridge_harness/domowik_bridge.dart';

export 'package:domowik_bridge_harness/domowik_bridge.dart';

/// The framework name CocoaPods builds for this pod.
const String _framework = 'domowik_bridge_plugin';

/// The soname cargo gives the `cdylib` on every ELF platform.
const String _soname = 'libdomowik_bridge.so';

/// How one platform packages the native core, and therefore what
/// [DynamicLibrary.open] has to be handed on it.
final class BridgeLibraryTarget {
  const BridgeLibraryTarget({
    required this.openArgument,
    required this.packaging,
  });

  /// The exact string passed to [DynamicLibrary.open].
  ///
  /// Every one of these is RELATIVE on purpose. An absolute path would be the
  /// build machine's, which is the precise defect that made the macOS slice
  /// use a `staticlib`: a cargo `cdylib` records its own build directory as
  /// its install name and produces an app that runs for whoever built it and
  /// nobody else.
  final String openArgument;

  /// One line on how the file arrives at that path.
  final String packaging;
}

/// What this spike packages for [operatingSystem], or `null` when it packages
/// nothing.
///
/// Keyed by [Platform.operatingSystem] values. It is a lookup rather than a
/// chain of `Platform.isX` checks so that it can be TESTED: the platform
/// getters answer for the host running the test, so a macOS test run could
/// never reach the Linux branch and the Linux answer would ship unexamined.
///
/// A `null` is a real answer — "this spike has no packaging here" — and the
/// caller is expected to fail loudly rather than guess a filename. Guessing
/// would make the spike report coverage it does not have.
BridgeLibraryTarget? bridgeLibraryTargetFor(String operatingSystem) {
  switch (operatingSystem) {
    case 'macos':
      // Linked INTO the pod framework as a staticlib, which Xcode embeds in
      // `Contents/Frameworks`, signs with the app, and gives an `@rpath` id.
      // The relative path resolves against the loader rpath, so the app finds
      // its own copy and never a stray one on the machine.
      return const BridgeLibraryTarget(
        openArgument: '$_framework.framework/$_framework',
        packaging:
            'staticlib force-loaded into the plugin framework; embedded and '
            'signed by Xcode; resolved through the loader rpath',
      );
    case 'linux':
      // The cdylib itself is fine here: ELF has no install-name-equivalent
      // baked into the file, so the loader resolves the soname against the
      // runner's RPATH, which Flutter sets to `$ORIGIN/lib`. The plugin's
      // CMake puts the `.so` in that directory.
      return const BridgeLibraryTarget(
        openArgument: _soname,
        packaging:
            'cdylib installed into the bundle lib/ directory by the plugin '
            'CMake; resolved through the runner RPATH \$ORIGIN/lib',
      );
    case 'android':
      // Same soname, different mechanism again, and NOT the mechanism this
      // comment first claimed. The per-ABI `.so` ships in the APK under
      // `lib/<abi>/`, and with `extractNativeLibs=false` — what this build
      // sets — nothing is ever extracted: the loader maps the library
      // straight out of the APK, which is why it must be `Stored` rather than
      // `Deflated`. The application's native library directory exists and is
      // EMPTY, observed on an emulator, so a packaging note promising extraction
      // would send a reader looking in the wrong place.
      //
      // The bare soname is still right, and it is right for a stronger reason
      // than convenience: there is no path a caller could name here. The
      // image lives at `<apk>!/lib/<abi>/<soname>` inside an archive whose own
      // path contains two install-scoped random segments.
      return const BridgeLibraryTarget(
        openArgument: _soname,
        packaging:
            'per-ABI cdylib stored uncompressed in the APK under lib/<abi>/ '
            'and mapped from it directly (extractNativeLibs=false); resolved '
            'by soname by the bionic loader',
      );
    default:
      // iOS, Windows, Fuchsia and anything else. iOS is NOT merely "later":
      // a cdylib is not loadable there the way it is elsewhere, so it needs
      // the staticlib-in-a-framework shape macOS now uses, built and signed
      // for iOS. Nothing here has been built for it.
      return null;
  }
}

/// Opens the core and binds the shared declarations to it.
///
/// Throws [UnsupportedError] on a platform this spike packages nothing for,
/// rather than guessing a filename.
Bridge openBridge() {
  final target = bridgeLibraryTargetFor(Platform.operatingSystem);
  if (target == null) {
    throw UnsupportedError(
      'The Domowik bridge spike packages no native core for '
      '${Platform.operatingSystem} (issue 0024).',
    );
  }
  return Bridge.fromLibrary(DynamicLibrary.open(target.openArgument));
}
