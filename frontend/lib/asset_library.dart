import 'dart:io';
import 'dart:async';
import 'dart:math' as math;
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

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
      corners = (j['cornersMm'] as List).map(_point).toList();

  final String id, levelId, name, digest;
  final Offset position;
  final double widthMm, depthMm, rotationDeg;
  final double? heightMm;
  final List<Offset> corners;
  static Offset _point(Object? v) {
    final p = v as List;
    return Offset((p[0] as num).toDouble(), (p[1] as num).toDouble());
  }
}

class LibraryModel {
  const LibraryModel({
    required this.name,
    this.file,
    this.digest,
    this.sourceDirectory,
  });
  final String name;
  final String? file, digest;
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

/// Filesystem enumeration only: no model reads, extraction or decoding. Symlinks
/// are skipped. Work is capped at 10,000 entries and 64 directory levels; a
/// truncated or inaccessible library has an explicit diagnostic.
Future<DirectoryModels> scanAssetDirectories(List<String> directories) async {
  final models = <LibraryModel>[];
  final diagnostics = <String>[];
  final seen = <String>{};
  var visited = 0;
  Future<void> walk(String path, int depth, String sourceDirectory) async {
    if (!seen.add(path)) return;
    if (depth > 64) {
      diagnostics.add('Directory nesting limit reached: $path');
      return;
    }
    try {
      await for (final entry in Directory(path).list(followLinks: false)) {
        if (++visited > 10000) {
          if (!diagnostics.contains(
            'Library scan limited to 10,000 entries.',
          )) {
            diagnostics.add('Library scan limited to 10,000 entries.');
          }
          return;
        }
        if (entry is Directory) {
          await walk(entry.path, depth + 1, sourceDirectory);
        } else if (entry is File) {
          final name = entry.uri.pathSegments.last;
          final dot = name.lastIndexOf('.');
          if (dot > 0 &&
              {
                'glb',
                'obj',
                'zip',
              }.contains(name.substring(dot + 1).toLowerCase())) {
            models.add(
              LibraryModel(
                name: name.substring(0, dot),
                file: entry.absolute.path,
                sourceDirectory: sourceDirectory,
              ),
            );
          }
        }
      }
    } on FileSystemException catch (e) {
      diagnostics.add('Cannot read $path: ${e.message}');
    }
  }

  for (final directory in directories) {
    if (visited > 10000) break;
    await walk(directory, 0, directory);
  }
  models.sort((a, b) => a.file!.compareTo(b.file!));
  return DirectoryModels(models, diagnostics);
}

class AssetLibraryDialog extends StatefulWidget {
  const AssetLibraryDialog({
    super.key,
    required this.directories,
    required this.embedded,
    required this.onAttach,
    required this.onReplace,
  });
  final List<String> directories;
  final List<LibraryModel> embedded;
  final FutureOr<List<String>?> Function(String) onAttach;
  final FutureOr<List<String>?> Function(String, String) onReplace;

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
  static const _files = MethodChannel('invalid.domowik-spike/files');
  int _scanGeneration = 0;

  @override
  void initState() {
    super.initState();
    _directories = List.of(widget.directories);
    _scan(widget.directories);
  }

  Future<void> _scan(List<String> directories) async {
    final generation = ++_scanGeneration;
    setState(() {
      _loading = true;
      _directories = List.of(directories);
    });
    final result = await scanAssetDirectories(directories);
    if (!mounted || generation != _scanGeneration) return;
    setState(() {
      _models = [...result.models, ...widget.embedded];
      _diagnostics = result.diagnostics;
      _selected = null;
      _loading = false;
    });
  }

  @override
  void dispose() {
    for (final c in [_directory, _search]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final query = _search.text.toLowerCase();
    final models = _models
        .where((m) => m.name.toLowerCase().contains(query))
        .toList();
    return AlertDialog(
      title: const Text('Plan asset library'),
      content: SizedBox(
        width: 620,
        height: 470,
        child: Column(
          children: [
            const Text(
              'Add GLB, OBJ or ZIP models. Defined dimensions are preserved; models with unknown units keep their proportions. Resize after placement.',
            ),
            const SizedBox(height: 12),
            Wrap(
              spacing: 8,
              children: [
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
            TextField(
              controller: _search,
              decoration: const InputDecoration(labelText: 'Search models'),
              onChanged: (_) => setState(() {}),
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
                  ? const Center(
                      child: Text(
                        'No models found. Add a directory to get started.',
                      ),
                    )
                  : ListView.builder(
                      itemCount: models.length,
                      itemBuilder: (_, i) {
                        final m = models[i];
                        return ListTile(
                          key: ValueKey<String>(m.file ?? m.digest!),
                          dense: true,
                          leading: const Icon(Icons.chair_outlined),
                          title: Text(m.name),
                          subtitle: Text(
                            m.file ?? 'Embedded in this plan',
                            maxLines: 1,
                            overflow: TextOverflow.ellipsis,
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
  });
  final AssetFootprint asset;
  final double heightMm;
  @override
  State<AssetPropertiesDialog> createState() => _AssetPropertiesDialogState();
}

class _AssetPropertiesDialogState extends State<AssetPropertiesDialog> {
  late final List<TextEditingController> _fields = [
    for (final v in [
      widget.asset.position.dx,
      widget.asset.position.dy,
      widget.asset.widthMm,
      widget.asset.depthMm,
      widget.asset.rotationDeg,
      widget.heightMm,
    ])
      TextEditingController(text: '$v'),
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
    final values = _fields.map((c) => double.tryParse(c.text)).toList();
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
                      'X (mm)',
                      'Y (mm)',
                      'Width (mm)',
                      'Depth (mm)',
                      'Rotation (degrees CCW)',
                      'Height (mm)',
                    ][i],
                  ),
                  onChanged: (_) => setState(() {}),
                ),
              if (!valid)
                const Text(
                  'Use finite coordinates and positive dimensions up to 1,000,000 mm.',
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

/// A centered resize. Locked proportions scale all axes from the initial size.
class ResizeAssetDialog extends StatefulWidget {
  const ResizeAssetDialog({
    super.key,
    required this.asset,
    required this.heightMm,
    this.preview,
  });
  final AssetFootprint asset;
  final double heightMm;

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
    for (final v in _ratio) TextEditingController(text: v.toStringAsFixed(1)),
  ];
  bool _locked = true;

  List<double?> get _values => _dimensions;
  bool _valid(List<double?> values) =>
      values.every((v) => v != null && v.isFinite && v > 0 && v <= 1000000);

  void _changed(int axis) {
    final value = double.tryParse(_fields[axis].text);
    _dimensions[axis] = value;
    if (_locked && value != null && value.isFinite && value > 0) {
      for (var i = 0; i < 3; i++) {
        if (i != axis) {
          final scaled = _ratio[i] * value / _ratio[axis];
          _dimensions[i] = scaled;
          _fields[i].text = scaled.toStringAsFixed(1);
        }
      }
    }
    if (_valid(_dimensions)) _lastValid = _dimensions.cast<double>().toList();
    setState(() {});
  }

  void _formatField(int axis) {
    final value = _dimensions[axis];
    if (value != null && value.isFinite) {
      _fields[axis].text = value.toStringAsFixed(1);
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
                  labelText: '${['Width', 'Depth', 'Height'][i]} (mm)',
                ),
                inputFormatters: [
                  TextInputFormatter.withFunction((before, after) {
                    final value = double.tryParse(after.text);
                    if (value != null &&
                        value.isFinite &&
                        RegExp(r'\.\d{2,}').hasMatch(after.text)) {
                      final text = value.toStringAsFixed(1);
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
              const Text('Use positive dimensions up to 1,000,000 mm.'),
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
