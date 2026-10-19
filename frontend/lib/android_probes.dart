// Two probes that only Android needs, and that only Android can answer.
//
// The shared bundle self-test asks whether the packaged core WORKS. These ask
// the two questions that are specific to this platform:
//
//   LIFECYCLE. Android may stop, destroy and recreate the thing holding a
//   native handle, which neither macOS nor Linux does to a desktop window. A
//   plan handle is a registry entry in the Rust bridge that only
//   `domowik_plan_close` removes (finding F6), so an app that leaks an activity
//   leaks a document, and the leak is invisible from the Dart side unless
//   something counts. This probe counts, across real transitions driven from
//   outside the process.
//
//   COST. The recorded call-overhead numbers are macOS/arm64 figures taken
//   through a desktop Dart runtime. They say nothing about this platform, and
//   guessing across a boundary is what the macOS README already refuses to do
//   about the Go numbers. These run the same shapes on the emulated device.
//
// Both live here rather than in `main.dart` because they are Android's, and
// because the editor's own boot path should not grow a platform's test code.
library;

import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:flutter/widgets.dart';

/// Probe token for the lifecycle probe.
const String lifecycleProbe = 'lifecycle';

/// Probe token for the on-device measurements.
const String benchmarkProbe = 'benchmark';

int _seq = 0;

String _mint(int n) {
  final s = n.toRadixString(16).padLeft(4, '0');
  return '0194$s-6b18-7d3a-a153-1ee2f33cc4${(n % 256).toRadixString(16).padLeft(2, '0')}';
}

Map<String, Object?> _levelBatch(String levelId) => {
  'operations': [
    {
      'operationId': _mint(++_seq),
      'entityId': levelId,
      'kind': 'createEntity',
      'entity': {
        'type': 'level',
        'id': levelId,
        'name': 'Ground',
        'elevationMm': 0.0,
        'floorThicknessMm': 200.0,
        'defaultWallHeightMm': 2500.0,
        'defaultCeilingHeightMm': 2500.0,
      },
    },
  ],
};

Map<String, Object?> _wallOperation(String levelId, String wallId, int index) => {
  'operationId': _mint(++_seq),
  'entityId': wallId,
  'kind': 'createEntity',
  'entity': {
    'type': 'wall',
    'id': wallId,
    'levelId': levelId,
    'startMm': [index * 100.0, 0.0],
    'endMm': [index * 100.0 + 100.0, 0.0],
    'thicknessMm': 200.0,
    'heightMm': 2500.0,
  },
};

Map<String, Object?> _wallBatch(String levelId, String wallId, int index) => {
  'operations': [_wallOperation(levelId, wallId, index)],
};

/// Sends one batch and releases whatever it produced.
int _execute(Bridge bridge, int handle, Map<String, Object?> batch) {
  final text = jsonEncode(batch);
  final bytes = utf8.encode(text);
  final ptr = toNative(text);
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    final code = bridge.planExecute(handle, ptr, bytes.length, out);
    if (code == ok) {
      bridge.takeString(out);
    }
    return code;
  } finally {
    release(ptr);
    release(out);
  }
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

/// Holds a live plan across Android lifecycle transitions and reports the
/// bridge's own counters at each one.
///
/// The counters are the point. A transition that "looked fine" proves nothing;
/// `openPlans` and `outstandingBuffers` are the two numbers that would move if
/// a configuration change quietly built a second session or stranded a buffer,
/// and they are read from the core rather than tracked in Dart, so a Dart-side
/// bookkeeping error cannot hide a native leak.
class _LifecycleProbe with WidgetsBindingObserver {
  _LifecycleProbe(this._bridge, this._handle, this._levelId);

  final Bridge _bridge;
  final int _handle;
  final String _levelId;
  final List<String> _seen = [];

  /// Distinct physical sizes seen. Rotation is only PROVEN by more than one.
  final Set<String> _sizes = {};

  /// Transitions after which the core could not be re-driven.
  int _brokenAfter = 0;

  void start() {
    WidgetsBinding.instance.addObserver(this);
    _report('start');
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    _seen.add(state.name);
    _report('lifecycle:${state.name}');
    // SELF-TERMINATING, on the condition that matters rather than on a clock.
    //
    // The first version was told when to stop by the harness touching a file in
    // the app's cache directory. That was dropped: whether the app had been
    // paused, recreated or restarted changed whether anything was still polling
    // for the file, so a run that did not finish could not be told apart from a
    // run that finished badly -- and it is exactly the pause/recreate behaviour
    // under test that decided which. The probe now ends when it has itself
    // observed a pause followed by a return to resumed, which is the full round
    // trip and needs nobody outside the process to notice anything.
    if (state == AppLifecycleState.resumed && _seen.contains('paused')) {
      finish();
    }
  }

  @override
  void didChangeMetrics() {
    // Rotation arrives here, not as a lifecycle state: the manifest declares
    // `configChanges` including orientation and screenSize, so Android hands
    // the change to the running activity instead of destroying it. That is a
    // fact about this app's configuration, so the probe reports the size it
    // actually sees rather than asserting an orientation it assumed.
    final view = WidgetsBinding.instance.platformDispatcher.views.first;
    final size = view.physicalSize;
    final described = '${size.width.toInt()}x${size.height.toInt()}';
    _sizes.add(described);
    _report('metrics:$described');
  }

  /// Re-drives the core after a transition. A handle that merely still EXISTS
  /// is not a handle that still works.
  void _report(String what) {
    final wall = _mint(++_seq);
    final code = _execute(_bridge, _handle, _wallBatch(_levelId, wall, _seq));
    // COUNTED, not merely printed. This line used to report the status code
    // and let the verdict ignore it, so a transition after which every call
    // failed would still have produced a PASSED run.
    if (code != ok) {
      _brokenAfter++;
    }
    stdout.writeln(
      'LIFECYCLE $what '
      'execute=$code '
      'openPlans=${_bridge.openPlans()} '
      'outstandingBuffers=${_bridge.outstandingBuffers()}',
    );
  }

  /// Prints the verdict and exits.
  void finish() {
    WidgetsBinding.instance.removeObserver(this);
    final closed = _bridge.planClose(_handle);
    final plans = _bridge.openPlans();
    final buffers = _bridge.outstandingBuffers();
    stdout.writeln('LIFECYCLE transitions seen: ${_seen.join(',')}');
    stdout.writeln('LIFECYCLE sizes seen: ${_sizes.join(',')}');
    stdout.writeln('LIFECYCLE close=$closed openPlans=$plans buffers=$buffers '
        'brokenAfter=$_brokenAfter');
    // ROTATION IS PART OF THE VERDICT.
    //
    // The first version required only that some transition had been seen,
    // which `finish` is reached by observing anyway -- an assertion that could
    // not fail where it was evaluated. Worse, nothing in the verdict depended
    // on a rotation at all, so a harness that forgot to rotate would still have
    // produced a run reported as evidence that rotation was survived. Two
    // distinct physical sizes is what makes that claim load-bearing.
    final rotated = _sizes.length >= 2;
    final good = closed == ok &&
        plans == 0 &&
        buffers == 0 &&
        _brokenAfter == 0 &&
        rotated &&
        _seen.contains('paused') &&
        _seen.contains('resumed');
    stdout.writeln(
      good
          ? 'LIFECYCLE PROBE PASSED'
          : 'LIFECYCLE PROBE FAILED: close=$closed plans=$plans '
                'buffers=$buffers brokenAfter=$_brokenAfter '
                'distinctSizes=${_sizes.length} '
                'transitions=${_seen.join(',')}',
    );
    exit(good ? 0 : 1);
  }
}

/// Runs the lifecycle probe. Returns without exiting: the verdict comes later,
/// after the harness has driven the transitions.
void runLifecycleProbe() {
  WidgetsFlutterBinding.ensureInitialized();
  final bridge = openBridge();
  final handleOut = alloc<Uint64>(sizeOf<Uint64>());
  final int handle;
  try {
    if (bridge.planCreate(handleOut) != ok) {
      stdout.writeln('LIFECYCLE PROBE FAILED: could not create a plan');
      exit(1);
    }
    handle = handleOut.value;
  } finally {
    release(handleOut);
  }
  final levelId = _mint(9000);
  _execute(bridge, handle, _levelBatch(levelId));

  final probe = _LifecycleProbe(bridge, handle, levelId);
  probe.start();
  runApp(const ColoredBox(color: Color(0xFF28796C)));
}

// ---------------------------------------------------------------------------
// Measurements
// ---------------------------------------------------------------------------

/// Median of [samples] nanosecond timings, which is what gets reported.
///
/// A mean over an emulated device is dominated by scheduler noise; the spread
/// is printed alongside so a reader can see how much to trust the middle.
String _summarise(List<int> ns) {
  ns.sort();
  final median = ns[ns.length ~/ 2];
  return '${median}ns (min ${ns.first}, max ${ns.last}, n=${ns.length})';
}

/// Measures the shapes the macOS table measures, on the device this runs on.
void runBenchmarkProbe() {
  WidgetsFlutterBinding.ensureInitialized();
  final bridge = openBridge();
  stdout.writeln('BENCHMARK abi=${bridge.abiVersion()}');

  // 1. The boundary floor: a nullary export that does nothing but return.
  const warmup = 2000;
  const rounds = 20000;
  // The results are ACCUMULATED and printed. An unused return value invites the
  // reader to wonder whether the call was optimised away; consuming it settles
  // the question instead of arguing about what the compiler may do.
  var sink = 0;
  for (var i = 0; i < warmup; i++) {
    sink += bridge.abiVersion();
  }
  final nullary = <int>[];
  for (var round = 0; round < 21; round++) {
    final watch = Stopwatch()..start();
    for (var i = 0; i < rounds; i++) {
      sink += bridge.abiVersion();
    }
    watch.stop();
    nullary.add((watch.elapsedMicroseconds * 1000) ~/ rounds);
  }
  stdout.writeln('BENCHMARK nullary_export ${_summarise(nullary)}');
  stdout.writeln('BENCHMARK nullary_checksum $sink');

  // 2. Dart's own JSON encoding, with no boundary involved. Reported because
  //    the macOS round-trip number was once read as an FFI cost when most of
  //    it was never the boundary at all.
  final sampleBatch = _wallBatch(_mint(9000), _mint(9001), 1);
  const encodes = 1000;
  final encode = <int>[];
  for (var round = 0; round < 21; round++) {
    final watch = Stopwatch()..start();
    for (var i = 0; i < encodes; i++) {
      jsonEncode(sampleBatch);
    }
    watch.stop();
    // Nanoseconds PER ENCODE, the same unit as every other row here. Written
    // out rather than simplified: for this particular count the two factors
    // cancel, and a reader who simplified it would be left with a number whose
    // unit depends on a loop bound somewhere else in the function.
    encode.add((watch.elapsedMicroseconds * 1000) ~/ encodes);
  }
  stdout.writeln('BENCHMARK json_encode_1000x ${_summarise(encode)}');

  // 3. One operation at a time, averaged over a plan growing to 100 walls.
  //
  // Named for what it does rather than for the interactive case it stands in
  // for: the history is short at the start of each round and 100 operations
  // long at the end, so the figure is an average over that growth and not a
  // steady-state cost at any one depth.
  const singleOps = 100;
  final singleOp = <int>[];
  for (var round = 0; round < 11; round++) {
    final handleOut = alloc<Uint64>(sizeOf<Uint64>());
    bridge.planCreate(handleOut);
    final handle = handleOut.value;
    release(handleOut);
    final levelId = _mint(9000);
    _execute(bridge, handle, _levelBatch(levelId));
    final watch = Stopwatch()..start();
    for (var i = 0; i < singleOps; i++) {
      _execute(bridge, handle, _wallBatch(levelId, _mint(++_seq), i));
    }
    watch.stop();
    singleOp.add((watch.elapsedMicroseconds * 1000) ~/ singleOps);
    bridge.planClose(handle);
  }
  stdout.writeln('BENCHMARK single_op_to_100 ${_summarise(singleOp)}');

  // 4. The same work batched, which is the comparison that decides whether
  //    batching is worth its complexity on this platform too.
  const perBatch = 20;
  final batched = <int>[];
  for (var round = 0; round < 11; round++) {
    final handleOut = alloc<Uint64>(sizeOf<Uint64>());
    bridge.planCreate(handleOut);
    final handle = handleOut.value;
    release(handleOut);
    final levelId = _mint(9000);
    _execute(bridge, handle, _levelBatch(levelId));
    final watch = Stopwatch()..start();
    for (var i = 0; i < 5; i++) {
      final operations = <Map<String, Object?>>[
        for (var j = 0; j < perBatch; j++)
          _wallOperation(levelId, _mint(++_seq), i * perBatch + j),
      ];
      _execute(bridge, handle, {'operations': operations});
    }
    watch.stop();
    batched.add((watch.elapsedMicroseconds * 1000) ~/ (5 * perBatch));
    bridge.planClose(handle);
  }
  stdout.writeln('BENCHMARK batched_${perBatch}_per_op ${_summarise(batched)}');

  stdout.writeln('BENCHMARK openPlans=${bridge.openPlans()} '
      'outstandingBuffers=${bridge.outstandingBuffers()}');
  final clean = bridge.openPlans() == 0 && bridge.outstandingBuffers() == 0;
  stdout.writeln(clean ? 'BENCHMARK PROBE PASSED' : 'BENCHMARK PROBE FAILED: leaked');
  exit(clean ? 0 : 1);
}
