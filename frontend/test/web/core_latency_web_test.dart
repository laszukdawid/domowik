@TestOn('browser')
library;

import 'package:domowik_editor/core/web_core.dart';
import 'package:flutter_test/flutter_test.dart';

import '../core_latency_harness.dart';

void main() {
  test('browser core latency on the shared harness', () async {
    final core = await WebCore.instantiate(
      await fetchBytes('domowik_bridge.wasm'),
    );
    final results = measureCoreLatency(core, 'browser');
    expect(results.values.every((v) => v >= 0), isTrue);
    expect(core.outstandingBuffers(), 0);
  });
}
