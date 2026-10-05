package persist_test

import (
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// snapshot_wire_test.go covers FORMAT-SPEC §5.1's snapshot encoding rules:
// the exclusivity of the two member families, ED-0038's legacy reading
// convention and its fail-closed limits, and the round trip of a position
// whose operation index is not zero.

// completeEmptyNewSnapshot is §7.5's brand-new document in the
// post-ED-0037 member set — the shape the current writer emits, and the
// control that makes the required-member cases discriminating rather than
// merely red.
const completeEmptyNewSnapshot = `{"entities":{},"frontier":{},"batchPositions":{},` +
	`"winnerPositions":{},"createdPositions":{},"tombstones":{},` +
	`"batchMetadata":{"recentBatchIds":[]}}`

const (
	wireActorA = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	wireActorB = "01948f80-0000-7000-8000-00000000bb00"
	wireLevel  = "019534c7-0000-7000-8000-000000000001"
	wireGone   = "01948f55-6b18-7d3a-a153-1ee2f33cc402"
)

func parseSnapshotText(t *testing.T, body string) (document.State, *persist.Error) {
	t.Helper()
	st, _, err := persist.ParseSnapshotDocument([]byte(body))
	return st, err
}

// TestSnapshotEncodingsAreExclusive pins §5.1: "The two encodings are
// EXCLUSIVE. A snapshot carrying members of both — any position-bearing
// member alongside any pre-ED-0037 member — is neither a conforming
// snapshot nor a legacy one, and REJECTS: it is ambiguous required data
// ... and no reader may convert it partly."
func TestSnapshotEncodingsAreExclusive(t *testing.T) {
	for _, tc := range []struct {
		name string
		body string
	}{
		{
			// Both families present and BOTH EMPTY. Selection is by member
			// PRESENCE, never by entry count: an empty object is still a
			// present member, and a len(map) > 0 test would read this as
			// carrying neither family. That is ED-0041's third instance —
			// an empty collection satisfying a per-entry test vacuously —
			// which this issue produced in its previous round.
			name: "empty objects of both families still collide",
			body: `{"entities":{},"frontier":{},"winnerPositions":{},"createdKeys":{}}`,
		},
		{
			name: "populated new member beside a legacy member",
			body: `{"entities":{},"frontier":{},` +
				`"batchPositions":{"0000000000000000001-` + wireActorA + `":{}},` +
				`"winnerPositions":{},"createdPositions":{},"tombstones":{},` +
				`"batchMetadata":{"recentBatchIds":[]},"winnerKeys":{}}`,
		},
		{
			name: "one tombstone carrying both inner shapes",
			body: `{"entities":{},"frontier":{},"tombstones":{"` + wireGone + `":{` +
				`"deletingKey":{"operationCounter":5,"actorId":"` + wireActorA + `"},` +
				`"deletingPosition":{"batch":"0000000000000000005-` + wireActorA + `","operationIndex":0}}}}`,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := parseSnapshotText(t, tc.body); err == nil {
				t.Fatal("a snapshot carrying members of both families is ambiguous required data and must reject")
			}
		})
	}

	t.Run("control: a single family parses", func(t *testing.T) {
		// The new family requires its COMPLETE member set (§5.1: a snapshot
		// contains ALL of S1-S5); the legacy family does not, because §5.1
		// blesses the empty legacy snapshot in terms.
		if _, err := parseSnapshotText(t, completeEmptyNewSnapshot); err != nil {
			t.Fatalf("the complete new family must parse: %v", err)
		}
		if _, err := parseSnapshotText(t, `{"entities":{},"frontier":{},"winnerKeys":{}}`); err != nil {
			t.Fatalf("the legacy family alone must parse: %v", err)
		}
	})
}

// TestLegacySnapshotReadingConvention pins ED-0038's ruling and the limits
// §5.1 places on it.
func TestLegacySnapshotReadingConvention(t *testing.T) {
	t.Run("single actor reads under the convention", func(t *testing.T) {
		// "each retained winner is treated as its own one-operation batch,
		// giving causalPastSize = operationCounter − 1,
		// counterBase = operationCounter, and operationIndex = 0."
		body := `{"entities":{},"frontier":{"` + wireActorA + `":9},` +
			`"winnerKeys":{"` + wireLevel + `.name":{"operationCounter":7,"actorId":"` + wireActorA + `"}}}`
		st, err := parseSnapshotText(t, body)
		if err != nil {
			t.Fatalf("a single-actor legacy snapshot must read: %v", err)
		}
		w, ok := st.WinnerPositions[document.NewFieldKey(mustUUIDText(t, wireLevel), "name")]
		if !ok {
			t.Fatal("the winner must be converted to a position")
		}
		ref := document.BatchOf(w)
		if ref.Base != 7 || document.OperationIndexOf(w) != 0 {
			t.Fatalf("the convention gives counterBase = operationCounter and index 0, got %+v", w)
		}
		fr, ok := st.BatchPositions[ref]
		if !ok {
			t.Fatal("the synthesized batch must appear in the shared table")
		}
		// causalPastSize = 7 - 1 = 6, carried as the frontier whose sum is 6
		// rather than as a scalar: §5.1 S3 stores the frontier, and a sum
		// has no inverse.
		if got := fr[mustUUIDText(t, wireActorA)]; got != 6 {
			t.Fatalf("causalPastSize must be operationCounter-1 = 6, got frontier counter %d", got)
		}
	})

	t.Run("counter 1 synthesizes an empty frontier, not a zero entry", func(t *testing.T) {
		// F1: an absent actor is nothing observed. A zero-valued entry
		// would be a second spelling of the same fact.
		body := `{"entities":{},"frontier":{"` + wireActorA + `":3},` +
			`"winnerKeys":{"` + wireLevel + `.name":{"operationCounter":1,"actorId":"` + wireActorA + `"}}}`
		st, err := parseSnapshotText(t, body)
		if err != nil {
			t.Fatalf("parse: %v", err)
		}
		// The loop below is a per-entry test, so it says nothing at all
		// over an empty table. Requiring the entry to EXIST first is what
		// makes the assertion an instrument rather than a formality — the
		// same empty-collection shape this issue produced as ED-0041's
		// third instance.
		ref := document.NewBatchRef(1, mustUUIDText(t, wireActorA))
		fr, ok := st.BatchPositions[ref]
		if !ok {
			t.Fatalf("the convention must synthesize a table entry for counterBase 1, got %v", st.BatchPositions)
		}
		if len(fr) != 0 {
			t.Fatalf("counterBase 1 has no prior operation to report, got %v", fr)
		}
	})

	t.Run("two actors in the union fail closed", func(t *testing.T) {
		// §5.1/ED-0038: "With two or more actors in that union the winners
		// cannot be placed at all ... otherwise rejected." Any convention
		// would invent cross-actor causal relationships that never existed.
		body := `{"entities":{},"frontier":{"` + wireActorA + `":9},` +
			`"winnerKeys":{"` + wireLevel + `.name":{"operationCounter":7,"actorId":"` + wireActorB + `"}}}`
		_, err := parseSnapshotText(t, body)
		if err == nil {
			t.Fatal("a legacy snapshot spanning two actors must fail closed")
		}
		if !strings.Contains(err.Error(), "fail-closed") {
			t.Fatalf("the diagnostic must name the fail-closed posture, got %v", err)
		}
	})

	t.Run("the actor universe spans the KEYS, not only the frontier", func(t *testing.T) {
		// ED-0040 queue item 9, closed in 3cca9ee: a key actor need not
		// appear in the frontier, so a frontier-only predicate would route
		// this two-actor snapshot into the single-actor convention.
		body := `{"entities":{},"frontier":{},` +
			`"winnerKeys":{"` + wireLevel + `.name":{"operationCounter":7,"actorId":"` + wireActorA + `"},` +
			`"` + wireLevel + `.elevationMm":{"operationCounter":8,"actorId":"` + wireActorB + `"}}}`
		if _, err := parseSnapshotText(t, body); err == nil {
			t.Fatal("two key actors and an EMPTY frontier is still a two-actor universe")
		}
	})

	t.Run("an empty legacy snapshot is the empty projection", func(t *testing.T) {
		// §7.5's brand-new document: nothing to place, so no convention
		// applies. This is also what keeps the committed
		// golden/snapshots/snapshot-empty-projection.json fixture valid.
		st, err := parseSnapshotText(t, `{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`)
		if err != nil {
			t.Fatalf("the empty legacy snapshot must read: %v", err)
		}
		if len(st.Entities) != 0 || len(st.WinnerPositions) != 0 || len(st.BatchPositions) != 0 {
			t.Fatalf("it must read as the empty projection, got %+v", st)
		}
	})

	t.Run("a legacy tombstone fails closed in the parse-only path", func(t *testing.T) {
		// §5.1: the pre-ED-0037 shape records a deletingKey but, creation
		// metadata being live-entity-only, no creation key for a deleted
		// id, so I5's live interval is reconstructable only from the
		// retained creating batch. ParseSnapshotDocument holds no log, so
		// it must refuse rather than guess: "guessing the creation side
		// would apply a late batch positioned before the true creation
		// where full replay discards it whole — a silently wrong
		// projection, which is worse than a refusal."
		body := `{"entities":{},"frontier":{"` + wireActorA + `":9},` +
			`"tombstones":{"` + wireGone + `":{"deletingKey":{"operationCounter":5,"actorId":"` + wireActorA + `"}}}}`
		if _, err := parseSnapshotText(t, body); err == nil {
			t.Fatal("a legacy tombstone with no retained creating batch must fail closed")
		}
	})
}

// TestSnapshotRoundTripsANonZeroOperationIndex is the round trip M16
// requires. A zero-only round trip would pass even if the writer or the
// reader dropped the index entirely, which is exactly the kind of test that
// looks like evidence and exercises nothing.
func TestSnapshotRoundTripsANonZeroOperationIndex(t *testing.T) {
	actor := mustUUIDText(t, wireActorA)
	lvl := mustUUIDText(t, wireLevel)

	// One batch with three operations, so the last create's position
	// carries operationIndex 2.
	proj := document.New()
	ops := []*document.Operation{
		mustCreateLevelOp(t, "01948f5b-0000-7000-8000-000000000001", "019534c7-0000-7000-8000-00000000000a"),
		mustCreateLevelOp(t, "01948f5b-0000-7000-8000-000000000002", "019534c7-0000-7000-8000-00000000000b"),
		mustCreateLevelOp(t, "01948f5b-0000-7000-8000-000000000003", wireLevel),
	}
	b, err := document.NewBatch(mustUUIDText(t, "01948f5b-c4e4-7e42-b996-f552a18df77f"), actor, 1, nil, ops...)
	if err != nil {
		t.Fatalf("NewBatch: %v", err)
	}
	if err := proj.Deliver(b); err != nil {
		t.Fatalf("Deliver: %v", err)
	}
	before := proj.State()
	if got := document.OperationIndexOf(before.CreatedPositions[lvl]); got != 2 {
		t.Fatalf("precondition: the third operation must carry index 2, got %d", got)
	}

	pkg, werr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           before,
		RetainedBatches: []*document.Batch{},
	})
	if werr != nil {
		t.Fatalf("BuildPackage: %v", werr)
	}
	view, lerr := persist.Load(pkg)
	if lerr != nil {
		t.Fatalf("Load: %v", lerr)
	}
	after := view.Projection.State()
	if got := document.OperationIndexOf(after.CreatedPositions[lvl]); got != 2 {
		t.Fatalf("operationIndex must survive the round trip, got %d", got)
	}
	if before.CreatedPositions[lvl] != after.CreatedPositions[lvl] {
		t.Fatalf("creation position changed across the round trip: %+v -> %+v",
			before.CreatedPositions[lvl], after.CreatedPositions[lvl])
	}
	// The shared table must come back exactly, because causalPastSize is
	// the sum of what it holds.
	if len(after.BatchPositions) != len(before.BatchPositions) {
		t.Fatalf("batchPositions size changed: %d -> %d", len(before.BatchPositions), len(after.BatchPositions))
	}
}

func mustCreateLevelOp(t *testing.T, opID, entityID string) *document.Operation {
	t.Helper()
	raw := []byte(`{"id":"` + entityID + `","type":"level","name":"L","elevationMm":0,` +
		`"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}`)
	op, err := document.NewCreateOp(mustUUIDText(t, opID), raw)
	if err != nil {
		t.Fatalf("NewCreateOp: %v", err)
	}
	return op
}
