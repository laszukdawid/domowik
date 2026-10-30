import 'dart:async';
import 'dart:typed_data';

import 'package:domowik_editor/account.dart';
import 'package:domowik_editor/core/domowik_core.dart' show ok, rejected;
import 'package:domowik_editor/hosted_workflow.dart';
import 'package:domowik_editor/plan_session.dart';
import 'package:flutter_test/flutter_test.dart';

const _session = AccountSession(token: 'csrf');
final _plan = HostedPlan(
  id: 'plan-1',
  title: 'Flat',
  revision: '1',
  updatedAt: DateTime.utc(2026),
);
const _lease = HostedPlanLease(
  leaseId: 'lease-1',
  revision: '1',
  expiresInSeconds: 90,
);

Uint8List _bytes(List<int> values) => Uint8List.fromList(values);

class _Client implements HostedPlanClient {
  final calls = <String>[];
  HostedPlanLease acquireResult = _lease;
  HostedPlanLease renewResult = const HostedPlanLease(
    leaseId: 'lease-1',
    revision: '0',
  );
  HostedPlanDownload downloadResult = HostedPlanDownload(
    bytes: _bytes([1]),
    revision: '1',
  );
  HostedPlan saveResult = HostedPlan(
    id: 'plan-1',
    title: 'Flat',
    revision: '2',
    updatedAt: DateTime.utc(2026),
  );
  Future<HostedPlanLease> Function()? acquire;
  Future<HostedPlanLease> Function()? renew;
  Future<HostedPlanDownload> Function()? download;
  Future<HostedPlan> Function(String revision)? save;
  Future<void> Function()? release;

  @override
  Future<HostedPlanLease> acquireLease(
    AccountSession session,
    String planId, {
    bool force = false,
  }) {
    calls.add('acquire:$force');
    return acquire?.call() ?? Future.value(acquireResult);
  }

  @override
  Future<HostedPlanDownload> downloadPlan(
    AccountSession session,
    String planId,
  ) {
    calls.add('download');
    return download?.call() ?? Future.value(downloadResult);
  }

  @override
  Future<HostedPlanLease> renewLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) {
    calls.add('renew');
    return renew?.call() ?? Future.value(renewResult);
  }

  @override
  Future<void> releaseLease(
    AccountSession session,
    String planId,
    String leaseId,
  ) {
    calls.add('release');
    return release?.call() ?? Future.value();
  }

  @override
  Future<HostedPlan> savePlanContent(
    AccountSession session,
    String planId,
    String leaseId,
    String expectedRevision,
    Uint8List bytes,
  ) {
    calls.add('save:$leaseId:$expectedRevision:${bytes.join(',')}');
    return save?.call(expectedRevision) ?? Future.value(saveResult);
  }

  @override
  Future<HostedPlan> createPlan(AccountSession session, String title) =>
      throw UnimplementedError();

  @override
  Future<HostedPlan> importPlan(
    AccountSession session,
    String title,
    Uint8List bytes,
  ) => throw UnimplementedError();

  @override
  Future<List<HostedPlan>> listPlans(AccountSession session) =>
      throw UnimplementedError();
}

class _Document implements HostedPlanDocument {
  Uint8List bytes = _bytes([7]);
  int generation = 0;
  int markSavedCalls = 0;
  bool dirty = true;

  @override
  bool get isDirty => dirty;

  @override
  HostedDocumentCapture capture() => HostedDocumentCapture(
    bytes: Uint8List.fromList(bytes),
    token: generation,
  );

  @override
  PlanFileResult markSaved(HostedDocumentCapture capture) {
    markSavedCalls++;
    if (capture.token != generation) {
      return const PlanFileResult(rejected, detail: 'changed during save');
    }
    dirty = false;
    return const PlanFileResult(ok);
  }
}

HostedWorkflowCoordinator _coordinator(
  _Client client, {
  Duration saveTimeout = const Duration(seconds: 30),
}) => HostedWorkflowCoordinator(
  client: client,
  session: _session,
  renewalInterval: const Duration(days: 1),
  saveTimeout: saveTimeout,
);

void main() {
  test(
    'acquires the browser lease before downloading and checks its revision',
    () async {
      final client = _Client();
      final workflow = _coordinator(client);
      final checkout = await workflow.checkout(_plan);
      expect(client.calls, ['acquire:false', 'download']);
      expect(checkout.download.bytes, [1]);
      expect(workflow.expectedRevision, '1');
      expect(await workflow.release(), isTrue);
      await workflow.dispose();
    },
  );

  test(
    'a changed download revision refuses checkout and releases its lease',
    () async {
      final client = _Client()
        ..downloadResult = HostedPlanDownload(
          bytes: _bytes([3]),
          revision: '2',
        );
      final workflow = _coordinator(client);
      await expectLater(
        workflow.checkout(_plan),
        throwsA(isA<AccountException>()),
      );
      expect(client.calls, ['acquire:false', 'download', 'release']);
      expect(workflow.lease, isNull);
      expect(workflow.state, HostedWorkflowState.conflict);
      await workflow.dispose();
    },
  );

  test(
    'save uses the expected revision and only a completed PUT advances it',
    () async {
      final client = _Client();
      final workflow = _coordinator(client);
      await workflow.checkout(_plan);
      final document = _Document();
      final result = await workflow.save(document);
      expect(client.calls.last, 'save:lease-1:1:7');
      expect(result.revision, '2');
      expect(result.localDocumentClean, isTrue);
      expect(document.dirty, isFalse);
      expect(workflow.expectedRevision, '2');

      client.renewResult = const HostedPlanLease(
        leaseId: 'lease-1',
        revision: '1',
      );
      await workflow.renew();
      expect(
        workflow.expectedRevision,
        '2',
        reason: 'renew cannot regress PUT revision',
      );
      await workflow.release();
      await workflow.dispose();
    },
  );

  test('edits during an in-flight save stay dirty and recoverable', () async {
    final client = _Client();
    final pendingSave = Completer<HostedPlan>();
    client.save = (_) => pendingSave.future;
    final workflow = _coordinator(client);
    await workflow.checkout(_plan);
    final document = _Document();
    final saving = workflow.save(document);
    document.bytes = _bytes([8, 9]);
    document.generation++;
    pendingSave.complete(client.saveResult);
    final result = await saving;
    expect(result.localDocumentClean, isFalse);
    expect(document.dirty, isTrue);
    expect(workflow.state, HostedWorkflowState.recoveryRequired);
    expect(workflow.recoveryBytes, [8, 9]);
    expect(workflow.expectedRevision, '2');
    await workflow.release();
    await workflow.dispose();
  });

  test('timed out PUT stays blocked and ignores a late success', () async {
    final client = _Client();
    final pendingSave = Completer<HostedPlan>();
    client.save = (_) => pendingSave.future;
    final workflow = _coordinator(
      client,
      saveTimeout: const Duration(milliseconds: 15),
    );
    await workflow.checkout(_plan);
    final document = _Document();

    await expectLater(
      workflow.save(document),
      throwsA(isA<TimeoutException>()),
    );
    expect(workflow.state, HostedWorkflowState.recoveryRequired);
    expect(workflow.expectedRevision, '1');
    expect(workflow.recoveryBytes, [7]);
    expect(document.dirty, isTrue);
    expect(document.markSavedCalls, 0);

    pendingSave.complete(client.saveResult);
    await Future<void>.delayed(Duration.zero);
    expect(workflow.expectedRevision, '1');
    expect(workflow.recoveryBytes, [7]);
    expect(document.dirty, isTrue);
    expect(document.markSavedCalls, 0);
    expect(await workflow.release(), isTrue);
    await workflow.dispose();
  });

  test(
    'a stale renewal cannot turn a revision conflict back into ready',
    () async {
      final client = _Client();
      final pendingRenew = Completer<HostedPlanLease>();
      client.renew = () => pendingRenew.future;
      client.save = (_) => Future.error(
        const AccountException('stale writer', code: 'revision-conflict'),
      );
      final workflow = _coordinator(client);
      await workflow.checkout(_plan);
      final renewing = workflow.renew();
      final document = _Document();
      await expectLater(
        workflow.save(document),
        throwsA(isA<AccountException>()),
      );
      expect(workflow.state, HostedWorkflowState.conflict);
      pendingRenew.complete(
        const HostedPlanLease(leaseId: 'lease-1', revision: '1'),
      );
      await renewing;
      expect(workflow.state, HostedWorkflowState.conflict);
      expect(workflow.expectedRevision, '1');
      expect(workflow.recoveryBytes, [7]);
      await workflow.release();
      await workflow.dispose();
    },
  );

  test('checkout disposed while acquiring releases the late lease', () async {
    final client = _Client();
    final pendingAcquire = Completer<HostedPlanLease>();
    client.acquire = () => pendingAcquire.future;
    final workflow = _coordinator(client);
    final opening = workflow.checkout(_plan);
    await workflow.dispose();
    pendingAcquire.complete(_lease);
    await expectLater(opening, throwsA(isA<StateError>()));
    expect(client.calls, ['acquire:false', 'release']);
    expect(workflow.state, HostedWorkflowState.disposed);
    expect(workflow.lease, isNull);
  });

  test(
    'checkout disposed while downloading cannot install a lease or timer',
    () async {
      final client = _Client();
      final pendingDownload = Completer<HostedPlanDownload>();
      client.download = () => pendingDownload.future;
      final workflow = _coordinator(client);
      final opening = workflow.checkout(_plan);
      await Future<void>.delayed(Duration.zero);
      await workflow.dispose();
      pendingDownload.complete(client.downloadResult);
      await expectLater(opening, throwsA(isA<StateError>()));
      expect(client.calls, ['acquire:false', 'download', 'release']);
      expect(workflow.state, HostedWorkflowState.disposed);
      expect(workflow.lease, isNull);
    },
  );

  test(
    'release is excluded during save, then can detach after completion',
    () async {
      final client = _Client();
      final pendingSave = Completer<HostedPlan>();
      client.save = (_) => pendingSave.future;
      final workflow = _coordinator(client);
      await workflow.checkout(_plan);
      final saving = workflow.save(_Document());
      await expectLater(workflow.release(), throwsA(isA<StateError>()));
      await expectLater(workflow.save(_Document()), throwsA(isA<StateError>()));
      pendingSave.complete(client.saveResult);
      await saving;
      expect(await workflow.release(), isTrue);
      expect(workflow.state, HostedWorkflowState.idle);
      await workflow.dispose();
    },
  );

  test('approved detach proceeds when the lease already expired', () async {
    final client = _Client()
      ..release = () => Future.error(
        const AccountException('expired', code: 'lease-not-held'),
      );
    final workflow = _coordinator(client);
    await workflow.checkout(_plan);
    expect(await workflow.release(), isTrue);
    expect(workflow.lease, isNull);
    expect(workflow.state, HostedWorkflowState.idle);
    await workflow.dispose();
  });

  test(
    'successful late save does not hide lease loss reported by renewal',
    () async {
      final client = _Client();
      final pendingSave = Completer<HostedPlan>();
      client.save = (_) => pendingSave.future;
      client.renew = () => Future.error(
        const AccountException('lease expired', code: 'lease-not-held'),
      );
      client.release = () => Future.error(
        const AccountException('lease expired', code: 'lease-not-held'),
      );
      final workflow = _coordinator(client);
      await workflow.checkout(_plan);
      final saving = workflow.save(_Document());
      await expectLater(workflow.renew(), throwsA(isA<AccountException>()));
      expect(workflow.state, HostedWorkflowState.leaseLost);
      pendingSave.complete(client.saveResult);
      await saving;
      expect(workflow.state, HostedWorkflowState.leaseLost);
      expect(workflow.expectedRevision, '2');
      expect(await workflow.release(), isTrue);
      await workflow.dispose();
    },
  );
}
