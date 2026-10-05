package migration

import (
	"errors"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

const corpusDir = "../../testdata/corpus"

func corpusFile(t *testing.T, rel string) []byte {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(corpusDir, rel))
	if err != nil {
		t.Fatalf("read corpus fixture %s: %v", rel, err)
	}
	return b
}

// corpusBatchBytes is the golden single-create batch used when
// assembling hostile probes verbatim (mirrors the framing harness).
var corpusBatchBytes = []byte(`{"batchId":"01948f5b-c4e4-7e42-b996-f552a18df77f","actorId":"01948f5a-ca77-7a09-9e76-5ace11b12fd2","counterBase":42,"frontier":{"01948f5a-ca77-7a09-9e76-5ace11b12fd2":41},"operations":[{"operationId":"01948f5d-6e18-7c01-8f0d-77f974bd8036","kind":"createEntity","entityId":"01948f55-6b18-7d3a-a153-1ee2f33cc402","fields":{"id":"01948f55-6b18-7d3a-a153-1ee2f33cc402","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}}]}`)

func mustAssemble(t *testing.T, raws map[string][]byte) []byte {
	t.Helper()
	pkg, werr := persist.AssembleRawPackage(raws)
	if werr != nil {
		t.Fatalf("assemble: %v", werr)
	}
	return pkg
}

func writeTemp(t *testing.T, name string, data []byte) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), name)
	if err := os.WriteFile(p, data, 0o644); err != nil {
		t.Fatal(err)
	}
	return p
}

func goldenCopy(t *testing.T, rel string) string {
	t.Helper()
	return writeTemp(t, "src.domo", corpusFile(t, rel))
}

func assertNoLeftovers(t *testing.T, dir string) {
	t.Helper()
	es, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	for _, e := range es {
		if strings.Contains(e.Name(), ".tmp-") {
			t.Fatalf("leftover candidate %q was not cleaned", e.Name())
		}
	}
}

func assertUntouched(t *testing.T, path string, want []byte) {
	t.Helper()
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("sentinel read: %v", err)
	}
	if string(got) != string(want) {
		t.Fatal("pre-effect gate failed: target bytes changed")
	}
}

type failingWriter struct{}

func (failingWriter) Write([]byte) (int, error) { return 0, errors.New("injected short write") }

// withSchemaTables installs simulated supported/released tables for one
// test and restores the shipped ones afterwards.
//
// Both sets are assigned ABSOLUTELY, never appended to. The shipped
// released table is empty (§8.2 V6), so appending "0.2".."0.4" to it would
// silently omit "0.1" and every step starting there would refuse for the
// wrong reason.
//
// emittedSchema is NOT simulated: it is a const, and persist writes that
// one label regardless. Nothing here pretends to be a future build.
func withSchemaTables(t *testing.T, supported, released []string) {
	t.Helper()
	savedSupported := supportedSchemas
	savedReleased := releasedSchemas
	supportedSchemas = supported
	releasedSchemas = released
	t.Cleanup(func() {
		supportedSchemas = savedSupported
		releasedSchemas = savedReleased
	})
}

// futureLadder is the asymmetric simulation used by the chain-machinery
// tests: four supported minors, but only the first three released, so the
// terminal rung 0.4 is supported and UNRELEASED. That asymmetry is what
// makes a "To must be released" mutant detectable; extending both sets
// identically would hide it.
func futureLadder(t *testing.T) {
	t.Helper()
	withSchemaTables(t, []string{"0.1", "0.2", "0.3", "0.4"}, []string{"0.1", "0.2", "0.3"})
}

// syntheticTransforms converts test-only steps into execution transforms.
// It exists because executeMigration takes transforms, not Steps: a
// synthetic transform must never be able to describe itself as a migration
// between schemas in observable output.
//
// NON-CLAIMS, deliberately explicit. Driving executeMigration with these
// exercises §9.7 save MECHANICS only. It is NOT a simulated future build,
// NOT a positive migration, NOT §8.2 V4 coverage, and it fabricates no
// successor schema. This build emits exactly one label (§8.1 V1) and
// internal/persist writes it unconditionally — correctly, since V1 says a
// writer states exactly which schema it emits. That is a TESTABILITY
// LIMIT of this package, not a defect in persist: a genuine future-build
// simulation is impossible from inside internal/migration.
func syntheticTransforms(steps []Step) []namedTransform {
	out := make([]namedTransform, 0, len(steps))
	for i, s := range steps {
		out = append(out, namedTransform{name: fmt.Sprintf("synthetic-%d", i), apply: s.Apply})
	}
	return out
}

// runSynthetic drives the save machine with synthetic transforms. It
// constructs no Report — it takes the U2 findings from
// preservationFindings directly, so nothing on this path can build, hold,
// or return an object carrying a schema claim.
func runSynthetic(t *testing.T, steps []Step, source, target string, opts Options) (*persist.SaveResult, error) {
	t.Helper()
	view, lerr := persist.LoadFile(source)
	if lerr != nil {
		t.Fatalf("load %s: %v", source, lerr)
	}
	return executeMigration(view, target, syntheticTransforms(steps), preservationFindings(view), opts)
}

// stepFlip is a TEST-ONLY step proving the ordered-chain machinery; it
// is never part of the shipped production registry.
func stepFlip(from, to string) Step {
	return Step{From: from, To: to, Apply: func(p *payload) error {
		if p.profile == "thin" {
			p.profile = "portable"
		} else {
			p.profile = "thin"
		}
		return nil
	}}
}

// flipChain covers 0.1 -> 0.4 with three observable toggles; the odd
// count leaves the emitted profile "portable".
func flipChain() []Step {
	return []Step{stepFlip("0.1", "0.2"), stepFlip("0.2", "0.3"), stepFlip("0.3", "0.4")}
}

// The shipped tables must stay honest. FORMAT-SPEC §8.2 V6 records that no
// 0.N schema has been publicly released and that the predecessor set is
// empty, so the released table is EMPTY while 0.1 is both emitted and
// supported.
//
// Subtests, not a single fatal sequence: one failing assertion must not
// mask the others.
//
// This test establishes that releasedSchemas is empty. The other half of
// the unfillability claim — that an empty released set is what refuses a
// Step — is TestOnlyReleasedSchemasMayOriginateSteps. Neither half is the
// claim on its own, and the previous version of this test asserted neither:
// it pinned the released set as {0.1}, which was the false premise issue
// 0031 exists to correct.
func TestShippedRegistryHonesty(t *testing.T) {
	t.Run("production chain is empty", func(t *testing.T) {
		if len(chain) != 0 {
			t.Fatalf("production chain must be empty until a successor schema is specified, has %d steps", len(chain))
		}
	})
	t.Run("released set is empty per 8.2 V6", func(t *testing.T) {
		if len(releasedSchemas) != 0 {
			t.Fatalf("§8.2 V6 records an EMPTY predecessor set, got %v", releasedSchemas)
		}
		for _, label := range []string{"0.1", "0.0", "0.2"} {
			if IsReleasedSchema(label) {
				t.Errorf("%q must not be a released schema: v0.1 is UNRELEASED (§8.2 V6)", label)
			}
		}
	})
	t.Run("emitted schema is exactly one label per 8.1 V1", func(t *testing.T) {
		if emittedSchema != "0.1" {
			t.Fatalf("emitted schema = %q, want 0.1", emittedSchema)
		}
		if !IsSupportedSchema(emittedSchema) {
			t.Fatal("this build must be able to read what it writes")
		}
	})
	t.Run("supported set is exactly {0.1}", func(t *testing.T) {
		if len(supportedSchemas) != 1 || supportedSchemas[0] != "0.1" {
			t.Fatalf("supported schemas = %v, want [0.1]", supportedSchemas)
		}
		for _, label := range []string{"0.0", "0.2", "1.0"} {
			if IsSupportedSchema(label) {
				t.Errorf("%q must not be supported", label)
			}
		}
	})
	t.Run("V1 label form", func(t *testing.T) {
		for _, bad := range []string{"", "0", "01.0", "0.01", "1", "0.1.0", "a.b"} {
			if IsSchemaLabel(bad) {
				t.Errorf("%q must not parse as a V1 schema label", bad)
			}
		}
		if !IsSchemaLabel("10.11") {
			t.Error("multi-digit MAJOR.MINOR labels are valid V1 form")
		}
	})
	t.Run("empty registry validates", func(t *testing.T) {
		// ED-0041 note: this is an OBSERVATION, not a proof. A per-entry
		// loop over zero entries establishes nothing about the per-entry
		// rules. The non-empty probes are
		// TestOnlyReleasedSchemasMayOriginateSteps,
		// TestStepSourceMustBeReadable and
		// TestLastSupportedSchemaHasNoSuccessor.
		if serr := validateRegistry(chain); serr != nil {
			t.Fatalf("shipped empty registry must validate, got %v", serr)
		}
	})
}

// P1. The §8.2 V4 origin rule is what refuses a Step, and the released set
// being empty is what makes the shipped chain unfillable rather than merely
// empty.
//
// ISOLATION (ED-0053): the witness step passes every other gate by
// construction — Apply non-nil, both labels valid V1 form, From supported,
// To supported, not a self-loop, no duplicate, and adjacency exact
// (supportedIndex 0 -> 1). The ONLY thing that can refuse it is the origin
// rule.
//
// CONTROL, requiring no code change: the identical step against identical
// tables, with "0.1" added to the released set, VALIDATES. Exactly one
// element of one slice differs between the refusing and the accepting run,
// so no other gate is capable of firing on this input.
func TestOnlyReleasedSchemasMayOriginateSteps(t *testing.T) {
	step := []Step{{From: "0.1", To: "0.2", Apply: func(*payload) error { return nil }}}

	t.Run("witness: empty released set refuses", func(t *testing.T) {
		withSchemaTables(t, []string{"0.1", "0.2"}, nil)
		serr := validateRegistry(step)
		if serr == nil {
			t.Fatal("a step may not originate at an unreleased schema while §8.2 V6 records an empty predecessor set")
		}
		if serr.Category != persist.CatInvalidFieldSyntax {
			t.Fatalf("category = %s, want invalid-field-syntax (%v)", serr.Category, serr)
		}
		if !strings.Contains(serr.Detail, "starts at a non-released schema") {
			t.Fatalf("detail %q must name the origin rule", serr.Detail)
		}
	})

	t.Run("control: same step, 0.1 released, validates", func(t *testing.T) {
		withSchemaTables(t, []string{"0.1", "0.2"}, []string{"0.1"})
		if serr := validateRegistry(step); serr != nil {
			t.Fatalf("control must validate; if it refuses, the witness above proves nothing: %v", serr)
		}
	})
}

// P2. To is checked for SUPPORT and deliberately NOT for release. A target
// may legitimately be supported before it is published — while its release
// is being prepared and tested — and a released-To rule would reject that
// valid pre-publication state.
//
// futureLadder models exactly that state and nothing more: 0.4 is supported
// and not yet released. It does NOT simulate emission; emittedSchema stays
// the const "0.1" throughout, and nothing here claims otherwise.
//
// ISOLATION: the asymmetry is the point. Extending both sets identically
// would make a released-To mutant undetectable.
//
// CONTROL: the 0.1->0.2->0.3 prefix, whose every To IS released, must keep
// validating. That separates "detected because the terminal rung is
// unreleased" from "detected because the chain is long" or "because of the
// flip transform".
// Witness and control are SUBTESTS: under a To-must-be-released mutant the
// witness fails, and the control must still run and stay green. A single
// fatal sequence would abort at the witness and the control would never
// execute, which is precisely the evidence that separates "detected because
// the terminal rung is unreleased" from "detected because the chain is
// long".
func TestChainMayEndAtASupportedUnreleasedSchema(t *testing.T) {
	futureLadder(t)
	full := flipChain()

	t.Run("witness: chain ending at a supported unreleased rung validates", func(t *testing.T) {
		if !IsSupportedSchema("0.4") || IsReleasedSchema("0.4") {
			t.Fatal("witness is void: 0.4 must be supported and unreleased for this test to mean anything")
		}
		if serr := validateRegistry(full); serr != nil {
			t.Fatalf("terminal rung 0.4 is supported and unreleased, which is legal: %v", serr)
		}
	})

	t.Run("control: prefix whose every target is released validates", func(t *testing.T) {
		prefix := full[:2]
		for _, s := range prefix {
			if !IsReleasedSchema(s.To) {
				t.Fatalf("control is void: prefix step %s->%s must have a RELEASED target", s.From, s.To)
			}
		}
		if serr := validateRegistry(prefix); serr != nil {
			t.Fatalf("control must validate: %v", serr)
		}
	})
}

// P3. From must be SUPPORTED as well as released. Without that gate
// supportedIndex(From) is -1, so next becomes 0 and the adjacency check
// ACCEPTS a step originating at a schema this build cannot read.
//
// ISOLATION: 0.1 is released but not supported here, so only the
// readability rule can refuse. The mutation that matters is deleting that
// gate: adjacency must then ACCEPT, which is what proves the gate is
// load-bearing rather than shadowed.
//
// CONTROL, no code change: add 0.1 to the supported set and the same step
// validates.
func TestStepSourceMustBeReadable(t *testing.T) {
	step := []Step{{From: "0.1", To: "0.2", Apply: func(*payload) error { return nil }}}

	t.Run("witness: released but unreadable source refuses", func(t *testing.T) {
		withSchemaTables(t, []string{"0.2"}, []string{"0.1"})
		serr := validateRegistry(step)
		if serr == nil {
			t.Fatal("a step may not originate at a schema this build cannot read")
		}
		if !strings.Contains(serr.Detail, "cannot read") {
			t.Fatalf("detail %q must name the readability rule", serr.Detail)
		}
		// Guard against the shadow this test exists to rule out: with the
		// gate removed, adjacency would compute -1+1 == 0 == index("0.2")
		// and accept.
		if supportedIndex("0.1") != -1 || supportedIndex("0.2") != 0 {
			t.Fatal("witness is void: the -1 adjacency shadow is not set up")
		}
	})

	t.Run("control: same step with 0.1 supported validates", func(t *testing.T) {
		withSchemaTables(t, []string{"0.1", "0.2"}, []string{"0.1"})
		if serr := validateRegistry(step); serr != nil {
			t.Fatalf("control must validate: %v", serr)
		}
	})
}

// P4. A step originating at the LAST supported schema has no successor and
// must refuse cleanly.
//
// Before issue 0031 the adjacency diagnostic formatted itself with
// releasedSchemas[next] without a bounds check, so this input panicked with
// index out of range inside the error path. The existing "interior
// regression" case in TestRegistryValidationRefusesMalformedSteps misses it
// by luck: it uses a four-entry table where next == 2 is in range.
//
// ISOLATION: To is both supported AND released here, and the step is not a
// self-loop, carries a transform, uses valid labels and duplicates nothing —
// so no gate before adjacency can fire.
//
// CONTROL, no code change: the same two labels in the other direction
// validate. Only the direction differs.
func TestLastSupportedSchemaHasNoSuccessor(t *testing.T) {
	withSchemaTables(t, []string{"0.1", "0.2"}, []string{"0.1", "0.2"})
	noop := func(*payload) error { return nil }

	serr := validateRegistry([]Step{{From: "0.2", To: "0.1", Apply: noop}})
	if serr == nil {
		t.Fatal("a step from the last supported schema has no successor and must refuse")
	}
	if !strings.Contains(serr.Detail, "has no successor") {
		t.Fatalf("detail %q must name the missing successor", serr.Detail)
	}

	if serr := validateRegistry([]Step{{From: "0.1", To: "0.2", Apply: noop}}); serr != nil {
		t.Fatalf("control must validate; only the direction differs: %v", serr)
	}
}

// P5. Endpoint support is gated ONCE, over both endpoints together.
//
// ISOLATION: with from == to == "9.9" a per-endpoint gate would be masked
// by its twin, so no mutation could isolate either. One check can be
// removed cleanly, and when it is, this call must SUCCEED — "9.9" == "9.9"
// returns the empty chain.
//
// What else could refuse this input? Nothing. IsSchemaLabel("9.9") is true,
// so the label-form gate cannot fire; steps is nil, so validateRegistry
// returns without iterating; from == to, so no path walk and no
// "no tested migration path" refusal is reachable.
//
// CONTROL, no code change: the same shape with supported endpoints
// succeeds, proving the from == to return is reachable.
func TestEndpointSupportGatedOnce(t *testing.T) {
	if !IsSchemaLabel("9.9") {
		t.Fatal("witness is void: 9.9 must be valid V1 form so the label gate cannot fire")
	}

	sub, serr := selectChain("9.9", "9.9", nil)
	if serr == nil {
		t.Fatal("unreadable endpoints must refuse even when from == to")
	}
	if serr.Category != persist.CatUnsupportedVersion {
		t.Fatalf("category = %s, want unsupported-version (%v)", serr.Category, serr)
	}
	if !strings.Contains(serr.Detail, "9.9") || !strings.Contains(serr.Detail, "0.1") {
		t.Fatalf("§8.2 V3 requires the diagnostic to name both versions: %q", serr.Detail)
	}
	if sub != nil {
		t.Fatal("refusal must not return a plan")
	}

	if sub, serr := selectChain(emittedSchema, emittedSchema, nil); serr != nil || len(sub) != 0 {
		t.Fatalf("control: supported endpoints must succeed with an empty chain, got %v %v", sub, serr)
	}
}

// P6. The supported table must track what internal/persist actually
// accepts. Two copies of a version fact drifting apart is the defect class
// this whole issue exists to remove, and persist exports no accessor for
// its label, so this binds to OBSERVED reader behavior instead.
//
// LIMIT, stated rather than hidden: this is an observed-behavior binding,
// not a total one. A real binding needs internal/persist to export its
// accepted label; that package is out of scope on this branch.
func TestSupportedSetTracksPersistGate(t *testing.T) {
	view, lerr := persist.LoadFile(goldenCopy(t, "golden/packages/thin-valid.domo"))
	if lerr != nil {
		t.Fatalf("load golden: %v", lerr)
	}
	if view.FormatVersion != emittedSchema {
		t.Fatalf("golden package is labelled %q but this build emits %q", view.FormatVersion, emittedSchema)
	}
	if !IsSupportedSchema(view.FormatVersion) {
		t.Fatalf("the reader accepted %q, so it must be in the supported table %v",
			view.FormatVersion, supportedSchemas)
	}
	// The corpus pins these two as rejected by the reader; the table must
	// agree that they are unreadable.
	for _, rejected := range []string{"0.0", "0.2"} {
		if IsSupportedSchema(rejected) {
			t.Errorf("%q is pinned as rejected by the reader corpus but the table claims support", rejected)
		}
	}
}

// P7a. A Report may only be built from a plan that ENDS at the label this
// build emits. This is the assertion that would have caught my own earlier
// design, in which a synthetic plan produced a report claiming steps
// terminating at 0.4 beside a package the writer had labelled 0.1.
//
// ISOLATION: flipChain starts at 0.1, which is where the document is, and
// each step begins where the previous ended, so the contiguity branch
// cannot fire. Only the terminus check can refuse this.
//
// CONTROL, no code change: the empty plan from the same source succeeds.
func TestReportPlanMustEndAtEmittedSchema(t *testing.T) {
	futureLadder(t)
	view := &persist.PackageView{FormatVersion: "0.1"}

	_, gerr := newReport(view, "src.domo", flipChain())
	if gerr == nil {
		t.Fatal("a plan ending at 0.4 describes a package this build will never write")
	}
	if !strings.Contains(gerr.Detail, "ends at 0.4") || !strings.Contains(gerr.Detail, emittedSchema) {
		t.Fatalf("detail %q must name the plan terminus and the emitted label", gerr.Detail)
	}

	rep, gerr := newReport(view, "src.domo", nil)
	if gerr != nil {
		t.Fatalf("control: empty plan from an emitted-label source must succeed: %v", gerr)
	}
	if rep.TargetVersion != emittedSchema || !rep.AlreadyAtTarget {
		t.Fatalf("control report is wrong: %+v", rep)
	}
}

// P7b. A Report's plan must be CONTIGUOUS.
//
// ISOLATION: the witness plan {0.2->0.1} is chosen deliberately because it
// PASSES the terminus check — it ends at 0.1, the emitted label — so only
// contiguity can refuse it. With the contiguity check removed, cur becomes
// 0.1, the terminus check passes, and the call SUCCEEDS; that success is
// the proof.
//
// CONTROL, no code change: the empty plan succeeds.
func TestReportPlanMustBeContiguous(t *testing.T) {
	view := &persist.PackageView{FormatVersion: "0.1"}
	disjoint := []Step{{From: "0.2", To: "0.1", Apply: func(*payload) error { return nil }}}

	if disjoint[0].To != emittedSchema {
		t.Fatal("witness is void: the plan must END correctly so only contiguity can refuse it")
	}

	_, gerr := newReport(view, "src.domo", disjoint)
	if gerr == nil {
		t.Fatal("a plan starting at 0.2 cannot describe a document sitting at 0.1")
	}
	if !strings.Contains(gerr.Detail, "starts at 0.2") || !strings.Contains(gerr.Detail, "document is at 0.1") {
		t.Fatalf("detail %q must name both the step origin and the document's actual label", gerr.Detail)
	}

	if _, gerr := newReport(view, "src.domo", nil); gerr != nil {
		t.Fatalf("control: empty plan must succeed: %v", gerr)
	}
}

// P7c. The production report agrees with the package on disk.
//
// LIMIT, stated rather than hidden: with an empty shipped chain and a
// single emitted label, no real multi-step migration is executable on this
// branch, so report-versus-bytes coherence is observable ONLY in the
// already-at-target case. The general invariant is enforced at construction
// and proven by P7a/P7b, NOT observed end to end. This test claims nothing
// about non-empty migration coverage.
func TestProductionReportMatchesPackage(t *testing.T) {
	src := goldenCopy(t, "golden/packages/thin-valid.domo")
	view, lerr := persist.LoadFile(src)
	if lerr != nil {
		t.Fatalf("load: %v", lerr)
	}
	rep, err := Inspect(src)
	if err != nil {
		t.Fatalf("inspect: %v", err)
	}
	if rep.SourceVersion != view.FormatVersion {
		t.Errorf("SourceVersion %q != package label %q", rep.SourceVersion, view.FormatVersion)
	}
	if rep.TargetVersion != view.FormatVersion {
		t.Errorf("TargetVersion %q != the label the writer emits for this package, %q",
			rep.TargetVersion, view.FormatVersion)
	}
	if !rep.AlreadyAtTarget {
		t.Errorf("source and target schema are both %q, so the report must say already-at-target",
			rep.SourceVersion)
	}
	if len(rep.Steps) != 0 {
		t.Errorf("already-at-target report must claim no steps, got %v", rep.Steps)
	}
}

// F1: registry integrity is checked over the WHOLE table before path
// selection. Each case uses from==to so only the malformed entry can
// trigger the refusal — proving unused steps refuse too — and asserts
// the exact category plus a distinctive diagnostic fragment.
func TestRegistryValidationRefusesMalformedSteps(t *testing.T) {
	futureLadder(t)
	noop := func(*payload) error { return nil }
	cases := []struct {
		name    string
		steps   []Step
		wantMsg string
	}{
		{
			name:    "nil transform",
			steps:   []Step{{From: "0.1", To: "0.2"}},
			wantMsg: "carries no transform",
		},
		{
			name:    "invalid label form in unused step",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "01.2", To: "0.3", Apply: noop}},
			wantMsg: "decimal MAJOR.MINOR",
		},
		{
			// 0.4 is supported but NOT released under futureLadder, so
			// this refuses on the §8.2 V4 origin rule.
			name:    "unreleased source in unused step",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "0.4", To: "0.9", Apply: noop}},
			wantMsg: "starts at a non-released schema",
		},
		{
			name:    "never-released source in unused step",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "0.0", To: "0.1", Apply: noop}},
			wantMsg: "starts at a non-released schema",
		},
		{
			// 0.3 is released AND supported, so the origin gates pass and
			// only the unreadable TARGET can refuse this one.
			name:    "unsupported target in unused step",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "0.3", To: "0.9", Apply: noop}},
			wantMsg: "advances to a schema this build cannot read",
		},
		{
			name:    "self-loop in unused step",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "0.2", To: "0.2", Apply: noop}},
			wantMsg: "self-loop",
		},
		{
			name:    "duplicate From at front",
			steps:   []Step{{From: "0.1", To: "0.2", Apply: noop}, {From: "0.1", To: "0.2", Apply: noop}},
			wantMsg: "duplicates source 0.1 already claimed by step 0",
		},
		{
			name: "duplicate From hidden behind a valid prefix",
			steps: []Step{
				{From: "0.1", To: "0.2", Apply: noop},
				{From: "0.2", To: "0.3", Apply: noop},
				{From: "0.3", To: "0.4", Apply: noop},
				{From: "0.2", To: "0.3", Apply: noop},
			},
			wantMsg: "duplicates source 0.2 already claimed by step 1",
		},
		{
			name:    "skipped minor",
			steps:   []Step{{From: "0.1", To: "0.3", Apply: noop}},
			wantMsg: "must advance to the next supported minor 0.2",
		},
		{
			// Interior regression: next is in range, so the adjacency
			// diagnostic itself is what refuses. The boundary case where
			// From is the LAST supported schema is a separate test
			// (TestLastSupportedSchemaHasNoSuccessor) because before
			// issue 0031 it panicked instead of refusing, and this case
			// misses it by luck.
			name:    "interior regression",
			steps:   []Step{{From: "0.2", To: "0.1", Apply: noop}},
			wantMsg: "must advance to the next supported minor 0.3",
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, serr := selectChain("0.1", "0.1", tc.steps)
			if serr == nil {
				t.Fatal("malformed registry must refuse even when no path is requested")
			}
			if serr.Category != persist.CatInvalidFieldSyntax || serr.Stage != persist.StageManifest {
				t.Fatalf("category/stage = %s/%s, want invalid-field-syntax/manifest (%v)",
					serr.Category, serr.Stage, serr)
			}
			if !strings.Contains(serr.Detail, tc.wantMsg) {
				t.Fatalf("detail %q missing %q", serr.Detail, tc.wantMsg)
			}
		})
	}
}

func TestSelectChainOrdering(t *testing.T) {
	futureLadder(t)
	noop := func(*payload) error { return nil }
	steps := []Step{
		{From: "0.3", To: "0.4", Apply: noop},
		stepFlip("0.2", "0.3"), // declared out of order on purpose
		stepFlip("0.1", "0.2"),
	}
	sub, serr := selectChain("0.1", "0.3", steps)
	if serr != nil {
		t.Fatalf("full path: %v", serr)
	}
	if len(sub) != 2 || sub[0].From != "0.1" || sub[1].From != "0.2" {
		t.Fatalf("chain must be ordered 0.1->0.2->0.3, got %v", sub)
	}
	if sub, serr = selectChain("0.2", "0.4", steps); serr != nil || len(sub) != 2 {
		t.Fatalf("sub-chain from interior schema: %v %v", sub, serr)
	}
	if _, serr = selectChain("0.0", "0.1", steps); serr == nil ||
		serr.Category != persist.CatUnsupportedVersion {
		t.Fatalf("unreadable source must refuse unsupported-version, got %v", serr)
	}
	if _, serr = selectChain("0.1", "0.9", steps); serr == nil ||
		serr.Category != persist.CatUnsupportedVersion {
		t.Fatalf("unreachable target must refuse with actionable diagnostic, got %v", serr)
	}
	// Under the adjacency rule a regression is a registry-integrity
	// violation refused before path selection; genuine cycles are
	// structurally impossible because every accepted step strictly
	// advances one supported minor.
	regressing := []Step{
		{From: "0.1", To: "0.2", Apply: noop},
		{From: "0.2", To: "0.1", Apply: noop},
	}
	if _, serr = selectChain("0.1", "0.2", regressing); serr == nil ||
		!strings.Contains(serr.Detail, "next supported minor") {
		t.Fatalf("regression step must refuse at registry validation, got %v", serr)
	}
	if sub, serr = selectChain("0.1", "0.1", steps); serr != nil || len(sub) != 0 {
		t.Fatalf("from==to yields empty chain, got %v %v", sub, serr)
	}
	// Relocated from the save-machine harness, which no longer obtains a
	// Report and so can no longer assert schema step order. The claim
	// belongs here, where schema labels legitimately live.
	full, serr := selectChain("0.1", "0.4", flipChain())
	if serr != nil {
		t.Fatalf("full four-rung path: %v", serr)
	}
	if len(full) != 3 || full[0].From != "0.1" || full[1].From != "0.2" ||
		full[2].From != "0.3" || full[2].To != "0.4" {
		t.Fatalf("plan must be ordered 0.1->0.2->0.3->0.4, got %v", full)
	}
}

func TestInspectGoldenPackages(t *testing.T) {
	for _, rel := range []string{"golden/packages/thin-valid.domo", "golden/packages/portable-valid.domo"} {
		rep, err := Inspect(goldenCopy(t, rel))
		if err != nil {
			t.Fatalf("%s inspect: %v", rel, err)
		}
		if !rep.ValidationOK || !rep.AlreadyAtTarget {
			t.Fatalf("%s: validation=%v alreadyAtTarget=%v", rel, rep.ValidationOK, rep.AlreadyAtTarget)
		}
		if rep.SourceVersion != "0.1" || rep.TargetVersion != "0.1" || len(rep.Steps) != 0 {
			t.Fatalf("%s: unexpected plan %+v", rel, rep)
		}
		if len(rep.Preservation) != 0 {
			t.Fatalf("%s: golden package must be preservation-clean: %v", rel, rep.Preservation)
		}
	}
}

// Pre-effect gates: unsupported labels and foreign namespaces reject
// through the reader before any effect, leaving any existing target
// byte-identical and creating no temporary candidates.
func TestPreEffectGatesLeaveTargetUntouched(t *testing.T) {
	newer := corpusFile(t, "migration/newer-minor-reject.json")
	older := corpusFile(t, "migration/older-minor-unknown-label.json")
	foreignPkg := mustAssemble(t, map[string][]byte{
		"assets/descriptors.json": []byte(`{}`),
		"ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json": corpusBatchBytes,
		"future/widget.bin": []byte("hostile"),
	})
	cases := []struct {
		name string
		data []byte
		want persist.Category
	}{
		{"newer minor 0.2", mustAssemble(t, map[string][]byte{"domowik.json": newer}), persist.CatUnsupportedVersion},
		{"never-released 0.0", mustAssemble(t, map[string][]byte{"domowik.json": older}), persist.CatUnsupportedVersion},
		{"foreign namespace under 0.1", foreignPkg, persist.CatInvalidEntryName},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			dir := t.TempDir()
			src := filepath.Join(dir, "src.domo")
			if err := os.WriteFile(src, tc.data, 0o644); err != nil {
				t.Fatal(err)
			}
			target := filepath.Join(dir, "target.domo")
			sentinel := []byte("sentinel-old-package")
			if err := os.WriteFile(target, sentinel, 0o644); err != nil {
				t.Fatal(err)
			}
			if _, ierr := Inspect(src); ierr == nil {
				t.Fatal("inspect must reject pre-effect")
			} else if c, ok := persist.CategoryOf(ierr); !ok || c != tc.want {
				t.Fatalf("inspect category = %v, want %s (%v)", c, tc.want, ierr)
			}
			if _, _, rerr := Run(src, Options{Target: target}); rerr == nil {
				t.Fatal("run must reject pre-effect")
			} else if c, ok := persist.CategoryOf(rerr); !ok || c != tc.want {
				t.Fatalf("run category = %v, want %s (%v)", c, tc.want, rerr)
			}
			assertUntouched(t, target, sentinel)
			assertNoLeftovers(t, dir)
		})
	}
}

// Precise idempotency: a source already at the target schema is the
// detected no-op — success reported, no temporary candidate, no byte
// written.
func TestRunIdempotentAlreadyAtTarget(t *testing.T) {
	src := goldenCopy(t, "golden/packages/thin-valid.domo")
	before, err := os.ReadFile(src)
	if err != nil {
		t.Fatal(err)
	}
	rep, res, rerr := Run(src, Options{})
	if rerr != nil {
		t.Fatalf("run: %v", rerr)
	}
	if res != nil {
		t.Fatal("no-op must not produce a save result")
	}
	if !rep.AlreadyAtTarget || len(rep.Steps) != 0 || rep.TargetPath != src {
		t.Fatalf("unexpected report %+v", rep)
	}
	after, rerr := os.ReadFile(src)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if string(before) != string(after) {
		t.Fatal("idempotent run rewrote the source package")
	}
	assertNoLeftovers(t, filepath.Dir(src))
}

// Unknown optional members that the full-knowledge writer cannot
// re-serialize force safe-write refusal naming each finding (U2/L32);
// reading and dry-run reporting stay successful.
func TestUnknownOptionalPreserveOrRefuse(t *testing.T) {
	snapshot := []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{},"batchMetadata":{"recentBatchIds":[]},"legacyExtra":{"note":"kept opaque"}}`)
	pkg := mustAssemble(t, map[string][]byte{
		"snapshot/state.json": snapshot,
	})
	src := writeTemp(t, "unknown.domo", pkg)

	rep, ierr := Inspect(src)
	if ierr != nil {
		t.Fatalf("inspect: %v", ierr)
	}
	if !rep.ValidationOK || len(rep.Preservation) != 1 ||
		!strings.Contains(rep.Preservation[0], "legacyExtra") {
		t.Fatalf("read must succeed with named finding, got %+v", rep)
	}

	target := filepath.Join(t.TempDir(), "out.domo")
	_, rerr := runSynthetic(t, flipChain(), src, target, Options{Target: target})
	if rerr == nil {
		t.Fatal("write-back must be refused when unknown members cannot be carried")
	}
	if c, ok := persist.CategoryOf(rerr); !ok || c != persist.CatSafeWriteRefusal {
		t.Fatalf("category = %v, want safe-write-refusal (%v)", c, rerr)
	}
	if _, serr := os.Lstat(target); !os.IsNotExist(serr) {
		t.Fatal("refused migration must not create the target")
	}
	assertNoLeftovers(t, filepath.Dir(target))
}

// Save MECHANICS over the migration write path, driven by synthetic
// transforms. It asserts no schema claim: see syntheticTransforms for the
// explicit non-claims, and TestSelectChainOrdering for the plan-order
// assertion that used to live here.
func TestMigrationThroughSaveMachineHappyPath(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src.domo")
	if err := os.WriteFile(src, corpusFile(t, "golden/packages/thin-valid.domo"), 0o644); err != nil {
		t.Fatal(err)
	}
	sourceBytes, err := os.ReadFile(src)
	if err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(dir, "out.domo")
	res, rerr := runSynthetic(t, flipChain(), src, target, Options{Target: target})
	if rerr != nil {
		t.Fatalf("run: %v", rerr)
	}
	if res == nil || res.DurabilityCaveat {
		t.Fatalf("expected clean save result, got %+v", res)
	}
	if len(res.Phases) != 6 || res.Phases[0] != persist.S0TempCreated || res.Phases[5] != persist.S5DirSynced {
		t.Fatalf("phases = %v", res.Phases)
	}
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatalf("reload migrated output: %v", lerr)
	}
	// The writer emits exactly one label (§8.1 V1) no matter how many
	// synthetic transforms ran. Three toggles moved the profile; the
	// schema label did not move, and nothing here claims it did.
	if view.Profile != "portable" || view.FormatVersion != emittedSchema {
		t.Fatalf("output wrong: profile=%s version=%s", view.Profile, view.FormatVersion)
	}
	if state := view.Projection.State(); len(state.Entities) != 1 {
		t.Fatalf("projection lost its entity across migration")
	}
	after, rerr := os.ReadFile(src)
	if rerr != nil {
		t.Fatal(rerr)
	}
	if string(sourceBytes) != string(after) {
		t.Fatal("original source file must survive the migration untouched")
	}
	assertNoLeftovers(t, dir)
}

func stepFailing(msg string) Step {
	return Step{From: "0.1", To: "0.2", Apply: func(*payload) error { return errors.New(msg) }}
}

// A failing step aborts strictly before any effect: no target, no
// temporary candidate.
func TestStepFailureAbortsBeforeAnyEffect(t *testing.T) {
	dir := t.TempDir()
	src := filepath.Join(dir, "src.domo")
	if err := os.WriteFile(src, corpusFile(t, "golden/packages/thin-valid.domo"), 0o644); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(dir, "out.domo")
	failing := stepFailing("injected transform failure")
	steps := append([]Step{failing}, flipChain()[1:]...)
	_, rerr := runSynthetic(t, steps, src, target, Options{Target: target})
	if rerr == nil {
		t.Fatal("step failure must abort the run")
	}
	if c, ok := persist.CategoryOf(rerr); !ok || c != persist.CatSafeWriteRefusal {
		t.Fatalf("category = %v (%v)", c, rerr)
	}
	if !strings.Contains(rerr.Error(), "before any effect") || !strings.Contains(rerr.Error(), "injected transform failure") {
		t.Fatalf("diagnostic must name the transform and the phase: %v", rerr)
	}
	if _, serr := os.Lstat(target); !os.IsNotExist(serr) {
		t.Fatal("aborted migration must not create the target")
	}
	assertNoLeftovers(t, dir)
}

// F-matrix proof over the migration save path: deterministic hook-based
// injection asserting target-untouched / leftover-cleanable outcomes at
// S1, S2, S3, and S4.
func TestMigrationSaveFaultMatrix(t *testing.T) {
	setup := func(t *testing.T) (string, string) {
		dir := t.TempDir()
		src := filepath.Join(dir, "src.domo")
		if err := os.WriteFile(src, corpusFile(t, "golden/packages/thin-valid.domo"), 0o644); err != nil {
			t.Fatal(err)
		}
		return src, filepath.Join(dir, "out.domo")
	}
	steps := flipChain()

	t.Run("F2 write interrupted leaves no target and cleans leftovers", func(t *testing.T) {
		src, target := setup(t)
		hooks := &persist.SaveHooks{Writer: func(w io.Writer) io.Writer { return failingWriter{} }}
		_, rerr := runSynthetic(t, steps, src, target, Options{Target: target, Hooks: hooks})
		if rerr == nil || !strings.Contains(rerr.Error(), "candidate write interrupted") {
			t.Fatalf("expected X2/F2 abort, got %v", rerr)
		}
		if _, serr := os.Lstat(target); !os.IsNotExist(serr) {
			t.Fatal("target must not exist after interrupted candidate write")
		}
		assertNoLeftovers(t, filepath.Dir(target))
	})

	t.Run("F3 candidate validation failure aborts with target untouched", func(t *testing.T) {
		src, target := setup(t)
		hooks := &persist.SaveHooks{Validate: func(string) *persist.Error {
			return &persist.Error{Category: persist.CatChecksumError, Stage: persist.StageDigest,
				Detail: "injected candidate corruption"}
		}}
		_, rerr := runSynthetic(t, steps, src, target, Options{Target: target, Hooks: hooks})
		if rerr == nil || !strings.Contains(rerr.Error(), "candidate-invalid (F3)") {
			t.Fatalf("expected F3 abort, got %v", rerr)
		}
		if _, serr := os.Lstat(target); !os.IsNotExist(serr) {
			t.Fatal("target must not exist after failed validation")
		}
		assertNoLeftovers(t, filepath.Dir(target))
	})

	t.Run("F4 content sync failure aborts with target untouched", func(t *testing.T) {
		src, target := setup(t)
		hooks := &persist.SaveHooks{AfterPhase: func(p persist.SavePhase) error {
			if p == persist.S3Synced {
				return errors.New("injected fsync failure")
			}
			return nil
		}}
		_, rerr := runSynthetic(t, steps, src, target, Options{Target: target, Hooks: hooks})
		if rerr == nil || !strings.Contains(rerr.Error(), "aborted after S3 (F4)") {
			t.Fatalf("expected F4 abort, got %v", rerr)
		}
		if _, serr := os.Lstat(target); !os.IsNotExist(serr) {
			t.Fatal("target must not exist after sync failure")
		}
		assertNoLeftovers(t, filepath.Dir(target))
	})

	t.Run("F5 replace failure reports old-or-new and leftover stays cleanable", func(t *testing.T) {
		src, target := setup(t)
		// Establish a previous generation at the target.
		if _, rerr := runSynthetic(t, steps, src, target, Options{Target: target}); rerr != nil {
			t.Fatal(rerr)
		}
		oldBytes, err := os.ReadFile(target)
		if err != nil {
			t.Fatal(err)
		}
		hooks := &persist.SaveHooks{Replace: func(tmpPath, targetPath string) error {
			return errors.New("injected sharing violation")
		}}
		_, rerr := runSynthetic(t, steps, src, target, Options{Target: target, Hooks: hooks})
		if rerr == nil || !strings.Contains(rerr.Error(), "atomic replace failed (F5)") {
			t.Fatalf("expected F5 report, got %v", rerr)
		}
		now, rerr := os.ReadFile(target)
		if rerr != nil {
			t.Fatal(rerr)
		}
		if string(now) != string(oldBytes) {
			t.Fatal("failed replace left neither clearly old nor new; rename atomicity assumption violated")
		}
		removed, kept := persist.CleanLeftovers(target)
		// The F5 candidate completed S1 with a valid package signature,
		// so the RS3 auto-clean rule must KEEP it for explicit user
		// action rather than remove it (never partially promoted).
		if len(removed)+len(kept) != 1 || len(kept) != 1 {
			t.Fatalf("F5 leftover must stay discoverable and preserved, removed=%v kept=%v", removed, kept)
		}
	})
}

func TestReportRenderDeterministic(t *testing.T) {
	a := &Report{SourcePath: "s", TargetPath: "t", SourceVersion: "0.1", TargetVersion: "0.1",
		ValidationOK: true, AlreadyAtTarget: true}
	b := &Report{SourcePath: "s", TargetPath: "t", SourceVersion: "0.1", TargetVersion: "0.1",
		ValidationOK: true, AlreadyAtTarget: true}
	if a.Render("x") != b.Render("x") {
		t.Fatal("equal reports must render identically")
	}
	out := a.Render("no change")
	for _, want := range []string{"schema: 0.1 -> 0.1", "validation: ok", "steps: 0", "preservation: clean", "action: no change"} {
		if !strings.Contains(out, want) {
			t.Fatalf("render missing %q:\n%s", want, out)
		}
	}
}
