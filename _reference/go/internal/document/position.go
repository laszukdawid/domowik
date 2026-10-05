package document

import (
	"fmt"
	"math"
	"math/bits"
)

// This file implements FORMAT-SPEC §4.4 F3's ordering position and §4.6
// LWW1's write position (owner ruling ED-0037 as refined by its comment
// (b)). Ordering is a SORT on that position, never a set of pairwise
// precedence constraints: the constraint reading admits cycles and then
// defines no order at all, which is the defect ED-0036 recorded.

// MaxOperationsPerBatch is row L23's operations-per-batch ceiling. It also
// bounds a valid operation index, which is why the wire-facing index is
// validated against it before any narrowing to int.
const MaxOperationsPerBatch = 65535

// MaxFrontierMembers is row L18's object member count, the largest number
// of actors a single frontier may name.
const MaxFrontierMembers = 65536

// causalPast is the number of operations in a batch's causal past — the
// sum of its frontier's counters (F1/F3). It does NOT fit int64: a
// frontier may carry L18 = 65,536 members (§9.3), each an int64 counter
// within L13, so the exact sum reaches 65536 × (2⁶³ − 1) = 2⁷⁹ − 2¹⁶.
// F3 requires implementations to compare it EXACTLY and FORBIDS wrapping
// or saturating arithmetic, because either would change the order. Two
// unsigned words carry the value with room to spare (2⁷⁹ < 2¹²⁸), so
// overflow is unreachable rather than merely unlikely.
type causalPast struct{ hi, lo uint64 }

// addCounter accumulates one non-negative frontier counter.
func (p causalPast) addCounter(n int64) causalPast {
	lo, carry := bits.Add64(p.lo, uint64(n), 0)
	hi, spill := bits.Add64(p.hi, 0, carry)
	if spill != 0 {
		// Unreachable at L13/L18 bounds; a panic here would mean a caller
		// bypassed frontier validation, and silently wrapping is exactly
		// what F3 forbids.
		panic("document: causal past size overflowed 128 bits")
	}
	return causalPast{hi: hi, lo: lo}
}

func (p causalPast) compare(q causalPast) int {
	if p.hi != q.hi {
		if p.hi < q.hi {
			return -1
		}
		return 1
	}
	if p.lo != q.lo {
		if p.lo < q.lo {
			return -1
		}
		return 1
	}
	return 0
}

func (p causalPast) String() string {
	if p.hi == 0 {
		return fmt.Sprintf("%d", p.lo)
	}
	return fmt.Sprintf("0x%x%016x", p.hi, p.lo)
}

// sumFrontier computes a frontier's causal past size exactly. Negative
// counters are rejected upstream by validateFraming and FromSnapshot; they
// are treated as zero here so a bypassed caller cannot manufacture a
// smaller past than it declared.
func sumFrontier(frontier map[UUID]int64) causalPast {
	var p causalPast
	for _, n := range frontier {
		if n <= 0 {
			continue
		}
		p = p.addCounter(n)
	}
	return p
}

// batchRef identifies one batch by the pair B3 makes unique. Its wire
// spelling is the §7.6 entry stem `<counterBase padded to 19>-<actorId>`;
// this is the parsed form, so ordering never re-parses text.
type batchRef struct {
	Base  int64
	Actor UUID
}

func (r batchRef) String() string {
	return fmt.Sprintf("%019d-%s", r.Base, r.Actor.String())
}

// position is one write's place in the canonical order: a REFERENCE to the
// batch whose frontier supplies `causalPastSize`, plus the operation's
// index within that batch (§4.6 LWW1, ED-0037 comment (b)).
//
// It deliberately carries no past-size scalar. §5.1 S3 stores the
// referenced batch's frontier and nothing else, and a sum has no inverse,
// so a cached scalar would be a second source of truth that could drift
// from the one the wire actually holds. Resolution happens through the
// owning table (see positionTable).
type position struct {
	Ref     batchRef
	OpIndex int64
}

// batchPositions maps each referenced batch to its own frontier verbatim —
// the §5.1 S3 `batchPositions` table in parsed form. Sharing through this
// table is the only permitted encoding; a second one would fork readers.
type batchPositions map[batchRef]map[UUID]int64

func (t batchPositions) clone() batchPositions {
	if t == nil {
		return nil
	}
	out := make(batchPositions, len(t))
	for ref, fr := range t {
		cp := make(map[UUID]int64, len(fr))
		for a, n := range fr {
			cp[a] = n
		}
		out[ref] = cp
	}
	return out
}

func (t batchPositions) equal(other batchPositions) bool {
	if len(t) != len(other) {
		return false
	}
	for ref, fr := range t {
		ofr, ok := other[ref]
		if !ok || len(ofr) != len(fr) {
			return false
		}
		for a, n := range fr {
			if ofr[a] != n {
				return false
			}
		}
	}
	return true
}

// positionTable resolves positions against the frontiers that define them
// and memoizes each batch's past size. The memo is derived state and is
// never serialized: parsing always recomputes the sum from the retained
// frontier and never trusts a stored scalar.
type positionTable struct {
	frontiers batchPositions
	pastMemo  map[batchRef]causalPast
}

// clone returns an independent table. Framing orders batches against a
// clone so a rejected call leaves the committed table untouched.
func (pt *positionTable) clone() *positionTable {
	out := &positionTable{frontiers: pt.frontiers.clone(), pastMemo: map[batchRef]causalPast{}}
	if out.frontiers == nil {
		out.frontiers = batchPositions{}
	}
	for ref, p := range pt.pastMemo {
		out.pastMemo[ref] = p
	}
	return out
}

func newPositionTable() *positionTable {
	return &positionTable{frontiers: batchPositions{}, pastMemo: map[batchRef]causalPast{}}
}

// note records a batch's frontier. Re-noting the same ref with a different
// frontier is a programming error: B3 makes (counterBase, actorId) unique,
// so one ref has exactly one frontier.
func (pt *positionTable) note(ref batchRef, frontier map[UUID]int64) error {
	if prev, seen := pt.frontiers[ref]; seen {
		if !sameFrontier(prev, frontier) {
			return wrapFraming("batch %s carries two different frontiers", ref.String())
		}
		return nil
	}
	cp := make(map[UUID]int64, len(frontier))
	for a, n := range frontier {
		cp[a] = n
	}
	pt.frontiers[ref] = cp
	return nil
}

// past returns the referenced batch's causal past size. A reference to an
// absent batch is absent required data (§5.1 S3, §9.4) and callers must
// have rejected it before ordering; ok reports that so no caller can
// silently order against a zero.
func (pt *positionTable) past(ref batchRef) (causalPast, bool) {
	if p, ok := pt.pastMemo[ref]; ok {
		return p, true
	}
	fr, ok := pt.frontiers[ref]
	if !ok {
		return causalPast{}, false
	}
	p := sumFrontier(fr)
	pt.pastMemo[ref] = p
	return p, true
}

// compare orders two write positions lexicographically by
// (causalPastSize, counterBase, actorId, operationIndex) with actorId as an
// unsigned 128-bit integer (E5) — §4.6 LWW1. The leading three components
// are exactly F3's batch position P and are constant within a batch, so a
// batch's writes sort contiguously and two batches' operations can never
// interleave (B1). ED-0037 comment (c) records why the alternative
// completion — folding the operation index into the past size — was
// rejected: it breaks exactly that property.
//
// A position whose batch is absent from the table cannot be placed. That
// is a caller error, and ordering against a fabricated zero would silently
// invent causality, so it panics rather than guessing.
func (pt *positionTable) compare(a, b position) int {
	pa, oka := pt.past(a.Ref)
	pb, okb := pt.past(b.Ref)
	if !oka || !okb {
		panic("document: write position references a batch absent from the position table")
	}
	if c := pa.compare(pb); c != 0 {
		return c
	}
	if a.Ref.Base != b.Ref.Base {
		if a.Ref.Base < b.Ref.Base {
			return -1
		}
		return 1
	}
	if c := a.Ref.Actor.Compare(b.Ref.Actor); c != 0 {
		return c
	}
	switch {
	case a.OpIndex < b.OpIndex:
		return -1
	case a.OpIndex > b.OpIndex:
		return 1
	}
	return 0
}

// less orders two BATCHES by F3's position P — the leading three
// components. Uniqueness of (counterBase, actorId) under B3 makes it a
// strict total order, so exactly one canonical order exists.
func (pt *positionTable) lessBatch(a, b batchRef) bool {
	pa, oka := pt.past(a)
	pb, okb := pt.past(b)
	if !oka || !okb {
		panic("document: batch position references a batch absent from the position table")
	}
	if c := pa.compare(pb); c != 0 {
		return c < 0
	}
	if a.Base != b.Base {
		return a.Base < b.Base
	}
	return a.Actor.Compare(b.Actor) < 0
}

func sameFrontier(a, b map[UUID]int64) bool {
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

// validateOperationIndex bounds a wire-supplied operation index before it
// is narrowed anywhere. Row L23 caps operations per batch at 65,535, so a
// valid index is within [0, 65534]; persisted validity must never depend
// on the host width of int.
func validateOperationIndex(n int64) error {
	if n < 0 {
		return wrapFraming("operationIndex must not be negative, got %d", n)
	}
	if n >= MaxOperationsPerBatch {
		return wrapFraming("operationIndex %d exceeds row L23's operations-per-batch ceiling", n)
	}
	return nil
}

// batchRefOf returns the reference identifying b.
func batchRefOf(b *Batch) batchRef {
	return batchRef{Base: b.CounterBase, Actor: b.ActorID}
}

// positionOf returns the write position of operation i of b.
func positionOf(b *Batch, i int) position {
	return position{Ref: batchRefOf(b), OpIndex: int64(i)}
}

// maxCausalPast is the largest causal past size the ratified limits admit:
// L18 members each at L13's signed-64-bit ceiling. It exists so the
// exactness test asserts the real bound rather than a round power of two.
func maxCausalPast() causalPast {
	var p causalPast
	for i := 0; i < MaxFrontierMembers; i++ {
		p = p.addCounter(math.MaxInt64)
	}
	return p
}
