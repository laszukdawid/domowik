package persist

import (
	"crypto/sha256"
	"io"
	"regexp"
	"strings"
	"time"

	"domowik.dev/domowik/internal/document"
)

// reader.go: the single-pass Load pipeline. Ordering follows §9.5:
// physical structure, then names/profile, then manifest schema gate,
// then per-entry streaming under the guards, then digest arbitration,
// then semantic decoding, then atomic framing checks, then replay.

// PackageView is the outcome of a successful load. Degradation classes
// surface here rather than as failures: digest-mismatched blobs are
// missing-asset equivalents, unverifiable optional previews are
// excluded, and the projection always reflects the delivered set.
type PackageView struct {
	FormatVersion string
	Profile       string
	AssetRoot     []string

	Projection *document.Projection
	Retained   []*document.Batch

	DescriptorsRaw []byte
	Blobs          map[string][]byte // hex digest -> verified bytes
	Preview        map[string][]byte // logical name -> raw bytes (optional)

	// Preservation bookkeeping (U2/L32): unknown OPTIONAL members this
	// schema tolerates on read but cannot re-serialize. A non-empty list
	// forces safe-write refusal on write-back; silent stripping is
	// forbidden everywhere.
	ManifestUnknown []string
	SnapshotUnknown []string
	BatchUnknown    map[string][]string // ops entry name -> unknown members

	MissingAssets []string // digest identifiers treated as missing blobs
	Diagnostics   []string

	// arch backs StreamEntry: bounded sequential extraction of any
	// single entry without whole-entry buffering (§7.1 C1/C2/C6).
	arch     *archive
	manifest *Manifest

	// keepOpen holds the file handle of a LoadFile-opened view so its
	// entry streams remain readable after Load returns; Close releases
	// it. In-memory Load views leave this nil.
	keepOpen io.Closer
}

// Close releases the backing input of a file-loaded view. Streaming
// surfaces (StreamEntry) are unusable afterwards. Safe to call on any
// view; safe to call more than once.
func (v *PackageView) Close() error {
	if v == nil || v.keepOpen == nil {
		return nil
	}
	err := v.keepOpen.Close()
	v.keepOpen = nil
	return err
}

// EntryStream is a guarded logical stream over one INVENTORIED package
// entry. Bytes are RAW AND UNTRUSTED until Verified() returns nil:
// CRC-32 and SHA-256 accumulate during reading against the manifest
// inventory, all §9.3 guards apply incrementally before bytes are
// yielded, and finish() enforces declared-vs-observed length, CRC-32,
// and the inventory SHA-256 (digest-before-decode/admission, §9.5) at
// end of stream. Callers MUST NOT decode, admit, or otherwise consume
// the semantic content of a stream whose Verified verdict is non-nil or
// that was abandoned before EOF (C6: abandonment admits nothing).
type EntryStream struct {
	*entryStream
}

// Verified reports the end-of-stream integrity verdict: nil only after
// the stream reached EOF with matching declared length, CRC-32, and
// manifest inventory SHA-256.
func (s *EntryStream) Verified() error {
	if !s.finished {
		return fail(CatChecksumError, StageDigest, "stream not drained to EOF")
	}
	if s.entryStream.err != nil {
		return s.entryStream.err
	}
	return nil
}

// StreamEntry opens one entry for bounded sequential extraction located
// via the central directory (C1); decompressed bytes hash incrementally
// outside container compression (C2); all L1–L5/L15/L16 guards apply
// with cancellation safety — abandoning the stream mid-way admits
// nothing anywhere (C6). Only entries listed in the manifest inventory
// are openable; the row-L5 admission cap binds blob-scope entries during
// streaming exactly as in Load.
func (v *PackageView) StreamEntry(name string) (*EntryStream, error) {
	if v.arch == nil || v.manifest == nil {
		return nil, fail(CatMalformedContainer, StageEntry, "view has no backing input")
	}
	ns, nerr := classifyNamespace(name)
	if nerr != nil {
		return nil, nerr
	}
	inv, listed := v.manifest.Entries[name]
	if !listed {
		return nil, fail(CatChecksumError, StageDigest,
			"entry %q is not listed in the manifest inventory; refusing raw access", name)
	}
	for _, e := range v.arch.entries {
		if e.name != name {
			continue
		}
		st, oerr := v.arch.openEntry(e)
		if oerr != nil {
			return nil, oerr
		}
		st.extraCap = scopeCap(ns)
		st.expectedSHA = inv.SHA256
		return &EntryStream{entryStream: st}, nil
	}
	return nil, fail(CatMissingManifest, StageNames, "no entry %q", name)
}

const (
	manifestClosedMembers = "\x00formatVersion\x00profile\x00assetRoot\x00entries"
	snapshotClosedMembers = "\x00entities\x00frontier\x00batchPositions\x00winnerPositions\x00createdPositions\x00tombstones\x00batchMetadata" +
		// Pre-ED-0037 members stay in the CLOSED set: a legacy snapshot is
		// readable under ED-0038, so its members are known rather than
		// unknown-optional data (§8.4).
		"\x00winnerKeys\x00createdKeys"

	// minInventoryRecordBytes restates the L7 computed-reserve unit for
	// the manifest inventory: sha256 identifier plus quoted key plus
	// byteLength overhead. Reservation derives from entry counts times
	// this minimum, never from declared lengths (C4).
	minInventoryRecordBytes = 200
)

// scannedPackage is the shared outcome of the pipeline prefix both Load
// and the parse-only inspection seam (inspect.go) consume: everything
// through container, names/namespace, manifest, limit, digest, and
// structural batch-framing stages, before any semantic decoding.
type scannedPackage struct {
	view           *PackageView
	arch           *archive
	manifest       *Manifest
	snapshotEntry  *entryInfo
	snapshotInv    InventoryEntry
	snapshotRaw    []byte
	descriptorsRaw []byte
	batchDocs      map[string]*batchDoc
	batchOrder     []string
}

// Load parses and validates one package from an in-memory image.
func Load(data []byte) (*PackageView, error) {
	return loadArchive(newByteReaderAt(data), int64(len(data)))
}

// LoadFile opens and loads one package from a path. The returned view
// keeps the file open for StreamEntry until Close is called.
func LoadFile(path string) (*PackageView, error) {
	f, err := openFileReadOnly(path)
	if err != nil {
		return nil, err
	}
	st, statErr := f.Stat()
	if statErr != nil {
		f.Close()
		return nil, fail(CatMalformedContainer, StageContainer, "unstatable input: %v", statErr)
	}
	if !st.Mode().IsRegular() {
		f.Close()
		return nil, fail(CatMalformedContainer, StageContainer, "target is not a regular file")
	}
	view, lerr := loadArchive(f, st.Size())
	if lerr != nil {
		f.Close()
		return nil, lerr
	}
	view.keepOpen = f
	return view, nil
}

// scanArchive runs the shared pipeline prefix — container, logical
// names and namespace classification, manifest schema and V3 gate,
// limit guards, per-entry streaming with digest arbitration, inventory
// coverage, and structural batch framing — without any semantic
// decoding. Both Load and the parse-only inspection seam consume it.
func scanArchive(src io.ReaderAt, size int64) (*scannedPackage, *Error) {
	a, ferr := openArchive(src, size)
	if ferr != nil {
		return nil, ferr
	}

	// Names: validate every logical name, then collisions, then the
	// closed namespace set — in the precedence order the oracle pins.
	names := make([]string, 0, len(a.entries))
	byNS := map[namespace][]*entryInfo{}
	for _, e := range a.entries {
		if verr := validateLogicalName(e.name); verr != nil {
			return nil, verr
		}
		names = append(names, e.name)
	}
	if cerr := classifyCollisions(names); cerr != nil {
		return nil, cerr
	}
	manifestCount := 0
	for _, e := range a.entries {
		ns, nerr := classifyNamespace(e.name)
		if nerr != nil {
			return nil, nerr
		}
		if ns == nsManifest {
			manifestCount++
		}
		byNS[ns] = append(byNS[ns], e)
	}
	switch {
	case manifestCount == 0:
		return nil, fail(CatMissingManifest, StageManifest, "no domowik.json entry")
	case manifestCount > 1:
		return nil, fail(CatAmbiguousManifest, StageManifest, "%d manifest entries", manifestCount)
	}

	view := &PackageView{
		Blobs:         map[string][]byte{},
		Preview:       map[string][]byte{},
		BatchUnknown:  map[string][]string{},
		MissingAssets: []string{},
		Diagnostics:   []string{},
	}

	// Manifest first: locate, stream under guards, verify digest, parse.
	manifestEntry := byNS[nsManifest][0]
	reserve := uint64(len(a.entries)) * minInventoryRecordBytes
	if reserve > LimitManifestReserveBytes {
		return nil, fail(CatLimitManifestReserve, StageManifest,
			"computed manifest reserve %d exceeds %d (L7)", reserve, LimitManifestReserveBytes)
	}
	manifestRaw, ferr := a.readVerified(manifestEntry, 0)
	if ferr != nil {
		return nil, ferr
	}
	mroot, perr := parseLimitedJSON(manifestRaw)
	if perr != nil {
		return nil, perr
	}
	manifest, merr := parseManifest(mroot)
	if merr != nil {
		return nil, merr
	}
	view.FormatVersion = manifest.FormatVersion
	view.Profile = manifest.Profile
	view.AssetRoot = manifest.AssetRoot

	// V3 reader acceptance gate: accept exactly the minors this reader
	// claims; the diagnostic names both versions, guessing is forbidden.
	if manifest.FormatVersion != formatVersionV01 {
		return nil, fail(CatUnsupportedVersion, StageManifest,
			"package schema %s exceeds or mismatches reader-supported schema %s; migration runs only on explicit save/export",
			manifest.FormatVersion, formatVersionV01)
	}
	for _, k := range manifestObjectKeys(mroot) {
		if !strings.Contains(manifestClosedMembers, "\x00"+k) {
			view.ManifestUnknown = append(view.ManifestUnknown, k)
		}
	}

	inventory := func(name string) (InventoryEntry, bool) {
		e, ok := manifest.Entries[name]
		return e, ok
	}

	// Single streaming pass over every remaining entry.
	var snapshotRaw, descriptorsRaw []byte
	batchDocs := map[string]*batchDoc{}
	var batchOrder []string
	for _, e := range a.entries {
		if e.name == manifestEntry.name {
			continue
		}
		ns, _ := classifyNamespace(e.name)
		inv, listed := inventory(e.name)
		raw, serr := a.readVerified(e, scopeCap(ns))
		if serr != nil && serr.Category != CatChecksumError {
			return nil, serr
		}
		if serr != nil { // CRC failed: structural scopes reject, blob scope degrades
			if ns == nsBlob || ns == nsPreviewData {
				view.MissingAssets = append(view.MissingAssets, e.name)
				continue
			}
			return nil, serr
		}
		if !listed {
			switch ns {
			case nsBlob:
				view.MissingAssets = append(view.MissingAssets, e.name)
				continue
			case nsPreviewData, nsPreviewIndex:
				view.Diagnostics = append(view.Diagnostics,
					"unlisted preview entry excluded: "+e.name)
				continue
			default:
				return nil, fail(CatChecksumError, StageDigest,
					"structural entry %q is not listed in the manifest inventory", e.name)
			}
		}
		digest := "sha256:" + rawSHA256Hex(raw)
		if digest != inv.SHA256 {
			if ns == nsBlob || ns == nsPreviewData {
				view.MissingAssets = append(view.MissingAssets, e.name)
				continue
			}
			if ns == nsPreviewIndex {
				view.Diagnostics = append(view.Diagnostics,
					"unverifiable preview index excluded: "+e.name)
				continue
			}
			return nil, fail(CatChecksumError, StageDigest,
				"digest mismatch on structural entry %q", e.name)
		}
		if uint64(len(raw)) != inv.ByteLength {
			return nil, fail(CatMalformedLength, StageDigest,
				"inventory byteLength for %q contradicts observed growth (C3)", e.name)
		}
		switch ns {
		case nsSnapshot:
			snapshotRaw = raw
		case nsDescriptors:
			descriptorsRaw = raw
		case nsOps:
			doc, berr := parseBatchDocFraming(raw)
			if berr != nil {
				return nil, berr
			}
			batchDocs[e.name] = doc
			batchOrder = append(batchOrder, e.name)
		case nsBlob:
			view.Blobs[strings.TrimPrefix(inv.SHA256, "sha256:")] = raw
		case nsPreviewData, nsPreviewIndex:
			view.Preview[e.name] = raw
		case nsForeign:
			return nil, fail(CatInvalidEntryName, StageNames, "unreachable foreign namespace")
		}
	}
	sortStrings(batchOrder)

	// Inventory coverage: every listed entry must be physically present.
	// A listed-but-absent structural entry rejects checksum-error; a
	// listed-but-absent blob follows missing-blob handling (§7.4).
	present := map[string]bool{}
	for _, e := range a.entries {
		present[e.name] = true
	}
	for name := range manifest.Entries {
		if present[name] {
			continue
		}
		if ns, _ := classifyNamespace(name); ns == nsBlob || ns == nsPreviewData {
			view.MissingAssets = append(view.MissingAssets, name)
			continue
		}
		return nil, fail(CatChecksumError, StageDigest,
			"manifest lists structural entry %q that is absent from the package", name)
	}

	if len(byNS[nsSnapshot]) == 0 {
		return nil, fail(CatMissingManifest, StageSchema,
			"snapshot/state.json absent; exactly one snapshot section is required (§7.5)")
	}

	snapEntry := byNS[nsSnapshot][0]
	snapInv, _ := inventory(snapEntry.name)
	return &scannedPackage{
		view:           view,
		arch:           a,
		manifest:       manifest,
		snapshotEntry:  snapEntry,
		snapshotInv:    snapInv,
		snapshotRaw:    snapshotRaw,
		descriptorsRaw: descriptorsRaw,
		batchDocs:      batchDocs,
		batchOrder:     batchOrder,
	}, nil
}

func loadArchive(src io.ReaderAt, size int64) (*PackageView, error) {
	sc, serr := scanArchive(src, size)
	if serr != nil {
		return nil, serr
	}
	a := sc.arch
	view := sc.view
	manifest := sc.manifest
	snapshotRaw := sc.snapshotRaw
	descriptorsRaw := sc.descriptorsRaw
	batchDocs := sc.batchDocs
	batchOrder := sc.batchOrder

	// Semantic decode of the snapshot under its L23 metadata ceiling.
	if capErr := checkSnapshotMetadataCeiling(sc.snapshotInv); capErr != nil {
		return nil, capErr
	}
	sroot, perr := parseLimitedJSON(snapshotRaw)
	if perr != nil {
		return nil, perr
	}
	snap, serr2 := parseSnapshot(sroot)
	if serr2 != nil {
		return nil, serr2
	}
	for _, k := range jsonObjectKeys(sroot) {
		if !strings.Contains(snapshotClosedMembers, "\x00"+k) {
			view.SnapshotUnknown = append(view.SnapshotUnknown, k)
		}
	}
	view.DescriptorsRaw = descriptorsRaw
	if len(descriptorsRaw) > 0 {
		if derr := parseDescriptors(descriptorsRaw); derr != nil {
			return nil, derr
		}
	}

	// Framing enumeration BEFORE replay (§7.6): envelopes, name/content
	// binding, duplicate ids, reserved-range structure.
	if len(batchOrder) > LimitBatchesPerPackage {
		return nil, fail(CatLimitOperationCount, StageFraming,
			"batch count exceeds L23")
	}
	totalOps := 0
	seenBatchIDs := map[string]string{}
	rangesByActor := map[string][][2]int64{}
	var batches []*document.Batch
	for _, name := range batchOrder {
		d := batchDocs[name]
		if derr := bindNameToContent(name, d); derr != nil {
			return nil, derr
		}
		if prev := seenBatchIDs[d.batchID]; prev != "" {
			return nil, fail(CatMalformedFraming, StageFraming,
				"duplicate batch id %s in %q and %q", d.batchID, prev, name)
		}
		seenBatchIDs[d.batchID] = name
		ops, oerr := buildBatchOperations(d)
		if oerr != nil {
			return nil, oerr
		}
		totalOps += len(ops)
		if totalOps > LimitTotalRetainedOps {
			return nil, fail(CatLimitOperationCount, StageFraming,
				"total retained operations exceed L23")
		}
		actorEnd := d.base + int64(len(ops)) - 1
		rangesByActor[d.actorID] = append(rangesByActor[d.actorID], [2]int64{d.base, actorEnd})
		b, berr := document.NewBatch(mustUUID(d.batchID), mustUUID(d.actorID), d.base, frontierOf(d), ops...)
		if berr != nil {
			return nil, framingErr(berr)
		}
		batches = append(batches, b)
		if unk, ok := batchDocs[name].unknownMembers(); ok {
			view.BatchUnknown[name] = unk
		}
	}
	// Reserved-range structure is inside the seam's framing scope
	// (§7.6/ED-0032): overlapping or gapped actor ranges fail here
	// before replay, not incidentally downstream. Parity with inspect.go:206.
	if rerr := checkReservedRanges(rangesByActor); rerr != nil {
		return nil, rerr
	}

	// Replay: baseline (snapshot) plus the delivered set, canonical order
	// inside the seam constructor; structural inconsistencies fail
	// closed there too. Retained operations continue from the snapshot
	// frontier (or imply the compacted prefix for actors it does not
	// name); gaps and overlaps reject before any effect.
	// Legacy conversion happens HERE and nowhere earlier: §5.1 makes a
	// legacy tombstone recoverable only from the deleted id's retained
	// creating batch, and the batches exist only at this point in the
	// pipeline (ED-0038).
	snapState := snap.State
	if snap.Legacy != nil {
		converted, cerr := ConvertLegacySnapshot(snap.Legacy, batches)
		if cerr != nil {
			return nil, cerr
		}
		snapState = converted
	}
	proj, rerr := document.ReplayFromSnapshot(snapState, batches)
	if rerr != nil {
		return nil, framingErr(rerr)
	}
	view.Retained = batches
	view.Projection = proj
	view.arch = a
	view.manifest = manifest
	for _, d := range proj.Diagnostics() {
		view.Diagnostics = append(view.Diagnostics, string(d.Code)+": "+d.Detail)
	}
	return view, nil
}

// scopeCap returns the additional per-entry logical cap for a namespace:
// blob entries admit under L5 during streaming digest verification.
func scopeCap(ns namespace) uint64 {
	if ns == nsBlob {
		return LimitBlobBytes
	}
	return 0
}

// readVerified streams one entry under the guards, returning its logical
// bytes. extraCap (when nonzero) is an additional observed-expansion cap
// applied alongside L3 (row L5 for blob admission).
func (a *archive) readVerified(e *entryInfo, extraCap uint64) ([]byte, *Error) {
	st, oerr := a.openEntry(e)
	if oerr != nil {
		return nil, oerr
	}
	st.extraCap = extraCap
	data, rerr := io.ReadAll(st)
	if rerr != nil {
		if fe, ok := rerr.(*Error); ok {
			return nil, fe
		}
		return nil, fail(CatMalformedContainer, StageEntry, "read error: %v", rerr)
	}
	return data, nil
}

var timestampShape = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$`)

// validateTimestamp enforces the L19 RFC 3339 UTC fixed form.
func validateTimestamp(s string) *Error {
	if !timestampShape.MatchString(s) {
		return fail(CatInvalidFieldSyntax, StageSchema,
			"timestamp %q is not RFC 3339 UTC fixed form", s)
	}
	if _, err := time.Parse(time.RFC3339, s); err != nil {
		return fail(CatInvalidFieldSyntax, StageSchema, "timestamp %q invalid: %v", s, err)
	}
	return nil
}

// ---------------------------------------------------------------------------
// small helpers

func newByteReaderAt(b []byte) io.ReaderAt { return &sliceRA{b: b} }

type sliceRA struct{ b []byte }

func (s *sliceRA) ReadAt(p []byte, off int64) (int, error) {
	if off >= int64(len(s.b)) {
		return 0, io.EOF
	}
	n := copy(p, s.b[off:])
	return n, nil
}

func rawSHA256Hex(b []byte) string {
	sum := sha256.Sum256(b)
	const hexdigits = "0123456789abcdef"
	out := make([]byte, len(sum)*2)
	for i, c := range sum {
		out[i*2] = hexdigits[c>>4]
		out[i*2+1] = hexdigits[c&0xf]
	}
	return string(out)
}

func sortStrings(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}

func mustUUID(s string) document.UUID {
	u, err := document.ParseUUID(s)
	if err != nil {
		panic("persist: validated UUID failed to parse: " + s)
	}
	return u
}

func frontierOf(d *batchDoc) map[document.UUID]int64 {
	out := make(map[document.UUID]int64, len(d.frontier))
	for a, n := range d.frontier {
		out[mustUUID(a)] = n
	}
	return out
}
