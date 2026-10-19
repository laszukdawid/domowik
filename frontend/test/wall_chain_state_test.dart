import 'package:domowik_editor/wall_chain_state.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('an accepted segment records its stored endpoints and direction', () {
    final chain = WallChainState()..start(const Offset(0.1, 0.2));
    expect(chain.previousSegment, isNull);
    chain.accept(const Offset(3000.3, 1000.7));
    expect(chain.previousSegment, (
      const Offset(0.1, 0.2),
      const Offset(3000.3, 1000.7),
    ));
    expect(
      chain.previousDirection,
      const Offset(3000.3, 1000.7) - const Offset(0.1, 0.2),
    );
    expect(chain.anchor, const Offset(3000.3, 1000.7));
  });

  test('starting again or clearing forgets the previous segment', () {
    final chain = WallChainState()
      ..start(Offset.zero)
      ..accept(const Offset(10, 0));
    chain.start(const Offset(5, 5));
    expect(chain.previousSegment, isNull);
    chain
      ..accept(const Offset(6, 5))
      ..clear();
    expect(chain.previousSegment, isNull);
    expect(chain.anchor, isNull);
  });
}
