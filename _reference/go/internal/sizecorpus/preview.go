package sizecorpus

import (
	"bytes"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"
	"hash/crc32"
	"os/exec"
	"strings"
)

// Reference thumbnail encoder, per ASSET-STRATEGY §10.9: PNG signature;
// IHDR(512,512,8,6,0,0,0); one IDAT carrying DEFLATE (zlib level 9, default
// strategy) over 512 scanlines of 00 + 2048 x FF; IEND. The pinned evidence
// vector is the zlib 1.3.2 output: 1809 bytes total. Environments whose
// output differs FAIL the check; there is no silent acceptance. Go's
// compress/zlib does not reproduce this DEFLATE byte stream (measured), so
// the encoder shells out to python3's unmodified stdlib zlib module — a
// dev-only tool recorded in docs/third-party/tools.json whose effective
// runtime matches the §10.10-recorded zlib 1.3.2 artifact.
const (
	refPNGWidthPx   = 512
	refPNGHeightPx  = 512
	refPNGLength    = 1809
	refPNGSHA256Hex = "49c8c56d509296669f01cf18365984d4b4a3920b9da04b137eb0443d6eefe29f"
)

// PythonAvailable reports whether the reference encoder's interpreter can be
// located at all.
func PythonAvailable() bool {
	_, err := exec.LookPath("python3")
	return err == nil
}

// ZlibRuntimeVersion reports the zlib runtime version behind python3's
// stdlib, for measurement records.
func ZlibRuntimeVersion() (string, error) {
	out, err := exec.Command("python3", "-c", "import zlib;print(zlib.ZLIB_RUNTIME_VERSION)").Output()
	if err != nil {
		return "", fmt.Errorf("query python3 zlib runtime version: %w", err)
	}
	return strings.TrimSpace(string(out)), nil
}

func execPython(args ...string) string {
	out, err := exec.Command("python3", args...).Output()
	if err != nil {
		return fmt.Sprintf("(python3 %v failed: %v)\n", args, err)
	}
	return string(out)
}

// ReferenceThumbnail returns the §10.9 reference thumbnail PNG bytes,
// failing unless both length and SHA-256 match the pinned vector.
func ReferenceThumbnail() ([]byte, error) {
	png, err := assembleReferencePNG()
	if err != nil {
		return nil, err
	}
	if len(png) != refPNGLength {
		return nil, fmt.Errorf("reference thumbnail vector mismatch: got %d bytes, want %d", len(png), refPNGLength)
	}
	sum := sha256.Sum256(png)
	if got := hex.EncodeToString(sum[:]); got != refPNGSHA256Hex {
		return nil, fmt.Errorf("reference thumbnail vector mismatch: sha256 %s does not equal the pinned ASSET-STRATEGY §10.9 value; environment output differs and fails the check (no silent acceptance)", got)
	}
	return png, nil
}

func assembleReferencePNG() ([]byte, error) {
	idat, err := pyZlibDeflateScanlines()
	if err != nil {
		return nil, err
	}
	png := []byte{0x89, 'P', 'N', 'G', 0x0d, 0x0a, 0x1a, 0x0a}
	ihdr := make([]byte, 13)
	binary.BigEndian.PutUint32(ihdr[0:], refPNGWidthPx)
	binary.BigEndian.PutUint32(ihdr[4:], refPNGHeightPx)
	ihdr[8], ihdr[9] = 8, 6 // bit depth 8, color type 6 (RGBA)
	png = append(png, pngChunk("IHDR", ihdr)...)
	png = append(png, pngChunk("IDAT", idat)...)
	return append(png, pngChunk("IEND", nil)...), nil
}

func pngChunk(typ string, data []byte) []byte {
	out := make([]byte, 0, len(data)+12)
	var l [4]byte
	binary.BigEndian.PutUint32(l[:], uint32(len(data)))
	out = append(out, l[:]...)
	out = append(out, typ...)
	out = append(out, data...)
	sum := crc32.ChecksumIEEE(append([]byte(typ), data...))
	binary.BigEndian.PutUint32(l[:], sum)
	return append(out, l[:]...)
}

func pyZlibDeflateScanlines() ([]byte, error) {
	script := `import zlib,sys
sys.stdout.buffer.write(zlib.compress(b"".join(b"\x00" + b"\xff"*2048 for _ in range(512)), 9))
`
	var out bytes.Buffer
	cmd := exec.Command("python3", "-c", script)
	cmd.Stdout = &out
	cmd.Stderr = nil
	if err := cmd.Run(); err != nil {
		return nil, fmt.Errorf("python3 stdlib zlib DEFLATE failed: %w", err)
	}
	return out.Bytes(), nil
}

// PreviewSourceKey names the derived-from key recorded in preview records.
const PreviewSourceKey = "sizecorpus/thumbnail"
