import 'dart:typed_data';

import '../file_workflows.dart';
import '../plan_session.dart';
import 'browser_files.dart';

FileDialogs defaultFileDialogs() => const BrowserFileDialogs();

/// Browsers write through downloads ([PlanByteTarget]), never to a path.
Future<void> writePlanCopy(String path, Uint8List bytes) =>
    throw UnsupportedError('A browser cannot write to a file path');

/// Plan pickers for a browser tab. Opening reads the picked file's bytes;
/// saving writes encoded bytes to a chosen file or downloads them. A tab
/// cannot hold its close for the editor's dialog, so unsaved changes use the
/// browser's own leave-page prompt instead of the close handler.
class BrowserFileDialogs
    implements FileDialogs, ExportDialogs, UnsavedChangesGuard {
  const BrowserFileDialogs();

  @override
  Future<PlanSource?> openPlan() async {
    final file = await pickFile('.domo');
    return file == null ? null : PlanBytes(file.name, file.bytes);
  }

  @override
  Future<PlanTarget?> savePlan(String suggestedName) async {
    final target = await chooseSaveTarget(suggestedName);
    return target == null ? null : PlanByteTarget(target.name, target.write);
  }

  @override
  Future<PlanByteTarget?> saveUsdz(String suggestedName) async {
    final target = await chooseSaveTarget(
      suggestedName,
      description: 'USD 3D scene',
      mediaType: 'model/vnd.usdz+zip',
      extension: '.usdz',
    );
    return target == null ? null : PlanByteTarget(target.name, target.write);
  }

  @override
  void setCloseHandler(Future<bool> Function()? handler) {}

  @override
  void setUnsavedChanges(bool unsaved) => guardUnsavedChanges(unsaved);
}
