package persist

import "domowik.dev/domowik/internal/document"

// legacy.go implements FORMAT-SPEC §5.1's legacy snapshot rules and owner
// ruling ED-0038's documented reading convention.
//
// A pre-ED-0037 snapshot carries `winnerKeys`, `createdKeys`, and
// `deletingKey` members whose values are bare `(operationCounter, actorId)`
// pairs. Those pairs cannot place a write against a later concurrent one,
// which is why ED-0037 replaced them. ED-0038's ruling is that a legacy
// package is read under ONE documented convention, that **V5 exactness is
// explicitly NOT claimed** under it, and that the first save rewrites exact
// positions so the ambiguity cannot outlive a single read.
//
// The convention is a CONVENTION, not a derivation. ED-0037 comment (a)
// showed the identity it adopts cannot be derived as fact: two different
// histories serialize to identical bytes once retained logs are dropped —
// N creates in one multi-operation batch, or N one-operation batches — and
// no reader can tell them apart. The convention picks the second reading
// deliberately, because it maximises a legacy winner's position and so
// makes an existing value harder for a late concurrent write to displace,
// which is the safer default for a local-first application opening its own
// older file.

// ConvertLegacySnapshot applies ED-0038's convention, using retained
// batches only where §5.1 requires evidence rather than convention.
//
// retained may be nil. When it is, a legacy snapshot carrying any
// tombstone fails closed: the pre-ED-0037 shape records a `deletingKey`
// but, because creation metadata was live-entity-only, no creation key for
// a deleted id, so I5's live interval cannot be reconstructed. §5.1 forbids
// inventing it — "guessing the creation side would apply a late batch
// positioned before the true creation where full replay discards it whole
// — a silently wrong projection, which is worse than a refusal".
func ConvertLegacySnapshot(l *LegacySnapshot, retained []*document.Batch) (document.State, *Error) {
	st := document.State{
		Entities:         l.Entities,
		Frontier:         l.Frontier,
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		CreatedPositions: map[document.UUID]document.Position{},
		Tombstones:       map[document.UUID]document.Position{},
	}

	// §5.1: an EMPTY legacy snapshot — empty entities, empty frontier, and
	// no winner, creation, or tombstone entries, which is §7.5's brand-new
	// document — has nothing to place. It reads as the empty projection and
	// no convention applies.
	if l.IsEmptyProjection() {
		return st, nil
	}

	universe := l.actorUniverse()
	if len(universe) != 1 {
		// §5.1/ED-0038: with two or more actors in the union the winners
		// cannot be placed at all — any convention would invent cross-actor
		// causal relationships that never existed. The snapshot is
		// fail-closed, exactly migratable only for a field whose full
		// relevant write history is retained or attested, and otherwise
		// rejected. ED-0037 comment (d) is why "the old winner's batch is
		// retained" is not sufficient: retaining the old max-K winner's
		// batch derives only that candidate's new position and cannot
		// exclude a different write — lower K, greater position — that lost
		// under the old rule and whose value is gone.
		return document.State{}, fail(CatInvalidFieldSyntax, StageSchema,
			"legacy snapshot spans %d actors across its frontier and retained keys: "+
				"winners cannot be placed and the snapshot is fail-closed (FS §5.1, ED-0038)",
			len(universe))
	}

	// Retained EVIDENCE is recorded before any convention is synthesized,
	// so that where the two could describe the same batch the evidence
	// stands and a contradiction fails closed rather than being papered
	// over by a convention that only exists for the absence of evidence.
	for id, deletingKey := range l.Tombstones {
		deleting, dfr, cerr := legacyConventionPosition(deletingKey, "deleting key")
		if cerr != nil {
			return document.State{}, cerr
		}
		pos, fr, found, serr := creationClosedBy(retained, id, deleting, dfr)
		if serr != nil {
			return document.State{}, serr
		}
		if !found {
			// §5.1: a legacy snapshot carrying ANY tombstone is fail-closed
			// unless the deleted id's creating batch is still retained. The
			// pre-ED-0037 shape records a deletingKey but, because creation
			// metadata was live-entity-only, no creation key for a deleted
			// id, so I5's live interval cannot be reconstructed. No
			// convention may invent it: guessing the creation side would
			// apply a late batch positioned before the true creation where
			// full replay discards it whole — a silently wrong projection,
			// which is worse than a refusal.
			return document.State{}, fail(CatInvalidFieldSyntax, StageSchema,
				"legacy snapshot tombstones %s but retains no create whose interval that delete closes: "+
					"the live interval cannot be reconstructed and no convention may invent it (FS §5.1 S4)",
				id.String())
		}
		st.CreatedPositions[id] = pos
		st.BatchPositions[document.BatchOf(pos)] = fr
	}

	// The convention: each retained key is treated as its own
	// ONE-OPERATION batch, giving counterBase = operationCounter,
	// operationIndex = 0, and causalPastSize = operationCounter − 1.
	convert := func(k legacyKey, what string) (document.Position, *Error) {
		pos, want, cerr := legacyConventionPosition(k, what)
		if cerr != nil {
			return document.Position{}, cerr
		}
		ref := document.BatchOf(pos)
		if have, seen := st.BatchPositions[ref]; seen {
			if !sameCounters(have, want) {
				return document.Position{}, fail(CatInvalidFieldSyntax, StageSchema,
					"legacy %s places batch %s at a causal past the retained batch contradicts: "+
						"the convention and the retained evidence disagree (FS §5.1, ED-0038)",
					what, document.StemOf(ref))
			}
			return pos, nil
		}
		st.BatchPositions[ref] = want
		return pos, nil
	}

	for f, k := range l.WinnerKeys {
		pos, err := convert(k, "winner key")
		if err != nil {
			return document.State{}, err
		}
		st.WinnerPositions[f] = pos
	}
	for id, k := range l.CreatedKeys {
		pos, err := convert(k, "creation key")
		if err != nil {
			return document.State{}, err
		}
		// Retained evidence already placed the creation of a tombstoned id.
		// Writing over it here would let LOOP ORDER decide between evidence
		// and convention, which is the opposite of the ordering this
		// function states: a disagreement is a contradiction and fails
		// closed.
		if prior, recovered := st.CreatedPositions[id]; recovered && prior != pos {
			return document.State{}, fail(CatInvalidFieldSyntax, StageSchema,
				"legacy creation key for %s contradicts the create retained for it: "+
					"convention says %s#%d, the retained log says %s#%d (FS §5.1, ED-0038)",
				id.String(),
				document.StemOf(document.BatchOf(pos)), document.OperationIndexOf(pos),
				document.StemOf(document.BatchOf(prior)), document.OperationIndexOf(prior))
		}
		st.CreatedPositions[id] = pos
	}
	for id, k := range l.Tombstones {
		pos, err := convert(k, "deleting key")
		if err != nil {
			return document.State{}, err
		}
		st.Tombstones[id] = pos
	}

	// Every live entity needs its creation position too, or LWW4 cannot be
	// re-decided against a late concurrent create (§5.1 S1).
	for _, e := range l.Entities {
		if _, ok := st.CreatedPositions[e.EntityID()]; ok {
			continue
		}
		return document.State{}, fail(CatInvalidFieldSyntax, StageSchema,
			"legacy snapshot holds live entity %s with no creation key (FS §5.1 S1)",
			e.EntityID().String())
	}
	return st, nil
}

// legacyConventionPosition applies ED-0038's reading convention to one
// pre-ED-0037 key: it is treated as its own ONE-OPERATION batch, giving
// counterBase = operationCounter, operationIndex = 0 and causalPastSize =
// operationCounter - 1. The past size is returned as the synthesized
// frontier whose sum is that number, because §5.1 S3 stores the frontier
// and a sum has no inverse. At counter 1 the frontier is EMPTY rather than
// {A: 0}: F1 says an absent actor is nothing observed.
func legacyConventionPosition(k legacyKey, what string) (document.Position, map[document.UUID]int64, *Error) {
	if k.Counter < 1 {
		return document.Position{}, nil, fail(CatInvalidFieldSyntax, StageSchema,
			"legacy %s carries operationCounter %d below 1 (B3)", what, k.Counter)
	}
	if k.Actor == (document.UUID{}) {
		return document.Position{}, nil, fail(CatInvalidFieldSyntax, StageSchema,
			"legacy %s names a zero actorId", what)
	}
	pos, err := document.NewPosition(document.NewBatchRef(k.Counter, k.Actor), 0)
	if err != nil {
		return document.Position{}, nil, fail(CatInvalidFieldSyntax, StageSchema, "%v", err)
	}
	fr := map[document.UUID]int64{}
	if k.Counter > 1 {
		fr[k.Actor] = k.Counter - 1
	}
	return pos, fr, nil
}

// creationClosedBy finds the create whose live interval the given delete
// closes: the GREATEST creation position strictly below the deleting
// position.
//
// It used to return the first syntactic create for the id, which is wrong
// for any retained history that created, deleted, restored and deleted the
// same id — the final tombstone closes the RESTORED interval, and the first
// match is the original create. Picking the first match is not a
// conservative approximation of the right answer; it is a different
// interval, and I5 would then classify every late batch between the two
// creates on the wrong side of branch 1.
//
// Where no retained create qualifies the caller fails closed. §5.1 is
// explicit that guessing the creation side produces a silently wrong
// projection, which is worse than a refusal.
func creationClosedBy(
	retained []*document.Batch,
	id document.UUID,
	deleting document.Position,
	deletingFrontier map[document.UUID]int64,
) (document.Position, map[document.UUID]int64, bool, *Error) {
	// Every candidate is ordered against the delete through the real
	// frontiers, so the comparison uses F3's arithmetic rather than a
	// counter heuristic.
	table := map[document.BatchRef]map[document.UUID]int64{
		document.BatchOf(deleting): deletingFrontier,
	}
	type candidate struct {
		pos document.Position
		fr  map[document.UUID]int64
	}
	var candidates []candidate
	for _, b := range retained {
		if b == nil {
			continue
		}
		ref := document.NewBatchRef(b.CounterBase, b.ActorID)
		for i, op := range b.Operations {
			if op == nil || op.Kind != document.OpCreateEntity || op.EntityID != id {
				continue
			}
			pos, err := document.NewPosition(ref, int64(i))
			if err != nil {
				return document.Position{}, nil, false, fail(CatInvalidFieldSyntax, StageSchema, "%v", err)
			}
			fr := make(map[document.UUID]int64, len(b.Frontier))
			for a, n := range b.Frontier {
				fr[a] = n
			}
			if _, clash := table[ref]; !clash {
				table[ref] = fr
			}
			candidates = append(candidates, candidate{pos: pos, fr: fr})
		}
	}

	var best candidate
	found := false
	for _, c := range candidates {
		cmp, err := document.ComparePositions(table, c.pos, deleting)
		if err != nil {
			return document.Position{}, nil, false, fail(CatInvalidFieldSyntax, StageSchema, "%v", err)
		}
		if cmp >= 0 {
			continue // not inside the interval this delete closes
		}
		if !found {
			best, found = c, true
			continue
		}
		later, err := document.ComparePositions(table, c.pos, best.pos)
		if err != nil {
			return document.Position{}, nil, false, fail(CatInvalidFieldSyntax, StageSchema, "%v", err)
		}
		if later > 0 {
			best = c
		}
	}
	if !found {
		return document.Position{}, nil, false, nil
	}
	return best.pos, best.fr, true, nil
}

func retainedFrontier(retained []*document.Batch, ref document.BatchRef) (map[document.UUID]int64, bool) {
	for _, b := range retained {
		if b == nil {
			continue
		}
		if document.NewBatchRef(b.CounterBase, b.ActorID) != ref {
			continue
		}
		fr := make(map[document.UUID]int64, len(b.Frontier))
		for a, n := range b.Frontier {
			fr[a] = n
		}
		return fr, true
	}
	return nil, false
}

func sameCounters(a, b map[document.UUID]int64) bool {
	if len(a) != len(b) {
		return false
	}
	for k, v := range a {
		if b[k] != v {
			return false
		}
	}
	return true
}

// emptySnapshotDocument is the §7.5 brand-new-document snapshot in the
// post-ED-0037 member set. §7.5 requires the section to be present and to
// serialize the empty projection rather than be omitted.
//
// It is emitted in the NEW shape even though an empty legacy snapshot is
// also readable: a writer that emitted pre-ED-0037 members would create a
// legacy package on every new document, and ED-0038's convention exists to
// read files this project wrote BEFORE the position widening, not to keep
// producing them.
const emptySnapshotDocument = `{"entities":{},"frontier":{},"batchPositions":{},` +
	`"winnerPositions":{},"createdPositions":{},"tombstones":{},` +
	`"batchMetadata":{"recentBatchIds":[]}}`
