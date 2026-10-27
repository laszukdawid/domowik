// dart:ffi harness for the Domowik bridge spike (issue 0024).
//
// Loads the REAL shared library with `DynamicLibrary.open` — the same call
// Flutter makes — and drives the stateful seam, then measures the boundary.
//
// Run: dart run bin/harness.dart <path-to-libdomowik_bridge.dylib>
//
// This is plain Dart rather than a Flutter app on purpose. `DynamicLibrary`,
// `NativeCallable`, `Pointer` and the calling convention are the Dart VM's,
// not Flutter's, so everything about the BOUNDARY is proven here. What a
// Flutter app would add is bundle packaging — a separate question, recorded in
// the README as assessed rather than answered.

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:domowik_bridge_harness/domowik_bridge.dart';

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

int checks = 0;
int failures = 0;

void check(bool ok_, String what) {
  checks++;
  if (!ok_) {
    failures++;
    stderr.writeln('FAIL: $what');
  }
}

void checkEq(Object? got, Object? want, String what) {
  checks++;
  if (got != want) {
    failures++;
    stderr.writeln('FAIL: $what (got $got, want $want)');
  }
}

/// A request creating one level, with a distinct id per call.
int _seq = 0;
String levelRequest() {
  _seq++;
  final id = '0194${_seq.toRadixString(16).padLeft(4, '0')}'
      '-6b18-7d3a-a153-1ee2f33cc402';
  return jsonEncode({
    'operations': [
      {
        'operationId': id,
        'entityId': id,
        'kind': 'createEntity',
        'entity': {
          'type': 'level',
          'id': id,
          'name': 'Ground',
          'elevationMm': 0.0,
          'floorThicknessMm': 200.0,
          'defaultWallHeightMm': 2400.0,
          'defaultCeilingHeightMm': 2400.0,
        },
      }
    ]
  });
}

void main(List<String> args) {
  if (args.isEmpty) {
    stderr.writeln('usage: dart run bin/harness.dart <path-to-library>');
    exit(2);
  }
  final b = Bridge(args[0]);
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  final handleOut = alloc<Uint64>(sizeOf<Uint64>());

  checkEq(b.abiVersion(), 22, 'abi version');

  // ---- a stateful round trip ----
  checkEq(b.planCreateDefault(handleOut), ok, 'clean editor baseline');
  final h = handleOut.value;
  check(h != 0, 'handle is non-zero');

  final reqText = levelRequest();
  final reqPtr = toNative(reqText);
  checkEq(b.planExecute(h, reqPtr, utf8.encode(reqText).length, out), ok,
      'execute');
  var body = jsonDecode(b.takeString(out));
  checkEq(body['entities'], 2, 'execute added a level above baseline');
  checkEq(body['undo_depth'], 1, 'undo depth');
  final levelId = jsonDecode(reqText)['operations'][0]['entityId'] as String;
  release(reqPtr);

  // State persists across calls, which is the whole reason for a handle.
  checkEq(b.planSnapshot(h, out), ok, 'snapshot');
  check(b.takeString(out).contains('Ground'), 'snapshot sees the level');

  checkEq(b.planUndo(h, out), ok, 'undo');
  body = jsonDecode(b.takeString(out));
  checkEq(body['entities'], 1, 'undo preserves baseline');
  checkEq(body['redo_depth'], 1, 'redo depth');
  checkEq(b.planRedo(h, out), ok, 'redo');
  body = jsonDecode(b.takeString(out));
  checkEq(body['entities'], 2, 'redo restores level');
  checkEq(b.planState(h, out), ok, 'state');
  body = jsonDecode(b.takeString(out));
  checkEq(body['undo_depth'], 1, 'state reads live undo depth');
  checkEq(body['dirty'], true, 'state reads dirty');
  checkEq(body['document_revision'], 3,
      'accepted execute, undo and redo each advance revision');
  checkEq(b.planCreateDefault(handleOut), ok, 'default plan');
  final baseline = handleOut.value;
  checkEq(b.planState(baseline, out), ok, 'baseline state');
  body = jsonDecode(b.takeString(out));
  checkEq(body['entities'], 1, 'baseline has ground level');
  checkEq(body['undo_depth'], 0, 'baseline has no history');
  checkEq(body['dirty'], false, 'baseline is clean');
  checkEq(body['document_revision'], 0, 'baseline starts at revision zero');
  checkEq(b.planClose(baseline), ok, 'close baseline');

  // ---- snapping (ABI 12) ----
  {
    final snapHandleOut = alloc<Uint64>(sizeOf<Uint64>());
    checkEq(b.planCreateDefault(snapHandleOut), ok, 'snap baseline');
    final snapHandle = snapHandleOut.value;
    final snapSnapshotOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    checkEq(b.planSnapshot(snapHandle, snapSnapshotOut), ok,
        'snap baseline snapshot');
    final snapDocument =
        jsonDecode(b.takeString(snapSnapshotOut)) as Map<String, Object?>;
    final snapLevelId =
        (snapDocument['levels'] as List<Object?>).first as Map<String, Object?>;
    final levelForSnap = snapLevelId['id'] as String;
    final snapRequest = <String, Object?>{
      'levelId': levelForSnap,
      'pointMm': [0.0, 0.0],
      'radiusMm': 120.0,
      'visibleRectMm': [-1000.0, -1000.0, 1000.0, 1000.0],
      'gridSpacingMm': 100.0,
      'gridEnabled': true,
      'objectEnabled': true,
      'angleEnabled': true,
      'altDisabled': false,
    };
    final beforeSnap = jsonEncode(snapDocument);
    final snapStateOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    checkEq(b.planState(snapHandle, snapStateOut), ok, 'snap state before');
    final beforeState = jsonEncode(jsonDecode(b.takeString(snapStateOut)));
    final grid = b.snap(snapHandle, snapRequest);
    final candidates = grid['candidates'] as List<Object?>;
    check(
        candidates.isNotEmpty &&
            (() {
              final item = candidates.first as Map<String, Object?>;
              final point = (item['pointMm'] as List<Object?>).cast<num>();
              return item['class'] == 'grid' && point[0] == 0 && point[1] == 0;
            })(),
        'empty document grid query returns grid origin first');
    final repeated = b.snap(snapHandle, snapRequest);
    checkEq(jsonEncode(repeated), jsonEncode(grid),
        'repeated snap returns identical response');
    final disabled = b.snap(snapHandle, {
      ...snapRequest,
      'gridEnabled': false,
      'objectEnabled': false,
      'angleEnabled': false,
    });
    checkEq((disabled['candidates'] as List<Object?>).length, 0,
        'disabled snapping returns empty');
    // Wall exclusion is SUPPORTED (R6.1.3): the core decides it per
    // constant-owner boundary sub-span, so a request carrying
    // `excludedWallIds` is answered rather than refused. This id names no wall
    // on this level, which excludes nothing and is not an error — a caller may
    // legitimately hold a stale or foreign id — so the answer must match the
    // unexcluded query exactly.
    final excluded = b.snap(snapHandle, {
      ...snapRequest,
      'excludedWallIds': ['01947fff-6b18-7d3a-a153-1ee2f33cc700'],
    });
    checkEq(jsonEncode(excluded), jsonEncode(grid),
        'a wall id naming nothing on this level excludes nothing');
    checkEq(
        b.planSnapshot(snapHandle, snapSnapshotOut), ok, 'snap state snapshot');
    final afterSnap = jsonEncode(jsonDecode(b.takeString(snapSnapshotOut)));
    checkEq(afterSnap, beforeSnap, 'snap queries do not change state');
    checkEq(b.planState(snapHandle, snapStateOut), ok, 'snap state after');
    final afterState = jsonEncode(jsonDecode(b.takeString(snapStateOut)));
    checkEq(afterState, beforeState, 'snap queries preserve plan state');
    checkEq(b.planClose(snapHandle), ok, 'close snap baseline');
    release(snapStateOut);
    release(snapSnapshotOut);
    release(snapHandleOut);
  }

  // ---- hit testing (ABI 9) ----
  {
    const wallId = '01947fff-6b18-7d3a-a153-1ee2f33cc700';
    final wallText = jsonEncode({
      'operations': [
        {
          'operationId': '01947ffe-6b18-7d3a-a153-1ee2f33cc700',
          'entityId': wallId,
          'kind': 'createEntity',
          'entity': {
            'type': 'wall',
            'id': wallId,
            'levelId': levelId,
            'startMm': [0.0, 0.0],
            'endMm': [4000.0, 0.0],
            'thicknessMm': 200.0,
            'heightMm': 2500.0,
          },
        }
      ],
    });
    final wallPtr = toNative(wallText);
    checkEq(b.planExecute(h, wallPtr, utf8.encode(wallText).length, out), ok,
        'create a wall to hit');
    b.takeString(out);
    release(wallPtr);

    String request(double x, double y, double radius) => jsonEncode({
          'levelId': levelId,
          'pointMm': [x, y],
          'radiusMm': radius,
        });

    // The decoded candidates of the last successful call.
    List<Object?> hits = const [];
    int hitTest(String text) {
      final ptr = toNative(text);
      try {
        final code = b.planHitTest(h, ptr, utf8.encode(text).length, out);
        hits = code == ok
            ? jsonDecode(b.takeString(out)) as List<Object?>
            : const [];
        return code;
      } finally {
        release(ptr);
      }
    }

    // On the centre line, 2000 mm from either endpoint, so the wall's REGION
    // is the only candidate inside a 120 mm radius.
    checkEq(hitTest(request(2000.0, 0.0, 120.0)), ok, 'hit test');
    checkEq(hits.length, 1, 'one candidate on the centre line');
    final hit = hits.first as Map<String, Object?>;
    checkEq(hit['class'], 'region', 'the wall region is the candidate');
    checkEq(hit['entityId'], wallId, 'the candidate names the wall');
    checkEq(hit['distanceSquaredMm2'], 0.0, 'exactly zero, squared');

    // Built once, reused after. A cache that rebuilt on every query would
    // return byte-identical answers, so a check on the RESULTS passes on the
    // broken implementation; only the counter can see the difference.
    final builds = b.debugHitIndexBuilds(h);
    checkEq(builds, 1, 'the index was built exactly once');
    checkEq(hitTest(request(2000.0, 0.0, 120.0)), ok, 'hit test again');
    checkEq(b.debugHitIndexBuilds(h), builds, 'the index was REUSED');

    // An edit drops it, and the answer changes with the document.
    checkEq(b.planUndo(h, out), ok, 'undo the wall');
    b.takeString(out);
    checkEq(hitTest(request(2000.0, 0.0, 120.0)), ok, 'hit test after undo');
    checkEq(b.debugHitIndexBuilds(h), builds + 1, 'an edit drops the index');
    checkEq(hits.isEmpty, true, 'the undone wall is no longer hit');
    checkEq(b.planRedo(h, out), ok, 'redo the wall');
    b.takeString(out);

    // Far outside the plan domain: answered with nothing, and answered
    // WITHOUT building or querying an index. Panning a long way is ordinary.
    checkEq(hitTest(request(2000.0, 0.0, 120.0)), ok, 'hit test after redo');
    final settled = b.debugHitIndexBuilds(h);
    checkEq(hitTest(request(1e9, 0.0, 120.0)), ok,
        'an out-of-domain point is answered, not refused');
    checkEq(hits.isEmpty, true, 'the answer is the empty array');
    checkEq(b.debugHitIndexBuilds(h), settled,
        'an out-of-domain query did no index work');

    // 240000 mm is 24 logical px at the 0.0001 px/mm zoom floor: the widest
    // radius a legal viewport can ask for. Inclusive.
    checkEq(hitTest(request(0.0, 0.0, 240000.0)), ok,
        'the inclusive radius limit is accepted');
    for (final bad in [
      request(0.0, 0.0, 240000.1),
      request(0.0, 0.0, -1.0),
      '{"levelId":"$levelId","pointMm":[1e400,0],"radiusMm":120}',
      '{"levelId":"$levelId","pointMm":[0,0],"radiusMm":1e400}',
      '{"levelId":"not-a-uuid","pointMm":[0,0],"radiusMm":120}',
      '{not json',
    ]) {
      checkEq(hitTest(bad), malformedRequest, 'refused: $bad');
    }

    final probe = request(0.0, 0.0, 120.0);
    final probePtr = toNative(probe);
    for (final forged in [0, 999999, 0xdeadbeef]) {
      checkEq(b.planHitTest(forged, probePtr, utf8.encode(probe).length, out),
          unknownHandle, 'forged handle $forged');
    }
    release(probePtr);
    checkEq(b.debugHitIndexBuilds(0xdeadbeef), -1,
        'the debug probe reports an unknown handle as -1');
  }

  // ---- ownership abuse, from a language with no borrow checker ----
  checkEq(b.planSnapshot(h, out), ok, 'snapshot for the free probes');
  final issued = out.ref.token;
  checkEq(b.bufferFree(issued), ok, 'first free succeeds');
  checkEq(b.bufferFree(issued), unknownBuffer, 'double free is REFUSED');

  // Release takes a TOKEN, so Dart has no pointer to get wrong. Any integer is
  // safe to pass and is refused rather than acted on.
  for (final forged in [0, 1, issued + 1, 0x7fffffffffffffff]) {
    checkEq(
        b.bufferFree(forged), unknownBuffer, 'forged token $forged refused');
  }

  // ---- forged and stale handles ----
  for (final forged in [0, 999999, 0xdeadbeef, 0x7fffffffffffffff]) {
    checkEq(
        b.planSnapshot(forged, out), unknownHandle, 'forged handle $forged');
  }
  checkEq(b.planClose(h), ok, 'close');
  checkEq(b.planClose(h), unknownHandle, 'double close detected');
  checkEq(b.planSnapshot(h, out), unknownHandle, 'stale handle detected');

  checkEq(b.planCreate(handleOut), ok, 'second plan');
  final h2 = handleOut.value;
  check(h2 != h, 'handles are never reissued');

  // ---- malformed input and the sticky error ----
  final junk = toNative('{not json');
  checkEq(
      b.planExecute(h2, junk, 9, out), malformedRequest, 'malformed request');
  release(junk);
  checkEq(b.lastError(h2, out), ok, 'last_error');
  final first = b.takeString(out);
  check(first.isNotEmpty, 'a reason was recorded');
  checkEq(b.lastError(h2, out), ok, 'last_error again');
  checkEq(b.takeString(out), first, 'the error is sticky and unchanged');

  // ---- panic containment, into a Dart frame ----
  checkEq(b.debugPanic(), panicContained, 'panic contained');
  // "Contained" has to mean the library still works, not merely that the
  // process survived the return.
  final afterText = levelRequest();
  final afterPtr = toNative(afterText);
  checkEq(b.planExecute(h2, afterPtr, utf8.encode(afterText).length, out), ok,
      'usable after a contained panic');
  b.takeString(out);
  release(afterPtr);

  // ---- cancellation ----
  final started = DateTime.now();
  // `busy` blocks this isolate, so only the completing case is exercised from
  // Dart. The cross-thread cancel is covered in `charness/main.c`, which now
  // genuinely does it — this comment previously claimed that coverage while
  // the C harness only ran `busy` on its main thread.
  checkEq(b.busy(h2, 5), ok, 'a short busy run completes');
  check(DateTime.now().difference(started).inSeconds < 5,
      'busy returned promptly');

  // A cancel is recorded and never walked back by a later operation.
  checkEq(b.cancel(h2), ok, 'cancel records');
  final epoch = b.debugCancelEpoch(h2);
  check(epoch > 0, 'the cancel was recorded');
  checkEq(b.busy(h2, 0), ok, 'an operation runs after a cancel');
  checkEq(
      b.debugCancelEpoch(h2), epoch, 'no operation erased the cancellation');

  // ---- measurements ----
  _measure(b);

  checkEq(b.planClose(h2), ok, 'close h2');
  checkEq(b.outstandingBuffers(), 0, 'no buffer left outstanding');
  checkEq(b.openPlans(), 0, 'no plan left open');

  release(out);
  release(handleOut);

  stdout.writeln('$checks checks, $failures failures');
  if (failures == 0) {
    stdout.writeln('DART HARNESS PASSED');
  }
  exit(failures == 0 ? 0 : 1);
}

/// Boundary measurements, printed as `MEASURE <name> <value> <unit>` so a
/// script can scrape them without parsing prose.
///
/// Each number isolates ONE cost. The first draft reported a single
/// "round trip" that silently mixed three: the FFI hop, Dart's own JSON
/// encoding, and the core's per-commit cost growing as the measurement itself
/// added 2,000 operations to one plan. That number was real and told you
/// nothing about where the time went.
void _measure(Bridge b) {
  // 1. The boundary FLOOR: a nullary export that returns a constant.
  //    Everything else is this plus work.
  const nullaryIterations = 200000;
  var sw = Stopwatch()..start();
  for (var i = 0; i < nullaryIterations; i++) {
    b.abiVersion();
  }
  sw.stop();
  final nullaryNs = sw.elapsedMicroseconds * 1000 / nullaryIterations;
  stdout.writeln('MEASURE nullary_call ${nullaryNs.toStringAsFixed(1)} ns');

  // 2. Dart's own JSON encoding, measured ALONE, so the round trip below can
  //    be attributed rather than guessed at. This never touches the boundary.
  const encodeIterations = 20000;
  sw = Stopwatch()..start();
  for (var i = 0; i < encodeIterations; i++) {
    levelRequest();
  }
  sw.stop();
  final encodeUs = sw.elapsedMicroseconds / encodeIterations;
  stdout
      .writeln('MEASURE caller_json_encode ${encodeUs.toStringAsFixed(2)} us');

  // 3. One operation per call on a plan with SHORT history, which is the
  //    interactive case: a user drawing into a document that is not enormous.
  //    A fresh plan per round keeps the core's per-commit cost out of the
  //    number — issue 0038 measures that separately, and mixing them here
  //    would hide both.
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  final handleOut = alloc<Uint64>(sizeOf<Uint64>());
  const rounds = 40;
  const opsPerRound = 25;
  sw = Stopwatch()..start();
  for (var r = 0; r < rounds; r++) {
    b.planCreate(handleOut);
    final h = handleOut.value;
    for (var i = 0; i < opsPerRound; i++) {
      final text = levelRequest();
      final ptr = toNative(text);
      final code = b.planExecute(h, ptr, utf8.encode(text).length, out);
      if (code != ok) throw StateError('execute failed with $code');
      b.takeString(out);
      release(ptr);
    }
    b.planClose(h);
  }
  sw.stop();
  final shortUs = sw.elapsedMicroseconds / (rounds * opsPerRound);
  stdout.writeln(
      'MEASURE single_op_short_history ${shortUs.toStringAsFixed(2)} us');

  // 4. The same call on a plan whose history grows to 2,000, so the shape of
  //    the core's cost is visible from the far side of the boundary rather
  //    than only in the core's own benchmark.
  b.planCreate(handleOut);
  final grow = handleOut.value;
  const growIterations = 2000;
  sw = Stopwatch()..start();
  for (var i = 0; i < growIterations; i++) {
    final text = levelRequest();
    final ptr = toNative(text);
    final code = b.planExecute(grow, ptr, utf8.encode(text).length, out);
    if (code != ok) throw StateError('execute failed with $code');
    b.takeString(out);
    release(ptr);
  }
  sw.stop();
  final growUs = sw.elapsedMicroseconds / growIterations;
  stdout.writeln('MEASURE single_op_to_2000 ${growUs.toStringAsFixed(2)} us');
  b.planClose(grow);

  // 5. Twenty operations per call, pricing batching against the same work.
  b.planCreate(handleOut);
  final batched = handleOut.value;
  const batchRounds = 100;
  const batchSize = 20;
  sw = Stopwatch()..start();
  for (var i = 0; i < batchRounds; i++) {
    final ops = <Map<String, Object?>>[];
    for (var j = 0; j < batchSize; j++) {
      ops.add(jsonDecode(levelRequest())['operations'][0]);
    }
    final text = jsonEncode({'operations': ops});
    final ptr = toNative(text);
    final code = b.planExecute(batched, ptr, utf8.encode(text).length, out);
    if (code != ok) throw StateError('batched execute failed with $code');
    b.takeString(out);
    release(ptr);
  }
  sw.stop();
  final perOpUs = sw.elapsedMicroseconds / (batchRounds * batchSize);
  stdout.writeln('MEASURE batched20_per_op ${perOpUs.toStringAsFixed(2)} us');
  b.planClose(batched);

  release(out);
  release(handleOut);
}
