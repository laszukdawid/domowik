// Headless self-verification, run from inside the built application bundle.
//
// WHY THIS EXISTS. The claim this spike now has to support is "a Flutter macOS
// app loads and drives the Rust core from its own bundle". Evidence for that
// cannot be "the author looked at a window": the adversarial review of this
// spike's previous round found that its weakest findings were the ones whose
// evidence would have survived the bug. So the bundle verifies itself, prints
// what it checked, and exits non-zero when anything fails.
//
// It runs BEFORE `runApp`, so no window and no display are needed. Triggered
// by the environment rather than by `main`'s arguments, because a Flutter
// desktop entry point does not receive the process's command line.

import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';

/// Set this to `1` in the environment to run the self-test and exit.
const String selftestVariable = 'DOMOWIK_SPIKE_SELFTEST';

/// True when the process was asked to self-verify instead of showing a window.
bool get selftestRequested => Platform.environment[selftestVariable] == '1';

int _checks = 0;
int _failures = 0;

void _check(bool passed, String what) {
  _checks++;
  if (passed) {
    stdout.writeln('  ok   $what');
  } else {
    _failures++;
    stdout.writeln('  FAIL $what');
  }
}

void _checkEq(Object? got, Object? want, String what) {
  _check(got == want, '$what (got $got, want $want)');
}

String _mint(int n) {
  final s = n.toRadixString(16).padLeft(4, '0');
  return '0194$s-6b18-7d3a-a153-1ee2f33cc4${(n % 256).toRadixString(16).padLeft(2, '0')}';
}

int _seq = 0;

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

Map<String, Object?> _wallBatch(
  String levelId,
  String wallId,
  List<double> start,
  List<double> end, {
  double thicknessMm = 200.0,
}) => {
  'operations': [
    {
      'operationId': _mint(++_seq),
      'entityId': wallId,
      'kind': 'createEntity',
      'entity': {
        'type': 'wall',
        'id': wallId,
        'levelId': levelId,
        'startMm': start,
        'endMm': end,
        'thicknessMm': thicknessMm,
        'heightMm': 2500.0,
      },
    },
  ],
};

/// Sends one batch, releasing whatever it produced. Returns the status code.
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

Map<String, Object?> _snapshot(Bridge bridge, int handle) {
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    final code = bridge.planSnapshot(handle, out);
    if (code != ok) {
      throw StateError('snapshot returned $code');
    }
    return jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
  } finally {
    release(out);
  }
}

int _wallCount(Bridge bridge, int handle) =>
    (_snapshot(bridge, handle)['walls'] as List<Object?>? ?? const []).length;

/// The directory a loaded native image must be UNDER for this process to
/// have loaded its own copy, or `null` when no such directory can be named.
///
/// This is the reference the provenance check compares `dladdr`'s answer
/// against, so getting it wrong in either direction matters: too broad and the
/// check passes on a stray library elsewhere on the machine, too narrow and it
/// fails on a correctly packaged app.
///
/// Split out from [bundleRoot] and given its inputs as ARGUMENTS so it can be
/// tested for every platform from any host. [Platform.resolvedExecutable]
/// answers for the machine running the test, so a macOS test run could never
/// reach the Linux branch.
///
/// [platformProvidedRoot], when given, wins. Android needs it: the loaded
/// image lives inside the APK, whose path carries two install-scoped random
/// segments, and it is not derivable from the executable path — on Android
/// [Platform.resolvedExecutable] is the zygote, `/system/bin/app_process64`,
/// which says nothing about the application. Only the platform side can
/// report it. Passing it in keeps the comparison itself identical on every
/// platform, so there is no Android branch in the check.
String? bundleRootFor(
  String operatingSystem,
  String resolvedExecutable, {
  String? platformProvidedRoot,
}) {
  if (platformProvidedRoot != null) {
    return platformProvidedRoot;
  }
  final parts = resolvedExecutable.split('/');
  switch (operatingSystem) {
    case 'macos':
      // `<root>.app/Contents/MacOS/<binary>` is three levels below the root.
      if (parts.length < 4) {
        return null;
      }
      final root = parts.sublist(0, parts.length - 3).join('/');
      return root.endsWith('.app') ? root : null;
    case 'linux':
      // A Flutter Linux bundle puts the executable at the bundle ROOT, with
      // `lib/` and `data/` beside it, so the root is simply the containing
      // directory. There is no `.app`-style suffix to confirm it with, which
      // makes this weaker than the macOS case; the check it feeds is still
      // meaningful, because a library loaded from `/usr/lib` fails it.
      if (parts.length < 2) {
        return null;
      }
      return parts.sublist(0, parts.length - 1).join('/');
    default:
      return null;
  }
}

/// Environment variables the Android runner publishes before the engine starts.
///
/// [androidApkVariable] is the one the provenance check uses. The other is
/// published too and is deliberately NOT used: see [bundleRoot].
const String androidApkVariable = 'DOMOWIK_ANDROID_APK';
const String androidNativeLibDirVariable = 'DOMOWIK_ANDROID_NATIVE_LIB_DIR';

/// The directory this process must have loaded its native core from.
///
/// On Android the answer is the APK FILE, not a directory, and not the
/// application's native library directory. That is the counter-intuitive part
/// of this platform and it is worth stating because the obvious choice is
/// wrong: with `extractNativeLibs=false` nothing is ever extracted, the loader
/// maps the library straight out of the APK, and the native library directory
/// EXISTS AND IS EMPTY. Observed on a named ARM64 emulator, not a phone:
///
///     nativeLibraryDir  /data/app/~~<i>/<pkg>-<i>/lib/arm64        (empty)
///     loaded image      /data/app/~~<i>/<pkg>-<i>/base.apk!/lib/arm64-v8a/...
///
/// so a predicate written against the native library directory FAILS on a
/// correctly packaged APK. The APK path is also the tighter of the two
/// available answers: it proves the library came out of THIS ARCHIVE, where
/// the containing directory would also accept a stray `.so` dropped beside it.
///
/// Both `~~<i>` segments change on every install, so nothing here can be
/// pinned and the value has to come from the platform. It arrives through the
/// process environment, set before the Flutter engine starts.
///
/// When the variable is absent on Android this returns null and the check
/// FAILS rather than passing vacuously. A runner that failed to publish is
/// then indistinguishable from one that never did, which is the right
/// direction to fail in; the probe log prints both variables so the log says
/// which it was.
String? bundleRoot() => bundleRootFor(
  Platform.operatingSystem,
  Platform.resolvedExecutable,
  platformProvidedRoot: Platform.environment[androidApkVariable],
);

/// The derived-geometry response as a comparable string.
String _derivedJson(Bridge bridge, int handle, String levelId) {
  final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  final id = toNative('$levelId\u0000');
  try {
    if (bridge.planDerived(handle, id, out) != ok) return '<failed>';
    return bridge.takeString(out);
  } finally {
    release(id);
    release(out);
  }
}

/// Runs every check and returns the process exit code.
int runSelftest() {
  stdout.writeln('Domowik bridge spike — bundle self-test');
  stdout.writeln('  executable: ${Platform.resolvedExecutable}');

  final Bridge bridge;
  try {
    bridge = openBridge();
  } on Object catch (e) {
    stdout.writeln('  FAIL the framework did not load from the bundle: $e');
    stdout.writeln('1 check, 1 failure');
    stdout.writeln('BUNDLE SELFTEST FAILED');
    return 1;
  }
  // Where did that code actually come from? `DynamicLibrary.open` succeeding
  // says nothing: an identically named framework elsewhere on the machine
  // satisfies it, and the check would pass on the exact bug it exists to
  // catch. `dladdr` reports the image the loader really used.
  final image = bridge.loadedImagePath();
  final bundle = bundleRoot();
  _check(image != null, 'the loaded image can be named ($image)');
  _check(
    image != null && bundle != null && image.startsWith(bundle),
    'the core was loaded from INSIDE this bundle\n'
    '         image:  $image\n'
    '         bundle: $bundle',
  );
  _checkEq(bridge.abiVersion(), 16, 'abi version');

  final handleOut = alloc<Uint64>(sizeOf<Uint64>());
  int handle;
  try {
    _checkEq(bridge.planCreateDefault(handleOut), ok, 'create editor baseline');
    final baseline = handleOut.value;
    final stateOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      _checkEq(
        bridge.planRedo(baseline, stateOut),
        ok,
        'baseline redo is a no-op',
      );
      bridge.takeString(stateOut);
      _checkEq(
        bridge.planState(baseline, stateOut),
        ok,
        'baseline session state',
      );
      final state = jsonDecode(bridge.takeString(stateOut));
      _checkEq(state['entities'], 1, 'baseline contains Ground');
      _checkEq(state['undo_depth'], 0, 'baseline has no undo history');
      _checkEq(state['dirty'], false, 'baseline is clean');
    } finally {
      release(stateOut);
      _checkEq(bridge.planClose(baseline), ok, 'close baseline');
    }
    _checkEq(bridge.planCreate(handleOut), ok, 'plan create');
    handle = handleOut.value;
  } finally {
    release(handleOut);
  }
  _check(handle != 0, 'the handle is not zero');
  _checkEq(bridge.openPlans(), 1, 'one plan open');

  final levelId = _mint(9000);
  _checkEq(
    _execute(bridge, handle, _levelBatch(levelId)),
    ok,
    'create a level',
  );

  final snap = bridge.snap(handle, {
    'levelId': levelId,
    'pointMm': [0.0, 0.0],
    'radiusMm': 120.0,
    'visibleRectMm': [-1000.0, -1000.0, 1000.0, 1000.0],
    'gridSpacingMm': 100.0,
  });
  final snapped = (snap['candidates'] as List).first as Map;
  _checkEq(snapped['class'], 'grid', 'bundled snap export returns grid');
  _checkEq(
    jsonEncode(snapped['pointMm']),
    '[0.0,0.0]',
    'bundled snap export returns the nearest lattice point',
  );

  const corners = [
    [0.0, 0.0],
    [5000.0, 0.0],
    [5000.0, 4000.0],
    [0.0, 4000.0],
  ];
  for (var i = 0; i < 4; i++) {
    final code = _execute(
      bridge,
      handle,
      _wallBatch(levelId, _mint(9100 + i), corners[i], corners[(i + 1) % 4]),
    );
    _checkEq(code, ok, 'create wall ${i + 1}');
  }
  _checkEq(_wallCount(bridge, handle), 4, 'the document holds four walls');

  // DERIVED GEOMETRY must cross the boundary, or the app is drawing sticks.
  // The room built above is 5000x4000 to the centre lines and 4800x3800 to the
  // inner wall faces; the core reports the latter, which is what a person
  // means by floor area and is 1.76 m^2 away from the naive answer.
  final derivedOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  final levelPtr = toNative('$levelId\u0000');
  try {
    final code = bridge.planDerived(handle, levelPtr, derivedOut);
    _checkEq(code, ok, 'derived geometry');
    if (code == ok) {
      final d =
          jsonDecode(bridge.takeString(derivedOut)) as Map<String, Object?>;
      _checkEq(
        (d['occupied'] as List<Object?>).length,
        1,
        'one occupied component',
      );
      _checkEq((d['rooms'] as List<Object?>).length, 1, 'one room');
      final area = (d['floorAreaMm2'] as num).toDouble();
      _check(
        (area - 4800.0 * 3800.0).abs() < 1.0,
        'floor area is measured to the inner faces (got $area)',
      );
    }
  } finally {
    release(levelPtr);
    release(derivedOut);
  }

  // HIT TESTING has to cross the boundary as well. The editor asks the core
  // what a click landed on; ranking the candidates in Dart instead would put a
  // second answer to that question in the view, drifting from the CLI the
  // first time a precedence rule changed.
  final hitOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    List<Object?> hitTest(double x, double y, double radiusMm) {
      final text = jsonEncode({
        'levelId': levelId,
        'pointMm': [x, y],
        'radiusMm': radiusMm,
      });
      final ptr = toNative(text);
      try {
        final code = bridge.planHitTest(
          handle,
          ptr,
          utf8.encode(text).length,
          hitOut,
        );
        if (code != ok) {
          throw StateError('hit_test returned $code');
        }
        return jsonDecode(bridge.takeString(hitOut)) as List<Object?>;
      } finally {
        release(ptr);
      }
    }

    List<Object?> classesAt(double x, double y, double radiusMm) => hitTest(
      x,
      y,
      radiusMm,
    ).map((h) => (h as Map<String, Object?>)['class']).toList();

    // The corner two walls share. These walls were created as raw operations,
    // so no join entity exists and both endpoints rank as FREE endpoints —
    // above the two regions they sit inside.
    _checkEq(
      classesAt(0.0, 0.0, 120.0).join(','),
      'free-endpoint,free-endpoint,region,region',
      'endpoints outrank the regions they sit in',
    );

    // Mid-wall, 2500 mm from either endpoint: the region alone.
    _checkEq(
      classesAt(2500.0, 0.0, 120.0).join(','),
      'region',
      'a mid-wall click lands on the region',
    );

    // Clear of every wall: an empty result, which is an answer and not an
    // error. So is a point far outside the plan domain.
    _checkEq(
      classesAt(2500.0, 2000.0, 120.0).length,
      0,
      'the middle of the room is empty',
    );
    _checkEq(
      classesAt(1e9, 0.0, 120.0).length,
      0,
      'a point outside the plan domain is empty, not an error',
    );

    // The index is built once per document version and reused. A cache that
    // rebuilt on every query would return byte-identical answers, so only the
    // debug counter can tell the two apart.
    final builds = bridge.debugHitIndexBuilds(handle);
    _check(builds >= 1, 'the hit index was built ($builds)');
    hitTest(0.0, 0.0, 120.0);
    _checkEq(
      bridge.debugHitIndexBuilds(handle),
      builds,
      'the hit index is reused across queries',
    );
  } finally {
    release(hitOut);
  }

  // The core must REFUSE this, atomically, leaving the document alone.
  final before = _wallCount(bridge, handle);
  final refused = _execute(
    bridge,
    handle,
    _wallBatch(
      levelId,
      _mint(9200),
      [1000.0, 1000.0],
      [1000.0, 1000.0],
      thicknessMm: -50.0,
    ),
  );
  _checkEq(
    refused,
    rejected,
    'a zero-length wall of negative thickness is refused',
  );
  _checkEq(
    _wallCount(bridge, handle),
    before,
    'the refused batch changed nothing',
  );

  final errorOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    _checkEq(
      bridge.lastError(handle, errorOut),
      ok,
      'the refusal has a reason',
    );
    final reason = bridge.takeString(errorOut);
    _check(reason.isNotEmpty, 'the reason is not empty: "$reason"');
  } finally {
    release(errorOut);
  }

  // Undo, through the bundle, on a document the bundle built.
  final undoOut = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
  try {
    _checkEq(bridge.planUndo(handle, undoOut), ok, 'undo');
    final batchId = bridge.takeString(undoOut);
    _check(batchId.isNotEmpty, 'undo names the batch it compensated');
  } finally {
    release(undoOut);
  }
  _checkEq(_wallCount(bridge, handle), 3, 'undo removed exactly one wall');

  // A Rust panic must not take this process down. The release profile sets
  // `panic = "unwind"` for exactly this; if that were ever reversed, the
  // process would die on the next line instead of reporting a status.
  _checkEq(bridge.debugPanic(), panicContained, 'a Rust panic is contained');
  _checkEq(
    _wallCount(bridge, handle),
    3,
    'the plan still works after the panic',
  );

  // SAVE AND REOPEN, from inside the bundle. An editor you cannot save from
  // is not an editor, and a round trip that only checks the snapshot would
  // miss the operation log entirely — so this compares the DERIVED geometry,
  // which is a function of the whole history.
  final dir = Directory.systemTemp.createTempSync('domowik-bundle-selftest');
  try {
    final file = '${dir.path}/round-trip.domo';
    final pathPtr = toNative('$file\u0000');
    try {
      _checkEq(bridge.planSave(handle, pathPtr), ok, 'save');
      _check(File(file).existsSync(), 'the file is on disk');
    } finally {
      release(pathPtr);
    }

    final before = _derivedJson(bridge, handle, levelId);
    final reopenPtr = toNative('$file\u0000');
    final handleOut = alloc<Uint64>(sizeOf<Uint64>());
    try {
      _checkEq(bridge.planOpen(reopenPtr, handleOut), ok, 'reopen');
      final after = _derivedJson(bridge, handleOut.value, levelId);
      _checkEq(after, before, 'the reopened plan derives the same geometry');
      _checkEq(
        bridge.planClose(handleOut.value),
        ok,
        'close the reopened plan',
      );
    } finally {
      release(reopenPtr);
      release(handleOut);
    }
  } finally {
    dir.deleteSync(recursive: true);
  }

  _checkEq(bridge.planClose(handle), ok, 'plan close');
  _checkEq(bridge.openPlans(), 0, 'no plan left open');
  _checkEq(bridge.outstandingBuffers(), 0, 'no buffer left outstanding');

  stdout.writeln('$_checks checks, $_failures failures');
  if (_failures == 0) {
    stdout.writeln('BUNDLE SELFTEST PASSED');
    return 0;
  }
  stdout.writeln('BUNDLE SELFTEST FAILED');
  return 1;
}
