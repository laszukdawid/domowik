package document

import (
	"encoding/json"
	"errors"
	"fmt"
	"sort"
)

// ErrMalformedFraming is the pre-replay reject class (FORMAT-SPEC §4.3
// row 4): duplicate batch ids with differing content, gaps or overlaps in
// an actor's counter ranges, unknown operation kinds, torn batches. When
// it fires, NO batch of the delivered set has any effect.
var ErrMalformedFraming = errors.New("malformed-framing")

// ErrCausalPrerequisiteMissing reports delivery of a batch whose frontier
// points to counters not covered by the applied history or the delivered
// set (§4.4 F3). The projection is defined over the causal closure;
// delivering an open set refuses safely.
var ErrCausalPrerequisiteMissing = errors.New("causal-prerequisite-missing")

// BatchRejectedError is the user-input validation class (§4.3 row 1): the
// named batch rejects atomically BEFORE any effect and the state is
// unchanged by it. No undo entry concept applies at this seam.
//
// The WHOLE CALL rolls back, not merely the named batch: Deliver takes a
// SET, F3 defines the projection over that set, and a set containing an
// invalid member does not define one. Every batch of a failed call is left
// unapplied and unregistered, so a corrected resend applies normally. This
// comment previously claimed that batches committed earlier in the same
// call remain committed; that was never what replayAndCommit did, and the
// only thing a failed call used to leave behind was a registered batch id
// that made a later redelivery skip silently as a duplicate.
type BatchRejectedError struct {
	BatchID UUID
	Err     error
}

func (e *BatchRejectedError) Error() string {
	return fmt.Sprintf("batch %s rejected before any effect: %v", e.BatchID.String(), e.Err)
}

func (e *BatchRejectedError) Unwrap() error { return e.Err }

func wrapFraming(format string, args ...any) error {
	return fmt.Errorf("%w: %s", ErrMalformedFraming, fmt.Sprintf(format, args...))
}

// DiagnosticCode names non-error projection outcomes recorded for
// observability (I5: stale discard is "silent to end users ... recorded in
// projection diagnostics"; LWW4 records discarded creates).
type DiagnosticCode string

const (
	DiagStaleBatchDiscard        DiagnosticCode = "stale-batch-discard"
	DiagAnomalousDiscardedCreate DiagnosticCode = "anomalous-discarded-create"
	// DiagAuthorizedRestore records an authorized tombstone restoration
	// (BEHAVIOR-SPEC R8.1.3/U5): a createEntity whose target id carried a
	// tombstone that THIS device's own undo chain (or own accepted local
	// delete, for undo compensation) produced. Only the unexported
	// deliverRestoring entry point can ever supply such keys; Deliver
	// never does, so ordinary replay keeps resurrection blocked (I3).
	DiagAuthorizedRestore DiagnosticCode = "authorized-tombstone-restore"
)

// Diagnostic records one deterministic projection outcome.
type Diagnostic struct {
	Code     DiagnosticCode
	BatchID  UUID
	ActorID  UUID
	OpIndex  int // -1 when the outcome is batch-level
	EntityID UUID
	Detail   string
}

// fieldKey identifies a mergeable top-level field: entity id plus
// serialized field name.
type fieldKey struct {
	Entity UUID
	Field  string
}

type counterRange struct{ base, end int64 }

// stateBaseline is the frozen construction state a projection replays
// against: the empty document for New(), or the snapshot contents for
// FromSnapshot. Entities are immutable once stored, so sharing pointers
// between baseline and live maps is safe.
type stateBaseline struct {
	entities map[UUID]Entity
	// createdPositions holds the creation position of every id within the
	// horizon, TOMBSTONED ids included (§5.1 S1): a tombstone alone cannot
	// distinguish a late batch positioned BEFORE the creation from one
	// inside the live interval, and treating creation metadata as
	// live-entity-only is exactly what collapses I5 branches 1 and 2.
	createdPositions map[UUID]position
	// tombstones holds the DELETING position only (§5.1 S4). The live
	// interval [creationPosition, deletingPosition) is composed with
	// createdPositions rather than duplicated here, so one datum has one
	// home on the wire and in memory alike.
	tombstones      map[UUID]position
	winnerPositions map[fieldKey]position
	frontier        map[UUID]int64
	ranges          map[UUID][]counterRange
	batchFrontiers  batchPositions
}

// Projection is the immutable/current document state produced from
// delivered batches (GLOSSARY: Projection). All mutation flows through
// Deliver; observers receive deep copies via State and Entity.
//
// Delivery model (FORMAT-SPEC §4.4 F3): batches may arrive in any order,
// in any call granularity. The projection is defined as the canonical
// function of the TOTAL delivered set — every accepted batch joins p.log,
// and each delivery deterministically replays baseline ⊕ log in canonical
// order — so arrival sequence and call splitting cannot change the result
// (V2/V3/V7), including stale-discard outcomes that depend on a target's
// existence at a batch's canonical position. Retention of the live log is
// unbounded pending lwwHorizon fixation (§3.1/§5.1 S3), which keeps the
// replay well-defined; compaction policy (0027) owns any future bound.
type Projection struct {
	entities         map[UUID]Entity
	createdPositions map[UUID]position // live AND tombstoned ids (S1)
	tombstones       map[UUID]position // entityId → deleting position (S4/I3)
	winnerPositions  map[fieldKey]position
	frontier         map[UUID]int64
	batches          map[UUID]*Batch
	ranges           map[UUID][]counterRange
	diagnostics      []Diagnostic

	// positions resolves every referenced batch to its own frontier and is
	// the only place causalPastSize is computed (§5.1 S3). It holds the
	// snapshot's batchPositions table plus every delivered batch.
	positions *positionTable

	log        []*Batch                   // accepted deliveries, arrival order
	baseline   *stateBaseline             // construction state (replay root)
	restorable map[UUID]map[UUID]position // authorized-restoration deleting positions per batch id
}

// New returns an empty projection.
func New() *Projection {
	p := &Projection{
		entities:         map[UUID]Entity{},
		createdPositions: map[UUID]position{},
		tombstones:       map[UUID]position{},
		winnerPositions:  map[fieldKey]position{},
		frontier:         map[UUID]int64{},
		batches:          map[UUID]*Batch{},
		ranges:           map[UUID][]counterRange{},
		positions:        newPositionTable(),
		restorable:       map[UUID]map[UUID]position{},
	}
	p.baseline = p.captureBaseline()
	return p
}

// captureBaseline freezes the current committed state as the replay root.
func (p *Projection) captureBaseline() *stateBaseline {
	b := &stateBaseline{
		entities:         cloneEntityMap(p.entities),
		createdPositions: make(map[UUID]position, len(p.createdPositions)),
		tombstones:       make(map[UUID]position, len(p.tombstones)),
		winnerPositions:  make(map[fieldKey]position, len(p.winnerPositions)),
		frontier:         make(map[UUID]int64, len(p.frontier)),
		ranges:           make(map[UUID][]counterRange, len(p.ranges)),
		// Exactly the frontiers the committed position maps reference. The
		// live table also holds every delivered batch's frontier, and
		// carrying those into the replay root would let a rolled-back
		// delivery's frontier outlive it.
		batchFrontiers: p.referencedFrontiers(p.winnerPositions, p.createdPositions, p.tombstones),
	}
	for id, k := range p.createdPositions {
		b.createdPositions[id] = k
	}
	for id, k := range p.tombstones {
		b.tombstones[id] = k
	}
	for f, k := range p.winnerPositions {
		b.winnerPositions[f] = k
	}
	for a, n := range p.frontier {
		b.frontier[a] = n
	}
	for a, rs := range p.ranges {
		b.ranges[a] = append([]counterRange(nil), rs...)
	}
	return b
}

// restoreBaseline resets working state to the replay root. Maps are
// copied so replay bookkeeping never aliases the baseline.
func (p *Projection) restoreBaseline() {
	p.entities = cloneEntityMap(p.baseline.entities)
	p.createdPositions = make(map[UUID]position, len(p.baseline.createdPositions))
	for id, k := range p.baseline.createdPositions {
		p.createdPositions[id] = k
	}
	p.tombstones = make(map[UUID]position, len(p.baseline.tombstones))
	for id, k := range p.baseline.tombstones {
		p.tombstones[id] = k
	}
	p.winnerPositions = make(map[fieldKey]position, len(p.baseline.winnerPositions))
	for f, k := range p.baseline.winnerPositions {
		p.winnerPositions[f] = k
	}
	p.frontier = make(map[UUID]int64, len(p.baseline.frontier))
	for a, n := range p.baseline.frontier {
		p.frontier[a] = n
	}
	p.ranges = make(map[UUID][]counterRange, len(p.baseline.ranges))
	for a, rs := range p.baseline.ranges {
		p.ranges[a] = append([]counterRange(nil), rs...)
	}
	p.diagnostics = nil
}

// transactionState is the COMPLETE pre-commit snapshot of everything a
// delivery may mutate before it is able to fail, and rollback restores all
// of it (ED-0056).
//
// It exists as one capture/restore pair rather than a habit of remembering
// to save things, because this branch produced three separate leaks of the
// same kind: the applied-identity set, the position table, and — worst —
// the restoration-authorization map, where a failed transaction left
// CAPABILITY behind and a later unrelated batch reusing the uncommitted
// batch id consumed it to resurrect a tombstoned entity. Three leaks of one
// kind is one missing invariant, and fixing three fields would have left
// the fourth for whoever next writes a pre-commit mutation.
//
// The enumeration is derived from the struct, not from what came to mind
// while writing it: TestRollbackEnumeratesEveryProjectionField asserts the
// exact set of Projection field names against a declared classification, so
// a thirteenth field fails the build's tests until someone classifies it.
// Entities are immutable once stored and every commit swaps whole maps, so
// restoring these references discards partial effects.
type transactionState struct {
	entities         map[UUID]Entity
	createdPositions map[UUID]position
	tombstones       map[UUID]position
	winnerPositions  map[fieldKey]position
	frontier         map[UUID]int64
	batches          map[UUID]*Batch
	ranges           map[UUID][]counterRange
	diagnostics      []Diagnostic
	positions        *positionTable
	restorable       map[UUID]map[UUID]position
	log              []*Batch
}

// begin captures the transaction boundary. It is taken at the PUBLIC entry
// point, not inside replayAndCommit, because deliverRestoring writes its
// authorization before replay begins: a boundary drawn any later would
// capture the leak as part of the "before" state and restore it faithfully.
func (p *Projection) begin() transactionState {
	batches := make(map[UUID]*Batch, len(p.batches))
	for id, b := range p.batches {
		batches[id] = b
	}
	restorable := make(map[UUID]map[UUID]position, len(p.restorable))
	for id, m := range p.restorable {
		restorable[id] = m
	}
	return transactionState{
		// Reference capture is enough for the maps a commit REPLACES
		// wholesale: applyBatch stages into fresh maps and swaps them in,
		// so the captured references are never written through.
		entities: p.entities, createdPositions: p.createdPositions, tombstones: p.tombstones,
		winnerPositions: p.winnerPositions, frontier: p.frontier, ranges: p.ranges,
		diagnostics: p.diagnostics, log: p.log,
		// These three are mutated IN PLACE before the commit point, so a
		// captured reference would be written through and restore nothing.
		// The position table learns frontiers during framing, noteApplied
		// registers batch ids as it goes, and deliverRestoring writes its
		// authorization before replay begins. Capturing the pointer instead
		// of the contents is precisely the mistake that makes a rollback
		// look complete and do nothing.
		positions:  p.positions.clone(),
		batches:    batches,
		restorable: restorable,
	}
}

func (p *Projection) rollback(t transactionState) {
	p.entities, p.createdPositions, p.tombstones = t.entities, t.createdPositions, t.tombstones
	p.winnerPositions, p.frontier, p.ranges = t.winnerPositions, t.frontier, t.ranges
	p.diagnostics = t.diagnostics
	p.positions = t.positions
	p.batches = t.batches
	p.restorable = t.restorable
	p.log = t.log
}

// Deliver accepts a SET of batches (§4.4 F3): framing checks reject the
// whole call before any effect; accepted batches join the delivered log
// and the projection deterministically replays the TOTAL set in CANONICAL
// ORDER — the ascending sort by ordering position P = (causalPastSize,
// counterBase, actorId). Arrival sequence and call splitting cannot change
// the projected result (V2/V3/V7), including stale-discard outcomes
// evaluated at each operation's canonical write position.
func (p *Projection) Deliver(batches ...*Batch) error {
	tx := p.begin()
	if err := p.deliverWithin(batches); err != nil {
		p.rollback(tx)
		return err
	}
	return nil
}

func (p *Projection) deliverWithin(batches []*Batch) error {
	order, skip, err := p.frameAndOrder(batches)
	if err != nil {
		return err
	}
	if len(order) == 0 {
		_ = skip // identical redelivery only: ignore silently (I2/V1)
		return nil
	}
	return p.replayAndCommit(order)
}

// deliverRestoring is the unexported entry point for authorized tombstone
// restoration (BEHAVIOR-SPEC R8.1.3/U5). It behaves exactly like
// Deliver for the single batch b, except that a createEntity whose target
// id is tombstoned with EXACTLY the deletingKey listed in restorable has
// that tombstone cleared and proceeds (recorded as DiagAuthorizedRestore).
// The authorization is recorded per batch id and re-applied on every
// replay, where recomputed tombstones carry identical deletingKeys.
// Deliver itself never registers restorable entries, so sync-created
// tombstones can never be restored here.
func (p *Projection) deliverRestoring(b *Batch, restorable map[UUID]position) error {
	tx := p.begin()
	if err := p.deliverRestoringWithin(b, restorable); err != nil {
		// A failed restoring call takes its AUTHORIZATION back with
		// everything else. A successful one keeps it, because replay
		// re-applies authorization on every subsequent delivery and
		// dropping it would make a committed restoration resolve
		// differently the next time the log is replayed.
		p.rollback(tx)
		return err
	}
	return nil
}

func (p *Projection) deliverRestoringWithin(b *Batch, restorable map[UUID]position) error {
	order, _, err := p.frameAndOrder([]*Batch{b})
	if err != nil {
		return err
	}
	if len(order) == 0 {
		return nil
	}
	p.restorable[b.BatchID] = restorable
	return p.replayAndCommit(order)
}

// replayAndCommit folds fresh batches into the delivered log and rebuilds
// the projection as baseline ⊕ canonical(log ∪ fresh). Any failure leaves
// the previously committed state and diagnostics untouched.
func (p *Projection) replayAndCommit(fresh []*Batch) error {
	all := make([]*Batch, 0, len(p.log)+len(fresh))
	all = append(all, p.log...)
	all = append(all, fresh...)
	// The replay root's position table is exactly the snapshot's
	// batchPositions (§5.1 S3); every replayed batch re-notes its own
	// frontier as it applies. Rollback is the caller's: Deliver and
	// deliverRestoring own the transaction boundary, so nothing here needs
	// a second save/restore pair of its own.
	freshTable := &positionTable{
		frontiers: p.baseline.batchFrontiers.clone(),
		pastMemo:  map[batchRef]causalPast{},
	}
	if freshTable.frontiers == nil {
		freshTable.frontiers = batchPositions{}
	}
	ordered, err := p.chainAndOrderOver(all, p.baseline.ranges, freshTable)
	if err != nil {
		return err
	}
	p.restoreBaseline()
	p.positions = freshTable
	for _, b := range ordered {
		if err := p.applyBatch(b, p.restorable[b.BatchID]); err != nil {
			return err
		}
	}
	p.log = all
	return nil
}

// frameAndOrder performs pre-replay framing checks across the union of
// applied history and the delivered set, collapses identical redeliveries
// (applying one copy), verifies causal closure, and returns the canonical
// application order.
func (p *Projection) frameAndOrder(delivered []*Batch) ([]*Batch, map[UUID]bool, error) {
	for _, b := range delivered {
		if b == nil {
			return nil, nil, wrapFraming("delivered batch is absent")
		}
		// Batch fields are exported; manually constructed values are
		// untrusted and re-validated fail-closed before replay.
		if err := validateFraming(b); err != nil {
			return nil, nil, err
		}
	}
	skip := map[UUID]bool{}
	unique := make([]*Batch, 0, len(delivered))
	inSet := map[UUID]*Batch{}
	for _, b := range delivered {
		if prev, seen := inSet[b.BatchID]; seen {
			if !prev.equalContent(b) {
				return nil, nil, wrapFraming("duplicate batch id %s with differing content", b.BatchID.String())
			}
			continue // identical second copy within one set applies once
		}
		if prev, dup := p.batches[b.BatchID]; dup {
			if !prev.equalContent(b) {
				return nil, nil, wrapFraming("duplicate batch id %s with differing content", b.BatchID.String())
			}
			skip[b.BatchID] = true // already applied: silent redelivery (I2/V1)
			continue
		}
		inSet[b.BatchID] = b
		unique = append(unique, b)
	}

	// Retained positions are authenticated against ALL delivered batches,
	// BEFORE watermark classification: a covered batch's frontier is exactly
	// what the snapshot's position table claims to know, and suppressing it
	// first would mean never comparing the two.
	if err := p.authenticateAgainstBatches(unique); err != nil {
		return nil, nil, err
	}

	// Watermark classification (I2/V5) for ordinary deliveries: classify
	// covered redeliveries BEFORE building the ancestry set for closure
	// validation. A covered batch whose ENTIRE reserved range lies at or
	// below the applying replica's per-actor watermark is already applied
	// and must be ignored silently, without becoming ancestry evidence for
	// fresh batches (BLOCKER 2 fix). A range STRADDLING the watermark can
	// never have been applied atomically (B5) and is a torn-batch framing
	// anomaly. Batches fully above the watermark proceed to closure
	// validation and chaining.
	applied := 0
	for _, b := range unique {
		w := p.frontier[b.ActorID]
		switch {
		case b.EndCounter() <= w:
			skip[b.BatchID] = true // already applied (I2/V1/V5)
			applied++
		case b.CounterBase <= w:
			return nil, nil, wrapFraming(
				"batch %s counter range [%d,%d] straddles %s's watermark %d (torn batch)",
				b.BatchID.String(), b.CounterBase, b.EndCounter(), b.ActorID.String(), w)
		}
	}

	// FRONTIER CLOSURE (§4.4 F4 obligation 2), over the union of the
	// applied history and the NON-COVERED incoming set. §4.5 I2 is precise:
	// watermark classification "never precedes, gates, or replaces the
	// pre-replay structural checks §4.3 row 4 applies to the retained log"
	// — and retained-log construction (FromSnapshot/ReplayFromSnapshot)
	// validates every retained entry before any suppression. But in ordinary
	// Deliver, a covered redelivery is NOT retained log; it is an I2-
	// suppressed duplicate that must be silently ignored without altering
	// the fresh batch verdict (BLOCKER 2 fix).
	nonCovered := make([]*Batch, 0, len(unique)-applied)
	for _, b := range unique {
		if !skip[b.BatchID] {
			nonCovered = append(nonCovered, b)
		}
	}
	known := make([]*Batch, 0, len(p.batches)+len(nonCovered))
	for _, b := range p.batches {
		known = append(known, b)
	}
	known = append(known, nonCovered...)
	if err := validateFrontierClosure(known); err != nil {
		return nil, nil, err
	}

	// Per-actor reserved ranges, causal closure, and canonical ordering
	// over the non-skipped unique set.
	nonSkipped := make([]*Batch, 0, len(unique))
	for _, b := range unique {
		if !skip[b.BatchID] {
			nonSkipped = append(nonSkipped, b)
		}
	}
	// Ordering during framing notes frontiers into the live table, which is
	// a mutation. That is safe because the transaction boundary owns the
	// rollback: a rejected call restores the table with everything else.
	// An earlier version avoided the mutation with a scratch clone instead,
	// which worked but left TWO mechanisms for one property, and a reader
	// could not tell which was load-bearing. One invariant, one mechanism
	// (ED-0056).
	order, err := p.chainAndOrderOver(nonSkipped, p.ranges, p.positions)
	if err != nil {
		return nil, nil, err
	}
	return order, skip, nil
}

// chainAndOrderOver validates per-actor reserved-range chaining (B3) and
// causal prerequisite coverage over bs, then returns THE canonical order:
// the ascending sort by ordering position P = (causalPastSize,
// counterBase, actorId).
//
// The sort replaces a topological walk that broke ties by ascending
// (counterBase, actorId) among currently ready batches. That walk always
// produced SOME linear extension of causality, but not the canonical one:
// it disagreed with P wherever two concurrent batches had counter bases
// ordered opposite to their causal past sizes, and F3 defines the
// projection over P alone. F3 also explains why the order must be a sort
// and not a set of pairwise constraints — "a rule of the form 'causality,
// with incomparable pairs ordered by ascending counter' states constraints
// that can form a CYCLE and then define no order, whereas a sort by one
// key with unique values cannot" (ED-0036).
func (p *Projection) chainAndOrderOver(bs []*Batch, ranges map[UUID][]counterRange, tbl *positionTable) ([]*Batch, error) {
	nextBase := map[UUID]int64{}
	for actor, rs := range ranges {
		nextBase[actor] = rs[len(rs)-1].end + 1
	}
	newByActor := map[UUID][]*Batch{}
	for _, b := range bs {
		newByActor[b.ActorID] = append(newByActor[b.ActorID], b)
	}
	for actor, sorted := range newByActor {
		sort.Slice(sorted, func(i, j int) bool { return sorted[i].CounterBase < sorted[j].CounterBase })
		expected := nextBase[actor]
		if expected == 0 {
			expected = 1
		}
		for _, b := range sorted {
			if b.CounterBase != expected {
				return nil, wrapFraming(
					"actor %s counter range [%d,%d] breaks disjoint contiguous ordering (expected base %d)",
					actor.String(), b.CounterBase, b.EndCounter(), expected)
			}
			expected = b.EndCounter() + 1
		}
	}

	// Causal edges and closure (F3/I5 prerequisites).
	coveredThrough := func(actor UUID, deliveredToo bool) int64 {
		n := int64(0)
		if v, ok := p.frontier[actor]; ok {
			n = v
		}
		if deliveredToo {
			for _, b := range newByActor[actor] {
				if b.EndCounter() > n {
					n = b.EndCounter()
				}
			}
		}
		return n
	}
	for _, b := range bs {
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
			if n >= b.CounterBase && a == b.ActorID {
				return nil, wrapFraming("batch %s claims observation of its own reserved range", b.BatchID.String())
			}
			if covered := coveredThrough(a, true); n > covered {
				return nil, fmt.Errorf("%w: batch %s requires actor %s through counter %d, covered only through %d",
					ErrCausalPrerequisiteMissing, b.BatchID.String(), a.String(), n, covered)
			}
		}
	}

	// Every batch's own frontier is what defines its position, so the table
	// learns it here, before anything is compared.
	for _, b := range bs {
		if err := tbl.note(batchRefOf(b), b.Frontier); err != nil {
			return nil, err
		}
	}
	order := append([]*Batch(nil), bs...)
	sort.Slice(order, func(i, j int) bool {
		return tbl.lessBatch(batchRefOf(order[i]), batchRefOf(order[j]))
	})
	return order, nil
}

// lifecycleBranch is I5's three-way classification of a target at one
// operation's write position (FORMAT-SPEC §4.5 I5). The two-way reading —
// present-or-absent in the CURRENT projection — makes branchLive
// indistinguishable from branchAfterDelete and silently changes an
// unrelated field's value; §4.5 states plainly that it is not conforming.
type lifecycleBranch int

const (
	// branchBeforeCreate: the target's creation position is not earlier
	// than the operation's write position, so the target is absent at it.
	branchBeforeCreate lifecycleBranch = iota
	// branchLive: the target exists at that position.
	branchLive
	// branchAfterDelete: the target's deleting position is earlier than the
	// operation's write position, so the target is tombstoned at it.
	branchAfterDelete
)

// stage holds one batch's speculative effects. Creation positions outlive
// deletion (S1), so stageCreated keeps tombstoned ids and the live
// interval [creation, deleting) is composed from the two maps rather than
// stored twice.
type stage struct {
	entities map[UUID]Entity
	created  map[UUID]position
	tombs    map[UUID]position
}

// classify places a target at write position w in exactly one of I5's
// three branches. It is evaluated per TARGET and at each operation's OWN
// write position, never once per batch: one batch may target several
// entities in different lifecycle states (ED-0037 comment (t)).
func (p *Projection) classify(st *stage, id UUID, w position) lifecycleBranch {
	created, known := st.created[id]
	if !known {
		// No retained creation position. §5.1 S1 retains them "for each
		// entity within the horizon" (`lwwHorizon`, Provisional and
		// unfixed), so an entity compacted from BEFORE the horizon
		// legitimately has none, and §5.1 already concedes that pre-horizon
		// metadata is recoverable from a snapshot by no reader. Where the
		// interval is absent the test degrades to presence — which is what
		// the interval refines, not something it contradicts — so a
		// long-existing entity keeps accepting writes instead of having
		// them silently discarded. A tombstone is never in this position:
		// §5.1 S4 makes a tombstone without its creation position a reject,
		// because there branch 1 and branch 2 genuinely collapse.
		if _, present := st.entities[id]; present {
			return branchLive
		}
		return branchBeforeCreate
	}
	if p.positions.compare(created, w) >= 0 {
		return branchBeforeCreate
	}
	if deleted, gone := st.tombs[id]; gone && p.positions.compare(deleted, w) < 0 {
		return branchAfterDelete
	}
	return branchLive
}

// applyBatch stages all operations of b against an overlay, checks the
// commit-time invariants at the batch boundary, and commits atomically
// (B1). Any user-input rejection leaves zero effects; stale discard leaves
// zero effects and only a diagnostic; both advance nothing but the caller's
// bookkeeping stays consistent because ranges were validated up front.
func (p *Projection) applyBatch(b *Batch, restorable map[UUID]position) error {
	if err := p.positions.note(batchRefOf(b), b.Frontier); err != nil {
		return err
	}
	st := &stage{
		entities: cloneEntityMap(p.entities),
		created:  make(map[UUID]position, len(p.createdPositions)),
		tombs:    make(map[UUID]position, len(p.tombstones)),
	}
	for k, v := range p.createdPositions {
		st.created[k] = v
	}
	for k, v := range p.tombstones {
		st.tombs[k] = v
	}
	// Winner-position recording (LWW1/S3/LWW5): each field keeps the
	// GREATEST write position ever recorded, compared against committed
	// positions AND earlier staged writes of this batch, so a
	// never-compacted replica and a snapshot-seeded one resolve
	// late-concurrent writes identically (V5).
	winners := &winnerSet{table: p.positions, base: p.winnerPositions, overlay: map[fieldKey]position{}}
	var diags []Diagnostic
	recordWinners := func(id UUID, names []string, w position) {
		for _, n := range names {
			winners.record(fieldKey{Entity: id, Field: n}, w)
		}
	}

	stale := false
	staleDetail := ""
	for i, op := range b.Operations {
		w := positionOf(b, i)
		switch op.Kind {
		case OpCreateEntity:
			ent, err := decodeEntity(op.Payload)
			if err != nil {
				return &BatchRejectedError{BatchID: b.BatchID, Err: err}
			}
			if ent.EntityID() != op.EntityID {
				return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
					codeInvalidFieldSyntax, "operation entityId %s does not match payload id %s",
					op.EntityID.String(), ent.EntityID().String())}
			}
			// A createEntity is never classified by the live interval: it
			// ESTABLISHES existence at its own position rather than
			// requiring a pre-existing target (§4.5 I5). What it must be
			// placed against is any tombstone for the same id.
			if deleting, gone := st.tombs[op.EntityID]; gone {
				if r, ok := restorable[op.EntityID]; ok && r == deleting {
					// Authorized restoration (R8.1.3/E6): the live
					// tombstone's deleting position is exactly one the
					// caller vetted as this device's own undo artifact.
					delete(st.tombs, op.EntityID)
					diags = append(diags, Diagnostic{
						Code: DiagAuthorizedRestore, BatchID: b.BatchID, ActorID: b.ActorID,
						OpIndex: i, EntityID: op.EntityID,
						Detail: "tombstone cleared for authorized restoration (R8.1.3)",
					})
				} else if p.positions.compare(w, deleting) > 0 {
					// Positioned AFTER the delete: no resurrection (I3),
					// stale on branch-3 terms.
					stale = true
					staleDetail = "createEntity is positioned after a tombstone for its id; no resurrection (I3)"
					break
				} else {
					// Positioned BEFORE the delete. In full history this
					// create competed with whichever create currently owns
					// the id and the later delete then tombstoned the
					// winner, so a seeded replay must re-run that LWW4
					// collision even though nothing materializes: the
					// retained creation position is the boundary of I5
					// branch 1 versus branch 2 for every LATER late batch,
					// and leaving it stale would misclassify them.
					prev, hadCreate := st.created[op.EntityID]
					if !hadCreate {
						// A tombstone always carries its creation position
						// (§5.1 S4; FromSnapshot enforces it), so this is
						// unreachable from a validated snapshot. It fails
						// closed rather than inventing a boundary.
						return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
							codeMissingRequired,
							"entity %s is tombstoned with no creation position, so this create cannot be placed (§5.1 S4)",
							op.EntityID.String())}
					}
					if batchObserves(b, prev) {
						return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
							codeDuplicateElement,
							"entity %s was already created by a batch this one OBSERVES: a causally ordered "+
								"same-id create is an O4 rejection, not a LWW4 collision (§4.5)", op.EntityID.String())}
					}
					switch {
					case p.positions.compare(w, prev) <= 0:
						diags = append(diags, Diagnostic{
							Code: DiagAnomalousDiscardedCreate, BatchID: b.BatchID, ActorID: b.ActorID,
							OpIndex: i, EntityID: op.EntityID,
							Detail: "concurrent create collision resolved by greater write position (LWW4); incoming create discarded",
						})
					default:
						diags = append(diags, Diagnostic{
							Code: DiagAnomalousDiscardedCreate, BatchID: b.BatchID, ActorID: b.ActorID,
							OpIndex: i, EntityID: op.EntityID,
							Detail: "concurrent create collision resolved by greater write position (LWW4); prior create discarded",
						})
						// The winner owns the id, so the live interval it
						// opens is the one the tombstone closes. Full
						// replay records the winner's field positions
						// before the delete removes the entity, and a
						// delete does not erase winner positions, so the
						// seeded path records them too.
						st.created[op.EntityID] = w
						recordWinners(op.EntityID, mergeableFieldNames(ent), w)
					}
					continue
				}
			}
			if _, live := st.entities[op.EntityID]; live {
				prev, decidable := st.created[op.EntityID]
				if !decidable {
					// §5.1 S1 retains creation positions "for each entity
					// WITHIN THE HORIZON", and `lwwHorizon` is Provisional
					// with no value asserted, so a pre-horizon entity
					// legitimately has none. Presence still degrades
					// gracefully for I5 (see classify), but LWW4 cannot:
					// S1 keeps the creation position precisely "so that
					// LWW4's create collisions can be re-decided against a
					// late concurrent create", and without it there is no
					// ranking to make. It fails CLOSED — a reader that
					// guessed here would pick a winner at random, and one
					// that panicked would be worse than either.
					//
					// This narrows when `lwwHorizon` is fixed (ED-0040
					// queue item 7): a fixed horizon makes "within the
					// horizon" determinable, and the absence becomes
					// checkable rather than merely tolerated.
					return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
						codeMissingRequired,
						"entity %s is live but its creation position was compacted beyond the retained horizon, "+
							"so this create collision cannot be re-decided (§5.1 S1/LWW4)", op.EntityID.String())}
				}
				if prev.Ref.Actor == b.ActorID {
					// O4/LWW4: duplicate ids reject before any effect.
					// LWW4 collision resolution exists only for CONCURRENT
					// creates from DIFFERENT actors ("within one actor
					// stream the situation is impossible"); a same-actor
					// duplicate reaching the seam fails closed.
					return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
						codeDuplicateElement, "entity %s already created by this actor stream", op.EntityID.String())}
				}
				if batchObserves(b, prev) {
					// §4.5, as narrowed by 3cca9ee: "a CONCURRENT collision
					// with another create on the same id — neither create
					// observing the other — resolves under LWW4, whose
					// scope is exactly that case. A causally ordered
					// same-id create is not a LWW4 collision: the later
					// author observed the first create, so O4 local
					// validation rejects it at authoring time." A batch
					// that reaches this seam anyway was authored by a
					// non-conforming or hostile writer, which is the
					// threat model rather than an excuse (ED-0049).
					return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr(fieldID,
						codeDuplicateElement,
						"entity %s was already created by a batch this one OBSERVES: a causally ordered "+
							"same-id create is an O4 rejection, not a LWW4 collision (§4.5)", op.EntityID.String())}
				}
				if p.positions.compare(prev, w) < 0 {
					delete(st.entities, op.EntityID)
					diags = append(diags, Diagnostic{
						Code: DiagAnomalousDiscardedCreate, BatchID: b.BatchID, ActorID: b.ActorID,
						OpIndex: i, EntityID: op.EntityID,
						Detail: "concurrent create collision resolved by greater write position (LWW4); prior create discarded",
					})
				} else {
					diags = append(diags, Diagnostic{
						Code: DiagAnomalousDiscardedCreate, BatchID: b.BatchID, ActorID: b.ActorID,
						OpIndex: i, EntityID: op.EntityID,
						Detail: "concurrent create collision resolved by greater write position (LWW4); incoming create discarded",
					})
					continue
				}
			}
			st.entities[op.EntityID] = ent
			st.created[op.EntityID] = w
			recordWinners(op.EntityID, mergeableFieldNames(ent), w)
		case OpSetFields:
			switch p.classify(st, op.EntityID, w) {
			case branchBeforeCreate:
				stale = true
				staleDetail = "setFields targets an entity that does not exist at the operation's write position (I5 branch 1)"
			case branchAfterDelete:
				stale = true
				staleDetail = "setFields targets an entity tombstoned before the operation's write position (I5 branch 3)"
			case branchLive:
				var fields map[string]json.RawMessage
				if err := json.Unmarshal(op.Payload, &fields); err != nil {
					return &BatchRejectedError{BatchID: b.BatchID, Err: fieldErr("",
						codeInvalidFieldSyntax, "setFields payload must be an object mapping field names to values")}
				}
				names := make([]string, 0, len(fields))
				for n := range fields {
					names = append(names, n)
				}
				sort.Strings(names)
				cur, materialized := st.entities[op.EntityID]
				if !materialized {
					// Live at w, but a canonically LATER delete already
					// folded into the replay root has tombstoned the
					// target. LWW3: that delete supersedes this write, so
					// it produces no field state — while the batch is NOT
					// stale and its writes to OTHER targets still apply.
					// Winner positions are still recorded, because full
					// replay records them before the delete and a delete
					// does not erase them; suppressing them here would
					// make a seeded replay disagree with full replay on
					// exactly the metadata V6 compares.
					recordWinners(op.EntityID, names, w)
					continue
				}
				// Validate the FULL payload first so a user-input
				// rejection stays batch-atomic regardless of which fields
				// win LWW.
				scratch := cloneEntity(cur)
				if err := applySetFields(scratch, fields); err != nil {
					return &BatchRejectedError{BatchID: b.BatchID, Err: err}
				}
				// Apply only the writes whose position beats the field's
				// retained winner position (LWW1). CAUSALITY GOVERNS: a
				// write whose batch observes another write's batch has a
				// strictly greater causalPastSize and wins regardless of
				// counters.
				winning := map[string]json.RawMessage{}
				for n, v := range fields {
					if winners.dominates(fieldKey{Entity: op.EntityID, Field: n}, w) {
						winning[n] = v
					}
				}
				if len(winning) > 0 {
					modified := cloneEntity(cur)
					if err := applySetFields(modified, winning); err != nil {
						return &BatchRejectedError{BatchID: b.BatchID, Err: err}
					}
					st.entities[op.EntityID] = modified
				}
				recordWinners(op.EntityID, names, w)
			}
		case OpDeleteEntity:
			switch p.classify(st, op.EntityID, w) {
			case branchBeforeCreate:
				stale = true
				staleDetail = "deleteEntity targets an entity that does not exist at the operation's write position (I5 branch 1)"
			case branchAfterDelete:
				stale = true
				staleDetail = "deleteEntity targets an already-tombstoned entity at the operation's write position (I5 branch 3)"
			case branchLive:
				// Closes the live interval at its own position. The
				// creation position stays retained so the interval
				// survives deletion (S4).
				delete(st.entities, op.EntityID)
				st.tombs[op.EntityID] = w
			}
		}
		if stale {
			break
		}
	}
	if stale {
		p.diagnostics = append(p.diagnostics, Diagnostic{
			Code: DiagStaleBatchDiscard, BatchID: b.BatchID, ActorID: b.ActorID, OpIndex: -1,
			Detail: staleDetail + "; entire batch discarded (I5/B1)",
		})
		p.noteApplied(b)
		return nil
	}

	// Commit-time referential integrity (O4) against post-batch state:
	// unknown levelId/hostWallId/membership targets, prohibited self-
	// references. Checked here so one batch may create a level and its
	// walls together while a dangling reference anywhere rejects the
	// whole batch before any effect.
	for id, ent := range st.entities {
		refCheck := func(ref UUID, name string, want Kind) error {
			if ref == id {
				return fieldErr(name, codeInvalidFieldSyntax, "self-reference where prohibited")
			}
			target, ok := st.entities[ref]
			if !ok {
				return fieldErr(name, codeMissingRequired, "reference target %s absent", ref.String())
			}
			if target.Kind() != want {
				return fieldErr(name, codeUnknownEntityType, "reference target %s is a %s, not a %s",
					ref.String(), target.Kind(), want)
			}
			return nil
		}
		var err error
		switch e := ent.(type) {
		case *Wall:
			err = refCheck(e.LevelID, fieldLevelID, KindLevel)
		case *Opening:
			err = refCheck(e.HostWallID, fieldHostWall, KindWall)
		case *WallJoin:
			for _, m := range e.Members {
				if err = refCheck(m.WallID, FieldMembers, KindWall); err != nil {
					break
				}
			}
		case *RoomRecord:
			err = refCheck(e.LevelID, fieldLevelID, KindLevel)
		case *Label:
			err = refCheck(e.LevelID, fieldLevelID, KindLevel)
		case *Dimension:
			err = refCheck(e.LevelID, fieldLevelID, KindLevel)
		case *AssetInstance:
			err = refCheck(e.LevelID, fieldLevelID, KindLevel)
		}
		if err != nil {
			return &BatchRejectedError{BatchID: b.BatchID, Err: err}
		}
	}

	// Commit.
	p.entities = st.entities
	p.createdPositions = st.created
	p.tombstones = st.tombs
	p.winnerPositions = winners.merged()
	p.diagnostics = append(p.diagnostics, diags...)
	p.noteApplied(b)
	return nil
}

// winnerSet merges per-field write positions under LWW1: a field's
// retained position is the greatest among all writes observed for it.
// Because the write position is intrinsic to each write and totally
// ordered, the greatest position over a set equals the greatest over its
// parts, so ONE retained position per field is sufficient and no losing
// value is ever needed again (§5.1 S3; LWW2's discarding stands).
//
// The base holds the committed map; the overlay stages this batch's
// updates so later operations in the same batch compare against earlier
// ones, and merged() folds the overlay over the base.
type winnerSet struct {
	table   *positionTable
	base    map[fieldKey]position
	overlay map[fieldKey]position
}

func (ws *winnerSet) record(f fieldKey, w position) {
	cur, ok := ws.overlay[f]
	if !ok {
		cur, ok = ws.base[f]
	}
	if !ok || ws.table.compare(w, cur) > 0 {
		ws.overlay[f] = w
	}
}

// dominates reports whether w beats the retained winner position of f (or
// f has none yet) — the gate deciding whether an incoming write's VALUE is
// applied.
func (ws *winnerSet) dominates(f fieldKey, w position) bool {
	cur, ok := ws.overlay[f]
	if !ok {
		cur, ok = ws.base[f]
	}
	return !ok || ws.table.compare(w, cur) > 0
}

func (ws *winnerSet) merged() map[fieldKey]position {
	out := make(map[fieldKey]position, len(ws.base)+len(ws.overlay))
	for f, w := range ws.base {
		out[f] = w
	}
	for f, w := range ws.overlay {
		out[f] = w
	}
	return out
}

func (p *Projection) noteApplied(b *Batch) {
	p.batches[b.BatchID] = b
	p.ranges[b.ActorID] = append(p.ranges[b.ActorID], counterRange{b.CounterBase, b.EndCounter()})
	if p.frontier[b.ActorID] < b.EndCounter() {
		p.frontier[b.ActorID] = b.EndCounter()
	}
}

// State is an immutable deep-copy view of the projected authoritative
// state and its conflict metadata (§5.1 S1–S4 shape).
type State struct {
	// Entities holds every live entity in ascending E5 id order (C1
	// recommends ascending emission; order carries no semantics).
	Entities []Entity
	// Frontier is the causal frontier of all applied operations (S2/F1).
	Frontier map[UUID]int64
	// BatchPositions maps every referenced batch to its own frontier —
	// §5.1 S3's shared table, and the ONLY permitted encoding for
	// positions. `causalPastSize` is the sum of the referenced frontier's
	// counters and is never carried as a scalar: a sum has no inverse, so
	// a stored one would be a second source of truth that could drift from
	// the frontier the wire actually holds. The table contains EXACTLY the
	// batches the maps below reference — no unreferenced entries, and no
	// reference to an absent batch.
	BatchPositions map[batchRef]map[UUID]int64
	// WinnerPositions maps each written mergeable field to its winning
	// write position (S3/LWW5). Retention is unbounded pending lwwHorizon
	// fixation.
	WinnerPositions map[fieldKey]position
	// CreatedPositions maps each entity id within the horizon to the write
	// position of the createEntity that owns it — TOMBSTONED ids included
	// (S1), because a tombstone alone cannot distinguish a late batch
	// positioned before the creation from one inside the live interval.
	CreatedPositions map[UUID]position
	// Tombstones maps deleted entity ids to their DELETING position
	// (S4/I3). The live interval is [CreatedPositions[id], Tombstones[id]).
	Tombstones map[UUID]position
}

// State returns the current projection as a deep copy; mutating the result
// never affects the projection (no public direct-mutation API).
func (p *Projection) State() State {
	ids := make([]UUID, 0, len(p.entities))
	for id := range p.entities {
		ids = append(ids, id)
	}
	SortUUIDs(ids)
	ents := make([]Entity, 0, len(ids))
	for _, id := range ids {
		ents = append(ents, cloneEntity(p.entities[id]))
	}
	tombs := make(map[UUID]position, len(p.tombstones))
	for id, w := range p.tombstones {
		tombs[id] = w
	}
	fr := make(map[UUID]int64, len(p.frontier))
	for a, n := range p.frontier {
		fr[a] = n
	}
	wp := make(map[fieldKey]position, len(p.winnerPositions))
	for f, w := range p.winnerPositions {
		wp[f] = w
	}
	cp := make(map[UUID]position, len(p.createdPositions))
	for id, w := range p.createdPositions {
		cp[id] = w
	}
	return State{
		Entities:         ents,
		Frontier:         fr,
		BatchPositions:   p.referencedFrontiers(wp, cp, tombs),
		WinnerPositions:  wp,
		CreatedPositions: cp,
		Tombstones:       tombs,
	}
}

// referencedFrontiers returns EXACTLY the batch frontiers the supplied
// position maps reference. The live table also holds frontiers of batches
// that are only in the delivered log — every applied batch notes itself —
// and §5.1 S3 forbids emitting those: "batchPositions contains EXACTLY the
// batches those maps reference: no unreferenced entries". Pruning here
// rather than at the serializer keeps the invariant true of every State a
// caller can observe, not only of the ones that reach a writer.
func (p *Projection) referencedFrontiers(
	winners map[fieldKey]position,
	created map[UUID]position,
	tombs map[UUID]position,
) map[batchRef]map[UUID]int64 {
	out := map[batchRef]map[UUID]int64{}
	keep := func(w position) {
		if _, done := out[w.Ref]; done {
			return
		}
		fr, ok := p.positions.frontiers[w.Ref]
		if !ok {
			// Unreachable: every recorded position was noted from a batch
			// or a snapshot table before it could be compared. Emitting a
			// dangling reference would be absent required data (§9.4), and
			// inventing an empty frontier would silently claim a zero
			// causal past.
			panic("document: recorded position references a batch absent from the position table")
		}
		cp := make(map[UUID]int64, len(fr))
		for a, n := range fr {
			cp[a] = n
		}
		out[w.Ref] = cp
	}
	for _, w := range winners {
		keep(w)
	}
	for _, w := range created {
		keep(w)
	}
	for _, w := range tombs {
		keep(w)
	}
	return out
}

// Entity returns a deep copy of one live entity, or nil when the id is
// absent or tombstoned.
func (p *Projection) Entity(id UUID) Entity {
	e, ok := p.entities[id]
	if !ok {
		return nil
	}
	return cloneEntity(e)
}

// Tombstone reports whether the id is tombstoned and by which deleting
// position (S4/I3).
func (p *Projection) Tombstone(id UUID) (position, bool) {
	w, ok := p.tombstones[id]
	return w, ok
}

// Diagnostics returns a copy of the recorded projection outcomes.
func (p *Projection) Diagnostics() []Diagnostic {
	out := make([]Diagnostic, len(p.diagnostics))
	copy(out, p.diagnostics)
	return out
}

func cloneEntityMap(m map[UUID]Entity) map[UUID]Entity {
	out := make(map[UUID]Entity, len(m))
	for id, e := range m {
		out[id] = e // clones happen lazily on write via cloneEntity
	}
	return out
}

// cloneEntity deep-copies any typed entity including preserved unknown
// fields, so staged mutations can never alias committed state.
func cloneEntity(e Entity) Entity {
	unk := make([]UnknownField, len(e.UnknownFields()))
	copy(unk, e.UnknownFields())
	switch t := e.(type) {
	case *Level:
		c := *t
		c.unknown = unk
		if t.BackgroundImages != nil {
			c.BackgroundImages = append([]BackgroundImage(nil), t.BackgroundImages...)
		}
		return &c
	case *Wall:
		c := *t
		c.unknown = unk
		if t.StyleRefs != nil {
			c.StyleRefs = append([]string(nil), t.StyleRefs...)
		}
		return &c
	case *WallJoin:
		c := *t
		c.unknown = unk
		c.Members = append([]WallMember(nil), t.Members...)
		return &c
	case *Opening:
		c := *t
		c.unknown = unk
		if t.StyleRefs != nil {
			c.StyleRefs = append([]string(nil), t.StyleRefs...)
		}
		return &c
	case *RoomRecord:
		c := *t
		c.unknown = unk
		c.Name = cloneStrPtr(t.Name)
		c.FloorStyleRef = cloneStrPtr(t.FloorStyleRef)
		c.CeilingStyleRef = cloneStrPtr(t.CeilingStyleRef)
		return &c
	case *Label:
		c := *t
		c.unknown = unk
		return &c
	case *Dimension:
		c := *t
		c.unknown = unk
		c.LabelOverride = cloneStrPtr(t.LabelOverride)
		return &c
	case *AssetInstance:
		c := *t
		c.unknown = unk
		c.DisplayName = cloneStrPtr(t.DisplayName)
		return &c
	default:
		panic(fmt.Sprintf("document: unclonable entity type %T", e))
	}
}

func cloneStrPtr(s *string) *string {
	if s == nil {
		return nil
	}
	c := *s
	return &c
}
