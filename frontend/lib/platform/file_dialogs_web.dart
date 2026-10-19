import '../file_workflows.dart';
import '../plan_session.dart';
import 'browser_files.dart';

FileDialogs defaultFileDialogs() => const BrowserFileDialogs();

/// Plan pickers for a browser tab. Opening reads the picked file's bytes;
/// saving writes encoded bytes to a chosen file or downloads them. A tab
/// cannot hold its close for the editor's dialog, so unsaved changes use the
/// browser's own leave-page prompt instead of the close handler.
class BrowserFileDialogs implements FileDialogs, UnsavedChangesGuard {
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
  void setCloseHandler(Future<bool> Function()? handler) {}

  @override
  void setUnsavedChanges(bool unsaved) => guardUnsavedChanges(unsaved);
}
