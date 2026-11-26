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
    // Shadow only this test page's navigator/canvas properties. These tests
    // control GPU/WebGL availability explicitly without relying on host flags.
    evaluate('''
      window.domowikTestGpuDescriptor =
        Object.getOwnPropertyDescriptor(navigator, 'gpu');
      window.domowikTestGetContext = HTMLCanvasElement.prototype.getContext;
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
      if (window.domowikTestGetContext) {
        HTMLCanvasElement.prototype.getContext = window.domowikTestGetContext;
      }
      delete window.domowikTestGpuDescriptor;
      delete window.domowikTestGetContext;
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

  test(
    'missing WebGPU falls back to WebGL2 when WebGL2 is available',
    () async {
      evaluate('''
      Object.defineProperty(navigator, 'gpu', {
        configurable: true, value: undefined
      });
    ''');
      final presenter = WebScenePresenter(
        const _UnusedCore(),
        graphicsPolicy: const WebGraphicsPolicy.allowed(),
      );
      final created = (await presenter.invokeMapMethod<String, Object?>(
        'create',
      ))!;
      final session = created['session'];
      addTearDown(
        () => presenter.invokeMethod<void>('destroy', {'session': session}),
      );
      final stats = (await presenter.invokeMapMethod<String, Object?>('stats', {
        'session': session,
      }))!;
      expect(stats['adapter'], contains('WebGL2'));
    },
  );

  test('a null WebGPU adapter falls back to WebGL2 cleanly', () async {
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
    final created = (await presenter.invokeMapMethod<String, Object?>(
      'create',
    ))!;
    final session = created['session'];
    addTearDown(
      () => presenter.invokeMethod<void>('destroy', {'session': session}),
    );
    final stats = (await presenter.invokeMapMethod<String, Object?>('stats', {
      'session': session,
    }))!;
    expect(
      (evaluate('window.domowikTestAdapterRequests') as JSNumber).toDartInt,
      greaterThan(0),
    );
    expect(stats['adapter'], contains('WebGL2'));
  });

  test('missing WebGPU and WebGL2 is reported as no-webgpu refusal', () async {
    evaluate('''
      Object.defineProperty(navigator, 'gpu', {
        configurable: true, value: undefined
      });
      HTMLCanvasElement.prototype.getContext = function(type) {
        if (type === 'webgl2' || type === 'webgl') return null;
        return window.domowikTestGetContext.apply(this, arguments);
      };
    ''');
    final presenter = WebScenePresenter(
      const _UnusedCore(),
      graphicsPolicy: const WebGraphicsPolicy.allowed(),
    );
    await expectLater(
      presenter.invokeMethod<Object?>('create'),
      throwsA(
        isA<PlatformException>()
            .having((e) => e.code, 'code', 'no-webgpu')
            .having((e) => e.message, 'message', contains('WebGL2')),
      ),
    );
  });
}
