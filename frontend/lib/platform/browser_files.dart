// Browser file access for the web editor (issue 0088), bound by hand with
// `dart:js_interop` so the editor keeps an empty third-party package ledger.
//
// Opening uses an `<input type="file">`. Saving prefers the File System Access
// API (`showSaveFilePicker`, Chromium), which reports whether the write
// completed; elsewhere it falls back to a download, which the browser
// completes on its own and cannot confirm.

import 'dart:async';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'model_import_types.dart';

extension type _Document(JSObject _) implements JSObject {
  external _Element createElement(String tag);
  external _Element get body;
}

extension type _Element(JSObject _) implements JSObject {
  external set type(String value);
  external set accept(String value);
  external set href(String value);
  external set download(String value);
  external set style(String value);
  external set multiple(bool value);
  external set webkitdirectory(bool value);
  external _FileList? get files;
  external void click();
  external void remove();
  external void appendChild(_Element child);
  external void addEventListener(String type, JSFunction listener);
}

extension type _FileList(JSObject _) implements JSObject {
  external int get length;
  external _File? item(int index);
}

extension type _File(JSObject _) implements JSObject {
  external String get name;
  external String get webkitRelativePath;
  external int get size;
  external JSPromise<JSArrayBuffer> arrayBuffer();
}

extension type _Window(JSObject _) implements JSObject {
  external void addEventListener(String type, JSFunction listener);
}

extension type _BeforeUnload(JSObject _) implements JSObject {
  external void preventDefault();
  external set returnValue(String value);
}

@JS('document')
external _Document get _document;

@JS('window')
external _Window get _window;

@JS('URL.createObjectURL')
external String _createObjectUrl(JSObject blob);

@JS('URL.revokeObjectURL')
external void _revokeObjectUrl(String url);

@JS('Blob')
extension type _Blob._(JSObject _) implements JSObject {
  external factory _Blob(JSArray<JSAny> parts, JSObject options);
}

/// A file the user picked: its name and contents.
class PickedFile {
  const PickedFile(this.name, this.bytes);
  final String name;
  final Uint8List bytes;
}

/// A selected file from a browser directory upload. Its bytes are read only
/// when the import reaches this file.
class BrowserDirectoryFile {
  const BrowserDirectoryFile(this.name, this.sourcePath, this.readBytes);

  final String name;
  final String sourcePath;
  final Future<Uint8List> Function() readBytes;
}

class BrowserDirectorySelection {
  const BrowserDirectorySelection(this.files, this.diagnostics);

  final List<BrowserDirectoryFile> files;
  final List<String> diagnostics;
}

/// Asks the user for one file matching [accept] (for example `.domo`).
/// Completes with null if they cancel.
Future<PickedFile?> pickFile(String accept) {
  final input = _document.createElement('input')
    ..type = 'file'
    ..accept = accept
    ..style = 'display:none';
  _document.body.appendChild(input);
  final picked = Completer<PickedFile?>();
  void finish(PickedFile? file) {
    if (!picked.isCompleted) picked.complete(file);
    input.remove();
  }

  input.addEventListener(
    'change',
    ((JSObject _) {
      final file = input.files?.item(0);
      if (file == null) return finish(null);
      file.arrayBuffer().toDart.then(
        (buffer) => finish(PickedFile(file.name, buffer.toDart.asUint8List())),
        onError: (Object error, StackTrace stack) {
          if (!picked.isCompleted) picked.completeError(error, stack);
          input.remove();
        },
      );
    }).toJS,
  );
  input.addEventListener('cancel', ((JSObject _) => finish(null)).toJS);
  input.click();
  return picked.future;
}

/// Asks the browser to upload a selected directory tree. `webkitdirectory`
/// recursively includes files from nested folders; non-model files are
/// ignored. Enumeration is capped so an accidental enormous folder stays
/// responsive and reports that it was truncated.
Future<BrowserDirectorySelection?> pickDirectoryFiles() {
  final input = _document.createElement('input')
    ..type = 'file'
    ..accept = '.glb,.obj,.zip'
    ..multiple = true
    ..webkitdirectory = true
    ..style = 'display:none';
  _document.body.appendChild(input);
  final picked = Completer<BrowserDirectorySelection?>();
  void finish(BrowserDirectorySelection? selection) {
    if (!picked.isCompleted) picked.complete(selection);
    input.remove();
  }

  input.addEventListener(
    'change',
    ((JSObject _) {
      final files = input.files;
      if (files == null) return finish(null);
      final diagnostics = <String>[];
      final count = files.length > 10000 ? 10000 : files.length;
      if (files.length > count) {
        diagnostics.add('Directory import limited to the first 10,000 files.');
      }
      final selected = <BrowserDirectoryFile>[];
      for (var index = 0; index < count; index++) {
        final file = files.item(index);
        if (file == null || !_isModelFile(file.name)) continue;
        selected.add(
          BrowserDirectoryFile(
            file.name,
            file.webkitRelativePath.isEmpty
                ? file.name
                : file.webkitRelativePath,
            () => _readModelFile(file),
          ),
        );
      }
      finish(BrowserDirectorySelection(selected, diagnostics));
    }).toJS,
  );
  input.addEventListener('cancel', ((JSObject _) => finish(null)).toJS);
  input.click();
  return picked.future;
}

bool _isModelFile(String name) {
  final lower = name.toLowerCase();
  return lower.endsWith('.glb') ||
      lower.endsWith('.obj') ||
      lower.endsWith('.zip');
}

Future<Uint8List> _readModelFile(_File file) async {
  if (file.size == 0 || file.size > maxModelImportBytes) {
    throw FormatException(
      'Model is empty or exceeds the per-file 256 MiB limit',
      file.name,
    );
  }
  final bytes = (await file.arrayBuffer().toDart).toDart.asUint8List();
  if (bytes.isEmpty || bytes.length > maxModelImportBytes) {
    throw FormatException(
      'Model is empty or exceeds the per-file 256 MiB limit',
      file.name,
    );
  }
  return bytes;
}

/// Whether this browser can ask where to save and report the write.
bool get canChooseSaveLocation => globalContext.has('showSaveFilePicker');

/// A place to save chosen up front, or null if the user cancelled.
///
/// With the File System Access API the user picks a file now, and `write`
/// completes with true once the file is written or throws if it was not.
/// Otherwise `write` downloads the bytes under [suggestedName] and completes
/// with false: a page learns nothing about whether the download was kept.
Future<({String name, Future<bool> Function(Uint8List) write})?>
chooseSaveTarget(String suggestedName) async {
  if (!canChooseSaveLocation) {
    return (
      name: suggestedName,
      write: (Uint8List bytes) async {
        _download(suggestedName, bytes);
        return false;
      },
    );
  }
  final options = JSObject()
    ..setProperty('suggestedName'.toJS, suggestedName.toJS)
    ..setProperty(
      'types'.toJS,
      [
        JSObject()
          ..setProperty('description'.toJS, 'Domowik plan'.toJS)
          ..setProperty(
            'accept'.toJS,
            JSObject()
              ..setProperty('application/zip'.toJS, ['.domo'.toJS].toJS),
          ),
      ].toJS,
    );
  final JSObject handle;
  try {
    handle = await globalContext
        .callMethod<JSPromise<JSObject>>('showSaveFilePicker'.toJS, options)
        .toDart;
  } catch (error) {
    // AbortError: the user closed the picker.
    if ('$error'.contains('AbortError')) return null;
    rethrow;
  }
  final name = handle.getProperty<JSString>('name'.toJS).toDart;
  return (
    name: name,
    write: (Uint8List bytes) async {
      final writable = await handle
          .callMethod<JSPromise<JSObject>>('createWritable'.toJS)
          .toDart;
      try {
        await writable
            .callMethod<JSPromise<JSAny?>>('write'.toJS, bytes.toJS)
            .toDart;
        await writable.callMethod<JSPromise<JSAny?>>('close'.toJS).toDart;
      } catch (_) {
        // Release the file and discard the partial write before reporting.
        try {
          await writable.callMethod<JSPromise<JSAny?>>('abort'.toJS).toDart;
        } catch (_) {}
        rethrow;
      }
      return true;
    },
  );
}

void _download(String name, Uint8List bytes) {
  final blob = _Blob(
    [bytes.toJS].toJS,
    JSObject()..setProperty('type'.toJS, 'application/zip'.toJS),
  );
  final url = _createObjectUrl(blob);
  final anchor = _document.createElement('a')
    ..href = url
    ..download = name
    ..style = 'display:none';
  _document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  // The click has started the download; the URL is no longer needed.
  Timer(const Duration(seconds: 30), () => _revokeObjectUrl(url));
}

bool _unsaved = false;
bool _guardInstalled = false;

/// Makes leaving the page ask first while [unsaved] is true. Browsers show
/// their own wording; a page cannot replace it or wait for an answer.
void guardUnsavedChanges(bool unsaved) {
  _unsaved = unsaved;
  if (_guardInstalled) return;
  _guardInstalled = true;
  _window.addEventListener(
    'beforeunload',
    ((_BeforeUnload event) {
      if (!_unsaved) return;
      event.preventDefault();
      event.returnValue = '';
    }).toJS,
  );
}
