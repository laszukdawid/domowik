import Cocoa
import FlutterMacOS

@main
class AppDelegate: FlutterAppDelegate {
  private var terminationPending = false

  override func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
    // Quitting does not call windowShouldClose, so it needs the same guard.
    guard let window = sender.windows.compactMap({ $0 as? MainFlutterWindow }).first else {
      return .terminateNow
    }
    if window.closeApproved { return .terminateNow }
    if terminationPending { return .terminateCancel }
    terminationPending = true
    window.requestEditorClose { [weak self] approved in
      self?.terminationPending = false
      sender.reply(toApplicationShouldTerminate: approved)
    }
    return .terminateLater
  }

  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }
}
