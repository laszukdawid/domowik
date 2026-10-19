import '../android_probes.dart' as android_probes;
import '../probe_request.dart';
import '../selftest.dart';
import 'host.dart';

/// Runs a requested headless probe and returns true if it took over the
/// process; false means start the application normally.
bool runProbeEntry(List<String> arguments) {
  // Android cannot be handed an environment, so a probe arrives here as an
  // entry-point argument instead. See `probe_request.dart`.
  applyProbeArguments(arguments);
  if (arguments.isNotEmpty) {
    reportPlatformProvenance();
  }
  // The package can verify itself without a window. See `selftest.dart` for
  // why the evidence has to come from inside the built application rather
  // than from the author having looked at one.
  if (selftestRequested || probeRequestedByArgument(selftestProbe)) {
    hostExit(runSelftest());
  }
  if (probeRequestedByArgument(lifecycleProbe)) {
    android_probes.runLifecycleProbe();
    return true;
  }
  if (probeRequestedByArgument(benchmarkProbe)) {
    android_probes.runBenchmarkProbe();
    return true;
  }
  return false;
}

/// Whether the first-frame probe was requested by argument.
bool get firstFrameProbeArgument => probeRequestedByArgument(firstFrameProbe);
