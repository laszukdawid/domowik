package cli

import (
	"encoding/json"
	"fmt"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// applyError is one apply-command failure carrying an exact FORMAT-SPEC
// §9.3/§9.4 rejection token (or a usage token for argument-level errors).
// Detail text is actionable and never a raw library error.
type applyError struct {
	Code   string
	Detail string
}

func (e *applyError) Error() string {
	if e.Detail == "" {
		return e.Code
	}
	return fmt.Sprintf("%s: %s", e.Code, e.Detail)
}

func applyErr(code, format string, args ...any) *applyError {
	return &applyError{Code: code, Detail: fmt.Sprintf(format, args...)}
}

// Rejection tokens used by the apply surface. Container/persist categories
// flow through unchanged from persist.Error; these cover script-side
// enforcement only.
const (
	codeUsage              = "usage-error"
	codeInvalidFieldSyntax = "invalid-field-syntax" // matches document vocabulary
	codeMalformedFraming   = "malformed-framing"
)

// The external script schema is UNTRUSTED INPUT (FORMAT-SPEC §9.1): it
// carries command CONTENT only. Actor identity, counters, and causal
// frontiers are system-assigned; any appearance of that metadata in a
// script is rejected before parsing proceeds so stored-operation authority
// can never be spoofed accidentally (issue 0015 criterion 1).
//
// Accepted shape (closed member sets at every level):
//
//	{"batches": [
//	  {"batchId": "<uuid>",            // OPTIONAL: idempotency key
//	   "operations": [
//	     {"operationId": "<uuid>",     // REQUIRED (O1 explicit identity)
//	      "kind": "createEntity",      "entityId": "<uuid>", "payload": {complete entity}}
//	     {"operationId": "<uuid>",     "kind": "setFields",    "entityId": "<uuid>", "fields": {top-level field → value}}
//	     {"operationId": "<uuid>",     "kind": "deleteEntity", "entityId": "<uuid>"}
//	   ]}]}

const (
	scriptSpoofHint = " actor identity, counters, and frontier are system-assigned and must not appear in scripts"
)

var (
	spoofMembers = map[string]bool{
		"actorId": true, "actor_id": true,
		"counterBase": true, "counter_base": true,
		"frontier": true,
	}
	batchMembers = map[string]bool{"batchId": true, "operations": true}
	opMembers    = map[string]bool{"operationId": true, "kind": true, "entityId": true, "payload": true, "fields": true}
)

// scriptBatch is one parsed script batch with NORMALIZED operations
// (Agreement v3 phase 1): every operation has passed through the document
// constructors, so payload bytes are in canonical form and semantic
// redelivery comparison (phase 2) is formatting-insensitive.
type scriptBatch struct {
	declaredID  document.UUID // zero when omitted
	operations  []*document.Operation
	declaredStr string // declared batchId text for diagnostics, "" when omitted
}

// scanLimitedJSON validates raw bytes as JSON under the §9.3 parser
// guards encoding/json does not enforce: L11 nesting depth, L12 number
// token length, L17 array element count, L18 object member count, plus
// duplicate-key detection per §9.4. It builds no value tree; syntax
// errors here are reported before any json.Unmarshal allocation.
func scanLimitedJSON(data []byte) *applyError {
	s := &scriptScanner{data: data}
	if err := s.scanValue(1); err != nil {
		return err
	}
	s.skipWS()
	if s.pos != len(s.data) {
		return applyErr("trailing-data", "unexpected trailing bytes after the JSON document")
	}
	return nil
}

type scriptScanner struct {
	data []byte
	pos  int
}

func (s *scriptScanner) skipWS() {
	for s.pos < len(s.data) {
		switch s.data[s.pos] {
		case ' ', '\t', '\n', '\r':
			s.pos++
		default:
			return
		}
	}
}

func (s *scriptScanner) peek() byte {
	return s.data[s.pos]
}

func (s *scriptScanner) scanValue(depth int) *applyError {
	if depth > persist.LimitJSONDepth { // L11: depth == 128 parses
		return applyErr("limit-nesting-depth", "JSON nesting depth would exceed %d", persist.LimitJSONDepth)
	}
	s.skipWS()
	if s.pos >= len(s.data) {
		return applyErr("invalid-field-syntax", "unexpected end of JSON input")
	}
	switch c := s.peek(); {
	case c == '{':
		return s.scanObject(depth)
	case c == '[':
		return s.scanArray(depth)
	case c == '"':
		return s.scanString()
	case c == 't':
		return s.expectLiteral("true")
	case c == 'f':
		return s.expectLiteral("false")
	case c == 'n':
		return s.expectLiteral("null")
	case c == '-' || (c >= '0' && c <= '9'):
		return s.scanNumber()
	default:
		return applyErr("invalid-field-syntax", "unexpected character %q in JSON input", c)
	}
}

func (s *scriptScanner) expectLiteral(lit string) *applyError {
	if s.pos+len(lit) > len(s.data) || string(s.data[s.pos:s.pos+len(lit)]) != lit {
		return applyErr("invalid-field-syntax", "invalid JSON literal")
	}
	s.pos += len(lit)
	return nil
}

func (s *scriptScanner) scanObject(depth int) *applyError {
	s.pos++ // consume '{'
	members := 0
	keys := map[string]struct{}{}
	s.skipWS()
	if s.pos < len(s.data) && s.peek() == '}' {
		s.pos++
		return nil
	}
	for {
		s.skipWS()
		if s.pos >= len(s.data) || s.peek() != '"' {
			return applyErr("invalid-field-syntax", "expected object member name")
		}
		key, err := s.scanStringRaw()
		if err != nil {
			return err
		}
		members++
		if members > persist.LimitObjectMemberCount { // L18: == N succeeds
			return applyErr("limit-member-count", "object exceeds %d members", persist.LimitObjectMemberCount)
		}
		if _, dup := keys[key]; dup {
			return applyErr("duplicate-object-key", "duplicate object key %q", key)
		}
		keys[key] = struct{}{}
		s.skipWS()
		if s.pos >= len(s.data) || s.peek() != ':' {
			return applyErr("invalid-field-syntax", "expected ':' after object member name")
		}
		s.pos++
		if err := s.scanValue(depth + 1); err != nil {
			return err
		}
		s.skipWS()
		if s.pos >= len(s.data) {
			return applyErr("invalid-field-syntax", "unterminated object")
		}
		switch s.peek() {
		case ',':
			s.pos++
		case '}':
			s.pos++
			return nil
		default:
			return applyErr("invalid-field-syntax", "expected ',' or '}' in object")
		}
	}
}

func (s *scriptScanner) scanArray(depth int) *applyError {
	s.pos++ // consume '['
	elements := 0
	s.skipWS()
	if s.pos < len(s.data) && s.peek() == ']' {
		s.pos++
		return nil
	}
	for {
		if err := s.scanValue(depth + 1); err != nil {
			return err
		}
		elements++
		if elements > persist.LimitArrayElementCount { // L17: == N succeeds
			return applyErr("limit-array-count", "array exceeds %d elements", persist.LimitArrayElementCount)
		}
		s.skipWS()
		if s.pos >= len(s.data) {
			return applyErr("invalid-field-syntax", "unterminated array")
		}
		switch s.peek() {
		case ',':
			s.pos++
		case ']':
			s.pos++
			return nil
		default:
			return applyErr("invalid-field-syntax", "expected ',' or ']' in array")
		}
	}
}

func (s *scriptScanner) scanNumber() *applyError {
	start := s.pos
	if s.peek() == '-' {
		s.pos++
	}
	digits := 0
	for s.pos < len(s.data) && s.data[s.pos] >= '0' && s.data[s.pos] <= '9' {
		s.pos++
		digits++
	}
	if digits == 0 {
		return applyErr("invalid-field-syntax", "number requires digits")
	}
	if s.pos < len(s.data) && s.peek() == '.' {
		s.pos++
		frac := 0
		for s.pos < len(s.data) && s.data[s.pos] >= '0' && s.data[s.pos] <= '9' {
			s.pos++
			frac++
		}
		if frac == 0 {
			return applyErr("invalid-field-syntax", "fraction part requires digits")
		}
	}
	if s.pos < len(s.data) && (s.peek() == 'e' || s.peek() == 'E') {
		s.pos++
		if s.pos < len(s.data) && (s.peek() == '+' || s.peek() == '-') {
			s.pos++
		}
		exp := 0
		for s.pos < len(s.data) && s.data[s.pos] >= '0' && s.data[s.pos] <= '9' {
			s.pos++
			exp++
		}
		if exp == 0 {
			return applyErr("invalid-field-syntax", "exponent requires digits")
		}
	}
	if n := s.pos - start; n > persist.LimitNumberTokenChars { // L12
		return applyErr("limit-number-token", "number literal of %d characters exceeds %d", n, persist.LimitNumberTokenChars)
	}
	return nil
}

// scanString consumes one JSON string, enforcing control-character
// rejection and basic escape shape; deep Unicode-escape validation is
// delegated to encoding/json on the already-bounded input.
func (s *scriptScanner) scanString() *applyError {
	_, err := s.scanStringRaw()
	return err
}

func (s *scriptScanner) scanStringRaw() (string, *applyError) {
	s.pos++ // consume opening quote
	start := s.pos
	for s.pos < len(s.data) {
		c := s.data[s.pos]
		switch {
		case c == '"':
			out := string(s.data[start:s.pos])
			s.pos++
			return out, nil
		case c == '\\':
			s.pos++
			if s.pos >= len(s.data) {
				break
			}
			e := s.data[s.pos]
			if e == 'u' {
				if s.pos+4 >= len(s.data) {
					return "", applyErr("invalid-field-syntax", "truncated unicode escape")
				}
				s.pos += 5
				continue
			}
			switch e {
			case '"', '\\', '/', 'b', 'f', 'n', 'r', 't':
				s.pos++
				continue
			}
			return "", applyErr("invalid-field-syntax", "invalid escape character %q", e)
		case c < 0x20:
			return "", applyErr("invalid-field-syntax", "control character in string")
		default:
			s.pos++
		}
	}
	return "", applyErr("invalid-field-syntax", "unterminated string")
}

// parseScript decodes the closed-member external script schema over
// scanner-validated bytes, normalizes every operation through the
// document constructors (phase 1), and enforces the L23 operations-per-
// batch cap at parse time.
func parseScript(data []byte) ([]*scriptBatch, *applyError) {
	if err := scanLimitedJSON(data); err != nil {
		return nil, err
	}
	var top map[string]json.RawMessage
	if err := json.Unmarshal(data, &top); err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "script is not a valid JSON document: %v", err)
	}
	if len(top) != 1 {
		return nil, applyErr(codeInvalidFieldSyntax, "script top-level object must carry exactly the member \"batches\"")
	}
	rawBatches, ok := top["batches"]
	if !ok {
		return nil, applyErr(codeInvalidFieldSyntax, "script top-level object must carry exactly the member \"batches\"")
	}
	var rawList []json.RawMessage
	if err := json.Unmarshal(rawBatches, &rawList); err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "\"batches\" must be an array of batch objects")
	}
	if len(rawList) > persist.LimitBatchesPerPackage { // L23 batches cap, script side
		return nil, applyErr("limit-operation-count", "script declares %d batches, exceeding the %d envelope",
			len(rawList), persist.LimitBatchesPerPackage)
	}
	out := make([]*scriptBatch, 0, len(rawList))
	for i, rb := range rawList {
		b, berr := parseScriptBatch(i, rb)
		if berr != nil {
			return nil, berr
		}
		out = append(out, b)
	}
	return out, nil
}

func parseScriptBatch(index int, raw json.RawMessage) (*scriptBatch, *applyError) {
	var members map[string]json.RawMessage
	if err := json.Unmarshal(raw, &members); err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "batch %d must be a JSON object", index)
	}
	for name := range members {
		if spoofMembers[name] {
			return nil, applyErr(codeInvalidFieldSyntax,
				"batch %d carries forbidden member %q;%s", index, name, scriptSpoofHint)
		}
		if !batchMembers[name] {
			return nil, applyErr(codeInvalidFieldSyntax, "batch %d carries unknown member %q", index, name)
		}
	}
	rawOps, ok := members["operations"]
	if !ok {
		return nil, applyErr(codeInvalidFieldSyntax, "batch %d requires \"operations\"", index)
	}
	var opList []json.RawMessage
	if err := json.Unmarshal(rawOps, &opList); err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "batch %d \"operations\" must be an array", index)
	}
	if len(opList) == 0 {
		return nil, applyErr(codeMalformedFraming, "batch %d contains no operations; empty batches are malformed-framing rejects (B3)", index)
	}
	if len(opList) > persist.LimitOperationsPerBatch { // L23
		return nil, applyErr("limit-operation-count", "batch %d carries %d operations, exceeding the %d envelope",
			index, len(opList), persist.LimitOperationsPerBatch)
	}

	b := &scriptBatch{}
	if rawID, declared := members["batchId"]; declared {
		var idText string
		if err := json.Unmarshal(rawID, &idText); err != nil {
			return nil, applyErr(codeInvalidFieldSyntax, "batch %d \"batchId\" must be a UUID string", index)
		}
		id, perr := document.ParseUUID(idText)
		if perr != nil {
			return nil, applyErr(codeInvalidFieldSyntax, "batch %d \"batchId\" is not a canonical RFC 9562 UUID", index)
		}
		b.declaredID = id
		b.declaredStr = idText
	}
	b.operations = make([]*document.Operation, 0, len(opList))
	for j, ro := range opList {
		op, oerr := parseScriptOp(index, j, ro)
		if oerr != nil {
			return nil, oerr
		}
		b.operations = append(b.operations, op)
	}
	return b, nil
}

func parseScriptOp(batchIndex, opIndex int, raw json.RawMessage) (*document.Operation, *applyError) {
	where := fmt.Sprintf("batch %d operation %d", batchIndex, opIndex)
	var members map[string]json.RawMessage
	if err := json.Unmarshal(raw, &members); err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "%s must be a JSON object", where)
	}
	for name := range members {
		if spoofMembers[name] {
			return nil, applyErr(codeInvalidFieldSyntax,
				"%s carries forbidden member %q;%s", where, name, scriptSpoofHint)
		}
		if !opMembers[name] {
			return nil, applyErr(codeInvalidFieldSyntax, "%s carries unknown member %q", where, name)
		}
	}
	var idText, kindText, entityText string
	for name, dst := range map[string]*string{
		"operationId": &idText, "kind": &kindText, "entityId": &entityText,
	} {
		rawV, ok := members[name]
		if !ok {
			return nil, applyErr(codeInvalidFieldSyntax, "%s requires %q", where, name)
		}
		if err := json.Unmarshal(rawV, dst); err != nil {
			return nil, applyErr(codeInvalidFieldSyntax, "%s %q must be a string", where, name)
		}
	}
	opID, err := document.ParseUUID(idText)
	if err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "%s \"operationId\" is not a canonical RFC 9562 UUID (O1)", where)
	}
	entityID, err := document.ParseUUID(entityText)
	if err != nil {
		return nil, applyErr(codeInvalidFieldSyntax, "%s \"entityId\" is not a canonical RFC 9562 UUID", where)
	}

	_, hasPayload := members["payload"]
	_, hasFields := members["fields"]
	switch document.OpKind(kindText) {
	case document.OpCreateEntity:
		if !hasPayload {
			return nil, applyErr(codeInvalidFieldSyntax, "%s createEntity requires a complete \"payload\" entity object", where)
		}
		if hasFields {
			return nil, applyErr(codeInvalidFieldSyntax, "%s createEntity must not carry \"fields\"", where)
		}
		op, cerr := document.NewCreateOp(opID, members["payload"])
		if cerr != nil {
			return nil, wrapDocReject(where, cerr)
		}
		if op.EntityID != entityID {
			return nil, applyErr(codeInvalidFieldSyntax,
				"%s entityId %s does not match payload id %s", where, entityText, op.EntityID.String())
		}
		return op, nil
	case document.OpSetFields:
		if !hasFields {
			return nil, applyErr(codeInvalidFieldSyntax, "%s setFields requires a \"fields\" mapping", where)
		}
		if hasPayload {
			return nil, applyErr(codeInvalidFieldSyntax, "%s setFields must not carry \"payload\"", where)
		}
		var fields map[string]json.RawMessage
		if err := json.Unmarshal(members["fields"], &fields); err != nil || fields == nil {
			return nil, applyErr(codeInvalidFieldSyntax,
				"%s setFields \"fields\" must be an object mapping top-level field names to values", where)
		}
		op, serr := document.NewSetFieldsOp(opID, entityID, fields)
		if serr != nil {
			return nil, wrapDocReject(where, serr)
		}
		return op, nil
	case document.OpDeleteEntity:
		if hasPayload || hasFields {
			return nil, applyErr(codeInvalidFieldSyntax, "%s deleteEntity carries no payload", where)
		}
		op, derr := document.NewDeleteOp(opID, entityID)
		if derr != nil {
			return nil, wrapDocReject(where, derr)
		}
		return op, nil
	default:
		return nil, applyErr(codeInvalidFieldSyntax,
			"%s unknown kind %q refuses safely before replay (O2)", where, kindText)
	}
}

// wrapDocReject renders a document-layer field rejection with its exact
// code preserved where available.
func wrapDocReject(where string, err error) *applyError {
	if fe, ok := err.(*document.FieldError); ok {
		return applyErr(fe.Code, "%s: %s", where, fe.Detail)
	}
	return applyErr(codeInvalidFieldSyntax, "%s: %v", where, err)
}

// canonicalOps normalizes ANY operation list — script or stored — into
// comparable form (Agreement v3 phase 2): createEntity payloads are
// re-encoded canonically through Decode/EncodeEntity, setFields payloads
// are rebuilt through the sorted-key constructor, deleteEntity stays
// empty. Formatting and member order therefore cannot make a semantically
// identical redelivery look different.
func canonicalOps(ops []*document.Operation) ([][]byte, *applyError) {
	out := make([][]byte, 0, len(ops))
	for _, op := range ops {
		var payload []byte
		switch op.Kind {
		case document.OpCreateEntity:
			ent, err := document.DecodeEntity(op.Payload)
			if err != nil {
				return nil, applyErr(codeInvalidFieldSyntax, "stored createEntity payload fails validation: %v", err)
			}
			raw, err := document.EncodeEntity(ent)
			if err != nil {
				return nil, applyErr(codeInvalidFieldSyntax, "stored createEntity payload cannot be canonicalized: %v", err)
			}
			payload = raw
		case document.OpSetFields:
			var fields map[string]json.RawMessage
			if err := json.Unmarshal(op.Payload, &fields); err != nil {
				return nil, applyErr(codeInvalidFieldSyntax, "stored setFields payload is not a field mapping")
			}
			rebuilt, err := document.NewSetFieldsOp(op.OperationID, op.EntityID, fields)
			if err != nil {
				return nil, applyErr(codeInvalidFieldSyntax, "stored setFields payload cannot be canonicalized: %v", err)
			}
			payload = rebuilt.Payload
		case document.OpDeleteEntity:
			payload = nil
		default:
			return nil, applyErr(codeInvalidFieldSyntax, "unknown operation kind %q", string(op.Kind))
		}
		fingerprint, merr := json.Marshal([]any{
			op.OperationID.String(), string(op.Kind), op.EntityID.String(), string(payload),
		})
		if merr != nil {
			return nil, applyErr(codeInvalidFieldSyntax, "operation fingerprint failed")
		}
		out = append(out, fingerprint)
	}
	return out, nil
}

// sameCommandContent compares two normalized operation lists by count and
// per-index canonical fingerprint. Stored-batch actor/counter/frontier are
// deliberately excluded: they are system-assigned metadata, not command
// content (Agreement v3 phase 2 definition).
func sameCommandContent(a, b []*document.Operation) bool {
	if len(a) != len(b) {
		return false
	}
	fa, aerr := canonicalOps(a)
	if aerr != nil {
		return false
	}
	fb, berr := canonicalOps(b)
	if berr != nil {
		return false
	}
	for i := range fa {
		if string(fa[i]) != string(fb[i]) {
			return false
		}
	}
	return true
}
