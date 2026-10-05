package corpusgen

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"hash/crc32"
)

// Named quantities (ED-0019 discipline): every numeric constant below cites
// its definition site. No independent magnitude literals appear elsewhere.
// Named quantities (ED-0019 discipline): numeric constants cite their
// definition site; no independent magnitude literals appear elsewhere.
const blobAHex = "9ca758f319e76338ee864522157a8733f1ce25b9e101e637d69a8d240b832022"

var blobA = []byte("domowik synthetic background tile A (issue 0028 golden fixture)")

const actorX = "01948f5a-ca77-7a09-9e76-5ace11b12fd2"

const emptySnapshot = `{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`

const emptyDescriptors = `{}`

const batchDoc = `{"batchId":"01948f5b-c4e4-7e42-b996-f552a18df77f","actorId":"01948f5a-ca77-7a09-9e76-5ace11b12fd2","counterBase":42,"frontier":{"01948f5a-ca77-7a09-9e76-5ace11b12fd2":41},"operations":[{"operationId":"01948f5d-6e18-7c01-8f0d-77f974bd8036","kind":"createEntity","entityId":"01948f55-6b18-7d3a-a153-1ee2f33cc402","fields":{"id":"01948f55-6b18-7d3a-a153-1ee2f33cc402","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}}]}`

// Artifact is one generated corpus file.
type Artifact struct {
	RelPath string // relative to testdata/corpus
	Bytes   []byte
}

func mustJSON(v interface{}) []byte {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return b
}

func sha256hex(b []byte) string {
	h := sha256.Sum256(b)
	return hex.EncodeToString(h[:])
}

type manifest struct {
	FormatVersion string            `json:"formatVersion"`
	Profile       string            `json:"profile"`
	AssetRoot     []assetRootEntry  `json:"assetRoot"`
	Entries       map[string]digest `json:"entries"`
}

type assetRootEntry struct {
	Digest string `json:"digest"`
}

type digest struct {
	SHA256     string `json:"sha256"`
	ByteLength int    `json:"byteLength"`
}

func buildManifest(profile string, roots []string, others []Entry) []byte {
	// AssetRoot is initialized non-nil so an EMPTY root serializes as [] and
	// never as null: FS 7.4 defines assetRoot as a required
	// ascending-by-digest ARRAY, and null is not an array. The reader
	// currently tolerates null (wire.go, "generators may serialize an absent
	// table as null"), but that is parser laxity and cannot make a fixture
	// conforming — the same reasoning that put the full S1-S5 member set into
	// the ratio twin's snapshot payload. Thin packages, including the
	// golden/packages/thin-valid.domo success fixture and every hostile base
	// derived from it, previously emitted null here.
	m := manifest{
		FormatVersion: "0.1",
		Profile:       profile,
		AssetRoot:     []assetRootEntry{},
		Entries:       map[string]digest{},
	}
	for _, r := range roots {
		m.AssetRoot = append(m.AssetRoot, assetRootEntry{Digest: "sha256:" + r})
	}
	for _, e := range others {
		if e.Name == "domowik.json" {
			continue
		}
		m.Entries[e.Name] = digest{SHA256: "sha256:" + sha256hex(e.Data), ByteLength: len(e.Data)}
	}
	return mustJSON(m)
}

// validThin assembles a conforming thin package (store method throughout).
func validThin() []byte {
	others := []Entry{
		{Name: "assets/descriptors.json", Data: []byte(emptyDescriptors)},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
	}
	all := append([]Entry{{Name: "domowik.json", Data: buildManifest("thin", nil, others)}}, others...)
	sortEntries(all)
	return WriteArchive(all, nil)
}

// validPortable assembles a conforming portable package embedding blob A.
func validPortable() []byte {
	desc := mustJSON(map[string]interface{}{
		"sha256:" + blobAHex: map[string]interface{}{
			"digest":      "sha256:" + blobAHex,
			"byteLength":  len(blobA),
			"mediaType":   "image/png",
			"purpose":     "background-image",
			"encoding":    "identity",
			"requirement": "required",
			"provenance": map[string]interface{}{
				"displayName":       "Synthetic floor tile",
				"sourceDescription": "authored for the issue 0028 corpus",
				"author":            "",
				"acquiredAt":        "2026-08-24T00:00:00Z",
			},
		},
	})
	others := []Entry{
		{Name: "assets/descriptors.json", Data: desc},
		{Name: "blob/sha256/" + blobAHex[:2] + "/" + blobAHex[2:], Data: blobA},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
	}
	all := append([]Entry{{Name: "domowik.json", Data: buildManifest("portable", []string{blobAHex}, others)}}, others...)
	sortEntries(all)
	return WriteArchive(all, nil)
}

func sortEntries(es []Entry) {
	for i := 1; i < len(es); i++ {
		for j := i; j > 0 && es[j].Name < es[j-1].Name; j-- {
			es[j], es[j-1] = es[j-1], es[j]
		}
	}
}

// baseFor returns the canonical thin entries plus domowik.json, before
// archive serialization, so hostile variants can patch single fields.
func baseFor() []Entry {
	others := []Entry{
		{Name: "assets/descriptors.json", Data: []byte(emptyDescriptors)},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
	}
	all := append([]Entry{{Name: "domowik.json", Data: buildManifest("thin", nil, others)}}, others...)
	sortEntries(all)
	return all
}

func serialize(es []Entry) []byte { return WriteArchive(es, nil) }

func withPatched(index int, mutate func(*Entry), es []Entry) []Entry {
	out := make([]Entry, len(es))
	copy(out, es)
	mutate(&out[index])
	return out
}

func u32(v uint32) *uint32 { return &v }

// withoutName returns es minus the entry carrying name. Callers that need
// to remove a specific logical entry must use this rather than a slice
// index: sortEntries orders by name, so positions are not stable.
func withoutName(es []Entry, name string) []Entry {
	out := make([]Entry, 0, len(es))
	for _, e := range es {
		if e.Name == name {
			continue
		}
		out = append(out, e)
	}
	if len(out) == len(es) {
		panic("no entry " + name)
	}
	return out
}

// baseWithSnapshot returns the canonical thin entry set carrying snap as the
// snapshot document, with domowik.json built over exactly those bytes. Any
// fixture whose oracle is SUCCESS must be assembled this way so the manifest
// inventory describes the payload it actually ships.
func baseWithSnapshot(snap []byte) []Entry {
	others := []Entry{
		{Name: "assets/descriptors.json", Data: []byte(emptyDescriptors)},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: snap},
	}
	all := append([]Entry{{Name: "domowik.json", Data: buildManifest("thin", nil, others)}}, others...)
	sortEntries(all)
	return all
}

// emptyWriterSnapshot is the complete FS 5.1 S1-S5 snapshot document for an
// empty projection, in the member set and order the writer itself emits
// (buildSnapshotJSON): S1 entities and createdPositions, S2 frontier, S3
// batchPositions and winnerPositions, S4 tombstones, S5
// batchMetadata.recentBatchIds.
//
// It carried the pre-ED-0037 winnerKeys/createdKeys and no batchPositions
// until issue 0011 widened the writer, at which point the comment above it
// became false: this constant claimed to be what buildSnapshotJSON emits
// while actually being a LEGACY snapshot. A generator that claims to match
// the writer and does not makes every regeneration comparison built on it
// non-discriminating by construction, so it is corrected before anything
// else is proven against it.
//
// The reader no longer treats these members as optional either: §5.1 says a
// snapshot contains ALL of S1-S5, and omitting one is absent required data.
const emptyWriterSnapshot = `{"entities":{},"frontier":{},"batchPositions":{},` +
	`"winnerPositions":{},"createdPositions":{},"tombstones":{},` +
	`"batchMetadata":{"recentBatchIds":[]}}`

// RatioTwinPayload builds the under-L15 twin's logical payload: the complete
// empty S1-S5 snapshot document above, padded to microBombLogicalBytes with
// one run of insignificant inter-token whitespace (FS 3.3), so a genuine
// high-ratio deflate stream still expands to a fully conforming snapshot.
// Exported so the sidecar checker recomputes the stored stream from the SAME
// bytes the package ships instead of assuming an incompressible-run stand-in.
func RatioTwinPayload() []byte {
	// Pad before the final brace: whitespace between tokens is
	// insignificant, so the padded document parses identically.
	head, tail := emptyWriterSnapshot[:len(emptyWriterSnapshot)-1], emptyWriterSnapshot[len(emptyWriterSnapshot)-1:]
	pad := microBombLogicalBytes - len(head) - len(tail)
	out := make([]byte, 0, microBombLogicalBytes)
	out = append(out, head...)
	out = append(out, bytes.Repeat([]byte{' '}, pad)...)
	return append(out, tail...)
}

func findName(es []Entry, name string) int {
	for i := range es {
		if es[i].Name == name {
			return i
		}
	}
	panic("no entry " + name)
}

func AllArtifacts() []Artifact {
	var arts []Artifact
	add := func(rel string, b []byte) { arts = append(arts, Artifact{rel, b}) }

	add("golden/packages/thin-valid.domo", validThin())
	add("golden/packages/portable-valid.domo", validPortable())

	base := baseFor()
	snapIdx := findName(base, "snapshot/state.json")

	hostile := func(rel string, es []Entry) { add("hostile/container/"+rel, serialize(es)) }
	patchOne := func(rel string, idx int, mutate func(*Entry)) {
		hostile(rel, withPatched(idx, mutate, base))
	}
	patchEOCD := func(rel string, f func([]byte) []byte) {
		add("hostile/container/"+rel, WriteArchive(base, f))
	}

	add("hostile/container/empty-file.domo", []byte{})
	add("hostile/container/bad-signature.domo", mutateByte(validThin(), 0))
	add("hostile/container/truncated-header.domo", validThin()[10:])
	vt := validThin()
	add("hostile/container/truncated-eocd.domo", vt[:len(vt)-8])
	add("hostile/container/trailing-data.domo", append(append([]byte{}, vt...), bytes.Repeat([]byte{0x44}, 16)...))
	add("hostile/container/concatenated-packages.domo", append(append([]byte{}, vt...), vt...))

	// Central-directory offset moved into the middle of the first local
	// payload region: overlapping regions (T1).
	patchEOCD("overlapping-regions.domo", func(b []byte) []byte {
		b[len(b)-6] = 0x20 // low byte of cdOffset -> points inside local data
		return b
	})

	patchOne("malformed-length.domo", snapIdx, func(e *Entry) { e.LocalOnlyUncomp = u32(uint32(len(e.Data) + declaredLengthLieDelta)) })
	patchOne("encrypted-entry.domo", snapIdx, func(e *Entry) { e.Flags |= 0x0001 })
	patchOne("unsupported-method.domo", snapIdx, func(e *Entry) { e.Method = unsupportedMethodProbe })
	patchOne("checksum-error.domo", snapIdx, func(e *Entry) {
		c := *u32(crcOf(e.Data) ^ 0xffff)
		e.OverrideCRC = &c
		e.CentralOnlyCRC = &c
	})
	patchOne("utf8-flag-clear.domo", snapIdx, func(e *Entry) { e.SuppressUTF8Flag = true })
	patchOne("nonzero-comment.domo", snapIdx, func(e *Entry) { e.Comment = "hello" })
	// FS 7.2's comment rule has TWO limbs -- "Entry comments AND the archive
	// comment MUST be zero-length" -- and nonzero-comment.domo above exercises
	// only the entry limb. A reader that enforces entry comments and ignores
	// the EOCD's own comment passed the whole corpus (issue 0029,
	// corpus-defect).
	//
	// BOTH halves of the EOCD comment are written: the bytes, and the
	// commentLength field they follow. Writing only the bytes leaves them
	// outside the declared record, so the reject becomes trailing-data under
	// T1 and the comment rule goes untested again; writing only the length
	// declares a comment that is not there and truncates the archive.
	patchEOCD("nonzero-archive-comment.domo", func(b []byte) []byte {
		comment := []byte(archiveComment)
		binary.LittleEndian.PutUint16(b[len(b)-2:], uint16(len(comment)))
		return append(b, comment...)
	})

	patchOne("foreign-extra-field.domo", snapIdx, func(e *Entry) { e.Extra = []byte{0x75, 0x70, 2, 0, 1, 2} })
	patchOne("version-needed-over.domo", snapIdx, func(e *Entry) { e.VersionNeeded = maxSupportedVersionNeeded + 1 })
	patchOne("directory-entry-attr.domo", snapIdx, func(e *Entry) { e.ExtAttrs = 0x0010 })
	patchOne("symlink-designation.domo", snapIdx, func(e *Entry) {
		e.VersionMadeBy = 0x0300
		e.ExtAttrs = (0o120777 << 16)
	})

	dup := append([]Entry{}, base...)
	dup = append(dup, Entry{Name: "snapshot/state.json", Data: []byte("{}")})
	hostile("duplicate-logical-name.domo", dup)

	casepair := append([]Entry{}, base...)
	casepair = append(casepair, Entry{Name: "SNAPSHOT/STATE.JSON", Data: []byte("{}")})
	sortEntries(casepair)
	hostile("name-casefold-collision.domo", casepair)

	// The second manifest is built over the SAME entry set as the first, so the
	// only difference between them is the case-folded name. It was built over
	// base[1:] -- the descriptors-less slice -- which made the two manifests
	// disagree on their inventories for a reason the fixture does not name.
	ambig := append([]Entry{}, base...)
	ambig = append(ambig, Entry{Name: "DOMOWIK.JSON", Data: buildManifest("thin", nil, []Entry{
		{Name: "assets/descriptors.json", Data: []byte(emptyDescriptors)},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
	})})
	sortEntries(ambig)
	hostile("ambiguous-manifest.domo", ambig)

	// FS 9.4 missing-manifest row: a structurally valid archive with no
	// domowik.json at all. The entry is removed BY NAME; sortEntries orders
	// ascending, so "assets/descriptors.json" precedes it and a positional
	// drop removes the wrong entry, leaving the manifest listing an absent
	// inventory member (checksum-error) instead.
	noManifest := withoutName(base, "domowik.json")
	hostile("missing-manifest.domo", noManifest)

	// A name fixture must carry EXACTLY ONE defect: the hostile name.
	//
	// It therefore ships the full canonical entry set plus the hostile entry,
	// with domowik.json rebuilt OVER that set -- FS 7.4 requires entries to map
	// "EVERY other logical entry name", so an archive member the inventory
	// omits is a second, unrelated defect.
	//
	// This previously sliced the sorted base at index 1, meaning "everything
	// but the manifest". Sorted ascending, index 0 is
	// assets/descriptors.json, so the slice dropped the DESCRIPTORS and kept a
	// manifest that still inventoried them (issue 0062 review; ED-0104).
	traverse := func(name string) []Entry {
		others := []Entry{
			{Name: "assets/descriptors.json", Data: []byte(emptyDescriptors)},
			{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
			{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
			{Name: name, Data: []byte("hostile")},
		}
		all := append([]Entry{{Name: "domowik.json", Data: buildManifest("thin", nil, others)}}, others...)
		sortEntries(all)
		return all
	}
	hostile("traversal-dotdot.domo", traverse("../domowik.json"))
	hostile("absolute-path.domo", traverse("/snapshot/state.json"))
	hostile("drive-form.domo", traverse("C:snapshot-state.json"))
	hostile("backslash-name.domo", traverse("snapshot\\state.json"))
	hostile("nul-byte-name.domo", traverse("snapshot/state\x00x.json"))
	hostile("dot-segment-name.domo", traverse("snapshot/./state.json"))
	hostile("overlong-component-L9.domo", traverse("preview/"+string(bytes.Repeat([]byte{'a'}, 300))+".json"))
	hostile("overlong-name-L8.domo", traverse(string(bytes.Repeat([]byte("b/"), 600))+"c.json"))

	// FS 7.2 single-disk requirement: "this disk number" != 0. The field is
	// addressed from the END of the fixed-length EOCD record so the patch
	// cannot drift into cdOffset (the previous len(b)-4 landed at EOCD+18,
	// producing an impossible offset and a malformed-length reject instead).
	patchEOCD("spanned-disk-number.domo", func(b []byte) []byte {
		b[len(b)-eocdFixedLen+eocdDiskNumberOffset] = 1
		return b
	})

	// Zip64 locator with contradictory trailing offset appended after EOCD.
	loc := make([]byte, 20)
	put32(loc[0:], sigLoc64)
	put32(loc[4:], 0xdeadbeef)
	put64x8(loc[8:], 1<<40)
	put32(loc[16:], 1)
	add("hostile/container/zip64-malformed-locator.domo",
		append(append([]byte{}, vt...), loc...))

	eocd64 := make([]byte, 44) // truncated: real record is 56 bytes
	put32(eocd64[0:], sigEOCD64)
	add("hostile/container/zip64-truncated-eocd64.domo",
		append(append([]byte{}, vt...), eocd64...))

	// Declared expanded size one byte beyond the ratified L3 quantity in BOTH
	// headers with a tiny honest stream: the deterministic rejection class is
	// malformed-length (FS 9.4 declared-vs-observed row; FS 9.5 physical
	// stage wins), with the L3 boundary expressed only through the constant.
	patchOne("declared-size-lie-limit-L3.domo", snapIdx, func(e *Entry) {
		lie := uint32(l3EntryExpandedBytes + 1)
		e.LocalOnlyUncomp = &lie
		e.OverrideUncomp = &lie
	})

	// Genuine high-ratio stream just UNDER L15 (inert boundary twin), whose
	// oracle is SUCCESS: the package must therefore stay conforming end to
	// end. The payload is a valid FS 5.1 snapshot document padded to the
	// corpus safety cap with insignificant inter-token whitespace (FS 3.3),
	// and the manifest is rebuilt OVER the patched entry set. Patching the
	// canonical base in place instead — as this fixture previously did —
	// left the inventory digest describing the pre-patch bytes
	// (checksum-error) and substituted a payload that is not a parsable
	// snapshot document at all.
	ratioData := RatioTwinPayload()
	stream := deflateRaw(ratioData)
	ratioEntries := baseWithSnapshot(ratioData)
	ratioIdx := findName(ratioEntries, "snapshot/state.json")
	hostile("ratio-under-L15-success-twin.domo", withPatched(ratioIdx, func(e *Entry) {
		e.Method = 8
		e.StoredOverride = stream
	}, ratioEntries))
	add("hostile/container/ratio-under-L15-success-twin.meta.json", mustJSON(map[string]int64{
		"logicalBytes": int64(len(ratioData)),
		"storedBytes":  int64(len(stream)),
		"ratioPercent": int64(len(ratioData)) * 100 / int64(len(stream)),
	}))

	// Declared-metadata probe ABOVE the L15 ratio branch threshold: the
	// declared uncompressed size encodes >1000:1 against a stored stream at
	// least l15SmallInputFloorBytes long (FS 9.3 L15 switches to an absolute
	// expanded bound below that floor). Expected token is malformed-length:
	// FS 9.4/9.5 make the physical declared-vs-observed stage deterministic.
	patchOne("limit-expansion-ratio-over-L15.domo", snapIdx, func(e *Entry) {
		e.Method = 8
		storedLen := len(deflateRaw(e.Data))
		if storedLen < l15SmallInputFloorBytes {
			storedLen = l15SmallInputFloorBytes
		}
		lie := uint32(storedLen) * (l15RejectionRatio + 1) // declared ratio one step beyond L15's rejection point
		e.OverrideUncomp = &lie
		e.LocalOnlyUncomp = &lie
	})

	// Portable package whose embedded blob no longer matches its listed
	// digest: portability claim broken (missing-asset-equivalence).
	portableBroken := portableBrokenDigest()
	add("hostile/container/digest-mismatch-portable.domo", portableBroken)

	return arts
}

func portableBrokenDigest() []byte {
	es := baseForPort()
	// Corrupt the blob payload AFTER the manifest was built from it.
	i := findName(es, "blob/sha256/"+blobAHex[:2]+"/"+blobAHex[2:])
	es[i].Data = append([]byte{}, es[i].Data...)
	es[i].Data[0] ^= 0xff
	return serialize(es)
}

func baseForPort() []Entry {
	desc := mustJSON(map[string]interface{}{
		"sha256:" + blobAHex: map[string]interface{}{
			"digest":      "sha256:" + blobAHex,
			"byteLength":  len(blobA),
			"mediaType":   "image/png",
			"purpose":     "background-image",
			"encoding":    "identity",
			"requirement": "required",
			"provenance": map[string]interface{}{
				"displayName":       "Synthetic floor tile",
				"sourceDescription": "authored for the issue 0028 corpus",
				"author":            "",
				"acquiredAt":        "2026-08-24T00:00:00Z",
			},
		},
	})
	others := []Entry{
		{Name: "assets/descriptors.json", Data: desc},
		{Name: "blob/sha256/" + blobAHex[:2] + "/" + blobAHex[2:], Data: blobA},
		{Name: "ops/" + fmt.Sprintf("%019d", 42) + "-" + actorX + ".json", Data: []byte(batchDoc)},
		{Name: "snapshot/state.json", Data: []byte(emptySnapshot)},
	}
	all := append([]Entry{{Name: "domowik.json", Data: buildManifest("portable", []string{blobAHex}, others)}}, others...)
	sortEntries(all)
	return all
}

func crcOf(b []byte) uint32 { return crc32.ChecksumIEEE(b) }

func mutateByte(b []byte, i int) []byte {
	out := append([]byte{}, b...)
	out[i] ^= 0xff
	return out
}

func put32(b []byte, v uint32) {
	b[0] = byte(v)
	b[1] = byte(v >> 8)
	b[2] = byte(v >> 16)
	b[3] = byte(v >> 24)
}

func put64x8(b []byte, v uint64) {
	for i := 0; i < 8; i++ {
		b[i] = byte(v >> (8 * i))
	}
}

// Named corpus safety parameter (not a specification quantity): committed
// artifacts stay at or under this bound so every fixture remains inert;
// introduced by reviewer amendment 2 and recorded in testdata/corpus/README.md.
const corpusArtifactCapBytes = 256 << 10

// The end-of-central-directory record is a fixed 22 bytes when no archive
// comment is present: signature(0), thisDiskNumber(4), cdStartDisk(6),
// entriesOnThisDisk(8), totalEntries(10), cdSize(12), cdOffset(16),
// commentLength(20). These names exist so hostile EOCD patches address a
// field rather than an offset counted from the end of the file.
const (
	eocdFixedLen         = 22
	eocdDiskNumberOffset = 4
)

// microBombLogicalBytes is the logical expansion of the under-L15 genuine
// stream twin, chosen as the corpus safety cap itself: the largest inert size
// the safety gate permits.
const microBombLogicalBytes = corpusArtifactCapBytes

// l15SmallInputFloorBytes restates the FS 9.3 row L15 boundary quantity:
// inputs whose physical size is below this floor use the absolute expanded
// bound instead of the ratio, so declared-ratio probes must store at least
// this many bytes to engage the ratio branch deterministically.
const l15SmallInputFloorBytes = 64

// declaredLengthLieDelta is an arbitrary-but-fixed contradiction margin for
// the malformed-length probe; the class is triggered by ANY
// declared-vs-observed mismatch, so the delta carries no limit semantics.
const declaredLengthLieDelta = 4096

// archiveComment is the comment the FS 7.2 archive-comment limb probe carries.
// Any non-empty value violates the rule identically; the text names the fixture
// so a hexdump explains itself.
const archiveComment = "domowik corpus archive comment probe"

// unsupportedMethodProbe is an arbitrary method ID outside the ratified FS
// 7.7 allowlist {0 store, 8 deflate}; any such value rejects identically.
const unsupportedMethodProbe = 12

// maxSupportedVersionNeeded restates the FS 7.2 version bound: values above
// 45 reject as unsupported-version.
const maxSupportedVersionNeeded = 45

// l15RejectionRatio restates the FS 9.3 row L15 ratified quantity:
// expansion ratios greater than 1000:1 reject; exactly 1000 succeeds.
const l15RejectionRatio = 1000

// l3EntryExpandedBytes restates the FS 9.3 row L3 ratified quantity
// (per-entry expanded bytes, 512 MiB); used only as the reference point
// (+1) for the declared-size contradiction probe.
const l3EntryExpandedBytes = 1 << 29
