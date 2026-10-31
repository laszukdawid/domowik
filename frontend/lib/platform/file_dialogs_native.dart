import 'dart:io';

import 'package:flutter/services.dart';

import '../file_workflows.dart';
import '../plan_session.dart';

FileDialogs defaultFileDialogs() => const DesktopFileDialogs();

/// Native plan dialogs plus the desktop `.usdz` export location.
class DesktopFileDialogs extends NativeFileDialogs implements ExportDialogs {
  const DesktopFileDialogs();
  static const _channel = MethodChannel('invalid.domowik-spike/files');

  @override
  Future<PlanByteTarget?> saveUsdz(String suggestedName) async {
    final path = await _channel.invokeMethod<String>('saveUsdz', suggestedName);
    if (path == null) return null;
    return PlanByteTarget(path, (Uint8List bytes) async {
      await File(path).writeAsBytes(bytes, flush: true);
      return true;
    });
  }
}

/// Writes a recovery copy of [bytes] (an encoding from the core) to [path]
/// through a temporary file in the same directory, so a failed write never
/// leaves a partial file at [path].
Future<void> writePlanCopy(String path, Uint8List bytes) async {
  final temporary = File(
    '$path.tmp-$pid-${DateTime.now().microsecondsSinceEpoch}',
  );
  try {
    await temporary.writeAsBytes(bytes, flush: true);
    await temporary.rename(path);
  } on Object {
    try {
      await temporary.delete();
    } on FileSystemException {
      // Nothing was written, or it is already gone.
    }
    rethrow;
  }
}
