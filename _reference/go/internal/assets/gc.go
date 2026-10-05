package assets

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"domowik.dev/domowik/internal/persist"
)

// gc.go: cache garbage collection under ASSET-STRATEGY §10.7.
//
//	G1  a digest is collectable only once its last protecting record is
//	    durably gone (the two-phase metadata protocol yields
//	    over-retention on crash, never premature deletion);
//	G2  admission and collection are mutually atomic against the
//	    metadata store (single in-process writer, see the package scope
//	    boundary);
//	G3  collection never inspects, decodes, or rewrites content — it
//	    unlinks namespace entries by digest alone;
//	G4  correctness never rides on GC: every durable root holds a pin,
//	    so collecting exactly unpinned digests cannot destroy referenced
//	    data, and any metadata anomaly stops all collection fail-closed.

// GCResult reports one collector run.
type GCResult struct {
	FailedClosed bool   // true: an anomaly was present; nothing was collected
	AnomalyKind  string // empty when FailedClosed is false
	AnomalyText  string
	Protected    int      // pinned digests observed at run start
	Collected    []string // full digest identifiers removed this run
	// SweptStaging counts abandoned import-staging files removed from
	// tmp/ this run (process death mid-stream leaves them outside every
	// namespace). Sweeping runs only after the metadata anomaly gate, so
	// a fail-closed run sweeps nothing; only regular files whose names
	// carry the staging prefix are removed — never any blob or descriptor.
	SweptStaging int
}

// collectible describes one cache entry considered for collection.
type collectible struct {
	hex       string
	blobPath  string
	descPath  string
	hasBlob   bool
	hasDesc   bool
	pinned    bool
	dirtySize uint64 // blob bytes freed when collected
}

// Collect runs one garbage-collection pass. On any metadata anomaly it
// returns a FailedClosed result having deleted NOTHING; repair via
// Repair is the only route back to active collection.
func (s *Store) Collect() (*GCResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	v, anomaly, err := s.loadPinStoreLocked()
	if err != nil {
		return nil, err
	}
	if anomaly != nil {
		return &GCResult{FailedClosed: true, AnomalyKind: anomaly.Kind, AnomalyText: anomaly.Detail}, nil
	}
	res := &GCResult{}
	s.sweepStagingLocked(res)

	protected := map[string]bool{}
	for _, r := range v.state.Records {
		protected[r.hexDigest()] = true
	}

	entries := map[string]*collectible{}
	blobRoot := filepath.Join(s.root, layoutBlob, "sha256")
	shards, serr := os.ReadDir(blobRoot)
	if serr != nil && !os.IsNotExist(serr) {
		return nil, readErr("blob enumeration failed: %v", serr)
	}
	for _, shard := range shards {
		if !shard.IsDir() || len(shard.Name()) != 2 {
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
			name := fi.Name()
			if !fi.Type().IsRegular() || len(shard.Name())+len(name) != hexDigestLen {
				continue // foreign file: leave untouched, never delete blind
			}
			e := s.entryFor(shard.Name()+name, protected)
			e.hasBlob = true
			if info, ierr := fi.Info(); ierr == nil {
				e.dirtySize = uint64(info.Size())
			}
			entries[e.hex] = e
		}
	}
	descRoot := filepath.Join(s.root, layoutDesc, "sha256")
	shards, serr = os.ReadDir(descRoot)
	if serr != nil && !os.IsNotExist(serr) {
		return nil, readErr("descriptor enumeration failed: %v", serr)
	}
	for _, shard := range shards {
		if !shard.IsDir() || len(shard.Name()) != 2 {
			continue
		}
		files, ferr := os.ReadDir(filepath.Join(descRoot, shard.Name()))
		if ferr != nil {
			continue
		}
		for _, fi := range files {
			name := strings.TrimSuffix(fi.Name(), descSuffix)
			if name == fi.Name() || len(shard.Name())+len(name) != hexDigestLen {
				continue
			}
			hexDigest := shard.Name() + name
			e := s.entryFor(hexDigest, protected)
			e.hasDesc = fi.Type().IsRegular()
			if prev, ok := entries[hexDigest]; ok {
				prev.hasDesc = e.hasDesc
			} else {
				entries[hexDigest] = e
			}
		}
	}

	res.Protected = len(protected)
	var order []string
	for hex, e := range entries {
		if e.pinned {
			continue
		}
		order = append(order, hex)
	}
	sort.Strings(order)
	for _, hex := range order {
		e := entries[hex]
		if e.hasBlob {
			if rerr := os.Remove(e.blobPath); rerr != nil && !os.IsNotExist(rerr) {
				return res, readErr("collect: blob unlink failed for %s%s: %v", digestPrefix, hex, rerr)
			}
		}
		if e.hasDesc {
			if rerr := os.Remove(e.descPath); rerr != nil && !os.IsNotExist(rerr) {
				return res, readErr("collect: descriptor unlink failed for %s%s: %v", digestPrefix, hex, rerr)
			}
		}
		res.Collected = append(res.Collected, digestPrefix+hex)
	}
	return res, nil
}

func (s *Store) entryFor(hexDigest string, protected map[string]bool) *collectible {
	return &collectible{
		hex:      hexDigest,
		blobPath: s.BlobPath(hexDigest),
		descPath: s.DescPath(hexDigest),
		pinned:   protected[hexDigest],
	}
}

// sweepStagingLocked removes abandoned import-staging files from tmp/.
// Callers hold s.mu and must have passed the metadata anomaly gate: a
// fail-closed collector deletes nothing, staging files included. Only
// regular files directly under tmp/ whose names carry the staging prefix
// are removed; nothing is traversed recursively and no digest-keyed
// namespace entry is ever a candidate (G3: content-blind by name alone).
func (s *Store) sweepStagingLocked(res *GCResult) {
	entries, err := os.ReadDir(s.tmpPath())
	if err != nil {
		return // a missing or unreadable tmp directory has nothing to sweep
	}
	for _, fi := range entries {
		if !fi.Type().IsRegular() || !strings.HasPrefix(fi.Name(), importPrefix) {
			continue
		}
		if rerr := os.Remove(filepath.Join(s.tmpPath(), fi.Name())); rerr == nil {
			res.SweptStaging++
		}
	}
}

// RepairResult reports one validated rebuild.
type RepairResult struct {
	WasAnomalous bool
	KeptRecords  int
	// ConservativePins counts cached blobs re-pinned by the rebuild
	// because some prior record was unreadable (retained-until-repair:
	// their digests could no longer be proven unprotected).
	ConservativePins int
	Resumed          bool
}

// Repair rebuilds the pin store to a clean validated state so collection
// can resume (§10.7). Checksum-valid records survive verbatim. If ANY
// record or file was unreadable, the rebuild re-pins the conservative
// superset — EVERY cached blob becomes manually pinned by owner
// "rebuild" — because unreadable records' previously pinned digests stay
// protected until repair can prove otherwise. The rebuilt generation
// continues monotonically past whatever was recoverable.
func (s *Store) Repair() (*RepairResult, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	v, anomaly, err := s.loadPinStoreLocked()
	if err != nil {
		return nil, err
	}
	res := &RepairResult{WasAnomalous: anomaly != nil}

	baseGen := int64(0)
	if v.state != nil && v.state.Generation > baseGen {
		baseGen = v.state.Generation
	}

	next := pinStoreState{Generation: baseGen + 1, Records: []pinRecord{}}
	if v.state != nil {
		for _, r := range v.state.Records {
			if r.valid() {
				next.Records = append(next.Records, r)
			}
		}
	}
	res.KeptRecords = len(next.Records)

	if res.WasAnomalous {
		conservative := 0
		for _, hex := range s.cachedBlobDigests() {
			rec := pinRecord{
				Kind:      recordKindPin,
				Digest:    digestPrefix + hex,
				OwnerID:   ownerRebuild,
				RootKind:  rootKindManual,
				CreatedAt: nowTimestampUTC(),
			}
			rec.Checksum = rec.computeChecksum()
			next.Records = append(next.Records, rec)
			conservative++
		}
		res.ConservativePins = conservative
	}

	raw, merr := json.Marshal(&next)
	if merr != nil {
		return nil, writeFail("rebuild serialization failed: %v", merr)
	}
	markerTmp := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	if werr := os.WriteFile(markerTmp, []byte(itoaInt64(next.Generation)), 0o644); werr != nil {
		return nil, writeFail("rebuild marker staging failed: %v", werr)
	}
	if rerr := syncRename(markerTmp, s.nextGenPath()); rerr != nil {
		return nil, writeFail("rebuild marker placement failed: %v", rerr)
	}
	storeTmp := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	if werr := os.WriteFile(storeTmp, raw, 0o644); werr != nil {
		return nil, writeFail("rebuild staging failed: %v", werr)
	}
	if rerr := syncRename(storeTmp, s.storePath()); rerr != nil {
		return nil, writeFail("rebuild replacement failed: %v", rerr)
	}

	// Validate the rebuild with a fresh load before declaring collection
	// resumed (collection resumes only after a clean validated rebuild).
	check, checkAnomaly, cerr := s.loadPinStoreLocked()
	if cerr != nil {
		return nil, cerr
	}
	if checkAnomaly != nil || check.state == nil || check.state.Generation != next.Generation {
		return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageRecovery,
			Detail: "rebuilt pin store failed validation; collection stays stopped"}
	}
	res.Resumed = true
	return res, nil
}

func (s *Store) cachedBlobDigests() []string {
	var out []string
	blobRoot := filepath.Join(s.root, layoutBlob, "sha256")
	shards, err := os.ReadDir(blobRoot)
	if err != nil {
		return nil
	}
	for _, shard := range shards {
		if !shard.IsDir() || len(shard.Name()) != 2 {
			continue
		}
		files, ferr := os.ReadDir(filepath.Join(blobRoot, shard.Name()))
		if ferr != nil {
			continue
		}
		for _, fi := range files {
			if fi.Type().IsRegular() && len(shard.Name())+len(fi.Name()) == hexDigestLen {
				out = append(out, shard.Name()+fi.Name())
			}
		}
	}
	sort.Strings(out)
	return out
}

func itoaInt64(v int64) string {
	return strconv.FormatInt(v, 10)
}
