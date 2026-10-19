// The browser core host releases every request block when an allocation in
// the module fails part-way through a call (issue 0088 review round 3). The
// bridge's own allocation exports are wrapped to fail one chosen allocation
// and to count blocks still held, without exhausting the browser's memory.
@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:js_interop_unsafe';
import 'dart:typed_data';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('eval')
external JSAny? _eval(JSString text);

void js(String text) => _eval(text.toJS);

int counter(String key) =>
    globalContext.getProperty<JSNumber>(key.toJS).toDartInt;

void main() {
  // The 1st allocation is the file name, the 2nd the model bytes, the 3rd
  // the response descriptor: each failure point must leave nothing held.
  for (final failing in [1, 2, 3]) {
    test('a failed allocation $failing in importModel holds no block', () async {
      js('''
        globalThis.testInstantiate = WebAssembly.instantiate;
        globalThis.testLive = new Map();
        globalThis.testAllocs = 0;
        globalThis.testFailAt = -1;
        WebAssembly.instantiate = async function(...args) {
          const result = await globalThis.testInstantiate(...args);
          const real = result.instance.exports;
          const exports = {...real};
          exports.domowik_web_alloc = function(length) {
            globalThis.testAllocs++;
            if (globalThis.testAllocs === globalThis.testFailAt) return 0;
            const ptr = real.domowik_web_alloc(length);
            if (ptr) globalThis.testLive.set(ptr, length);
            return ptr;
          };
          exports.domowik_web_free = function(ptr, length) {
            globalThis.testLive.delete(ptr);
            real.domowik_web_free(ptr, length);
          };
          return {...result, instance: {exports}};
        };
      ''');
      late WebCore core;
      try {
        core = await WebCore.instantiate(
          await fetchBytes('domowik_bridge.wasm'),
        );
      } finally {
        js('WebAssembly.instantiate = globalThis.testInstantiate;');
      }
      final handle = core.createDefault().handle;
      expect(handle, isNonZero);
      addTearDown(() => core.close(handle));
      js('globalThis.testLive.clear(); '
          'globalThis.testFailAt = globalThis.testAllocs + $failing;');
      expect(
        () => core.importModel(
          handle,
          'chair.obj',
          Uint8List.fromList([1, 2, 3]),
        ),
        throwsA(isA<StateError>()),
      );
      js('globalThis.testLiveCount = globalThis.testLive.size;');
      expect(counter('testLiveCount'), 0, reason: 'no request block held');
      expect(core.failure, isNull, reason: 'an allocation failure is not a trap');
      expect(core.state(handle).code, ok);
      expect(core.outstandingBuffers(), 0);
    });
  }
}
