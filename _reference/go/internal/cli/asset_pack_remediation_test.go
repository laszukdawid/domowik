package cli

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/assets"
	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// asset_pack_remediation_test.go: pack-surface rows of the duplicate/
// orphan matrix routed to issue 0026 by the issue 0018 release audit
// under ED-0027. Duplicate physical ZIP entry names are not positively
// constructible here — digest-keyed source maps plus the injective
// §10.3 mapping leave no reachable duplicate-name state through this
// surface — so these proofs pin the observable consequence (reference
// convergence embeds exactly one physical blob per identity) without
// deciding row-L29 semantics, which stay with their spec owners.

type staticBlobSource struct{ data []byte }

func (b *staticBlobSource) DeclaredLength() uint64 { return uint64(len(b.data)) }

func (b *staticBlobSource) Open() (io.Reader, error) { return bytes.NewReader(b.data), nil }

const remediationInstanceID = "01963456-7abc-7def-8000-000000000002"

func convergedEntities(t *testing.T, hex string) []document.Entity {
	t.Helper()
	levelRaw := json.RawMessage(`{"id":"` + testLevelID + `","type":"level","name":"Ground floor",` +
		`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2700,` +
		`"backgroundImages":[{"digest":"sha256:` + hex + `","anchor":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0},"stackOrdinal":0}]}`)
	instanceRaw := json.RawMessage(`{"id":"` + remediationInstanceID + `","type":"assetInstance",` +
		`"levelId":"` + testLevelID + `","digest":"sha256:` + hex + `",` +
		`"transform":{"translationMm":[5,5],"scaleUniform":1,"rotationDeg":0}}`)
	ent, derr := document.DecodeEntity(levelRaw)
	if derr != nil {
		t.Fatalf("decode level: %v", derr)
	}
	inst, ierr := document.DecodeEntity(instanceRaw)
	if ierr != nil {
		t.Fatalf("decode instance: %v", ierr)
	}
	return []document.Entity{ent, inst}
}

func descriptorDocument(t *testing.T, hex string, n int) json.RawMessage {
	t.Helper()
	return json.RawMessage(`{"sha256:` + hex + `":{"digest":"sha256:` + hex + `","byteLength":` +
		fmt.Sprintf("%d", n) + `,"mediaType":"application/x-domowik-test"}}`)
}

func writePackageFile(t *testing.T, path string, pkg []byte) {
	t.Helper()
	if _, lerr := persist.Load(pkg); lerr != nil {
		t.Fatalf("fixture does not load: %v", lerr)
	}
	if err := os.WriteFile(path, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
}

// assertSingleEmbeddedIdentity loads an output package and proves every
// U(P) member resolved into exactly one physically embedded,
// byte-faithful blob (P1 positive-completeness direction).
func assertSingleEmbeddedIdentity(t *testing.T, path, hex string, content []byte) {
	t.Helper()
	view, err := persist.LoadFile(path)
	if err != nil {
		t.Fatalf("output package invalid: %v", err)
	}
	defer view.Close()
	if len(view.Blobs) != 1 {
		t.Fatalf("expected exactly one physical blob occurrence, got %d", len(view.Blobs))
	}
	got, ok := view.Blobs[hex]
	if !ok || !bytes.Equal(got, content) {
		t.Fatal("embedded blob lost byte fidelity against its identity")
	}
	if gotInv, listed := view.EntryInventory(persist.BlobEntryName(hex)); !listed ||
		gotInv.ByteLength != uint64(len(content)) || gotInv.SHA256 != "sha256:"+hex {
		t.Fatalf("manifest inventory wrong for the embedded blob: %+v %t", gotInv, listed)
	}
	if len(view.AssetRoot) != 1 || view.AssetRoot[0] != "sha256:"+hex {
		t.Fatalf("asset-root table diverged from U(P): %v", view.AssetRoot)
	}
	u, uerr := assets.ReferencedUniverse(view)
	if uerr != nil || len(u.Digests) != 1 || u.Digests[0] != "sha256:"+hex {
		t.Fatalf("recomputed output universe wrong: %+v (%v)", u.Digests, uerr)
	}
	for _, id := range u.Digests {
		h := strings.TrimPrefix(id, "sha256:")
		out, ok := view.Blobs[h]
		if !ok || !bytes.Equal(out, content) {
			t.Fatalf("universe member %s did not resolve embedded and verified", id)
		}
	}
}

// D4/P1: one digest referenced as descriptor key, root-table entry,
// instance digest, and level background image embeds EXACTLY ONE
// physical blob per conversion, byte-faithfully, from both resolution
// branches (input-embedded and cache-backed); two runs of the same
// writer over the same input are byte-identical.
func TestPortablePackEmbedsConvergedReferenceClassesOnce(t *testing.T) {
	cache := newCache(t)
	store, _ := assets.Open(cache)
	content := []byte(testBlobA + "-converged")
	hex := importBytes(t, store, cache, content, "background-image")

	inputPath := filepath.Join(t.TempDir(), "portable-input.domo")
	pkg, berr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfilePortable(),
		State:           document.State{Entities: convergedEntities(t, hex)},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{hex: &staticBlobSource{data: content}},
		AssetRoot:       []string{"sha256:" + hex},
		DescriptorsRaw:  descriptorDocument(t, hex, len(content)),
	})
	if berr != nil {
		t.Fatalf("BuildPackage(portable input): %v", berr)
	}
	writePackageFile(t, inputPath, pkg)

	dir := t.TempDir()
	out1 := filepath.Join(dir, "out1.domo")
	stdout, stderr, code := runOK(t, []string{"pack", "--portable", "--cache", cache, inputPath, out1})
	if code != 0 {
		t.Fatalf("pack failed (%d): %s%s", code, stdout, stderr)
	}
	if !strings.Contains(stdout, "resolved-embedded-blobs: 1") {
		t.Fatalf("converged references multiplied embedded blobs:\n%s", stdout)
	}
	assertSingleEmbeddedIdentity(t, out1, hex, content)

	// Same-writer determinism: a second identical run must not invent or
	// drop any physical occurrence.
	out2 := filepath.Join(dir, "out2.domo")
	if _, _, code = runOK(t, []string{"pack", "--portable", "--cache", cache, inputPath, out2}); code != 0 {
		t.Fatal("second pack failed")
	}
	b1, e1 := os.ReadFile(out1)
	b2, e2 := os.ReadFile(out2)
	if e1 != nil || e2 != nil || !bytes.Equal(b1, b2) {
		t.Fatal("identical pack runs produced different packages")
	}

	// Cache-backed branch: a THIN input over the same converged
	// references resolves externally and embeds once from the cache.
	thinPath := filepath.Join(t.TempDir(), "thin-input.domo")
	thinPkg, terr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           document.State{Entities: convergedEntities(t, hex)},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{},
		AssetRoot:       []string{"sha256:" + hex},
		DescriptorsRaw:  descriptorDocument(t, hex, len(content)),
	})
	if terr != nil {
		t.Fatalf("BuildPackage(thin input): %v", terr)
	}
	writePackageFile(t, thinPath, thinPkg)

	out3 := filepath.Join(dir, "out3.domo")
	stdout, stderr, code = runOK(t, []string{"pack", "--portable", "--cache", cache, thinPath, out3})
	if code != 0 {
		t.Fatalf("cache-backed pack failed (%d): %s%s", code, stdout, stderr)
	}
	if !strings.Contains(stdout, "resolved-external-blobs: 1") ||
		!strings.Contains(stdout, fmt.Sprintf("logical-external-bytes: %d", len(content))) {
		t.Fatalf("size classes wrong for cache-backed embedding:\n%s", stdout)
	}
	assertSingleEmbeddedIdentity(t, out3, hex, content)
}

// D5: duplicated and unsorted asset-root INPUT entries merge to the
// unique ascending wire table; the reader-side ordering contract then
// accepts the regenerated manifest.
func TestWriterMergesDuplicateAssetRootInput(t *testing.T) {
	x := fmt.Sprintf("%064x", 0x21)
	y := fmt.Sprintf("%064x", 0x42)
	pkg, berr := persist.BuildPackage(&persist.WriteInput{
		Profile:         persist.ProfileThin(),
		State:           document.State{Entities: []document.Entity{}},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{},
		AssetRoot:       []string{"sha256:" + y, "sha256:" + x, "sha256:" + y},
	})
	if berr != nil {
		t.Fatalf("BuildPackage with duplicated roots: %v", berr)
	}
	view, lerr := persist.Load(pkg)
	if lerr != nil {
		t.Fatalf("merged table rejected by reader: %v", lerr)
	}
	want := []string{"sha256:" + x, "sha256:" + y}
	if len(view.AssetRoot) != 2 || view.AssetRoot[0] != want[0] || view.AssetRoot[1] != want[1] {
		t.Fatalf("duplicate root entries survived: %v", view.AssetRoot)
	}
}
