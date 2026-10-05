// Measurement harness, deliberately outside test/. No timing assertions.
// Run explicitly: flutter test benchmark/level_render_benchmark.dart
//
// WHAT THIS IS NOT.
//
//  - Not a correctness test. `test/level_painter_test.dart` owns that.
//  - Not a frame-budget claim. It records CPU time to build a display list in
//    a flutter-test JIT process: no GPU, no raster thread, no presentation,
//    no device. Numeric frame targets belong to issues 0032/0033 on named
//    reference devices (BEHAVIOR-SPEC §8.9), and nothing printed here may be
//    quoted as one.
//  - NOT a valid enclosed plan, and not claimed to be representative of one.
//    The payload is a RENDERER WORKLOAD: axis-aligned rectangles at a
//    plausible polygon count, generated to exercise the paint path. It is not
//    what `domowik_plan_derived` would return for any real document — the
//    rings do not enclose the rooms they sit beside, and no join geometry
//    exists. Only the SHAPES and COUNTS are native-like.
//
// Per level, per the generator below: 24 rooms, 48 wall records, 48 occupied
// rings, 24 room rings, 12 apertures.
//
// The single question it answers: how does paint cost scale as passive levels
// are added underneath the active one?

import 'dart:convert';
import 'dart:io';
import 'dart:ui' as ui;

import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const Size viewportSize = Size(1200, 800);
const int roomsPerLevel = 24;
const int warmupSamples = 20;
const int samples = 200;

/// One level's renderer workload.
///
/// Two wall records and two occupied rings per room — the room's south and
/// west edges only. Adjacent rooms therefore share no geometry and no room is
/// enclosed; that is deliberate and is why this is not a plan.
(List<Wall>, Derived) levelPayload(int seed) {
  const roomW = 4000.0, roomH = 3000.0, thickness = 200.0;
  final walls = <Wall>[];
  final rooms = <Ring>[];
  final occupied = <Ring>[];
  final openings = <Aperture>[];
  final offset = Offset(seed * 250.0, seed * 250.0);

  Ring rect(double l, double t, double r, double b, double area) => Ring(
    [
      Offset(l, t) + offset,
      Offset(r, t) + offset,
      Offset(r, b) + offset,
      Offset(l, b) + offset,
    ],
    const [],
    area,
  );

  for (var i = 0; i < roomsPerLevel; i++) {
    final col = i % 6, row = i ~/ 6;
    final x = col * roomW, y = row * roomH;
    rooms.add(
      rect(
        x + thickness / 2,
        y + roomH - thickness / 2,
        x + roomW - thickness / 2,
        y + thickness / 2,
        (roomW - thickness) * (roomH - thickness),
      ),
    );
    occupied.add(rect(x, y + thickness, x + roomW, y, 0)); // south edge
    occupied.add(rect(x, y + roomH, x + thickness, y, 0)); // west edge
    walls.add(
      Wall(
        'w$seed-$i-h',
        Offset(x, y) + offset,
        Offset(x + roomW, y) + offset,
        thickness,
        2500,
      ),
    );
    walls.add(
      Wall(
        'w$seed-$i-v',
        Offset(x, y) + offset,
        Offset(x, y + roomH) + offset,
        thickness,
        2500,
      ),
    );
    if (i.isEven) {
      final u = Offset(x + roomW / 2, y) + offset;
      openings.add(
        Aperture(
          'o$seed-$i',
          true,
          [
            u + const Offset(-450, -100),
            u + const Offset(450, -100),
            u + const Offset(450, 100),
            u + const Offset(-450, 100),
          ],
          u + const Offset(-450, 0),
          u + const Offset(-450, 900),
          90,
        ),
      );
    }
  }
  return (walls, Derived(occupied, rooms, openings, 0));
}

/// Mean and order statistics over `values`, in the unit they arrive in.
Map<String, double> stats(List<double> values) {
  final sorted = [...values]..sort();
  double at(int percent) {
    final index = ((percent / 100) * sorted.length).ceil() - 1;
    return sorted[index.clamp(0, sorted.length - 1)];
  }

  return {
    'mean': values.reduce((a, b) => a + b) / values.length,
    'p50': at(50),
    'p95': at(95),
    'p99': at(99),
  };
}

void main() {
  test('level render CPU measurement (NOT a frame budget or a gate)', () {
    final (activeWalls, activeDerived) = levelPayload(0);
    final view = PlanViewport.fit(
      viewportSize,
      PlanPainter.boundsOf(activeWalls, activeDerived),
    );

    List<PlanLayer> background(int passive) => [
      for (var i = 1; i <= passive; i++)
        () {
          final (walls, derived) = levelPayload(i);
          // The lower half ghosted, the rest visible, so both emphasis paths
          // are exercised at every level count.
          return PlanLayer(
            levelId: 'level-$i',
            walls: walls,
            derived: derived,
            ghosted: i <= passive / 2,
          );
        }(),
    ];

    Map<String, Object?> measure(int levels) {
      final painter = PlanPainter(
        activeWalls,
        activeDerived,
        null,
        null,
        view: view,
        backgroundLevels: background(levels - 1),
      );
      final us = <double>[];
      for (var i = 0; i < warmupSamples + samples; i++) {
        final timer = Stopwatch()..start();
        final recorder = ui.PictureRecorder();
        painter.paint(Canvas(recorder), viewportSize);
        recorder.endRecording().dispose();
        timer.stop();
        if (i >= warmupSamples) {
          us.add(timer.elapsedTicks * 1e6 / timer.frequency);
        }
      }
      return {
        'levels': levels,
        'ghostedLevels': ((levels - 1) / 2).floor(),
        'wallRecords': levels * roomsPerLevel * 2,
        'occupiedRings': levels * roomsPerLevel * 2,
        'roomRings': levels * roomsPerLevel,
        'apertures': levels * (roomsPerLevel ~/ 2),
        'paintUs': stats(us),
      };
    }

    final records = [
      for (final levels in [1, 3, 8]) measure(levels),
    ];

    stdout.writeln(
      jsonEncode({
        'harness': 'level-render-cpu-v1',
        'runtime': Platform.version,
        'os': Platform.operatingSystemVersion,
        'processors': Platform.numberOfProcessors,
        'mode':
            'flutter-test JIT; display-list recording only; '
            'no GPU/raster/presentation',
        'payload':
            'SYNTHETIC renderer workload; not a valid enclosed plan and not '
            'claimed representative of one',
        'resolutionLogicalPx': [viewportSize.width, viewportSize.height],
        'roomsPerLevel': roomsPerLevel,
        'ghostOpacity': PlanPainter.ghostOpacity,
        'paintSamples': samples,
        'paintWarmupSamples': warmupSamples,
        'records': records,
      }),
    );

    // The only assertion: the harness ran and produced finite numbers. A
    // threshold here would be the frame-budget claim this file disclaims.
    expect(
      records.every(
        (r) => (r['paintUs']! as Map<String, double>).values.every(
          (v) => v.isFinite && v > 0,
        ),
      ),
      isTrue,
    );
  });
}
