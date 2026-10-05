package document

import (
	"slices"
)

// canonicalizeFloat maps negative zero to positive zero (FORMAT-SPEC
// §3.2): validation canonicalizes -0 to +0 so equality comparisons on
// stored values are simultaneously numerical and bit-safe. The value must
// already be known finite (L13); non-finite inputs are rejected before
// this function is reached.
func canonicalizeFloat(v float64) float64 {
	if v == 0 {
		return 0
	}
	return v
}

// compareFloatTotal orders two finite binary64 values ascending after
// negative-zero canonicalization. C3 invokes the IEEE-754 total order for
// binary64; because every value passing validation is finite and
// de-canonicalized (-0 mapped to +0), the plain numeric comparison
// coincides exactly with totalOrder on the admitted domain.
func compareFloatTotal(a, b float64) int {
	if a < b {
		return -1
	}
	if a > b {
		return 1
	}
	return 0
}

// sortSetElements sorts elements into their declared canonical order and
// reports whether any two adjacent elements were equal under the full sort
// key (C3: permutations are equivalent, duplicates reject deterministically
// because the key covers every serialized field).
func sortSetElements[E any](elems []E, cmp func(a, b E) int) bool {
	slices.SortStableFunc(elems, cmp)
	for i := 1; i < len(elems); i++ {
		if cmp(elems[i-1], elems[i]) == 0 {
			return false
		}
	}
	return true
}

// compareByteString compares two strings lexicographically by byte value
// (ascending), as required for digest components of the C3 background-image
// tuple.
func compareByteString(a, b string) int {
	switch {
	case a < b:
		return -1
	case a > b:
		return 1
	default:
		return 0
	}
}
