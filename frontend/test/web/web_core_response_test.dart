// The browser core host releases a core response however copying it into
// Dart ends (issue 0088 review round 4). A host byte-copy failure is played by
// making the page's Uint8Array constructor throw RangeError once, as a failed
// allocation does; the WebAssembly reply itself is untouched.
@TestOn('browser')
library;

import 'dart:js_interop';
import 'dart:js_interop_unsafe';

import 'package:domowik_editor/core/domowik_core.dart';
import 'package:domowik_editor/core/web_core.dart';
import 'package:flutter_test/flutter_test.dart';

@JS('eval')
external JSAny? _eval(JSString text);

void js(String text) => _eval(text.toJS);

int counter(String key) =>
    globalContext.getProperty<JSNumber>(key.toJS).toDartInt;

void main() {
  late WebCore core;
  setUpAll(() async {
    core = await WebCore.instantiate(await fetchBytes('domowik_bridge.wasm'));
  });

  for (final binary in [false, true]) {
    test('a failed copy of a ${binary ? 'binary' : 'JSON'} response releases it',
        () async {
      final handle = core.createDefault().handle;
      addTearDown(() => core.close(handle));
      expect(core.outstandingBuffers(), 0);
      js('''
        globalThis.testUint8Array = Uint8Array;
        globalThis.testCopyFailures = 0;
        globalThis.Uint8Array = new Proxy(globalThis.testUint8Array, {
          construct(target, args) {
            if (ArrayBuffer.isView(args[0]) && globalThis.testCopyFailures === 0) {
              globalThis.testCopyFailures++;
              throw new RangeError('injected host copy allocation failure');
            }
            return Reflect.construct(target, args, target);
          }
        });
      ''');
      try {
        expect(
          () => binary ? core.encode(handle) : core.state(handle),
          throwsA(anything),
        );
      } finally {
        js('globalThis.Uint8Array = globalThis.testUint8Array;');
      }
      expect(counter('testCopyFailures'), 1, reason: 'the copy was failed');
      expect(core.failure, isNull, reason: 'a host failure is not a trap');
      expect(core.outstandingBuffers(), 0, reason: 'the response was released');
      expect(core.state(handle).code, ok);
      expect(core.outstandingBuffers(), 0);
    });
  }
}
