// The browser 3D presenter (issue 0088, ADR 0012).
//
// Implements the presenter protocol `ThreeDView` speaks (see
// scene_presenter.dart) in Dart, over the shared renderer compiled to
// WebAssembly (`domowik_renderer.js` + `_bg.wasm`, built from
// frontend/web-renderer and placed beside the page). Like the macOS presenter
// it loads scenes itself (`loadScene: true`): the core derives a binary scene
// frame and the renderer decodes it, so no geometry passes through Dart
// collections.
//
// Each 3D view gets its own canvas, shown through a platform view with
// pointer events passed through to Flutter, which keeps all navigation input
// in the shared Dart camera code. WebGPU is required; without it `create`
// fails and the view says so.

import 'dart:convert';
import 'dart:js_interop';
import 'dart:typed_data';
import 'dart:ui_web' as ui_web;

import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

import '../core/domowik_core.dart';
import '../scene_presenter.dart';

ScenePresenter hostScenePresenter(MethodChannel channel, DomowikCore core) =>
    WebScenePresenter(core);

extension type _Canvas(JSObject _) implements JSObject {
  external set width(int value);
  external set height(int value);
  external int get width;
  external int get height;
  external void remove();
  external _Style get style;
}

extension type _Style(JSObject _) implements JSObject {
  external set cssText(String value);
}

extension type _Document(JSObject _) implements JSObject {
  external _Canvas createElement(String tag);
}

@JS('document')
external _Document get _document;

extension type _RendererModule(JSObject _) implements JSObject {
  @JS('default')
  external JSPromise<JSAny?> init();
  @JS('WebRenderer')
  external _RendererClass get renderer;
}

extension type _RendererClass(JSObject _) implements JSObject {
  external JSPromise<_Renderer> create(_Canvas canvas);
}

extension type _Renderer(JSObject _) implements JSObject {
  external JSString setScene(JSUint8Array frame);
  external void render(JSFloat32Array camera, int width, int height);
  external JSString stats();
  external void free();
}

@JS('navigator.gpu')
external JSObject? get _gpu;

Future<_RendererModule>? _module;

/// Loads the renderer module once per page, resolved against the page URL so
/// the editor and its browser tests both find it beside them.
Future<_RendererModule> _loadModule() => _module ??= () async {
  final url = Uri.base.resolve('domowik_renderer.js').toString();
  final module = _RendererModule(await importModule(url.toJS).toDart);
  await module.init().toDart;
  return module;
}();

class _Session {
  _Session(this.canvas, this.renderer);
  final _Canvas canvas;
  final _Renderer renderer;
  int cameraUpdates = 0;
}

/// The browser presenter. One instance per 3D view; sessions are canvases.
class WebScenePresenter implements ScenePresenter {
  WebScenePresenter(this.core);
  final DomowikCore core;
  static int _nextSession = 1;
  final Map<int, _Session> _sessions = {};

  _Session _session(Object? arguments) {
    final id = (arguments as Map)['session'] as int?;
    final session = _sessions[id];
    if (session == null) {
      throw PlatformException(code: 'unknown-session', message: '$id');
    }
    return session;
  }

  Future<Object?> _invoke(String method, Object? arguments) async {
    switch (method) {
      case 'create':
        if (_gpu == null) {
          throw PlatformException(
            code: 'no-webgpu',
            message:
                'the 3D view needs WebGPU, which this browser does not offer',
          );
        }
        final module = await _loadModule();
        final canvas = _document.createElement('canvas')
          ..style.cssText =
              'width:100%;height:100%;display:block;pointer-events:none;';
        final _Renderer renderer;
        try {
          renderer = await module.renderer.create(canvas).toDart;
        } catch (error) {
          throw PlatformException(code: 'no-webgpu', message: '$error');
        }
        final id = _nextSession++;
        _sessions[id] = _Session(canvas, renderer);
        ui_web.platformViewRegistry.registerViewFactory(
          _viewType(id),
          (int viewId) => canvas,
        );
        return {'textureId': id, 'session': id, 'loadScene': true};
      case 'destroy':
        final id = (arguments as Map)['session'] as int?;
        final session = _sessions.remove(id);
        session?.renderer.free();
        session?.canvas.remove();
        return null;
      case 'reset':
        _session(arguments);
        return null;
      case 'loadScene':
        final args = arguments as Map;
        final session = _session(args);
        final plan = args['plan'] as int;
        final reply = core.sceneFrame(
          plan,
          jsonEncode({
            'levelIds': args['levelIds'],
            'ghostedIds': args['ghostedIds'],
            'selectedIds': args['selectedIds'] ?? const [],
            'poseOnly': args['poseOnly'] ?? false,
            'sceneToken': args['sceneToken'],
          }),
        );
        if (!reply.succeeded) {
          throw PlatformException(
            code: 'scene-${reply.code}',
            message: core.lastError(plan),
          );
        }
        final String summary;
        try {
          summary = session.renderer.setScene(reply.bytes!.toJS).toDart;
        } catch (error) {
          throw PlatformException(code: 'scene', message: '$error');
        }
        return {
          ...jsonDecode(summary) as Map<String, Object?>,
          'generation': args['generation'],
        };
      case 'camera':
        final args = arguments as Map;
        final session = _session(args);
        final width = args['width'] as int, height = args['height'] as int;
        if (session.canvas.width != width) session.canvas.width = width;
        if (session.canvas.height != height) session.canvas.height = height;
        final matrix = Float32List.fromList(
          (args['matrix'] as List)
              .cast<num>()
              .map((v) => v.toDouble())
              .toList(),
        );
        try {
          session.renderer.render(matrix.toJS, width, height);
        } catch (error) {
          throw PlatformException(code: 'render', message: '$error');
        }
        session.cameraUpdates++;
        return null;
      case 'stats':
        final session = _session(arguments);
        return {
          ...jsonDecode(session.renderer.stats().toDart)
              as Map<String, Object?>,
          'cameraUpdates': session.cameraUpdates,
          'width': session.canvas.width,
          'height': session.canvas.height,
        };
      default:
        // Diagnostic readbacks are native probes.
        throw MissingPluginException('$method is not offered in the browser');
    }
  }

  static String _viewType(int session) => 'domowik-3d-$session';

  @override
  Future<T?> invokeMethod<T>(String method, [Object? arguments]) async =>
      await _invoke(method, arguments) as T?;

  @override
  Future<Map<K, V>?> invokeMapMethod<K, V>(
    String method, [
    Object? arguments,
  ]) async => (await _invoke(method, arguments) as Map?)?.cast<K, V>();

  @override
  void setMethodCallHandler(Future<Object?> Function(MethodCall)? handler) {}

  @override
  Widget surface(int textureId) =>
      HtmlElementView(viewType: _viewType(textureId));
}
