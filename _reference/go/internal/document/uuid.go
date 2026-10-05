package document

import (
	"errors"
	"slices"
)

// UUID is an RFC 9562 unique identifier in its 128-bit big-endian form
// (FORMAT-SPEC §2.1 E1). The zero value is not a valid identifier; obtain
// values via ParseUUID.
type UUID [16]byte

// ErrInvalidUUID reports input that is not a canonical lowercase hyphenated
// RFC 9562 UUID of exactly 36 characters (FORMAT-SPEC §2.1 E1, parser
// bound L19).
var ErrInvalidUUID = errors.New("invalid-field-syntax: entity/batch/operation id is not a canonical RFC 9562 UUID")

// ParseUUID validates s against the canonical lowercase hyphenated textual
// form, exactly 36 characters, and returns its 128-bit value.
//
// Version opacity (E2): the four-bit version field is checked for
// well-formedness only — every nibble is well-formed, so no rule here may
// depend on the version. The two-bit variant field must be the RFC 9562
// variant `10`; other variants are not RFC 9562 UUIDs and reject.
func ParseUUID(s string) (UUID, error) {
	var u UUID
	if len(s) != 36 {
		return u, ErrInvalidUUID
	}
	for _, i := range [...]int{8, 13, 18, 23} {
		if s[i] != '-' {
			return u, ErrInvalidUUID
		}
	}
	hex := "0123456789abcdef"
	pos := 0
	for i := 0; i < 36; i++ {
		if s[i] == '-' {
			continue
		}
		hi := -1
		for j := 0; j < 16; j++ {
			if s[i] == hex[j] {
				hi = j
				break
			}
		}
		if hi < 0 {
			return u, ErrInvalidUUID
		}
		u[pos/2] |= byte(hi)
		if pos%2 == 0 {
			u[pos/2] <<= 4
		}
		pos++
	}
	if u[8]&0xC0 != 0x80 {
		return u, ErrInvalidUUID
	}
	return u, nil
}

// String renders the canonical lowercase hyphenated form.
func (u UUID) String() string {
	const hexDigits = "0123456789abcdef"
	out := make([]byte, 36)
	pos := 0
	for i := 0; i < 16; i++ {
		if i == 4 || i == 6 || i == 8 || i == 10 {
			out[pos] = '-'
			pos++
		}
		out[pos] = hexDigits[u[i]>>4]
		out[pos+1] = hexDigits[u[i]&0x0F]
		pos += 2
	}
	return string(out)
}

// Compare orders two UUIDs by ascending unsigned 128-bit integer value,
// most significant octet first — the E5 total order used wherever stable
// UUIDs are compared.
func (u UUID) Compare(other UUID) int {
	for i := range u {
		if u[i] != other[i] {
			if u[i] < other[i] {
				return -1
			}
			return 1
		}
	}
	return 0
}

// SortUUIDs sorts ids in place into ascending E5 order.
func SortUUIDs(ids []UUID) {
	slices.SortFunc(ids, UUID.Compare)
}
