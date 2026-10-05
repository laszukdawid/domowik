// Spike application for issue 0024.
//
// WHAT THIS PROVES that the standalone harnesses could not. The plain-Dart and
// C harnesses load the core with an absolute path handed to them on the
// command line. An APPLICATION has no such path: it has a bundle, and the
// question is whether the native code is inside it, whether the loader finds
// it, whether code signing survives it, and whether a Rust panic on a UI
// thread takes the window down. Nothing here is simulated — every number and
// every wall on screen came out of the Rust core through the C ABI.
//
// It remains experimental. The workspace presents the core's geometry and
// routes user gestures through the shared editing API.

import 'dart:convert';
import 'dart:ffi' hide Size;
import 'dart:io';
import 'dart:math' as math;
import 'dart:ui';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/services.dart';

import 'android_probes.dart' as android_probes;
import 'probe_request.dart';
import 'selftest.dart';
import 'editor_shell.dart';
import 'plan_viewport.dart';
import 'selection_state.dart';
import 'inspector_metric.dart';
import 'level_state.dart';
import 'level_properties_form.dart';
import 'warnings_banner.dart';
import 'plan_painter.dart';
import 'wall_chain_state.dart';

// Preserve the spike's existing public imports while keeping scene payloads
// and painting separate from the editor's input/file lifecycle.
export 'plan_painter.dart';

/// Set to `1` in the environment to drive the real UI for one interaction,
/// report what the painter actually received, and exit.
///
/// The headless self-test proves the bundle can CALL the core. It does not
/// prove the widget tree renders what the core returned, and "the author
/// looked at a window" is not evidence that survives review. This probe runs
/// the same widgets, the same button handler and the same painter as a person
/// clicking, and reports from inside `paint`.
const String firstFrameVariable = 'DOMOWIK_SPIKE_FIRSTFRAME';

bool get _firstFrameProbeRequested =>
    Platform.environment[firstFrameVariable] == '1' ||
    probeRequestedByArgument(firstFrameProbe);

/// The ABI revision this build was written against.
///
/// A mismatch is fatal on purpose. The header says a caller reading a
/// different number must not proceed, and an app that proceeded anyway would
/// be interpreting one struct layout as another.
const int expectedAbi = 13;

void main(List<String> arguments) {
  // Android cannot be handed an environment, so a probe arrives here as an
  // entry-point argument instead. See `probe_request.dart`.
  applyProbeArguments(arguments);
  if (arguments.isNotEmpty) {
    reportPlatformProvenance();
  }

  // The package can verify itself without a window. See `selftest.dart` for
  // why the evidence has to come from inside the built application rather
  // than from the author having looked at one.
  if (selftestRequested || probeRequestedByArgument(selftestProbe)) {
    exit(runSelftest());
  }
  if (probeRequestedByArgument(lifecycleProbe)) {
    return android_probes.runLifecycleProbe();
  }
  if (probeRequestedByArgument(benchmarkProbe)) {
    return android_probes.runBenchmarkProbe();
  }
  runApp(const SpikeApp());
}

class SpikeApp extends StatelessWidget {
  const SpikeApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Domowik',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(
          seedColor: const Color(0xFF28796C),
          surface: const Color(0xFFFAFAF7),
        ),
        scaffoldBackgroundColor: const Color(0xFFFAFAF7),
        useMaterial3: true,
      ),
      home: const PlanPage(),
    );
  }
}

/// What a drag is doing, decided when it starts.
enum _DragMode { none, draw, endpoint, body, pan, place, selectOnly, marquee }

/// Native hit candidates retain their entity identity and authoritative order.
class _PlanHit {
  const _PlanHit(this.kind, this.entityId, this.closest);

  final String kind;
  final String entityId;
  final Offset closest;
}

class _SnapPreview {
  const _SnapPreview(this.point, this.guide);
  final Offset point;
  final (Offset, Offset)? guide;
}

class PlanPage extends StatefulWidget {
  const PlanPage({super.key, this.bridgeLoader});

  /// Tests load the real native library directly; bundles use [openBridge].
  final Bridge Function()? bridgeLoader;

  @override
  State<PlanPage> createState() => _PlanPageState();
}

class _PlanPageState extends State<PlanPage> with WidgetsBindingObserver {
  Bridge? _bridge;
  String? _loadError;
  int _abi = 0;
  int _handle = 0;
  final LevelViewState _levelView = LevelViewState();
  String? get _levelId => _levelView.activeId;
  bool _resetLevelView = true;
  String? _filePath;
  bool _dirty = false;
  int _undoDepth = 0;
  int _redoDepth = 0;
  bool _fileActionPending = false;

  List<LevelInfo> get _levels => _levelView.levels;
  Map<String, List<Wall>> _wallsByLevel = const {};
  Map<String, List<(Wall, bool)>> _allJoins = const {};
  final Map<String, Derived> _derivedByLevel = {};
  Map<String, String> _currentGeometryKeys = const {};
  final Map<String, String> _cachedGeometryKeys = {};
  int? _sceneHandle;
  List<PlanLayer> _backgroundLevels = const [];

  List<Wall> _walls = const [];
  Map<String, List<(Wall, bool)>> _joins = const {};
  Derived _derived = Derived.empty;
  final SelectionController _selection = SelectionController();
  String? get _selected => _selection.primary;
  int _documentRevision = 0;
  int _modifierMask = 0;
  final GlobalKey _paintKey = GlobalKey();
  Offset? _dragFrom;
  Offset? _dragTo;
  Offset? _pointerPosition;
  Offset? _pointerDownPosition;
  bool _dragClassified = false;
  PointerDeviceKind _pointerKind = PointerDeviceKind.mouse;
  SelectionSnapshot? _selectionBeforeDrag;
  final Map<int, Offset> _touches = {};
  final Set<int> _releasedGestureTouches = {};
  PlanViewport? _navigationStart;
  int? _trackpadPointer;
  double _trackpadScale = 1;
  Offset? _navigationCentroid;

  /// What the current drag is doing. Decided on pointer-down from what is
  /// under the cursor, so one gesture serves drawing, dragging an endpoint and
  /// moving a wall without a toolbar of modes to get wrong.
  _DragMode _mode = _DragMode.none;
  String? _dragWall;
  bool _dragIsStart = false;

  PlanViewport _view = PlanViewport(zoom: 0.1, origin: Offset.zero);
  Size _canvasSize = Size.zero;
  Rect? _contentBounds;
  bool _fitRequested = true;
  bool _showGrid = true;
  bool _snapGrid = true;
  bool _snapObject = true;
  bool _snapAngle = true;
  bool _snapAltDisabled = false;
  _SnapPreview? _snapPreview;
  bool _snapFailed = false;
  bool _snapStartQueried = false;
  Offset? _snapRawStart;
  bool _snapReevalScheduled = false;
  final FocusNode _canvasFocus = FocusNode();
  int? _activePointer;
  PlanViewport? _panStart;

  /// What a click on a wall places. `null` means selection.
  String? _placing;

  /// Whether the creation-only Walls chain tool (`wall-chain-tool`) is armed,
  /// as opposed to the default hybrid Select / draw mode.
  bool _wallChainTool = false;
  final WallChainState _chain = WallChainState();
  Offset? _chainHoverAt;
  Offset? _chainHoverScreen;
  bool _chainHoverReevalScheduled = false;
  Duration? _lastChainClickTime;
  Offset? _lastChainClickScreen;
  String _status = '';
  bool _statusIsError = false;

  /// R2.1 validation warnings on the CURRENT document (duplicate elevation,
  /// slab intrusion, ceiling-below-wall-height): non-blocking, so surfaced
  /// separately from `_status`/`_statusIsError`, which report the last
  /// accept/refuse action. Read from `planState`'s `warnings` array on every
  /// refresh; each entry's `message` is shown verbatim regardless of `code`,
  /// so a warning code this build does not recognise is still readable.
  List<String> _warnings = const [];
  int _openPlans = 0;
  int _outstandingBuffers = 0;
  int _panicsSurvived = 0;

  int _idSeq = 0;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _modifierMask = _readModifierMask();
    HardwareKeyboard.instance.addHandler(_observeSelectionKeys);
    GestureBinding.instance.pointerRouter.addGlobalRoute(
      _observeSelectionHover,
    );
    _files.setMethodCallHandler((call) async {
      if (call.method == 'requestClose') {
        return _fileAction(_confirmLeaving);
      }
      throw MissingPluginException('Unknown editor method: ${call.method}');
    });
    _open();
    if (_firstFrameProbeRequested) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _runFirstFrameProbe(),
      );
    }
  }

  /// Adds four walls through the ordinary button handler, then reports what
  /// the painter was handed on the frame after.
  void _runFirstFrameProbe() {
    if (_loadError != null) {
      stdout.writeln('FIRST FRAME PROBE FAILED: $_loadError');
      exit(1);
    }
    for (var i = 0; i < 4; i++) {
      _addWall();
    }
    WidgetsBinding.instance.addPostFrameCallback((_) async {
      // WAIT for the painter to report, bounded, instead of sampling once.
      //
      // One post-frame callback is not a guarantee that the frame it follows
      // painted the walls this probe just added: the callback fires after
      // whichever frame happened to be in flight, and on Android that is
      // reliably the one BEFORE the setState from `_addWall` has been laid
      // out. Sampling once made the probe a race that macOS and Linux happened
      // to win and Android reliably lost.
      //
      // This cannot pass vacuously. It still requires `painted == 4` below;
      // all the loop does is stop reading the counter too early, and it
      // reports how many frames it actually waited so a regression that slows
      // painting is visible rather than silently absorbed.
      const maxFrames = 120;
      var framesWaited = 0;
      while (PlanPainter.wallsPainted != _walls.length &&
          framesWaited < maxFrames) {
        framesWaited++;
        WidgetsBinding.instance.scheduleFrame();
        await WidgetsBinding.instance.endOfFrame;
      }
      final painted = PlanPainter.wallsPainted;
      stdout.writeln('frames waited for the painter: $framesWaited');
      final lit = await _rasterisedPixels();
      final image = _bridge?.loadedImagePath();
      final bundle = bundleRoot();
      final buffers = _bridge?.outstandingBuffers() ?? -1;

      stdout.writeln('walls in document: ${_walls.length}');
      stdout.writeln('walls drawn by the painter: $painted');
      stdout.writeln('non-background pixels rasterised: $lit');
      stdout.writeln('outstanding buffers: $buffers');
      stdout.writeln('image: $image');

      final good =
          _walls.length == 4 &&
          painted == 4 &&
          lit > 0 &&
          buffers == 0 &&
          image != null &&
          bundle != null &&
          image.startsWith(bundle);
      stdout.writeln(
        good
            ? 'FIRST FRAME PROBE PASSED'
            : 'FIRST FRAME PROBE FAILED: the UI did not render the core\'s walls '
                  'from inside the bundle without leaking',
      );
      exit(good ? 0 : 1);
    });
    // A post-frame callback does not itself schedule the next frame.
    WidgetsBinding.instance.scheduleFrame();
  }

  @override
  void dispose() {
    WidgetsBinding.instance.removeObserver(this);
    HardwareKeyboard.instance.removeHandler(_observeSelectionKeys);
    GestureBinding.instance.pointerRouter.removeGlobalRoute(
      _observeSelectionHover,
    );
    _files.setMethodCallHandler(null);
    _canvasFocus.dispose();
    final bridge = _bridge;
    if (bridge != null && _handle != 0) {
      // Closing returns only after work already inside the core has finished,
      // so this is safe to do while the window is going away.
      bridge.planClose(_handle);
    }
    super.dispose();
  }

  SelectionContext get _selectionContext => SelectionContext(
    viewKey: (_view.zoom, _view.origin),
    levelId: _levelId,
    documentRevision: _documentRevision,
    visibilityRevision: _levelView.revision,
    modifierMask: _modifierMask,
    pointerKind: _pointerKind,
  );

  static int _readModifierMask() {
    final keyboard = HardwareKeyboard.instance;
    return (keyboard.isControlPressed ? controlModifier : 0) |
        (keyboard.isMetaPressed ? metaModifier : 0) |
        (keyboard.isShiftPressed ? shiftModifier : 0) |
        (keyboard.isAltPressed ? altModifier : 0);
  }

  // Observe key edges even while another control has focus. Comparing only
  // the modifier mask at the next click loses press/release round trips.
  bool _observeSelectionKeys(KeyEvent event) {
    final next = _readModifierMask();
    if (next != _modifierMask) {
      _modifierMask = next;
      _selection.observeContext(_selectionContext);
    }
    return false;
  }

  // A global hover route also sees excursions outside the canvas, where its
  // own Listener cannot receive hover. It changes cycle state, not selection.
  void _observeSelectionHover(PointerEvent event) {
    if (event is! PointerHoverEvent ||
        !_finite(event.position) ||
        !_finite(event.delta) ||
        _activePointer != null ||
        _navigationStart != null)
      return;
    final box = _paintKey.currentContext?.findRenderObject();
    if (box is! RenderBox || !box.hasSize) return;
    final at = box.globalToLocal(event.position);
    if (!_finite(at) || !_finite(_view.toPlan(at))) return;
    _pointerKind = event.kind;
    _selection.observeContext(_selectionContext);
    _selection.observePointer(at);
    if (_wallChainTool && _chain.anchor != null) {
      // Preview follows hover between segments (mouse/stylus). Native
      // snapping owns the winning point and guide, same as a live drag —
      // this is the input event, not build/paint, so the FFI call is fine.
      _chainHoverScreen = at;
      _evaluateChainHover(at);
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state != AppLifecycleState.resumed) _cancelDrag();
  }

  void _open() {
    try {
      final bridge = (widget.bridgeLoader ?? openBridge)();
      final abi = bridge.abiVersion();
      if (abi != expectedAbi) {
        setState(() {
          _loadError =
              'The bundled core reports ABI $abi; this build was '
              'written against $expectedAbi. Refusing to proceed.';
        });
        return;
      }
      setState(() {
        _bridge = bridge;
        _abi = abi;
      });
      _newPlan();
    } on Object catch (e) {
      setState(() => _loadError = '$e');
    }
  }

  /// A version-7-shaped identifier.
  ///
  /// Hand-rolled rather than taken from a package, for the reason recorded in
  /// the pubspec: this spike's dependency ledger is empty and stays that way.
  /// Uniqueness within one session is all that is needed here.
  String _mintId() {
    _idSeq++;
    final ms = DateTime.now().millisecondsSinceEpoch & 0xFFFFFFFFFFFF;
    final rand = math.Random(_idSeq * 2654435761);
    String hex(int v, int digits) => v.toRadixString(16).padLeft(digits, '0');
    final timeHigh = hex((ms >> 16) & 0xFFFFFFFF, 8);
    final timeLow = hex(ms & 0xFFFF, 4);
    final verAndSeq = hex(0x7000 | (_idSeq & 0x0FFF), 4);
    final variant = hex(0x8000 | rand.nextInt(0x4000), 4);
    final tail =
        hex(rand.nextInt(0xFFFFFFFF), 8) + hex(rand.nextInt(0xFFFF), 4);
    return '$timeHigh-$timeLow-$verAndSeq-$variant-$tail';
  }

  /// Rasterises the painter and counts pixels that are not the background.
  ///
  /// `wallsPainted` proves `drawLine` was CALLED. It does not prove anything
  /// reached a raster: a zero-width stroke, a transparent colour or a
  /// degenerate transform all call `drawLine` and produce nothing. This runs
  /// the same painter with the same data through a real raster and counts
  /// what came out.
  Future<int> _rasterisedPixels() async {
    const width = 320;
    const height = 240;
    final recorder = PictureRecorder();
    final canvas = Canvas(recorder);
    PlanPainter(
      _walls,
      _derived,
      _selected,
      null,
      view: PlanViewport.fit(
        const Size(320, 240),
        PlanPainter.boundsOf(_walls, _derived),
      ),
      showGrid: false,
    ).paint(canvas, Size(width.toDouble(), height.toDouble()));
    final picture = recorder.endRecording();
    final image = await picture.toImage(width, height);
    final data = await image.toByteData(format: ImageByteFormat.rawRgba);
    picture.dispose();
    image.dispose();
    if (data == null) {
      return 0;
    }
    var lit = 0;
    final bytes = data.buffer.asUint8List();
    for (var i = 3; i < bytes.length; i += 4) {
      if (bytes[i] != 0) {
        lit++;
      }
    }
    return lit;
  }

  /// Sends one batch and returns the status code, refreshing the view.
  int _execute(Map<String, Object?> request, String what) {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) {
      return invalidArgument;
    }
    final text = jsonEncode(request);
    final ptr = toNative(text);
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.planExecute(
        _handle,
        ptr,
        utf8.encode(text).length,
        out,
      );
      if (code == ok) {
        // The response buffer is OWNED by us and must go back through the ABI.
        bridge.takeString(out);
        _setStatus('$what accepted', isError: false);
      } else {
        _setStatus(
          '$what refused: ${_describe(code)} — ${_lastError(bridge)}',
          isError: true,
        );
      }
      return code;
    } finally {
      release(ptr);
      release(out);
      _refresh();
    }
  }

  String _lastError(Bridge bridge) {
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.lastError(_handle, out);
      if (code != ok) {
        return 'no detail (last_error returned $code)';
      }
      final text = bridge.takeString(out);
      return text.isEmpty ? 'no detail' : text;
    } finally {
      release(out);
    }
  }

  static String _describe(int code) {
    switch (code) {
      case ok:
        return 'ok';
      case invalidArgument:
        return 'invalid argument';
      case unknownHandle:
        return 'unknown handle';
      case unknownBuffer:
        return 'unknown buffer';
      case malformedRequest:
        return 'malformed request';
      case rejected:
        return 'rejected by the core';
      case panicContained:
        return 'panic contained';
      case canceled:
        return 'canceled';
      default:
        return 'status $code';
    }
  }

  void _setStatus(String message, {required bool isError}) {
    _status = message;
    _statusIsError = isError;
  }

  /// Reads `planState`'s additive `warnings` array. Each entry's shape is
  /// `{code, level, other?/below?, message}`; only `message` is relied on for
  /// display, so a warning `code` this build predates is still shown
  /// intelligibly rather than dropped.
  static List<String> _parseWarnings(Map<String, Object?> state) {
    final raw = state['warnings'] as List<Object?>? ?? const [];
    return raw
        .map((w) {
          final map = w as Map<String, Object?>?;
          return map?['message'] as String?;
        })
        .whereType<String>()
        .toList();
  }

  /// Re-reads the document from the core and repaints.
  void _refresh() {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) {
      return;
    }
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final stateCode = bridge.planState(_handle, out);
      if (stateCode != ok) {
        setState(
          () => _setStatus(
            'session state failed: ${_describe(stateCode)}',
            isError: true,
          ),
        );
        return;
      }
      final state = jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
      final revision = state['document_revision'] as int;
      if (!_resetLevelView &&
          _sceneHandle == _handle &&
          revision == _documentRevision) {
        // A read/save/refusal changes no scene input or selection-cycle state.
        setState(() {
          _dirty = state['dirty'] as bool;
          _undoDepth = state['undo_depth'] as int;
          _redoDepth = state['redo_depth'] as int;
          _warnings = _parseWarnings(state);
          if (_levelView.paintOrder.any(
            (level) => !_derivedByLevel.containsKey(level.id),
          )) {
            _rebuildVisibleScene(); // Retry a previously failed native read.
          }
          _openPlans = bridge.openPlans();
          _outstandingBuffers = bridge.outstandingBuffers();
        });
        return;
      }
      final code = bridge.planSnapshot(_handle, out);
      if (code != ok) {
        setState(
          () =>
              _setStatus('snapshot failed: ${_describe(code)}', isError: true),
        );
        return;
      }
      final document =
          jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
      final geometryInputs = <String, List<Object?>>{};
      final levels = (document['levels'] as List<Object?>? ?? const [])
          .map((e) => e as Map<String, Object?>)
          .map((l) {
            geometryInputs[l['id'] as String] = [
              l,
              <Object?>[],
              <Object?>[],
              <Object?>[],
              // Room records. They are an input to the native derivation —
              // the label, its anchor and every status come out of it — so a
              // rename that did not appear in this key would leave the cached
              // payload in place and the new name invisible until something
              // else moved.
              <Object?>[],
            ];
            return LevelInfo(
              id: l['id'] as String,
              name: (l['name'] as String?) ?? 'Level',
              elevationMm: (l['elevationMm'] as num).toDouble(),
              floorThicknessMm: (l['floorThicknessMm'] as num).toDouble(),
              defaultWallHeightMm: (l['defaultWallHeightMm'] as num).toDouble(),
              defaultCeilingHeightMm: (l['defaultCeilingHeightMm'] as num)
                  .toDouble(),
            );
          })
          .toList();
      final wallsByLevel = <String, List<Wall>>{};
      final wallsById = <String, Wall>{};
      final wallLevels = <String, String>{};
      for (final entry in (document['walls'] as List<Object?>? ?? const [])) {
        final w = entry as Map<String, Object?>;
        final s = (w['startMm'] as List<Object?>).cast<num>();
        final e = (w['endMm'] as List<Object?>).cast<num>();
        final wall = Wall(
          w['id'] as String,
          Offset(s[0].toDouble(), s[1].toDouble()),
          Offset(e[0].toDouble(), e[1].toDouble()),
          (w['thicknessMm'] as num).toDouble(),
        );
        wallsById[wall.id] = wall;
        wallLevels[wall.id] = w['levelId'] as String;
        (geometryInputs[w['levelId']]?[1] as List<Object?>?)?.add(w);
        wallsByLevel.putIfAbsent(w['levelId'] as String, () => []).add(wall);
      }
      final joins = <String, List<(Wall, bool)>>{};
      for (final entry in (document['joins'] as List<Object?>? ?? const [])) {
        final join = entry as Map<String, Object?>;
        final members = <(Wall, bool)>[];
        final joinedLevels = <String>{};
        for (final entry in join['members'] as List<Object?>) {
          final member = entry as Map<String, Object?>;
          final wall = wallsById[member['wallId']];
          if (wall != null) {
            members.add((wall, member['role'] == 'Start'));
            joinedLevels.add(wallLevels[wall.id]!);
          }
        }
        for (final level in joinedLevels) {
          (geometryInputs[level]?[2] as List<Object?>?)?.add(join);
        }
        // Membership is a set. Choose a stable transport endpoint, not a new
        // selection identity, when the core moves this joined corner.
        members.sort((a, b) {
          final wallOrder = a.$1.id.compareTo(b.$1.id);
          return wallOrder != 0 ? wallOrder : (a.$2 ? 0 : 1) - (b.$2 ? 0 : 1);
        });
        if (members.isNotEmpty) joins[join['id'] as String] = members;
      }
      for (final entry
          in (document['openings'] as List<Object?>? ?? const [])) {
        final opening = entry as Map<String, Object?>;
        final level = wallLevels[opening['hostWallId']];
        (geometryInputs[level]?[3] as List<Object?>?)?.add(opening);
      }
      for (final entry
          in (document['roomRecords'] as List<Object?>? ?? const [])) {
        final record = entry as Map<String, Object?>;
        (geometryInputs[record['levelId']]?[4] as List<Object?>?)?.add(record);
      }
      // Exact equality of the native derivation's authoritative inputs, not
      // a collision-prone hash or a Dart geometry implementation. Whole
      // entities cover field edits; unrelated levels keep their cached result.
      final geometryKeys = {
        for (final entry in geometryInputs.entries)
          entry.key: jsonEncode(entry.value),
      };
      setState(() {
        _dirty = state['dirty'] as bool;
        _undoDepth = state['undo_depth'] as int;
        _redoDepth = state['redo_depth'] as int;
        final previousLevel = _levelId;
        if (_resetLevelView) {
          _levelView.reset(levels);
          _resetLevelView = false;
        } else {
          _levelView.reconcile(levels);
        }
        if (_levelId != previousLevel) _selection.clear();
        _wallsByLevel = wallsByLevel;
        _allJoins = joins;
        _currentGeometryKeys = geometryKeys;
        _documentRevision = revision;
        _warnings = _parseWarnings(state);
        _selection.observeContext(_selectionContext);
        _rebuildVisibleScene();
        _selection.retainSurvivors({
          for (final wall in _walls) wall.id,
          ..._joins.keys,
        });
        _openPlans = bridge.openPlans();
        _outstandingBuffers = bridge.outstandingBuffers();
      });
    } finally {
      release(out);
    }
  }

  /// Asks the CORE for the level's occupied region and rooms.
  ///
  /// The snapshot above carries centre lines only. Everything a person
  /// recognises as a floor plan — the wall mass with its mitred corners, the
  /// rooms, the area — is derived, and it is derived once, in Rust.
  Derived? _readDerived(Bridge bridge, String levelId) {
    if (_handle == 0) {
      return null;
    }
    final id = toNative('$levelId\u0000');
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.planDerived(_handle, id, out);
      if (code != ok) {
        _setStatus('level geometry failed: ${_describe(code)}', isError: true);
        return null;
      }
      final j = jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
      List<Ring> rings(String key) => (j[key] as List<Object?>)
          .map((e) => Ring.fromJson(e as Map<String, Object?>))
          .toList();
      return Derived(
        rings('occupied'),
        rings('rooms'),
        (j['openings'] as List<Object?>)
            .map((e) => Aperture.fromJson(e as Map<String, Object?>))
            .toList(),
        (j['floorAreaMm2'] as num).toDouble(),
        roomRecords: ((j['roomRecords'] as List<Object?>?) ?? const [])
            .map((e) => RoomRecordView.fromJson(e as Map<String, Object?>))
            .toList(),
      );
    } finally {
      release(id);
      release(out);
    }
  }

  /// Reuses Rust's derived payloads while that level's inputs are unchanged.
  /// View changes only compose layers; hidden/deleted payloads are evicted so
  /// visiting hidden levels cannot accumulate an unbounded historical cache.
  /// Called inside setState. It never changes document/history or the camera.
  void _rebuildVisibleScene() {
    final bridge = _bridge;
    if (bridge == null) return;
    if (_sceneHandle != _handle) {
      _derivedByLevel.clear();
      _cachedGeometryKeys.clear();
      _sceneHandle = _handle;
    }
    final visible = _levelView.paintOrder;
    final flags = _levelView.visibility;
    final visibleIds = visible.map((level) => level.id).toSet();
    _derivedByLevel.removeWhere((id, _) => !visibleIds.contains(id));
    _cachedGeometryKeys.removeWhere((id, _) => !visibleIds.contains(id));
    Derived derivedFor(String id) {
      final key = _currentGeometryKeys[id];
      final cached = _derivedByLevel[id];
      if (cached != null && key == _cachedGeometryKeys[id]) return cached;
      final derived = _readDerived(bridge, id);
      if (derived != null) {
        _derivedByLevel[id] = derived;
        _cachedGeometryKeys[id] = key!;
        return derived;
      }
      // Do not cache failures as empty geometry or show stale old geometry.
      _derivedByLevel.remove(id);
      _cachedGeometryKeys.remove(id);
      return Derived.empty;
    }

    _walls = _wallsByLevel[_levelId] ?? const [];
    final activeWallIds = _walls.map((wall) => wall.id).toSet();
    _joins = {
      for (final entry in _allJoins.entries)
        if (entry.value.every((member) => activeWallIds.contains(member.$1.id)))
          entry.key: entry.value,
    };
    _derived = _levelId == null ? Derived.empty : derivedFor(_levelId!);
    _backgroundLevels = [
      for (final level in visible)
        if (level.id != _levelId)
          PlanLayer(
            levelId: level.id,
            walls: _wallsByLevel[level.id] ?? const [],
            derived: derivedFor(level.id),
            ghosted: flags[level.id] == LevelVisibility.ghosted,
          ),
    ];
    var bounds = PlanPainter.boundsOf(_walls, _derived);
    for (final layer in _backgroundLevels) {
      final next = PlanPainter.boundsOf(layer.walls, layer.derived);
      if (next != null) bounds = bounds?.expandToInclude(next) ?? next;
    }
    _contentBounds = bounds;
    _outstandingBuffers = bridge.outstandingBuffers();
  }

  void _activateLevel(String id) {
    if (id == _levelId || !_levels.any((level) => level.id == id)) return;
    _cancelDrag();
    setState(() {
      if (!_levelView.activate(id)) return;
      _selection.clear();
      _selection.observeContext(_selectionContext);
      _rebuildVisibleScene();
    });
  }

  void _setLevelVisibility(String id, LevelVisibility visibility) {
    setState(() {
      if (!_levelView.setVisibility(id, visibility)) return;
      // Observe at the transition, not just at the eventual next click: a
      // hide/show round trip must not resurrect the previous cycle.
      _selection.observeContext(_selectionContext);
      _rebuildVisibleScene();
    });
  }

  /// Opens the level-properties dialog and, only if the user actually
  /// changed something, submits it as one atomic `setLevel` gesture.
  /// `level` is passed straight from the live `_levels` list, so the form
  /// always seeds from the current authoritative snapshot, never a stale
  /// cached copy. Active/visibility flags are untouched either way — this
  /// edits authoritative fields only (R2.4: changing them never touches
  /// view state).
  ///
  /// Routed through `_fileAction`, the SAME serialization New/Open/Save use,
  /// rather than a second untracked async path: it cancels any active drag
  /// before the modal opens, and — because `_fileActionPending` blocks New/
  /// Open/Close for its whole duration — a document or handle swap through
  /// THOSE controls cannot land while this dialog is showing, so `level`/
  /// `_handle` are still the ones the user was editing when it resolves.
  /// That does NOT make the `mounted` check redundant: this widget's own
  /// State can still be disposed during the `await` for reasons `_fileAction`
  /// does not gate at all — the page being popped, a hot reload, the test
  /// harness tearing down the tree — so the guard stays load-bearing.
  Future<void> _editLevelProperties(LevelInfo level) async {
    await _fileAction(() async {
      final changes = await LevelPropertiesDialog.show(context, level);
      if (!mounted || changes == null || changes.isEmpty) return false;
      _gesture({
        'kind': 'setLevel',
        'levelId': level.id,
        ...changes.toJson(),
      }, 'edit level');
      return true;
    });
  }

  /// The one thing Dart cannot do alone: ask AppKit for a path.
  ///
  /// A cancelled dialog returns null, which is not an error — see the Swift
  /// side for why that distinction matters.
  static const _files = MethodChannel('invalid.domowik-spike/files');

  /// Serialises file dialogs and replacement approvals on the UI isolate.
  /// Document input is blocked until the entire action has finished.
  Future<bool> _fileAction(Future<bool> Function() action) async {
    if (_fileActionPending || !mounted) return false;
    _cancelDrag();
    setState(() {
      _fileActionPending = true;
    });
    try {
      return await action();
    } on Object catch (error) {
      if (mounted) {
        setState(() => _setStatus('File action failed: $error', isError: true));
      }
      return false;
    } finally {
      if (mounted) setState(() => _fileActionPending = false);
    }
  }

  Future<bool> _confirmLeaving() async {
    if (!_dirty) return true;
    final choice = await showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Save your changes?'),
        content: const Text(
          'Save this plan before leaving, or discard the changes.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, 'cancel'),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, 'discard'),
            child: const Text('Discard'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(context, 'save'),
            child: const Text('Save changes'),
          ),
        ],
      ),
    );
    if (!mounted) return false;
    if (choice == 'discard') return true;
    return choice == 'save' && await _saveFile();
  }

  Future<void> _requestNewPlan() async {
    await _fileAction(() async => await _confirmLeaving() && _newPlan());
  }

  Future<void> _requestOpenFile() async {
    await _fileAction(() async => await _confirmLeaving() && await _openFile());
  }

  Future<void> _requestSaveFile() async {
    await _fileAction(_saveFile);
  }

  void _deleteActiveLevel() {
    final id = _levelId;
    if (id == null) return;
    _cancelDrag();
    _gesture({'kind': 'deleteLevel', 'levelId': id}, 'delete level');
  }

  /// Adds a storey above the highest one.
  void _addLevel() {
    _cancelDrag();
    final id = _mintId();
    final elevation = _levels.isEmpty ? 0.0 : _levels.last.elevationMm + 3000.0;
    final code = _execute({
      'operations': [
        {
          'operationId': _mintId(),
          'entityId': id,
          'kind': 'createEntity',
          'entity': {
            'type': 'level',
            'id': id,
            'name': 'Level ${_levels.length + 1}',
            'elevationMm': elevation,
            'floorThicknessMm': 200.0,
            'defaultWallHeightMm': 2500.0,
            'defaultCeilingHeightMm': 2500.0,
          },
        },
      ],
    }, 'add level');
    if (code == ok) _activateLevel(id);
  }

  Future<bool> _openFile() async {
    final path = await _files.invokeMethod<String>('openPlan');
    if (path == null || !mounted) return false;
    final bridge = _bridge;
    if (bridge == null) return false;

    final pathPtr = toNative('$path\u0000');
    final handleOut = alloc<Uint64>(sizeOf<Uint64>());
    try {
      final code = bridge.planOpen(pathPtr, handleOut);
      if (code != ok) {
        setState(
          () => _setStatus('could not open: ${_describe(code)}', isError: true),
        );
        return false;
      }
      // The old plan goes only after the new one is safely open, so a failed
      // open leaves the user with what they had.
      if (_handle != 0) bridge.planClose(_handle);
      _handle = handleOut.value;
      _filePath = path;
      _selection.clear();
      _placing = null;
      _dirty = false;
      _fitRequested = true;
      _resetLevelView = true;
      _showGrid = true;
      _snapGrid = true;
      _snapObject = true;
      _snapAngle = true;
      _snapPreview = null;
      _snapFailed = false;
      _snapStartQueried = false;
      _snapRawStart = null;
    } finally {
      release(pathPtr);
      release(handleOut);
    }
    _refresh();
    setState(() => _setStatus('opened $path', isError: false));
    return true;
  }

  Future<bool> _saveFile() async {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) return false;
    final suggested = _filePath?.split('/').last ?? 'plan.domo';
    final path = await _files.invokeMethod<String>('savePlan', suggested);
    if (path == null || !mounted) return false;

    final pathPtr = toNative('$path\u0000');
    try {
      final code = bridge.planSave(_handle, pathPtr);
      setState(() {
        if (code == ok) {
          _filePath = path;
          _dirty = false;
          _setStatus('saved to $path', isError: false);
        } else {
          _setStatus(
            'could not save: ${_describe(code)} — ${_lastError(bridge)}',
            isError: true,
          );
        }
      });
      return code == ok;
    } finally {
      release(pathPtr);
      _refresh();
    }
  }

  bool _newPlan() {
    final bridge = _bridge;
    if (bridge == null) {
      return false;
    }
    final handleOut = alloc<Uint64>(sizeOf<Uint64>());
    var candidate = 0;
    try {
      final code = bridge.planCreateDefault(handleOut);
      if (code != ok) {
        setState(
          () => _setStatus(
            'could not create a plan: ${_describe(code)}',
            isError: true,
          ),
        );
        return false;
      }
      candidate = handleOut.value;
      if (_handle != 0) bridge.planClose(_handle);
      setState(() {
        _handle = candidate;
        _filePath = null;
        _selection.clear();
        _placing = null;
        _dirty = false;
        _fitRequested = true;
        _resetLevelView = true;
        _showGrid = true;
        _snapGrid = true;
        _snapObject = true;
        _snapAngle = true;
        _snapPreview = null;
        _snapFailed = false;
        _snapStartQueried = false;
        _snapRawStart = null;
        _setStatus('new plan', isError: false);
      });
      _refresh();
      return true;
    } finally {
      if (candidate != 0 && candidate != _handle) bridge.planClose(candidate);
      release(handleOut);
    }
  }

  /// Adds the next wall of a rectangle, one side per press.
  void _addWall() {
    final levelId = _levelId;
    if (levelId == null) {
      return;
    }
    const corners = [
      Offset(0, 0),
      Offset(5000, 0),
      Offset(5000, 4000),
      Offset(0, 4000),
    ];
    final i = _walls.length % corners.length;
    final a = corners[i];
    final b = corners[(i + 1) % corners.length];
    final id = _mintId();
    _execute({
      'operations': [
        {
          'operationId': _mintId(),
          'entityId': id,
          'kind': 'createEntity',
          'entity': {
            'type': 'wall',
            'id': id,
            'levelId': levelId,
            'startMm': [a.dx, a.dy],
            'endMm': [b.dx, b.dy],
            'thicknessMm': 200.0,
            'heightMm': 2500.0,
          },
        },
      ],
    }, 'add wall');
  }

  /// Sends a wall the core must REFUSE, so the refusal path is visible in the
  /// running app and not only in a test.
  void _addInvalidWall() {
    final levelId = _levelId;
    if (levelId == null) {
      return;
    }
    final id = _mintId();
    _execute({
      'operations': [
        {
          'operationId': _mintId(),
          'entityId': id,
          'kind': 'createEntity',
          'entity': {
            'type': 'wall',
            'id': id,
            'levelId': levelId,
            'startMm': [1000.0, 1000.0],
            'endMm': [1000.0, 1000.0],
            'thicknessMm': -50.0,
            'heightMm': 2500.0,
          },
        },
      ],
    }, 'zero-length wall of negative thickness');
  }

  void _changeHistory({bool redo = false}) {
    if (_fileActionPending) return;
    _cancelDrag();
    final bridge = _bridge;
    if (bridge == null || _handle == 0) {
      return;
    }
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final action = redo ? 'redo' : 'undo';
      final code = redo
          ? bridge.planRedo(_handle, out)
          : bridge.planUndo(_handle, out);
      if (code != ok) {
        setState(
          () => _setStatus('$action failed: ${_describe(code)}', isError: true),
        );
        return;
      }
      final response =
          jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
      final batch = response['batch_id'] as String;
      setState(
        () => _setStatus(
          batch.isEmpty ? 'nothing left to $action' : '$action applied',
          isError: false,
        ),
      );
    } finally {
      release(out);
      _refresh();
    }
  }

  /// Panics inside Rust, on this thread, on purpose.
  ///
  /// The release profile sets `panic = "unwind"` precisely so this returns a
  /// status code instead of aborting. If that decision were ever reversed, the
  /// window would vanish here rather than the counter going up.
  void _panic() {
    final bridge = _bridge;
    if (bridge == null) {
      return;
    }
    final code = bridge.debugPanic();
    setState(() {
      _panicsSurvived++;
      _setStatus(
        'Rust panicked and returned ${_describe(code)}; the app is still here '
        '($_panicsSurvived survived)',
        isError: code != panicContained,
      );
    });
    _refresh();
  }

  @override
  Widget build(BuildContext context) {
    final error = _loadError;
    if (error != null) {
      return Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(32),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                const Icon(Icons.link_off, size: 48),
                const SizedBox(height: 16),
                const Text(
                  'The native core did not load.',
                  style: TextStyle(fontSize: 18),
                ),
                const SizedBox(height: 8),
                SelectableText(error, textAlign: TextAlign.center),
              ],
            ),
          ),
        ),
      );
    }

    return AbsorbPointer(
      absorbing: _fileActionPending,
      child: EditorShell(
        fileName:
            '${_filePath?.split('/').last ?? 'Untitled plan'}${_dirty ? ' • Unsaved' : ''}',
        onNew: _requestNewPlan,
        onOpen: _requestOpenFile,
        onSave: _requestSaveFile,
        onUndo: _undoDepth > 0 ? _changeHistory : null,
        onRedo: _redoDepth > 0 ? () => _changeHistory(redo: true) : null,
        tools: Row(
          children: [
            _tool('Select / draw', Icons.near_me_outlined, null),
            const SizedBox(width: 8),
            _wallsTool(),
            const SizedBox(width: 8),
            _tool('Door', Icons.door_front_door_outlined, 'door'),
            const SizedBox(width: 8),
            _tool('Window', Icons.window_outlined, 'window'),
            const SizedBox(width: 12),
            IconButton(
              onPressed: () => _zoomView(1 / 1.25),
              tooltip: 'Zoom out',
              icon: const Icon(Icons.remove),
            ),
            IconButton(
              onPressed: () => _zoomView(1.25),
              tooltip: 'Zoom in',
              icon: const Icon(Icons.add),
            ),
            IconButton(
              onPressed: () => setState(() => _fitRequested = true),
              tooltip: 'Fit plan',
              icon: const Icon(Icons.fit_screen),
            ),
            IconButton(
              onPressed: () => setState(() => _showGrid = !_showGrid),
              tooltip: _showGrid
                  ? 'Hide grid and rulers'
                  : 'Show grid and rulers',
              isSelected: _showGrid,
              icon: const Icon(Icons.grid_4x4),
            ),
            IconButton(
              key: const ValueKey('snap-grid'),
              onPressed: () => _toggleSnap('grid'),
              tooltip: 'Grid snapping (new walls)',
              isSelected: _snapGrid,
              icon: const Icon(Icons.grid_on),
            ),
            IconButton(
              key: const ValueKey('snap-object'),
              onPressed: () => _toggleSnap('object'),
              tooltip: 'Object snapping (new walls)',
              isSelected: _snapObject,
              icon: const Icon(Icons.adjust),
            ),
            IconButton(
              key: const ValueKey('snap-angle'),
              onPressed: () => _toggleSnap('angle'),
              tooltip: 'Angle snapping (new walls)',
              isSelected: _snapAngle,
              icon: const Icon(Icons.change_history),
            ),
          ],
        ),
        inspector: _controls(context),
        canvas: Stack(
          children: [
            Positioned.fill(child: _canvas(context)),
            Positioned(
              top: 16,
              left: 16,
              right: 16,
              // Both hints share one bounded overlay: warnings never cover
              // the tool instructions or resize the plan viewport.
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.stretch,
                children: [
                  IgnorePointer(
                    child: Align(
                      alignment: Alignment.topLeft,
                      child: Card(
                        elevation: 0,
                        child: Padding(
                          padding: const EdgeInsets.symmetric(
                            horizontal: 12,
                            vertical: 8,
                          ),
                          child: Text(
                            _wallChainTool
                                ? (_chain.anchor == null
                                      ? 'Click or drag to start a wall chain'
                                      : 'Click or drag to continue the chain · Enter/double-click/right-click to finish · Esc to cancel the preview')
                                : _placing == null
                                ? 'Drag to draw · Click a wall to select'
                                : 'Click a wall to place a $_placing',
                            style: Theme.of(context).textTheme.bodySmall,
                          ),
                        ),
                      ),
                    ),
                  ),
                  if (_warnings.isNotEmpty)
                    Align(
                      alignment: Alignment.topRight,
                      child: WarningsBanner(warnings: _warnings),
                    ),
                ],
              ),
            ),
            if (_walls.isEmpty)
              const Center(
                child: IgnorePointer(
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Icon(
                        Icons.architecture,
                        size: 44,
                        color: Color(0xFF28796C),
                      ),
                      SizedBox(height: 12),
                      Text(
                        'Make room for your ideas',
                        style: TextStyle(
                          fontSize: 22,
                          fontWeight: FontWeight.w600,
                        ),
                      ),
                      SizedBox(height: 8),
                      Text('Drag anywhere to draw your first wall.'),
                    ],
                  ),
                ),
              ),
          ],
        ),
        status: _status.isEmpty ? 'Ready to draw' : _status,
        statusIsError: _statusIsError,
      ),
    );
  }

  Widget _tool(String label, IconData icon, String? placing) {
    return ChoiceChip(
      avatar: Icon(icon, size: 18),
      label: Text(label),
      selected: !_wallChainTool && _placing == placing,
      showCheckmark: false,
      onSelected: (_) {
        _cancelDrag();
        setState(() {
          _wallChainTool = false;
          _chain.clear();
          _placing = placing;
        });
      },
    );
  }

  /// The explicit, creation-only Walls chain tool. Distinct from the default
  /// hybrid Select / draw mode: it never selects, drags endpoints/bodies, or
  /// marquees, and re-arming it (even while already selected) starts a fresh
  /// chain session per R6.5.10 (a tool switch is a finish event).
  Widget _wallsTool() {
    return ChoiceChip(
      key: const ValueKey('wall-chain-tool'),
      avatar: const Icon(Icons.polyline_outlined, size: 18),
      label: const Text('Walls'),
      selected: _wallChainTool,
      showCheckmark: false,
      onSelected: (_) {
        _cancelDrag();
        setState(() {
          _placing = null;
          _wallChainTool = true;
          _chain.clear();
        });
      },
    );
  }

  /// The drawing surface: drag to draw a wall, click to select, Delete to
  /// remove.
  ///
  /// Coordinates are converted through the SAME transform the painter used, so
  /// a wall lands where the pointer was. Keeping two mappings in step by hand
  /// is how a drawing tool ends up half a wall off at some zoom levels.
  void _zoomView(double factor) {
    setState(
      () => _applyView(_view.zoomAt(factor, _canvasSize.center(Offset.zero))),
    );
  }

  /// Reproject transient input when navigation changes the camera. The
  /// gesture anchor stays in plan space; the cursor stays in screen space.
  /// Call within setState, or while laying out an already scheduled frame.
  void _applyView(PlanViewport view) {
    _view = view;
    _selection.observeContext(_selectionContext);
    if (_activePointer != null &&
        _mode != _DragMode.pan &&
        _pointerPosition != null) {
      _dragTo = view.toPlan(_pointerPosition!);
      if ((_mode == _DragMode.draw || _editingDrag) &&
          _dragClassified &&
          !_snapReevalScheduled) {
        _snapReevalScheduled = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          _snapReevalScheduled = false;
          if (!mounted || !_dragClassified || _pointerPosition == null) return;
          final cursor = _view.toPlan(_pointerPosition!);
          if (_mode == _DragMode.draw) {
            _evaluateCreationSnap(cursor);
          } else if (_editingDrag) {
            _evaluateEditSnap(cursor);
          }
        });
      }
    } else if (_wallChainTool &&
        _chain.anchor != null &&
        _chainHoverScreen != null &&
        !_chainHoverReevalScheduled) {
      // Camera changes must reevaluate the IDLE chain hover preview too, not
      // only an active drag: the previous native winner may no longer apply
      // once the view moved.
      _chainHoverReevalScheduled = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _chainHoverReevalScheduled = false;
        if (mounted &&
            _wallChainTool &&
            _chain.anchor != null &&
            _chainHoverScreen != null) {
          _evaluateChainHover(_chainHoverScreen!);
        }
      });
    }
  }

  void _toggleSnap(String kind) {
    setState(() {
      switch (kind) {
        case 'grid':
          _snapGrid = !_snapGrid;
          break;
        case 'object':
          _snapObject = !_snapObject;
          break;
        case 'angle':
          _snapAngle = !_snapAngle;
          break;
      }
    });
    _reevaluateCreationSnap();
  }

  void _reevaluateCreationSnap() {
    if (_mode == _DragMode.draw &&
        _dragClassified &&
        _pointerPosition != null) {
      _evaluateCreationSnap(_view.toPlan(_pointerPosition!));
    } else if (_wallChainTool &&
        _chain.anchor != null &&
        _chainHoverScreen != null) {
      _evaluateChainHover(_chainHoverScreen!);
    }
  }

  void _cancelDrag({bool clearSelection = false}) {
    final chainPending = _wallChainTool && _chain.anchor != null;
    if (_activePointer == null &&
        _navigationStart == null &&
        !clearSelection &&
        !chainPending) {
      return;
    }
    setState(() {
      if (_activePointer != null && _selectionBeforeDrag != null) {
        _selection.restoreSelection(_selectionBeforeDrag!);
      }
      if (_panStart != null) _applyView(_panStart!);
      if (_navigationStart != null) _applyView(_navigationStart!);
      _releasedGestureTouches.addAll(_touches.keys);
      _touches.clear();
      _navigationStart = null;
      _navigationCentroid = null;
      _trackpadPointer = null;
      _panStart = null;
      _activePointer = null;
      _dragFrom = null;
      _dragTo = null;
      _dragWall = null;
      _pointerPosition = null;
      _pointerDownPosition = null;
      _dragClassified = false;
      _selectionBeforeDrag = null;
      _mode = _DragMode.none;
      _snapPreview = null;
      _snapFailed = false;
      _snapStartQueried = false;
      _snapRawStart = null;
      _snapAltDisabled = false;
      // R6.5.10 cancel: discards only the uncommitted chain preview;
      // committed segments are already independent batches and unaffected.
      _chain.clear();
      _chainHoverAt = null;
      _chainHoverScreen = null;
      _chainHoverReevalScheduled = false;
      _lastChainClickTime = null;
      _lastChainClickScreen = null;
      if (clearSelection) _selection.clear();
    });
  }

  static bool _finite(Offset value) => value.dx.isFinite && value.dy.isFinite;

  void _startTrackpad(PointerPanZoomStartEvent event) {
    if (_fileActionPending ||
        !_finite(event.localPosition) ||
        _navigationStart != null)
      return;
    _cancelDrag();
    _canvasFocus.requestFocus();
    _navigationStart = _view;
    _trackpadPointer = event.pointer;
    _trackpadScale = 1;
    _navigationCentroid = event.localPosition;
  }

  void _updateTrackpad(PointerPanZoomUpdateEvent event) {
    if (event.pointer != _trackpadPointer ||
        !_finite(event.localPosition) ||
        !_finite(event.localPan) ||
        !_finite(event.localPanDelta))
      return;
    // pan is a cumulative vector, not a second local position. Transform its
    // global endpoint once: adding localPan would double-count this canvas's
    // layout translation on the pinned Flutter SDK.
    final centroid = PointerEvent.transformPosition(
      event.transform,
      event.position + event.pan,
    );
    final next = _view.panZoom(
      previousCentroid: _navigationCentroid!,
      currentCentroid: centroid,
      scale: event.scale / _trackpadScale,
    );
    if (identical(next, _view)) return; // Invalid step, including scale zero.
    setState(() {
      _applyView(next);
      _trackpadScale = event.scale;
      _navigationCentroid = centroid;
    });
  }

  /// The second touch restores the interrupted edit BEFORE starting a pinch.
  bool _startSecondTouch(PointerDownEvent event) {
    if (event.kind != PointerDeviceKind.touch || _touches.length != 1)
      return false;
    final first = _touches.entries.single;
    if (first.key == event.pointer) return true;
    _cancelDrag();
    _releasedGestureTouches.remove(first.key);
    _touches[first.key] = first.value;
    _touches[event.pointer] = event.localPosition;
    _navigationStart = _view;
    return true;
  }

  bool _moveTouches(PointerMoveEvent event) {
    if (_touches.length != 2 || !_touches.containsKey(event.pointer))
      return false;
    final oldPoints = _touches.values.toList(growable: false);
    final other = _touches.entries
        .firstWhere((entry) => entry.key != event.pointer)
        .value;
    final oldDistance = (oldPoints[0] - oldPoints[1]).distance;
    final newDistance = (event.localPosition - other).distance;
    // Coincident fingers cannot define a scale step. Discard it whole.
    final next = _view.panZoom(
      previousCentroid: (oldPoints[0] + oldPoints[1]) / 2,
      currentCentroid: (event.localPosition + other) / 2,
      scale: newDistance / oldDistance,
    );
    if (!identical(next, _view)) {
      setState(() {
        _applyView(next);
        _touches[event.pointer] = event.localPosition;
      });
    }
    return true;
  }

  Widget _canvas(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final size = constraints.biggest;
        if (_fitRequested && !size.isEmpty) {
          _applyView(PlanViewport.fit(size, _contentBounds));
          _fitRequested = false;
        } else if (size != _canvasSize) {
          // Resize around the same plan-space centre, without fitting content.
          final shift = Offset(
            (size.width - _canvasSize.width) / 2,
            (size.height - _canvasSize.height) / 2,
          );
          _applyView(_view.pan(shift));
          _panStart = _panStart?.pan(shift);
        }
        _canvasSize = size;
        return Focus(
          focusNode: _canvasFocus,
          autofocus: true,
          onFocusChange: (focused) {
            if (!focused) _cancelDrag();
          },
          onKeyEvent: (node, event) {
            if (_fileActionPending) return KeyEventResult.handled;
            if (event is! KeyDownEvent && event is! KeyRepeatEvent)
              return KeyEventResult.ignored;
            final key = event.logicalKey;
            if (key == LogicalKeyboardKey.keyZ &&
                (HardwareKeyboard.instance.isControlPressed ||
                    HardwareKeyboard.instance.isMetaPressed)) {
              _changeHistory(redo: HardwareKeyboard.instance.isShiftPressed);
            } else if (key == LogicalKeyboardKey.enter && _wallChainTool) {
              // R6.5.10 finish event: ends the chain after any already-
              // committed segments, never adding a trailing one. Fires even
              // mid-drag — the in-flight preview is cancelled first.
              _finishChain();
            } else if (key == LogicalKeyboardKey.escape) {
              // R6.5.9: a pending chain anchor (even with no active pointer)
              // is an ACTIVE gesture, not idle — Escape must cancel its
              // preview and retain whatever was selected before the chain
              // tool was armed, never fall through to idle's selection-clear.
              final chainActive = _wallChainTool && _chain.anchor != null;
              _cancelDrag(
                clearSelection:
                    _activePointer == null &&
                    _navigationStart == null &&
                    !chainActive,
              );
            } else if (key == LogicalKeyboardKey.delete ||
                key == LogicalKeyboardKey.backspace) {
              _cancelDrag();
              _deleteSelected();
            } else if (key == LogicalKeyboardKey.equal ||
                key == LogicalKeyboardKey.numpadAdd) {
              _zoomView(1.25);
            } else if (key == LogicalKeyboardKey.minus ||
                key == LogicalKeyboardKey.numpadSubtract) {
              _zoomView(1 / 1.25);
            } else {
              final step = _view.gridStepMm * _view.zoom;
              final delta = switch (key) {
                LogicalKeyboardKey.arrowLeft => Offset(-step, 0),
                LogicalKeyboardKey.arrowRight => Offset(step, 0),
                LogicalKeyboardKey.arrowUp => Offset(0, -step),
                LogicalKeyboardKey.arrowDown => Offset(0, step),
                _ => null,
              };
              if (delta == null) return KeyEventResult.ignored;
              setState(() => _applyView(_view.pan(delta)));
            }
            return KeyEventResult.handled;
          },
          child: Listener(
            key: const ValueKey('plan-input'),
            onPointerPanZoomStart: _startTrackpad,
            onPointerPanZoomUpdate: _updateTrackpad,
            onPointerPanZoomEnd: (event) {
              if (event.pointer != _trackpadPointer ||
                  !_finite(event.localPosition))
                return;
              _trackpadPointer = null;
              _navigationStart = null;
              _navigationCentroid = null;
            },
            onPointerSignal: (event) {
              if (event is PointerScrollEvent) {
                final keyboard = HardwareKeyboard.instance;
                setState(
                  () => _applyView(
                    _view.wheel(
                      event.scrollDelta,
                      event.localPosition,
                      zooming:
                          keyboard.isControlPressed || keyboard.isMetaPressed,
                      shift: keyboard.isShiftPressed,
                    ),
                  ),
                );
              }
            },
            onPointerDown: (event) {
              if (!event.localPosition.dx.isFinite ||
                  !event.localPosition.dy.isFinite ||
                  !_finite(_view.toPlan(event.localPosition)))
                return;
              if (_wallChainTool && event.buttons == kSecondaryMouseButton) {
                // R6.5.10 finish event: ends the chain, adds no segment.
                // Fires even mid-drag; the in-flight preview is discarded
                // and its owning pointer released, so its eventual up is
                // stale (R6.5.4).
                _finishChain();
                return;
              }
              if (event.buttons != kPrimaryButton &&
                  event.buttons != kMiddleMouseButton)
                return;
              if (_releasedGestureTouches.isNotEmpty ||
                  _navigationStart != null)
                return;
              if (_startSecondTouch(event)) return;
              if (_activePointer != null) return;
              _canvasFocus.requestFocus();
              setState(() {
                if (event.kind == PointerDeviceKind.touch)
                  _touches[event.pointer] = event.localPosition;
                _activePointer = event.pointer;
                _pointerPosition = event.localPosition;
                _pointerDownPosition = event.localPosition;
                _dragClassified = false;
                _pointerKind = event.kind;
                _selection.observeContext(_selectionContext);
                _selection.observePointer(event.localPosition);
                _selectionBeforeDrag = _selection.snapshot;
                final at = _view.toPlan(event.localPosition);
                _dragFrom = at;
                _dragTo = at;
                _snapRawStart = at;
                _snapStartQueried = false;
                _mode = _classifyGrab(at, event);
                // R6.4.2: Alt latches for the WHOLE chain gesture, sampled
                // once at the chain's first segment and preserved through
                // every continuation until finish/cancel. Standalone
                // Select / draw drags keep resampling on every press.
                if (!_wallChainTool || _chain.anchor == null) {
                  _snapAltDisabled = HardwareKeyboard.instance.isAltPressed;
                }
                _panStart = _mode == _DragMode.pan ? _view : null;
              });
            },
            onPointerMove: (event) {
              if (!event.localPosition.dx.isFinite ||
                  !event.localPosition.dy.isFinite ||
                  !event.delta.dx.isFinite ||
                  !event.delta.dy.isFinite ||
                  !_finite(_view.toPlan(event.localPosition)))
                return;
              if (_moveTouches(event)) return;
              if (event.pointer != _activePointer) return;
              _selection.observePointer(event.localPosition);
              final draw = _mode == _DragMode.draw;
              setState(() {
                if (_touches.containsKey(event.pointer))
                  _touches[event.pointer] = event.localPosition;
                _pointerPosition = event.localPosition;
                _classifyDisplacement(event.localPosition);
                if (_mode == _DragMode.pan) {
                  _applyView(_view.pan(event.delta));
                } else {
                  _dragTo = _view.toPlan(event.localPosition);
                }
              });
              if (draw && _dragClassified)
                _evaluateCreationSnap(_view.toPlan(event.localPosition));
              if (_editingDrag && _dragClassified)
                _evaluateEditSnap(_view.toPlan(event.localPosition));
            },
            onPointerUp: (event) {
              if (!event.localPosition.dx.isFinite ||
                  !event.localPosition.dy.isFinite ||
                  !_finite(_view.toPlan(event.localPosition)))
                return;
              if (_releasedGestureTouches.remove(event.pointer)) return;
              if (_touches.length == 2 && _touches.containsKey(event.pointer)) {
                _touches.remove(event.pointer);
                _releasedGestureTouches.addAll(_touches.keys);
                _touches.clear();
                _navigationStart = null;
                return;
              }
              if (event.pointer != _activePointer) return;
              _selection.observePointer(event.localPosition);
              _touches.remove(event.pointer);
              _activePointer = null;
              _panStart = null;
              _classifyDisplacement(event.localPosition);
              if (_mode != _DragMode.pan) {
                _dragTo = _view.toPlan(event.localPosition);
              }
              if (_mode == _DragMode.draw && _dragClassified)
                _evaluateCreationSnap(_view.toPlan(event.localPosition));
              if (_editingDrag && _dragClassified)
                _evaluateEditSnap(_view.toPlan(event.localPosition));
              _pointerPosition = null;
              _finishDrag(event.localPosition, event.timeStamp);
            },
            onPointerCancel: (event) {
              if (!_finite(event.localPosition)) return;
              if (event.pointer == _activePointer ||
                  _touches.containsKey(event.pointer) ||
                  event.pointer == _trackpadPointer)
                _cancelDrag();
              _releasedGestureTouches.remove(event.pointer);
            },
            child: ColoredBox(
              color: const Color(0xFFFAFAF7),
              child: CustomPaint(
                key: _paintKey,
                painter: PlanPainter(
                  _walls,
                  _derived,
                  _selected,
                  _draft ?? _chainPreview,
                  view: _view,
                  showGrid: _showGrid,
                  backgroundLevels: _backgroundLevels,
                  selectedJoin: _selectedJoinPosition,
                  selectedIds: _selection.ids,
                  selectedJoins: _selectedJoinPositions,
                  marqueeRect: _marqueeRect,
                  snapPoint: _snapPreview?.point,
                  snapGuide: _snapPreview?.guide,
                ),
                child: const SizedBox.expand(),
              ),
            ),
          ),
        );
      },
    );
  }

  /// Decides what this drag will do from what is under the cursor.
  ///
  /// Endpoints win over bodies, because they are smaller targets and the
  /// gesture people reach for most often is nudging a corner. A modifier or a
  /// non-primary button pans, so navigation never competes with drawing.
  _DragMode _classifyGrab(Offset at, PointerDownEvent e) {
    if (e.buttons == kMiddleMouseButton ||
        HardwareKeyboard.instance.logicalKeysPressed.contains(
          LogicalKeyboardKey.space,
        )) {
      return _DragMode.pan;
    }
    if (_wallChainTool) {
      // Creation-only: no hit-testing, selection, endpoint/body drag, or
      // marquee. The chain's fixed anchor (R6.5.10), not the raw press
      // point, is the segment start once a chain is already underway.
      final anchor = _chain.anchor;
      if (anchor != null) {
        _dragFrom = anchor;
        _snapRawStart = anchor;
        // The anchor is already the exact accepted endpoint (R6.5.10): a
        // continuation drag must not re-query native snapping for it, only
        // for the moving end.
        _snapStartQueried = true;
      }
      return _DragMode.draw;
    }
    if (_placing != null) return _DragMode.place;
    final hits = _hitsAt(at);
    if (hits == null) return _DragMode.none;
    final modified = (_modifierMask & toggleModifierMask) != 0;
    // R7.2.4: modifier-drag from an EMPTY start performs additive marquee.
    // Plain drag from empty draws a wall, unchanged.
    if (hits.isEmpty) return modified ? _DragMode.marquee : _DragMode.draw;
    final hit = hits.first;
    if (modified) {
      // R7.2.6: a modifier drag over an entity edits only when it is the
      // EXISTING primary; otherwise it emits no edit at all. This is
      // deliberately NOT `_DragMode.marquee` — starting on a real (if
      // non-primary) entity is a different gesture from starting on empty
      // canvas, and must stay the inert no-op R7.2.6 specifies, not a
      // rectangle selection.
      if (hit.entityId != _selection.primary) return _DragMode.selectOnly;
    } else {
      _selection.selectForDrag(hit.entityId);
    }
    if (hit.kind == 'join-node') {
      final member = _joins[hit.entityId]?.firstOrNull;
      if (member == null) return _DragMode.none;
      _dragWall = member.$1.id;
      _dragIsStart = member.$2;
      return _DragMode.endpoint;
    }
    _dragWall = hit.entityId;
    if (hit.kind == 'free-endpoint') {
      final wall = _walls.firstWhere((wall) => wall.id == hit.entityId);
      _dragIsStart = hit.closest == wall.start;
      return _DragMode.endpoint;
    }
    return _DragMode.body;
  }

  Offset? get _selectedJoinPosition {
    final member = _joins[_selected]?.firstOrNull;
    return member == null
        ? null
        : (member.$2 ? member.$1.start : member.$1.end);
  }

  Map<String, Offset> get _selectedJoinPositions => {
    for (final id in _selection.ids)
      if (_joins[id]?.firstOrNull case final member?)
        id: member.$2 ? member.$1.start : member.$1.end,
  };

  /// One native query serves press, click and opening-host targeting. A null
  /// result is an error, never empty space on which drawing may begin.
  List<_PlanHit>? _hitsAt(Offset at) {
    final bridge = _bridge;
    final level = _levelId;
    if (bridge == null || level == null || !_finite(at)) return null;
    final request = jsonEncode({
      'levelId': level,
      'pointMm': [at.dx, at.dy],
      'radiusMm':
          (_pointerKind == PointerDeviceKind.touch ? 24.0 : 12.0) / _view.zoom,
    });
    final ptr = toNative(request);
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.planHitTest(
        _handle,
        ptr,
        utf8.encode(request).length,
        out,
      );
      if (code != ok) {
        _setStatus('hit test failed: ${_describe(code)}', isError: true);
        return null;
      }
      return (jsonDecode(bridge.takeString(out)) as List<Object?>).map((entry) {
        final hit = entry as Map<String, Object?>;
        final point = (hit['closestMm'] as List<Object?>).cast<num>();
        return _PlanHit(
          hit['class'] as String,
          hit['entityId'] as String,
          Offset(point[0].toDouble(), point[1].toDouble()),
        );
      }).toList();
    } finally {
      release(ptr);
      release(out);
    }
  }

  /// The R7.3 marquee query: entity IDs whose interaction shape intersects
  /// the rectangle from `a` to `b` (plan mm, either order), in the core's
  /// marquee-key order. Called ONCE, on release — see `_marqueeRect`.
  List<String>? _marqueeAt(Offset a, Offset b) {
    final bridge = _bridge;
    final level = _levelId;
    if (bridge == null || level == null || !_finite(a) || !_finite(b)) {
      return null;
    }
    final request = jsonEncode({
      'levelId': level,
      'corner1Mm': [a.dx, a.dy],
      'corner2Mm': [b.dx, b.dy],
    });
    final ptr = toNative(request);
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.planMarquee(
        _handle,
        ptr,
        utf8.encode(request).length,
        out,
      );
      if (code != ok) {
        _setStatus('marquee failed: ${_describe(code)}', isError: true);
        return null;
      }
      final response =
          jsonDecode(bridge.takeString(out)) as Map<String, Object?>;
      return (response['entityIds'] as List<Object?>).cast<String>();
    } finally {
      release(ptr);
      release(out);
    }
  }

  /// R6.5.1: classification uses screen displacement, never plan-space length.
  /// Once the threshold has been crossed, returning to the start stays a drag.
  void _classifyDisplacement(Offset position) {
    final start = _pointerDownPosition;
    if (start != null && (position - start).distance > 5) {
      _dragClassified = true;
    }
  }

  /// One native snap query — the single request/response path shared by the
  /// active-drag preview below, chain click commits, and the idle chain
  /// hover preview, so all three read the same native winner and guide
  /// instead of drifting apart. Returns null on an ACTUAL bridge failure
  /// (surfaced via status here), which is distinct from an empty candidate
  /// list: a miss still returns the raw point unchanged (R6.6.1 — snapping
  /// is input quantization only, never a reason to invent geometry). Callers
  /// must never mutate state from a null result.
  ({Offset point, (Offset, Offset)? guide, bool snapped})? _queryNativeSnap(
    Offset raw, {
    Offset? anchorMm,
    Offset? previousDirectionMm,
    List<String> excludedWallIds = const [],
    List<String> excludedJoinIds = const [],
    bool angleTargets = true,
  }) {
    final bridge = _bridge;
    final level = _levelId;
    if (bridge == null || level == null || !_finite(raw)) return null;
    final a = _view.toPlan(Offset.zero);
    final z = _view.toPlan(Offset(_canvasSize.width, _canvasSize.height));
    final request = <String, Object?>{
      'levelId': level,
      'pointMm': [raw.dx, raw.dy],
      'radiusMm':
          (_pointerKind == PointerDeviceKind.touch ? 24.0 : 12.0) / _view.zoom,
      'visibleRectMm': [
        math.min(a.dx, z.dx),
        math.min(a.dy, z.dy),
        math.max(a.dx, z.dx),
        math.max(a.dy, z.dy),
      ],
      if (anchorMm != null) 'anchorMm': [anchorMm.dx, anchorMm.dy],
      // Native code adds angle targets relative to the accepted segment,
      // alongside the global family; the editor does not enumerate angles.
      if (previousDirectionMm != null)
        'previousDirection': [previousDirectionMm.dx, previousDirectionMm.dy],
      if (excludedWallIds.isNotEmpty) 'excludedWallIds': excludedWallIds,
      if (excludedJoinIds.isNotEmpty) 'excludedJoinIds': excludedJoinIds,
      'gridSpacingMm': _view.gridStepMm,
      'gridEnabled': _snapGrid,
      'objectEnabled': _snapObject,
      // R6.2.1 puts both angle families in wall-chain creation ONLY, so an
      // edit drag passes an anchor for the R6.1.3 exclusions without turning
      // that anchor into a ray origin.
      'angleEnabled': angleTargets && _snapAngle,
      'altDisabled': _snapAltDisabled,
    };
    try {
      final response = bridge.snap(_handle, request);
      final candidates = response['candidates'] as List<Object?>? ?? const [];
      if (candidates.isEmpty) return (point: raw, guide: null, snapped: false);
      final candidate = candidates.first as Map<String, Object?>;
      final pointMm = (candidate['pointMm'] as List<Object?>).cast<num>();
      final point = Offset(pointMm[0].toDouble(), pointMm[1].toDouble());
      if (!_finite(point)) return null;
      final guideMm = candidate['guideMm'] as List<Object?>?;
      final guide = guideMm == null
          ? null
          : (
              Offset(
                ((guideMm[0] as List<Object?>)[0] as num).toDouble(),
                ((guideMm[0] as List<Object?>)[1] as num).toDouble(),
              ),
              Offset(
                ((guideMm[1] as List<Object?>)[0] as num).toDouble(),
                ((guideMm[1] as List<Object?>)[1] as num).toDouble(),
              ),
            );
      return (point: point, guide: guide, snapped: true);
    } on BridgeException catch (error) {
      _setStatus('snap failed: ${error.detail ?? error.code}', isError: true);
      return null;
    }
  }

  /// Queries native snapping for the active new-wall pointer, caching its
  /// authoritative point and optional guide for the next frame.
  bool _queryCreationSnap(Offset cursor, {bool isStart = false}) {
    final bridge = _bridge;
    final level = _levelId;
    if (bridge == null || level == null || !_finite(cursor)) return false;
    final result = _queryNativeSnap(
      cursor,
      anchorMm: !isStart ? _dragFrom : null,
      previousDirectionMm: _wallChainTool ? _chain.previousDirection : null,
    );
    if (result == null) {
      setState(() {
        _snapPreview = null;
        _snapFailed = true;
      });
      return false;
    }
    setState(() {
      if (isStart) _dragFrom = result.point;
      _dragTo = result.point;
      _snapPreview = !result.snapped
          ? null
          : _SnapPreview(result.point, result.guide);
      _snapFailed = false;
    });
    if (isStart) _snapStartQueried = true;
    return true;
  }

  bool _evaluateCreationSnap(Offset cursor) {
    if (!_dragClassified || !_finite(cursor)) return false;
    if (!_snapStartQueried &&
        _snapRawStart != null &&
        !_queryCreationSnap(_snapRawStart!, isStart: true)) {
      return false;
    }
    return _queryCreationSnap(cursor);
  }

  /// Reevaluates the IDLE chain hover preview (no active pointer) against
  /// the current view/prefs — the same `_queryNativeSnap` path an active
  /// drag uses, so the preview always shows the native winner and guide,
  /// never a raw, un-snapped hover point.
  void _evaluateChainHover(Offset screenPosition) {
    final anchor = _chain.anchor;
    if (!_wallChainTool || anchor == null || !_finite(screenPosition)) return;
    final raw = _view.toPlan(screenPosition);
    if (!_finite(raw)) return;
    final result = _queryNativeSnap(
      raw,
      anchorMm: anchor,
      previousDirectionMm: _chain.previousDirection,
    );
    if (!mounted) return;
    setState(() {
      if (result == null) {
        _chainHoverAt = null;
        _snapPreview = null;
      } else {
        _chainHoverAt = result.point;
        _snapPreview = !result.snapped
            ? null
            : _SnapPreview(result.point, result.guide);
      }
    });
  }

  /// The in-progress wall, or null when the drag is too short to be one.
  (Offset, Offset)? get _draft {
    if (_mode != _DragMode.draw || !_dragClassified) return null;
    final a = _dragFrom, b = _dragTo;
    if (_snapFailed || a == null || b == null || a == b) return null;
    return (a, b);
  }

  /// The chain's hover preview between segments (no active pointer): the
  /// fixed anchor to the last observed hover position. A live drag already
  /// paints through `_draft` (dragFrom is the anchor while chaining).
  (Offset, Offset)? get _chainPreview {
    if (!_wallChainTool || _activePointer != null) return null;
    final anchor = _chain.anchor;
    final hover = _chainHoverAt;
    if (anchor == null || hover == null || !_finite(hover) || hover == anchor) {
      return null;
    }
    return (anchor, hover);
  }

  /// The rubber-band rectangle being dragged out, or null otherwise.
  ///
  /// VIEW-ONLY, like `_draft`: pure Dart geometry, no native query while this
  /// is shown. `_finishDrag` issues exactly one `domowik_plan_marquee` call,
  /// at release.
  (Offset, Offset)? get _marqueeRect {
    if (_mode != _DragMode.marquee || !_dragClassified) return null;
    final a = _dragFrom, b = _dragTo;
    if (a == null || b == null) return null;
    return (a, b);
  }

  /// Whether this gesture edits existing geometry, which is what decides
  /// that the moved wall excludes its own features (R6.1.3).
  bool get _editingDrag =>
      _mode == _DragMode.endpoint || _mode == _DragMode.body;

  /// The joins whose NODE this gesture moves.
  ///
  /// Only the nodes: the core derives patch exclusion from the wall list
  /// itself, because it knows the patch deforms whichever end is held and a
  /// front end cannot be relied on to work that out. A body drag carries both
  /// of the wall's nodes (R3.4.2); an endpoint drag carries only the one it
  /// grabbed, and the node at the far end stays where it is.
  List<String> get _movedJoinIds {
    final wallId = _dragWall;
    if (wallId == null) return const [];
    final body = _mode == _DragMode.body;
    return [
      for (final entry in _joins.entries)
        if (entry.value.any(
          (member) =>
              member.$1.id == wallId && (body || member.$2 == _dragIsStart),
        ))
          entry.key,
    ];
  }

  /// One native snap query for an endpoint or body drag.
  ///
  /// The editor holds no rule of its own here (ED-0101): it states which wall
  /// the gesture is moving and which of its nodes travel, and the core decides
  /// what remains a legal target. A failed query suppresses the commit rather
  /// than falling back to raw coordinates, exactly as the creation path does.
  bool _queryEditSnap(Offset cursor, {bool isStart = false}) {
    final wallId = _dragWall;
    if (!_editingDrag || wallId == null || !_finite(cursor)) return false;
    final result = _queryNativeSnap(
      cursor,
      // R6.1.3: the fixed endpoint is the anchor, so every candidate sitting
      // exactly on it — which is every acceptance producing a zero-length
      // wall (R3.0.5) — is filtered before ranking.
      anchorMm: _mode == _DragMode.endpoint
          ? _opposite(wallId, _dragIsStart)
          : null,
      excludedWallIds: [wallId],
      excludedJoinIds: _movedJoinIds,
      angleTargets: false,
    );
    if (result == null) {
      setState(() {
        _snapPreview = null;
        _snapFailed = true;
      });
      return false;
    }
    setState(() {
      if (isStart) _dragFrom = result.point;
      _dragTo = result.point;
      _snapPreview = !result.snapped
          ? null
          : _SnapPreview(result.point, result.guide);
      _snapFailed = false;
    });
    if (isStart) _snapStartQueried = true;
    return true;
  }

  /// Evaluates an edit drag, snapping the grab point as well as the cursor.
  ///
  /// Both ends go through the same query because R6.0.4 makes snapping a pure
  /// function of the pointer: measuring a body translation from a RAW grab to
  /// a SNAPPED release would invent a displacement the user never made, and
  /// would turn a drag that came back to where it started into an edit.
  bool _evaluateEditSnap(Offset cursor) {
    if (!_dragClassified || !_finite(cursor)) return false;
    final raw = _snapRawStart;
    if (!_snapStartQueried &&
        raw != null &&
        !_queryEditSnap(raw, isStart: true)) {
      return false;
    }
    return _queryEditSnap(cursor);
  }

  /// Ends the chain (double-click, Enter, right-click, tool switch): clears
  /// the pending anchor/preview after any already-committed segments,
  /// without adding a trailing one. Fires even mid-drag: `_cancelDrag`
  /// discards the in-flight preview and releases its owning pointer FIRST,
  /// so the eventual up for that pointer is stale and discarded entirely
  /// (R6.5.4), never completing a trailing segment — and, being the same
  /// path every other cancel uses, it also restores pre-gesture selection,
  /// rolls back an in-progress pan, and suppresses the released touch
  /// exactly like Escape/focus-loss/second-touch do. A no-op when nothing
  /// is pending (`_cancelDrag`'s own early-return guard covers that).
  void _finishChain() {
    _cancelDrag();
  }

  void _finishDrag(Offset logicalPosition, Duration timeStamp) {
    final mode = _mode;
    final classified = _dragClassified;
    final draft = _draft;
    final from = _dragFrom;
    final to = _dragTo;
    final wall = _dragWall;
    final before = _selectionBeforeDrag;
    // Only `draw` mode may create a chain segment. `_wallChainTool` can also
    // classify as `pan` (middle-click/space), which must keep panning, never
    // be diverted into a wall commit.
    final chaining = _wallChainTool && mode == _DragMode.draw;
    final snapFailed = _snapFailed;
    setState(() {
      _dragFrom = null;
      _dragTo = null;
      _mode = _DragMode.none;
      _dragWall = null;
      _pointerDownPosition = null;
      _dragClassified = false;
      _selectionBeforeDrag = null;
      _snapPreview = null;
      _snapFailed = false;
      _snapStartQueried = false;
      _snapRawStart = null;
    });

    if (chaining) {
      _finishChainGesture(
        classified,
        from,
        to,
        snapFailed,
        timeStamp,
        logicalPosition,
      );
      return;
    }

    if (mode != _DragMode.pan && mode != _DragMode.none && !classified) {
      final at = to ?? from;
      if (before != null) _selection.restoreSelection(before);
      if (at != null && !_placeOpeningAt(at)) _selectAt(at, logicalPosition);
      return;
    }

    switch (mode) {
      case _DragMode.pan:
      case _DragMode.none:
      case _DragMode.place:
      case _DragMode.selectOnly:
        return;
      case _DragMode.marquee:
        // ONE native query, here at release — not per pointer move.
        if (from != null && to != null) {
          final ids = _marqueeAt(from, to);
          if (ids != null) {
            setState(() => _selection.marquee(ids, additive: true));
          }
        }
        return;
      case _DragMode.endpoint:
        // `to` already carries the native winner; a refused query leaves the
        // gesture with no authoritative point, so it commits nothing.
        if (wall != null && from != null && to != null && to != from) {
          if (snapFailed) return;
          _moveEndpoint(wall, _dragIsStart, to);
        }
        return;
      case _DragMode.body:
        if (wall != null && from != null && to != null && to != from) {
          if (snapFailed) return;
          _moveWall(wall, from, to);
        }
        return;
      case _DragMode.draw:
        if (draft == null) {
          return;
        }
        _createWall(draft.$1, draft.$2);
    }
  }

  /// The endpoint that is NOT being dragged, which is what the axis snap for a
  /// dragged endpoint must measure against.
  Offset _opposite(String wallId, bool draggingStart) {
    final w = _walls.firstWhere((w) => w.id == wallId);
    return draggingStart ? w.end : w.start;
  }

  /// Places a door or window at the point on the wall nearest `at`.
  ///
  /// `u` is the parameter along the host's centre line, which is what R4.0.5
  /// stores — the click is projected onto the wall rather than used directly,
  /// so an opening always sits ON its host however imprecise the click was.
  bool _placeOpeningAt(Offset at) {
    final kind = _placing;
    if (kind == null) return false;
    final hits = _hitsAt(at);
    if (hits == null) return true;
    final host = hits.where((hit) => hit.kind == 'region').firstOrNull;
    final best = _walls.where((wall) => wall.id == host?.entityId).firstOrNull;
    if (best == null) {
      setState(
        () => _setStatus('click on a wall to place a $kind', isError: true),
      );
      return true;
    }
    final ab = best.end - best.start;
    final len2 = ab.dx * ab.dx + ab.dy * ab.dy;
    final u = len2 == 0
        ? 0.5
        : (((at - best.start).dx * ab.dx + (at - best.start).dy * ab.dy) / len2)
              .clamp(0.0, 1.0);
    _gesture({
      'kind': 'addOpening',
      'hostWallId': best.id,
      'opening': kind,
      'u': u,
      'widthMm': kind == 'door' ? 900.0 : 1200.0,
      'heightMm': kind == 'door' ? 2100.0 : 1400.0,
      'sillMm': kind == 'door' ? 0.0 : 900.0,
    }, 'place $kind');
    setState(() => _placing = null);
    return true;
  }

  /// Cycles native-ranked entity IDs without reimplementing geometry ranking.
  void _selectAt(Offset at, Offset logicalPosition) {
    final hits = _hitsAt(at);
    if (hits == null) return;
    setState(() {
      _selection.click(
        hits.map((hit) => hit.entityId),
        logicalPosition,
        _selectionContext,
      );
      _setStatus(
        _selection.ids.isEmpty
            ? 'nothing selected'
            : _selected == null
            ? '${_selection.ids.length} selected; no primary'
            : _joins.containsKey(_selected)
            ? 'selected a joined corner'
            : '${_selection.ids.length} selected; primary wall',
        isError: false,
      );
    });
  }

  void _createWall(Offset from, Offset to) {
    final levelId = _levelId;
    if (levelId == null) return;
    // Through the GESTURE call, not a hand-built batch. Drawing a wall across
    // another splits it, the split lands in the same batch, and the openings
    // on the crossed wall are remapped or the whole edit is refused. Building
    // the operations here would silently skip all of that.
    // `heightMm` is deliberately OMITTED: R2.1 has the gesture resolve the
    // active level's CURRENT `defaultWallHeightMm` at commit time, so a
    // level with a non-default wall height draws walls at that height
    // without this client tracking it. An explicit height override would
    // still win if a future control ever supplies one.
    _gesture({
      'kind': 'addWall',
      'levelId': levelId,
      'start': [from.dx, from.dy],
      'end': [to.dx, to.dy],
      'thicknessMm': 200.0,
    }, 'draw wall');
  }

  /// Advances (or starts) the Walls chain tool for one completed gesture —
  /// either a classified drag or an unclassified click — per R6.5.10/R3.1.1.
  void _finishChainGesture(
    bool classified,
    Offset? from,
    Offset? to,
    bool snapFailed,
    Duration timeStamp,
    Offset logicalPosition,
  ) {
    if (classified) {
      _lastChainClickTime = null;
      _lastChainClickScreen = null;
      if (snapFailed ||
          from == null ||
          to == null ||
          !_finite(from) ||
          !_finite(to) ||
          from == to) {
        return; // Cancelled/degenerate/failed-snap drag: anchor unmoved.
      }
      _createChainSegment(from, to);
      return;
    }

    // R6.5.1/R6.5.10: an unclassified release is a click. The first click of
    // a chain establishes the anchor with zero operations; every later click
    // completes a segment from that fixed anchor. A double-click's second
    // click is a finish event, not a zero-length segment.
    final lastTime = _lastChainClickTime;
    final lastScreen = _lastChainClickScreen;
    final isDoubleClick =
        lastTime != null &&
        lastScreen != null &&
        (timeStamp - lastTime) <= kDoubleTapTimeout &&
        (logicalPosition - lastScreen).distance <= kDoubleTapSlop;
    _lastChainClickTime = timeStamp;
    _lastChainClickScreen = logicalPosition;
    if (isDoubleClick) {
      _lastChainClickTime = null;
      _lastChainClickScreen = null;
      _finishChain();
      return;
    }

    final raw = to ?? from;
    if (raw == null || !_finite(raw)) return;
    final anchor = _chain.anchor;
    if (anchor == null) {
      // No previousDirection yet: this click IS the chain's first anchor.
      final result = _queryNativeSnap(raw);
      if (result == null) return; // Native failure surfaced; suppress start.
      if (!_finite(result.point)) return;
      setState(() {
        _chain.start(result.point);
        _snapPreview = !result.snapped
            ? null
            : _SnapPreview(result.point, result.guide);
      });
      return;
    }
    final result = _queryNativeSnap(
      raw,
      anchorMm: anchor,
      previousDirectionMm: _chain.previousDirection,
    );
    if (result == null) return; // Native failure surfaced; suppress commit.
    if (!_finite(result.point) || result.point == anchor) {
      return; // No zero-length click-click segment.
    }
    _createChainSegment(anchor, result.point);
  }

  /// Sends one chain segment through the ordinary `addWall` gesture (the
  /// same batch/undo boundary as splits/joins it triggers). The chain only
  /// advances past the accepted endpoint on success — a rejected segment
  /// must not move the anchor or `previousDirection` (R6.5.10). A first
  /// segment reached via a drag (no prior click establishing an anchor)
  /// initializes the chain's start from its actual snapped `from` first, so
  /// `previousDirection` is set rather than staying null.
  void _createChainSegment(Offset from, Offset to) {
    final levelId = _levelId;
    if (levelId == null) return;
    final code = _gesture({
      'kind': 'addWall',
      'levelId': levelId,
      'start': [from.dx, from.dy],
      'end': [to.dx, to.dy],
      'thicknessMm': 200.0,
    }, 'chain wall segment');
    if (code == ok) {
      setState(() {
        if (_chain.anchor == null) _chain.start(from);
        _chain.accept(to);
        _snapPreview = null;
        _chainHoverAt = null;
      });
    }
  }

  void _deleteSelected() {
    final id = _selected;
    if (_joins.containsKey(id)) {
      setState(
        () => _setStatus(
          'Select a wall to delete it; a joined corner is not a wall.',
          isError: true,
        ),
      );
      return;
    }
    if (id == null) {
      _setStatus(
        _selection.ids.isEmpty ? 'nothing selected' : 'no primary selected',
        isError: true,
      );
      setState(() {});
      return;
    }
    // Deleting a wall drops its join memberships and cascades its openings
    // (R3.1.8); a bare deleteEntity would leave both dangling.
    _gesture({'kind': 'deleteWall', 'wallId': id}, 'delete wall');
  }

  /// Moves one endpoint of a wall.
  ///
  /// R3.4.2 makes this move the whole JOIN NODE when the endpoint is joined —
  /// every member endpoint travels together in one batch, because moving only
  /// this wall's end would tear the corner apart and leave a join whose
  /// members disagree.
  void _moveEndpoint(String wallId, bool isStart, Offset to) {
    _gesture({
      'kind': 'moveWall',
      'wallId': wallId,
      if (isStart) 'start': [to.dx, to.dy] else 'end': [to.dx, to.dy],
    }, 'drag endpoint');
  }

  /// Translates a wall, carrying both of its nodes.
  void _moveWall(String wallId, Offset from, Offset to) {
    final w = _walls.firstWhere((w) => w.id == wallId);
    final d = to - from;
    _gesture({
      'kind': 'moveWall',
      'wallId': wallId,
      'start': [w.start.dx + d.dx, w.start.dy + d.dy],
      'end': [w.end.dx + d.dx, w.end.dy + d.dy],
    }, 'move wall');
  }

  /// Sends one gesture and refreshes.
  /// One row per room face, each with the name the core resolved and a button
  /// that sets it.
  ///
  /// Rooms are listed by FACE rather than by record because that is what a
  /// person points at: they want to name the room they can see. What happens
  /// underneath differs — a face with no record needs one created, a face with
  /// one needs it updated — and [`_nameRoom`] asks the core for the right
  /// command rather than deciding here.
  List<Widget> _roomPanel(ThemeData theme) {
    if (_derived.rooms.isEmpty) return const [];
    final homeless = _derived.roomRecords
        .where((r) => r.status != 'associated' && r.status != 'superseded')
        .length;
    return [
      const SizedBox(height: 8),
      for (var i = 0; i < _derived.rooms.length; i++)
        Padding(
          key: ValueKey('room-row-$i'),
          padding: const EdgeInsets.symmetric(vertical: 2),
          child: Row(
            children: [
              Expanded(
                child: Text(
                  _derived.rooms[i].label ?? 'Unnamed room',
                  style: _derived.rooms[i].label == null
                      ? theme.textTheme.bodySmall
                      : theme.textTheme.bodyMedium,
                  overflow: TextOverflow.ellipsis,
                ),
              ),
              Text(
                '${_derived.rooms[i].displayAreaM2 ?? '—'} m²',
                style: theme.textTheme.bodySmall,
              ),
              IconButton(
                key: ValueKey('name-room-$i'),
                tooltip: _derived.rooms[i].recordId == null
                    ? 'Name this room'
                    : 'Rename this room',
                icon: const Icon(Icons.label_outline, size: 18),
                onPressed: () => _nameRoom(_derived.rooms[i]),
              ),
            ],
          ),
        ),
      if (homeless > 0)
        Padding(
          padding: const EdgeInsets.only(top: 4),
          child: Text(
            '$homeless room record(s) seed no face on this level.',
            style: theme.textTheme.bodySmall,
          ),
        ),
    ];
  }

  /// Names or renames one room face.
  ///
  /// ED-0101 in one method. The editor decides only what the USER is trying to
  /// do — name the room they clicked — and sends a gesture. Which of the three
  /// R5.2.2 commands that becomes follows from whether the face already has an
  /// effective record, and the core owns everything after that: whether the
  /// supplied seed is strictly inside, which record wins a tie-break, what
  /// status each one ends up with. Even the SEED comes from the core, as the
  /// face's `seedCandidateMm`; picking a point here would be a containment
  /// test in the view, and it would put a record in a wall the first time
  /// someone named an L-shaped room.
  Future<void> _nameRoom(Ring room) async {
    final levelId = _levelId;
    if (levelId == null) return;
    final existing = room.recordId;
    final name = await _askForRoomName(
      existing == null ? '' : (room.label == 'Room' ? '' : room.label ?? ''),
      isNew: existing == null,
    );
    if (!mounted || name == null) return;

    if (existing != null) {
      _gesture({
        'kind': 'setRoom',
        'roomId': existing,
        'name': name,
      }, 'rename room');
      return;
    }
    final seed = room.seedCandidateMm;
    if (seed == null) {
      setState(
        () => _setStatus('that room has no interior to seed', isError: true),
      );
      return;
    }
    _gesture({
      'kind': 'addRoom',
      'levelId': levelId,
      'seed': [seed.dx, seed.dy],
      'name': name,
    }, 'name room');
  }

  /// Asks for a room name. Returns null when cancelled, which is not the same
  /// as returning the empty string: the empty string is a legal name.
  Future<String?> _askForRoomName(String initial, {required bool isNew}) {
    final controller = TextEditingController(text: initial);
    return showDialog<String>(
      context: context,
      builder: (context) => AlertDialog(
        title: Text(isNew ? 'Name this room' : 'Rename this room'),
        content: TextField(
          key: const ValueKey('room-name-field'),
          controller: controller,
          autofocus: true,
          decoration: const InputDecoration(labelText: 'Room name'),
          onSubmitted: (value) => Navigator.pop(context, value),
        ),
        actions: [
          TextButton(
            key: const ValueKey('room-name-cancel'),
            onPressed: () => Navigator.pop(context),
            child: const Text('Cancel'),
          ),
          FilledButton(
            key: const ValueKey('room-name-save'),
            onPressed: () => Navigator.pop(context, controller.text),
            child: const Text('Save'),
          ),
        ],
      ),
    );
  }

  int _gesture(Map<String, Object?> request, String what) {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) return invalidArgument;
    final text = jsonEncode(request);
    final ptr = toNative(text);
    final out = alloc<DomowikBuf>(sizeOf<DomowikBuf>());
    try {
      final code = bridge.planEdit(_handle, ptr, utf8.encode(text).length, out);
      if (code == ok) {
        bridge.takeString(out);
        _setStatus('$what accepted', isError: false);
      } else {
        _setStatus(
          '$what refused: ${_describe(code)} — ${_lastError(bridge)}',
          isError: true,
        );
      }
      return code;
    } finally {
      release(ptr);
      release(out);
      _refresh();
    }
  }

  Widget _controls(BuildContext context) {
    final theme = Theme.of(context);
    final flags = _levelView.visibility;
    final selected = _walls.where((wall) => wall.id == _selected).firstOrNull;
    return ListView(
      padding: const EdgeInsets.all(16),
      children: [
        Text('Your plan', style: theme.textTheme.titleLarge),
        const SizedBox(height: 4),
        Text('Experimental 2D editor', style: theme.textTheme.bodySmall),
        const SizedBox(height: 20),
        Row(
          children: [
            Text('Level', style: theme.textTheme.titleSmall),
            const Spacer(),
            IconButton(
              onPressed: _addLevel,
              icon: const Icon(Icons.add, size: 18),
              tooltip: 'Add a storey above',
            ),
          ],
        ),
        // A plain list rather than radios: `RadioListTile`'s group API is
        // deprecated in this Flutter, and a selectable row says the same thing
        // without adopting a widget that is on its way out.
        for (final level in _levels)
          ListTile(
            key: ValueKey('level-${level.id}'),
            dense: true,
            contentPadding: EdgeInsets.zero,
            selected: level.id == _levelId,
            leading: Icon(
              level.id == _levelId ? Icons.layers : Icons.layers_outlined,
              size: 18,
            ),
            title: Text(level.name),
            subtitle: Text(
              level.id == _levelId
                  ? 'Active'
                  : _visibilityLabel(flags[level.id]!),
            ),
            onTap: () => _activateLevel(level.id),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                IconButton(
                  key: ValueKey('level-properties-${level.id}'),
                  tooltip: 'Edit ${level.name}',
                  icon: const Icon(Icons.tune, size: 18),
                  onPressed: () => _editLevelProperties(level),
                ),
                PopupMenuButton<LevelVisibility>(
                  key: ValueKey('level-visibility-${level.id}'),
                  tooltip: 'Visibility: ${level.name}',
                  enabled: level.id != _levelId,
                  initialValue: flags[level.id],
                  icon: Icon(switch (flags[level.id]!) {
                    LevelVisibility.visible => Icons.visibility_outlined,
                    LevelVisibility.ghosted => Icons.layers_outlined,
                    LevelVisibility.hidden => Icons.visibility_off_outlined,
                  }, size: 18),
                  onSelected: (value) => _setLevelVisibility(level.id, value),
                  itemBuilder: (_) => [
                    for (final visibility in LevelVisibility.values)
                      PopupMenuItem(
                        value: visibility,
                        child: Text(_visibilityLabel(visibility)),
                      ),
                  ],
                ),
              ],
            ),
          ),
        OutlinedButton.icon(
          key: const ValueKey('delete-active-level'),
          onPressed: _levelId == null ? null : _deleteActiveLevel,
          icon: const Icon(Icons.delete_outline, size: 18),
          label: const Text('Delete active level'),
        ),
        const Divider(height: 32),
        Text('On this level', style: theme.textTheme.titleSmall),
        const SizedBox(height: 8),
        _counter('Walls', '${_walls.length}'),
        _counter('Openings', '${_derived.openings.length}'),
        _counter('Rooms', '${_derived.rooms.length}'),
        _counter(
          'Floor area',
          '${(_derived.floorAreaMm2 / 1e6).toStringAsFixed(2)} m²',
        ),
        ..._roomPanel(theme),
        const Divider(height: 32),
        Text('Selection', style: theme.textTheme.titleSmall),
        const SizedBox(height: 8),
        _counter('Selected', '${_selection.ids.length}'),
        if (_selection.ids.isNotEmpty && _selected == null) ...[
          const Text('No primary selected'),
          Text(
            'Click an object to make it the primary before editing or deleting.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
        ],
        if (_joins.containsKey(_selected)) ...[
          const Text('Joined corner'),
          Text(
            '${_joins[_selected]!.length} connected wall ends',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
          Text(
            'Drag this corner to move all connected ends together.',
            style: theme.textTheme.bodySmall,
          ),
        ] else if (selected == null)
          Text(
            'Click a wall to see its dimensions. Drag its ends to reshape it.',
            style: theme.textTheme.bodySmall,
          )
        else ...[
          _counter(
            'Length',
            '${(selected.end - selected.start).distance.toStringAsFixed(0)} mm',
          ),
          _counter(
            'Thickness',
            '${selected.thicknessMm.toStringAsFixed(0)} mm',
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            onPressed: _deleteSelected,
            icon: const Icon(Icons.delete_outline, size: 18),
            label: const Text('Delete wall'),
          ),
        ],
        const Divider(height: 32),
        Text('Move around', style: theme.textTheme.titleSmall),
        const SizedBox(height: 8),
        Text(
          'Scroll to pan. Hold ⌘ or Ctrl while scrolling to zoom. '
          'Middle-drag or hold Space and drag to pan; Escape cancels. '
          'Drag a wall to move it, or an endpoint to move its corner.',
          style: theme.textTheme.bodySmall,
        ),
        const SizedBox(height: 8),
        Text(
          'Click again to cycle overlapping objects. Shift, ⌘ or Ctrl-click '
          'toggles selection. Edits act only on the primary object.',
          style: theme.textTheme.bodySmall,
        ),
        const SizedBox(height: 8),
        Text(
          'Shift, ⌘ or Ctrl-drag from empty space adds everything in the '
          'rectangle to the selection. Escape cancels.',
          style: theme.textTheme.bodySmall,
        ),
        const Divider(height: 32),
        ExpansionTile(
          key: const PageStorageKey('editor-developer-diagnostics'),
          tilePadding: EdgeInsets.zero,
          title: const Text(
            'Developer diagnostics',
            style: TextStyle(fontSize: 13),
          ),
          children: [
            _counter('Bridge ABI', '$_abi'),
            _counter('Open plans', '$_openPlans'),
            _counter('Outstanding buffers', '$_outstandingBuffers'),
            _counter('Panics survived', '$_panicsSurvived'),
            const SizedBox(height: 8),
            OutlinedButton.icon(
              onPressed: _addWall,
              icon: const Icon(Icons.add),
              label: const Text('Add demo rectangle wall'),
            ),
            OutlinedButton.icon(
              onPressed: _addInvalidWall,
              icon: const Icon(Icons.block),
              label: const Text('Send invalid wall'),
            ),
            OutlinedButton.icon(
              onPressed: _panic,
              icon: const Icon(Icons.bolt),
              label: const Text('Panic inside Rust'),
            ),
          ],
        ),
      ],
    );
  }

  Widget _counter(String label, String value) {
    return InspectorMetric(label: label, value: value);
  }

  static String _visibilityLabel(LevelVisibility visibility) =>
      switch (visibility) {
        LevelVisibility.visible => 'Visible',
        LevelVisibility.ghosted => 'Ghosted',
        LevelVisibility.hidden => 'Hidden',
      };
}
