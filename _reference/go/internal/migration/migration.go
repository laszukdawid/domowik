// Package migration implements issue 0031: format migrations FROM released
// experimental 0.N predecessors TO the schema this build emits
// (FORMAT-SPEC §8.1 V1, §8.2).
//
// The two endpoints are not the same concept. "Between released schemas"
// would assert that both ends are released. A chain's target only has to be
// readable: a schema MAY be supported while its release is still being
// prepared and tested, and §8.2 V6 makes release status a dated fact that
// changes at publication, not a property of the label.
//
// Three distinct schema concepts, each with its own specification anchor.
// Fusing them into one table is what made this package assert, until issue
// 0031, that v0.1 had been publicly released:
//
//   - EMITTED (§8.1 V1): formatVersion is "the sole schema identifier" and
//     "a writer states exactly which schema it emits". Exactly one label,
//     the const emittedSchema.
//   - SUPPORTED (§8.2 V3): the minors this reader "claims to support".
//     internal/persist owns the enforcing gate and names the same concept
//     "reader-supported" (reader.go).
//   - RELEASED (§8.2 V4/V5/V6): the publicly released schemas. EMPTY.
//
// Reviewer-accepted checkpoint design:
//
//   - The migration chain is explicit, ordered, and data-driven: a plain
//     slice of Step values, applied in order, each declaring its source
//     and successor schema.
//   - A Step may only originate at a RELEASED schema (§8.2 V4). Because
//     §8.2 V6 records the released set as empty, the production chain is
//     not merely empty but UNFILLABLE: no step can be added without first
//     recording a release, which is the same change V6 already requires.
//     No positive migration is fabricated.
//   - Both endpoints of a requested migration must be SUPPORTED (§8.2 V3).
//     Labels this build cannot read reject unsupported-version BEFORE any
//     effect, with a diagnostic naming both versions.
//   - A Report is constructed only from a plan that composes from the
//     source label to emittedSchema, so a report can never describe a
//     package the writer will not produce.
//   - Migration produces a COMPLETE new package exclusively through the
//     persist.SaveDocument §9.7 S0–S5 machine (V4): never an in-place
//     edit, with the original preserved until migrated output validates
//     (S2) and atomic replacement succeeds (S4). Opening stays
//     read-only; migration runs only on this explicit call path.
//   - Unknown optional data is preserved opaquely or the write is
//     refused (U2/L32): entity-level unknowns ride the document codec
//     round trip; unknown manifest/snapshot/batch members become named
//     refusal reasons enforced by the writer, so silent stripping is
//     structurally impossible.
//   - Foreign entry namespaces reject pre-effect through the reader's
//     closed v0.1 namespace set (§7.3/U3); this package never bypasses
//     persist.LoadFile to reach raw entries.
package migration

import (
	"fmt"
	"os"
	"sort"
	"strings"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// emittedSchema is the ONE schema label this build writes. FORMAT-SPEC
// §8.1 V1 makes it exactly one value — the manifest formatVersion is "the
// sole schema identifier" and "a writer states exactly which schema it
// emits" — so it is a constant, not the end of a list. Nothing in this
// package derives it by indexing a table.
const emittedSchema = "0.1"

// supportedSchemas lists the schema labels this build can READ, in
// ascending order (FORMAT-SPEC §8.2 V3: a reader accepts exactly the
// experimental minors it claims to support). internal/persist owns the
// enforcing gate and names the same concept "reader-supported"; this table
// must track it, which TestSupportedSetTracksPersistGate checks against the
// reader's observed behavior.
var supportedSchemas = []string{"0.1"}

// releasedSchemas lists every PUBLICLY RELEASED schema of the experimental
// 0.N series, ascending (FORMAT-SPEC §8.2 V4/V5/V6).
//
// It is EMPTY. §8.2 V6 records as a dated fact that no 0.N schema has been
// publicly released and that the predecessor set is empty; issue 0018's
// release-candidate audit found no independently evidenced predecessor
// release. v0.1 is UNRELEASED, so "0.1" is deliberately absent here even
// though it is both emitted and supported.
//
// V6 binds whoever releases a 0.N schema to update that bullet in the same
// change; this table is that change's other half. While it is empty no
// migration Step can exist at all, because validateRegistry requires every
// step to originate at a released schema.
var releasedSchemas []string

// IsReleasedSchema reports whether s names a publicly released schema
// (FORMAT-SPEC §8.2 V4). The released set is empty per §8.2 V6, so this
// currently returns false for EVERY label, "0.1" included.
func IsReleasedSchema(s string) bool {
	for _, r := range releasedSchemas {
		if r == s {
			return true
		}
	}
	return false
}

// IsSupportedSchema reports whether this build can read a package labelled
// s (FORMAT-SPEC §8.2 V3). Support is independent of release: v0.1 is
// supported and unreleased.
func IsSupportedSchema(s string) bool {
	for _, r := range supportedSchemas {
		if r == s {
			return true
		}
	}
	return false
}

// IsSchemaLabel validates the §8.1 V1 decimal MAJOR.MINOR form without
// leading zeros. Syntactic validity is separate from reader acceptance
// (§8.2 V3), which IsSupportedSchema owns, and from release status
// (§8.2 V4), which IsReleasedSchema owns.
func IsSchemaLabel(s string) bool {
	major, minor, found := strings.Cut(s, ".")
	if !found || major == "" || minor == "" {
		return false
	}
	if !allDigits(major) || !allDigits(minor) {
		return false
	}
	if len(major) > 1 && major[0] == '0' {
		return false
	}
	if len(minor) > 1 && minor[0] == '0' {
		return false
	}
	return true
}

func allDigits(s string) bool {
	for i := 0; i < len(s); i++ {
		if s[i] < '0' || s[i] > '9' {
			return false
		}
	}
	return len(s) > 0
}

// payload is the mutable migration document flowing through the chain.
type payload struct {
	profile     string
	state       document.State
	retained    []*document.Batch
	assetRoot   []string
	descriptors []byte
	blobs       map[string][]byte
	preview     map[string][]byte
	refusals    []string
}

// Step transforms one schema into its declared successor. The explicit
// ordered-chain contract binds every registry entry, whether or not a
// given request traverses it:
//
//   - Apply is non-nil;
//   - From and To are valid §8.1 V1 decimal MAJOR.MINOR labels;
//   - From names a RELEASED schema (§8.2 V4 attaches the migration duty to
//     released schemas only). This is what makes the shipped registry
//     unfillable while §8.2 V6 records an empty released set;
//   - From names a SUPPORTED schema: a transform cannot run on a package
//     this build cannot read, and §8.2 V4 scopes the duty to a later
//     release "that claims to open it";
//   - To names a SUPPORTED schema. To is deliberately NOT required to be
//     released: a target may legitimately be supported BEFORE it is
//     published, while its release is being prepared and tested, and
//     requiring released status would reject that valid pre-publication
//     state. Release status is a dated fact that changes at publication
//     (§8.2 V6), so it is not a property this rule can key on;
//   - To is exactly the next supported minor after From (no self-loops,
//     no regressions, no skipped minors);
//   - no two steps share the same From.
//
// Under these rules the chain is acyclic by construction. Violations
// refuse before any path selection; silently ignoring a dead-but-broken
// entry would let later requests resolve through a partially corrupt
// registry.
type Step struct {
	From  string
	To    string
	Apply func(*payload) error
}

// chain is the production migration registry in execution order. It is
// EMPTY, and while releasedSchemas is empty it cannot be anything else:
// every Step must originate at a released schema, so validateRegistry
// refuses any entry added here. Authoring a step requires first recording
// a release (§8.2 V6) and supplying a specified successor schema with its
// conformance corpus (V4; no fabricated positive migration).
var chain []Step

// supportedIndex returns the position of s in the ascending supported
// table, or -1 when absent.
func supportedIndex(s string) int {
	for i, r := range supportedSchemas {
		if r == s {
			return i
		}
	}
	return -1
}

// validateRegistry enforces the full Step contract over the ENTIRE
// registry before any path selection. Every violation refuses with a
// diagnostic naming the offending index and cause; nothing is skipped,
// deduplicated by traversal order, or tolerated as unreachable.
//
// While releasedSchemas is empty (§8.2 V6) the released check below
// refuses every possible entry, which is what makes the shipped chain
// unfillable rather than merely empty.
func validateRegistry(steps []Step) *persist.Error {
	refuse := func(format string, args ...any) *persist.Error {
		return &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageManifest,
			Detail: fmt.Sprintf(format, args...)}
	}
	from := map[string]int{}
	for i, s := range steps {
		if s.Apply == nil {
			return refuse("registry step %d (%s->%s) carries no transform", i, s.From, s.To)
		}
		if !IsSchemaLabel(s.From) || !IsSchemaLabel(s.To) {
			return refuse("registry step %d (%q->%q) does not use decimal MAJOR.MINOR labels", i, s.From, s.To)
		}
		if !IsReleasedSchema(s.From) {
			return refuse("registry step %d (%s->%s) starts at a non-released schema; §8.2 V4 attaches the migration duty to released schemas only, and the released set is %v",
				i, s.From, s.To, releasedSchemas)
		}
		if !IsSupportedSchema(s.From) {
			return refuse("registry step %d (%s->%s) starts at a schema this build cannot read; supported schemas are %v",
				i, s.From, s.To, supportedSchemas)
		}
		// To is checked for SUPPORT, never for release: a target may be
		// supported while its release is still being prepared, and
		// refusing it then would reject a valid pre-publication state.
		if !IsSupportedSchema(s.To) {
			return refuse("registry step %d (%s->%s) advances to a schema this build cannot read; supported schemas are %v",
				i, s.From, s.To, supportedSchemas)
		}
		if s.From == s.To {
			return refuse("registry step %d is a self-loop on %s", i, s.From)
		}
		if prev, dup := from[s.From]; dup {
			return refuse("registry step %d duplicates source %s already claimed by step %d", i, s.From, prev)
		}
		from[s.From] = i
		next := supportedIndex(s.From) + 1
		if next >= len(supportedSchemas) {
			// From is the LAST supported schema, so no successor exists.
			// Refusing here also keeps the diagnostic below from indexing
			// one past the end of the table, which is how this branch
			// panicked before issue 0031.
			return refuse("registry step %d (%s->%s) starts at the last supported schema %s, which has no successor",
				i, s.From, s.To, s.From)
		}
		if supportedIndex(s.To) != next {
			return refuse("registry step %d (%s->%s) must advance to the next supported minor %s",
				i, s.From, s.To, supportedSchemas[next])
		}
	}
	return nil
}

// selectChain first validates the whole registry, then resolves the
// ordered sub-chain carrying a document from `from` to `to`, refusing
// with an actionable unsupported-version diagnostic when no honest path
// exists. Both endpoints must be SUPPORTED (§8.2 V3); release status is
// a Step-authoring rule enforced in validateRegistry, not a property of
// the request. from == to yields an empty chain (already at target). The
// walk's visited-guard is defense in depth only: under validateRegistry
// adjacency every step strictly advances one minor, so cycles cannot
// arise.
func selectChain(from, to string, steps []Step) ([]Step, *persist.Error) {
	if serr := validateRegistry(steps); serr != nil {
		return nil, serr
	}
	refuse := func(format string, args ...any) *persist.Error {
		return &persist.Error{Category: persist.CatUnsupportedVersion, Stage: persist.StageManifest,
			Detail: fmt.Sprintf(format, args...)}
	}
	if !IsSchemaLabel(from) || !IsSchemaLabel(to) {
		return nil, refuse("schema labels must be decimal MAJOR.MINOR form: from=%q to=%q", from, to)
	}
	// ONE endpoint check, deliberately not two. When both endpoints carry
	// the same unsupported label a per-endpoint gate is masked by its
	// twin, so no mutation could isolate either one. §8.2 V3 requires the
	// diagnostic to name both versions, which this does.
	if !IsSupportedSchema(from) || !IsSupportedSchema(to) {
		return nil, refuse("cannot migrate %s -> %s: this build reads only %v; refusing before any effect",
			from, to, supportedSchemas)
	}
	if from == to {
		return nil, nil
	}
	cur := from
	var out []Step
	visited := map[string]bool{from: true}
	for cur != to {
		found := -1
		for i, s := range steps {
			if s.From == cur {
				found = i
				break
			}
		}
		if found < 0 {
			return nil, refuse("no tested migration path from %s to %s exists; released schemas are %v; refusing rather than guessing",
				from, to, releasedSchemas)
		}
		out = append(out, steps[found])
		cur = steps[found].To
		if visited[cur] {
			return nil, &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageManifest,
				Detail: fmt.Sprintf("cyclic migration chain revisits %s", cur)}
		}
		visited[cur] = true
	}
	return out, nil
}

// Report is the observable outcome surface shared by dry-run inspection
// and executed migration (issue acceptance: report source/target schema
// and dry-run validation). Render output is deterministic for equal
// reports.
type Report struct {
	SourcePath      string
	TargetPath      string
	SourceVersion   string
	TargetVersion   string
	Steps           []string // "<from>-><to>" entries in execution order
	Preservation    []string // U2/L32 findings; non-empty forces safe-write refusal on write-back
	ValidationOK    bool     // full read-side validation of the source succeeded
	AlreadyAtTarget bool     // precise idempotency: source already at target schema; no write performed or needed
}

// Render formats the report as deterministic text ending with the given
// action line. Diagnostics never embed untrusted paths as control text;
// they are data lines only.
func (r *Report) Render(action string) string {
	b := &strings.Builder{}
	fmt.Fprintf(b, "source: %s\n", r.SourcePath)
	fmt.Fprintf(b, "target: %s\n", r.TargetPath)
	fmt.Fprintf(b, "schema: %s -> %s\n", r.SourceVersion, r.TargetVersion)
	status := "ok"
	if !r.ValidationOK {
		status = "failed"
	}
	fmt.Fprintf(b, "validation: %s\n", status)
	fmt.Fprintf(b, "steps: %d", len(r.Steps))
	for _, s := range r.Steps {
		fmt.Fprintf(b, " %s", s)
	}
	b.WriteByte('\n')
	if len(r.Preservation) == 0 {
		b.WriteString("preservation: clean\n")
	} else {
		b.WriteString("preservation findings (write-back will be refused):\n")
		for _, f := range r.Preservation {
			fmt.Fprintf(b, "  - %s\n", f)
		}
	}
	fmt.Fprintf(b, "action: %s\n", action)
	return b.String()
}

// preservationFindings collects U2 bookkeeping from the read side.
// Entity-level unknown OPTIONAL fields do not appear here: the document
// codec preserves them opaquely across the round trip. Manifest,
// snapshot, and batch members cannot be re-serialized by the
// full-knowledge writer and therefore force refusal when present.
func preservationFindings(v *persist.PackageView) []string {
	var out []string
	for _, k := range v.ManifestUnknown {
		out = append(out, "manifest unknown member "+k+" cannot be carried into the rewritten package")
	}
	for _, k := range v.SnapshotUnknown {
		out = append(out, "snapshot unknown member "+k+" cannot be carried into the rewritten package")
	}
	names := make([]string, 0, len(v.BatchUnknown))
	for name := range v.BatchUnknown {
		names = append(names, name)
	}
	sort.Strings(names)
	for _, name := range names {
		members := append([]string(nil), v.BatchUnknown[name]...)
		sort.Strings(members)
		for _, k := range members {
			out = append(out, "batch "+name+" unknown member "+k+" cannot be carried into the rewritten batch document")
		}
	}
	return out
}

// newReport builds the observable report from an ALREADY-SELECTED plan.
//
// It refuses any plan that does not compose from the source label to
// emittedSchema: contiguously, starting where the document actually is,
// and ending at the label the writer will actually emit. A report whose
// step list ends anywhere else would describe a package that will not
// exist — which is the same class of false version claim issue 0031 was
// dispatched to remove, one layer up.
//
// The target version is always emittedSchema (§8.1 V1). It is never
// derived from the plan, so no plan can make the report disagree with the
// bytes that get written.
func newReport(v *persist.PackageView, sourcePath string, plan []Step) (*Report, *persist.Error) {
	refuse := func(format string, args ...any) *persist.Error {
		return &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageManifest,
			Detail: fmt.Sprintf(format, args...)}
	}
	cur := v.FormatVersion
	for i, s := range plan {
		if s.From != cur {
			return nil, refuse("migration plan step %d starts at %s but the document is at %s", i, s.From, cur)
		}
		cur = s.To
	}
	if cur != emittedSchema {
		return nil, refuse("migration plan ends at %s but this build emits %s", cur, emittedSchema)
	}
	rep := &Report{
		SourcePath:      sourcePath,
		SourceVersion:   v.FormatVersion,
		TargetVersion:   emittedSchema,
		ValidationOK:    true,
		AlreadyAtTarget: len(plan) == 0,
		Preservation:    preservationFindings(v),
	}
	for _, s := range plan {
		rep.Steps = append(rep.Steps, s.From+"->"+s.To)
	}
	return rep, nil
}

// planFor loads a source package and selects its migration plan. Both
// gates that can refuse a request live below it: the reader's own
// physical/namespace/schema checks, then selectChain's §8.2 V3 endpoint
// rule.
func planFor(source string) (*persist.PackageView, []Step, error) {
	view, lerr := persist.LoadFile(source)
	if lerr != nil {
		return nil, nil, lerr
	}
	plan, serr := selectChain(view.FormatVersion, emittedSchema, chain)
	if serr != nil {
		return nil, nil, serr
	}
	return view, plan, nil
}

// Options controls one Run invocation.
type Options struct {
	// Target names the migrated output path; empty migrates in place.
	Target string
	// DryRun inspects and reports without writing anything.
	DryRun bool
	// ConfirmOverwrite passes explicit user confirmation through the
	// BEHAVIOR-SPEC §9.2 external-modification gate.
	ConfirmOverwrite bool
	// Hooks injects deterministic faults into the S0–S5 machine.
	Hooks *persist.SaveHooks
}

// Inspect performs the dry-run surface: load the source through the
// full reader (physical, namespace, schema gates fail closed there),
// select the plan, and collect preservation findings. No write of any
// kind occurs.
func Inspect(source string) (*Report, error) {
	view, plan, err := planFor(source)
	if err != nil {
		return nil, err
	}
	rep, gerr := newReport(view, source, plan)
	if gerr != nil {
		return nil, gerr
	}
	return rep, nil
}

// namedTransform is one payload transform with a name used ONLY in
// diagnostics. It is never a Report field: executeMigration is deliberately
// unable to make a schema claim, so a synthetic transform cannot describe
// itself as a migration between schemas.
type namedTransform struct {
	name  string
	apply func(*payload) error
}

// transformsOf converts a selected plan into execution transforms, naming
// each after the schema pair it genuinely carries.
func transformsOf(plan []Step) []namedTransform {
	out := make([]namedTransform, 0, len(plan))
	for _, s := range plan {
		out = append(out, namedTransform{name: s.From + "->" + s.To, apply: s.Apply})
	}
	return out
}

// Run executes the migration: load, select the plan, report, then
// materialize the migrated package exclusively through
// persist.SaveDocument (§9.7 S0–S5). With the shipped empty chain every
// invocation is the detected no-op (AlreadyAtTarget): no temporary file,
// no write, no replacement ever occurs.
func Run(source string, opts Options) (*Report, *persist.SaveResult, error) {
	view, plan, err := planFor(source)
	if err != nil {
		return nil, nil, err
	}
	rep, gerr := newReport(view, source, plan)
	if gerr != nil {
		return nil, nil, gerr
	}
	target := opts.Target
	if target == "" {
		target = source
	}
	rep.TargetPath = target
	if opts.DryRun || rep.AlreadyAtTarget {
		return rep, nil, nil
	}
	res, xerr := executeMigration(view, target, transformsOf(plan), rep.Preservation, opts)
	return rep, res, xerr
}

// executeMigration applies transforms in order and writes the result
// through the §9.7 S0–S5 machine. It knows nothing about schema labels,
// constructs no Report, and therefore cannot state or imply a version.
//
// This is the seam the save/fault harness drives with synthetic
// transforms. Because it cannot produce a Report, a synthetic transform
// can never appear as a migration step in observable output.
//
// Transforms run strictly before any byte reaches disk; a failing
// transform aborts with zero effects.
func executeMigration(view *persist.PackageView, target string, transforms []namedTransform,
	refusals []string, opts Options) (*persist.SaveResult, error) {
	p := &payload{
		profile:     view.Profile,
		state:       view.Projection.State(),
		retained:    view.Retained,
		assetRoot:   view.AssetRoot,
		descriptors: view.DescriptorsRaw,
		blobs:       view.Blobs,
		preview:     view.Preview,
		refusals:    append([]string{}, refusals...),
	}
	for i := range transforms {
		if err := transforms[i].apply(p); err != nil {
			return nil, &persist.Error{Category: persist.CatSafeWriteRefusal, Stage: persist.StageSave,
				Detail: fmt.Sprintf("migration step %s aborted before any effect: %v", transforms[i].name, err)}
		}
	}

	bl, berr := persist.BaselineTarget(target)
	if berr != nil {
		return nil, berr
	}
	mode := os.FileMode(0o644)
	if fi, err := os.Stat(target); err == nil {
		mode = fi.Mode().Perm()
	}
	in := &persist.WriteInput{
		Profile:           p.profile,
		State:             p.state,
		RetainedBatches:   p.retained,
		AssetRoot:         p.assetRoot,
		DescriptorsRaw:    p.descriptors,
		Blobs:             p.blobs,
		PreviewEntries:    p.preview,
		RefusalReasons:    p.refusals,
		Baseline:          bl,
		BaselineConfirmed: opts.ConfirmOverwrite,
	}
	return persist.SaveDocument(target, mode, in, opts.Hooks)
}
