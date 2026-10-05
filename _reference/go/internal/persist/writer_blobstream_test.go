package persist_test

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// blobStreamSeamSource is a deterministic in-memory BlobSource over
// fixed bytes; each Open returns an independent reader.
type blobStreamSeamSource struct {
	content  []byte
	declared uint64
	short    bool // lie about the declared length to exercise C3
	failN    int  // when > 0, fail after yielding that many bytes
}

func (s *blobStreamSeamSource) DeclaredLength() uint64 {
	if s.short {
		return uint64(len(s.content) - 1)
	}
	if s.declared != 0 {
		return s.declared
	}
	return uint64(len(s.content))
}

func (s *blobStreamSeamSource) Open() (io.Reader, error) {
	r := &replayReader{content: s.content}
	if s.failN > 0 {
		r.failAfter = int64(s.failN)
	}
	return r, nil
}

type replayReader struct {
	content   []byte
	pos       int
	failAfter int64
	sent      int64
}

func (r *replayReader) Read(p []byte) (int, error) {
	if r.failAfter > 0 && r.sent >= r.failAfter {
		return 0, io.ErrClosedPipe
	}
	if r.pos >= len(r.content) {
		return 0, io.EOF
	}
	n := copy(p, r.content[r.pos:])
	r.pos += n
	r.sent += int64(n)
	return n, nil
}

func documentEmptyState() document.State { return document.State{} }

func streamTestInput(hexDigest string, src persist.BlobSource) *persist.WriteInput {
	state := documentEmptyState()
	return &persist.WriteInput{
		Profile:         persist.ProfilePortable(),
		State:           state,
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{hexDigest: src},
	}
}

func seamHexOf(b []byte) string {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

func TestBlobStreamSeamProducesLoadablePackage(t *testing.T) {
	blob := bytes.Repeat([]byte("domowik-streaming-blob\n"), 4000) // ~88 KiB
	hexDigest := seamHexOf(blob)
	pkg, berr := persist.BuildPackage(streamTestInput(hexDigest, &blobStreamSeamSource{content: blob}))
	if berr != nil {
		t.Fatalf("BuildPackage with BlobSources failed: %v", berr)
	}
	view, err := persist.Load(pkg)
	if err != nil {
		t.Fatalf("streamed package failed to load: %v", err)
	}
	if view.Profile != "portable" || len(view.AssetRoot) != 1 || view.AssetRoot[0] != "sha256:"+hexDigest {
		t.Fatalf("unexpected manifest: profile=%s assetRoot=%v", view.Profile, view.AssetRoot)
	}
	name := "blob/sha256/" + hexDigest[:2] + "/" + hexDigest[2:]
	got, ok := view.Blobs[hexDigest]
	if !ok || !bytes.Equal(got, blob) {
		t.Fatalf("embedded streamed blob mismatch or absent")
	}
	inv, ok := view.EntryInventory(name)
	if !ok || inv.ByteLength != uint64(len(blob)) || inv.SHA256 != "sha256:"+hexDigest {
		t.Fatalf("inventory wrong: %+v", inv)
	}
	// The guarded stream surface re-verifies the entry end-to-end.
	st, serr := view.StreamEntry(name)
	if serr != nil {
		t.Fatalf("StreamEntry: %v", serr)
	}
	streamed, rerr := readAllGuarded(st)
	if rerr != nil {
		t.Fatalf("guarded stream read: %v", rerr)
	}
	if !bytes.Equal(streamed, blob) || st.Verified() != nil {
		t.Fatalf("guarded stream content mismatch")
	}
}

func TestBlobStreamSeamDeclaredLengthMismatchAborts(t *testing.T) {
	blob := []byte("declared length must match observed growth exactly")
	hexDigest := seamHexOf(blob)
	_, berr := persist.BuildPackage(streamTestInput(hexDigest, &blobStreamSeamSource{content: blob, short: true}))
	if berr == nil {
		t.Fatalf("short declared length was accepted (C3 violation)")
	}
	if cat, ok := persist.CategoryOf(berr); !ok || cat != "malformed-length" {
		t.Fatalf("expected malformed-length, got %v", berr)
	}
}

func TestBlobStreamSeamMidStreamFailureAborts(t *testing.T) {
	blob := bytes.Repeat([]byte("x"), 4096)
	hexDigest := seamHexOf(blob)
	_, berr := persist.BuildPackage(streamTestInput(hexDigest, &blobStreamSeamSource{content: blob, failN: 64}))
	if berr == nil {
		t.Fatalf("mid-stream source failure was accepted")
	}
	if !strings.Contains(berr.Error(), "blob source read failed") {
		t.Fatalf("unexpected error: %v", berr)
	}
}

func TestBlobStreamSeamOversizeDeclarationRejected(t *testing.T) {
	blob := []byte("tiny")
	hexDigest := seamHexOf(blob)
	src := &blobStreamSeamSource{content: blob, declared: uint64(persist.LimitBlobBytes) + 1}
	_, berr := persist.BuildPackage(streamTestInput(hexDigest, src))
	if berr == nil {
		t.Fatalf("declaration above L5 was accepted")
	}
	if cat, ok := persist.CategoryOf(berr); !ok || cat != "limit-blob-bytes" {
		t.Fatalf("expected limit-blob-bytes, got %v", berr)
	}
}

func TestBlobStreamAndBufferedMutuallyExclusive(t *testing.T) {
	blob := []byte("same digest both ways")
	hexDigest := seamHexOf(blob)
	in := streamTestInput(hexDigest, &blobStreamSeamSource{content: blob})
	in.Blobs = map[string][]byte{hexDigest: blob}
	_, berr := persist.BuildPackage(in)
	if berr == nil {
		t.Fatalf("dual-sourced digest accepted")
	}
	if cat, ok := persist.CategoryOf(berr); !ok || cat != "safe-write-refusal" {
		t.Fatalf("expected safe-write-refusal, got %v", berr)
	}
}

func readAllGuarded(st *persist.EntryStream) ([]byte, error) {
	var out bytes.Buffer
	buf := make([]byte, 8192)
	for {
		n, err := st.Read(buf)
		out.Write(buf[:n])
		if err == io.EOF {
			return out.Bytes(), nil
		}
		if err != nil {
			return out.Bytes(), err
		}
	}
}

func TestBlobStreamSeamDigestIdentityMismatchAborts(t *testing.T) {
	blob := []byte("content whose bytes do not match its claimed digest identity")
	// The entry claims a DIFFERENT digest identity than its bytes hash to.
	wrong := "sha256:" + strings.Repeat("ab", 32)
	in := streamTestInput(strings.TrimPrefix(wrong, "sha256:"), &blobStreamSeamSource{content: blob})
	_, berr := persist.BuildPackage(in)
	if berr == nil {
		t.Fatalf("stream whose content contradicts its claimed digest was embedded")
	}
	if cat, ok := persist.CategoryOf(berr); !ok || cat != "checksum-error" {
		t.Fatalf("expected checksum-error, got %v", berr)
	}
}

func TestLoadFileCloseReleasesStreamingSurface(t *testing.T) {
	pkg, berr := persist.BuildPackage(streamTestInput(
		seamHexOf([]byte("close-lifetime-check")),
		&blobStreamSeamSource{content: []byte("close-lifetime-check")}))
	if berr != nil {
		t.Fatal(berr)
	}
	path := filepath.Join(t.TempDir(), "cand.domo")
	os.WriteFile(path, pkg, 0o644)

	view, err := persist.LoadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	name := persist.BlobEntryName(seamHexOf([]byte("close-lifetime-check")))
	if _, serr := view.StreamEntry(name); serr != nil {
		t.Fatalf("StreamEntry before Close: %v", serr)
	}
	if cerr := view.Close(); cerr != nil {
		t.Fatalf("Close: %v", cerr)
	}
	if _, serr := view.StreamEntry(name); serr == nil {
		t.Fatal("StreamEntry succeeded after Close")
	}
	// Close is idempotent.
	if cerr := view.Close(); cerr != nil {
		t.Fatalf("second Close: %v", cerr)
	}
}
