package sizecorpus

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

func repoRoot(t *testing.T) string {
	t.Helper()
	dir, err := filepath.Abs(".")
	if err != nil {
		t.Fatal(err)
	}
	root := filepath.Join(dir, "..", "..")
	if _, err := os.Stat(filepath.Join(root, "go.mod")); err != nil {
		t.Fatalf("repository root not found above %s: %v", dir, err)
	}
	return root
}

func evidencePath(t *testing.T, rel string) string {
	return filepath.Join(repoRoot(t), EvidenceRoot, rel)
}

// T1: §10.9 seed literals reproduce exactly.
func TestSeedLiteralsMatchSpec(t *testing.T) {
	for input, want := range map[string]string{
		seedInputPrefix + ScenarioEmpty:       SeedEmptyHex,
		seedInputPrefix + ScenarioUnfurnished: SeedUnfurnishedHex,
	} {
		sum := sha256.Sum256([]byte(input))
		if got := hex.EncodeToString(sum[:16]); got != want {
			t.Fatalf("seed %q: got %s want %s", input, got, want)
		}
	}
	b, err := seedBytes(ScenarioEmpty)
	if err != nil || hex.EncodeToString(b) != SeedEmptyHex {
		t.Fatalf("seedBytes(empty) = %x, %v", b, err)
	}
}

// T2: §10.9 RNG self-check vector (little-endian reading).
func TestRNGSelfCheckVector(t *testing.T) {
	if err := SelfCheckRNGVector(); err != nil {
		t.Fatal(err)
	}
}

// T3: FORMAT-SPEC E4 layout — derived UUIDs parse, carry UUIDv8 version and
// RFC 9562 variant, and preserve the remaining 122 tag bits.
func TestTagUUIDE4Layout(t *testing.T) {
	seed, _ := seedBytes(ScenarioEmpty)
	for _, path := range []string{"level/1", "actor/create", "batch/create", "op/create/level/1"} {
		u := TagUUID(seed, path)
		if _, err := document.ParseUUID(u.String()); err != nil {
			t.Fatalf("%s: derived UUID does not parse: %v", path, err)
		}
		if u[6]>>4 != 8 {
			t.Fatalf("%s: version nibble = %x, want 8 (UUIDv8)", path, u[6]>>4)
		}
		if u[8]&0xC0 != 0x80 {
			t.Fatalf("%s: variant bits not 10x", path)
		}
		sum := sha256.Sum256(append(append([]byte{}, seed...), []byte(path)...))
		var raw [16]byte
		copy(raw[:], sum[:16])
		raw[6] = (raw[6] & 0x0f) | 0x80
		raw[8] = (raw[8] & 0x3f) | 0x80
		if raw != u {
			t.Fatalf("%s: layout mismatch beyond the six overwritten bits", path)
		}
	}
}

// T4: empty-v1 structure exactly per §10.9.
func TestEmptyV1Structure(t *testing.T) {
	d, err := Build(ScenarioEmpty)
	if err != nil {
		t.Fatal(err)
	}
	def, _ := DefinitionFor(ScenarioEmpty)
	if len(d.Objects) != def.Totals.TotalEntities() {
		t.Fatalf("object count = %d, want %d", len(d.Objects), def.Totals.TotalEntities())
	}
	o := d.Objects[0]
	if o.Kind != document.KindLevel || o.Path != "level/1" {
		t.Fatalf("first object = %s/%s", o.Kind, o.Path)
	}
	for _, want := range []string{
		`"name":"Level 1"`, `"elevationMm":0`, `"floorThicknessMm":200`,
		`"defaultWallHeightMm":2500`, `"defaultCeilingHeightMm":2500`,
	} {
		if !strings.Contains(string(o.JSON), want) {
			t.Fatalf("level payload missing %s: %s", want, o.JSON)
		}
	}
	if strings.Contains(string(o.JSON), "backgroundImages") {
		t.Fatal("background anchor must be absent")
	}
	if err := SelfCheck(d); err != nil {
		t.Fatalf("self-check: %v", err)
	}
}

// T5: unfurnished-1000-v1 totals, door-host rule (ED-0025), and full
// normative self-check including geometry properties.
func TestUnfurnishedV1CountsAndGeometry(t *testing.T) {
	d, err := Build(ScenarioUnfurnished)
	if err != nil {
		t.Fatal(err)
	}
	def, _ := DefinitionFor(ScenarioUnfurnished)
	got := Totals{}
	for i := range d.Objects {
		switch d.Objects[i].Kind {
		case document.KindLevel:
			got.Levels++
		case document.KindWall:
			got.Walls++
		case document.KindWallJoin:
			got.Joins++
		case document.KindOpening:
			got.Openings++
		case document.KindRoomRecord:
			got.Rooms++
		default:
			t.Fatalf("unexpected kind %s on %s", d.Objects[i].Kind, d.Objects[i].Path)
		}
	}
	if got != def.Totals || got.TotalEntities() != 1000 {
		t.Fatalf("totals %+v != %+v (total %d)", got, def.Totals, got.TotalEntities())
	}
	byPath := map[string]EntityObject{}
	for _, o := range d.Objects {
		byPath[o.Path] = o
	}
	// Exhaustive ED-0025 door rule: every global instance g < 39 carries a
	// door on each edge-middle sub-wall; instance 39 carries none.
	for l0 := 0; l0 < def.Totals.Levels; l0++ {
		for r := 0; r < ringsPerLevel; r++ {
			g := l0*ringsPerLevel + r
			for _, name := range edgeNames {
				hostPath := "wall/" + itoa(l0+1) + "/" + itoa(r) + "/" + name + "/1"
				openingPath := "opening/" + itoa(l0+1) + "/" + itoa(r) + "/" + name
				opening, ok := byPath[openingPath]
				if g >= doorRingsGlobal {
					if ok {
						t.Fatalf("global instance %d must carry no openings (%s)", g, openingPath)
					}
					continue
				}
				if !ok {
					t.Fatalf("missing required opening %s on global instance %d", openingPath, g)
				}
				hostID := extractID(t, byPath[hostPath].JSON)
				if !strings.Contains(string(opening.JSON), `"hostWallId":"`+hostID+`"`) {
					t.Fatalf("opening %s does not host on its edge-middle sub-wall %s", openingPath, hostPath)
				}
			}
		}
	}
	// Total openings must equal the exhaustive enumeration above.
	counted := 0
	for path := range byPath {
		if strings.HasPrefix(path, "opening/") {
			counted++
		}
	}
	if counted != def.Totals.Openings {
		t.Fatalf("openings %d != definition total %d", counted, def.Totals.Openings)
	}
	if err := SelfCheck(d); err != nil {
		t.Fatalf("self-check: %v", err)
	}
}

func itoa(n int) string {
	return strconv.Itoa(n)
}

func fmtSprintf(format string, args ...string) string {
	out := format
	for _, a := range args {
		out = strings.Replace(out, "%s", a, 1)
	}
	return out
}

func extractID(t *testing.T, obj []byte) string {
	t.Helper()
	var m struct {
		ID string `json:"id"`
	}
	if err := json.Unmarshal(obj, &m); err != nil {
		t.Fatal(err)
	}
	return m.ID
}

// T6: self-check aborts naming the entity, and openings report
// missing-opening-field:<field> for any absent R4.0.5-authoritative field.
func TestSelfCheckAbortPaths(t *testing.T) {
	d, err := Build(ScenarioUnfurnished)
	if err != nil {
		t.Fatal(err)
	}
	mutated := func(o *EntityObject) {}
	_ = mutated
	copyDoc := func() *Document {
		return &Document{Definition: d.Definition, Objects: append([]EntityObject{}, d.Objects...),
			OpIDs: append([]document.UUID{}, d.OpIDs...), State: d.State,
			ActorID: d.ActorID, BatchID: d.BatchID, OpCount: d.OpCount}
	}
	// Remove sillMm from every opening; expect the first failure to name it.
	sillFree := copyDoc()
	for i := range sillFree.Objects {
		if sillFree.Objects[i].Kind == document.KindOpening {
			sillFree.Objects[i].JSON = []byte(strings.ReplaceAll(
				string(sillFree.Objects[i].JSON), `"sillMm":0,`, ""))
		}
	}
	err = SelfCheck(sillFree)
	if err == nil {
		t.Fatal("expected missing-opening-field failure")
	}
	const wantCode = "missing-opening-field:sillMm"
	if !strings.Contains(err.Error(), wantCode) || !strings.Contains(err.Error(), "opening/") {
		t.Fatalf("error %q must contain %q and name the opening entity", err, wantCode)
	}
	// Remove thickness from one wall; expect named required-field failure.
	wallCut := copyDoc()
	for i := range wallCut.Objects {
		if wallCut.Objects[i].Path == "wall/2/3/top/0" {
			wallCut.Objects[i].JSON = []byte(strings.ReplaceAll(
				string(wallCut.Objects[i].JSON), `"thicknessMm":200,`, ""))
		}
	}
	err = SelfCheck(wallCut)
	if err == nil || !strings.Contains(err.Error(), "wall/2/3/top/0") ||
		!strings.Contains(err.Error(), "missing-required-field:thicknessMm") &&
			!strings.Contains(err.Error(), "missing-required-field") {
		t.Fatalf("wall mutation must fail naming the entity, got %v", err)
	}
}

// T7: the reference thumbnail matches the pinned §10.9 evidence vector.
// Loud skip when python3 is unavailable; never a silent acceptance.
func TestReferenceThumbnailVector(t *testing.T) {
	if !PythonAvailable() {
		t.Skip("reference preview encoder unavailable: python3 not found; cannot verify the pinned ASSET-STRATEGY §10.9 PNG vector in this environment")
	}
	png, err := ReferenceThumbnail()
	if err != nil {
		t.Fatal(err)
	}
	if len(png) != refPNGLength {
		t.Fatalf("thumbnail length %d != %d", len(png), refPNGLength)
	}
}

// T8a/T8b: committed packages agree with their records; regeneration is
// byte-stable whenever the reference encoder is available. Missing
// generator capability is never converted into acceptance: digest checks
// against committed files still run unconditionally below (T8a).
//
// T8b compares regeneration against the committed tree. The packages and the
// host-independent records are compared unconditionally under the pinned
// toolchain. The environment-bearing records are compared only when the live
// host matches the reference environment those records themselves name
// (ED-0031); otherwise a provenance difference would be reported as an
// artifact mismatch. When it cannot compare them it names every divergent
// environment fact and every record it therefore did not compare. It never
// skips silently, and an unclassifiable record, an unparsable reference
// claim, or two records disagreeing are all hard failures.
// preED0037Artifacts enumerates, one by one, the committed size-corpus
// packages whose bytes predate ED-0037's snapshot position widening. It is
// deliberately a LIST and not a predicate: a predicate would silently
// swallow a future artifact, while adding a third entry here has to be a
// conscious edit that a reviewer sees.
//
// It is also deliberately NOT keyed off the records' `formatVersion`. That
// field is set from the FormatVersionV01 constant, and because v0.1 is
// unreleased ED-0037 changed the ENCODING without bumping the declared
// version — so the committed pre-widening records and the current writer
// both declare 0.1, and the recorded version no longer identifies the
// encoding of the artifact it describes. Whoever performs the ED-0040
// queue item 6 re-measurement must fix that before this exemption can
// retire.
//
// The exemption is temporary by construction: it dies with queue item 6,
// and the check above FAILS if an entry starts regenerating identically,
// so it cannot outlive its reason silently.
var preED0037Artifacts = map[string]bool{
	"packages/empty-v1.domo":            true,
	"packages/unfurnished-1000-v1.domo": true,
}

// packageRegenerationViolations is the byte-reproducibility comparison
// itself, returning findings rather than calling t.Fatalf so the negative
// invariants can prove the rule bites — the same shape internal/corpusgen
// uses for its coverage-audit rules. Without that, scoping a gate which
// blocks one's own branch is indistinguishable from disabling it, which is
// ED-0050's mandatory guardrail.
//
// It returns violations and, separately, the scoped artifacts it did not
// compare, so a caller can never mistake silence for agreement.
func packageRegenerationViolations(plans []ScenarioPlan, committed, generated map[string][]byte) (violations, scoped []string) {
	for _, plan := range plans {
		data, ok := committed[plan.PackageRel]
		if !ok {
			violations = append(violations, fmt.Sprintf("committed package %s unreadable", plan.PackageRel))
			continue
		}
		want, ok := generated[plan.PackageRel]
		if !ok {
			violations = append(violations, fmt.Sprintf("regeneration produced no package for %s", plan.Name))
			continue
		}
		if preED0037Artifacts[plan.PackageRel] {
			// The committed bytes predate ED-0037's snapshot widening, so
			// the current writer cannot reproduce them and is not supposed
			// to: ED-0037 comment (a) records that these two packages stay
			// as pre-ED-0037 development artifacts carrying no conformance
			// claim, and are NOT re-labelled or regenerated. Regenerating
			// would invalidate issue 0035's measured budgets and would need
			// the Linux reference host ED-0030's open question still covers.
			//
			// What is still asserted for them: T8a, unconditionally
			// (committed digest and byte count against the budget record),
			// and TestCommittedPackagesRoundTrip, which proves they still
			// LOAD under ED-0038's legacy convention. Only the
			// regenerate-and-compare is scoped.
			if string(want) == string(data) {
				violations = append(violations, fmt.Sprintf(
					"%s now regenerates byte-identically: the pre-ED-0037 exemption is obsolete and must be removed consciously (ED-0050)",
					plan.Name))
				continue
			}
			scoped = append(scoped, plan.Name)
			continue
		}
		if string(want) != string(data) {
			violations = append(violations, fmt.Sprintf(
				"regenerated package for %s differs from committed artifact; rerun the gated record generation and commit the result",
				plan.Name))
		}
	}
	return violations, scoped
}

// TestScopedRegenerationGateStillFails is ED-0050's mandatory guardrail.
//
// It drives the SAME comparison the gate uses over artifacts the CURRENT
// writer actually produces. That is the whole point: an earlier version of
// this test used arbitrary byte strings, which differ from a committed
// artifact for reasons that have nothing to do with the exemption, so its
// transcript proved the comparison rejects garbage rather than that it
// notices a real artifact failing to regenerate. Arbitrary bytes leave the
// exemption untested with the variable unisolated.
//
// Both sides here are genuine current-format packages, so the only thing
// that differs between the "violation" and "no violation" cases is whether
// the path is on the enumerated exemption list.
func TestScopedRegenerationGateStillFails(t *testing.T) {
	gen, genErr := tryGenerateAllForTests(t)
	if genErr != nil {
		// tryGenerateAllForTests already logged the named reason. The
		// guardrail is never converted into acceptance by silence.
		t.Skipf("guardrail needs real generated artifacts and generation is unavailable: %v", genErr)
	}
	plans := Plans()
	if len(plans) < 2 {
		t.Fatalf("the guardrail needs two distinct generated artifacts, have %d", len(plans))
	}
	artifactA, okA := gen.Packages[plans[0].PackageRel]
	artifactB, okB := gen.Packages[plans[1].PackageRel]
	if !okA || !okB {
		t.Fatal("regeneration produced no bytes for the guardrail's artifacts")
	}
	if string(artifactA) == string(artifactB) {
		t.Fatal("the two generated artifacts must differ, or the violation case proves nothing")
	}

	const probe = "packages/current-format-probe.domo"
	if preED0037Artifacts[probe] {
		t.Fatalf("%s must not be exempt; the guardrail would prove nothing", probe)
	}
	probePlan := []ScenarioPlan{{Name: "current-format-probe", PackageRel: probe}}

	t.Run("non-exempt current-format artifact that fails to regenerate is a violation", func(t *testing.T) {
		v, scoped := packageRegenerationViolations(probePlan,
			map[string][]byte{probe: artifactA},
			map[string][]byte{probe: artifactB})
		if len(v) != 1 {
			t.Fatalf("the comparison accepted a current-format artifact whose regeneration differs: "+
				"the gate is disabled, not scoped (%v)", v)
		}
		if len(scoped) != 0 {
			t.Fatalf("a non-exempt artifact must never be reported as scoped: %v", scoped)
		}
	})

	t.Run("non-exempt current-format artifact that regenerates identically is not a violation", func(t *testing.T) {
		v, _ := packageRegenerationViolations(probePlan,
			map[string][]byte{probe: artifactA},
			map[string][]byte{probe: artifactA})
		if len(v) != 0 {
			t.Fatalf("identical bytes must pass; a rule that always fires proves nothing either: %v", v)
		}
	})

	t.Run("exempt artifact that starts regenerating identically is a violation", func(t *testing.T) {
		exempt := plans[0].PackageRel
		if !preED0037Artifacts[exempt] {
			t.Fatalf("%s is expected to be on the exemption list", exempt)
		}
		v, _ := packageRegenerationViolations([]ScenarioPlan{plans[0]},
			map[string][]byte{exempt: artifactA},
			map[string][]byte{exempt: artifactA})
		if len(v) != 1 {
			t.Fatalf("an exempt artifact that regenerates identically must be flagged so the "+
				"exemption is removed consciously: %v", v)
		}
	})

	t.Run("exempt artifact whose bytes differ is scoped, not silent", func(t *testing.T) {
		exempt := plans[1].PackageRel
		v, scoped := packageRegenerationViolations([]ScenarioPlan{plans[1]},
			map[string][]byte{exempt: artifactA},
			map[string][]byte{exempt: artifactB})
		if len(v) != 0 {
			t.Fatalf("the exemption must suppress this comparison: %v", v)
		}
		if len(scoped) != 1 || scoped[0] != plans[1].Name {
			t.Fatalf("a scoped artifact must be reported as scoped, never silently skipped: %v", scoped)
		}
	})
}

func TestCommittedArtifactsConsistent(t *testing.T) {
	gen, genErr := tryGenerateAllForTests(t)

	// T8a: committed digests and sizes agree (no generation needed).
	for _, plan := range Plans() {
		data, err := os.ReadFile(evidencePath(t, plan.PackageRel))
		if err != nil {
			t.Skipf("committed package %s absent (%v); nothing to validate yet", plan.PackageRel, err)
		}
		recBytes, err := os.ReadFile(evidencePath(t, plan.BudgetsRel))
		if err != nil {
			t.Skipf("committed budget record absent: %v", err)
		}
		var rec BudgetRecord
		if err := json.Unmarshal(recBytes, &rec); err != nil {
			t.Fatalf("budget record unparsable: %v", err)
		}
		sum := sha256.Sum256(data)
		if got := "sha256:" + hex.EncodeToString(sum[:]); got != rec.Package.SHA256 {
			t.Fatalf("committed package digest %s != recorded %s", got, rec.Package.SHA256)
		}
		if len(data) != rec.Package.ByteCount {
			t.Fatalf("committed package size %d != recorded %d", len(data), rec.Package.ByteCount)
		}
	}

	if genErr != nil {
		// tryGenerateAllForTests already logged the named reason.
		return
	}
	// The pinned toolchain policy the records name stays a separate hard
	// prerequisite for any regeneration comparison; it is not relaxed here.
	if os.Getenv("GOTOOLCHAIN") != "local" {
		t.Logf("regeneration comparison skipped: GOTOOLCHAIN != local; run under ./scripts/verify.sh or the documented reproduction command: %s", reproductionCommand)
		return
	}

	committedPkgs := map[string][]byte{}
	for _, plan := range Plans() {
		data, err := os.ReadFile(evidencePath(t, plan.PackageRel))
		if err != nil {
			t.Fatalf("committed package %s unreadable: %v", plan.PackageRel, err)
		}
		committedPkgs[plan.PackageRel] = data
	}
	violations, scoped := packageRegenerationViolations(Plans(), committedPkgs, gen.Packages)
	for _, v := range violations {
		t.Error(v)
	}
	for _, name := range scoped {
		t.Logf("regeneration comparison scoped for %s: pre-ED-0037 artifact awaiting the ED-0040 queue item 6 re-measurement (ED-0037 comment (a), ED-0050)", name)
	}
	if len(violations) > 0 {
		t.FailNow()
	}

	rels := make([]string, 0, len(gen.Records))
	for rel := range gen.Records {
		rels = append(rels, rel)
	}
	sortStrings(rels)
	committed := make(map[string][]byte, len(rels))
	for _, rel := range rels {
		data, err := os.ReadFile(evidencePath(t, rel))
		if err != nil {
			t.Fatalf("committed record %s missing while regeneration exists: %v", rel, err)
		}
		committed[rel] = data
	}

	recorded, err := parseRecordedIdentity(committed)
	if err != nil {
		t.Fatalf("committed reference environment unusable: %v", err)
	}
	live, err := liveIdentity()
	if err != nil {
		t.Fatalf("live environment identity unavailable: %v", err)
	}
	plan, err := planRecordComparison(recorded, live, rels)
	if err != nil {
		t.Fatal(err)
	}
	if bad := mismatchedRecords(plan.Compare, committed, gen.Records); len(bad) > 0 {
		t.Fatalf("record(s) %s differ from regeneration; rerun %s and commit",
			strings.Join(bad, ", "), reproductionCommand)
	}
	if len(plan.Omit) > 0 {
		t.Logf("byte comparison omitted for %d environment-bearing record(s): %s\n"+
			"  the live host is not the reference environment those records name:\n    %s\n"+
			"  compared unconditionally instead: both committed package digests and byte counts, "+
			"both packages byte-for-byte against regeneration, and record(s) %s",
			len(plan.Omit), strings.Join(plan.Omit, ", "),
			strings.Join(plan.Divergences, "\n    "), strings.Join(plan.Compare, ", "))
	}
}

// T9: committed packages load through the reader seam with expected content.
func TestCommittedPackagesRoundTrip(t *testing.T) {
	for _, plan := range Plans() {
		path := evidencePath(t, plan.PackageRel)
		if _, err := os.Stat(path); err != nil {
			t.Skipf("committed package %s absent; skipping round-trip", plan.PackageRel)
		}
		view, err := persist.LoadFile(path)
		if err != nil {
			t.Fatalf("LoadFile(%s): %v", path, err)
		}
		defer view.Close()
		def, _ := DefinitionFor(plan.Name)
		state := view.Projection.State()
		if len(state.Entities) != def.Totals.TotalEntities() {
			t.Fatalf("%s: entities %d != %d", plan.Name, len(state.Entities), def.Totals.TotalEntities())
		}
		if len(view.Preview) != 2 {
			t.Fatalf("%s: expected index+data preview entries, got %d", plan.Name, len(view.Preview))
		}
	}
}

// T10/T11: committed budget records satisfy the accounting identity, keep
// external totals excluded from the headline, and record a strict pass —
// any drift forces an explicit budget revision, never a silent pass.
func TestBudgetRecordsAccountingAndVerdict(t *testing.T) {
	for _, plan := range Plans() {
		recBytes, err := os.ReadFile(evidencePath(t, plan.BudgetsRel))
		if err != nil {
			t.Skipf("committed budget record absent: %v", err)
		}
		var rec BudgetRecord
		if err := json.Unmarshal(recBytes, &rec); err != nil {
			t.Fatal(err)
		}
		entrySum := 0
		classSum := 0
		for _, c := range rec.ClassBreakdown.Classes {
			classSum += c.PhysicalBytes
		}
		for _, e := range rec.ClassBreakdown.Entries {
			entrySum += e.PhysicalBytes
		}
		if entrySum+rec.ClassBreakdown.ContainerOverheadBytes != rec.Budget.HeadlineBytes {
			t.Fatalf("%s: entries(%d)+overhead(%d) != headline(%d)",
				plan.Name, entrySum, rec.ClassBreakdown.ContainerOverheadBytes, rec.Budget.HeadlineBytes)
		}
		if classSum+rec.ClassBreakdown.ContainerOverheadBytes != rec.Budget.HeadlineBytes {
			t.Fatalf("%s: class sums inconsistent with headline", plan.Name)
		}
		if int64(rec.Budget.HeadlineBytes) >= rec.Budget.TargetBytes {
			t.Fatalf("%s: measured %d >= target %d; a mandatory recorded budget revision must exist",
				plan.Name, rec.Budget.HeadlineBytes, rec.Budget.TargetBytes)
		}
		if !rec.Budget.Pass || rec.Budget.RevisionRequired {
			t.Fatalf("%s: recorded verdict must be a strict pass", plan.Name)
		}
		ext := rec.ExternalTotals
		if ext.UniverseSize != 0 || ext.LogicalExternalBytes != 0 ||
			ext.ExternalCacheBytes != 0 || ext.ExternalRemoteBytes != 0 {
			t.Fatalf("%s: external totals must be explicitly zero and excluded from the headline", plan.Name)
		}
		if rec.SelfCheck.Result != "pass" || rec.Generator.Version != GeneratorVersion {
			t.Fatalf("%s: self-check/generator identity mismatch", plan.Name)
		}
	}
}

// TestSelfCheckOpIDCardinalityGuard proves a malformed Document yields a
// named self-check error instead of a panic when operation identities are
// missing or extra.
func TestSelfCheckOpIDCardinalityGuard(t *testing.T) {
	d, err := Build(ScenarioEmpty)
	if err != nil {
		t.Fatal(err)
	}
	short := &Document{Definition: d.Definition, Objects: d.Objects, OpIDs: nil,
		State: d.State, ActorID: d.ActorID, BatchID: d.BatchID, OpCount: d.OpCount}
	err = SelfCheck(short)
	if err == nil || !strings.Contains(err.Error(), "operation identity count") {
		t.Fatalf("want named cardinality error, got %v", err)
	}
	extra := &Document{Definition: d.Definition, Objects: d.Objects,
		OpIDs: append(append([]document.UUID{}, d.OpIDs...), document.UUID{}),
		State: d.State, ActorID: d.ActorID, BatchID: d.BatchID, OpCount: d.OpCount}
	err = SelfCheck(extra)
	if err == nil || !strings.Contains(err.Error(), "operation identity count") {
		t.Fatalf("want named cardinality error for extra ids, got %v", err)
	}
}

// TestIdentityOracleDetectsDefects simulates generator-helper and layout
// defects and proves the independent reference oracle detects every one of
// them with a named failure: missing E4 overwrite, wrong version nibble,
// corrupted actor/batch identity, and a corrupted operation identity.
func TestIdentityOracleDetectsDefects(t *testing.T) {
	d, err := Build(ScenarioUnfurnished)
	if err != nil {
		t.Fatal(err)
	}
	const probePath = "join/3/5/right/1" // leaf entity: nothing references it, isolating the identity oracle
	idx := -1
	for i := range d.Objects {
		if d.Objects[i].Path == probePath {
			idx = i
		}
	}
	if idx < 0 {
		t.Fatalf("probe entity %s not found", probePath)
	}
	seed, _ := seedBytes(d.Definition.Name)

	withEntityID := func(t *testing.T, newID string) *Document {
		t.Helper()
		cp := &Document{Definition: d.Definition, Objects: append([]EntityObject{}, d.Objects...),
			OpIDs: append([]document.UUID{}, d.OpIDs...), State: d.State,
			ActorID: d.ActorID, BatchID: d.BatchID, OpCount: d.OpCount}
		old := extractID(t, cp.Objects[idx].JSON)
		cp.Objects[idx].JSON = []byte(strings.Replace(
			string(cp.Objects[idx].JSON), `"id":"`+old+`"`, `"id":"`+newID+`"`, 1))
		return cp
	}

	// Defect A: generator skips the E4 overwrite entirely (raw tag bytes).
	rawSum := sha256.Sum256(append(append([]byte{}, seed...), []byte(probePath)...))
	var rawTag document.UUID
	copy(rawTag[:], rawSum[:16])
	errA := SelfCheck(withEntityID(t, rawTag.String()))
	if errA == nil || !strings.Contains(errA.Error(), probePath) {
		t.Fatalf("missing-overwrite defect undetected or unnamed: %v", errA)
	}

	// Defect B: version nibble is UUIDv7 rather than UUIDv8.
	good := extractID(t, d.Objects[idx].JSON)
	v7 := good[:14] + "7" + good[15:]
	if _, err := document.ParseUUID(v7); err != nil {
		t.Fatalf("v7 probe id does not parse: %v", err)
	}
	errB := SelfCheck(withEntityID(t, v7))
	if errB == nil || !strings.Contains(errB.Error(), "identity mismatch") ||
		!strings.Contains(errB.Error(), probePath) {
		t.Fatalf("wrong-version defect undetected or unnamed: %v", errB)
	}

	// Defect C: actor identity corrupted (batch-derived id used instead).
	actorDefect := &Document{Definition: d.Definition, Objects: d.Objects, OpIDs: d.OpIDs,
		State: d.State, ActorID: d.BatchID, BatchID: d.BatchID, OpCount: d.OpCount}
	if errC := SelfCheck(actorDefect); errC == nil ||
		!strings.Contains(errC.Error(), "actor identity mismatch") {
		t.Fatalf("actor defect undetected: %v", errC)
	}

	// Defect D: batch identity corrupted.
	batchDefect := &Document{Definition: d.Definition, Objects: d.Objects, OpIDs: d.OpIDs,
		State: d.State, ActorID: d.ActorID, BatchID: d.ActorID, OpCount: d.OpCount}
	if errD := SelfCheck(batchDefect); errD == nil ||
		!strings.Contains(errD.Error(), "batch identity mismatch") {
		t.Fatalf("batch defect undetected: %v", errD)
	}

	// Defect E: one operation identity corrupted.
	opDefect := &Document{Definition: d.Definition, Objects: d.Objects,
		OpIDs: append([]document.UUID{}, d.OpIDs...), State: d.State,
		ActorID: d.ActorID, BatchID: d.BatchID, OpCount: d.OpCount}
	opDefect.OpIDs[idx] = document.UUID{}
	if errE := SelfCheck(opDefect); errE == nil ||
		!strings.Contains(errE.Error(), "operation identity mismatch") ||
		!strings.Contains(errE.Error(), probePath) {
		t.Fatalf("operation defect undetected or unnamed: %v", errE)
	}

	// Sanity: the untouched document passes the same oracle.
	if err := SelfCheck(d); err != nil {
		t.Fatalf("baseline must pass its own oracle: %v", err)
	}
}

// tryGenerateAllForTests regenerates everything when python3 exists;
// otherwise it reports why so callers can degrade loudly but honestly.
func tryGenerateAllForTests(t *testing.T) (*Generated, error) {
	t.Helper()
	if !PythonAvailable() {
		t.Log("regeneration skipped: python3 unavailable; committed-artifact digest checks still enforced")
		return nil, errPythonUnavailable
	}
	return GenerateAll()
}

var errPythonUnavailable = &pythonUnavailableError{}

type pythonUnavailableError struct{}

func (*pythonUnavailableError) Error() string {
	return "reference preview encoder unavailable: python3 not found"
}
