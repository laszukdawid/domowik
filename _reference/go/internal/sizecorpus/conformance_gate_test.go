package sizecorpus

import (
	"fmt"
	"os"
	"strings"
	"testing"
)

// Targeted coverage for the ED-0031 environment-scoped evidence gate:
// classification is closed, malformed or mutually inconsistent reference
// records fail closed, the conforming host compares everything, the
// non-conforming host says exactly what it omitted, and a genuine artifact
// mismatch still fails.

// The reference environment the committed records name, restated
// independently here. This is deliberately a second statement of the same
// facts rather than a read of the records: it lets the conforming branch run
// on a non-conforming host, and if the committed evidence is ever recaptured
// on another host these literals fail and force the change to be deliberate.
func recordedHostIdentity() referenceIdentity {
	return referenceIdentity{
		Env: Environment{
			OS:          "linux",
			Kernel:      "7.1.8-arch1-3",
			CPUModel:    "AMD Ryzen 9 7950X 16-Core Processor",
			LogicalCPUs: 32,
			MemTotalKiB: 64853676,
			Filesystem:  "btrfs",
			GoVersion:   "go1.27.0",
			GOOS:        "linux",
			GOARCH:      "amd64",
			GOTOOLCHAIN: "local",
		},
		ZlibRuntime:    "1.3.2",
		PythonIdentity: "Python 3.14.7 (main, Aug 14 2026, 15:35:28) [Clang 22.1.3 ]",
	}
}

// allRecordRels is the committed record set GenerateAll produces.
func allRecordRels() []string {
	rels := append([]string{}, envBearingRawRecords...)
	for _, plan := range Plans() {
		rels = append(rels, plan.BudgetsRel, plan.InventorRel)
	}
	rels = append(rels, "encoding-candidates.json")
	sortStrings(rels)
	return rels
}

func committedRecords(t *testing.T) map[string][]byte {
	t.Helper()
	out := map[string][]byte{}
	for _, rel := range allRecordRels() {
		data, err := os.ReadFile(evidencePath(t, rel))
		if err != nil {
			t.Fatalf("committed record %s unreadable: %v", rel, err)
		}
		out[rel] = data
	}
	return out
}

func withRecord(base map[string][]byte, rel string, data []byte) map[string][]byte {
	out := make(map[string][]byte, len(base))
	for k, v := range base {
		out[k] = v
	}
	if data == nil {
		delete(out, rel)
	} else {
		out[rel] = data
	}
	return out
}

// The recorded reference identity parses out of the committed records and
// matches the environment those records name, so the full-comparison branch
// is reachable on the reference host: every record is compared, nothing is
// omitted. This exercises the real parser, not a comparison of one value
// against itself.
func TestRecordedIdentityParsesAndConforms(t *testing.T) {
	recorded, err := parseRecordedIdentity(committedRecords(t))
	if err != nil {
		t.Fatalf("committed reference environment unusable: %v", err)
	}
	want := recordedHostIdentity()
	if recorded != want {
		t.Fatalf("parsed reference identity\n got %+v\nwant %+v", recorded, want)
	}
	if div := identityDivergences(recorded, want); len(div) != 0 {
		t.Fatalf("reference host must conform to its own records, got %v", div)
	}
	plan, err := planRecordComparison(recorded, want, allRecordRels())
	if err != nil {
		t.Fatal(err)
	}
	if len(plan.Omit) != 0 {
		t.Fatalf("conforming host must omit nothing, omitted %v", plan.Omit)
	}
	if len(plan.Compare) != len(allRecordRels()) {
		t.Fatalf("conforming host must compare all %d records, compared %v", len(allRecordRels()), plan.Compare)
	}
}

// Every field of the identity is part of the predicate: flipping any one of
// them alone makes the host non-conforming, names that field, and omits
// exactly the seven environment-bearing records while the host-independent
// record is still compared.
func TestIdentityDivergenceIsFieldComplete(t *testing.T) {
	envBearing := 0
	for _, rel := range allRecordRels() {
		class, err := classifyRecord(rel)
		if err != nil {
			t.Fatal(err)
		}
		if class == recordEnvBearing {
			envBearing++
		}
	}
	if envBearing != 7 {
		t.Fatalf("expected 7 environment-bearing records, got %d", envBearing)
	}

	for _, tc := range []struct {
		field  string
		mutate func(*referenceIdentity)
	}{
		{"os", func(i *referenceIdentity) { i.Env.OS = "plan9" }},
		{"kernel", func(i *referenceIdentity) { i.Env.Kernel = "0.0.0" }},
		{"cpuModel", func(i *referenceIdentity) { i.Env.CPUModel = "other cpu" }},
		{"logicalCpus", func(i *referenceIdentity) { i.Env.LogicalCPUs = 8 }},
		{"memTotalKiB", func(i *referenceIdentity) { i.Env.MemTotalKiB = 1 }},
		{"filesystem", func(i *referenceIdentity) { i.Env.Filesystem = "ext4" }},
		{"goVersion", func(i *referenceIdentity) { i.Env.GoVersion = "go1.26.5" }},
		{"goos", func(i *referenceIdentity) { i.Env.GOOS = "darwin" }},
		{"goarch", func(i *referenceIdentity) { i.Env.GOARCH = "arm64" }},
		{"gotoolchain", func(i *referenceIdentity) { i.Env.GOTOOLCHAIN = "auto" }},
		{"zlibRuntimeVersion", func(i *referenceIdentity) { i.ZlibRuntime = "1.2.12" }},
		{"python3Identity", func(i *referenceIdentity) { i.PythonIdentity = "Python 3.14.6" }},
	} {
		t.Run(tc.field, func(t *testing.T) {
			recorded := recordedHostIdentity()
			live := recordedHostIdentity()
			tc.mutate(&live)
			div := identityDivergences(recorded, live)
			if len(div) != 1 {
				t.Fatalf("expected exactly one divergence, got %v", div)
			}
			if !strings.HasPrefix(div[0], tc.field+": ") {
				t.Fatalf("divergence must name %s, got %q", tc.field, div[0])
			}
			plan, err := planRecordComparison(recorded, live, allRecordRels())
			if err != nil {
				t.Fatal(err)
			}
			if len(plan.Omit) != 7 {
				t.Fatalf("expected the 7 environment-bearing records omitted, got %v", plan.Omit)
			}
			if len(plan.Compare) != 1 || plan.Compare[0] != "encoding-candidates.json" {
				t.Fatalf("host-independent record must still be compared, compared %v", plan.Compare)
			}
		})
	}
}

// An unclassified record name is a hard failure rather than inheriting either
// treatment, both directly and through the planner.
func TestRecordClassificationIsClosed(t *testing.T) {
	for _, rel := range allRecordRels() {
		if _, err := classifyRecord(rel); err != nil {
			t.Fatalf("committed record %s must classify: %v", rel, err)
		}
	}
	if _, err := classifyRecord("raw/new-provenance.txt"); err == nil {
		t.Fatal("unknown record name must not classify")
	}
	recorded := recordedHostIdentity()
	if _, err := planRecordComparison(recorded, recorded, []string{"budget-empty-v1.json", "surprise.json"}); err == nil {
		t.Fatal("planner must refuse an unclassified record")
	}
}

// Malformed, incomplete, or mutually contradictory reference records must
// fail closed: none of them may be reported as a mere environment
// difference, because that would let a corrupt claim widen or narrow the gate.
func TestRecordedIdentityFailsClosed(t *testing.T) {
	base := committedRecords(t)
	budget := Plans()[0].BudgetsRel
	inventory := Plans()[0].InventorRel

	for _, tc := range []struct {
		name    string
		records map[string][]byte
		wantIn  string
	}{
		{
			name:    "budget record absent",
			records: withRecord(base, budget, nil),
			wantIn:  "absent",
		},
		{
			name:    "raw environment absent",
			records: withRecord(base, "raw/environment.txt", nil),
			wantIn:  "absent",
		},
		{
			name:    "budget record malformed",
			records: withRecord(base, budget, []byte("{\"schemaVersion\":")),
			wantIn:  budget,
		},
		{
			name: "budget record carries an unknown field",
			records: withRecord(base, budget,
				[]byte(strings.Replace(string(base[budget]), "{\n  \"schemaVersion\"",
					"{\n  \"unreviewedField\": 1,\n  \"schemaVersion\"", 1))),
			wantIn: "unreviewedField",
		},
		{
			name: "inventory carries an unknown field",
			records: withRecord(base, inventory,
				[]byte(strings.Replace(string(base[inventory]), "{\n  \"schemaVersion\"",
					"{\n  \"unreviewedField\": 1,\n  \"schemaVersion\"", 1))),
			wantIn: "unreviewedField",
		},
		{
			name: "raw go-version disagrees with the budget environment",
			records: withRecord(base, "raw/go-version.txt",
				[]byte(strings.Replace(string(base["raw/go-version.txt"]), "goarch=amd64", "goarch=arm64", 1))),
			wantIn: "disagree on goarch",
		},
		{
			name: "inventory zlib disagrees with the raw python record",
			records: withRecord(base, inventory,
				[]byte(strings.Replace(string(base[inventory]),
					"\"zlibRuntimeVersion\": \"1.3.2\"", "\"zlibRuntimeVersion\": \"1.2.12\"", 1))),
			wantIn: "disagree on zlibRuntimeVersion",
		},
		{
			name: "raw environment restates a different cpu",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]),
					"cpu=AMD Ryzen 9 7950X 16-Core Processor", "cpu=other cpu", 1))),
			wantIn: "disagree on cpuModel",
		},
		{
			name: "raw environment loses a field",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]), "filesystem=btrfs\n", "", 1))),
			wantIn: "\"filesystem\" absent",
		},
		{
			name: "raw environment names another generator",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]),
					"generator "+GeneratorVersion, "generator 0035-part-a.99", 1))),
			wantIn: "generator",
		},
		{
			name: "raw python record contradicts the pinned thumbnail digest",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(strings.Replace(string(base["raw/python3-zlib.txt"]), refPNGSHA256Hex,
					strings.Repeat("0", len(refPNGSHA256Hex)), 1))),
			wantIn: "thumbnailSha256",
		},
		{
			name: "raw python record contradicts the pinned thumbnail size",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(strings.Replace(string(base["raw/python3-zlib.txt"]),
					fmt.Sprintf("thumbnailBytes=%d", refPNGLength), "thumbnailBytes=1", 1))),
			wantIn: "thumbnailBytes",
		},
		{
			name: "raw python record loses its identity banner",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(strings.SplitN(string(base["raw/python3-zlib.txt"]), "\n", 2)[1])),
			wantIn: "python identity",
		},
		{
			name: "raw environment carries an unknown field",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]),
					"filesystem=btrfs\n", "filesystem=btrfs\nswap=0\n", 1))),
			wantIn: "unknown field \"swap\"",
		},
		{
			name: "raw environment repeats a field",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]),
					"os=linux\n", "os=linux\nos=linux\n", 1))),
			wantIn: "duplicate field \"os\"",
		},
		{
			name: "raw environment repeats a toolchain pair",
			records: withRecord(base, "raw/environment.txt",
				[]byte(strings.Replace(string(base["raw/environment.txt"]),
					"goos=linux goarch=amd64", "goos=linux goos=linux goarch=amd64", 1))),
			wantIn: "duplicate field \"goos\"",
		},
		{
			name: "raw go-version carries an unknown field",
			records: withRecord(base, "raw/go-version.txt",
				[]byte(string(base["raw/go-version.txt"])+"cgo=0\n")),
			wantIn: "unknown field \"cgo\"",
		},
		{
			name: "raw go-version repeats a field",
			records: withRecord(base, "raw/go-version.txt",
				[]byte(string(base["raw/go-version.txt"])+"goarch=amd64\n")),
			wantIn: "duplicate field \"goarch\"",
		},
		{
			name: "raw python record carries an unknown field",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(string(base["raw/python3-zlib.txt"])+"opensslVersion=3.0\n")),
			wantIn: "unknown field \"opensslVersion\"",
		},
		{
			name: "raw python record repeats a field",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(string(base["raw/python3-zlib.txt"])+"zlibRuntimeVersion=1.3.2\n")),
			wantIn: "duplicate field \"zlibRuntimeVersion\"",
		},
		{
			name: "raw python record hides a field line in its banner",
			records: withRecord(base, "raw/python3-zlib.txt",
				[]byte(strings.Replace(string(base["raw/python3-zlib.txt"]),
					"zlibRuntimeVersion=", "zlibBuildVersion=1.2.11\nzlibRuntimeVersion=", 1))),
			wantIn: "unknown field line",
		},
		{
			name: "budget record states a top-level key twice",
			records: withRecord(base, budget,
				[]byte(strings.Replace(string(base[budget]), "{\n  \"schemaVersion\"",
					"{\n  \"scenario\": \"empty-v1\",\n  \"scenario\": \"other\",\n  \"schemaVersion\"", 1))),
			wantIn: "duplicate key \"scenario\"",
		},
		{
			name: "budget record states a nested key twice",
			records: withRecord(base, budget,
				[]byte(strings.Replace(string(base[budget]), "\"os\": \"linux\",",
					"\"os\": \"linux\",\n    \"os\": \"darwin\",", 1))),
			wantIn: "duplicate key \"os\"",
		},
		{
			name: "inventory states a key twice inside an entry",
			records: withRecord(base, inventory,
				[]byte(strings.Replace(string(base[inventory]), "\"kind\": \"thumbnail\",",
					"\"kind\": \"thumbnail\",\n      \"kind\": \"index\",", 1))),
			wantIn: "duplicate key \"kind\"",
		},
		{
			name: "budget record is followed by a second JSON document",
			records: withRecord(base, budget,
				[]byte(string(base[budget])+"{}\n")),
			wantIn: "second JSON document",
		},
		{
			name: "budget record is followed by non-JSON trailing content",
			records: withRecord(base, budget,
				[]byte(string(base[budget])+"trailing garbage\n")),
			wantIn: "trailing content",
		},
		{
			name: "budget compression library drops its zlib claim",
			records: withRecord(base, budget,
				[]byte(strings.Replace(string(base[budget]), "python3 stdlib zlib 1.3.2", "python3 stdlib zlib", 1))),
			wantIn: "compressionLibrary",
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			_, err := parseRecordedIdentity(tc.records)
			if err == nil {
				t.Fatal("expected a hard failure, got none")
			}
			if !strings.Contains(err.Error(), tc.wantIn) {
				t.Fatalf("error must name the defect (%q), got %v", tc.wantIn, err)
			}
		})
	}
}

// A genuine mismatch in anything the gate does compare is still detected on a
// non-conforming host: the host-independent record and, once the host
// conforms, the environment-bearing records too.
func TestComparisonDetectsGenuineMismatch(t *testing.T) {
	committed := committedRecords(t)
	regenerated := map[string][]byte{}
	for rel, data := range committed {
		regenerated[rel] = data
	}
	if bad := mismatchedRecords(allRecordRels(), committed, regenerated); len(bad) != 0 {
		t.Fatalf("identical inputs must not mismatch, got %v", bad)
	}

	drifted := withRecord(regenerated, "encoding-candidates.json",
		append(append([]byte{}, regenerated["encoding-candidates.json"]...), '\n'))
	recorded := recordedHostIdentity()
	live := recordedHostIdentity()
	live.Env.GOOS = "darwin" // non-conforming host
	plan, err := planRecordComparison(recorded, live, allRecordRels())
	if err != nil {
		t.Fatal(err)
	}
	bad := mismatchedRecords(plan.Compare, committed, drifted)
	if len(bad) != 1 || bad[0] != "encoding-candidates.json" {
		t.Fatalf("host-independent drift must be caught on a non-conforming host, got %v", bad)
	}

	// The same drift in an environment-bearing record is caught once the host
	// conforms, which is what makes the omission a scope decision rather than
	// a hole.
	envDrift := withRecord(regenerated, Plans()[0].BudgetsRel,
		append(append([]byte{}, regenerated[Plans()[0].BudgetsRel]...), '\n'))
	conforming, err := planRecordComparison(recorded, recorded, allRecordRels())
	if err != nil {
		t.Fatal(err)
	}
	bad = mismatchedRecords(conforming.Compare, committed, envDrift)
	if len(bad) != 1 || bad[0] != Plans()[0].BudgetsRel {
		t.Fatalf("environment-bearing drift must be caught on a conforming host, got %v", bad)
	}
}

// The committed preview inventories are pinned semantically, unconditionally
// and without python3: every field except the zlib runtime is fixed by the
// §10.9 reference vector and this package's own constants. The zlib runtime is
// an environment claim and is validated by the fail-closed reference parser
// instead, never hardcoded here.
func TestCommittedPreviewInventoriesPinned(t *testing.T) {
	indexJSON := fmt.Sprintf(`{"previews":[{"digest":"sha256:%s","byteLength":%d,"sourceKey":"%s"}]}`,
		refPNGSHA256Hex, refPNGLength, PreviewSourceKey)

	for _, plan := range Plans() {
		data, err := os.ReadFile(evidencePath(t, plan.InventorRel))
		if err != nil {
			t.Fatalf("committed preview inventory %s unreadable: %v", plan.InventorRel, err)
		}
		var inv PreviewInventory
		if err := strictUnmarshal(plan.InventorRel, data, &inv); err != nil {
			t.Fatal(err)
		}
		if inv.SchemaVersion != PreviewSchema {
			t.Errorf("%s: schemaVersion %q != %q", plan.InventorRel, inv.SchemaVersion, PreviewSchema)
		}
		if inv.Scenario != plan.Name {
			t.Errorf("%s: scenario %q != %q", plan.InventorRel, inv.Scenario, plan.Name)
		}
		if len(inv.Entries) != 2 {
			t.Fatalf("%s: expected exactly the thumbnail and index rows, got %d", plan.InventorRel, len(inv.Entries))
		}

		thumb := inv.Entries[0]
		wantNamespace := "preview/data/" + refPNGSHA256Hex[:2] + "/" + refPNGSHA256Hex[2:]
		if thumb.Namespace != wantNamespace {
			t.Errorf("%s: thumbnail namespace %q != %q", plan.InventorRel, thumb.Namespace, wantNamespace)
		}
		if thumb.Kind != "thumbnail" {
			t.Errorf("%s: thumbnail kind %q != \"thumbnail\"", plan.InventorRel, thumb.Kind)
		}
		if want := "sha256:" + refPNGSHA256Hex; thumb.Digest != want {
			t.Errorf("%s: thumbnail digest %q != %q", plan.InventorRel, thumb.Digest, want)
		}
		if thumb.ByteLength != refPNGLength {
			t.Errorf("%s: thumbnail byteLength %d != %d", plan.InventorRel, thumb.ByteLength, refPNGLength)
		}
		if thumb.SourceKey != PreviewSourceKey {
			t.Errorf("%s: thumbnail sourceKey %q != %q", plan.InventorRel, thumb.SourceKey, PreviewSourceKey)
		}
		if thumb.Dimensions == nil {
			t.Fatalf("%s: thumbnail row states no dimensions", plan.InventorRel)
		}
		if thumb.Dimensions.WidthPx != refPNGWidthPx || thumb.Dimensions.HeightPx != refPNGHeightPx {
			t.Errorf("%s: thumbnail dimensions %dx%d != %dx%d", plan.InventorRel,
				thumb.Dimensions.WidthPx, thumb.Dimensions.HeightPx, refPNGWidthPx, refPNGHeightPx)
		}
		if thumb.Encoder == nil {
			t.Fatalf("%s: thumbnail row states no encoder", plan.InventorRel)
		}
		if want := "ASSET-STRATEGY §10.9 reference thumbnail encoder"; thumb.Encoder.Name != want {
			t.Errorf("%s: encoder name %q != %q", plan.InventorRel, thumb.Encoder.Name, want)
		}
		if want := "python3 stdlib zlib level 9 over 512 scanlines of 00 + 2048 x FF; PNG RGBA8 IHDR/IEND assembled by this generator"; thumb.Encoder.Implementation != want {
			t.Errorf("%s: encoder implementation %q != %q", plan.InventorRel, thumb.Encoder.Implementation, want)
		}
		if want := fmt.Sprintf("pinned §10.9 evidence vector %d B / sha256 %s", refPNGLength, refPNGSHA256Hex); thumb.Encoder.VectorBound != want {
			t.Errorf("%s: encoder vectorBinding %q != %q", plan.InventorRel, thumb.Encoder.VectorBound, want)
		}
		if thumb.Encoder.LadderReductions != 0 {
			t.Errorf("%s: encoder ladderReductionsApplied %d != 0", plan.InventorRel, thumb.Encoder.LadderReductions)
		}

		index := inv.Entries[1]
		if index.Namespace != "preview/index.json" {
			t.Errorf("%s: index namespace %q != \"preview/index.json\"", plan.InventorRel, index.Namespace)
		}
		if index.Kind != "index" {
			t.Errorf("%s: index kind %q != \"index\"", plan.InventorRel, index.Kind)
		}
		if want := "sha256:" + hexString(sha256Of([]byte(indexJSON))); index.Digest != want {
			t.Errorf("%s: index digest %q != %q (derived from the pinned vector)", plan.InventorRel, index.Digest, want)
		}
		if index.ByteLength != len(indexJSON) {
			t.Errorf("%s: index byteLength %d != %d", plan.InventorRel, index.ByteLength, len(indexJSON))
		}
		if index.Dimensions != nil || index.Encoder != nil || index.SourceKey != "" {
			t.Errorf("%s: index row must carry no dimensions, encoder, or sourceKey", plan.InventorRel)
		}
	}
}
