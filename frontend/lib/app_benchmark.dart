part of 'main.dart';

/// `DOMOWIK_BENCHMARK=<results.json>` runs the whole-application benchmark in
/// a release build and exits (scripts/benchmark.sh, `task benchmark`).
const String benchmarkVariable = 'DOMOWIK_BENCHMARK';

/// `name=path;name=path`: the plans to measure, in order.
const String benchmarkPlansVariable = 'DOMOWIK_BENCHMARK_PLANS';

/// A directory of furniture models placed into plans that have few.
const String benchmarkModelsVariable = 'DOMOWIK_BENCHMARK_MODELS';

/// Drives the real editor through scripted interactions and records frame,
/// latency, presenter and memory measurements. Timings depend on the host;
/// nothing here is a pass/fail gate. Native desktop only.
class _AppBenchmark {
  _AppBenchmark(this.page, this.output);

  final _PlanPageState page;
  final String output;
  final List<FrameSample> _frames = [];
  final List<Map<String, Object?>> _scenarios = [];
  final List<String> _problems = [];
  final _pointers = _SyntheticMouse();
  late Map<String, Object?> _workload;

  static const _steps = 120;

  /// Fewest 3D previews a working drag produces over [_steps] frames. A slow
  /// host coalesces moves into fewer previews, which the latency figures
  /// show; a drag that grabbed a wall or whose preview path broke produces
  /// none or one.
  static const _minPreviews = 10;

  Future<void> run() async {
    page._benchmarking = true;
    SchedulerBinding.instance.addTimingsCallback(_onTimings);
    final started = sceneClock.elapsedMicroseconds;
    var exitCode = 0;
    try {
      await _waitForCore();
      final plans = (hostEnvironment(benchmarkPlansVariable) ?? '')
          .split(';')
          .where((e) => e.contains('='))
          .map((e) => e.split('='))
          .toList();
      if (plans.isEmpty) throw StateError('$benchmarkPlansVariable is empty');
      final workloads = <Map<String, Object?>>[];
      for (final spec in plans) {
        final [name, path, ...flags] = spec;
        hostLog('benchmark: workload $name');
        _workload = {'name': name, 'path': path};
        workloads.add(_workload);
        await _measureWorkload(name, path, furnish: flags.contains('furnish'));
      }
      // Engine timing reports are batched; let the last batch arrive.
      for (var i = 0; i < 12; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 100));
        await _frame();
      }
      for (final s in _scenarios) {
        final window = s.remove('_window') as (int, int)?;
        if (window != null) {
          s['frames'] = summariseFrames(_frames, window.$1, window.$2);
        }
      }
      final (os, osVersion, cpus) = hostSystem();
      final (rss, maxRss) = hostMemory();
      await hostWritePrivate(
        output,
        const JsonEncoder.withIndent(' ').convert({
          'schema': 1,
          'kind': 'domowik-app-benchmark',
          'confidential': true,
          'host': {
            'os': os,
            'osVersion': osVersion,
            'logicalCpus': cpus,
            'dart': hostDartVersion(),
          },
          'durationSeconds': (sceneClock.elapsedMicroseconds - started) / 1e6,
          'workloads': workloads,
          'scenarios': _scenarios,
          'memory': {'rssBytes': rss, 'maxRssBytes': maxRss},
          'problems': _problems,
        }),
      );
      hostLog('benchmark: wrote $output');
      if (_problems.isNotEmpty) {
        hostLog('benchmark: ${_problems.length} problem(s) recorded');
      }
      hostLog('BENCHMARK COMPLETE');
    } catch (error, stack) {
      hostLogError('BENCHMARK FAILED: $error\n$stack');
      exitCode = 1;
    } finally {
      SchedulerBinding.instance.removeTimingsCallback(_onTimings);
    }
    hostExit(exitCode);
  }

  void _onTimings(List<FrameTiming> timings) =>
      _frames.addAll(timings.map(FrameSample.of));

  Future<void> _waitForCore() async {
    for (
      var i = 0;
      i < 400 && (page._bridge == null || page._handle == 0);
      i++
    ) {
      if (page._loadError != null) throw StateError(page._loadError!);
      await Future<void>.delayed(const Duration(milliseconds: 25));
    }
    if (page._bridge == null) throw StateError('the core did not load');
  }

  /// Pumps one frame and returns its raw vsync stamp on the engine clock.
  Future<int> _frame() {
    final stamp = Completer<int>();
    SchedulerBinding.instance.addPostFrameCallback(
      (_) => stamp.complete(
        SchedulerBinding.instance.currentSystemFrameTimeStamp.inMicroseconds,
      ),
    );
    SchedulerBinding.instance.scheduleFrame();
    return stamp.future;
  }

  ThreeDViewState get _view {
    final view = page._threeDKey.currentState;
    if (view == null) throw StateError('the 3D view is not mounted');
    return view;
  }

  /// Waits for a scene that started loading at or after [since] (sceneClock
  /// µs) to be INSTALLED with nothing left pending. Failed loads do not count;
  /// the view retries them. Null on timeout, which callers record as missing.
  Future<SceneLoadSample?> _loadAfter(
    int since, {
    int timeoutMs = 20000,
  }) async {
    final view = _view;
    final deadline = sceneClock.elapsedMicroseconds + timeoutMs * 1000;
    while (sceneClock.elapsedMicroseconds < deadline) {
      await _frame();
      final done = (view.loadSamples ?? const <SceneLoadSample>[])
          .where((s) => s.startMicros >= since && s.ok)
          .toList();
      if (done.isNotEmpty && !view.loading) return done.last;
    }
    return null;
  }

  /// Records an action -> 3D latency, or a miss when no scene was installed.
  void _to3d(_Latency latency, SceneLoadSample? load, int t0) => load == null
      ? latency.missing++
      : latency.samples.add((load.endMicros - t0) / 1000);

  /// Times a scenario: frames between its first and last vsync, the presenter
  /// and load counters across it, memory after it, and whatever [body] adds.
  Future<void> _scenario(
    String name,
    String description,
    Future<void> Function(Map<String, Object?> record) body,
  ) async {
    hostLog('benchmark:   $name');
    final view = _view;
    await _settle();
    view.loadSamples = [];
    final statsBefore = await view.stats();
    final requestsBefore = (view.sceneRequests, view.meshRequests);
    final record = <String, Object?>{
      'workload': _workload['name'],
      'name': name,
      'description': description,
    };
    final from = await _frame();
    final windowStart = sceneClock.elapsedMicroseconds;
    final wall = Stopwatch()..start();
    try {
      await body(record);
    } catch (error) {
      _problems.add('${_workload['name']}/$name: $error');
      record['error'] = '$error';
    }
    final to = await _frame();
    record['wallSeconds'] = wall.elapsedMicroseconds / 1e6;
    record['_window'] = (from, to);
    final statsAfter = await view.stats();
    record['presenter'] = {
      'before': statsBefore,
      'after': statsAfter,
      if (statsBefore['frames'] is int && statsAfter['frames'] is int)
        'frames':
            (statsAfter['frames'] as int) - (statsBefore['frames'] as int),
      if (statsBefore['meshUploads'] is int && statsAfter['meshUploads'] is int)
        'meshUploads':
            (statsAfter['meshUploads'] as int) -
            (statsBefore['meshUploads'] as int),
      if (statsBefore['cameraUpdates'] is int &&
          statsAfter['cameraUpdates'] is int)
        'cameraUpdates':
            (statsAfter['cameraUpdates'] as int) -
            (statsBefore['cameraUpdates'] as int),
    };
    final loads = [
      for (final s in view.loadSamples!)
        if (s.startMicros >= windowStart) s,
    ];
    view.loadSamples = null;
    final presenterFrames = (record['presenter'] as Map)['frames'];
    if (presenterFrames is int && wall.elapsedMicroseconds > 0) {
      record['presenterFps'] =
          presenterFrames / (wall.elapsedMicroseconds / 1e6);
    }
    record['sceneLoads'] = {
      'full': Summary.of([
        for (final s in loads)
          if (!s.poseOnly && s.ok) s.micros / 1000,
      ])?.toJson(),
      'transformOnly': Summary.of([
        for (final s in loads)
          if (s.poseOnly && s.ok) s.micros / 1000,
      ])?.toJson(),
      'failed': loads.where((s) => !s.ok).length,
      'sceneRequests': view.sceneRequests - requestsBefore.$1,
      'meshRequests': view.meshRequests - requestsBefore.$2,
    };
    final (rss, maxRss) = hostMemory();
    record['memory'] = {'rssBytes': rss, 'maxRssBytes': maxRss};
    _scenarios.add(record);
  }

  /// Lets in-flight scene work and timing batches finish before measuring.
  Future<void> _settle() async {
    final view = _view;
    for (var i = 0; i < 600; i++) {
      await _frame();
      if (!view.loading && i > 5) break;
    }
  }

  /// Writes a latency summary. Misses are interactions that never produced
  /// their result; they make the metric incomplete and are reported as
  /// problems rather than silently leaving the distribution.
  void _latencies(Map<String, Object?> record, String key, Object value) {
    final latency = value is _Latency
        ? value
        : (_Latency()..samples.addAll(value as List<double>));
    record[key] = {
      ...?Summary.of(latency.samples)?.toJson(),
      'missing': latency.missing,
      'raw': latency.samples,
    };
    if (latency.missing > 0) {
      _problems.add(
        '${_workload['name']}/${record['name']}: $key missing '
        '${latency.missing} of ${latency.missing + latency.samples.length}',
      );
    }
  }

  Future<void> _measureWorkload(
    String name,
    String path, {
    required bool furnish,
  }) async {
    final bytes = await hostReadBytes(path);
    page._benchSetState(() => page._viewMode = 'split');
    await _settle();

    // ---- Open: plan bytes to 2D refresh, then to an installed 3D scene.
    await _scenario(
      'open',
      'Open the plan in split view: core open + 2D refresh, then the first '
          'complete 3D scene.',
      (record) async {
        final view = _view;
        view.loadSamples = [];
        final t0 = sceneClock.elapsedMicroseconds;
        final opened = page._session!.open(PlanBytes(name, bytes));
        if (!opened.succeeded) throw StateError('could not open $path');
        page._resetForPlanReplacement();
        page._refresh();
        final refreshed = sceneClock.elapsedMicroseconds;
        await _frame();
        final shown = await _loadAfter(t0, timeoutMs: 60000);
        record['openToRefreshMs'] = (refreshed - t0) / 1000;
        record['openTo3dMs'] = shown == null
            ? null
            : (shown.endMicros - t0) / 1000;
        if (shown == null) throw StateError('no 3D scene after opening');
      },
    );

    // Measure the busiest floor: plans open on their first listed level,
    // which may be a near-empty crawlspace.
    final busiest = page._levels.reduce(
      (a, b) =>
          (page._wallsByLevel[b.id]?.length ?? 0) >
              (page._wallsByLevel[a.id]?.length ?? 0)
          ? b
          : a,
    );
    if (busiest.id != page._levelId) {
      final since = sceneClock.elapsedMicroseconds;
      _view.loadSamples = [];
      page._activateLevel(busiest.id);
      await _loadAfter(since, timeoutMs: 3000);
      _view.loadSamples = null;
    }
    _workload['activeLevel'] = busiest.name;
    final views = PlatformDispatcher.instance.views;
    _workload['layout'] = {
      'split': page._userConfig.splitOrientation.name,
      'planLogicalPx': [page._canvasSize.width, page._canvasSize.height],
      'threeDLogicalPx': [_view.viewportSize.width, _view.viewportSize.height],
      'devicePixelRatio': views.isEmpty ? null : views.first.devicePixelRatio,
    };
    if (furnish) await _ensureFurniture();
    final levels = page._levels.length;
    final walls = page._walls.length;
    final assets = page._activeAssets.length;
    _workload.addAll({
      'levels': levels,
      'activeLevelWalls': walls,
      'activeLevelFurniture': assets,
      'activeLevelOpenings': page._derived.openings.length,
    });
    final stats = await _view.stats();
    _workload['triangles'] = stats['triangles'];
    _workload['drawCalls'] = stats['drawCalls'];
    _workload['driver'] = stats['driver'];

    // ---- Orbit: one camera step per frame, as a mouse drag delivers.
    await _scenario(
      'orbit',
      'Orbit the 3D camera one step per frame for $_steps frames; navigation '
          'must not rebuild scenes or upload meshes.',
      (record) async {
        for (var i = 0; i < _steps * 2; i++) {
          _view.camera.orbit(3, i.isEven ? 0.4 : -0.4);
          _view.cameraChanged();
          await _frame();
        }
      },
    );

    // ---- Picking: what a 3D click or orbit press costs.
    await _scenario(
      'pick',
      'Cast 3D picks on a 20 × 10 grid across the 3D viewport.',
      (record) async {
        final size = _view.viewportSize;
        final ms = _Latency();
        var hits = 0;
        for (var y = 0; y < 10; y++) {
          for (var x = 0; x < 20; x++) {
            final at = Offset(
              size.width * (x + 0.5) / 20,
              size.height * (y + 0.5) / 10,
            );
            final watch = Stopwatch()..start();
            final answer = _view.pickAt(at);
            final elapsed = watch.elapsedMicroseconds / 1000;
            // Null means no current scene could answer: not a fast pick.
            if (answer == null) {
              ms.missing++;
            } else {
              ms.samples.add(elapsed);
              if (answer['hit'] != null) hits++;
            }
          }
          await _frame();
        }
        _latencies(record, 'pickMs', ms);
        record['hits'] = hits;
      },
    );

    // ---- Selection: tint changes go through transform-only updates.
    await _scenario(
      'selection',
      'Select 40 walls and models in turn; each change re-tints 3D through '
          'a transform-only update.',
      (record) async {
        final ids = [
          ...page._walls.take(20).map((w) => w.id),
          ...page._activeAssets.take(20).map((a) => a.id),
        ];
        final ms = _Latency();
        for (final id in ids) {
          final t0 = sceneClock.elapsedMicroseconds;
          page._clickSelect([id], Offset.zero);
          _to3d(ms, await _loadAfter(t0), t0);
        }
        page._benchSetState(page._selection.clear);
        _latencies(record, 'selectTo3dMs', ms);
      },
    );

    // ---- Furniture drag: live 3D preview, then commit on release.
    await _scenario(
      'drag-furniture',
      'Drag one model across the 2D plan for $_steps frames with the live 3D '
          'preview, then release to commit.',
      (record) async {
        final asset = _onCanvas(page._activeAssets).firstOrNull;
        if (asset == null) throw StateError('no model visible to drag');
        await _drag(record, asset.position, const Offset(1, 0.4), [
          asset.id,
        ], _DragMode.asset);
      },
    );

    // ---- Group drag: several models previewed together.
    await _scenario(
      'drag-group',
      'Select up to 20 models and drag them together for $_steps frames with '
          'the live 3D preview, then release to commit.',
      (record) async {
        final group = _onCanvas(page._activeAssets).take(20).toList();
        if (group.length < 2) throw StateError('fewer than two models visible');
        page._benchSetState(
          () =>
              page._selection.marquee(group.map((a) => a.id), additive: false),
        );
        await _settle();
        record['groupSize'] = group.length;
        // A press on the primary keeps the whole selection for the drag.
        final primary = group.firstWhere(
          (a) => a.id == page._selection.primary,
          orElse: () => group.last,
        );
        await _drag(record, primary.position, const Offset(-1, 0.3), [
          for (final a in group) a.id,
        ], _DragMode.group);
      },
    );

    // ---- Wall edits: each commit rebuilds the 3D scene.
    await _scenario(
      'wall-edit',
      'Commit 20 wall edits (alternating height changes and 100 mm moves); '
          'each rebuilds derived geometry and the complete 3D scene.',
      (record) async {
        final targets = page._walls.take(10).toList();
        if (targets.isEmpty) throw StateError('no walls on the active level');
        final commit = <double>[];
        final to3d = _Latency();
        for (var i = 0; i < 20; i++) {
          final wall = targets[i % targets.length];
          final t0 = sceneClock.elapsedMicroseconds;
          final code = page._gesture(
            i.isEven
                ? {'kind': 'setWall', 'wallId': wall.id, 'heightMm': 2600 + i}
                : {
                    'kind': 'translateSelection',
                    'entityIds': [wall.id],
                    'delta': [i % 4 == 1 ? 100 : -100, 0],
                  },
            'benchmark edit',
          );
          commit.add((sceneClock.elapsedMicroseconds - t0) / 1000);
          if (code != ok) {
            _problems.add('${_workload['name']}/wall-edit: edit $i refused');
            continue;
          }
          _to3d(to3d, await _loadAfter(t0), t0);
        }
        _latencies(record, 'commitAndRefreshMs', commit);
        _latencies(record, 'editTo3dMs', to3d);
      },
    );

    // ---- Undo and redo of those edits.
    await _scenario(
      'undo-redo',
      'Undo then redo 10 edits; each step re-projects the document and '
          'rebuilds the 3D scene.',
      (record) async {
        final step = <double>[];
        final to3d = _Latency();
        for (var i = 0; i < 20; i++) {
          final t0 = sceneClock.elapsedMicroseconds;
          page._changeHistory(redo: i >= 10);
          step.add((sceneClock.elapsedMicroseconds - t0) / 1000);
          _to3d(to3d, await _loadAfter(t0), t0);
        }
        _latencies(record, 'historyStepMs', step);
        _latencies(record, 'historyTo3dMs', to3d);
      },
    );

    // ---- Level switching.
    await _scenario(
      'switch-level',
      'Activate each level in turn, three rounds: 2D refresh and the 3D scene '
          'for the new visibility.',
      (record) async {
        final ids = page._levels.map((l) => l.id).toList();
        final activate = <double>[];
        final to3d = _Latency();
        for (var round = 0; round < 3; round++) {
          for (final id in ids) {
            if (id == page._levelId) continue;
            final t0 = sceneClock.elapsedMicroseconds;
            page._activateLevel(id);
            activate.add((sceneClock.elapsedMicroseconds - t0) / 1000);
            _to3d(to3d, await _loadAfter(t0), t0);
          }
        }
        _latencies(record, 'activateMs', activate);
        _latencies(record, 'activateTo3dMs', to3d);
      },
    );
  }

  /// Plans with fewer than 40 models on the active level get a grid of
  /// models from [benchmarkModelsVariable], so drags and picks have content.
  Future<void> _ensureFurniture() async {
    final directory = hostEnvironment(benchmarkModelsVariable);
    if (directory == null || page._activeAssets.length >= 40) return;
    final files = (hostEnvironment('DOMOWIK_BENCHMARK_MODEL_FILES') ?? '')
        .split(';')
        .where((f) => f.isNotEmpty)
        .toList();
    if (files.isEmpty) return;
    final bounds = PlanPainter.boundsOf(page._walls, page._derived);
    if (bounds == null) return;
    _view.loadSamples = [];
    final since = sceneClock.elapsedMicroseconds;
    final watch = Stopwatch()..start();
    var placed = 0;
    const target = 200;
    final columns = 20;
    for (var i = 0; i < target; i++) {
      final at = Offset(
        bounds.left + bounds.width * ((i % columns) + 0.5) / columns,
        bounds.top +
            bounds.height * ((i ~/ columns) + 0.5) / (target / columns),
      );
      final result = page._assetRequest({
        'kind': 'place',
        'sourceDirectory': directory,
        'file': '$directory/${files[i % files.length]}',
        'levelId': page._levelId,
        'positionMm': [at.dx, at.dy],
        'dimensionsMm': [600, 600],
      });
      if (result != null) placed++;
    }
    page._refresh();
    _workload['furniturePlaced'] = placed;
    _workload['furniturePlacementSeconds'] = watch.elapsedMicroseconds / 1e6;
    await _loadAfter(since, timeoutMs: 60000);
    _view.loadSamples = null;
  }

  /// Models whose centre is on the 2D canvas and clear of every wall axis by
  /// 400 mm and by 30 screen pixels (wall and endpoint hits use a pixel
  /// radius, so a zoomed-out plan needs more plan distance), nearest the
  /// canvas centre first. A press there grabs the model, not a wall.
  List<AssetFootprint> _onCanvas(List<AssetFootprint> assets) {
    final size = page._canvasSize;
    final centre = size.center(Offset.zero);
    final clearance = math.max(400.0, 30 / page._view.zoom);
    double toWall(Offset p, Wall w) {
      final d = w.end - w.start;
      final length2 = d.dx * d.dx + d.dy * d.dy;
      final t = length2 == 0
          ? 0.0
          : (((p - w.start).dx * d.dx + (p - w.start).dy * d.dy) / length2)
                .clamp(0.0, 1.0);
      return (p - (w.start + d * t)).distance;
    }

    final visible = [
      for (final a in assets)
        if ((Offset.zero & size)
                .deflate(40)
                .contains(page._view.toScreen(a.position)) &&
            page._walls.every((w) => toWall(a.position, w) > clearance))
          a,
    ];
    visible.sort(
      (a, b) => (page._view.toScreen(a.position) - centre).distance.compareTo(
        (page._view.toScreen(b.position) - centre).distance,
      ),
    );
    return visible;
  }

  /// A mouse drag on the 2D canvas from plan point [from], moving 2 px per
  /// frame along [direction] for [_steps] frames, then release.
  ///
  /// Records, for the transform-only preview loads: their duration, and the
  /// input -> 3D latency from the oldest pointer move each load absorbed to
  /// its installed scene. On release it checks the drag really was [mode],
  /// that every model in [ids] moved and the document advanced, and times
  /// release -> the committed scene. Anything else is reported as a problem.
  Future<void> _drag(
    Map<String, Object?> record,
    Offset from,
    Offset direction,
    List<String> ids,
    _DragMode mode,
  ) async {
    final box = page._paintKey.currentContext!.findRenderObject()! as RenderBox;
    final start = box.localToGlobal(page._view.toScreen(from));
    final unit = direction / direction.distance;
    final view = _view;
    Map<String, Offset> positions() => {
      for (final a in page._assetInstances)
        if (ids.contains(a.id)) a.id: a.position,
    };
    final before = positions();
    final revision = page._documentRevision;
    // Only moves made once the editor classified the drag can be previewed,
    // and only loads after its first translation preview are previews (the
    // press itself may re-tint a newly selected model).
    // Each classified move is carried by the first translation preview sent
    // after it; record that preview's pose revision. A move's input -> 3D
    // latency runs to the first installed scene whose revision includes it,
    // so a load that started later but still showed an older preview is
    // never credited with the move.
    final moves = <(int, int)>[];
    await _pointers.press(start);
    var at = start;
    for (var i = 0; i < _steps; i++) {
      at += unit * 2;
      final sent = sceneClock.elapsedMicroseconds;
      final revision = page._posePreviewRevision;
      _pointers.move(at);
      final classified = page._dragClassified;
      await _frame();
      if (classified && page._posePreviewRevision > revision) {
        moves.add((sent, page._posePreviewRevision));
      }
      if (i == 10) record['dragMode'] = page._mode.name;
    }
    // The last move's preview may still be in flight: wait for it.
    if (moves.isNotEmpty) await _loadAfter(moves.last.$1);
    final firstRevision = moves.isEmpty ? null : moves.first.$2;
    final previews = [
      for (final s in view.loadSamples!)
        if (firstRevision != null &&
            s.poseOnly &&
            s.ok &&
            s.poseRevision >= firstRevision)
          s,
    ]..sort((a, b) => a.startMicros.compareTo(b.startMicros));
    final inputTo3d = _Latency();
    for (final ms in inputLatencies(moves, [
      for (final s in previews) (s.startMicros, s.endMicros, s.poseRevision),
    ])) {
      if (ms == null) {
        inputTo3d.missing++;
      } else {
        inputTo3d.samples.add(ms);
      }
    }
    final t0 = sceneClock.elapsedMicroseconds;
    _pointers.release(at);
    final release = _Latency();
    _to3d(release, await _loadAfter(t0), t0);
    final after = positions();
    final moved = ids.every(
      (id) =>
          before[id] != null &&
          after[id] != null &&
          (after[id]! - before[id]!).distance > 1,
    );
    final committed = moved && page._documentRevision != revision;
    record['previewLoads'] = previews.length;
    _latencies(record, 'previewLoadMs', [
      for (final s in previews) s.micros / 1000,
    ]);
    _latencies(record, 'dragInputTo3dMs', inputTo3d);
    record['releaseTo3dMs'] = release.samples.firstOrNull;
    record['committed'] = committed;
    record['statusAfterRelease'] = page._status;
    final where = '${_workload['name']}/${record['name']}';
    if (record['dragMode'] != mode.name) {
      _problems.add(
        '$where: expected a ${mode.name} drag, got ${record['dragMode']} '
        '(status "${page._status}")',
      );
    }
    if (previews.length < _minPreviews) {
      _problems.add('$where: ${previews.length} 3D previews in $_steps frames');
    }
    if (!committed) {
      _problems.add(
        '$where: release did not commit the move (status "${page._status}")',
      );
    }
    if (release.missing > 0) {
      _problems.add('$where: no 3D scene after release');
    }
  }
}

/// One latency metric's samples, plus interactions that never produced a
/// result within their timeout.
class _Latency {
  final List<double> samples = [];
  int missing = 0;
}

/// Mouse input injected through the gesture binding, exactly as the
/// platform delivers it, so drags exercise the editor's real handlers.
class _SyntheticMouse {
  static const _device = 9137;
  int _pointer = 9137;
  bool _added = false;

  Duration get _now => Duration(microseconds: sceneClock.elapsedMicroseconds);

  void _dispatch(PointerEvent event) =>
      GestureBinding.instance.handlePointerEvent(event);

  Future<void> press(Offset at) async {
    if (!_added) {
      _dispatch(
        PointerAddedEvent(
          timeStamp: _now,
          kind: PointerDeviceKind.mouse,
          device: _device,
          position: at,
        ),
      );
      _added = true;
    }
    _dispatch(
      PointerHoverEvent(
        timeStamp: _now,
        kind: PointerDeviceKind.mouse,
        device: _device,
        position: at,
      ),
    );
    _pointer++;
    _dispatch(
      PointerDownEvent(
        timeStamp: _now,
        pointer: _pointer,
        kind: PointerDeviceKind.mouse,
        device: _device,
        position: at,
        buttons: kPrimaryMouseButton,
      ),
    );
    _last = at;
  }

  Offset _last = Offset.zero;

  void move(Offset at) {
    _dispatch(
      PointerMoveEvent(
        timeStamp: _now,
        pointer: _pointer,
        kind: PointerDeviceKind.mouse,
        device: _device,
        position: at,
        delta: at - _last,
        buttons: kPrimaryMouseButton,
      ),
    );
    _last = at;
  }

  void release(Offset at) {
    _dispatch(
      PointerUpEvent(
        timeStamp: _now,
        pointer: _pointer,
        kind: PointerDeviceKind.mouse,
        device: _device,
        position: at,
      ),
    );
    _last = at;
  }
}
