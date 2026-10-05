package persist_test

import (
	"testing"

	"domowik.dev/domowik/internal/persist"
)

// TestZeroFrontierMemberGap demonstrates that wire.go:523 accepts zero-valued
// frontier members, exposing a gap between the wire parser and F1's prohibition.
func TestZeroFrontierMemberGap(t *testing.T) {
	// Load the hostile fixture with zero-valued frontier member
	raw := corpusFile(t, "hostile/framing/zero-frontier-member.json")

	// Assemble into a minimal package
	pkg, err := persist.AssembleRawPackage(map[string][]byte{
		"snapshot/state.json": raw,
	})
	if err != nil {
		t.Fatalf("AssembleRawPackage: %v", err)
	}

	// Attempt to load - currently PASSES (demonstrates gap)
	// After fix: should REJECT with invalid-field-syntax at StageSchema
	_, loadErr := persist.Load(pkg)
	if loadErr == nil {
		t.Errorf("DEMONSTRATES GAP: zero-valued frontier member was ACCEPTED but F1 forbids it")
		t.Logf("Expected: reject with invalid-field-syntax at StageSchema (per issue 0005)")
		t.Logf("Actual: accepted and normalized at snapshot.go:106")
		t.Logf("Gap location: wire.go:523 checks n<0 but not n==0")
	} else {
		cat, ok := persist.CategoryOf(loadErr)
		if !ok {
			t.Fatalf("error has no category: %v", loadErr)
		}
		if cat != persist.CatInvalidFieldSyntax {
			t.Errorf("expected invalid-field-syntax, got %s: %v", cat, loadErr)
		} else {
			t.Logf("Correctly rejects with invalid-field-syntax")
		}
	}
}
