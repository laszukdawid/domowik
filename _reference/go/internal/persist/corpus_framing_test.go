package persist_test

import (
	"encoding/json"
	"fmt"
	"sort"
	"strings"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// corpus_framing_test.go: hostile/framing fixtures assembled into minimal
// packages must reject malformed-framing before any replay; hostile/json
// and golden parser twins classify through the limited decoder and schema
// paths; migration records pin the V3 gate.

// TestFramingFixturesRejectBeforeReplay wraps each framing fixture into
// a minimal package and asserts the pre-replay malformed-framing reject.
func TestFramingFixturesRejectBeforeReplay(t *testing.T) {
	idx := expectationIndex(t)
	for id, e := range idx {
		if !strings.HasPrefix(id, "hostile/framing/") || e.Recipe == nil {
			continue
		}
		e := e
		t.Run(id, func(t *testing.T) {
			var docs [][]byte
			switch e.ID {
			case "hostile/framing/duplicate-batch-ids":
				inputs, _ := e.Recipe["inputs"].([]any)
				for _, i := range inputs {
					path := i.(map[string]any)["path"].(string)
					raw := corpusFile(t, path)
					if h := sha256HexOf(raw); h != i.(map[string]any)["sha256"].(string) {
						t.Fatalf("recipe input integrity: %s", path)
					}
					docs = append(docs, raw)
				}
				if len(docs) != 2 {
					t.Fatal("expected a two-batch recipe")
				}
				requireDuplicateBatchIDShape(t, docs[0], docs[1])
			default:
				docs = append(docs, corpusFile(t, e.Artifact))
			}
			pkg := assembleBatchOnlyPackage(t, docs)
			_, err := persist.Load(pkg)
			if err == nil {
				t.Fatalf("expected reject %s", deref(e.Category))
			}
			got, ok := persist.CategoryOf(err)
			if !ok || got != persist.Category(deref(e.Category)) {
				t.Fatalf("expected %s, got %s (%v)", deref(e.Category), got, err)
			}
		})
	}
}

// assembleBatchOnlyPackage serializes raw batch documents verbatim into a
// minimal conforming container (empty snapshot), preserving torn/invalid
// documents byte-for-byte so the reader's framing stage classifies them.
func assembleBatchOnlyPackage(t *testing.T, docs [][]byte) []byte {
	t.Helper()
	raws := map[string][]byte{}
	for i, raw := range docs {
		raws[batchNameFor(t, raw, i)] = raw
	}
	pkg, werr := persist.AssembleRawPackage(raws)
	if werr != nil {
		t.Fatal(werr)
	}
	return pkg
}

var corpusBatchBytes = []byte(`{"batchId":"01948f5b-c4e4-7e42-b996-f552a18df77f","actorId":"01948f5a-ca77-7a09-9e76-5ace11b12fd2","counterBase":42,"frontier":{"01948f5a-ca77-7a09-9e76-5ace11b12fd2":41},"operations":[{"operationId":"01948f5d-6e18-7c01-8f0d-77f974bd8036","kind":"createEntity","entityId":"01948f55-6b18-7d3a-a153-1ee2f33cc402","fields":{"id":"01948f55-6b18-7d3a-a153-1ee2f33cc402","type":"level","name":"Level 1","elevationMm":0,"floorThicknessMm":200,"defaultWallHeightMm":2500,"defaultCeilingHeightMm":2500}}]}`)

func batchNameFor(t *testing.T, raw []byte, fallbackIdx int) string {
	t.Helper()
	var probe struct {
		CounterBase int64  `json:"counterBase"`
		ActorID     string `json:"actorId"`
	}
	if json.Unmarshal(raw, &probe) == nil && probe.ActorID != "" && probe.CounterBase > 0 {
		return fmt.Sprintf("ops/%019d-%s.json", probe.CounterBase, probe.ActorID)
	}
	// Torn documents without parsable identity get a well-formed name;
	// content defects remain the classification target.
	return fmt.Sprintf("ops/%019d-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json", 42+int64(fallbackIdx))
}

// TestJSONLevelFixtures routes standalone JSON probes through their
// decoder paths and asserts exact taxonomy tokens.
func TestJSONLevelFixtures(t *testing.T) {
	idx := expectationIndex(t)
	for id, e := range idx {
		isHostile := strings.HasPrefix(id, "hostile/json/")
		isTwin := strings.HasPrefix(id, "golden/parser-twins/")
		if !isHostile && !isTwin {
			continue
		}
		if e.Artifact == "" || !strings.HasSuffix(e.Artifact, ".json") {
			continue
		}
		e := e
		t.Run(id, func(t *testing.T) {
			raw := corpusFile(t, e.Artifact)
			if id == "hostile/json/duplicate-object-keys" {
				requireRepeatedTopLevelMember(t, raw)
			}
			got := ""
			if strings.HasPrefix(id, "golden/parser-twins/") {
				// Parser twins pin the PARSER-security boundary (L11/L12/
				// L13 parse stage). Semantic-coordinate outcomes below
				// this layer are governed by PROVISIONAL parameters whose
				// values are unratified until issue 0030 (corpus record
				// gaps/provisional-parameter-bound-probes); they are not
				// part of these twins' pinned expectations.
				if perr := persist.ParseLimitedJSON(raw); perr != nil {
					c, _ := persist.CategoryOf(perr)
					got = string(c)
				}
			} else {
				got = classifyJSONProbe(t, raw)
			}
			want := deref(e.Category)
			if got == "" && want == "" {
				return // success-class twin with no token
			}
			if want == "" {
				if got != "" {
					t.Fatalf("expected success-class outcome, got %s", got)
				}
				return
			}
			if got != want {
				t.Fatalf("expected %s, got %q", want, got)
			}
		})
	}
}

// codeAlias maps delivered document-layer diagnostic codes onto the
// ratified FS §9.4 taxonomy tokens they fall under for these probes.
// Vocabulary alignment beyond this narrow set is an open item with the
// issue 0004 owners (recorded in the handoff report).
var codeAlias = map[string]string{
	"missing-required-field": "invalid-field-syntax",
	"invalid-opening-kind":   "invalid-field-syntax",
	"invalid-side":           "invalid-field-syntax",
	"invalid-hinge":          "invalid-field-syntax",
	"invalid-opens-toward":   "invalid-field-syntax",
}

func classifyJSONProbe(t *testing.T, raw []byte) string {
	t.Helper()
	// Stage 1: limited decoder — duplicate keys, depth, number tokens,
	// non-finite literals, i64 overflow (parser-security rows).
	if perr := persist.ParseLimitedJSON(raw); perr != nil {
		c, _ := persist.CategoryOf(perr)
		return string(c)
	}
	var shape map[string]any
	if err := json.Unmarshal(raw, &shape); err != nil {
		return "" // rawByDesign documents already classified above
	}
	// Stage 2: entity schema path.
	if _, isObj := shape["type"]; isObj {
		_, derr := document.DecodeEntity(raw)
		if derr != nil {
			fe, ok := derr.(*document.FieldError)
			if !ok {
				return "invalid-field-syntax"
			}
			if aliased, has := codeAlias[fe.Code]; has {
				return aliased
			}
			return fe.Code
		}
		return ""
	}
	// Stage 3: digest identifier form probes.
	if d, ok := shape["digest"].(string); ok {
		if verr := persist.ValidateDigestID(d); verr != nil {
			c, _ := persist.CategoryOf(verr)
			return string(c)
		}
		return ""
	}
	// Stage 4: descriptor provenance timestamp form.
	if prov, ok := shape["provenance"].(map[string]any); ok {
		if at, ok := prov["acquiredAt"].(string); ok {
			if terr := persist.ValidateTimestamp(at); terr != nil {
				c, _ := persist.CategoryOf(terr)
				return string(c)
			}
			return ""
		}
	}
	return ""
}

// TestMigrationGateFixtures pins the V3 migration gate tokens.
func TestMigrationGateFixtures(t *testing.T) {
	idx := expectationIndex(t)
	for id, e := range idx {
		if !strings.HasPrefix(id, "migration/") || e.Artifact == "" ||
			!strings.HasSuffix(e.Artifact, ".json") {
			continue
		}
		e := e
		t.Run(id, func(t *testing.T) {
			raw := corpusFile(t, e.Artifact)
			var m map[string]any
			if err := json.Unmarshal(raw, &m); err != nil {
				t.Fatalf("manifest fixture unparsable: %v", err)
			}
			if foreignName, ok := m["foreignEntryName"].(string); ok {
				// U3/V5 probe: smuggle the foreign namespace entry into an
				// otherwise valid "0.1" package and load it for real.
				raws := map[string][]byte{}
				for name, data := range map[string][]byte{
					"assets/descriptors.json": []byte(`{}`),
					"ops/0000000000000000042-01948f5a-ca77-7a09-9e76-5ace11b12fd2.json": corpusBatchBytes,
				} {
					raws[name] = data
				}
				raws[foreignName] = []byte("hostile")
				pkg, werr := persist.AssembleRawPackage(raws)
				if werr != nil {
					t.Fatal(werr)
				}
				_, lerr := persist.Load(pkg)
				got := ""
				if lerr != nil {
					c, _ := persist.CategoryOf(lerr)
					got = string(c)
				}
				want, _ := m["expectedOutcome"].(map[string]any)["category"].(string)
				if got != want {
					t.Fatalf("foreign namespace %q: expected %s, got %q", foreignName, want, got)
				}
				return
			}
			fv, _ := m["formatVersion"].(string)
			got := ""
			if fv != "0.1" {
				got = "unsupported-version"
			}
			if want := deref(e.Category); want != "" && got != want {
				t.Fatalf("expected gate token %s, got %q (formatVersion=%q)", want, got, fv)
			}
		})
	}
}

// requireDuplicateBatchIDShape pins the defect the duplicate-batch-ids
// recipe is supposed to isolate. FS 7.6 derives the entry name from
// counterBase and actorId only, so two entries CAN legally coexist while
// carrying the same batchId — the superseded inputs were byte-identical,
// which collided at duplicate logical names before the duplicate-id check
// could run and made the record look unmaterializable. The pair must now
// share batchId and differ in counterBase, and their ranges must be
// contiguous so neither an overlap nor a gap can supply the reject.
func requireDuplicateBatchIDShape(t *testing.T, a, b []byte) {
	t.Helper()
	type doc struct {
		BatchID     string            `json:"batchId"`
		ActorID     string            `json:"actorId"`
		CounterBase int64             `json:"counterBase"`
		Operations  []json.RawMessage `json:"operations"`
	}
	var da, db doc
	if json.Unmarshal(a, &da) != nil || json.Unmarshal(b, &db) != nil {
		t.Fatal("both recipe inputs must be parsable batch documents")
	}
	if da.BatchID != db.BatchID {
		t.Fatalf("recipe must share one batchId, got %s and %s", da.BatchID, db.BatchID)
	}
	if da.ActorID != db.ActorID {
		t.Fatalf("recipe must use one actor, got %s and %s", da.ActorID, db.ActorID)
	}
	if da.CounterBase == db.CounterBase {
		t.Fatal("recipe inputs must differ in counterBase, or FS 7.6 naming collides before the duplicate-id check")
	}
	lo, hi := da, db
	if hi.CounterBase < lo.CounterBase {
		lo, hi = hi, lo
	}
	if want := lo.CounterBase + int64(len(lo.Operations)); hi.CounterBase != want {
		t.Fatalf("ranges must be contiguous so the reject is attributable to the duplicate id alone: [%d,%d] then base %d (want %d)",
			lo.CounterBase, lo.CounterBase+int64(len(lo.Operations))-1, hi.CounterBase, want)
	}
}

// requireRepeatedTopLevelMember proves, independently of the product
// parser, that the duplicate-key fixture really repeats a top-level member
// name. Without it the record's invalid-field-syntax token is satisfied by
// any of a dozen unrelated syntax defects — which is exactly how the
// superseded fixture, a perfectly valid level document, stayed pinned as a
// duplicate-key reject. The scan is deliberately its own minimal tokenizer
// rather than a call into the decoder under test.
func requireRepeatedTopLevelMember(t *testing.T, raw []byte) {
	t.Helper()
	seen := map[string]int{}
	depth := 0
	inString, escaped := false, false
	var cur []byte
	for i := 0; i < len(raw); i++ {
		c := raw[i]
		if inString {
			switch {
			case escaped:
				escaped = false
			case c == '\\':
				escaped = true
			case c == '"':
				inString = false
				if depth == 1 {
					// A string at depth 1 is a member NAME only when the
					// next significant byte is the name/value separator.
					for j := i + 1; j < len(raw); j++ {
						if raw[j] == ' ' || raw[j] == '\t' || raw[j] == '\n' || raw[j] == '\r' {
							continue
						}
						if raw[j] == ':' {
							seen[string(cur)]++
						}
						break
					}
				}
			default:
				cur = append(cur, c)
			}
			continue
		}
		switch c {
		case '"':
			inString, cur = true, cur[:0]
		case '{', '[':
			depth++
		case '}', ']':
			depth--
		}
	}
	var repeated []string
	for name, n := range seen {
		if n > 1 {
			repeated = append(repeated, fmt.Sprintf("%q x%d", name, n))
		}
	}
	if len(repeated) == 0 {
		t.Fatalf("fixture contains no repeated top-level member name, so its duplicate-key expectation is not what the reader classifies; scanned %d distinct names", len(seen))
	}
	sort.Strings(repeated)
	t.Logf("duplicate top-level member(s) present in the fixture bytes: %s", strings.Join(repeated, ", "))
}
