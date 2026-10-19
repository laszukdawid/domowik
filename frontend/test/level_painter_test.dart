// Level rendering: BEHAVIOR-SPEC §2.4's tiers, order and emphasis (0051).
//
// Rasterised rather than mocked. A recording canvas would prove which calls
// were issued and nothing about which pixels won, and stacking order IS the
// thing under test — "active above passive" is only true if the active colour
// survives at a shared pixel.
//
// `Picture.toImage` is a real async engine call and never completes inside
// `testWidgets`'s fake-async zone: every raster here runs through
// `tester.runAsync`, and every Picture and Image is disposed. A previous
// version of this file deadlocked for ten minutes on exactly that.

import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:domowik_editor/plan_painter.dart';
import 'package:domowik_editor/plan_viewport.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const Size canvasSize = Size(800, 600);
const Timeout rasterTimeout = Timeout(Duration(seconds: 60));

/// Plan (0,0) at the canvas centre, 0.1 px/mm.
final PlanViewport view = PlanViewport(
  zoom: 0.1,
  origin: Offset(canvasSize.width / 2, canvasSize.height / 2),
);

Ring box(
  double left,
  double top,
  double right,
  double bottom, {
  double area = 0,
  String? label,
  Offset? anchor,
}) => Ring(
  [
    Offset(left, top),
    Offset(right, top),
    Offset(right, bottom),
    Offset(left, bottom),
  ],
  const [],
  area,
  // A label is drawn only where the CORE resolved one. A face with no
  // effective record shows nothing at all (BEHAVIOR-SPEC R5.3.3 absence), so
  // a room in these fixtures is unlabelled unless it is given one.
  recordId: label == null ? null : 'record',
  label: label,
  displayAreaM2: label == null ? null : (area / 1e6).toStringAsFixed(2),
  labelAnchorMm: label == null ? null : (anchor ?? Offset.zero),
);

Wall wall(String id, Offset start, Offset end) =>
    Wall(id, start, end, 200, 2500);

/// Occupied region only — the DARK wall fill.
Derived occupiedOf(Ring r) => Derived([r], const [], const [], 0);

/// Rooms only — the LIGHT room fill. A different primitive from
/// [occupiedOf], so an overlap test does not rest on two identical fills.
Derived roomsOf(List<Ring> rooms) => Derived(const [], rooms, const [], 0);

/// Paints `painter` and returns raw RGBA. MUST be awaited inside
/// `tester.runAsync`.
Future<ByteData> raster(PlanPainter painter) async {
  final recorder = ui.PictureRecorder();
  final canvas = Canvas(recorder);
  // An opaque ground, so "transparent" and "nothing drawn" stay separable.
  canvas.drawRect(
    Rect.fromLTWH(0, 0, canvasSize.width, canvasSize.height),
    Paint()..color = const Color(0xFFFFFFFF),
  );
  painter.paint(canvas, canvasSize);
  final picture = recorder.endRecording();
  final image = await picture.toImage(
    canvasSize.width.toInt(),
    canvasSize.height.toInt(),
  );
  final data = await image.toByteData();
  image.dispose();
  picture.dispose();
  return data!;
}

extension on ByteData {
  Color at(Offset p) {
    final i = ((p.dy.toInt() * canvasSize.width.toInt()) + p.dx.toInt()) * 4;
    return Color.fromARGB(
      getUint8(i + 3),
      getUint8(i),
      getUint8(i + 1),
      getUint8(i + 2),
    );
  }
}

/// Perceived darkness against the white ground: bigger means more emphasis.
double ink(Color c) => 255 * 3 - (c.r * 255 + c.g * 255 + c.b * 255);

/// Total ink over a region, for content too sparse to probe pixel by pixel.
double inkIn(ByteData pixels, Rect area) {
  var total = 0.0;
  for (var y = area.top.toInt(); y < area.bottom.toInt(); y++) {
    for (var x = area.left.toInt(); x < area.right.toInt(); x++) {
      total += ink(pixels.at(Offset(x.toDouble(), y.toDouble())));
    }
  }
  return total;
}

void main() {
  // Active: a dark occupied region over screen x 200..600, y 150..450.
  final activeRegion = box(-2000, 1500, 2000, -1500);
  // Passive: a light ROOM over screen x 300..700, y 250..550. Different
  // primitive, different colour, partially overlapping.
  final passiveRoom = box(-1000, 500, 3000, -2500);

  final overlap = view.toScreen(const Offset(0, 0)); // (400, 300)
  final passiveOnly = view.toScreen(const Offset(2500, -2000)); // (650, 500)
  final activeOnly = view.toScreen(const Offset(-1800, 1200)); // (220, 180)

  // NO walls in the colour fixtures. A layer with walls and no occupied
  // region falls back to dark centre lines, which land exactly on the overlap
  // probe and made the light room fill unobservable there.
  PlanLayer lightLayer({bool ghosted = false, String id = 'light'}) =>
      PlanLayer(
        levelId: id,
        walls: const [],
        derived: roomsOf([passiveRoom]),
        ghosted: ghosted,
      );

  PlanLayer darkLayer({bool ghosted = false, String id = 'dark'}) => PlanLayer(
    levelId: id,
    walls: const [],
    derived: occupiedOf(passiveRoom),
    ghosted: ghosted,
  );

  PlanPainter activeOver(List<PlanLayer> background) => PlanPainter(
    [wall('active-wall', const Offset(-2000, 0), const Offset(2000, 0))],
    occupiedOf(activeRegion),
    null,
    null,
    view: view,
    showGrid: false,
    backgroundLevels: background,
  );

  PlanPainter passiveOnlyPainter(List<PlanLayer> background) => PlanPainter(
    const [],
    Derived.empty,
    null,
    null,
    view: view,
    showGrid: false,
    backgroundLevels: background,
  );

  testWidgets('the active level paints over a passive one', (tester) async {
    await tester.runAsync(() async {
      final layer = lightLayer();
      final both = await raster(activeOver([layer]));
      final activeAlone = await raster(activeOver(const []));
      final passiveAlone = await raster(passiveOnlyPainter([layer]));

      // The fixture is only meaningful if the two differ where they meet.
      expect(
        activeAlone.at(overlap),
        isNot(passiveAlone.at(overlap)),
        reason: 'dark occupied fill must differ from the light room fill',
      );
      expect(
        both.at(overlap),
        activeAlone.at(overlap),
        reason: 'the ACTIVE level wins the shared pixel',
      );
      // And the passive level still shows where the active one does not.
      expect(both.at(passiveOnly), passiveAlone.at(passiveOnly));
      expect(both.at(passiveOnly), isNot(const Color(0xFFFFFFFF)));
    });
  }, timeout: rasterTimeout);

  testWidgets('a ghosted layer is fainter than a visible one', (tester) async {
    await tester.runAsync(() async {
      // The DARK layer, so the emphasis difference is far above quantization.
      final normal = await raster(activeOver([darkLayer()]));
      final ghost = await raster(activeOver([darkLayer(ghosted: true)]));
      final blank = await raster(activeOver(const []));

      final normalInk = ink(normal.at(passiveOnly));
      final ghostInk = ink(ghost.at(passiveOnly));
      expect(ghostInk, lessThan(normalInk), reason: 'ghost is reduced');
      expect(
        ghostInk,
        greaterThan(ink(blank.at(passiveOnly))),
        reason: 'ghost is rendered, not hidden',
      );
      // The ACTIVE level is untouched by a passive layer's emphasis.
      expect(ghost.at(activeOnly), normal.at(activeOnly));
      expect(ghost.at(activeOnly), blank.at(activeOnly));
    });
  }, timeout: rasterTimeout);

  testWidgets('passive layers paint in the order given', (tester) async {
    await tester.runAsync(() async {
      // Identical geometry, DIFFERENT opaque fills — two near-white fills
      // composited over each other are indistinguishable and would prove
      // nothing about order. Whichever layer is painted LAST wins the pixel.
      final dark = darkLayer();
      final light = lightLayer();
      final darkUnder = await raster(passiveOnlyPainter([dark, light]));
      final darkOver = await raster(passiveOnlyPainter([light, dark]));
      expect(darkUnder.at(passiveOnly), isNot(darkOver.at(passiveOnly)));
      expect(
        ink(darkOver.at(passiveOnly)),
        greaterThan(ink(darkUnder.at(passiveOnly))),
        reason: 'the dark layer supplied LAST is the one that survives',
      );
      // And the list order alone decides it: same two layers, both visible.
      expect(dark.ghosted, isFalse);
      expect(light.ghosted, isFalse);
    });
  }, timeout: rasterTimeout);

  testWidgets('selection never reaches a passive layer', (tester) async {
    await tester.runAsync(() async {
      // This layer DOES carry a wall, so there is an id to select.
      final layer = PlanLayer(
        levelId: 'below',
        walls: [
          wall('passive-wall', const Offset(-1000, 0), const Offset(3000, 0)),
        ],
        derived: roomsOf([passiveRoom]),
      );
      // The passive layer's wall id is SELECTED. A painter drawing selection
      // per layer would highlight it; §2.4 says non-active levels never are.
      final selected = await raster(
        PlanPainter(
          const [],
          Derived.empty,
          'passive-wall',
          null,
          view: view,
          showGrid: false,
          selectedIds: const {'passive-wall'},
          backgroundLevels: [layer],
        ),
      );
      final plain = await raster(passiveOnlyPainter([layer]));
      for (final probe in [overlap, passiveOnly, activeOnly]) {
        expect(
          selected.at(probe),
          plain.at(probe),
          reason: 'selecting a passive wall changed pixel $probe',
        );
      }
    });
  }, timeout: rasterTimeout);

  testWidgets('passive levels keep their area labels, at tier emphasis', (
    tester,
  ) async {
    await tester.runAsync(() async {
      // Isolated by DIFFERENCE: the same room with and without a LABEL
      // differs only by the label, so the fill underneath cancels.
      //
      // The discriminator used to be a positive area, which stopped working
      // when labels became the core's answer rather than the painter's: an
      // area alone no longer draws anything, because a face nobody named has
      // no label (R5.3.3).
      Future<double> labelInk({required bool ghosted}) async {
        PlanPainter of(Ring r) => passiveOnlyPainter([
          PlanLayer(
            levelId: 'below',
            walls: const [],
            derived: roomsOf([r]),
            ghosted: ghosted,
          ),
        ]);
        final centre = view.toScreen(const Offset(0, 0));
        final label = Rect.fromCenter(center: centre, width: 80, height: 24);
        return inkIn(
              await raster(
                of(box(-2000, 1500, 2000, -1500, area: 12e6, label: 'Salon')),
              ),
              label,
            ) -
            inkIn(
              await raster(of(box(-2000, 1500, 2000, -1500, area: 12e6))),
              label,
            );
      }

      final visible = await labelInk(ghosted: false);
      final ghost = await labelInk(ghosted: true);
      expect(
        visible,
        greaterThan(0),
        reason: 'a visible non-active level renders normally, labels included',
      );
      expect(ghost, greaterThan(0), reason: 'a ghosted level keeps its label');
      expect(
        ghost,
        lessThan(visible),
        reason: 'the label is tiered with the layer, not suppressed',
      );
    });
  }, timeout: rasterTimeout);

  test('wallsPainted counts the ACTIVE level only', () {
    final recorder = ui.PictureRecorder();
    PlanPainter(
      [
        wall('a', const Offset(0, 0), const Offset(1000, 0)),
        wall('b', const Offset(0, 500), const Offset(1000, 500)),
      ],
      Derived.empty, // no occupied region, so centre lines are drawn
      null,
      null,
      view: view,
      showGrid: false,
      backgroundLevels: [
        PlanLayer(
          levelId: 'below',
          walls: [
            wall('c', const Offset(0, 0), const Offset(1000, 0)),
            wall('d', const Offset(0, 0), const Offset(1000, 0)),
            wall('e', const Offset(0, 0), const Offset(1000, 0)),
          ],
          derived: Derived.empty,
        ),
      ],
    ).paint(Canvas(recorder), canvasSize);
    recorder.endRecording().dispose();
    expect(PlanPainter.wallsPainted, 2, reason: 'three passive walls excluded');
  });

  group('shouldRepaint', () {
    final base = PlanPainter(const [], Derived.empty, null, null, view: view);

    test('is false for an identical painter', () {
      expect(
        PlanPainter(
          base.walls,
          base.derived,
          null,
          null,
          view: view,
        ).shouldRepaint(base),
        isFalse,
      );
    });

    test('is true when a background layer is added, removed or reordered', () {
      const one = PlanLayer(levelId: 'a', walls: [], derived: Derived.empty);
      const two = PlanLayer(levelId: 'b', walls: [], derived: Derived.empty);
      final withOne = PlanPainter(
        base.walls,
        base.derived,
        null,
        null,
        view: view,
        backgroundLevels: const [one],
      );
      final withBoth = PlanPainter(
        base.walls,
        base.derived,
        null,
        null,
        view: view,
        backgroundLevels: const [one, two],
      );
      final reordered = PlanPainter(
        base.walls,
        base.derived,
        null,
        null,
        view: view,
        backgroundLevels: const [two, one],
      );
      expect(withOne.shouldRepaint(base), isTrue);
      expect(withBoth.shouldRepaint(withOne), isTrue);
      expect(reordered.shouldRepaint(withBoth), isTrue);
      expect(withOne.shouldRepaint(withOne), isFalse);
    });

    test('is true when only a layer\'s ghost flag changes', () {
      const walls = <Wall>[];
      final visible = PlanPainter(
        base.walls,
        base.derived,
        null,
        null,
        view: view,
        backgroundLevels: const [
          PlanLayer(levelId: 'a', walls: walls, derived: Derived.empty),
        ],
      );
      final ghosted = PlanPainter(
        base.walls,
        base.derived,
        null,
        null,
        view: view,
        backgroundLevels: const [
          PlanLayer(
            levelId: 'a',
            walls: walls,
            derived: Derived.empty,
            ghosted: true,
          ),
        ],
      );
      expect(ghosted.shouldRepaint(visible), isTrue);
    });
  });

  testWidgets('a face the core gave no label draws none', (tester) async {
    await tester.runAsync(() async {
      // R5.3.3 absence, and the reason the painter cannot decide this for
      // itself: whether a face has an effective record is a §5.2 question
      // answered on the other side of the bridge (ED-0101).
      final painter = passiveOnlyPainter([
        PlanLayer(
          levelId: 'below',
          walls: const [],
          derived: roomsOf([box(-2000, 1500, 2000, -1500, area: 12e6)]),
          ghosted: false,
        ),
      ]);
      final centre = view.toScreen(const Offset(0, 0));
      final where = Rect.fromCenter(center: centre, width: 80, height: 24);
      final withLabel = passiveOnlyPainter([
        PlanLayer(
          levelId: 'below',
          walls: const [],
          derived: roomsOf([
            box(-2000, 1500, 2000, -1500, area: 12e6, label: 'Salon'),
          ]),
          ghosted: false,
        ),
      ]);
      expect(
        inkIn(await raster(painter), where),
        lessThan(inkIn(await raster(withLabel), where)),
        reason: 'an unnamed face must not be labelled with its area alone',
      );
    });
  }, timeout: rasterTimeout);

  testWidgets('the label sits at the core anchor, not the ring centroid', (
    tester,
  ) async {
    await tester.runAsync(() async {
      // The anchor is the effective record's seed (R5.3.3), which for an
      // L-shaped room is nowhere near the centroid. Moving it must move the
      // ink.
      PlanPainter at(Offset anchor) => passiveOnlyPainter([
        PlanLayer(
          levelId: 'below',
          walls: const [],
          derived: roomsOf([
            box(
              -2000,
              1500,
              2000,
              -1500,
              area: 12e6,
              label: 'Salon',
              anchor: anchor,
            ),
          ]),
          ghosted: false,
        ),
      ]);
      final offCentre = view.toScreen(const Offset(1200, 0));
      final where = Rect.fromCenter(center: offCentre, width: 80, height: 24);
      expect(
        inkIn(await raster(at(const Offset(1200, 0))), where),
        greaterThan(inkIn(await raster(at(Offset.zero)), where)),
        reason: 'the label follows the anchor the core supplied',
      );
    });
  }, timeout: rasterTimeout);

  // R4.2.2: walls are painted from the core's DRAWN region (cuts applied),
  // and an aperture's tint lies BENEATH it — so material the cut never
  // removes (a join patch, another wall) keeps the wall colour, while the
  // gap the cut did remove shows the tint.
  testWidgets('apertures never erase material the drawn region keeps', (
    tester,
  ) async {
    await tester.runAsync(() async {
      // The unamended region: x -2000..2000. The drawn region keeps its left
      // part and a "patch" square x 1000..2000 inside the cut's footprint.
      final unamended = box(-2000, 1000, 2000, -1000);
      final kept = box(-2000, 1000, 0, -1000);
      final patch = box(1000, 1000, 2000, -1000);
      const cut = [
        Offset(0, -1000),
        Offset(2000, -1000),
        Offset(2000, 1000),
        Offset(0, 1000),
      ];
      final door = Aperture('door', true, cut, Offset.zero, Offset.zero, 0);
      final painter = PlanPainter(
        const [],
        Derived([unamended], const [], [door], 0, drawn: [kept, patch]),
        null,
        null,
        view: view,
        showGrid: false,
      );
      final pixels = await raster(painter);
      final wallAt = pixels.at(view.toScreen(const Offset(-1000, 0)));
      final patchAt = pixels.at(view.toScreen(const Offset(1500, 0)));
      final gapAt = pixels.at(view.toScreen(const Offset(500, 0)));
      expect(patchAt, wallAt, reason: 'the kept patch shows the wall colour');
      expect(gapAt, isNot(wallAt), reason: 'the removed gap is not wall');
      expect(gapAt, isNot(const Color(0xFFFFFFFF)), reason: 'it shows the tint');
    });
  }, timeout: rasterTimeout);
}
