import 'dart:ui';

import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test(
    'screen grid preserves origin, major-line phase and coordinate labels',
    () {
      for (final origin in [-1234.5, -10.0, 0.0, 134.0, 512.0, 1e7]) {
        for (final zoom in [0.0001, 0.036, 0.1, 0.125, 1.37, 100.0]) {
          final view = PlanViewport(zoom: zoom, origin: Offset(origin, origin));
          for (final horizontal in [false, true]) {
            final lines = view.gridLines(1200, horizontal: horizontal).toList();
            expect(lines.length, lessThanOrEqualTo(151));
            expect(lines, isNotEmpty);
            for (final line in lines) {
              final mm = line.millimetres!;
              final pixel = horizontal
                  ? view.toScreen(Offset(0, mm)).dy
                  : view.toScreen(Offset(mm, 0)).dx;
              expect(line.pixel, closeTo(pixel, 1e-7));
              expect(line.pixel, inInclusiveRange(0, 1200));
              final index = (mm / view.gridStepMm).round();
              expect(line.major, index % 5 == 0);
              expect(line.axis, index == 0);
            }
            for (var i = 1; i < lines.length; i++) {
              expect(
                lines[i].pixel - lines[i - 1].pixel,
                closeTo(view.gridStepMm * zoom, 1e-9),
              );
            }
          }
        }
      }
    },
  );

  test('huge origins keep a bounded finite grid without misleading labels', () {
    for (final origin in [-1e30, 1e30, -1e308, 1e308]) {
      for (final zoom in [PlanViewport.minZoom, 0.1, PlanViewport.maxZoom]) {
        final view = PlanViewport(zoom: zoom, origin: Offset(origin, origin));
        for (final horizontal in [false, true]) {
          final lines = view.gridLines(1200, horizontal: horizontal).toList();
          expect(lines.length, inInclusiveRange(1, 151));
          for (final line in lines) {
            expect(line.pixel, inInclusiveRange(0, 1200));
            expect(line.millimetres, isNull);
            expect(line.axis, isFalse);
          }
          expect(lines.where((line) => line.major), isNotEmpty);
        }
      }
    }
  });

  test('invalid and unsupported synthetic grid extents fail boundedly', () {
    final view = PlanViewport(zoom: 0.1, origin: Offset.zero);
    for (final extent in [0.0, -1.0, double.nan, double.infinity, 1e308]) {
      expect(view.gridLines(extent), isEmpty);
    }
    final spacing = view.gridStepMm * view.zoom;
    expect(
      view.gridLines((PlanViewport.maxGridLines - 1) * spacing).length,
      PlanViewport.maxGridLines,
    );
    expect(view.gridLines(PlanViewport.maxGridLines * spacing), isEmpty);
  });

  test('normative coordinate mapping uses a right-handed plan', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    expect(view.toScreen(const Offset(1000, 2000)), const Offset(612, 184));
    expect(view.toPlan(const Offset(612, 184)), const Offset(1000, 2000));
  });

  test(
    'fit applies five-percent margins and the specified empty/degenerate defaults',
    () {
      final view = PlanViewport.fit(
        const Size(800, 600),
        const Rect.fromLTWH(1000, 2000, 4000, 3000),
      );
      expect(view.zoom, 0.18);
      expect(view.toScreen(const Offset(3000, 3500)), const Offset(400, 300));
      expect(view.toScreen(const Offset(1000, 5000)), const Offset(40, 30));
      final empty = PlanViewport.fit(const Size(800, 600), null);
      expect(empty.zoom, 0.1);
      expect(empty.toPlan(const Offset(400, 300)), Offset.zero);
      final line = PlanViewport.fit(
        const Size(800, 600),
        const Rect.fromLTWH(1000, 2000, 0, 3000),
      );
      expect(line.zoom, 0.1);
      expect(line.toScreen(const Offset(1000, 3500)), const Offset(400, 300));
    },
  );

  test('zoom keeps the cursor anchor fixed even when saturated', () {
    final initial = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    const anchor = Offset(700, 100);
    final point = initial.toPlan(anchor);
    for (final factor in [1.25, 0.8, 1e200, 1e-200]) {
      final result = initial.zoomAt(factor, anchor);
      expect(result.toScreen(point).dx, closeTo(anchor.dx, 1e-7));
      expect(result.toScreen(point).dy, closeTo(anchor.dy, 1e-7));
      expect(result.zoom, inInclusiveRange(0.0001, 100));
    }
  });

  test('wheel panning, shift panning and modifier zoom match section 1.4', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    const anchor = Offset(324, 68);
    expect(
      view
          .wheel(const Offset(7, 100), anchor, zooming: false, shift: false)
          .origin,
      const Offset(512, 484),
    );
    expect(
      view
          .wheel(const Offset(7, 100), anchor, zooming: false, shift: true)
          .origin,
      const Offset(519, 384),
    );
    expect(
      view
          .wheel(const Offset(0, 100), anchor, zooming: false, shift: true)
          .origin,
      const Offset(612, 384),
    );
    final zoomed = view.wheel(
      const Offset(7, 100),
      anchor,
      zooming: true,
      shift: false,
    );
    expect(zoomed.zoom, closeTo(0.08, 1e-15));
    expect(zoomed.toPlan(anchor).dx, closeTo(view.toPlan(anchor).dx, 1e-8));
    expect(zoomed.toPlan(anchor).dy, closeTo(view.toPlan(anchor).dy, 1e-8));
    expect(
      view
          .wheel(const Offset(0, -1e300), anchor, zooming: true, shift: false)
          .zoom,
      100,
    );
    expect(
      view
          .wheel(const Offset(0, 1e300), anchor, zooming: true, shift: false)
          .zoom,
      0.0001,
    );
  });

  test('invalid events leave all camera fields unchanged', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    expect(view.pan(const Offset(double.nan, 10)), same(view));
    expect(view.zoomAt(double.nan, Offset.zero), same(view));
    expect(view.zoomAt(-1, Offset.zero), same(view));
    expect(view.zoomAt(1.25, const Offset(10, double.infinity)), same(view));
    expect(
      view.wheel(
        const Offset(double.nan, 100),
        Offset.zero,
        zooming: true,
        shift: false,
      ),
      same(view),
    );
    expect(
      () => PlanViewport.fit(const Size(double.nan, 600), null),
      throwsArgumentError,
    );
    expect(
      () => PlanViewport(zoom: double.infinity, origin: Offset.zero),
      throwsArgumentError,
    );
  });

  test('grid adaptation bounds visible work across the zoom range', () {
    for (final (zoom, step) in [
      (0.1, 100.0),
      (0.05, 200.0),
      (0.0001, 100000.0),
      (100.0, 0.1),
    ]) {
      final view = PlanViewport(zoom: zoom, origin: const Offset(512, 384));
      expect(view.gridStepMm, step);
      final visible = view.visibleBounds(const Size(1200, 800));
      expect(visible.width / step, lessThanOrEqualTo(1200 / 8));
      expect(visible.height / step, lessThanOrEqualTo(800 / 8));
    }
  });

  test('two-touch pan and zoom reproduce the S10 vector exactly', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    // BEHAVIOR-SPEC S10: centroid (800,384) -> (810,384) over plan (2880,0),
    // finger distance 200 -> 250 px. Every value in the fixture is an exact
    // rational, so this compares exactly rather than under a tolerance.
    final stepped = view.panZoom(
      previousCentroid: const Offset(800, 384),
      currentCentroid: const Offset(810, 384),
      scale: 250 / 200,
    );
    expect(view.toPlan(const Offset(800, 384)), const Offset(2880, 0));
    expect(stepped.zoom, 0.125);
    expect(stepped.origin, const Offset(450, 384));
  });

  test('a two-touch step composes the pan and zoom primitives', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    const previous = Offset(800, 384);
    const current = Offset(810, 384);
    expect(
      view
          .panZoom(
            previousCentroid: previous,
            currentCentroid: current,
            scale: 1,
          )
          .origin,
      view.pan(current - previous).origin,
    );
    const centroid = Offset(700, 100);
    expect(
      view.panZoom(
        previousCentroid: centroid,
        currentCentroid: centroid,
        scale: 1.25,
      ),
      view.zoomAt(1.25, centroid),
    );
  });

  test('a saturated two-touch ratio still anchors on the new centroid', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    const previous = Offset(800, 300);
    const current = Offset(810, 320);
    final held = view.toPlan(previous);
    for (final (scale, expectedZoom) in [(1e6, 100.0), (1e-9, 0.0001)]) {
      final stepped = view.panZoom(
        previousCentroid: previous,
        currentCentroid: current,
        scale: scale,
      );
      // Clamping happens before anchoring, so the plan point the fingers
      // started on lands under where they are now even past a zoom limit.
      expect(stepped.zoom, expectedZoom);
      expect(stepped.toScreen(held).dx, closeTo(current.dx, 1e-9));
      expect(stepped.toScreen(held).dy, closeTo(current.dy, 1e-9));
    }
  });

  test('invalid two-touch steps leave all camera fields unchanged', () {
    final view = PlanViewport(zoom: 0.1, origin: const Offset(512, 384));
    const previous = Offset(800, 300);
    const current = Offset(810, 320);
    for (final scale in [
      0.0,
      -1.0,
      -0.0,
      double.nan,
      double.infinity,
      double.negativeInfinity,
    ]) {
      expect(
        view.panZoom(
          previousCentroid: previous,
          currentCentroid: current,
          scale: scale,
        ),
        same(view),
      );
    }
    for (final centroid in [
      const Offset(double.nan, 300),
      const Offset(800, double.nan),
      const Offset(double.infinity, 300),
      const Offset(800, double.negativeInfinity),
    ]) {
      expect(
        view.panZoom(
          previousCentroid: centroid,
          currentCentroid: current,
          scale: 1.25,
        ),
        same(view),
      );
      expect(
        view.panZoom(
          previousCentroid: previous,
          currentCentroid: centroid,
          scale: 1.25,
        ),
        same(view),
      );
    }
    // An accepted ratio whose anchored origin overflows is discarded whole
    // too: a partially applied camera would be worse than a dropped step.
    final far = PlanViewport(zoom: 0.1, origin: const Offset(1e308, 1e308));
    expect(
      far.panZoom(
        previousCentroid: previous,
        currentCentroid: current,
        scale: 1e6,
      ),
      same(far),
    );
  });
}
