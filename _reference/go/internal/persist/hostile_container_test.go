package persist_test

import (
	"crypto/sha256"
	"encoding/binary"
	"io"
	"sort"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

// hexOf formats a digest as lowercase hex.
func hexOf(b []byte) string {
	const hexdigits = "0123456789abcdef"
	out := make([]byte, len(b)*2)
	for i, c := range b {
		out[i*2] = hexdigits[c>>4]
		out[i*2+1] = hexdigits[c&0xf]
	}
	return string(out)
}

func sha256HexOf(b []byte) string {
	sum := sha256.Sum256(b)
	return hexOf(sum[:])
}

// TestHostileContainerFixtures runs every hostile/container .domo fixture
// through the reader and asserts the exact ratified rejection token (or
// the pinned non-reject outcome) from the EXPECTATIONS.md oracle.
//
// Verdict semantics: EVERY record must match its oracle with an exact
// token; there is no open-question escape hatch. The three generator
// defects that previously required one — missing-manifest dropping the
// wrong entry, spanned-disk-number patching cdOffset instead of the disk
// field, and the ratio twin's post-manifest payload swap — are fixed at
// the generator (issue 0028 ED-0027 continuation), so a divergence here
// is now unconditionally a failure.
func TestHostileContainerFixtures(t *testing.T) {
	idx := expectationIndex(t)
	var containerIDs []string
	for id, e := range idx {
		if strings.HasPrefix(id, "hostile/container/") && strings.HasSuffix(e.Artifact, ".domo") {
			containerIDs = append(containerIDs, id)
		}
	}
	sort.Slice(containerIDs, func(i, j int) bool { return containerIDs[i] < containerIDs[j] })

	for _, id := range containerIDs {
		e := idx[id]
		t.Run(id, func(t *testing.T) {
			data := corpusFile(t, e.Artifact)
			if e.SHA256 != "" && sha256HexOf(data) != e.SHA256 {
				t.Fatalf("fixture integrity: sha256 of %s does not match the oracle record", e.Artifact)
			}
			_, err := persist.Load(data)
			if !matchesOracle(err, e.Outcome, deref(e.Category)) {
				got, _ := persist.CategoryOf(err)
				t.Fatalf("expected outcome=%s category=%s, got outcome=%s category=%s (%v)",
					e.Outcome, deref(e.Category), outcomeWord(err), got, err)
			}
		})
	}
	if len(containerIDs) == 0 {
		t.Fatal("no hostile container fixtures exercised")
	}
	// Subtest results are not observable from the parent, so recompute
	// divergence independently: the count must be zero.
	diverging := 0
	for _, id := range containerIDs {
		e := idx[id]
		data := corpusFile(t, e.Artifact)
		_, err := persist.Load(data)
		if !matchesOracle(err, e.Outcome, deref(e.Category)) {
			t.Errorf("mismatch for %s: reader outcome=%s", id, outcomeWord(err))
			diverging++
		}
	}
	if diverging != 0 {
		t.Errorf("hostile/container divergence count %d must be zero", diverging)
	}
	t.Logf("VERDICT: hostile/container = %d exact-token passes, 0 skips, 0 divergences",
		len(containerIDs))
}

// TestSpannedDiskFixtureDiffersOnlyInTheDiskField pins WHICH EOCD field
// the spanned-disk fixture corrupts. Its malformed-container token is
// reachable from several unrelated EOCD defects, so the category alone
// cannot prove the FS 7.2 single-disk requirement is what fired: the
// superseded fixture patched a cdOffset byte and still rejected — as
// malformed-length — which is exactly how that defect survived review.
//
// The fixture is the valid thin golden package with one field changed, so
// the assertion is byte-exact: same length, one differing byte, and that
// byte inside the EOCD thisDiskNumber field. The EOCD is a fixed 22-byte
// record when no archive comment is present — signature(0),
// thisDiskNumber(4), cdStartDisk(6), entriesOnThisDisk(8),
// totalEntries(10), cdSize(12), cdOffset(16), commentLength(20) — so
// restoring that field alone must make the bytes load cleanly.
func TestSpannedDiskFixtureDiffersOnlyInTheDiskField(t *testing.T) {
	const (
		eocdLen       = 22
		diskNumberOff = 4
		diskNumberLen = 2
		commentLenOff = 20
	)
	patched := corpusFile(t, "hostile/container/spanned-disk-number.domo")
	clean := corpusFile(t, "golden/packages/thin-valid.domo")
	if len(patched) != len(clean) {
		t.Fatalf("fixture length %d != clean package length %d: the patch resized the archive",
			len(patched), len(clean))
	}
	if len(patched) < eocdLen {
		t.Fatal("fixture is shorter than an EOCD record")
	}
	eocdAt := len(patched) - eocdLen
	if binary.LittleEndian.Uint16(patched[eocdAt+commentLenOff:]) != 0 {
		t.Fatal("fixture carries an archive comment, so the EOCD is not the trailing 22 bytes")
	}

	var differing []int
	for i := range patched {
		if patched[i] != clean[i] {
			differing = append(differing, i)
		}
	}
	if len(differing) != 1 {
		t.Fatalf("expected exactly one differing byte, got %d at %v", len(differing), differing)
	}
	if off := differing[0] - eocdAt; off < diskNumberOff || off >= diskNumberOff+diskNumberLen {
		t.Fatalf("the differing byte is at EOCD+%d, outside thisDiskNumber [%d,%d): FS 7.2 is not what this fixture probes",
			off, diskNumberOff, diskNumberOff+diskNumberLen)
	}
	if binary.LittleEndian.Uint16(patched[eocdAt+diskNumberOff:]) == 0 {
		t.Fatal("thisDiskNumber must be nonzero for a spanned-archive reject")
	}

	// Restoring only that field must recover a loadable package, which is
	// what makes the reject attributable to the single-disk requirement.
	if _, err := persist.Load(patched); err == nil {
		t.Fatal("precondition: the fixture must reject")
	}
	restored := append([]byte(nil), patched...)
	binary.LittleEndian.PutUint16(restored[eocdAt+diskNumberOff:], 0)
	if _, err := persist.Load(restored); err != nil {
		t.Fatalf("restoring thisDiskNumber must yield a loadable package, got %v", err)
	}
}

func matchesOracle(err error, outcome, category string) bool {
	switch outcome {
	case "reject":
		if err == nil {
			return false
		}
		got, ok := persist.CategoryOf(err)
		return ok && string(got) == category
	case "success":
		return err == nil
	case "missing-asset-equivalence":
		return err == nil
	default:
		return false
	}
}

func deref(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func outcomeWord(err error) string {
	if err == nil {
		return "success"
	}
	return "reject"
}

func sha256SumBytes(b []byte) []byte {
	sum := sha256.Sum256(b)
	return sum[:]
}

// TestStreamEntryRejectsDigestMismatch pins digest-before-exposure on
// the streaming surface: a blob whose bytes contradict its inventory
// SHA-256 drains with CRC and length intact but MUST fail Verified, so
// no decoder or admission path can consume it (FS 9.5).
func TestStreamEntryRejectsDigestMismatch(t *testing.T) {
	view, lerr := persist.Load(corpusFile(t, "hostile/container/digest-mismatch-portable.domo"))
	if lerr != nil {
		t.Fatalf("package loads degraded: %v", lerr)
	}
	if len(view.MissingAssets) == 0 {
		t.Fatal("fixture must classify its blob as missing-asset equivalent")
	}
	blobName := view.MissingAssets[0] // MissingAssets carries logical entry names
	es, serr := view.StreamEntry(blobName)
	if serr != nil {
		t.Fatalf("raw stream opens for bounded extraction: %v", serr)
	}
	// The mismatch arbitrates at end of stream: ReadAll surfaces it and
	// Verified() pins the checksum-class verdict, so no decoder or
	// admission path can treat these bytes as verified content.
	buf, rerr := io.ReadAll(es)
	if rerr == nil && es.Verified() == nil {
		t.Fatal("digest-mismatched stream must fail its integrity verdict")
	}
	_ = buf
	verr := es.Verified()
	if verr == nil {
		verr = rerr
	}
	if c, _ := persist.CategoryOf(verr); c != persist.CatChecksumError {
		t.Fatalf("expected checksum-class verdict, got %s (%v)", c, verr)
	}
}
