package document

import (
	"strings"
	"testing"
)

func TestParseUUIDAcceptsCanonicalForm(t *testing.T) {
	tests := []struct {
		in   string
		want UUID
	}{
		{"019534c7-0000-7000-8000-000000000001", UUID{
			0x01, 0x95, 0x34, 0xc7, 0x00, 0x00, 0x70, 0x00,
			0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
		}},
		{"ffffffff-ffff-7fff-bfff-ffffffffffff", UUID{
			0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f, 0xff,
			0xbf, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
		}},
	}
	for _, tc := range tests {
		u, err := ParseUUID(tc.in)
		if err != nil {
			t.Fatalf("ParseUUID(%q): %v", tc.in, err)
		}
		if u.Compare(tc.want) != 0 {
			t.Fatalf("ParseUUID(%q) = %v, want %v", tc.in, u.String(), tc.want.String())
		}
		if got := u.String(); got != tc.in {
			t.Fatalf("round trip: got %q want %q", got, tc.in)
		}
	}
}

func TestParseUUIDVersionOpacity(t *testing.T) {
	// E2: any RFC-valid version nibble is accepted; no rule may depend on
	// the version.
	for _, in := range []string{
		"00000000-0000-1000-8000-000000000000",
		"00000000-0000-4000-8000-000000000000",
		"00000000-0000-f000-8000-000000000000",
	} {
		if _, err := ParseUUID(in); err != nil {
			t.Fatalf("ParseUUID(%q): %v", in, err)
		}
	}
}

func TestParseUUIDRejects(t *testing.T) {
	valid := "019534c7-0000-7000-8000-000000000001"
	tests := []struct {
		name string
		in   string
	}{
		{"empty", ""},
		{"too short", valid[:35]},
		{"too long", valid + "0"},
		{"uppercase hex", strings.ToUpper(valid)},
		{"missing hyphen", "019534c7000070008000000000000001"},
		{"hyphen misplaced", "019534c-7000-07000800-000000000001"},
		{"non-hex glyph", "019534cg-0000-7000-8000-000000000001"},
		{"wrong variant NCS", "019534c7-0000-7000-0000-000000000001"},
		{"wrong variant reserved", "019534c7-0000-7000-c000-000000000001"},
	}
	for _, tc := range tests {
		if _, err := ParseUUID(tc.in); err == nil {
			t.Fatalf("%s: ParseUUID(%q) accepted", tc.name, tc.in)
		}
	}
}

func TestUUIDCompareIsUnsignedBigEndianOrder(t *testing.T) {
	low := mustParseUUID(t, "00000000-0000-7000-8000-000000000001")
	high := mustParseUUID(t, "ffffffff-ffff-7fff-8000-000000000000")
	if low.Compare(high) >= 0 || high.Compare(low) <= 0 || low.Compare(low) != 0 {
		t.Fatal("E5 total order violated")
	}
	msbFirst := mustParseUUID(t, "80000000-0000-7000-8000-000000000000")
	small := mustParseUUID(t, "00000000-0000-7000-8000-000000000000")
	if msbFirst.Compare(small) <= 0 {
		t.Fatal("comparison must weigh the most significant octet first")
	}
}
