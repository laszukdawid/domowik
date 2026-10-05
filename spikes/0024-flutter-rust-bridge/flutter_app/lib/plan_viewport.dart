import 'dart:math' as math;
import 'dart:ui';

/// One visible, origin-aligned grid line. [millimetres] is absent when the
/// world coordinate cannot distinguish adjacent lattice indices in binary64;
/// callers must omit its label, rather than print a misleading measurement.
class PlanGridLine {
  const PlanGridLine(this.pixel, this.millimetres, this.major, this.axis);
  final double pixel;
  final double? millimetres;
  final bool major;
  final bool axis;
}

/// Local view state from BEHAVIOR-SPEC 1.2–1.5; it never owns document data.
///
/// Plan coordinates are millimetres with +Y upward. Screen coordinates are
/// logical pixels with +Y downward. Scale is clamped to [0.0001, 100] px/mm.
/// Immutable values let painting and input share an exact camera snapshot.
class PlanViewport {
  const PlanViewport._(this.zoom, this.origin);

  factory PlanViewport({required double zoom, required Offset origin}) {
    if (!zoom.isFinite || !origin.dx.isFinite || !origin.dy.isFinite) {
      throw ArgumentError('Viewport values must be finite');
    }
    return PlanViewport._(zoom.clamp(minZoom, maxZoom), origin);
  }

  static const double minZoom = 0.0001;
  static const double maxZoom = 100;
  static const double defaultZoom = 0.1;
  final double zoom;
  final Offset origin;

  /// Empty/degenerate content uses the default scale. Other bounds fit inside
  /// a five-percent margin per edge. Rejects invalid requests as a whole.
  factory PlanViewport.fit(Size size, Rect? bounds) {
    if (!size.width.isFinite ||
        !size.height.isFinite ||
        size.width < 0 ||
        size.height < 0 ||
        (bounds != null &&
            (!bounds.isFinite || bounds.width < 0 || bounds.height < 0))) {
      throw ArgumentError(
        'Fit needs finite viewport dimensions and ordered bounds',
      );
    }
    var zoom = defaultZoom;
    final centre = bounds?.center ?? Offset.zero;
    if (bounds != null && bounds.width > 0 && bounds.height > 0) {
      zoom = math
          .min(
            size.width * 0.9 / bounds.width,
            size.height * 0.9 / bounds.height,
          )
          .clamp(minZoom, maxZoom);
    }
    return PlanViewport(
      zoom: zoom,
      origin: Offset(
        size.width / 2 - centre.dx * zoom,
        size.height / 2 + centre.dy * zoom,
      ),
    );
  }

  Offset toScreen(Offset mm) =>
      Offset(origin.dx + mm.dx * zoom, origin.dy - mm.dy * zoom);
  Offset toPlan(Offset px) =>
      Offset((px.dx - origin.dx) / zoom, (origin.dy - px.dy) / zoom);

  PlanViewport pan(Offset delta) {
    final next = origin + delta;
    if (!next.dx.isFinite || !next.dy.isFinite) return this;
    return PlanViewport._(zoom, next);
  }

  /// Keeps the plan point under [anchor] fixed, including at the zoom clamps.
  /// Invalid gesture steps are no-ops, not partially applied camera changes.
  PlanViewport zoomAt(double factor, Offset anchor) {
    if (!factor.isFinite ||
        factor <= 0 ||
        !anchor.dx.isFinite ||
        !anchor.dy.isFinite) {
      return this;
    }
    final nextZoom = (zoom * factor).clamp(minZoom, maxZoom);
    final ratio = nextZoom / zoom;
    final nextOrigin = anchor + (origin - anchor) * ratio;
    if (!nextOrigin.dx.isFinite || !nextOrigin.dy.isFinite) return this;
    return PlanViewport._(nextZoom, nextOrigin);
  }

  /// One two-touch step: centroid translation and scale applied together.
  ///
  /// The plan point under [previousCentroid] ends up under [currentCentroid],
  /// so translation and the scale's anchor cannot disagree. [scale] is the
  /// ratio of current to previous finger distance; it saturates at the zoom
  /// clamp BEFORE anchoring, which is what keeps a pinch past a limit
  /// tracking the fingers instead of drifting.
  ///
  /// A non-finite, zero or negative [scale], a non-finite centroid, or an
  /// origin that overflows leaves the camera untouched: the step is discarded
  /// whole, never partially applied (R6.5.8).
  PlanViewport panZoom({
    required Offset previousCentroid,
    required Offset currentCentroid,
    required double scale,
  }) {
    if (!scale.isFinite ||
        scale <= 0 ||
        !previousCentroid.dx.isFinite ||
        !previousCentroid.dy.isFinite ||
        !currentCentroid.dx.isFinite ||
        !currentCentroid.dy.isFinite) {
      return this;
    }
    final nextZoom = (zoom * scale).clamp(minZoom, maxZoom);
    final ratio = nextZoom / zoom;
    final nextOrigin = currentCentroid + (origin - previousCentroid) * ratio;
    if (!nextOrigin.dx.isFinite || !nextOrigin.dy.isFinite) return this;
    return PlanViewport._(nextZoom, nextOrigin);
  }

  PlanViewport wheel(
    Offset delta,
    Offset anchor, {
    required bool zooming,
    required bool shift,
  }) {
    if (!delta.dx.isFinite ||
        !delta.dy.isFinite ||
        !anchor.dx.isFinite ||
        !anchor.dy.isFinite) {
      return this;
    }
    if (!zooming) return pan(shift ? Offset(delta.dx, 0) : Offset(0, delta.dy));
    // Exponents beyond this already saturate from either legal zoom extreme.
    // Bounding before pow avoids overflow on large but finite wheel events.
    final exponent = (-delta.dy / 100).clamp(-100.0, 100.0);
    return zoomAt(math.pow(1.25, exponent).toDouble(), anchor);
  }

  /// Smallest spacing in {1,2,5} × 10^n with at least eight logical pixels.
  double get gridStepMm {
    final needed = 8 / zoom;
    final decade = math
        .pow(10, (math.log(needed) / math.ln10).floor())
        .toDouble();
    for (final multiplier in const [1, 2, 5, 10]) {
      final step = decade * multiplier;
      if (step * zoom >= 8) return step;
    }
    return decade * 10;
  }

  /// Grid traversal in screen space, independent of inverse-coordinate range.
  ///
  /// Reducing the origin modulo five spacings preserves major-line phase
  /// without converting an unbounded lattice index to a machine integer.
  /// The counter starts at zero and is bounded by viewport pixels / spacing.
  /// Invalid or unrenderably large extents requiring more than [maxGridLines]
  /// lines are refused with an empty iterable; no camera/document state changes.
  /// [horizontal] selects horizontal lines (+Y plan maps toward smaller pixels).
  Iterable<PlanGridLine> gridLines(
    double extent, {
    bool horizontal = false,
  }) sync* {
    if (!extent.isFinite || extent <= 0) return;
    final step = gridStepMm;
    final spacing = step * zoom;
    final count = extent / spacing;
    if (count >= maxGridLines) return;
    final component = horizontal ? origin.dy : origin.dx;
    final majorPhase = component % (spacing * 5);
    final first = majorPhase % spacing;
    final firstMajor = ((majorPhase - first) / spacing).round();
    final limit = count.floor() + 1;
    for (var local = 0; local < limit; local++) {
      final pixel = first + local * spacing;
      if (pixel > extent) break;
      final coordinate =
          (horizontal ? component - pixel : pixel - component) / zoom;
      final lattice = coordinate / step;
      // All integers through 2^53-1 are exactly representable, but their
      // half-step rounding neighbourhood is not. Stay within 2^52 here.
      final index = lattice.isFinite && lattice.abs() < 4503599627370496
          ? lattice.round()
          : null;
      yield PlanGridLine(
        pixel,
        index == null ? null : index * step,
        local % 5 == firstMajor,
        index == 0,
      );
    }
  }

  /// Safety ceiling for an unsupported synthetic layout (>130k logical px at
  /// minimum spacing). Normal traversal still scales with actual viewport size.
  static const int maxGridLines = 16384;

  Rect visibleBounds(Size size) {
    final topLeft = toPlan(Offset.zero);
    final bottomRight = toPlan(Offset(size.width, size.height));
    return Rect.fromLTRB(
      topLeft.dx,
      bottomRight.dy,
      bottomRight.dx,
      topLeft.dy,
    );
  }

  @override
  bool operator ==(Object other) =>
      other is PlanViewport && zoom == other.zoom && origin == other.origin;

  @override
  int get hashCode => Object.hash(zoom, origin);
}
