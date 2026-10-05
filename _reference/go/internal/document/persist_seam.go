package document

import "encoding/json"

// This file is the labeled provisional handoff seam for issue 0029
// (container persistence and recovery). It EXTENDS the delivered issue
// 0010/0011 material without forking or weakening it: every declaration
// here is a thin alias or wrapper over existing unexported definitions so
// the persistence layer can serialize/deserialize snapshot S1–S5 shapes
// (FORMAT-SPEC §5.1) and read the applied batch log for operation framing,
// without gaining any new mutation path.

// FieldKey aliases the LWW field key (entity id plus serialized field
// name) so persistence code can range over State.WinnerKeys and rebuild
// the map for FromSnapshot. It adds no behavior.
type FieldKey = fieldKey

// BatchRef aliases the identity of one referenced batch — the pair B3
// makes unique. Its wire spelling is the §7.6 entry stem
// `<counterBase padded to 19>-<actorId>`; persistence owns that spelling,
// the document layer owns the ordering. It adds no behavior.
type BatchRef = batchRef

// Position aliases the write position of §4.6 LWW1 as refined by ED-0037
// comment (b): a reference into the shared batch-position table plus the
// operation's index within that batch. `causalPastSize` is derived from
// the referenced frontier and is deliberately not a member — a sum has no
// inverse, so a stored one could drift from the frontier the wire holds.
type Position = position

// NewFieldKey constructs a FieldKey. Persistence owns wire syntax of the
// S3 winner-key table; construction stays explicit so no caller can forge
// an invalid zero value unnoticed — FromSnapshot validates the result.
func NewFieldKey(entity UUID, field string) FieldKey {
	return fieldKey{Entity: entity, Field: field}
}

// NewBatchRef constructs a BatchRef from a parsed §7.6 stem.
func NewBatchRef(counterBase int64, actor UUID) BatchRef {
	return batchRef{Base: counterBase, Actor: actor}
}

// Stem renders the reference in its §7.6 entry-name form. Persistence
// serializes this; nothing in the document layer parses it back.
func StemOf(ref BatchRef) string { return ref.String() }

// NewPosition constructs a write position, validating the operation index
// against row L23's operations-per-batch ceiling before any narrowing.
// Persisted validity must never depend on the host width of int, so the
// index is int64 end to end.
func NewPosition(ref BatchRef, operationIndex int64) (Position, error) {
	if err := validateOperationIndex(operationIndex); err != nil {
		return position{}, err
	}
	return position{Ref: ref, OpIndex: operationIndex}, nil
}

// BatchOf and OperationIndexOf read a position's two components.
func BatchOf(p Position) BatchRef       { return p.Ref }
func OperationIndexOf(p Position) int64 { return p.OpIndex }

// ComparePositions orders two write positions against a batch-position
// table of the §5.1 S3 shape, returning -1, 0 or 1. Both references must be
// present in the table: `causalPastSize` is the sum of the referenced
// frontier, so a position whose batch the table does not carry cannot be
// placed at all, and returning a guess would silently invent an ordering.
//
// It exists so the persistence layer can select among candidate positions —
// ED-0038's legacy tombstone recovery has to choose the create whose
// interval a delete closes — without duplicating the exact 128-bit
// arithmetic F3 requires.
func ComparePositions(table map[BatchRef]map[UUID]int64, a, b Position) (int, error) {
	pt := newPositionTable()
	for ref, fr := range table {
		if err := pt.note(ref, fr); err != nil {
			return 0, err
		}
	}
	for _, w := range [...]Position{a, b} {
		if _, ok := pt.frontiers[w.Ref]; !ok {
			return 0, wrapFraming(
				"cannot order a position whose batch %s is absent from the table", w.Ref.String())
		}
	}
	return pt.compare(a, b), nil
}

// SumFrontier computes a frontier's causalPastSize exactly (F1/F3). It is
// exposed so the persistence layer can report a position's ordering
// component without duplicating the 128-bit accumulator, never so a caller
// can store the result: §5.1 S3 carries the frontier, not the sum.
func SumFrontier(frontier map[UUID]int64) string {
	return sumFrontier(frontier).String()
}

// EncodeEntity serializes one entity to its canonical §2.3 JSON object,
// including preserved unknown optional fields (U2). It exposes the same
// encoder used for authoritative create payloads; nothing else changes.
func EncodeEntity(e Entity) (json.RawMessage, error) {
	return encodeEntity(e)
}

// DecodeEntity validates one complete §2.3 entity object and returns its
// typed form with opaque unknown optional fields preserved. It is the
// decode side of EncodeEntity with identical semantics to create-payload
// validation; unknown kinds refuse safely exactly as before.
func DecodeEntity(raw json.RawMessage) (Entity, error) {
	return decodeEntity(raw)
}

// Log returns deep-ish copies of the accepted batch deliveries in arrival
// order (FORMAT-SPEC §4.4 F3: arrival order is never semantic; the writer
// re-serializes batches under deterministic entry names regardless).
// Batches are immutable once delivered; sharing their values is safe, but
// callers must treat the result as read-only. Local session undo/redo
// state is deliberately NOT exposed here: it is device-local and never
// persisted (§4.8, BEHAVIOR-SPEC §8).
func (p *Projection) Log() []*Batch {
	out := make([]*Batch, len(p.log))
	copy(out, p.log)
	return out
}

// ReplayFromSnapshot rebuilds the projection from snapshot contents plus
// the retained operation log — the persistence-side counterpart of
// FromSnapshot (FORMAT-SPEC §5.1, §7.5; vector V6). It extends, never
// weakens, the delivered replay/session material:
//
//   - the snapshot state seeds the replay root exactly as FromSnapshot
//     constructs it;
//   - for an actor ABSENT from the snapshot frontier whose earliest
//     retained batch begins above counter 1, the covered prefix
//     [1..base-1] is the same B3 implication FromSnapshot already
//     records for watermarks ("a watermark W implies reservations
//     covering exactly [1..W]"): those counters were reserved by B3
//     contiguity and compacted away. A genuine gap — an actor whose
//     watermark ends BELOW an implied-prefix boundary minus one — still
//     rejects as malformed-framing;
//   - the log then replays through Deliver unchanged, so canonical
//     ordering, dedup, stale discard, and LWW resolution keep their exact
//     delivered semantics.
func ReplayFromSnapshot(s State, log []*Batch) (*Projection, error) {
	// The log IS the retained log, so its structural checks run here,
	// before Deliver's watermark classification can suppress any entry
	// (§4.5 I2; ED-0046). Two retained ranges overlapping wholly below the
	// watermark reached range chaining through a set they had already been
	// filtered out of.
	if err := validateNoRangeOverlap(log); err != nil {
		return nil, err
	}
	proj, err := FromSnapshot(s, nil)
	if err != nil {
		return nil, err
	}
	minBase := map[UUID]int64{}
	for _, b := range log {
		if b == nil {
			continue
		}
		if m, ok := minBase[b.ActorID]; !ok || b.CounterBase < m {
			minBase[b.ActorID] = b.CounterBase
		}
	}
	for actor, mb := range minBase {
		w := proj.frontier[actor]
		_, known := proj.frontier[actor]
		switch {
		case mb <= w:
			// Fully covered prefix: watermark classification in Deliver
			// skips these batches (I2/V5); nothing to imply.
		case mb == w+1 || !known:
			if !known {
				// Actor absent from the snapshot frontier: counters
				// [1..mb-1] are the compacted prefix implied by B3
				// contiguity (same implication as a watermark).
				if mb > 1 {
					proj.ranges[actor] = append(proj.ranges[actor], counterRange{w + 1, mb - 1})
					proj.frontier[actor] = mb - 1
				}
			}
			// mb == w+1 chains naturally at the watermark.
		default:
			return nil, wrapFraming(
				"actor %s retained log starts at %d but the snapshot watermark is %d: genuine gap",
				actor.String(), mb, w)
		}
	}
	proj.baseline = proj.captureBaseline()

	// BLOCKER 2: The log IS the retained log, so closure and authentication
	// checks run here explicitly, BEFORE Deliver's watermark classification
	// can suppress any entry (§4.5 I2; ED-0046).
	//
	// Closure gate (validateFrontierClosure): LOAD-BEARING. Ordinary Deliver
	// deliberately excludes covered batches from closure validation (I2 "no
	// effects, no error"), so nothing else validates closure for the
	// retained-log domain. Removing this gate causes
	// TestCoveredRedeliveryDoesNotPoisonFreshBatchClosure/retained-log-domain
	// to fail (mutation verified).
	if err := validateFrontierClosure(log); err != nil {
		return nil, err
	}

	// Authentication gate (authenticateAgainstBatches): CURRENTLY REDUNDANT
	// with projection.go:431, which authenticates ALL unique batches
	// (including covered) BEFORE watermark classification. Ordinary Deliver
	// performs this authentication because "suppressing it first would mean
	// never comparing the two" (projection.go:427-430). This gate becomes
	// LOAD-BEARING the moment ordinary Deliver stops authenticating covered
	// batches (the symmetric change to the closure split). No test can
	// witness it today because the downstream check always runs; attempting
	// to construct such a test requires violating §5.1 S3 (batchPositions
	// must contain exactly the referenced batches). Efficacy is unverified
	// per ED-0072 second branch: documented as currently redundant, preserved
	// against the refactor that would make it load-bearing.
	if err := proj.authenticateAgainstBatches(log); err != nil {
		return nil, err
	}

	if err := proj.Deliver(log...); err != nil {
		return nil, err
	}
	return proj, nil
}
