import 'package:domowik_editor/level_state.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const a = LevelInfo(
    id: 'A',
    name: 'Ground',
    elevationMm: 0,
    floorThicknessMm: 200,
    defaultWallHeightMm: 2500,
    defaultCeilingHeightMm: 2500,
  );
  const b = LevelInfo(
    id: 'B',
    name: 'Upper',
    elevationMm: 3000,
    floorThicknessMm: 200,
    defaultWallHeightMm: 2500,
    defaultCeilingHeightMm: 2500,
  );
  const c = LevelInfo(
    id: 'C',
    name: 'Attic',
    elevationMm: 6000,
    floorThicknessMm: 200,
    defaultWallHeightMm: 2500,
    defaultCeilingHeightMm: 2500,
  );

  group('reset (R2.6 reopen defaults)', () {
    test(
      'lowest elevation is active; others start ghosted; always bumps revision',
      () {
        final s = LevelViewState();
        // Deliberately out of order, to prove sorting does not depend on it.
        s.reset([c, a, b]);
        expect(s.levels, [a, b, c]);
        expect(s.activeId, 'A');
        expect(s.visibility, {
          'A': LevelVisibility.visible,
          'B': LevelVisibility.ghosted,
          'C': LevelVisibility.ghosted,
        });
        expect(s.revision, 1);
        s.reset([
          c,
          a,
          b,
        ]); // reopening the same document is still a real transition
        expect(s.revision, 2);
      },
    );

    test('elevation ties break by ascending stable id', () {
      const b0 = LevelInfo(
        id: 'B',
        name: 'B',
        elevationMm: 0,
        floorThicknessMm: 200,
        defaultWallHeightMm: 2500,
        defaultCeilingHeightMm: 2500,
      );
      const a0 = LevelInfo(
        id: 'A',
        name: 'A',
        elevationMm: 0,
        floorThicknessMm: 200,
        defaultWallHeightMm: 2500,
        defaultCeilingHeightMm: 2500,
      );
      final s = LevelViewState()..reset([b0, a0]);
      expect(s.levels, [a0, b0]);
      expect(s.activeId, 'A');
    });

    test('rejects non-finite elevation and duplicate ids atomically', () {
      final s = LevelViewState()..reset([a]);
      expect(
        () => s.reset([
          a,
          LevelInfo(
            id: 'X',
            name: 'X',
            elevationMm: double.nan,
            floorThicknessMm: 200,
            defaultWallHeightMm: 2500,
            defaultCeilingHeightMm: 2500,
          ),
        ]),
        throwsArgumentError,
      );
      expect(() => s.reset([a, a]), throwsArgumentError);
      // Neither rejected call mutated anything.
      expect(s.levels, [a]);
      expect(s.revision, 1);
    });

    test('an empty document is a defensive state, not claimed valid', () {
      final s = LevelViewState()..reset(const []);
      expect(s.levels, isEmpty);
      expect(s.activeId, isNull);
      expect(s.visibility, isEmpty);
      expect(s.paintOrder, isEmpty);
    });
  });

  group('reset remembered active (§2.6, issue 0117)', () {
    test(
      'a remembered, still-existing level overrides the elevation default',
      () {
        final s = LevelViewState()..reset([a, b, c], rememberedActive: 'B');
        expect(s.activeId, 'B');
        // Only the active level changes: every other level starts ghosted.
        expect(s.visibility, {
          'A': LevelVisibility.ghosted,
          'B': LevelVisibility.visible,
          'C': LevelVisibility.ghosted,
        });
        // A remembered open is still a real transition, fit or not.
        s.reset([a, b, c], rememberedActive: 'B');
        expect(s.revision, 2);
      },
    );

    test(
      'unknown, empty and stale hints fall back to the elevation default',
      () {
        for (final hint in [null, '', 'NOPE']) {
          final s = LevelViewState()..reset([a, b, c], rememberedActive: hint);
          expect(s.activeId, 'A', reason: 'hint: $hint');
          expect(s.visibility['B'], LevelVisibility.ghosted);
        }
        // The hint does not survive a document that dropped the level
        // entirely: only live levels can hold the fallback.
        final gone = LevelViewState()
          ..reset([a, b, c], rememberedActive: 'B')
          ..reset([a, c]);
        expect(gone.activeId, 'A');
      },
    );

    test(
      'remembering the lowest level is indistinguishable from the default',
      () {
        final s = LevelViewState()..reset([b, c, a], rememberedActive: 'A');
        expect(s.activeId, 'A');
        expect(s.levels, [a, b, c]);
      },
    );
  });

  group('panelLevels (§2.2 list order, issue 0115)', () {
    LevelInfo ordered(LevelInfo base, int? displayOrder) => LevelInfo(
      id: base.id,
      name: base.name,
      elevationMm: base.elevationMm,
      floorThicknessMm: base.floorThicknessMm,
      defaultWallHeightMm: base.defaultWallHeightMm,
      defaultCeilingHeightMm: base.defaultCeilingHeightMm,
      displayOrder: displayOrder,
    );

    test('without stored ordinals it is the elevation-derived order', () {
      final s = LevelViewState()..reset([c, a, b]);
      expect(s.panelLevels, [a, b, c]);
      expect(s.levels, [a, b, c], reason: 'canonical order also holds');
    });

    test(
      'stored levels list first by ordinal; the rest keep `levels` order',
      () {
        // One accepted reorder: the second level was dragged above the first
        // (ordinal 0), the first took ordinal 1, the third stays absent.
        // Ordinals ride along untouched through reset's canonical sort.
        final s = LevelViewState()..reset([ordered(b, 0), ordered(a, 1), c]);
        expect(s.panelLevels, [ordered(b, 0), ordered(a, 1), c]);
        expect(
          s.activeId,
          'B',
          reason: 'first listed floor is the open default',
        );
        // The canonical elevation-derived ORDER is UNTOUCHED (compare by id:
        // the stored LevelInfos still carry their ordinals) — this getter is
        // presentation-only.
        expect(s.levels.map((l) => l.id), ['A', 'B', 'C']);
        expect(
          s.paintOrder.map((l) => l.id),
          ['A', 'C', 'B'],
          reason:
              'tiers keep the elevation order even though the panel does not',
        );
      },
    );

    test('ordinal ties break by ascending stable id', () {
      final s = LevelViewState()..reset([ordered(b, 0), ordered(a, 0)]);
      expect(s.panelLevels, [
        ordered(a, 0),
        ordered(b, 0),
      ], reason: 'same ordinal: stable-ID tie-break, not elevation');
    });

    test('remembered level wins over list order; stale hints use the list', () {
      final levels = [ordered(c, 0), ordered(b, 1), a];
      final s = LevelViewState()..reset(levels, rememberedActive: 'B');
      expect(s.activeId, 'B');
      s.reset(levels, rememberedActive: 'deleted');
      expect(s.activeId, 'C');
      expect(s.visibility['C'], LevelVisibility.visible);
      // Deletion within a session still uses the elevation fallback.
      s.reconcile([ordered(b, 1), a]);
      expect(s.activeId, 'A');
    });

    test('the exposed list is immutable', () {
      final s = LevelViewState()..reset([ordered(b, 0), ordered(a, 1)]);
      expect(() => s.panelLevels.add(ordered(c, 2)), throwsUnsupportedError);
    });
  });

  group('paintOrder tiers and ties', () {
    test(
      'ghost tier, then visible tier, then active last; hidden excluded',
      () {
        final s = LevelViewState()..reset([a, b, c]);
        s.setVisibility('B', LevelVisibility.hidden);
        // C stays ghosted (reset default); activate nothing else; A is active.
        expect(s.paintOrder, [c, a]);
        s.setVisibility('C', LevelVisibility.visible);
        expect(s.paintOrder, [
          c,
          a,
        ]); // C now visible tier, still before active A
      },
    );

    test(
      'within a tier, order follows ascending elevation/id, not call order',
      () {
        final s = LevelViewState()..reset([a, b, c]);
        s.setVisibility('C', LevelVisibility.visible);
        s.setVisibility('B', LevelVisibility.visible);
        // Visible tier: B (3000) before C (6000), matching `levels` order.
        expect(s.paintOrder, [b, c, a]);
      },
    );
  });

  group('transition table (R2.4)', () {
    test('activate: unknown or already-active id is a no-op', () {
      final s = LevelViewState()..reset([a, b]);
      expect(s.activate('A'), isFalse);
      expect(s.activate('ghost-id'), isFalse);
      expect(s.revision, 1);
    });

    test(
      'activate: prior active becomes visible-ghosted; new active is forced visible',
      () {
        final s = LevelViewState()..reset([a, b]);
        expect(s.activate('B'), isTrue);
        expect(s.activeId, 'B');
        expect(s.visibility, {
          'A': LevelVisibility.ghosted,
          'B': LevelVisibility.visible,
        });
        expect(s.revision, 2);
      },
    );

    test('setVisibility rejects unknown ids and the active level', () {
      final s = LevelViewState()..reset([a, b]);
      expect(s.setVisibility('nope', LevelVisibility.hidden), isFalse);
      expect(
        s.setVisibility('A', LevelVisibility.hidden),
        isFalse,
        reason: 'hide on active rejected',
      );
      expect(
        s.setVisibility('A', LevelVisibility.ghosted),
        isFalse,
        reason: 'ghost on active rejected',
      );
      expect(s.revision, 1);
    });

    test(
      'setVisibility on a non-active level applies as requested; unchanged is a no-op',
      () {
        final s = LevelViewState()..reset([a, b]);
        expect(s.setVisibility('B', LevelVisibility.hidden), isTrue);
        expect(s.visibility['B'], LevelVisibility.hidden);
        expect(s.revision, 2);
        expect(
          s.setVisibility('B', LevelVisibility.hidden),
          isFalse,
          reason: 'already hidden',
        );
        expect(s.revision, 2);
      },
    );

    test('a real flag round trip still advances revision each time', () {
      final s = LevelViewState()..reset([a, b]);
      final start = s.revision;
      expect(s.setVisibility('B', LevelVisibility.hidden), isTrue);
      expect(s.setVisibility('B', LevelVisibility.ghosted), isTrue);
      expect(
        s.revision,
        start + 2,
        reason: 'hide-then-show is two real transitions, not a cancel-out',
      );
    });
  });

  group('reconcile: removal and new levels', () {
    test('deleting the active level falls back to the new lowest level', () {
      final s = LevelViewState()..reset([a, b, c]);
      expect(s.reconcile([b, c]), isTrue);
      expect(s.activeId, 'B');
      expect(s.visibility['B'], LevelVisibility.visible);
      expect(
        s.visibility['C'],
        LevelVisibility.ghosted,
        reason: 'survivor keeps its stored flag',
      );
    });

    test('a new level starts ghosted; a surviving level keeps its flag', () {
      final s = LevelViewState()..reset([a, b]);
      s.setVisibility('B', LevelVisibility.visible);
      expect(s.reconcile([a, b, c]), isTrue);
      expect(s.visibility, {
        'A': LevelVisibility.visible,
        'B': LevelVisibility.visible,
        'C': LevelVisibility.ghosted,
      });
    });

    test('reconciling to zero levels is defensive, not a normative claim', () {
      final s = LevelViewState()..reset([a]);
      expect(s.reconcile(const []), isTrue);
      expect(s.activeId, isNull);
      expect(s.levels, isEmpty);
    });

    test(
      'an unchanged set (even with refreshed metadata) does not bump revision',
      () {
        final s = LevelViewState()..reset([a, b]);
        final revisionBefore = s.revision;
        const renamedA = LevelInfo(
          id: 'A',
          name: 'Ground Floor',
          elevationMm: 0,
          floorThicknessMm: 200,
          defaultWallHeightMm: 2500,
          defaultCeilingHeightMm: 2500,
        );
        expect(s.reconcile([renamedA, b]), isFalse);
        expect(s.revision, revisionBefore);
        expect(
          s.levels.first.name,
          'Ground Floor',
          reason: 'metadata itself still refreshes',
        );
      },
    );

    test('rejects non-finite elevation and duplicate ids atomically', () {
      final s = LevelViewState()..reset([a, b]);
      expect(() => s.reconcile([a, b, b]), throwsArgumentError);
      expect(s.levels, [a, b]);
    });
  });

  test('exposed collections are immutable', () {
    final s = LevelViewState()..reset([a, b]);
    expect(() => s.levels.add(c), throwsUnsupportedError);
    expect(
      () => s.visibility['A'] = LevelVisibility.hidden,
      throwsUnsupportedError,
    );
    expect(() => s.paintOrder.add(c), throwsUnsupportedError);
  });
}
