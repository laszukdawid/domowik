package sizecorpus

import (
	"crypto/sha256"
	"fmt"
	"strings"

	"domowik.dev/domowik/internal/document"
)

// deriveIdentityReference is the SELF-CHECK ORACLE for §10.9/E4 identity
// derivation. It is deliberately implemented through different mechanics
// than the generator helper TagUUID so a defect in either cannot hide behind
// the other: one-shot SHA-256 over a single concatenated buffer (versus
// streaming hash.Write), manual textual UUID assembly with a lookup table
// (versus byte-array mutation), and the E4 version/variant overwrite applied
// at the character level before parsing through document.ParseUUID. It is
// exercised over EVERY generated identity — entities, operations, actor,
// batch — not sampled paths.
func deriveIdentityReference(seed []byte, path string) (document.UUID, error) {
	input := make([]byte, 0, len(seed)+len(path))
	input = append(input, seed...)
	input = append(input, path...)
	sum := sha256.Sum256(input)

	var sb strings.Builder
	for i := 0; i < 16; i++ {
		if i == 4 || i == 6 || i == 8 || i == 10 {
			sb.WriteByte('-')
		}
		b := sum[i]
		sb.WriteByte(hexDigit(b >> 4))
		sb.WriteByte(hexDigit(b & 0x0f))
	}
	text := []byte(sb.String())
	if len(text) != 36 {
		return document.UUID{}, fmt.Errorf("reference assembler produced %d characters", len(text))
	}

	// E4 at the textual level: octet 6 occupies characters 14-15; its high
	// nibble (character 14) becomes '8' (UUIDv8). Octet 8 occupies
	// characters 19-20; its high nibble (character 19) keeps its two low
	// bits and takes the RFC 9562 variant prefix 10, landing in 8..b.
	vn := digitVal(text[14])
	if vn < 0 {
		return document.UUID{}, fmt.Errorf("reference assembler produced non-hex version nibble %q", text[14])
	}
	text[14] = hexDigit(byte(vn&0x0) | 0x8)
	hn := digitVal(text[19])
	if hn < 0 {
		return document.UUID{}, fmt.Errorf("reference assembler produced non-hex variant nibble %q", text[19])
	}
	text[19] = hexDigit(byte(hn&0x3) | 0x8)

	ref := string(text)
	u, err := document.ParseUUID(ref)
	if err != nil {
		return document.UUID{}, fmt.Errorf("reference identity %s does not parse: %v", ref, err)
	}
	if u[6]>>4 != 8 || u[8]&0xC0 != 0x80 {
		return document.UUID{}, fmt.Errorf("reference identity %s violates the E4 layout", ref)
	}
	return u, nil
}

func hexDigit(v byte) byte {
	const digits = "0123456789abcdef"
	return digits[v&0x0f]
}

func digitVal(c byte) int {
	switch {
	case c >= '0' && c <= '9':
		return int(c - '0')
	case c >= 'a' && c <= 'f':
		return int(c-'a') + 10
	default:
		return -1
	}
}
