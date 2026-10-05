import 'package:domowik_editor/plan_geometry_cache.dart';
import 'package:domowik_editor/plan_painter.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const first = Derived([], [], [], 1);
  const second = Derived([], [], [], 2);
  late PlanGeometryCache cache;
  setUp(() {
    cache = PlanGeometryCache();
    cache.retain(1, {'a', 'b'});
  });

  test('same content key reuses geometry, independent of camera changes', () {
    expect(cache.resolve('a', 'v1', () => first), same(first));
    expect(cache.retain(1, {'a', 'b'}), isFalse);
    expect(
      cache.resolve('a', 'v1', () => throw StateError('must not load')),
      same(first),
    );
    expect(cache.resolve('a', 'v2', () => second), same(second));
  });

  test('visibility eviction preserves other levels', () {
    cache.resolve('a', 'v1', () => first);
    cache.resolve('b', 'v1', () => second);
    cache.retain(1, {'b'});
    expect(cache.contains('a'), isFalse);
    expect(
      cache.resolve('b', 'v1', () => throw StateError('must not load')),
      same(second),
    );
    cache.retain(1, {'a', 'b'});
    expect(cache.resolve('a', 'v1', () => second), same(second));
  });

  test('failed read removes stale geometry and retries without key change', () {
    cache.resolve('a', 'v1', () => first);
    expect(cache.resolve('a', 'v2', () => null), same(Derived.empty));
    expect(cache.contains('a'), isFalse);
    expect(cache.resolve('a', 'v2', () => second), same(second));
  });

  test('missing key is usable but never cached', () {
    expect(cache.resolve('a', null, () => first), same(first));
    expect(cache.contains('a'), isFalse);
    expect(cache.resolve('a', null, () => null), same(Derived.empty));
  });

  test('session replacement and explicit clear invalidate all levels', () {
    cache.resolve('a', 'v1', () => first);
    expect(cache.retain(2, {'a'}), isTrue);
    expect(cache.session, 2);
    expect(cache.contains('a'), isFalse);
    cache.resolve('a', 'v1', () => second);
    cache.clear();
    expect(cache.session, isNull);
    expect(cache.contains('a'), isFalse);
  });
}
