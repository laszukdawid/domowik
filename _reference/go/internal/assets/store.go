package assets

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"domowik.dev/domowik/internal/persist"
)

// store.go: the local content-addressed cache. Layout under the root:
//
//	blob/sha256/<2-hex>/<62-hex>   admitted blobs (§10.3 mapping)
//	desc/sha256/<2-hex>/<62-hex>.json  descriptor documents
//	meta/store.json                checksummed pin records + generation
//	meta/next-gen                  expected next-generation marker
//	tmp/                           staging for atomic placements
//
// Admission order realizes "digest/length before admission" (§10.6) and
// G1: the blob is streamed, hashed, and length-checked to a staging file
// BEFORE any namespace-visible placement; descriptors are written only
// after their blob exists; pins are separate durable records. An
// interruption therefore leaves at worst an unreferenced orphan blob
// that GC safely collects.

const (
	layoutBlob   = "blob"
	layoutDesc   = "desc"
	layoutMeta   = "meta"
	layoutTmp    = "tmp"
	descSuffix   = ".json"
	storeFile    = "store.json"
	nextGenFile  = "next-gen"
	importPrefix = "import-"

	// importChunkBytes is the streaming copy buffer size; a transport
	// detail with no normative meaning.
	importChunkBytes = 64 << 10
)

// Store is one on-disk asset cache.
type Store struct {
	root string
	mu   sync.Mutex

	// crashAfterMarker, when set by fault-injection tests, runs after
	// the next-generation marker is durably placed and before the pin
	// store replaces itself; it simulates process death at exactly that
	// boundary.
	crashAfterMarker func()

	// crashAfterBlobAdmit, when set by fault-injection tests, runs after
	// a verified staging blob has been renamed into the blob namespace
	// and before its descriptor is recorded; it simulates process death
	// between admission stages so tests can prove the resulting state is
	// a detectable, safely collectible orphan rather than a half-admitted
	// asset.
	crashAfterBlobAdmit func()
}

// Open prepares (creating if needed) the directory layout of the cache
// rooted at root.
func Open(root string) (*Store, error) {
	s := &Store{root: root}
	for _, d := range []string{layoutBlob, layoutDesc, layoutMeta, layoutTmp} {
		if err := os.MkdirAll(filepath.Join(root, d), 0o755); err != nil {
			return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
				Detail: "cache directory creation failed: " + err.Error()}
		}
	}
	return s, nil
}

func (s *Store) Root() string { return s.root }

func (s *Store) BlobPath(hexDigest string) string {
	return filepath.Join(s.root, blobRelPath(hexDigest))
}

func (s *Store) descRelPath(hexDigest string) string {
	return layoutDesc + "/sha256/" + hexDigest[:2] + "/" + hexDigest[2:] + descSuffix
}

func (s *Store) DescPath(hexDigest string) string {
	return filepath.Join(s.root, s.descRelPath(hexDigest))
}

func (s *Store) storePath() string   { return filepath.Join(s.root, layoutMeta, storeFile) }
func (s *Store) nextGenPath() string { return filepath.Join(s.root, layoutMeta, nextGenFile) }
func (s *Store) tmpPath() string     { return filepath.Join(s.root, layoutTmp) }

// HasBlob reports whether a blob file exists for the digest.
func (s *Store) HasBlob(hexDigest string) bool {
	fi, err := os.Stat(s.BlobPath(hexDigest))
	return err == nil && fi.Mode().IsRegular()
}

// ImportInput carries one streaming import request. Zero-valued optional
// fields receive documented defaults.
type ImportInput struct {
	// Reader streams the logical blob bytes B; it is consumed exactly
	// once, hashed incrementally, and bounded by L5 during the read.
	Reader io.Reader
	// MediaType is a required type/subtype token.
	MediaType string
	// Purpose is a required role token (e.g. "background-image").
	Purpose string
	// Requirement defaults to required when empty.
	Requirement string
	// Provenance attribution metadata (never functional). SourceDescription
	// defaults to "user import"; AcquiredAt zero means now (UTC).
	DisplayName       string
	Author            string
	SourceDescription string
	AcquiredAt        time.Time
}

// ImportResult reports a completed admission.
type ImportResult struct {
	Descriptor *Descriptor
	// Deduplicated is true when an identical blob was already cached;
	// the existing copy was kept (writers deduplicate before write,
	// §10.6).
	Deduplicated bool
}

// Import streams the input into the cache. The byte stream is fully
// digested and length-capped before admission; no partial or unverified
// content ever becomes visible in the blob namespace. Metadata fields
// are validated before any bytes are read.
func (s *Store) Import(in ImportInput) (*ImportResult, error) {
	if in.MediaType == "" {
		return nil, invalidField("media type is required for import")
	}
	if err := validateMediaType(in.MediaType); err != nil {
		return nil, err
	}
	if in.Purpose == "" {
		return nil, invalidField("purpose is required for import")
	}
	if err := validateToken(in.Purpose); err != nil || len(in.Purpose) > purposeMaxBytes {
		return nil, invalidField("purpose must be a token of at most %d bytes", purposeMaxBytes)
	}
	requirement := in.Requirement
	if requirement == "" {
		requirement = RequirementRequired
	}
	if requirement != RequirementRequired && requirement != RequirementOptional {
		return nil, invalidField("requirement must be required or optional")
	}
	for _, f := range []struct{ name, val string }{
		{"display name", in.DisplayName}, {"author", in.Author}, {"source description", in.SourceDescription},
	} {
		if len(f.val) > displayMaxBytes {
			return nil, invalidField("%s exceeds %d bytes (L19)", f.name, displayMaxBytes)
		}
	}
	source := in.SourceDescription
	if source == "" {
		source = "user import"
	}
	acquiredAt := in.AcquiredAt
	if acquiredAt.IsZero() {
		acquiredAt = time.Now().UTC()
	}
	acquiredText := acquiredAt.UTC().Format("2006-01-02T15:04:05Z")

	s.mu.Lock()
	defer s.mu.Unlock()

	// Stage 1: stream-verify to a private staging file. Digest and
	// observed length are known before anything is admitted.
	staging := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	f, err := os.OpenFile(staging, os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o644)
	if err != nil {
		return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "staging file creation failed: " + err.Error()}
	}
	h := sha256.New()
	counter := &countingWriter{w: f, h: h, limit: persist.LimitBlobBytes}
	if _, cerr := io.CopyBuffer(counter, in.Reader, make([]byte, importChunkBytes)); cerr != nil {
		f.Close()
		os.Remove(staging)
		if lim, ok := cerr.(*limitExceededError); ok {
			return nil, &persist.Error{Category: persist.CatLimitBlobBytes, Stage: persist.StageDigest,
				Detail: "asset exceeds the per-blob cap during streaming verification (L5) after " +
					itoaUint(lim.observed) + " bytes"}
		}
		return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "staging write failed: " + cerr.Error()}
	}
	if err := f.Close(); err != nil {
		os.Remove(staging)
		return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "staging close failed: " + err.Error()}
	}
	hexDigest := hex.EncodeToString(h.Sum(nil))
	length := counter.n

	// Stage 2: admit the verified blob atomically. An identical cached
	// blob deduplicates; writers never rewrite identical content.
	dest := s.BlobPath(hexDigest)
	dedup := false
	if _, err := os.Lstat(dest); err == nil {
		dedup = true
		os.Remove(staging)
	} else {
		if merr := os.MkdirAll(filepath.Dir(dest), 0o755); merr != nil {
			os.Remove(staging)
			return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
				Detail: "blob shard creation failed: " + merr.Error()}
		}
		if rerr := os.Rename(staging, dest); rerr != nil {
			os.Remove(staging)
			return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
				Detail: "blob admission rename failed: " + rerr.Error()}
		}
		if s.crashAfterBlobAdmit != nil {
			s.crashAfterBlobAdmit()
		}
	}

	// Stage 3: record the descriptor behind the now-present blob. A
	// crash here leaves an unpinned orphan blob — detectable and safely
	// collectible (issue 0026 AC), never a half-admitted asset.
	desc := &Descriptor{
		Digest:                digestPrefix + hexDigest,
		HexDigest:             hexDigest,
		ByteLength:            length,
		MediaType:             in.MediaType,
		Purpose:               in.Purpose,
		Encoding:              EncodingIdentity,
		Requirement:           requirement,
		ProvenanceDisplayName: in.DisplayName,
		ProvenanceSource:      source,
		ProvenanceAuthor:      in.Author,
		AcquiredAt:            acquiredText,
	}
	raw, derr := BuildDescriptorJSON(desc)
	if derr != nil {
		return nil, derr
	}
	if perr := s.parseAndPlace(desc, raw); perr != nil {
		return nil, perr
	}
	return &ImportResult{Descriptor: desc, Deduplicated: dedup}, nil
}

func (s *Store) parseAndPlace(desc *Descriptor, raw []byte) error {
	if _, perr := ParseDescriptor(raw); perr != nil {
		return perr // constructor defect guard: never place an invalid document
	}
	dest := s.DescPath(desc.HexDigest)
	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "descriptor shard creation failed: " + err.Error()}
	}
	tmp := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	if err := writeFileAtomic(tmp, dest, raw); err != nil {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "descriptor placement failed: " + err.Error()}
	}
	return nil
}

// GetDescriptor loads and validates the descriptor recorded for a digest.
func (s *Store) GetDescriptor(hexDigest string) (*Descriptor, error) {
	if err := checkHexDigest(hexDigest); err != nil {
		return nil, err
	}
	raw, err := os.ReadFile(s.DescPath(hexDigest))
	if err != nil {
		if os.IsNotExist(err) {
			return nil, missingErr("no descriptor is recorded for %s", digestPrefix+hexDigest)
		}
		return nil, readErr("descriptor read failed: %v", err)
	}
	return ParseDescriptor(raw)
}

// AssetInfo summarizes one cached asset for listing surfaces.
type AssetInfo struct {
	Digest      string // full sha256: identifier
	HexDigest   string
	ByteLength  uint64
	MediaType   string
	Purpose     string
	Requirement string
	Pinned      bool
	BlobPresent bool
}

// Totals distinguishes the size classes of ASSET-STRATEGY §10.4/§10.9:
// cache bytes live in the local store; remote bytes are always zero
// while retrieval is consent-gated and disabled by default; package
// bytes belong to the pack surface.
type Totals struct {
	CacheBlobCount int
	CacheBlobBytes uint64
	RemoteBytes    uint64 // retrieval disabled by default: always 0 in v0.1 CLI scope
	RemoteNote     string
}

// List enumerates cached descriptors (sorted by digest) plus totals.
func (s *Store) List() ([]AssetInfo, *Totals, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	pins, anomaly, err := s.loadPinStoreLocked()
	if err != nil {
		return nil, nil, err
	}
	pinned := map[string]bool{}
	if anomaly == nil {
		for _, rec := range pins.state.Records {
			pinned[rec.hexDigest()] = true
		}
	}

	type descEntry struct {
		hex  string
		desc *Descriptor
	}
	var descs descList
	descRoot := filepath.Join(s.root, layoutDesc, "sha256")
	shards, serr := os.ReadDir(descRoot)
	if serr != nil && !os.IsNotExist(serr) {
		return nil, nil, readErr("descriptor enumeration failed: %v", serr)
	}
	for _, shard := range shards {
		if !shard.IsDir() {
			continue
		}
		files, ferr := os.ReadDir(filepath.Join(descRoot, shard.Name()))
		if ferr != nil {
			continue
		}
		for _, fi := range files {
			name := fi.Name()
			if !strings.HasSuffix(name, descSuffix) {
				continue
			}
			hexDigest := shard.Name() + strings.TrimSuffix(name, descSuffix)
			raw, rerr := os.ReadFile(filepath.Join(descRoot, shard.Name(), name))
			if rerr != nil {
				continue // unreadable metadata files are skipped, not fatal
			}
			d, perr := ParseDescriptor(raw)
			if perr != nil || d.HexDigest != hexDigest {
				continue // foreign or defective files never enter the listing
			}
			descs = append(descs, descEntry{hex: hexDigest, desc: d})
		}
	}
	sortByHex(descs)

	out := make([]AssetInfo, 0, len(descs))
	for _, e := range descs {
		out = append(out, AssetInfo{
			Digest:      e.desc.Digest,
			HexDigest:   e.hex,
			ByteLength:  e.desc.ByteLength,
			MediaType:   e.desc.MediaType,
			Purpose:     e.desc.Purpose,
			Requirement: e.desc.Requirement,
			Pinned:      pinned[e.hex],
			BlobPresent: s.HasBlob(e.hex),
		})
	}

	totals := &Totals{RemoteNote: "retrieval disabled (consent default)"}
	blobRoot := filepath.Join(s.root, layoutBlob, "sha256")
	shards, serr = os.ReadDir(blobRoot)
	if serr != nil && !os.IsNotExist(serr) {
		return nil, nil, readErr("blob enumeration failed: %v", serr)
	}
	for _, shard := range shards {
		if !shard.IsDir() {
			continue
		}
		files, ferr := os.ReadDir(filepath.Join(blobRoot, shard.Name()))
		if ferr != nil {
			continue
		}
		for _, fi := range files {
			if !fi.Type().IsRegular() {
				continue
			}
			info, ierr := fi.Info()
			if ierr != nil {
				continue
			}
			totals.CacheBlobCount++
			totals.CacheBlobBytes += uint64(info.Size())
		}
	}
	return out, totals, nil
}

// Verify streams the cached blob through SHA-256 against its expected
// digest identifier (integrity against a fixed expected value only,
// §10.1) and returns the observed byte length.
func (s *Store) Verify(hexDigest string) (uint64, error) {
	if err := checkHexDigest(hexDigest); err != nil {
		return 0, err
	}
	path := s.BlobPath(hexDigest)
	f, err := os.Open(path)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, missingErr("blob %s is not present in the cache", digestPrefix+hexDigest)
		}
		return 0, readErr("blob open failed: %v", err)
	}
	defer f.Close()
	h := sha256.New()
	n, cerr := io.Copy(h, f)
	if cerr != nil {
		return 0, readErr("blob read failed: %v", cerr)
	}
	got := hex.EncodeToString(h.Sum(nil))
	if got != hexDigest {
		return uint64(n), &persist.Error{Category: persist.CatChecksumError, Stage: persist.StageDigest,
			Detail: "cached blob fails digest verification against " + digestPrefix + hexDigest +
				"; digest mismatch is treated as missing (§10.6)"}
	}
	return uint64(n), nil
}

// Remove deletes an unpinned asset (blob and descriptor) from the cache.
// Pinned blobs refuse removal fail-closed: GC correctness relies on pins
// covering every root (§10.7 G4), so manual protection wins over explicit
// removal until every pin names another owner or is released elsewhere.
func (s *Store) Remove(hexDigest string) error {
	if err := checkHexDigest(hexDigest); err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()

	pins, anomaly, err := s.loadPinStoreLocked()
	if err != nil {
		return err
	}
	if anomaly != nil {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageRecovery,
			Detail: "pin metadata anomaly (" + anomaly.Kind + "); removal refused fail-closed until repair"}
	}
	if owners := pins.pinnersOf(hexDigest); len(owners) > 0 {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
			Detail: "blob " + digestPrefix + hexDigest + " is pinned by " + strings.Join(owners, ", ") +
				"; release or rebuild the pins before removal"}
	}
	if !s.HasBlob(hexDigest) {
		return missingErr("blob %s is not present in the cache", digestPrefix+hexDigest)
	}
	if err := os.Remove(s.BlobPath(hexDigest)); err != nil {
		return readErr("blob removal failed: %v", err)
	}
	if err := os.Remove(s.DescPath(hexDigest)); err != nil && !os.IsNotExist(err) {
		return readErr("descriptor removal failed: %v", err)
	}
	return nil
}

// ---------------------------------------------------------------------------
// small helpers

type countingWriter struct {
	w       io.Writer
	h       interface{ Write([]byte) (int, error) }
	limit   uint64
	n       uint64
	limited bool
}

type limitExceededError struct{ observed uint64 }

func (e *limitExceededError) Error() string { return "stream exceeded its admission cap" }

func (c *countingWriter) Write(p []byte) (int, error) {
	if c.n+uint64(len(p)) > c.limit {
		c.limited = true
		// Consume nothing further: reject before admitting any surplus byte.
		return 0, &limitExceededError{observed: c.n}
	}
	nw, werr := c.w.Write(p)
	if nw > 0 {
		c.n += uint64(nw)
		if c.h != nil {
			_, _ = c.h.Write(p[:nw])
		}
	}
	return nw, werr
}

func writeFileAtomic(tmp, dest string, data []byte) error {
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return err
	}
	if err := os.Rename(tmp, dest); err != nil {
		os.Remove(tmp)
		return err
	}
	return nil
}

func randSuffix() string {
	var b [8]byte
	if _, err := rand.Read(b[:]); err != nil {
		binary.BigEndian.PutUint64(b[:], uint64(time.Now().UnixNano()))
	}
	return hex.EncodeToString(b[:])
}

func itoaUint(v uint64) string {
	if v == 0 {
		return "0"
	}
	var buf [20]byte
	i := len(buf)
	for v > 0 {
		i--
		buf[i] = byte('0' + v%10)
		v /= 10
	}
	return string(buf[i:])
}

func missingErr(format string, args ...any) error {
	return &persist.Error{Category: persist.CatChecksumError, Stage: persist.StageDigest,
		Detail: sprintf(format, args...)}
}

func readErr(format string, args ...any) error {
	return &persist.Error{Category: persist.CatMalformedContainer, Stage: persist.StageEntry,
		Detail: sprintf(format, args...)}
}

type descList []struct {
	hex  string
	desc *Descriptor
}

func sortByHex(entries descList) {
	for i := 1; i < len(entries); i++ {
		for j := i; j > 0 && entries[j].hex < entries[j-1].hex; j-- {
			entries[j], entries[j-1] = entries[j-1], entries[j]
		}
	}
}

func sprintf(format string, args ...any) string {
	return fmt.Sprintf(format, args...)
}
