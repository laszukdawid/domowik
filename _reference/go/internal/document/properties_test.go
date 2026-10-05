package document

import (
	"encoding/json"
	"fmt"
	"math/rand"
	"reflect"
	"testing"
)

// Deterministic property tests (fixed seeds, no timing): random valid and
// invalid entity sets projected through the apply seam. Every assertion is
// reproducible; a failure names its seed.

const propertySeedBase = 20260824

func jsonMarshal(m map[string]any) json.RawMessage {
	b, err := json.Marshal(m)
	if err != nil {
		panic(err)
	}
	return b
}

func levelMap(id UUID, name string) map[string]any {
	return map[string]any{
		"id": id.String(), "type": "level", "name": name,
		"elevationMm": 0, "floorThicknessMm": 200,
		"defaultWallHeightMm": 2500, "defaultCeilingHeightMm": 2500,
	}
}

func wallMap(id, levelID UUID, x1, y1, x2, y2 float64) map[string]any {
	return map[string]any{
		"id": id.String(), "type": "wall", "levelId": levelID.String(),
		"startMm": []float64{x1, y1}, "endMm": []float64{x2, y2},
		"thicknessMm": 120, "heightMm": 2500,
	}
}

func roomRecordMap(id, levelID UUID, x, y float64) map[string]any {
	return map[string]any{
		"id": id.String(), "type": "roomRecord", "levelId": levelID.String(),
		"seedMm": []float64{x, y},
	}
}

func dimensionMap(id, levelID UUID) map[string]any {
	return map[string]any{
		"id": id.String(), "type": "dimension", "levelId": levelID.String(),
		"startMm": []float64{0, 0}, "endMm": []float64{4000, 0},
	}
}

type propEntity struct {
	id   UUID
	kind Kind
	raw  json.RawMessage
}

// propWorld accumulates valid entities with resolvable references.
type propWorld struct {
	rng    *rand.Rand
	nextID uint64
	levels []UUID
	walls  []UUID
	ents   []propEntity
}

func newPropWorld(seed int64) *propWorld {
	return &propWorld{rng: rand.New(rand.NewSource(seed)), nextID: 100}
}

func (w *propWorld) id() UUID {
	w.nextID++
	return numUUID(w.nextID)
}

func (w *propWorld) pickLevel() UUID { return w.levels[w.rng.Intn(len(w.levels))] }

func (w *propWorld) coord() float64 {
	return float64(w.rng.Intn(20001)) - 10000 // within [-1e4, 1e4] << CoordBoundMm
}

func (w *propWorld) dim() float64 { return float64(50 + w.rng.Intn(5000)) }

func (w *propWorld) token() string { return fmt.Sprintf("S%d", w.rng.Intn(6)) }

func (w *propWorld) digest() string {
	const hexDigits = "0123456789abcdef"
	out := make([]byte, 64)
	for i := range out {
		out[i] = hexDigits[w.rng.Intn(16)]
	}
	return "sha256:" + string(out)
}

func (w *propWorld) add(id UUID, kind Kind, m map[string]any) {
	w.ents = append(w.ents, propEntity{id: id, kind: kind, raw: jsonMarshal(m)})
}

func (w *propWorld) generate() {
	for i := 0; i < 1+w.rng.Intn(3); i++ {
		lvl := w.id()
		m := levelMap(lvl, fmt.Sprintf("L%d", i))
		if w.rng.Intn(2) == 0 {
			imgs := make([]any, 0, 2)
			for j := 0; j < 1+w.rng.Intn(2); j++ {
				imgs = append(imgs, map[string]any{
					"digest": w.digest(),
					"anchor": map[string]any{
						"translationMm": []float64{w.coord(), w.coord()},
						"scaleUniform":  float64(1 + w.rng.Intn(10)),
						"rotationDeg":   float64(w.rng.Intn(360)),
					},
					"stackOrdinal": int64(w.rng.Intn(5)),
				})
			}
			m["backgroundImages"] = imgs
		}
		w.add(lvl, KindLevel, m)
		w.levels = append(w.levels, lvl)
	}
	for i := 0; i < 3+w.rng.Intn(6); i++ {
		wall := w.id()
		x1, y1 := w.coord(), w.coord()
		var x2, y2 float64
		for {
			x2, y2 = w.coord(), w.coord()
			if x1 != x2 || y1 != y2 {
				break
			}
		}
		m := wallMap(wall, w.pickLevel(), x1, y1, x2, y2)
		if w.rng.Intn(2) == 0 {
			toks := map[string]struct{}{}
			for j := 0; j < 1+w.rng.Intn(3); j++ {
				toks[w.token()] = struct{}{}
			}
			arr := make([]any, 0, len(toks))
			for tok := range toks {
				arr = append(arr, tok)
			}
			m["styleRefs"] = arr
		}
		w.add(wall, KindWall, m)
		w.walls = append(w.walls, wall)
	}
	if len(w.walls) >= 2 && w.rng.Intn(2) == 0 {
		a := w.walls[w.rng.Intn(len(w.walls))]
		b := w.walls[w.rng.Intn(len(w.walls))]
		if a != b {
			id := w.id()
			w.add(id, KindWallJoin, map[string]any{
				"id": id.String(), "type": "wallJoin",
				"members": []any{
					map[string]any{"wallId": a.String(), "role": "End"},
					map[string]any{"wallId": b.String(), "role": "Start"},
				},
			})
		}
	}
	for i := 0; i < 1+w.rng.Intn(4); i++ {
		host := w.walls[w.rng.Intn(len(w.walls))]
		sides := []string{"Left", "Right"}
		hinges := []string{"nearStart", "nearEnd"}
		kinds := []string{"door", "window"}
		id := w.id()
		w.add(id, KindOpening, map[string]any{
			"id": id.String(), "type": "opening", "kind": kinds[w.rng.Intn(2)],
			"hostWallId": host.String(),
			"u":          float64(w.rng.Intn(101)) / 100,
			"widthMm":    w.dim(), "heightMm": w.dim(), "depthMm": w.dim(),
			"sillMm": float64(w.rng.Intn(int(DimensionCapMm / 4))),
			"side":   sides[w.rng.Intn(2)], "hinge": hinges[w.rng.Intn(2)],
			"opensToward": sides[w.rng.Intn(2)], "swingDeg": float64(w.rng.Intn(int(SwingDegMax) + 1)),
		})
	}
	for i := 0; i < 1+w.rng.Intn(3); i++ {
		id := w.id()
		m := roomRecordMap(id, w.pickLevel(), w.coord(), w.coord())
		switch w.rng.Intn(3) {
		case 0:
			m["name"] = ""
		case 1:
			m["name"] = fmt.Sprintf("room-%d", w.rng.Intn(100))
		}
		w.add(id, KindRoomRecord, m)
	}
	for i := 0; i < 1+w.rng.Intn(3); i++ {
		lvl := w.pickLevel()
		id := w.id()
		w.add(id, KindLabel, map[string]any{
			"id": id.String(), "type": "label", "levelId": lvl.String(),
			"anchorMm": []float64{w.coord(), w.coord()},
			"name":     fmt.Sprintf("lbl-%d", w.rng.Intn(9)),
		})
	}
	for i := 0; i < 1+w.rng.Intn(3); i++ {
		lvl := w.pickLevel()
		id := w.id()
		m := dimensionMap(id, lvl)
		m["startMm"] = []float64{w.coord(), w.coord()}
		m["endMm"] = []float64{w.coord(), w.coord()}
		w.add(id, KindDimension, m)
	}
	for i := 0; i < 1+w.rng.Intn(3); i++ {
		lvl := w.pickLevel()
		id := w.id()
		w.add(id, KindAssetInstance, map[string]any{
			"id": id.String(), "type": "assetInstance", "levelId": lvl.String(),
			"digest": w.digest(),
			"transform": map[string]any{
				"translationMm": []float64{w.coord(), w.coord()},
				"scaleUniform":  float64(1 + w.rng.Intn(20)),
				"rotationDeg":   float64(w.rng.Intn(360)),
			},
		})
	}
}

// propBatches partitions creation ops into contiguous single-actor batches
// and appends schema-field update batches (counter ranges stay contiguous).
func propBatches(rng *rand.Rand, world *propWorld, actorNum uint64) []*Batch {
	actor := actorID(actorNum)
	var batches []*Batch
	counter := int64(1)
	opNum := uint64(5000)

	newBatch := func(frontier map[UUID]int64, ops ...*Operation) {
		b, err := NewBatch(numUUID(uint64(counter)*7+11), actor, counter, closedFrontier(actor, counter, frontier), ops...)
		if err != nil {
			panic(err)
		}
		counter += int64(len(ops))
		batches = append(batches, b)
	}

	i := 0
	for i < len(world.ents) {
		n := 1 + rng.Intn(3)
		if i+n > len(world.ents) {
			n = len(world.ents) - i
		}
		ops := make([]*Operation, 0, n)
		for _, e := range world.ents[i : i+n] {
			opNum++
			op, err := NewCreateOp(numUUID(opNum), e.raw)
			if err != nil {
				panic(err)
			}
			ops = append(ops, op)
		}
		newBatch(nil, ops...)
		i += n
	}

	live := make([]propEntity, len(world.ents))
	copy(live, world.ents)
	for k := 0; k < 3+rng.Intn(4); k++ {
		e := live[rng.Intn(len(live))]
		var field, value string
		switch e.kind {
		case KindLevel:
			field, value = FieldElevationMm, `-25`
		case KindWall:
			field, value = FieldThicknessMm, `150`
		case KindOpening:
			field, value = FieldSwingDeg, `45`
		case KindLabel:
			field, value = FieldName, `"prop-label"`
		case KindRoomRecord:
			field, value = FieldSeedMm, `[10,10]`
		default:
			continue // wallJoin members / assetInstance transforms are replaced wholesale; skip
		}
		opNum++
		newBatch(nil, mustSetFieldsPanic(numUUID(opNum), e.id,
			map[string]json.RawMessage{field: json.RawMessage(value)}))
	}
	return batches
}

func mustSetFieldsPanic(opID UUID, target UUID, fields map[string]json.RawMessage) *Operation {
	op, err := NewSetFieldsOp(opID, target, fields)
	if err != nil {
		panic(err)
	}
	return op
}

func TestPropertyValidSetsProjectDeterministically(t *testing.T) {
	for seed := int64(propertySeedBase); seed < propertySeedBase+12; seed++ {
		seed := seed
		t.Run(fmt.Sprintf("seed-%d", seed), func(t *testing.T) {
			world := newPropWorld(seed)
			world.generate()
			rng := rand.New(rand.NewSource(seed*31 + 7))
			batches := propBatches(rng, world, actorMain)

			ref := New()
			err := ref.Deliver(batches...)
			if err != nil {
				t.Fatalf("valid generated set rejected: %v", err)
			}
			want := ref.State()

			again := New()
			if err := again.Deliver(batches...); err != nil {
				t.Fatalf("re-run rejected: %v", err)
			}
			if !reflect.DeepEqual(want, again.State()) {
				t.Fatal("projection must be deterministic for identical input")
			}

			for perm := 0; perm < 6; perm++ {
				shuffled := append([]*Batch(nil), batches...)
				rng.Shuffle(len(shuffled), func(a, b int) { shuffled[a], shuffled[b] = shuffled[b], shuffled[a] })
				p := New()
				if err := p.Deliver(shuffled...); err != nil {
					t.Fatalf("permuted delivery rejected: %v", err)
				}
				if !reflect.DeepEqual(want, p.State()) {
					t.Fatalf("permutation %d changed the projection", perm)
				}
			}

			sortedByK := append([]*Batch(nil), batches...)
			sortBatchesByK(sortedByK)
			inc := New()
			for _, b := range sortedByK {
				if err := inc.Deliver(b); err != nil {
					t.Fatalf("incremental delivery rejected at %d: %v", b.CounterBase, err)
				}
			}
			if !reflect.DeepEqual(want, inc.State()) {
				t.Fatal("incremental delivery diverged from set delivery")
			}

			dup := New()
			if err := dup.Deliver(batches...); err != nil {
				t.Fatalf("first delivery: %v", err)
			}
			if err := dup.Deliver(batches...); err != nil {
				t.Fatalf("redelivery errored: %v", err)
			}
			if !reflect.DeepEqual(want, dup.State()) {
				t.Fatal("duplicate delivery changed state")
			}
		})
	}
}

func sortBatchesByK(bs []*Batch) {
	for i := 1; i < len(bs); i++ {
		for j := i; j > 0; j-- {
			x, y := bs[j], bs[j-1]
			if x.CounterBase < y.CounterBase ||
				(x.CounterBase == y.CounterBase && x.ActorID.Compare(y.ActorID) < 0) {
				bs[j], bs[j-1] = bs[j-1], bs[j]
				continue
			}
			break
		}
	}
}

type invalidMutation struct {
	name  string
	apply func(t *testing.T, m map[string]any)
}

func invalidMutations() []invalidMutation {
	tooBig := -(2 * CoordBoundMm)
	tooDim := 2 * DimensionCapMm
	return []invalidMutation{
		{"coordinate out of range", func(_ *testing.T, m map[string]any) { m["startMm"] = []float64{tooBig - 1, 0} }},
		{"dimension above cap", func(_ *testing.T, m map[string]any) { m["thicknessMm"] = tooDim }},
		{"zero thickness", func(_ *testing.T, m map[string]any) { m["thicknessMm"] = 0 }},
		{"null required", func(_ *testing.T, m map[string]any) { m["heightMm"] = nil }},
		{"unknown kind", func(_ *testing.T, m map[string]any) { m["type"] = "splat" }},
		{"malformed uuid", func(_ *testing.T, m map[string]any) { m["levelId"] = "zzzz" }},
		{"duplicate styleRefs", func(_ *testing.T, m map[string]any) { m["styleRefs"] = []string{"A", "A"} }},
	}
}

func TestPropertyInvalidMutationsRejectAtomically(t *testing.T) {
	for seed := int64(propertySeedBase); seed < propertySeedBase+6; seed++ {
		world := newPropWorld(seed)
		world.generate()

		for _, mut := range invalidMutations() {
			t.Run(mut.name, func(t *testing.T) {
				victim := wallMap(world.id(), world.levels[0], 0, 0, 3000, 0)
				mut.apply(t, victim)
				op, err := NewCreateOp(numUUID(777), jsonMarshal(victim))
				if err != nil {
					return // constructor-level rejection also satisfies fail-closed
				}
				b, err := NewBatch(numUUID(778), actorID(actorX), 1, nil, op)
				if err != nil {
					return
				}
				p := New()
				if err := p.Deliver(b); err == nil {
					t.Fatalf("invalid mutation %q accepted", mut.name)
				}
				got := p.State()
				if len(got.Entities) != 0 || len(got.WinnerPositions) != 0 {
					t.Fatal("atomicity: rejected batch left effects")
				}
			})
		}
	}
}
