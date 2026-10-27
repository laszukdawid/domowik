import Cocoa
import FlutterMacOS

/// Hosts the Flutter view and supplies the one thing Dart cannot do alone: a
/// native file dialog.
///
/// This is a METHOD CHANNEL, deliberately, and it is the second half of the
/// Flutter/native story issue 0024 exists to test. `dart:ffi` carries the
/// document to and from Rust; a dialog is AppKit's, must run on the main
/// thread, and returns a path — which is exactly the shape a method channel
/// is for. A pub package would have supplied this, and would also have
/// supplied a licence, a provenance record and a notice obligation
/// (docs/THIRD-PARTY-POLICY.md) for about thirty lines of Cocoa.
class MainFlutterWindow: NSWindow, NSWindowDelegate {
  private static let channelName = "invalid.domowik-spike/files"
  private var editorChannel: FlutterMethodChannel?
  private var closeRequestPending = false
  private(set) var closeApproved = false

  /// Window-close and application-quit share Dart's dirty-session guard.
  /// Missing handlers, channel errors and overlapping requests fail closed.
  func requestEditorClose(completion: @escaping (Bool) -> Void) {
    if closeApproved {
      completion(true)
      return
    }
    guard !closeRequestPending, let channel = editorChannel else {
      completion(false)
      return
    }
    closeRequestPending = true
    channel.invokeMethod("requestClose", arguments: nil) { [weak self] reply in
      guard let self = self else { completion(false); return }
      self.closeRequestPending = false
      let approved = (reply as? Bool) == true
      self.closeApproved = approved
      completion(approved)
    }
  }

  func windowShouldClose(_ sender: NSWindow) -> Bool {
    requestEditorClose { [weak self] approved in
      // close() bypasses windowShouldClose; performClose would prompt again.
      if approved { self?.close() }
    }
    return false
  }

  /// Room for the plan, the 3D view and the inspector side by side: the
  /// nib's 800x600 puts the editor below its 900-pixel compact layout.
  /// Clamped to the window's screen and centred.
  func applyDefaultSize() {
    let visible = (screen ?? NSScreen.main)?.visibleFrame
    let size = NSSize(
      width: min(1280, (visible?.width ?? 1280) * 0.9),
      height: min(820, (visible?.height ?? 820) * 0.9))
    contentMinSize = NSSize(width: 720, height: 480)
    let target = frameRect(forContentRect: NSRect(origin: frame.origin, size: size))
    setFrame(target, display: true)
    center()
  }

  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    let windowFrame = self.frame
    self.contentViewController = flutterViewController
    self.setFrame(windowFrame, display: true)
    // After the first layout pass, when the window is on its screen.
    DispatchQueue.main.async { [weak self] in self?.applyDefaultSize() }

    RegisterGeneratedPlugins(registry: flutterViewController)

    let channel = FlutterMethodChannel(
      name: MainFlutterWindow.channelName,
      binaryMessenger: flutterViewController.engine.binaryMessenger)
    editorChannel = channel
    channel.setMethodCallHandler { call, result in
      switch call.method {
      case "pickAssetDirectory":
        let panel = NSOpenPanel()
        panel.title = "Choose asset directory"
        panel.prompt = "Add directory"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        if let initialPath = call.arguments as? String {
          panel.directoryURL = URL(fileURLWithPath: initialPath, isDirectory: true)
        }
        result(panel.runModal() == .OK ? panel.url?.path : nil)
      case "openPlan":
        let panel = NSOpenPanel()
        panel.allowedFileTypes = ["domo"]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        // `nil` rather than a path is a CANCEL, not an error. Reporting a
        // cancelled dialog as a failure would put an error banner in front of
        // a user who simply changed their mind.
        result(panel.runModal() == .OK ? panel.url?.path : nil)
      case "savePlan":
        let panel = NSSavePanel()
        panel.allowedFileTypes = ["domo"]
        panel.nameFieldStringValue =
          (call.arguments as? String) ?? "plan.domo"
        result(panel.runModal() == .OK ? panel.url?.path : nil)
      case "saveUsdz":
        let panel = NSSavePanel()
        panel.allowedFileTypes = ["usdz"]
        panel.nameFieldStringValue =
          (call.arguments as? String) ?? "plan.usdz"
        result(panel.runModal() == .OK ? panel.url?.path : nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }

    super.awakeFromNib()
    delegate = self
  }
}
