import 'package:domowik_editor/selection_state.dart';
import 'package:flutter/gestures.dart' show PointerDeviceKind;
import 'package:flutter_test/flutter_test.dart';

void main() {
  SelectionContext ctx({
    Object viewKey = const (0.1, 0.0, 0.0),
    String? levelId = 'L1',
    int documentRevision = 0,
    int visibilityRevision = 0,
    int modifierMask = 0,
    PointerDeviceKind pointerKind = PointerDeviceKind.mouse,
  }) => SelectionContext(
    viewKey: viewKey,
    levelId: levelId,
    documentRevision: documentRevision,
    visibilityRevision: visibilityRevision,
    modifierMask: modifierMask,
    pointerKind: pointerKind,
  );

  group('SelectionSnapshot', () {
    test('ids are unmodifiable and value-equal regardless of set identity', () {
      final s = SelectionSnapshot({'A', 'B'}, 'B');
      expect(() => s.ids.add('C'), throwsUnsupportedError);
      expect(s, SelectionSnapshot({'B', 'A'}, 'B'));
    });

    test('a primary not in ids is rejected in release builds too', () {
      // ArgumentError, not assert: this is a public constructor, and an
      // impossible snapshot must be unconstructible even where asserts are
      // stripped (release builds).
      expect(() => SelectionSnapshot({'A'}, 'B'), throwsArgumentError);
    });
  });

  group('L9 — one entity, multiple native features, one cycle stop', () {
    test('dedupe by first occurrence; three clicks cycle [A, B]', () {
      final c = SelectionController();
      const raw = ['A', 'A', 'A', 'B'];
      const pos = Offset(0, 0);
      c.click(raw, pos, ctx());
      expect(c.ids, {'A'});
      expect(c.primary, 'A');
      c.click(raw, pos, ctx());
      expect(c.ids, {'B'});
      expect(c.primary, 'B');
      c.click(raw, pos, ctx());
      expect(c.ids, {'A'});
      expect(c.primary, 'A');
    });
  });

  group('L10 — fixed anchor and round-trip reset observations', () {
    test('anchor stays at the first click; in-radius clicks advance', () {
      final c = SelectionController();
      const list = ['A', 'B', 'C'];
      c.click(list, const Offset(400, 300), ctx());
      expect(c.primary, 'A');
      c.click(list, const Offset(412, 300), ctx());
      expect(c.primary, 'B', reason: 'inclusive-boundary in-radius advance');
    });

    test('an out-of-radius excursion resets even after returning', () {
      final c = SelectionController();
      const list = ['A', 'B', 'C'];
      c.click(list, const Offset(400, 300), ctx());
      c.click(list, const Offset(412, 300), ctx());
      expect(c.primary, 'B');
      c.observePointer(const Offset(412.25, 300)); // > 12px from (400,300)
      c.observePointer(const Offset(400, 300)); // back to the anchor
      c.click(list, const Offset(400, 300), ctx());
      expect(c.primary, 'A', reason: 'reset restarts at first, never C');
    });

    for (final field in [
      'view',
      'level',
      'visibility',
      'modifier',
      'pointer',
    ]) {
      test('$field round trip resets the cycle', () {
        final c = SelectionController();
        const list = ['A', 'B', 'C'];
        const pos = Offset(400, 300);
        c.click(list, pos, ctx());
        expect(c.primary, 'A');
        switch (field) {
          case 'view':
            c.observeContext(ctx(viewKey: const (0.2, 10.0, 0.0)));
            c.observeContext(ctx());
          case 'level':
            c.observeContext(ctx(levelId: 'L2'));
            c.observeContext(ctx());
          case 'visibility':
            c.observeContext(ctx(visibilityRevision: 1));
            c.observeContext(ctx(visibilityRevision: 2));
          case 'modifier':
            c.observeContext(ctx(modifierMask: shiftModifier));
            c.observeContext(ctx());
          case 'pointer':
            c.observeContext(ctx(pointerKind: PointerDeviceKind.touch));
            c.observeContext(ctx());
        }
        c.click(list, pos, ctx());
        expect(c.primary, 'A', reason: 'restart at first, not the successor B');
      });
    }

    test('an accepted distant edit resets even with an unchanged local list; '
        'a refused/no-op edit or read/save does not', () {
      final c = SelectionController();
      const list = ['A', 'B', 'C'];
      const pos = Offset(400, 300);
      c.click(list, pos, ctx());
      expect(c.primary, 'A');
      c.observeContext(ctx(documentRevision: 1)); // accepted elsewhere
      c.click(list, pos, ctx(documentRevision: 1));
      expect(c.primary, 'A', reason: 'candidate-set change always resets');

      // No document-revision bump: a refused edit, no-op undo, read or
      // save must not disturb the still-active cycle.
      c.click(list, pos, ctx(documentRevision: 1));
      expect(c.primary, 'B', reason: 'unchanged revision keeps cycling');
    });
  });

  test(
    'a no-op context observation and a no-op survivor prune preserve cycling',
    () {
      final c = SelectionController();
      const list = ['A', 'B', 'C'];
      const pos = Offset(400, 300);
      c.click(list, pos, ctx());
      c.observeContext(ctx()); // identical value: not a change
      c.retainSurvivors({'A', 'B', 'C'}); // removes nothing
      c.click(list, pos, ctx());
      expect(c.primary, 'B', reason: 'neither call disturbed the cycle');
    },
  );

  group('marquee (R7.3.2)', () {
    test('replacement dedupes native order and makes the last ID primary', () {
      final c = SelectionController();
      c.marquee(const ['B', 'A', 'B', 'C', 'A'], additive: false);
      expect(c.ids, {'A', 'B', 'C'});
      expect(c.primary, 'C');
    });

    test('additive adds only new IDs and makes the last new ID primary', () {
      final c = SelectionController()
        ..marquee(const ['A', 'B'], additive: false);
      c.marquee(const ['B', 'C', 'C', 'A', 'D'], additive: true);
      expect(c.ids, {'A', 'B', 'C', 'D'});
      expect(c.primary, 'D');
    });

    test('additive no-new and empty results preserve the exact selection', () {
      final c = SelectionController()
        ..marquee(const ['A', 'B'], additive: false);
      c.marquee(const ['B', 'A', 'B'], additive: true);
      expect(c.snapshot, SelectionSnapshot({'A', 'B'}, 'B'));
      c.marquee(const [], additive: true);
      expect(c.snapshot, SelectionSnapshot({'A', 'B'}, 'B'));
    });

    test('empty replacement clears primary and selection', () {
      final c = SelectionController()..marquee(const ['A'], additive: false);
      c.marquee(const [], additive: false);
      expect(c.snapshot, SelectionSnapshot.empty);
    });

    test(
      'accepted marquee ends cycling, so the next plain click starts first',
      () {
        final c = SelectionController();
        const position = Offset(0, 0);
        c.click(const ['A', 'B'], position, ctx());
        expect(c.primary, 'A');
        c.marquee(const ['B'], additive: true);
        expect(c.primary, 'B');
        c.click(const ['A', 'B'], position, ctx());
        expect(c.primary, 'A');
      },
    );

    test(
      'replacement (additive: false) discards the OLD selection entirely',
      () {
        // Starting from a NONEMPTY prior selection, unlike the dedup/primary
        // test above: a replacement that merely UNIONED in the new IDs would
        // pass that test too, so this one must observe the old ID gone.
        final c = SelectionController()..marquee(const ['X'], additive: false);
        expect(c.ids, {'X'});
        c.marquee(const ['A', 'B'], additive: false);
        expect(c.ids, {'A', 'B'});
        expect(
          c.ids.contains('X'),
          isFalse,
          reason: 'the old selection must be REPLACED, not merged into',
        );
        expect(c.primary, 'B');
      },
    );

    test('additive marquee onto an ALREADY-unset primary keeps it unset when '
        'nothing is new, and sets it when something is', () {
      // R7.4: primary can be unset while ids is non-empty (e.g. after
      // deleting the primary). A marquee that finds nothing new must not
      // silently invent a primary just because an already-selected ID
      // intersected the rectangle.
      final c = SelectionController()
        ..restoreSelection(SelectionSnapshot({'A'}, null));
      c.marquee(const ['A'], additive: true);
      expect(
        c.snapshot,
        SelectionSnapshot({'A'}, null),
        reason:
            'A is already selected: nothing new, primary stays UNSET, '
            'not silently reassigned to A',
      );

      c.marquee(const ['A', 'B'], additive: true);
      expect(
        c.snapshot,
        SelectionSnapshot({'A', 'B'}, 'B'),
        reason:
            'B is new: primary becomes the last NEWLY added ID, even '
            'though the prior primary was unset',
      );
    });

    test('an additive marquee with no new IDs still ends cycling, even though '
        'selection and primary do not change', () {
      // Each click in a cycle REPLACES `ids` with the singleton `{chosen}`
      // (it is not accumulating), so "advance the cycle to B" leaves
      // `ids == {B}` — the marquee below must repeat exactly that ID to
      // be a genuine no-new-addition, not merely reuse one of several.
      final c = SelectionController();
      const list = ['A', 'B', 'C'];
      const pos = Offset(0, 0);
      c.click(list, pos, ctx());
      expect(c.primary, 'A');
      c.click(list, pos, ctx());
      expect(c.primary, 'B', reason: 'still the same cycle, advanced to B');
      expect(c.ids, {'B'});

      c.marquee(const ['B'], additive: true);
      expect(
        c.snapshot,
        SelectionSnapshot({'B'}, 'B'),
        reason: 'B is already selected: nothing new, nothing changes',
      );

      c.click(list, pos, ctx());
      expect(
        c.primary,
        'A',
        reason:
            'the no-op marquee still ended the cycle, so this restarts '
            'at the top rather than continuing on to C',
      );
    });
  });

  // PH1's selection-state half, which needs no placeholder entity to exist.
  //
  // This is the claim that the controller needs NO change for R7.5.3, made
  // checkable instead of asserted. The controller stores opaque IDs, so a
  // placeholder instance ID travels the same path a wall ID does. What the
  // controller cannot cover, and what still blocks R7.5.3, is everything that
  // decides WHICH ids arrive: the `placeholder-box` hit class, the section 9.6
  // box, and the document entity behind them.
  group('PH1 — placeholder IDs travel the ordinary selection path', () {
    const instance = '019534c7-0000-7000-8000-000000000009';

    test('a marquee containing the box selects the instance as primary', () {
      final c = SelectionController();
      c.marquee(const [instance], additive: false);
      expect(c.snapshot, SelectionSnapshot({instance}, instance));
    });

    test('deleting the primary placeholder empties the selection and unsets '
        'primary', () {
      final c = SelectionController()
        ..marquee(const [instance], additive: false);
      // PH1's deletion step, through the same R7.4 survivor path a deleted
      // wall takes: the instance ceases to exist, so it leaves the set, and
      // primary goes UNSET rather than moving to anything.
      c.retainSurvivors(const {});
      expect(c.snapshot, SelectionSnapshot.empty);
    });

    test('a placeholder and a wall in one marquee keep native order and the '
        'last-added primary', () {
      // R7.3.2 ranks by MARQUEE KEY, which spans classes: the caller applies
      // the core's order and the controller preserves it, whatever mix of
      // classes produced it.
      const wall = '019534c7-0000-7000-8000-000000000002';
      final c = SelectionController();
      c.marquee(const [wall, instance], additive: false);
      expect(c.ids, {wall, instance});
      expect(c.primary, instance);
    });
  });

  test('retainSurvivors unsets primary only when it is actually removed', () {
    final c = SelectionController();
    c.click(
      const ['A', 'B'],
      const Offset(0, 0),
      ctx(modifierMask: controlModifier),
    );
    c.click(
      const ['B'],
      const Offset(0, 0),
      ctx(modifierMask: controlModifier),
    );
    expect(c.ids, {'A', 'B'});
    expect(c.primary, 'B');
    c.retainSurvivors({'A'});
    expect(c.ids, {'A'});
    expect(
      c.primary,
      isNull,
      reason: 'R7.4: primary UNSET when it ceases to exist',
    );
  });

  group('modifiers (R7.2.4/R7.2.4a/R7.2.3a)', () {
    test(
      'Ctrl/Meta/Shift toggle the top-ranked candidate; Alt alone cycles',
      () {
        final c = SelectionController();
        c.click(
          const ['A'],
          const Offset(0, 0),
          ctx(modifierMask: controlModifier),
        );
        expect(c.snapshot, SelectionSnapshot({'A'}, 'A'));
        c.click(
          const ['A'],
          const Offset(0, 0),
          ctx(modifierMask: metaModifier),
        );
        expect(
          c.snapshot,
          SelectionSnapshot({}, null),
          reason: 'removing the primary unsets it',
        );

        final alt = SelectionController();
        alt.click(
          const ['A', 'B'],
          const Offset(0, 0),
          ctx(modifierMask: altModifier),
        );
        expect(alt.primary, 'A', reason: 'Alt alone does not enable toggling');
        alt.click(
          const ['A', 'B'],
          const Offset(0, 0),
          ctx(modifierMask: altModifier),
        );
        expect(alt.primary, 'B', reason: 'still an ordinary cycling click');
      },
    );

    test(
      'an empty click clears selection and ends the cycle, modifier or not',
      () {
        final c = SelectionController();
        c.click(const ['A', 'B'], const Offset(0, 0), ctx());
        c.click(const [], const Offset(0, 0), ctx(modifierMask: shiftModifier));
        expect(c.snapshot, SelectionSnapshot({}, null));
      },
    );

    test('L12 — a held toggle modifier toggles the same top target and ends '
        'the cycle; a following plain click restarts it', () {
      final c = SelectionController();
      const raw = ['A', 'A', 'B']; // dedupes to the projected list [A, B]
      const pos = Offset(0, 0);
      final shift = ctx(modifierMask: shiftModifier);

      c.click(raw, pos, shift);
      expect(c.snapshot, SelectionSnapshot({'A'}, 'A'));
      c.click(raw, pos, shift);
      expect(
        c.snapshot,
        SelectionSnapshot({}, null),
        reason:
            'repeated held-modifier click toggles the SAME top '
            'target off, not the next overlapping entity',
      );
      c.click(raw, pos, shift);
      expect(c.snapshot, SelectionSnapshot({'A'}, 'A'));

      // Every toggle click ended the cycle, so switching to plain
      // restarts at the top-ranked entity rather than continuing to
      // whatever B's position in the list would have been.
      c.click(raw, pos, ctx());
      expect(
        c.primary,
        'A',
        reason: 'plain click after toggling restarts at first',
      );
      c.click(raw, pos, ctx());
      expect(c.primary, 'B', reason: 'now a genuine continuing plain cycle');
    });
  });

  group('L11 — potential drag preserves or restores the correct selection', () {
    test('press on the current primary preserves the whole set', () {
      final c = SelectionController();
      c.click(
        const ['A', 'B'],
        const Offset(0, 0),
        ctx(modifierMask: controlModifier),
      );
      c.click(
        const ['B'],
        const Offset(0, 0),
        ctx(modifierMask: controlModifier),
      );
      expect(c.snapshot, SelectionSnapshot({'A', 'B'}, 'B'));
      c.selectForDrag('B');
      expect(c.snapshot, SelectionSnapshot({'A', 'B'}, 'B'));
    });

    test('press elsewhere tentatively narrows; a click restores pre-press '
        'selection first; cancel restores it exactly', () {
      final pre = SelectionSnapshot({'A', 'B'}, 'B');
      final c = SelectionController()..restoreSelection(pre);
      c.selectForDrag('A');
      expect(c.snapshot, SelectionSnapshot({'A'}, 'A'));

      // Release classifies as a click: caller restores pre-press first.
      c.restoreSelection(pre);
      c.click(const ['A'], const Offset(0, 0), ctx());
      expect(c.snapshot, SelectionSnapshot({'A'}, 'A'));

      // A different gesture instead cancels.
      c.selectForDrag('A');
      c.restoreSelection(pre);
      expect(c.snapshot, pre);
    });
  });
}
