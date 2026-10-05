package assets

import (
	"bytes"
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// remediation_matrix_test.go: executable duplicate/orphan/crash proofs
// routed to issue 0026 by the issue 0018 release audit under ED-0027.
// Row identifiers (D*, O*, F*, R1, G2, G3, P4) reference the pre-edit
// matrix agreed with the reviewer. The row-L29 physical-occurrence
// tolerance stays an upstream spec question and is deliberately not
// decided or tested here.

func shardFiles(t *testing.T, root string, shard string) []string {
	t.Helper()
	entries, err := os.ReadDir(filepath.Join(root, "blob", "sha256", shard))
	if err != nil {
		t.Fatalf("read blob shard: %v", err)
	}
	var out []string
	for _, fi := range entries {
		out = append(out, fi.Name())
	}
	return out
}

// D1/D2: duplicate logical content identity deduplicates to one physical
// cache copy; repeated descriptor placement stays idempotent at the
// file level while the delivered replace policy makes the recorded
// descriptor reflect the most recent admission's metadata.
func TestDuplicateImportIdentityLeavesSingleCacheCopy(t *testing.T) {
	s := newTestStore(t)
	content := blobBytes("duplicate-identity", 512)

	first := mustImport(t, s, content, "texture")
	second, err := s.Import(ImportInput{
		Reader:    bytes.NewReader(content),
		MediaType: "image/png",
		Purpose:   "furniture-model",
	})
	if err != nil {
		t.Fatalf("second import: %v", err)
	}
	if !second.Deduplicated || second.Descriptor.HexDigest != first.HexDigest {
		t.Fatalf("identical bytes must deduplicate to one identity: %+v", second)
	}

	shard := first.HexDigest[:2]
	if files := shardFiles(t, s.Root(), shard); len(files) != 1 || files[0] != first.HexDigest[2:] {
		t.Fatalf("duplicate admission produced %d blob files: %v", len(files), files)
	}
	descs, derr := os.ReadDir(filepath.Join(s.Root(), "desc", "sha256", shard))
	if derr != nil || len(descs) != 1 {
		t.Fatalf("descriptor placement is not idempotent at file level: %d entries (%v)", len(descs), derr)
	}
	got, gerr := s.GetDescriptor(first.HexDigest)
	if gerr != nil {
		t.Fatalf("descriptor unreadable after re-import: %v", gerr)
	}
	if got.MediaType != "image/png" || got.Purpose != "furniture-model" {
		t.Fatalf("descriptor did not reflect the latest admitted metadata: %+v", got)
	}
	list, totals, lerr := s.List()
	if lerr != nil || len(list) != 1 || totals.CacheBlobCount != 1 {
		t.Fatalf("listing diverged from physical copies: %d rows, %+v", len(list), totals)
	}
	entries, _ := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if len(entries) != 0 {
		t.Fatalf("staging leftovers after duplicate imports: %v", entries)
	}
}

// D3: one digest referenced simultaneously through all three U(P)
// classes converges to a single universe member; provenance keeps the
// per-class membership without multiplying identity.
func TestUniverseConvergesDuplicateReferenceClassesToOneMember(t *testing.T) {
	hex := hexOfBlob([]byte("converged-reference-payload"))
	levelID := "01963456-7abc-7def-8000-000000000001"
	instID := "01963456-7abc-7def-8000-000000000002"

	levelRaw := json.RawMessage(`{"id":"` + levelID + `","type":"level","name":"Ground floor",` +
		`"elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2700,` +
		`"backgroundImages":[{"digest":"sha256:` + hex + `","anchor":{"translationMm":[0,0],"scaleUniform":1,"rotationDeg":0},"stackOrdinal":0}]}`)
	instanceRaw := json.RawMessage(`{"id":"` + instID + `","type":"assetInstance","levelId":"` + levelID + `",` +
		`"digest":"sha256:` + hex + `","transform":{"translationMm":[10,10],"scaleUniform":1,"rotationDeg":0}}`)
	descriptors := json.RawMessage(`{"sha256:` + hex + `":{"digest":"sha256:` + hex +
		`","byteLength":10,"mediaType":"image/png"}}`)

	pkg, berr := persist.BuildPackage(&persist.WriteInput{
		Profile: persist.ProfilePortable(),
		State: document.State{
			Entities: []document.Entity{mustDecodeEntity(t, levelRaw), mustDecodeEntity(t, instanceRaw)},
		},
		RetainedBatches: []*document.Batch{},
		BlobSources:     map[string]persist.BlobSource{},
		AssetRoot:       []string{"sha256:" + hex},
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
	if len(u.Digests) != 1 || u.Digests[0] != "sha256:"+hex {
		t.Fatalf("duplicate reference classes multiplied U(P): %v", u.Digests)
	}
	if len(u.FromDescriptors) != 1 || u.FromDescriptors[0] != hex {
		t.Fatalf("descriptor-class provenance wrong: %+v", u)
	}
	if len(u.FromRootTable) != 1 || u.FromRootTable[0] != hex {
		t.Fatalf("root-table provenance wrong: %+v", u)
	}
	// Delivered provenance model: instance digests and level
	// background-image digests share the FromInstances class, so one
	// converged digest contributes twice there while U(P) stays a
	// single member.
	if len(u.FromInstances) != 2 {
		t.Fatalf("reference-class convergence wrong: %+v", u)
	}
	for _, h := range u.FromInstances {
		if h != hex {
			t.Fatalf("foreign digest in instance class: %+v", u)
		}
	}
}

// O1: process death mid-stream (simulated by a panicking source) leaves
// at most a private staging file: invisible to every lookup surface,
// never admitted, and removed by the next clean collection pass.
func TestMidStreamDeathConfinesLeftoverToStagingAndSweepsIt(t *testing.T) {
	s := newTestStore(t)
	data := blobBytes("midstream-death", 2048)

	func() {
		defer func() { _ = recover() }()
		r := &panickingReader{data: data, panicAfter: 1024}
		_, _ = s.Import(ImportInput{Reader: r, MediaType: "image/png", Purpose: "texture"})
	}()

	staging, rerr := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if rerr != nil || len(staging) != 1 {
		t.Fatalf("expected exactly one abandoned staging file, got %d (%v)", len(staging), rerr)
	}
	if !strings.HasPrefix(staging[0].Name(), importPrefix) {
		t.Fatalf("leftover lacks the staging prefix: %s", staging[0].Name())
	}
	if s.HasBlob(hexOfBlob(data)) {
		t.Fatal("unverified content became namespace-visible")
	}
	if _, gerr := s.GetDescriptor(hexOfBlob(data)); gerr == nil {
		t.Fatal("descriptor exists for an unadmitted stream")
	}
	list, totals, lerr := s.List()
	if lerr != nil || len(list) != 0 || totals.CacheBlobCount != 0 {
		t.Fatalf("dead-stream leftover surfaced in listing: %d rows %+v", len(list), totals)
	}

	res, cerr := s.Collect()
	if cerr != nil || res.FailedClosed {
		t.Fatalf("clean collection failed: %+v %v", res, cerr)
	}
	if res.SweptStaging != 1 || len(res.Collected) != 0 {
		t.Fatalf("sweep result wrong: %+v", res)
	}
	left, _ := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if len(left) != 0 {
		t.Fatalf("staging not emptied by sweep: %v", left)
	}
}

type panickingReader struct {
	data       []byte
	panicAfter int
	pos        int
}

func (r *panickingReader) Read(p []byte) (int, error) {
	if r.pos >= r.panicAfter {
		panic("simulated process death mid-import")
	}
	n := copy(p, r.data[r.pos:])
	r.pos += n
	return n, nil
}

// O2: death between blob admission and descriptor recording leaves a
// detectable orphan — blob present, descriptor absent, listing empty —
// that collection then removes exactly, with nothing deleted beforehand.
func TestDeathBetweenAdmissionAndDescriptorLeavesCollectibleOrphan(t *testing.T) {
	s := newTestStore(t)
	content := blobBytes("admission-crash", 256)
	hex := hexOfBlob(content)

	s.crashAfterBlobAdmit = func() { panic("simulated death after blob rename") }
	func() {
		defer func() { _ = recover() }()
		_, _ = s.Import(ImportInput{
			Reader:    bytes.NewReader(content),
			MediaType: "image/png",
			Purpose:   "texture",
		})
	}()
	s.crashAfterBlobAdmit = nil

	if !s.HasBlob(hex) {
		t.Fatal("admitted blob vanished before any collection ran")
	}
	if _, gerr := s.GetDescriptor(hex); gerr == nil {
		t.Fatal("crash state carries a descriptor")
	}
	if _, serr := os.Stat(s.DescPath(hex)); !os.IsNotExist(serr) {
		t.Fatal("partial descriptor file survived the crash point")
	}
	list, totals, lerr := s.List()
	if lerr != nil || len(list) != 0 {
		t.Fatalf("orphan must not appear as an asset row: %d rows (%v)", len(list), lerr)
	}
	if totals.CacheBlobCount != 1 || totals.CacheBlobBytes != uint64(len(content)) {
		t.Fatalf("orphan bytes invisible to cache accounting: %+v", totals)
	}

	res, cerr := s.Collect()
	if cerr != nil || res.FailedClosed {
		t.Fatalf("collect over orphan: %+v %v", res, cerr)
	}
	if len(res.Collected) != 1 || res.Collected[0] != digestPrefix+hex {
		t.Fatalf("collection report wrong: %v", res.Collected)
	}
	if s.HasBlob(hex) {
		t.Fatal("orphan blob survived its collection")
	}
}

// F1–F7 + crash injection: every metadata anomaly stops collection with
// ZERO deletion; repair resumes conservatively over the full cached
// superset; releasing one conservative pin makes exactly that digest
// collectable again while every remaining pin still protects its blob.
func TestAnomalyRepairResumeMatrix(t *testing.T) {
	recordMutations := map[string]func(raw []byte) []byte{
		"checksum-flip": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Records[0].Checksum = strings.Replace(st.Records[0].Checksum, "a", "b", 1)
			out, _ := json.Marshal(&st)
			return out
		},
		"generation-gap": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Generation += 5
			out, _ := json.Marshal(&st)
			return out
		},
		"generation-regression": func(raw []byte) []byte {
			var st pinStoreState
			json.Unmarshal(raw, &st)
			st.Generation--
			out, _ := json.Marshal(&st)
			return out
		},
		"truncation": func(raw []byte) []byte { return raw[:len(raw)/2] },
		"garbage-record": func([]byte) []byte {
			return []byte(`{"generation":1,"records":[{"kind":"pin"}]}`)
		},
	}

	cases := []struct {
		name  string
		setup func(t *testing.T) (*Store, [2]string)
	}{
		{name: "marker-not-decimal", setup: func(t *testing.T) (*Store, [2]string) {
			s, digests := gcFixture(t)
			if werr := os.WriteFile(s.nextGenPath(), []byte("seven"), 0o644); werr != nil {
				t.Fatal(werr)
			}
			return s, digests
		}},
		{name: "marker-without-store", setup: func(t *testing.T) (*Store, [2]string) {
			s := newTestStore(t)
			a := mustImport(t, s, blobBytes("mwos-a", 64), "texture")
			b := mustImport(t, s, blobBytes("mwos-b", 64), "texture")
			if werr := os.WriteFile(s.nextGenPath(), []byte("1"), 0o644); werr != nil {
				t.Fatal(werr)
			}
			return s, [2]string{a.HexDigest, b.HexDigest}
		}},
		{name: "death-between-marker-and-store", setup: func(t *testing.T) (*Store, [2]string) {
			s := newTestStore(t)
			a := mustImport(t, s, blobBytes("dbms-a", 64), "texture")
			b := mustImport(t, s, blobBytes("dbms-b", 64), "texture")
			s.crashAfterMarker = func() { panic("simulated mutation death") }
			func() {
				defer func() { _ = recover() }()
				_ = s.AddPin(b.HexDigest, "late-owner", rootKindManual, "")
			}()
			s.crashAfterMarker = nil
			return s, [2]string{a.HexDigest, b.HexDigest}
		}},
	}
	for name, mutate := range recordMutations {
		mutate := mutate
		cases = append(cases, struct {
			name  string
			setup func(t *testing.T) (*Store, [2]string)
		}{name: name, setup: func(t *testing.T) (*Store, [2]string) {
			s, digests := gcFixture(t)
			mutateStoreFile(t, s, mutate)
			return s, digests
		}})
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			s, digests := tc.setup(t)

			res := expectFailClosedZeroCollection(t, s, digests)
			if res.SweptStaging != 0 {
				t.Fatalf("fail-closed run swept staging files: %+v", res)
			}

			rep, rerr := s.Repair()
			if rerr != nil || !rep.Resumed {
				t.Fatalf("repair did not resume: %+v %v", rep, rerr)
			}
			if rep.ConservativePins != 2 {
				t.Fatalf("conservative superset must re-pin every cached blob: %+v", rep)
			}

			after, cerr := s.Collect()
			if cerr != nil || after.FailedClosed {
				t.Fatalf("post-repair collect: %+v %v", after, cerr)
			}
			if len(after.Collected) != 0 {
				t.Fatalf("conservative repair must protect everything before release: %v", after.Collected)
			}
			if !s.HasBlob(digests[0]) || !s.HasBlob(digests[1]) {
				t.Fatal("repair lost cached blobs")
			}

			if rerr := s.ReleasePin(digests[1], ownerRebuild); rerr != nil {
				t.Fatalf("releasing the targeted rebuild pin failed: %v", rerr)
			}
			final, ferr := s.Collect()
			if ferr != nil || final.FailedClosed {
				t.Fatalf("targeted collect: %+v %v", final, ferr)
			}
			if len(final.Collected) != 1 || final.Collected[0] != digestPrefix+digests[1] {
				t.Fatalf("release made more than the selected digest collectable: %v", final.Collected)
			}
			if !s.HasBlob(digests[0]) {
				t.Fatal("remaining pins stopped protecting their blobs")
			}
		})
	}
}

// F8: an unreadable metadata FILE is an io anomaly — collection stays
// fail-closed, mutations refuse, and repair refuses rather than fighting
// the filesystem fault; restoring the file resumes normal operation.
func TestIOAnomalyFailsClosedUntilFileRestored(t *testing.T) {
	s, digests := gcFixture(t)
	raw, rerr := os.ReadFile(s.storePath())
	if rerr != nil {
		t.Fatal(rerr)
	}
	if err := os.Remove(s.storePath()); err != nil {
		t.Fatal(err)
	}
	if err := os.Mkdir(s.storePath(), 0o755); err != nil {
		t.Fatal(err)
	}

	res := expectFailClosedZeroCollection(t, s, digests)
	if res.AnomalyKind != "io" {
		t.Fatalf("expected io anomaly, got %s", res.AnomalyKind)
	}
	if aerr := s.AddPin(digests[1], "owner-x", rootKindManual, ""); aerr == nil {
		t.Fatal("mutation accepted on unreadable metadata store")
	}
	if _, rerr := s.Repair(); rerr == nil {
		t.Fatal("repair claimed success against a directory-shaped store")
	}
	expectFailClosedZeroCollection(t, s, digests)

	if err := os.Remove(s.storePath()); err != nil {
		t.Fatal(err)
	}
	if werr := os.WriteFile(s.storePath(), raw, 0o644); werr != nil {
		t.Fatal(werr)
	}
	rep, rerr := s.Repair()
	if rerr != nil || !rep.Resumed {
		t.Fatalf("restore did not resume repair: %+v %v", rep, rerr)
	}
}

// G2: a pin outlives the blob it protects; readmitting identical bytes
// restores protection atomically because the durable record never went
// away (G2 direction).
func TestReadmissionRestoresProtectionForPinnedDigest(t *testing.T) {
	s := newTestStore(t)
	content := blobBytes("readmission", 128)
	d := mustImport(t, s, content, "background-image")
	if perr := s.AddPin(d.HexDigest, "doc-1", rootKindSnapshot, ""); perr != nil {
		t.Fatalf("AddPin: %v", perr)
	}
	if err := os.Remove(s.BlobPath(d.HexDigest)); err != nil {
		t.Fatal(err)
	}

	again, ierr := s.Import(ImportInput{
		Reader:    bytes.NewReader(content),
		MediaType: "image/png",
		Purpose:   "background-image",
	})
	if ierr != nil {
		t.Fatalf("readmission: %v", ierr)
	}
	if again.Deduplicated {
		t.Fatal("readmission reported deduplication against a lost blob")
	}

	list, _, lerr := s.List()
	if lerr != nil || len(list) != 1 || !list[0].Pinned {
		t.Fatalf("pinned status lost across blob loss: %+v (%v)", list, lerr)
	}
	res, cerr := s.Collect()
	if cerr != nil || res.FailedClosed || len(res.Collected) != 0 || !s.HasBlob(d.HexDigest) {
		t.Fatalf("readmitted pinned blob was collected: %+v %v", res, cerr)
	}
}

// G3: collection unlinks strictly by digest shape; foreign names,
// nested directories, and wrong-length shards survive untouched.
func TestCollectNeverTouchesForeignNamespaceEntries(t *testing.T) {
	s, digests := gcFixture(t)
	foreign := []string{
		filepath.Join(s.Root(), "blob", "sha256", "zz", "not-a-digest-name"),
		filepath.Join(s.Root(), "blob", "sha256", "ab", "nested", "inner.bin"),
		filepath.Join(s.Root(), "blob", "sha256", "toolongshard", "x"),
	}
	for _, p := range foreign {
		if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(p, []byte("foreign"), 0o644); err != nil {
			t.Fatal(err)
		}
	}

	res, cerr := s.Collect()
	if cerr != nil || res.FailedClosed {
		t.Fatalf("collect: %+v %v", res, cerr)
	}
	if len(res.Collected) != 1 || res.Collected[0] != digestPrefix+digests[1] {
		t.Fatalf("unexpected collection set: %v", res.Collected)
	}
	for _, p := range foreign {
		if _, serr := os.Stat(p); serr != nil {
			t.Fatalf("foreign namespace entry was removed: %s (%v)", p, serr)
		}
	}
	if !s.HasBlob(digests[0]) {
		t.Fatal("pinned blob collected alongside foreign entries")
	}
}

// P4: the ratified per-blob cap binds end-to-end during streaming
// import; the rejection names the L5 class and admits nothing.
func TestImportEnforcesBlobCapEndToEnd(t *testing.T) {
	s := newTestStore(t)
	_, err := s.Import(ImportInput{
		Reader:    endlessZeroReader{},
		MediaType: "application/x-domowik-test",
		Purpose:   "texture",
	})
	if err == nil {
		t.Fatal("oversize stream accepted")
	}
	cat, ok := persist.CategoryOf(err)
	if !ok || cat != persist.CatLimitBlobBytes {
		t.Fatalf("rejection lacks the limit-blob-bytes class: %v", err)
	}
	if !strings.Contains(err.Error(), "L5") {
		t.Fatalf("diagnostic does not name the ratified row: %v", err)
	}
	entries, rerr := os.ReadDir(filepath.Join(s.Root(), layoutTmp))
	if rerr != nil || len(entries) != 0 {
		t.Fatalf("rejected import left staging behind: %d entries (%v)", len(entries), rerr)
	}
	if _, totals, lerr := s.List(); lerr != nil || totals.CacheBlobCount != 0 {
		t.Fatalf("oversize stream was admitted: %+v", totals)
	}
}

type endlessZeroReader struct{}

func (endlessZeroReader) Read(p []byte) (int, error) {
	for i := range p {
		p[i] = 0
	}
	return len(p), nil
}

var _ io.Reader = endlessZeroReader{}
