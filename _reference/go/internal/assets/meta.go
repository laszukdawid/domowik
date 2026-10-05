package assets

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"domowik.dev/domowik/internal/persist"
)

// meta.go: the durable pin store of ASSET-STRATEGY §10.7. Records carry
// checksums and the store carries a monotonic generation counter;
// startup verifies both. Any anomaly — checksum failure, generation
// gap/regression against the next-gen expectation marker, truncation,
// unparsable record — puts the collector into fail-closed mode with
// ZERO collection until a validated repair (§10.7 fail-closed rule).
//
// Write ordering realizes G1 (a digest becomes collectable only after
// its last protecting record is durably removed; crashes yield
// over-retention): every mutation first durably places the next-gen
// expectation marker, then replaces store.json atomically. A crash in
// between leaves the marker ahead of the store, which startup classifies
// as a regression anomaly — the safe direction.

const (
	recordKindPin = "pin"

	ownerRebuild = "rebuild"

	rootKindSnapshot          = "snapshot"
	rootKindOperationFrontier = "operation-frontier"
	rootKindRootTable         = "root-table"
	rootKindManual            = "manual"
)

func validRootKind(k string) bool {
	switch k {
	case rootKindSnapshot, rootKindOperationFrontier, rootKindRootTable, rootKindManual:
		return true
	}
	return false
}

type pinRecord struct {
	Kind      string `json:"kind"`
	Digest    string `json:"digest"` // full sha256: identifier
	OwnerID   string `json:"ownerId"`
	RootKind  string `json:"rootKind"`
	CreatedAt string `json:"createdAt"`
	Checksum  string `json:"checksum"`
}

func (r *pinRecord) hexDigest() string { return strings.TrimPrefix(r.Digest, digestPrefix) }

// canonical returns the exact byte sequence the record checksum covers.
func (r *pinRecord) canonical() []byte {
	return []byte(strings.Join([]string{r.Kind, r.Digest, r.OwnerID, r.RootKind, r.CreatedAt}, "\x00"))
}

func (r *pinRecord) computeChecksum() string {
	sum := sha256.Sum256(r.canonical())
	return digestPrefix + hex.EncodeToString(sum[:])
}

func (r *pinRecord) valid() bool {
	if r.Kind != recordKindPin || !validRootKind(r.RootKind) || r.OwnerID == "" {
		return false
	}
	if err := persist.ValidateDigestID(r.Digest); err != nil {
		return false
	}
	if terr := persist.ValidateTimestamp(r.CreatedAt); terr != nil {
		return false
	}
	return r.Checksum == r.computeChecksum()
}

type pinStoreState struct {
	Generation int64       `json:"generation"`
	Records    []pinRecord `json:"records"`
}

// Anomaly describes one fail-closed metadata finding.
type Anomaly struct {
	Kind   string // checksum | record | unparsable | gap | regression | io
	Detail string
}

// pinView is the verified outcome of loading the metadata store.
type pinView struct {
	state    *pinStoreState // nil when anomaly != nil
	anomaly  *Anomaly
	present  bool // store.json exists
	rawState []byte
}

// loadPinStoreLocked loads and fully verifies the metadata store. The
// caller must hold s.mu. A fresh cache (no files at all) yields an empty
// generation-0 state with no anomaly; any PRESENT but defective file is
// an anomaly, never silently ignored.
func (s *Store) loadPinStoreLocked() (*pinView, *Anomaly, error) {
	v := &pinView{}
	raw, err := os.ReadFile(s.storePath())
	if err != nil {
		if !os.IsNotExist(err) {
			v.anomaly = &Anomaly{Kind: "io", Detail: err.Error()}
			return v, v.anomaly, nil
		}
		if _, gerr := os.Stat(s.nextGenPath()); gerr == nil {
			// Expectation marker without a store: the very first mutation
			// died between marker and store placement. Fail closed; the
			// rebuild path recovers conservatively.
			v.anomaly = &Anomaly{Kind: "regression",
				Detail: "generation marker exists without a pin store"}
			return v, v.anomaly, nil
		}
		v.state = &pinStoreState{}
		return v, nil, nil
	}
	v.present = true
	v.rawState = raw

	st := &pinStoreState{}
	dec := json.NewDecoder(strings.NewReader(string(raw)))
	if jerr := dec.Decode(st); jerr != nil {
		v.anomaly = &Anomaly{Kind: "unparsable", Detail: "pin store is truncated or unparsable: " + jerr.Error()}
		return v, v.anomaly, nil
	}
	if st.Records == nil && len(raw) > 0 && strings.Contains(string(raw), "\"records\"") {
		// records explicitly null: treat as unparsable shape
		v.anomaly = &Anomaly{Kind: "unparsable", Detail: "records member must be an array"}
		return v, v.anomaly, nil
	}
	for i := range st.Records {
		if !st.Records[i].valid() {
			v.anomaly = &Anomaly{Kind: "checksum",
				Detail: sprintf("record %d fails its checksum or field validation", i)}
			return v, v.anomaly, nil
		}
	}

	expectRaw, gerr := os.ReadFile(s.nextGenPath())
	switch {
	case gerr == nil:
		expect, perr := strconv.ParseInt(strings.TrimSpace(string(expectRaw)), 10, 64)
		if perr != nil {
			v.anomaly = &Anomaly{Kind: "unparsable", Detail: "next-generation marker is not decimal"}
			return v, v.anomaly, nil
		}
		switch {
		case st.Generation < expect:
			v.anomaly = &Anomaly{Kind: "regression",
				Detail: sprintf("store generation %d precedes expected %d; a prior mutation was interrupted", st.Generation, expect)}
			return v, v.anomaly, nil
		case st.Generation > expect:
			v.anomaly = &Anomaly{Kind: "gap",
				Detail: sprintf("store generation %d jumps past expected %d", st.Generation, expect)}
			return v, v.anomaly, nil
		}
	case os.IsNotExist(gerr):
		// First boot on a pre-marker store: acceptable only for
		// generation 0 stores written before any mutation; a mutated
		// store always has a marker. Generation > 0 without a marker is
		// therefore an anomaly.
		if st.Generation != 0 {
			v.anomaly = &Anomaly{Kind: "gap",
				Detail: sprintf("mutated store generation %d lacks its expectation marker", st.Generation)}
			return v, v.anomaly, nil
		}
	default:
		v.anomaly = &Anomaly{Kind: "io", Detail: gerr.Error()}
		return v, v.anomaly, nil
	}

	v.state = st
	return v, nil, nil
}

// mutatePinStoreLocked applies fn to the verified state and persists it
// atomically with the two-phase generation protocol. Callers hold s.mu.
func (s *Store) mutatePinStoreLocked(v *pinView, fn func(*pinStoreState)) error {
	if err := refuseOnAnomaly(v); err != nil {
		return err
	}
	next := *v.state
	next.Generation = v.state.Generation + 1
	next.Records = append([]pinRecord{}, v.state.Records...)
	fn(&next)

	// Phase 1: durably publish the expectation for the new generation.
	markerTmp := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	if err := os.WriteFile(markerTmp, []byte(strconv.FormatInt(next.Generation, 10)), 0o644); err != nil {
		return writeFail("marker staging failed: %v", err)
	}
	if err := syncRename(markerTmp, s.nextGenPath()); err != nil {
		return writeFail("marker placement failed: %v", err)
	}
	if s.crashAfterMarker != nil {
		s.crashAfterMarker()
	}

	// Phase 2: replace the store atomically.
	storeTmp := filepath.Join(s.tmpPath(), importPrefix+randSuffix())
	raw, merr := json.Marshal(&next)
	if merr != nil {
		return writeFail("pin store serialization failed: %v", merr)
	}
	if err := os.WriteFile(storeTmp, raw, 0o644); err != nil {
		return writeFail("pin store staging failed: %v", err)
	}
	if err := syncRename(storeTmp, s.storePath()); err != nil {
		return writeFail("pin store replacement failed: %v", err)
	}
	return nil
}

// refuseOnAnomaly converts a failed-closed metadata load into a
// mutation refusal. Every metadata mutation must call this before
// touching v.state: an anomalous store yields a nil state and any
// dereference would be undefined behavior rather than a safe refusal.
func refuseOnAnomaly(v *pinView) error {
	if v.anomaly != nil {
		return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageRecovery,
			Detail: "pin metadata anomaly (" + v.anomaly.Kind + "); refusing mutation until repair: " + v.anomaly.Detail}
	}
	return nil
}

// AddPin records one durable protection root. Duplicate identical pins
// are idempotent; a conflicting redefinition of the same (digest, owner)
// pair refuses.
func (s *Store) AddPin(hexDigest, ownerID, rootKind, createdAt string) error {
	if err := checkHexDigest(hexDigest); err != nil {
		return err
	}
	if ownerID == "" {
		return invalidField("pin ownerId must be non-empty")
	}
	if !validRootKind(rootKind) {
		return invalidField("rootKind must be snapshot, operation-frontier, root-table, or manual")
	}
	if createdAt == "" {
		createdAt = nowTimestampUTC()
	} else if terr := persist.ValidateTimestamp(createdAt); terr != nil {
		return terr
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	v, _, err := s.loadPinStoreLocked()
	if err != nil {
		return err
	}
	if aerr := refuseOnAnomaly(v); aerr != nil {
		return aerr
	}
	for _, r := range v.state.Records {
		if r.hexDigest() == hexDigest && r.OwnerID == ownerID {
			if r.RootKind != rootKind {
				return invalidField("digest %s%s already pinned by %s with rootKind %s",
					digestPrefix, hexDigest, ownerID, r.RootKind)
			}
			return nil // idempotent set semantics
		}
	}
	rec := pinRecord{
		Kind:      recordKindPin,
		Digest:    digestPrefix + hexDigest,
		OwnerID:   ownerID,
		RootKind:  rootKind,
		CreatedAt: createdAt,
	}
	rec.Checksum = rec.computeChecksum()
	return s.mutatePinStoreLocked(v, func(st *pinStoreState) {
		st.Records = append(st.Records, rec)
	})
}

// ReleasePin removes the named owner's pin for a digest. Releasing a
// nonexistent pin refuses rather than pretend success.
func (s *Store) ReleasePin(hexDigest, ownerID string) error {
	if err := checkHexDigest(hexDigest); err != nil {
		return err
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	v, _, err := s.loadPinStoreLocked()
	if err != nil {
		return err
	}
	if aerr := refuseOnAnomaly(v); aerr != nil {
		return aerr
	}
	found := -1
	for i := range v.state.Records {
		r := &v.state.Records[i]
		if r.hexDigest() == hexDigest && r.OwnerID == ownerID {
			if found >= 0 {
				return invalidField("duplicate pin records for %s%s by %s", digestPrefix, hexDigest, ownerID)
			}
			found = i
		}
	}
	if found < 0 {
		return missingErr("no pin by %s covers %s%s", ownerID, digestPrefix, hexDigest)
	}
	return s.mutatePinStoreLocked(v, func(st *pinStoreState) {
		st.Records = append(st.Records[:found], st.Records[found+1:]...)
	})
}

func (p *pinView) pinnersOf(hexDigest string) []string {
	var owners []string
	if p.state == nil {
		return nil
	}
	for _, r := range p.state.Records {
		if r.hexDigest() == hexDigest {
			owners = append(owners, r.OwnerID)
		}
	}
	return owners
}

var timeNowUTC = func() time.Time { return time.Now().UTC() }

func nowTimestampUTC() string {
	return timeNowUTC().Format("2006-01-02T15:04:05Z")
}

func writeFail(format string, args ...any) error {
	return &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
		Detail: sprintf(format, args...)}
}

// syncRename stages-then-renames with a content sync so the expectation
// marker is durable before the store replacement it anticipates.
func syncRename(tmp, dest string) error {
	if f, err := os.Open(tmp); err == nil {
		_ = f.Sync()
		f.Close()
	}
	if err := os.Rename(tmp, dest); err != nil {
		os.Remove(tmp)
		return err
	}
	return nil
}
