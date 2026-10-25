import 'dart:async';
import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';

import 'camera_3d.dart';
import 'core/domowik_core.dart';
import 'editor_workflows.dart';
import 'platform/scene_presenter_host.dart';
import 'scene_presenter.dart';

/// Monotonic clock for scene-load instrumentation; benchmarks share it.
final Stopwatch sceneClock = Stopwatch()..start();

/// One scene load: whether it was transform-only, when it started and ended
/// on [sceneClock] (µs), whether it installed a scene, and the widget's
/// `poseRevision` when it started. A load includes at least every pose
/// preview up to that revision; it may include a later one.
class SceneLoadSample {
  const SceneLoadSample(
    this.poseOnly,
    this.startMicros,
    this.endMicros,
    this.ok,
    this.poseRevision,
  );
  final bool poseOnly, ok;
  final int startMicros, endMicros, poseRevision;
  int get micros => endMicros - startMicros;
}

/// What a click in the 3D view landed on. [entityId] is null for floors,
/// join patches and empty space; [planMm] is the hit in plan millimetres.
class ScenePick {
  const ScenePick({
    this.entityId,
    this.levelId,
    this.planMm,
    required this.position,
    this.open = false,
  });
  final String? entityId, levelId;
  final Offset? planMm;

  /// The click in the 3D view's logical pixels.
  final Offset position;

  /// A double click: callers reveal the hit, not only select it.
  final bool open;
}

/// GPU texture client. Document revisions trigger asynchronous scene work;
/// pointer/keyboard navigation sends only one coalesced camera matrix per frame.
class ThreeDView extends StatefulWidget {
  const ThreeDView({
    super.key,
    required this.core,
    required this.handle,
    required this.revision,
    this.assetRevision = 0,
    this.poseRevision = 0,
    required this.levelIds,
    required this.ghostedIds,
    required this.floorElevationMm,
    required this.active,
    this.selectedIds = const [],
    this.onPick,
    this.onUndo,
    this.onRedo,
  });
  final DomowikCore core;
  final int handle, revision;

  /// The core's embedded-content revision; a change (an uploaded model
  /// restoring missing bytes) reloads the scene like a document change.
  final int assetRevision;
  final int poseRevision;
  final List<String> levelIds, ghostedIds;
  final double floorElevationMm;
  final bool active;

  /// Entities tinted in 3D. A change re-sends transforms only, never meshes.
  final List<String> selectedIds;
  final ValueChanged<ScenePick>? onPick;
  final VoidCallback? onUndo, onRedo;
  @override
  State<ThreeDView> createState() => ThreeDViewState();
}

class ThreeDViewState extends State<ThreeDView> with TickerProviderStateMixin {
  static const channel = MethodChannel('invalid.domowik-spike/three-d');

  /// This host's presenter: the native one on [channel], or the browser's.
  late final ScenePresenter _presenter = hostScenePresenter(
    channel,
    widget.core,
  );
  final Camera3D camera = Camera3D();
  final FocusNode _focus = FocusNode();
  final Set<LogicalKeyboardKey> _held = {};
  final Set<String> _uploadedModels = {};
  // Created in initState: a lazy ticker would first be created by dispose()
  // for a view that never navigated, which Flutter forbids.
  late final Ticker _ticker;
  Duration? _lastTick;
  int? textureId, session;
  int _generation = 0;
  // Presenters that derive scenes inside Rust (macOS, issue 0101) say so at
  // `create`; they receive the plan handle instead of pushed geometry, and a
  // monotonic load number that lets them refuse a stale load finishing late.
  bool _presenterLoadsScene = false;
  bool _hasScene = false, _poseOnly = false;
  int? _sceneToken, _poseVersion;
  bool _installingScene = false;
  final GlobalKey _textureBoundary = GlobalKey();
  int _loadNumber = 0;
  bool _creating = false,
      _loading = false,
      _sceneDirty = true,
      _fitNext = true,
      _cameraScheduled = false,
      _cameraInFlight = false;
  bool _resetSurface = true;
  bool _mouseDragging = false, _pan = false;
  // Click detection and the orbit pivot picked under the press.
  Offset? _pressAt;
  bool _pressMoved = false, _pressPrimary = false;
  final Set<int> _down = {};
  Duration? _lastClickTime;
  Offset? _lastClickAt;
  List<double>? _pivot;
  // Animated framing: interpolates target and distance.
  late final Ticker _flight;
  Duration? _flightStart;
  List<double> _flightFrom = const [], _flightTo = const [];
  double _flightFromDistance = 0, _flightToDistance = 0;
  double _lastScale = 1;
  Size _size = const Size(800, 600);
  List<double> _center = [0, 1, 0], _origin = [0, 0];
  double _radius = 5;
  String? _error;
  // Errors that clear themselves: the presenter's frame health (pushed as a
  // `health` call) and the last camera update. Kept apart from `_error` so a
  // recovery never hides an unrelated scene or surface error.
  String? _renderError, _cameraError;
  String? get _shownError => _error ?? _renderError ?? _cameraError;
  List<String> _diagnostics = [];
  Map<String, Object?>? _pendingCamera;
  int sceneRequests = 0, meshRequests = 0;

  /// Scene-load samples, recorded only while non-null (benchmarks).
  List<SceneLoadSample>? loadSamples;

  /// The 3D viewport's logical size, as picks address it.
  Size get viewportSize => _size;
  String? get diagnosticError => _shownError;
  bool get loading => _loading;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_tick);
    _flight = createTicker(_fly);
    _presenter.setMethodCallHandler(_fromPresenter);
    if (widget.active) _ensureSurface();
  }

  /// Calls the presenter makes on its own: `health` reports a frame failure
  /// (`error` set) or the recovery that follows it (`error` null).
  Future<Object?> _fromPresenter(MethodCall call) async {
    final args = call.arguments;
    if (call.method != 'health' || args is! Map || args['session'] != session) {
      return null;
    }
    if (mounted) {
      final error = args['error'];
      setState(
        () => _renderError = error is String
            ? '3D rendering failed: $error'
            : null,
      );
    }
    return null;
  }

  @override
  void didUpdateWidget(ThreeDView old) {
    super.didUpdateWidget(old);
    if (old.handle != widget.handle ||
        old.revision != widget.revision ||
        old.assetRevision != widget.assetRevision ||
        old.levelIds.join(',') != widget.levelIds.join(',') ||
        old.ghostedIds.join(',') != widget.ghostedIds.join(',')) {
      _generation++;
      _sceneDirty = true;
      _poseOnly = false;
      if (old.handle != widget.handle) {
        _interrupt();
        _uploadedModels.clear();
        _fitNext = true;
        _origin = [0, 0];
        _resetSurface = true;
        _hasScene = false;
      }
    } else if (old.poseRevision != widget.poseRevision ||
        old.selectedIds.join(',') != widget.selectedIds.join(',')) {
      // Preview changes must not starve the display by invalidating every
      // in-flight load. Present that result, then load the latest pose. There
      // is at most one in flight and one dirty/latest request, never a queue.
      // During a full load, the pending update can reuse that load's baseline.
      if (!_sceneDirty) _poseOnly = _hasScene || _loading;
      _sceneDirty = true;
    }
    if (widget.active) {
      _ensureSurface();
    } else {
      _held.clear();
      _ticker.stop();
      _flight.stop();
      _focus.unfocus();
    }
  }

  @override
  void dispose() {
    _generation++;
    _presenter.setMethodCallHandler(null);
    _ticker.dispose();
    _flight.dispose();
    _focus.dispose();
    if (session != null)
      _presenter
          .invokeMethod<void>('destroy', {'session': session})
          .catchError((Object _) {});
    super.dispose();
  }

  Future<void> _ensureSurface() async {
    if (_creating || !mounted || !widget.active) return;
    if (session == null) {
      _creating = true;
      try {
        final result = (await _presenter.invokeMapMethod<String, Object?>(
          'create',
        ))!;
        if (!mounted) {
          await _presenter.invokeMethod<void>('destroy', {
            'session': result['session'],
          });
          return;
        }
        setState(() {
          textureId = result['textureId'] as int;
          session = result['session'] as int;
          _presenterLoadsScene = result['loadScene'] == true;
          _error = null;
        });
      } catch (e) {
        if (mounted) setState(() => _error = '3D GPU surface unavailable: $e');
        return;
      } finally {
        _creating = false;
      }
    }
    if (_sceneDirty && !_loading) _loadScene();
  }

  Future<void> _loadScene() async {
    if (session == null || _loading || !widget.active) return;
    final generation = _generation, handle = widget.handle, surface = session!;
    final poseOnly = _poseOnly && _hasScene;
    final started = sceneClock.elapsedMicroseconds;
    final poseRevision = widget.poseRevision;
    var installed = false;
    _poseOnly = false;
    _loading = true;
    _sceneDirty = false;
    setState(() => _error = null);
    bool current() =>
        mounted &&
        generation == _generation &&
        session == surface &&
        widget.active;
    try {
      if (_resetSurface) {
        await _presenter.invokeMethod<void>('reset', {'session': surface});
        _resetSurface = false;
      }
      sceneRequests++;
      final Map<String, Object?> scene;
      if (_presenterLoadsScene) {
        scene = (await _presenter.invokeMapMethod<String, Object?>(
          'loadScene',
          {
            'session': surface,
            'plan': handle,
            'generation': ++_loadNumber,
            'levelIds': widget.levelIds,
            'ghostedIds': widget.ghostedIds,
            'selectedIds': widget.selectedIds,
            'poseOnly': poseOnly,
            'sceneToken': _sceneToken,
          },
        ))!;
      } else {
        scene = (await widget.core.assetsInBackground(handle, {
          'kind': 'scene',
          'levelIds': widget.levelIds,
          'ghostedIds': widget.ghostedIds,
          'selectedIds': widget.selectedIds,
          'poseOnly': poseOnly,
          'sceneToken': _sceneToken,
        })) as Map<String, Object?>;
      }
      if (!current()) {
        // A presenter that loads in Rust has already installed this scene,
        // but its camera metadata was not applied: load again when shown.
        _sceneDirty = true;
        return;
      }
      if (_presenterLoadsScene) {
        _finishScene(scene);
        installed = true;
        return;
      }
      final meshes = scene['instanceOnly'] == true
          ? <String, Object?>{}
          : scene['meshes'] as Map<String, Object?>;
      // Mutable geometry must not replace resident meshes individually: old
      // nodes/camera could render with only some meshes rebased to a new origin.
      final geometry = {
        for (final entry in meshes.entries)
          entry.key: {
            'vertices': Float32List.fromList(
              (entry.value as List)
                  .cast<num>()
                  .map((v) => v.toDouble())
                  .toList(),
            ),
          },
      };
      for (final digest in (scene['requiredModels'] as List).cast<String>()) {
        if (_uploadedModels.contains(digest)) continue;
        if (!current()) return;
        meshRequests++;
        final mesh = (await widget.core.assetsInBackground(handle, {
          'kind': 'mesh',
          'digest': digest,
        })) as Map<String, Object?>;
        if (!current()) return;
        final texture = mesh['texture'] as Map<String, Object?>?;
        await _presenter.invokeMethod<void>('mesh', {
          'session': surface,
          'id': digest,
          if (texture != null)
            'texture': {
              'width': texture['width'],
              'height': texture['height'],
              'layers': texture['layers'],
              'rgba': Uint8List.fromList((texture['rgba'] as List).cast<int>()),
              'coordinates': Float32List.fromList(
                (texture['coordinates'] as List)
                    .cast<num>()
                    .map((v) => v.toDouble())
                    .toList(),
              ),
            },
          'vertices': Float32List.fromList(
            (mesh['vertices'] as List)
                .cast<num>()
                .map((v) => v.toDouble())
                .toList(),
          ),
        });
        _uploadedModels.add(digest);
      }
      if (!current()) return;
      final nodes = [
        for (final item in scene['nodes'] as List)
          {
            'mesh': item['mesh'],
            'transparent': item['transparent'],
            for (final key in ['matrix', 'normalMatrix', 'color'])
              key: Float32List.fromList(
                (item[key] as List)
                    .cast<num>()
                    .map((v) => v.toDouble())
                    .toList(),
              ),
          },
      ];
      final nextCamera = Camera3D.copy(camera);
      _rebaseCamera(nextCamera, scene);
      nextCamera.radius = math.max(1, (scene['radius'] as num).toDouble());
      if (_fitNext) {
        nextCamera.fit(
          (scene['center'] as List)
              .cast<num>()
              .map((v) => v.toDouble())
              .toList(),
          (scene['radius'] as num).toDouble(),
        );
      }
      _installingScene = true;
      _pendingCamera = null;
      try {
        await _presenter.invokeMethod<void>('scene', {
          ..._cameraPacket(nextCamera),
          'session': surface,
          'meshes': geometry,
          'nodes': nodes,
        });
        // Even if a newer revision arrived during commit, this is the scene
        // now on screen. Adopt its origin before navigation or the next load.
        if (mounted && session == surface && widget.handle == handle) {
          _finishScene(scene);
          installed = true;
        }
      } finally {
        _installingScene = false;
        if (mounted && _pendingCamera != null) _sendCamera();
      }
      if (!current()) return;
    } catch (e) {
      if (current()) {
        if (poseOnly) {
          // A presenter may have lost resident resources. Retry once with a
          // complete scene; never keep requesting names-only data after that.
          _hasScene = false;
          _poseOnly = false;
          _sceneDirty = true;
        } else {
          setState(() => _error = 'Could not build 3D scene: $e');
        }
      }
    } finally {
      loadSamples?.add(
        SceneLoadSample(
          poseOnly,
          started,
          sceneClock.elapsedMicroseconds,
          installed,
          poseRevision,
        ),
      );
      _loading = false;
      if (mounted) {
        setState(() {});
        if (_sceneDirty && widget.active) _loadScene();
      }
    }
  }

  void _finishScene(Map<String, Object?> scene) {
    final nextOrigin = (scene['originMm'] as List).cast<num>();
    final shiftX = (_origin[0] - nextOrigin[0]) / 1000;
    final shiftZ = -(_origin[1] - nextOrigin[1]) / 1000;
    _rebaseCamera(camera, scene);
    // Framing animation and camera adopt the committed origin together.
    for (final point in [_flightFrom, _flightTo]) {
      if (point.length == 3) {
        point[0] += shiftX;
        point[2] += shiftZ;
      }
    }
    _origin = (scene['originMm'] as List)
        .cast<num>()
        .map((v) => v.toDouble())
        .toList();
    _center = (scene['center'] as List)
        .cast<num>()
        .map((v) => v.toDouble())
        .toList();
    _radius = (scene['radius'] as num).toDouble();
    camera.radius = math.max(1, _radius);
    _hasScene = true;
    _sceneToken = (scene['sceneToken'] as num?)?.toInt();
    _poseVersion = (scene['poseVersion'] as num?)?.toInt();
    if (_fitNext) {
      camera.fit(_center, _radius);
      _fitNext = false;
    }
    setState(
      () => _diagnostics = (scene['diagnostics'] as List).cast<String>(),
    );
    _sendCamera();
  }

  /// Zooms toward the scene point under logical pixel [at].
  void _zoomAt(Offset at, double factor) {
    final ray = camera.ray(at.dx, at.dy, _size.width, _size.height);
    if (ray == null) {
      camera.zoom(factor);
    } else {
      camera.zoomToward(factor, ray.$2);
    }
    _sendCamera();
  }

  void _rebaseCamera(Camera3D target, Map<String, Object?> scene) {
    final origin = (scene['originMm'] as List).cast<num>();
    target.rebase(
      (_origin[0] - origin[0]) / 1000,
      -(_origin[1] - origin[1]) / 1000,
    );
  }

  Map<String, Object?> _cameraPacket(Camera3D target) {
    final ratio = MediaQuery.devicePixelRatioOf(context);
    return {
      'session': session,
      'matrix': target.matrix(_size.width / math.max(1, _size.height)),
      'width': (_size.width * ratio).round().clamp(32, 4096),
      'height': (_size.height * ratio).round().clamp(32, 4096),
    };
  }

  void _sendCamera() {
    if (!mounted || !widget.active || session == null) return;
    _pendingCamera = _cameraPacket(camera);
    if (_cameraScheduled) return;
    _cameraScheduled = true;
    SchedulerBinding.instance.addPostFrameCallback((_) {
      _cameraScheduled = false;
      _flushCamera();
    });
    SchedulerBinding.instance.scheduleFrame();
  }

  Future<void> _flushCamera() async {
    if (_cameraInFlight || _installingScene || !mounted) return;
    final packet = _pendingCamera;
    if (packet == null) return;
    _pendingCamera = null;
    _cameraInFlight = true;
    try {
      await _presenter.invokeMethod<void>('camera', packet);
      if (mounted && _cameraError != null) setState(() => _cameraError = null);
    } catch (e) {
      if (mounted) setState(() => _cameraError = '3D camera update failed: $e');
    } finally {
      _cameraInFlight = false;
      if (_pendingCamera != null && mounted) _flushCamera();
    }
  }

  void _tick(Duration elapsed) {
    final dt = _lastTick == null
        ? 0.0
        : (elapsed - _lastTick!).inMicroseconds / 1e6;
    _lastTick = elapsed;
    if (!widget.active || _held.isEmpty) return;
    bool down(LogicalKeyboardKey k) => _held.contains(k);
    final speed =
        math.min(dt, 0.05) *
        (HardwareKeyboard.instance.isShiftPressed ? 9.0 : 3.0);
    if (camera.walking) {
      camera.move(
        (down(LogicalKeyboardKey.keyW) || down(LogicalKeyboardKey.arrowUp)
                ? speed
                : 0) -
            (down(LogicalKeyboardKey.keyS) || down(LogicalKeyboardKey.arrowDown)
                ? speed
                : 0),
        (down(LogicalKeyboardKey.keyD) ? speed : 0) -
            (down(LogicalKeyboardKey.keyA) ? speed : 0),
        (down(LogicalKeyboardKey.keyE) ? speed : 0) -
            (down(LogicalKeyboardKey.keyQ) ? speed : 0),
      );
      if (down(LogicalKeyboardKey.arrowLeft)) camera.yaw += speed * 0.4;
      if (down(LogicalKeyboardKey.arrowRight)) camera.yaw -= speed * 0.4;
    } else {
      camera.orbit(
        (down(LogicalKeyboardKey.arrowRight) ? speed * 30 : 0) -
            (down(LogicalKeyboardKey.arrowLeft) ? speed * 30 : 0),
        (down(LogicalKeyboardKey.arrowDown) ? speed * 30 : 0) -
            (down(LogicalKeyboardKey.arrowUp) ? speed * 30 : 0),
      );
    }
    _sendCamera();
  }

  /// A synchronous view query against the presented scene, or null when it
  /// is stale, refused, or no scene is installed yet.
  Map<String, Object?>? _sceneQuery(Map<String, Object?> request) {
    final token = _sceneToken, poseVersion = _poseVersion;
    if (!_hasScene || token == null || poseVersion == null) return null;
    final reply = widget.core.assets(
      widget.handle,
      jsonEncode({
        ...request,
        'levelIds': widget.levelIds,
        'ghostedIds': widget.ghostedIds,
        'sceneToken': token,
        'poseVersion': poseVersion,
      }),
    );
    if (!reply.succeeded) return null;
    final result = jsonDecode(reply.text!) as Map<String, Object?>;
    return result['stale'] == true ? null : result;
  }

  /// The pick answer under logical pixel [at]: `{hit: …}` with a null hit
  /// for empty space, or null when no current scene could answer (stale,
  /// loading or refused), which callers must not treat as empty space.
  Map<String, Object?>? pickAt(Offset at) {
    final ray = camera.ray(at.dx, at.dy, _size.width, _size.height);
    if (ray == null) return null;
    return _sceneQuery({
      'kind': 'pick3d',
      'originM': ray.$1,
      'direction': ray.$2,
      'rangeM': camera.visibleRange(ray.$2),
    });
  }

  void _click(Offset at, Duration now) {
    final open =
        _lastClickTime != null &&
        _lastClickAt != null &&
        now - _lastClickTime! < const Duration(milliseconds: 400) &&
        (at - _lastClickAt!).distance < 6;
    _lastClickTime = open ? null : now;
    _lastClickAt = at;
    final onPick = widget.onPick;
    if (onPick == null) return;
    final answer = pickAt(at);
    // The scene on screen is being replaced: neither select nor clear.
    if (answer == null) return;
    final hit = answer['hit'] as Map<String, Object?>?;
    final point = (hit?['pointM'] as List?)?.cast<num>();
    onPick(
      ScenePick(
        entityId: hit?['entityId'] as String?,
        levelId: hit?['levelId'] as String?,
        planMm: point == null
            ? null
            : Offset(
                _origin[0] + point[0] * 1000,
                _origin[1] - point[2] * 1000,
              ),
        position: at,
        open: open,
      ),
    );
  }

  /// Frames the selection, or the whole scene when nothing selected is drawn.
  void frameSelection() {
    final bounds = widget.selectedIds.isEmpty
        ? null
        : _sceneQuery({
                'kind': 'bounds3d',
                'entityIds': widget.selectedIds,
              })?['bounds']
              as Map<String, Object?>?;
    if (bounds == null) {
      _flyTo(_center, _radius);
      return;
    }
    final low = (bounds['lowM'] as List).cast<num>();
    final high = (bounds['highM'] as List).cast<num>();
    final half = [for (var i = 0; i < 3; i++) (high[i] - low[i]) / 2];
    _flyTo([
      for (var i = 0; i < 3; i++) (low[i] + high[i]) / 2,
    ], math.sqrt(half.fold<double>(0, (s, v) => s + v * v)));
  }

  /// Animates the orbit camera to frame a sphere over 250 ms.
  void _flyTo(List<double> center, double extent) {
    camera.walking = false;
    _flightFrom = List.of(camera.target);
    _flightFromDistance = camera.distance;
    _flightTo = List.of(center);
    camera.radius = math.max(camera.radius, extent);
    _flightToDistance = camera.framingDistance(
      extent,
      _size.width / math.max(1, _size.height),
    );
    _flightStart = null;
    if (!_flight.isActive) _flight.start();
    setState(() {});
  }

  void _fly(Duration elapsed) {
    _flightStart ??= elapsed;
    final t = math.min(1.0, (elapsed - _flightStart!).inMicroseconds / 250000);
    final k = 1 - math.pow(1 - t, 3).toDouble();
    camera.target = [
      for (var i = 0; i < 3; i++)
        _flightFrom[i] + (_flightTo[i] - _flightFrom[i]) * k,
    ];
    camera.distance =
        _flightFromDistance + (_flightToDistance - _flightFromDistance) * k;
    _sendCamera();
    if (t >= 1) _flight.stop();
  }

  /// Any direct navigation takes over from an animated framing.
  void _interrupt() {
    if (_flight.isActive) _flight.stop();
  }

  Future<Map<String, Object?>> stats() async => (await _presenter
      .invokeMapMethod<String, Object?>('stats', {'session': session}))!;
  Future<void> diagnosticCapture() async {
    await _presenter.invokeMethod<void>('diagnosticCapture', {
      'session': session,
    });
    _sendCamera();
  }

  /// Probe/automation seam using exactly the normal coalesced camera path.
  void cameraChanged() => _sendCamera();

  /// An explicit one-time native orientation diagnostic, outside normal frames.
  /// Diagnostic: counts red pixels in the top half and blue pixels in the
  /// bottom half of the texture as FLUTTER composites it, so the check covers
  /// the presenter's hand-off and Flutter's sampling, not only what the
  /// renderer drew. Same classification as the native readback.
  Future<Map<String, int>> compositedOrientation() async {
    final boundary =
        _textureBoundary.currentContext?.findRenderObject()
            as RenderRepaintBoundary?;
    if (boundary == null) return {'topRedPixels': 0, 'bottomBluePixels': 0};
    final image = await boundary.toImage();
    try {
      final bytes = (await image.toByteData())!;
      var top = 0, bottom = 0;
      for (var i = 0; i < bytes.lengthInBytes; i += 4) {
        final r = bytes.getUint8(i), g = bytes.getUint8(i + 1);
        final b = bytes.getUint8(i + 2);
        final row = i ~/ 4 ~/ image.width;
        if (row < image.height ~/ 2 && r > 80 && r > 2 * b && r > 2 * g) top++;
        if (row >= image.height ~/ 2 && b > 80 && b > 2 * r && b > 2 * g) {
          bottom++;
        }
      }
      return {'topRedPixels': top, 'bottomBluePixels': bottom};
    } finally {
      image.dispose();
    }
  }

  Future<Map<String, Object?>> probeOrientation() async {
    await _presenter.invokeMethod<void>('orientationProbe', {
      'session': session,
    });
    Map<String, Object?> result = {};
    for (var i = 0; i < 200; i++) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
      result = (await _presenter.invokeMapMethod<String, Object?>(
        'orientationStats',
        {'session': session},
      ))!;
      if ((result['topRedPixels'] as int) > 1000 &&
          (result['bottomBluePixels'] as int) > 1000)
        break;
    }
    return result;
  }

  @override
  Widget build(BuildContext context) {
    // Stretched: a toolbar narrower than the pane left a gap at its sides
    // through which whatever lay behind the 3D view showed.
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: [
        Material(
          color: Theme.of(context).colorScheme.surfaceContainerLow,
          child: SingleChildScrollView(
            scrollDirection: Axis.horizontal,
            child: Row(
              children: [
                const Padding(
                  padding: EdgeInsets.symmetric(horizontal: 12),
                  child: Text('3D view'),
                ),
                ChoiceChip(
                  key: const ValueKey('3d-orbit'),
                  label: const Text('Orbit'),
                  selected: !camera.walking,
                  onSelected: (_) {
                    _interrupt();
                    setState(() => camera.fit(_center, _radius));
                    _sendCamera();
                    _focus.requestFocus();
                  },
                ),
                const SizedBox(width: 8),
                ChoiceChip(
                  key: const ValueKey('3d-walk'),
                  label: const Text('Walk through'),
                  selected: camera.walking,
                  onSelected: (_) {
                    _interrupt();
                    setState(
                      () => camera.enterWalk(widget.floorElevationMm / 1000),
                    );
                    _sendCamera();
                    _focus.requestFocus();
                  },
                ),
                IconButton(
                  key: const ValueKey('3d-fit'),
                  tooltip: 'Fit 3D scene',
                  onPressed: () {
                    _interrupt();
                    setState(() => camera.fit(_center, _radius));
                    _sendCamera();
                    _focus.requestFocus();
                  },
                  icon: const Icon(Icons.fit_screen),
                ),
                IconButton(
                  tooltip: '3D renderer statistics',
                  onPressed: () async {
                    final data = await stats();
                    if (!context.mounted) return;
                    showDialog<void>(
                      context: context,
                      builder: (_) => AlertDialog(
                        title: const Text('3D renderer'),
                        content: SelectableText(
                          data.entries
                              .map((e) => '${e.key}: ${e.value}')
                              .join('\n'),
                        ),
                        actions: [
                          TextButton(
                            onPressed: () => Navigator.pop(context),
                            child: const Text('Close'),
                          ),
                        ],
                      ),
                    );
                  },
                  icon: const Icon(Icons.speed),
                ),
              ],
            ),
          ),
        ),
        Expanded(
          child: LayoutBuilder(
            builder: (context, constraints) {
              if (_size != constraints.biggest &&
                  !constraints.biggest.isEmpty) {
                _size = constraints.biggest;
                _sendCamera();
              }
              return Stack(
                fit: StackFit.expand,
                children: [
                  Focus(
                    focusNode: _focus,
                    onFocusChange: (focused) {
                      if (!focused) {
                        _held.clear();
                        _ticker.stop();
                      }
                    },
                    onKeyEvent: (_, event) {
                      final key = event.logicalKey;
                      if (event is KeyUpEvent) {
                        _held.remove(key);
                        if (_held.isEmpty) _ticker.stop();
                        return KeyEventResult.handled;
                      }
                      if (event is! KeyDownEvent && event is! KeyRepeatEvent)
                        return KeyEventResult.ignored;
                      final history = EditorWorkflows.heldHistoryShortcut(key);
                      if (history != null) {
                        (history == HistoryShortcut.redo
                                ? widget.onRedo
                                : widget.onUndo)
                            ?.call();
                        return KeyEventResult.handled;
                      }
                      if (key == LogicalKeyboardKey.keyF) {
                        frameSelection();
                        return KeyEventResult.handled;
                      }
                      if (key == LogicalKeyboardKey.home) {
                        _interrupt();
                        camera.fit(_center, _radius);
                        _sendCamera();
                        return KeyEventResult.handled;
                      }
                      const keys = [
                        LogicalKeyboardKey.keyW,
                        LogicalKeyboardKey.keyA,
                        LogicalKeyboardKey.keyS,
                        LogicalKeyboardKey.keyD,
                        LogicalKeyboardKey.keyQ,
                        LogicalKeyboardKey.keyE,
                        LogicalKeyboardKey.arrowUp,
                        LogicalKeyboardKey.arrowDown,
                        LogicalKeyboardKey.arrowLeft,
                        LogicalKeyboardKey.arrowRight,
                      ];
                      if (keys.contains(key)) {
                        _interrupt();
                        _held.add(key);
                        if (!_ticker.isActive) {
                          _lastTick = null;
                          _ticker.start();
                        }
                        return KeyEventResult.handled;
                      }
                      return KeyEventResult.ignored;
                    },
                    child: Listener(
                      onPointerDown: (e) {
                        _focus.requestFocus();
                        _interrupt();
                        _down.add(e.pointer);
                        _pressAt = e.localPosition;
                        // A second finger makes this a gesture, never a click.
                        _pressMoved = _down.length > 1;
                        _pivot = null;
                        final primary =
                            e.kind != PointerDeviceKind.mouse ||
                            e.buttons == kPrimaryMouseButton;
                        _pressPrimary = primary;
                        if (e.kind == PointerDeviceKind.mouse) {
                          _mouseDragging = true;
                          _pan =
                              e.buttons == kMiddleMouseButton ||
                              e.buttons == kSecondaryMouseButton ||
                              HardwareKeyboard.instance.isShiftPressed;
                        }
                        if (primary && !_pan && !camera.walking) {
                          _pivot =
                              ((pickAt(e.localPosition)?['hit']
                                          as Map?)?['pointM']
                                      as List?)
                                  ?.cast<num>()
                                  .map((v) => v.toDouble())
                                  .toList();
                        }
                      },
                      onPointerMove: (e) {
                        final press = _pressAt;
                        if (press != null &&
                            (e.localPosition - press).distance >
                                (e.kind == PointerDeviceKind.mouse ? 4 : 10))
                          _pressMoved = true;
                        if (!_mouseDragging ||
                            e.kind != PointerDeviceKind.mouse)
                          return;
                        if (_pan)
                          camera.pan(e.delta.dx, e.delta.dy);
                        else if (_pivot case final pivot?)
                          camera.orbitAround(pivot, e.delta.dx, e.delta.dy);
                        else
                          camera.orbit(e.delta.dx, e.delta.dy);
                        _sendCamera();
                      },
                      onPointerUp: (e) {
                        _down.remove(e.pointer);
                        // Shift-drag pans, but a Shift-click still selects.
                        final click =
                            _pressAt != null && !_pressMoved && _pressPrimary;
                        _mouseDragging = false;
                        _pressAt = null;
                        _pivot = null;
                        if (click) _click(e.localPosition, e.timeStamp);
                      },
                      onPointerCancel: (e) {
                        _down.remove(e.pointer);
                        _mouseDragging = false;
                        _pressAt = null;
                        _pivot = null;
                      },
                      onPointerSignal: (e) {
                        if (e is PointerScrollEvent) {
                          GestureBinding.instance.pointerSignalResolver
                              .register(e, (_) {
                                _interrupt();
                                _zoomAt(
                                  e.localPosition,
                                  math.exp(e.scrollDelta.dy * 0.002),
                                );
                              });
                        }
                      },
                      onPointerPanZoomStart: (_) {
                        _interrupt();
                        _lastScale = 1;
                      },
                      onPointerPanZoomUpdate: (e) {
                        camera.pan(e.panDelta.dx, e.panDelta.dy);
                        _zoomAt(e.localPosition, _lastScale / e.scale);
                        _lastScale = e.scale;
                      },
                      child: GestureDetector(
                        onScaleStart: (_) => _lastScale = 1,
                        onScaleUpdate: (e) {
                          if (_mouseDragging) return;
                          if (e.pointerCount >= 2) {
                            camera.pan(
                              e.focalPointDelta.dx,
                              e.focalPointDelta.dy,
                            );
                            camera.zoom(_lastScale / e.scale);
                          } else if (_pivot case final pivot?) {
                            camera.orbitAround(
                              pivot,
                              e.focalPointDelta.dx,
                              e.focalPointDelta.dy,
                            );
                          } else {
                            camera.orbit(
                              e.focalPointDelta.dx,
                              e.focalPointDelta.dy,
                            );
                          }
                          _lastScale = e.scale;
                          _sendCamera();
                        },
                        child: Stack(
                          fit: StackFit.expand,
                          children: [
                            ColoredBox(
                              color: const Color(0xFFD6E6F0),
                              child: textureId == null
                                  ? const SizedBox()
                                  : RepaintBoundary(
                                      key: _textureBoundary,
                                      child: _presenter.surface(textureId!),
                                    ),
                            ),
                            if (_loading)
                              const Positioned(
                                top: 8,
                                right: 8,
                                child: Card(
                                  child: Padding(
                                    padding: EdgeInsets.all(8),
                                    child: Text('Updating 3D scene…'),
                                  ),
                                ),
                              ),
                            Positioned(
                              left: 8,
                              bottom: 8,
                              right: 8,
                              child: IgnorePointer(
                                child: Text(
                                  camera.walking
                                      ? 'Drag to look · WASD to move · Q/E height · Shift faster'
                                      : 'Click to select · drag to orbit · right/Shift-drag to pan · scroll to zoom · F to frame',
                                  style: const TextStyle(fontSize: 12),
                                ),
                              ),
                            ),
                            if (_diagnostics.isNotEmpty)
                              Positioned(
                                left: 8,
                                top: 8,
                                child: Tooltip(
                                  message: _diagnostics.join('\n'),
                                  child: Text(
                                    '${_diagnostics.length} model warning(s)',
                                  ),
                                ),
                              ),
                          ],
                        ),
                      ),
                    ),
                  ),
                  // A sibling of the navigation handlers: selecting diagnostic
                  // text must not focus the camera or orbit/pan the scene.
                  if (_shownError case final error?)
                    Center(
                      child: SingleChildScrollView(
                        child: Card(
                          child: Padding(
                            padding: const EdgeInsets.all(16),
                            child: Column(
                              mainAxisSize: MainAxisSize.min,
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                SelectableText(error),
                                const SizedBox(height: 8),
                                TextButton.icon(
                                  onPressed: () async {
                                    try {
                                      await Clipboard.setData(
                                        ClipboardData(text: error),
                                      );
                                    } catch (_) {
                                      if (!context.mounted) return;
                                      ScaffoldMessenger.of(context)
                                          .showSnackBar(
                                            const SnackBar(
                                              content: Text(
                                                'Could not copy the 3D error. Select the text and copy it manually.',
                                              ),
                                            ),
                                          );
                                    }
                                  },
                                  icon: const Icon(Icons.copy_outlined),
                                  label: const Text('Copy 3D error'),
                                ),
                              ],
                            ),
                          ),
                        ),
                      ),
                    ),
                ],
              );
            },
          ),
        ),
      ],
    );
  }
}
