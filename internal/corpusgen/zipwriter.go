// Package corpusgen is development-only tooling for issue 0028: it builds the
// committed .domo corpus artifacts deterministically and validates the
// machine-readable expectation manifest and artifact-safety caps. It contains
// NO reader, writer, or validator behavior for the Domowik format itself;
// the ZIP assembly below exists solely to materialize inert fixture bytes.
package corpusgen

import (
	"bytes"
	"compress/flate"
	"encoding/binary"
	"hash/crc32"
)

const (
	sigLocal   = 0x04034b50
	sigCentral = 0x02014b50
	sigEOCD    = 0x06054b50
	sigEOCD64  = 0x06064b50
	sigLoc64   = 0x07064b50
)

// Entry is one physical archive member under full byte-level control.
type Entry struct {
	Name          string
	Data          []byte
	Method        uint16 // 0 store, 8 deflate
	Flags         uint16 // general-purpose bit flags
	VersionNeeded uint16
	VersionMadeBy uint16
	IntAttrs      uint16
	ExtAttrs      uint32
	Extra         []byte
	Comment       string
	DiskStart     uint16

	// Overrides applied AFTER canonical field computation (hostile deltas).
	OverrideCRC      *uint32
	OverrideUncomp   *uint32
	OverrideComp     *uint32
	LocalOnlyUncomp  *uint32 // contradicting value written ONLY in the local header
	CentralOnlyCRC   *uint32
	SuppressUTF8Flag bool

	// StoredOverride writes exactly these bytes as the stored stream while
	// declaring sizes/CRC derived from Data (pre-compressed fixtures).
	StoredOverride []byte
}

type centralRecord struct {
	e      Entry
	offset uint32
	crc    uint32
	usize  uint32
	csize  uint32
}

// WriteArchive serializes entries in slice order (physical order is never
// semantic; hostile ordering fixtures are authored explicitly).
func WriteArchive(entries []Entry, patchEOCD func([]byte) []byte) []byte {
	var buf bytes.Buffer
	var centrals []centralRecord
	for _, e := range entries {
		payload := e.Data
		crc := crc32.ChecksumIEEE(payload)
		usize := uint32(len(payload))
		var stored []byte
		if e.StoredOverride != nil {
			stored = e.StoredOverride
		} else if e.Method == 8 {
			stored = deflateRaw(payload)
		} else {
			stored = payload
		}
		csize := uint32(len(stored))
		if e.OverrideCRC != nil {
			crc = *e.OverrideCRC
		}
		if e.OverrideUncomp != nil {
			usize = *e.OverrideUncomp
		}
		if e.OverrideComp != nil {
			csize = *e.OverrideComp
		}
		flags := e.Flags
		if !e.SuppressUTF8Flag {
			flags |= 0x0800
		}
		nameLen := uint16(len(e.Name))
		extraLen := uint16(len(e.Extra))

		off := uint32(buf.Len())
		local := make([]byte, 30)
		binary.LittleEndian.PutUint32(local[0:], sigLocal)
		binary.LittleEndian.PutUint16(local[4:], e.VersionNeeded)
		binary.LittleEndian.PutUint16(local[6:], flags)
		binary.LittleEndian.PutUint16(local[8:], e.Method)
		binary.LittleEndian.PutUint32(local[14:], crc)
		lu := usize
		if e.LocalOnlyUncomp != nil {
			lu = *e.LocalOnlyUncomp
		}
		binary.LittleEndian.PutUint32(local[18:], csize)
		binary.LittleEndian.PutUint32(local[22:], lu)
		binary.LittleEndian.PutUint16(local[26:], nameLen)
		binary.LittleEndian.PutUint16(local[28:], extraLen)
		buf.Write(local)
		buf.WriteString(e.Name)
		buf.Write(e.Extra)
		buf.Write(stored)

		ccrc, cusize := crc, usize
		if e.CentralOnlyCRC != nil {
			ccrc = *e.CentralOnlyCRC
		}
		centrals = append(centrals, centralRecord{e: e, offset: off, crc: ccrc, usize: cusize, csize: csize})
	}

	cdStart := uint32(buf.Len())
	for _, c := range centrals {
		flags := c.e.Flags
		if !c.e.SuppressUTF8Flag {
			flags |= 0x0800
		}
		rec := make([]byte, 46)
		binary.LittleEndian.PutUint32(rec[0:], sigCentral)
		binary.LittleEndian.PutUint16(rec[4:], c.e.VersionMadeBy)
		binary.LittleEndian.PutUint16(rec[6:], c.e.VersionNeeded)
		binary.LittleEndian.PutUint16(rec[8:], flags)
		binary.LittleEndian.PutUint16(rec[10:], c.e.Method)
		binary.LittleEndian.PutUint32(rec[16:], c.crc)
		binary.LittleEndian.PutUint32(rec[20:], c.csize)
		binary.LittleEndian.PutUint32(rec[24:], c.usize)
		binary.LittleEndian.PutUint16(rec[28:], uint16(len(c.e.Name)))
		binary.LittleEndian.PutUint16(rec[30:], uint16(len(c.e.Extra)))
		binary.LittleEndian.PutUint16(rec[32:], uint16(len(c.e.Comment)))
		binary.LittleEndian.PutUint16(rec[34:], c.e.DiskStart)
		binary.LittleEndian.PutUint16(rec[36:], c.e.IntAttrs)
		binary.LittleEndian.PutUint32(rec[38:], c.e.ExtAttrs)
		binary.LittleEndian.PutUint32(rec[42:], c.offset)
		buf.Write(rec)
		buf.WriteString(c.e.Name)
		buf.Write(c.e.Extra)
		buf.WriteString(c.e.Comment)
	}
	cdSize := uint32(buf.Len()) - cdStart

	eocd := make([]byte, 22)
	binary.LittleEndian.PutUint32(eocd[0:], sigEOCD)
	binary.LittleEndian.PutUint16(eocd[8:], uint16(len(centrals)))
	binary.LittleEndian.PutUint16(eocd[10:], uint16(len(centrals)))
	binary.LittleEndian.PutUint32(eocd[12:], cdSize)
	binary.LittleEndian.PutUint32(eocd[16:], cdStart)
	buf.Write(eocd)
	out := buf.Bytes()
	if patchEOCD != nil {
		out = patchEOCD(out)
	}
	return out
}

func deflateRaw(b []byte) []byte {
	var buf bytes.Buffer
	fw, _ := flate.NewWriter(&buf, flate.BestCompression)
	_, _ = fw.Write(b)
	_ = fw.Close()
	return buf.Bytes()
}
