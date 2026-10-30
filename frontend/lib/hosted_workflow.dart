import 'dart:async';
import 'dart:typed_data';

import 'account.dart';
import 'core/domowik_core.dart' show rejected;
import 'plan_session.dart';

/// A small seam around the editor's one authoritative package encoder. Tests
/// can model delayed uploads without constructing a native/WASM core.
abstract interface class HostedPlanDocument {
  HostedDocumentCapture capture();
  PlanFileResult markSaved(HostedDocumentCapture capture);
  bool get isDirty;
}

class HostedDocumentCapture {
  const HostedDocumentCapture({required this.bytes, required this.token});

  final Uint8List bytes;
  final Object token;
}

/// Adapts the existing PlanSession encode/markSaved boundary. Rust remains
/// responsible for package semantics and for deciding whether a captured
/// edit revision is still current after a network request.
class PlanSessionHostedDocument implements HostedPlanDocument {
  const PlanSessionHostedDocument(this.session);

  final PlanSession session;

  @override
  bool get isDirty => session.isDirty;

  @override
  HostedDocumentCapture capture() {
    final snapshot = session.captureForSave();
    return HostedDocumentCapture(bytes: snapshot.bytes, token: snapshot);
  }

  @override
  PlanFileResult markSaved(HostedDocumentCapture capture) {
    final token = capture.token;
    if (token is! PlanSessionSnapshot) {
      return const PlanFileResult(rejected, detail: 'invalid save capture');
    }
    return session.markSaved(token);
  }
}

enum HostedWorkflowState {
  idle,
  checkingOut,
  ready,
  saving,
  leaseLost,
  conflict,
  recoveryRequired,
  disposed,
}

class HostedPlanCheckout {
  const HostedPlanCheckout({
    required this.plan,
    required this.lease,
    required this.download,
  });

  final HostedPlan plan;
  final HostedPlanLease lease;
  final HostedPlanDownload download;
}

class HostedSaveResult {
  const HostedSaveResult({
    required this.revision,
    required this.localDocumentClean,
  });

  final String revision;

  /// False when the server accepted the captured bytes but the editor changed
  /// during the upload, or the open document was replaced before completion.
  final bool localDocumentClean;
}

/// Coordinates one browser writer checkout. It never opens or interprets
/// package bytes itself: callers open checkout bytes through PlanSession, and
/// hosted saves encode and mark saved through [HostedPlanDocument].
class HostedWorkflowCoordinator {
  HostedWorkflowCoordinator({
    required this.client,
    required this.session,
    this.renewalInterval,
    this.saveTimeout = const Duration(seconds: 30),
    this.onStateChanged,
  });

  final HostedPlanClient client;
  final AccountSession session;
  final Duration? renewalInterval;
  final Duration saveTimeout;
  final void Function(HostedWorkflowState state)? onStateChanged;

  HostedWorkflowState _state = HostedWorkflowState.idle;
  HostedPlan? _plan;
  HostedPlanLease? _lease;
  Timer? _renewalTimer;
  bool _renewing = false;
  bool _saving = false;
  Uint8List? _recoveryBytes;
  bool _disposed = false;
  int _checkoutGeneration = 0;

  HostedWorkflowState get state => _state;
  HostedPlan? get plan => _plan;
  HostedPlanLease? get lease => _lease;
  String? get expectedRevision => _lease == null ? null : _expectedRevision;
  Uint8List? get recoveryBytes =>
      _recoveryBytes == null ? null : Uint8List.fromList(_recoveryBytes!);

  String? _expectedRevision;

  void _setState(HostedWorkflowState state) {
    if (_state == state) return;
    _state = state;
    onStateChanged?.call(state);
  }

  /// Acquires the browser lease before downloading. A changed revision
  /// between those two requests is refused and the newly acquired lease is
  /// released, so a caller never opens bytes under the wrong revision.
  Future<HostedPlanCheckout> checkout(HostedPlan plan) async {
    _checkLive();
    if (_lease != null || _state == HostedWorkflowState.checkingOut) {
      throw StateError(
        'Release the current hosted plan before opening another',
      );
    }
    final generation = ++_checkoutGeneration;
    _setState(HostedWorkflowState.checkingOut);
    HostedPlanLease? acquired;
    try {
      acquired = await client.acquireLease(session, plan.id, force: false);
      _checkCheckout(generation);
      final download = await client.downloadPlan(session, plan.id);
      _checkCheckout(generation);
      if (download.revision != acquired.revision) {
        _setState(HostedWorkflowState.conflict);
        throw const AccountException(
          'The hosted plan changed while it was being opened',
          code: 'revision-conflict',
        );
      }
      _plan = plan;
      _lease = acquired;
      _expectedRevision = acquired.revision;
      _recoveryBytes = null;
      _setState(HostedWorkflowState.ready);
      _startRenewal(acquired);
      return HostedPlanCheckout(
        plan: plan,
        lease: acquired,
        download: download,
      );
    } on Object {
      if (acquired != null) {
        try {
          await client.releaseLease(session, plan.id, acquired.leaseId);
        } on Object {
          // The server expires leases; preserve the original checkout error.
        }
      }
      if (!_disposed &&
          generation == _checkoutGeneration &&
          _state != HostedWorkflowState.conflict) {
        _setState(HostedWorkflowState.recoveryRequired);
      }
      rethrow;
    }
  }

  /// Renews the current writer lease. Lease responses can include a revision,
  /// but they never advance or roll back the revision used for conditional
  /// content saves; only an acknowledged PUT does that.
  Future<void> renew() async {
    _checkLive();
    final plan = _plan;
    final lease = _lease;
    if (plan == null || lease == null || _renewing) return;
    _renewing = true;
    try {
      final renewed = await client.renewLease(session, plan.id, lease.leaseId);
      if (_disposed || _lease?.leaseId != lease.leaseId) return;
      _lease = HostedPlanLease(
        leaseId: lease.leaseId,
        revision: lease.revision,
        mode: renewed.mode,
        expiresInSeconds: renewed.expiresInSeconds,
      );
    } on Object {
      if (!_disposed && _lease?.leaseId == lease.leaseId) {
        _setState(HostedWorkflowState.leaseLost);
        _renewalTimer?.cancel();
      }
      rethrow;
    } finally {
      _renewing = false;
    }
  }

  /// Uploads one Rust-encoded snapshot with the current lease and exact
  /// expected revision. Failures keep a downloadable recovery copy and leave
  /// the core dirty. A later edit during upload remains dirty because the
  /// capture's core revision, rather than the upload's completion time, is
  /// passed back to PlanSession.
  Future<HostedSaveResult> save(HostedPlanDocument document) async {
    _checkLive();
    final plan = _plan;
    final lease = _lease;
    final expected = _expectedRevision;
    if (plan == null || lease == null || expected == null) {
      throw StateError('No hosted writer lease is active');
    }
    if (_state == HostedWorkflowState.leaseLost ||
        _state == HostedWorkflowState.conflict) {
      throw StateError('The hosted writer must be recovered before saving');
    }
    if (_saving || _state == HostedWorkflowState.saving) {
      throw StateError('A hosted save is already in progress');
    }
    final generation = _checkoutGeneration;
    final leaseId = lease.leaseId;
    final capture = document.capture();
    _saving = true;
    _recoveryBytes = capture.bytes;
    _setState(HostedWorkflowState.saving);
    try {
      final saved = await client
          .savePlanContent(session, plan.id, leaseId, expected, capture.bytes)
          .timeout(saveTimeout);
      if (_disposed ||
          generation != _checkoutGeneration ||
          _lease?.leaseId != leaseId) {
        return HostedSaveResult(
          revision: saved.revision,
          localDocumentClean: false,
        );
      }
      _expectedRevision = saved.revision;
      final mark = document.markSaved(capture);
      final clean = mark.succeeded && !document.isDirty;
      if (clean) {
        _recoveryBytes = null;
      } else {
        try {
          _recoveryBytes = document.capture().bytes;
        } on Object {
          // The original upload snapshot remains a usable recovery copy.
        }
      }
      if (_state != HostedWorkflowState.leaseLost &&
          _state != HostedWorkflowState.conflict) {
        _setState(
          clean
              ? HostedWorkflowState.ready
              : HostedWorkflowState.recoveryRequired,
        );
      }
      return HostedSaveResult(
        revision: saved.revision,
        localDocumentClean: clean,
      );
    } on Object catch (error) {
      if (!_disposed &&
          generation == _checkoutGeneration &&
          _lease?.leaseId == leaseId) {
        if (_state != HostedWorkflowState.leaseLost &&
            _state != HostedWorkflowState.conflict) {
          _setState(
            error is AccountException &&
                    (error.code == 'revision-conflict' ||
                        error.code == 'lease-not-held')
                ? (error.code == 'revision-conflict'
                      ? HostedWorkflowState.conflict
                      : HostedWorkflowState.leaseLost)
                : HostedWorkflowState.recoveryRequired,
          );
        }
        try {
          _recoveryBytes = document.capture().bytes;
        } on Object {
          // Keep the exact bytes already offered to the server as fallback.
        }
      }
      rethrow;
    } finally {
      _saving = false;
    }
  }

  /// Releases the lease after the caller has completed its dirty-workflow
  /// decision. The coordinator deliberately does not decide whether edits may
  /// be discarded; FileWorkflowController owns that interaction.
  Future<bool> release({bool bestEffort = true}) async {
    _checkLive();
    final plan = _plan;
    final lease = _lease;
    if (plan == null || lease == null) return true;
    if (_saving) throw StateError('Wait for the hosted save to finish');
    _renewalTimer?.cancel();
    _renewalTimer = null;
    try {
      await client
          .releaseLease(session, plan.id, lease.leaseId)
          .timeout(const Duration(seconds: 3));
      _clearReleasedLease();
      return true;
    } on AccountException catch (error) {
      if (error.code == 'lease-not-held') {
        _clearReleasedLease();
        return true;
      }
      // Detaching after an approved local recovery/discard must not trap the
      // user behind a transient release failure. The server lease will expire.
      if (bestEffort) {
        _clearReleasedLease();
      } else {
        _startRenewal(lease);
      }
      return false;
    } on Object {
      if (bestEffort) {
        _clearReleasedLease();
      } else {
        _startRenewal(lease);
      }
      return false;
    }
  }

  void _clearReleasedLease() {
    ++_checkoutGeneration;
    _renewalTimer?.cancel();
    _renewalTimer = null;
    _plan = null;
    _lease = null;
    _expectedRevision = null;
    _setState(HostedWorkflowState.idle);
  }

  void _startRenewal(HostedPlanLease lease) {
    final seconds = lease.expiresInSeconds ?? 90;
    final interval =
        renewalInterval ?? Duration(seconds: (seconds ~/ 2).clamp(10, 45));
    _renewalTimer?.cancel();
    _renewalTimer = Timer.periodic(interval, (_) {
      renew().ignore();
    });
  }

  void _checkLive() {
    if (_disposed) throw StateError('Hosted workflow is disposed');
  }

  void _checkCheckout(int generation) {
    if (_disposed || generation != _checkoutGeneration) {
      throw StateError('Hosted checkout was cancelled');
    }
  }

  /// Stops renewal and best-effort releases the lease. A network interruption
  /// cannot make the server forget the lease; expiry remains its backstop.
  Future<void> dispose() async {
    if (_disposed) return;
    final plan = _plan;
    final lease = _lease;
    _disposed = true;
    ++_checkoutGeneration;
    _renewalTimer?.cancel();
    _renewalTimer = null;
    _setState(HostedWorkflowState.disposed);
    if (plan != null && lease != null) {
      try {
        await client
            .releaseLease(session, plan.id, lease.leaseId)
            .timeout(const Duration(seconds: 3));
      } on Object {
        // The bounded lease expiry is the server-side cleanup path.
      }
    }
  }
}
