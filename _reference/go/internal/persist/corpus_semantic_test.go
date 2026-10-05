package persist_test

import (
	"encoding/json"
	"reflect"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// corpus_semantic_test.go: golden/adversarial/migration JSON fixtures and
// the V1–V7 deterministic projection vectors, asserted against the
// machine-readable `assert` blocks of EXPECTATIONS.md.

func mustParse(t *testing.T, rel string) map[string]any {
	t.Helper()
	var m map[string]any
	if err := json.Unmarshal(corpusFile(t, rel), &m); err != nil {
		t.Fatalf("%s: %v", rel, err)
	}
	return m
}

func batchFromFixture(t *testing.T, doc any) *document.Batch {
	t.Helper()
	raw, err := json.Marshal(doc)
	if err != nil {
		t.Fatal(err)
	}
	b, perr := buildBatchForTest(raw)
	if perr != nil {
		t.Fatalf("batch fixture unparsable: %v", perr)
	}
	return b
}

// TestGoldenEntityFixtures decodes every golden entity through the
// document schema and round-trips it through EncodeEntity/DecodeEntity.
func TestGoldenEntityFixtures(t *testing.T) {
	for _, e := range loadExpectations(t) {
		if !strings.HasPrefix(e.ID, "golden/entities/") {
			continue
		}
		e := e
		t.Run(e.ID, func(t *testing.T) {
			raw := corpusFile(t, e.Artifact)
			var probe map[string]any
			_ = json.Unmarshal(raw, &probe)
			var docs []json.RawMessage
			if bundle, ok := probe["entities"].(map[string]any); ok {
				for _, v := range bundle {
					b, _ := json.Marshal(v)
					docs = append(docs, b)
				}
			} else {
				docs = append(docs, json.RawMessage(raw))
			}
			if len(docs) == 0 {
				t.Fatal("fixture carries no entities")
			}
			for _, docRaw := range docs {
				ent, err := document.DecodeEntity(docRaw)
				if err != nil {
					t.Fatalf("decode: %v", err)
				}
				if kind, _ := e.Assert["entityKind"].(string); kind != "" && string(ent.Kind()) != kind {
					t.Fatalf("kind mismatch: %s vs %s", ent.Kind(), kind)
				}
				reenc, err := document.EncodeEntity(ent)
				if err != nil {
					t.Fatalf("re-encode: %v", err)
				}
				again, err := document.DecodeEntity(reenc)
				if err != nil {
					t.Fatalf("re-decode: %v", err)
				}
				if again.EntityID() != ent.EntityID() {
					t.Fatal("identity lost across codec round trip")
				}
			}
		})
	}
}

// entitySeededState builds a minimal replay root holding exactly the
// named entities (by trailing short form) from the golden snapshot, with
// otherwise-empty causal metadata. Vectors presuppose precisely these
// live targets and nothing else.
func entitySeededState(t *testing.T, shorts ...string) document.State {
	t.Helper()
	raw := corpusFile(t, "golden/snapshots/snapshot-full-s1-s5.json")
	state, _, err := persist.ParseSnapshotDocument(raw)
	if err != nil {
		t.Fatalf("snapshot fixture: %v", err)
	}
	want := map[string]bool{}
	for _, s := range shorts {
		want[s] = true
	}
	seed := document.State{
		Tombstones:       map[document.UUID]document.Position{},
		Frontier:         map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		CreatedPositions: map[document.UUID]document.Position{},
	}
	// S2 covers every operation the snapshot retains a position for. A seed
	// keeping creation positions while claiming to have applied nothing is
	// not a snapshot any writer could produce, and FromSnapshot now says
	// so — the watermark is raised to cover exactly the positions kept, and
	// each vector raises it further to describe its own compacted history.
	for _, e := range state.Entities {
		id := e.EntityID().String()
		if !want[id[len(id)-4:]] {
			continue
		}
		seed.Entities = append(seed.Entities, e)
		// §5.1 S1 makes the creation position required for every entity in
		// the horizon, and I5 branch 1 versus branch 2 is decided against
		// it: a seed that dropped it could not say whether a late batch
		// precedes the creation or falls inside the live interval.
		w, ok := state.CreatedPositions[e.EntityID()]
		if !ok {
			t.Fatalf("golden snapshot carries no creation position for %s", id)
		}
		seed.CreatedPositions[e.EntityID()] = w
		carryBatchPosition(t, &seed, state, w)
		ref := document.BatchOf(w)
		if counter := ref.Base + document.OperationIndexOf(w); seed.Frontier[ref.Actor] < counter {
			seed.Frontier[ref.Actor] = counter
		}
	}
	return seed
}

// carryBatchPosition copies the frontier defining w into the seed's shared
// table. §5.1 S3 requires the table to hold exactly the referenced batches,
// so seeds are assembled reference by reference rather than wholesale.
func carryBatchPosition(t *testing.T, seed *document.State, from document.State, w document.Position) {
	t.Helper()
	ref := document.BatchOf(w)
	if _, done := seed.BatchPositions[ref]; done {
		return
	}
	fr, ok := from.BatchPositions[ref]
	if !ok {
		t.Fatalf("no batchPositions entry for %s", document.StemOf(ref))
	}
	cp := make(map[document.UUID]int64, len(fr))
	for a, n := range fr {
		cp[a] = n
	}
	seed.BatchPositions[ref] = cp
}

// fullSnapshotState loads golden/snapshots/snapshot-full-s1-s5.json
// verbatim as the replay root (V6 snapshot replay).
func fullSnapshotState(t *testing.T) document.State {
	t.Helper()
	raw := corpusFile(t, "golden/snapshots/snapshot-full-s1-s5.json")
	state, _, err := persist.ParseSnapshotDocument(raw)
	if err != nil {
		t.Fatalf("snapshot fixture: %v", err)
	}
	return state
}

// TestProjectionVectors replays V1–V7 fixture sets and checks the pinned
// canonical outcomes.
func TestProjectionVectors(t *testing.T) {
	idx := expectationIndex(t)

	replay := func(t *testing.T, batches []*document.Batch, seed document.State) *document.Projection {
		t.Helper()
		// Each vector describes an actor whose prior history is COMPACTED:
		// its batches start above counter 1 with everything below them
		// dropped. S2 is the record of what the snapshot covers, so the
		// seed states that prior history explicitly rather than leaving the
		// actor absent and relying on an implication. Raising the watermark
		// to one below an actor's earliest delivered batch is exactly the
		// history the fixture prose already describes.
		for _, b := range batches {
			if b == nil {
				continue
			}
			if w := b.CounterBase - 1; seed.Frontier[b.ActorID] < w {
				seed.Frontier[b.ActorID] = w
			}
		}
		proj, err := document.ReplayFromSnapshot(seed, batches)
		if err != nil {
			t.Fatalf("replay: %v", err)
		}
		return proj
	}

	collect := func(t *testing.T, fixture map[string]any) []*document.Batch {
		t.Helper()
		var batches []*document.Batch
		if arr, ok := fixture["delivered"].([]any); ok {
			for _, d := range arr {
				batches = append(batches, batchFromFixture(t, d))
			}
		}
		if m, ok := fixture["batches"].(map[string]any); ok {
			for _, v := range m {
				batches = append(batches, batchFromFixture(t, v))
			}
		}
		return batches
	}

	t.Run("adversarial/vectors/v1-duplicate-delivery", func(t *testing.T) {
		e := idx["adversarial/vectors/v1-duplicate-delivery"]
		fixture := mustParse(t, e.Artifact)
		batches := collect(t, fixture)
		if len(batches) != 2 || batches[0].BatchID != batches[1].BatchID {
			t.Fatal("fixture must carry one identical duplicate pair")
		}
		proj := replay(t, batches, entitySeededState(t, "0001"))
		checkV1(t, e.Assert, proj)
	})
	t.Run("adversarial/vectors/v2-reordered-delivery", func(t *testing.T) {
		e := idx["adversarial/vectors/v2-reordered-delivery"]
		checkV2(t, e, mustParse(t, e.Artifact))
	})
	t.Run("adversarial/vectors/v3-concurrent-set-set", func(t *testing.T) {
		e := idx["adversarial/vectors/v3-concurrent-set-set"]
		fixture := mustParse(t, e.Artifact)
		proj := replay(t, collect(t, fixture), entitySeededState(t, "0001", "0002"))
		checkWinner(t, e.Assert, proj)
	})
	t.Run("adversarial/vectors/v4-delete-vs-set", func(t *testing.T) {
		e := idx["adversarial/vectors/v4-delete-vs-set"]
		fixture := mustParse(t, e.Artifact)
		proj := replay(t, collect(t, fixture), entitySeededState(t, "0001", "0002"))
		checkTombstone(t, e.Assert, proj)
	})
	t.Run("adversarial/vectors/v4b-stale-set-after-delete", func(t *testing.T) {
		e := idx["adversarial/vectors/v4b-stale-set-after-delete"]
		fixture := mustParse(t, e.Artifact)
		proj := replay(t, collect(t, fixture), entitySeededState(t, "0001", "0002"))
		checkStaleDiscard(t, e.Assert, proj)
	})
	t.Run("adversarial/vectors/v5-late-concurrent-after-compaction", func(t *testing.T) {
		e := idx["adversarial/vectors/v5-late-concurrent-after-compaction"]
		fixture := mustParse(t, e.Artifact)
		seed := entitySeededState(t, "0001", "0002")
		snapMeta := fixture["snapshot"].(map[string]any)
		for actor, n := range snapMeta["frontier"].(map[string]any) {
			u, uerr := document.ParseUUID(actor)
			if uerr != nil {
				t.Fatal(uerr)
			}
			seed.Frontier[u] = int64(n.(float64))
		}
		// The fixture's own batchPositions define the retained winner's
		// causalPastSize; nothing derives it from the position itself.
		for stem, fr := range snapMeta["batchPositions"].(map[string]any) {
			ref := mustStem(t, stem)
			counters := map[document.UUID]int64{}
			for actor, n := range fr.(map[string]any) {
				u, uerr := document.ParseUUID(actor)
				if uerr != nil {
					t.Fatal(uerr)
				}
				counters[u] = int64(n.(float64))
			}
			seed.BatchPositions[ref] = counters
		}
		for key, w := range snapMeta["winnerPositions"].(map[string]any) {
			entityText, field, _ := strings.Cut(key, ".")
			u, uerr := document.ParseUUID(entityText)
			if uerr != nil {
				t.Fatal(uerr)
			}
			seed.WinnerPositions[document.NewFieldKey(u, field)] = mustPosition(t, w)
		}
		var late []*document.Batch
		late = append(late, batchFromFixture(t, fixture["lateBatch"]))
		proj := replay(t, late, seed)
		checkV5(t, e.Assert, proj)
	})
	t.Run("adversarial/vectors/v6-snapshot-replay", func(t *testing.T) {
		e := idx["adversarial/vectors/v6-snapshot-replay"]
		fixture := mustParse(t, e.Artifact)
		sub, ok := fixture["subsequentBatch"].(map[string]any)
		if !ok {
			t.Fatal("v6 requires subsequentBatch")
		}
		proj := replay(t, []*document.Batch{batchFromFixture(t, sub)}, fullSnapshotState(t))
		checkV6GoldenPath(t, e.Assert, proj)
		checkV6EqualsFullReplay(t, e.Assert, fixture)
	})
	t.Run("adversarial/vectors/v7-create-vs-modify", func(t *testing.T) {
		e := idx["adversarial/vectors/v7-create-vs-modify"]
		fixture := mustParse(t, e.Artifact)
		checkV7(t, e.Assert, fixture)
	})
}

func frontierOfState(proj *document.Projection) map[string]int64 {
	out := map[string]int64{}
	for a, n := range proj.State().Frontier {
		out[a.String()] = n
	}
	return out
}

func checkV1(t *testing.T, assert map[string]any, proj *document.Projection) {
	wantFrontier, _ := assert["frontierAfter"].(map[string]any)
	got := frontierOfState(proj)
	for actor, wantN := range wantFrontier {
		want := int64(wantN.(float64))
		if got[actor] != want {
			t.Fatalf("frontier[%s]=%d want %d", actor, got[actor], want)
		}
	}
	// entityCountAfter pins the VECTOR-created population: the wall must
	// exist exactly once after the duplicate delivery (the seeded level is
	// pre-existing context, not part of the counted delta).
	walls := 0
	for _, ent := range proj.State().Entities {
		if ent.Kind() == document.KindWall {
			walls++
		}
	}
	if ec, _ := assert["entityCountAfter"].(float64); walls != int(ec) {
		t.Fatalf("wall count %d want %d (I2: duplicate ignored)", walls, int(ec))
	}
}

// checkV2 executes the reordered-delivery vector. Every assertion FS §4.4
// F3 determines is asserted, including the one the delivered
// implementation used to contradict.
//
// The executionBlocked marker this function once honoured is gone, removed
// only after it had become untrue: issue 0011's write position makes
// B-later's causalPastSize 69 (frontier {Y:9, X:60}) beat A-earlier's 59,
// so the causally later write wins and the projection holds 150. The
// harness's own guard fired first and said so — "finalThicknessMm now
// MATCHES its oracle (150): the issue-0011 conformance defect is resolved,
// so remove the executionBlocked marker consciously" — which is exactly the
// conscious act ED-0033 designed it to force.
//
// ED-0045 restates the discharge as an OBSERVABLE rather than a mechanism,
// because two earlier formulations specified machinery nobody had checked
// existed: V2 must be demonstrably executed and demonstrably able to fail.
// The second half is a mutation proof recorded in this issue's evidence,
// not something this comment can assert.
func checkV2(t *testing.T, e Expectation, fixture map[string]any) {
	t.Helper()
	const (
		actorXText = "01948f5a-ca77-7a09-9e76-5ace11b12fd2" // A-earlier
		actorYText = "01948f80-0000-7000-8000-00000000bb00" // B-later
		earlierKey = "A-earlier"
		laterKey   = "B-later"
	)
	if e.Blocks("finalThicknessMm") {
		t.Fatal("V2 carries an executionBlocked marker again; the oracle is asserted unconditionally now")
	}
	named, _ := fixture["batches"].(map[string]any)
	if _, ok := named[earlierKey]; !ok {
		t.Fatalf("fixture must carry %q", earlierKey)
	}
	if _, ok := named[laterKey]; !ok {
		t.Fatalf("fixture must carry %q", laterKey)
	}
	batch := func(key string) *document.Batch { return batchFromFixture(t, named[key]) }

	// The causal edge IS the vector. Assert it from the fixture bytes before
	// running anything: if B-later stopped observing A-earlier, the two
	// batches would merely be CONCURRENT, and every subtest below would
	// still pass — holding via stale discard, order equality trivially, and
	// the value assertion by the tie-break rather than by causality. A V7
	// case would then masquerade as V2, which is precisely the assertion
	// theatre this remediation exists to remove.
	requireCausalEdge(t, batch(earlierKey), batch(laterKey), earlierKey, laterKey)

	// Both actors carry compacted prior history: A-earlier reserves [60,60]
	// behind X's watermark 59, B-later reserves [10,10] behind Y's 9. Only
	// B-later's own frontier names the other actor (X through 60), which is
	// what makes it causally LATER rather than concurrent.
	seed := func() document.State {
		st := entitySeededState(t, "0001")
		st.Frontier[mustUUIDText(t, actorXText)] = 59
		st.Frontier[mustUUIDText(t, actorYText)] = 9
		return st
	}

	t.Run("causal-holding", func(t *testing.T) {
		// F3: a batch whose causal prerequisites are absent must not take
		// effect. F3 calls holding "an implementation strategy", so the
		// refusal's error identity is NOT asserted as a normative token —
		// only the observable absence of effect.
		proj, perr := document.FromSnapshot(seed(), nil)
		if perr != nil {
			t.Fatal(perr)
		}
		before := proj.State()
		derr := proj.Deliver(batch(laterKey))
		after := proj.State()
		if !reflect.DeepEqual(before, after) {
			t.Fatal("F3: B-later must not take effect before A-earlier is present")
		}
		if _, live := entityByID(after, "019534c7-0000-7000-8000-000000000002"); live {
			t.Fatal("F3: B-later's target must not be conjured into existence")
		}
		t.Logf("B-later alone does not apply; the implementation refuses it with: %v", derr)
	})

	t.Run("order-independence", func(t *testing.T) {
		// F3: the projection is a function of the delivered SET, never of
		// arrival sequence or call granularity. Separate sequential calls
		// are compared against one reordered call.
		sequential, perr := document.FromSnapshot(seed(), nil)
		if perr != nil {
			t.Fatal(perr)
		}
		if err := sequential.Deliver(batch(earlierKey)); err != nil {
			t.Fatalf("delivering A-earlier: %v", err)
		}
		if err := sequential.Deliver(batch(laterKey)); err != nil {
			t.Fatalf("delivering B-later after its prerequisite: %v", err)
		}

		reordered, perr := document.FromSnapshot(seed(), nil)
		if perr != nil {
			t.Fatal(perr)
		}
		if err := reordered.Deliver(batch(laterKey), batch(earlierKey)); err != nil {
			t.Fatalf("delivering the reordered set: %v", err)
		}
		if !reflect.DeepEqual(sequential.State(), reordered.State()) {
			t.Fatal("F3: arrival order and call granularity must not change the projection")
		}
		// Guard against the equality holding vacuously.
		if _, ok := entityByID(sequential.State(), "019534c7-0000-7000-8000-000000000002"); !ok {
			t.Fatal("both deliveries must produce the vector's wall")
		}
	})

	t.Run("final-projection-equals-in-order-delivery", func(t *testing.T) {
		const assertion = "finalThicknessMm"
		want, _ := e.Assert[assertion].(float64)
		if want == 0 {
			t.Fatalf("oracle must pin %s", assertion)
		}
		proj, perr := document.FromSnapshot(seed(), nil)
		if perr != nil {
			t.Fatal(perr)
		}
		if err := proj.Deliver(batch(earlierKey), batch(laterKey)); err != nil {
			t.Fatalf("delivering the vector: %v", err)
		}
		got := fieldOf(findEntityByType(t, proj, "wall"), "thicknessMm")
		if got != want {
			t.Fatalf("thicknessMm=%v want %v (canonical order must equal in-order delivery)", got, want)
		}
		// The winner is asserted as a POSITION as well as a value, so a
		// coincidentally-correct value cannot pass for a correct ordering.
		wall := mustUUIDText(t, "019534c7-0000-7000-8000-000000000002")
		w, ok := proj.State().WinnerPositions[document.NewFieldKey(wall, "thicknessMm")]
		if !ok {
			t.Fatal("the winning write must retain a position (S3)")
		}
		if document.BatchOf(w).Actor != mustUUIDText(t, actorYText) {
			t.Fatalf("the causally later writer must own the retained position, got %v", w)
		}
	})
}

func checkWinner(t *testing.T, assert map[string]any, proj *document.Projection) {
	w, _ := assert["winner"].(map[string]any)
	field, _ := w["field"].(string)
	wantBatch, _ := w["batch"].(string)
	wantIndex := int64(0)
	if v, ok := w["operationIndex"].(float64); ok {
		wantIndex = int64(v)
	}
	state := proj.State()
	matched := false
	for f, got := range state.WinnerPositions {
		if f.Field != field {
			continue
		}
		if document.StemOf(document.BatchOf(got)) == wantBatch && document.OperationIndexOf(got) == wantIndex {
			matched = true
		}
	}
	if !matched {
		t.Fatalf("no winner position for %s at %s#%d; got %v", field, wantBatch, wantIndex, state.WinnerPositions)
	}
}

func mustStem(t *testing.T, stem string) document.BatchRef {
	t.Helper()
	const digits = 19
	if len(stem) != digits+1+36 || stem[digits] != '-' {
		t.Fatalf("malformed batch stem %q", stem)
	}
	var base int64
	for i := 0; i < digits; i++ {
		c := stem[i]
		if c < '0' || c > '9' {
			t.Fatalf("malformed batch stem %q", stem)
		}
		base = base*10 + int64(c-'0')
	}
	actor, err := document.ParseUUID(stem[digits+1:])
	if err != nil {
		t.Fatalf("malformed batch stem %q: %v", stem, err)
	}
	return document.NewBatchRef(base, actor)
}

func mustPosition(t *testing.T, v any) document.Position {
	t.Helper()
	m, ok := v.(map[string]any)
	if !ok {
		t.Fatalf("position is not an object: %v", v)
	}
	idx := int64(0)
	if n, ok := m["operationIndex"].(float64); ok {
		idx = int64(n)
	}
	pos, err := document.NewPosition(mustStem(t, m["batch"].(string)), idx)
	if err != nil {
		t.Fatal(err)
	}
	return pos
}

func checkTombstone(t *testing.T, assert map[string]any, proj *document.Projection) {
	if tomb, _ := assert["tombstoneRetained"].(bool); tomb {
		if len(proj.State().Tombstones) == 0 {
			t.Fatal("expected retained tombstone")
		}
	}
	if want, ok := assert["deletingPositionBatch"].(string); ok && want != "" {
		found := false
		for _, w := range proj.State().Tombstones {
			if document.StemOf(document.BatchOf(w)) == want {
				found = true
			}
		}
		if !found {
			t.Fatalf("no tombstone deleting position in batch %s", want)
		}
	}
}

func checkStaleDiscard(t *testing.T, assert map[string]any, proj *document.Projection) {
	discarded, _ := assert["discardedBatchIds"].([]any)
	if len(discarded) == 0 {
		t.Fatal("oracle requires discard-stale with discardedBatchIds")
	}
	// I5: the stale batch is discarded atomically and recorded in
	// projection diagnostics; nothing it targeted resurrects.
	found := false
	for _, d := range proj.Diagnostics() {
		if d.Code == document.DiagStaleBatchDiscard {
			found = true
		}
	}
	if !found {
		t.Fatal("stale discard must surface in projection diagnostics")
	}
}

func checkV5(t *testing.T, assert map[string]any, proj *document.Projection) {
	// Late-concurrent write must NOT win against the snapshot-retained
	// winner key: the field keeps its snapshot value.
	if applied, _ := assert["lateBatchApplied"].(bool); applied {
		t.Fatal("oracle pins lateBatchApplied=false")
	}
}

// emptyReplayRoot is the replay root of a replica that has compacted
// nothing: no entities, no causal metadata. ReplayFromSnapshot implies each
// actor's compacted prefix from its earliest retained batch, so a history
// starting above counter 1 chains here exactly as it would behind a
// watermark.
func emptyReplayRoot() document.State {
	return document.State{
		Tombstones:       map[document.UUID]document.Position{},
		Frontier:         map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		CreatedPositions: map[document.UUID]document.Position{},
	}
}

// checkV6GoldenPath pins the NAMED effects of replaying the subsequent
// batch on top of the committed golden snapshot. It replaces an assertion
// that the projection was merely non-empty, which passed for any snapshot
// and any batch.
func checkV6GoldenPath(t *testing.T, assert map[string]any, proj *document.Projection) {
	t.Helper()
	if eq, _ := assert["authoritativeStateEqualsFullReplay"].(bool); !eq {
		t.Fatalf("oracle must pin authoritativeStateEqualsFullReplay; got %v",
			assert["authoritativeStateEqualsFullReplay"])
	}
	state := proj.State()

	// S1: the snapshot's live entities survive and the subsequent batch's
	// setFields lands on the level it targets.
	const levelID = "019534c7-0000-7000-8000-000000000001"
	lvl, ok := entityByID(state, levelID)
	if !ok {
		t.Fatalf("snapshot level %s must survive replay", levelID)
	}
	if got := stringFieldOf(t, lvl, "name"); got != "Ground floor" {
		t.Fatalf("subsequent setFields must rename the level, got %q", got)
	}
	if _, ok := entityByID(state, "019534c7-0000-7000-8000-000000000002"); !ok {
		t.Fatal("the snapshot's wall must survive replay untouched")
	}

	// S2/B5: the frontier advances to the batch END and nowhere else.
	const actorX = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	if got := frontierOfState(proj)[actorX]; got != 46 {
		t.Fatalf("frontier[X] must advance to the subsequent batch end 46, got %d", got)
	}

	// S4/I3: a tombstone retained by the snapshot stays retained across
	// replay; compaction never resurrects.
	tomb, terr := document.ParseUUID("01948f55-6b18-7d3a-a153-1ee2f33cc402")
	if terr != nil {
		t.Fatal(terr)
	}
	if _, held := state.Tombstones[tomb]; !held {
		t.Fatal("snapshot-retained tombstone must survive replay (S4/I3)")
	}
	if _, live := entityByID(state, tomb.String()); live {
		t.Fatal("a tombstoned entity must not be live")
	}
}

// checkV6EqualsFullReplay proves the V6 claim itself: an authoritative
// projection rebuilt from a snapshot plus its subsequent log equals the
// projection of the full history.
//
// The two sides differ only in WHERE the history is absorbed. The compacted
// side is serialized through the real snapshot encoder by BuildPackage and
// read back through Load, so the equality crosses snapshot
// serialization/parsing rather than comparing an in-memory checkpoint that
// was never persisted. Comparison is over the complete projected State,
// including S3 winner keys and created keys.
func checkV6EqualsFullReplay(t *testing.T, assert map[string]any, fixture map[string]any) {
	t.Helper()
	raw, _ := fixture["fullHistory"].([]any)
	if len(raw) == 0 {
		t.Fatal("V6 requires fullHistory: equality with full-history replay cannot be asserted without one")
	}
	history := make([]*document.Batch, 0, len(raw))
	for _, h := range raw {
		history = append(history, batchFromFixture(t, h))
	}
	sub := batchFromFixture(t, fixture["subsequentBatch"])

	full, err := document.ReplayFromSnapshot(emptyReplayRoot(),
		append(append([]*document.Batch{}, history...), sub))
	if err != nil {
		t.Fatalf("full-history replay: %v", err)
	}
	mid, err := document.ReplayFromSnapshot(emptyReplayRoot(), history)
	if err != nil {
		t.Fatalf("history-only replay (the compaction root): %v", err)
	}

	pkg, werr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           mid.State(),
		RetainedBatches: []*document.Batch{sub},
	})
	if werr != nil {
		t.Fatalf("BuildPackage over the compacted state: %v", werr)
	}
	view, lerr := persist.Load(pkg)
	if lerr != nil {
		t.Fatalf("Load of the compacted package: %v", lerr)
	}

	wantState, gotState := full.State(), view.Projection.State()
	if !reflect.DeepEqual(gotState, wantState) {
		t.Fatalf("V6: snapshot+log projection differs from full-history replay\n got %+v\nwant %+v",
			gotState, wantState)
	}
	// Guard against the equality passing vacuously on empty state.
	if len(wantState.Entities) == 0 || len(wantState.Tombstones) == 0 ||
		len(wantState.WinnerPositions) == 0 || len(wantState.BatchPositions) == 0 {
		t.Fatalf("V6 equality must be asserted over non-trivial state: %d entities, %d tombstones, %d winner positions, %d referenced batches",
			len(wantState.Entities), len(wantState.Tombstones),
			len(wantState.WinnerPositions), len(wantState.BatchPositions))
	}
}

// checkV7 executes BOTH canonical orderings of the concurrent
// create-versus-modify vector, for real, and pins each outcome.
//
// The fixture carries one create and two alternative modify batches. In
// both, actor Y does NOT observe X's create (X is absent from Y's
// frontier), which is the V7 setup; only Y's causal past size moves
// relative to the create's 109. The two orderings are therefore two separate
// delivered SETS — one actor stream cannot hold Y ranges [55,55] and
// [120,120] without a gapped-range framing reject.
//
// Each set is delivered in both sequential orders, through separate
// Deliver calls rather than a reordered slice in one call, because F3's
// claim is about delivery and call granularity, not argument order.
func checkV7(t *testing.T, assert map[string]any, fixture map[string]any) {
	t.Helper()
	oo, _ := assert["orderingOutcomes"].(map[string]any)
	for _, want := range []string{"modify-canonically-precedes", "modify-canonically-follows"} {
		if _, ok := oo[want]; !ok {
			t.Fatalf("oracle must record the %s outcome", want)
		}
	}
	const (
		wallID     = "019534c7-0000-7000-8000-000000000002"
		actorXText = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
		actorYText = "01948f80-0000-7000-8000-00000000bb00"
		createBase = 110
	)

	for _, tc := range []struct {
		ordering    string
		modifyKey   string
		modifyBase  int64
		wantOutcome string
		wantThick   float64
		wantStale   bool
	}{
		{"modify-canonically-precedes", "modifyBatch", 55, "discard-stale", 120, true},
		{"modify-canonically-follows", "modifyBatchFollowing", 120, "apply", 150, false},
	} {
		tc := tc
		t.Run(tc.ordering, func(t *testing.T) {
			if got, _ := oo[tc.ordering].(string); got != tc.wantOutcome {
				t.Fatalf("oracle pins %s=%q, this check proves %q", tc.ordering, got, tc.wantOutcome)
			}
			if _, ok := fixture[tc.modifyKey].(map[string]any); !ok {
				t.Fatalf("fixture lacks %s: both orderings must be materialized, not merely described",
					tc.modifyKey)
			}

			// "Y, not observing X's create" IS the V7 setup. Assert it from
			// the fixture bytes for BOTH alternatives before delivering:
			// giving the following modify a frontier naming X would preserve
			// every outcome asserted below while silently turning this into
			// a causally-ordered case — the V2 vector, not V7.
			requireNotObserving(t,
				batchFromFixture(t, fixture["createBatch"]),
				batchFromFixture(t, fixture[tc.modifyKey]),
				"createBatch", tc.modifyKey)

			// Both actors carry compacted prior history, which is what makes
			// bases 110, 55 and 120 legal at all: a watermark W implies a
			// reservation covering exactly [1..W] (B3 forbids in-stream
			// gaps). Neither batch's own frontier names the other actor, so
			// the two stay CONCURRENT — the V7 setup — and only Y's key
			// moves relative to the create's K=(110,X).
			seed := func() document.State {
				st := entitySeededState(t, "0001")
				st.Frontier[mustUUIDText(t, actorXText)] = createBase - 1
				st.Frontier[mustUUIDText(t, actorYText)] = tc.modifyBase - 1
				return st
			}

			states := make([]document.State, 0, 2)
			for _, order := range [][]string{{"createBatch", tc.modifyKey}, {tc.modifyKey, "createBatch"}} {
				proj, perr := document.FromSnapshot(seed(), nil)
				if perr != nil {
					t.Fatal(perr)
				}
				for _, key := range order {
					// A batch delivered before its concurrent partner may be
					// refused or discarded on the way through; F3 makes
					// holding and call granularity implementation strategy,
					// so only the projection after the WHOLE set is
					// delivered carries semantics.
					if derr := proj.Deliver(batchFromFixture(t, fixture[key])); derr != nil {
						t.Fatalf("arrival order %v: delivering %s failed: %v", order, key, derr)
					}
				}

				// I5: the stale-discard verdict is read from the FINAL
				// projection. Intermediate diagnostics are cleared on each
				// canonical re-replay, so accumulating them across calls
				// would report an arrival-order artifact rather than the
				// outcome of the delivered set.
				staleSeen := false
				for _, d := range proj.Diagnostics() {
					if d.Code == document.DiagStaleBatchDiscard {
						staleSeen = true
					}
				}
				if staleSeen != tc.wantStale {
					t.Fatalf("arrival order %v: stale-discard diagnostic present=%v, want %v (I5)",
						order, staleSeen, tc.wantStale)
				}

				st := proj.State()
				wall, ok := entityByID(st, wallID)
				if !ok {
					t.Fatalf("arrival order %v: the create must stand regardless of ordering", order)
				}
				if got := fieldOf(wall, "thicknessMm"); got != tc.wantThick {
					t.Fatalf("arrival order %v: thicknessMm=%v want %v", order, got, tc.wantThick)
				}
				states = append(states, st)
			}
			if !reflect.DeepEqual(states[0], states[1]) {
				t.Fatal("V7/F3: the outcome must be a function of the delivered SET, not of arrival order")
			}
		})
	}
}

// The two helpers below assert a vector's causal PRECONDITION from the
// fixture bytes before anything is delivered. They exist because every
// downstream assertion in V2 and V7 survives the precondition being broken:
// strip A-earlier's actor from B-later's frontier and V2's holding subtest
// still passes (via stale discard), its order-equality still passes, and its
// witness still observes the same value — a concurrent V7 case silently
// masquerading as V2.
//
// Each helper is mutation-checked: breaking the precondition in the fixture
// bytes must fail here BEFORE any subtest runs. The probes and their exact
// failure output are recorded in the issue 0028 ED-0027 handoff rather than
// under docs/evidence/, which this continuation is scoped out of.
//
// requireCausalEdge asserts that later observes earlier through earlier's
// END counter, which is what makes the pair causally ordered rather than
// concurrent (FS §4.4 F1/F2, B5: a frontier only ever names batch ends).
func requireCausalEdge(t *testing.T, earlier, later *document.Batch, earlierKey, laterKey string) {
	t.Helper()
	if earlier.ActorID == later.ActorID {
		t.Fatalf("%s and %s must come from DIFFERENT actors; a single actor's writes are ordered by counter, not by frontier",
			earlierKey, laterKey)
	}
	seen, ok := later.Frontier[earlier.ActorID]
	if !ok {
		t.Fatalf("%s's frontier does not name %s's actor %s: the two batches are CONCURRENT, so this fixture is not the causally-ordered vector it claims to be",
			laterKey, earlierKey, earlier.ActorID.String())
	}
	if want := earlier.EndCounter(); seen != want {
		t.Fatalf("%s observes %s's actor through counter %d, but %s ends at %d: the causal edge under test must name the batch END exactly (B5)",
			laterKey, earlierKey, seen, earlierKey, want)
	}
	if n, ok := earlier.Frontier[later.ActorID]; ok && n > 0 {
		t.Fatalf("%s observes %s's actor through %d: the edge must run in ONE direction, or the pair is not ordered as the vector claims",
			earlierKey, laterKey, n)
	}
}

// requireNotObserving asserts the converse precondition: the two batches are
// concurrent because neither names the other's actor. FS §4.7 V3/V7 both
// state this as setup, and without it a concurrency vector silently becomes
// a causally-ordered one while every downstream assertion still passes.
func requireNotObserving(t *testing.T, a, b *document.Batch, aKey, bKey string) {
	t.Helper()
	if a.ActorID == b.ActorID {
		t.Fatalf("%s and %s must come from DIFFERENT actors to be concurrent", aKey, bKey)
	}
	if n, ok := b.Frontier[a.ActorID]; ok && n > 0 {
		t.Fatalf("%s's frontier observes %s's actor %s through counter %d: the vector requires that it does NOT observe it, so this is a causally-ordered case, not a concurrent one",
			bKey, aKey, a.ActorID.String(), n)
	}
	if n, ok := a.Frontier[b.ActorID]; ok && n > 0 {
		t.Fatalf("%s's frontier observes %s's actor %s through counter %d: the vector requires mutual non-observation",
			aKey, bKey, b.ActorID.String(), n)
	}
}

func mustUUIDText(t *testing.T, s string) document.UUID {
	t.Helper()
	u, err := document.ParseUUID(s)
	if err != nil {
		t.Fatalf("uuid %q: %v", s, err)
	}
	return u
}

func entityByID(state document.State, id string) (document.Entity, bool) {
	for _, e := range state.Entities {
		if e.EntityID().String() == id {
			return e, true
		}
	}
	return nil, false
}

func stringFieldOf(t *testing.T, e document.Entity, field string) string {
	t.Helper()
	raw, err := document.EncodeEntity(e)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if uerr := json.Unmarshal(raw, &m); uerr != nil {
		t.Fatal(uerr)
	}
	s, _ := m[field].(string)
	return s
}

func findEntityByType(t *testing.T, proj *document.Projection, typ string) document.Entity {
	t.Helper()
	for _, e := range proj.State().Entities {
		if string(e.Kind()) == typ {
			return e
		}
	}
	t.Fatalf("no %s entity in projection", typ)
	return nil
}
