package sizecorpus

import (
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"fmt"

	"domowik.dev/domowik/internal/document"
)

// Pinned ASSET-STRATEGY §10.9 literals. The seeds are the first 16 bytes of
// SHA-256 over the exact ASCII input (no trailing newline); the RNG vector is
// the first 8 bytes of SHA-256 over ASCII(domain) || LE64(counter), read as a
// little-endian uint64.
const (
	SeedEmptyHex       = "6bcb425196e349a259d7d4861e76edf7"
	SeedUnfurnishedHex = "d2ef08ed140beebf797b8a2ddf34bf1c"

	rngSelfCheckDomain = "domowik-corpus/rng/selfcheck"
	rngSelfCheckVector = uint64(0x88DBCD98B68422D4)
)

const seedInputPrefix = "domowik-corpus/"

// Scenario seeds by name.
func seedBytes(scenario string) ([]byte, error) {
	var hexSeed string
	switch scenario {
	case "empty-v1":
		hexSeed = SeedEmptyHex
	case "unfurnished-1000-v1":
		hexSeed = SeedUnfurnishedHex
	default:
		return nil, fmt.Errorf("unknown scenario %q", scenario)
	}
	b, err := hex.DecodeString(hexSeed)
	if err != nil {
		return nil, fmt.Errorf("scenario %s seed: %w", scenario, err)
	}
	return b, nil
}

// NextRNG implements the §10.9 deterministic primitive
// next(domain, counter) = first 8 bytes of SHA-256(ASCII(domain) ‖ LE64(counter)),
// interpreted little-endian. It serves extension slots only; all Part A
// geometry and identity values are formula-determined literals.
func NextRNG(domain string, counter uint64) uint64 {
	h := sha256.New()
	h.Write([]byte(domain))
	var ctr [8]byte
	binary.LittleEndian.PutUint64(ctr[:], counter)
	h.Write(ctr[:])
	return binary.LittleEndian.Uint64(h.Sum(nil)[:8])
}

// SelfCheckRNGVector verifies the §10.9 RNG vector: domain
// "domowik-corpus/rng/selfcheck", counter 0 → 0x88DBCD98B68422D4.
func SelfCheckRNGVector() error {
	if got := NextRNG(rngSelfCheckDomain, 0); got != rngSelfCheckVector {
		return fmt.Errorf("rng self-check vector mismatch: got %#016x want %#016x", got, rngSelfCheckVector)
	}
	return nil
}

// TagUUID derives an entity identifier per ASSET-STRATEGY §10.9 read with
// FORMAT-SPEC §2.1 E4: tag = first 16 bytes of SHA-256(seedBytes ‖ ASCII(path));
// the four-bit version field (high nibble of octet 6) is overwritten with
// UUIDv8 (1000) and the two-bit variant field (top bits of octet 8) with the
// RFC 9562 value 10. The remaining 122 tag bits are carried unchanged; the
// six overwritten bits are the documented E4 cost and expected values are
// pinned only after the overwrite. The result always parses via
// document.ParseUUID.
func TagUUID(seed []byte, path string) document.UUID {
	h := sha256.New()
	h.Write(seed)
	h.Write([]byte(path))
	var u document.UUID
	copy(u[:], h.Sum(nil)[:16])
	u[6] = (u[6] & 0x0f) | 0x80
	u[8] = (u[8] & 0x3f) | 0x80
	return u
}
