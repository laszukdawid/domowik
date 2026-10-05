package assets

import (
	"bytes"
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// gc.go tests: orphan collection, opaque pinned data protection, the
// injected metadata-anomaly matrix (fail-closed zero collection), and
// crash injection at the two-phase generation boundary.

func gcFixture(t *testing.T) (*Store, [2]string) {
	t.Helper()
	s := newTestStore(t)
	pinned := mustImport(t, s, blobBytes("pinned", 128), "texture")
	orphan := mustImport(t, s, blobBytes("orphan", 128), "texture")
	if err := s.AddPin(pinned.HexDigest, "doc-1", rootKindRootTable, ""); err != nil {
		t.Fatalf("AddPin: %v", err)
	}
	return s, [2]string{pinned.HexDigest, orphan.HexDigest}
}

// TestGCOrphanFromInterruptedInsertionIsCollectable realizes the issue
// 0026 acceptance criterion: insertion plus referencing operation is
// crash-consistent or leaves a detectable, safely collectible orphan.
func TestGCOrphanFromInterruptedInsertionIsCollectable(t *testing.T) {
	s, digests := gcFixture(t)
	res, err := s.Collect()
	if err != nil {
		t.Fatalf("Collect: %v", err)
	}
	if res.FailedClosed {
		t.Fatalf("unexpected fail-closed: %s %s", res.AnomalyKind, res.AnomalyText)
	}
	if len(res.Collected) != 1 || res.Collected[0] != digestPrefix+digests[1] {
		t.Fatalf("collected set wrong: %v", res.Collected)
	}
	if !s.HasBlob(digests[0]) {
		t.Fatal("pinned blob was collected — G4 violation")
	}
	if s.HasBlob(digests[1]) {
		t.Fatal("orphan blob survived collection")
	}
}

func TestGCProtectsOpaqueReferencedDataWithoutDescriptor(t *testing.T) {
	s := newTestStore(t)
	content := blobBytes("opaque", 96)
	res, err := s.Import(ImportInput{Reader: bytes.NewReader(content), MediaType: "image/png", Purpose: "texture"})
	if err != nil {
		t.Fatal(err)
	}
	// A foreign writer placed referenced data this store cannot
	// describe: only the pin names it. Collection must not inspect,
	// decode, or require comprehension of the blob (G3/G4).
	os.Remove(s.DescPath(res.Descriptor.HexDigest))
	if err := s.AddPin(res.Descriptor.HexDigest, "foreign-root", rootKindOperationFrontier, ""); err != nil {
		t.Fatalf("AddPin: %v", err)
	}
	got, cerr := s.Collect()
	if cerr != nil || got.FailedClosed {
		t.Fatalf("Collect: %+v %v", got, cerr)
	}
	if len(got.Collected) != 0 || !s.HasBlob(res.Descriptor.HexDigest) {
		t.Fatalf("opaque pinned data collected: %v", got.Collected)
	}
}

func mutateStoreFile(t *testing.T, s *Store, fn func(raw []byte) []byte) {
	t.Helper()
	raw, err := os.ReadFile(s.storePath())
	if err != nil {
		t.Fatal(err)
	}
	if werr := os.WriteFile(s.storePath(), fn(raw), 0o644); werr != nil {
		t.Fatal(werr)
	}
}

func expectFailClosedZeroCollection(t *testing.T, s *Store, digests [2]string) *GCResult {
	t.Helper()
	res, err := s.Collect()
	if err != nil {
		t.Fatalf("Collect errored instead of failing closed: %v", err)
	}
	if !res.FailedClosed {
		t.Fatalf("collector ran despite metadata anomaly: %+v", res)
	}
	if len(res.Collected) != 0 {
		t.Fatalf("fail-closed collector deleted data: %v", res.Collected)
	}
	if !s.HasBlob(digests[0]) || !s.HasBlob(digests[1]) {
		t.Fatal("anomalous state destroyed blobs")
	}
	return res
}

func TestAnomalyChecksumFlipFailsClosed(t *testing.T) {
	s, digests := gcFixture(t)
	mutateStoreFile(t, s, func(raw []byte) []byte {
		var st pinStoreState
		json.Unmarshal(raw, &st)
		st.Records[0].Checksum = strings.Replace(st.Records[0].Checksum, "a", "b", 1)
		out, _ := json.Marshal(&st)
		return out
	})
	expectFailClosedZeroCollection(t, s, digests)
}

func TestAnomalyGenerationGapFailsClosed(t *testing.T) {
	s, digests := gcFixture(t)
	mutateStoreFile(t, s, func(raw []byte) []byte {
		var st pinStoreState
		json.Unmarshal(raw, &st)
		st.Generation += 5 // jump past the expectation marker: gap
		out, _ := json.Marshal(&st)
		return out
	})
	res := expectFailClosedZeroCollection(t, s, digests)
	if res.AnomalyKind != "gap" {
		t.Fatalf("expected gap anomaly, got %s", res.AnomalyKind)
	}
}

func TestAnomalyGenerationRegressionFailsClosed(t *testing.T) {
	s, digests := gcFixture(t)
	mutateStoreFile(t, s, func(raw []byte) []byte {
		var st pinStoreState
		json.Unmarshal(raw, &st)
		st.Generation-- // store behind its expectation marker
		out, _ := json.Marshal(&st)
		return out
	})
	res := expectFailClosedZeroCollection(t, s, digests)
	if res.AnomalyKind != "regression" {
		t.Fatalf("expected regression anomaly, got %s", res.AnomalyKind)
	}
}

func TestAnomalyTruncationFailsClosed(t *testing.T) {
	s, digests := gcFixture(t)
	mutateStoreFile(t, s, func(raw []byte) []byte { return raw[:len(raw)/2] })
	expectFailClosedZeroCollection(t, s, digests)
}

func TestAnomalyGarbageRecordFailsClosed(t *testing.T) {
	s, digests := gcFixture(t)
	mutateStoreFile(t, s, func([]byte) []byte { return []byte("{\"generation\":1,\"records\":[{\"kind\":\"pin\"}]}") })
	expectFailClosedZeroCollection(t, s, digests)
}

// TestCrashInjectionBetweenMarkerAndStore simulates process death at the
// exact G1 boundary: the next-generation marker is durable but the pin
// store replacement never happened.
func TestCrashInjectionBetweenMarkerAndStore(t *testing.T) {
	s, digests := gcFixture(t)

	crashed := false
	saved := s.crashAfterMarker
	s.crashAfterMarker = func() {
		crashed = true
		panic("simulated crash after marker placement")
	}
	func() {
		defer func() { _ = recover() }()
		_ = s.AddPin(digests[1], "late-owner", rootKindManual, "")
	}()
	s.crashAfterMarker = saved
	if !crashed {
		t.Fatal("crash hook never fired")
	}

	// Startup sees the interrupted mutation and refuses to collect.
	res := expectFailClosedZeroCollection(t, s, digests)
	if res.AnomalyKind != "regression" {
		t.Fatalf("expected regression anomaly, got %s (%s)", res.AnomalyKind, res.AnomalyText)
	}

	// Repair rebuilds a clean validated store; collection resumes with
	// the conservative superset protecting every cached blob.
	rep, rerr := s.Repair()
	if rerr != nil {
		t.Fatalf("Repair: %v", rerr)
	}
	if !rep.Resumed || rep.ConservativePins != 2 {
		t.Fatalf("unexpected repair result: %+v", rep)
	}
	after, cerr := s.Collect()
	if cerr != nil || after.FailedClosed {
		t.Fatalf("collection did not resume after repair: %+v %v", after, cerr)
	}
	if len(after.Collected) != 0 {
		t.Fatalf("repair must protect the conservative superset, collected %v", after.Collected)
	}
	if !s.HasBlob(digests[0]) || !s.HasBlob(digests[1]) {
		t.Fatal("repair lost cached blobs")
	}
}

func TestRepairKeepsValidRecordsWithoutConservativePinsWhenClean(t *testing.T) {
	s, digests := gcFixture(t)
	rep, err := s.Repair()
	if err != nil {
		t.Fatalf("Repair on clean store: %v", err)
	}
	if rep.WasAnomalous || rep.ConservativePins != 0 || rep.KeptRecords != 1 || !rep.Resumed {
		t.Fatalf("unexpected repair result: %+v", rep)
	}
	after, cerr := s.Collect()
	if cerr != nil || after.FailedClosed {
		t.Fatalf("post-repair collect: %+v %v", after, cerr)
	}
	if len(after.Collected) != 1 || after.Collected[0] != digestPrefix+digests[1] {
		t.Fatalf("post-repair collection wrong: %v", after.Collected)
	}
}

// TestMutationsFailClosedWithoutPanic proves every metadata mutation
// refuses safely on an anomalous store instead of dereferencing the nil
// verified state.
func TestMutationsFailClosedWithoutPanic(t *testing.T) {
	anomalies := map[string]func([]byte) []byte{
		"checksum-flip": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Records[0].Checksum = strings.Replace(st.Records[0].Checksum, "a", "b", 1)
			out, _ := json.Marshal(&st)
			return out
		},
		"generation-gap": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Generation += 5
			out, _ := json.Marshal(&st)
			return out
		},
		"generation-regression": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Generation--
			out, _ := json.Marshal(&st)
			return out
		},
	}
	for name, mutate := range anomalies {
		t.Run(name, func(t *testing.T) {
			s, _ := gcFixture(t)
			mutateStoreFile(t, s, mutate)

			mustRefuse := func(err error) {
				t.Helper()
				if err == nil {
					t.Fatalf("%s: mutation on anomalous store was accepted", name)
				}
				if !strings.Contains(err.Error(), "fail") && !strings.Contains(err.Error(), "anomaly") &&
					!strings.Contains(err.Error(), "refusing mutation") {
					t.Fatalf("%s: refusal lacks fail-closed context: %v", name, err)
				}
			}

			d := digestPrefix + hexOfBlob([]byte("new-content-for-pin"))
			mustRefuse(s.AddPin(hexOfBlob([]byte("new-content-for-pin")), "owner-x", rootKindManual, ""))
			mustRefuse(s.ReleasePin(hexOfBlob([]byte("pinned")), "doc-1"))
			_ = d

			// Repair is the only route back to an operable store.
			rep, rerr := s.Repair()
			if rerr != nil || !rep.Resumed {
				t.Fatalf("%s: repair did not resume: %+v %v", name, rep, rerr)
			}
			if aerr := s.AddPin(hexOfBlob([]byte("new-content-for-pin")), "owner-x", rootKindManual, ""); aerr != nil {
				t.Fatalf("%s: AddPin after repair failed: %v", name, aerr)
			}
		})
	}
}

var (
	_ = json.Marshal
	_ = strings.Contains
)
