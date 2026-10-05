package cli

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// Deterministic identity (Agreement v3 point b): a counter-backed source
// yields well-formed RFC 9562-shaped UUIDs; no test depends on randomness
// or time. Assertions prefer structural effects over literal ids.
type mintSeq struct{ n uint64 }

func (m *mintSeq) next() document.UUID {
	n := atomic.AddUint64(&m.n, 1)
	var u document.UUID
	for i := 0; i < 8; i++ {
		u[i] = byte(n >> (8 * i))
	}
	u[6] = (u[6] & 0x0F) | 0x40 // version nibble
	u[8] = (u[8] & 0x3F) | 0x80 // variant bits
	return u
}

func withDeterministicMint(t *testing.T) {
	t.Helper()
	seq := &mintSeq{}
	prev := applyMintUUID
	applyMintUUID = seq.next
	t.Cleanup(func() { applyMintUUID = prev })
}

const (
	fxActor  = "019534c7-0000-7000-8000-0000000000aa"
	fxBatch1 = "019534c7-0000-7000-8000-0000000000b1"
	fxBatch2 = "019534c7-0000-7000-8000-0000000000b2"
	fxLevel  = "019534c7-0000-7000-8000-000000000001"
	fxWall   = "019534c7-0000-7000-8000-000000000002"
)

const fxLevelJSON = `{"id":"` + fxLevel + `","type":"level","name":"Level 1","elevationMm":0,
 "floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}`

const fxWallJSON = `{"id":"` + fxWall + `","type":"wall","levelId":"` + fxLevel + `",
 "startMm":[0,2500],"endMm":[4000,2500],"thicknessMm":120,"heightMm":2500}`

// writeDoc materializes a thin package containing the given entities.
func writeDoc(t *testing.T, dir string, entityJSON ...string) string {
	t.Helper()
	state := document.State{
		Tombstones:       map[document.UUID]document.Position{},
		Frontier:         map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		CreatedPositions: map[document.UUID]document.Position{},
	}
	for _, raw := range entityJSON {
		ent, err := document.DecodeEntity([]byte(raw))
		if err != nil {
			t.Fatalf("fixture entity invalid: %v", err)
		}
		state.Entities = append(state.Entities, ent)
	}
	pkg, werr := persist.BuildPackage(&persist.WriteInput{
		Profile: "thin", State: state, Blobs: map[string][]byte{},
	})
	if werr != nil {
		t.Fatalf("BuildPackage: %v", werr)
	}
	path := filepath.Join(dir, "doc.domo")
	if err := os.WriteFile(path, pkg, 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func writeScript(t *testing.T, dir, body string) string {
	t.Helper()
	path := filepath.Join(dir, "script.json")
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatal(err)
	}
	return path
}

func runApplyCmd(args ...string) (int, string, string) {
	var stdout, stderr bytes.Buffer
	code := Run(append([]string{"apply"}, args...), &stdout, &stderr)
	return code, stdout.String(), stderr.String()
}

func readFileBytes(t *testing.T, path string) []byte {
	t.Helper()
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return data
}

func mustLoadProjection(t *testing.T, path string) *document.Projection {
	t.Helper()
	view, err := persist.LoadFile(path)
	if err != nil {
		t.Fatalf("reload failed: %v", err)
	}
	return view.Projection
}

func TestApplyCreatesEntitiesEndToEnd(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	script := writeScript(t, dir, fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s},
		  {"operationId":"019534c7-0000-7000-8000-000000000102","kind":"createEntity","entityId":"%s","payload":%s}
		]}]}`, fxBatch1, fxLevel, fxLevelJSON, fxWall, fxWallJSON))

	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", script)
	if code != 0 || errStr != "" {
		t.Fatalf("exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "saved") || !strings.Contains(out, "+2") {
		t.Fatalf("unexpected report: %q", out)
	}
	proj := mustLoadProjection(t, doc)
	if proj.Entity(mustParse(t, fxLevel)) == nil || proj.Entity(mustParse(t, fxWall)) == nil {
		t.Fatal("applied entities absent after reload")
	}
}

func TestApplyRerunSkipsAndWritesNothing(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	scriptBody := fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s}
		]}]}`, fxBatch1, fxLevel, fxLevelJSON)
	script := writeScript(t, dir, scriptBody)

	if code, _, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", script); code != 0 {
		t.Fatalf("first run exit=%d stderr=%q", code, errStr)
	}
	first := readFileBytes(t, doc)
	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", script)
	if code != 0 || errStr != "" {
		t.Fatalf("rerun exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "skipped") {
		t.Fatalf("rerun did not report skip: %q", out)
	}
	if got := readFileBytes(t, doc); !bytes.Equal(first, got) {
		t.Fatal("all-skip invocation rewrote the target file")
	}
}

func TestApplyRollsBackWholeInvocationOnLaterBatchFailure(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	before := readFileBytes(t, doc)
	script := writeScript(t, dir, fmt.Sprintf(
		`{"batches":[
		  {"batchId":"%s","operations":[
		    {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s}]},
		  {"batchId":"%s","operations":[
		    {"operationId":"019534c7-0000-7000-8000-000000000102","kind":"setFields","entityId":"%s",
		     "fields":{"bogusFieldMm":1}}]}
		]}`, fxBatch1, fxLevel, fxLevelJSON, fxBatch2, fxLevel))

	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", script)
	if code != 1 {
		t.Fatalf("exit=%d stdout=%q", code, out)
	}
	if !strings.Contains(errStr, "rejected before any effect") {
		t.Fatalf("stderr lacks rejection diagnostic: %q", errStr)
	}
	if !strings.Contains(errStr, "unknown-field") {
		t.Fatalf("stderr lost the underlying document diagnostic code: %q", errStr)
	}
	if out != "" {
		t.Fatalf("failed invocation wrote to stdout: %q", out)
	}
	if got := readFileBytes(t, doc); !bytes.Equal(before, got) {
		t.Fatal("later-batch failure left a partial write behind")
	}
	if e := mustLoadProjection(t, doc).Entity(mustParse(t, fxLevel)); e != nil {
		t.Fatal("earlier batch of the failed invocation survived the rollback")
	}
}

func TestApplyDryRunReportsWithoutWriting(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	before := readFileBytes(t, doc)
	script := writeScript(t, dir, fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s},
		  {"operationId":"019534c7-0000-7000-8000-000000000102","kind":"createEntity","entityId":"%s","payload":%s}
		]}]}`, fxBatch1, fxLevel, fxLevelJSON, fxWall, fxWallJSON))

	code, out, errStr := runApplyCmd("--document", doc, "--dry-run", "--actor", fxActor, "--script", script)
	if code != 0 || errStr != "" {
		t.Fatalf("exit=%d stderr=%q", code, errStr)
	}
	for _, want := range []string{"no writes performed", "+2", mustParse(t, fxLevel).String()} {
		if !strings.Contains(out, want) {
			t.Fatalf("dry-run report missing %q: %q", want, out)
		}
	}
	if got := readFileBytes(t, doc); !bytes.Equal(before, got) {
		t.Fatal("dry-run modified the target file")
	}
}

func TestApplyRejectsSpoofedMetadata(t *testing.T) {
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	cases := map[string]string{
		"batch frontier": `{"batches":[{"batchId":"` + fxBatch1 + `","frontier":{},"operations":[` +
			`{"operationId":"019534c7-0000-7000-8000-000000000101","kind":"deleteEntity","entityId":"` + fxLevel + `"}]}]}`,
		"batch counterBase": `{"batches":[{"counterBase":42,"operations":[` +
			`{"operationId":"019534c7-0000-7000-8000-000000000101","kind":"deleteEntity","entityId":"` + fxLevel + `"}]}]}`,
		"op actorId": `{"batches":[{"operations":[` +
			`{"operationId":"019534c7-0000-7000-8000-000000000101","kind":"deleteEntity","entityId":"` + fxLevel + `","actorId":"` + fxActor + `"}]}]}`,
	}
	for name, body := range cases {
		script := writeScript(t, dir, body)
		code, out, errStr := runApplyCmd("--document", doc, "--script", script)
		if code != 1 || out != "" {
			t.Fatalf("%s: exit=%d stdout=%q", name, code, out)
		}
		if !strings.Contains(errStr, "system-assigned") {
			t.Fatalf("%s: stderr lacks spoof explanation: %q", name, errStr)
		}
	}
}

func TestApplyChainsCountersAfterSkippedStoredDuplicate(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	create := fmt.Sprintf(`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s}
		]}]}`, fxBatch1, fxLevel, fxLevelJSON)
	redeclareAndExtend := fmt.Sprintf(`{"batches":[
		  {"batchId":"%s","operations":[
		    {"operationId":"019534c7-0000-7000-8000-000000000101","kind":"createEntity","entityId":"%s","payload":%s}]},
		  {"batchId":"%s","operations":[
		    {"operationId":"019534c7-0000-7000-8000-000000000103","kind":"setFields","entityId":"%s",
		     "fields":{"name":"Renamed"}}]}
		]}`, fxBatch1, fxLevel, fxLevelJSON, fxBatch2, fxLevel)

	if code, _, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, create)); code != 0 {
		t.Fatalf("run1 exit=%d stderr=%q", code, errStr)
	}
	// Run 2 redeclares B1 (stored-identical => skipped) and adds B2; B3
	// chaining requires that the skipped batch consumed NO counter slot.
	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, redeclareAndExtend))
	if code != 0 {
		t.Fatalf("run2 exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "1 skipped") || !strings.Contains(out, "1 applied") {
		t.Fatalf("run2 report unexpected: %q", out)
	}
	if !strings.Contains(out, "skipped (duplicate of applied batch)") ||
		!strings.Contains(out, ": applied (1 ops)") {
		t.Fatalf("run2 per-batch outcome lines missing: %q", out)
	}
	proj := mustLoadProjection(t, doc)
	level := proj.Entity(mustParse(t, fxLevel))
	if level == nil {
		t.Fatal("level vanished")
	}
	raw, _ := document.EncodeEntity(level)
	if !strings.Contains(string(raw), "Renamed") {
		t.Fatalf("second candidate did not chain contiguously: %s", raw)
	}
}

func TestApplyCanonicalRedeliveryIsSkippedDespiteFormatting(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir, fxLevelJSON)
	setFields := func(order string) string {
		fields := map[string]string{
			"a": `"defaultWallHeightMm":2600,"floorThicknessMm":150`,
			"b": ` "floorThicknessMm":150 , "defaultWallHeightMm":2600 `,
		}
		return fmt.Sprintf(`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000201","kind":"setFields","entityId":"%s",
		   "fields":{%s}}
		]}]}`, fxBatch1, fxLevel, fields[order])
	}
	if code, _, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, setFields("a"))); code != 0 {
		t.Fatalf("run1 exit=%d stderr=%q", code, errStr)
	}
	first := readFileBytes(t, doc)
	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, setFields("b")))
	if code != 0 || errStr != "" {
		t.Fatalf("run2 exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "skipped") {
		t.Fatalf("canonical redelivery not recognized as duplicate: %q", out)
	}
	if got := readFileBytes(t, doc); !bytes.Equal(first, got) {
		t.Fatal("duplicate redelivery changed the target")
	}
}

func TestApplyDifferingContentUnderSameBatchIdRejects(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir, fxLevelJSON)
	first := fmt.Sprintf(`{"batches":[{"batchId":"%s","operations":[
	  {"operationId":"019534c7-0000-7000-8000-000000000201","kind":"setFields","entityId":"%s","fields":{"name":"First"}}]}]}`,
		fxBatch1, fxLevel)
	if code, _, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, first)); code != 0 {
		t.Fatalf("run1 exit=%d stderr=%q", code, errStr)
	}
	before := readFileBytes(t, doc)
	conflict := strings.Replace(first, "First", "Second", 1)
	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", writeScript(t, dir, conflict))
	if code != 1 || out != "" {
		t.Fatalf("conflict exit=%d stdout=%q", code, out)
	}
	if !strings.Contains(errStr, "malformed-framing") || !strings.Contains(errStr, "differing command content") {
		t.Fatalf("stderr lacks framing token: %q", errStr)
	}
	if got := readFileBytes(t, doc); !bytes.Equal(before, got) {
		t.Fatal("conflicting redelivery modified the target")
	}
}

func TestApplyStaleDiscardIsReportedNotFatal(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	script := writeScript(t, dir, fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000501","kind":"deleteEntity","entityId":"%s"}]}]}`,
		fxBatch1, fxLevel))

	code, out, errStr := runApplyCmd("--document", doc, "--actor", fxActor, "--script", script)
	if code != 0 || errStr != "" {
		t.Fatalf("exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "discarded stale") || !strings.Contains(out, "stale-batch-discard") {
		t.Fatalf("stale discard not reported as diagnostic outcome: %q", out)
	}
}

func TestApplyUsageErrorsExitTwo(t *testing.T) {
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	script := writeScript(t, dir, `{"batches":[]}`)
	cases := [][]string{
		nil,
		{"--script", script}, // missing --document
		{"--document", doc},  // missing script source
		{"--document", doc, "--stdin", "--script", script},
		{"--document", doc, "--script", script, "extra"},
		{"--document", doc, "--script", script, "--actor", "not-a-uuid"},
	}
	for _, args := range cases {
		code, out, errStr := runApplyCmd(args...)
		if code != 2 || out != "" {
			t.Fatalf("args %v: exit=%d stdout=%q", args, code, out)
		}
		if !strings.Contains(errStr, "Usage:") {
			t.Fatalf("args %v: stderr lacks usage: %q", args, errStr)
		}
	}
}

func TestApplyEmptyBatchesRejected(t *testing.T) {
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	script := writeScript(t, dir, `{"batches":[{"batchId":"`+fxBatch1+`","operations":[]}]}`)
	code, out, errStr := runApplyCmd("--document", doc, "--script", script)
	if code != 1 || out != "" {
		t.Fatalf("exit=%d stdout=%q", code, out)
	}
	if !strings.Contains(errStr, "malformed-framing") {
		t.Fatalf("stderr lacks malformed-framing: %q", errStr)
	}
}

func TestApplyPerBatchOperationCap(t *testing.T) {
	dir := t.TempDir()
	doc := writeDoc(t, dir)
	var ops []string
	for i := 0; i <= persist.LimitOperationsPerBatch; i++ { // one over L23
		ops = append(ops, fmt.Sprintf(
			`{"operationId":"019534ff-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"%s"}`, i, fxLevel))
	}
	body := `{"batches":[{"batchId":"` + fxBatch1 + `","operations":[` + strings.Join(ops, ",") + `]}]}`
	script := writeScript(t, dir, body)
	code, out, errStr := runApplyCmd("--document", doc, "--script", script)
	if code != 1 || out != "" {
		t.Fatalf("exit=%d stdout=%q", code, out)
	}
	if !strings.Contains(errStr, "limit-operation-count") {
		t.Fatalf("stderr lacks limit token: %q", errStr)
	}
}

func TestApplyStdinSource(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir, fxLevelJSON)
	orig := stdinReader
	stdinReader = strings.NewReader(fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000301","kind":"setFields","entityId":"%s","fields":{"name":"ViaStdin"}}]}]}`,
		fxBatch1, fxLevel))
	defer func() { stdinReader = orig }()

	code, out, errStr := runApplyCmd("--document", doc, "--stdin", "--actor", fxActor)
	if code != 0 || errStr != "" {
		t.Fatalf("exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "saved") {
		t.Fatalf("unexpected report: %q", out)
	}
	raw, _ := document.EncodeEntity(mustLoadProjection(t, doc).Entity(mustParse(t, fxLevel)))
	if !strings.Contains(string(raw), "ViaStdin") {
		t.Fatal("stdin script had no effect")
	}
}

func TestApplyDryRunNeverTouchesBaselinePath(t *testing.T) {
	withDeterministicMint(t)
	dir := t.TempDir()
	doc := writeDoc(t, dir, fxLevelJSON)
	script := writeScript(t, dir, fmt.Sprintf(
		`{"batches":[{"batchId":"%s","operations":[
		  {"operationId":"019534c7-0000-7000-8000-000000000401","kind":"setFields","entityId":"%s","fields":{"name":"Projected"}}]}]}`,
		fxBatch1, fxLevel))

	orig := applyBaselineTarget
	applyBaselineTarget = func(path string) (*persist.TargetBaseline, error) {
		t.Errorf("dry-run must not acquire the save-side baseline (path %q)", path)
		return &persist.TargetBaseline{SHA256: "forbidden"}, nil
	}
	defer func() { applyBaselineTarget = orig }()

	code, out, errStr := runApplyCmd("--document", doc, "--dry-run", "--actor", fxActor, "--script", script)
	if code != 0 || errStr != "" {
		t.Fatalf("exit=%d stderr=%q", code, errStr)
	}
	if !strings.Contains(out, "no writes performed") {
		t.Fatalf("unexpected report: %q", out)
	}
}

// Scanner boundary probes (L11/L12, §9.4 trailing data, duplicate keys).
func TestScanLimitedJSONBoundaries(t *testing.T) {
	depthOK, depthBad := nestedArrays(128), nestedArrays(129)
	if err := scanLimitedJSON([]byte(depthOK)); err != nil {
		t.Fatalf("depth 128 must parse: %v", err)
	}
	if err := scanLimitedJSON([]byte(depthBad)); err == nil || err.Code != "limit-nesting-depth" {
		t.Fatalf("depth 129 must reject with limit-nesting-depth, got %v", err)
	}

	num64 := `{"x":` + strings.Repeat("9", 64) + `}`
	num65 := `{"x":` + strings.Repeat("9", 65) + `}`
	if err := scanLimitedJSON([]byte(num64)); err != nil {
		t.Fatalf("64-char number token must parse: %v", err)
	}
	if err := scanLimitedJSON([]byte(num65)); err == nil || err.Code != "limit-number-token" {
		t.Fatalf("65-char number token must reject with limit-number-token, got %v", err)
	}

	if err := scanLimitedJSON([]byte(`{} {}`)); err == nil || err.Code != "trailing-data" {
		t.Fatalf("trailing bytes must reject, got %v", err)
	}
	if err := scanLimitedJSON([]byte(`{"a":1,"a":2}`)); err == nil {
		t.Fatal("duplicate object key must reject")
	}
}

func nestedArrays(n int) string {
	return strings.Repeat("[", n) + strings.Repeat("]", n)
}

func mustParse(t *testing.T, s string) document.UUID {
	t.Helper()
	u, err := document.ParseUUID(s)
	if err != nil {
		t.Fatalf("bad fixture uuid %q: %v", s, err)
	}
	return u
}
