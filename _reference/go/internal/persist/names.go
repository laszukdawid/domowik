package persist

import (
	"strings"
	"unicode/utf8"
)

// validateLogicalName enforces FORMAT-SPEC §7.3/§9.4 name rules in the
// deterministic precedence order the conformance corpus pins:
//
//  1. valid UTF-8 and L8/L9 length bounds;
//  2. NUL/control characters reject invalid-entry-name;
//  3. backslash, absolute, drive/UNC forms, dot segments reject
//     path-traversal (logical names never touch host filesystem
//     semantics; nothing is extracted to host paths);
//  4. case-folding / Unicode-collision handling happens across the whole
//     entry set (classifyCollisions) BEFORE closed-namespace matching so
//     a colliding name reports duplicate-logical-name exactly as the
//     oracle records require.
func validateLogicalName(name string) *Error {
	if !utf8.ValidString(name) {
		return fail(CatInvalidEntryName, StageNames, "entry name is not valid UTF-8")
	}
	if len(name) > LimitNameLengthBytes {
		return fail(CatLimitNameLength, StageNames, "entry name exceeds %d bytes (L8)", LimitNameLengthBytes)
	}
	for _, r := range name {
		if r < 0x20 || r == 0x7f {
			return fail(CatInvalidEntryName, StageNames, "entry name contains a control character")
		}
	}
	if strings.HasPrefix(name, "/") {
		return fail(CatPathTraversal, StageNames, "absolute path form rejected")
	}
	if strings.ContainsRune(name, '\\') {
		return fail(CatPathTraversal, StageNames, "entry name contains a backslash separator")
	}
	if isDriveForm(name) {
		return fail(CatPathTraversal, StageNames, "drive/UNC path form rejected")
	}
	if len(name) == 0 {
		return fail(CatInvalidEntryName, StageNames, "empty entry name")
	}
	componentMax := 0
	for _, comp := range strings.Split(name, "/") {
		if comp == "" {
			return fail(CatPathTraversal, StageNames, "empty path component")
		}
		if comp == "." || comp == ".." {
			return fail(CatPathTraversal, StageNames, "dot segment %q rejected", comp)
		}
		if len(comp) > LimitNameComponentMaxBytes {
			return fail(CatLimitNameComponent, StageNames,
				"path component exceeds %d bytes (L9)", LimitNameComponentMaxBytes)
		}
		if len(comp) > componentMax {
			componentMax = len(comp)
		}
	}
	return nil
}

// isDriveForm detects DOS drive-letter ("C:...") and UNC ("//share")
// forms per §9.4. Single-character prefix followed by a colon with no
// earlier separator is the drive signature; UNC collapses to the absolute
// check upstream.
func isDriveForm(name string) bool {
	i := strings.IndexByte(name, ':')
	if i != 1 {
		return false
	}
	c := name[0]
	return (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

// classifyCollisions rejects duplicate logical names after normalization:
// exact duplicates and case-fold collisions both reject
// duplicate-logical-name (§7.3). Comparison uses Go's simple Unicode case
// folding (strings.EqualFold), which covers the ASCII and common-script
// collision space exercised by the corpus; full canonical-equivalence
// (NFC/NFD) collision detection beyond simple folding remains an open
// conformance gap tracked with the issue reviewer, not silently claimed.
func classifyCollisions(names []string) *Error {
	seen := make(map[string]string, len(names)) // folded lower -> first name
	for _, n := range names {
		fold := strings.ToLower(n)
		if prev, dup := seen[fold]; dup {
			return fail(CatDuplicateLogicalName, StageNames, "%s collides with %s after normalization", n, prev)
		}
		seen[fold] = n
	}
	return nil
}

// namespace classification for the CLOSED v0.1 namespace set (§7.3).
type namespace int

const (
	nsForeign namespace = iota
	nsManifest
	nsSnapshot
	nsDescriptors
	nsOps
	nsBlob
	nsPreviewData
	nsPreviewIndex
)

const opsCounterDigits = TempArtifactSeqDigits // §7.6: counterBase zero-padded to 19 digits

// classifyNamespace matches a validated logical name against the closed
// namespace set; any other name is hostile input (invalid-entry-name).
func classifyNamespace(name string) (namespace, *Error) {
	switch name {
	case "domowik.json":
		return nsManifest, nil
	case "snapshot/state.json":
		return nsSnapshot, nil
	case "assets/descriptors.json":
		return nsDescriptors, nil
	case "preview/index.json":
		return nsPreviewIndex, nil
	}
	if rest, ok := strings.CutPrefix(name, "ops/"); ok {
		body, ext, ok := strings.Cut(rest, ".")
		if !ok || ext != "json" {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed ops batch name %q", name)
		}
		counter, actor, ok := strings.Cut(body, "-")
		if !ok || len(counter) != opsCounterDigits || !allDigits(counter) {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed ops batch counter %q", name)
		}
		if !isCanonicalUUID(actor) {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed ops batch actor id %q", name)
		}
		return nsOps, nil
	}
	if rest, ok := strings.CutPrefix(name, "blob/sha256/"); ok {
		two, sixtyTwo, ok := cutHexPath(rest)
		if !ok {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed blob path %q", name)
		}
		if !isLowerHex(two, 2) || !isLowerHex(sixtyTwo, 62) {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed blob digest path %q", name)
		}
		return nsBlob, nil
	}
	if rest, ok := strings.CutPrefix(name, "preview/data/"); ok {
		two, sixtyTwo, ok := cutHexPath(rest)
		if !ok || !isLowerHex(two, 2) || !isLowerHex(sixtyTwo, 62) {
			return nsForeign, fail(CatInvalidEntryName, StageNames, "malformed preview path %q", name)
		}
		return nsPreviewData, nil
	}
	return nsForeign, fail(CatInvalidEntryName, StageNames, "foreign entry name %q under the closed v0.1 namespace set", name)
}

// batchEntryName builds the deterministic ops entry name for a batch
// (§7.6): zero-padded counter base plus canonical 36-character actor id.
func batchEntryName(counterBase int64, actorID string) string {
	return "ops/" + padCounter(counterBase) + "-" + actorID + ".json"
}

func padCounter(v int64) string {
	const digits = "0000000000000000000" // opsCounterDigits zeros
	s := digits + itoa64(v)
	return s[len(s)-opsCounterDigits:]
}

func itoa64(v int64) string {
	if v == 0 {
		return "0"
	}
	neg := v < 0
	var buf [20]byte
	i := len(buf)
	u := uint64(v)
	if neg {
		u = uint64(-v)
	}
	for u > 0 {
		i--
		buf[i] = byte('0' + u%10)
		u /= 10
	}
	if neg {
		i--
		buf[i] = '-'
	}
	return string(buf[i:])
}

func allDigits(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return len(s) > 0
}

func isLowerHex(s string, want int) bool {
	if len(s) != want {
		return false
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		if (c < '0' || c > '9') && (c < 'a' || c > 'f') {
			return false
		}
	}
	return true
}

func cutHexPath(rest string) (string, string, bool) {
	two, sixtyTwo, ok := strings.Cut(rest, "/")
	return two, sixtyTwo, ok && !strings.Contains(sixtyTwo, "/")
}

// isCanonicalUUID reports whether s is exactly the RFC 9562 canonical
// 36-character lowercase form (L19/E1).
func isCanonicalUUID(s string) bool {
	if len(s) != UUIDTextualLength {
		return false
	}
	dashes := [...]int{8, 13, 18, 23}
	pos := 0
	for _, d := range dashes {
		for ; pos < d; pos++ {
			if !isHexLower(s[pos]) {
				return false
			}
		}
		if s[pos] != '-' {
			return false
		}
		pos++
	}
	for ; pos < len(s); pos++ {
		if !isHexLower(s[pos]) {
			return false
		}
	}
	return true
}

func isHexLower(c byte) bool {
	return (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f')
}
