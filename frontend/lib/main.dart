// Domowik editor, using the shared Rust core through the native C ABI.
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

import 'dart:async';
import 'dart:convert';
import 'dart:ui' as ui;
import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui';

import 'package:flutter/foundation.dart' show kIsWeb;
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/services.dart';

import 'core/domowik_core.dart';
import 'core/platform_core.dart';
import 'platform/entry.dart';
import 'platform/file_dialogs.dart';
import 'platform/host.dart';
import 'platform/model_import.dart';
import 'platform/catalog_host.dart';
import 'platform/account_host.dart';
import 'account.dart';
import 'account_button.dart';
import 'hosted_library.dart';
import 'hosted_workflow.dart';
import 'property_planner.dart';
import 'catalog.dart';
import 'editor_fonts.dart';
import 'editor_shell.dart';
import 'editor_workflows.dart';
import 'file_workflows.dart';
import 'plan_session.dart';
import 'plan_geometry_cache.dart';
import 'plan_viewport.dart';
import 'selection_state.dart';
import 'inspector_metric.dart';
import 'level_state.dart';
import 'level_properties_form.dart';
import 'opening_properties_form.dart';
import 'wall_properties_form.dart';
import 'solid_colors_dialog.dart';
import 'asset_library.dart';
import 'asset_rotation.dart';
import 'user_config.dart';
import 'preferences_dialog.dart';
import 'three_d_view.dart';
import 'web_graphics_policy.dart';
import 'warnings_banner.dart';
import 'plan_painter.dart';
import 'wall_chain_state.dart';
import 'benchmark_stats.dart';
import 'staircase_properties_form.dart';
import 'stair_drawing.dart';
import 'stair_resize.dart';
import 'live_property_edits.dart';

// Preserve the editor's existing public imports while keeping scene payloads
// and painting separate from the editor's input/file lifecycle.
export 'plan_painter.dart';

part 'app_benchmark.dart';

/// Set to `1` in the environment to drive the real UI for one interaction,
/// report what the painter actually received, and exit.
///
/// The headless self-test proves the bundle can CALL the core. It does not
/// prove the widget tree renders what the core returned, and "the author
/// looked at a window" is not evidence that survives review. This probe runs
/// the same widgets, the same button handler and the same painter as a person
/// clicking, and reports from inside `paint`.
const String firstFrameVariable = 'DOMOWIK_SPIKE_FIRSTFRAME';

/// Set to `1` as a page URL query parameter to run the browser build's
/// release probe (see `_runWebProbe`).
const String webProbeVariable = 'DOMOWIK_WEB_PROBE';

bool get _firstFrameProbeRequested =>
    hostEnvironment(firstFrameVariable) == '1' || firstFrameProbeArgument;

/// The ABI revision this build was written against.
///
/// A mismatch is fatal on purpose. The header says a caller reading a
/// different number must not proceed, and an app that proceeded anyway would
/// be interpreting one struct layout as another.
const int expectedAbi = 25;

void main(List<String> arguments) {
  // Native builds may be launched as a headless probe; see platform/entry.dart.
  if (runProbeEntry(arguments)) return;
  if (kIsWeb) {
    // Secondary clicks open the editor's own menus (asset actions), which the
    // browser's page menu would otherwise cover (issue 0088). The call goes
    // through a platform channel, so the binding must exist first.
    WidgetsFlutterBinding.ensureInitialized();
    BrowserContextMenu.disableContextMenu();
  }
  runApp(const DomowikApp());
}

class DomowikApp extends StatelessWidget {
  const DomowikApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Domowik',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        fontFamily: editorFontFamily,
        fontFamilyFallback: editorFontFallback,
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
enum _DragMode {
  staircase,
  stairResize,
  none,
  draw,
  endpoint,
  body,
  opening,
  openingResize,
  openingSwing,
  asset,
  assetRotate,
  poseAccept,
  pan,
  place,
  selectOnly,
  marquee,
  group,
}

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
  const PlanPage({
    super.key,
    this.bridgeLoader,
    this.configStore,
    this.fileDialogs,
    this.modelPicker,
    this.modelDirectoryPicker,
    this.modelUploads,
    this.catalogClient,
    this.accountClient,
    this.graphicsPolicy,
  });

  /// Tests load the real native library directly; bundles use
  /// [loadPlatformCore].
  final DomowikCore Function()? bridgeLoader;

  /// Tests can inject a temporary preference file. Native-loader tests default
  /// to in-memory preferences so they never read/write the real user's config.
  final UserConfigStore? configStore;

  /// Native dialogs by default; tests may inject paths and close approval.
  final FileDialogs? fileDialogs;

  /// Picks a model file to upload; the browser's file chooser by default.
  final Future<(String, Uint8List)?> Function()? modelPicker;

  /// Picks a directory of model files; platforms may provide their native or
  /// browser directory chooser.
  final Future<PickedModelDirectory?> Function()? modelDirectoryPicker;

  /// Whether the asset library offers uploads instead of directories; the
  /// host decides by default (browsers have no directories, issue 0088).
  final bool? modelUploads;

  /// Browser hosts use the first-party catalog by default. Custom core hosts
  /// (including tests) opt in explicitly so they never contact a default server.
  /// Native hosts do not enable hosted discovery. The page owns and closes a
  /// supplied client; it must not be shared with another page.
  final CatalogClient? catalogClient;

  /// Optional account transport, primarily for lifecycle tests. The hosted
  /// UI appears only when this client also supports browser plan operations.
  final AccountClient? accountClient;

  /// Browser safety defaults to the host policy; tests may inject a policy.
  final WebGraphicsPolicy? graphicsPolicy;

  @override
  State<PlanPage> createState() => _PlanPageState();
}

class _PlanPageState extends State<PlanPage>
    with WidgetsBindingObserver, SingleTickerProviderStateMixin {
  late final TabController _inspectorTabs;
  final ScrollController _selectionScroll = ScrollController();
  final LivePropertyEdits _livePropertyEdits = LivePropertyEdits();
  DomowikCore? _bridge;
  late final CatalogClient? _catalogClient =
      widget.catalogClient ??
      (widget.bridgeLoader == null ? createCatalogClient() : null);

  /// Signing in to a server, when `DOMOWIK_SERVER_URL` configures one;
  /// otherwise the editor is anonymous and local.
  late final AccountClient? _accountClient =
      widget.accountClient ?? createAccountClient();
  AccountSession? _accountSession;
  final ValueNotifier<int> _propertyContextChanges = ValueNotifier(0);
  final ValueNotifier<int> _accountSessionChanges = ValueNotifier(0);
  int? _propertyObservedDocumentRevision;
  String? _propertyObservedPlanId;
  String? _propertyObservedLevelId;
  bool? _propertyObservedDirty;
  HostedPlan? _hostedPlan;
  HostedWorkflowCoordinator? _hostedWorkflow;
  String? _hostedLibraryActionError;
  bool _hostedSaveBlocked = false;
  bool _hostedSaveInFlight = false;
  String? _loadError;
  int _abi = 0;
  PlanSession? _session;
  int get _handle => _session?.handle ?? 0;
  final LevelViewState _levelView = LevelViewState();
  String? get _levelId => _levelView.activeId;
  bool _resetLevelView = true;
  String? get _filePath => _session?.path;
  bool _dirty = false;
  int _undoDepth = 0;
  int _redoDepth = 0;
  bool get _fileActionPending => _fileWorkflow.busy;
  late final FileDialogs _fileDialogs =
      widget.fileDialogs ?? defaultFileDialogs();
  late final FileWorkflowController _fileWorkflow = FileWorkflowController(
    isDirty: () => _dirty,
    askToLeave: _askToLeave,
    save: _saveFile,
    saveCopy: _saveLocalRecoveryCopy,
    beforeAction: () {
      _livePropertyEdits.flush();
      if (!_acceptAssetPose()) {
        throw StateError('Fix invalid placement values before continuing.');
      }
      _cancelDrag();
    },
    onBusyChanged: (_) {
      if (mounted) setState(() {});
    },
    onError: (error) {
      if (mounted) {
        final message = _friendlyWorkflowError(error);
        setState(() {
          _hostedLibraryActionError = message;
          _setStatus('File action failed: $message', isError: true);
        });
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text('File action failed: $message')));
      }
    },
  );

  List<LevelInfo> get _levels => _levelView.levels;
  Map<String, List<Wall>> _wallsByLevel = const {};
  Map<String, List<(Wall, bool)>> _allJoins = const {};
  final PlanGeometryCache _geometryCache = PlanGeometryCache();
  Map<String, String> _currentGeometryKeys = const {};
  List<PlanLayer> _backgroundLevels = const [];
  Offset? _stairStart;
  List<PlanConstruction> _stairPreview = const [];
  String? _resizingStairId, _stairResizeEdge;
  Offset? _stairHoverScreen;
  bool _stairPreviewReevalScheduled = false;
  String? _expandedStaircaseId;

  List<Wall> _walls = const [];
  List<AssetFootprint> _assetInstances = const [];

  /// How many captured instances the plan-session clipboard holds (issue
  /// 0113), mirrored from the native `list` response. Never persisted, never
  /// part of undo: plan replacement resets it with the native session.
  int _clipboardCount = 0;

  /// The snapshot's `documentId` (issue 0116): the stable key the device's
  /// remembered level hint (issue 0117) is stored under. Parsed on every
  /// snapshot read, exactly as the level list is — no document command can
  /// change it mid-session, so there is no second parse path. It never
  /// reaches the document being edited from here: it is a key for config
  /// storage only.
  String? _documentId;

  String? get _planPreferencesKey => _session?.levelPreferencesKey(
    _documentId,
    _levels.map((level) => level.id),
  );

  List<String> _assetDirectories = const [];
  late final UserConfigStore _userConfig =
      widget.configStore ??
      (widget.bridgeLoader == null
          ? UserConfigStore.standard()
          : UserConfigStore.inMemory());
  late final Future<void> _configReady;
  List<String> get _libraryDirectories =>
      {..._assetDirectories, ..._userConfig.assetDirectories}.toList();
  List<LibraryModel> _embeddedModels = const [];
  AssetChoice? _assetChoice;
  String? _poseAssetId;
  List<double>? _poseInitial;
  List<double>? _poseDraft;
  bool _poseDraftValid = true;
  List<double>? _poseDragInitial;
  AssetRotationDrag? _assetRotationDrag;
  double? _poseHeight;
  int _posePreviewRevision = 0;
  List<double>? _pendingPose;
  bool _poseFrameScheduled = false;
  // Furniture translation overlay shown in 3D while a 2D drag is in flight.
  bool _dragPreviewActive = false, _dragPreviewScheduled = false;
  Map<String, ui.Image> _modelPreviews = const {};
  Map<String, String> _modelPreviewErrors = const {};
  final Set<(int, String)> _previewPending = {};
  bool _previewWorkerRunning = false;
  List<AssetFootprint> get _activeAssets =>
      _assetInstances.where((a) => a.levelId == _levelId).toList();
  Map<String, List<(Wall, bool)>> _joins = const {};
  Derived _derived = Derived.empty;
  final SelectionController _selection = SelectionController();
  String? get _selected => _selection.primary;
  int _documentRevision = 0;

  /// The core's embedded-content revision: an uploaded model can supply bytes
  /// a placed instance lacked, changing its footprint and 3D mesh without a
  /// document change (issue 0088).
  int _assetRevision = 0;
  int _modifierMask = 0;
  final GlobalKey _paintKey = GlobalKey();
  final GlobalKey<ThreeDViewState> _threeDKey = GlobalKey<ThreeDViewState>();
  String _viewMode = '2d';
  late final WebGraphicsPolicy _hostGraphicsPolicy =
      widget.graphicsPolicy ?? hostGraphicsPolicy();
  String? _web3dFailure;
  WebGraphicsPolicy get _graphicsPolicy => _web3dFailure == null
      ? _hostGraphicsPolicy
      : const WebGraphicsPolicy.safe2d(
          'Safe 2D: 3D could not start with your browser\'s current graphics setup. '
          'You can continue editing and saving your plan in 2D. '
          'Your saved view preferences have not been changed. '
          'Reload the page to retry 3D after changing your browser setup.',
        );
  String get _effectiveViewMode => _graphicsPolicy.viewMode(_viewMode);
  Offset? _dragFrom;
  Offset? _dragTo;
  Offset? _pointerPosition;
  Offset? _pointerDownPosition;

  /// Latest canvas point in plan millimetres, the paste anchor (issue 0113).
  /// Null until the pointer has been over this canvas.
  Offset? _hoverPlanMm;
  bool _dragClassified = false;
  PointerDeviceKind _pointerKind = PointerDeviceKind.mouse;
  SelectionSnapshot? _selectionBeforeDrag;
  final Map<int, Offset> _touches = {};
  final Set<int> _releasedGestureTouches = {};
  PlanViewport? _navigationStart;
  int? _trackpadPointer;
  double _trackpadScale = 1;
  Offset? _navigationCentroid;

  /// What the current drag is doing, decided from the active tool and hit.
  _DragMode _mode = _DragMode.none;
  String? _dragWall;
  String? _dragOpening;
  String? _openingTargetHost;
  bool _openingResizeStart = false;
  bool _dragIsStart = false;

  PlanViewport _view = PlanViewport(zoom: 0.1, origin: Offset.zero);
  Size _canvasSize = Size.zero;
  Rect? _contentBounds;
  bool _fitRequested = true;
  String _laidOutViewMode = '2d';
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
  String _doorMechanism = 'doorHinged';
  String _windowMechanism = 'windowSideHung';

  /// Whether the creation-only Walls chain tool (`wall-chain-tool`) is armed.
  bool _wallChainTool = false;
  MouseCursor _hoverCursor = SystemMouseCursors.basic;

  /// Whether the pointer is over a wall that can host the opening being placed.
  bool _hoverOnOpeningHost = false;
  Offset? _openingHoverScreen;
  bool _openingHoverReevalScheduled = false;
  final WallChainState _chain = WallChainState();
  Offset? _chainHoverAt;
  Offset? _chainHoverScreen;
  bool _chainHoverReevalScheduled = false;
  Duration? _lastChainClickTime;
  Offset? _lastChainClickScreen;
  String _status = '';
  bool _statusIsError = false;

  /// Validation warnings on the CURRENT document: non-blocking, so surfaced
  /// separately from `_status`/`_statusIsError`, which report the last
  /// accept/refuse action. Read from `planState`'s `warnings` array on every
  /// refresh and formatted against the current snapshot so entity references
  /// are useful to a person, not just stable to the core.
  List<String> _warnings = const [];
  List<Set<String>> _warningEntities = const [];
  Set<String> _highlightedWarningIds = const {};
  int _wallOverlapCount = 0;
  int _openPlans = 0;
  int _outstandingBuffers = 0;
  int _panicsSurvived = 0;

  int _idSeq = 0;

  @override
  void initState() {
    super.initState();
    _inspectorTabs = TabController(length: 2, vsync: this)
      ..addListener(_inspectorTabChanged);
    _configReady = _userConfig
        .load()
        .then((_) {
          if (mounted) setState(() {});
        })
        .catchError((Object error) {
          if (mounted)
            setState(
              () => _setStatus(
                'Could not load user configuration: $error',
                isError: true,
              ),
            );
        });
    WidgetsBinding.instance.addObserver(this);
    _modifierMask = _readModifierMask();
    HardwareKeyboard.instance.addHandler(_observeSelectionKeys);
    GestureBinding.instance.pointerRouter.addGlobalRoute(
      _observeSelectionHover,
    );
    _fileDialogs.setCloseHandler(
      () => _fileWorkflow.run(() async {
        if (!await _fileWorkflow.confirmLeaving()) return false;
        await _userConfig.flush();
        return true;
      }),
    );
    _open();
    if (hostEnvironment('DOMOWIK_3D_PROBE') == '1') {
      WidgetsBinding.instance.addPostFrameCallback((_) => _runThreeDProbe());
    }
    if (_firstFrameProbeRequested) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _runFirstFrameProbe(),
      );
    }
    if (hostEnvironment(webProbeVariable) == '1') {
      WidgetsBinding.instance.addPostFrameCallback((_) => _runWebProbe());
    }
    if (hostEnvironment(benchmarkVariable) case final output?
        when hostHasFilesystem) {
      WidgetsBinding.instance.addPostFrameCallback(
        (_) => _AppBenchmark(this, output).run(),
      );
    }
  }

  /// Set while the benchmark drives the editor; isolates user preferences.
  bool _benchmarking = false;

  /// [setState] for the benchmark runner, which lives outside this class.
  void _benchSetState(VoidCallback change) => setState(change);

  /// The browser build's release probe (issue 0088, frontend/scripts/
  /// web-probe.sh): the shipped page, its WebAssembly core and renderer, one
  /// pass through the editing, history, file, asset and 3D workflows, and a
  /// verdict on the console. Runs only when the page URL asks for it.
  Future<void> _runWebProbe() async {
    final failures = <String>[];
    void check(bool ok, String what) {
      hostLog('web probe: ${ok ? 'ok' : 'FAILED'} $what');
      if (!ok) failures.add(what);
    }

    Future<void> frames([int count = 3]) async {
      for (var i = 0; i < count; i++) {
        WidgetsBinding.instance.scheduleFrame();
        await WidgetsBinding.instance.endOfFrame;
      }
    }

    try {
      for (var i = 0; i < 400 && (_bridge == null || _handle == 0); i++) {
        if (_loadError != null) throw StateError(_loadError!);
        await Future<void>.delayed(const Duration(milliseconds: 25));
      }
      final core = _bridge!;
      check(core.abiVersion() == expectedAbi, 'core ABI $expectedAbi');
      check(!core.supportsPaths, 'the browser core reaches no filesystem');

      for (var i = 0; i < 4; i++) {
        _addWall();
      }
      await frames();
      check(_walls.length == 4, 'four walls drawn');
      check(_derived.rooms.length == 1, 'the core derives one room');
      check(PlanPainter.wallsPainted == 4, 'the painter drew four walls');
      check(await _rasterisedPixels() > 0, 'the plan rasterises');

      _changeHistory();
      check(_walls.length == 3, 'undo removes a wall');
      _changeHistory(redo: true);
      check(_walls.length == 4, 'redo restores it');

      Uint8List? saved;
      final session = _session!;
      final save = await session.save(
        PlanByteTarget('probe.domo', (bytes) async {
          saved = bytes;
          return true;
        }),
      );
      _refresh();
      check(save.succeeded && saved != null, 'the plan encodes for saving');
      check(!_dirty, 'a confirmed save leaves the plan clean');
      check(
        session.open(PlanBytes('probe.domo', saved!)).succeeded,
        'the saved bytes reopen',
      );
      _resetForPlanReplacement();
      _refresh();
      await frames();
      check(
        _walls.length == 4 && _derived.rooms.length == 1,
        'the reopened plan has its walls and room',
      );

      const cube =
          'v -1 0 -1\nv 1 0 -1\nv 1 1 -1\nv -1 1 -1\nv -1 0 1\nv 1 0 1\n'
          'v 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 5 8 4\nf 2 3 7 6\n'
          'f 4 8 7 3\nf 1 2 6 5\n';
      final imported = core.importModel(
        _handle,
        'cube.obj',
        Uint8List.fromList(utf8.encode(cube)),
      );
      check(imported.succeeded, 'an uploaded model embeds');
      final digest = (jsonDecode(imported.text ?? '{}') as Map)['digest'];
      final placed = _assetMutation({
        'kind': 'place',
        'digest': digest,
        'levelId': _levelId,
        'positionMm': [2500, 2000],
      }, 'Probe model');
      check(placed != null && _assetInstances.length == 1, 'the model places');

      // The characters from U+00A0 up the bundled fonts must supply, drawn on
      // every platform: by the painter as a room label, and by a widget as
      // the status line. ⌘ otherwise appears only in macOS hints, so a Linux
      // run would never prove its fallback. A font the engine cannot load
      // from the bundle fails scripts/web-probe.sh (ADR 0012 §6).
      const glyphs = 'Łódź ⌘ 12 m² — ±5°';
      final seed = _derived.rooms.single.seedCandidateMm!;
      _gesture({
        'kind': 'addRoom',
        'levelId': _levelId,
        'seed': [seed.dx, seed.dy],
        'name': glyphs,
      }, 'Probe room name');
      setState(() => _setStatus(glyphs, isError: false));
      await frames();
      check(
        _derived.rooms.single.label == glyphs,
        "the bundled fonts' glyphs reach the painter and the status line",
      );

      setState(() => _viewMode = 'split');
      Map<String, Object?>? stats;
      String? why;
      for (var i = 0; i < 400; i++) {
        await Future<void>.delayed(const Duration(milliseconds: 25));
        final view = _threeDKey.currentState;
        why = view?.diagnosticError;
        if (view?.session == null) continue;
        stats = await view!.stats();
        if ((stats['triangles'] as int? ?? 0) > 0 &&
            (stats['frames'] as int? ?? 0) > 0) {
          break;
        }
      }
      final rendered = (stats?['triangles'] as int? ?? 0) > 0;
      check(rendered, '3D draws the plan on WebGPU (${why ?? 'no error'})');
      if (stats != null) hostLog('web probe 3D: ${jsonEncode(stats)}');
      check(core.failure == null, 'the core is still healthy');
      check(core.outstandingBuffers() == 0, 'no core buffer is outstanding');
    } catch (error, stack) {
      failures.add('$error');
      hostLogError('web probe error: $error\n$stack');
    }
    hostLog(
      failures.isEmpty
          ? 'WEB PROBE PASSED'
          : 'WEB PROBE FAILED: ${failures.join('; ')}',
    );
  }

  Future<void> _runThreeDProbe() async {
    try {
      // Core loading and the remembered-floor config are asynchronous. Like
      // the web probe, wait for _open to create its plan, not merely a frame.
      for (var i = 0; i < 400 && (_bridge == null || _handle == 0); i++) {
        if (_loadError != null) throw StateError(_loadError!);
        await Future<void>.delayed(const Duration(milliseconds: 25));
      }
      if (_bridge == null || _handle == 0) {
        throw StateError(_loadError ?? 'Timed out waiting for the probe plan');
      }
      for (var i = 0; i < 4; i++) {
        _addWall();
      }
      final (directory, file, cleanup) = await hostProbeModel(
        hostEnvironment('DOMOWIK_3D_PROBE_MODEL'),
        'v -1 0 -1\nv 1 0 -1\nv 1 1 -1\nv -1 1 -1\nv -1 0 1\nv 1 0 1\nv 1 1 1\nv -1 1 1\nf 1 4 3 2\nf 5 6 7 8\nf 1 5 8 4\nf 2 3 7 6\nf 4 8 7 3\nf 1 2 6 5\n',
      );
      try {
        for (var i = 0; i < 25; i++) {
          final result = _assetMutation({
            'kind': 'place',
            'sourceDirectory': directory,
            'file': file,
            'levelId': _levelId,
            'positionMm': [650 + (i % 5) * 750, 500 + (i ~/ 5) * 650],
            'dimensionsMm': [400, 400],
          }, 'Probe model');
          if (result == null)
            throw StateError('Could not place original probe model');
        }
        setState(() => _viewMode = 'split');
        Map<String, Object?>? before;
        for (var i = 0; i < 500; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 20));
          final view = _threeDKey.currentState;
          if (view?.session != null) {
            final stats = await view!.stats();
            if ((stats['triangles'] as int) > 0 &&
                (stats['frames'] as int) >= 1) {
              before = stats;
              break;
            }
          }
        }
        if (before == null) {
          final view = _threeDKey.currentState;
          hostLog(
            '3D diagnostic: session=${view?.session}, scene requests=${view?.sceneRequests}, mesh requests=${view?.meshRequests}, error=${view?.diagnosticError}',
          );
          if (view?.session != null)
            hostLog('3D native diagnostic: ${jsonEncode(await view!.stats())}');
          throw StateError('Native 3D surface did not draw geometry');
        }
        final view = _threeDKey.currentState!;
        await view.diagnosticCapture();
        for (var i = 0; i < 100; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 20));
          if (((await view.stats())['diagnosticPixels'] as int) > 1000) break;
        }
        final baseline = await view.stats();
        final sceneRequests = view.sceneRequests,
            meshRequests = view.meshRequests;
        final revision = _documentRevision;
        for (var i = 0; i < 120; i++) {
          view.camera.orbit(2, 0);
          view.cameraChanged();
          await Future<void>.delayed(const Duration(milliseconds: 20));
        }
        // Camera packets deliberately coalesce per frame. On a loaded software
        // renderer 120 gestures need not yield 21 packets in 2.4 seconds. This
        // is a cache-correctness probe, not a host-independent frame-rate gate:
        // keep the same packet-count assertion, with bounded additional input.
        for (var i = 0; i < 100; i++) {
          final progress = await view.stats();
          if ((progress['cameraUpdates'] as int) >
              (baseline['cameraUpdates'] as int) + 20)
            break;
          view.camera.orbit(2, 0);
          view.cameraChanged();
          await Future<void>.delayed(const Duration(milliseconds: 50));
        }
        final after = await view.stats();
        hostLog('3D GPU probe: ${jsonEncode(after)}');
        hostLog(
          '3D scene requests: $sceneRequests -> ${view.sceneRequests}; mesh requests: $meshRequests -> ${view.meshRequests}',
        );
        var good =
            (after['diagnosticPixels'] as int) > 1000 &&
            after['meshUploads'] == baseline['meshUploads'] &&
            after['instanceUploads'] == baseline['instanceUploads'] &&
            after['sceneUpdates'] == baseline['sceneUpdates'] &&
            view.sceneRequests == sceneRequests &&
            view.meshRequests == meshRequests &&
            _documentRevision == revision &&
            (after['cameraUpdates'] as int) >
                (baseline['cameraUpdates'] as int) + 20 &&
            (after['drawCalls'] as int) <= 4;
        // Presenters that load scenes inside Rust (macOS, issue 0101) report
        // their pooled targets; they must also follow edits and undo, and
        // keep that pool bounded across resizes.
        if (after.containsKey('targets')) {
          // Frames kept being produced AND pulled by Flutter throughout
          // navigation; pulls alone could be re-pulls of one stale frame.
          final pulled =
              (after['consumerPulls'] as int) >
                  (baseline['consumerPulls'] as int) + 10 &&
              (after['frames'] as int) > (baseline['frames'] as int) + 10;
          hostLog(
            '3D consumer probe: pulls ${baseline['consumerPulls']} -> ${after['consumerPulls']}, leases ${after['leases']}, pool waits ${after['poolWaits']}',
          );
          good = good && pulled;
          good = good && await _probeSceneFollowsHistory(view, after);
          good = good && await _probeHiddenLoadReloadsWhenShown(view);
          good = good && await _probeResizeKeepsPoolBounded(view);
        } else {
          // Linux's pushed scene packet must reject invalid instances without
          // installing even its otherwise valid replacement meshes/camera.
          good = good && await _probeRejectedScenePacket(view);
          good =
              good && await _probeSceneFollowsHistory(view, await view.stats());
        }
        if (hostEnvironment('DOMOWIK_3D_ORIENTATION_PROBE') == '1') {
          final orientation = await view.probeOrientation();
          var upright =
              (orientation['topRedPixels'] as int) > 1000 &&
              (orientation['bottomBluePixels'] as int) > 1000;
          if (after.containsKey('targets')) {
            // What Flutter composites, not only what the renderer drew.
            await Future<void>.delayed(const Duration(milliseconds: 200));
            final composited = await view.compositedOrientation();
            hostLog('3D composited orientation: ${jsonEncode(composited)}');
            upright =
                upright &&
                composited['topRedPixels']! > 1000 &&
                composited['bottomBluePixels']! > 1000;
          }
          hostLog('3D orientation probe: ${jsonEncode(orientation)}');
          hostLog(
            upright
                ? '3D ORIENTATION PROBE PASSED'
                : '3D ORIENTATION PROBE FAILED',
          );
          good = good && upright;
        }
        hostLog(good ? '3D GPU PROBE PASSED' : '3D GPU PROBE FAILED');
        await cleanup();
        hostExit(good ? 0 : 1);
      } finally {
        await cleanup();
      }
    } catch (e, stack) {
      hostLogError('3D GPU PROBE FAILED: $e\n$stack');
      hostExit(1);
    }
  }

  Future<Map<String, Object?>> _awaitStats(
    ThreeDViewState view,
    bool Function(Map<String, Object?>) done,
  ) async {
    var stats = await view.stats();
    for (var i = 0; i < 200 && !done(stats); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 20));
      stats = await view.stats();
    }
    return stats;
  }

  Future<bool> _probeRejectedScenePacket(ThreeDViewState view) async {
    final before = await view.stats();
    var rejected = false;
    try {
      await ThreeDViewState.channel.invokeMethod<void>('scene', {
        'session': view.session,
        'meshes': {
          'builtin:cube': {'vertices': Float32List(30)},
        },
        'matrix': Float64List.fromList(
          List.generate(16, (i) => i % 5 == 0 ? 1.0 : 0.0),
        ),
        'width': 512,
        'height': 512,
        'nodes': [
          {'mesh': 'builtin:cube', 'matrix': Float32List(1)},
        ],
      });
    } on PlatformException {
      rejected = true;
    }
    await view.diagnosticCapture();
    final after = await _awaitStats(
      view,
      (s) => (s['frames'] as int) > (before['frames'] as int),
    );
    final good =
        rejected &&
        after['sceneUpdates'] == before['sceneUpdates'] &&
        after['meshUploads'] == before['meshUploads'] &&
        after['instanceUploads'] == before['instanceUploads'] &&
        after['triangles'] == before['triangles'] &&
        (after['diagnosticPixels'] as int) > 1000;
    hostLog(
      good
          ? '3D SCENE ATOMICITY PROBE PASSED'
          : '3D SCENE ATOMICITY PROBE FAILED',
    );
    return good;
  }

  /// Undoing the last placement must remove its geometry from the 3D scene
  /// and redoing it must restore it, each through one scene load.
  Future<bool> _probeSceneFollowsHistory(
    ThreeDViewState view,
    Map<String, Object?> before,
  ) async {
    final updates = before['sceneUpdates'] as int,
        triangles = before['triangles'] as int;
    _changeHistory();
    final undone = await _awaitStats(
      view,
      (s) =>
          (s['sceneUpdates'] as int) > updates &&
          (s['triangles'] as int) < triangles,
    );
    _changeHistory(redo: true);
    final redone = await _awaitStats(
      view,
      (s) =>
          (s['sceneUpdates'] as int) > (undone['sceneUpdates'] as int) &&
          s['triangles'] == triangles,
    );
    hostLog(
      '3D history probe: triangles $triangles -> ${undone['triangles']} -> ${redone['triangles']}',
    );
    return (undone['triangles'] as int) < triangles &&
        redone['triangles'] == triangles &&
        redone['error'] == '';
  }

  /// A history change whose scene load starts while 3D is visible and is
  /// still finishing as the view hides must be on screen once the view is
  /// shown again. Whichever order the load and the hide take, the shown scene
  /// is the undone one.
  Future<bool> _probeHiddenLoadReloadsWhenShown(ThreeDViewState view) async {
    final before = await view.stats();
    final updates = before['sceneUpdates'] as int,
        triangles = before['triangles'] as int;
    _changeHistory();
    await WidgetsBinding.instance.endOfFrame;
    setState(() => _viewMode = '2d');
    await Future<void>.delayed(const Duration(milliseconds: 300));
    setState(() => _viewMode = 'split');
    final shown = await _awaitStats(
      view,
      (s) =>
          (s['sceneUpdates'] as int) > updates &&
          (s['triangles'] as int) < triangles &&
          !view.loading,
    );
    _changeHistory(redo: true);
    final restored = await _awaitStats(
      view,
      (s) =>
          (s['sceneUpdates'] as int) > (shown['sceneUpdates'] as int) &&
          s['triangles'] == triangles,
    );
    hostLog(
      '3D hidden-load probe: triangles $triangles -> ${shown['triangles']} -> ${restored['triangles']}, scene updates $updates -> ${shown['sceneUpdates']}',
    );
    return (shown['triangles'] as int) < triangles &&
        restored['triangles'] == triangles &&
        restored['error'] == '';
  }

  /// Twenty resizes may recreate the buffer pool each time, but attached
  /// targets stay within the pool bound and frames keep arriving.
  Future<bool> _probeResizeKeepsPoolBounded(ThreeDViewState view) async {
    final frames = (await view.stats())['frames'] as int;
    final matrix = view.camera.matrix(1.5);
    for (var i = 0; i < 20; i++) {
      await ThreeDViewState.channel.invokeMethod<void>('camera', {
        'session': view.session,
        'matrix': matrix,
        'width': 400 + 40 * i,
        'height': 300 + 30 * i,
      });
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    final stats = await _awaitStats(
      view,
      (s) => (s['frames'] as int) > frames + 5,
    );
    view.cameraChanged();
    hostLog(
      '3D resize probe: targets ${stats['targets']}, leases ${stats['leases']}, frames $frames -> ${stats['frames']}, pool waits ${stats['poolWaits']}',
    );
    // Old pools' buffers stay leased after a resize; the lease cap bounds
    // them across every pool generation.
    return (stats['targets'] as int) <= 12 &&
        (stats['leases'] as int) <= 10 &&
        (stats['frames'] as int) > frames + 5 &&
        stats['error'] == '';
  }

  /// Adds four walls through the ordinary button handler, then reports what
  /// the painter was handed on the frame after.
  Future<void> _runFirstFrameProbe() async {
    // The callback is registered while initState is still starting the
    // asynchronous core/config load. On a cold native launch the first frame
    // can therefore arrive before the initial plan has selected its level;
    // adding walls at that point is a silent no-op. Wait for the same ready
    // state a user sees before driving the ordinary add-wall handler.
    const maxStartupWaits = 400;
    var startupWaits = 0;
    while ((_bridge == null || _handle == 0 || _levelId == null) &&
        _loadError == null &&
        startupWaits < maxStartupWaits) {
      startupWaits++;
      await Future<void>.delayed(const Duration(milliseconds: 25));
    }
    if (_loadError != null) {
      hostLog('FIRST FRAME PROBE FAILED: $_loadError');
      hostExit(1);
    }
    if (_bridge == null || _handle == 0 || _levelId == null) {
      hostLog(
        'FIRST FRAME PROBE FAILED: the editor did not become ready '
        'within ${maxStartupWaits * 25} ms',
      );
      hostExit(1);
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
      hostLog('frames waited for the painter: $framesWaited');
      final lit = await _rasterisedPixels();
      final image = _bridge?.loadedImagePath();
      final bundle = hostBundleRoot();
      final buffers = _bridge?.outstandingBuffers() ?? -1;

      hostLog('walls in document: ${_walls.length}');
      hostLog('walls drawn by the painter: $painted');
      hostLog('non-background pixels rasterised: $lit');
      hostLog('outstanding buffers: $buffers');
      hostLog('image: $image');

      final good =
          _walls.length == 4 &&
          painted == 4 &&
          lit > 0 &&
          buffers == 0 &&
          image != null &&
          bundle != null &&
          image.startsWith(bundle);
      hostLog(
        good
            ? 'FIRST FRAME PROBE PASSED'
            : 'FIRST FRAME PROBE FAILED: the UI did not render the core\'s walls '
                  'from inside the bundle without leaking',
      );
      hostExit(good ? 0 : 1);
    });
    // A post-frame callback does not itself schedule the next frame.
    WidgetsBinding.instance.scheduleFrame();
  }

  @override
  void dispose() {
    _catalogClient?.close();
    _hostedWorkflow?.dispose().ignore();
    _livePropertyEdits.cancel();
    _inspectorTabs.dispose();
    _selectionScroll.dispose();
    for (final image in _modelPreviews.values) {
      image.dispose();
    }
    WidgetsBinding.instance.removeObserver(this);
    HardwareKeyboard.instance.removeHandler(_observeSelectionKeys);
    GestureBinding.instance.pointerRouter.removeGlobalRoute(
      _observeSelectionHover,
    );
    _fileWorkflow.dispose();
    _fileDialogs.setCloseHandler(null);
    _geometryCache.clear();
    _canvasFocus.dispose();
    _session?.dispose();
    _propertyContextChanges.dispose();
    _accountSessionChanges.dispose();
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
      final shiftChanged = ((next ^ _modifierMask) & shiftModifier) != 0;
      final controlChanged = ((next ^ _modifierMask) & controlModifier) != 0;
      _modifierMask = next;
      _selection.observeContext(_selectionContext);
      if (shiftChanged &&
          _mode == _DragMode.assetRotate &&
          _dragClassified &&
          _pointerPosition != null) {
        _updateAssetRotation(_view.toPlan(_pointerPosition!));
      }
      if (controlChanged && _placing == 'staircase') {
        final pointer = _mode == _DragMode.staircase && _dragClassified
            ? _pointerPosition
            : _stairStart != null && _activePointer == null
            ? _stairHoverScreen
            : null;
        if (pointer != null) _updateStairPreview(_view.toPlan(pointer));
      }
      if (controlChanged && _mode == _DragMode.stairResize && _dragClassified) {
        _updateStairResizePreview();
      }
    }
    final keyboard = HardwareKeyboard.instance;
    if (event is KeyDownEvent &&
        event.logicalKey == LogicalKeyboardKey.keyR &&
        !keyboard.isControlPressed &&
        !keyboard.isMetaPressed &&
        !keyboard.isAltPressed &&
        !_fileActionPending &&
        _activePointer == null &&
        _navigationStart == null &&
        _poseAssetId == null &&
        ModalRoute.of(context)?.isCurrent == true) {
      final focus = FocusManager.instance.primaryFocus?.context;
      final typing =
          focus?.widget is EditableText ||
          focus?.findAncestorWidgetOfExactType<EditableText>() != null;
      final asset = _activeAssets.where((a) => a.id == _selected).firstOrNull;
      if (!typing && asset != null) {
        _rotateAsset(asset, show3d: false);
        return true;
      }
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

  Future<void> _open() async {
    try {
      final loader = widget.bridgeLoader;
      final bridge = loader != null ? loader() : await loadPlatformCore();
      if (!mounted) return;
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
        _session = PlanSession(bridge);
        _abi = abi;
      });
      // Issue 0117: the config store holds the remembered active level the
      // FIRST refresh applies through `LevelViewState.reset`, so it must be
      // readable before that snapshot can resolve — the two load futures
      // would otherwise race, and a reopen could land on the elevation
      // default whenever the config lagged. A failed load still reaches here:
      // `_configReady` completes normally and behaves like an empty store.
      await _configReady;
      if (!mounted) return;
      await _newPlan();
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
      units: _userConfig.unitSystem,
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
    try {
      final reply = bridge.execute(_handle, jsonEncode(request));
      if (reply.succeeded) {
        _setStatus('$what accepted', isError: false);
      } else {
        _setStatus(
          '$what refused: ${_describe(reply.code)} — ${_readableError(_lastError(bridge))}',
          isError: true,
        );
      }
      return reply.code;
    } finally {
      _refresh();
    }
  }

  String _lastError(DomowikCore bridge) {
    final text = bridge.lastError(_handle);
    if (text == null) return 'no detail (last_error failed)';
    return text.isEmpty ? 'no detail' : text;
  }

  Object? _assetRequest(Map<String, Object?> request) {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) return null;
    final reply = bridge.assets(_handle, jsonEncode(request));
    if (!reply.succeeded) {
      _setStatus(
        'Asset action refused: ${_readableError(_lastError(bridge))}',
        isError: true,
      );
      return null;
    }
    return jsonDecode(reply.text!);
  }

  Object? _assetMutation(Map<String, Object?> request, String what) {
    if (!_acceptAssetPose()) return null;
    final result = _assetRequest(request);
    if (result != null) _setStatus('$what accepted', isError: false);
    _refresh();
    return result;
  }

  Future<void> _openPreferences() async {
    await _configReady;
    if (!mounted) return;
    await _fileAction(() async {
      final key = _planPreferencesKey;
      final previous = _userConfig.viewModeFor(key);
      final remembered = _userConfig.rememberedViewModes.containsKey(key);
      final saved = await showDialog<bool>(
        context: context,
        barrierDismissible: false,
        builder: (_) => PreferencesDialog(
          store: _userConfig,
          documentKey: key,
          layoutName: _filePath?.split('/').last,
          currentViewMode: _viewMode,
        ),
      );
      if (!mounted || saved != true) return false;
      setState(() {
        final next = _userConfig.viewModeFor(key);
        // An unrelated preference must not reset a temporary view choice.
        if (next != previous ||
            remembered != _userConfig.rememberedViewModes.containsKey(key)) {
          _viewMode = next;
        }
      });
      return true;
    });
  }

  void _changeViewMode(String mode) {
    if (_fileActionPending || !planViewModes.containsKey(mode)) return;
    if (mode != '2d' && !_graphicsPolicy.allows3d) {
      setState(
        () => _setStatus(_graphicsPolicy.disabledReason!, isError: false),
      );
      return;
    }
    _cancelDrag();
    setState(() => _viewMode = mode);
    _rememberViewMode();
  }

  void _on3dUnavailable(String details) {
    if (!mounted || _web3dFailure != null) return;
    // Keep the requested mode and persisted preferences intact. The effective
    // policy now unmounts 3D and suppresses retries for this page's lifetime.
    setState(() => _web3dFailure = details);
  }

  void _rememberViewMode() {
    if (_benchmarking || !_graphicsPolicy.allows3d) return;
    final key = _planPreferencesKey;
    if (!_userConfig.rememberedViewModes.containsKey(key)) return;
    _userConfig.saveRememberedViewMode(key, _viewMode).catchError((
      Object error,
    ) {
      if (mounted && _planPreferencesKey == key) {
        setState(
          () => _setStatus('Could not remember view: $error', isError: true),
        );
      }
    });
  }

  Future<void> _openAssetLibrary() async {
    if (!_acceptAssetPose()) return;
    await _configReady;
    if (!mounted) return;
    _cancelDrag();
    final handle = _handle;
    final choice = await showDialog<AssetChoice>(
      context: context,
      builder: (_) => AssetLibraryDialog(
        directories: _libraryDirectories,
        embedded: _embeddedModels,
        initialView: _userConfig.assetLibraryView,
        onViewChanged: (view) async {
          try {
            await _userConfig.saveAssetLibraryView(view);
          } catch (e) {
            if (mounted)
              setState(
                () => _setStatus(
                  'Could not remember asset view: $e',
                  isError: true,
                ),
              );
          }
        },
        onPreview: (model) async {
          if (model.catalog case final entry?) {
            return _catalogClient!.preview(entry);
          }
          final core = _bridge;
          if (!mounted || core == null || _handle != handle) {
            throw StateError('The plan is no longer open');
          }
          final result = await core.assetsInBackground(handle, {
            'kind': 'thumbnail',
            if (model.file != null) 'file': model.file,
            if (model.file != null && model.sourceDirectory != null)
              'sourceDirectory': model.sourceDirectory,
            if (model.file == null) 'digest': model.digest,
          }) as Map<String, Object?>;
          return Uint8List.fromList((result['png'] as List).cast<int>());
        },
        onUpload: _modelUploads ? () => _uploadModel(handle) : null,
        onUploadDirectory: modelDirectoryImportAvailable
            ? () => _uploadModelDirectory(handle)
            : null,
        directoriesAvailable: !_modelUploads,
        onDiscover: _catalogClient == null
            ? null
            : () async => [
                for (final entry in await _catalogClient.discover())
                  LibraryModel(
                    name: entry.name,
                    digest: entry.digest,
                    sourceDescription: entry.description,
                    catalog: entry,
                  ),
              ],
        onAttach: (directory) async {
          if (_handle != handle) return null;
          final path = await hostResolveDirectory(directory);
          if (path == null) return null;
          await _userConfig.saveDirectories([
            ..._userConfig.assetDirectories,
            path,
          ]);
          if (!mounted || _handle != handle) return null;
          setState(
            () => _setStatus(
              'Asset directory remembered in user configuration',
              isError: false,
            ),
          );
          return _libraryDirectories;
        },
        onReplace: (previous, directory) async {
          if (_handle != handle) return null;
          final path = await hostResolveDirectory(directory);
          if (path == null) return null;
          if (_assetDirectories.contains(previous)) {
            final result = _assetMutation({
              'kind': 'replaceDirectory',
              'previousDirectory': previous,
              'directory': path,
            }, 'Change asset directory');
            if (result == null) return null;
          }
          await _userConfig.saveDirectories([
            for (final p in _userConfig.assetDirectories)
              if (p != previous) p,
            path,
          ]);
          if (!mounted || _handle != handle) return null;
          setState(() {});
          return _libraryDirectories;
        },
      ),
    );
    if (!mounted || _handle != handle || choice == null) return;
    var selected = choice;
    if (choice.model.catalog case final entry?) {
      final imported = await _fileAction(() async {
        _setStatus('Downloading ${entry.name}…', isError: false);
        try {
          final bytes = await _catalogClient!.download(entry);
          if (!mounted || _handle != handle) return false;
          final core = _bridge!;
          final reply = core.importModel(
            handle,
            'Domowik catalog (Apache-2.0)/${entry.name}.glb',
            bytes,
          );
          if (!reply.succeeded)
            throw CoreException(reply.code, _lastError(core));
          final result = jsonDecode(reply.text!) as Map<String, Object?>;
          if (result['digest'] != entry.digest)
            throw StateError('Imported catalog digest changed');
          _refresh();
          selected = AssetChoice(
            LibraryModel(name: entry.name, digest: entry.digest),
          );
          return true;
        } catch (_) {
          if (mounted)
            setState(
              () => _setStatus(
                'Could not add ${entry.name} from the catalog. Reopen Furniture to retry.',
                isError: true,
              ),
            );
          return false;
        }
      });
      if (!imported || !mounted || _handle != handle) return;
    }
    setState(() {
      _assetChoice = selected;
      _placing = 'asset';
      _wallChainTool = false;
      _hoverCursor = SystemMouseCursors.basic;
    });
    _canvasFocus.requestFocus();
  }

  /// Embeds a model file the user picks (browser hosts, issue 0088) and
  /// returns the plan's embedded models, or null if nothing was added.
  bool get _modelUploads => widget.modelUploads ?? !hostHasFilesystem;

  Future<List<LibraryModel>?> _uploadModel(int handle) async {
    final picked = await (widget.modelPicker ?? pickModelFile)();
    final core = _bridge;
    if (picked == null || core == null || !mounted || _handle != handle) {
      return null;
    }
    final (name, bytes) = picked;
    final reply = core.importModel(handle, name, bytes);
    if (!reply.succeeded) {
      throw CoreException(reply.code, _lastError(core));
    }
    _refresh();
    return _embeddedModels;
  }

  /// Embeds every supported file from a selected model directory, one file at a
  /// time so the whole directory's bytes are never held in memory together.
  Future<AssetDirectoryImport?> _uploadModelDirectory(int handle) async {
    final selection =
        await (widget.modelDirectoryPicker ?? pickModelDirectory)();
    final core = _bridge;
    if (selection == null || core == null || !mounted || _handle != handle) {
      return null;
    }

    setState(
      () => _setStatus(
        'Importing models from the selected folder…',
        isError: false,
      ),
    );
    final diagnostics = [...selection.diagnostics];
    var imported = 0;
    var failed = 0;
    var selected = 0;
    try {
      await for (final file in selection.files) {
        selected++;
        if (!mounted || _handle != handle) return null;
        try {
          final bytes = await file.readBytes();
          if (!mounted || _handle != handle) return null;
          final reply = core.importModel(
            handle,
            file.sourcePath ?? file.name,
            bytes,
          );
          if (!reply.succeeded) {
            throw CoreException(reply.code, _lastError(core));
          }
          imported++;
        } catch (e) {
          failed++;
          if (failed <= 5) diagnostics.add('Could not import ${file.name}: $e');
        }
      }
    } catch (e) {
      failed++;
      diagnostics.add('Could not finish reading the selected folder: $e');
    }

    if (!mounted || _handle != handle) return null;
    if (selected == 0) {
      diagnostics.insert(
        0,
        'No supported GLB, OBJ, ZIP or USD files were found.',
      );
    }
    if (failed > 5) {
      diagnostics.add('${failed - 5} additional model imports failed.');
    }

    _refresh();
    setState(() {
      _setStatus(
        imported == 0
            ? 'No models were imported${failed == 0 ? '' : '; $failed failed'}'
            : 'Imported $imported model${imported == 1 ? '' : 's'}'
                  '${failed == 0 ? '' : '; $failed failed'}',
        isError: imported == 0 && failed > 0,
      );
    });
    return AssetDirectoryImport(_embeddedModels, diagnostics);
  }

  bool _placeAssetAt(Offset at) {
    if (_placing != 'asset') return false;
    final choice = _assetChoice;
    if (choice == null || _levelId == null) return true;
    _placeChosenAsset(choice, at);
    return true;
  }

  Future<void> _placeChosenAsset(AssetChoice choice, Offset at) async {
    final core = _bridge;
    if (core == null) return;
    final request = <String, Object?>{
      'kind': 'place',
      'levelId': _levelId,
      // Placement commits at whole-millimetre resolution (roundPositionMm).
      'positionMm': [roundPositionMm(at.dx), roundPositionMm(at.dy)],
      if (choice.model.file != null) 'file': choice.model.file,
      if (choice.model.sourceDirectory != null)
        'sourceDirectory': choice.model.sourceDirectory,
      if (choice.model.digest != null) 'digest': choice.model.digest,
    };
    await _fileAction(() async {
      _setStatus('Importing ${choice.model.name}…', isError: false);
      Map<String, Object?>? result;
      try {
        result = await core.assetsInBackground(
          _handle,
          request,
        ) as Map<String, Object?>?;
      } catch (e) {
        if (mounted)
          setState(() => _setStatus('Model import failed: $e', isError: true));
        return false;
      }
      if (!mounted || result == null) return false;
      final diagnostic = result['sizingDiagnostic'] as String?;
      final source = result['sizingSource'];
      _setStatus(
        'Placed ${choice.model.name}${diagnostic != null
            ? ' · Using a 1 m cube: $diagnostic'
            : source == 'proportions'
            ? ' · Units unknown; proportions preserved at a 1 m longest dimension'
            : ''}',
        isError: diagnostic != null,
      );
      _refresh();
      final instanceId = result['instanceId'] as String;
      _showSelectionInspector();
      setState(() {
        _selection.selectForDrag(instanceId);
        _placing = null;
        _assetChoice = null;
      });
      return true;
    });
  }

  void _setAsset(
    AssetFootprint asset, {
    Offset? position,
    List<double>? values,
  }) {
    final raw =
        position ??
        (values == null ? asset.position : Offset(values[0], values[1]));
    // Single-item drags, dialog edits and resize pass-throughs all commit at
    // the editor's whole-millimetre placement resolution (roundPositionMm).
    final at = Offset(roundPositionMm(raw.dx), roundPositionMm(raw.dy));
    _assetMutation({
      'kind': 'set',
      'instanceId': asset.id,
      'positionMm': [at.dx, at.dy],
      'dimensionsMm': values == null
          ? [asset.widthMm, asset.depthMm]
          : [values[2], values[3]],
      'rotationDeg': values == null ? asset.rotationDeg : values[4],
      if (values != null) 'heightMm': values[5],
    }, 'Edit model');
  }

  Future<void> _editAsset(AssetFootprint asset) async {
    _cancelAssetPose();
    final handle = _handle;
    final height = await _assetHeight(asset);
    if (!mounted || _handle != handle) return;
    final values = await showDialog<List<double>>(
      context: context,
      builder: (_) => AssetPropertiesDialog(
        asset: asset,
        heightMm: height,
        units: _userConfig.unitSystem,
      ),
    );
    if (mounted && _handle == handle && values != null)
      _setAsset(asset, values: values);
  }

  Future<void> _editAssetColors(AssetFootprint asset) async {
    await _fileAction(() async {
      final core = _bridge;
      if (core == null) return false;
      final handle = _handle;
      try {
        final raw = await core.assetsInBackground(handle, {
          'kind': 'materials',
          'digest': asset.digest,
        }) as List;
        if (!mounted || _handle != handle) return false;
        final materials = [
          for (final entry in raw)
            SolidColorMaterial(
              entry['key'] as String,
              entry['name'] as String,
              '#${(entry['color'] as List).take(3).map((v) => ((v as num) * 255).round().clamp(0, 255).toRadixString(16).padLeft(2, '0')).join().toUpperCase()}',
            ),
        ];
        final colors = await showDialog<Map<String, String>>(
          context: context,
          builder: (_) => SolidColorsDialog(
            title: '${asset.name} · colors',
            materials: materials,
            colors: asset.materialColors,
          ),
        );
        if (!mounted || colors == null || _handle != handle) return false;
        _assetMutation({
          'kind': 'colors',
          'instanceId': asset.id,
          'materialColors': colors,
        }, 'Edit model colors');
        return true;
      } catch (e) {
        _setStatus('Cannot edit model colors: $e', isError: true);
        return false;
      }
    });
  }

  /// One wall: each face is named by the room beside it. Several walls:
  /// both faces together, since sides differ per wall orientation.
  Future<void> _editWallColors(List<Wall> walls) async {
    final sides = walls.length == 1 ? _derived.wallSides[walls.first.id] : null;
    if (sides != null) return _editWallFaces(walls.first, sides);
    await _fileAction(() async {
      final common = walls.every((w) => w.color == walls.first.color)
          ? walls.first.color
          : null;
      final colors = await showDialog<Map<String, String>>(
        context: context,
        builder: (_) => SolidColorsDialog(
          title: walls.length == 1
              ? 'Wall color'
              : '${walls.length} walls · color',
          materials: const [
            SolidColorMaterial('wall', 'Wall color (both sides)', '#E8DCC4'),
          ],
          colors: {if (common != null) 'wall': common},
        ),
      );
      if (!mounted || colors == null) return false;
      _gesture({
        'kind': 'setWallColors',
        'wallIds': walls.map((w) => w.id).toList(),
        'color': colors['wall'],
      }, 'Edit wall colors');
      return true;
    });
  }

  Future<void> _editWallFaces(Wall wall, WallSidesView sides) async {
    final [left, right] = sides.rooms;
    final names = left != null && left == right
        ? ['$left (left)', '$right (right)']
        : [
            left ?? (right == null ? 'Left' : 'Other'),
            right ?? (left == null ? 'Right' : 'Other'),
          ];
    await _fileAction(() async {
      final handle = _handle;
      final colors = await showDialog<Map<String, String>>(
        context: context,
        builder: (_) => SolidColorsDialog(
          title: 'Wall color',
          materials: [
            SolidColorMaterial('left', '${names[0]} side', '#E8DCC4'),
            SolidColorMaterial('right', '${names[1]} side', '#E8DCC4'),
          ],
          colors: {
            if (sides.colors[0] != null) 'left': sides.colors[0]!,
            if (sides.colors[1] != null) 'right': sides.colors[1]!,
          },
        ),
      );
      if (!mounted || colors == null || _handle != handle) return false;
      _gesture({
        'kind': 'setWallFaces',
        'wallId': wall.id,
        'faces': [colors['left'], colors['right']],
      }, 'Edit wall colors');
      return true;
    });
  }

  /// Colors every wall face bounding a named room in one undoable step.
  Future<void> _paintRoom(Ring room) async {
    final roomId = room.recordId;
    if (roomId == null) return;
    await _fileAction(() async {
      final handle = _handle;
      final colors = await showDialog<Map<String, String>>(
        context: context,
        builder: (_) => SolidColorsDialog(
          title: '${room.label ?? 'Room'} · wall color',
          materials: const [
            SolidColorMaterial('walls', 'Walls facing this room', '#E8DCC4'),
          ],
          colors: const {},
        ),
      );
      if (!mounted || colors == null || _handle != handle) return false;
      _gesture({
        'kind': 'paintRoom',
        'roomId': roomId,
        'color': colors['walls'],
      }, 'Paint room walls');
      return true;
    });
  }

  Future<void> _editDoorColor(Aperture door) async {
    await _fileAction(() async {
      final handle = _handle;
      final colors = await showDialog<Map<String, String>>(
        context: context,
        builder: (_) => SolidColorsDialog(
          title: 'Door color',
          materials: [
            SolidColorMaterial(
              'door',
              'Door leaf',
              door.originalColor ?? '#9E6B40',
            ),
          ],
          colors: {if (door.color != null) 'door': door.color!},
        ),
      );
      if (!mounted || colors == null || _handle != handle) return false;
      _gesture({
        'kind': 'setDoorAppearance',
        'openingId': door.id,
        'color': colors['door'],
        'doorStyle': door.doorStyle,
      }, 'Edit door color');
      return true;
    });
  }

  Future<double> _assetHeight(AssetFootprint asset) async {
    if (asset.heightMm != null) return asset.heightMm!;
    // Legacy plans inferred height from stored width. Measure off the UI thread
    // only when editing them; leave their document metadata untouched otherwise.
    try {
      final core = _bridge;
      if (core != null) {
        final size = await core.assetsInBackground(_handle, {
          'kind': 'size',
          'digest': asset.digest,
        }) as Map<String, Object?>;
        return math.max(
          1.0,
          asset.widthMm * (size['heightRatio'] as num).toDouble(),
        );
      }
    } catch (_) {
      /* Missing models retain the legacy cube height. */
    }
    return asset.widthMm;
  }

  Future<void> _resizeAsset(AssetFootprint asset) async {
    _cancelAssetPose();
    final handle = _handle;
    final height = await _assetHeight(asset);
    if (!mounted || _handle != handle) return;
    final size = await showDialog<List<double>>(
      context: context,
      builder: (_) => ResizeAssetDialog(
        units: _userConfig.unitSystem,
        asset: asset,
        heightMm: height,
        preview: _modelPreviews[asset.previewKey],
      ),
    );
    if (mounted && _handle == handle && size != null) {
      _setAsset(
        asset,
        values: [
          asset.position.dx,
          asset.position.dy,
          size[0],
          size[1],
          asset.rotationDeg,
          size[2],
        ],
      );
    }
  }

  Future<void> _assetContextMenu(PointerDownEvent event) async {
    if (_poseAssetId != null ||
        _activePointer != null ||
        _navigationStart != null ||
        _placing != null)
      return;
    _pointerKind = event.kind;
    final at = _view.toPlan(event.localPosition);
    final hits = _hitsAt(at);
    final hit = hits?.firstOrNull;
    AssetFootprint? asset;
    if (hit != null && hit.kind == 'asset-footprint') {
      asset = _activeAssets.where((a) => a.id == hit.entityId).firstOrNull;
    }
    // Empty space still offers Paste while the clipboard holds instances;
    // otherwise the menu belongs to the hit model alone.
    if (asset == null && _clipboardCount == 0) return;
    final handle = _handle;
    final overlay = Overlay.of(context).context.findRenderObject() as RenderBox;
    final point = overlay.globalToLocal(event.position);
    final choice = await showMenu<String>(
      context: context,
      position: RelativeRect.fromRect(
        Rect.fromLTWH(point.dx, point.dy, 0, 0),
        Offset.zero & overlay.size,
      ),
      items: [
        if (asset != null) ...const [
          PopupMenuItem(value: 'select', child: Text('Select')),
          PopupMenuItem(value: 'resize', child: Text('Resize…')),
          PopupMenuItem(value: 'rotate', child: Text('Rotate…')),
          PopupMenuDivider(),
          PopupMenuItem(value: 'copy', child: Text('Copy')),
          PopupMenuItem(value: 'cut', child: Text('Cut')),
        ],
        if (_clipboardCount > 0)
          const PopupMenuItem(value: 'paste', child: Text('Paste here')),
      ],
    );
    if (!mounted || _handle != handle || choice == null) return;
    if (choice == 'paste') {
      _pasteClipboard(at: at);
      return;
    }
    if (asset == null) return;
    final picked = asset;
    _showSelectionInspector();
    setState(() => _selection.selectForDrag(picked.id));
    if (choice == 'resize') await _resizeAsset(picked);
    if (choice == 'rotate') await _rotateAsset(picked);
    if (choice == 'copy') _copySelection(cut: false);
    if (choice == 'cut') _copySelection(cut: true);
  }

  Future<void> _rotateAsset(AssetFootprint asset, {bool show3d = true}) async {
    if (_poseAssetId == asset.id || !_acceptAssetPose()) return;
    _cancelDrag();
    final handle = _handle;
    final height = await _assetHeight(asset);
    if (!mounted || _handle != handle || _selected != asset.id) return;
    _showSelectionInspector();
    _canvasFocus.requestFocus();
    setState(() {
      _wallChainTool = false;
      _chain.clear();
      _placing = null;
      _assetChoice = null;
      _poseHeight = height;
      _selection.selectForDrag(asset.id);
      _poseAssetId = asset.id;
      // Seeded at the editor's whole-millimetre placement resolution, so an
      // untouched Apply equals the panel seeds and never dispatches a
      // rounding-only history entry.
      _poseInitial = [
        roundPositionMm(asset.position.dx),
        roundPositionMm(asset.position.dy),
        roundPositionMm(asset.elevationMm),
        asset.rotationXDeg,
        asset.rotationYDeg,
        asset.rotationDeg,
      ];
      _poseDraft = List.of(_poseInitial!);
      _poseDraftValid = true;
      if (show3d && _graphicsPolicy.allows3d) _viewMode = 'split';
    });
    if (show3d) _rememberViewMode();
  }

  void _previewAssetPose(List<double> values) {
    setState(() => _poseDraft = List.of(values));
    _pendingPose = List.of(values);
    if (_poseFrameScheduled) return;
    _poseFrameScheduled = true;
    WidgetsBinding.instance.scheduleFrameCallback((_) {
      _poseFrameScheduled = false;
      final pending = _pendingPose;
      _pendingPose = null;
      if (mounted && pending != null && _poseAssetId != null)
        _sendAssetPosePreview(pending);
    });
  }

  /// The furniture a drag would move and the offset its release commits, or
  /// null when the drag has no 3D preview (walls, openings, mixed groups).
  (List<String>, Offset)? get _dragPreviewTarget {
    final from = _dragFrom, to = _dragTo;
    if (from == null || to == null || !_dragClassified) return null;
    if (_mode == _DragMode.asset) {
      final asset = _activeAssets.where((a) => a.id == _dragWall).firstOrNull;
      if (asset == null) return null;
      // Match _setAsset's whole-millimetre commit so 3D shows the result.
      final p = asset.position + to - from;
      final at = Offset(roundPositionMm(p.dx), roundPositionMm(p.dy));
      return ([asset.id], at - asset.position);
    }
    if (_mode == _DragMode.group) {
      final ids = _selection.ids;
      final assets = {for (final a in _activeAssets) a.id};
      if (ids.isEmpty || !ids.every(assets.contains)) return null;
      return (ids.toList(), to - from);
    }
    return null;
  }

  /// Sends at most one translation preview per frame, using the latest drag.
  void _scheduleDragPreview() {
    if (_dragPreviewScheduled ||
        _effectiveViewMode == '2d' ||
        _poseAssetId != null ||
        _dragPreviewTarget == null)
      return;
    _dragPreviewScheduled = true;
    WidgetsBinding.instance.scheduleFrameCallback((_) {
      _dragPreviewScheduled = false;
      final target = _dragPreviewTarget;
      if (!mounted || target == null || _activePointer == null) return;
      final (ids, delta) = target;
      final result = _assetRequest({
        'kind': 'translatePreview',
        'instanceIds': ids,
        'deltaMm': [delta.dx, delta.dy],
      });
      if (result == null) return;
      setState(() {
        _dragPreviewActive = true;
        _posePreviewRevision++;
      });
    });
  }

  void _clearDragPreview() {
    if (!_dragPreviewActive) return;
    _dragPreviewActive = false;
    _assetRequest({'kind': 'clearPosePreview'});
    _posePreviewRevision++;
  }

  void _sendAssetPosePreview(List<double> values) {
    final id = _poseAssetId;
    if (id == null) return;
    final result = _assetRequest({
      'kind': 'pose',
      'instanceId': id,
      'positionMm': values.sublist(0, 3),
      'rotationDeg': values.sublist(3),
      'preview': true,
      'heightMm': _poseHeight,
    });
    if (result != null) {
      final response = result as Map<String, Object?>;
      setState(() {
        _assetInstances = [
          for (final a in response['instances'] as List)
            AssetFootprint.fromJson(a as Map<String, Object?>),
        ];
        _posePreviewRevision++;
      });
    } else {
      _refresh(); // Surface failures, including a stopped browser core.
    }
  }

  void _applyAssetPose(List<double> values) {
    _pendingPose = null;
    final id = _poseAssetId, initial = _poseInitial;
    if (id == null || initial == null) return;
    if (List.generate(6, (i) => values[i] == initial[i]).every((v) => v)) {
      _cancelAssetPose();
      return;
    }
    final result = _assetRequest({
      'kind': 'pose',
      'instanceId': id,
      'positionMm': values.sublist(0, 3),
      'rotationDeg': values.sublist(3),
      'heightMm': _poseHeight,
    });
    if (result == null) {
      setState(() {}); // Show the refusal while retaining the editable draft.
      return;
    }
    setState(() {
      _poseAssetId = null;
      _poseInitial = null;
      _poseDraft = null;
      _posePreviewRevision++;
    });
    _refresh();
  }

  bool get _poseHasChanges {
    final draft = _poseDraft, initial = _poseInitial;
    return _poseAssetId != null &&
        _poseDraftValid &&
        draft != null &&
        initial != null &&
        List.generate(6, (i) => draft[i] != initial[i]).any((v) => v);
  }

  /// Finish the latest draft, including input still queued for a preview frame.
  /// Refusal keeps rotation active and prevents the click's follow-up action.
  bool _acceptAssetPose() {
    if (_poseAssetId == null) return true;
    if (!_poseDraftValid) {
      setState(
        () => _setStatus(
          'Fix invalid placement values before finishing.',
          isError: true,
        ),
      );
      return false;
    }
    _cancelDrag();
    final values = _poseDraft;
    if (values == null) return false;
    _applyAssetPose(List.of(values));
    return _poseAssetId == null;
  }

  void _cancelAssetPose() {
    if (_assetRotationDrag != null) _cancelDrag();
    _pendingPose = null;
    if (_poseAssetId == null) return;
    _assetRequest({'kind': 'clearPosePreview'});
    _poseAssetId = null;
    _poseInitial = null;
    _poseDraft = null;
    _posePreviewRevision++;
    _refresh();
  }

  Future<void> _loadModelPreviews() async {
    if (_previewWorkerRunning || !mounted) return;
    final core = _bridge;
    if (core == null) return;
    _previewWorkerRunning = true;
    try {
      while (mounted && _handle != 0) {
        final visible = [
          ..._activeAssets,
          for (final layer in _backgroundLevels) ...layer.assets,
        ];
        final asset = visible
            .where(
              (a) =>
                  !_modelPreviews.containsKey(a.previewKey) &&
                  !_modelPreviewErrors.containsKey(a.previewKey),
            )
            .firstOrNull;
        if (asset == null) break;
        final handle = _handle;
        final assetRevision = _assetRevision;
        final key = (handle, asset.previewKey);
        _previewPending.add(key);
        ui.Image? image;
        String? error;
        try {
          final payload = await core.assetsInBackground(handle, {
            'kind': 'preview',
            'digest': asset.digest,
            'materialColors': asset.materialColors,
          }) as Map<String, Object?>;
          final png = Uint8List.fromList((payload['png'] as List).cast<int>());
          final codec = await ui.instantiateImageCodec(png);
          try {
            image = (await codec.getNextFrame()).image;
          } finally {
            codec.dispose();
          }
        } catch (e) {
          error = '$e';
        }
        _previewPending.remove(key);
        if (!mounted || _handle != handle) {
          image?.dispose();
          continue;
        }
        setState(() {
          if (image != null) {
            // 128 previews at 256² RGBA is at most 32 MiB. Evict an old,
            // currently invisible preview before admitting another.
            if (_modelPreviews.length >= 128) {
              final used = visible.map((a) => a.previewKey).toSet();
              final victim = _modelPreviews.keys
                  .where((d) => !used.contains(d))
                  .firstOrNull;
              if (victim != null) {
                _modelPreviews[victim]!.dispose();
                _modelPreviews = {..._modelPreviews}..remove(victim);
              } else {
                image!.dispose();
                image = null;
                error = 'Visible model preview cache is full';
              }
            }
            if (image != null)
              _modelPreviews = {..._modelPreviews, asset.previewKey: image!};
          }
          // A failure from before an asset change may no longer hold; the
          // next pass retries against the new content.
          if (error != null && assetRevision == _assetRevision)
            _modelPreviewErrors = {
              ..._modelPreviewErrors,
              asset.previewKey: error!,
            };
        });
      }
    } finally {
      _previewWorkerRunning = false;
    }
  }

  /// Core errors use stable IDs for diagnostics. Resolve known entities before
  /// showing an error; unknown IDs are not useful to someone editing a plan.
  String _readableError(String message) {
    final names = <String, String>{
      for (final level in _levels)
        level.id: level.name.trim().isEmpty ? 'unnamed level' : level.name,
    };
    for (final walls in _wallsByLevel.values) {
      for (var i = 0; i < walls.length; i++) {
        names[walls[i].id] = 'wall segment ${i + 1}';
      }
    }
    return message.replaceAllMapped(
      RegExp(r'\b[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\b'),
      (match) => names[match.group(0)] ?? 'an object',
    );
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

  /// Reads `planState`'s additive `warnings` array. Known entity references are
  /// resolved against the current snapshot: levels have their document names,
  /// while unnamed walls get deterministic per-level labels. A warning code
  /// this build does not recognise still falls back to its native message.
  static List<String> _parseWarnings(
    Map<String, Object?> state, {
    Iterable<LevelInfo> levels = const <LevelInfo>[],
    Map<String, List<Wall>> wallsByLevel = const <String, List<Wall>>{},
  }) {
    final levelNames = <String, String>{
      for (final level in levels)
        level.id: level.name.trim().isEmpty ? 'Unnamed level' : level.name,
    };
    final entityNames = <String, String>{...levelNames};
    for (final walls in wallsByLevel.values) {
      for (var i = 0; i < walls.length; i++) {
        entityNames[walls[i].id] = 'Wall segment ${i + 1}';
      }
    }

    String levelName(Object? id) =>
        id is String ? levelNames[id] ?? 'Unknown level' : 'Unknown level';
    String wallName(Object? id) =>
        id is String ? entityNames[id] ?? 'Unknown wall' : 'Unknown wall';
    String replaceEntityIds(String message) {
      var formatted = message;
      for (final entry in entityNames.entries) {
        formatted = formatted.replaceAll(entry.key, entry.value);
      }
      return formatted;
    }

    final raw = state['warnings'] as List<Object?>? ?? const [];
    return raw
        .map((w) {
          final map = w as Map<String, Object?>?;
          if (map == null) return null;
          switch (map['code']) {
            case 'wall-area-overlap':
              return '${wallName(map['wall'])} and '
                  '${wallName(map['other'])} on ${levelName(map['level'])} '
                  'overlap in thickness, but their axes do not meet';
            case 'slab-intrudes-below-level':
              return '${levelName(map['level'])}: its floor slab reaches below '
                  'the finished floor of ${levelName(map['below'])}';
            case 'ceiling-below-wall-height':
              return '${levelName(map['level'])}: ceiling height is below the '
                  'default wall height';
            case 'duplicate-elevation':
              return '${levelName(map['level'])}: shares elevation with '
                  '${levelName(map['other'])}';
            default:
              final message = map['message'] as String?;
              return message == null ? null : replaceEntityIds(message);
          }
        })
        .whereType<String>()
        .toList();
  }

  static int _countWallOverlaps(Map<String, Object?> state) =>
      (state['warnings'] as List<Object?>? ?? const [])
          .where(
            (warning) =>
                warning is Map<String, Object?> &&
                warning['code'] == 'wall-area-overlap',
          )
          .length;

  static List<Set<String>> _warningTargets(Map<String, Object?> state) => [
    for (final item in state['warnings'] as List<Object?>? ?? const [])
      {
        if (item is Map<String, Object?>)
          for (final key in ['wall', 'other', 'level', 'below'])
            if (item[key] is String) item[key] as String,
      },
  ];

  /// Re-reads the document from the core and repaints.
  void _refresh() {
    final bridge = _bridge;
    if (bridge == null || _handle == 0) {
      return;
    }
    final failure = bridge.failure;
    if (failure != null) {
      // Only the browser core can stop; its state is gone with it.
      setState(
        () => _loadError =
            'The core stopped and this plan cannot continue. Unsaved changes '
            'are lost; reload the page to start again.\n\n$failure',
      );
      return;
    }
    {
      final stateReply = bridge.state(_handle);
      if (!stateReply.succeeded) {
        setState(
          () => _setStatus(
            'session state failed: ${_describe(stateReply.code)}',
            isError: true,
          ),
        );
        return;
      }
      final state = jsonDecode(stateReply.text!) as Map<String, Object?>;
      final assetState =
          _assetRequest({'kind': 'list'}) as Map<String, Object?>?;
      if (assetState != null) {
        _assetDirectories = (assetState['directories'] as List).cast<String>();
        _embeddedModels = [
          for (final m in assetState['models'] as List)
            LibraryModel(
              name: m['name'] as String? ?? 'Model',
              digest: m['digest'] as String,
              sourceDescription: m['source'] as String?,
            ),
        ];
        _assetInstances = [
          for (final a in assetState['instances'] as List)
            AssetFootprint.fromJson(a as Map<String, Object?>),
        ];
        _clipboardCount =
            assetState['clipboardCount'] as int? ?? _clipboardCount;
      }
      final revision = state['document_revision'] as int;
      final dirty = state['dirty'] as bool;
      final assetRevision = state['asset_revision'] as int? ?? 0;
      _observePropertyContext(
        documentRevision: revision,
        levelId: _levelId,
        dirty: dirty,
      );
      if (!_resetLevelView &&
          _geometryCache.session == _handle &&
          revision == _documentRevision &&
          assetRevision == _assetRevision) {
        // A read/save/refusal changes no scene input or selection-cycle state.
        setState(() {
          _dirty = state['dirty'] as bool;
          _undoDepth = state['undo_depth'] as int;
          _redoDepth = state['redo_depth'] as int;
          _warnings = _parseWarnings(
            state,
            levels: _levels,
            wallsByLevel: _wallsByLevel,
          );
          _wallOverlapCount = _countWallOverlaps(state);
          _warningEntities = _warningTargets(state);
          if (_levelView.paintOrder.any(
            (level) => !_geometryCache.contains(level.id),
          )) {
            _rebuildVisibleScene(); // Retry a previously failed native read.
          }
          _openPlans = bridge.openPlans();
          _outstandingBuffers = bridge.outstandingBuffers();
        });
        return;
      }
      final snapshot = bridge.snapshot(_handle);
      if (!snapshot.succeeded) {
        setState(
          () => _setStatus(
            'snapshot failed: ${_describe(snapshot.code)}',
            isError: true,
          ),
        );
        return;
      }
      final document = jsonDecode(snapshot.text!) as Map<String, Object?>;
      final geometryInputs = <String, List<Object?>>{};
      final documentId = document['documentId'] as String?;
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
              // The stored panel ordinal (issue 0115), when the document
              // carries one. Ordinals are u53-capped by the format, so the
              // double round trip is exact.
              displayOrder: (l['displayOrder'] as num?)?.toInt(),
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
          (w['heightMm'] as num).toDouble(),
          color: w['color'] as String?,
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
          entry.key: jsonEncode([
            entry.value,
            document['constructionBodies'],
            document['hostedOpenings'],
            document['levelConnections'],
            document['staircases'],
            if ((document['staircases'] as List<Object?>? ?? const [])
                .isNotEmpty)
              document['levels'],
          ]),
      };
      setState(() {
        _dirty = state['dirty'] as bool;
        _undoDepth = state['undo_depth'] as int;
        _redoDepth = state['redo_depth'] as int;
        final previousLevel = _levelId;
        _documentId = documentId;
        if (_resetLevelView) {
          final key = _session?.levelPreferencesKey(
            documentId,
            levels.map((l) => l.id),
          );
          _viewMode = _benchmarking ? '2d' : _userConfig.viewModeFor(key);
          // Issue 0117: the reopen default is the device-remembered level for
          // this document identity when it still exists, not always the
          // first listed level; LevelViewState.reset falls back whenever
          // there is no remembered value to match. Level flags and camera
          // framing still reset per BEHAVIOR-SPEC §2.6.
          _levelView.reset(
            levels,
            // A benchmark must not depend on, or change, the user's choice.
            rememberedActive: _benchmarking
                ? null
                : _userConfig.lastActiveLevels[key],
          );
          _resetLevelView = false;
        } else {
          _levelView.reconcile(levels);
        }
        if (_levelId != previousLevel) _selection.clear();
        _wallsByLevel = wallsByLevel;
        _allJoins = joins;
        _currentGeometryKeys = geometryKeys;
        _documentRevision = revision;
        if (assetRevision != _assetRevision) {
          // Restored bytes can make a failed preview succeed; images stay,
          // being keyed by their immutable digest.
          _modelPreviewErrors = const {};
        }
        _assetRevision = assetRevision;
        _warnings = _parseWarnings(
          state,
          levels: levels,
          wallsByLevel: wallsByLevel,
        );
        _wallOverlapCount = _countWallOverlaps(state);
        _warningEntities = _warningTargets(state);
        _highlightedWarningIds = const {};
        _selection.observeContext(_selectionContext);
        _rebuildVisibleScene();
        _selection.retainSurvivors({
          for (final asset in _activeAssets) asset.id,
          for (final wall in _walls) wall.id,
          for (final opening in _derived.openings) opening.id,
          for (final item in _derived.construction)
            if (item.kind == 'staircase') item.id,
          ..._joins.keys,
        });
        _openPlans = bridge.openPlans();
        _outstandingBuffers = bridge.outstandingBuffers();
      });
    }
  }

  void _observePropertyContext({
    required int documentRevision,
    required String? levelId,
    required bool dirty,
  }) {
    final planId = _hostedPlan?.id;
    if (_propertyObservedDocumentRevision == documentRevision &&
        _propertyObservedPlanId == planId &&
        _propertyObservedLevelId == levelId &&
        _propertyObservedDirty == dirty) {
      return;
    }
    _propertyObservedDocumentRevision = documentRevision;
    _propertyObservedPlanId = planId;
    _propertyObservedLevelId = levelId;
    _propertyObservedDirty = dirty;
    _propertyContextChanges.value++;
  }

  /// Asks the CORE for the level's occupied region and rooms.
  ///
  /// The snapshot above carries centre lines only. Everything a person
  /// recognises as a floor plan — the wall mass with its mitred corners, the
  /// rooms, the area — is derived, and it is derived once, in Rust.
  Derived? _readDerived(DomowikCore bridge, String levelId) {
    if (_handle == 0) {
      return null;
    }
    {
      final reply = bridge.derived(_handle, levelId);
      if (!reply.succeeded) {
        _setStatus(
          'level geometry failed: ${_describe(reply.code)}',
          isError: true,
        );
        return null;
      }
      final j = jsonDecode(reply.text!) as Map<String, Object?>;
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
        drawn: j['drawn'] == null ? null : rings('drawn'),
        wallPaints: (j['wallPaints'] as List<Object?>? ?? const [])
            .map((e) => WallPaintView.fromJson(e as Map<String, Object?>))
            .toList(),
        wallSides: {
          for (final MapEntry(:key, :value)
              in (j['wallSides'] as Map<String, Object?>? ?? const {}).entries)
            key: WallSidesView.fromJson(value as Map<String, Object?>),
        },
        floorSurfaces: j['floorSurfaces'] == null
            ? null
            : rings('floorSurfaces'),
        construction: ((j['construction'] as List<Object?>?) ?? const [])
            .map((e) => PlanConstruction.fromJson(e as Map<String, Object?>))
            .toList(),
        constructionDiagnostics:
            (j['constructionDiagnostics'] as List<Object?>? ?? const [])
                .cast<String>(),
      );
    }
  }

  /// Reuses Rust's derived payloads while that level's inputs are unchanged.
  /// View changes only compose layers; hidden/deleted payloads are evicted so
  /// visiting hidden levels cannot accumulate an unbounded historical cache.
  /// Called inside setState. It never changes document/history or the camera.
  void _rebuildVisibleScene() {
    final bridge = _bridge;
    if (bridge == null) return;
    final visible = _levelView.paintOrder;
    final flags = _levelView.visibility;
    if (_geometryCache.retain(
      _handle,
      visible.map((level) => level.id).toSet(),
    )) {
      for (final image in _modelPreviews.values) {
        image.dispose();
      }
      _modelPreviews = const {};
      _modelPreviewErrors = const {};
    }
    Derived derivedFor(String id) => _geometryCache.resolve(
      id,
      _currentGeometryKeys[id],
      () => _readDerived(bridge, id),
    );

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
            assets: _assetInstances
                .where((a) => a.levelId == level.id)
                .toList(),
          ),
    ];
    var bounds = PlanPainter.boundsOf(_walls, _derived, assets: _activeAssets);
    for (final layer in _backgroundLevels) {
      final next = PlanPainter.boundsOf(
        layer.walls,
        layer.derived,
        assets: layer.assets,
      );
      if (next != null) bounds = bounds?.expandToInclude(next) ?? next;
    }
    _contentBounds = bounds;
    if (_placing == 'staircase' && !_canAddStairs) {
      _placing = null;
      _stairStart = null;
      _stairPreview = const [];
      _stairHoverScreen = null;
      _hoverCursor = SystemMouseCursors.basic;
    }
    _hoverOnOpeningHost = _openingHostUnder(_hoverPlanMm);
    _loadModelPreviews();
    _outstandingBuffers = bridge.outstandingBuffers();
  }

  List<String> get _drawingWarnings => [
    ..._warnings,
    ...{
      ..._derived.constructionDiagnostics,
      for (final layer in _backgroundLevels)
        ...layer.derived.constructionDiagnostics,
    },
  ];

  void _activateLevel(String id) {
    if (!_levels.any((level) => level.id == id)) return;
    if (id == _levelId) {
      _rememberActiveLevel(id);
      return;
    }
    if (!_acceptAssetPose()) return;
    _livePropertyEdits.flush();
    _cancelDrag();
    setState(() {
      if (!_levelView.activate(id)) return;
      _selection.clear();
      _selection.observeContext(_selectionContext);
      _rebuildVisibleScene();
    });
    // Issue 0117: this activation is the next reopen's default, keyed by the
    // document identity the snapshot carried. Deliberately fire-and-forget:
    // the hint is device-local view state, so a config write that failed —
    // which has already marked the store unwritable for the interactive
    // preferences — must never fail the edit the user just made.
    _rememberActiveLevel(id);
  }

  void _rememberActiveLevel(String id) {
    if (_benchmarking) return;
    final key = _session?.levelPreferencesKey(
      _documentId,
      _levels.map((level) => level.id),
    );
    _userConfig.saveLastActiveLevel(key, id).catchError((Object _) {});
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

  /// Submits one whole-list `orderLevels` gesture (BEHAVIOR-SPEC §2.2, issue
  /// 0115) after a floor-panel drag. `onReorderItem` reports the DESTINATION
  /// index with the departed-item adjustment already applied, so the
  /// permutation is exactly the drag's move over the presentation order
  /// ([`LevelViewState.panelLevels`]) the panel renders — never the
  /// canonical `levels` list. Nothing moves locally: the document is the
  /// only source of the order, the refresh after the gesture
  /// repopulates the panel from its answer, and a refused drag simply leaves
  /// the stored order on screen.
  void _reorderLevels(int oldIndex, int newIndex) {
    final ids = _levelView.panelLevels.map((level) => level.id).toList();
    if (oldIndex == newIndex) return; // dropped where it was: no gesture
    ids.insert(newIndex, ids.removeAt(oldIndex));
    _gesture({'kind': 'orderLevels', 'levelIds': ids}, 'reorder floors');
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
      final changes = await LevelPropertiesDialog.show(
        context,
        level,
        units: _userConfig.unitSystem,
      );
      if (!mounted || changes == null || changes.isEmpty) return false;
      _gesture({
        'kind': 'setLevel',
        'levelId': level.id,
        ...changes.toJson(),
      }, 'edit level');
      return true;
    });
  }

  /// Opens the wall-measurements dialog and, only if something changed,
  /// submits every changed field as ONE atomic `setWall` gesture (R7.1.6).
  /// `lengthMm` travels to the bridge, which resolves it about the stored
  /// Start; this view derives no endpoint itself (ED-0101). Routed through
  /// `_fileAction` for the same serialization reason as the level and opening
  /// forms: an active drag is cancelled before the modal opens and no
  /// document or handle swap can land while it resolves.
  Future<void> _editWallProperties(Wall wall) async {
    await _fileAction(() async {
      final changes = await WallPropertiesDialog.show(
        context,
        wall,
        units: _userConfig.unitSystem,
      );
      if (!mounted || changes == null || changes.isEmpty) return false;
      _gesture({
        'kind': 'setWall',
        'wallId': wall.id,
        ...changes.toJson(),
      }, 'edit wall');
      return true;
    });
  }

  Future<void> _editOpeningProperties(Aperture opening) async {
    await _fileAction(() async {
      final values = await OpeningPropertiesDialog.show(
        context,
        opening,
        units: _userConfig.unitSystem,
      );
      if (!mounted || values == null) return false;
      final changes = <String, double>{
        for (final entry in values.entries)
          if (entry.value !=
              switch (entry.key) {
                'widthMm' => opening.widthMm,
                'heightMm' => opening.heightMm,
                'sillMm' => opening.sillMm,
                _ => opening.depthMm,
              })
            entry.key: entry.value,
      };
      if (changes.isEmpty) return false;
      _gesture({
        'kind': 'setOpening',
        'openingId': opening.id,
        ...changes,
      }, 'edit opening');
      return true;
    });
  }

  /// R7.1.7: one explicit group command, independent of selection primary.
  Future<void> _editWallsProperties(List<Wall> walls) async {
    await _fileAction(() async {
      final changes = await WallPropertiesDialog.showMultiple(
        context,
        walls,
        units: _userConfig.unitSystem,
      );
      if (!mounted || changes == null || changes.isEmpty) return false;
      _gesture({
        'kind': 'setWalls',
        'wallIds': walls.map((wall) => wall.id).toList(),
        ...changes.toJson(),
      }, 'edit walls');
      return true;
    });
  }

  /// Property dialogs share the same serialized gate as file actions.
  Future<bool> _fileAction(Future<bool> Function() action) =>
      _fileWorkflow.run(action);

  Future<LeaveDecision> _askToLeave() async {
    final hosted = _hostedPlan != null;
    final choice = await showDialog<LeaveDecision>(
      context: context,
      builder: (context) => AlertDialog(
        title: const Text('Save your changes?'),
        content: Text(
          hosted
              ? 'Save this online plan before leaving, download a copy, or leave without saving.'
              : 'Save this plan before leaving, or leave without saving.',
        ),
        actions: [
          TextButton(
            autofocus: true,
            onPressed: () => Navigator.pop(context, LeaveDecision.cancel),
            child: const Text('Cancel'),
          ),
          TextButton(
            onPressed: () => Navigator.pop(context, LeaveDecision.discard),
            child: const Text("Don't save"),
          ),
          if (hosted)
            OutlinedButton(
              key: const ValueKey('leave-download-copy'),
              onPressed: () => Navigator.pop(context, LeaveDecision.saveCopy),
              child: const Text('Download a copy'),
            ),
          FilledButton(
            onPressed: () => Navigator.pop(context, LeaveDecision.save),
            child: const Text('Save'),
          ),
        ],
      ),
    );
    return choice ?? LeaveDecision.cancel;
  }

  Future<void> _requestNewPlan() async {
    await _fileWorkflow.replace(() async => _newPlan());
  }

  Future<void> _requestOpenFile() async {
    await _fileWorkflow.replace(_openFile);
  }

  Future<void> _requestSaveFile() async {
    await _fileAction(_saveFile);
  }

  Future<void> _requestSaveAsFile() async {
    await _fileAction(_saveAsFile);
  }

  Future<void> _requestNewOnlinePlan() async {
    final title = await askOnlinePlanTitle(
      context,
      action: 'New online plan',
      confirm: 'Create',
      initialValue: 'Untitled plan',
    );
    if (title == null || !mounted) return;
    await _requestCreateHostedPlan(title);
  }

  Future<void> _requestOpenOnlinePlan() async {
    final client = _hostedPlanClient;
    final accountSession = _accountSession;
    if (client == null || accountSession == null) return;
    _hostedLibraryActionError = null;
    await showOnlinePlans(
      context,
      client: client,
      session: accountSession,
      onOpen: _requestOpenHostedPlan,
      lastActionError: () => _hostedLibraryActionError,
    );
  }

  Future<void> _requestSaveAsOnlinePlan() async {
    final title = await askOnlinePlanTitle(
      context,
      action: 'Save as new online plan',
      confirm: 'Save',
      initialValue: _currentPlanTitle,
    );
    if (title == null || !mounted) return;
    await _fileAction(() => _saveAsOnlinePlan(title));
  }

  Future<void> _requestExportUsdz() async {
    await _fileAction(_exportUsdz);
  }

  HostedPlanClient? get _hostedPlanClient {
    final client = _accountClient;
    if (client is HostedPlanClient) return client as HostedPlanClient;
    return null;
  }

  PropertyPlannerClient? get _propertyPlannerClient {
    final client = _accountClient;
    if (client is PropertyPlannerClient) return client as PropertyPlannerClient;
    return null;
  }

  String _friendlyWorkflowError(Object error) {
    if (error is AccountException && error.code == 'lease-held') {
      return 'This plan is being edited in another browser or API session. '
          'Close it there to release its writer lease, then refresh and try again. '
          'Your current plan remains open.';
    }
    if (error is AccountException && error.code == 'revision-conflict') {
      return 'This online plan has a newer version. Reopen it to continue; '
          'your current plan remains open.';
    }
    final detail = error is AccountException ? error.message : '$error';
    return 'The hosted action failed: $detail. Your current plan remains open.';
  }

  void _accountSessionChanged(AccountSession? session) {
    if (!mounted) return;
    _accountSessionChanges.value++;
    setState(() => _accountSession = session);
  }

  Future<bool> _beforeAccountSignIn() => _fileWorkflow.run(() async {
    if (!await _fileWorkflow.confirmLeaving()) return false;
    return mounted;
  });

  Future<bool> _signOutFromAccount(AccountSession accountSession) =>
      _fileWorkflow.run(() async {
        if (!await _fileWorkflow.confirmLeaving()) return false;
        final client = _accountClient;
        if (client == null) return false;
        await _detachHosted();
        try {
          await client.signOut(accountSession);
        } on Object catch (error) {
          if (mounted) {
            setState(
              () => _setStatus('could not sign out: $error', isError: true),
            );
            ScaffoldMessenger.of(context).showSnackBar(
              SnackBar(content: Text('Could not sign out: $error')),
            );
          }
          return false;
        }
        if (mounted) {
          _accountSessionChanges.value++;
          setState(() {
            _accountSession = null;
            _setStatus(
              _dirty
                  ? 'signed out; current edits are local and unsaved'
                  : 'signed out',
              isError: false,
            );
          });
        }
        return true;
      });

  /// Online plans need a hosted service and a signed-in account.
  bool get _onlineAvailable =>
      _hostedPlanClient != null && _accountSession != null;

  String get _currentPlanTitle =>
      _hostedPlan?.title ??
      _filePath?.split('/').last.replaceAll(RegExp(r'\.domo$'), '') ??
      'Untitled plan';

  Widget? _propertyPlans() {
    final client = _hostedPlanClient;
    final propertyClient = _propertyPlannerClient;
    if (client == null || propertyClient == null) return null;
    final accountSessionRevision = _accountSessionChanges.value;
    return PropertyPlannerManagerButton(
      client: propertyClient,
      planClient: client,
      session: _accountSession,
      propertyHint: Uri.base.queryParameters['property'],
      isSessionCurrent: (session) =>
          mounted &&
          identical(_accountSession, session) &&
          _accountSessionChanges.value == accountSessionRevision,
      sessionChanges: _accountSessionChanges,
      onOpen: _requestOpenHostedPlan,
    );
  }

  Future<bool> _requestOpenHostedPlan(HostedPlan plan) {
    _hostedLibraryActionError = null;
    return _fileWorkflow.replace(() => _checkoutHostedPlan(plan));
  }

  Future<bool> _requestCreateHostedPlan(String title) {
    _hostedLibraryActionError = null;
    return _fileWorkflow.replace(() async {
      final client = _hostedPlanClient;
      final accountSession = _accountSession;
      if (client == null || accountSession == null) return false;
      final plan = await client.createPlan(accountSession, title);
      return _checkoutHostedPlan(plan);
    });
  }

  /// Uploads the current encoding as a new online plan and makes it where
  /// Save writes, keeping the open document and its history. Nothing changes
  /// unless the new plan's lease matches the uploaded revision.
  Future<bool> _saveAsOnlinePlan(String title) async {
    final client = _hostedPlanClient;
    final accountSession = _accountSession;
    final session = _session;
    if (client == null || accountSession == null || session == null) {
      return false;
    }
    final snapshot = session.captureForSave();
    final plan = await client.importPlan(accountSession, title, snapshot.bytes);
    if (!mounted) return false;
    final candidate = _hostedWorkflowFor(client, accountSession);
    Object? failure;
    try {
      if (!await candidate.attach(plan, baseline: plan.revision)) {
        failure = 'it changed before it could be opened';
      }
    } on Object catch (error) {
      failure = error is AccountException ? error.message : error;
    }
    if (failure != null || !mounted) {
      await candidate.dispose();
      if (mounted) {
        setState(
          () => _setStatus(
            'saved "$title" online, but could not switch to it ($failure). '
            'It is listed under Open online plan; your current plan is unchanged',
            isError: true,
          ),
        );
      }
      return false;
    }
    final previous = _hostedWorkflow;
    session.detachFile(plan.title);
    final marked = session.markSaved(snapshot);
    setState(() {
      _hostedPlan = plan;
      _hostedWorkflow = candidate;
      _hostedSaveBlocked = false;
      _setStatus(
        marked.succeeded && !session.isDirty
            ? 'saved as new online plan "${plan.title}"'
            : 'saved as new online plan "${plan.title}"; newer edits are not saved yet',
        isError: false,
      );
    });
    _refresh();
    if (previous != null) {
      await previous.release();
      await previous.dispose();
    }
    return marked.succeeded;
  }

  Future<bool> _checkoutHostedPlan(HostedPlan plan) async {
    final client = _hostedPlanClient;
    final accountSession = _accountSession;
    final session = _session;
    if (client == null || accountSession == null || session == null) {
      return false;
    }
    final previous = _hostedWorkflow;
    final previousPlan = _hostedPlan;
    final previousBaseline = previous?.expectedRevision;
    final reopeningSamePlan = previous != null && previousPlan?.id == plan.id;
    if (reopeningSamePlan) {
      final released = await previous.release(bestEffort: false);
      if (!released) {
        throw const AccountException(
          'Could not confirm release of the current hosted lease. The open plan is preserved; retry after the connection recovers.',
        );
      }
    }
    final candidate = _hostedWorkflowFor(client, accountSession);
    try {
      final checkout = await candidate.checkout(plan);
      if (!mounted) {
        await candidate.dispose();
        return false;
      }
      final result = session.open(
        PlanBytes(plan.title, checkout.download.bytes),
      );
      if (!result.succeeded) {
        await candidate.dispose();
        final message = 'Could not open online plan: ${_describe(result.code)}';
        setState(() {
          _hostedLibraryActionError = message;
          _setStatus(message, isError: true);
        });
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(message)));
        if (reopeningSamePlan && previousPlan != null) {
          await _restoreHostedLeaseAfterFailedReopen(
            previous,
            previousPlan,
            previousBaseline,
          );
        }
        return false;
      }
      setState(() {
        _resetForPlanReplacement();
        _hostedPlan = plan;
        _hostedWorkflow = candidate;
        _hostedSaveBlocked = false;
        _setStatus('opened online plan "${plan.title}"', isError: false);
      });
      _refresh();
      if (previous != null && !identical(previous, candidate)) {
        await previous.release();
        await previous.dispose();
      }
      return true;
    } on Object {
      await candidate.dispose();
      if (reopeningSamePlan && previousPlan != null) {
        await _restoreHostedLeaseAfterFailedReopen(
          previous,
          previousPlan,
          previousBaseline,
        );
      }
      rethrow;
    }
  }

  /// Re-takes the lease of the plan whose bytes are still open after a
  /// failed reopen. The editor keeps [baseline], the revision those bytes
  /// were last confirmed at; a different server revision is a conflict.
  Future<void> _restoreHostedLeaseAfterFailedReopen(
    HostedWorkflowCoordinator previous,
    HostedPlan previousPlan,
    String? baseline,
  ) async {
    try {
      if (baseline == null) throw StateError('no confirmed revision');
      final attached = await previous.attach(previousPlan, baseline: baseline);
      if (!mounted) return;
      setState(() {
        _hostedWorkflow = previous;
        _hostedPlan = previousPlan;
        _hostedSaveBlocked = !attached;
        _setStatus(
          attached
              ? 'reopen failed; your edits are still open and Save still saves online'
              : 'reopen failed. $_onlineConflictMessage',
          isError: true,
        );
      });
    } on Object {
      await previous.dispose();
      if (!mounted) return;
      setState(() {
        _hostedWorkflow = null;
        _hostedPlan = previousPlan;
        _hostedSaveBlocked = true;
        _setStatus(
          'reopen failed and the online plan could not be reconnected. '
          'Your edits are still open; use Save as to keep them',
          isError: true,
        );
      });
    }
  }

  static const _onlineConflictMessage =
      'The online version changed since your last confirmed save. Your edits '
      'are still open. Use Save as to keep them separately.';

  /// One writer checkout. Only a conflict is reported as it happens: a lost
  /// lease is recovered by the next Save.
  HostedWorkflowCoordinator _hostedWorkflowFor(
    HostedPlanClient client,
    AccountSession accountSession,
  ) {
    late final HostedWorkflowCoordinator workflow;
    workflow = HostedWorkflowCoordinator(
      client: client,
      session: accountSession,
      onStateChanged: (state) {
        if (!mounted || !identical(_hostedWorkflow, workflow)) return;
        if (state != HostedWorkflowState.conflict) return;
        setState(() {
          _hostedSaveBlocked = true;
          _setStatus(_onlineConflictMessage, isError: true);
        });
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(_status)));
      },
    );
    return workflow;
  }

  String _onlineRecoveryMessage(Object error) {
    if (error is AccountException && error.code == 'revision-conflict') {
      return _onlineConflictMessage;
    }
    if (error is AccountException && error.code == 'lease-held') {
      return 'This online plan is open in another tab or session. Close it '
          'there, or wait about a minute for it to expire, then Save again. '
          'Your edits are still open.';
    }
    final detail = error is AccountException ? error.message : '$error';
    return 'Could not reach the online plan ($detail). Your edits are still '
        'open; Save again to retry.';
  }

  Future<void> _detachHosted() async {
    final previous = _hostedWorkflow;
    _hostedWorkflow = null;
    _hostedPlan = null;
    _hostedSaveBlocked = false;
    if (previous != null) {
      await previous.release();
      await previous.dispose();
    }
  }

  /// Saves the online plan. After a lost lease or an unconfirmed save it
  /// first reconnects, which succeeds only while nobody else has saved.
  Future<bool> _saveHostedPlan() async {
    final workflow = _hostedWorkflow;
    final session = _session;
    if (session == null) return false;
    if (workflow == null) {
      setState(
        () => _setStatus(
          'this plan is no longer connected online. Your edits are still '
          'open; use Save as to keep them',
          isError: true,
        ),
      );
      return false;
    }
    _hostedSaveInFlight = true;
    try {
      if (_hostedSaveBlocked ||
          workflow.state == HostedWorkflowState.leaseLost ||
          workflow.state == HostedWorkflowState.conflict) {
        try {
          await workflow.recover();
        } on Object catch (error) {
          if (mounted) {
            setState(() {
              _hostedSaveBlocked = true;
              _setStatus(_onlineRecoveryMessage(error), isError: true);
            });
            ScaffoldMessenger.of(context)
                .showSnackBar(SnackBar(content: Text(_status)));
          }
          return false;
        }
        if (!mounted) return false;
        _hostedSaveBlocked = false;
      }
      final result = await workflow.save(PlanSessionHostedDocument(session));
      if (!mounted) return false;
      _hostedSaveBlocked = false;
      setState(
        () => _setStatus(
          result.localDocumentClean
              ? 'saved online'
              : 'saved online; edits made while saving are not saved yet',
          isError: !result.localDocumentClean,
        ),
      );
      _refresh();
      return result.localDocumentClean;
    } on Object catch (error) {
      if (mounted) {
        setState(() {
          _hostedSaveBlocked = true;
          _setStatus(
            workflow.state == HostedWorkflowState.conflict
                ? _onlineConflictMessage
                : 'could not confirm the online save '
                      '(${error is AccountException ? error.message : error}). '
                      'Your edits are still open; Save again to retry',
            isError: true,
          );
        });
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(_status)));
        _refresh();
      }
      return false;
    } finally {
      _hostedSaveInFlight = false;
    }
  }

  /// Writes the current authoritative encoding to a browser download without
  /// marking its Rust revision saved or binding it as the hosted plan's target.
  Future<bool> _saveLocalRecoveryCopy() async {
    final session = _session;
    if (_hostedPlan == null || session == null || _handle == 0) return false;
    final target = await _fileDialogs.savePlan('${_hostedPlan!.title}.domo');
    if (target == null || !mounted) return false;
    if (target is! PlanByteTarget) {
      setState(
        () => _setStatus(
          'downloading a copy is unavailable on this host',
          isError: true,
        ),
      );
      return false;
    }
    try {
      final snapshot = session.captureForSave();
      final stored = await target.write(snapshot.bytes);
      if (!mounted) return stored;
      setState(
        () => _setStatus(
          stored
              ? 'downloaded a copy to ${target.name}; the online plan is ${_dirty ? 'not saved yet' : 'saved'}'
              : 'downloaded ${target.name}, but this browser cannot confirm it was kept',
          isError: false,
        ),
      );
      return stored;
    } on Object catch (error) {
      if (mounted) {
        setState(
          () => _setStatus('could not download a copy: $error', isError: true),
        );
      }
      return false;
    }
  }

  void _deleteLevel(String id) {
    // The document contract permits deletion only from the active level's
    // controls. Keep the guard here even though inactive layer menus omit it.
    if (id != _levelId) return;
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
    final source = await _fileDialogs.openPlan();
    if (source == null || !mounted) return false;
    // Reopening must see the last chosen mode even if its write was queued.
    await _userConfig.flush();
    if (!mounted) return false;
    final session = _session;
    if (session == null) return false;
    final result = session.open(source);
    if (!result.succeeded) {
      setState(
        () => _setStatus(
          'could not open: ${_describe(result.code)}',
          isError: true,
        ),
      );
      return false;
    }
    await _detachHosted();
    setState(() {
      _resetForPlanReplacement();
      _setStatus('opened ${source.name}', isError: false);
    });
    _refresh();
    return true;
  }

  /// Plain Save rewrites the plan's own file — the path it was opened from
  /// or last saved to, or a browser file a picker confirmed once — without
  /// asking where. With no such file (a new plan, or one this browser could
  /// only download) it must ask, exactly as Save As does.
  Future<bool> _saveFile() async {
    if (_hostedPlan != null) return _saveHostedPlan();
    final session = _session;
    if (session == null || _handle == 0) return false;
    final target = session.boundTarget;
    if (target == null) return _saveAsFile();
    return _storePlan(session, target);
  }

  /// Save As asks where to write, every time, and the plan adopts the
  /// chosen file on success.
  Future<bool> _saveAsFile() async {
    if (_hostedPlan != null) return _saveLocalRecoveryCopy();
    final session = _session;
    if (session == null || _handle == 0) return false;
    final suggested = _filePath?.split('/').last ?? 'plan.domo';
    final target = await _fileDialogs.savePlan(suggested);
    if (target == null || !mounted) return false;
    return _storePlan(session, target);
  }

  /// Writes every level as a `.usdz` 3D scene where the user chooses. The
  /// plan itself, its file and its saved state are unchanged.
  Future<bool> _exportUsdz() async {
    final session = _session;
    final dialogs = _fileDialogs;
    if (session == null || _handle == 0 || dialogs is! ExportDialogs) {
      return false;
    }
    final exporter = dialogs as ExportDialogs;
    final name = _filePath?.split('/').last ?? 'plan.domo';
    final base = name.endsWith('.domo')
        ? name.substring(0, name.length - 5)
        : name;
    final target = await exporter.saveUsdz('$base.usdz');
    if (target == null || !mounted) return false;
    final reply = session.core.exportUsdz(_handle);
    if (!reply.succeeded) {
      final detail = session.core.lastError(_handle) ?? 'no detail';
      setState(
        () => _setStatus(
          'could not export: ${_describe(reply.code)} — ${_readableError(detail)}',
          isError: true,
        ),
      );
      return false;
    }
    final stored = await target.write(reply.bytes!);
    if (!mounted) return stored;
    // Export warnings, which name models replaced with placeholder boxes.
    final notes = (jsonDecode(reply.text ?? '[]') as List).cast<String>();
    final where = stored
        ? 'exported 3D scene to ${target.name}'
        : 'downloaded ${target.name}';
    setState(
      () => _setStatus(
        notes.isEmpty ? where : '$where; warnings: ${notes.join('; ')}',
        isError: false,
      ),
    );
    return stored;
  }

  /// Stores into [target]. A failed or unconfirmed save leaves the plan
  /// dirty and its current file untouched.
  Future<bool> _storePlan(PlanSession session, PlanTarget target) async {
    final hadChanges = _dirty;
    final rememberView = _userConfig.rememberedViewModes.containsKey(
      _planPreferencesKey,
    );

    try {
      final result = await session.save(target);
      if (!mounted) return false;
      // A legacy plan's Save As changes its local file key, not its format.
      if (result.succeeded && _documentId == null && _levelId != null) {
        _rememberActiveLevel(_levelId!);
        if (rememberView && !_benchmarking) {
          // Save As changes a legacy file's local key. Carry its opt-in to
          // the new destination, without changing the original file's hint.
          _userConfig
              .saveRememberedViewMode(_planPreferencesKey, _viewMode)
              .catchError((Object _) {});
        }
      }
      setState(() {
        if (result.succeeded) {
          _dirty = false;
          _setStatus('saved to ${target.name}', isError: false);
        } else if (result.unconfirmed) {
          // Not a save: leaving would discard the only certain copy.
          _setStatus(
            hadChanges
                ? 'downloaded ${target.name}; this browser cannot confirm it '
                      'was kept, so the plan stays unsaved. Check the '
                      "download, then choose Don't save to leave"
                : 'downloaded ${target.name}; this browser cannot confirm it '
                      'was kept',
            isError: false,
          );
        } else {
          _setStatus(
            'could not save: ${_describe(result.code)} — ${_readableError(result.detail ?? 'no detail')}',
            isError: true,
          );
        }
      });
      return result.succeeded;
    } finally {
      _refresh();
    }
  }

  Future<bool> _newPlan() async {
    final session = _session;
    if (session == null) return false;
    final result = session.create();
    if (!result.succeeded) {
      setState(
        () => _setStatus(
          'could not create a plan: ${_describe(result.code)}',
          isError: true,
        ),
      );
      return false;
    }
    await _detachHosted();
    setState(() {
      _resetForPlanReplacement();
      _setStatus('new plan created', isError: false);
    });
    _refresh();
    return true;
  }

  /// Shared UI baseline after the session successfully replaces a plan.
  void _resetForPlanReplacement() {
    _selection.clear();
    _placing = null;
    _wallChainTool = false;
    _dirty = false;
    _fitRequested = true;
    _resetLevelView = true;
    // The native clipboard dies with the replaced plan session (issue 0113).
    _clipboardCount = 0;
    _hoverPlanMm = null;
    _showGrid = true;
    _snapGrid = true;
    _snapObject = true;
    _snapAngle = true;
    _snapPreview = null;
    _snapFailed = false;
    _snapStartQueried = false;
    _snapRawStart = null;
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
    _livePropertyEdits.flush();
    if (!_acceptAssetPose()) return;
    _cancelDrag();
    final bridge = _bridge;
    if (bridge == null || _handle == 0) {
      return;
    }
    try {
      final action = redo ? 'redo' : 'undo';
      final reply = redo ? bridge.redo(_handle) : bridge.undo(_handle);
      if (!reply.succeeded) {
        setState(
          () => _setStatus(
            '$action failed: ${_describe(reply.code)}',
            isError: true,
          ),
        );
        return;
      }
      final response = jsonDecode(reply.text!) as Map<String, Object?>;
      final batch = response['batch_id'] as String;
      setState(
        () => _setStatus(
          batch.isEmpty ? 'nothing left to $action' : '$action applied',
          isError: false,
        ),
      );
    } finally {
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
                Text(
                  _bridge?.failure == null
                      ? 'The core did not load.'
                      : 'The core stopped.',
                  style: const TextStyle(fontSize: 18),
                ),
                const SizedBox(height: 8),
                SelectableText(error, textAlign: TextAlign.center),
              ],
            ),
          ),
        ),
      );
    }
    final dialogs = _fileDialogs;
    if (dialogs is UnsavedChangesGuard) {
      (dialogs as UnsavedChangesGuard).setUnsavedChanges(_dirty);
    }
    if (_bridge == null) {
      // The browser fetches and compiles the core asynchronously.
      return const Scaffold(
        body: Center(
          child: CircularProgressIndicator(semanticsLabel: 'Loading the core'),
        ),
      );
    }

    return AbsorbPointer(
      absorbing: _fileActionPending && !_hostedSaveInFlight,
      child: EditorShell(
        fileName: _hostedPlan == null
            ? '${_filePath?.split('/').last ?? 'Untitled plan'}${_dirty ? ' • Unsaved' : ''}'
            : 'Online · ${_hostedPlan!.title}${_hostedSaveBlocked
                  ? ' • Not saved online'
                  : _dirty
                  ? ' • Unsaved'
                  : ''}',
        onNew: _requestNewPlan,
        onOpen: _requestOpenFile,
        onNewOnline: _onlineAvailable ? _requestNewOnlinePlan : null,
        onOpenOnline: _onlineAvailable ? _requestOpenOnlinePlan : null,
        onSave: _requestSaveFile,
        onSaveAs: _requestSaveAsFile,
        saveAsLabel: _hostedPlan == null
            ? 'Save as…'
            : 'Download a copy (.domo)',
        onSaveAsOnline: _onlineAvailable ? _requestSaveAsOnlinePlan : null,
        onExportUsdz: _fileDialogs is ExportDialogs ? _requestExportUsdz : null,
        onAssets: _openAssetLibrary,
        onPreferences: _openPreferences,
        account: _accountClient == null
            ? null
            : AccountButton(
                client: _accountClient,
                onSessionChanged: _accountSessionChanged,
                beforeSignIn: _beforeAccountSignIn,
                onSignOut: _signOutFromAccount,
              ),
        propertyPlans: _propertyPlans(),
        viewMode: _effectiveViewMode,
        onViewModeChanged: _changeViewMode,
        onUndo: _undoDepth > 0 || _poseHasChanges ? _changeHistory : null,
        onRedo: _redoDepth > 0 ? () => _changeHistory(redo: true) : null,
        tools: Row(
          children: [
            _tool('Select', Icons.near_me_outlined, null),
            const SizedBox(width: 8),
            _wallsTool(),
            const SizedBox(width: 8),
            _stairsTool(),
            const SizedBox(width: 8),
            _tool('Doors', Icons.door_front_door_outlined, 'door'),
            PopupMenuButton<String>(
              key: const ValueKey('door-type-menu'),
              tooltip: 'Choose door type',
              icon: const Icon(Icons.arrow_drop_down),
              onSelected: (type) {
                if (!_acceptAssetPose()) return;
                _cancelDrag();
                setState(() {
                  _wallChainTool = false;
                  _doorMechanism = type;
                  _placing = 'door';
                  _hoverCursor = SystemMouseCursors.basic;
                  _hoverOnOpeningHost = _openingHostUnder(_hoverPlanMm);
                });
              },
              itemBuilder: (_) => const [
                PopupMenuItem(value: 'doorHinged', child: Text('Hinged door')),
                PopupMenuItem(
                  value: 'doorSliding',
                  child: Text('Sliding door'),
                ),
              ],
            ),
            const SizedBox(width: 8),
            _tool('Windows', Icons.window_outlined, 'window'),
            PopupMenuButton<String>(
              key: const ValueKey('window-type-menu'),
              tooltip: 'Choose window type',
              icon: const Icon(Icons.arrow_drop_down),
              onSelected: (type) {
                if (!_acceptAssetPose()) return;
                _cancelDrag();
                setState(() {
                  _wallChainTool = false;
                  _windowMechanism = type;
                  _placing = 'window';
                  _hoverCursor = SystemMouseCursors.basic;
                  _hoverOnOpeningHost = _openingHostUnder(_hoverPlanMm);
                });
              },
              itemBuilder: (_) => const [
                PopupMenuItem(
                  value: 'windowSideHung',
                  child: Text('Side-hung window'),
                ),
                PopupMenuItem(
                  value: 'windowDoubleSwing',
                  child: Text('Double-swing window'),
                ),
                PopupMenuItem(value: 'windowLift', child: Text('Lift window')),
              ],
            ),
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
        canvas: _workspace(
          Stack(
            children: [
              Positioned.fill(child: _canvas(context)),
              // The tool hint sits at the bottom edge, small and translucent,
              // so it never covers the drawing near the top-left; warnings stay
              // at the top right. Neither resizes the plan viewport.
              Positioned(
                left: 44,
                right: 16,
                bottom: 10,
                child: IgnorePointer(
                  child: Align(
                    alignment: Alignment.bottomLeft,
                    child: DecoratedBox(
                      decoration: BoxDecoration(
                        color: Theme.of(context).colorScheme.surface
                            .withValues(alpha: 0.85),
                        borderRadius: BorderRadius.circular(6),
                      ),
                      child: Padding(
                        padding: const EdgeInsets.symmetric(
                          horizontal: 8,
                          vertical: 4,
                        ),
                        child: Text(
                          _wallChainTool
                              ? (_chain.anchor == null
                                    ? 'Click or drag to start a wall chain'
                                    : 'Click or drag to continue the chain · Enter/double-click/right-click to finish · Esc to cancel the preview')
                              : _placing == null
                              ? 'Drag empty space to select · Shift-click to add · Drag selection to move'
                              : _placing == 'asset'
                              ? 'Click to place ${_assetChoice?.model.name ?? 'model'} · Esc to cancel'
                              : _placing == 'staircase'
                              ? 'Click or drag from this floor’s upper end toward the lower end · Esc to cancel'
                              : 'Click a wall to place a $_placing',
                          style: Theme.of(context).textTheme.labelSmall,
                        ),
                      ),
                    ),
                  ),
                ),
              ),
              if (_drawingWarnings.isNotEmpty)
                Positioned(
                  top: 26,
                  left: 44,
                  right: 16,
                  // Bounded on both sides, so a narrow canvas wraps the
                  // summary instead of letting it spill past the left edge.
                  child: Align(
                    alignment: Alignment.topRight,
                    child: WarningsBanner(
                      warnings: _drawingWarnings,
                      wallOverlapCount: _wallOverlapCount,
                      warningTargets: _warningEntities,
                      onHighlightChanged: (ids) {
                        if (mounted)
                          setState(() => _highlightedWarningIds = ids);
                      },
                    ),
                  ),
                ),
              if (_walls.isEmpty &&
                  _activeAssets.isEmpty &&
                  _derived.construction.isEmpty &&
                  _stairPreview.isEmpty &&
                  _placing != 'staircase')
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
                        Text('Choose Walls to draw your first wall.'),
                      ],
                    ),
                  ),
                ),
            ],
          ),
        ),
        status: _status.isEmpty ? 'Ready to select' : _status,
        statusIsError: _statusIsError,
      ),
    );
  }

  Widget _workspace(Widget plan) {
    if (_graphicsPolicy.disabledReason case final reason?) {
      // Never mount ThreeDView: saved modes and implicit 3D actions must not
      // initialize a browser GPU surface while safe mode is active.
      return Column(
        children: [
          Material(
            key: const ValueKey('safe-2d-banner'),
            color: Theme.of(context).colorScheme.secondaryContainer,
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                mainAxisSize: MainAxisSize.min,
                children: [
                  Row(
                    children: [
                      const Icon(Icons.info_outline),
                      const SizedBox(width: 12),
                      Expanded(child: SelectableText(reason)),
                    ],
                  ),
                  if (_web3dFailure case final details?)
                    ExpansionTile(
                      title: const Text('3D diagnostics'),
                      children: [
                        ConstrainedBox(
                          constraints: const BoxConstraints(maxHeight: 120),
                          child: SingleChildScrollView(
                            child: SelectableText(details),
                          ),
                        ),
                      ],
                    ),
                ],
              ),
            ),
          ),
          Expanded(child: plan),
        ],
      );
    }
    // A different view mode changes the plan pane's size, whichever path
    // changed it; keeping the old zoom would crop the plan, so frame the
    // content again on the next layout.
    if (_viewMode != _laidOutViewMode) {
      _laidOutViewMode = _viewMode;
      _fitRequested = true;
    }
    return LayoutBuilder(
      builder: (context, constraints) {
        final split = _viewMode == 'split';
        final vertical =
            _userConfig.splitOrientation == SplitOrientation.vertical;
        final divider =
            math.max(
              0.0,
              (vertical ? constraints.maxWidth : constraints.maxHeight) - 4,
            ) /
            2;
        final levels = _levelView.paintOrder;
        final elevation =
            _levels.where((l) => l.id == _levelId).firstOrNull?.elevationMm ??
            0.0;
        return Stack(
          children: [
            Positioned(
              key: const ValueKey('workspace-2d'),
              left: 0,
              top: 0,
              width: split && vertical ? divider : constraints.maxWidth,
              height: split && !vertical ? divider : constraints.maxHeight,
              // Clipped: the plan's overlays must stay inside its half of a
              // split, or they show through the 3D pane's toolbar.
              child: Offstage(
                offstage: _viewMode == '3d',
                child: ClipRect(child: plan),
              ),
            ),
            if (split)
              Positioned(
                key: const ValueKey('workspace-divider'),
                left: vertical ? divider : 0,
                top: vertical ? 0 : divider,
                width: vertical ? 4 : constraints.maxWidth,
                height: vertical ? constraints.maxHeight : 4,
                child: const ColoredBox(color: Color(0xFFB6C5CE)),
              ),
            Positioned(
              key: const ValueKey('workspace-3d'),
              left: split && vertical ? divider + 4 : 0,
              right: 0,
              bottom: 0,
              top: split && !vertical ? divider + 4 : 0,
              child: Offstage(
                offstage: _viewMode == '2d',
                child: ThreeDView(
                  key: _threeDKey,
                  core: _bridge!,
                  handle: _handle,
                  revision: _documentRevision,
                  poseRevision: _posePreviewRevision,
                  assetRevision: _assetRevision,
                  levelIds: levels.map((l) => l.id).toList(),
                  ghostedIds: [
                    for (final level in levels)
                      if (_levelView.visibility[level.id] ==
                          LevelVisibility.ghosted)
                        level.id,
                  ],
                  floorElevationMm: elevation,
                  active: _viewMode != '2d',
                  selectedIds: _selection.ids.toList()..sort(),
                  onPick: _pickIn3d,
                  onUnavailable: _on3dUnavailable,
                  onUndo: _undoDepth > 0 || _poseHasChanges
                      ? _changeHistory
                      : null,
                  onRedo: _redoDepth > 0
                      ? () => _changeHistory(redo: true)
                      : null,
                ),
              ),
            ),
          ],
        );
      },
    );
  }

  Widget _tool(String label, IconData icon, String? placing) {
    return ChoiceChip(
      avatar: Icon(icon, size: 18),
      label: Text(label),
      tooltip: switch (placing) {
        null =>
          'Select: drag empty space to select objects; drag selection to move',
        'door' => 'Place a door: click a wall',
        'window' => 'Place a window: click a wall',
        _ => label,
      },
      selected: !_wallChainTool && _placing == placing,
      showCheckmark: false,
      onSelected: (_) {
        if (!_acceptAssetPose()) return;
        _cancelDrag();
        setState(() {
          _wallChainTool = false;
          _chain.clear();
          _placing = placing;
          _hoverCursor = SystemMouseCursors.basic;
          _hoverOnOpeningHost = _openingHostUnder(_hoverPlanMm);
        });
      },
    );
  }

  /// The explicit, creation-only Walls chain tool. Unlike Select it never
  /// selects, drags endpoints/bodies, or
  /// marquees, and re-arming it (even while already selected) starts a fresh
  /// chain session per R6.5.10 (a tool switch is a finish event).
  Widget _wallsTool() {
    return ChoiceChip(
      key: const ValueKey('wall-chain-tool'),
      avatar: const Icon(Icons.polyline_outlined, size: 18),
      label: const Text('Walls'),
      tooltip:
          'Draw connected walls: click or drag point to point; '
          'Enter or double-click to finish',
      selected: _wallChainTool,
      showCheckmark: false,
      onSelected: (_) {
        if (!_acceptAssetPose()) return;
        _cancelDrag();
        setState(() {
          _placing = null;
          _wallChainTool = true;
          _hoverCursor = SystemMouseCursors.basic;
          _chain.clear();
        });
      },
    );
  }

  bool get _canAddStairs {
    final active = _levels.where((l) => l.id == _levelId).firstOrNull;
    return active != null &&
        _levels.any((l) => l.elevationMm < active.elevationMm);
  }

  Widget _stairsTool() => ChoiceChip(
    key: const ValueKey('add-stairs-tool'),
    avatar: const Icon(Icons.stairs_outlined, size: 18),
    label: const Text('Stairs'),
    tooltip: _canAddStairs
        ? 'Draw downstairs to the nearest level below; hold Ctrl for 15° increments'
        : 'Add a level below this floor before drawing stairs',
    selected: _placing == 'staircase',
    showCheckmark: false,
    onSelected: !_canAddStairs
        ? null
        : (_) {
            if (!_acceptAssetPose()) return;
            _cancelDrag();
            setState(() {
              _wallChainTool = false;
              _chain.clear();
              _placing = 'staircase';
              _hoverCursor = SystemMouseCursors.basic;
              _inspectorTabs.index = 1;
            });
            _canvasFocus.requestFocus();
          },
  );

  Map<String, Object?> _stairDrawingRequest(Offset start, Offset end) {
    final endpoint = HardwareKeyboard.instance.isControlPressed
        ? constrainStairDrawingEnd(start, end)
        : end;
    return {
      'upperLevelId': _levelId,
      'startMm': [start.dx, start.dy],
      'endMm': [endpoint.dx, endpoint.dy],
    };
  }

  void _updateStairPreview(Offset end) {
    final start = _stairStart ?? _dragFrom, core = _bridge;
    if (start == null || core == null) return;
    if (start == end) {
      setState(() => _stairPreview = const []);
      return;
    }
    final reply = core.stairPreview(
      _handle,
      jsonEncode(_stairDrawingRequest(start, end)),
    );
    final graphics = reply.succeeded
        ? (jsonDecode(reply.text!) as List<Object?>)
              .map((v) => PlanConstruction.fromJson(v as Map<String, Object?>))
              .toList()
        : <PlanConstruction>[];
    setState(() => _stairPreview = graphics);
  }

  List<StairResizeHandle> get _stairHandles {
    if (_wallChainTool ||
        _placing != null ||
        _poseAssetId != null ||
        _selection.ids.length != 1)
      return const [];
    final item = _derived.construction
        .where((s) => s.id == _selected && s.kind == 'staircase')
        .firstOrNull;
    return item == null ? const [] : stairResizeHandles(item.outline, _view);
  }

  StairResizeHandle? _stairHandleAt(Offset screen, {bool touch = false}) {
    final radius = touch ? 24.0 : 12.0;
    final handles =
        _stairHandles
            .where((h) => (h.position - screen).distance <= radius)
            .toList()
          ..sort(
            (a, b) => (a.position - screen).distanceSquared.compareTo(
              (b.position - screen).distanceSquared,
            ),
          );
    return handles.firstOrNull;
  }

  /// Plan cursor for the active tool: drawing tools use a crosshair, point
  /// placements an add cursor (forbidden off a wall for openings), and Select
  /// the hover-derived cursor.
  MouseCursor get _planCursor {
    if (_wallChainTool || _placing == 'staircase') {
      return SystemMouseCursors.precise;
    }
    return switch (_placing) {
      'door' || 'window' =>
        _hoverOnOpeningHost
            ? SystemMouseCursors.copy
            : SystemMouseCursors.forbidden,
      'asset' => SystemMouseCursors.copy,
      _ => _hoverCursor,
    };
  }

  MouseCursor _stairResizeCursor(StairResizeHandle h) {
    if (h.edge == 'lower' || h.edge == 'upper') return SystemMouseCursors.move;
    final d = h.direction;
    if (d.dx.abs() > d.dy.abs() * 2) return SystemMouseCursors.resizeLeftRight;
    if (d.dy.abs() > d.dx.abs() * 2) return SystemMouseCursors.resizeUpDown;
    return d.dx * d.dy >= 0
        ? SystemMouseCursors.resizeUpLeftDownRight
        : SystemMouseCursors.resizeUpRightDownLeft;
  }

  Map<String, Object?>? get _stairResizeRequest {
    final from = _dragFrom, to = _dragTo;
    if (_resizingStairId == null ||
        _stairResizeEdge == null ||
        from == null ||
        to == null)
      return null;
    final delta = to - from;
    return {
      'staircaseId': _resizingStairId,
      'levelId': _levelId,
      'edge': _stairResizeEdge,
      'deltaMm': [delta.dx, delta.dy],
      'snapAngle': HardwareKeyboard.instance.isControlPressed,
    };
  }

  void _updateStairResizePreview() {
    final request = _stairResizeRequest, core = _bridge;
    if (request == null || core == null) return;
    final reply = core.stairPreview(_handle, jsonEncode(request));
    setState(
      () => _stairPreview = reply.succeeded
          ? (jsonDecode(reply.text!) as List<Object?>)
                .map(
                  (v) => PlanConstruction.fromJson(v as Map<String, Object?>),
                )
                .toList()
          : const [],
    );
  }

  /// Canvas navigation uses the same transform as painting and gestures.
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
      if ((_mode == _DragMode.draw ||
              _mode == _DragMode.staircase ||
              _mode == _DragMode.stairResize ||
              _editingDrag ||
              _mode == _DragMode.opening) &&
          _dragClassified &&
          !_snapReevalScheduled) {
        _snapReevalScheduled = true;
        WidgetsBinding.instance.addPostFrameCallback((_) {
          _snapReevalScheduled = false;
          if (!mounted || !_dragClassified || _pointerPosition == null) return;
          final cursor = _view.toPlan(_pointerPosition!);
          if (_mode == _DragMode.draw) {
            _evaluateCreationSnap(cursor);
          } else if (_mode == _DragMode.staircase) {
            _updateStairPreview(cursor);
          } else if (_mode == _DragMode.stairResize) {
            _updateStairResizePreview();
          } else if (_editingDrag) {
            _evaluateEditSnap(cursor);
          } else if (_mode == _DragMode.opening) {
            _updateOpeningTarget(cursor);
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
    if (_placing == 'staircase' &&
        _stairStart != null &&
        _activePointer == null &&
        _stairHoverScreen != null &&
        !_stairPreviewReevalScheduled) {
      _stairPreviewReevalScheduled = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _stairPreviewReevalScheduled = false;
        if (mounted &&
            _placing == 'staircase' &&
            _stairStart != null &&
            _stairHoverScreen != null) {
          _updateStairPreview(_view.toPlan(_stairHoverScreen!));
        }
      });
    }
    // A view change moves walls under a still pointer; recheck the host.
    if ((_placing == 'door' || _placing == 'window') &&
        _openingHoverScreen != null &&
        !_openingHoverReevalScheduled) {
      _openingHoverReevalScheduled = true;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        _openingHoverReevalScheduled = false;
        final screen = _openingHoverScreen;
        if (!mounted || screen == null) return;
        _hoverPlanMm = _view.toPlan(screen);
        final onHost = _openingHostUnder(_hoverPlanMm);
        if (onHost != _hoverOnOpeningHost) {
          setState(() => _hoverOnOpeningHost = onHost);
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
    _clearDragPreview();
    final chainPending = _wallChainTool && _chain.anchor != null;
    if (_activePointer == null &&
        _navigationStart == null &&
        !clearSelection &&
        !chainPending &&
        _stairStart == null &&
        _stairPreview.isEmpty) {
      return;
    }
    final poseBeforeDrag = _poseDragInitial;
    _poseDragInitial = null;
    _assetRotationDrag = null;
    if (poseBeforeDrag != null && _poseAssetId != null) {
      _previewAssetPose(poseBeforeDrag);
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
      _stairStart = null;
      _stairPreview = const [];
      _stairHoverScreen = null;
      _resizingStairId = null;
      _stairResizeEdge = null;
      _dragWall = null;
      _dragOpening = null;
      _openingTargetHost = null;
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
            final history = EditorWorkflows.heldHistoryShortcut(key);
            final clipboard = EditorWorkflows.heldClipboardShortcut(key);
            if (history != null) {
              _changeHistory(redo: history == HistoryShortcut.redo);
            } else if (clipboard == ClipboardShortcut.copy) {
              _copySelection(cut: false);
            } else if (clipboard == ClipboardShortcut.cut) {
              _copySelection(cut: true);
            } else if (clipboard == ClipboardShortcut.paste) {
              _pasteClipboard();
            } else if (key == LogicalKeyboardKey.enter && _wallChainTool) {
              // R6.5.10 finish event: ends the chain after any already-
              // committed segments, never adding a trailing one. Fires even
              // mid-drag — the in-flight preview is cancelled first.
              _finishChain();
            } else if (key == LogicalKeyboardKey.escape) {
              _cancelAssetPose();
              if (_placing == 'asset') {
                setState(() {
                  _placing = null;
                  _assetChoice = null;
                });
              }
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
                      zooming: EditorWorkflows.commandModifier(
                        control: keyboard.isControlPressed,
                        meta: keyboard.isMetaPressed,
                      ),
                      shift: keyboard.isShiftPressed,
                    ),
                  ),
                );
              } else if (event is PointerScaleEvent) {
                // Browsers deliver a macOS trackpad pinch as ctrl+wheel,
                // which the web engine reports as a scale signal.
                setState(
                  () => _applyView(
                    _view.zoomAt(event.scale, event.localPosition),
                  ),
                );
              }
            },
            onPointerDown: (event) {
              if (!event.localPosition.dx.isFinite ||
                  !event.localPosition.dy.isFinite ||
                  !_finite(_view.toPlan(event.localPosition)))
                return;
              final keyboard = HardwareKeyboard.instance;
              final spacePan = keyboard.logicalKeysPressed.contains(
                LogicalKeyboardKey.space,
              );
              if (_poseAssetId != null &&
                  _activePointer == null &&
                  _navigationStart == null &&
                  _releasedGestureTouches.isEmpty &&
                  event.kind != PointerDeviceKind.touch &&
                  !spacePan &&
                  (event.buttons == kPrimaryButton ||
                      event.buttons == kSecondaryMouseButton) &&
                  !_assetRotationHandleHit(
                    event.localPosition,
                    touch: event.kind == PointerDeviceKind.touch,
                  )) {
                if (!_acceptAssetPose()) return;
              }
              if (_wallChainTool && event.buttons == kSecondaryMouseButton) {
                // R6.5.10 finish event: ends the chain, adds no segment.
                // Fires even mid-drag; the in-flight preview is discarded
                // and its owning pointer released, so its eventual up is
                // stale (R6.5.4).
                _finishChain();
                return;
              }
              if (event.buttons == kSecondaryMouseButton) {
                _assetContextMenu(event);
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
                _hoverPlanMm = at;
                _dragFrom = at;
                _dragTo = at;
                _snapRawStart = at;
                _snapStartQueried = false;
                _mode = _classifyGrab(at, event);
                // R6.4.2: Alt latches for the WHOLE chain gesture, sampled
                // once at the chain's first segment and preserved through
                // every continuation until finish/cancel. Select editing
                // drags keep resampling on every press.
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
              if (_mode == _DragMode.opening && _dragClassified)
                _updateOpeningTarget(_view.toPlan(event.localPosition));
              if (_mode == _DragMode.assetRotate && _dragClassified)
                _updateAssetRotation(_view.toPlan(event.localPosition));
              if (_dragClassified) _scheduleDragPreview();
              if (_mode == _DragMode.staircase && _dragClassified)
                _updateStairPreview(_view.toPlan(event.localPosition));
              if (_mode == _DragMode.stairResize && _dragClassified)
                _updateStairResizePreview();
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
              if (_mode == _DragMode.opening && _dragClassified)
                _updateOpeningTarget(_view.toPlan(event.localPosition));
              if (_mode == _DragMode.assetRotate && _dragClassified)
                _updateAssetRotation(_view.toPlan(event.localPosition));
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
            child: MouseRegion(
              key: const ValueKey('plan-cursor-region'),
              cursor: _planCursor,
              onHover: (event) {
                final at = _view.toPlan(event.localPosition);
                if (!_finite(at)) return;
                _hoverPlanMm = at;
                if (_placing == 'door' || _placing == 'window') {
                  _openingHoverScreen = event.localPosition;
                  final onHost = _openingHostUnder(at);
                  if (onHost != _hoverOnOpeningHost)
                    setState(() => _hoverOnOpeningHost = onHost);
                  return;
                }
                if (_placing == 'staircase' &&
                    _stairStart != null &&
                    _activePointer == null) {
                  _stairHoverScreen = event.localPosition;
                  _updateStairPreview(at);
                  return;
                }
                if (_poseAssetId != null) {
                  final next = _assetRotationHandleHit(event.localPosition)
                      ? SystemMouseCursors.grab
                      : SystemMouseCursors.basic;
                  if (next != _hoverCursor) setState(() => _hoverCursor = next);
                  return;
                }
                if (_activePointer != null ||
                    _wallChainTool ||
                    _placing != null)
                  return;
                final hits = _hitsAt(at);
                final stairHandle = _stairHandleAt(event.localPosition);
                if (stairHandle != null) {
                  final cursor = _stairResizeCursor(stairHandle);
                  if (cursor != _hoverCursor)
                    setState(() => _hoverCursor = cursor);
                  return;
                }
                final opening = _derived.openings
                    .where(
                      (o) =>
                          hits?.any(
                            (hit) =>
                                hit.kind == 'opening' && hit.entityId == o.id,
                          ) ??
                          false,
                    )
                    .firstOrNull;
                final host = _walls
                    .where((w) => w.id == opening?.hostWallId)
                    .firstOrNull;
                MouseCursor next = SystemMouseCursors.basic;
                if (opening != null &&
                    host != null &&
                    _resizeEdgeAt(at, opening, host) != null) {
                  final axis =
                      _view.toScreen(host.end) - _view.toScreen(host.start);
                  if (axis.dx.abs() > axis.dy.abs() * 2) {
                    next = SystemMouseCursors.resizeLeftRight;
                  } else if (axis.dy.abs() > axis.dx.abs() * 2) {
                    next = SystemMouseCursors.resizeUpDown;
                  } else {
                    next = axis.dx * axis.dy >= 0
                        ? SystemMouseCursors.resizeUpLeftDownRight
                        : SystemMouseCursors.resizeUpRightDownLeft;
                  }
                }
                if (next != _hoverCursor) setState(() => _hoverCursor = next);
              },
              onExit: (_) {
                _hoverPlanMm = null;
                _stairHoverScreen = null;
                _openingHoverScreen = null;
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
                    units: _userConfig.unitSystem,
                    showGrid: _showGrid,
                    backgroundLevels: _backgroundLevels,
                    assets: _activeAssets,
                    rotatingAssetId: _poseAssetId,
                    groupPreview:
                        _mode == _DragMode.group &&
                            _dragClassified &&
                            _dragFrom != null &&
                            _dragTo != null
                        ? (_selection.ids, _dragTo! - _dragFrom!)
                        : null,
                    modelPreviews: _modelPreviews,
                    constructionPreview: _stairPreview,
                    resizingStairId: _resizingStairId,
                    showStairResizeHandles:
                        !_wallChainTool &&
                        _placing == null &&
                        _poseAssetId == null,
                    assetPreview:
                        _mode == _DragMode.asset &&
                            _dragClassified &&
                            _dragFrom != null &&
                            _dragTo != null
                        ? (_dragWall!, _dragTo! - _dragFrom!)
                        : null,
                    selectedJoin: _selectedJoinPosition,
                    selectedIds: _selection.ids,
                    selectedJoins: _selectedJoinPositions,
                    marqueeRect: _marqueeRect,
                    editPreview: _editPreview,
                    openingPreview: _openingPreview,
                    warningIds: _highlightedWarningIds,
                    doorSwingPreview: _doorSwingPreview,
                    snapPoint: _snapPreview?.point,
                    snapGuide: _snapPreview?.guide,
                  ),
                  child: const SizedBox.expand(),
                ),
              ),
            ),
          ),
        );
      },
    );
  }

  bool _assetRotationHandleHit(Offset screen, {bool touch = false}) {
    final asset = _activeAssets.where((a) => a.id == _poseAssetId).firstOrNull;
    return asset != null &&
        (assetRotationHandlePosition(asset, _view) - screen).distance <=
            (touch ? 22 : 14);
  }

  void _updateAssetRotation(Offset at) {
    final drag = _assetRotationDrag, initial = _poseDragInitial;
    if (drag == null || initial == null || _poseAssetId == null) return;
    _previewAssetPose([
      ...initial.take(5),
      drag.update(
        at,
        zoom: _view.zoom,
        snapTo15Degrees: HardwareKeyboard.instance.isShiftPressed,
      ),
    ]);
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
    if (_placing == 'staircase') {
      _dragFrom = _stairStart ?? at;
      return _DragMode.staircase;
    }
    if (_placing != null) return _DragMode.place;
    if (_poseAssetId != null) {
      _showSelectionInspector();
      final asset = _activeAssets
          .where((a) => a.id == _poseAssetId)
          .firstOrNull;
      if (_poseDraftValid &&
          asset != null &&
          _assetRotationHandleHit(
            e.localPosition,
            touch: e.kind == PointerDeviceKind.touch,
          )) {
        _poseDragInitial = List.of(_poseDraft!);
        _assetRotationDrag = AssetRotationDrag(
          center: asset.position,
          start: at,
          angleDeg: _poseDraft![5],
        );
        return _DragMode.assetRotate;
      }
      // A touch click-away accepts on release, so a second finger can still
      // interrupt it to start navigation without committing the draft.
      return e.kind == PointerDeviceKind.touch
          ? _DragMode.poseAccept
          : _DragMode.none;
    }
    final stairHandle = _stairHandleAt(
      e.localPosition,
      touch: e.kind == PointerDeviceKind.touch,
    );
    if (stairHandle != null) {
      _resizingStairId = _selected;
      _stairResizeEdge = stairHandle.edge;
      _showSelectionInspector();
      return _DragMode.stairResize;
    }
    final selectedDoor = _derived.openings
        .where((o) => o.id == _selected && o.isDoor)
        .firstOrNull;
    final doorHost = _walls
        .where((w) => w.id == selectedDoor?.hostWallId)
        .firstOrNull;
    if (selectedDoor != null &&
        selectedDoor.mechanism != 'doorSliding' &&
        doorHost != null &&
        _screenDistanceToSegment(
              _view.toScreen(at),
              _view.toScreen(doorHost.start),
              _view.toScreen(doorHost.end),
            ) >
            doorHost.thicknessMm * _view.zoom / 2 + 3 &&
        (selectedDoor.tip - selectedDoor.hinge).distance > 0 &&
        _screenDistanceToSegment(
              _view.toScreen(at),
              _view.toScreen(selectedDoor.hinge),
              _view.toScreen(selectedDoor.tip),
            ) <
            9) {
      _dragOpening = selectedDoor.id;
      return _DragMode.openingSwing;
    }
    final hits = _hitsAt(at);
    if (hits == null) return _DragMode.none;
    final modified = (_modifierMask & toggleModifierMask) != 0;
    // Select never creates walls: plain marquee replaces, modifiers add.
    if (hits.isEmpty) {
      return _DragMode.marquee;
    }
    final hit = hits.first;
    _showSelectionInspector();
    if (!modified &&
        _selection.ids.length > 1 &&
        _selection.ids.contains(hit.entityId)) {
      return _DragMode.group;
    }
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
    if (hit.kind == 'opening') {
      _dragOpening = hit.entityId;
      final opening = _derived.openings
          .where((o) => o.id == hit.entityId)
          .firstOrNull;
      final host = _walls.where((w) => w.id == opening?.hostWallId).firstOrNull;
      final resizeStart = opening == null || host == null
          ? null
          : _resizeEdgeAt(at, opening, host);
      if (resizeStart != null) {
        _openingResizeStart = resizeStart;
        return _DragMode.openingResize;
      }
      return _DragMode.opening;
    }
    if (hit.kind == 'placeholder-box') return _DragMode.selectOnly;
    if (hit.kind == 'staircase') return _DragMode.group;
    if (hit.kind == 'asset-footprint') {
      _dragWall = hit.entityId;
      return _DragMode.asset;
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
    final radiusPx = _pointerKind == PointerDeviceKind.touch ? 24.0 : 12.0;
    final request = jsonEncode({
      'levelId': level,
      'pointMm': [at.dx, at.dy],
      // The core also accepts an exact `radiusPx`/`zoom` pair (ED-0112), but
      // the pointer above is a rounded `toPlan` result: an exact radius with
      // a rounded pointer loses clicks exactly on the boundary, so the
      // rounded radius is sent until the pointer is exact too (issue 0012).
      'radiusMm': radiusPx / _view.zoom,
    });
    {
      final reply = bridge.hitTest(_handle, request);
      if (!reply.succeeded) {
        _setStatus('hit test failed: ${_describe(reply.code)}', isError: true);
        return null;
      }
      final assetHits = _assetRequest({
        'kind': 'hit',
        'levelId': level,
        'pointMm': [at.dx, at.dy],
        'radiusMm':
            (_pointerKind == PointerDeviceKind.touch ? 24.0 : 12.0) /
            _view.zoom,
      }) as List<Object?>?;
      if (assetHits == null) return null;
      return [...(jsonDecode(reply.text!) as List<Object?>), ...assetHits].map((
        entry,
      ) {
        final hit = entry as Map<String, Object?>;
        final point = (hit['closestMm'] as List<Object?>).cast<num>();
        return _PlanHit(
          hit['class'] as String,
          hit['entityId'] as String,
          Offset(point[0].toDouble(), point[1].toDouble()),
        );
      }).toList();
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
    final reply = bridge.marquee(_handle, request);
    if (!reply.succeeded) {
      _setStatus('marquee failed: ${_describe(reply.code)}', isError: true);
      return null;
    }
    final response = jsonDecode(reply.text!) as Map<String, Object?>;
    return (response['entityIds'] as List<Object?>).cast<String>();
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
    (Offset, Offset)? previousSegmentMm,
    List<String> excludedWallIds = const [],
    List<String> excludedJoinIds = const [],
    bool angleTargets = true,
  }) {
    final bridge = _bridge;
    final level = _levelId;
    if (bridge == null || level == null || !_finite(raw)) return null;
    final a = _view.toPlan(Offset.zero);
    final z = _view.toPlan(Offset(_canvasSize.width, _canvasSize.height));
    final radiusPx = _pointerKind == PointerDeviceKind.touch ? 24.0 : 12.0;
    final request = <String, Object?>{
      'levelId': level,
      'pointMm': [raw.dx, raw.dy],
      // See `_hitsAt`: the rounded radius, until the pointer is exact too.
      'radiusMm': radiusPx / _view.zoom,
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
      if (previousSegmentMm != null)
        'previousSegmentMm': [
          [previousSegmentMm.$1.dx, previousSegmentMm.$1.dy],
          [previousSegmentMm.$2.dx, previousSegmentMm.$2.dy],
        ],
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
    {
      final reply = bridge.snap(_handle, jsonEncode(request));
      if (!reply.succeeded) {
        final detail = bridge.lastError(_handle);
        _setStatus('snap failed: ${detail ?? reply.code}', isError: true);
        return null;
      }
      final response = jsonDecode(reply.text!) as Map<String, Object?>;
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
      previousSegmentMm: _wallChainTool ? _chain.previousSegment : null,
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
      previousSegmentMm: _chain.previousSegment,
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

  static double _screenDistanceToSegment(Offset p, Offset a, Offset b) {
    final axis = b - a;
    if (axis.distanceSquared == 0) return (p - a).distance;
    final t =
        (((p - a).dx * axis.dx + (p - a).dy * axis.dy) / axis.distanceSquared)
            .clamp(0.0, 1.0);
    return (p - (a + axis * t)).distance;
  }

  Wall? get _openingHost {
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    return _walls
        .where((wall) => wall.id == (_openingTargetHost ?? opening?.hostWallId))
        .firstOrNull;
  }

  double? _onWall(Wall wall, Offset at) {
    final axis = wall.end - wall.start;
    final d2 = axis.distanceSquared;
    if (!d2.isFinite || d2 <= 0) return null;
    final u =
        ((at - wall.start).dx * axis.dx + (at - wall.start).dy * axis.dy) / d2;
    return u.isFinite ? u : null;
  }

  bool? _resizeEdgeAt(Offset at, Aperture opening, Wall host) {
    if (opening.u == null || opening.widthMm == null) return null;
    final length = (host.end - host.start).distance;
    final t = _onWall(host, at);
    if (length <= 0 || t == null) return null;
    final left =
        (t - opening.u! + opening.widthMm! / (2 * length)).abs() *
        length *
        _view.zoom;
    final right =
        (t - opening.u! - opening.widthMm! / (2 * length)).abs() *
        length *
        _view.zoom;
    return math.min(left, right) <= 8 ? left <= right : null;
  }

  void _updateOpeningTarget(Offset cursor) {
    if (_mode != _DragMode.opening || !_dragClassified) return;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    if (opening == null) return;
    final hits = _hitsAt(cursor);
    if (hits == null) return;
    final candidate = hits
        .where(
          (hit) =>
              hit.kind == 'region' &&
              hit.closest == cursor &&
              hit.entityId != opening.hostWallId,
        )
        .firstOrNull;
    setState(() => _openingTargetHost = candidate?.entityId);
  }

  /// Project pointer displacement onto the opening's own host axis. The
  /// grabbed point keeps its offset from the opening centre while dragging.
  double? get _openingDragU {
    if (_mode != _DragMode.opening || !_dragClassified) return null;
    final from = _dragFrom, to = _dragTo;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    if (from == null || to == null || opening?.u == null) return null;
    final host = _openingHost;
    if (host == null) return null;
    if (host.id != opening!.hostWallId) return _onWall(host, to);
    final axis = host.end - host.start;
    final lengthSquared = axis.distanceSquared;
    if (lengthSquared <= 0 || !lengthSquared.isFinite) return null;
    final u =
        opening.u! +
        (to - from).dx * axis.dx / lengthSquared +
        (to - from).dy * axis.dy / lengthSquared;
    return u.isFinite ? u : null;
  }

  (double, double)? get _openingResize {
    if (_mode != _DragMode.openingResize || !_dragClassified) return null;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    final host = _openingHost;
    final from = _dragFrom, to = _dragTo;
    if (opening?.u == null ||
        opening?.widthMm == null ||
        host == null ||
        from == null ||
        to == null)
      return null;
    final length = (host.end - host.start).distance;
    if (length <= 0) return null;
    final movement = _onWall(host, host.start + to - from);
    if (movement == null) return null;
    final original = opening!;
    final lo = original.u! - original.widthMm! / (2 * length);
    final hi = original.u! + original.widthMm! / (2 * length);
    final moving = (_openingResizeStart ? lo : hi) + movement;
    final fixed = _openingResizeStart ? hi : lo;
    if ((_openingResizeStart && moving >= fixed) ||
        (!_openingResizeStart && moving <= fixed))
      return null;
    return ((moving + fixed) / 2, (moving - fixed).abs() * length);
  }

  String? get _doorSwingSide {
    if (!_dragClassified ||
        (_mode != _DragMode.openingSwing && _mode != _DragMode.opening) ||
        _openingTargetHost != null)
      return null;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening && o.isDoor)
        .firstOrNull;
    final host = _openingHost;
    final from = _dragFrom, to = _dragTo;
    if (opening == null ||
        opening.mechanism == 'doorSliding' ||
        host == null ||
        from == null ||
        to == null ||
        opening.u == null)
      return null;
    final axis = host.end - host.start;
    if (axis.distance <= 0) return null;
    final movement = to - from;
    final cross = axis.dx * movement.dy - axis.dy * movement.dx;
    if (cross.abs() / axis.distance * _view.zoom <= 5) return null;
    final centre = host.start + axis * opening.u!;
    final side = axis.dx * (to.dy - centre.dy) - axis.dy * (to.dx - centre.dx);
    return side >= 0 ? 'Left' : 'Right';
  }

  (String, Offset)? get _doorSwingPreview {
    final side = _doorSwingSide;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    final host = _openingHost;
    if (side == null ||
        opening == null ||
        host == null ||
        side == opening.opensToward)
      return null;
    final axis = host.end - host.start;
    final unit = axis / axis.distance;
    final fromHinge = opening.tip - opening.hinge;
    final along = unit * (fromHinge.dx * unit.dx + fromHinge.dy * unit.dy);
    return (opening.id, opening.hinge + along * 2 - fromHinge);
  }

  (String, Offset, Offset, double)? get _openingPreview {
    final u = _openingDragU;
    final resize = _openingResize;
    final opening = _derived.openings
        .where((o) => o.id == _dragOpening)
        .firstOrNull;
    if ((u == null && resize == null) || opening?.widthMm == null) return null;
    final host = _openingHost;
    if (host == null) return null;
    final axis = host.end - host.start;
    final candidateU = resize?.$1 ?? u!;
    final width = resize?.$2 ?? opening!.widthMm!;
    if (candidateU == opening!.u &&
        width == opening.widthMm &&
        host.id == opening.hostWallId)
      return null;
    if (candidateU * axis.distance <= width / 2 ||
        (1 - candidateU) * axis.distance <= width / 2)
      return null;
    return (
      opening.id,
      host.start + axis * (candidateU - width / (2 * axis.distance)),
      host.start + axis * (candidateU + width / (2 * axis.distance)),
      host.thicknessMm,
    );
  }

  /// Live axes of the moved wall and its directly attached neighbors. This is
  /// a transient guide; the core still resolves joins and topology on commit.
  List<(Offset, Offset, double)>? get _editPreview {
    if (_mode == _DragMode.group && _dragClassified) {
      final from = _dragFrom, to = _dragTo;
      if (from == null || to == null) return null;
      final delta = to - from;
      final moving = _selection.ids;
      final ends = <(String, bool)>{
        for (final wall in _walls)
          if (moving.contains(wall.id)) ...[(wall.id, true), (wall.id, false)],
      };
      for (final members in _joins.values) {
        if (members.any((m) => moving.contains(m.$1.id))) {
          ends.addAll(members.map((m) => (m.$1.id, m.$2)));
        }
      }
      return [
        for (final wall in _walls)
          if (ends.contains((wall.id, true)) || ends.contains((wall.id, false)))
            (
              wall.start +
                  (ends.contains((wall.id, true)) ? delta : Offset.zero),
              wall.end +
                  (ends.contains((wall.id, false)) ? delta : Offset.zero),
              wall.thicknessMm,
            ),
      ];
    }
    if (!_editingDrag || !_dragClassified || _snapFailed) return null;
    final id = _dragWall;
    final from = _dragFrom;
    final to = _dragTo;
    if (id == null || from == null || to == null) return null;
    final wall = _walls.where((candidate) => candidate.id == id).firstOrNull;
    if (wall == null) return null;
    if (to == from) return null;
    final delta = to - from;
    final moved = <String, (Offset?, Offset?)>{};
    void moveNode(bool start, Offset position) {
      for (final members in _joins.values) {
        if (!members.any(
          (member) => member.$1.id == id && member.$2 == start,
        )) {
          continue;
        }
        for (final member in members) {
          final previous = moved[member.$1.id] ?? (null, null);
          moved[member.$1.id] = member.$2
              ? (position, previous.$2)
              : (previous.$1, position);
        }
      }
    }

    if (_mode == _DragMode.body) {
      moveNode(true, wall.start + delta);
      moveNode(false, wall.end + delta);
      moved[id] = (wall.start + delta, wall.end + delta);
    } else {
      moveNode(_dragIsStart, to);
      moved[id] = _dragIsStart ? (to, null) : (null, to);
    }
    return [
      for (final candidate in _walls)
        if (moved[candidate.id] case final ends?)
          (
            ends.$1 ?? candidate.start,
            ends.$2 ?? candidate.end,
            candidate.thicknessMm,
          ),
    ];
  }

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
    _clearDragPreview();
    final mode = _mode;
    final classified = _dragClassified;
    final draft = _draft;
    final from = _dragFrom;
    final to = _dragTo;
    final wall = _dragWall;
    final opening = _dragOpening;
    final openingU = _openingDragU;
    final openingResize = _openingResize;
    final targetHost = _openingTargetHost;
    final swingSide = _doorSwingSide;
    final before = _selectionBeforeDrag;
    // Only `draw` mode may create a chain segment. `_wallChainTool` can also
    // classify as `pan` (middle-click/space), which must keep panning, never
    // be diverted into a wall commit.
    final chaining = _wallChainTool && mode == _DragMode.draw;
    final snapFailed = _snapFailed;
    final stairResizeRequest = _stairResizeRequest;
    setState(() {
      _dragFrom = null;
      _dragTo = null;
      _mode = _DragMode.none;
      _dragWall = null;
      _dragOpening = null;
      _openingTargetHost = null;
      _pointerDownPosition = null;
      _dragClassified = false;
      _selectionBeforeDrag = null;
      _snapPreview = null;
      _snapFailed = false;
      _snapStartQueried = false;
      _snapRawStart = null;
      _assetRotationDrag = null;
      _poseDragInitial = null;
      _resizingStairId = null;
      _stairResizeEdge = null;
      if (mode == _DragMode.stairResize) _stairPreview = const [];
    });

    if (mode == _DragMode.stairResize) {
      if (classified && from != to && stairResizeRequest != null) {
        _gesture({
          'kind': 'resizeStaircase',
          'request': stairResizeRequest,
        }, 'resize stairs');
      }
      return;
    }

    // Releasing the handle keeps a draft only, including a click without drag.
    if (mode == _DragMode.staircase) {
      if (from == null || to == null) return;
      if (!classified && _stairStart == null) {
        setState(() => _stairStart = from);
        return;
      }
      final existing = _derived.construction.map((s) => s.id).toSet();
      final code = _gesture({
        'kind': 'addDescendingStaircase',
        'request': _stairDrawingRequest(from, to),
      }, 'add staircase');
      setState(() {
        _stairStart = null;
        _stairPreview = const [];
        if (code == ok) {
          _expandedStaircaseId = _derived.construction
              .where((s) => s.kind == 'staircase' && !existing.contains(s.id))
              .firstOrNull
              ?.id;
          _inspectorTabs.index = 1;
        }
      });
      return;
    }

    // Releasing the handle keeps a draft only, including a click without drag.
    // In particular, it must not fall through to object selection or history.
    if (mode == _DragMode.assetRotate) return;
    if (mode == _DragMode.poseAccept) {
      if (!classified && _acceptAssetPose() && to != null) {
        _selectAt(to, logicalPosition);
      }
      return;
    }

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
      if (at != null && !_placeAssetAt(at) && !_placeOpeningAt(at))
        _selectAt(at, logicalPosition);
      return;
    }

    switch (mode) {
      case _DragMode.staircase:
      case _DragMode.stairResize:
        return;
      case _DragMode.asset:
        final asset = _activeAssets.where((a) => a.id == wall).firstOrNull;
        if (asset != null && from != null && to != null && from != to) {
          _setAsset(asset, position: asset.position + to - from);
        }
        return;
      case _DragMode.pan:
      case _DragMode.assetRotate:
      case _DragMode.poseAccept:
      case _DragMode.none:
      case _DragMode.place:
      case _DragMode.selectOnly:
        return;
      case _DragMode.marquee:
        // ONE native query, here at release — not per pointer move.
        if (from != null && to != null) {
          final ids = _marqueeAt(from, to);
          if (ids != null) {
            if (ids.isNotEmpty) _showSelectionInspector();
            setState(
              () => _selection.marquee(
                ids,
                additive: (_modifierMask & toggleModifierMask) != 0,
              ),
            );
          }
        }
        return;
      case _DragMode.group:
        if (from != null && to != null && from != to) {
          final delta = to - from;
          _gesture({
            'kind': 'translateSelection',
            'entityIds': _selection.ids.toList(),
            'delta': [delta.dx, delta.dy],
          }, 'move selection');
        }
        return;
      case _DragMode.opening:
        final current = _derived.openings
            .where((candidate) => candidate.id == opening)
            .firstOrNull;
        if (opening == null || openingU == null || current == null) return;
        final host = _walls
            .where((w) => w.id == current.hostWallId)
            .firstOrNull;
        if (targetHost != null && targetHost != current.hostWallId) {
          final newId = _mintId();
          final code = _gesture({
            'kind': 'rehostOpening',
            'openingId': opening,
            'hostWallId': targetHost,
            'newId': newId,
            'u': openingU,
          }, 'move opening to wall');
          if (code == ok) setState(() => _selection.selectForDrag(newId));
        } else if (swingSide != null &&
            swingSide != current.opensToward &&
            from != null &&
            to != null &&
            host != null &&
            (to - from).distance > 0 &&
            (openingU - current.u!).abs() *
                    (host.end - host.start).distance *
                    _view.zoom <=
                5) {
          _gesture({
            'kind': 'setOpening',
            'openingId': opening,
            'opensToward': swingSide,
          }, 'change door swing');
        } else if (openingU != current.u) {
          _gesture({
            'kind': 'moveOpening',
            'openingId': opening,
            'u': openingU,
          }, 'move opening');
        }
        return;
      case _DragMode.openingResize:
        final current = _derived.openings
            .where((o) => o.id == opening)
            .firstOrNull;
        if (opening != null &&
            openingResize != null &&
            current != null &&
            (openingResize.$1 != current.u ||
                openingResize.$2 != current.widthMm)) {
          _gesture({
            'kind': 'setOpening',
            'openingId': opening,
            'u': openingResize.$1,
            'widthMm': openingResize.$2,
          }, 'resize opening');
        }
        return;
      case _DragMode.openingSwing:
        final current = _derived.openings
            .where((o) => o.id == opening)
            .firstOrNull;
        if (opening != null &&
            swingSide != null &&
            swingSide != current?.opensToward) {
          _gesture({
            'kind': 'setOpening',
            'openingId': opening,
            'opensToward': swingSide,
          }, 'change door swing');
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

  /// Whether `at` is over a wall that can host the opening being placed.
  bool _openingHostUnder(Offset? at) {
    if (at == null || (_placing != 'door' && _placing != 'window')) {
      return false;
    }
    final hits = _hitsAt(at);
    return hits != null && _openingHostIn(hits) != null;
  }

  Wall? _openingHostIn(List<_PlanHit> hits) {
    final host = hits.where((hit) => hit.kind == 'region').firstOrNull;
    return _walls.where((wall) => wall.id == host?.entityId).firstOrNull;
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
    final best = _openingHostIn(hits);
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
      'mechanism': kind == 'door' ? _doorMechanism : _windowMechanism,
    }, 'place $kind');
    setState(() => _placing = null);
    return true;
  }

  /// Cycles native-ranked entity IDs without reimplementing geometry ranking.
  void _selectAt(Offset at, Offset logicalPosition) {
    final hits = _hitsAt(at);
    if (hits == null) return;
    _clickSelect(hits.map((hit) => hit.entityId), logicalPosition);
  }

  /// A selection click on ranked [ids]; shared by the plan and the 3D view.
  void _clickSelect(Iterable<String> ids, Offset logicalPosition) {
    setState(() {
      _selection.click(ids, logicalPosition, _selectionContext);
      if (_selection.ids.isNotEmpty) _showSelectionInspector();
      _setStatus(
        _selection.ids.isEmpty
            ? 'nothing selected'
            : _selected == null
            ? '${_selection.ids.length} selected; no primary'
            : _joins.containsKey(_selected)
            ? 'selected a joined corner'
            : _derived.openings.any((opening) => opening.id == _selected)
            ? 'selected an opening'
            : _activeAssets.any((asset) => asset.id == _selected)
            ? 'selected a model footprint'
            : _derived.construction.any(
                (item) => item.kind == 'staircase' && item.id == _selected,
              )
            ? 'selected stairs'
            : '${_selection.ids.length} selected; primary wall',
        isError: false,
      );
    });
  }

  /// A click in the 3D view selects what it hit, switching to that entity's
  /// level when needed; a double click also centres the plan on the hit.
  void _pickIn3d(ScenePick pick) {
    // A plan gesture in flight owns selection until it finishes.
    if (_activePointer != null || !_acceptAssetPose()) return;
    final id = pick.entityId, level = pick.levelId;
    if (id != null && level != null && level != _levelId) {
      _activateLevel(level);
      if (_levelId != level) return; // The switch was refused.
    }
    _clickSelect(id == null ? const [] : [id], pick.position);
    final at = pick.planMm;
    if (pick.open && at != null && _viewMode != '3d' && !_canvasSize.isEmpty) {
      setState(
        () => _applyView(
          _view.pan(_canvasSize.center(Offset.zero) - _view.toScreen(at)),
        ),
      );
    }
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
      previousSegmentMm: _chain.previousSegment,
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
    if (_activeAssets.any((a) => a.id == id)) {
      _assetMutation({'kind': 'delete', 'instanceId': id}, 'Delete model');
      return;
    }
    if (_derived.openings.any((opening) => opening.id == id)) {
      _gesture({'kind': 'deleteOpening', 'openingId': id}, 'delete opening');
      return;
    }
    if (_derived.construction.any(
      (item) => item.kind == 'staircase' && item.id == id,
    )) {
      _livePropertyEdits.cancel();
      _gesture({'kind': 'deleteStaircase', 'staircaseId': id}, 'delete stairs');
      return;
    }
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

  /// Captures the selected models for pasting (issue 0113).
  ///
  /// Only assets are captured: a selection that also holds walls, openings or
  /// room faces contributes its models alone, and one without a model is
  /// inert with a message. Cut captures and deletes in ONE native batch, so a
  /// refusal leaves the document and the clipboard unchanged; the anchor is
  /// the selection's primary when it is part of the captured set.
  void _copySelection({required bool cut}) {
    final selected = _selection.ids;
    final assets = _activeAssets.where((a) => selected.contains(a.id)).toList();
    if (assets.isEmpty) {
      setState(
        () => _setStatus(
          cut ? 'Select a model to cut' : 'Select a model to copy',
          isError: true,
        ),
      );
      return;
    }
    final primary = _selected;
    final anchor = assets.any((a) => a.id == primary)
        ? primary!
        : assets.first.id;
    final result = _assetMutation({
      'kind': cut ? 'cut' : 'copy',
      'instanceIds': [for (final a in assets) a.id],
      'referenceId': anchor,
    }, cut ? 'Cut model' : 'Copy model') as Map<String, Object?>?;
    if (result == null) return;
    final count = result['clipboardCount'] as int? ?? assets.length;
    final label = count == 1 ? '1 model' : '$count models';
    setState(
      () => _setStatus(cut ? 'Cut $label' : 'Copied $label', isError: false),
    );
  }

  /// Creates the clipboard's instances on the ACTIVE level, anchored at the
  /// pointer (issue 0113).
  ///
  /// The menu supplies its own point; a shortcut uses the last canvas-hover
  /// position, falling back to the canvas centre. The native core refuses
  /// whole when the clipboard is empty, the level is gone or any captured
  /// model's bytes are no longer embedded. Selection follows the new anchor.
  void _pasteClipboard({Offset? at}) {
    if (_clipboardCount == 0) {
      setState(() => _setStatus('The clipboard is empty', isError: true));
      return;
    }
    final levelId = _levelId;
    if (levelId == null) return;
    final target = at ?? _hoverPlanMm ?? _viewportCentreMm;
    if (target == null) return;
    final response = _assetMutation({
      'kind': 'paste',
      'levelId': levelId,
      // Whole-millimetre placement resolution (roundPositionMm).
      'positionMm': [roundPositionMm(target.dx), roundPositionMm(target.dy)],
    }, 'Paste') as Map<String, Object?>?;
    if (response == null) return;
    final id = response['instanceId'] as String?;
    if (id == null) return;
    final count = response['clipboardCount'] as int? ?? _clipboardCount;
    final label = count == 1 ? '1 model' : '$count models';
    _showSelectionInspector();
    setState(() {
      _selection.selectForDrag(id);
      _setStatus('Pasted $label', isError: false);
    });
  }

  /// The plan-space point at the centre of the canvas, the paste fallback.
  Offset? get _viewportCentreMm => _canvasSize.isEmpty
      ? null
      : _view.toPlan(Offset(_canvasSize.width / 2, _canvasSize.height / 2));

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
  Future<void> _openPropertyRoomInventory(
    String roomId,
    String roomName,
  ) async {
    final client = _propertyPlannerClient;
    final session = _accountSession;
    final planId = _hostedPlan?.id;
    final levelId = _levelId;
    final propertyContextRevision = _propertyContextChanges.value;
    final accountSessionRevision = _accountSessionChanges.value;
    if (client == null ||
        session == null ||
        planId == null ||
        levelId == null) {
      return;
    }
    bool isCurrent() =>
        mounted &&
        identical(_accountSession, session) &&
        _accountSessionChanges.value == accountSessionRevision &&
        _propertyContextChanges.value == propertyContextRevision &&
        _hostedPlan?.id == planId &&
        !_dirty &&
        _levelId == levelId &&
        _derived.roomRecords.any(
          (record) => record.id == roomId && record.isAssociated,
        );
    await showDialog<void>(
      context: context,
      builder: (_) => PropertyRoomInventoryDialog(
        client: client,
        session: session,
        planId: planId,
        roomId: roomId,
        roomName: roomName,
        isContextCurrent: isCurrent,
        sessionChanges: _accountSessionChanges,
        contextChanges: _propertyContextChanges,
      ),
    );
  }

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
                _userConfig.unitSystem.area(
                  _derived.rooms[i].areaMm2,
                  metricText: _derived.rooms[i].displayAreaM2,
                ),
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
              IconButton(
                key: ValueKey('paint-room-$i'),
                tooltip: _derived.rooms[i].recordId == null
                    ? 'Name this room to paint its walls'
                    : 'Paint walls',
                icon: const Icon(Icons.format_paint_outlined, size: 18),
                onPressed: _derived.rooms[i].recordId == null
                    ? null
                    : () => _paintRoom(_derived.rooms[i]),
              ),
              if (_propertyPlannerClient != null &&
                  _hostedPlan != null &&
                  _derived.rooms[i].recordId != null &&
                  _derived.roomRecords.any(
                    (record) =>
                        record.id == _derived.rooms[i].recordId &&
                        record.isAssociated &&
                        record.name != null &&
                        record.name!.trim().isNotEmpty,
                  ))
                IconButton(
                  key: ValueKey('room-homehero-${_derived.rooms[i].recordId}'),
                  tooltip: _dirty
                      ? 'Save the online plan before linking inventory'
                      : 'HomeHero inventory and location link',
                  icon: const Icon(Icons.inventory_2_outlined, size: 18),
                  onPressed: _dirty || _accountSession == null
                      ? null
                      : () => _openPropertyRoomInventory(
                          _derived.rooms[i].recordId!,
                          _derived.rooms[i].label ?? 'Room',
                        ),
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

  List<Widget> _staircasePanel(ThemeData theme) => [
    const Divider(height: 24),
    Text('Stairs', style: theme.textTheme.titleSmall),
    if (_derived.construction.every((item) => item.kind != 'staircase'))
      const Padding(
        padding: EdgeInsets.symmetric(vertical: 8),
        child: Text('Use Stairs to draw down to the level below.'),
      ),
    for (final item in _derived.construction.where(
      (item) => item.kind == 'staircase',
    ))
      ExpansionTile(
        // Expansion stores a bool; the inspector and text scrollables store
        // doubles. Separate PageStorage paths prevent their values colliding.
        key: PageStorageKey('stair-card-${item.id}'),
        initiallyExpanded: _expandedStaircaseId == item.id,
        maintainState: true,
        childrenPadding: const EdgeInsets.symmetric(vertical: 8),
        leading: const Icon(Icons.stairs),
        title: Text(item.name),
        subtitle: Text(item.direction),
        onExpansionChanged: (expanded) =>
            setState(() => _expandedStaircaseId = expanded ? item.id : null),
        children: [
          if (item.staircase != null)
            StaircasePropertiesForm(
              key: ValueKey('stair-properties-$_handle-$_levelId-${item.id}'),
              name: item.name,
              properties: item.staircase!,
              revision: _documentRevision,
              units: _userConfig.unitSystem,
              enabled: !_fileActionPending,
              liveEdits: _livePropertyEdits,
              onChanged: (changes) =>
                  _gesture({
                    'kind': 'setStaircase',
                    'staircaseId': item.id,
                    ...changes,
                  }, 'edit staircase') ==
                  ok,
            ),
          Align(
            alignment: Alignment.centerRight,
            child: Tooltip(
              message: 'Remove stairs; keep the independent floor opening',
              child: TextButton.icon(
                icon: const Icon(Icons.delete_outline, size: 18),
                label: const Text('Remove stairs'),
                onPressed: () {
                  _livePropertyEdits.cancel();
                  _gesture({
                    'kind': 'deleteStaircase',
                    'staircaseId': item.id,
                  }, 'remove staircase');
                },
              ),
            ),
          ),
        ],
      ),
  ];

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
    if (!_acceptAssetPose()) return invalidArgument;
    final bridge = _bridge;
    if (bridge == null || _handle == 0) return invalidArgument;
    try {
      final code = bridge.edit(_handle, jsonEncode(request)).code;
      if (code == ok) {
        _setStatus('$what accepted', isError: false);
      } else {
        final detail = _readableError(_lastError(bridge));
        const levelDeletionPrefix =
            'command rejected: level deletion rejected: ';
        if (request['kind'] == 'deleteLevel' &&
            code == rejected &&
            detail.startsWith(levelDeletionPrefix)) {
          final reason = detail
              .substring(levelDeletionPrefix.length)
              .replaceFirst(RegExp(r'^level '), '');
          _setStatus('Cannot delete level: $reason', isError: true);
        } else {
          _setStatus(
            '$what refused: ${_describe(code)} — $detail',
            isError: true,
          );
        }
      }
      return code;
    } finally {
      _refresh();
    }
  }

  Widget _controls(BuildContext context) {
    final selectedAsset = _activeAssets
        .where((a) => a.id == _selected)
        .firstOrNull;
    final theme = Theme.of(context);
    final flags = _levelView.visibility;
    final selected = _walls.where((wall) => wall.id == _selected).firstOrNull;
    final selectedWalls = _walls
        .where((wall) => _selection.ids.contains(wall.id))
        .toList();
    final bulkWalls =
        selectedWalls.length > 1 &&
        selectedWalls.length == _selection.ids.length;
    final selectedOpening = _derived.openings
        .where((o) => o.id == _selected)
        .firstOrNull;
    final selectedStair = _derived.construction
        .where((item) => item.kind == 'staircase' && item.id == _selected)
        .firstOrNull;
    final floorPanel = ListView(
      key: const PageStorageKey('floor-inspector'),
      padding: const EdgeInsets.all(16),
      children: [
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
        // The level rows are DRAGGABLE (issue 0115): the panel presents the
        // document-carried list order, and a drag submits one whole-list
        // `orderLevels` gesture. Everything else here keeps its proven
        // shape — a plain list rather than radios for the active level, and
        // the tier/visibility menus below. The nested shrink-wrap list is
        // non-scrolling (its parent ListView owns the scroll), and the drag
        // only starts from the explicit handle so taps stay activation.
        ReorderableListView(
          shrinkWrap: true,
          buildDefaultDragHandles: false,
          physics: const NeverScrollableScrollPhysics(),
          padding: EdgeInsets.zero,
          header: const Text(
            'Drag the handle to reorder floors',
            style: TextStyle(fontSize: 11, height: 1.5),
          ),
          onReorderItem: _reorderLevels,
          children: [
            // A plain list rather than radios: `RadioListTile`'s group API is
            // deprecated in this Flutter, and a selectable row says the same
            // thing without adopting a widget that is on its way out. The
            // PANEL lists the document-carried order; every elevation-based
            // behaviour keeps reading `_levels`.
            for (final level in _levelView.panelLevels)
              ListTile(
                key: ValueKey('level-${level.id}'),
                dense: true,
                contentPadding: EdgeInsets.zero,
                selected:
                    level.id == _levelId ||
                    _highlightedWarningIds.contains(level.id),
                selectedTileColor: _highlightedWarningIds.contains(level.id)
                    ? Colors.amber.shade200
                    : null,
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
                    ReorderableDragStartListener(
                      key: ValueKey('level-drag-${level.id}'),
                      // The row index in the panel's presentation order,
                      // which onReorder maps back through panelLevels.
                      index: _levelView.panelLevels.indexOf(level),
                      child: const Icon(Icons.drag_handle, size: 18),
                    ),
                    PopupMenuButton<String>(
                      key: ValueKey('level-options-${level.id}'),
                      tooltip: 'Edit options for ${level.name}',
                      icon: const Icon(Icons.more_vert, size: 18),
                      onSelected: (action) {
                        switch (action) {
                          case 'properties':
                            _editLevelProperties(level);
                            break;
                          case 'delete':
                            _deleteLevel(level.id);
                            break;
                        }
                      },
                      itemBuilder: (_) => [
                        PopupMenuItem(
                          key: ValueKey('edit-level-${level.id}'),
                          value: 'properties',
                          child: const Text('Edit properties'),
                        ),
                        if (level.id == _levelId) ...[
                          const PopupMenuDivider(),
                          const PopupMenuItem(
                            key: ValueKey('delete-active-level'),
                            value: 'delete',
                            child: Text('Delete floor'),
                          ),
                        ],
                      ],
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
                      onSelected: (value) =>
                          _setLevelVisibility(level.id, value),
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
          ],
        ),
        const Divider(height: 32),
        Text('On this level', style: theme.textTheme.titleSmall),
        const SizedBox(height: 8),
        _counter('Walls', '${_walls.length}'),
        _counter('Openings', '${_derived.openings.length}'),
        _counter('Rooms', '${_derived.rooms.length}'),
        _counter(
          'Floor area',
          _userConfig.unitSystem.area(_derived.floorAreaMm2),
        ),
        ..._staircasePanel(theme),
        ..._roomPanel(theme),
      ],
    );
    final selectionPanel = ListView(
      key: const PageStorageKey('selection-inspector'),
      controller: _selectionScroll,
      padding: const EdgeInsets.all(16),
      children: [
        _counter('Selected', '${_selection.ids.length}'),
        if (!bulkWalls && _selection.ids.isNotEmpty && _selected == null) ...[
          const Text('No primary selected'),
          Text(
            'Click an object to make it the primary before editing or deleting.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
        ],
        if (bulkWalls) ...[
          Text('${selectedWalls.length} walls selected'),
          OutlinedButton.icon(
            key: const ValueKey('edit-walls-dimensions'),
            onPressed: () => _editWallsProperties(selectedWalls),
            icon: const Icon(Icons.tune, size: 18),
            label: const Text('Edit height and thickness'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('edit-walls-colors'),
            onPressed: () => _editWallColors(selectedWalls),
            icon: const Icon(Icons.palette_outlined, size: 18),
            label: const Text('Edit color'),
          ),
        ] else if (_joins.containsKey(_selected)) ...[
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
        ] else if (selectedStair != null) ...[
          Text(selectedStair.name),
          const Text(
            'Drag to move; use side handles for width, or end handles for length and angle. Hold Ctrl for 15° increments. The floor opening follows.',
          ),
          Tooltip(
            message: 'Delete stairs; keep the independent floor opening',
            child: OutlinedButton.icon(
              key: const ValueKey('delete-staircase'),
              onPressed: _fileActionPending ? null : _deleteSelected,
              icon: const Icon(Icons.delete_outline, size: 18),
              label: const Text('Delete stairs'),
            ),
          ),
        ] else if (selectedAsset != null) ...[
          Text(selectedAsset.name),
          if (_poseAssetId == selectedAsset.id)
            AssetPoseControls(
              units: _userConfig.unitSystem,
              key: ValueKey('pose-${selectedAsset.id}'),
              initial: _poseInitial!,
              rotationZDeg: _poseDraft?[5],
              onValidityChanged: (valid) =>
                  setState(() => _poseDraftValid = valid),
              onPreview: _previewAssetPose,
              onApply: _applyAssetPose,
              onCancel: _cancelAssetPose,
            )
          else
            OutlinedButton.icon(
              key: const ValueKey('pose-asset'),
              onPressed: () => _rotateAsset(selectedAsset),
              icon: const Icon(Icons.threed_rotation, size: 18),
              label: const Text('Position and rotate in 3D'),
            ),
          if (_modelPreviewErrors[selectedAsset.previewKey] case final error?)
            Text(
              'Model preview unavailable: $error',
              style: theme.textTheme.bodySmall,
            ),
          _counter(
            'Width',
            _userConfig.unitSystem.length(selectedAsset.widthMm),
          ),
          _counter(
            'Depth',
            _userConfig.unitSystem.length(selectedAsset.depthMm),
          ),
          if (selectedAsset.heightMm != null)
            _counter(
              'Height',
              _userConfig.unitSystem.length(selectedAsset.heightMm!),
            ),
          _counter(
            'Rotation',
            '${selectedAsset.rotationDeg.toStringAsFixed(1)}°',
          ),
          OutlinedButton.icon(
            key: const ValueKey('edit-asset'),
            onPressed: _poseAssetId != null
                ? null
                : () => _editAsset(selectedAsset),
            icon: const Icon(Icons.tune, size: 18),
            label: const Text('Edit model properties'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('edit-asset-colors'),
            onPressed: _poseAssetId != null
                ? null
                : () => _editAssetColors(selectedAsset),
            icon: const Icon(Icons.palette_outlined, size: 18),
            label: const Text('Edit colors'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('resize-asset'),
            onPressed: _poseAssetId != null
                ? null
                : () => _resizeAsset(selectedAsset),
            icon: const Icon(Icons.open_in_full, size: 18),
            label: const Text('Resize model'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('delete-asset'),
            onPressed: _poseAssetId != null ? null : _deleteSelected,
            icon: const Icon(Icons.delete_outline, size: 18),
            label: const Text('Delete model'),
          ),
        ] else if (selectedOpening != null) ...[
          Text(selectedOpening.isDoor ? 'Door' : 'Window'),
          if (selectedOpening.isDoor) ...[
            DropdownButtonFormField<String>(
              key: ValueKey(
                'door-style-${selectedOpening.id}-${selectedOpening.doorStyle}',
              ),
              initialValue: selectedOpening.doorStyle,
              decoration: const InputDecoration(labelText: 'Style'),
              isExpanded: true,
              items: const [
                DropdownMenuItem(value: 'solid', child: Text('Solid')),
                DropdownMenuItem(
                  value: 'glass',
                  child: Text('Glass (transparent)'),
                ),
              ],
              onChanged: (value) {
                if (value != null)
                  _gesture({
                    'kind': 'setDoorAppearance',
                    'openingId': selectedOpening.id,
                    'color': selectedOpening.color,
                    'doorStyle': value,
                  }, 'Change door style');
              },
            ),
            OutlinedButton.icon(
              key: const ValueKey('edit-door-color'),
              onPressed: () => _editDoorColor(selectedOpening),
              icon: const Icon(Icons.palette_outlined, size: 18),
              label: const Text('Edit color'),
            ),
          ],
          DropdownButtonFormField<String>(
            key: const ValueKey('opening-mechanism'),
            isExpanded: true,
            initialValue:
                selectedOpening.mechanism ??
                (selectedOpening.isDoor ? 'doorHinged' : 'windowSideHung'),
            decoration: const InputDecoration(labelText: 'Type'),
            items: [
              for (final (value, label)
                  in selectedOpening.isDoor
                      ? [
                          ('doorHinged', 'Hinged door'),
                          ('doorSliding', 'Sliding door'),
                        ]
                      : [
                          ('windowSideHung', 'Side-hung window'),
                          ('windowDoubleSwing', 'Double-swing window'),
                          ('windowLift', 'Lift window'),
                        ])
                DropdownMenuItem(value: value, child: Text(label)),
            ],
            onChanged: (value) {
              if (value != null)
                _gesture({
                  'kind': 'setOpening',
                  'openingId': selectedOpening.id,
                  'mechanism': value,
                }, 'change opening type');
            },
          ),
          if (selectedOpening.mechanism == 'doorHinged' ||
              selectedOpening.mechanism == 'windowSideHung' ||
              selectedOpening.mechanism == null)
            DropdownButtonFormField<String>(
              key: const ValueKey('opening-hinge'),
              isExpanded: true,
              initialValue: selectedOpening.hingeEnd ?? 'nearStart',
              decoration: const InputDecoration(labelText: 'Hinge side'),
              items: const [
                DropdownMenuItem(
                  value: 'nearStart',
                  child: Text('At wall start'),
                ),
                DropdownMenuItem(value: 'nearEnd', child: Text('At wall end')),
              ],
              onChanged: (value) {
                if (value != null)
                  _gesture({
                    'kind': 'setOpening',
                    'openingId': selectedOpening.id,
                    'hinge': value,
                  }, 'change hinge side');
              },
            ),
          if (selectedOpening.isDoor &&
              selectedOpening.mechanism != 'doorSliding')
            DropdownButtonFormField<String>(
              key: const ValueKey('opening-swing-side'),
              isExpanded: true,
              initialValue: selectedOpening.opensToward ?? 'Left',
              decoration: const InputDecoration(
                labelText: 'Swings toward wall side',
              ),
              items: const [
                DropdownMenuItem(value: 'Left', child: Text('Left side')),
                DropdownMenuItem(value: 'Right', child: Text('Right side')),
              ],
              onChanged: (value) {
                if (value != null)
                  _gesture({
                    'kind': 'setOpening',
                    'openingId': selectedOpening.id,
                    'opensToward': value,
                  }, 'change swing side');
              },
            ),
          _counter(
            'Width',
            selectedOpening.widthMm == null
                ? '—'
                : _userConfig.unitSystem.length(selectedOpening.widthMm!),
          ),
          _counter(
            'Height',
            selectedOpening.heightMm == null
                ? '—'
                : _userConfig.unitSystem.length(selectedOpening.heightMm!),
          ),
          _counter(
            'Sill',
            selectedOpening.sillMm == null
                ? '—'
                : _userConfig.unitSystem.length(selectedOpening.sillMm!),
          ),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            key: const ValueKey('edit-opening-size'),
            onPressed: selectedOpening.widthMm == null
                ? null
                : () => _editOpeningProperties(selectedOpening),
            icon: const Icon(Icons.tune, size: 18),
            label: const Text('Edit opening size'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('delete-opening'),
            onPressed: _deleteSelected,
            icon: const Icon(Icons.delete_outline, size: 18),
            label: const Text('Delete opening'),
          ),
        ] else if (selected == null)
          Text(
            'Click a wall to see its dimensions. Drag its ends to reshape it.',
            style: theme.textTheme.bodySmall,
          )
        else ...[
          _counter('Length', _userConfig.unitSystem.length(selected.lengthMm)),
          _counter(
            'Thickness',
            _userConfig.unitSystem.length(selected.thicknessMm),
          ),
          _counter('Height', _userConfig.unitSystem.length(selected.heightMm)),
          const SizedBox(height: 8),
          OutlinedButton.icon(
            key: const ValueKey('edit-wall-dimensions'),
            onPressed: () => _editWallProperties(selected),
            icon: const Icon(Icons.tune, size: 18),
            label: const Text('Edit dimensions'),
          ),
          OutlinedButton.icon(
            key: const ValueKey('edit-wall-color'),
            onPressed: () => _editWallColors([selected]),
            icon: const Icon(Icons.palette_outlined, size: 18),
            label: const Text('Edit color'),
          ),
          OutlinedButton.icon(
            onPressed: _deleteSelected,
            icon: const Icon(Icons.delete_outline, size: 18),
            label: const Text('Delete wall'),
          ),
        ],
        const Divider(height: 32),
        const EditorWorkflowHints(),
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
    return Column(
      children: [
        Padding(
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
          child: Align(
            alignment: Alignment.centerLeft,
            child: Text('Your plan', style: theme.textTheme.titleLarge),
          ),
        ),
        TabBar(
          controller: _inspectorTabs,
          tabs: const [
            Tab(key: ValueKey('inspector-selection-tab'), text: 'Selection'),
            Tab(key: ValueKey('inspector-floor-tab'), text: 'Floor'),
          ],
        ),
        // Keep both lists mounted so tab changes preserve draft placement and scroll.
        Expanded(
          child: IndexedStack(
            index: _inspectorTabs.index,
            children: [selectionPanel, floorPanel],
          ),
        ),
      ],
    );
  }

  void _inspectorTabChanged() {
    if (mounted) setState(() {});
  }

  void _showSelectionInspector() {
    _inspectorTabs.index = 0;
    if (_selectionScroll.hasClients) _selectionScroll.jumpTo(0);
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
