import 'dart:io';

import '../selftest.dart' show bundleRoot;

/// Whether this host has a filesystem the editor may browse and write.
const bool hostHasFilesystem = true;

/// The value of an environment variable, or null.
String? hostEnvironment(String key) => Platform.environment[key];

/// A line of probe output on stdout.
void hostLog(String line) => stdout.writeln(line);

/// A line of probe output on stderr.
void hostLogError(String line) => stderr.writeln(line);

/// Ends the process; used only by command-line probes.
Never hostExit(int code) => exit(code);

/// The canonical path of an existing directory, or null when there is none.
Future<String?> hostResolveDirectory(String path) async {
  final folder = Directory(path);
  if (!await folder.exists()) return null;
  return folder.resolveSymbolicLinks();
}

/// Writes `contents` to a fresh temporary directory, or uses `supplied`, and
/// returns `(directory, file, cleanup)`. For the native 3D probe.
Future<(String, String, Future<void> Function())> hostProbeModel(
  String? supplied,
  String contents,
) async {
  final root = await Directory.systemTemp.createTemp('domowik-3d-probe-');
  final file = File(supplied ?? '${root.path}/original.obj');
  if (supplied == null) await file.writeAsString(contents);
  return (
    file.parent.path,
    file.path,
    () async {
      if (await root.exists()) await root.delete(recursive: true);
    },
  );
}

/// The application bundle directory, for the first-frame probe's packaging
/// check, or null when it cannot be determined.
String? hostBundleRoot() => bundleRoot();
