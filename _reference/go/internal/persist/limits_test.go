package persist_test

import (
	"bytes"
	"compress/flate"
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"hash/crc32"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

// limits_test.go: executable boundary probes for the ratified §9.3 rows
// that can be materialized within test budgets. Every magnitude cites its
// row via the named constants of the persist package (ED-0021); no
// independent limit literals appear below. Rows whose faithful bytes are
// GiB-scale (L1/L2 cumulative) are covered structurally by guard
// placement plus these boundary twins.

func catOf(t *testing.T, err error) persist.Category {
	t.Helper()
	c, _ := persist.CategoryOf(err)
	return c
}

// TestLimitL17L18ArrayMemberBoundaries pins L17/L18: == limit parses,
// one-over rejects at the parser before element allocation matters.
func TestLimitL17L18ArrayMemberBoundaries(t *testing.T) {
	build := func(n int) []byte {
		var buf bytes.Buffer
		buf.WriteByte('[')
		for i := 0; i < n; i++ {
			if i > 0 {
				buf.WriteByte(',')
			}
			buf.WriteString("1")
		}
		buf.WriteByte(']')
		return buf.Bytes()
	}
	if err := persist.ParseLimitedJSON(build(persist.LimitArrayElementCount)); err != nil {
		t.Fatalf("L17: array at limit must parse: %v", err)
	}
	err := persist.ParseLimitedJSON(build(persist.LimitArrayElementCount + 1))
	if err == nil || catOf(t, err) != persist.CatLimitArrayCount {
		t.Fatalf("L17: one-over must reject limit-array-count, got %v", err)
	}

	member := func(n int) []byte {
		var buf bytes.Buffer
		buf.WriteByte('{')
		for i := 0; i < n; i++ {
			if i > 0 {
				buf.WriteByte(',')
			}
			fmt.Fprintf(&buf, "\"k%d\":1", i)
		}
		buf.WriteByte('}')
		return buf.Bytes()
	}
	if err := persist.ParseLimitedJSON(member(persist.LimitObjectMemberCount)); err != nil {
		t.Fatalf("L18: object at limit must parse: %v", err)
	}
	err = persist.ParseLimitedJSON(member(persist.LimitObjectMemberCount + 1))
	if err == nil || catOf(t, err) != persist.CatLimitMemberCount {
		t.Fatalf("L18: one-over must reject limit-member-count, got %v", err)
	}
}

// TestLimitL15RatioTwins finds genuine deflate streams just below and
// just above the ratified ratio boundary and asserts incremental
// enforcement on the real decompression path.
func TestLimitL15RatioTwins(t *testing.T) {
	deflated := func(n int) []byte {
		var buf bytes.Buffer
		fw, _ := flate.NewWriter(&buf, flate.BestCompression)
		fw.Write(bytes.Repeat([]byte{0}, n))
		fw.Close()
		return buf.Bytes()
	}
	overN, underN := -1, -1
	for _, n := range []int{1 << 12, 1 << 14, 1 << 16, 1 << 18, 1 << 20, 1 << 22} {
		ratio := float64(n) / float64(len(deflated(n)))
		if ratio > float64(persist.LimitExpansionRatio) && overN < 0 {
			overN = n
		}
		if ratio <= float64(persist.LimitExpansionRatio) && ratio > 900 && underN < 0 {
			underN = n
		}
	}
	if overN < 0 || underN < 0 {
		t.Skip("no deterministic twins found in the probed range; guards covered structurally")
	}
	load := func(logical int) error {
		// The optional preview namespace carries opaque derived bytes, so
		// a zero run is legitimate payload there while the ratio guard
		// still binds during streaming (C8: physical vs expanded classes).
		pkg := deflateEntryPackage(t,
			"preview/data/00/"+strings.Repeat("a", 62),
			bytes.Repeat([]byte{0}, logical), deflated(logical))
		_, err := persist.Load(pkg)
		return err
	}
	if err := load(underN); err != nil {
		t.Fatalf("L15: under-limit genuine stream must succeed: %v", err)
	}
	err := load(overN)
	if err == nil {
		t.Fatal("L15: over-ratio stream must reject")
	}
	if c := catOf(t, err); c != persist.CatLimitExpansionRatio && c != persist.CatMalformedLength {
		t.Fatalf("L15 token: got %s (%v)", c, err)
	}
}

// deflateEntryPackage builds a thin package whose named structural entry
// carries an explicit stored (deflate) stream plus an empty-snapshot-free
// manifest, giving byte-level control over the compressed path.
func deflateEntryPackage(t testing.TB, name string, logical, stream []byte) []byte {
	t.Helper()
	const (
		sigLocal   uint32 = 0x04034b50
		sigCentral uint32 = 0x02014b50
		sigEOCD    uint32 = 0x06054b50
	)
	var buf bytes.Buffer
	u32 := func(v uint32) []byte {
		out := make([]byte, 4)
		binary.LittleEndian.PutUint32(out, v)
		return out
	}
	u16 := func(v uint16) []byte {
		out := make([]byte, 2)
		binary.LittleEndian.PutUint16(out, v)
		return out
	}
	type inv struct {
		SHA256     string `json:"sha256"`
		ByteLength int    `json:"byteLength"`
	}
	sum := sha256.Sum256(logical)
	emptySnapshot := []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`)
	sumEmpty := sha256.Sum256(emptySnapshot)
	manifest, merr := json.Marshal(map[string]any{
		"formatVersion": "0.1",
		"profile":       "thin",
		"assetRoot":     nil,
		"entries": map[string]inv{
			name:                  {"sha256:" + hexOf(sum[:]), len(logical)},
			"snapshot/state.json": {"sha256:" + hexOf(sumEmpty[:]), len(emptySnapshot)},
		},
	})
	if merr != nil {
		t.Fatal(merr)
	}
	compressed := deflateBytesTest(manifest)

	type item struct {
		name    string
		logical []byte
		stored  []byte
		method  uint16
	}
	items := []item{
		{"domowik.json", manifest, compressed, 8},
		{name, logical, stream, 8},
		{"snapshot/state.json", []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`),
			nil, 0},
	}
	// The empty snapshot travels stored (method 0).
	items[2].stored = items[2].logical
	var centrals [][]byte
	var centralsAlt [][]byte
	cdStart := uint32(0)
	for _, it := range items {
		crc := crc32.ChecksumIEEE(it.logical)
		off := uint32(buf.Len())
		buf.Write(bytes.Join([][]byte{
			u32(sigLocal), u16(20), u16(0x0800), u16(it.method),
			{0, 0}, {0, 0}, u32(crc),
			u32(uint32(len(it.stored))), u32(uint32(len(it.logical))),
			u16(uint16(len(it.name))), u16(0),
		}, nil))
		buf.WriteString(it.name)
		buf.Write(it.stored)
		rec := bytes.Join([][]byte{
			u32(sigCentral), u16(0), u16(20), u16(0x0800), u16(it.method),
			{0, 0}, {0, 0}, u32(crc),
			u32(uint32(len(it.stored))), u32(uint32(len(it.logical))),
			u16(uint16(len(it.name))), u16(0), u16(0), u16(0), u16(0), u32(0),
			u32(off),
		}, nil)
		centrals = append(centrals, rec)
		centralsAlt = append([][]byte{rec}, centralsAlt...)
	}
	_ = centralsAlt
	cdStart = uint32(buf.Len())
	for idx, rec := range centrals {
		buf.Write(rec)
		buf.WriteString(items[idx].name)
	}
	cdSize := uint32(buf.Len()) - cdStart
	buf.Write(bytes.Join([][]byte{
		u32(sigEOCD), u16(0), u16(0), u16(uint16(len(items))), u16(uint16(len(items))),
		u32(cdSize), u32(cdStart), u16(0),
	}, nil))
	return buf.Bytes()
}

func deflateBytesTest(b []byte) []byte {
	var buf bytes.Buffer
	fw, _ := flate.NewWriter(&buf, flate.DefaultCompression)
	fw.Write(b)
	fw.Close()
	return buf.Bytes()
}

// TestLimitLOpsPerBatchBoundary pins the L23 operations-per-batch edge.
func TestLimitLOpsPerBatchBoundary(t *testing.T) {
	batch := func(n int) []byte {
		ops := make([]map[string]any, n)
		for i := range ops {
			ops[i] = map[string]any{
				"operationId": fmt.Sprintf("01949100-0000-7000-8000-%012d", i),
				"kind":        "deleteEntity",
				"entityId":    "01949101-0000-7000-8000-000000000001",
			}
		}
		doc := map[string]any{
			"batchId":     "01949102-0000-7000-8000-000000000009",
			"actorId":     "01948f5a-ca77-7a09-9e76-5ace11b12fd2",
			"counterBase": 1,
			"frontier":    map[string]int64{},
			"operations":  ops,
		}
		out, _ := json.Marshal(doc)
		return out
	}
	if _, perr := persist.ParseBatchDocument(batch(persist.LimitOperationsPerBatch)); perr != nil {
		t.Fatalf("L23: batch at operation limit must frame-check cleanly: %v", perr)
	}
	_, perr := persist.ParseBatchDocument(batch(persist.LimitOperationsPerBatch + 1))
	if perr == nil || catOf(t, perr) != persist.CatLimitOperationCount {
		t.Fatalf("L23: one-over must reject limit-operation-count, got %v", perr)
	}
}

// TestLimitSnapshotMetadataCeiling pins row L23's snapshot-metadata cap:
// a snapshot section above the ceiling rejects before semantic parsing.
func TestLimitSnapshotMetadataCeiling(t *testing.T) {
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
	_, lerr := persist.Load(pkg)
	if lerr == nil || catOf(t, lerr) != persist.CatLimitSnapshotMeta {
		t.Fatalf("snapshot metadata ceiling: expected limit-snapshot-metadata, got %v", lerr)
	}
}

// TestLimitL6EntryCountBoundary pins the entry-count edge with valid
// preview-namespace names.
func TestLimitL6EntryCountBoundary(t *testing.T) {
	entryName := func(i int) string {
		const digits = "0123456789abcdef"
		out := make([]byte, 62)
		for j := range out {
			out[j] = digits[(uint(i)>>(4*(j%16)))&0xf] // injective in i
		}
		return fmt.Sprintf("preview/data/%02x/%s", (i*37)%256, string(out))
	}
	build := func(n int) map[string][]byte {
		raws := map[string][]byte{}
		for i := 0; i < n; i++ {
			raws[entryName(i)] = []byte("p")
		}
		return raws
	}
	// The two structural entries (manifest + snapshot) count toward L6,
	// so the preview population is the remainder.
	pkgAt, werr := persist.AssembleRawPackage(build(persist.LimitEntryCount - 2))
	if werr != nil {
		t.Fatalf("L6: package at the ceiling must assemble: %v", werr)
	}
	if _, lerr := persist.Load(pkgAt); lerr != nil {
		t.Fatalf("L6: == 65535 entries succeeds per the row boundary; got %v", lerr)
	}
	// One over the ceiling: the writer refuses at L6 with the same
	// ratified token (writers must enforce limits at write time too).
	_, werr = persist.AssembleRawPackage(build(persist.LimitEntryCount - 1))
	if werr == nil || catOf(t, werr) != persist.CatLimitEntryCount {
		t.Fatalf("L6: one-over must refuse limit-entry-count, got %v", werr)
	}
}

// TestLimitL6EntryCountReaderRefusal pins L6 reader-side enforcement:
// a hostile Zip64 package claiming 65,536 entries must be refused at
// zipread.go:215 BEFORE the attacker-controlled e.count reaches the two
// proportional allocations at :219 (reserve calculation) and :248 (entries
// slice allocation).
func TestLimitL6EntryCountReaderRefusal(t *testing.T) {
	// Build a valid baseline package, then construct a Zip64 structure
	// around it that claims 65,536 entries (one over limit).
	base, werr := persist.AssembleRawPackage(map[string][]byte{
		"preview/data/00/x": []byte("p"),
	})
	if werr != nil {
		t.Fatalf("baseline package assembly: %v", werr)
	}

	// Zip64 EOCD (56 bytes) claiming 65,536 entries:
	//   [0:4]   signature 0x06064b50
	//   [4:12]  size of record (44, excludes sig and size field itself)
	//   [12:14] version made by
	//   [14:16] version needed
	//   [16:20] disk number
	//   [20:24] disk with CD start
	//   [24:32] entries on this disk (uint64)
	//   [32:40] total entries (uint64) ← 65536 here
	//   [40:48] CD size (uint64)
	//   [48:56] CD offset (uint64)
	z64eocd := make([]byte, 56)
	binary.LittleEndian.PutUint32(z64eocd[0:], 0x06064b50)            // sig
	binary.LittleEndian.PutUint64(z64eocd[4:], 44)                    // record size
	binary.LittleEndian.PutUint16(z64eocd[12:], 45)                   // version made by
	binary.LittleEndian.PutUint16(z64eocd[14:], 45)                   // version needed
	binary.LittleEndian.PutUint32(z64eocd[16:], 0)                    // disk number
	binary.LittleEndian.PutUint32(z64eocd[20:], 0)                    // CD disk
	binary.LittleEndian.PutUint64(z64eocd[24:], 65536)                // entries on disk
	binary.LittleEndian.PutUint64(z64eocd[32:], 65536)                // total entries (OVER LIMIT)
	binary.LittleEndian.PutUint64(z64eocd[40:], 0)                    // CD size
	binary.LittleEndian.PutUint64(z64eocd[48:], uint64(len(z64eocd))) // CD offset (after this record)

	// Prepend Zip64 EOCD, then append locator + standard EOCD.
	// The locator sits 20 bytes before the standard EOCD and points back
	// to offset 0 where we placed the Zip64 EOCD.
	z64loc := make([]byte, 20)
	binary.LittleEndian.PutUint32(z64loc[0:], 0x07064b50) // locator sig
	binary.LittleEndian.PutUint32(z64loc[4:], 0)          // disk with Z64 EOCD
	binary.LittleEndian.PutUint64(z64loc[8:], 0)          // offset of Z64 EOCD = 0
	binary.LittleEndian.PutUint32(z64loc[16:], 1)         // total disks

	// Standard EOCD with 0xFFFF markers to signal Zip64.
	stdEOCD := base[len(base)-22:]
	patched := append([]byte(nil), stdEOCD...)
	binary.LittleEndian.PutUint16(patched[8:], 0xFFFF)  // entries on disk (Z64 marker)
	binary.LittleEndian.PutUint16(patched[10:], 0xFFFF) // total entries (Z64 marker)

	// Layout: [Zip64 EOCD 56B] [original package without EOCD] [locator 20B] [patched EOCD 22B]
	hostile := append(z64eocd, base[:len(base)-22]...)
	hostile = append(hostile, z64loc...)
	hostile = append(hostile, patched...)

	// The reader must refuse at zipread.go:215 before e.count reaches:
	//   :219  reserve := e.count * minCentralOverhead
	//   :248  a.entries = make([]*entryInfo, 0, e.count)
	_, err := persist.Load(hostile)
	if err == nil {
		t.Fatal("L6-reader: Zip64 package claiming 65,536 entries must be refused")
	}
	if catOf(t, err) != persist.CatLimitEntryCount {
		t.Fatalf("L6-reader: expected limit-entry-count, got %s (%v)", catOf(t, err), err)
	}
}
