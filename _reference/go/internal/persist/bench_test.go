package persist_test

import (
	"bytes"
	"compress/flate"
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// bench_test.go: evidence-only benchmarks (Performance Policy). They are
// NEVER run by ./scripts/verify.sh correctness gates; results are recorded
// under docs/evidence/ with environment disclosure. Representative corpus
// inputs: the golden thin/portable packages plus a synthesized mid-size
// document built from golden fixtures.

func benchWriteInput(b *testing.B, entities int) *persist.WriteInput {
	b.Helper()
	levelRaw, err := os.ReadFile(filepath.Join(corpusDir, "golden/entities/level-minimal.json"))
	if err != nil {
		b.Fatal(err)
	}
	wallRaw, err := os.ReadFile(filepath.Join(corpusDir, "golden/entities/wall-minimal.json"))
	if err != nil {
		b.Fatal(err)
	}
	state := document.State{
		Tombstones:       map[document.UUID]document.Position{},
		Frontier:         map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		CreatedPositions: map[document.UUID]document.Position{},
	}
	for i := 0; i < entities; i++ {
		raw := levelRaw
		if i%2 == 1 {
			raw = wallRaw
		}
		var obj map[string]any
		if json.Unmarshal(raw, &obj) != nil {
			b.Fatal("fixture unparsable")
		}
		id, _ := obj["id"].(string)
		obj["id"] = fmt.Sprintf("019534c7-0000-7000-8000-%012d", i)
		obj["name"] = fmt.Sprintf("entity %d", i)
		delete(obj, "levelId")
		if _, isWall := obj["type"].(string); isWall && obj["type"] == "wall" {
			obj["levelId"] = "019534c7-0000-7000-8000-000000000000"
		}
		_ = id
		fixed, _ := json.Marshal(obj)
		ent, derr := document.DecodeEntity(fixed)
		if derr != nil {
			b.Fatal(derr)
		}
		state.Entities = append(state.Entities, ent)
	}
	return &persist.WriteInput{Profile: "thin", State: state, Blobs: map[string][]byte{}}
}

func BenchmarkSaveThin100Entities(b *testing.B) {
	input := benchWriteInput(b, 100)
	dir := b.TempDir()
	target := filepath.Join(dir, "bench.domo")
	var peak uint64
	for i := 0; i < b.N; i++ {
		var ms runtime.MemStats
		runtime.GC()
		runtime.ReadMemStats(&ms)
		base := ms.TotalAlloc
		res, err := persist.SaveDocument(target, 0o644, input, nil)
		if err != nil {
			b.Fatal(err)
		}
		_ = res
		runtime.ReadMemStats(&ms)
		if d := ms.TotalAlloc - base; d > peak {
			peak = d
		}
		fi, _ := os.Stat(target)
		if fi != nil {
			b.ReportMetric(float64(fi.Size()), "package-bytes")
		}
		os.Remove(target)
	}
	b.ReportMetric(float64(peak)/1e6, "cumulative-alloc-MB/op")
}

func BenchmarkLoadGoldenPortable(b *testing.B) {
	data, err := os.ReadFile(filepath.Join(corpusDir, "golden/packages/portable-valid.domo"))
	if err != nil {
		b.Fatal(err)
	}
	var peak uint64
	for i := 0; i < b.N; i++ {
		var ms runtime.MemStats
		runtime.GC()
		runtime.ReadMemStats(&ms)
		base := ms.TotalAlloc
		if _, err := persist.Load(data); err != nil {
			b.Fatal(err)
		}
		runtime.ReadMemStats(&ms)
		if d := ms.TotalAlloc - base; d > peak {
			peak = d
		}
	}
	b.ReportMetric(float64(peak), "cumulative-alloc-B/op")
}

func BenchmarkRoundTripThin1kEntities(b *testing.B) {
	input := benchWriteInput(b, 1000)
	pkg, werr := persist.BuildPackage(input)
	if werr != nil {
		b.Fatal(werr)
	}
	b.ReportMetric(float64(len(pkg)), "package-bytes")
	var peak uint64
	for i := 0; i < b.N; i++ {
		var ms runtime.MemStats
		runtime.GC()
		runtime.ReadMemStats(&ms)
		base := ms.TotalAlloc
		view, lerr := persist.Load(pkg)
		if lerr != nil {
			b.Fatal(lerr)
		}
		input2 := &persist.WriteInput{
			Profile:         view.Profile,
			State:           view.Projection.State(),
			RetainedBatches: view.Retained,
			AssetRoot:       view.AssetRoot,
			Blobs:           view.Blobs,
			DescriptorsRaw:  view.DescriptorsRaw,
		}
		if _, werr := persist.BuildPackage(input2); werr != nil {
			b.Fatal(werr)
		}
		runtime.ReadMemStats(&ms)
		if d := ms.TotalAlloc - base; d > peak {
			peak = d
		}
	}
	b.ReportMetric(float64(peak)/1e6, "cumulative-alloc-MB/op")
}

// BenchmarkStreamingLargeEntry exercises the chunked large-entry path:
// a deflate-compressed multi-megabyte preview blob streamed through the
// full guard pipeline.
func BenchmarkStreamingLargeEntry(b *testing.B) {
	// 512 KiB of zeros compresses to roughly 967:1 — just under the L15
	// boundary, so the stream exercises the full guarded pipeline legally.
	logical := bytes.Repeat([]byte{0}, 512<<10)
	var stream bytes.Buffer
	fw, _ := flate.NewWriter(&stream, flate.BestCompression)
	fw.Write(logical)
	fw.Close()
	sum := sha256.Sum256(logical)
	hexSum := fmt.Sprintf("%x", sum[:])
	full := "preview/data/" + hexSum[:2] + "/" + hexSum[2:]
	pkg := deflateEntryPackage(b, full, logical, stream.Bytes())
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		if _, err := persist.Load(pkg); err != nil {
			b.Fatal(err)
		}
	}
}
