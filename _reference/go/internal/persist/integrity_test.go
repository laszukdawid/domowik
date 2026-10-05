package persist

import (
	"testing"

	"domowik.dev/domowik/internal/document"
)

// integrity_test.go: the persistence half of the eleven-defect wave r0011
// found against the committed position implementation, adopted as tracked
// regression tests (ED-0054 amendment (a)). Every case here was RED against
// the code as first submitted.
//
// The required-member case is a RECURRENCE, not a fresh bug: accepting `{}`
// as a silently empty projection is ED-0041's third instance — the
// empty-map vacuity found during the compatibility wave — reappearing in
// code written after that record existed. The remedy is therefore a
// required-member assertion the parser structurally cannot pass without,
// not a comment. The complete-empty-shape control is what makes the nine
// rejection cases discriminating rather than merely red.

func TestNewSnapshotRequiresTheCompleteS1S5MemberSet(t *testing.T) {
	complete := `{"entities":{},"frontier":{},"batchPositions":{},"winnerPositions":{},"createdPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`
	if _, _, err := ParseSnapshotDocument([]byte(complete)); err != nil {
		t.Fatalf("control: complete empty new snapshot must parse: %v", err)
	}
	for _, tc := range []struct {
		name string
		body string
	}{
		{"empty object", `{}`},
		{"only one new-family member", `{"winnerPositions":{}}`},
		{"missing entities", `{"frontier":{},"batchPositions":{},"winnerPositions":{},"createdPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing frontier", `{"entities":{},"batchPositions":{},"winnerPositions":{},"createdPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing batchPositions", `{"entities":{},"frontier":{},"winnerPositions":{},"createdPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing winnerPositions", `{"entities":{},"frontier":{},"batchPositions":{},"createdPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing createdPositions", `{"entities":{},"frontier":{},"batchPositions":{},"winnerPositions":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing tombstones", `{"entities":{},"frontier":{},"batchPositions":{},"winnerPositions":{},"createdPositions":{},"batchMetadata":{"recentBatchIds":[]}}`},
		{"missing batchMetadata", `{"entities":{},"frontier":{},"batchPositions":{},"winnerPositions":{},"createdPositions":{},"tombstones":{}}`},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, _, err := ParseSnapshotDocument([]byte(tc.body)); err == nil {
				t.Fatal("FS 5.1 says a snapshot contains all S1-S5 members; absent required data must reject")
			}
		})
	}
}

func TestInspectReportsMixedSnapshotFamilies(t *testing.T) {
	const actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	const entity = "019534c7-0000-7000-8000-000000000001"
	body := []byte(`{"entities":{},"frontier":{},"winnerPositions":{},"winnerKeys":{"` + entity + `.name":{"operationCounter":1,"actorId":"` + actor + `"}},"tombstones":{}}`)
	s, err := decodeSnapshotSummary(body)
	if err != nil {
		t.Fatalf("structural summary: %v", err)
	}
	if !s.Legacy {
		t.Fatal("Inspect silently hid the present legacy family when an empty new-family member was also present")
	}
}

func TestInspectReportsMixedTombstoneShape(t *testing.T) {
	const actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	const entity = "019534c7-0000-7000-8000-000000000001"
	body := []byte(`{"entities":{},"frontier":{},"tombstones":{"` + entity + `":{"deletingPosition":{"batch":"0000000000000000001-` + actor + `","operationIndex":0},"deletingKey":{"operationCounter":1,"actorId":"` + actor + `"}}}}`)
	s, err := decodeSnapshotSummary(body)
	if err != nil {
		t.Fatalf("structural summary: %v", err)
	}
	if !s.Legacy {
		t.Fatal("Inspect silently hid a deletingKey when deletingPosition was also present")
	}
}

// TestLegacyTombstoneUsesTheCreateWhoseIntervalItCloses is r0011's blocker 2.
//
// A retained single-actor history that creates, deletes, RESTORES and
// deletes the same id carries two creates for it. The final tombstone closes
// the interval the SECOND one opened. Returning the first syntactic match is
// not a conservative approximation of that — it is a different interval, and
// I5 would then place every late batch between the two creates on the wrong
// side of branch 1, applying writes full replay discards whole.
//
// §5.1 permits a refusal and forbids a guess, so a recovery that cannot
// identify the closing create must fail closed. This accepts either outcome
// and rejects only the wrong answer.
func TestLegacyTombstoneUsesTheCreateWhoseIntervalItCloses(t *testing.T) {
	const (
		actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
		ent   = "019534c7-0000-7000-8000-000000000001"
	)
	a := mustUUIDIn(t, actor)
	e := mustUUIDIn(t, ent)

	// Four one-operation batches: create, delete, restore-create, delete.
	batch := func(n uint64, base int64, op *document.Operation) *document.Batch {
		t.Helper()
		fr := map[document.UUID]int64{}
		if base > 1 {
			fr[a] = base - 1
		}
		b, err := document.NewBatch(mustUUIDIn(t, batchUUID(n)), a, base, fr, op)
		if err != nil {
			t.Fatalf("NewBatch: %v", err)
		}
		return b
	}
	lvl := func(id uint64) []byte {
		return []byte(`{"id":"` + ent + `","type":"level","name":"L` + itoaForTest(id) + `",` +
			`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}`)
	}
	mkCreate := func(n uint64, raw []byte) *document.Operation {
		op, err := document.NewCreateOp(mustUUIDIn(t, opUUID(n)), raw)
		if err != nil {
			t.Fatalf("NewCreateOp: %v", err)
		}
		return op
	}
	mkDelete := func(n uint64) *document.Operation {
		op, err := document.NewDeleteOp(mustUUIDIn(t, opUUID(n)), e)
		if err != nil {
			t.Fatalf("NewDeleteOp: %v", err)
		}
		return op
	}
	retained := []*document.Batch{
		batch(1, 1, mkCreate(1, lvl(1))),
		batch(2, 2, mkDelete(2)),
		batch(3, 3, mkCreate(3, lvl(3))), // the restore: base 3
		batch(4, 4, mkDelete(4)),
	}

	// A legacy snapshot whose deletingKey is the FINAL delete at counter 4.
	legacy := &LegacySnapshot{
		Frontier:    map[document.UUID]int64{a: 4},
		WinnerKeys:  map[document.FieldKey]legacyKey{},
		CreatedKeys: map[document.UUID]legacyKey{},
		Tombstones:  map[document.UUID]legacyKey{e: {Counter: 4, Actor: a}},
	}
	st, err := ConvertLegacySnapshot(legacy, retained)
	if err != nil {
		// Fail-closed is an acceptable outcome; a wrong answer is not.
		t.Logf("conversion failed closed, which §5.1 permits: %v", err)
		return
	}
	got, ok := st.CreatedPositions[e]
	if !ok {
		t.Fatal("a converted tombstone must carry the creation position that closes its interval")
	}
	if base := document.BatchOf(got).Base; base != 3 {
		t.Fatalf("the final tombstone closes the RESTORED interval opened at counterBase 3, got %d "+
			"(counterBase 1 is the original create, which a first-match scan returns)", base)
	}
}

// mustUUIDIn parses a UUID inside package persist; the persist_test helper
// of the same purpose is not visible from here.
// TestLegacyCreationKeyMayNotOverwriteRetainedEvidence is the sibling of
// blocker 2. The tombstone-recovery pass derives a creation position from
// RETAINED bytes; the CreatedKeys pass then applies ED-0038's CONVENTION to
// the same id. Letting the second write over the first makes LOOP ORDER the
// arbiter between evidence and convention, which is the opposite of the
// ordering this conversion states — and the pre-ED-0037 shape recorded
// createdKeys for LIVE entities only, so a createdKeys entry for a
// tombstoned id is already a malformed or hostile input rather than a
// normal one.
func TestLegacyCreationKeyMayNotOverwriteRetainedEvidence(t *testing.T) {
	const (
		actor = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
		ent   = "019534c7-0000-7000-8000-000000000001"
	)
	a := mustUUIDIn(t, actor)
	e := mustUUIDIn(t, ent)

	raw := []byte(`{"id":"` + ent + `","type":"level","name":"L",` +
		`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}`)
	create, err := document.NewCreateOp(mustUUIDIn(t, opUUID(1)), raw)
	if err != nil {
		t.Fatalf("NewCreateOp: %v", err)
	}
	del, err := document.NewDeleteOp(mustUUIDIn(t, opUUID(2)), e)
	if err != nil {
		t.Fatalf("NewDeleteOp: %v", err)
	}
	b1, err := document.NewBatch(mustUUIDIn(t, batchUUID(1)), a, 1, nil, create)
	if err != nil {
		t.Fatalf("NewBatch: %v", err)
	}
	b2, err := document.NewBatch(mustUUIDIn(t, batchUUID(2)), a, 2, map[document.UUID]int64{a: 1}, del)
	if err != nil {
		t.Fatalf("NewBatch: %v", err)
	}
	retained := []*document.Batch{b1, b2}

	// The retained log places the create at counterBase 1. The snapshot's
	// createdKeys claims counter 9, i.e. counterBase 9 under the
	// convention — a contradiction, not a refinement.
	legacy := &LegacySnapshot{
		Frontier:    map[document.UUID]int64{a: 9},
		WinnerKeys:  map[document.FieldKey]legacyKey{},
		CreatedKeys: map[document.UUID]legacyKey{e: {Counter: 9, Actor: a}},
		Tombstones:  map[document.UUID]legacyKey{e: {Counter: 2, Actor: a}},
	}
	if _, cerr := ConvertLegacySnapshot(legacy, retained); cerr == nil {
		t.Fatal("a creation key contradicting the retained create must fail closed, " +
			"not be resolved by which loop runs last")
	}

	t.Run("control: an agreeing creation key converts", func(t *testing.T) {
		agreeing := &LegacySnapshot{
			Frontier:    map[document.UUID]int64{a: 9},
			WinnerKeys:  map[document.FieldKey]legacyKey{},
			CreatedKeys: map[document.UUID]legacyKey{},
			Tombstones:  map[document.UUID]legacyKey{e: {Counter: 2, Actor: a}},
		}
		st, cerr := ConvertLegacySnapshot(agreeing, retained)
		if cerr != nil {
			t.Fatalf("a legacy tombstone whose create is retained must convert: %v", cerr)
		}
		if got := document.BatchOf(st.CreatedPositions[e]).Base; got != 1 {
			t.Fatalf("the retained create sits at counterBase 1, got %d", got)
		}
	})
}

func mustUUIDIn(t *testing.T, s string) document.UUID {
	t.Helper()
	u, err := document.ParseUUID(s)
	if err != nil {
		t.Fatalf("parse %q: %v", s, err)
	}
	return u
}

func itoaForTest(n uint64) string {
	if n == 0 {
		return "0"
	}
	var b []byte
	for n > 0 {
		b = append([]byte{byte('0' + n%10)}, b...)
		n /= 10
	}
	return string(b)
}

func batchUUID(n uint64) string { return "01948f5b-0000-7000-8000-" + hex12ForTest(n) }
func opUUID(n uint64) string    { return "01948f5c-0000-7000-8000-" + hex12ForTest(n) }

func hex12ForTest(n uint64) string {
	const digits = "0123456789abcdef"
	out := make([]byte, 12)
	for i := 11; i >= 0; i-- {
		out[i] = digits[n&0xf]
		n >>= 4
	}
	return string(out)
}
