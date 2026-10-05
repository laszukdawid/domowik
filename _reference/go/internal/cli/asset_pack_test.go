package cli

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/assets"
	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

func runOK(t *testing.T, args []string) (string, string, int) {
	t.Helper()
	var stdout, stderr strings.Builder
	code := Run(args, &stdout, &stderr)
	return stdout.String(), stderr.String(), code
}

func newCache(t *testing.T) string {
	t.Helper()
	dir := filepath.Join(t.TempDir(), "cache")
	if _, err := assets.Open(dir); err != nil {
		t.Fatal(err)
	}
	return dir
}

const (
	testLevelID  = "01963456-7abc-7def-8000-000000000001"
	testBlobA    = "\x01\x02\x03\x04asset-alpha-payload"
	testBlobB    = "\x05\x06\x07\x08asset-beta-payload-with-more-bytes"
	testPreviewD = "5f1b3d2e4a6b8c0d9e7f2a3b4c5d6e7f8091a2b3c4d5e6f70819a2b3c4d5e6f7"
)

func hexOfBytes(b []byte) string {
	sum := sha256Hex(b)
	return sum
}

func TestAssetCommandLifecycle(t *testing.T) {
	cache := newCache(t)
	payload := []byte(testBlobA)
	src := filepath.Join(t.TempDir(), "tile.png")
	if err := os.WriteFile(src, payload, 0o644); err != nil {
		t.Fatal(err)
	}

	out, errOut, code := runOK(t, []string{"asset", "add",
		"--media-type", "image/png", "--purpose", "texture",
		"--display-name", "Tile", "--author", "tester", "--cache", cache, src})
	if code != 0 {
		t.Fatalf("add failed (%d): %s", code, errOut)
	}
	digest := extractField(t, out, "digest: ")
	if want := "sha256:" + sha256Hex(payload); digest != want {
		t.Fatalf("digest %s, want %s", digest, want)
	}
	if !strings.Contains(out, fmt.Sprintf("byteLength: %d", len(payload))) {
		t.Fatalf("byteLength line missing: %q", out)
	}

	if out, errOut, code = runOK(t, []string{"asset", "verify", "--cache", cache, digest}); code != 0 {
		t.Fatalf("verify failed (%d): %s%s", code, errOut, out)
	}

	out, _, _ = runOK(t, []string{"asset", "list", "--cache", cache})
	if !strings.Contains(out, digest) || !strings.Contains(out, "remote-bytes-logical: 0") ||
		!strings.Contains(out, fmt.Sprintf("cache-bytes-physical: %d", len(payload))) {
		t.Fatalf("list output incomplete:\n%s", out)
	}

	_, errOut, code = runOK(t, []string{"asset", "pin", "--owner", "doc-1", "--cache", cache, digest})
	if code != 0 {
		t.Fatalf("pin failed: %s", errOut)
	}
	_, errOut, code = runOK(t, []string{"asset", "remove", "--cache", cache, digest})
	if code != 1 || !strings.Contains(errOut, "pinned") {
		t.Fatalf("pinned removal must refuse with exit 1, got %d: %s", code, errOut)
	}
	if _, _, code = runOK(t, []string{"asset", "unpin", "--owner", "doc-1", "--cache", cache, digest}); code != 0 {
		t.Fatal("unpin failed")
	}
	out, _, code = runOK(t, []string{"asset", "gc", "--cache", cache})
	if code != 0 || !strings.Contains(out, "collected: 1\n") {
		t.Fatalf("gc did not collect the unpinned asset: %s%s", out, errOut)
	}
}

// TestPackProfilesPreservePreviewsAndEmbedUniverse builds a thin package
// whose level references one cached blob and whose descriptors name a
// second; packs portable (both embed), then thin again (neither embeds,
// previews survive every step).
func TestPackProfilesPreservePreviewsAndEmbedUniverse(t *testing.T) {
	cache := newCache(t)
	store, _ := assets.Open(cache)

	dA := importBytes(t, store, cache, []byte(testBlobA), "background-image")
	dB := importBytes(t, store, cache, []byte(testBlobB), "furniture-model")

	inputPath := filepath.Join(t.TempDir(), "input.domo")
	writeInputPackage(t, inputPath, cache, map[string][]byte{
		dA: []byte(testBlobA), dB: []byte(testBlobB),
	})

	portablePath := filepath.Join(t.TempDir(), "portable.domo")
	out, errOut, code := runOK(t, []string{"pack", "--portable", "--cache", cache, inputPath, portablePath})
	if code != 0 {
		t.Fatalf("portable pack failed (%d): %s%s", code, errOut, out)
	}
	// The input is thin: every U(P) member resolves externally from the
	// cache and gets embedded into the portable output.
	if !strings.Contains(out, "resolved-embedded-blobs: 0") ||
		!strings.Contains(out, "resolved-external-blobs: 2") ||
		!strings.Contains(out, fmt.Sprintf("logical-external-bytes: %d", len(testBlobA)+len(testBlobB))) ||
		!strings.Contains(out, "package-bytes-physical:") {
		t.Fatalf("size-class totals wrong before packing:\n%s", out)
	}
	view, lerr := persist.LoadFile(portablePath)
	if lerr != nil {
		t.Fatalf("portable package invalid: %v", lerr)
	}
	if view.Profile != "portable" || len(view.Blobs) != 2 {
		t.Fatalf("portable view wrong: profile=%s blobs=%d", view.Profile, len(view.Blobs))
	}
	assertPreviewsPreserved(t, view)
	if len(view.AssetRoot) != 2 {
		t.Fatalf("U(P) root table lost entries: %v", view.AssetRoot)
	}

	thinPath := filepath.Join(t.TempDir(), "thin.domo")
	out, errOut, code = runOK(t, []string{"pack", "--thin", "--cache", cache, portablePath, thinPath})
	if code != 0 {
		t.Fatalf("thin pack failed (%d): %s%s", code, errOut, out)
	}
	tview, lerr := persist.LoadFile(thinPath)
	if lerr != nil {
		t.Fatalf("thin package invalid: %v", lerr)
	}
	if tview.Profile != "thin" || len(tview.Blobs) != 0 {
		t.Fatalf("thin view wrong: profile=%s blobs=%d", tview.Profile, len(tview.Blobs))
	}
	assertPreviewsPreserved(t, tview)
	if len(tview.Projection.State().Entities) != 1 {
		t.Fatal("projection lost across conversions")
	}
}

// TestPortablePackReportsCompleteOmissions proves the preflight runs
// BEFORE any output exists and that the report names EVERY unresolved
// U(P) member, never just the first failure.
func TestPortablePackReportsCompleteOmissions(t *testing.T) {
	cache := newCache(t)
	store, _ := assets.Open(cache)

	good := importBytes(t, store, cache, []byte(testBlobA), "background-image")

	missing := fmt.Sprintf("%064x", 0xAA)
	corruptContent := []byte(testBlobB)
	corrupt := importBytes(t, store, cache, corruptContent, "furniture-model")
	// Corrupt the cached copy AFTER admission (bit rot simulation):
	// its digest identity stays in U(P), its bytes no longer verify.
	blobPath := store.BlobPath(corrupt)
	raw, _ := os.ReadFile(blobPath)
	raw[0] ^= 0xFF
	os.WriteFile(blobPath, raw, 0o644)

	inputPath := filepath.Join(t.TempDir(), "input.domo")
	writeInputPackage(t, inputPath, cache, map[string][]byte{
		good:    []byte(testBlobA),
		missing: []byte("\x00\x00missing-from-cache"),
		corrupt: corruptContent,
	})

	outputPath := filepath.Join(t.TempDir(), "never.domo")
	var stdout, stderr strings.Builder
	code := Run([]string{"pack", "--portable", "--cache", cache, inputPath, outputPath},
		&stdout, &stderr)
	if code != 1 {
		t.Fatalf("incomplete portable pack must exit 1, got %d\nstderr:%s", code, stderr.String())
	}
	if strings.Contains(stdout.String(), "packed:") {
		t.Fatal("aborted pack claimed success")
	}
	for _, d := range []string{missing, corrupt} {
		if !strings.Contains(stderr.String(), "sha256:"+d) {
			t.Fatalf("omission report lacks sha256:%s\nreport:\n%s", d, stderr.String())
		}
	}
	if !strings.Contains(stderr.String(), "reason: missing") || !strings.Contains(stderr.String(), "reason: corrupt") {
		t.Fatalf("omission report lacks reason classes:\n%s", stderr.String())
	}
	if _, err := os.Stat(outputPath); !os.IsNotExist(err) {
		t.Fatal("output file exists after aborted pack")
	}
}

// --- helpers ---

func sha256Hex(b []byte) string {
	sum := sha256.Sum256(b)
	return hex.EncodeToString(sum[:])
}

func extractField(t *testing.T, out, prefix string) string {
	t.Helper()
	for _, line := range strings.Split(out, "\n") {
		if after, ok := strings.CutPrefix(line, prefix); ok {
			return strings.TrimSpace(after)
		}
	}
	t.Fatalf("line with prefix %q absent in %q", prefix, out)
	return ""
}

func importBytes(t *testing.T, store *assets.Store, _ string, content []byte, purpose string) string {
	t.Helper()
	res, err := store.Import(assets.ImportInput{
		Reader:    strings.NewReader(string(content)),
		MediaType: "application/x-domowik-test",
		Purpose:   purpose,
	})
	if err != nil {
		t.Fatalf("store import: %v", err)
	}
	return res.Descriptor.HexDigest
}

func assertPreviewsPreserved(t *testing.T, view *persist.PackageView) {
	t.Helper()
	if len(view.Preview) < 2 {
		t.Fatalf("previews were dropped: %v", view.Preview)
	}
	foundData, foundIndex := false, false
	for name := range view.Preview {
		switch {
		case strings.HasPrefix(name, "preview/data/"):
			foundData = true
		case name == "preview/index.json":
			foundIndex = true
		}
	}
	if !foundData || !foundIndex {
		t.Fatalf("preview namespace incomplete: %v", view.Preview)
	}
}

// writeInputPackage materializes a valid thin input package referencing
// the given digests through a level background image, descriptors, the
// asset-root table, and preserved preview entries.
func writeInputPackage(t *testing.T, path, _ string, blobs map[string][]byte, extraRoots ...string) {
	t.Helper()
	levelRaw := json.RawMessage(`{"id":"` + testLevelID + `","type":"level","name":"Ground floor",` +
		`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2700,` +
		`"backgroundImages":[{"digest":"sha256://REPLACED/","anchor":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0},"stackOrdinal":0}]}`)

	var bgDigest string
	keys := make([]string, 0, len(blobs))
	for k := range blobs {
		keys = append(keys, k)
	}
	sortStringsSimple(keys)
	bgDigest = keys[0]
	levelRaw = json.RawMessage(strings.Replace(string(levelRaw), "sha256://REPLACED/", "sha256:"+bgDigest, 1))

	ent, derr := document.DecodeEntity(levelRaw)
	if derr != nil {
		t.Fatalf("decode level: %v", derr)
	}

	descMembers := make(map[string]json.RawMessage, len(blobs))
	rootTable := make([]string, 0, len(blobs))
	for hex, b := range blobs {
		member := fmt.Sprintf(`{"digest":"sha256:%s","byteLength":%d,"mediaType":"application/x-domowik-test",`+
			`"requirement":"required"}`, hex, len(b))
		descMembers["sha256:"+hex] = json.RawMessage(member)
		rootTable = append(rootTable, "sha256:"+hex)
	}
	for _, extra := range extraRoots {
		rootTable = append(rootTable, "sha256:"+extra)
	}
	sortStringsSimple(rootTable)
	descriptorsRaw, jerr := json.Marshal(descMembers)
	if jerr != nil {
		t.Fatal(jerr)
	}

	previews := map[string][]byte{
		"preview/data/" + testPreviewD[:2] + "/" + testPreviewD[2:]: []byte("preview-payload"),
		"preview/index.json": []byte("{}"),
	}

	pkg, berr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           document.State{Entities: []document.Entity{ent}},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{},
		AssetRoot:       rootTable,
		DescriptorsRaw:  descriptorsRaw,
		PreviewEntries:  previews,
	})
	if berr != nil {
		t.Fatalf("BuildPackage(input): %v", berr)
	}
	if _, lerr := persist.Load(pkg); lerr != nil {
		t.Fatalf("input fixture does not load: %v", lerr)
	}
	if err := os.WriteFile(path, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
}

func sortStringsSimple(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}

// TestThinPackSucceedsWithUnresolvedUniverseMembers pins the deterministic
// rule: only portable omissions abort packing. A thin pack whose U(P)
// contains a member that is neither embedded nor cached still succeeds —
// the root table and descriptors preserve the reference — and reports an
// actionable missing diagnostic instead of failing.
func TestThinPackSucceedsWithUnresolvedUniverseMembers(t *testing.T) {
	cache := newCache(t)
	store, _ := assets.Open(cache)

	cached := importBytes(t, store, cache, []byte(testBlobA), "background-image")
	missing := fmt.Sprintf("%064x", 0xBB) // referenced nowhere resolvable

	inputPath := filepath.Join(t.TempDir(), "input.domo")
	writeInputPackage(t, inputPath, cache, map[string][]byte{
		cached: []byte(testBlobA),
	}, missing)

	thinPath := filepath.Join(t.TempDir(), "thin.domo")
	var stdout, stderr strings.Builder
	code := Run([]string{"pack", "--thin", "--cache", cache, inputPath, thinPath},
		&stdout, &stderr)
	if code != 0 {
		t.Fatalf("thin pack must not abort on unresolved U(P) members (exit %d)\nstdout:\n%s\nstderr:\n%s",
			code, stdout.String(), stderr.String())
	}

	if !strings.Contains(stdout.String(), "diagnostic: missing: sha256:"+missing) ||
		!strings.Contains(stdout.String(), "resolves nowhere") {
		t.Fatalf("missing reference diagnostic absent from output:\n%s", stdout.String())
	}
	if !strings.Contains(stdout.String(), "resolved-embedded-blobs: 0") {
		t.Fatalf("thin must embed no U(P) blobs:\n%s", stdout.String())
	}
	if _, err := os.Stat(thinPath); err != nil {
		t.Fatalf("thin output was not written: %v", err)
	}

	view, lerr := persist.LoadFile(thinPath)
	if lerr != nil {
		t.Fatalf("thin package invalid: %v", lerr)
	}
	defer view.Close()
	found := false
	for _, id := range view.AssetRoot {
		if id == "sha256:"+missing {
			found = true
		}
	}
	if !found {
		t.Fatalf("root table dropped the unresolved reference: %v", view.AssetRoot)
	}
	assertPreviewsPreserved(t, view)
}

// TestPortableStillAbortsOnSameInput proves the profile split: the very
// same unresolved member makes the portable conversion of the same input
// fail with the complete omission report.
func TestPortableStillAbortsOnSameInput(t *testing.T) {
	cache := newCache(t)
	store, _ := assets.Open(cache)
	cached := importBytes(t, store, cache, []byte(testBlobA), "background-image")
	missing := fmt.Sprintf("%064x", 0xCC)

	inputPath := filepath.Join(t.TempDir(), "input.domo")
	writeInputPackage(t, inputPath, cache, map[string][]byte{cached: []byte(testBlobA)}, missing)

	outputPath := filepath.Join(t.TempDir(), "portable.domo")
	var stdout, stderr strings.Builder
	code := Run([]string{"pack", "--portable", "--cache", cache, inputPath, outputPath},
		&stdout, &stderr)
	if code != 1 || !strings.Contains(stderr.String(), "omitted-digest: sha256:"+missing) {
		t.Fatalf("portable pack must abort completely with a complete report (exit %d):\n%s",
			code, stderr.String())
	}
	if _, err := os.Stat(outputPath); !os.IsNotExist(err) {
		t.Fatal("output exists after aborted portable pack")
	}
}
