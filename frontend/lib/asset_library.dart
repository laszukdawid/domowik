import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart' show ScrollCacheExtent;
import 'package:flutter/services.dart';

import 'measurement_units.dart';
import 'asset_thumbnails.dart';
import 'user_config.dart';
import 'platform/asset_scan.dart';
import 'platform/host.dart';

export 'platform/asset_scan.dart' show scanAssetDirectories;

/// Named rectangle supplied by native code; all coordinates are plan mm.
class AssetFootprint {
  AssetFootprint.fromJson(Map<String, Object?> j)
    : id = j['id'] as String,
      levelId = j['levelId'] as String,
      digest = j['digest'] as String,
      name = j['name'] as String,
      position = _point(j['positionMm']),
      widthMm = ((j['dimensionsMm'] as List)[0] as num).toDouble(),
      depthMm = ((j['dimensionsMm'] as List)[1] as num).toDouble(),
      heightMm = (j['heightMm'] as num?)?.toDouble(),
      rotationDeg = (j['rotationDeg'] as num).toDouble(),
      elevationMm = ((j['pose3d'] as List?)?[0] as num?)?.toDouble() ?? 0,
      rotationXDeg = ((j['pose3d'] as List?)?[1] as num?)?.toDouble() ?? 0,
      rotationYDeg = ((j['pose3d'] as List?)?[2] as num?)?.toDouble() ?? 0,
      corners = (j['cornersMm'] as List).map(_point).toList();

  final String id, levelId, name, digest;
  final Offset position;
  final double widthMm, depthMm, rotationDeg;
  final double elevationMm, rotationXDeg, rotationYDeg;
  final double? heightMm;
  final List<Offset> corners;
  static Offset _point(Object? v) {
    final p = v as List;
    return Offset((p[0] as num).toDouble(), (p[1] as num).toDouble());
  }
}

/// Placement resolution for asset instances: X, Y and elevation are whole
/// millimetres. Screen-to-plan conversion and typed input otherwise carry
/// full binary64 digits, which are numeric noise here, never a placed
/// feature; every position the editor shows or sends resolves through
/// [roundPositionMm]. Half away from zero, and idempotent on whole values,
/// so committers downstream cannot disagree about a rounded value.
double roundPositionMm(double value) => value.roundToDouble();

/// Field text for one whole-millimetre position component. `-0` formats
/// with a sign, so zero prints unsigned.
String formatPositionMm(double value) {
  final rounded = roundPositionMm(value);
  return rounded == 0 ? '0' : rounded.toStringAsFixed(0);
}

class LibraryModel {
  const LibraryModel({
    required this.name,
    this.file,
    this.digest,
    this.sourceDirectory,
    this.sourceDescription,
  });
  final String name;
  final String? file, digest, sourceDescription;
  final String? sourceDirectory;
}

class AssetChoice {
  const AssetChoice(this.model);
  final LibraryModel model;
}

class DirectoryModels {
  const DirectoryModels(this.models, this.diagnostics);
  final List<LibraryModel> models;
  final List<String> diagnostics;
}

/// Current embedded models and any non-fatal directory import diagnostics.
class AssetDirectoryImport {
  const AssetDirectoryImport(this.embedded, this.diagnostics);

  final List<LibraryModel> embedded;
  final List<String> diagnostics;
}

class AssetLibraryDialog extends StatefulWidget {
  const AssetLibraryDialog({
    super.key,
    required this.directories,
    required this.embedded,
    required this.onAttach,
    required this.onReplace,
    this.onUpload,
    this.onUploadDirectory,
    this.directoriesAvailable = hostHasFilesystem,
    this.onPreview,
    this.initialView = AssetLibraryView.list,
    this.onViewChanged,
  });
  final List<String> directories;
  final List<LibraryModel> embedded;
  final FutureOr<List<String>?> Function(String) onAttach;
  final FutureOr<List<String>?> Function(String, String) onReplace;

  /// Asks the user for model files and embeds them in the plan, returning the
  /// plan's embedded models afterwards, or null if nothing was added. Offered
  /// where directories are not (the browser, issue 0088).
  final Future<List<LibraryModel>?> Function()? onUpload;

  /// Recursively imports supported model files from a user-selected directory.
  final Future<AssetDirectoryImport?> Function()? onUploadDirectory;

  /// Whether directory libraries can be attached on this host.
  final bool directoriesAvailable;

  /// Read-only derived PNG generation. The dialog schedules only displayed
  /// entries and never imports a file just to show it.
  final Future<Uint8List> Function(LibraryModel)? onPreview;
  final AssetLibraryView initialView;
  final ValueChanged<AssetLibraryView>? onViewChanged;

  @override
  State<AssetLibraryDialog> createState() => _AssetLibraryDialogState();
}

class _AssetLibraryDialogState extends State<AssetLibraryDialog> {
  final _directory = TextEditingController();
  final _search = TextEditingController();
  List<LibraryModel> _models = [];
  List<String> _diagnostics = [];
  LibraryModel? _selected;
  bool _loading = true;
  bool _picking = false;
  bool _savingPath = false;
  bool _showPathEditor = false;
  String? _previousDirectory;
  late List<String> _directories;
  bool get _busy => _loading || _picking || _savingPath;
  late List<LibraryModel> _embedded = widget.embedded;
  static const _files = MethodChannel('invalid.domowik-spike/files');
  int _scanGeneration = 0;
  AssetThumbnailCache _thumbnails = AssetThumbnailCache();
  late AssetLibraryView _view = widget.initialView;

  @override
  void initState() {
    super.initState();
    _directories = List.of(widget.directories);
    _scan(widget.directories);
  }

  Future<void> _scan(List<String> directories) async {
    final generation = ++_scanGeneration;
    setState(() {
      _thumbnails.dispose();
      _thumbnails = AssetThumbnailCache();
      _loading = true;
      _directories = List.of(directories);
    });
    final result = await scanAssetDirectories(directories);
    if (!mounted || generation != _scanGeneration) return;
    setState(() {
      _models = [...result.models, ..._embedded];
      _diagnostics = result.diagnostics;
      _selected = null;
      _loading = false;
    });
  }

  @override
  void dispose() {
    _thumbnails.dispose();
    for (final c in [_directory, _search]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = _search.text.toLowerCase().replaceAll('\\', '/');
    bool matches(String? value) =>
        value != null &&
        value.toLowerCase().replaceAll('\\', '/').contains(query);
    final models = _models
        .where(
          (m) =>
              matches(m.name) ||
              matches(m.file) ||
              matches(m.sourceDirectory) ||
              matches(m.sourceDescription),
        )
        .toList();
    return AlertDialog(
      title: const Text('Plan asset library'),
      content: SizedBox(
        width: 760,
        height: math.min(
          560,
          math.max(240, MediaQuery.sizeOf(context).height - 240),
        ),
        child: Column(
          children: [
            const Text(
              'Add GLB, OBJ or ZIP models. Defined dimensions are preserved; models with unknown units keep their proportions. Resize after placement.',
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              children: [
                if (widget.onUpload != null)
                  OutlinedButton.icon(
                    key: const ValueKey('upload-asset-model'),
                    onPressed: _busy ? null : _upload,
                    icon: const Icon(Icons.upload_file),
                    label: const Text('Upload model'),
                  ),
                if (widget.onUploadDirectory != null)
                  OutlinedButton.icon(
                    key: const ValueKey('upload-asset-directory'),
                    onPressed: _busy ? null : _uploadDirectory,
                    icon: const Icon(Icons.drive_folder_upload_outlined),
                    label: const Text('Import folder'),
                  ),
                if (widget.directoriesAvailable) ...[
                  OutlinedButton.icon(
                    key: const ValueKey('attach-asset-directory'),
                    onPressed: _busy ? null : () => _pickDirectory(),
                    icon: const Icon(Icons.folder_open),
                    label: const Text('Add directory'),
                  ),
                  TextButton(
                    key: const ValueKey('enter-asset-path'),
                    onPressed: _busy ? null : () => _editPath(null),
                    child: const Text('Enter path'),
                  ),
                ],
              ],
            ),
            if (_directories.isNotEmpty)
              SizedBox(
                height: 76,
                child: ListView(
                  children: [
                    for (final directory in _directories)
                      ListTile(
                        dense: true,
                        contentPadding: EdgeInsets.zero,
                        title: Text(
                          directory,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                        ),
                        trailing: Row(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            IconButton(
                              key: ValueKey('edit-directory:$directory'),
                              tooltip: 'Edit directory path',
                              onPressed: _busy
                                  ? null
                                  : () => _editPath(directory),
                              icon: const Icon(Icons.edit_outlined),
                            ),
                            IconButton(
                              key: ValueKey('browse-directory:$directory'),
                              tooltip: 'Choose replacement directory',
                              onPressed: _busy
                                  ? null
                                  : () => _pickDirectory(previous: directory),
                              icon: const Icon(Icons.folder_open),
                            ),
                          ],
                        ),
                      ),
                  ],
                ),
              ),
            if (_showPathEditor)
              Row(
                children: [
                  Expanded(
                    child: TextField(
                      key: const ValueKey('asset-directory'),
                      controller: _directory,
                      enabled: !_picking,
                      decoration: InputDecoration(
                        labelText: _previousDirectory == null
                            ? 'Asset directory path'
                            : 'Replacement directory path',
                      ),
                      onSubmitted: (_) {
                        if (!_busy)
                          _attach(
                            _directory.text.trim(),
                            previous: _previousDirectory,
                          );
                      },
                    ),
                  ),
                  IconButton(
                    key: const ValueKey('apply-asset-path'),
                    tooltip: _previousDirectory == null
                        ? 'Add path'
                        : 'Save path',
                    onPressed: _busy
                        ? null
                        : () => _attach(
                            _directory.text.trim(),
                            previous: _previousDirectory,
                          ),
                    icon: const Icon(Icons.check),
                  ),
                  IconButton(
                    tooltip: 'Cancel path edit',
                    onPressed: _picking
                        ? null
                        : () => setState(() => _showPathEditor = false),
                    icon: const Icon(Icons.close),
                  ),
                ],
              ),
            Row(
              children: [
                Expanded(
                  child: TextField(
                    key: const ValueKey('asset-search'),
                    controller: _search,
                    decoration: const InputDecoration(
                      labelText: 'Search by name or path',
                    ),
                    onChanged: (_) => setState(() {}),
                  ),
                ),
                const SizedBox(width: 8),
                SegmentedButton<AssetLibraryView>(
                  showSelectedIcon: false,
                  segments: const [
                    ButtonSegment(
                      value: AssetLibraryView.list,
                      icon: Icon(Icons.view_list),
                      tooltip: 'List view',
                    ),
                    ButtonSegment(
                      value: AssetLibraryView.grid,
                      icon: Icon(Icons.grid_view),
                      tooltip: 'Grid view',
                    ),
                  ],
                  selected: {_view},
                  onSelectionChanged: (views) {
                    final view = views.single;
                    setState(() => _view = view);
                    widget.onViewChanged?.call(view);
                  },
                ),
              ],
            ),
            if (_diagnostics.isNotEmpty)
              Padding(
                padding: const EdgeInsets.symmetric(vertical: 4),
                child: Text(
                  _diagnostics.join('\n'),
                  maxLines: 3,
                  overflow: TextOverflow.ellipsis,
                  style: TextStyle(color: Theme.of(context).colorScheme.error),
                ),
              ),
            Expanded(
              child: _loading
                  ? const Center(child: CircularProgressIndicator())
                  : models.isEmpty
                  ? Center(
                      child: Text(
                        _models.isNotEmpty
                            ? 'No models match your search.'
                            : widget.directoriesAvailable
                            ? 'No models found. Add a directory to get started.'
                            : 'No models yet. Upload a model to get started.',
                      ),
                    )
                  : _view == AssetLibraryView.grid
                  ? LayoutBuilder(
                      builder: (_, constraints) => GridView.builder(
                        key: const ValueKey('asset-grid'),
                        scrollCacheExtent: const ScrollCacheExtent.pixels(0),
                        gridDelegate: SliverGridDelegateWithMaxCrossAxisExtent(
                          maxCrossAxisExtent: 220,
                          mainAxisExtent: math.min(
                            190,
                            math.max(80, constraints.maxHeight),
                          ),
                          crossAxisSpacing: 8,
                          mainAxisSpacing: 8,
                        ),
                        itemCount: models.length,
                        itemBuilder: (_, i) => _gridTile(models[i]),
                      ),
                    )
                  : ListView.builder(
                      key: const ValueKey('asset-list'),
                      scrollCacheExtent: const ScrollCacheExtent.pixels(0),
                      itemCount: models.length,
                      itemBuilder: (_, i) {
                        final m = models[i];
                        final source =
                            m.file ??
                            m.sourceDescription ??
                            'Embedded in this plan';
                        return ListTile(
                          key: ValueKey<String>(m.file ?? m.digest!),
                          dense: true,
                          leading: SizedBox.square(
                            dimension: 48,
                            child: _thumbnail(m),
                          ),
                          title: Text(
                            m.name,
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
                          ),
                          subtitle: Tooltip(
                            message: source,
                            child: Text(
                              source,
                              maxLines: 1,
                              overflow: TextOverflow.ellipsis,
                            ),
                          ),
                          selected: identical(m, _selected),
                          onTap: () => setState(() => _selected = m),
                        );
                      },
                    ),
            ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: _picking ? null : () => Navigator.pop(context),
          child: const Text('Close'),
        ),
        FilledButton(
          key: const ValueKey('choose-asset'),
          onPressed: _selected == null || _busy
              ? null
              : () => Navigator.pop(context, AssetChoice(_selected!)),
          child: const Text('Place in plan'),
        ),
      ],
    );
  }

  Widget _thumbnail(LibraryModel model) => AssetThumbnail(
    cache: _thumbnails,
    assetKey: model.file ?? model.digest!,
    name: model.name,
    load: widget.onPreview == null ? null : () => widget.onPreview!(model),
  );

  Widget _gridTile(LibraryModel model) {
    final selected = identical(model, _selected);
    final colors = Theme.of(context).colorScheme;
    final source =
        model.file ?? model.sourceDescription ?? 'Embedded in this plan';
    return Semantics(
      selected: selected,
      child: Card(
        key: ValueKey<String>(model.file ?? model.digest!),
        color: selected ? colors.secondaryContainer : null,
        shape: RoundedRectangleBorder(
          borderRadius: BorderRadius.circular(8),
          side: BorderSide(
            color: selected ? colors.primary : colors.outlineVariant,
            width: selected ? 2 : 1,
          ),
        ),
        clipBehavior: Clip.antiAlias,
        child: InkWell(
          onTap: () => setState(() => _selected = model),
          child: Padding(
            padding: const EdgeInsets.all(8),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Expanded(child: _thumbnail(model)),
                const SizedBox(height: 6),
                Tooltip(
                  message: '${model.name}\n$source',
                  child: Text(
                    model.name,
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    );
  }

  Future<void> _upload() async {
    final upload = widget.onUpload;
    if (upload == null || _busy) return;
    setState(() => _picking = true);
    try {
      final embedded = await upload();
      if (!mounted || embedded == null) return;
      final known = {for (final m in _embedded) m.digest};
      setState(() {
        _embedded = embedded;
        _models = [..._models.where((m) => m.file != null), ...embedded];
        _selected = embedded
            .where((m) => !known.contains(m.digest))
            .firstOrNull;
        _diagnostics = [];
      });
    } catch (e) {
      if (mounted)
        setState(() => _diagnostics = ['Could not add the model: $e']);
    } finally {
      if (mounted) setState(() => _picking = false);
    }
  }

  Future<void> _uploadDirectory() async {
    final upload = widget.onUploadDirectory;
    if (upload == null || _busy) return;
    setState(() => _picking = true);
    try {
      final result = await upload();
      if (!mounted || result == null) return;
      setState(() {
        _embedded = result.embedded;
        _models = [..._models.where((m) => m.file != null), ...result.embedded];
        _selected = null;
        _diagnostics = result.diagnostics;
      });
    } catch (e) {
      if (mounted) {
        setState(() => _diagnostics = ['Could not import the folder: $e']);
      }
    } finally {
      if (mounted) setState(() => _picking = false);
    }
  }

  void _editPath(String? previous) {
    setState(() {
      _previousDirectory = previous;
      _directory.text = previous ?? '';
      _showPathEditor = true;
    });
  }

  Future<void> _pickDirectory({String? previous}) async {
    if (_busy) return;
    setState(() => _picking = true);
    try {
      final directory = await _files.invokeMethod<String>(
        'pickAssetDirectory',
        previous,
      );
      if (!mounted || directory == null) return;
      await _attach(directory, previous: previous);
    } on MissingPluginException {
      if (mounted)
        setState(
          () => _diagnostics = [
            'Folder picking is unavailable on this platform. Use Enter path.',
          ],
        );
    } on PlatformException catch (e) {
      if (mounted)
        setState(
          () => _diagnostics = [
            'Could not open the folder picker: ${e.message ?? e.code}. You can use Enter path.',
          ],
        );
    } finally {
      if (mounted) setState(() => _picking = false);
    }
  }

  Future<void> _attach(String path, {String? previous}) async {
    if (_savingPath) return;
    setState(() => _savingPath = true);
    List<String>? directories;
    try {
      directories = await (previous == null
          ? widget.onAttach(path)
          : widget.onReplace(previous, path));
    } catch (e) {
      if (mounted)
        setState(
          () => _diagnostics = ['Could not save library preferences: $e'],
        );
      return;
    } finally {
      if (mounted) setState(() => _savingPath = false);
    }
    if (!mounted) return;
    if (directories == null) {
      setState(
        () => _diagnostics = [
          'Could not attach directory. Check the path and permissions.',
        ],
      );
    } else {
      setState(() {
        _showPathEditor = false;
        _previousDirectory = null;
        _directory.clear();
      });
      _scan(directories);
    }
  }
}

/// Numeric asset edits remain one atomic native batch.
class AssetPropertiesDialog extends StatefulWidget {
  const AssetPropertiesDialog({
    super.key,
    required this.asset,
    required this.heightMm,
    this.units = UnitSystem.metric,
  });
  final AssetFootprint asset;
  final double heightMm;
  final UnitSystem units;
  @override
  State<AssetPropertiesDialog> createState() => _AssetPropertiesDialogState();
}

class _AssetPropertiesDialogState extends State<AssetPropertiesDialog> {
  late final _original = [
    roundPositionMm(widget.asset.position.dx),
    roundPositionMm(widget.asset.position.dy),
    widget.asset.widthMm,
    widget.asset.depthMm,
    widget.asset.rotationDeg,
    widget.heightMm,
  ];
  late final List<TextEditingController> _fields = [
    // Positions seed at the editor's whole-millimetre resolution (a drag can
    // store a fraction); dimensions and angles keep their own precision.
    for (final (i, v) in [
      widget.asset.position.dx,
      widget.asset.position.dy,
      widget.asset.widthMm,
      widget.asset.depthMm,
      widget.asset.rotationDeg,
      widget.heightMm,
    ].indexed)
      TextEditingController(
        text: widget.units == UnitSystem.metric
            ? (i < 2 ? formatPositionMm(v) : '$v')
            : i == 4
            ? '$v'
            : widget.units.editLength(_original[i]),
      ),
  ];
  @override
  void dispose() {
    for (final c in _fields) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final values = [
      for (var i = 0; i < _fields.length; i++)
        i == 4
            ? double.tryParse(_fields[i].text)
            : widget.units.parseLength(
                _fields[i].text,
                originalMm: _original[i],
              ),
    ];
    final valid =
        values.every((v) => v != null && v.isFinite) &&
        values[2]! > 0 &&
        values[2]! <= 1000000 &&
        values[3]! > 0 &&
        values[3]! <= 1000000 &&
        values[5]! > 0 &&
        values[5]! <= 1000000;
    return AlertDialog(
      title: Text(widget.asset.name),
      content: SizedBox(
        width: 360,
        child: SingleChildScrollView(
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              for (var i = 0; i < _fields.length; i++)
                TextField(
                  key: ValueKey('asset-property-$i'),
                  controller: _fields[i],
                  decoration: InputDecoration(
                    labelText: [
                      'X (${widget.units.lengthSymbol})',
                      'Y (${widget.units.lengthSymbol})',
                      'Width (${widget.units.lengthSymbol})',
                      'Depth (${widget.units.lengthSymbol})',
                      'Rotation (degrees CCW)',
                      'Height (${widget.units.lengthSymbol})',
                    ][i],
                  ),
                  onChanged: (_) => setState(() {}),
                ),
              if (!valid)
                Text(
                  'Use finite coordinates and positive dimensions up to ${widget.units.length(1000000)}.',
                ),
            ],
          ),
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('apply-asset-properties'),
          onPressed: valid
              ? () => Navigator.pop(context, values.cast<double>())
              : null,
          child: const Text('Apply'),
        ),
      ],
    );
  }
}

/// Shared live placement controls; callbacks own preview/commit transport.
class AssetPoseControls extends StatefulWidget {
  const AssetPoseControls({
    super.key,
    required this.initial,
    required this.onPreview,
    required this.onApply,
    required this.onCancel,
    this.rotationZDeg,
    this.onValidityChanged,
    this.units = UnitSystem.metric,
  });
  final List<double> initial;

  /// Current yaw from direct plan dragging; other draft fields remain intact.
  final double? rotationZDeg;
  final ValueChanged<bool>? onValidityChanged;
  final UnitSystem units;
  final ValueChanged<List<double>> onPreview, onApply;
  final VoidCallback onCancel;
  @override
  State<AssetPoseControls> createState() => _AssetPoseControlsState();
}

class _AssetPoseControlsState extends State<AssetPoseControls> {
  late final _fields = [
    // X, Y and elevation seed at the editor's whole-millimetre resolution;
    // angles keep their own precision.
    for (final (i, v) in widget.initial.indexed)
      TextEditingController(
        text: i < 3
            ? widget.units == UnitSystem.metric
                  ? formatPositionMm(v)
                  : widget.units.editLength(roundPositionMm(v))
            : '$v',
      ),
  ];

  @override
  void didUpdateWidget(AssetPoseControls oldWidget) {
    super.didUpdateWidget(oldWidget);
    final angle = widget.rotationZDeg;
    if (angle != null &&
        angle != oldWidget.rotationZDeg &&
        double.tryParse(_fields[5].text) != angle) {
      _fields[5].text = '$angle';
    }
  }

  List<double>? get values {
    final parsed = [
      for (var i = 0; i < _fields.length; i++)
        i < 3
            ? widget.units.parseLength(
                _fields[i].text,
                originalMm: roundPositionMm(widget.initial[i]),
              )
            : double.tryParse(_fields[i].text),
    ];
    if (parsed.any((v) => v == null || !v.isFinite)) return null;
    // Preview and Apply receive this same list, so a typed fraction commits
    // exactly the whole-millimetre value the 3D preview showed.
    final result = [
      for (final (i, v) in parsed.indexed) i < 3 ? roundPositionMm(v!) : v!,
    ];
    return result.take(3).any((v) => v.abs() > 10000000) ? null : result;
  }

  void changed() {
    setState(() {});
    final v = values;
    widget.onValidityChanged?.call(v != null);
    if (v != null) widget.onPreview(v);
  }

  void step(int i, double delta) {
    final value = i < 3
        ? widget.units.parseLength(_fields[i].text)
        : double.tryParse(_fields[i].text);
    if (value == null || !value.isFinite) return;
    final next = value + delta;
    _fields[i].text = i < 3
        ? widget.units.editLength(roundPositionMm(next))
        : '$next';
    changed();
  }

  @override
  void dispose() {
    for (final field in _fields) {
      field.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final v = values;
    return CallbackShortcuts(
      bindings: {
        const SingleActivator(LogicalKeyboardKey.escape): widget.onCancel,
      },
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          const Text('3D placement · live preview'),
          const Text(
            'Click elsewhere in the plan or Apply to keep changes. '
            'Undo restores the previous pose; Cancel discards the preview.',
          ),
          const Text(
            'Drag the round handle in the 2D plan to rotate around Z. '
            'Hold Shift for 15° steps.',
          ),
          const Text(
            'X then Y then Z rotations, about the center. Elevation is relative to this level.',
          ),
          for (var i = 0; i < 6; i++) ...[
            Row(
              children: [
                Expanded(
                  child: TextField(
                    key: ValueKey('asset-pose-$i'),
                    controller: _fields[i],
                    keyboardType: const TextInputType.numberWithOptions(
                      signed: true,
                      decimal: true,
                    ),
                    decoration: InputDecoration(
                      labelText: [
                        'X (${widget.units.lengthSymbol})',
                        'Y (${widget.units.lengthSymbol})',
                        'Elevation (${widget.units.lengthSymbol})',
                        'Rotate X (degrees)',
                        'Rotate Y (degrees)',
                        'Rotate Z (degrees)',
                      ][i],
                    ),
                    onChanged: (_) => changed(),
                  ),
                ),
                IconButton(
                  tooltip: i < 3
                      ? 'Decrease ${widget.units.length(10)}'
                      : 'Decrease 1 degree',
                  onPressed: () => step(i, i < 3 ? -10 : -1),
                  icon: const Icon(Icons.remove),
                ),
                IconButton(
                  tooltip: i < 3
                      ? 'Increase ${widget.units.length(10)}'
                      : 'Increase 1 degree',
                  onPressed: () => step(i, i < 3 ? 10 : 1),
                  icon: const Icon(Icons.add),
                ),
              ],
            ),
            if (i >= 3)
              Slider(
                key: ValueKey('asset-pose-slider-$i'),
                min: -180,
                max: 180,
                value: (double.tryParse(_fields[i].text) ?? 0).isFinite
                    ? (double.tryParse(_fields[i].text) ?? 0)
                          .clamp(-180, 180)
                          .toDouble()
                    : 0,
                onChanged: (angle) {
                  _fields[i].text = angle.toStringAsFixed(1);
                  changed();
                },
              ),
          ],
          Wrap(
            children: [
              TextButton(
                onPressed: () => step(3, 90),
                child: const Text('X +90°'),
              ),
              TextButton(
                onPressed: () => step(4, 90),
                child: const Text('Y +90°'),
              ),
            ],
          ),
          if (v == null)
            Text(
              'Use finite angles and coordinates within ±${widget.units.length(10000000)}.',
            ),
          Row(
            children: [
              TextButton(
                key: const ValueKey('cancel-asset-pose'),
                onPressed: widget.onCancel,
                child: const Text('Cancel'),
              ),
              FilledButton(
                key: const ValueKey('apply-asset-pose'),
                onPressed: v == null ? null : () => widget.onApply(v),
                child: const Text('Apply'),
              ),
            ],
          ),
        ],
      ),
    );
  }
}

/// A centered resize. Locked proportions scale all axes from the initial size.
class ResizeAssetDialog extends StatefulWidget {
  const ResizeAssetDialog({
    super.key,
    required this.asset,
    required this.heightMm,
    this.preview,
    this.units = UnitSystem.metric,
  });
  final AssetFootprint asset;
  final double heightMm;
  final UnitSystem units;

  /// Borrowed cached art; the owning plan session retains/disposes this image.
  final ui.Image? preview;

  @override
  State<ResizeAssetDialog> createState() => _ResizeAssetDialogState();
}

class _ResizeAssetDialogState extends State<ResizeAssetDialog> {
  late List<double> _ratio = [
    widget.asset.widthMm,
    widget.asset.depthMm,
    widget.heightMm,
  ];
  late final _original = List<double>.of(_ratio);
  late List<double?> _dimensions = List<double?>.of(_ratio);
  late List<double> _lastValid = List<double>.of(_ratio);
  late final _fields = [
    for (final v in _ratio) TextEditingController(text: _format(v)),
  ];
  bool _locked = true;

  String _format(double mm) => widget.units == UnitSystem.metric
      ? mm.toStringAsFixed(1)
      : widget.units.editLength(mm);

  List<double?> get _values => _dimensions;
  bool _valid(List<double?> values) =>
      values.every((v) => v != null && v.isFinite && v > 0 && v <= 1000000);

  void _changed(int axis) {
    final value = widget.units.parseLength(
      _fields[axis].text,
      originalMm: _dimensions[axis],
    );
    _dimensions[axis] = value;
    if (_locked && value != null && value.isFinite && value > 0) {
      for (var i = 0; i < 3; i++) {
        if (i != axis) {
          final scaled = _ratio[i] * value / _ratio[axis];
          _dimensions[i] = scaled;
          _fields[i].text = _format(scaled);
        }
      }
    }
    if (_valid(_dimensions)) _lastValid = _dimensions.cast<double>().toList();
    setState(() {});
  }

  void _formatField(int axis) {
    final value = _dimensions[axis];
    if (value != null && value.isFinite) {
      _fields[axis].text = _format(value);
    }
  }

  @override
  void dispose() {
    for (final field in _fields) {
      field.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final values = _values;
    final valid = _valid(values);
    return AlertDialog(
      constraints: const BoxConstraints(maxWidth: 420),
      insetPadding: const EdgeInsets.symmetric(horizontal: 20, vertical: 24),
      title: Tooltip(
        message: widget.asset.name,
        child: Text(
          'Resize ${widget.asset.name.runes.length > 50 ? '${String.fromCharCodes(widget.asset.name.runes.take(49))}…' : widget.asset.name}',
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
        ),
      ),
      scrollable: true,
      content: SizedBox(
        width: 360,
        child: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            const Align(
              alignment: Alignment.centerLeft,
              child: Text('Size preview'),
            ),
            SizedBox(
              key: const ValueKey('resize-preview'),
              height: 160,
              width: double.infinity,
              child: RepaintBoundary(
                key: const ValueKey('resize-preview-pixels'),
                child: CustomPaint(
                  painter: ResizePreviewPainter(
                    original: _original,
                    dimensions: _lastValid,
                    image: widget.preview,
                    color: Theme.of(context).colorScheme.primary,
                    outline: Theme.of(context).colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ),
            Row(
              children: [
                Expanded(
                  child: Text(
                    widget.preview == null
                        ? 'Top view · footprint'
                        : 'Top view',
                  ),
                ),
                const Text('Height'),
              ],
            ),
            const Align(
              alignment: Alignment.centerLeft,
              child: Text(
                'Dashed outline: original size',
                style: TextStyle(fontSize: 12),
              ),
            ),
            CheckboxListTile(
              key: const ValueKey('resize-lock-proportions'),
              contentPadding: EdgeInsets.zero,
              title: const Text('Keep proportions'),
              value: _locked,
              onChanged: (value) => setState(() {
                _locked = value!;
                if (_locked && valid) _ratio = values.cast<double>().toList();
              }),
            ),
            for (var i = 0; i < 3; i++)
              TextField(
                key: ValueKey('resize-asset-$i'),
                controller: _fields[i],
                decoration: InputDecoration(
                  labelText:
                      '${['Width', 'Depth', 'Height'][i]} (${widget.units.lengthSymbol})',
                ),
                inputFormatters: [
                  TextInputFormatter.withFunction((before, after) {
                    final value = double.tryParse(after.text);
                    if (value != null &&
                        value.isFinite &&
                        (widget.units == UnitSystem.metric
                                ? RegExp(r'\.\d{2,}')
                                : RegExp(r'\.\d{5,}'))
                            .hasMatch(after.text)) {
                      final text = value.toStringAsFixed(
                        widget.units == UnitSystem.metric ? 1 : 4,
                      );
                      return TextEditingValue(
                        text: text,
                        selection: TextSelection.collapsed(offset: text.length),
                      );
                    }
                    return after;
                  }),
                ],
                onChanged: (_) => _changed(i),
                onEditingComplete: () {
                  _formatField(i);
                  FocusScope.of(context).nextFocus();
                },
                onTapOutside: (_) {
                  _formatField(i);
                  FocusScope.of(context).unfocus();
                },
              ),
            if (!valid)
              Text(
                'Use positive dimensions up to ${widget.units.length(1000000)}.',
              ),
          ],
        ),
      ),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context),
          child: const Text('Cancel'),
        ),
        FilledButton(
          key: const ValueKey('apply-asset-resize'),
          onPressed: valid
              ? () => Navigator.pop(context, values.cast<double>())
              : null,
          child: const Text('Apply'),
        ),
      ],
    );
  }
}

/// Display-only comparison at a shared scale per view. Does not derive plan
/// geometry or own image resources. Dimensions are width/depth/height in mm.
class ResizePreviewPainter extends CustomPainter {
  const ResizePreviewPainter({
    required this.original,
    required this.dimensions,
    required this.image,
    required this.color,
    required this.outline,
  });
  final List<double> original, dimensions;
  final ui.Image? image;
  final Color color, outline;

  Rect footprint(Size size, List<double> values) {
    final available = math.max(1.0, size.width - 60);
    final scale = math.min(
      (available - 16) / math.max(original[0] * 1.5, dimensions[0]),
      (size.height - 16) / math.max(original[1] * 1.5, dimensions[1]),
    );
    return Rect.fromCenter(
      center: Offset(available / 2, size.height / 2),
      width: values[0] * scale,
      height: values[1] * scale,
    );
  }

  double heightPixels(Size size, double height) =>
      height * (size.height - 16) / math.max(original[2] * 1.5, dimensions[2]);

  void _dashed(Canvas canvas, Offset from, Offset to, Paint paint) {
    final delta = to - from;
    final length = delta.distance;
    if (length == 0) return;
    for (double d = 0; d < length; d += 8) {
      canvas.drawLine(
        from + delta * (d / length),
        from + delta * (math.min(d + 4, length) / length),
        paint,
      );
    }
  }

  @override
  void paint(Canvas canvas, Size size) {
    final proposed = footprint(size, dimensions);
    final before = footprint(size, original);
    final border = Paint()
      ..color = outline
      ..strokeWidth = 1;
    canvas.drawRect(proposed, Paint()..color = color.withValues(alpha: 0.12));
    if (image case final image?) {
      canvas.drawImageRect(
        image,
        Rect.fromLTWH(0, 0, image.width.toDouble(), image.height.toDouble()),
        proposed,
        Paint()..filterQuality = FilterQuality.medium,
      );
    }
    canvas.drawRect(
      proposed,
      Paint()
        ..color = color
        ..style = PaintingStyle.stroke
        ..strokeWidth = 2,
    );
    _dashed(canvas, before.topLeft, before.topRight, border);
    _dashed(canvas, before.topRight, before.bottomRight, border);
    _dashed(canvas, before.bottomRight, before.bottomLeft, border);
    _dashed(canvas, before.bottomLeft, before.topLeft, border);
    final bottom = size.height - 8;
    final x = size.width - 30;
    final height = heightPixels(size, dimensions[2]);
    canvas.drawRect(
      Rect.fromLTWH(x - 10, bottom - height, 20, height),
      Paint()..color = color.withValues(alpha: 0.5),
    );
    final oldTop = bottom - heightPixels(size, original[2]);
    _dashed(canvas, Offset(x - 20, oldTop), Offset(x + 20, oldTop), border);
    _dashed(canvas, Offset(x - 20, oldTop), Offset(x - 20, bottom), border);
    _dashed(canvas, Offset(x + 20, oldTop), Offset(x + 20, bottom), border);
  }

  @override
  bool shouldRepaint(ResizePreviewPainter old) =>
      image != old.image ||
      color != old.color ||
      outline != old.outline ||
      List.generate(
        3,
        (i) =>
            original[i] != old.original[i] ||
            dimensions[i] != old.dimensions[i],
      ).any((v) => v);
}
