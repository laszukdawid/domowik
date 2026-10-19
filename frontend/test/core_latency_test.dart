@TestOn('vm')
library;

import 'dart:ffi';
import 'dart:io';

import 'package:domowik_bridge_plugin/domowik_bridge_plugin.dart';
import 'package:domowik_editor/core/ffi_core.dart';
import 'package:flutter_test/flutter_test.dart';

import 'core_latency_harness.dart';

void main() {
  test('native core latency on the shared harness', () {
    final suffix = Platform.isMacOS ? 'dylib' : 'so';
    final bridge = Bridge.fromLibrary(
      DynamicLibrary.open(
        File('target/release/libdomowik_bridge.$suffix').absolute.path,
      ),
    );
    final results = measureCoreLatency(FfiCore(bridge), 'native');
    expect(results.values.every((v) => v >= 0), isTrue);
    expect(bridge.outstandingBuffers(), 0);
  });
}
