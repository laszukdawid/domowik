// The plan canvas: the document models the core hands back, and the painter
// that draws them (issue 0051).
//
// Extracted from `main.dart` and extended with passive level rendering.
//
// Every polygon, aperture and room here was computed by the Rust core. NO
// geometry is derived in Dart: rooms are measured to the INNER wall faces,
// which is what a person means by floor area and is several square metres
// away from what the centre lines enclose, and a second implementation in the
// view would disagree with the core the first time a join rule changed
// (ED-0101).

import 'dart:math' as math;
import 'dart:typed_data';
import 'dart:ui' as ui;

import 'package:flutter/foundation.dart' show listEquals;
import 'package:flutter/material.dart';

import 'editor_fonts.dart';
import 'plan_viewport.dart';
import 'stair_resize.dart';
import 'measurement_units.dart';
import 'asset_library.dart';
import 'asset_rotation.dart';
import 'solid_colors_dialog.dart' show solidColor;

class Wall {
  const Wall(
    this.id,
    this.start,
    this.end,
    this.thicknessMm,
    this.heightMm, {
    this.color,
  });
  final String? color;

  final String id;
  final Offset start;
  final Offset end;
  final double thicknessMm;

  /// Authoritative per wall (R3.0.6). It affects vertical extrusion only, so
  /// the 2D canvas never draws with it — the inspector edits and shows it.
  final double heightMm;

  /// Centreline length, derived from the stored axis for display.
  double get lengthMm => (end - start).distance;
}

/// A polygon with holes, in plan millimetres.
///
/// For a ROOM face this also carries what the core decided about its label
/// (BEHAVIOR-SPEC §5.2/§5.3): which record is effectively associated, the text
/// to show, the rounded area string, where to anchor it, and a point inside
/// the face that a new record could be seeded at. None of that is computed
/// here. ED-0101: a front end chooses what the user is trying to do and never
/// decides what it means, and "which of two records names this room" is
/// exactly a meaning.
class Ring {
  const Ring(
    this.outer,
    this.holes,
    this.areaMm2, {
    this.recordId,
    this.label,
    this.displayAreaM2,
    this.labelAnchorMm,
    this.seedCandidateMm,
  });

  final List<Offset> outer;
  final List<List<Offset>> holes;

  /// Set for rooms; zero for occupied-region components.
  final double areaMm2;

  /// The effectively associated record (R5.2.4), or null when the face has no
  /// candidates and therefore no label at all (R5.3.3 absence).
  final String? recordId;

  /// The label text the core resolved: the record's name, or `Room`.
  final String? label;

  /// The area under the R5.3.2 display rule. A string, because the rounding is
  /// normative — square metres, two decimals, exact halves to EVEN — and
  /// `toStringAsFixed(2)` is not that rule.
  final String? displayAreaM2;

  /// Where R5.3.3 anchors the label: the effective record's seed.
  final Offset? labelAnchorMm;

  /// A point strictly inside the face, for seeding a new record.
  final Offset? seedCandidateMm;

  static Ring fromJson(Map<String, Object?> j) {
    List<Offset> pts(Object? v) => (v as List<Object?>)
        .map((e) => (e as List<Object?>).cast<num>())
        .map((p) => Offset(p[0].toDouble(), p[1].toDouble()))
        .toList();
    Offset? point(Object? v) {
      if (v == null) return null;
      final p = (v as List<Object?>).cast<num>();
      return Offset(p[0].toDouble(), p[1].toDouble());
    }

    return Ring(
      pts(j['outer']),
      (j['holes'] as List<Object?>).map(pts).toList(),
      (j['areaMm2'] as num?)?.toDouble() ?? 0.0,
      recordId: j['recordId'] as String?,
      label: j['label'] as String?,
      displayAreaM2: j['displayAreaM2'] as String?,
      labelAnchorMm: point(j['labelAnchorMm']),
      seedCandidateMm: point(j['seedCandidateMm']),
    );
  }
}

/// One room record as the core projected it (BEHAVIOR-SPEC §5.2).
///
/// `status` is DERIVED and arrives already decided: `associated`,
/// `superseded`, `seed-on-boundary`, `seed-in-wall` or `seed-in-exterior`.
/// The editor displays it and never computes it.
class RoomRecordView {
  const RoomRecordView(
    this.id,
    this.seedMm,
    this.name,
    this.status, {
    this.floorStyleRef,
    this.ceilingStyleRef,
  });

  final String id;
  final Offset seedMm;

  /// The AUTHORITATIVE name. Null and the empty string are different values
  /// here and stay different; only the display collapses both to `Room`.
  final String? name;
  final String status;
  final String? floorStyleRef;
  final String? ceilingStyleRef;

  bool get isAssociated => status == 'associated';

  static RoomRecordView fromJson(Map<String, Object?> j) {
    final seed = (j['seedMm'] as List<Object?>).cast<num>();
    return RoomRecordView(
      j['id'] as String,
      Offset(seed[0].toDouble(), seed[1].toDouble()),
      j['name'] as String?,
      j['status'] as String,
      floorStyleRef: j['floorStyleRef'] as String?,
      ceilingStyleRef: j['ceilingStyleRef'] as String?,
    );
  }
}

/// One opening's derived geometry: the aperture cut and the door leaf.
class Aperture {
  const Aperture(
    this.id,
    this.isDoor,
    this.cut,
    this.hinge,
    this.tip,
    this.swingDeg, {
    this.hostWallId,
    this.u,
    this.widthMm,
    this.heightMm,
    this.sillMm,
    this.depthMm,
    this.opensToward,
    this.hingeEnd,
    this.mechanism,
    this.color,
    this.doorStyle = 'solid',
    this.leafColor,
    this.originalColor,
    this.glyphSegments = const [],
  });

  final String id;
  final bool isDoor;
  final List<Offset> cut;
  final Offset hinge;
  final Offset tip;
  final double swingDeg;
  final String? hostWallId;
  final double? u;
  final double? widthMm;
  final double? heightMm;
  final double? sillMm;
  final double? depthMm;
  final String? opensToward;
  final String? hingeEnd;
  final String? mechanism;
  final String? color;
  final String doorStyle;
  final String? leafColor;
  final String? originalColor;
  final List<(Offset, Offset)> glyphSegments;

  static Aperture fromJson(Map<String, Object?> j) {
    Offset pt(Object? v) {
      final p = (v as List<Object?>).cast<num>();
      return Offset(p[0].toDouble(), p[1].toDouble());
    }

    final leaf = (j['leaf'] as List<Object?>);
    return Aperture(
      j['id'] as String,
      j['kind'] == 'door',
      (j['cut'] as List<Object?>).map(pt).toList(),
      pt(leaf[0]),
      pt(leaf[1]),
      (j['swingDeg'] as num).toDouble(),
      hostWallId: j['hostWallId'] as String?,
      u: (j['u'] as num?)?.toDouble(),
      widthMm: (j['widthMm'] as num?)?.toDouble(),
      heightMm: (j['heightMm'] as num?)?.toDouble(),
      sillMm: (j['sillMm'] as num?)?.toDouble(),
      depthMm: (j['depthMm'] as num?)?.toDouble(),
      opensToward: j['opensToward'] as String?,
      hingeEnd: j['hingeEnd'] as String?,
      mechanism: j['mechanism'] as String?,
      color: j['color'] as String?,
      doorStyle: j['doorStyle'] as String? ?? 'solid',
      leafColor: j['leafColor'] as String?,
      originalColor: j['originalColor'] as String?,
      glyphSegments: [
        for (final segment
            in (j['glyphSegments'] as List<Object?>? ?? const []))
          (pt((segment as List<Object?>)[0]), pt(segment[1])),
      ],
    );
  }
}

/// What the core derived for the level: the region walls occupy and the rooms
/// their complement encloses.
///
/// Never recomputed here. Rooms are measured to the INNER wall faces, which is
/// what a person means by floor area and is several square metres away from
/// what the centre lines enclose; a second implementation in the view would
/// disagree with the core the first time a join rule changed.
class Derived {
  const Derived(
    this.occupied,
    this.rooms,
    this.openings,
    this.floorAreaMm2, {
    this.roomRecords = const [],
    this.construction = const [],
    this.constructionDiagnostics = const [],
    this.floorSurfaces,
    this.wallPaints = const [],
    List<Ring>? drawn,
  }) : _drawn = drawn;

  /// The UNAMENDED occupied region (R3.2.1): what rooms, snapping and hit
  /// testing use. Walls are painted from [drawn].
  final List<Ring> occupied;

  final List<Ring>? _drawn;

  /// The region to PAINT: the occupied region with opening cuts (R4.2.2),
  /// computed by the core; the unamended region when there are no cuts.
  List<Ring> get drawn => _drawn ?? occupied;
  final List<Ring> rooms;
  final List<Aperture> openings;
  final double floorAreaMm2;

  /// Every room record on the level, in stable-UUID order. Separate from
  /// `rooms` because faces and records are separate things: a face exists with
  /// no record, and a record persists with no face.
  final List<RoomRecordView> roomRecords;
  final List<PlanConstruction> construction;
  final List<String> constructionDiagnostics;
  final List<Ring>? floorSurfaces;
  final List<WallPaintView> wallPaints;

  static const empty = Derived([], [], [], 0.0);
}

class WallPaintView {
  WallPaintView.fromJson(Map<String, Object?> j)
    : color = j['color'] as String?,
      ring = Ring.fromJson({...j, 'holes': const <Object?>[]});
  final String? color;
  final Ring ring;
}

/// Paint-only architectural geometry supplied by Rust, on either stair level.
class PlanConstruction {
  const PlanConstruction({
    required this.id,
    required this.kind,
    required this.name,
    required this.outline,
    required this.holes,
    required this.segments,
    required this.labelAt,
    required this.direction,
    this.staircase,
  });
  final String id, kind, name, direction;
  final List<Offset> outline;
  final List<List<Offset>> holes;
  final List<(Offset, Offset)> segments;
  final Offset labelAt;
  final Map<String, Object?>? staircase;
  factory PlanConstruction.fromJson(Map<String, Object?> j) {
    Offset pt(Object? p) {
      final a = p as List<Object?>;
      return Offset((a[0] as num).toDouble(), (a[1] as num).toDouble());
    }

    return PlanConstruction(
      id: j['id'] as String,
      kind: j['kind'] as String,
      name: j['name'] as String,
      outline: (j['outlineMm'] as List<Object?>).map(pt).toList(),
      holes: (j['holesMm'] as List<Object?>)
          .map((h) => (h as List<Object?>).map(pt).toList())
          .toList(),
      segments: (j['segmentsMm'] as List<Object?>).map((s) {
        final a = s as List<Object?>;
        return (pt(a[0]), pt(a[1]));
      }).toList(),
      labelAt: pt(j['labelMm']),
      direction: j['direction'] as String,
      staircase: j['staircase'] as Map<String, Object?>?,
    );
  }
}

/// One NON-ACTIVE level, drawn beneath the active one (BEHAVIOR-SPEC §2.4).
///
/// Carries no selection state. The editor must independently exclude passive
/// levels from targeting, snapping and document edits; rendering alone cannot
/// enforce that boundary.
///
/// Geometry lists are borrowed, not copied. Callers must not mutate them after
/// publication; replace changed payloads so identity-based repaint detection
/// remains valid without walking or copying every polygon each frame.
///
/// `ghosted` distinguishes the two rendered non-active tiers: `false` is the
/// VISIBLE tier (normal rendering, below the active level), `true` the GHOSTED
/// tier (reduced emphasis, below visible). Hidden levels are not represented:
/// they are simply absent from [PlanPainter.backgroundLevels].
@immutable
class PlanLayer {
  const PlanLayer({
    required this.levelId,
    required this.walls,
    required this.derived,
    this.ghosted = false,
    this.assets = const [],
  });

  final String levelId;
  final List<Wall> walls;
  final Derived derived;
  final bool ghosted;
  final List<AssetFootprint> assets;

  /// Identity comparison on `walls` and `derived`, matching the convention
  /// [PlanPainter.shouldRepaint] already uses for the active level: these
  /// lists are rebuilt wholesale when the document changes, so identity is
  /// both correct and O(1). Deep equality would walk every polygon on every
  /// frame to learn what a pointer comparison already knows.
  @override
  bool operator ==(Object other) =>
      other is PlanLayer &&
      other.levelId == levelId &&
      other.ghosted == ghosted &&
      other.walls == walls &&
      other.derived == derived &&
      listEquals(other.assets, assets);

  @override
  int get hashCode =>
      Object.hash(levelId, ghosted, walls, derived, Object.hashAll(assets));
}

class PlanPainter extends CustomPainter {
  const PlanPainter(
    this.walls,
    this.derived,
    this.selected,
    this.draft, {
    required this.view,
    this.units = UnitSystem.metric,
    this.showGrid = true,
    this.selectedJoin,
    this.selectedIds = const {},
    this.selectedJoins = const {},
    this.backgroundLevels = const [],
    this.marqueeRect,
    this.editPreview,
    this.openingPreview,
    this.warningIds = const {},
    this.doorSwingPreview,
    this.snapPoint,
    this.snapGuide,
    this.assets = const [],
    this.assetPreview,
    this.rotatingAssetId,
    this.groupPreview,
    this.modelPreviews = const {},
    this.constructionPreview = const [],
    this.resizingStairId,
    this.showStairResizeHandles = true,
  });

  final List<Wall> walls;
  final Derived derived;
  final List<AssetFootprint> assets;
  final (String, Offset)? assetPreview;
  final String? rotatingAssetId;

  /// Same logical-pixel handle position used by canvas hit testing.
  Offset? get assetRotationHandle {
    final asset = assets.where((a) => a.id == rotatingAssetId).firstOrNull;
    return asset == null ? null : assetRotationHandlePosition(asset, view);
  }

  final (Set<String>, Offset)? groupPreview;
  final Map<String, ui.Image> modelPreviews;
  final List<PlanConstruction> constructionPreview;
  final String? resizingStairId;
  final bool showStairResizeHandles;

  List<StairResizeHandle> get stairHandles {
    if (!showStairResizeHandles ||
        selected == null ||
        selectedIds.length > 1 ||
        groupPreview != null)
      return const [];
    final item =
        (resizingStairId == selected && constructionPreview.isNotEmpty
                ? constructionPreview
                : derived.construction)
            .where((s) => s.id == selected && s.kind == 'staircase')
            .firstOrNull;
    return item == null ? const [] : stairResizeHandles(item.outline, view);
  }

  final PlanViewport view;
  final UnitSystem units;
  final bool showGrid;

  /// Non-active levels, painted beneath the active one in the order given.
  ///
  /// The CALLER owns the order and must supply it bottom-to-top: ghosted
  /// levels first, then visible ones, each tier by ascending elevation with a
  /// stable-ID tie-break (§2.4, §2.2). The painter does not sort — it has no
  /// elevation to sort by, and inventing one here would put a second ordering
  /// rule beside the level controls' own.
  final List<PlanLayer> backgroundLevels;

  /// Selected wall or join identity, if any.
  final String? selected;
  final Offset? selectedJoin;
  final Set<String> selectedIds;
  final Map<String, Offset> selectedJoins;

  /// The wall being dragged out right now, before it is committed.
  final (Offset, Offset)? draft;

  /// The marquee rectangle being dragged out right now (issue 0059), as two
  /// opposite corners in plan mm, either order. VIEW-ONLY: no native query
  /// backs this while it is shown — see `_marqueeRect` in `main.dart`.
  final (Offset, Offset)? marqueeRect;

  /// Transient edited wall axes and thicknesses, before commit.
  final List<(Offset, Offset, double)>? editPreview;

  /// The candidate aperture span (id, endpoints in plan mm, host thickness).
  final (String, Offset, Offset, double)? openingPreview;
  final Set<String> warningIds;

  /// Candidate leaf tip; hinge remains at the core-derived point.
  final (String, Offset)? doorSwingPreview;

  final Offset? snapPoint;
  final (Offset, Offset)? snapGuide;

  /// How many walls the most recent `paint` drew FOR THE ACTIVE LEVEL.
  ///
  /// Written from inside `paint` so the first-frame probe reports what the
  /// render path did, not what the state object believed it would do.
  ///
  /// Background layers are deliberately NOT counted. The probe asks "did the
  /// document I am editing reach the canvas", and a count that grew with the
  /// number of levels merely visible underneath would answer a different
  /// question and would stop failing on exactly the bug it exists to catch.
  static int wallsPainted = 0;

  /// Reduced-emphasis alpha for a ghosted layer.
  ///
  /// Applied PER PAINT rather than through `saveLayer`. `saveLayer` would give
  /// true group opacity — overlapping shapes inside one layer would not
  /// darken each other — but it allocates and composites an offscreen buffer
  /// per layer per frame, which is the expensive thing to do on every pan of a
  /// multi-level plan. The visible trade is that a ghosted layer's own
  /// overlapping geometry (a wall crossing its room fill) composites slightly
  /// darker than a grouped version would. That is acceptable for a tier whose
  /// only job is spatial context, and it is recorded here rather than
  /// discovered later.
  static const double ghostOpacity = 0.32;

  /// Content bounds of the walls and derived geometry SUPPLIED, computed only
  /// after document changes.
  ///
  /// Describes its arguments and nothing else: it takes one level's content
  /// and returns that level's extent. Which levels a fit should span is the
  /// CALLER's decision — call it per level and union the results to fit
  /// everything drawable, or pass the active level alone. `null` when the
  /// supplied content has no extent.
  static Rect? boundsOf(
    List<Wall> walls,
    Derived derived, {
    List<AssetFootprint> assets = const [],
  }) {
    var minX = double.infinity, minY = double.infinity;
    var maxX = -double.infinity, maxY = -double.infinity;
    void see(Offset p) {
      minX = math.min(minX, p.dx);
      minY = math.min(minY, p.dy);
      maxX = math.max(maxX, p.dx);
      maxY = math.max(maxY, p.dy);
    }

    for (final w in walls) {
      see(w.start);
      see(w.end);
    }
    for (final asset in assets) {
      asset.corners.forEach(see);
    }
    for (final r in derived.occupied) {
      r.outer.forEach(see);
    }
    for (final item in derived.construction) {
      item.outline.forEach(see);
      for (final segment in item.segments) {
        see(segment.$1);
        see(segment.$2);
      }
    }
    for (final opening in derived.openings) {
      opening.cut.forEach(see);
      for (final segment in opening.glyphSegments) {
        see(segment.$1);
        see(segment.$2);
      }
      if (opening.isDoor && opening.glyphSegments.isEmpty) {
        see(opening.hinge);
        see(opening.tip);
      }
    }
    if (minX > maxX) return null;
    return Rect.fromLTRB(minX, minY, maxX, maxY);
  }

  Offset toScreen(Offset mm) => view.toScreen(mm);

  Path _pathOf(Ring r) {
    final path = Path();
    void ring(List<Offset> pts) {
      if (pts.isEmpty) return;
      path.moveTo(toScreen(pts.first).dx, toScreen(pts.first).dy);
      for (final p in pts.skip(1)) {
        path.lineTo(toScreen(p).dx, toScreen(p).dy);
      }
      path.close();
    }

    ring(r.outer);
    // Holes wind the other way; `evenOdd` turns them into voids rather than
    // overdrawing them.
    for (final h in r.holes) {
      ring(h);
    }
    path.fillType = PathFillType.evenOdd;
    return path;
  }

  /// `base` at `opacity`, multiplying whatever alpha it already carries.
  static Color _fade(Color base, double opacity) =>
      opacity >= 1.0 ? base : base.withValues(alpha: base.a * opacity);

  @override
  void paint(Canvas canvas, Size size) {
    // Labels and footprints near an edge must not spill past the plan pane
    // (over the rulers' neighbours, the split divider or the 3D view).
    canvas.clipRect(Offset.zero & size);
    // ONE grid, under everything. Drawing it per layer would darken it in
    // proportion to how many levels happen to be visible.
    if (showGrid) _paintGrid(canvas, size, rulers: false);

    // Passive levels, bottom to top in the order the caller supplied. No
    // selection is available to them and none is drawn: §2.4 makes non-active
    // levels visual only.
    for (final layer in backgroundLevels) {
      _paintLevel(
        canvas,
        layer.walls,
        layer.derived,
        opacity: layer.ghosted ? ghostOpacity : 1.0,
        countWalls: false,
      );
      _paintAssets(
        canvas,
        layer.assets,
        opacity: layer.ghosted ? ghostOpacity : 1.0,
        active: false,
      );
    }

    // The active level, at full emphasis, above every passive one.
    _paintLevel(canvas, walls, derived, opacity: 1.0, countWalls: true);
    _paintAssets(canvas, assets, opacity: 1.0, active: true);
    if (constructionPreview.isNotEmpty) {
      _paintLevel(
        canvas,
        const [],
        Derived(
          const [],
          const [],
          const [],
          0,
          construction: constructionPreview,
        ),
        opacity: 0.65,
        countWalls: false,
      );
    }

    _paintSelectionAndDraft(canvas);

    if (showGrid) _paintGrid(canvas, size, rulers: true);
    for (final handle in stairHandles) {
      final rect = Rect.fromCenter(
        center: handle.position,
        width: 10,
        height: 10,
      );
      canvas.drawRect(rect, Paint()..color = Colors.white);
      canvas.drawRect(
        rect,
        Paint()
          ..color = const Color(0xFFE0663A)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    final handle = assetRotationHandle;
    if (handle != null) {
      final asset = assets.firstWhere((a) => a.id == rotatingAssetId);
      final ink = Paint()
        ..color = const Color(0xFF167D8D)
        ..strokeWidth = 2;
      canvas.drawLine(toScreen(asset.position), handle, ink);
      canvas.drawCircle(handle, 10, Paint()..color = Colors.white);
      canvas.drawCircle(handle, 10, ink..style = PaintingStyle.stroke);
      canvas.drawArc(
        Rect.fromCircle(center: handle, radius: 5),
        -math.pi / 2,
        math.pi * 1.5,
        false,
        ink,
      );
    }
  }

  void _paintAssets(
    Canvas canvas,
    List<AssetFootprint> models, {
    required double opacity,
    required bool active,
  }) {
    for (final asset in models) {
      final delta = active && (groupPreview?.$1.contains(asset.id) ?? false)
          ? groupPreview!.$2
          : active && assetPreview?.$1 == asset.id
          ? assetPreview!.$2
          : Offset.zero;
      final points = asset.corners.map((p) => toScreen(p + delta)).toList();
      if (points.length != 4 ||
          points.any((p) => !p.dx.isFinite || !p.dy.isFinite))
        continue;
      final path = Path()..addPolygon(points, true);
      final chosen =
          active && (selected == asset.id || selectedIds.contains(asset.id));
      // A tilted model's cached, untilted image is not its top projection.
      final image =
          asset.rotationXDeg % 360 != 0 || asset.rotationYDeg % 360 != 0
          ? null
          : modelPreviews[asset.previewKey];
      if (image != null) {
        // Native footprint corners transport the transform, including rotation.
        // Image top-left is local (-width/2,+depth/2), hence corner 3.
        final x = (points[2] - points[3]) / image.width.toDouble();
        final y = (points[0] - points[3]) / image.height.toDouble();
        canvas.save();
        canvas.transform(
          Float64List.fromList([
            x.dx,
            x.dy,
            0,
            0,
            y.dx,
            y.dy,
            0,
            0,
            0,
            0,
            1,
            0,
            points[3].dx,
            points[3].dy,
            0,
            1,
          ]),
        );
        canvas.drawImage(
          image,
          Offset.zero,
          Paint()
            ..filterQuality = FilterQuality.medium
            ..color = Color.fromRGBO(255, 255, 255, opacity),
        );
        canvas.restore();
      } else
        canvas.drawPath(
          path,
          Paint()
            ..color = _fade(
              chosen ? const Color(0xFFB8DDD5) : const Color(0xFFE0D6C5),
              opacity,
            ),
        );
      if (chosen || image == null)
        canvas.drawPath(
          path,
          Paint()
            ..color = _fade(
              chosen ? const Color(0xFF28796C) : const Color(0xFF80725F),
              opacity,
            )
            ..style = PaintingStyle.stroke
            ..strokeWidth = chosen ? 2 : 1,
        );
      final center = image == null
          ? toScreen(asset.position + delta)
          : Offset(
              toScreen(asset.position + delta).dx,
              points.map((p) => p.dy).reduce(math.max) + 10,
            );
      final label = TextPainter(
        text: TextSpan(
          text: asset.name,
          style: TextStyle(
            fontFamily: editorFontFamily,
            fontFamilyFallback: editorFontFallback,
            fontSize: 11,
            color: _fade(const Color(0xFF33475B), opacity),
          ),
        ),
        // One line, never broken mid-word: a name may run wider than a
        // small footprint, up to 180 px, and is ellipsised beyond that.
        maxLines: 1,
        ellipsis: '…',
        textAlign: TextAlign.center,
        textDirection: TextDirection.ltr,
      )..layout(maxWidth: 180);
      label.paint(canvas, center - Offset(label.width / 2, label.height / 2));
      label.dispose();
    }
  }

  /// One level's geometry, all of it from the core.
  ///
  /// Every tier renders the SAME things — rooms, their area labels, the
  /// occupied region, centre-line fallback and apertures — and differs only in
  /// `opacity`. §2.4 defines the visible tier as "normal rendering" below the
  /// active level, so dropping an annotation the active level shows would be a
  /// rendering decision the specification has not made; tiering the existing
  /// output is not.
  void _paintLevel(
    Canvas canvas,
    List<Wall> levelWalls,
    Derived levelDerived, {
    required double opacity,
    required bool countWalls,
  }) {
    final scale = view.zoom;

    // Rooms first, beneath the walls, each labelled with the area the CORE
    // measured.
    final roomFill = Paint()..color = _fade(const Color(0xFFEFF3F7), opacity);
    for (final room in levelDerived.floorSurfaces ?? levelDerived.rooms) {
      canvas.drawPath(_pathOf(room), roomFill);
    }
    for (final room in levelDerived.rooms) {
      _labelRoom(canvas, room, opacity);
    }

    // Aperture tint BENEATH the wall mass: the core's drawn region already
    // has the cuts (R4.2.2), so a fill painted over it would erase join
    // patches and other walls' material that the cut never removes.
    for (final o in levelDerived.openings) {
      if (o.cut.isEmpty) continue;
      final path = Path()
        ..moveTo(toScreen(o.cut.first).dx, toScreen(o.cut.first).dy);
      for (final p in o.cut.skip(1)) {
        path.lineTo(toScreen(p).dx, toScreen(p).dy);
      }
      path.close();
      canvas.drawPath(
        path,
        Paint()
          ..color = _fade(
            o.isDoor ? const Color(0xFFF7F6F3) : const Color(0xFFBCD3E8),
            opacity,
          ),
      );
    }

    // The drawn region: one filled path per component, holes included.
    final colored = levelDerived.wallPaints.isNotEmpty;
    if (colored && opacity < 1) {
      // Composite the wall mass once: overlapping source contours must not
      // make a ghosted colored level darker than an ordinary ghosted level.
      canvas.saveLayer(
        null,
        Paint()..color = Colors.white.withValues(alpha: opacity),
      );
    }
    final wallOpacity = colored ? 1.0 : opacity;
    final wallFill = Paint()
      ..color = _fade(const Color(0xFF33475B), wallOpacity);
    for (final part in levelDerived.drawn) {
      canvas.drawPath(_pathOf(part), wallFill);
    }
    if (levelDerived.wallPaints.isNotEmpty) {
      final clip = Path()..fillType = PathFillType.evenOdd;
      for (final part in levelDerived.drawn) {
        clip.addPath(_pathOf(part), Offset.zero);
      }
      canvas.save();
      canvas.clipPath(clip, doAntiAlias: false);
      for (final part in levelDerived.wallPaints) {
        canvas.drawPath(
          _pathOf(part.ring),
          Paint()
            ..color = _fade(
              part.color == null
                  ? const Color(0xFF33475B)
                  : solidColor(part.color!),
              wallOpacity,
            ),
        );
      }
      canvas.restore();
    }
    if (colored && opacity < 1) canvas.restore();

    for (final item in levelDerived.construction) {
      if (item.id == resizingStairId &&
          constructionPreview.isNotEmpty &&
          !identical(levelDerived.construction, constructionPreview))
        continue;
      canvas.save();
      final moving = groupPreview;
      // Both floor drawings represent the same assembly. Move the passive
      // drawing too, but keep selection emphasis exclusive to the active one.
      if (moving != null && moving.$1.contains(item.id)) {
        canvas.translate(moving.$2.dx * scale, -moving.$2.dy * scale);
      }
      final selectedConstruction =
          countWalls && (item.id == selected || selectedIds.contains(item.id));
      final path = Path()..fillType = PathFillType.evenOdd;
      for (final ring in [item.outline, ...item.holes]) {
        if (ring.isEmpty) continue;
        final first = toScreen(ring.first);
        path.moveTo(first.dx, first.dy);
        for (final p in ring.skip(1)) {
          final s = toScreen(p);
          path.lineTo(s.dx, s.dy);
        }
        path.close();
      }
      canvas.drawPath(
        path,
        Paint()
          ..color = _fade(
            selectedConstruction
                ? const Color(0x66E0663A)
                : const Color(0xFFE6DFD2),
            opacity,
          ),
      );
      final ink = Paint()
        ..color = _fade(
          selectedConstruction
              ? (item.id == selected
                    ? const Color(0xFFE0663A)
                    : const Color(0xFF73A7A0))
              : const Color(0xFF405363),
          opacity,
        )
        ..style = PaintingStyle.stroke
        ..strokeWidth = math.max(1.0, 12 * scale);
      canvas.drawPath(path, ink);
      for (final segment in item.segments) {
        canvas.drawLine(toScreen(segment.$1), toScreen(segment.$2), ink);
      }
      if (item.direction.isNotEmpty) {
        final text = TextPainter(
          text: TextSpan(
            text: item.direction,
            style: TextStyle(
              fontSize: 11,
              color: _fade(const Color(0xFF243B53), opacity),
              backgroundColor: _fade(const Color(0xFFE6DFD2), opacity),
            ),
          ),
          textDirection: TextDirection.ltr,
        )..layout();
        final at = toScreen(item.labelAt);
        text.paint(canvas, at - Offset(text.width / 2, text.height + 3));
        text.dispose();
      }
      canvas.restore();
    }

    // Centre lines are drawn only when the core has not produced a region yet
    // — a single unjoined wall still has one, but an empty plan does not.
    var drawn = 0;
    if (levelDerived.occupied.isEmpty) {
      for (final w in levelWalls) {
        final paint = Paint()
          ..color = _fade(const Color(0xFF33475B), opacity)
          ..strokeWidth = math.max(1.0, w.thicknessMm * scale)
          ..strokeCap = StrokeCap.butt;
        canvas.drawLine(toScreen(w.start), toScreen(w.end), paint);
        drawn++;
      }
    } else {
      drawn = levelWalls.length;
    }
    if (countWalls) wallsPainted = drawn;

    // The leaf and glyph over the wall mass: a door reads as a gap in the
    // wall with a leaf swinging out of it. Every point came from the core.
    for (final o in levelDerived.openings) {
      final glyph = o.glyphSegments.isNotEmpty
          ? o.glyphSegments
          : o.isDoor
          ? [(o.hinge, o.tip)]
          : <(Offset, Offset)>[];
      for (final segment in glyph) {
        canvas.drawLine(
          toScreen(segment.$1),
          toScreen(segment.$2),
          Paint()
            ..color = _fade(
              o.isDoor
                  ? (o.leafColor == null
                        ? const Color(0xFF8A6F3C)
                        : solidColor(o.leafColor!))
                  : const Color(0xFF517B9C),
              opacity,
            )
            ..strokeWidth = math.max(1.0, (o.isDoor ? 30.0 : 20.0) * scale),
        );
      }
    }
  }

  /// Selection and the uncommitted drag, over everything.
  ///
  /// Reads only the ACTIVE level's walls. A passive layer has no selection to
  /// draw, and could not supply one: [PlanLayer] carries none.
  void _paintSelectionAndDraft(Canvas canvas) {
    final scale = view.zoom;
    for (final wall in walls) {
      if (!warningIds.contains(wall.id)) continue;
      canvas.drawLine(
        toScreen(wall.start),
        toScreen(wall.end),
        Paint()
          ..color = const Color(0xCCFFB300)
          ..strokeWidth = math.max(8, wall.thicknessMm * scale + 12)
          ..strokeCap = StrokeCap.butt,
      );
    }
    for (final opening in derived.openings) {
      if (opening.id != selected && !selectedIds.contains(opening.id)) continue;
      final path = Path();
      if (opening.cut.isEmpty) continue;
      path.moveTo(
        toScreen(opening.cut.first).dx,
        toScreen(opening.cut.first).dy,
      );
      for (final point in opening.cut.skip(1)) {
        path.lineTo(toScreen(point).dx, toScreen(point).dy);
      }
      path.close();
      canvas.drawPath(
        path,
        Paint()
          ..color = opening.id == selected
              ? const Color(0x66E0663A)
              : const Color(0x6673A7A0),
      );
      canvas.drawPath(
        path,
        Paint()
          ..color = opening.id == selected
              ? const Color(0xFFE0663A)
              : const Color(0xFF73A7A0)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    if (openingPreview case (final _, final a, final b, final thickness)) {
      canvas.drawLine(
        toScreen(a),
        toScreen(b),
        Paint()
          ..color = const Color(0x9928796C)
          ..strokeWidth = math.max(2, thickness * scale)
          ..strokeCap = StrokeCap.butt,
      );
    }
    if (doorSwingPreview case (final id, final tip)) {
      final opening = derived.openings.where((o) => o.id == id).firstOrNull;
      if (opening != null) {
        canvas.drawLine(
          toScreen(opening.hinge),
          toScreen(tip),
          Paint()
            ..color = const Color(0xFF28796C)
            ..strokeWidth = 3,
        );
      }
    }
    final chosen = walls.where(
      (w) => w.id == selected || selectedIds.contains(w.id),
    );
    for (final w in chosen) {
      canvas.drawLine(
        toScreen(w.start),
        toScreen(w.end),
        Paint()
          ..color = w.id == selected
              ? const Color(0xFFE0663A)
              : const Color(0x9973A7A0)
          ..strokeWidth = math.max(2.0, w.thicknessMm * scale)
          ..strokeCap = StrokeCap.butt,
      );
    }

    for (final edit in editPreview ?? const <(Offset, Offset, double)>[]) {
      canvas.drawLine(
        toScreen(edit.$1),
        toScreen(edit.$2),
        Paint()
          ..color = const Color(0x9973A7D8)
          ..strokeWidth = math.max(2.0, edit.$3 * scale)
          ..strokeCap = StrokeCap.butt,
      );
    }

    for (final entry in selectedJoins.entries) {
      if (entry.key == selected) continue;
      canvas.drawCircle(
        toScreen(entry.value),
        5,
        Paint()..color = const Color(0x9973A7A0),
      );
    }
    final corner = selectedJoin;
    if (corner != null) {
      canvas.drawCircle(
        toScreen(corner),
        6,
        Paint()..color = const Color(0xFFE0663A),
      );
      canvas.drawCircle(toScreen(corner), 3, Paint()..color = Colors.white);
    }
    // The wall being dragged out, so the drag is visible before it commits.
    final d = draft;
    if (d != null) {
      canvas.drawLine(
        toScreen(d.$1),
        toScreen(d.$2),
        Paint()
          ..color = const Color(0x8833475B)
          ..strokeWidth = math.max(2.0, 200.0 * scale)
          ..strokeCap = StrokeCap.butt,
      );
    }
    final guide = snapGuide;
    if (guide != null) {
      canvas.drawLine(
        toScreen(guide.$1),
        toScreen(guide.$2),
        Paint()
          ..color = const Color(0xCC28796C)
          ..strokeWidth = 1.5,
      );
    }
    final point = snapPoint;
    if (point != null) {
      canvas.drawCircle(
        toScreen(point),
        5,
        Paint()
          ..color = const Color(0xFF28796C)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 2,
      );
    }
    // The marquee rectangle being dragged out (issue 0059). Purely a VIEW of
    // the two corners the pointer is at right now: no native query informs
    // this shape, and none of its content previews what would be selected.
    final m = marqueeRect;
    if (m != null) {
      final rect = Rect.fromPoints(toScreen(m.$1), toScreen(m.$2));
      canvas.drawRect(rect, Paint()..color = const Color(0x2233475B));
      canvas.drawRect(
        rect,
        Paint()
          ..color = const Color(0xFF33475B)
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1.0,
      );
    }
  }

  /// Iterates only visible grid lines: work scales with viewport pixels, not
  /// document extent or zoom. Rulers are a final overlay over plan geometry.
  void _paintGrid(Canvas canvas, Size size, {required bool rulers}) {
    final step = view.gridStepMm;
    final minor = Paint()..color = const Color(0xFFECEDE8);
    final major = Paint()..color = const Color(0xFFD9DED6);
    final axis = Paint()..color = const Color(0xFF97B4AB);
    if (rulers) {
      // Opaque: a translucent band let plan labels show through the scale.
      final background = Paint()..color = const Color(0xFFFAFAF7);
      canvas.drawRect(Rect.fromLTWH(0, 0, size.width, 18), background);
      canvas.drawRect(Rect.fromLTWH(0, 0, 36, size.height), background);
    }
    void label(double mm, Offset position) {
      final rulerUnitMm = units == UnitSystem.metric ? 1.0 : 304.8;
      final digits = math.max(
        0,
        (-math.log(step / rulerUnitMm) / math.ln10).ceil(),
      );
      final text = TextPainter(
        text: TextSpan(
          text: (mm / rulerUnitMm).toStringAsFixed(digits),
          style: const TextStyle(
            fontFamily: editorFontFamily,
            fontFamilyFallback: editorFontFallback,
            fontSize: 9,
            color: Color(0xFF65746C),
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      text.paint(canvas, position);
      text.dispose();
    }

    for (final line in view.gridLines(size.width)) {
      final x = line.pixel;
      if (rulers) {
        if (line.major && x >= 36 && line.millimetres != null) {
          label(line.millimetres!, Offset(x + 2, 2));
        }
      } else {
        canvas.drawLine(
          Offset(x, 0),
          Offset(x, size.height),
          line.axis
              ? axis
              : line.major
              ? major
              : minor,
        );
      }
    }
    for (final line in view.gridLines(size.height, horizontal: true)) {
      final y = line.pixel;
      if (rulers) {
        if (line.major && y >= 18 && line.millimetres != null) {
          label(line.millimetres!, Offset(2, y + 2));
        }
      } else {
        canvas.drawLine(
          Offset(0, y),
          Offset(size.width, y),
          line.axis
              ? axis
              : line.major
              ? major
              : minor,
        );
      }
    }
    if (rulers) {
      final text = TextPainter(
        text: TextSpan(
          text: units == UnitSystem.metric ? 'mm' : 'ft',
          style: const TextStyle(
            fontFamily: editorFontFamily,
            fontSize: 9,
            color: Color(0xFF65746C),
          ),
        ),
        textDirection: TextDirection.ltr,
      )..layout();
      text.paint(canvas, const Offset(2, 2));
      text.dispose();
    }
  }

  /// The room's label: its name and the area, both as the CORE resolved them.
  ///
  /// Three things here come from the other side of the bridge and are not
  /// recomputed: WHETHER there is a label at all (a face with no candidates
  /// shows none, R5.3.3), WHAT it says (the effective record's name, or the
  /// `Room` fallback), and WHERE it sits (that record's seed, not the ring's
  /// centroid — which for an L-shaped room is outside the room).
  ///
  /// Metric uses the core's area string. It used to be `toStringAsFixed(2)` of
  /// the raw area, which is not R5.3.2: that rule resolves a half to the EVEN last
  /// digit, so 30.245 m² shows as 30.24 and Dart's rounding gives 30.25. A
  /// normative rule implemented twice is a rule implemented differently.
  /// Imperial is only a display conversion of the core's unrounded area.
  void _labelRoom(Canvas canvas, Ring room, double opacity) {
    final anchor = room.labelAnchorMm;
    final label = room.label;
    final area = room.displayAreaM2;
    if (anchor == null || label == null || area == null) return;

    final at = toScreen(anchor);
    final tp = TextPainter(
      text: TextSpan(
        children: [
          TextSpan(
            text: '$label\n',
            style: TextStyle(
              fontFamily: editorFontFamily,
              fontFamilyFallback: editorFontFallback,
              color: _fade(const Color(0xFF33475B), opacity),
              fontSize: 13,
              fontWeight: FontWeight.w600,
            ),
          ),
          TextSpan(
            text: units.area(room.areaMm2, metricText: area),
            style: TextStyle(
              fontFamily: editorFontFamily,
              fontFamilyFallback: editorFontFallback,
              color: _fade(const Color(0xFF6B7A88), opacity),
              fontSize: 12,
            ),
          ),
        ],
      ),
      textAlign: TextAlign.center,
      textDirection: TextDirection.ltr,
    )..layout();
    tp.paint(canvas, at - Offset(tp.width / 2, tp.height / 2));
    tp.dispose();
  }

  @override
  bool shouldRepaint(PlanPainter old) =>
      old.constructionPreview != constructionPreview ||
      old.resizingStairId != resizingStairId ||
      old.showStairResizeHandles != showStairResizeHandles ||
      old.units != units ||
      old.assets != assets ||
      old.assetPreview != assetPreview ||
      old.rotatingAssetId != rotatingAssetId ||
      old.groupPreview != groupPreview ||
      old.modelPreviews != modelPreviews ||
      old.walls != walls ||
      old.derived != derived ||
      old.selected != selected ||
      old.selectedJoin != selectedJoin ||
      old.selectedIds != selectedIds ||
      old.selectedJoins != selectedJoins ||
      old.draft != draft ||
      old.marqueeRect != marqueeRect ||
      old.snapPoint != snapPoint ||
      old.snapGuide != snapGuide ||
      old.view != view ||
      old.showGrid != showGrid ||
      !listEquals(old.backgroundLevels, backgroundLevels);
}
