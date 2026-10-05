package assets

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
	"path/filepath"
	"testing"
)

func newTestStore(t *testing.T) *Store {
	t.Helper()
	s, err := Open(filepath.Join(t.TempDir(), "cache"))
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	return s
}

func blobBytes(seed string, n int) []byte {
	out := make([]byte, 0, n)
	for len(out) < n {
		sum := sha256.Sum256([]byte(seed + string(rune(len(out)))))
		out = append(out, sum[:]...)
	}
	return out[:n]
}

func hexOfBlob(b []byte) string {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

func mustImport(t *testing.T, s *Store, content []byte, purpose string) *Descriptor {
	t.Helper()
	res, err := s.Import(ImportInput{
		Reader:    bytes.NewReader(content),
		MediaType: "application/x-domowik-test",
		Purpose:   purpose,
	})
	if err != nil {
		t.Fatalf("Import: %v", err)
	}
	if res.Descriptor.HexDigest != hexOfBlob(content) {
		t.Fatalf("digest mismatch after import")
	}
	return res.Descriptor
}

func TestImportVerifiesBeforeAdmissionAndDeduplicates(t *testing.T) {
	s := newTestStore(t)
	content := blobBytes("asset", 4096)
	d1 := mustImport(t, s, content, "background-image")
	if got, err := s.Verify(d1.HexDigest); err != nil || got != uint64(len(content)) {
		t.Fatalf("Verify: %d %v", got, err)
	}
	res, err := s.Import(ImportInput{
		Reader:    bytes.NewReader(content),
		MediaType: "image/png",
		Purpose:   "texture",
	})
	if err != nil {
		t.Fatalf("second import: %v", err)
	}
	if !res.Deduplicated {
		t.Fatal("identical blob was not deduplicated")
	}
	if res.Descriptor.HexDigest != d1.HexDigest {
		t.Fatal("dedup changed identity")
	}
	entries, _ := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if len(entries) != 0 {
		t.Fatalf("staging directory not clean: %d entries", len(entries))
	}
}

type failingReader struct {
	data   []byte
	failAt int
	pos    int
}

func (r *failingReader) Read(p []byte) (int, error) {
	if r.pos >= r.failAt {
		return 0, os.ErrDeadlineExceeded
	}
	n := copy(p, r.data[r.pos:])
	r.pos += n
	return n, nil
}

func TestImportMidStreamFailureLeavesNothingBehind(t *testing.T) {
	s := newTestStore(t)
	data := blobBytes("partial", 2048)
	r := &failingReader{data: data, failAt: 1024}
	if _, err := s.Import(ImportInput{Reader: r, MediaType: "image/png", Purpose: "texture"}); err == nil {
		t.Fatal("mid-stream failure accepted")
	}
	if s.HasBlob(hexOfBlob(data)) {
		t.Fatal("unverified content was admitted")
	}
	entries, _ := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if len(entries) != 0 {
		t.Fatalf("staging leftover after failure: %d entries", len(entries))
	}
}

func TestCountingWriterEnforcesCapBeforeYield(t *testing.T) {
	cw := &countingWriter{w: io.Discard, limit: 10}
	if _, err := cw.Write(make([]byte, 8)); err != nil {
		t.Fatalf("under cap rejected: %v", err)
	}
	if _, err := cw.Write(make([]byte, 8)); err == nil {
		t.Fatal("over-cap write accepted")
	}
	if cw.n != 8 {
		t.Fatalf("surplus bytes were counted/admitted: %d", cw.n)
	}
}

func TestListTotalsAndPinnedStatus(t *testing.T) {
	s := newTestStore(t)
	a := mustImport(t, s, blobBytes("a", 100), "texture")
	b := mustImport(t, s, blobBytes("b", 50), "background-image")
	if err := s.AddPin(a.HexDigest, "doc-1", rootKindSnapshot, ""); err != nil {
		t.Fatalf("AddPin: %v", err)
	}
	list, totals, err := s.List()
	if err != nil {
		t.Fatalf("List: %v", err)
	}
	if len(list) != 2 || totals.CacheBlobCount != 2 || totals.CacheBlobBytes != 150 {
		t.Fatalf("unexpected listing: %+v %+v", list, totals)
	}
	if totals.RemoteBytes != 0 {
		t.Fatal("remote bytes must be zero while retrieval is disabled")
	}
	byHex := map[string]AssetInfo{}
	for _, info := range list {
		byHex[info.HexDigest] = info
	}
	if !byHex[a.HexDigest].Pinned || byHex[b.HexDigest].Pinned {
		t.Fatal("pin status wrong in listing")
	}
}

func TestRemoveRefusesPinnedThenSucceedsUnpinned(t *testing.T) {
	s := newTestStore(t)
	d := mustImport(t, s, blobBytes("rm", 64), "texture")
	if err := s.AddPin(d.HexDigest, "manual-owner", rootKindManual, ""); err != nil {
		t.Fatalf("AddPin: %v", err)
	}
	if err := s.Remove(d.HexDigest); err == nil {
		t.Fatal("pinned removal accepted; GC correctness relies on pins covering roots")
	}
	if err := s.ReleasePin(d.HexDigest, "manual-owner"); err != nil {
		t.Fatalf("ReleasePin: %v", err)
	}
	if err := s.Remove(d.HexDigest); err != nil {
		t.Fatalf("unpinned removal failed: %v", err)
	}
	if s.HasBlob(d.HexDigest) {
		t.Fatal("blob survived removal")
	}
	if _, err := s.GetDescriptor(d.HexDigest); err == nil {
		t.Fatal("descriptor survived removal")
	}
}
