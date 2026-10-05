package document

import (
	"fmt"
	"math"
	"sort"
)

// FromSnapshot builds a projection seeded from snapshot contents
// (FORMAT-SPEC §5.1 S1–S5) so that replaying the surviving operation log
// on top yields exactly the never-compacted projection for every
// permitted delivery order (V5/V6):
//
//   - entities, tombstones, winner positions, frontier, creation
//     positions, and the shared batch-position table are deep-copied
//     from s;
//   - per-actor counter chaining resumes from retained batch metadata when
//     the caller supplies it (S5), else from the implied reservation
//     [1..watermark]. B3 forbids gaps inside an actor stream, so a
//     watermark W implies reservations covering exactly [1..W]; the
//     implication adds no masking information because delivered batches are
//     classified against the watermark BEFORE any range chaining
//     (frameAndOrder): fully-covered redeliveries skip silently (I2/V5),
//     straddling ranges reject as torn.
//
// retained batches must be consistent with s or the constructor fails:
// framing validity, disjoint contiguous increasing ranges per actor, each
// actor's last retained range ending exactly at its watermark, and F4
// frontier closure across the retained set. The function never repairs,
// guesses, or silently drops inconsistencies (fail closed).
func FromSnapshot(s State, retained []*Batch) (*Projection, error) {
	p := New()
	for _, b := range retained {
		if b == nil {
			return nil, wrapFraming("snapshot retains an absent batch")
		}
		if err := validateFraming(b); err != nil {
			return nil, fmt.Errorf("snapshot retained batch %s: %w", b.BatchID.String(), err)
		}
		if prev, dup := p.batches[b.BatchID]; dup {
			return nil, wrapFraming("snapshot retains duplicate batch id %s", prev.BatchID.String())
		}
		p.batches[b.BatchID] = b
	}
	// Structural checks over the retained set. They run here, on the way
	// in, because a snapshot's retained batches are exactly "the retained
	// log" §4.5 I2 names, and no entry of it is exempt for lying below a
	// watermark.
	if err := validateNoRangeOverlap(retained); err != nil {
		return nil, err
	}
	if err := validateFrontierClosure(retained); err != nil {
		return nil, err
	}

	// The shared batch-position table (S3) is seeded before any position
	// is validated, because a position is only meaningful against the
	// frontier that defines it.
	for ref, fr := range s.BatchPositions {
		if ref.Actor == (UUID{}) {
			return nil, wrapFraming("snapshot batch position names a zero actorId")
		}
		if ref.Base < CounterBaseMin {
			return nil, wrapFraming("snapshot batch position for %s has counterBase %d below %d",
				ref.Actor.String(), ref.Base, CounterBaseMin)
		}
		for a, n := range fr {
			if a == (UUID{}) {
				return nil, wrapFraming("snapshot batch position %s names a zero actorId in its frontier", ref.String())
			}
			if n < 0 {
				return nil, wrapFraming("snapshot batch position %s carries a negative counter for %s",
					ref.String(), a.String())
			}
		}
		// B5: causal metadata advances past a batch only to its END, so a
		// batch's own frontier entry is strictly below its counterBase. A
		// self-observing frontier would inflate that batch's causalPastSize
		// by its own operations and could reorder it against a genuine
		// concurrent peer. NewBatch already refuses this for real batches;
		// a snapshot's synthesized table must be held to the same rule.
		if own, named := fr[ref.Actor]; named && own >= ref.Base {
			return nil, wrapFraming(
				"snapshot batch position %s claims observation of its own reserved range (own entry %d >= counterBase %d)",
				ref.String(), own, ref.Base)
		}
		if err := p.positions.note(ref, fr); err != nil {
			return nil, err
		}
	}
	// Every retained batch is also a position source: a late arrival may
	// be ordered against a batch the snapshot still carries.
	for _, b := range retained {
		if err := p.positions.note(batchRefOf(b), b.Frontier); err != nil {
			return nil, err
		}
	}

	for a, w := range s.Frontier {
		if a == (UUID{}) {
			return nil, wrapFraming("snapshot frontier names a zero actorId")
		}
		if w < 0 {
			return nil, wrapFraming("snapshot frontier counter for %s is negative", a.String())
		}
		// Normalize zero to absent at the snapshot S2 frontier boundary. F1
		// defines absence as nothing observed; this makes the two spellings
		// equivalent for snapshot watermark state. NOTE: equivalence does NOT
		// hold at snapshot position authentication (§5.1 S3), where the position
		// table compares map representations literally — a separate open finding.
		if w == 0 {
			continue
		}
		var rs []counterRange
		for _, b := range retainedByActor(retained, a) {
			rs = append(rs, counterRange{b.CounterBase, b.EndCounter()})
		}
		for i, r := range rs {
			if r.base < CounterBaseMin || r.end < r.base {
				return nil, wrapFraming(
					"retained batch range [%d,%d] of %s is invalid", r.base, r.end, a.String())
			}
			if i > 0 && r.base != rs[i-1].end+1 {
				return nil, wrapFraming(
					"retained batches for %s break disjoint contiguous ordering at [%d,%d]",
					a.String(), r.base, r.end)
			}
		}
		if len(rs) > 0 {
			if rs[len(rs)-1].end != w {
				return nil, wrapFraming(
					"actor %s: retained history ends at %d but frontier watermark is %d",
					a.String(), rs[len(rs)-1].end, w)
			}
		} else if w > 0 {
			// Implied reservation covering everything retention dropped
			// for this actor (B3 forbids in-stream gaps).
			rs = []counterRange{{1, w}}
		}
		p.ranges[a] = rs
		p.frontier[a] = w
	}
	for id := range p.batches {
		b := p.batches[id]
		w, ok := p.frontier[b.ActorID]
		if !ok {
			return nil, wrapFraming(
				"snapshot retains batch %s of actor %s absent from the frontier",
				id.String(), b.ActorID.String())
		}
		if b.EndCounter() > w {
			return nil, wrapFraming(
				"snapshot retains unapplied batch %s (ends %d above watermark %d)",
				id.String(), b.EndCounter(), w)
		}
	}

	for i, e := range s.Entities {
		if e == nil {
			return nil, wrapFraming("snapshot entity %d is absent", i)
		}
		id := e.EntityID()
		if id == (UUID{}) {
			return nil, wrapFraming("snapshot entity %d carries a zero id", i)
		}
		if _, dup := p.entities[id]; dup {
			return nil, wrapFraming("snapshot contains duplicate entity id %s", id.String())
		}
		if _, gone := s.Tombstones[id]; gone {
			return nil, wrapFraming("snapshot entity %s is simultaneously live and tombstoned", id.String())
		}
		p.entities[id] = cloneEntity(e)
	}

	// Positions are validated against the shared table: a reference to a
	// batch the table does not carry is absent required data (§5.1 S3,
	// §9.4) and must never be ordered against a fabricated zero.
	checkPos := func(w position, what string) error {
		if w.Ref.Actor == (UUID{}) {
			return wrapFraming("snapshot %s names a zero actorId", what)
		}
		if w.Ref.Base < CounterBaseMin {
			return wrapFraming("snapshot %s has counterBase %d below %d", what, w.Ref.Base, CounterBaseMin)
		}
		if err := validateOperationIndex(w.OpIndex); err != nil {
			return fmt.Errorf("snapshot %s: %w", what, err)
		}
		// The operation counter this position names is counterBase +
		// operationIndex (B3). Row L13 bounds it to signed 64 bits, and the
		// check is written as a subtraction because the addition it stands
		// for is exactly what would overflow.
		if w.Ref.Base > math.MaxInt64-w.OpIndex {
			return wrapFraming(
				"snapshot %s names operation counter %d + %d, which overflows signed 64-bit (L13)",
				what, w.Ref.Base, w.OpIndex)
		}
		// S2 is "the causal frontier of ALL operations the snapshot
		// covers", so a retained position cannot name an operation above
		// its own watermark: the snapshot would be citing a write it does
		// not claim to have applied.
		counter := w.Ref.Base + w.OpIndex
		if wm := p.frontier[w.Ref.Actor]; counter > wm {
			return wrapFraming(
				"snapshot %s names operation counter %d of actor %s, above the S2 watermark %d",
				what, counter, w.Ref.Actor.String(), wm)
		}
		if _, ok := p.positions.frontiers[w.Ref]; !ok {
			return wrapFraming(
				"snapshot %s references batch %s, which batchPositions does not carry: absent required data",
				what, w.Ref.String())
		}
		return nil
	}

	for id, w := range s.Tombstones {
		if id == (UUID{}) {
			return nil, wrapFraming("snapshot tombstone names a zero entityId")
		}
		if err := checkPos(w, "tombstone for "+id.String()); err != nil {
			return nil, err
		}
		p.tombstones[id] = w
	}
	for f, w := range s.WinnerPositions {
		if err := checkPos(w, fmt.Sprintf("winner position %s.%s", f.Entity.String(), f.Field)); err != nil {
			return nil, err
		}
		p.winnerPositions[f] = w
	}
	for id, w := range s.CreatedPositions {
		_, live := p.entities[id]
		_, gone := p.tombstones[id]
		if !live && !gone {
			// S1 retains creation positions for entities WITHIN THE
			// HORIZON, live or tombstoned. An id that is neither is not a
			// horizon member and the snapshot cannot say what it is.
			return nil, wrapFraming(
				"snapshot creation position names %s, which is neither live nor tombstoned", id.String())
		}
		if err := checkPos(w, "creation position for "+id.String()); err != nil {
			return nil, err
		}
		p.createdPositions[id] = w
	}
	// A tombstone without its creation position collapses I5 branches 1
	// and 2 (§5.1 S4): the live interval cannot be formed, so a late batch
	// positioned BEFORE the true creation would be applied where full
	// replay discards it whole. That is a silently wrong projection, which
	// is worse than a refusal.
	for id, deleting := range p.tombstones {
		created, ok := p.createdPositions[id]
		if !ok {
			return nil, wrapFraming(
				"snapshot tombstone for %s carries no creation position: the live interval cannot be formed (S4)",
				id.String())
		}
		// The interval is [creationPosition, deletingPosition): a delete
		// closes an interval its own create opened, so creation is strictly
		// earlier. An inverted or empty interval contains no position at
		// all, which would make I5 classify every late batch as branch 1 or
		// branch 3 and silently discard writes that full replay applies.
		if p.positions.compare(created, deleting) >= 0 {
			return nil, wrapFraming(
				"snapshot live interval for %s is empty or inverted: creation position is not strictly before the deleting position (S4/I5)",
				id.String())
		}
	}
	// A LIVE entity may legitimately lack a creation position: §5.1 S1
	// scopes createdPositions to entities "within the horizon", and
	// `lwwHorizon` is Provisional with no value asserted. Such an entity is
	// simply present, and LWW4 cannot be re-decided against a late
	// concurrent create for it — which §5.1 already states of pre-horizon
	// metadata generally. Only the TOMBSTONE case above is a reject, and
	// only because §5.1 S4 says so in terms.
	// EXACTLY the referenced batches, in both directions (§5.1 S3). The
	// missing direction is checked above, one reference at a time; this is
	// the surplus direction, which no single reference can catch.
	referenced := map[batchRef]bool{}
	for _, w := range p.winnerPositions {
		referenced[w.Ref] = true
	}
	for _, w := range p.createdPositions {
		referenced[w.Ref] = true
	}
	for _, w := range p.tombstones {
		referenced[w.Ref] = true
	}
	for ref := range s.BatchPositions {
		if !referenced[ref] {
			return nil, wrapFraming(
				"snapshot batchPositions carries batch %s, which no winner, creation, or tombstone position references",
				ref.String())
		}
	}

	// Retained batches authenticate the positions that name them.
	if err := p.authenticateAgainstBatches(retained); err != nil {
		return nil, err
	}

	// The seeded state is the replay root: later deliveries replay
	// baseline ⊕ canonical(delivered log), never re-applying the retained
	// batches whose effects the snapshot already folds in.
	p.baseline = p.captureBaseline()
	return p, nil
}

func retainedByActor(retained []*Batch, a UUID) []*Batch {
	var out []*Batch
	for _, b := range retained {
		if b != nil && b.ActorID == a {
			out = append(out, b)
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].CounterBase < out[j].CounterBase })
	return out
}
