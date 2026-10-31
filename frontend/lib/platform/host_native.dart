import 'dart:io';
import 'dart:typed_data';

import '../selftest.dart' show bundleRoot;
import '../web_graphics_policy.dart';

/// Browser safety policy never restricts a native presenter.
WebGraphicsPolicy hostGraphicsPolicy() => const WebGraphicsPolicy.allowed();

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

/// The bytes of the file at `path`, for the benchmark's plan inputs.
Future<Uint8List> hostReadBytes(String path) => File(path).readAsBytes();

/// Writes `text` to `path`, readable by its owner only (benchmark results).
Future<void> hostWritePrivate(String path, String text) async {
  final file = File(path);
  await file.writeAsString(text, flush: true);
  if (!Platform.isWindows) await Process.run('chmod', ['600', path]);
}

/// Current and peak resident memory of this process, in bytes.
(int, int) hostMemory() => (ProcessInfo.currentRss, ProcessInfo.maxRss);

/// The host OS name, version and logical CPU count.
(String, String, int) hostSystem() => (
  Platform.operatingSystem,
  Platform.operatingSystemVersion,
  Platform.numberOfProcessors,
);

/// The Dart runtime version string.
String hostDartVersion() => Platform.version;
