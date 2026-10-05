package assets

import (
	"strings"

	"domowik.dev/domowik/internal/persist"
)

// digest.go: digest identity helpers over ASSET-STRATEGY §10.1's
// `sha256:<64 lowercase hex>` identifier space. Uppercase hex or
// unprefixed forms are invalid identifiers.

const (
	digestPrefix = "sha256:"
	hexDigestLen = 64
)

// parseHexDigest validates a full digest identifier and returns the bare
// lowercase hex form.
func parseHexDigest(id string) (string, error) {
	if err := persist.ValidateDigestID(id); err != nil {
		return "", err
	}
	return strings.TrimPrefix(id, digestPrefix), nil
}

// checkHexDigest validates a bare 64-character lowercase hex digest.
func checkHexDigest(hex string) error {
	return persist.ValidateDigestID(digestPrefix + hex)
}

// blobRelPath maps a hex digest to its logical blob path per the §10.3
// mapping `blob/<algorithm>/<2-hex>/<62-hex>`: deterministic, injective,
// free of filesystem path semantics, and stable across thin/portable
// conversion. The same mapping names cache files under the store root.
func blobRelPath(hex string) string {
	return "blob/sha256/" + hex[:2] + "/" + hex[2:]
}
