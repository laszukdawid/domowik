import 'dart:async';

import 'package:domowik_editor/file_workflows.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  late List<Object> events;
  late bool dirty, saved;
  late LeaveDecision decision;
  late Future<LeaveDecision> Function() ask;
  late FileWorkflowController workflow;
  setUp(() {
    events = [];
    dirty = false;
    saved = true;
    decision = LeaveDecision.cancel;
    ask = () async {
      events.add('ask');
      return decision;
    };
    workflow = FileWorkflowController(
      isDirty: () => dirty,
      askToLeave: () => ask(),
      save: () async {
        events.add('save');
        return saved;
      },
      beforeAction: () => events.add('cancel gesture'),
      onBusyChanged: events.add,
      onError: events.add,
    );
  });
  tearDown(() => workflow.dispose());

  test(
    'clean replacement skips approval; gate covers the entire action',
    () async {
      expect(
        await workflow.replace(() async {
          expect(workflow.busy, isTrue);
          events.add('replace');
          return true;
        }),
        isTrue,
      );
      expect(events, ['cancel gesture', true, 'replace', false]);
      expect(workflow.busy, isFalse);
    },
  );

  test('cancel, discard and save use explicit decisions', () async {
    dirty = true;
    var replacements = 0;
    Future<bool> replace() async {
      replacements++;
      return true;
    }

    expect(await workflow.replace(replace), isFalse);
    expect(replacements, 0);
    decision = LeaveDecision.discard;
    expect(await workflow.replace(replace), isTrue);
    expect(replacements, 1);
    decision = LeaveDecision.save;
    saved = false;
    expect(await workflow.replace(replace), isFalse);
    expect(replacements, 1);
    saved = true;
    expect(await workflow.replace(replace), isTrue);
    expect(replacements, 2);
    expect(events.where((event) => event == 'save').length, 2);
  });

  test(
    'concurrent file/close/property actions are refused, not queued',
    () async {
      final pending = Completer<bool>();
      final first = workflow.run(() => pending.future);
      expect(workflow.busy, isTrue);
      expect(
        await workflow.run(() async => throw StateError('must not run')),
        isFalse,
      );
      expect(events, ['cancel gesture', true]);
      pending.complete(true);
      expect(await first, isTrue);
      expect(await workflow.run(() async => true), isTrue);
    },
  );

  test('errors report once and release the gate', () async {
    final error = StateError('picker failed');
    expect(await workflow.run(() async => throw error), isFalse);
    expect(events, ['cancel gesture', true, error, false]);
    expect(workflow.busy, isFalse);
    expect(await workflow.run(() async => true), isTrue);
  });

  test('disposal during approval prevents save and replacement', () async {
    dirty = true;
    final pending = Completer<LeaveDecision>();
    ask = () => pending.future;
    final result = workflow.replace(
      () async => throw StateError('must not run'),
    );
    workflow.dispose();
    pending.complete(LeaveDecision.save);
    expect(await result, isFalse);
    expect(events, ['cancel gesture', true]);
    expect(workflow.busy, isFalse);
    expect(await workflow.run(() async => true), isFalse);
  });

  test('disposal suppresses late error and UI callbacks', () async {
    final pending = Completer<bool>();
    final result = workflow.run(() => pending.future);
    workflow.dispose();
    pending.completeError(StateError('late failure'));
    expect(await result, isFalse);
    expect(events, ['cancel gesture', true]);
  });

  test(
    'native dialog adapter preserves paths, suggestions and cancellation',
    () async {
      const channel = MethodChannel('invalid.domowik-spike/files');
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      final calls = <MethodCall>[];
      messenger.setMockMethodCallHandler(channel, (call) async {
        calls.add(call);
        return call.method == 'openPlan' ? '/tmp/a plan.domo' : null;
      });
      addTearDown(() => messenger.setMockMethodCallHandler(channel, null));
      const dialogs = NativeFileDialogs();
      expect(await dialogs.openPlan(), '/tmp/a plan.domo');
      expect(await dialogs.savePlan('a plan.domo'), isNull);
      expect(calls.map((call) => call.method), ['openPlan', 'savePlan']);
      expect(calls.last.arguments, 'a plan.domo');
    },
  );
}
