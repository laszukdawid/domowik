package persist

import (
	"bytes"
	"compress/flate"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"hash"
	"hash/crc32"
	"io"
)

// zipwrite.go: the deterministic strict-profile writer. Conforming
// writers emit complete candidates (§7.8): every size is known before
// headers are written, so no data descriptors exist, bit 11 (UTF-8) is
// always set, timestamps are zeroed (never interpreted), attributes are
// regular-file/zeroed, and entries appear in ascending UTF-8 byte order
// of logical names (§7.2 physical-order obligation). Compression method
// selection is writer policy bounded by the §7.7 allowlist.
// zipWriterEntry is one candidate entry. Buffered entries carry data;
// streamed entries (src != nil) carry a BlobSource and are emitted with
// method store only, because conforming writers know every size before
// headers are written and the source declares its length up front.
type zipWriterEntry struct {
	name      string
	data      []byte
	method    uint16 // store or deflate only
	src       BlobSource
	declared  uint64
	hexDigest string
}

func buildArchive(entries []zipWriterEntry) ([]byte, *Error) {
	var buf bytes.Buffer
	if err := emitArchive(&buf, entries); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// emitArchive streams the complete candidate to w: local headers and
// stored payloads are written entry-by-entry (S1 stream-write, §9.7) so
// the full package is never materialized as one buffer; only the small
// central-directory records accumulate. Buffered payloads stay bounded by
// L3/L4 per entry; streamed blob entries run one bounded CRC pre-pass and
// one emission pass over their source, both enforcing declared-vs-observed
// length (C3) without buffering the payload.
func emitArchive(w io.Writer, entries []zipWriterEntry) *Error {
	if err := checkWriterOrdering(entries); err != nil {
		return err
	}
	type central struct {
		name   string
		method uint16
		crc    uint32
		csize  uint32
		usize  uint32
		offset uint32
	}
	var centrals []central
	var offset uint32
	for _, e := range entries {
		if e.src != nil {
			if e.method != methodStore {
				return fail(CatUnsupportedMethod, StageSave,
					"streamed entries are store-method only")
			}
			h := crc32.NewIEEE()
			sha := sha256.New()
			if _, ferr := streamBlob(nil, h, sha, e.src, e.declared); ferr != nil {
				return ferr
			}
			// Digest-before-embed (ASSET-STRATEGY 10.1/10.4): the
			// pre-pass arbitrates the stream's content against the
			// digest identity the entry claims; a mismatch aborts
			// packing completely before any output byte exists.
			if e.hexDigest != "" && "sha256:"+hex.EncodeToString(sha.Sum(nil)) != "sha256:"+e.hexDigest {
				return fail(CatChecksumError, StageDigest,
					"streamed blob fails verification against its declared identity %s; portability claim broken, packing aborted completely",
					e.hexDigest[:8]+"...")
			}
			crc := h.Sum32()
			storedLen := e.declared // store method: physical == logical bytes
			local := make([]byte, 30)
			binary.LittleEndian.PutUint32(local[0:], sigLocal)
			binary.LittleEndian.PutUint16(local[4:], 20)
			binary.LittleEndian.PutUint16(local[6:], flagUTF8)
			binary.LittleEndian.PutUint16(local[8:], e.method)
			binary.LittleEndian.PutUint32(local[14:], crc)
			binary.LittleEndian.PutUint32(local[18:], uint32(storedLen))
			binary.LittleEndian.PutUint32(local[22:], uint32(storedLen))
			binary.LittleEndian.PutUint16(local[26:], uint16(len(e.name)))
			if _, werr := w.Write(append(local, e.name...)); werr != nil {
				return fail(CatSafeWriteRefusal, StageSave, "candidate write failed (X2): %v", werr)
			}
			if _, ferr := streamBlob(w, nil, nil, e.src, e.declared); ferr != nil {
				return ferr
			}
			centrals = append(centrals, central{name: e.name, method: e.method,
				crc: crc, csize: uint32(storedLen), usize: uint32(storedLen), offset: offset})
			offset += uint32(30 + len(e.name) + int(storedLen))
			continue
		}
		crc := crc32.ChecksumIEEE(e.data)
		stored := e.data
		if e.method == methodDeflate {
			stored = deflateBytes(e.data)
		}
		if uint64(len(stored)) > LimitEntryPhysicalBytes {
			return fail(CatLimitEntryPhysical, StageSave, "stored entry %q exceeds L4", e.name)
		}
		local := make([]byte, 30)
		binary.LittleEndian.PutUint32(local[0:], sigLocal)
		binary.LittleEndian.PutUint16(local[4:], 20)
		binary.LittleEndian.PutUint16(local[6:], flagUTF8)
		binary.LittleEndian.PutUint16(local[8:], e.method)
		binary.LittleEndian.PutUint32(local[14:], crc)
		binary.LittleEndian.PutUint32(local[18:], uint32(len(stored)))
		binary.LittleEndian.PutUint32(local[22:], uint32(len(e.data)))
		binary.LittleEndian.PutUint16(local[26:], uint16(len(e.name)))
		if _, werr := w.Write(append(local, e.name...)); werr != nil {
			return fail(CatSafeWriteRefusal, StageSave, "candidate write failed (X2): %v", werr)
		}
		if _, werr := w.Write(stored); werr != nil {
			return fail(CatSafeWriteRefusal, StageSave, "candidate write failed (X2): %v", werr)
		}
		centrals = append(centrals, central{name: e.name, method: e.method,
			crc: crc, csize: uint32(len(stored)), usize: uint32(len(e.data)), offset: offset})
		offset += uint32(30 + len(e.name) + len(stored))
	}
	cdStart := offset
	cdSize := uint32(0)
	for _, c := range centrals {
		cdSize += uint32(46 + len(c.name))
		rec := make([]byte, 46)
		binary.LittleEndian.PutUint32(rec[0:], sigCentral)
		binary.LittleEndian.PutUint16(rec[6:], 20)
		binary.LittleEndian.PutUint16(rec[8:], flagUTF8)
		binary.LittleEndian.PutUint16(rec[10:], c.method)
		binary.LittleEndian.PutUint32(rec[16:], c.crc)
		binary.LittleEndian.PutUint32(rec[20:], c.csize)
		binary.LittleEndian.PutUint32(rec[24:], c.usize)
		binary.LittleEndian.PutUint16(rec[28:], uint16(len(c.name)))
		binary.LittleEndian.PutUint32(rec[42:], c.offset)
		if _, werr := w.Write(append(rec, c.name...)); werr != nil {
			return fail(CatSafeWriteRefusal, StageSave, "directory write failed: %v", werr)
		}
	}
	eocd := make([]byte, 22)
	binary.LittleEndian.PutUint32(eocd[0:], sigEOCD)
	binary.LittleEndian.PutUint16(eocd[8:], uint16(len(centrals)))
	binary.LittleEndian.PutUint16(eocd[10:], uint16(len(centrals)))
	binary.LittleEndian.PutUint32(eocd[12:], cdSize)
	binary.LittleEndian.PutUint32(eocd[16:], cdStart)
	if _, werr := w.Write(eocd); werr != nil {
		return fail(CatSafeWriteRefusal, StageSave, "terminator write failed: %v", werr)
	}
	return nil
}

// checkWriterOrdering enforces the physical-entry-order writer obligation
// and duplicate rejection at write time (§7.2/§9.4).
func checkWriterOrdering(entries []zipWriterEntry) *Error {
	if len(entries) > LimitEntryCount {
		return fail(CatLimitEntryCount, StageSave,
			"%d entries exceeds L6", len(entries))
	}
	for i := 1; i < len(entries); i++ {
		prev, cur := entries[i-1].name, entries[i].name
		if prev == cur {
			return fail(CatDuplicateLogicalName, StageSave, "duplicate entry %q at write time", cur)
		}
		if prev > cur {
			return fail(CatInvalidEntryName, StageSave,
				"entries must be emitted in ascending name order (writer contract)")
		}
	}
	return nil
}

func deflateBytes(b []byte) []byte {
	var buf bytes.Buffer
	fw, _ := flate.NewWriter(&buf, flate.DefaultCompression)
	_, _ = fw.Write(b)
	_ = fw.Close()
	return buf.Bytes()
}

// streamBlob drains one blob source in bounded chunks, hashing into h
// (when non-nil) and writing to dst (when non-nil). Both passes enforce
// the declared length against observed growth before yielding or writing
// any surplus byte (C3); a short stream is a declared-vs-observed reject.
func streamBlob(dst io.Writer, h hash.Hash32, sha hash.Hash, src BlobSource, declared uint64) (uint64, *Error) {
	r, oerr := src.Open()
	if oerr != nil {
		return 0, fail(CatSafeWriteRefusal, StageSave, "blob source open failed: %v", oerr)
	}
	if c, ok := r.(io.Closer); ok {
		defer c.Close()
	}
	buf := make([]byte, 32<<10)
	var total uint64
	for {
		nr, rerr := r.Read(buf)
		if nr > 0 {
			total += uint64(nr)
			if total > declared {
				return total, fail(CatMalformedLength, StageSave,
					"streamed blob exceeded its declared length %d (C3)", declared)
			}
			if h != nil {
				h.Write(buf[:nr])
			}
			if sha != nil {
				sha.Write(buf[:nr])
			}
			if dst != nil {
				if _, werr := dst.Write(buf[:nr]); werr != nil {
					return total, fail(CatSafeWriteRefusal, StageSave, "candidate write failed (X2): %v", werr)
				}
			}
		}
		if rerr == io.EOF {
			break
		}
		if rerr != nil {
			return total, fail(CatSafeWriteRefusal, StageSave, "blob source read failed: %v", rerr)
		}
	}
	if total != declared {
		return total, fail(CatMalformedLength, StageSave,
			"streamed blob length %d contradicts its declaration %d (C3)", total, declared)
	}
	return total, nil
}
