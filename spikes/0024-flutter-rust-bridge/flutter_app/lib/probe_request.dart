// How the two headless probes are requested, and why Android needs its own way.
//
// On macOS and Linux a probe is switched on with an environment variable,
// because a process started from a shell has an environment to switch it on
// with. An Android activity has no such thing: it is started by the system from
// an Intent, and nothing a caller puts in that intent appears in
// `Platform.environment`.
//
// Flutter's supported channel for this is the Dart entry point's arguments,
// which the Android embedding takes from `FlutterActivity.getDartEntrypointArgs`
// -- see `MainActivity.kt`. This file is the Dart half of that hand-off, kept
// apart from `main.dart` so the boot path holds the decision and nothing else.
//
// The point of routing probes through arguments rather than through a second
// build is that the probes then run inside THE SAME release APK a person would
// install. A separate probe build could differ from the shipped one in exactly
// the way the macOS slice was once wrong about (a stale native archive that
// both probes passed on), and a probe that verifies a different artefact
// verifies nothing.
library;

import 'dart:io';

/// The prefix `MainActivity` puts in front of the requested probe name.
const String _argumentPrefix = '--domowik-probe=';

/// The headless self-test, the argument analogue of `DOMOWIK_SPIKE_SELFTEST`.
const String selftestProbe = 'selftest';

/// The first-frame probe, the argument analogue of `DOMOWIK_SPIKE_FIRSTFRAME`.
const String firstFrameProbe = 'firstframe';

/// The two Android-only probes, which have no environment-variable analogue
/// because no other platform needs them. See `android_probes.dart`.
const String lifecycleProbe = 'lifecycle';
const String benchmarkProbe = 'benchmark';

const Set<String> _known = {
  selftestProbe,
  firstFrameProbe,
  lifecycleProbe,
  benchmarkProbe,
};

String? _requested;

/// Records what the entry point was asked to run.
///
/// An unrecognised probe name EXITS rather than falling through to the ordinary
/// UI. A harness reads a probe's verdict from its banner, so a mistyped name
/// that quietly started the editor would look exactly like a probe that ran and
/// printed nothing -- which is the shape of failure this spike has already been
/// caught by once.
void applyProbeArguments(List<String> arguments) {
  for (final argument in arguments) {
    if (!argument.startsWith(_argumentPrefix)) {
      continue;
    }
    final name = argument.substring(_argumentPrefix.length);
    if (!_known.contains(name)) {
      stdout.writeln(
        'PROBE REQUEST REJECTED: "$name" is not a probe. '
        'Known probes: ${_known.join(', ')}.',
      );
      exit(2);
    }
    _requested = name;
  }
}

/// True when [name] was requested through the entry point's arguments.
bool probeRequestedByArgument(String name) => _requested == name;

/// Names the two paths the Android runner publishes into the environment.
///
/// Only the platform side can report where an installed Android app actually
/// lives, so the runner puts these in the process environment before the engine
/// starts and the shared provenance check consumes the first of them. They are
/// printed by the CONSUMER rather than by the producer: a value the runner
/// believes it sent is not evidence, and two attempts to report them from
/// Kotlin produced nothing on a real run.
const String androidApkVariable = 'DOMOWIK_ANDROID_APK';
const String androidNativeLibDirVariable = 'DOMOWIK_ANDROID_NATIVE_LIB_DIR';

/// Prints what the platform published, or says plainly that it published
/// nothing. Silence here would be indistinguishable from an empty value.
void reportPlatformProvenance() {
  for (final name in const [androidApkVariable, androidNativeLibDirVariable]) {
    final value = Platform.environment[name];
    stdout.writeln('RUNNER $name=${value ?? '<unset>'}');
  }
}
