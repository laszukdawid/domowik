package sizecorpus

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"sort"

	"domowik.dev/domowik/internal/persist"
)

// MeasureThinPackage packs the scenario through the delivered 0026 writer
// seam as a thin package — complete snapshot, zero retained operation
// entries (history framing is Part B and is never folded into headline
// budgets), an explicit empty descriptors object, and the reference
// thumbnail embedded as the only preview-class content — then decomposes
// the produced archive into per-entry classes for the measurement record.
func MeasureThinPackage(d *Document, thumbnail []byte) (*Measurement, error) {
	in := WriteInputFor(d, thumbnail)
	pkg, perr := persist.BuildPackage(in)
	if perr != nil {
		return nil, fmt.Errorf("writer seam rejected package: %v", perr)
	}
	m, err := decompose(pkg)
	if err != nil {
		return nil, err
	}
	m.Package.SHA256 = "sha256:" + hexString(sha256Of(pkg))
	m.UniverseSize = 0 // zero descriptors and zero instances: U(P) = {} (§10.5)
	return m, nil
}

// PreviewIndexJSON renders the derived-cache index entry content for the
// reference thumbnail (reader-validated as an opaque listed entry).
func PreviewIndexJSON(thumbnail []byte) []byte {
	pngHex := hexString(sha256Of(thumbnail))
	return fmt.Appendf(nil,
		`{"previews":[{"digest":"sha256:%s","byteLength":%d,"sourceKey":"%s"}]}`,
		pngHex, len(thumbnail), PreviewSourceKey)
}

// WriteInputFor assembles the thin-profile writer input of a scenario:
// complete state, no retained batches, explicit empty descriptors object,
// and the reference thumbnail as index + data preview entries.
func WriteInputFor(d *Document, thumbnail []byte) *persist.WriteInput {
	pngHex := hexString(sha256Of(thumbnail))
	return &persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           d.State,
		RetainedBatches: nil,
		DescriptorsRaw:  []byte(`{}`),
		PreviewEntries: map[string][]byte{
			"preview/index.json":                            PreviewIndexJSON(thumbnail),
			"preview/data/" + pngHex[:2] + "/" + pngHex[2:]: thumbnail,
		},
	}
}

// Measurement is the physical decomposition of one thin package.
type Measurement struct {
	Package      PackageInfo
	Entries      []EntryMeasure
	ClassBytes   map[string]int // class -> physical bytes (entries only)
	ClassLogical map[string]int // class -> logical (manifest-inventoried) bytes
	Overhead     int            // container bytes outside entry local spans
	UniverseSize int            // U(P) membership count; previews are not members (§10.5)
}

// PackageInfo identifies one produced package.
type PackageInfo struct {
	Profile   string `json:"profile"`
	SHA256    string `json:"sha256"`
	ByteCount int    `json:"byteCount"`
}

// EntryMeasure records both physical archive cost and logical inventoried
// size of one entry.
type EntryMeasure struct {
	Name          string `json:"name"`
	Class         string `json:"class"`
	Method        string `json:"method"`
	LogicalBytes  uint64 `json:"logicalBytes"`
	PhysicalBytes int    `json:"physicalBytes"`
}

// Class names of the AC breakdown (manifest | descriptors | snapshot |
// history reported separately from content | preview line items).
const (
	classManifest     = "manifest"
	classDescriptors  = "descriptors"
	classSnapshot     = "snapshot"
	classHistory      = "history"
	classPreviewIndex = "previewIndex"
	classPreviewData  = "previewData"
)

func classOf(name string) (string, error) {
	switch name {
	case "domowik.json":
		return classManifest, nil
	case "assets/descriptors.json":
		return classDescriptors, nil
	case "snapshot/state.json":
		return classSnapshot, nil
	case "preview/index.json":
		return classPreviewIndex, nil
	}
	const opsPrefix = "ops/"
	const pdPrefix = "preview/data/"
	if len(name) > len(opsPrefix) && name[:len(opsPrefix)] == opsPrefix {
		return classHistory, nil
	}
	if len(name) > len(pdPrefix) && name[:len(pdPrefix)] == pdPrefix {
		return classPreviewData, nil
	}
	return "", fmt.Errorf("entry %q outside known classes", name)
}

type cdEntry struct {
	name       string
	method     uint16
	uncompSize uint64
	localOff   int
}

// decompose parses our own archive: a central-directory walk gives per-entry
// methods and sizes; contiguous local spans give exact physical costs whose
// sum must equal the package length exactly (accounting identity).
func decompose(pkg []byte) (*Measurement, error) {
	cdOffset, cdSize, err := parseEOCD(pkg)
	if err != nil {
		return nil, err
	}
	var entries []cdEntry
	for off := cdOffset; off < cdOffset+cdSize; {
		if !hasSig(pkg, off, sigCentralDirectory) {
			return nil, fmt.Errorf("central directory corrupt at %d", off)
		}
		method := binary.LittleEndian.Uint16(pkg[off+10:])
		uncomp := binary.LittleEndian.Uint32(pkg[off+24:])
		nameLen := int(binary.LittleEndian.Uint16(pkg[off+28:]))
		extraLen := int(binary.LittleEndian.Uint16(pkg[off+30:]))
		commentLen := int(binary.LittleEndian.Uint16(pkg[off+32:]))
		localOff := int(binary.LittleEndian.Uint32(pkg[off+42:]))
		name := string(pkg[off+46 : off+46+nameLen])
		entries = append(entries, cdEntry{name: name, method: method, uncompSize: uint64(uncomp), localOff: localOff})
		off += 46 + nameLen + extraLen + commentLen
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].localOff < entries[j].localOff })

	logical, err := manifestInventory(pkg, findManifest(entries))
	if err != nil {
		return nil, err
	}

	m := &Measurement{ClassBytes: map[string]int{}, ClassLogical: map[string]int{}}
	m.Package.Profile = persist.ProfileThin()
	m.Package.ByteCount = len(pkg)
	entrySum := 0
	for i, e := range entries {
		end := cdOffset
		if i+1 < len(entries) {
			end = entries[i+1].localOff
		}
		span := end - e.localOff
		if span <= 0 {
			return nil, fmt.Errorf("entry %q has non-positive physical span", e.name)
		}
		class, cerr := classOf(e.name)
		if cerr != nil {
			return nil, cerr
		}
		lb, ok := logical[e.name]
		if !ok {
			if e.name != "domowik.json" {
				return nil, fmt.Errorf("entry %q absent from manifest inventory", e.name)
			}
			lb = e.uncompSize // the manifest never inventories itself (§7.4)
		}
		if lb != e.uncompSize {
			return nil, fmt.Errorf("entry %q: manifest inventory disagrees with archive (%v vs %d)", e.name, lb, e.uncompSize)
		}
		m.Entries = append(m.Entries, EntryMeasure{
			Name: e.name, Class: class, Method: methodName(e.method),
			LogicalBytes: lb, PhysicalBytes: span,
		})
		m.ClassBytes[class] += span
		m.ClassLogical[class] += int(lb)
		entrySum += span
	}
	m.Overhead = len(pkg) - entrySum
	if m.Overhead < 0 {
		return nil, fmt.Errorf("container overhead negative")
	}
	sort.Slice(m.Entries, func(i, j int) bool { return m.Entries[i].Name < m.Entries[j].Name })
	return m, nil
}

func findManifest(entries []cdEntry) cdEntry {
	for _, e := range entries {
		if e.name == "domowik.json" {
			return e
		}
	}
	panic("package without manifest")
}

const (
	sigLocalHeader      = 0x04034b50
	sigCentralDirectory = 0x02014b50
	sigEndOfCentralDir  = 0x06054b50
)

// localDataStart returns the offset of entry payload data within its local
// header (local extra-field length may differ from the central one).
func localDataStart(pkg []byte, off int) int {
	nameLen := int(binary.LittleEndian.Uint16(pkg[off+26:]))
	extraLen := int(binary.LittleEndian.Uint16(pkg[off+28:]))
	return 30 + nameLen + extraLen
}

func parseEOCD(pkg []byte) (cdOffset, cdSize int, err error) {
	if len(pkg) < 22 || !hasSig(pkg, len(pkg)-22, sigEndOfCentralDir) {
		return 0, 0, fmt.Errorf("EOCD missing at fixed tail position (our writer emits no zip64 or comment)")
	}
	tail := len(pkg) - 22
	cdSize = int(binary.LittleEndian.Uint32(pkg[tail+12:]))
	cdOffset = int(binary.LittleEndian.Uint32(pkg[tail+16:]))
	return cdOffset, cdSize, nil
}

func hasSig(b []byte, off int, sig uint32) bool {
	if off < 0 || off+4 > len(b) {
		return false
	}
	return binary.LittleEndian.Uint32(b[off:]) == sig
}

func methodName(m uint16) string {
	if m == 0 {
		return "store"
	}
	if m == 8 {
		return "deflate"
	}
	return fmt.Sprintf("unknown-%d", m)
}

// manifestInventory extracts the inventoried logical byte lengths from the
// raw domowik.json bytes of the package.
func manifestInventory(pkg []byte, e cdEntry) (map[string]uint64, error) {
	if !hasSig(pkg, e.localOff, sigLocalHeader) {
		return nil, fmt.Errorf("local header of domowik.json not found")
	}
	start := e.localOff + localDataStart(pkg, e.localOff)
	raw := pkg[start : start+int(e.uncompSize)]
	var m struct {
		FormatVersion string            `json:"formatVersion"`
		Profile       string            `json:"profile"`
		AssetRoot     []json.RawMessage `json:"assetRoot"`
		Entries       map[string]struct {
			SHA256     string `json:"sha256"`
			ByteLength uint64 `json:"byteLength"`
		} `json:"entries"`
	}
	if err := json.Unmarshal(raw, &m); err != nil {
		return nil, fmt.Errorf("manifest unparsable: %w", err)
	}
	out := map[string]uint64{}
	for name, inv := range m.Entries {
		out[name] = inv.ByteLength
	}
	return out, nil
}

func sha256Of(b []byte) [32]byte { return sha256.Sum256(b) }

func hexString(b [32]byte) string { return hex.EncodeToString(b[:]) }
