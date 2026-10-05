// Where the native core is looked for, on every platform, from any host.
//
// WHY THIS FILE EXISTS. `openBridge` used to be `if (Platform.isMacOS)` and a
// throw. That shape cannot be tested for anything but the host running the
// test: the platform getters answer for THIS machine, so the Linux and Android
// answers would ship having never been read back by anything. Both are now
// looked up from a string, and the string is the test's input.
//
// These are checks on a decision table, not evidence that any platform loads
// anything. What actually loads is recorded in the spike README per platform.

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_spike_app/selftest.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  group('bridgeLibraryTargetFor', () {
    test('macOS opens the framework by its rpath-relative name', () {
      final target = bridgeLibraryTargetFor('macos')!;
      expect(
        target.openArgument,
        'domowik_bridge_plugin.framework/domowik_bridge_plugin',
      );
    });

    test('the ELF platforms open the cargo soname', () {
      expect(bridgeLibraryTargetFor('linux')!.openArgument,
          'libdomowik_bridge.so');
      expect(bridgeLibraryTargetFor('android')!.openArgument,
          'libdomowik_bridge.so');
    });

    test('every packaged platform names a DIFFERENT mechanism', () {
      // Linux and Android share a filename and share nothing else: one is
      // found through the runner RPATH, the other is extracted from an APK
      // and resolved by the bionic loader. A table that gave them the same
      // `packaging` line would be hiding that, and the Linux CMake would look
      // like it covered Android.
      final packaging = <String>{
        for (final os in ['macos', 'linux', 'android'])
          bridgeLibraryTargetFor(os)!.packaging,
      };
      expect(packaging, hasLength(3));
    });

    test('no open argument is absolute', () {
      // An absolute path is the build machine's. This is the defect that made
      // the macOS slice link a staticlib instead of shipping the cdylib.
      for (final os in ['macos', 'linux', 'android']) {
        expect(bridgeLibraryTargetFor(os)!.openArgument, isNot(startsWith('/')));
      }
    });

    test('unpackaged platforms answer null rather than guessing', () {
      // iOS above all: a cdylib is not loadable there the way it is elsewhere,
      // and nothing in this spike has been built for it. Returning a
      // plausible filename would make the spike report coverage it lacks.
      for (final os in ['ios', 'windows', 'fuchsia', '']) {
        expect(bridgeLibraryTargetFor(os), isNull, reason: os);
      }
    });
  });

  group('bundleRootFor', () {
    test('macOS strips Contents/MacOS and requires a .app', () {
      expect(
        bundleRootFor('macos', '/Apps/domowik.app/Contents/MacOS/domowik'),
        '/Apps/domowik.app',
      );
    });

    test('macOS answers null when the executable is not in a bundle', () {
      // A loose binary has no bundle to be inside, and claiming one would
      // make the provenance check compare against a directory that means
      // nothing.
      expect(bundleRootFor('macos', '/usr/local/bin/domowik'), isNull);
      expect(bundleRootFor('macos', '/domowik'), isNull);
    });

    test('Linux takes the directory holding the executable', () {
      // A Flutter Linux bundle is <root>/<binary> with lib/ and data/ beside
      // it, so the core lands at <root>/lib/libdomowik_bridge.so and the
      // startsWith comparison the self-test makes still holds.
      const root = '/opt/domowik/bundle';
      expect(bundleRootFor('linux', '$root/domowik_spike_app'), root);
      expect('$root/lib/libdomowik_bridge.so', startsWith(root));
    });

    test('a system library fails the Linux comparison', () {
      // The negative control. If this ever passes, the provenance check has
      // stopped distinguishing the app copy from a stray one.
      final root = bundleRootFor('linux', '/opt/domowik/bundle/app')!;
      expect('/usr/lib/libdomowik_bridge.so', isNot(startsWith(root)));
    });

    // The three strings below are VERBATIM from one run of the release APK on
    // a device, reported by the Android work stream: the two environment
    // values the runner publishes and the dladdr image path the probe read
    // back, all from the same install so the prefix relation is directly
    // checkable rather than assembled from separate runs.
    const apk =
        '/data/app/~~dpTArVC25hKepTsix9qvCA=='
        '/invalid.domowik_spike.domowik_spike_app-TWu4w0_XfdAQeSKztybDoA=='
        '/base.apk';
    const nativeLibDir =
        '/data/app/~~dpTArVC25hKepTsix9qvCA=='
        '/invalid.domowik_spike.domowik_spike_app-TWu4w0_XfdAQeSKztybDoA=='
        '/lib/arm64';
    const androidImage = '$apk!/lib/arm64-v8a/libdomowik_bridge.so';

    test('a platform-provided root wins, which is how Android reports', () {
      // On Android the executable is the zygote, so no amount of path
      // arithmetic finds where the core came from; the platform side supplies
      // it. `/system/bin/app_process64` is what resolvedExecutable really is
      // there.
      expect(
        bundleRootFor('android', '/system/bin/app_process64',
            platformProvidedRoot: apk),
        apk,
      );
      expect(androidImage, startsWith(apk));
    });

    test('the Android root is the APK, and the native lib dir would fail', () {
      // The counter-intuitive one, and the reason this predicate is written
      // against observed output instead of the obvious guess. With
      // extractNativeLibs=false nothing is extracted: the loader maps the
      // library out of the APK and the native library directory exists and is
      // EMPTY. Reaching for it, which is the natural choice, produces a check
      // that fails on a correctly packaged APK.
      expect(androidImage, isNot(startsWith(nativeLibDir)));
      expect(androidImage.contains('base.apk!/'), isTrue);
    });

    test('Android without a platform-provided root answers null', () {
      // A runner that failed to publish the variable fails the check rather
      // than passing it vacuously.
      expect(bundleRootFor('android', '/system/bin/app_process64'), isNull);
    });
  });
}
