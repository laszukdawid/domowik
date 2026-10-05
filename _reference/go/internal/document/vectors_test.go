package document

import (
	"encoding/json"
	"errors"
	"math/rand"
	"reflect"
	"testing"
)

// V5: a snapshot-seeded replica resolves late-concurrent writes identically
// to the never-compacted replica via retained winner POSITIONS, and
// redelivering compacted-away batches (unknown ids below the watermark)
// skips silently.
//
// The retained winner here is smallWrite at (2,1,X,0) — causal past 2,
// because its frontier {main:1, Y:1} observed both earlier writes. A late
// arrival is therefore ranked by ITS causal past first, not by its actor
// id: the previous form of this test had a fresh-stream late writer with
// past 0 beat the retained winner purely because its actorId sorted
// higher, which is the ED-0032 defect on the snapshot path.
func TestV5LateConcurrentResolvesIdenticallyAfterSnapshotSeed(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
	bigWrite := mustBatch(t, 2, y, 1, map[UUID]int64{main: 1},
		mustSetOp(t, 2000, lvl, setLevelName(t, "big")))
	smallWrite := mustBatch(t, 3, x, 1, map[UUID]int64{main: 1, y: 1},
		mustSetOp(t, 3000, lvl, setLevelName(t, "small")))

	full := New()
	if err := full.Deliver(create, bigWrite, smallWrite); err != nil {
		t.Fatalf("full replay: %v", err)
	}
	retained := full.State().WinnerPositions[fieldKey{Entity: lvl, Field: FieldName}]
	if retained != pos(1, x, 0) {
		t.Fatalf("precondition: the retained winner must be smallWrite, got %+v", retained)
	}

	for _, tc := range []struct {
		name     string
		frontier map[UUID]int64
		value    string
		wantWin  bool
	}{
		// Past size 2 ties the retained winner's, so the remaining
		// components decide: equal counterBase 1, and actorLateC sorts
		// above actorX.
		{"equal past, greater actor wins", map[UUID]int64{main: 1, y: 1}, "late-wins", true},
		// Past size 1 loses outright, however high the actor id sorts —
		// causality governs and the tie-break never runs.
		{"smaller past loses despite a greater actor", map[UUID]int64{main: 1}, "late-loses", false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			seeded, err := FromSnapshot(full.State(), nil)
			if err != nil {
				t.Fatalf("FromSnapshot: %v", err)
			}
			twin := New()
			if err := twin.Deliver(create, bigWrite, smallWrite); err != nil {
				t.Fatalf("twin replay: %v", err)
			}

			late := mustBatch(t, 4, actorID(actorLateC), 1, tc.frontier,
				mustSetOp(t, 4000, lvl, setLevelName(t, tc.value)))
			diagsBefore := len(seeded.Diagnostics())
			if err := seeded.Deliver(late); err != nil {
				t.Fatalf("late delivery on seeded replica: %v", err)
			}
			if err := twin.Deliver(late); err != nil {
				t.Fatalf("late delivery on uncompacted twin: %v", err)
			}
			if len(seeded.Diagnostics()) != diagsBefore {
				t.Fatal("watermark-covered redeliveries must stay silent")
			}
			if !reflect.DeepEqual(seeded.State(), twin.State()) {
				t.Fatal("V5: seeded replica diverged from the never-compacted replica")
			}
			got := seeded.Entity(lvl).(*Level).Name
			if tc.wantWin && got != tc.value {
				t.Fatalf("late writer should win against the retained position, got %q", got)
			}
			if !tc.wantWin && got == tc.value {
				t.Fatal("retained winner position must keep the write")
			}
		})
	}
}

// V6: replay from a snapshot plus the surviving log equals full-history
// replay for every permitted delivery order of that log.
func TestV6SnapshotPlusSuffixEqualsFullReplayAllOrders(t *testing.T) {
	for seed := int64(propertySeedBase); seed < propertySeedBase+6; seed++ {
		t.Run("seed", func(t *testing.T) {
			world := newPropWorld(seed)
			world.generate()
			rng := rand.New(rand.NewSource(seed*17 + 3))
			batches := propBatches(rng, world, actorMain)

			full := New()
			if err := full.Deliver(batches...); err != nil {
				t.Fatalf("full replay: %v", err)
			}
			want := full.State()

			for split := 0; split <= len(batches); split++ {
				suffix := batches[split:]
				for _, retained := range [][]*Batch{nil, suffix} {
					seeded, err := FromSnapshot(want, retained)
					if err != nil {
						t.Fatalf("FromSnapshot(split=%d): %v", split, err)
					}
					shuffled := append([]*Batch(nil), suffix...)
					rng.Shuffle(len(shuffled), func(a, b int) { shuffled[a], shuffled[b] = shuffled[b], shuffled[a] })
					if err := seeded.Deliver(shuffled...); err != nil {
						t.Fatalf("suffix delivery (split=%d): %v", split, err)
					}
					if !reflect.DeepEqual(want, seeded.State()) {
						t.Fatalf("V6: snapshot+suffix diverged at split %d", split)
					}
					// Full redelivery of every historical batch stays silent.
					if err := seeded.Deliver(batches...); err != nil {
						t.Fatalf("redelivery after seed rejected: %v", err)
					}
					if !reflect.DeepEqual(want, seeded.State()) {
						t.Fatal("V6/I2: redelivery changed state")
					}
				}
			}
		})
	}
}

// Watermark classification BEFORE range chaining: fully-below-watermark
// unknown-id batches skip silently (I2/V5), straddling ranges reject as
// torn, gaps reject, and the next contiguous range chains.
func TestWatermarkClassificationBeforeChaining(t *testing.T) {
	main := actorID(actorMain)
	lvl := numUUID(1)

	base := New()
	if err := base.Deliver(mustBatch(t, 1, main, 1, nil,
		mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")))); err != nil {
		t.Fatalf("seed: %v", err)
	}
	// Advance the watermark to 3 via two more batches.
	if err := base.Deliver(
		mustBatch(t, 2, main, 2, closedFrontier(main, 2, nil), mustSetOp(t, 1001, lvl, setLevelName(t, "two"))),
		mustBatch(t, 3, main, 3, closedFrontier(main, 3, nil), mustSetOp(t, 1002, lvl, setLevelName(t, "three"))),
	); err != nil {
		t.Fatalf("advance: %v", err)
	}
	snap := base.State()

	t.Run("fully below watermark with unknown id skips silently", func(t *testing.T) {
		p, err := FromSnapshot(snap, nil)
		if err != nil {
			t.Fatalf("seed: %v", err)
		}
		old := mustBatch(t, 90, main, 2, closedFrontier(main, 2, nil), mustSetOp(t, 9000, lvl, setLevelName(t, "ghost")))
		before := p.State()
		diags := len(p.Diagnostics())
		if err := p.Deliver(old); err != nil {
			t.Fatalf("I2/V5: below-watermark batch must skip silently, got %v", err)
		}
		if !reflect.DeepEqual(before, p.State()) || len(p.Diagnostics()) != diags {
			t.Fatal("skipped batch must have zero effect")
		}
	})

	t.Run("straddling range rejects as torn", func(t *testing.T) {
		p, _ := FromSnapshot(snap, nil)
		tear := mustBatch(t, 91, main, 3, closedFrontier(main, 3, nil),
			mustSetOp(t, 9100, lvl, setLevelName(t, "n")),
			mustSetOp(t, 9101, lvl, setLevelName(t, "m")))
		if err := p.Deliver(tear); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("tear must be malformed-framing, got %v", err)
		}
	})

	t.Run("gap above watermark rejects", func(t *testing.T) {
		p, _ := FromSnapshot(snap, nil)
		gap := mustBatch(t, 92, main, 5, closedFrontier(main, 5, nil), mustSetOp(t, 9200, lvl, setLevelName(t, "n")))
		if err := p.Deliver(gap); !errors.Is(err, ErrMalformedFraming) {
			t.Fatalf("gap must be malformed-framing, got %v", err)
		}
	})

	t.Run("next contiguous range chains and applies", func(t *testing.T) {
		p, _ := FromSnapshot(snap, nil)
		next := mustBatch(t, 93, main, 4, closedFrontier(main, 4, nil), mustSetOp(t, 9300, lvl, setLevelName(t, "four")))
		if err := p.Deliver(next); err != nil {
			t.Fatalf("contiguous delivery: %v", err)
		}
		if got := p.Entity(lvl).(*Level).Name; got != "four" {
			t.Fatalf("chained batch did not apply, got %q", got)
		}
	})
}

// FromSnapshot fails closed on inconsistent retention metadata instead of
// synthesizing masking ranges.
func TestFromSnapshotRejectsInconsistentRetention(t *testing.T) {
	main := actorID(actorMain)
	lvl := numUUID(1)
	b1 := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "L1")))
	b2 := mustBatch(t, 2, main, 2, closedFrontier(main, 2, nil), mustSetOp(t, 1001, lvl, setLevelName(t, "two")))
	b3 := mustBatch(t, 3, main, 3, closedFrontier(main, 3, nil), mustSetOp(t, 1002, lvl, setLevelName(t, "three")))

	full := New()
	if err := full.Deliver(b1, b2, b3); err != nil {
		t.Fatalf("history: %v", err)
	}
	snap := full.State()

	cases := []struct {
		name     string
		retained []*Batch
	}{
		{"partial retention missing the tail batch", []*Batch{b1}},
		{"retention beyond the frontier", []*Batch{b1, b2, b3, b3}},
		{"duplicate retained id", []*Batch{b3, b3}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := FromSnapshot(snap, tc.retained); err == nil {
				t.Fatal("inconsistent retention metadata must fail closed")
			}
		})
	}

	t.Run("consistent tail retention accepts and chains", func(t *testing.T) {
		p, err := FromSnapshot(snap, []*Batch{b3})
		if err != nil {
			t.Fatalf("consistent retention must accept: %v", err)
		}
		next := mustBatch(t, 4, main, 4, closedFrontier(main, 4, nil), mustSetOp(t, 1003, lvl, setLevelName(t, "four")))
		if err := p.Deliver(next); err != nil {
			t.Fatalf("chain after retained tail: %v", err)
		}
	})
}

// Creation keys survive snapshot seeding so late-concurrent create
// collisions (LWW4) resolve identically to the never-compacted replica.
func TestV5ConcurrentCreateCollisionAfterSnapshotSeed(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)
	fromY := rawMap(t, levelJSON(t, e, "from-Y"))
	fromY["elevationMm"] = json.RawMessage(`55`)
	bx := mustBatch(t, 1, x, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "from-X")))
	by := mustBatch(t, 2, y, 1, nil, mustCreateOp(t, 2000, rawJSON(t, fromY)))

	full := New()
	if err := full.Deliver(bx, by); err != nil {
		t.Fatalf("history: %v", err)
	}
	seeded, err := FromSnapshot(full.State(), nil)
	if err != nil {
		t.Fatalf("seed: %v", err)
	}
	// Redelivery of both collision participants must stay silent and leave
	// the resolved winner (greater K = Y) untouched.
	if err := seeded.Deliver(bx, by); err != nil {
		t.Fatalf("redelivery: %v", err)
	}
	ent := seeded.Entity(e).(*Level)
	if ent.Name != "from-Y" || ent.ElevationMm != 55 {
		t.Fatalf("LWW4 resolution not preserved through snapshot seeding: %+v", ent)
	}
	if !reflect.DeepEqual(full.State(), seeded.State()) {
		t.Fatal("seeded state diverged from uncompacted state")
	}
}

// F1 regression: concurrent writes delivered in SEPARATE Deliver calls
// must resolve by greatest K (LWW1/V3), not by arrival order.
func TestV3ConcurrentWritesIndependentAcrossSeparateCalls(t *testing.T) {
	main, x, y := actorID(actorMain), actorID(actorX), actorID(actorY)
	lvl := numUUID(1)
	create := mustBatch(t, 1, main, 1, nil, mustCreateOp(t, 1000, levelJSON(t, lvl, "base")))
	xw := mustBatch(t, 2, x, 1, nil, mustSetOp(t, 2000, lvl, setLevelName(t, "from-X")))
	yw := mustBatch(t, 3, y, 1, nil, mustSetOp(t, 3000, lvl, setLevelName(t, "from-Y")))

	orders := [][][]*Batch{
		{{create}, {xw}, {yw}}, // X arrives before Y
		{{create}, {yw}, {xw}}, // Y arrives before X
		{{create, xw}, {yw}},   // mixed call splits
		{{create, yw}, {xw}},
	}
	var states []State
	for i, order := range orders {
		p := New()
		for _, call := range order {
			if err := p.Deliver(call...); err != nil {
				t.Fatalf("order %d: %v", i, err)
			}
		}
		states = append(states, p.State())
	}
	for i := 1; i < len(states); i++ {
		if !reflect.DeepEqual(states[0], states[i]) {
			t.Fatalf("V3/F3: separate-call arrival order changed the projection at order %d", i)
		}
	}
	if got := states[0].Entities[0].(*Level).Name; got != "from-Y" {
		t.Fatalf("greatest-K writer ((3,y)) must win regardless of arrival, got %q", got)
	}
}

// Create collisions are resolved by comparing CREATION KEYS, so they were
// already arrival-order independent; this locks that across calls.
func TestLWW4ConcurrentCreateCollisionAcrossSeparateCalls(t *testing.T) {
	x, y := actorID(actorX), actorID(actorY)
	e := numUUID(1)
	fromY := rawMap(t, levelJSON(t, e, "from-Y"))
	fromY["elevationMm"] = json.RawMessage(`55`)
	bx := mustBatch(t, 1, x, 1, nil, mustCreateOp(t, 1000, levelJSON(t, e, "from-X")))
	by := mustBatch(t, 2, y, 1, nil, mustCreateOp(t, 2000, rawJSON(t, fromY)))

	var states []State
	for _, order := range [][][]*Batch{{{bx}, {by}}, {{by}, {bx}}} {
		p := New()
		for _, call := range order {
			if err := p.Deliver(call...); err != nil {
				t.Fatalf("Deliver: %v", err)
			}
		}
		states = append(states, p.State())
	}
	if !reflect.DeepEqual(states[0], states[1]) {
		t.Fatal("LWW4 collision resolution must not depend on arrival order")
	}
	ent := states[0].Entities[0].(*Level)
	if ent.Name != "from-Y" || ent.ElevationMm != 55 {
		t.Fatalf("greater-K create must be kept, got %+v", ent)
	}
}

// F3/V7 delivery model, explicit separate-call form: batches may arrive in
// any order and any call granularity. The projection is the canonical
// function of the TOTAL delivered set, so create-vs-modify concurrency
// resolves identically regardless of which batch arrives first — including
// the stale-discard outcome, which is evaluated at each batch's CANONICAL
// position during deterministic union replay.
func TestV7CreateVersusModifyIndependentAcrossSeparateCalls(t *testing.T) {
	for _, tc := range []struct {
		name        string
		creatorNum  uint64 // canonically FIRST actor wins the tie by lower id
		modifierNum uint64
		wantName    string // projected value for the modifier's write
		wantStale   bool   // whether the modify batch discards stale
	}{
		{
			name:        "modifier canonically follows create: it applies",
			creatorNum:  actorX, // a002 sorts first
			modifierNum: actorY, // a003 sorts second
			wantName:    "modified",
			wantStale:   false,
		},
		{
			name:        "modifier canonically precedes create: discarded stale",
			creatorNum:  actorY,
			modifierNum: actorX,
			wantName:    "created",
			wantStale:   true,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			e := numUUID(1)
			create := mustBatch(t, 1, actorID(tc.creatorNum), 1, nil,
				mustCreateOp(t, 1000, levelJSON(t, e, "created")))
			modify := mustBatch(t, 2, actorID(tc.modifierNum), 1, nil,
				mustSetOp(t, 2000, e, setLevelName(t, "modified")))

			var states []State
			var diagCounts []int
			for _, order := range [][][]*Batch{
				{{create}, {modify}}, // create arrives first
				{{modify}, {create}}, // modify arrives first (stale on arrival)
			} {
				p := New()
				for _, call := range order {
					if err := p.Deliver(call...); err != nil {
						t.Fatalf("Deliver: %v", err)
					}
				}
				states = append(states, p.State())
				diagCounts = append(diagCounts, len(p.Diagnostics()))
			}
			for i := 1; i < len(states); i++ {
				if !reflect.DeepEqual(states[0], states[i]) {
					t.Fatal("V7/F3: separate-call arrival order changed the projection")
				}
			}
			if got := states[0].Entities[0].(*Level).Name; got != tc.wantName {
				t.Fatalf("canonical resolution wrong: want %q got %q", tc.wantName, got)
			}
			// The stale-discard diagnostic presence must match the canonical
			// verdict in BOTH arrival orders (reconciled by union replay).
			for _, n := range diagCounts {
				has := n > 0
				if has != tc.wantStale {
					t.Fatalf("stale diagnostic presence %v != canonical verdict %v", has, tc.wantStale)
				}
			}
		})
	}
}
