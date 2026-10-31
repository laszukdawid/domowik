@TestOn('browser')
library;

import 'dart:js_interop';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/platform/scene_presenter_web.dart';
import 'package:domowik_editor/web_graphics_policy.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('eval')
external JSAny? _eval(JSString source);

JSAny? evaluate(String source) => _eval(source.toJS);

class _UnusedCore implements DomowikCore {
  const _UnusedCore();
  @override
  dynamic noSuchMethod(Invocation invocation) =>
      throw StateError('GPU admission must not call the document core');
}

void main() {
  setUp(() {
    // Shadow only this test page's navigator property. These tests never ask
    // the physical driver for an adapter, including on a known-unsafe host.
    evaluate('''
      window.domowikTestGpuDescriptor =
        Object.getOwnPropertyDescriptor(navigator, 'gpu');
      window.domowikTestGpuReads = 0;
      window.domowikTestAdapterRequests = 0;
    ''');
  });
  tearDown(() {
    evaluate('''
      if (window.domowikTestGpuDescriptor) {
        Object.defineProperty(navigator, 'gpu', window.domowikTestGpuDescriptor);
      } else {
        delete navigator.gpu;
      }
      delete window.domowikTestGpuDescriptor;
      delete window.domowikTestGpuReads;
      delete window.domowikTestAdapterRequests;
    ''');
  });

  test('Linux Firefox is refused without reading navigator.gpu', () async {
    int rendererLoads() => (evaluate('''
          performance.getEntriesByType('resource').filter(entry =>
            /domowik_renderer(?:_bg)?\\.(?:js|wasm)/.test(entry.name)
          ).length
        ''') as JSNumber).toDartInt;
    final loadsBefore = rendererLoads();
    evaluate('''
      Object.defineProperty(navigator, 'gpu', {
        configurable: true,
        get() {
          window.domowikTestGpuReads++;
          throw new Error('a protected page must never access WebGPU');
        }
      });
    ''');
    final presenter = WebScenePresenter(
      const _UnusedCore(),
      graphicsPolicy: WebGraphicsPolicy.forBrowser(
        userAgent: 'Mozilla/5.0 (X11; Linux x86_64) Firefox/159.0',
        page: Uri.parse('https://editor.invalid/'),
      ),
    );
    await expectLater(
      presenter.invokeMethod<Object?>('create'),
      throwsA(
        isA<PlatformException>().having(
          (e) => e.code,
          'code',
          'webgpu-disabled',
        ),
      ),
    );
    expect((evaluate('window.domowikTestGpuReads') as JSNumber).toDartInt, 0);
    expect(
      rendererLoads(),
      loadsBefore,
      reason: 'protected startup must not load the renderer module',
    );
  });

  test('missing WebGPU is reported without initializing a renderer', () async {
    evaluate('''
      Object.defineProperty(navigator, 'gpu', {
        configurable: true, value: undefined
      });
    ''');
    final presenter = WebScenePresenter(
      const _UnusedCore(),
      graphicsPolicy: const WebGraphicsPolicy.allowed(),
    );
    await expectLater(
      presenter.invokeMethod<Object?>('create'),
      throwsA(
        isA<PlatformException>().having((e) => e.code, 'code', 'no-webgpu'),
      ),
    );
  });

  test('a null adapter is an ordinary refusal, not a forced backend', () async {
    evaluate('''
      Object.defineProperty(navigator, 'gpu', {
        configurable: true,
        value: {
          requestAdapter() {
            window.domowikTestAdapterRequests++;
            return Promise.resolve(null);
          }
        }
      });
    ''');
    final presenter = WebScenePresenter(
      const _UnusedCore(),
      graphicsPolicy: const WebGraphicsPolicy.allowed(),
    );
    await expectLater(
      presenter.invokeMethod<Object?>('create'),
      throwsA(
        isA<PlatformException>().having((e) => e.code, 'code', 'no-webgpu'),
      ),
    );
    expect(
      (evaluate('window.domowikTestAdapterRequests') as JSNumber).toDartInt,
      greaterThan(0),
    );
  });
}
