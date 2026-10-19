import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:typed_data';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart'
    hide invalidArgument;
import 'package:domowik_editor/core/domowik_core.dart' show invalidArgument;
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  late Bridge bridge;
  late PlanSession session;
  late Directory scratch;
  setUp(() {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
    session = PlanSession(FfiCore(bridge));
    scratch = Directory.systemTemp.createTempSync('domowik-session-');
  });
  tearDown(() {
    session.dispose();
    expect(bridge.openPlans(), 0);
    expect(bridge.outstandingBuffers(), 0);
    scratch.deleteSync(recursive: true);
  });

  test(
    'create, save, reopen and replace keep exactly one owned plan',
    () async {
      expect(session.create().succeeded, isTrue);
      expect(session.handle, isNonZero);
      expect(session.path, isNull);
      expect(bridge.openPlans(), 1);
      final path = '${scratch.path}/my plan.domo';
      final original = session.handle;
      expect((await session.save(PlanPathTarget(path))).succeeded, isTrue);
      expect(session.handle, original);
      expect(session.path, path);
      expect(session.open(PlanPath(path)).succeeded, isTrue);
      expect(session.handle, isNot(original));
      expect(bridge.openPlans(), 1);
      expect(session.create().succeeded, isTrue);
      expect(session.path, isNull);
      expect(bridge.openPlans(), 1);
    },
  );

  test(
    'failed replacement/save preserve handle, path and original bytes',
    () async {
      session.create();
      final path = '${scratch.path}/original.domo';
      expect((await session.save(PlanPathTarget(path))).succeeded, isTrue);
      final bytes = File(path).readAsBytesSync();
      final handle = session.handle;
      expect(
        session.open(PlanPath('${scratch.path}/missing.domo')).succeeded,
        isFalse,
      );
      expect(session.handle, handle);
      expect(session.path, path);
      final result = await session.save(PlanPathTarget(scratch.path));
      expect(result.succeeded, isFalse);
      expect(result.detail, isNotEmpty);
      expect(session.handle, handle);
      expect(session.path, path);
      expect(File(path).readAsBytesSync(), bytes);
      expect(bridge.openPlans(), 1);
      expect(bridge.outstandingBuffers(), 0);
    },
  );

  test('a byte target stores what a path save writes and reopens', () async {
    session.create();
    final path = '${scratch.path}/disk.domo';
    await session.save(PlanPathTarget(path));
    Uint8List? stored;
    final result = await session.save(
      PlanByteTarget('browser.domo', (bytes) async {
        stored = bytes;
        return true;
      }),
    );
    expect(result.succeeded, isTrue);
    expect(session.path, 'browser.domo');
    expect(stored, File(path).readAsBytesSync());
    expect(session.open(PlanBytes('copy.domo', stored!)).succeeded, isTrue);
    expect(session.path, 'copy.domo');
    expect(
      session.open(PlanBytes('junk.domo', Uint8List.fromList([1, 2]))).code,
      isNot(0),
    );
    expect(session.path, 'copy.domo');
    expect(bridge.openPlans(), 1);
  });

  test('an unconfirmed byte write names the file but stays unsaved', () async {
    session.create();
    final core = FfiCore(bridge);
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
    expect(edit.succeeded, isTrue);
    final result = await session.save(
      PlanByteTarget('download.domo', (_) async => false),
    );
    expect(result.unconfirmed, isTrue);
    expect(result.succeeded, isFalse, reason: 'not a save');
    expect(session.path, 'download.domo');
    final state = core.state(session.handle);
    expect(state.text, contains('"dirty":true'));
  });

  test('a failed byte write keeps the plan unsaved and unnamed', () async {
    session.create();
    final failing = PlanByteTarget(
      'never.domo',
      (_) async => throw StateError('quota exceeded'),
    );
    await expectLater(session.save(failing), throwsStateError);
    expect(session.path, isNull);
  });

  test(
    'empty saves refuse; disposal is idempotent and rejects reuse',
    () async {
      expect(
        (await session.save(PlanPathTarget('${scratch.path}/empty.domo'))).code,
        invalidArgument,
      );
      session.create();
      session.dispose();
      session.dispose();
      expect(session.handle, 0);
      expect(() => session.create(), throwsStateError);
      expect(() => session.open(const PlanPath('anything')), throwsStateError);
      expect(
        () => session.save(const PlanPathTarget('anything')),
        throwsStateError,
      );
    },
  );

  test(
    'plain Save rewrites the bound file; a failed save keeps the binding',
    () async {
      session.create();
      expect(session.boundTarget, isNull, reason: 'a new plan has no file');
      final path = '${scratch.path}/bound.domo';
      expect((await session.save(PlanPathTarget(path))).succeeded, isTrue);
      var bound = session.boundTarget as PlanPathTarget?;
      expect(bound?.path, path);

      // An edit the first save never saw reaches the same file on re-save.
      final core = FfiCore(bridge);
      final level =
          ((jsonDecode(core.snapshot(session.handle).text!) as Map)['levels']
                  as List)
              .single['id'];
      expect(
        core
            .edit(
              session.handle,
              jsonEncode({
                'kind': 'addWall',
                'levelId': level,
                'start': [0, 0],
                'end': [4000, 0],
                'thicknessMm': 200,
                'heightMm': 2500,
              }),
            )
            .succeeded,
        isTrue,
      );
      expect((await session.save(session.boundTarget!)).succeeded, isTrue);
      final probe = core.openPath(path);
      expect(probe.code, ok);
      final walls =
          (jsonDecode(core.snapshot(probe.handle).text!) as Map)['walls']
              as List;
      expect(walls, hasLength(1));
      expect(core.close(probe.handle), ok);

      // A failed save must not move the binding off the real file.
      expect(
        (await session.save(PlanPathTarget(scratch.path))).succeeded,
        isFalse,
      );
      bound = session.boundTarget as PlanPathTarget?;
      expect(bound?.path, path);
    },
  );

  test('opening binds a plan to its file before any save', () async {
    final path = '${scratch.path}/opened.domo';
    final opened = PlanSession(FfiCore(bridge));
    try {
      expect(opened.create().succeeded, isTrue);
      expect((await opened.save(PlanPathTarget(path))).succeeded, isTrue);
      final probe = PlanSession(FfiCore(bridge));
      try {
        expect(probe.open(PlanPath(path)).succeeded, isTrue);
        expect(probe.path, path);
        expect((probe.boundTarget! as PlanPathTarget).path, path);
      } finally {
        probe.dispose();
      }
    } finally {
      opened.dispose();
    }
  });
}
