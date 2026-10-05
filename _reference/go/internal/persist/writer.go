package persist

import (
	"encoding/json"
	"io"

	"domowik.dev/domowik/internal/document"
)

// writer.go: deterministic complete-package assembly (§7.8/§7.10).
// Entries are emitted in ascending UTF-8 byte order of logical names;
// every non-manifest entry is inventoried in the manifest; the asset
// root is regenerated completely (invariant I4); unknown-preserved
// constructs force refusal before any bytes exist (U2/L32).

// ProfileThin and ProfilePortable are the §7.9/§7.4 profile values.
func ProfileThin() string     { return profileThin }
func ProfilePortable() string { return profilePortable }

// BlobEntryName derives the logical blob entry name of a hex digest via
// the ASSET-STRATEGY §10.3 mapping.
func BlobEntryName(hexDigest string) string {
	return "blob/sha256/" + hexDigest[:2] + "/" + hexDigest[2:]
}

// EntryInventory exposes the manifest inventory record of one listed
// logical entry name, so a streaming packer can declare the blob's
// byteLength (C3) before opening its guarded stream via StreamEntry.
func (v *PackageView) EntryInventory(name string) (InventoryEntry, bool) {
	if v == nil || v.manifest == nil {
		return InventoryEntry{}, false
	}
	inv, ok := v.manifest.Entries[name]
	return inv, ok
}

// BlobSource supplies one blob's logical bytes B (ASSET-STRATEGY §10.1)
// as a stream, so packers never buffer whole blobs. Open may be called
// more than once for the same source (the writer runs one bounded CRC
// pre-pass and one emission pass per entry); implementations must return
// an independent stream over the same bytes each time. DeclaredLength
// must equal len(B) exactly; a stream whose observed byte count differs
// from it aborts the write (C3 declared-vs-observed). If the returned
// reader also implements io.Closer, Close is called when the pass ends.
type BlobSource interface {
	DeclaredLength() uint64
	Open() (io.Reader, error)
}

// WriteInput mirrors PackageView for writers: full-knowledge writers
// regenerate the asset-root table completely; partial-knowledge callers
// must refuse instead of rewriting selectively.
type WriteInput struct {
	Profile         string
	State           document.State
	RetainedBatches []*document.Batch
	AssetRoot       []string // digest identifiers (ascending, unique)
	DescriptorsRaw  []byte   // preserved verbatim when unchanged semantics are proven
	Blobs           map[string][]byte
	PreviewEntries  map[string][]byte

	// BlobSources supplies embedded blob content as streams keyed by hex
	// digest (streaming seam; ASSET-STRATEGY §10.4/§10.5, §7.2 C1/C2).
	// Sources and Blobs must not name the same digest. Streamed entries
	// are emitted store-method; their manifest inventory records the
	// content digest and declared length, both re-verified by the S2
	// candidate validation.
	BlobSources map[string]BlobSource

	// RefusalReasons carries U2/L32 preservation findings from the read
	// side (unknown manifest/snapshot members, annotated batch entries).
	// Non-empty refuses the write with a safe-write-refusal diagnostic.
	RefusalReasons []string

	// Baseline records the load-time content digest of the target
	// (BEHAVIOR-SPEC §9.2 external-modification detection). SaveDocument
	// refuses silent overwrite when the target's current bytes differ,
	// unless BaselineConfirmed carries explicit user confirmation (or
	// the caller performs a save-as under a fresh target).
	Baseline          *TargetBaseline
	BaselineConfirmed bool
}

// AssembleRawPackage materializes a thin package from raw logical entry
// bytes (conformance-harness and development surface): the manifest is
// computed over exactly the given entries and an empty snapshot is added
// when absent. Raw bytes are preserved verbatim so readers classify
// content defects, not assembly artifacts.
func AssembleRawPackage(raws map[string][]byte) ([]byte, *Error) {
	names := make([]string, 0, len(raws))
	rawsUsed := map[string][]byte{}
	for name := range raws {
		names = append(names, name)
		rawsUsed[name] = raws[name]
	}
	sortStrings(names)
	var entries []zipWriterEntry
	for _, name := range names {
		if name == "domowik.json" {
			continue // handled verbatim after inventory computation
		}
		// Deliberately NO namespace gate here: this surface exists so
		// conformance harnesses can materialize hostile entries verbatim
		// and let readers classify them.
		entries = append(entries, zipWriterEntry{name: name, data: raws[name], method: methodStore})
	}
	if _, ok := raws["snapshot/state.json"]; !ok {
		entries = append(entries, zipWriterEntry{
			name:   "snapshot/state.json",
			data:   []byte(emptySnapshotDocument),
			method: methodStore,
		})
	}
	delete(rawsUsed, "domowik.json")
	var all []zipWriterEntry
	if provided, ok := raws["domowik.json"]; ok {
		// A caller-provided manifest is used VERBATIM (harness surface);
		// it is never inventoried into itself.
		all = append(entries, zipWriterEntry{name: "domowik.json", data: provided, method: methodStore})
	} else {
		sortEntrySlice(entries)
		manifest, merr := buildManifestJSON(profileThin, nil, entries)
		if merr != nil {
			return nil, merr
		}
		all = append(entries, zipWriterEntry{name: "domowik.json", data: manifest, method: methodDeflate})
	}
	sortEntrySlice(all)
	return buildArchive(all)
}

func BuildPackage(in *WriteInput) ([]byte, *Error) {
	if in.Profile != profileThin && in.Profile != profilePortable {
		return nil, fail(CatInvalidFieldSyntax, StageSave, "profile must be thin or portable")
	}
	if len(in.RefusalReasons) > 0 {
		return nil, fail(CatSafeWriteRefusal, StageSave,
			"unpreservable optional data would be stripped on write-back: %v", in.RefusalReasons)
	}

	// Writer envelope policy (L23/L31 named parameters): a save whose
	// retained-operation count cannot be bounded refuses rather than
	// emit an out-of-envelope package. Compaction to this target is the
	// snapshot-folding model below, so exceeding it indicates a caller
	// contract violation.
	totalOps := 0
	for _, b := range in.RetainedBatches {
		if b == nil || len(b.Operations) == 0 {
			return nil, fail(CatMalformedFraming, StageSave, "retained batch set contains an absent or empty batch")
		}
		totalOps += len(b.Operations)
	}
	if totalOps > LimitTotalRetainedOps || len(in.RetainedBatches) > LimitBatchesPerPackage {
		return nil, fail(CatSafeWriteRefusal, StageSave,
			"retained-operation envelope exceeds L23; compaction required before write")
	}

	entries := make([]zipWriterEntry, 0, 8)

	// Snapshot section: S1–S5 fold the complete current state (§7.5);
	// session undo/redo stacks are device-local and never serialized
	// (§4.8). The frontier written here is the sole frontier of record.
	recentIDs := make([]string, 0, len(in.RetainedBatches))
	for _, b := range in.RetainedBatches {
		recentIDs = append(recentIDs, b.BatchID.String())
	}
	snapshotRaw, serr := buildSnapshotJSON(in.State, recentIDs)
	if serr != nil {
		return nil, serr
	}
	if uint64(len(snapshotRaw)) > LimitSnapshotMetadataByte {
		return nil, fail(CatLimitSnapshotMeta, StageSave,
			"snapshot section exceeds the L23 metadata ceiling")
	}

	batchEntries := make([]zipWriterEntry, 0, len(in.RetainedBatches))
	for _, b := range in.RetainedBatches {
		raw, berr := buildBatchJSON(b)
		if berr != nil {
			return nil, berr
		}
		batchEntries = append(batchEntries, zipWriterEntry{
			name:   batchEntryName(b.CounterBase, b.ActorID.String()),
			data:   raw,
			method: methodStore,
		})
	}

	blobEntries, rootFromBlobs, berr := blobZipEntries(in)
	if berr != nil {
		return nil, berr
	}
	entries = append(entries, blobEntries...)

	if len(in.DescriptorsRaw) > 0 {
		if derr := parseDescriptors(in.DescriptorsRaw); derr != nil {
			return nil, derr
		}
		entries = append(entries, zipWriterEntry{
			name: "assets/descriptors.json", data: in.DescriptorsRaw, method: methodDeflate})
	}
	entries = append(entries, batchEntries...)
	entries = append(entries, zipWriterEntry{
		name: "snapshot/state.json", data: snapshotRaw, method: methodDeflate})

	previewNames := make([]string, 0, len(in.PreviewEntries))
	for name := range in.PreviewEntries {
		previewNames = append(previewNames, name)
	}
	sortStrings(previewNames)
	for _, name := range previewNames {
		ns, nerr := classifyNamespace(name)
		if nerr != nil || (ns != nsPreviewData && ns != nsPreviewIndex) {
			return nil, fail(CatInvalidEntryName, StageSave, "preview entry %q outside its namespace", name)
		}
		entries = append(entries, zipWriterEntry{name: name, data: in.PreviewEntries[name], method: methodStore})
	}

	entries = append(entries, zipWriterEntry{name: "domowik.json"}) // placeholder, replaced below
	sortEntrySlice(entries)

	roots, rerr := mergeAssetRoots(in.AssetRoot, rootFromBlobs)
	if rerr != nil {
		return nil, rerr
	}
	manifestRaw, merr := buildManifestJSON(in.Profile, roots, entries)
	if merr != nil {
		return nil, merr
	}
	for i := range entries {
		if entries[i].name == "domowik.json" {
			entries[i].data = manifestRaw
		}
	}
	return buildArchive(entries)
}

func sortEntrySlice(es []zipWriterEntry) {
	for i := 1; i < len(es); i++ {
		for j := i; j > 0 && es[j].name < es[j-1].name; j-- {
			es[j], es[j-1] = es[j-1], es[j]
		}
	}
}

// blobZipEntries materializes embedded blobs at their §10.3-derived
// paths and returns the digests to fold into the asset-root table.
// Buffered blobs verify before entry creation; streamed blobs carry their
// digest identity structurally and are re-verified end-to-end by the S2
// candidate validation (and by any preflight pass the packer runs).
func blobZipEntries(in *WriteInput) ([]zipWriterEntry, []string, *Error) {
	digests := make([]string, 0, len(in.Blobs)+len(in.BlobSources))
	entries := make([]zipWriterEntry, 0, len(in.Blobs)+len(in.BlobSources))
	for hexDigest, data := range in.Blobs {
		full := "sha256:" + hexDigest
		if verr := validateDigestID(full); verr != nil {
			return nil, nil, verr
		}
		actual := rawSHA256Hex(data)
		if actual != hexDigest {
			return nil, nil, fail(CatSafeWriteRefusal, StageSave,
				"embedded blob %s fails verification; portability claim broken, packing aborted completely",
				hexDigest[:8]+"...")
		}
		digests = append(digests, hexDigest)
		entries = append(entries, zipWriterEntry{
			name:   "blob/sha256/" + hexDigest[:2] + "/" + hexDigest[2:],
			data:   data,
			method: methodStore,
		})
	}
	for hexDigest, src := range in.BlobSources {
		if _, dup := in.Blobs[hexDigest]; dup {
			return nil, nil, fail(CatSafeWriteRefusal, StageSave,
				"blob %s supplied through both buffered and streaming sources", hexDigest[:8]+"...")
		}
		full := "sha256:" + hexDigest
		if verr := validateDigestID(full); verr != nil {
			return nil, nil, verr
		}
		if src == nil {
			return nil, nil, fail(CatSafeWriteRefusal, StageSave,
				"blob source for %s is absent", hexDigest[:8]+"...")
		}
		declared := src.DeclaredLength()
		if declared > LimitBlobBytes {
			return nil, nil, fail(CatLimitBlobBytes, StageSave,
				"declared length %d of blob %s exceeds L5", declared, hexDigest[:8]+"...")
		}
		digests = append(digests, hexDigest)
		entries = append(entries, zipWriterEntry{
			name:      "blob/sha256/" + hexDigest[:2] + "/" + hexDigest[2:],
			method:    methodStore,
			src:       src,
			declared:  declared,
			hexDigest: hexDigest,
		})
	}
	sortStrings(digests)
	sortEntrySlice(entries)
	return entries, digests, nil
}

// mergeAssetRoots unions caller-supplied roots with embedded-blob roots,
// ascending and unique (I1/I4). Embedded-blob roots arrive as bare hex
// (their entry name carries the digest) and are normalized to full
// identifiers here; a value that fails identifier validation after
// normalization is a caller contract violation, not a silently dropped
// root.
func mergeAssetRoots(supplied, embedded []string) ([]string, *Error) {
	normalize := func(d string) string {
		if len(d) == 64 && isLowerHex(d, 64) {
			return "sha256:" + d
		}
		return d
	}
	union := map[string]bool{}
	for _, d := range supplied {
		d = normalize(d)
		if verr := validateDigestID(d); verr != nil {
			return nil, fail(CatSafeWriteRefusal, StageSave,
				"asset-root entry %q fails digest identity validation", d)
		}
		union[d] = true
	}
	for _, d := range embedded {
		d = normalize(d)
		if verr := validateDigestID(d); verr != nil {
			return nil, verr // validated upstream; unreachable for conforming input
		}
		union[d] = true
	}
	out := make([]string, 0, len(union))
	for d := range union {
		out = append(out, d)
	}
	sortStrings(out)
	return out, nil
}

func buildManifestJSON(profile string, assetRoot []string, entries []zipWriterEntry) ([]byte, *Error) {
	prof, err := json.Marshal(profile)
	if err != nil {
		return nil, fail(CatInvalidFieldSyntax, StageSave, "%v", err)
	}
	buf := []byte(`{"formatVersion":"` + formatVersionV01 + `","profile":`)
	buf = append(buf, prof...)
	buf = append(buf, `,"assetRoot":[`...)
	for i, d := range assetRoot {
		if i > 0 {
			buf = append(buf, ',')
		}
		entry, err := json.Marshal(map[string]string{"digest": d})
		if err != nil {
			return nil, fail(CatInvalidFieldSyntax, StageSave, "%v", err)
		}
		buf = append(buf, entry...)
	}
	buf = append(buf, `],"entries":{`...)
	first := true
	for _, e := range entries {
		if e.name == "domowik.json" && e.data == nil {
			continue // the manifest never inventories itself
		}
		if !first {
			buf = append(buf, ',')
		}
		first = false
		key, err := json.Marshal(e.name)
		if err != nil {
			return nil, fail(CatInvalidFieldSyntax, StageSave, "%v", err)
		}
		buf = append(buf, key...)
		if e.src != nil {
			// Streamed blob entry: identity is the content digest and the
			// declared length; both are re-verified against observed bytes
			// during emission and again by S2 candidate validation.
			buf = append(buf, `:{"sha256":"sha256:`+e.hexDigest+`","byteLength":`+itoa64(int64(e.declared))+`}`...)
			continue
		}
		buf = append(buf, `:{"sha256":"sha256:`+rawSHA256Hex(e.data)+`","byteLength":`+itoa64(int64(len(e.data)))+`}`...)
	}
	buf = append(buf, '}', '}')
	return buf, nil
}
