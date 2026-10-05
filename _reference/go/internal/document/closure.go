package document

import (
	"encoding/json"
	"sort"
)

// Frontier closure validation — FORMAT-SPEC §4.4 F4 obligation 2.
//
// F3's sort is a linear extension of causality only if frontiers are
// TRANSITIVELY CLOSED: an omitted entry under-reports `causalPastSize` and
// can invert a comparison against a concurrent batch that a complete
// frontier would have won. F4 makes closure an authoring requirement and
// makes reader validation a MUST wherever the observed ancestors are still
// retained. Obligation 3 records the limit deliberately: closure is NOT
// provable for a batch whose ancestors were compacted, and nothing here
// claims otherwise — an unchecked ancestor is silently absent from the
// comparison rather than treated as satisfied.
//
// A violation is an inconsistent frontier and rejects `malformed-framing`
// BEFORE replay (§4.3 row 4, §9.4). §4.5 I2 puts this ahead of watermark
// suppression: "watermark classification never precedes, gates, or
// replaces the pre-replay structural checks §4.3 row 4 applies to the
// retained log, and a retained range at or below the watermark is exempt
// from none of them."

// validateFrontierClosure checks every batch in known against every other
// batch in known that it observes. known must be the union of the retained
// and delivered sets, so a batch is compared against exactly the ancestors
// whose frontiers are actually available.
//
// Cost, stated as implemented rather than as an aspiration. Queries are
// built in ONE pass over every batch's frontier entries, so the query set
// costs Σ|frontier| rather than actors × batches. Each actor's batches are
// then swept once in ascending end-counter order while a running pointwise
// maximum of their frontiers is folded together, and each query costs one
// scan of that running maximum. The total is (Σ over batches of |frontier|)
// for the folds and the query build, plus (Σ over query pairs of |running
// maximum|) for the comparisons — NOT a logarithmic bound, because
// dominance is a dimension-wise test. Each individual frontier is bounded
// by row L18 and the batch count by row L23, but the RUNNING MAXIMUM is a
// union across batches and is bounded by the number of distinct actors the
// set names, which on a malformed set is not itself bounded by L18 —
// certification is what this function performs, so it may not assume its
// own conclusion.
//
// validateFrontierClosureNaive is the direct oracle this is differentially
// tested against; the optimization does not certify itself.
func validateFrontierClosure(known []*Batch) error {
	if err := validateOwnPriorEnds(known); err != nil {
		return err
	}
	if err := validateNoPartialRangeObservation(known); err != nil {
		return err
	}

	byActor := batchesByActor(known)
	actors := sortedActors(byActor)
	for _, a := range actors {
		sorted := byActor[a]
		sort.Slice(sorted, func(i, j int) bool {
			if sorted[i].EndCounter() != sorted[j].EndCounter() {
				return sorted[i].EndCounter() < sorted[j].EndCounter()
			}
			return sorted[i].BatchID.Compare(sorted[j].BatchID) < 0
		})
	}

	// One query per (observer, observed actor) pair, built in a single pass
	// over the frontier entries rather than by rescanning every batch once
	// per actor.
	type query struct {
		observer  *Batch
		threshold int64
	}
	queriesByActor := map[UUID][]query{}
	for _, b := range known {
		if b == nil {
			continue
		}
		for a, n := range b.Frontier {
			if n <= 0 {
				continue
			}
			queriesByActor[a] = append(queriesByActor[a], query{observer: b, threshold: n})
		}
	}

	for _, a := range actors {
		queries := queriesByActor[a]
		if len(queries) == 0 {
			continue
		}
		ancestors := byActor[a]
		sort.Slice(queries, func(i, j int) bool {
			if queries[i].threshold != queries[j].threshold {
				return queries[i].threshold < queries[j].threshold
			}
			return queries[i].observer.BatchID.Compare(queries[j].observer.BatchID) < 0
		})
		running := map[UUID]int64{}
		next := 0
		for _, q := range queries {
			for next < len(ancestors) && ancestors[next].EndCounter() <= q.threshold {
				for z, n := range ancestors[next].Frontier {
					if n > running[z] {
						running[z] = n
					}
				}
				next++
			}
			for z, n := range running {
				if q.observer.Frontier[z] < n {
					return wrapFraming(
						"batch %s observes actor %s through counter %d but omits or under-reports actor %s (has %d, needs %d): inconsistent frontier (F4)",
						q.observer.BatchID.String(), a.String(), q.threshold,
						z.String(), q.observer.Frontier[z], n)
				}
			}
		}
	}
	return nil
}

// validateNoPartialRangeObservation rejects a frontier entry that points
// INSIDE a retained batch's reserved range rather than at its end.
//
// §4.1 B5: "Causal metadata advances past a batch only to its END ...
// Intermediate counters of a batch are never exposed in any frontier." A
// frontier naming counter 1 of a batch reserving [1,2] therefore claims to
// have observed half of an atomic batch, which B1 forbids any observer
// from projecting. It is an inconsistent frontier and rejects
// `malformed-framing` before replay (§4.3 row 4).
//
// The dominance sweep alone cannot catch it: that sweep folds only
// ancestors whose END lies at or below the threshold, so a batch whose
// range STRADDLES the threshold is skipped entirely and the observation is
// silently certified. Both the optimized checker and its naive oracle
// shared that blind spot, which is why this is one function used by both
// rather than two implementations of the same rule.
//
// It claims nothing about compacted ancestry: an intermediate counter is
// detectable only where the containing batch is still retained, exactly as
// F4 obligation 3 records for closure itself. A snapshot-only package
// naming counter 1 of an actor whose batches are all gone is unprovable,
// and unprovable is not a violation.
func validateNoPartialRangeObservation(known []*Batch) error {
	byActor := batchesByActor(known)
	// Per actor, ranges sorted by counterBase with a running maximum end.
	// "n lies strictly inside some range" is exactly "some range with
	// counterBase <= n has end > n", which the prefix maximum answers
	// exactly — including for overlapping ranges, which a separate check
	// rejects on the paths where B3 disjointness is verifiable.
	type ranges struct {
		bases    []int64
		maxEnds  []int64
		batchIDs []UUID
	}
	indexed := map[UUID]ranges{}
	for a, bs := range byActor {
		sorted := append([]*Batch(nil), bs...)
		sort.Slice(sorted, func(i, j int) bool {
			if sorted[i].CounterBase != sorted[j].CounterBase {
				return sorted[i].CounterBase < sorted[j].CounterBase
			}
			return sorted[i].EndCounter() < sorted[j].EndCounter()
		})
		r := ranges{}
		var running int64
		var runningID UUID
		for _, b := range sorted {
			if b.EndCounter() > running {
				running = b.EndCounter()
				runningID = b.BatchID
			}
			r.bases = append(r.bases, b.CounterBase)
			r.maxEnds = append(r.maxEnds, running)
			r.batchIDs = append(r.batchIDs, runningID)
		}
		indexed[a] = r
	}

	observers := append([]*Batch(nil), known...)
	sort.Slice(observers, func(i, j int) bool {
		if observers[i] == nil || observers[j] == nil {
			return observers[j] == nil
		}
		return observers[i].BatchID.Compare(observers[j].BatchID) < 0
	})
	for _, b := range observers {
		if b == nil {
			continue
		}
		actors := make([]UUID, 0, len(b.Frontier))
		for a := range b.Frontier {
			actors = append(actors, a)
		}
		SortUUIDs(actors)
		for _, a := range actors {
			n := b.Frontier[a]
			if n <= 0 {
				continue
			}
			r, ok := indexed[a]
			if !ok {
				continue // ancestry compacted: nothing to check against
			}
			// Rightmost range whose counterBase is at or below n.
			lo, hi := 0, len(r.bases)
			for lo < hi {
				mid := (lo + hi) / 2
				if r.bases[mid] <= n {
					lo = mid + 1
				} else {
					hi = mid
				}
			}
			if lo == 0 {
				continue
			}
			if r.maxEnds[lo-1] > n {
				return wrapFraming(
					"batch %s observes actor %s through counter %d, which lies INSIDE the reserved range of retained batch %s: "+
						"causal metadata advances only to a batch end (B5), so this is an inconsistent frontier (F4)",
					b.BatchID.String(), a.String(), n, r.batchIDs[lo-1].String())
			}
		}
	}
	return nil
}

func batchesByActor(known []*Batch) map[UUID][]*Batch {
	out := map[UUID][]*Batch{}
	seen := map[UUID]bool{}
	for _, b := range known {
		if b == nil || seen[b.BatchID] {
			continue
		}
		seen[b.BatchID] = true
		out[b.ActorID] = append(out[b.ActorID], b)
	}
	return out
}

func sortedActors(byActor map[UUID][]*Batch) []UUID {
	out := make([]UUID, 0, len(byActor))
	for a := range byActor {
		out = append(out, a)
	}
	SortUUIDs(out)
	return out
}

// validateOwnPriorEnds enforces F4's own-actor obligation. B3 makes an
// actor's ranges contiguous from 1, so a batch at counterBase c > 1 has
// exactly c-1 of its own operations behind it and must say so. ED-0037
// comment (e) records why the earlier "omitting the own entry is benign"
// reading was wrong: it preserves that actor's own STREAM ORDER only,
// because counterBase is the position's second component, while
// under-reporting the past size by the actor's whole prior stream can
// invert a cross-actor comparison.
func validateOwnPriorEnds(known []*Batch) error {
	for _, b := range known {
		if b == nil {
			continue
		}
		own, present := b.Frontier[b.ActorID]
		switch {
		case b.CounterBase > CounterBaseMin:
			if !present {
				return wrapFraming(
					"batch %s at counterBase %d omits its own prior end counter %d: inconsistent frontier (F4)",
					b.BatchID.String(), b.CounterBase, b.CounterBase-1)
			}
			if own != b.CounterBase-1 {
				return wrapFraming(
					"batch %s at counterBase %d reports its own prior end as %d, not %d: inconsistent frontier (F4)",
					b.BatchID.String(), b.CounterBase, own, b.CounterBase-1)
			}
		default:
			if present && own != 0 {
				return wrapFraming(
					"batch %s is its actor's first batch but claims own prior end %d: inconsistent frontier (F4)",
					b.BatchID.String(), own)
			}
		}
	}
	return nil
}

// validateFrontierClosureNaive is the direct statement of F4 obligation 2:
// for every ordered pair (observer, ancestor) where the observer's
// frontier reaches the ancestor's end counter, the observer's frontier
// must dominate the ancestor's. It is quadratic and exists as the
// differential oracle for validateFrontierClosure, not as a production
// path.
func validateFrontierClosureNaive(known []*Batch) error {
	if err := validateOwnPriorEnds(known); err != nil {
		return err
	}
	// The SAME partial-range rule, not a second implementation of it: an
	// oracle that reimplemented the check could reproduce the bug it exists
	// to catch, and this one did until it was found.
	if err := validateNoPartialRangeObservation(known); err != nil {
		return err
	}
	for _, b := range known {
		if b == nil {
			continue
		}
		for _, a := range known {
			if a == nil || a == b {
				continue
			}
			n, names := b.Frontier[a.ActorID]
			if !names || n <= 0 || a.EndCounter() > n {
				continue
			}
			for z, m := range a.Frontier {
				if m <= 0 {
					continue
				}
				if b.Frontier[z] < m {
					return wrapFraming(
						"batch %s observes actor %s through counter %d but omits or under-reports actor %s (has %d, needs %d): inconsistent frontier (F4)",
						b.BatchID.String(), a.ActorID.String(), n,
						z.String(), b.Frontier[z], m)
				}
			}
		}
	}
	return nil
}

// validateNoRangeOverlap rejects two batches of one actor whose reserved
// ranges intersect (B3: "reserved ranges are disjoint and strictly
// increasing").
//
// SCOPE: the RETAINED LOG, and only it. §4.5 I2 is precise about this —
// "the pre-replay structural checks §4.3 row 4 applies to THE RETAINED
// LOG, and a retained range at or below the watermark is exempt from none
// of them" — and it is equally precise that an ordinary REDELIVERY at or
// below the watermark "is already applied: ignore it silently and
// deterministically". A redelivered batch necessarily reoccupies a range
// the replica has already seen, so applying this check to arbitrary
// deliveries would turn I2's silent ignore into a framing reject and break
// V1/V5. It is therefore called from the snapshot/replay constructors,
// where the batch set IS the retained log, and never from Deliver.
//
// It runs BEFORE watermark suppression, which is the reordering ED-0046
// assigns here: two retained ranges overlapping wholly BELOW the watermark
// were previously filtered out of the set that reached range chaining, so
// the overlap was never seen. ED-0040 queue item 4 described the same
// reordering as "the Load path building its non-skipped set before
// per-actor range chaining".
//
// GAP detection is deliberately NOT performed. Compaction drops batches,
// so a gap in the retained set is not by itself a B3 violation, and the
// implied-prefix machinery in FromSnapshot/ReplayFromSnapshot already
// models the dropped counters. The discarded `checkReservedRanges` verdict
// in the persistence reader — which does treat gaps as rejects — remains
// issue 0029's (ED-0032, ED-0046).
func validateNoRangeOverlap(known []*Batch) error {
	byActor := map[UUID][]*Batch{}
	seen := map[UUID]bool{}
	for _, b := range known {
		if b == nil {
			continue
		}
		// One batch delivered twice is I2's silent duplicate, not two
		// batches reserving one range (V1). Differing content under one
		// batch id is a separate framing reject and is caught by the
		// duplicate check, which owns that comparison.
		if seen[b.BatchID] {
			continue
		}
		seen[b.BatchID] = true
		byActor[b.ActorID] = append(byActor[b.ActorID], b)
	}
	actors := make([]UUID, 0, len(byActor))
	for a := range byActor {
		actors = append(actors, a)
	}
	SortUUIDs(actors)
	for _, a := range actors {
		rs := byActor[a]
		sort.Slice(rs, func(i, j int) bool {
			if rs[i].CounterBase != rs[j].CounterBase {
				return rs[i].CounterBase < rs[j].CounterBase
			}
			return rs[i].EndCounter() < rs[j].EndCounter()
		})
		for i := 1; i < len(rs); i++ {
			if rs[i].CounterBase <= rs[i-1].EndCounter() {
				return wrapFraming(
					"actor %s reserves overlapping ranges [%d,%d] (batch %s) and [%d,%d] (batch %s)",
					a.String(),
					rs[i-1].CounterBase, rs[i-1].EndCounter(), rs[i-1].BatchID.String(),
					rs[i].CounterBase, rs[i].EndCounter(), rs[i].BatchID.String())
			}
		}
	}
	return nil
}

// batchObserves reports whether b's frontier reaches the operation at w —
// that is, whether b's author had already seen it.
//
// F1 makes `frontier[a]` exactly the number of a's operations observed, so
// the operation at counterBase+operationIndex is observed precisely when
// that count reaches it. The subtraction is guarded rather than written as
// an addition because counterBase may sit at row L13's ceiling, where
// base+index would overflow the very comparison it is meant to decide.
func batchObserves(b *Batch, w position) bool {
	n, named := b.Frontier[w.Ref.Actor]
	if !named || n < w.Ref.Base {
		return false
	}
	return n-w.Ref.Base >= w.OpIndex
}

// authenticateAgainstBatches checks retained positions against the batches
// that are actually present, in the two ways a batch can contradict a
// position naming it.
//
// FRONTIER: §5.1 S3's shared table stores each referenced batch's own
// frontier, and `causalPastSize` is the sum of it. If the batch itself is
// present and carries a different frontier, one of the two is lying about
// that batch's causal past, and believing the table would order the
// document by a history that never happened.
//
// OPERATION INDEX: a position names operation `operationIndex` of that
// batch. A one-operation batch has no operation 1, so a position claiming
// one references an operation that does not exist — absent required data
// (§9.4), not a tolerable rounding.
//
// Neither check claims anything about a batch that is NOT present: for
// compacted ancestry there is nothing to compare against, exactly as F4
// obligation 3 records for closure.
func (p *Projection) authenticateAgainstBatches(bs []*Batch) error {
	byRef := map[batchRef]*Batch{}
	for _, b := range bs {
		if b != nil {
			byRef[batchRefOf(b)] = b
		}
	}
	if len(byRef) == 0 {
		return nil
	}
	for _, b := range bs {
		if b == nil {
			continue
		}
		ref := batchRefOf(b)
		if fr, known := p.positions.frontiers[ref]; known && !sameFrontier(fr, b.Frontier) {
			return wrapFraming(
				"batch %s carries a frontier the snapshot's batchPositions entry for %s contradicts: "+
					"the two disagree about that batch's causal past (§5.1 S3)",
				b.BatchID.String(), ref.String())
		}
	}
	// Deterministic scan order so a malformed document always names the
	// same offending position.
	entities := make([]UUID, 0, len(p.createdPositions)+len(p.tombstones))
	for id := range p.createdPositions {
		entities = append(entities, id)
	}
	SortUUIDs(entities)
	for _, id := range entities {
		exp := positionExpectation{kind: OpCreateEntity, entity: id}
		if err := checkIndexWithin(byRef, p.createdPositions[id], exp, "creation position for "+id.String()); err != nil {
			return err
		}
	}
	tombIDs := make([]UUID, 0, len(p.tombstones))
	for id := range p.tombstones {
		tombIDs = append(tombIDs, id)
	}
	SortUUIDs(tombIDs)
	for _, id := range tombIDs {
		exp := positionExpectation{kind: OpDeleteEntity, entity: id}
		if err := checkIndexWithin(byRef, p.tombstones[id], exp, "tombstone for "+id.String()); err != nil {
			return err
		}
	}
	fields := make([]fieldKey, 0, len(p.winnerPositions))
	for f := range p.winnerPositions {
		fields = append(fields, f)
	}
	sort.Slice(fields, func(i, j int) bool {
		if c := fields[i].Entity.Compare(fields[j].Entity); c != 0 {
			return c < 0
		}
		return fields[i].Field < fields[j].Field
	})
	for _, f := range fields {
		what := "winner position " + f.Entity.String() + "." + f.Field
		exp := positionExpectation{entity: f.Entity, field: f.Field}
		if err := checkIndexWithin(byRef, p.winnerPositions[f], exp, what); err != nil {
			return err
		}
	}
	return nil
}

// checkIndexWithin binds one retained position to the exact operation it
// names. The index bound alone is not authentication: it says the
// operation EXISTS, and nothing about whether it is the operation the key
// claims. Without the semantic half, `createdPositions[E1]` could name a
// batch whose operation at that index creates E2 — the snapshot asserting
// one entity's lifecycle from another entity's write.
//
// §5.1 binds each retained datum to a specific operation: S1's creation
// position is "the write position of the createEntity operation that
// currently owns the id", S4's is "the write position of the deleteEntity
// operation", and S3's is "the winning WRITE POSITION ... keyed by the
// written field".
//
// Only a RETAINED batch can be authenticated. For a compacted one there is
// no payload to check against, and this claims nothing about it — the same
// posture F4 obligation 3 records for closure.
func checkIndexWithin(byRef map[batchRef]*Batch, w position, exp positionExpectation, what string) error {
	b, present := byRef[w.Ref]
	if !present {
		return nil
	}
	if w.OpIndex >= int64(len(b.Operations)) {
		return wrapFraming(
			"%s names operation %d of batch %s, which carries only %d operation(s)",
			what, w.OpIndex, w.Ref.String(), len(b.Operations))
	}
	op := b.Operations[w.OpIndex]
	if op.EntityID != exp.entity {
		return wrapFraming(
			"%s names operation %d of batch %s, which targets entity %s",
			what, w.OpIndex, w.Ref.String(), op.EntityID.String())
	}
	switch exp.kind {
	case OpCreateEntity, OpDeleteEntity:
		if op.Kind != exp.kind {
			return wrapFraming(
				"%s names operation %d of batch %s, which is a %s rather than a %s",
				what, w.OpIndex, w.Ref.String(), op.Kind, exp.kind)
		}
		return nil
	}

	// A winner position names whichever operation last wrote the FIELD, so
	// it may be either kind that writes one: a createEntity records a
	// winner for every mergeable field in its payload, and a setFields for
	// the fields its payload names.
	switch op.Kind {
	case OpCreateEntity:
		ent, err := decodeEntity(op.Payload)
		if err != nil {
			return wrapFraming("%s names an unreadable createEntity payload in batch %s: %v",
				what, w.Ref.String(), err)
		}
		for _, name := range mergeableFieldNames(ent) {
			if name == exp.field {
				return nil
			}
		}
		return wrapFraming(
			"%s names the createEntity at operation %d of batch %s, whose payload does not carry field %q",
			what, w.OpIndex, w.Ref.String(), exp.field)
	case OpSetFields:
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(op.Payload, &fields); err != nil {
			return wrapFraming("%s names an unreadable setFields payload in batch %s: %v",
				what, w.Ref.String(), err)
		}
		if _, ok := fields[exp.field]; !ok {
			return wrapFraming(
				"%s names the setFields at operation %d of batch %s, which does not write field %q",
				what, w.OpIndex, w.Ref.String(), exp.field)
		}
		return nil
	default:
		return wrapFraming(
			"%s names operation %d of batch %s, a %s, which writes no field",
			what, w.OpIndex, w.Ref.String(), op.Kind)
	}
}

// positionExpectation is what §5.1 says the operation at a retained
// position must be. kind is empty for a winner position, where either
// writing kind is admissible and the FIELD is the discriminator.
type positionExpectation struct {
	kind   OpKind
	entity UUID
	field  string
}
