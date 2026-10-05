package corpusgen

import (
	"archive/zip"
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"strings"
	"testing"
)

const corpusRoot = "../../testdata/corpus"

// artifactCapBytes restates the named corpus safety parameter defined in
// generate.go (a corpus-local inertness bound, NOT a specification quantity;
// reviewer amendment 2). Declared once here via the generator constant so the
// gate and the generator cannot drift apart.
const artifactCapBytes = corpusArtifactCapBytes

var update = flag.Bool("update", false, "rewrite committed corpus artifacts from the deterministic generator")

// minimumExpectedRecords guards against silent manifest truncation; the
// bound tracks the authored record count, not any specification quantity.
const minimumExpectedRecords = 150

func TestMain(m *testing.M) {
	flag.Parse()
	os.Exit(m.Run())
}

// TestGenerateCorpusArtifacts regenerates every committed generated artifact
// and, without -update, asserts byte equality. Only .domo packages and the
// ratio sidecar are generator-produced; all other committed fixtures carry a
// recorded sha256 that TestExpectationsManifest recomputes on every run
// (narrowed claim per reviewer amendment HIGH-2).
func TestGenerateCorpusArtifacts(t *testing.T) {
	for _, a := range AllArtifacts() {
		path := filepath.Join(corpusRoot, filepath.FromSlash(a.RelPath))
		if *update {
			if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
				t.Fatal(err)
			}
			if err := os.WriteFile(path, a.Bytes, 0o644); err != nil {
				t.Fatal(err)
			}
			continue
		}
		committed, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("%s missing; run go test ./internal/corpusgen -update: %v", a.RelPath, err)
		}
		if !bytes.Equal(committed, a.Bytes) {
			t.Errorf("%s does not match deterministic regeneration", a.RelPath)
		}
	}
}

// TestGeneratedManifestsAreConforming pins the FS 7.4 manifest field SHAPES
// in every generated package, independently of whether the reader happens to
// tolerate a looser one. assetRoot is a REQUIRED ascending-by-digest array;
// null is not an array, and the reader's tolerance of null ("generators may
// serialize an absent table as null", internal/persist/wire.go) is parser
// laxity that cannot make a fixture conforming. Thin packages — including the
// golden/packages/thin-valid.domo success fixture and every hostile base
// derived from it — emitted null until this gate existed, which meant a
// SUCCESS oracle rested on non-conforming bytes and hostile bases carried a
// second latent defect alongside the one they meant to isolate.
func TestGeneratedManifestsAreConforming(t *testing.T) {
	checked := 0
	for _, a := range AllArtifacts() {
		if !strings.HasSuffix(a.RelPath, ".domo") {
			continue
		}
		zr, err := zip.NewReader(bytes.NewReader(a.Bytes), int64(len(a.Bytes)))
		if err != nil {
			continue // deliberately malformed container fixtures
		}
		for _, f := range zr.File {
			if f.Name != "domowik.json" {
				continue
			}
			rc, oerr := f.Open()
			if oerr != nil {
				continue // deliberately corrupted entry fixtures
			}
			raw, rerr := io.ReadAll(rc)
			rc.Close()
			if rerr != nil {
				continue
			}
			var m map[string]json.RawMessage
			if json.Unmarshal(raw, &m) != nil {
				continue // manifests corrupted on purpose
			}
			checked++
			root, present := m["assetRoot"]
			if !present {
				t.Errorf("%s: manifest omits the required assetRoot field (FS 7.4)", a.RelPath)
				continue
			}
			// encoding/json unmarshals a literal null into a slice
			// WITHOUT error, leaving it nil, so the raw token has to be
			// rejected explicitly. Checking only Unmarshal made this gate
			// vacuous, which its own mutation probe caught.
			if bytes.Equal(bytes.TrimSpace(root), []byte("null")) {
				t.Errorf("%s: manifest assetRoot is null, but FS 7.4 requires an ascending-by-digest ARRAY; an empty root serializes as []",
					a.RelPath)
				continue
			}
			var arr []json.RawMessage
			if json.Unmarshal(root, &arr) != nil {
				t.Errorf("%s: manifest assetRoot is %s, which is not a JSON array (FS 7.4)",
					a.RelPath, string(root))
			}
			if _, ok := m["entries"]; !ok {
				t.Errorf("%s: manifest omits the required entries field (FS 7.4)", a.RelPath)
			}
		}
	}
	if checked == 0 {
		t.Fatal("no generated manifest was inspected; the gate is vacuous")
	}
}

// TestCorpusArtifactSafety is the independent inert-safety gate: no committed
// corpus file may exceed the bounded-artifact cap.
func TestCorpusArtifactSafety(t *testing.T) {
	err := filepath.WalkDir(corpusRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		if info.Size() > artifactCapBytes {
			t.Errorf("%s exceeds %d bytes (%d): corpus artifacts must stay inert and bounded", path, artifactCapBytes, info.Size())
		}
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
}

// recipeInput names one file owned by a recipe, carrying its sha256 so input
// tampering is detected by recomputation on every run (BLOCKER-3).
type recipeInput struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
}

// boundary is the machine-checkable at-limit/one-over semantic of a limit
// probe: which §9.3 quantity is probed and what outcome each side of the
// exact boundary must produce.
type boundary struct {
	Quantity       string `json:"quantity"`
	Row            string `json:"row"`
	AtLimitOutcome string `json:"atLimitOutcome"`
	OneOverOutcome string `json:"oneOverOutcome"`
}

type recipe struct {
	Kind                string        `json:"kind"`
	DeclaredLogicalSize string        `json:"declaredLogicalSize,omitempty"` // explanatory expression over row names
	Materialization     string        `json:"materialization,omitempty"`     // none | bounded
	BoundRows           []string      `json:"boundRows,omitempty"`
	Boundaries          []boundary    `json:"boundaries,omitempty"`
	Inputs              []recipeInput `json:"inputs,omitempty"`

	CommittedPhysicalBytes *int `json:"committedPhysicalBytes,omitempty"`
}

func (r *expectation) BoundRows() []string {
	if r.Recipe == nil {
		return nil
	}
	return r.Recipe.BoundRows
}

// recipeBoundaries returns the recipe's structured boundaries, keeping audit
// code nil-safe.
func (r *expectation) recipeBoundaries() []boundary {
	if r.Recipe == nil {
		return nil
	}
	return r.Recipe.Boundaries
}

type expectation struct {
	ID                     string   `json:"id"`
	Class                  string   `json:"class"`
	Artifact               string   `json:"artifact,omitempty"`
	SHA256                 string   `json:"sha256,omitempty"`
	Recipe                 *recipe  `json:"recipe,omitempty"`
	Outcome                string   `json:"outcome"`
	Category               *string  `json:"category"`
	Diagnostic             string   `json:"diagnostic,omitempty"`
	Clauses                []string `json:"clauses"`
	ImplementationDeferred bool     `json:"implementationDeferred"`
	RawByDesign            bool     `json:"rawByDesign,omitempty"`
	Notes                  string   `json:"notes,omitempty"`

	// RowRefs maps an artifact-backed record to the FS 9.3 rows it probes
	// directly (byte-level evidence), complementing recipe boundRows.
	RowRefs []string `json:"rowRefs,omitempty"`

	// Assert carries the structured normative assertions for this fixture.
	// BLOCKER-6: assertions live in the MANIFEST, never inside wire fixture
	// bytes; selected fields are cross-checked against the artifact content.
	Assert map[string]interface{} `json:"assert,omitempty"`

	// ExecutionBlocked marks named assertions of an OTHERWISE EXECUTING
	// record as not-yet-provable, with the route that owns them. It is
	// deliberately neither a pass nor a skip: the harness still executes
	// the record and pins the divergence, while the coverage audit refuses
	// to count the record as evidence. EXPECTATIONS.md is the single
	// authority for this state — no harness-local map may restate it.
	ExecutionBlocked *executionBlocked `json:"executionBlocked,omitempty"`
}

// executionBlocked is validated by TestExpectationsManifest: the state must
// be a known token, every named assertion must exist in the record's assert
// block, and the route, reason and clauses must be present so the blocker is
// legible without reading a test.
type executionBlocked struct {
	State      string   `json:"state"`
	Assertions []string `json:"assertions"`
	RoutedTo   string   `json:"routedTo"`
	Clauses    []string `json:"clauses"`
	Citations  []string `json:"citations,omitempty"`
	Reason     string   `json:"reason"`

	// Observed records the CURRENT non-conformant value of each blocked
	// assertion, so the harness can pin the specific divergence instead of
	// merely asserting inequality with the oracle. Without it, any drift to
	// a third value would stay green.
	Observed map[string]interface{} `json:"observed,omitempty"`
}

// validBlockedStates is the closed vocabulary of non-executing states. Each
// is distinct from both "passed" and "skipped": the record runs, and what is
// withheld is a named assertion, not the whole record.
var validBlockedStates = map[string]bool{
	"blocked-implementation-nonconformance": true,
	"blocked-open-question":                 true,
}

var validClasses = map[string]bool{
	"golden": true, "adversarial": true, "migration": true,
	"hostile": true, "declarative": true,
}

var validOutcomes = map[string]bool{
	"success":                       true,
	"reject":                        true,
	"warning":                       true,
	"degraded-read-only-inspection": true,
	"missing-asset-equivalence":     true,
	"safe-write-refusal":            true,
	"blocked-open-question":         true,
}

// ratifiedCategoryTokens is the closed token set from FORMAT-SPEC §9.4 plus
// the §9.3 error-category column. Proposed-but-unratified diagnostics are
// deliberately absent.
var ratifiedCategoryTokens = map[string]bool{
	"malformed-container": true, "malformed-length": true,
	"unsupported-method": true, "unsupported-extension": true,
	"unsupported-version": true, "encrypted-entry": true,
	"checksum-error": true, "trailing-data": true, "overlapping-regions": true,
	"path-traversal": true, "invalid-entry-name": true,
	"duplicate-logical-name": true, "ambiguous-manifest": true,
	"missing-manifest": true, "nonfinite-value": true, "numeric-overflow": true,
	"malformed-framing":    true,
	"invalid-field-syntax": true, "invalid-reference": true,
	"limit-total-expanded": true, "limit-total-physical": true,
	"limit-entry-expanded": true, "limit-entry-physical": true,
	"limit-blob-bytes": true, "limit-entry-count": true,
	"limit-manifest-reserve": true, "limit-archive-depth": true,
	"limit-nesting-depth": true, "limit-number-token": true,
	"limit-array-count": true, "limit-member-count": true,
	"limit-image-dimensions": true, "limit-model-complexity": true,
	"limit-chunk-count": true, "limit-name-length": true,
	"limit-name-component": true, "limit-expansion-ratio": true,
	"limit-aggregate-expansion": true, "limit-operation-count": true,
	"limit-snapshot-metadata": true, "limit-transfer-bytes": true,
	"limit-transfer-time": true, "limit-redirect-hops": true,
	"limit-hints-session": true, "limit-concurrent-retrieval": true,
}

var validRecipeKinds = map[string]bool{
	"limit-probe": true, "batch-pair": true, "policy-vector": true,
	"pure-function-vector": true, "fault-vector": true,
	"precedence-vector": true, "blocked-obligation": true,
}

var limitRowRe = regexp.MustCompile(`^L([1-9]|[12][0-9]|3[0-3])$`)

// extractFencedJSON returns every fenced ```json block body in a Markdown file.
func extractFencedJSON(t *testing.T, path string) [][]byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var blocks [][]byte
	lines := strings.Split(string(data), "\n")
	in := false
	var buf []string
	for _, line := range lines {
		switch {
		case !in && strings.HasPrefix(strings.TrimSpace(line), "```json"):
			in, buf = true, nil
		case in && strings.TrimSpace(line) == "```":
			blocks = append(blocks, []byte(strings.Join(buf, "\n")))
			in = false
		case in:
			buf = append(buf, line)
		}
	}
	if in {
		t.Fatalf("%s: unterminated json fence", path)
	}
	return blocks
}

func loadExpectations(t *testing.T) []expectation {
	t.Helper()
	blocks := extractFencedJSON(t, filepath.Join(corpusRoot, "EXPECTATIONS.md"))
	var all []expectation
	for i, b := range blocks {
		var probe interface{}
		if err := json.Unmarshal(b, &probe); err != nil {
			t.Fatalf("EXPECTATIONS.md fence %d: %v", i, err)
		}
		if _, isObj := probe.(map[string]interface{}); isObj {
			continue // coverage-audit object fence, not a record array
		}
		var batch []expectation
		if err := json.Unmarshal(b, &batch); err != nil {
			t.Fatalf("EXPECTATIONS.md fence %d: %v", i, err)
		}
		all = append(all, batch...)
	}
	return all
}

type auditEntry struct {
	Token     string   `json:"token"`
	Row       string   `json:"row"`
	Status    string   `json:"status"`
	CoveredBy []string `json:"coveredBy"`
	GapRecord string   `json:"gapRecord,omitempty"`
	Note      string   `json:"note,omitempty"`
}

type coverageAudit struct {
	CategoryAudit    []auditEntry      `json:"categoryAudit"`
	RowAudit         []auditEntry      `json:"rowAudit"`
	ResidualBlockers []residualBlocker `json:"residualBlockers"`
}

// residualBlocker is the reader-facing half of the honest-coverage contract:
// the manifest states, machine-readably, which records are NOT executed
// evidence and who owns them, so executed coverage can be told from residual
// blockers without opening a test file.
type residualBlocker struct {
	Record            string   `json:"record"`
	State             string   `json:"state"`
	BlockedAssertions []string `json:"blockedAssertions"`
	RoutedTo          string   `json:"routedTo"`
}

// executionBlockedViolations validates a record's executionBlocked marker;
// extracted so the negative invariants can prove the rules bite.
func executionBlockedViolations(r expectation) []string {
	eb := r.ExecutionBlocked
	if eb == nil {
		return nil
	}
	var errs []string
	if !validBlockedStates[eb.State] {
		errs = append(errs, fmt.Sprintf("executionBlocked.state %q is not a known blocked state", eb.State))
	}
	if len(eb.Assertions) == 0 {
		errs = append(errs, "executionBlocked must name at least one blocked assertion")
	}
	for _, name := range eb.Assertions {
		if _, ok := r.Assert[name]; !ok {
			errs = append(errs, fmt.Sprintf(
				"executionBlocked names assertion %q, which is absent from the record's assert block", name))
		}
	}
	if eb.RoutedTo == "" {
		errs = append(errs, "executionBlocked must name the route that owns it")
	}
	if eb.Reason == "" {
		errs = append(errs, "executionBlocked must state its reason")
	}
	if len(eb.Clauses) == 0 {
		errs = append(errs, "executionBlocked must cite the normative clauses in tension")
	}
	// Every blocked assertion must record what is observed instead, and the
	// observation must genuinely differ from the oracle: an "observed" value
	// equal to the oracle would mean the record is not blocked at all.
	blocked := map[string]bool{}
	for _, name := range eb.Assertions {
		blocked[name] = true
		if _, ok := eb.Observed[name]; !ok {
			errs = append(errs, fmt.Sprintf(
				"executionBlocked names assertion %q but records no observed value for it", name))
		}
	}
	for name, got := range eb.Observed {
		if !blocked[name] {
			errs = append(errs, fmt.Sprintf(
				"executionBlocked records an observed value for %q, which it does not list as blocked", name))
			continue
		}
		if want, ok := r.Assert[name]; ok && reflect.DeepEqual(got, want) {
			errs = append(errs, fmt.Sprintf(
				"executionBlocked observed value for %q equals the oracle (%v): the assertion is not blocked", name, want))
		}
	}
	return errs
}

// coveredCategoryViolations validates one "covered" category entry against
// the record index; extracted for negative invariant testing.
func coveredCategoryViolations(e auditEntry, recIDs map[string]bool, catOfRec map[string]string) []string {
	return coveredCategoryViolationsBlocked(e, recIDs, catOfRec, nil)
}

// coveredCategoryViolationsBlocked additionally refuses to let a "covered"
// claim rest on records whose assertions are blocked. A blocked record may
// appear alongside real evidence, but never as the whole of it: that is
// exactly the substitution issue 0018's audit condemned, where
// missing-manifest was recorded as covered by a single record that skipped.
func coveredCategoryViolationsBlocked(e auditEntry, recIDs map[string]bool, catOfRec map[string]string, blocked map[string]bool) []string {
	var errs []string
	if len(e.CoveredBy) == 0 {
		errs = append(errs, "covered without records")
	}
	executing := 0
	for _, id := range e.CoveredBy {
		if !recIDs[id] {
			errs = append(errs, fmt.Sprintf("covers unknown record %s", id))
			continue
		}
		if catOfRec[id] != e.Token {
			errs = append(errs, fmt.Sprintf("record %s has category %q", id, catOfRec[id]))
		}
		if !blocked[id] {
			executing++
		}
	}
	if len(e.CoveredBy) > 0 && executing == 0 {
		errs = append(errs, "covered only by records whose assertions are blocked: that is not executed coverage")
	}
	return errs
}

// coveredRowViolations validates one "covered" row entry against the record
// row mapping; extracted for negative invariant testing.
func coveredRowViolations(e auditEntry, recIDs map[string]bool, recRows map[string]map[string]bool) []string {
	return coveredRowViolationsBlocked(e, recIDs, recRows, nil)
}

func coveredRowViolationsBlocked(e auditEntry, recIDs map[string]bool, recRows map[string]map[string]bool, blocked map[string]bool) []string {
	var errs []string
	if len(e.CoveredBy) == 0 {
		errs = append(errs, "covered without records")
	}
	executing := 0
	for _, id := range e.CoveredBy {
		if !recIDs[id] {
			errs = append(errs, fmt.Sprintf("covers unknown record %s", id))
			continue
		}
		if !recRows[id][e.Row] {
			errs = append(errs, fmt.Sprintf("record %s does not map row %s", id, e.Row))
		}
		if !blocked[id] {
			executing++
		}
	}
	if len(e.CoveredBy) > 0 && executing == 0 {
		errs = append(errs, "covered only by records whose assertions are blocked: that is not executed coverage")
	}
	return errs
}

// auditedTokenSetViolations reports ratified categories missing from or
// duplicated in the audit.
func auditedTokenSetViolations(entries []auditEntry) []string {
	var errs []string
	seen := map[string]bool{}
	for _, e := range entries {
		if seen[e.Token] {
			errs = append(errs, fmt.Sprintf("duplicate audit entry %s", e.Token))
		}
		seen[e.Token] = true
		if !ratifiedCategoryTokens[e.Token] {
			errs = append(errs, fmt.Sprintf("token %q not in ratified set", e.Token))
		}
	}
	for tok := range ratifiedCategoryTokens {
		if !seen[tok] {
			errs = append(errs, fmt.Sprintf("ratified category %q missing from audit", tok))
		}
	}
	return errs
}

// recMapsRow reports whether a record maps a §9.3 row through recipe
// boundRows or structured boundaries.
func recMapsRow(recID, row string, recRows map[string]map[string]bool) bool {
	return recRows[recID][row]
}

// TestCoverageAudit enforces the honest-coverage contract (final review):
// every ratified category token and every FS 9.3 row appears exactly once;
// covered entries point at real records carrying that category/row; blocked
// entries point at an existing blocked-open-question gap record. The audit's
// existence with blocked entries means full taxonomy coverage is NOT claimed.
func TestCoverageAudit(t *testing.T) {
	blocks := extractFencedJSON(t, filepath.Join(corpusRoot, "EXPECTATIONS.md"))
	var audit coverageAudit
	found := false
	recIDs := map[string]bool{}
	catOfRec := map[string]string{} // record id -> its category (if any)
	for i, b := range blocks {
		var probe map[string]interface{}
		if err := json.Unmarshal(b, &probe); err != nil {
			continue
		}
		if _, hasCats := probe["categoryAudit"]; !hasCats {
			continue
		}
		if found {
			t.Fatal("multiple audit fences")
		}
		found = true
		raw, _ := json.Marshal(probe)
		if err := json.Unmarshal(raw, &audit); err != nil {
			t.Fatalf("audit fence %d: %v", i, err)
		}
	}
	if !found {
		t.Fatal("coverage audit fence missing")
	}
	recRows := map[string]map[string]bool{}
	gapIDs := map[string]bool{}
	blockedIDs := map[string]bool{}
	blockedState := map[string]string{}
	blockedAsserts := map[string][]string{}
	blockedRoute := map[string]string{}
	for _, b := range loadExpectations(t) {
		recIDs[b.ID] = true
		if b.Category != nil {
			catOfRec[b.ID] = *b.Category
		}
		if b.ExecutionBlocked != nil {
			blockedIDs[b.ID] = true
			blockedState[b.ID] = b.ExecutionBlocked.State
			blockedAsserts[b.ID] = b.ExecutionBlocked.Assertions
			blockedRoute[b.ID] = b.ExecutionBlocked.RoutedTo
		}
		rows := map[string]bool{}
		for _, row := range append(b.BoundRows(), b.RowRefs...) {
			rows[row] = true
		}
		for _, bd := range b.recipeBoundaries() {
			rows[bd.Row] = true
		}
		recRows[b.ID] = rows
		if b.Outcome == "blocked-open-question" {
			gapIDs[b.ID] = true
			blockedIDs[b.ID] = true
			if blockedState[b.ID] == "" {
				blockedState[b.ID] = "blocked-open-question"
			}
		}
	}

	seenTokens := map[string]bool{}
	blockedCats := 0
	for _, e := range audit.CategoryAudit {
		seenTokens[e.Token] = true
		if !ratifiedCategoryTokens[e.Token] {
			t.Errorf("audit token %q not in ratified set", e.Token)
		}
		switch e.Status {
		case "covered":
			for _, msg := range coveredCategoryViolationsBlocked(e, recIDs, catOfRec, blockedIDs) {
				t.Errorf("category %s %s", e.Token, msg)
			}
		case "blocked-upstream-spec", "no-reject-trigger-by-design":
			blockedCats++
			if len(e.CoveredBy) == 0 && len(e.Note) == 0 && e.GapRecord == "" {
				t.Errorf("blocked category %s needs a gapRecord or explanatory note", e.Token)
			}
			if e.GapRecord != "" && !gapIDs[e.GapRecord] {
				t.Errorf("category %s gapRecord %s missing or not blocked-open-question", e.Token, e.GapRecord)
			}
		default:
			t.Errorf("category %s unknown status %q", e.Token, e.Status)
		}
	}
	for _, msg := range auditedTokenSetViolations(audit.CategoryAudit) {
		t.Errorf("%s", msg)
	}
	_ = seenTokens
	if blockedCats == 0 {
		t.Errorf("audit claims complete taxonomy coverage; blocked entries must be present while gaps exist")
	}

	seenRows := map[string]bool{}
	for _, e := range audit.RowAudit {
		if seenRows[e.Row] {
			t.Errorf("duplicate row audit entry %s", e.Row)
		}
		seenRows[e.Row] = true
		if !limitRowRe.MatchString(e.Row) {
			t.Errorf("row audit entry %q invalid", e.Row)
		}
		switch e.Status {
		case "covered":
			for _, msg := range coveredRowViolationsBlocked(e, recIDs, recRows, blockedIDs) {
				t.Errorf("row %s %s", e.Row, msg)
			}
		case "blocked-owner-deferred", "blocked-upstream-spec":
			if !gapIDs[e.GapRecord] {
				t.Errorf("row %s gapRecord %s missing or not blocked-open-question", e.Row, e.GapRecord)
			}
		default:
			t.Errorf("row %s unknown status %q", e.Row, e.Status)
		}
	}
	for n := 1; n <= 33; n++ {
		if !seenRows[fmt.Sprintf("L%d", n)] {
			t.Errorf("row L%d missing from audit", n)
		}
	}

	// residualBlockers is the reader-facing statement of what is NOT
	// executed evidence. It must match the blocked record set exactly in
	// both directions, so the manifest cannot claim coverage the harness
	// withholds, nor withhold coverage the manifest does not disclose.
	listed := map[string]bool{}
	for _, rb := range audit.ResidualBlockers {
		if !recIDs[rb.Record] {
			t.Errorf("residualBlockers names unknown record %s", rb.Record)
			continue
		}
		if listed[rb.Record] {
			t.Errorf("residualBlockers lists %s twice", rb.Record)
		}
		listed[rb.Record] = true
		if !blockedIDs[rb.Record] {
			t.Errorf("residualBlockers lists %s, which carries neither executionBlocked nor a blocked-open-question outcome",
				rb.Record)
			continue
		}
		if want := blockedState[rb.Record]; rb.State != want {
			t.Errorf("residualBlockers %s: state %q disagrees with the record's %q", rb.Record, rb.State, want)
		}
		if got, want := rb.BlockedAssertions, blockedAsserts[rb.Record]; !reflect.DeepEqual(got, want) &&
			!(len(got) == 0 && len(want) == 0) {
			t.Errorf("residualBlockers %s: blockedAssertions %v disagree with the record's %v", rb.Record, got, want)
		}
		if route := blockedRoute[rb.Record]; route != "" && rb.RoutedTo != route {
			t.Errorf("residualBlockers %s: routedTo %q disagrees with the record's %q", rb.Record, rb.RoutedTo, route)
		}
		if rb.RoutedTo == "" {
			t.Errorf("residualBlockers %s: must name the route that owns it", rb.Record)
		}
	}
	for id := range blockedIDs {
		if !listed[id] {
			t.Errorf("record %s is blocked but absent from residualBlockers: a reader cannot tell executed coverage from residual blockers",
				id)
		}
	}
	if len(audit.ResidualBlockers) == 0 {
		t.Error("residualBlockers is empty while blocked records exist; full coverage must not be implied")
	}
}

func fixtureFiles(t *testing.T) map[string]bool {
	t.Helper()
	out := map[string]bool{}
	skip := map[string]bool{"README.md": true, "EXPECTATIONS.md": true}
	err := filepath.WalkDir(corpusRoot, func(path string, d fs.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return err
		}
		rel, _ := filepath.Rel(corpusRoot, path)
		rel = filepath.ToSlash(rel)
		top := strings.SplitN(rel, "/", 2)[0]
		if skip[rel] || rel == "parameters.json" {
			return nil
		}
		if top != "golden" && top != "adversarial" && top != "migration" && top != "hostile" {
			return nil
		}
		out[rel] = true
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	return out
}

// canonicalOutcomeEnum is the closed machine vocabulary for projection
// outcomes asserted in manifest records.
var canonicalOutcomeEnum = map[string]bool{
	"apply": true, "hold-then-apply": true,
	"discard-stale": true, "ignore-duplicate": true,
}

var uuid36Re = regexp.MustCompile(`^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`)

// batchStemRe is the §7.6 entry stem: counterBase zero-padded to 19 digits
// (the signed-64-bit ceiling L13 makes 19 total-order-preserving under byte
// comparison) followed by the canonical actorId. The padding is load-bearing,
// so a short form is a different name rather than a normalizable one.
var batchStemRe = regexp.MustCompile(`^[0-9]{19}-[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$`)

// checkTypedAssert validates the known typed assertion fields whenever they
// are present. Conditional-on-presence is deliberate and documented: the
// construct-specific REQUIREMENTS are enforced elsewhere (canonicalOutcome
// discard-stale => discardedBatchIds; warnings => warning outcome;
// limit-probe => structured boundaries), never by prose.
func checkTypedAssert(rec expectation, raw map[string]interface{}) []string {
	var errs []string
	fail := func(format string, args ...interface{}) { errs = append(errs, fmt.Sprintf(format, args...)) }
	if co, ok := raw["canonicalOutcome"]; ok {
		str, isStr := co.(string)
		if !isStr || !canonicalOutcomeEnum[str] {
			fail("assert.canonicalOutcome %v outside closed enum", co)
		}
		if str == "discard-stale" {
			ids, ok := raw["discardedBatchIds"].([]interface{})
			if !ok || len(ids) == 0 {
				fail("canonicalOutcome discard-stale requires non-empty discardedBatchIds")
			}
		}
	}
	if w, ok := raw["winner"]; ok {
		m, isObj := w.(map[string]interface{})
		if !isObj {
			fail("assert.winner must be an object")
		} else {
			// The winner is a POSITION: a reference into the snapshot's
			// batchPositions table (a §7.6 entry stem) plus the operation's
			// index within that batch (FS §4.6 LWW1, §5.1 S3). The
			// pre-ED-0037 (operationCounter, actorId) pair cannot place a
			// write against a later concurrent one and is no longer a valid
			// shape here.
			fld, _ := m["field"].(string)
			batch, _ := m["batch"].(string)
			idx, idxOK := m["operationIndex"].(float64)
			actor, _ := m["actorId"].(string)
			if fld == "" || !batchStemRe.MatchString(batch) || !idxOK || int(idx) < 0 ||
				!uuid36Re.MatchString(actor) {
				fail("assert.winner shape invalid (field/batch/operationIndex/actorId)")
			}
		}
	}
	if tr, ok := raw["tombstoneRetained"]; ok {
		if _, isBool := tr.(bool); !isBool {
			fail("assert.tombstoneRetained must be boolean")
		}
	}
	if dp, ok := raw["deletingPositionBatch"]; ok {
		if stem, isStr := dp.(string); !isStr || !batchStemRe.MatchString(stem) {
			fail("assert.deletingPositionBatch must be a §7.6 entry stem")
		}
	}
	for _, name := range []string{"winnerPositionsPresent", "createdPositionsPresent", "batchPositionsPresent"} {
		if v, ok := raw[name]; ok {
			if _, isBool := v.(bool); !isBool {
				fail("assert.%s must be boolean", name)
			}
		}
	}
	if ids, ok := raw["discardedBatchIds"]; ok && ids != nil {
		arr, isArr := ids.([]interface{})
		if !isArr {
			fail("assert.discardedBatchIds must be an array")
		} else {
			for _, v := range arr {
				sid, _ := v.(string)
				if len(sid) != 36 {
					fail("discardedBatchIds entries must be 36-character batch identifiers")
					break
				}
			}
		}
	}
	if w, ok := raw["warnings"]; ok {
		n, isNum := w.(float64)
		if !isNum || n < 1 || n != float64(int(n)) {
			fail("assert.warnings must be a positive integer")
		} else if rec.Outcome != "warning" {
			fail("assert.warnings requires record outcome warning")
		}
	}
	if c, ok := raw["commits"]; ok {
		if b, isBool := c.(bool); !isBool {
			fail("assert.commits must be boolean")
		} else if b && rec.Outcome == "reject" {
			fail("assert.commits=true contradicts reject outcome")
		}
	}
	if pf, ok := raw["preservedFields"]; ok {
		arr, isArr := pf.([]interface{})
		if !isArr || len(arr) == 0 {
			fail("assert.preservedFields must be a non-empty array")
		}
	}
	if wb, ok := raw["writeBackContract"]; ok {
		str, isStr := wb.(string)
		if !isStr || (str != "preserve" && str != "safe-write-refusal") {
			fail("assert.writeBackContract must be preserve|safe-write-refusal")
		}
	}
	return errs
}

// crossCheckAssertBytes ties selected manifest assertions to the actual bytes
// under test (BLOCKER-6): assertions live in the manifest, but these fields
// are verified against the artifact document itself.
func crossCheckAssertBytes(rec expectation, raw map[string]interface{}) []string {
	var errs []string
	fail := func(format string, args ...interface{}) { errs = append(errs, fmt.Sprintf(format, args...)) }
	data, err := os.ReadFile(filepath.Join(corpusRoot, filepath.FromSlash(rec.Artifact)))
	if err != nil {
		fail("cannot read artifact for cross-check: %v", err)
		return errs
	}
	var doc map[string]interface{}
	if err := json.Unmarshal(data, &doc); err != nil {
		fail("artifact %s does not parse as a JSON object during cross-check", rec.Artifact)
		return errs
	}
	if ek, ok := raw["entityKind"].(string); ok {
		if got, _ := doc["type"].(string); got != ek {
			fail("assert.entityKind %q does not match artifact type %q", ek, got)
		}
	}
	if cb, ok := raw["counterBase"].(float64); ok {
		got, _ := doc["counterBase"].(float64)
		if got != cb {
			fail("assert.counterBase %v does not match artifact counterBase %v", cb, got)
		}
	}
	if dc, ok := raw["descriptorCount"].(float64); ok {
		if len(doc) != int(dc) {
			fail("assert.descriptorCount %d does not match artifact member count %d", int(dc), len(doc))
		}
	}
	if ec, ok := raw["entityCount"].(float64); ok {
		ents, _ := doc["entities"].(map[string]interface{})
		if ents == nil || len(ents) != int(ec) {
			fail("assert.entityCount %d does not match artifact entities %v", int(ec), doc["entities"])
		}
	}
	return errs
}

// sidecarRelPath is the ONLY generator-produced metadata sidecar in the
// corpus; the sidecar exemption in assertChecks matches this exact path,
// not a suffix pattern (BLOCKER-8).
const sidecarRelPath = "hostile/container/ratio-under-L15-success-twin.meta.json"

// assertChecks returns the violation messages for one record's assert
// contract; an empty slice means compliant. Invariants are pinned by
// TestValidateAssertInvariants.
func assertChecks(rec expectation) []string {
	var errs []string
	fail := func(format string, args ...interface{}) { errs = append(errs, fmt.Sprintf(format, args...)) }
	if !strings.HasSuffix(rec.Artifact, ".json") {
		return nil // binary fixtures carry no assert contract
	}
	if rec.Artifact == sidecarRelPath {
		if rec.Assert != nil {
			fail("sidecar records must not carry assert objects")
		}
		return errs
	}
	data, err := os.ReadFile(filepath.Join(corpusRoot, filepath.FromSlash(rec.Artifact)))
	if err != nil {
		fail("cannot read artifact for assert validation: %v", err)
		return errs
	}
	var doc map[string]interface{}
	parseErr := json.Unmarshal(data, &doc)

	if rec.RawByDesign {
		// BLOCKER-8: the flag must be TRUE of the bytes — the document must
		// genuinely be non-JSON or a non-object top level.
		if parseErr == nil {
			fail("%s is marked rawByDesign but parses as a strict JSON object", rec.Artifact)
		}
		if rec.Assert != nil {
			fail("rawByDesign records must not carry assert objects")
		}
		return errs
	}
	if parseErr != nil {
		fail("%s does not parse as a JSON object but is not marked rawByDesign", rec.Artifact)
		return errs
	}
	// BLOCKER-7: a normal parseable JSON artifact MUST carry its manifest
	// assert; omission is no longer tolerated.
	if rec.Assert == nil {
		fail("%s lacks its required manifest assert object", rec.Artifact)
		return errs
	}
	raw := rec.Assert
	outcome, ok := raw["outcome"].(string)
	if !ok {
		fail("assert must carry a string outcome machine assertion")
		return errs
	}
	if !validOutcomes[outcome] && outcome != "success-on-read" {
		fail("assert.outcome %q outside vocabulary", outcome)
	}
	switch rec.Outcome {
	case "success":
		if outcome != "success" && outcome != "success-on-read" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	case "reject":
		if outcome != "reject" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	case "warning":
		if outcome != "warning" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	case "degraded-read-only-inspection":
		if outcome != "degraded-read-only-inspection" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	case "missing-asset-equivalence":
		if outcome != "missing-asset-equivalence" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	case "safe-write-refusal":
		if outcome != "safe-write-refusal" {
			fail("assert.outcome %q contradicts record outcome %q", outcome, rec.Outcome)
		}
	}
	errs = append(errs, checkTypedAssert(rec, raw)...)
	errs = append(errs, crossCheckAssertBytes(rec, raw)...)
	return errs
}

// TestExpectationsManifest enforces the machine-readable contract: vocabulary,
// uniqueness, clause citations, artifact digest recomputation, recipe shape
// and bound rows, category tokens, structured asserts, recipe-input validity,
// and full two-way coverage between this file and the committed tree.
func TestExpectationsManifest(t *testing.T) {
	recs := loadExpectations(t)
	if len(recs) < minimumExpectedRecords {
		t.Fatalf("suspiciously small manifest: %d records", len(recs))
	}
	seen := map[string]bool{}
	indexed := map[string]bool{}

	for _, r := range recs {
		if r.ID == "" {
			t.Fatal("record with empty id")
		}
		if seen[r.ID] {
			t.Errorf("duplicate id %s", r.ID)
		}
		seen[r.ID] = true
		if !validClasses[r.Class] {
			t.Errorf("%s: bad class %q", r.ID, r.Class)
		}
		if !validOutcomes[r.Outcome] {
			t.Errorf("%s: bad outcome %q", r.ID, r.Outcome)
		}
		if len(r.Clauses) == 0 {
			t.Errorf("%s: missing clauses", r.ID)
		}
		for _, row := range r.RowRefs {
			if !limitRowRe.MatchString(row) {
				t.Errorf("%s: rowRefs entry %q invalid", r.ID, row)
			}
		}
		// Reject records in executable scope must name their §9.3/§9.4 class;
		// declarative vectors may leave category null when no ratified class
		// exists (MED-8), but any present category must be a ratified token.
		if r.Category != nil {
			if !ratifiedCategoryTokens[*r.Category] {
				t.Errorf("%s: category %q is not a ratified §9.3/§9.4 token", r.ID, *r.Category)
			}
		} else if r.Outcome == "reject" && r.Class != "declarative" {
			t.Errorf("%s: reject record must carry a category token", r.ID)
		}

		for _, msg := range executionBlockedViolations(r) {
			t.Errorf("%s: %s", r.ID, msg)
		}

		hasArtifact := r.Artifact != ""
		hasRecipe := r.Recipe != nil
		if !hasArtifact && !hasRecipe {
			t.Errorf("%s: needs an artifact or a recipe", r.ID)
		}

		if hasArtifact {
			p := filepath.Join(corpusRoot, filepath.FromSlash(r.Artifact))
			info, err := os.Stat(p)
			if err != nil {
				t.Errorf("%s: artifact %s missing", r.ID, r.Artifact)
			} else {
				if info.Size() > artifactCapBytes {
					t.Errorf("%s: artifact exceeds cap", r.ID)
				}
				// HIGH-2: digests are RECOMPUTED from the referenced bytes on
				// every run, never trusted from a sidecar.
				if r.SHA256 == "" {
					t.Errorf("%s: artifact record missing sha256", r.ID)
				} else {
					body, err := os.ReadFile(p)
					if err != nil {
						t.Fatal(err)
					}
					sum := sha256.Sum256(body)
					if got := hex.EncodeToString(sum[:]); got != r.SHA256 {
						t.Errorf("%s: sha256 mismatch for %s: manifest %s vs actual %s", r.ID, r.Artifact, r.SHA256, got)
					}
				}
			}
			if indexed[r.Artifact] {
				t.Errorf("artifact %s indexed more than once", r.Artifact)
			}
			indexed[r.Artifact] = true
			for _, msg := range assertChecks(r) {
				t.Errorf("%s: %s", r.ID, msg)
			}
		}

		if hasRecipe {
			rc := r.Recipe
			if !validRecipeKinds[rc.Kind] {
				t.Errorf("%s: recipe kind %q outside whitelist", r.ID, rc.Kind)
			}
			if rc.Materialization != "none" && rc.Materialization != "bounded" {
				t.Errorf("%s: recipe materialization must be none|bounded", r.ID)
			}
			if rc.CommittedPhysicalBytes != nil && *rc.CommittedPhysicalBytes > artifactCapBytes {
				t.Errorf("%s: recipe commits oversized bytes", r.ID)
			}
			for _, row := range rc.BoundRows {
				if !limitRowRe.MatchString(row) {
					t.Errorf("%s: boundRows entry %q is not a ratified §9.3 row ID (L1–L32)", r.ID, row)
				}
			}
			if rc.Kind == "limit-probe" {
				if len(rc.BoundRows) == 0 {
					t.Errorf("%s: limit-probe recipe needs at least one boundRows entry", r.ID)
				}
				if rc.DeclaredLogicalSize == "" {
					t.Errorf("%s: limit-probe recipe needs a declaredLogicalSize expression over §9.3 row names", r.ID)
				}
			}
			// HIGH-3/BLOCKER-3: recipe inputs must exist, carry a sha256 that
			// is RECOMPUTED from the bytes on every run (tamper detection), be
			// covered exclusively by THIS record, and satisfy kind-specific
			// structural checks.
			for _, in := range rc.Inputs {
				ip := filepath.Join(corpusRoot, filepath.FromSlash(in.Path))
				body, err := os.ReadFile(ip)
				if err != nil {
					t.Errorf("%s: recipe input %s missing", r.ID, in.Path)
					continue
				}
				sum := sha256.Sum256(body)
				if got := hex.EncodeToString(sum[:]); got != in.SHA256 {
					t.Errorf("%s: recipe input %s sha256 mismatch (manifest %s vs actual %s)", r.ID, in.Path, in.SHA256, got)
				}
				if indexed[in.Path] {
					t.Errorf("input %s covered both by %s and its own record", in.Path, r.ID)
				}
				indexed[in.Path] = true
			}
			for _, b := range rc.Boundaries {
				if !limitRowRe.MatchString(b.Row) {
					t.Errorf("%s: boundary row %q is not a ratified §9.3 row ID", r.ID, b.Row)
				}
				for _, oc := range []string{b.AtLimitOutcome, b.OneOverOutcome} {
					if !validOutcomes[oc] && oc != "success-on-read" {
						t.Errorf("%s: boundary outcome %q outside vocabulary", r.ID, oc)
					}
				}
				if b.Quantity == "" {
					t.Errorf("%s: boundary requires a quantity token", r.ID)
				}
			}
			if rc.Kind == "limit-probe" && len(rc.Boundaries) == 0 {
				t.Errorf("%s: limit-probe requires at least one structured boundary (BLOCKER-3)", r.ID)
			}
			if rc.Kind == "batch-pair" {
				if len(rc.Inputs) != 2 {
					t.Errorf("%s: batch-pair requires exactly two inputs", r.ID)
					continue
				}
				var ids []string
				for _, in := range rc.Inputs {
					body, err := os.ReadFile(filepath.Join(corpusRoot, filepath.FromSlash(in.Path)))
					if err != nil {
						t.Fatal(err)
					}
					var doc struct {
						BatchID string `json:"batchId"`
					}
					if err := json.Unmarshal(body, &doc); err != nil {
						t.Errorf("%s: batch-pair input %s does not parse as a batch document", r.ID, in)
						continue
					}
					ids = append(ids, doc.BatchID)
				}
				if len(ids) == 2 && (ids[0] == "" || ids[0] != ids[1]) {
					t.Errorf("%s: batch-pair documents do not share one embedded batchId (%q vs %q)", r.ID, ids[0], ids[1])
				}
			}
		}

		if r.Outcome == "blocked-open-question" && !r.ImplementationDeferred {
			t.Errorf("%s: blocked-open-question must be marked implementationDeferred", r.ID)
		}
		if r.Class == "declarative" && !r.ImplementationDeferred {
			t.Errorf("%s: declarative records must be marked implementationDeferred", r.ID)
		}
	}

	files := fixtureFiles(t)
	for f := range files {
		if !indexed[f] {
			t.Errorf("fixture %s present but not indexed in EXPECTATIONS.md", f)
		}
	}
	for f := range indexed {
		if !files[f] {
			t.Errorf("EXPECTATIONS.md indexes %s but no such fixture exists", f)
		}
	}
}

// TestRatioTwinMeta recomputes the measured expansion of the genuine stream
// twin deterministically (MED-9): deflate of microBombLogicalBytes zero bytes
// must reproduce the committed sidecar numbers exactly and stay below L15.
func TestRatioTwinMeta(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join(corpusRoot,
		"hostile/container/ratio-under-L15-success-twin.meta.json"))
	if err != nil {
		t.Fatal(err)
	}
	var meta struct {
		LogicalBytes int64 `json:"logicalBytes"`
		StoredBytes  int64 `json:"storedBytes"`
		RatioPercent int64 `json:"ratioPercent"`
	}
	if err := json.Unmarshal(raw, &meta); err != nil {
		t.Fatal(err)
	}
	// Recompute from the payload the package actually ships. This
	// previously used bytes.Repeat([]byte{0}, LogicalBytes) as a stand-in,
	// which silently agreed with the sidecar only while the twin's entry
	// held an incompressible run that was not a parsable snapshot document
	// at all; the sidecar could then describe bytes the package did not
	// contain.
	payload := RatioTwinPayload()
	if int64(len(payload)) != meta.LogicalBytes {
		t.Fatalf("logicalBytes drifted: meta %d vs payload %d", meta.LogicalBytes, len(payload))
	}
	stream := deflateRaw(payload)
	if int64(len(stream)) != meta.StoredBytes {
		t.Fatalf("storedBytes drifted: meta %d vs recomputed %d", meta.StoredBytes, len(stream))
	}
	pct := meta.LogicalBytes * 100 / meta.StoredBytes
	if pct != meta.RatioPercent {
		t.Fatalf("ratioPercent inconsistent: %d vs %d", pct, meta.RatioPercent)
	}
	// Below the ratified L15 rejection ratio of 1000:1 (FS §9.3 L15).
	if meta.LogicalBytes >= meta.StoredBytes*1000 {
		t.Fatalf("expansion ratio %d:%d reached or exceeded L15", meta.LogicalBytes, meta.StoredBytes)
	}
	if meta.LogicalBytes > artifactCapBytes {
		t.Fatalf("logical expansion exceeds the inert cap")
	}

	// BLOCKER-5: the committed PACKAGE must correspond to its sidecar —
	// locate the deflate entry inside the twin .domo and check that its
	// stored/uncompressed sizes are exactly the sidecar's numbers.
	pkg, err := zip.OpenReader(filepath.Join(corpusRoot,
		"hostile/container/ratio-under-L15-success-twin.domo"))
	if err != nil {
		t.Fatal(err)
	}
	defer pkg.Close()
	found := false
	for _, f := range pkg.File {
		if f.Method != zip.Deflate {
			continue
		}
		if uint64(f.UncompressedSize64) == uint64(meta.LogicalBytes) &&
			uint64(f.CompressedSize64) == uint64(meta.StoredBytes) {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("no package entry matches the sidecar sizes (%d logical / %d stored): payload and sidecar diverge",
			meta.LogicalBytes, meta.StoredBytes)
	}
}

// TestValidateAssertInvariants pins the exact enforcement behavior of the
// assert contract (BLOCKER-7/8): required-assert omission, false rawByDesign
// claims, sidecar pollution, and outcome contradictions each MUST produce a
// violation; compliant records produce none.
func TestValidateAssertInvariants(t *testing.T) {
	good := func() expectation {
		return expectation{
			ID: "test/ok", Class: "golden",
			Artifact: "golden/entities/level-minimal.json",
			Outcome:  "success",
			Assert:   map[string]interface{}{"outcome": "success"},
		}
	}
	if errs := assertChecks(good()); len(errs) != 0 {
		t.Fatalf("compliant record flagged: %v", errs)
	}

	missing := good()
	missing.Assert = nil
	if errs := assertChecks(missing); len(errs) == 0 || !strings.Contains(errs[0], "required manifest assert object") {
		t.Errorf("BLOCKER-7 unenforced: missing assert passed: %v", errs)
	}

	falseRaw := good()
	falseRaw.RawByDesign = true
	if errs := assertChecks(falseRaw); len(errs) == 0 || !strings.Contains(strings.Join(errs, "; "), "rawByDesign") {
		t.Errorf("BLOCKER-8 unenforced: false rawByDesign claim passed: %v", errs)
	}

	sidecar := good()
	sidecar.Artifact = sidecarRelPath
	sidecar.Assert = map[string]interface{}{"outcome": "success"}
	if errs := assertChecks(sidecar); len(errs) == 0 || !strings.Contains(errs[0], "sidecar") {
		t.Errorf("sidecar pollution unenforced: %v", errs)
	}

	mismatch := good()
	mismatch.Assert = map[string]interface{}{"outcome": "reject"}
	if errs := assertChecks(mismatch); len(errs) == 0 || !strings.Contains(errs[0], "contradicts record outcome") {
		t.Errorf("outcome contradiction unenforced: %v", errs)
	}

	warn := good()
	warn.Outcome = "warning"
	warn.Assert = map[string]interface{}{"outcome": "success"}
	if errs := assertChecks(warn); len(errs) == 0 || !strings.Contains(errs[0], "contradicts") {
		t.Errorf("warning mismatch unenforced: %v", errs)
	}
}

// TestCoverageAuditInvariants proves omission and wrong-claim detection for
// the audit validators (final review blocker): a ratified token absent from
// the audit, a wrong-row claim, an unknown-record claim, and an empty
// coveredBy each MUST fail.
func TestCoverageAuditInvariants(t *testing.T) {
	tok := "malformed-container"
	entries := []auditEntry{
		{Token: tok, Status: "covered", CoveredBy: []string{"hostile/container/empty-file"}},
	}
	recIDs := map[string]bool{"hostile/container/empty-file": true}
	catOfRec := map[string]string{"hostile/container/empty-file": tok}
	recRows := map[string]map[string]bool{
		"hostile/container/empty-file": {"L1": true},
	}

	// omit a different ratified token -> must be flagged
	if errs := auditedTokenSetViolations(entries); len(errs) == 0 {
		t.Error("omitted ratified tokens not flagged")
	}

	wrongCat := auditEntry{Token: "checksum-error", Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file"}}
	if errs := coveredCategoryViolations(wrongCat, recIDs, catOfRec); len(errs) == 0 {
		t.Error("wrong-category claim not flagged")
	}

	wrongRow := auditEntry{Row: "L3", Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file"}}
	if errs := coveredRowViolations(wrongRow, recIDs, recRows); len(errs) == 0 ||
		!strings.Contains(strings.Join(errs, "; "), "does not map row L3") {
		t.Errorf("wrong-row claim not flagged: %v", errs)
	}

	rightRow := auditEntry{Row: "L1", Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file"}}
	if errs := coveredRowViolations(rightRow, recIDs, recRows); len(errs) != 0 {
		t.Errorf("correct row claim flagged: %v", errs)
	}

	unknownRec := auditEntry{Row: "L1", Status: "covered", CoveredBy: []string{"no/such/record"}}
	if errs := coveredRowViolations(unknownRec, recIDs, recRows); len(errs) == 0 {
		t.Error("unknown-record claim not flagged")
	}

	emptyCovered := auditEntry{Token: tok, Status: "covered"}
	if errs := coveredCategoryViolations(emptyCovered, recIDs, catOfRec); len(errs) == 0 {
		t.Error("empty coveredBy not flagged")
	}

	// A "covered" claim resting only on blocked records is the exact
	// substitution issue 0018's audit condemned: missing-manifest was
	// recorded as covered by a single record that skipped. These negative
	// invariants prove the rule bites rather than merely existing.
	blockedOnly := map[string]bool{"hostile/container/empty-file": true}
	soleBlockedCat := auditEntry{Token: tok, Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file"}}
	errs := coveredCategoryViolationsBlocked(soleBlockedCat, recIDs, catOfRec, blockedOnly)
	if len(errs) == 0 || !strings.Contains(strings.Join(errs, "; "), "not executed coverage") {
		t.Errorf("category covered solely by a blocked record not flagged: %v", errs)
	}
	soleBlockedRow := auditEntry{Row: "L1", Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file"}}
	errs = coveredRowViolationsBlocked(soleBlockedRow, recIDs, recRows, blockedOnly)
	if len(errs) == 0 || !strings.Contains(strings.Join(errs, "; "), "not executed coverage") {
		t.Errorf("row covered solely by a blocked record not flagged: %v", errs)
	}

	// A blocked record ALONGSIDE executing evidence is fine: the rule
	// forbids substitution, not co-listing.
	recIDs["hostile/container/bad-signature"] = true
	catOfRec["hostile/container/bad-signature"] = tok
	recRows["hostile/container/bad-signature"] = map[string]bool{"L1": true}
	mixedCat := auditEntry{Token: tok, Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file", "hostile/container/bad-signature"}}
	if errs := coveredCategoryViolationsBlocked(mixedCat, recIDs, catOfRec, blockedOnly); len(errs) != 0 {
		t.Errorf("blocked record co-listed with executing evidence must be accepted: %v", errs)
	}
	mixedRow := auditEntry{Row: "L1", Status: "covered",
		CoveredBy: []string{"hostile/container/empty-file", "hostile/container/bad-signature"}}
	if errs := coveredRowViolationsBlocked(mixedRow, recIDs, recRows, blockedOnly); len(errs) != 0 {
		t.Errorf("blocked record co-listed with executing evidence must be accepted: %v", errs)
	}
}

// TestExecutionBlockedMarkerIsEnforced proves the manifest marker's
// validation bites. The marker is the only thing standing between a green
// non-conformance witness and a false coverage claim, so a marker naming an
// assertion the record does not carry — or missing its route, reason, or
// clauses — must be a hard error rather than documentation.
func TestExecutionBlockedMarkerIsEnforced(t *testing.T) {
	base := func() expectation {
		return expectation{
			ID:      "adversarial/vectors/probe",
			Class:   "adversarial",
			Outcome: "success",
			Clauses: []string{"FS §4.7 V2"},
			Assert:  map[string]interface{}{"finalThicknessMm": 150.0},
			ExecutionBlocked: &executionBlocked{
				State:      "blocked-implementation-nonconformance",
				Assertions: []string{"finalThicknessMm"},
				Observed:   map[string]interface{}{"finalThicknessMm": 120.0},
				RoutedTo:   "ED-0032",
				Clauses:    []string{"FS §4.6 LWW1"},
				Reason:     "recorded reason",
			},
		}
	}
	for _, tc := range []struct {
		name    string
		mutate  func(*expectation)
		wantErr bool
	}{
		{"well-formed", func(*expectation) {}, false},
		{"unknown state", func(r *expectation) { r.ExecutionBlocked.State = "flaky" }, true},
		{"no assertions named", func(r *expectation) { r.ExecutionBlocked.Assertions = nil }, true},
		{"assertion absent from assert block", func(r *expectation) {
			r.ExecutionBlocked.Assertions = []string{"noSuchAssertion"}
		}, true},
		{"no route", func(r *expectation) { r.ExecutionBlocked.RoutedTo = "" }, true},
		{"no reason", func(r *expectation) { r.ExecutionBlocked.Reason = "" }, true},
		{"no clauses", func(r *expectation) { r.ExecutionBlocked.Clauses = nil }, true},
		{"no observed value for a blocked assertion", func(r *expectation) {
			r.ExecutionBlocked.Observed = nil
		}, true},
		{"observed value equals the oracle", func(r *expectation) {
			r.ExecutionBlocked.Observed = map[string]interface{}{"finalThicknessMm": 150.0}
		}, true},
		{"observed value for an unblocked assertion", func(r *expectation) {
			r.Assert["other"] = 1.0
			r.ExecutionBlocked.Observed["other"] = 2.0
		}, true},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			r := base()
			tc.mutate(&r)
			errs := executionBlockedViolations(r)
			if tc.wantErr && len(errs) == 0 {
				t.Error("expected a violation, got none")
			}
			if !tc.wantErr && len(errs) != 0 {
				t.Errorf("unexpected violations: %v", errs)
			}
		})
	}
}
