import 'dart:ffi';
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
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
    session = PlanSession(bridge);
    scratch = Directory.systemTemp.createTempSync('domowik-session-');
  });
  tearDown(() {
    session.dispose();
    expect(bridge.openPlans(), 0);
    expect(bridge.outstandingBuffers(), 0);
    scratch.deleteSync(recursive: true);
  });

  test('create, save, reopen and replace keep exactly one owned plan', () {
    expect(session.create().succeeded, isTrue);
    expect(session.handle, isNonZero);
    expect(session.path, isNull);
    expect(bridge.openPlans(), 1);
    final path = '${scratch.path}/my plan.domo';
    final original = session.handle;
    expect(session.save(path).succeeded, isTrue);
    expect(session.handle, original);
    expect(session.path, path);
    expect(session.open(path).succeeded, isTrue);
    expect(session.handle, isNot(original));
    expect(bridge.openPlans(), 1);
    expect(session.create().succeeded, isTrue);
    expect(session.path, isNull);
    expect(bridge.openPlans(), 1);
  });

  test('failed replacement/save preserve handle, path and original bytes', () {
    session.create();
    final path = '${scratch.path}/original.domo';
    expect(session.save(path).succeeded, isTrue);
    final bytes = File(path).readAsBytesSync();
    final handle = session.handle;
    expect(session.open('${scratch.path}/missing.domo').succeeded, isFalse);
    expect(session.handle, handle);
    expect(session.path, path);
    final result = session.save(scratch.path);
    expect(result.succeeded, isFalse);
    expect(result.detail, isNotEmpty);
    expect(session.handle, handle);
    expect(session.path, path);
    expect(File(path).readAsBytesSync(), bytes);
    expect(bridge.openPlans(), 1);
    expect(bridge.outstandingBuffers(), 0);
  });

  test('empty saves refuse; disposal is idempotent and rejects reuse', () {
    expect(session.save('${scratch.path}/empty.domo').code, invalidArgument);
    session.create();
    session.dispose();
    session.dispose();
    expect(session.handle, 0);
    expect(() => session.create(), throwsStateError);
    expect(() => session.open('anything'), throwsStateError);
    expect(() => session.save('anything'), throwsStateError);
  });
}
