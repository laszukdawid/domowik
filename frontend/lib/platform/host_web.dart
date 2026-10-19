// The browser host: no environment, no process, no filesystem. Probes that
// need them are native-only and never start here.

/// Whether this host has a filesystem the editor may browse and write.
const bool hostHasFilesystem = false;

/// Browsers have no process environment; a probe switch arrives as a query
/// parameter of the page URL instead (`?DOMOWIK_WEB_PROBE=1`).
String? hostEnvironment(String key) => Uri.base.queryParameters[key];

/// Probe output goes to the browser console.
void hostLog(String line) => print(line);

void hostLogError(String line) => print(line);

/// A page cannot end its process: the probe's verdict is its last console
/// line, and this records the exit code after it.
Never hostExit(int code) {
  print('DOMOWIK PROBE EXIT $code');
  throw UnsupportedError('exit($code) is not available in a browser');
}

/// Browsers expose no directories by path.
Future<String?> hostResolveDirectory(String path) async => null;

Future<(String, String, Future<void> Function())> hostProbeModel(
  String? supplied,
  String contents,
) => throw UnsupportedError('the 3D probe runs only on native hosts');

/// A page has no bundle directory.
String? hostBundleRoot() => null;
