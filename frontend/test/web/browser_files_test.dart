// The real browser file adapter (lib/platform/browser_files.dart), with the
// page's save APIs replaced to play the cases a browser cannot be made to
// produce on demand: no File System Access API, a blocked download, a write
// that fails. From review round 1 of issue 0088.
@TestOn('browser')
library;

import 'dart:convert';
import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:domowik_editor/file_workflows.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:domowik_editor/platform/browser_files.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('eval')
external JSAny? _eval(JSString text);

void js(String text) => _eval(text.toJS);

int counter(String key) =>
    globalContext.getProperty<JSNumber>(key.toJS).toDartInt;

void main() {
  late WebCore core;
  setUpAll(() async {
    core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
  });

  bool dirty(PlanSession session) =>
      (jsonDecode(core.state(session.handle).text!) as Map)['dirty'] as bool;

  PlanSession editedPlan() {
    final session = PlanSession(core)..create();
    addTearDown(session.dispose);
    final level =
        ((jsonDecode(core.snapshot(session.handle).text!) as Map)['levels']
                as List)
            .single['id'];
    final edit = core.edit(
      session.handle,
      jsonEncode({
        'kind': 'addWall',
        'levelId': level,
        'start': [0, 0],
        'end': [4000, 0],
        'thicknessMm': 200,
        'heightMm': 2500,
      }),
    );
    expect(edit.code, ok);
    return session;
  }

  test(
    'a download that may not be kept never authorizes replacing the plan',
    () async {
      js('''
      globalThis.testPicker = globalThis.showSaveFilePicker;
      delete globalThis.showSaveFilePicker;
      globalThis.testClick = HTMLAnchorElement.prototype.click;
      globalThis.testDownloads = 0;
      HTMLAnchorElement.prototype.click = function() { globalThis.testDownloads++; };
    ''');
      addTearDown(
        () => js('''
        if (globalThis.testPicker) globalThis.showSaveFilePicker = globalThis.testPicker;
        HTMLAnchorElement.prototype.click = globalThis.testClick;
      '''),
      );
      expect(canChooseSaveLocation, isFalse);
      final session = editedPlan();
      final original = session.handle;
      final flow = FileWorkflowController(
        isDirty: () => dirty(session),
        askToLeave: () async => LeaveDecision.save,
        save: () async {
          final target = (await chooseSaveTarget('blocked.domo'))!;
          return (await session.save(
            PlanByteTarget(target.name, target.write),
          )).succeeded;
        },
        beforeAction: () {},
        onBusyChanged: (_) {},
        onError: (error) => fail('$error'),
      );
      final replaced = await flow.replace(
        () async => session.create().succeeded,
      );
      expect(counter('testDownloads'), 1, reason: 'the download was started');
      expect(replaced, isFalse);
      expect(session.handle, original);
      expect(dirty(session), isTrue);
      expect(core.outstandingBuffers(), 0);
    },
  );

  test(
    'a failed File System Access write is aborted and stays unsaved',
    () async {
      js('''
      globalThis.testPicker = globalThis.showSaveFilePicker;
      globalThis.testAbort = 0;
      globalThis.testClose = 0;
      globalThis.showSaveFilePicker = async function() {
        return {name: 'quota.domo', createWritable: async function() {
          return {write: async function() { throw new Error('quota exceeded'); },
            close: async function() { globalThis.testClose++; },
            abort: async function() { globalThis.testAbort++; }};
        }};
      };
    ''');
      addTearDown(
        () => js('''
        if (globalThis.testPicker) globalThis.showSaveFilePicker = globalThis.testPicker;
        else delete globalThis.showSaveFilePicker;
      '''),
      );
      final session = editedPlan();
      final target = (await chooseSaveTarget('quota.domo'))!;
      await expectLater(
        session.save(PlanByteTarget(target.name, target.write)),
        throwsA(anything),
      );
      expect(counter('testAbort'), 1);
      expect(counter('testClose'), 0);
      expect(dirty(session), isTrue);
      expect(session.path, isNull);
    },
  );

  test('a confirmed File System Access write saves', () async {
    js('''
      globalThis.testPicker = globalThis.showSaveFilePicker;
      globalThis.testWritten = 0;
      globalThis.showSaveFilePicker = async function() {
        return {name: 'kept.domo', createWritable: async function() {
          return {write: async function(b) { globalThis.testWritten = b.length; },
            close: async function() {}, abort: async function() {}};
        }};
      };
    ''');
    addTearDown(
      () => js('''
        if (globalThis.testPicker) globalThis.showSaveFilePicker = globalThis.testPicker;
        else delete globalThis.showSaveFilePicker;
      '''),
    );
    final session = editedPlan();
    final target = (await chooseSaveTarget('kept.domo'))!;
    final result = await session.save(
      PlanByteTarget(target.name, target.write),
    );
    expect(result.succeeded, isTrue);
    expect(counter('testWritten'), greaterThan(0));
    expect(dirty(session), isFalse);
    expect(session.path, 'kept.domo');
  });

  test(
    'directory upload recursively selects supported model files only',
    () async {
      js('''
      globalThis.testFolderClick = HTMLInputElement.prototype.click;
      globalThis.testFolderEnabled = 0;
      HTMLInputElement.prototype.click = function() {
        globalThis.testFolderEnabled = this.webkitdirectory && this.multiple ? 1 : 0;
        const transfer = new DataTransfer();
        const chair = new File(['chair bytes'], 'chair.OBJ');
        Object.defineProperty(chair, 'webkitRelativePath',
          {value: 'Furniture/room/chair.OBJ'});
        const table = new File(['table bytes'], 'table.zip');
        Object.defineProperty(table, 'webkitRelativePath',
          {value: 'Furniture/kitchen/table.zip'});
        transfer.items.add(chair);
        transfer.items.add(table);
        transfer.items.add(new File(['notes'], 'notes.txt'));
        this.files = transfer.files;
        this.dispatchEvent(new Event('change'));
      };
    ''');
      addTearDown(
        () => js(
          'HTMLInputElement.prototype.click = globalThis.testFolderClick;',
        ),
      );

      final selection = (await pickDirectoryFiles())!;
      expect(counter('testFolderEnabled'), 1);
      expect(selection.files.map((file) => file.name), [
        'chair.OBJ',
        'table.zip',
      ]);
      expect(selection.files.map((file) => file.sourcePath), [
        'Furniture/room/chair.OBJ',
        'Furniture/kitchen/table.zip',
      ]);
      expect(
        utf8.decode(await selection.files.first.readBytes()),
        'chair bytes',
      );
      expect(selection.diagnostics, isEmpty);
    },
  );

  test('cancelling browser directory selection returns no import', () async {
    js('''
      globalThis.testFolderClick = HTMLInputElement.prototype.click;
      HTMLInputElement.prototype.click = function() {
        this.dispatchEvent(new Event('cancel'));
      };
    ''');
    addTearDown(
      () =>
          js('HTMLInputElement.prototype.click = globalThis.testFolderClick;'),
    );
    expect(await pickDirectoryFiles(), isNull);
  });
}
