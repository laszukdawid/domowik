package persist_test

import (
	"bytes"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// inspect_test.go: proofs for the ED-0023 parse-only inspection seam.
// Four proof classes are covered: differential non-execution (structurally
// framed but application-invalid packages inspect cleanly while Load
// rejects), corpus classification parity through the framing stage,
// inherited §9.3 limit enforcement, and the mutation probe property —
// every differential case flips to failure if operation construction,
// replay, or application ever enters the seam.

const (
	inspectActor  = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"
	inspectBatch  = "01948f5b-c4e4-7e42-b996-f552a18df77f"
	inspectOp     = "01948f5d-6e18-7c01-8f0d-77f974bd8036"
	inspectEntity = "01948f55-6b18-7d3a-a153-1ee2f33cc402"
)

func inspectBatchDoc(t *testing.T, kind, fieldsJSON string) []byte {
	t.Helper()
	op := fmt.Sprintf(`{"operationId":%q,"kind":%q,"entityId":%q`, inspectOp, kind, inspectEntity)
	if fieldsJSON != "" {
		op += `,"fields":` + fieldsJSON
	}
	op += `}`
	return []byte(fmt.Sprintf(
		`{"batchId":%q,"actorId":%q,"counterBase":42,"frontier":{%q:41},"operations":[%s]}`,
		inspectBatch, inspectActor, inspectActor, op))
}

func inspectEntryName(base int64) string {
	return fmt.Sprintf("ops/%019d-%s.json", base, inspectActor)
}

func inspectFrontierSnapshot(counter int64) []byte {
	return []byte(fmt.Sprintf(
		`{"entities":{},"frontier":{%q:%d},"winnerKeys":{},"tombstones":{}}`,
		inspectActor, counter))
}

// TestInspectMatchesLoadOnGoldenPackages pins parity on valid inputs:
// the seam sees exactly the manifest identity and batch framing Load
// delivers, minus everything executed.
func TestInspectMatchesLoadOnGoldenPackages(t *testing.T) {
	for _, rel := range []string{"golden/packages/thin-valid.domo", "golden/packages/portable-valid.domo"} {
		raw := corpusFile(t, rel)
		view, lerr := persist.Load(raw)
		if lerr != nil {
			t.Fatalf("%s: load: %v", rel, lerr)
		}
		insp, ierr := persist.Inspect(raw)
		if ierr != nil {
			t.Fatalf("%s: inspect: %v", rel, ierr)
		}
		if insp.FormatVersion != view.FormatVersion || insp.Profile != view.Profile {
			t.Fatalf("%s: manifest identity mismatch: %+v", rel, insp)
		}
		if len(insp.Batches) != len(view.Retained) {
			t.Fatalf("%s: batch count %d != retained %d", rel, len(insp.Batches), len(view.Retained))
		}
		retainedOps := map[string]int{}
		for _, b := range view.Retained {
			retainedOps[b.BatchID.String()] = len(b.Operations)
		}
		for _, s := range insp.Batches {
			n, ok := retainedOps[s.BatchID]
			if !ok {
				t.Fatalf("%s: inspected unknown batch %s", rel, s.BatchID)
			}
			if n != len(s.Operations) {
				t.Fatalf("%s: batch %s op count %d != %d", rel, s.BatchID, len(s.Operations), n)
			}
			delete(retainedOps, s.BatchID)
		}
		if len(retainedOps) != 0 {
			t.Fatalf("%s: uninspected retained batches %v", rel, retainedOps)
		}
	}
}

// TestInspectNonExecutionMutationProbe is the ED-0023 mutation probe:
// each package below is structurally framed yet application-invalid, so
// Load must reject it. If operation construction, snapshot decoding,
// replay, or delivery ever enters inspectArchive, the Inspect-accepts
// assertions flip and the probe fails.
func TestInspectNonExecutionMutationProbe(t *testing.T) {
	cases := []struct {
		name        string
		raws        map[string][]byte
		loadRejects string // exact Load taxonomy token
	}{
		{
			name: "unknown operation kind refuses only under construction",
			raws: map[string][]byte{
				inspectEntryName(42): inspectBatchDoc(t, "teleportEntity", ""),
			},
			loadRejects: "malformed-framing",
		},
		{
			name: "invalid entity payload refuses only under construction",
			raws: map[string][]byte{
				inspectEntryName(42): inspectBatchDoc(t, "createEntity", `{"id":"`+inspectEntity+`","type":"castle"}`),
			},
			loadRejects: "",
		},
		{
			name: "retained-log gap against snapshot watermark refuses only under replay",
			raws: map[string][]byte{
				"snapshot/state.json": inspectFrontierSnapshot(5),
				inspectEntryName(42):  inspectBatchDoc(t, "deleteEntity", ""),
			},
			loadRejects: "malformed-framing",
		},
	}
	for _, tc := range cases {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			pkg, werr := persist.AssembleRawPackage(tc.raws)
			if werr != nil {
				t.Fatal(werr)
			}
			_, lerr := persist.Load(pkg)
			if lerr == nil {
				t.Fatal("precondition broken: Load must reject an application-invalid package")
			}
			if tc.loadRejects != "" {
				if c, _ := persist.CategoryOf(lerr); string(c) != tc.loadRejects {
					t.Fatalf("precondition broken: expected %s, got %s (%v)", tc.loadRejects, c, lerr)
				}
			}
			insp, ierr := persist.Inspect(pkg)
			if ierr != nil {
				t.Fatalf("seam executed or over-reached: %v", ierr)
			}
			if len(insp.Batches) != 1 || len(insp.Batches[0].Operations) != 1 {
				t.Fatalf("unexpected framing summary: %+v", insp.Batches)
			}
		})
	}
}

// knownConstructionStage records corpus fixtures whose pinned
// malformed-framing reject fires inside document batch CONSTRUCTION
// (NewBatch), a stage the seam deliberately never enters (ED-0023).
var knownConstructionStage = map[string]bool{
	"hostile/framing/empty-operations-array": true,
	"hostile/framing/inconsistent-frontier":  true,
}

// framingMaterialization maps each artifact-based hostile/framing
// fixture to the batch entries (name -> raw document) that materialize
// its pinned defect as a single package. Wrapper artifacts contribute
// their named sub-documents; the binding-mismatch fixture is named with
// the contradicting counter per its own note.
func framingMaterialization(t *testing.T, id string) map[string][]byte {
	t.Helper()
	actor := inspectActor
	named := func(base int64, raw []byte) string {
		return fmt.Sprintf("ops/%019d-%s.json", base, actor)
	}
	switch id {
	case "hostile/framing/name-content-binding-mismatch":
		raw := corpusFile(t, "hostile/framing/name-content-binding-mismatch.json")
		return map[string][]byte{named(43, raw): raw} // embedded counterBase is 44
	case "hostile/framing/torn-batch-document":
		raw := corpusFile(t, "hostile/framing/torn-batch-document.json")
		return map[string][]byte{batchNameFor(t, raw, 0): raw}
	case "hostile/framing/empty-operations-array":
		raw := corpusFile(t, "hostile/framing/empty-operations-array.json")
		return map[string][]byte{batchNameFor(t, raw, 0): raw}
	case "hostile/framing/inconsistent-frontier":
		raw := corpusFile(t, "hostile/framing/inconsistent-frontier.json")
		return map[string][]byte{batchNameFor(t, raw, 0): raw}
	case "hostile/framing/zero-frontier-member":
		// Snapshot with zero-valued frontier member - wraps as snapshot entry
		raw := corpusFile(t, "hostile/framing/zero-frontier-member.json")
		return map[string][]byte{"snapshot/state.json": raw}
	case "hostile/framing/duplicate-batch-ids":
		// Recipe-based: two standalone batch documents that share one
		// batchId while differing in counterBase, so FS 7.6 naming keeps
		// them distinct and the duplicate-id check is what fires.
		raws := map[string][]byte{}
		for _, side := range []string{"a", "b"} {
			raw := corpusFile(t, "hostile/framing/duplicate-batch-id-"+side+".json")
			var probe struct {
				CounterBase int64 `json:"counterBase"`
			}
			if json.Unmarshal(raw, &probe) != nil {
				t.Fatalf("%s: input %q unparsable", id, side)
			}
			raws[named(probe.CounterBase, raw)] = raw
		}
		if len(raws) != 2 {
			t.Fatalf("%s: the two inputs collapsed to one entry name", id)
		}
		return raws
	case "hostile/framing/overlapping-reserved-ranges",
		"hostile/framing/gapped-reserved-ranges":
		var wrapper map[string]json.RawMessage
		if err := json.Unmarshal(corpusFile(t, id+".json"), &wrapper); err != nil {
			t.Fatal(err)
		}
		raws := map[string][]byte{}
		for _, side := range []string{"a", "b"} {
			raw, ok := wrapper[side]
			if !ok {
				continue
			}
			var probe struct {
				CounterBase int64  `json:"counterBase"`
				ActorID     string `json:"actorId"`
			}
			if json.Unmarshal(raw, &probe) != nil || probe.ActorID == "" {
				t.Fatalf("%s: sub-document %q lacks identity", id, side)
			}
			raws[named(probe.CounterBase, raw)] = raw
		}
		if len(raws) != 2 {
			t.Fatalf("%s: expected a two-batch wrapper", id)
		}
		return raws
	default:
		t.Fatalf("unrecorded framing fixture materialization: %s", id)
		return nil
	}
}

// TestInspectCorpusFramingParity walks every hostile/framing fixture
// through both paths: defects visible at or before structural framing
// classify IDENTICALLY (same taxonomy token); construction-stage defects
// split exactly along the recorded set; every remaining record is either
// exercised or explicitly recorded as unmaterializable.
func TestInspectCorpusFramingParity(t *testing.T) {
	idx := expectationIndex(t)
	seenFraming := map[string]bool{}
	for id, e := range idx {
		if !strings.HasPrefix(id, "hostile/framing/") {
			continue
		}
		e := e
		id := id
		t.Run(id, func(t *testing.T) {
			defer func() { seenFraming[id] = true }()
			pkg := assembleRawPackage(t, framingMaterialization(t, id))

			want := deref(e.Category)
			_, lerr := persist.Load(pkg)
			if lerr == nil {
				t.Fatal("precondition broken: framing fixture must reject under Load")
			}
			insp, ierr := persist.Inspect(pkg)
			if knownConstructionStage[id] {
				if ierr != nil {
					t.Fatalf("construction-stage defect must not reject the seam, got %v", ierr)
				}
				if len(insp.Batches) == 0 {
					t.Fatal("construction-stage fixture must still frame its batches")
				}
				return
			}
			if ierr != nil {
				if got, _ := persist.CategoryOf(ierr); got != persist.Category(want) {
					t.Fatalf("parity: expected %s, got %s (%v)", want, got, ierr)
				}
				return
			}
			t.Fatalf("unrecorded seam acceptance: Load rejects %s (%v) but Inspect accepts", want, lerr)
		})
	}
	for _, e := range loadExpectations(t) {
		if !strings.HasPrefix(e.ID, "hostile/framing/") || seenFraming[e.ID] {
			continue
		}
		t.Errorf("framing fixture never exercised: %s", e.ID)
	}
}

func assembleRawPackage(t *testing.T, raws map[string][]byte) []byte {
	t.Helper()
	pkg, werr := persist.AssembleRawPackage(raws)
	if werr != nil {
		t.Fatal(werr)
	}
	return pkg
}

// TestInspectDuplicateBatchIDsRejectsInBothPaths is constructed-batch
// coverage for the duplicate-id check, independent of the corpus fixture
// that also exercises it: two coexisting entries carrying the SAME
// batchId under distinct, individually binding-consistent names. Both
// paths must reject malformed-framing with identical tokens.
func TestInspectDuplicateBatchIDsRejectsInBothPaths(t *testing.T) {
	batchWithID := func(batchID string, base int64) ([]byte, string) {
		raw := []byte(fmt.Sprintf(
			`{"batchId":%q,"actorId":%q,"counterBase":%d,"frontier":{},"operations":[{"operationId":"01949100-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"01949101-0000-7000-8000-000000000001"}]}`,
			batchID, inspectActor, base, base))
		return raw, inspectEntryName(base)
	}
	rawA, nameA := batchWithID("01949102-0000-7000-8000-000000000001", 42)
	rawB, nameB := batchWithID("01949102-0000-7000-8000-000000000001", 44)
	if nameA == nameB {
		t.Fatal("materialization must use distinct entry names")
	}
	pkg := assembleRawPackage(t, map[string][]byte{nameA: rawA, nameB: rawB})
	_, lerr := persist.Load(pkg)
	if lerr == nil || catOf(t, lerr) != persist.CatMalformedFraming {
		t.Fatalf("precondition: duplicate batch ids must reject malformed-framing under Load, got %v", lerr)
	}
	_, ierr := persist.Inspect(pkg)
	if ierr == nil || catOf(t, ierr) != persist.CatMalformedFraming {
		t.Fatalf("duplicate batch ids must reject malformed-framing through the seam, got %v", ierr)
	}
}

// TestInspectReservedRangeStructureRejects pins the §7.6 reserved-range
// check inside the seam: overlapping and gapped actor ranges reject with
// exactly Load's malformed-framing token — while a single batch's
// snapshot-covered implied prefix never does.
func TestInspectReservedRangeStructureRejects(t *testing.T) {
	batchAt := func(batchID string, base int64, count int) ([]byte, string) {
		var buf bytes.Buffer
		fmt.Fprintf(&buf, `{"batchId":%q,"actorId":%q,"counterBase":%d,"frontier":{},"operations":[`, batchID, inspectActor, base)
		for i := 0; i < count; i++ {
			if i > 0 {
				buf.WriteByte(',')
			}
			fmt.Fprintf(&buf, `{"operationId":"01949100-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"01949101-0000-7000-8000-000000000001"}`, base+int64(i))
		}
		buf.WriteString("]}")
		return buf.Bytes(), inspectEntryName(base)
	}
	assemble := func(defs ...[3]any) []byte {
		t.Helper()
		raws := map[string][]byte{}
		for _, d := range defs {
			raw, name := batchAt(d[0].(string), d[1].(int64), d[2].(int))
			raws[name] = raw
		}
		pkg, werr := persist.AssembleRawPackage(raws)
		if werr != nil {
			t.Fatal(werr)
		}
		return pkg
	}
	t.Run("overlapping ranges reject in both paths", func(t *testing.T) {
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000a", int64(42), 2},
			[3]any{"01949102-0000-7000-8000-00000000000b", int64(43), 1},
		)
		if _, lerr := persist.Load(pkg); lerr == nil {
			t.Fatal("precondition broken: overlap must reject under Load")
		}
		if _, ierr := persist.Inspect(pkg); ierr == nil || catOf(t, ierr) != persist.CatMalformedFraming {
			t.Fatalf("overlap must reject malformed-framing through the seam, got %v", ierr)
		}
	})
	t.Run("gapped ranges reject in both paths", func(t *testing.T) {
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000c", int64(42), 1},
			[3]any{"01949102-0000-7000-8000-00000000000d", int64(44), 1},
		)
		if _, lerr := persist.Load(pkg); lerr == nil {
			t.Fatal("precondition broken: gap must reject under Load")
		}
		if _, ierr := persist.Inspect(pkg); ierr == nil || catOf(t, ierr) != persist.CatMalformedFraming {
			t.Fatalf("gap must reject malformed-framing through the seam, got %v", ierr)
		}
	})
	t.Run("contiguous ranges inspect cleanly", func(t *testing.T) {
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000e", int64(42), 1},
			[3]any{"01949102-0000-7000-8000-00000000000f", int64(43), 1},
		)
		if _, ierr := persist.Inspect(pkg); ierr != nil {
			t.Fatalf("contiguous ranges must pass, got %v", ierr)
		}
	})
}

// TestInspectInheritsLimits proves the seam enforces the ratified §9.3
// rows through its shared prefix and framing stage with identical tokens.
func TestInspectInheritsLimits(t *testing.T) {
	t.Run("L23 snapshot metadata ceiling", func(t *testing.T) {
		label := make([]byte, 1024)
		entity := func() []byte {
			return []byte(`{"entities":{"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa":{"id":"019534c7-0000-7000-8000-000000000009","type":"label","levelId":"019534c7-0000-7000-8000-000000000001","text":"` +
				string(label) + `"}},"frontier":{},"winnerKeys":{},"tombstones":{}}`)
		}
		snap := entity()
		for uint64(len(snap)) <= persist.LimitSnapshotMetadataByte && len(label) < 24<<20 {
			label = append(label, label...)
			snap = entity()
		}
		if uint64(len(snap)) <= persist.LimitSnapshotMetadataByte {
			t.Skip("could not materialize an over-ceiling snapshot cheaply")
		}
		pkg, werr := persist.AssembleRawPackage(map[string][]byte{"snapshot/state.json": snap})
		if werr != nil {
			t.Fatal(werr)
		}
		for _, probe := range []struct {
			name string
			fn   func([]byte) error
		}{
			{"load", func(b []byte) error { _, err := persist.Load(b); return err }},
			{"inspect", func(b []byte) error { _, err := persist.Inspect(b); return err }},
		} {
			err := probe.fn(pkg)
			if err == nil || catOf(t, err) != persist.CatLimitSnapshotMeta {
				t.Fatalf("%s: expected limit-snapshot-metadata, got %v", probe.name, err)
			}
		}
	})

	t.Run("L23 operations per batch boundary", func(t *testing.T) {
		batch := func(n int) []byte {
			var buf bytes.Buffer
			buf.WriteString(`{"batchId":"` + inspectBatch + `","actorId":"` + inspectActor +
				`","counterBase":1,"frontier":{},"operations":[`)
			for i := 0; i < n; i++ {
				if i > 0 {
					buf.WriteByte(',')
				}
				fmt.Fprintf(&buf, `{"operationId":"01949100-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"01949101-0000-7000-8000-000000000001"}`, i)
			}
			buf.WriteString("]}")
			return buf.Bytes()
		}
		pkgAt, werr := persist.AssembleRawPackage(map[string][]byte{
			inspectEntryName(1): batch(persist.LimitOperationsPerBatch),
		})
		if werr != nil {
			t.Fatal(werr)
		}
		insp, ierr := persist.Inspect(pkgAt)
		if ierr != nil {
			t.Fatalf("L23: == limit must frame-check cleanly through the seam: %v", ierr)
		}
		if len(insp.Batches[0].Operations) != persist.LimitOperationsPerBatch {
			t.Fatalf("L23: boundary count mismatch: %d", len(insp.Batches[0].Operations))
		}
		pkgOver, werr := persist.AssembleRawPackage(map[string][]byte{
			inspectEntryName(1): batch(persist.LimitOperationsPerBatch + 1),
		})
		if werr != nil {
			t.Fatal(werr)
		}
		_, ierr = persist.Inspect(pkgOver)
		if ierr == nil || catOf(t, ierr) != persist.CatLimitOperationCount {
			t.Fatalf("L23: one-over must reject limit-operation-count, got %v", ierr)
		}
	})

	t.Run("L17 parser guard inside a framed batch keeps its token", func(t *testing.T) {
		ops := make([]string, persist.LimitArrayElementCount+1)
		for i := range ops {
			ops[i] = "null"
		}
		doc := []byte(`{"batchId":"` + inspectBatch + `","actorId":"` + inspectActor +
			`","counterBase":42,"operations":[` + strings.Join(ops, ",") + `]}`)
		pkg, werr := persist.AssembleRawPackage(map[string][]byte{inspectEntryName(42): doc})
		if werr != nil {
			t.Fatal(werr)
		}
		_, ierr := persist.Inspect(pkg)
		if ierr == nil || catOf(t, ierr) != persist.CatLimitArrayCount {
			t.Fatalf("expected parser-security token limit-array-count, got %v", ierr)
		}
	})

	t.Run("L6 entry count boundary", func(t *testing.T) {
		entryName := func(i int) string {
			const digits = "0123456789abcdef"
			out := make([]byte, 62)
			for j := range out {
				out[j] = digits[(uint(i)>>(4*(j%16)))&0xf] // injective in i
			}
			return fmt.Sprintf("preview/data/%02x/%s", (i*37)%256, string(out))
		}
		raws := map[string][]byte{}
		for i := 0; i < persist.LimitEntryCount-2; i++ {
			raws[entryName(i)] = []byte("p")
		}
		pkgAt, werr := persist.AssembleRawPackage(raws)
		if werr != nil {
			t.Fatal(werr)
		}
		if _, ierr := persist.Inspect(pkgAt); ierr != nil {
			t.Fatalf("L6: == 65535 entries must inspect cleanly, got %v", ierr)
		}
	})

	t.Run("structural digest arbitration parity", func(t *testing.T) {
		snap := []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`)
		wrong := sha256.Sum256([]byte("not the snapshot bytes"))
		type inv struct {
			SHA256     string `json:"sha256"`
			ByteLength int    `json:"byteLength"`
		}
		manifest, merr := json.Marshal(map[string]any{
			"formatVersion": "0.1",
			"profile":       "thin",
			"assetRoot":     nil,
			"entries": map[string]inv{
				"snapshot/state.json": {"sha256:" + hexOf(wrong[:]), len(snap)},
			},
		})
		if merr != nil {
			t.Fatal(merr)
		}
		pkg, werr := persist.AssembleRawPackage(map[string][]byte{
			"domowik.json":        manifest,
			"snapshot/state.json": snap,
		})
		if werr != nil {
			t.Fatal(werr)
		}
		for _, probe := range []struct {
			name string
			fn   func([]byte) error
		}{
			{"load", func(b []byte) error { _, err := persist.Load(b); return err }},
			{"inspect", func(b []byte) error { _, err := persist.Inspect(b); return err }},
		} {
			err := probe.fn(pkg)
			if err == nil || catOf(t, err) != persist.CatChecksumError {
				t.Fatalf("%s: expected checksum-error on structural digest mismatch, got %v", probe.name, err)
			}
		}
	})
}

// TestInspectFileParityWithInspect pins identical input handling between
// the path-based and byte-based seam entries.
func TestInspectFileParityWithInspect(t *testing.T) {
	raw := corpusFile(t, "golden/packages/thin-valid.domo")
	dir := t.TempDir()
	path := filepath.Join(dir, "thin.domo")
	if err := os.WriteFile(path, raw, 0o444); err != nil {
		t.Fatal(err)
	}
	viaBytes, err := persist.Inspect(raw)
	if err != nil {
		t.Fatal(err)
	}
	viaFile, err := persist.InspectFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if viaFile.FormatVersion != viaBytes.FormatVersion ||
		viaFile.Profile != viaBytes.Profile ||
		len(viaFile.Batches) != len(viaBytes.Batches) {
		t.Fatalf("path/bytes divergence:\n%+v\n%+v", viaFile, viaBytes)
	}
	if _, err := persist.InspectFile(filepath.Join(dir, "absent.domo")); err == nil {
		t.Fatal("missing input must fail")
	}
}

// TestInspectSnapshotStructuralMetadata pins the seam's strictly
// structural snapshot read on a package whose snapshot carries known
// coverage metadata: shapes are read verbatim (no UUID validation, no
// payload decoding), unknown members land in U2 bookkeeping, and
// recent batch ids normalize ascending.
func TestInspectSnapshotStructuralMetadata(t *testing.T) {
	snap := []byte(`{
		"entities": {"stored-entity": {"any": "payload is never touched"}},
		"frontier": {"actor-a": 5, "actor-b": 2},
		"winnerKeys": {"entity-e.field-x": {"operationCounter": 7, "actorId": "actor-b"}},
		"createdKeys": {"entity-c": {"operationCounter": 3, "actorId": "actor-a"}},
		"tombstones": {"entity-d": {"deletingKey": {"operationCounter": 9, "actorId": "actor-c"}}},
		"batchMetadata": {"recentBatchIds": ["id-2", "id-1"]},
		"sneakyUnknownMember": true
	}`)
	pkg := assembleRawPackage(t, map[string][]byte{"snapshot/state.json": snap})
	insp, ierr := persist.Inspect(pkg)
	if ierr != nil {
		t.Fatal(ierr)
	}
	ss := insp.Snapshot
	if !ss.Present || ss.EntityCount != 1 {
		t.Fatalf("unexpected presence/count: %+v", ss)
	}
	if ss.Frontier["actor-a"] != 5 || ss.Frontier["actor-b"] != 2 || len(ss.Frontier) != 2 {
		t.Fatalf("frontier mismatch: %v", ss.Frontier)
	}
	// The fixture carries the pre-ED-0037 members, so the summary reports
	// ED-0038's reading CONVENTION — each key as its own one-operation
	// batch — and marks itself legacy so no reader mistakes the positions
	// for stored data.
	if !ss.Legacy {
		t.Fatal("a snapshot carrying winnerKeys/createdKeys/deletingKey must report as legacy")
	}
	wantWinner := persist.KeyedPositionSummary{Entity: "entity-e.field-x",
		Position: persist.PositionSummary{
			Batch: "0000000000000000007-actor-b", CounterBase: 7, Actor: "actor-b"}}
	if len(ss.WinnerPositions) != 1 || ss.WinnerPositions[0] != wantWinner {
		t.Fatalf("winner positions mismatch: %v", ss.WinnerPositions)
	}
	wantCreated := persist.KeyedPositionSummary{Entity: "entity-c",
		Position: persist.PositionSummary{
			Batch: "0000000000000000003-actor-a", CounterBase: 3, Actor: "actor-a"}}
	if len(ss.CreatedPositions) != 1 || ss.CreatedPositions[0] != wantCreated {
		t.Fatalf("creation positions mismatch: %v", ss.CreatedPositions)
	}
	wantTomb := persist.KeyedPositionSummary{Entity: "entity-d",
		Position: persist.PositionSummary{
			Batch: "0000000000000000009-actor-c", CounterBase: 9, Actor: "actor-c"}}
	if len(ss.Tombstones) != 1 || ss.Tombstones[0] != wantTomb {
		t.Fatalf("tombstones mismatch: %v", ss.Tombstones)
	}
	if len(ss.RecentBatchIDs) != 2 || ss.RecentBatchIDs[0] != "id-1" || ss.RecentBatchIDs[1] != "id-2" {
		t.Fatalf("recent batch ids mismatch: %v", ss.RecentBatchIDs)
	}
	if len(ss.UnknownMembers) != 1 || ss.UnknownMembers[0] != "sneakyUnknownMember" {
		t.Fatalf("unknown member bookkeeping mismatch: %v", ss.UnknownMembers)
	}
}

// TestInspectOperationRecordsParityWithLoad pins per-operation structural
// records and end counters against Load's executed outcome on valid
// packages: identical identity, kind, payload presence, and ranges.
func TestInspectOperationRecordsParityWithLoad(t *testing.T) {
	for _, rel := range []string{"golden/packages/thin-valid.domo", "golden/packages/portable-valid.domo"} {
		raw := corpusFile(t, rel)
		view, lerr := persist.Load(raw)
		if lerr != nil {
			t.Fatalf("%s: load: %v", rel, lerr)
		}
		insp, ierr := persist.Inspect(raw)
		if ierr != nil {
			t.Fatalf("%s: inspect: %v", rel, ierr)
		}
		retainedByID := map[string]*document.Batch{}
		for _, b := range view.Retained {
			retainedByID[b.BatchID.String()] = b
		}
		for _, s := range insp.Batches {
			b := retainedByID[s.BatchID]
			if b == nil || s.EndCounter != b.CounterBase+int64(len(b.Operations))-1 {
				t.Fatalf("%s: batch %s range mismatch", rel, s.BatchID)
			}
			if len(s.Operations) != len(b.Operations) {
				t.Fatalf("%s: batch %s op count mismatch", rel, s.BatchID)
			}
			for i, op := range s.Operations {
				want := b.Operations[i]
				if op.OperationID != want.OperationID.String() ||
					op.Kind != string(want.Kind) ||
					op.EntityID != want.EntityID.String() ||
					op.HasPayload != (want.Payload != nil) {
					t.Fatalf("%s: batch %s op %d metadata mismatch: %+v", rel, s.BatchID, i, op)
				}
			}
		}
	}
}
