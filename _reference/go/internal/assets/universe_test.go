package assets

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

func validDescriptor(t *testing.T, content string) (*Descriptor, []byte) {
	t.Helper()
	d := &Descriptor{
		Digest:                digestPrefix + hexOfBlob([]byte(content)),
		HexDigest:             hexOfBlob([]byte(content)),
		ByteLength:            2048,
		MediaType:             "image/png",
		Purpose:               "background-image",
		Encoding:              EncodingIdentity,
		Requirement:           RequirementRequired,
		ProvenanceDisplayName: "Wood floor tile",
		ProvenanceSource:      "user import",
		AcquiredAt:            "2026-08-24T00:00:00Z",
	}
	raw, err := BuildDescriptorJSON(d)
	if err != nil {
		t.Fatalf("BuildDescriptorJSON: %v", err)
	}
	if _, perr := ParseDescriptor(raw); perr != nil {
		t.Fatalf("round trip parse failed: %v", perr)
	}
	return d, raw
}

func TestParseDescriptorRejectsInvalidShapes(t *testing.T) {
	_, base := validDescriptor(t, "descriptor-under-test")
	reject := func(name string, mutate func(string) string) {
		t.Helper()
		raw := []byte(mutate(string(base)))
		if _, err := ParseDescriptor(raw); err == nil {
			t.Fatalf("%s: accepted an invalid descriptor", name)
		}
	}
	reject("unprefixed digest key", func(s string) string {
		return strings.Replace(s, digestPrefix+hexOfBlob([]byte("descriptor-under-test")),
			hexOfBlob([]byte("descriptor-under-test")), 1)
	})
	reject("bad requirement enum", func(s string) string {
		return strings.Replace(s, `"requirement":"required"`, `"requirement":"sometimes"`, 1)
	})
	reject("unknown encoding profile", func(s string) string {
		return strings.Replace(s, `"encoding":"identity"`, `"encoding":"zstd-rfc9000"`, 1)
	})
	reject("mediaType without subtype", func(s string) string {
		return strings.Replace(s, `"mediaType":"image/png"`, `"mediaType":"image"`, 1)
	})
	reject("non-token purpose", func(s string) string {
		return strings.Replace(s, `"purpose":"background-image"`, `"purpose":"has spaces"`, 1)
	})
	reject("byteLength negative", func(s string) string {
		return strings.Replace(s, `"byteLength":2048`, `"byteLength":-1`, 1)
	})
	reject("timestamp not UTC fixed form", func(s string) string {
		return strings.Replace(s, "2026-08-24T00:00:00Z", "2026-08-24T00:00:00+02:00", 1)
	})
}

func TestHintValidationRules(t *testing.T) {
	hint := func(url string) string {
		return `{"digest":"` + digestPrefix + strings.Repeat("a", 64) + `","byteLength":1,` +
			`"mediaType":"a/b","purpose":"p","encoding":"identity","requirement":"required",` +
			`"provenance":{"displayName":"","sourceDescription":"","author":"","acquiredAt":"2026-01-01T00:00:00Z"},` +
			`"hints":[{"url":"` + url + `","addedAt":"2026-01-01T00:00:00Z"}]}`
	}
	for _, url := range []string{
		"http://example.com/a.png",      // scheme outside the default allowlist
		"https://u:p@example.com/a.png", // userinfo prohibited
		"https://127.0.0.1/a.png",       // IP-literal host rejected
		"https://169.254.169.254/meta",  // link-local IP literal rejected
		"/relative/path.png",            // not absolute
	} {
		if _, err := ParseDescriptor([]byte(hint(url))); err == nil {
			t.Fatalf("hint %q accepted", url)
		}
	}
	ok := hint("https://Example.COM./tile.png") // normalization target form
	if _, err := ParseDescriptor([]byte(ok)); err != nil {
		t.Fatalf("valid https hint rejected: %v", err)
	}
}

func mustDecodeEntity(t *testing.T, raw json.RawMessage) document.Entity {
	t.Helper()
	e, err := document.DecodeEntity(raw)
	if err != nil {
		t.Fatalf("DecodeEntity: %v", err)
	}
	return e
}

func TestUniverseCoversAllThreeSources(t *testing.T) {
	descDigest := fmt.Sprintf("%064x", 0x10)
	rootDigest := fmt.Sprintf("%064x", 0x20)
	instDigest := fmt.Sprintf("%064x", 0x30)

	levelID := "01963456-7abc-7def-8000-000000000001"
	instID := "01963456-7abc-7def-8000-000000000002"

	levelRaw := json.RawMessage(`{"id":"` + levelID + `","type":"level","name":"Ground floor",` +
		`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2700,` +
		`"backgroundImages":[{"digest":"sha256:` + rootDigest + `","anchor":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0},"stackOrdinal":0}]}`)
	instanceRaw := json.RawMessage(`{"id":"` + instID + `","type":"assetInstance","levelId":"` + levelID + `",` +
		`"digest":"sha256:` + instDigest + `","transform":{"translationMm":[100,100],"scaleUniform":1,"rotationDeg":90}}`)

	descriptors := json.RawMessage(`{"sha256:` + descDigest + `":{"digest":"sha256:` + descDigest +
		`","byteLength":10,"mediaType":"image/png"}}`)

	pkg, berr := persist.BuildPackage(&persist.WriteInput{
		Profile: persist.ProfilePortable(),
		State: document.State{
			Entities: []document.Entity{mustDecodeEntity(t, levelRaw), mustDecodeEntity(t, instanceRaw)},
		},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{},
		AssetRoot:       []string{"sha256:" + fmt.Sprintf("%064x", 0x40)}, // root-table-only entry
		DescriptorsRaw:  descriptors,
	})
	if berr != nil {
		t.Fatalf("BuildPackage: %v", berr)
	}
	view, lerr := persist.Load(pkg)
	if lerr != nil {
		t.Fatalf("Load: %v", lerr)
	}
	u, uerr := ReferencedUniverse(view)
	if uerr != nil {
		t.Fatalf("ReferencedUniverse: %v", uerr)
	}
	rootOnly := "sha256:" + fmt.Sprintf("%064x", 0x40)
	want := []string{"sha256:" + descDigest, "sha256:" + rootDigest, "sha256:" + instDigest, rootOnly}
	if len(u.Digests) != 4 {
		t.Fatalf("U(P) incomplete: got %v want %v", u.Digests, want)
	}
	for i, id := range want {
		if u.Digests[i] != id {
			t.Fatalf("U(P)[%d]=%s want %s (full set %v)", i, u.Digests[i], id, u.Digests)
		}
	}
	if len(u.FromDescriptors) != 1 || u.FromRootTable[0] != fmt.Sprintf("%064x", 0x40) ||
		len(u.FromInstances) != 2 || len(u.FromRootTable) != 1 {
		t.Fatalf("source provenance wrong: %+v", u)
	}
	if u.Digests[3] != rootOnly {
		t.Fatalf("root-table-only digest missing from union tail: %v", u.Digests)
	}
}

func TestUniverseRejectsMalformedInstanceDigest(t *testing.T) {
	levelID := "01963456-7abc-7def-8000-000000000001"
	instanceRaw := json.RawMessage(`{"id":"01963456-7abc-7def-8000-000000000002","type":"assetInstance",` +
		`"levelId":"` + levelID + `","digest":"sha256:ZZZ","transform":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0}}`)
	// DecodeEntity itself rejects malformed digests; verify that boundary.
	if _, err := document.DecodeEntity(instanceRaw); err == nil {
		t.Fatal("malformed instance digest decoded")
	}
}
