import 'dart:async';
import 'dart:collection';
import 'dart:typed_data';

import 'package:flutter/material.dart';

class AssetThumbnailResult {
  const AssetThumbnailResult({this.png, this.error});
  final Uint8List? png;
  final String? error;
}

/// One dialog's derived PNG cache. At most 64 completed 256-pixel thumbnails
/// (including failures), one active decode, and only subscribed queued work.
/// No plan/image handles are owned here; Image.memory uses Flutter's image cache.
class AssetThumbnailCache {
  static const maxEntries = 64;
  final _cache = LinkedHashMap<String, AssetThumbnailResult>();
  final _pending = <String, _ThumbnailJob>{};
  final _queue = Queue<_ThumbnailJob>();
  bool _running = false, _closed = false;

  AssetThumbnailRequest request(String key, Future<Uint8List> Function() load) {
    if (_closed) {
      return AssetThumbnailRequest._(Future.value(null), () {});
    }
    final cached = _cache.remove(key);
    if (cached != null) {
      _cache[key] = cached;
      return AssetThumbnailRequest._(Future.value(cached), () {});
    }
    final job = _pending.putIfAbsent(key, () {
      final job = _ThumbnailJob(key, load);
      _queue.add(job);
      return job;
    });
    job.readers++;
    scheduleMicrotask(_pump);
    return AssetThumbnailRequest._(job.done.future, () {
      job.readers--;
      if (job.readers == 0 && _queue.remove(job)) {
        _pending.remove(key);
        job.done.complete(null);
      }
    });
  }

  Future<void> _pump() async {
    if (_closed || _running) return;
    _running = true;
    try {
      while (!_closed && _queue.isNotEmpty) {
        final job = _queue.removeFirst();
        AssetThumbnailResult result;
        try {
          final png = await job.load();
          if (png.isEmpty || png.length > 1024 * 1024) {
            throw const FormatException('Invalid thumbnail size');
          }
          result = AssetThumbnailResult(png: png);
        } catch (e) {
          result = AssetThumbnailResult(error: '$e');
        }
        _pending.remove(job.key);
        if (!_closed) {
          _cache[job.key] = result;
          while (_cache.length > maxEntries) {
            _cache.remove(_cache.keys.first);
          }
        }
        job.done.complete(_closed ? null : result);
      }
    } finally {
      _running = false;
    }
  }

  /// An active native decode is bounded but not cancellable. It may finish;
  /// queued work is discarded immediately and cannot reach the closed plan.
  void dispose() {
    _closed = true;
    for (final job in _queue) {
      _pending.remove(job.key);
      job.done.complete(null);
    }
    _queue.clear();
    _cache.clear();
  }
}

class _ThumbnailJob {
  _ThumbnailJob(this.key, this.load);
  final String key;
  final Future<Uint8List> Function() load;
  final done = Completer<AssetThumbnailResult?>();
  int readers = 0;
}

class AssetThumbnailRequest {
  AssetThumbnailRequest._(this.result, this._release);
  final Future<AssetThumbnailResult?> result;
  final VoidCallback _release;
  bool _released = false;
  void release() {
    if (_released) return;
    _released = true;
    _release();
  }
}

/// Visible tiles subscribe to queued work; scrolling them away cancels requests
/// that have not begun. Completed PNGs are reused when changing list/grid view.
class AssetThumbnail extends StatefulWidget {
  const AssetThumbnail({
    super.key,
    required this.cache,
    required this.assetKey,
    required this.name,
    required this.load,
  });
  final AssetThumbnailCache cache;
  final String assetKey, name;
  final Future<Uint8List> Function()? load;

  @override
  State<AssetThumbnail> createState() => _AssetThumbnailState();
}

class _AssetThumbnailState extends State<AssetThumbnail> {
  AssetThumbnailRequest? _request;

  void _subscribe() {
    _request = widget.load == null
        ? null
        : widget.cache.request(widget.assetKey, widget.load!);
  }

  @override
  void initState() {
    super.initState();
    _subscribe();
  }

  @override
  void didUpdateWidget(AssetThumbnail old) {
    super.didUpdateWidget(old);
    if (old.cache != widget.cache ||
        old.assetKey != widget.assetKey ||
        (old.load == null) != (widget.load == null)) {
      _request?.release();
      _subscribe();
    }
  }

  @override
  void dispose() {
    _request?.release();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => DecoratedBox(
    decoration: BoxDecoration(
      color: Theme.of(context).colorScheme.surfaceContainerHighest,
      borderRadius: BorderRadius.circular(6),
    ),
    child: FutureBuilder<AssetThumbnailResult?>(
      future: _request?.result,
      builder: (context, snapshot) {
        if (_request != null &&
            snapshot.connectionState != ConnectionState.done) {
          return const Center(
            child: Icon(
              Icons.hourglass_empty,
              semanticLabel: 'Loading preview',
            ),
          );
        }
        final result = snapshot.data;
        if (result?.png case final png?) {
          return Padding(
            padding: const EdgeInsets.all(3),
            child: Image.memory(
              png,
              fit: BoxFit.contain,
              semanticLabel: 'Preview of ${widget.name}',
              errorBuilder: (_, _, _) =>
                  _unavailable('Could not display the preview'),
            ),
          );
        }
        return _unavailable(result?.error ?? 'Preview unavailable');
      },
    ),
  );

  Widget _unavailable(String detail) => Tooltip(
    message: 'Preview unavailable: $detail',
    child: const Center(
      child: Icon(
        Icons.image_not_supported_outlined,
        semanticLabel: 'Preview unavailable',
      ),
    ),
  );
}
