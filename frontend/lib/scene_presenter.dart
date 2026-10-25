import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';

/// The 3D presenter protocol: the methods `ThreeDView` calls (`create`,
/// `destroy`, `reset`, `loadScene` or `mesh`/`scene`, `camera`, `stats`,
/// `diagnosticCapture`, `orientationProbe`, `orientationStats`) and the
/// `health` calls it receives, with [surface] showing the result.
///
/// Native presenters live in the platform runner or plugin and speak over a
/// [MethodChannel]; the browser presenter is Dart over the renderer's
/// WebAssembly module (issue 0088). The protocol is the same either way, so
/// the view does not know which it has.
/// `loadScene` optionally takes `poseOnly` and the last installed `sceneToken`.
/// A matching baseline allows an `instanceOnly` result: transforms reuse
/// resident mesh buffers. A missing/stale baseline returns a full scene and
/// a new token. Receivers must not upload the names-only mesh entries as empty
/// geometry; if resident resources are lost, retry a full load.
/// Linux push updates carry mutable `meshes`, `nodes`, and the rebased camera
/// (`matrix`, `width`, `height`) in one `scene` packet. The presenter validates
/// and compares CPU snapshots outside the render lock before installing the
/// complete packet under it. A stale mesh baseline refuses the whole packet.
/// Separate `mesh` calls are only for immutable content-addressed model data;
/// they must not replace visible geometry during a scene upload.
abstract interface class ScenePresenter {
  Future<T?> invokeMethod<T>(String method, [Object? arguments]);
  Future<Map<K, V>?> invokeMapMethod<K, V>(String method, [Object? arguments]);
  void setMethodCallHandler(Future<Object?> Function(MethodCall call)? handler);

  /// The widget that shows [textureId], the surface `create` returned.
  Widget surface(int textureId);
}

/// A presenter reached over a platform channel, shown as a Flutter texture.
class ChannelScenePresenter implements ScenePresenter {
  const ChannelScenePresenter(this.channel);
  final MethodChannel channel;

  @override
  Future<T?> invokeMethod<T>(String method, [Object? arguments]) =>
      channel.invokeMethod<T>(method, arguments);

  @override
  Future<Map<K, V>?> invokeMapMethod<K, V>(
    String method, [
    Object? arguments,
  ]) => channel.invokeMapMethod<K, V>(method, arguments);

  @override
  void setMethodCallHandler(
    Future<Object?> Function(MethodCall call)? handler,
  ) => channel.setMethodCallHandler(handler);

  @override
  Widget surface(int textureId) =>
      Texture(textureId: textureId, filterQuality: FilterQuality.low);
}
