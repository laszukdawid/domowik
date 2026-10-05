package persist

import (
	"bufio"
	"compress/flate"
	"crypto/sha256"
	"encoding/binary"
	"hash"
	"hash/crc32"
	"io"
	"math"
)

// zipread.go implements the bounded strict-profile ZIP reader. Structure
// handling follows FORMAT-SPEC §7.2/§7.8's normative profile description;
// no stdlib archive/zip helper participates, because every §9.3 limit
// must be enforced during reading, before allocation or extraction.

const (
	sigLocal   uint32 = 0x04034b50
	sigCentral uint32 = 0x02014b50
	sigEOCD    uint32 = 0x06054b50
	sigEOCD64  uint32 = 0x06064b50
	sigLoc64   uint32 = 0x07064b50

	methodStore   uint16 = 0
	methodDeflate uint16 = 8

	flagEncrypted      uint16 = 0x0001
	flagDataDescriptor uint16 = 0x0008
	flagUTF8           uint16 = 0x0800

	extraZip64ID uint16 = 0x0001

	maxVersionNeeded    uint16 = 45 // §7.2 version bound
	dosDirFlag          uint32 = 0x0010
	dosVolumeFlag       uint32 = 0x0008
	creatorSystemMSDOS  uint16 = 0
	creatorSystemUnix   uint16 = 3
	unixFileModeShift   uint32 = 16
	unixRegularFileMode uint32 = 0o100000
)

// entryInfo is one validated central-directory record.
type entryInfo struct {
	name          string
	flags         uint16
	method        uint16
	crc           uint32
	compSize      uint64
	uncompSize    uint64
	localOffset   uint64
	versionNeeded uint16
	versionMadeBy uint16
	intAttrs      uint16
	extAttrs      uint32
}

// archive is a located, structurally validated container.
type archive struct {
	src       io.ReaderAt
	size      int64
	entries   []*entryInfo
	bytesRead uint64 // L2 counter: every raw byte pulled from src
	totExpan  uint64 // L16 aggregate counters (saturating domain)
	totPhys   uint64
}

// openArchive locates and validates the container structure of the input.
// It enforces T1, the Zip64 structural rules, L6, and L7 before any
// proportional allocation, and runs every §7.2 profile check.
func openArchive(src io.ReaderAt, size int64) (*archive, *Error) {
	if size < 22 {
		return nil, fail(CatMalformedContainer, StageContainer, "input shorter than an end-of-central-directory record")
	}
	a := &archive{src: src, size: size}
	eocd, err := a.locateEOCD()
	if err != nil {
		return nil, err
	}
	return a.parseDirectory(eocd)
}

type eocdRecord struct {
	count    uint64
	cdSize   uint64
	cdOffset uint64
	end      int64 // exclusive end of the EOCD record in the file
}

// locateEOCD performs the bounded backward scan and the T1 trailing-data
// analysis. The scan window is fixed (22-byte record plus the maximum
// 65535-byte comment area plus the 20-byte Zip64 locator slot), never
// proportional to untrusted content.
func (a *archive) locateEOCD() (*eocdRecord, *Error) {
	window := int64(22 + 65535 + 20)
	start := a.size - window
	if start < 0 {
		start = 0
	}
	tail := make([]byte, a.size-start)
	if err := a.readAt(tail, start); err != nil {
		return nil, fail(CatMalformedContainer, StageContainer, "unreadable tail region: %v", err)
	}
	rel := -1
	for i := len(tail) - 22; i >= 0; i-- {
		if binary.LittleEndian.Uint32(tail[i:]) == sigEOCD {
			rel = i
			break
		}
	}
	if rel < 0 {
		return nil, fail(CatMalformedContainer, StageContainer, "end-of-central-directory signature not found")
	}
	eocdPos := start + int64(rel)
	rec := tail[rel : rel+22]
	commentLen := int(binary.LittleEndian.Uint16(rec[20:]))
	trailing := a.size - (eocdPos + 22 + int64(commentLen))

	if trailing < 0 || eocdPos+22+int64(commentLen) > a.size {
		return nil, fail(CatMalformedContainer, StageContainer, "truncated end-of-central-directory record")
	}
	if commentLen != 0 {
		return nil, fail(CatUnsupportedExtension, StageContainer,
			"archive comment present; the profile forbids comments")
	}
	if trailing > 0 {
		// T1 fail-closed rule. Trailing material that begins with a ZIP64
		// structure signature classifies as a malformed Zip64 chain
		// (§7.8 item 4); anything else is trailing/concatenated data.
		t := tail[int64(rel)+22:]
		if hasPrefixSig(t, sigLoc64) || hasPrefixSig(t, sigEOCD64) {
			return nil, fail(CatMalformedContainer, StageContainer,
				"malformed or misplaced Zip64 structure after the end-of-central-directory record")
		}
		return nil, fail(CatTrailingData, StageContainer,
			"%d trailing bytes after the container terminator (T1)", trailing)
	}

	out := &eocdRecord{
		count:    uint64(binary.LittleEndian.Uint16(rec[10:])),
		cdSize:   uint64(binary.LittleEndian.Uint32(rec[12:])),
		cdOffset: uint64(binary.LittleEndian.Uint32(rec[16:])),
		end:      eocdPos + 22,
	}
	diskNum := binary.LittleEndian.Uint16(rec[4:])
	cdDisk := binary.LittleEndian.Uint16(rec[6:])
	diskEntries := binary.LittleEndian.Uint16(rec[8:])
	if diskNum != 0 || cdDisk != 0 || uint64(diskEntries) != out.count {
		return nil, fail(CatMalformedContainer, StageContainer,
			"single-disk non-spanned archive required")
	}

	// Zip64 chain: a locator occupying the 20 bytes directly below the
	// EOCD mandates a valid Zip64 EOCD record (§7.8 items 1–4).
	if eocdPos >= 20 {
		loc := make([]byte, 20)
		if err := a.readAt(loc, eocdPos-20); err != nil {
			return nil, fail(CatMalformedContainer, StageContainer, "unreadable Zip64 locator region")
		}
		if binary.LittleEndian.Uint32(loc) == sigLoc64 {
			z64, err := a.resolveZip64(loc, eocdPos)
			if err != nil {
				return nil, err
			}
			out = z64
		}
	}
	return out, nil
}

func hasPrefixSig(b []byte, sig uint32) bool {
	return len(b) >= 4 && binary.LittleEndian.Uint32(b) == sig
}

func (a *archive) resolveZip64(loc []byte, eocdPos int64) (*eocdRecord, *Error) {
	totalDisks := binary.LittleEndian.Uint32(loc[16:])
	eocdDisk := binary.LittleEndian.Uint32(loc[4:])
	z64Off := int64(binary.LittleEndian.Uint64(loc[8:]))
	if totalDisks != 1 || eocdDisk != 0 {
		return nil, fail(CatMalformedContainer, StageContainer, "Zip64 locator names multiple disks")
	}
	if z64Off < 0 || z64Off+56 > eocdPos {
		return nil, fail(CatMalformedContainer, StageContainer,
			"Zip64 locator offset contradicts archive layout")
	}
	rec := make([]byte, 56)
	if err := a.readAt(rec, z64Off); err != nil {
		return nil, fail(CatMalformedContainer, StageContainer, "truncated Zip64 EOCD record")
	}
	if binary.LittleEndian.Uint32(rec) != sigEOCD64 {
		return nil, fail(CatMalformedContainer, StageContainer, "Zip64 locator does not address a Zip64 EOCD record")
	}
	declaredSize := int(binary.LittleEndian.Uint64(rec[4:]))
	if declaredSize+12 < 56 || z64Off+int64(declaredSize)+12 > eocdPos {
		return nil, fail(CatMalformedContainer, StageContainer, "contradictory Zip64 EOCD record size")
	}
	if binary.LittleEndian.Uint32(rec[16:]) != 0 || binary.LittleEndian.Uint32(rec[20:]) != 0 {
		return nil, fail(CatMalformedContainer, StageContainer, "Zip64 EOCD names nonzero disk numbers")
	}
	return &eocdRecord{
		count:    binary.LittleEndian.Uint64(rec[32:]),
		cdSize:   binary.LittleEndian.Uint64(rec[40:]),
		cdOffset: binary.LittleEndian.Uint64(rec[48:]),
		end:      eocdPos,
	}, nil
}

const minCentralOverhead = 47 // fixed 46-byte record + at least one name byte

// parseDirectory walks the central directory under the L6/L7 ordering:
// counts and computed reserves are validated before any proportional
// allocation, then every record receives its §7.2 profile check.
func (a *archive) parseDirectory(e *eocdRecord) (*archive, *Error) {
	if e.count > LimitEntryCount {
		return nil, fail(CatLimitEntryCount, StageDirectory,
			"%d entries exceeds the %d ceiling (L6)", e.count, LimitEntryCount)
	}
	reserve := e.count * minCentralOverhead
	if reserve > LimitManifestReserveBytes {
		return nil, fail(CatLimitManifestReserve, StageDirectory,
			"computed central-directory reserve %d exceeds %d (L7)", reserve, LimitManifestReserveBytes)
	}
	if e.cdOffset > uint64(a.size) || e.cdSize > uint64(a.size)-e.cdOffset ||
		e.cdOffset+e.cdSize > uint64(e.end) {
		// Classify overlap-style contradictions before record-level noise.
		return nil, a.classifyLayoutFault(e.cdOffset, e.cdSize)
	}
	// Exact-fit rule: the terminator must sit immediately behind the
	// directory (or behind the Zip64 chain when one is present). A
	// directory that would run past the terminator is truncated or
	// shifted input; material between directory end and terminator is T1
	// foreign data.
	directoryEnd := e.cdOffset + e.cdSize
	terminatorStart := uint64(e.end - 22)
	if directoryEnd > terminatorStart {
		return nil, fail(CatMalformedContainer, StageContainer,
			"declared directory overruns the container terminator")
	}
	if directoryEnd != terminatorStart &&
		directoryEnd != terminatorStart-(56+20) &&
		directoryEnd != terminatorStart-56 &&
		directoryEnd != terminatorStart-20 {
		return nil, fail(CatTrailingData, StageContainer,
			"foreign material occupies [%d,%d) before the container terminator (T1)",
			directoryEnd, terminatorStart)
	}
	a.entries = make([]*entryInfo, 0, e.count)
	pos := int64(e.cdOffset)
	cdEnd := pos + int64(e.cdSize)
	for i := uint64(0); i < e.count; i++ {
		head := make([]byte, 46)
		if pos+46 > cdEnd {
			return nil, a.classifyLayoutFault(e.cdOffset, e.cdSize)
		}
		if err := a.readAt(head, pos); err != nil {
			return nil, fail(CatMalformedContainer, StageDirectory, "truncated central-directory record")
		}
		if binary.LittleEndian.Uint32(head) != sigCentral {
			return nil, a.classifyLayoutFault(e.cdOffset, e.cdSize)
		}
		nameLen := int(binary.LittleEndian.Uint16(head[28:]))
		extraLen := int(binary.LittleEndian.Uint16(head[30:]))
		commentLen := int(binary.LittleEndian.Uint16(head[32:]))
		total := 46 + nameLen + extraLen + commentLen
		if pos+int64(total) > cdEnd {
			return nil, fail(CatMalformedLength, StageDirectory, "central-directory record overruns directory bounds")
		}
		body := make([]byte, total-46)
		if err := a.readAt(body, pos+46); err != nil {
			return nil, fail(CatMalformedContainer, StageDirectory, "truncated central-directory record body")
		}
		ent, perr := a.buildEntry(head, body, nameLen, extraLen, commentLen)
		if perr != nil {
			return nil, perr
		}
		a.entries = append(a.entries, ent)
		pos += int64(total)
	}
	if ferr := a.validateLayout(e.cdOffset); ferr != nil {
		return nil, ferr
	}
	return a, nil
}

// validateLayout proves the physical-entry chain covers exactly
// [0, cdStart) before any content is processed — the T1 structural
// analysis for prepended, concatenated, and overlapping layouts. Declared
// sizes only feed the offset walk (bounded by file size); they never
// allocate or extract.
func (a *archive) validateLayout(cdOffset uint64) *Error {
	generic := fail(CatMalformedContainer, StageDirectory, "invalid central-directory location")
	byOffset := make(map[uint64]*entryInfo, len(a.entries))
	for _, e := range a.entries {
		if _, dup := byOffset[e.localOffset]; dup {
			return fail(CatOverlappingRegions, StageContainer,
				"two entries claim one local-header region (T1)")
		}
		byOffset[e.localOffset] = e
	}
	if _, ok := byOffset[0]; !ok {
		return fail(CatTrailingData, StageContainer,
			"prepended or concatenated material precedes the first local entry (T1)")
	}
	var off uint64
	for visited := 0; visited <= len(a.entries); visited++ {
		e, ok := byOffset[off]
		if !ok {
			break // chain ends between declared entries: classify below
		}
		if int64(off)+30 > a.size {
			return generic
		}
		head := make([]byte, 30)
		if err := a.readAt(head, int64(off)); err != nil {
			return generic
		}
		if binary.LittleEndian.Uint32(head) != sigLocal {
			return generic
		}
		nameLen := uint64(binary.LittleEndian.Uint16(head[26:]))
		extraLen := uint64(binary.LittleEndian.Uint16(head[28:]))
		csizeField := binary.LittleEndian.Uint32(head[18:])
		var comp uint64
		switch {
		case csizeField == math.MaxUint32:
			comp = e.compSize // Zip64-resolved central value
		default:
			comp = uint64(csizeField)
		}
		if nameLen != uint64(len(e.name)) {
			return fail(CatMalformedLength, StageDirectory,
				"local/central name-length contradiction in layout walk")
		}
		next := off + 30 + nameLen + extraLen + comp
		if next <= off || next > uint64(a.size) {
			return generic
		}
		off = next
		delete(byOffset, e.localOffset)
		if len(byOffset) == 0 {
			break
		}
	}
	switch {
	case off == cdOffset && len(byOffset) == 0:
		return nil
	case cdOffset < off:
		return fail(CatOverlappingRegions, StageContainer,
			"central directory starts inside a local entry region (T1)")
	case len(byOffset) != 0:
		return fail(CatOverlappingRegions, StageContainer,
			"local entry regions do not tile contiguously up to the directory (T1)")
	default:
		return fail(CatOverlappingRegions, StageContainer,
			"central directory overlaps a foreign region (T1)")
	}
}

// classifyLayoutFault distinguishes overlapping-region layouts (T1 row)
// from ordinary malformed containers by walking the declared local-entry
// chain with hard bounds; it never drives allocation.
func (a *archive) classifyLayoutFault(cdOffset, cdSize uint64) *Error {
	generic := fail(CatMalformedContainer, StageDirectory, "invalid central-directory location")
	var off uint64
	for step := 0; step <= LimitEntryCount; step++ {
		if int64(off)+30 > a.size {
			break
		}
		head := make([]byte, 30)
		if err := a.readAt(head, int64(off)); err != nil {
			break
		}
		if binary.LittleEndian.Uint32(head) != sigLocal {
			break
		}
		csize := binary.LittleEndian.Uint32(head[18:])
		if csize == math.MaxUint32 {
			break // Zip64-resolved size unavailable in this diagnostic pass
		}
		next := off + 30 + uint64(binary.LittleEndian.Uint16(head[26:])) +
			uint64(binary.LittleEndian.Uint16(head[28:])) + uint64(csize)
		if next <= off || next > uint64(a.size) {
			break
		}
		off = next
	}
	if off > 0 && int64(off)+4 <= a.size && cdOffset < uint64(a.size) {
		sig := make([]byte, 4)
		if err := a.readAt(sig, int64(off)); err == nil &&
			binary.LittleEndian.Uint32(sig) == sigCentral &&
			cdOffset != off && cdOffset >= off {
			return fail(CatOverlappingRegions, StageContainer,
				"declared directory start %d overlaps the true directory at %d (T1)", cdOffset, off)
		}
	}
	if cdOffset < off && off > 0 {
		return fail(CatOverlappingRegions, StageContainer,
			"central directory starts inside a local entry region (T1)")
	}
	if cdSize > 0 && cdOffset+cdSize > uint64(a.size) {
		return fail(CatMalformedLength, StageDirectory, "central directory overruns the input")
	}
	return generic
}

func (a *archive) buildEntry(head, body []byte, nameLen, extraLen, commentLen int) (*entryInfo, *Error) {
	e := &entryInfo{
		flags:         binary.LittleEndian.Uint16(head[8:]),
		method:        binary.LittleEndian.Uint16(head[10:]),
		crc:           binary.LittleEndian.Uint32(head[16:]),
		compSize:      uint64(binary.LittleEndian.Uint32(head[20:])),
		uncompSize:    uint64(binary.LittleEndian.Uint32(head[24:])),
		localOffset:   uint64(binary.LittleEndian.Uint32(head[42:])),
		versionNeeded: binary.LittleEndian.Uint16(head[6:]),
		versionMadeBy: binary.LittleEndian.Uint16(head[4:]),
		intAttrs:      binary.LittleEndian.Uint16(head[38:]),
		extAttrs:      binary.LittleEndian.Uint32(head[38-2:]), // placeholder corrected below
	}
	e.extAttrs = binary.LittleEndian.Uint32(head[38:])
	name := string(body[:nameLen])
	extra := body[nameLen : nameLen+extraLen]

	if commentLen != 0 {
		return nil, fail(CatUnsupportedExtension, StageDirectory, "entry comments are forbidden")
	}
	if head[34] != 0 || head[35] != 0 { // disk number start
		return nil, fail(CatMalformedContainer, StageDirectory, "nonzero disk start in a single-disk archive")
	}

	// Profile checks in deterministic order (§7.2).
	if e.flags&flagEncrypted != 0 {
		return nil, fail(CatEncryptedEntry, StageDirectory, "encrypted entries are rejected unconditionally")
	}
	if e.flags&flagDataDescriptor != 0 {
		return nil, fail(CatMalformedContainer, StageDirectory,
			"data-descriptor flag set; conforming writers know every size up front")
	}
	if e.flags&flagUTF8 == 0 {
		return nil, fail(CatInvalidEntryName, StageDirectory, "language-encoding flag (bit 11) clear")
	}
	if e.method != methodStore && e.method != methodDeflate {
		return nil, fail(CatUnsupportedMethod, StageDirectory,
			"compression method %d outside the {store, deflate} allowlist", e.method)
	}
	if e.versionNeeded > maxVersionNeeded {
		return nil, fail(CatUnsupportedVersion, StageDirectory,
			"version-needed-to-extract %d above %d", e.versionNeeded, maxVersionNeeded)
	}
	if e.intAttrs != 0 {
		return nil, fail(CatInvalidEntryName, StageDirectory, "internal attributes must be zero")
	}
	if err := checkExternalAttrs(e.versionMadeBy, e.extAttrs); err != nil {
		return nil, err
	}
	if err := parseExtraFields(extra, e); err != nil {
		return nil, err
	}
	e.name = name
	return e, nil
}

func checkExternalAttrs(versionMadeBy uint16, extAttrs uint32) *Error {
	if extAttrs&(dosDirFlag|dosVolumeFlag) != 0 {
		return fail(CatInvalidEntryName, StageDirectory,
			"directory/volume attribute bits forbidden; regular-file entries only")
	}
	switch creator := uint16(versionMadeBy >> 8); creator {
	case creatorSystemUnix:
		mode := extAttrs >> unixFileModeShift
		if mode != 0 && mode&0xF000 != unixRegularFileMode {
			return fail(CatInvalidEntryName, StageDirectory,
				"special-file designation rejected; regular-file entries only")
		}
	case creatorSystemMSDOS:
		// Attribute byte carries only the DOS bits checked above.
	default:
		// Other creator systems: only the DOS bits are defined here; they
		// were checked unconditionally above.
	}
	return nil
}

// parseExtraFields rejects every extra field except the Zip64 extended
// information field (§7.2) and folds Zip64 sizes/offsets into the record.
func parseExtraFields(extra []byte, e *entryInfo) *Error {
	for len(extra) > 0 {
		if len(extra) < 4 {
			return fail(CatUnsupportedExtension, StageDirectory, "truncated extra field")
		}
		id := binary.LittleEndian.Uint16(extra)
		size := int(binary.LittleEndian.Uint16(extra[2:]))
		if len(extra) < 4+size {
			return fail(CatUnsupportedExtension, StageDirectory, "extra field overruns record")
		}
		payload := extra[4 : 4+size]
		if id != extraZip64ID {
			return fail(CatUnsupportedExtension, StageDirectory,
				"extra field 0x%04x outside the permitted Zip64 set", id)
		}
		// Zip64 extended information: fields appear in fixed order
		// (uncompressed, compressed, local offset, disk start) and are
		// present only when the corresponding base field is saturated.
		consume := func(base uint32, dst *uint64) {
			if base != math.MaxUint32 || len(payload) < 8 {
				return
			}
			*dst = binary.LittleEndian.Uint64(payload)
			payload = payload[8:]
		}
		consume(uint32(e.uncompSize), &e.uncompSize)
		consume(uint32(e.compSize), &e.compSize)
		consume(uint32(e.localOffset), &e.localOffset)
		extra = extra[4+size:]
	}
	return nil
}

func (a *archive) readAt(buf []byte, off int64) error {
	n, err := a.src.ReadAt(buf, off)
	a.bytesRead += uint64(n)
	if a.bytesRead > LimitTotalPhysicalBytes {
		return fail(CatLimitTotalPhysical, StageContainer, "raw input exceeds %d bytes (L2)", LimitTotalPhysicalBytes)
	}
	if err != nil && err != io.EOF {
		return err
	}
	return nil
}

// openEntry validates the entry's local header against the central
// record (name, method, CRC, sizes) and returns the guarded logical
// stream. No byte of entry content is yielded before the guards approve.
func (a *archive) openEntry(e *entryInfo) (*entryStream, *Error) {
	if e.localOffset+30 > uint64(a.size) {
		return nil, fail(CatMalformedContainer, StageEntry, "local header offset out of range")
	}
	head := make([]byte, 30)
	if err := a.readAt(head, int64(e.localOffset)); err != nil {
		return nil, fail(CatMalformedContainer, StageEntry, "unreadable local header")
	}
	if binary.LittleEndian.Uint32(head) != sigLocal {
		return nil, fail(CatMalformedContainer, StageEntry, "local header signature invalid")
	}
	lflags := binary.LittleEndian.Uint16(head[6:])
	lmethod := binary.LittleEndian.Uint16(head[8:])
	lcrc := binary.LittleEndian.Uint32(head[14:])
	lcsize := binary.LittleEndian.Uint32(head[18:])
	lusize := binary.LittleEndian.Uint32(head[22:])
	nameLen := int(binary.LittleEndian.Uint16(head[26:]))
	extraLen := int(binary.LittleEndian.Uint16(head[28:]))
	if int64(e.localOffset)+30+int64(nameLen)+int64(extraLen)+int64(e.compSize) > a.size {
		return nil, fail(CatMalformedLength, StageEntry, "declared entry overruns the input")
	}
	nameBuf := make([]byte, nameLen)
	if err := a.readAt(nameBuf, int64(e.localOffset)+30); err != nil {
		return nil, fail(CatMalformedContainer, StageEntry, "unreadable local header name")
	}
	if string(nameBuf) != e.name || lmethod != e.method || lcrc != e.crc ||
		lflags != e.flags {
		return nil, fail(CatMalformedContainer, StageEntry,
			"local/central disagreement on name, flags, method, or CRC")
	}
	// Local size fields: either equal to central values or saturated
	// markers resolved through the Zip64 extra, which — when carried in a
	// local header — MUST contain BOTH sizes (§7.1 item 1).
	okC := uint64(lcsize) == e.compSize
	okU := uint64(lusize) == e.uncompSize
	if (!okC || !okU) && extraLen > 0 {
		extra := make([]byte, extraLen)
		if err := a.readAt(extra, int64(e.localOffset)+30+int64(nameLen)); err != nil {
			return nil, fail(CatMalformedContainer, StageEntry, "unreadable local extra field")
		}
		tmp := &entryInfo{uncompSize: uint64(lusize), compSize: uint64(lcsize)}
		if uerr := parseExtraFields(extra, tmp); uerr != nil {
			return nil, uerr
		}
		okC = tmp.compSize == e.compSize
		okU = tmp.uncompSize == e.uncompSize
	}
	if !okC || !okU {
		return nil, fail(CatMalformedLength, StageEntry,
			"local/central size contradiction (declared-vs-declared)")
	}

	st := &entryStream{
		a:        a,
		declared: e.uncompSize,
		expected: e.crc,
		crc:      crc32.NewIEEE(),
		sha:      sha256.New(),
	}
	dataStart := int64(e.localOffset) + 30 + int64(nameLen) + int64(extraLen)
	raw := &countingReader{r: io.NewSectionReader(a.src, dataStart, int64(e.compSize)), a: a, s: st}
	switch e.method {
	case methodStore:
		st.src = raw
	case methodDeflate:
		st.src = flate.NewReader(bufio.NewReaderSize(raw, 32<<10))
	}
	return st, nil
}

// entryStream is the guarded logical stream of one entry. Every Read
// applies, in order, before any byte is returned to the caller:
// L4 physical growth, L2 total input, L3/L5 expanded caps (observed),
// L15 per-entry ratio (with the small-input absolute bound), L16
// aggregate ratio, L1 cumulative expanded — all with saturating
// arithmetic so counter overflow cannot bypass a guard.
type entryStream struct {
	a        *archive
	src      io.Reader
	declared uint64
	expected uint32
	crc      hash.Hash32
	sha      hash.Hash

	expanded uint64 // observed uncompressed bytes of this entry
	physical uint64 // observed stored bytes pulled for this entry
	extraCap uint64 // optional additional logical cap (row L5 scope)
	// expectedSHA pins the manifest inventory digest when the stream
	// comes from the public StreamEntry surface; finish() arbitrates it.
	expectedSHA string
	finished    bool
	err         *Error
}

func (s *entryStream) Read(p []byte) (int, error) {
	if s.err != nil {
		return 0, s.err
	}
	n, err := s.src.Read(p)
	if n > 0 {
		chunk := p[:n]
		s.expanded += uint64(n)
		newTotExpan := satAdd(s.a.totExpan, uint64(n))
		// Guards evaluate BEFORE the chunk is yielded.
		capLimit := uint64(LimitEntryExpandedBytes)
		if s.extraCap > 0 && s.extraCap < capLimit {
			capLimit = s.extraCap // row L5 blob-admission scope
		}
		if s.expanded > capLimit {
			s.fail(fail(CatLimitBlobBytes, StageEntry,
				"logical bytes exceed %d during streaming digest verification (L5)", capLimit))
			return 0, s.err
		}
		if s.expanded > LimitEntryExpandedBytes {
			s.fail(fail(CatLimitEntryExpanded, StageEntry,
				"expanded entry bytes exceed %d (L3)", LimitEntryExpandedBytes))
			return 0, s.err
		}
		if newTotExpan > LimitTotalExpandedBytes {
			s.fail(fail(CatLimitTotalExpanded, StageEntry,
				"cumulative expanded bytes exceed %d (L1)", LimitTotalExpandedBytes))
			return 0, s.err
		}
		if s.physical < L15SmallInputFloorBytes {
			if s.expanded > L15SmallInputExpandedBound {
				s.fail(fail(CatLimitExpansionRatio, StageEntry,
					"small-input expanded bound %d exceeded (L15)", L15SmallInputExpandedBound))
				return 0, s.err
			}
		} else if s.expanded > satMul(s.physical, LimitExpansionRatio) {
			s.fail(fail(CatLimitExpansionRatio, StageEntry,
				"expansion ratio exceeds %d:1 (L15)", LimitExpansionRatio))
			return 0, s.err
		}
		if newTotExpan > satMul(s.a.totPhys, L16AggregateRatio) &&
			newTotExpan > L16AggregateFloorBytes {
			s.fail(fail(CatLimitAggregateExp, StageEntry,
				"aggregate expansion exceeds %dx with %d-byte floor met (L16)",
				L16AggregateRatio, L16AggregateFloorBytes))
			return 0, s.err
		}
		s.a.totExpan = newTotExpan
		s.crc.Write(chunk)
		s.sha.Write(chunk)
	}
	if err == io.EOF {
		s.finish()
		return n, mapEOF(s.err)
	}
	if err != nil && s.err == nil {
		if fe, ok := err.(*Error); ok {
			s.err = fe
		} else {
			s.err = fail(CatMalformedContainer, StageEntry, "corrupt or unreadable deflate stream: %v", err)
		}
		return n, s.err
	}
	return n, err
}

func mapEOF(e *Error) error {
	if e != nil {
		return e
	}
	return io.EOF
}

// countingReader counts raw physical bytes per entry (L4) and in
// aggregate (L2/L16 physical side) at the source, before decompression.
type countingReader struct {
	r io.Reader
	a *archive
	s *entryStream
}

func (c *countingReader) Read(p []byte) (int, error) {
	n, err := c.r.Read(p)
	if n > 0 {
		c.s.physical = satAdd(c.s.physical, uint64(n))
		c.a.bytesRead = satAdd(c.a.bytesRead, uint64(n)) // single L2 ledger
		c.a.totPhys = satAdd(c.a.totPhys, uint64(n))
		if c.s.physical > LimitEntryPhysicalBytes {
			return n, fail(CatLimitEntryPhysical, StageEntry,
				"stored entry bytes exceed %d (L4)", LimitEntryPhysicalBytes)
		}
		if c.a.bytesRead > LimitTotalPhysicalBytes {
			return n, fail(CatLimitTotalPhysical, StageEntry,
				"raw input exceeds %d bytes (L2)", LimitTotalPhysicalBytes)
		}
	}
	return n, err
}

func (s *entryStream) fail(e *Error) {
	if s.err == nil {
		s.err = e
	}
}

// finish enforces declared-vs-observed equality and the CRC contract at
// end of stream (physical stage precedes checksum semantics, §9.5).
func (s *entryStream) finish() {
	if s.finished {
		return
	}
	s.finished = true
	if s.expanded != s.declared {
		s.fail(fail(CatMalformedLength, StageEntry,
			"declared expanded size %d contradicts observed %d", s.declared, s.expanded))
		return
	}
	if s.crc.Sum32() != s.expected {
		s.fail(fail(CatChecksumError, StageEntry, "stored-data CRC-32 mismatch"))
		return
	}
	if s.expectedSHA != "" && "sha256:"+s.sha256Hex() != s.expectedSHA {
		s.fail(fail(CatChecksumError, StageDigest,
			"digest mismatch against manifest inventory (missing-blob equivalence for blob scope)"))
	}
}

// sha256Hex returns the lowercase hex digest of everything streamed.
func (s *entryStream) sha256Hex() string {
	sum := s.sha.Sum(nil)
	const hexdigits = "0123456789abcdef"
	out := make([]byte, len(sum)*2)
	for i, b := range sum {
		out[i*2] = hexdigits[b>>4]
		out[i*2+1] = hexdigits[b&0xf]
	}
	return string(out)
}

func satAdd(x, y uint64) uint64 {
	s := x + y
	if s < x {
		return math.MaxUint64
	}
	return s
}

func satMul(x, y uint64) uint64 {
	if x == 0 || y == 0 {
		return 0
	}
	if x > math.MaxUint64/y {
		return math.MaxUint64
	}
	return x * y
}
