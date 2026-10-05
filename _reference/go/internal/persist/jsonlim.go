package persist

import (
	"strconv"
)

// jsonlim.go: a bounded recursive-descent JSON reader enforcing the
// parser-security rows of FORMAT-SPEC §9.3 during parsing, before value
// allocation where the row demands it:
//
//	L11 nesting depth (depth == 128 parses, > rejects)
//	L12 number literal token length
//	L13 non-finite literals reject nonfinite-value; float overflow and
//	    integer-typed values beyond signed 64-bit reject numeric-overflow
//	L17 array element count
//	L18 object member count
//	duplicate object member names reject invalid-field-syntax
//
// It exists because encoding/json cannot surface these classifications
// deterministically (e.g. NaN/Infinity arrive as syntax errors there).
// The accepted grammar is RFC 8259 with the two extensions the corpus
// pins as distinct classes: NaN / Infinity / -Infinity literals parse
// far enough to classify as nonfinite-value, and integers keep exact
// int64 semantics.

type jsonValue interface{}

type jsonObject struct {
	keys   []string
	values map[string]jsonValue
	// raws captures each member's exact source bytes so opaque payloads
	// (entity objects, setFields field values) round-trip value-faithfully
	// without re-serialization drift (§3.3 consequence discipline).
	raws map[string][]byte
}

func (o *jsonObject) get(key string) (jsonValue, bool) {
	v, ok := o.values[key]
	return v, ok
}

type jsonArray []jsonValue

type jsonString string

// jsonInt keeps exact signed 64-bit semantics for integer-typed fields.
type jsonInt int64

// jsonFloat marks a number that used a fraction or exponent form.
type jsonFloat float64

type jsonParser struct {
	buf   []byte
	pos   int
	depth int
}

const (
	maxJSONDepth        = LimitJSONDepth         // row L11
	maxNumberTokenChars = LimitNumberTokenChars  // row L12
	maxArrayElements    = LimitArrayElementCount // row L17
	maxObjectMembers    = LimitObjectMemberCount // row L18
)

func parseLimitedJSON(data []byte) (jsonValue, *Error) {
	p := &jsonParser{buf: data}
	p.skipWS()
	v, err := p.parseValue()
	if err != nil {
		return nil, err
	}
	p.skipWS()
	if p.pos != len(p.buf) {
		return nil, fail(CatInvalidFieldSyntax, StageSchema,
			"trailing content after JSON document at byte %d", p.pos)
	}
	return v, nil
}

func (p *jsonParser) skipWS() {
	for p.pos < len(p.buf) {
		switch p.buf[p.pos] {
		case ' ', '\t', '\n', '\r':
			p.pos++
		default:
			return
		}
	}
}

func (p *jsonParser) parseValue() (jsonValue, *Error) {
	if p.pos >= len(p.buf) {
		return nil, fail(CatInvalidFieldSyntax, StageSchema, "unexpected end of JSON document")
	}
	switch c := p.buf[p.pos]; {
	case c == '{':
		return p.parseObject()
	case c == '[':
		return p.parseArray()
	case c == '"':
		s, err := p.parseString()
		if err != nil {
			return nil, err
		}
		return jsonString(s), nil
	case c == 't':
		if err := p.expectLiteral("true"); err != nil {
			return nil, err
		}
		return jsonBool(true), nil
	case c == 'f':
		if err := p.expectLiteral("false"); err != nil {
			return nil, err
		}
		return jsonBool(false), nil
	case c == 'n':
		if err := p.expectLiteral("null"); err != nil {
			return nil, err
		}
		return jsonNull{}, nil
	case c == 'N' || c == 'I':
		if err := p.expectNonFinite(); err != nil {
			return nil, err
		}
		return nil, fail(CatNonfiniteValue, StageSchema,
			"non-finite literal rejected everywhere (L13)")
	case c == '-' || c == '+':
		if p.pos+1 < len(p.buf) && (p.buf[p.pos+1] == 'I' || p.buf[p.pos+1] == 'N') {
			if err := p.expectNonFinite(); err != nil {
				return nil, err
			}
			return nil, fail(CatNonfiniteValue, StageSchema,
				"non-finite literal rejected everywhere (L13)")
		}
		return p.parseNumber()
	default:
		if c == '-' || (c >= '0' && c <= '9') {
			return p.parseNumber()
		}
		return nil, fail(CatInvalidFieldSyntax, StageSchema,
			"invalid character %q at byte %d", string(rune(c)), p.pos)
	}
}

type jsonBool bool
type jsonNull struct{}

func (p *jsonParser) expectLiteral(lit string) *Error {
	if p.pos+len(lit) > len(p.buf) || string(p.buf[p.pos:p.pos+len(lit)]) != lit {
		return fail(CatInvalidFieldSyntax, StageSchema, "malformed literal at byte %d", p.pos)
	}
	p.pos += len(lit)
	return nil
}

func (p *jsonParser) expectNonFinite() *Error {
	for _, lit := range []string{"NaN", "Infinity", "-Infinity", "+Infinity"} {
		if p.pos+len(lit) <= len(p.buf) && string(p.buf[p.pos:p.pos+len(lit)]) == lit {
			p.pos += len(lit)
			return fail(CatNonfiniteValue, StageSchema, "%s rejected (L13)", lit)
		}
	}
	return fail(CatInvalidFieldSyntax, StageSchema, "invalid token at byte %d", p.pos)
}

func (p *jsonParser) parseNumber() (jsonValue, *Error) {
	start := p.pos
	if p.pos < len(p.buf) && p.buf[p.pos] == '-' {
		p.pos++
	}
	intDigits := 0
	for p.pos < len(p.buf) && p.buf[p.pos] >= '0' && p.buf[p.pos] <= '9' {
		p.pos++
		intDigits++
	}
	if intDigits == 0 {
		return nil, fail(CatInvalidFieldSyntax, StageSchema, "number without digits at byte %d", start)
	}
	isInt := true
	if p.pos < len(p.buf) && p.buf[p.pos] == '.' {
		isInt = false
		p.pos++
		frac := 0
		for p.pos < len(p.buf) && p.buf[p.pos] >= '0' && p.buf[p.pos] <= '9' {
			p.pos++
			frac++
		}
		if frac == 0 {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "number with empty fraction at byte %d", start)
		}
	}
	if p.pos < len(p.buf) && (p.buf[p.pos] == 'e' || p.buf[p.pos] == 'E') {
		isInt = false
		p.pos++
		if p.pos < len(p.buf) && (p.buf[p.pos] == '+' || p.buf[p.pos] == '-') {
			p.pos++
		}
		exp := 0
		for p.pos < len(p.buf) && p.buf[p.pos] >= '0' && p.buf[p.pos] <= '9' {
			p.pos++
			exp++
		}
		if exp == 0 {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "number with empty exponent at byte %d", start)
		}
	}
	tokenLen := p.pos - start
	if tokenLen > maxNumberTokenChars {
		return nil, fail(CatLimitNumberToken, StageSchema,
			"number token of %d characters exceeds %d (L12)", tokenLen, maxNumberTokenChars)
	}
	tok := string(p.buf[start:p.pos])
	if isInt {
		var v int64
		var neg bool
		t := tok
		if t[0] == '-' {
			neg = true
			t = t[1:]
		}
		const maxAbs = uint64(1) << 63
		var u uint64
		for i := 0; i < len(t); i++ {
			d := uint64(t[i] - '0')
			if u > (maxAbs-d)/10 {
				return p.numberFallback(tok)
			}
			u = u*10 + d
		}
		if neg {
			if u > maxAbs {
				return nil, fail(CatNumericOverflow, StageSchema, "integer out of range (L13)")
			}
			v = -int64(u-1) - 1
			if u == maxAbs {
				v = -9223372036854775808
			}
		} else {
			if u >= maxAbs {
				return nil, fail(CatNumericOverflow, StageSchema, "integer out of range (L13)")
			}
			v = int64(u)
		}
		return jsonInt(v), nil
	}
	f, ok := parseFloat(tok)
	if !ok || f != f || f > 1.7976931348623157e308 || f < -1.7976931348623157e308 {
		return nil, fail(CatNumericOverflow, StageSchema,
			"number %s outside IEEE-754 binary64 range (L13)", tok)
	}
	return jsonFloat(f), nil
}

// numberFallback classifies integer-syntax tokens beyond signed 64-bit:
// coordinates and other number-domain fields accept them when they remain
// finite binary64 values (L13 binds INTEGER-TYPED fields, which enforce
// their own numeric-overflow at the semantic layer); only float-range
// overflow stays a parse-time numeric-overflow.
func (p *jsonParser) numberFallback(tok string) (jsonValue, *Error) {
	f, ok := parseFloat(tok)
	if !ok || f != f || f > 1.7976931348623157e308 || f < -1.7976931348623157e308 {
		return nil, fail(CatNumericOverflow, StageSchema,
			"number %s outside IEEE-754 binary64 range (L13)", tok)
	}
	return jsonFloat(f), nil
}

// parseFloat wraps strconv.ParseFloat: ErrRange yields the saturated ±Inf
// value with ok=true so the caller can classify it as numeric-overflow
// (L13 float-range rejection) rather than a syntax error.
func parseFloat(tok string) (float64, bool) {
	f, err := strconv.ParseFloat(tok, 64)
	if err != nil {
		if ne, ok := err.(*strconv.NumError); ok && ne.Err == strconv.ErrRange {
			return f, true
		}
		return 0, false
	}
	return f, true
}

func (p *jsonParser) parseObject() (jsonValue, *Error) {
	if p.depth+1 > maxJSONDepth {
		return nil, fail(CatLimitNestingDepth, StageSchema,
			"nesting depth would exceed %d levels (L11)", maxJSONDepth)
	}
	p.depth++
	defer func() { p.depth-- }()
	p.pos++ // '{'
	obj := &jsonObject{values: map[string]jsonValue{}, raws: map[string][]byte{}}
	p.skipWS()
	if p.pos < len(p.buf) && p.buf[p.pos] == '}' {
		p.pos++
		return obj, nil
	}
	for {
		p.skipWS()
		if p.pos >= len(p.buf) || p.buf[p.pos] != '"' {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "object key expected at byte %d", p.pos)
		}
		key, err := p.parseString()
		if err != nil {
			return nil, err
		}
		if _, dup := obj.values[key]; dup {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"duplicate object member %q", key)
		}
		if len(obj.keys)+1 > maxObjectMembers {
			return nil, fail(CatLimitMemberCount, StageSchema,
				"object member count exceeds %d (L18)", maxObjectMembers)
		}
		p.skipWS()
		if p.pos >= len(p.buf) || p.buf[p.pos] != ':' {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "colon expected at byte %d", p.pos)
		}
		p.pos++
		p.skipWS()
		valueStart := p.pos
		v, err := p.parseValue()
		if err != nil {
			return nil, err
		}
		obj.keys = append(obj.keys, key)
		obj.values[key] = v
		obj.raws[key] = p.buf[valueStart:p.pos]
		p.skipWS()
		if p.pos >= len(p.buf) {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "unterminated object")
		}
		switch p.buf[p.pos] {
		case ',':
			p.pos++
		case '}':
			p.pos++
			return obj, nil
		default:
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"unexpected character %q in object at byte %d", string(rune(p.buf[p.pos])), p.pos)
		}
	}
}

func (p *jsonParser) parseArray() (jsonValue, *Error) {
	if p.depth+1 > maxJSONDepth {
		return nil, fail(CatLimitNestingDepth, StageSchema,
			"nesting depth would exceed %d levels (L11)", maxJSONDepth)
	}
	p.depth++
	defer func() { p.depth-- }()
	p.pos++ // '['
	arr := jsonArray{}
	p.skipWS()
	if p.pos < len(p.buf) && p.buf[p.pos] == ']' {
		p.pos++
		return arr, nil
	}
	for {
		p.skipWS()
		v, err := p.parseValue()
		if err != nil {
			return nil, err
		}
		arr = append(arr, v)
		if len(arr) > maxArrayElements {
			return nil, fail(CatLimitArrayCount, StageSchema,
				"array element count exceeds %d (L17)", maxArrayElements)
		}
		p.skipWS()
		if p.pos >= len(p.buf) {
			return nil, fail(CatInvalidFieldSyntax, StageSchema, "unterminated array")
		}
		switch p.buf[p.pos] {
		case ',':
			p.pos++
		case ']':
			p.pos++
			return arr, nil
		default:
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"unexpected character %q in array at byte %d", string(rune(p.buf[p.pos])), p.pos)
		}
	}
}

func (p *jsonParser) parseString() (string, *Error) {
	p.pos++ // opening quote
	var out []byte
	for {
		if p.pos >= len(p.buf) {
			return "", fail(CatInvalidFieldSyntax, StageSchema, "unterminated string")
		}
		c := p.buf[p.pos]
		switch {
		case c == '"':
			p.pos++
			return string(out), nil
		case c == '\\':
			p.pos++
			if p.pos >= len(p.buf) {
				return "", fail(CatInvalidFieldSyntax, StageSchema, "unterminated escape")
			}
			e := p.buf[p.pos]
			p.pos++
			switch e {
			case '"', '\\', '/':
				out = append(out, e)
			case 'b':
				out = append(out, '\b')
			case 'f':
				out = append(out, '\f')
			case 'n':
				out = append(out, '\n')
			case 'r':
				out = append(out, '\r')
			case 't':
				out = append(out, '\t')
			case 'u':
				r, err := p.parseUnicodeEscape()
				if err != nil {
					return "", err
				}
				out = utf16Append(out, r)
			default:
				return "", fail(CatInvalidFieldSyntax, StageSchema, "invalid escape \\%c", e)
			}
		case c < 0x20:
			return "", fail(CatInvalidFieldSyntax, StageSchema, "control character in string")
		default:
			out = append(out, c)
			p.pos++
		}
	}
}

func (p *jsonParser) parseUnicodeEscape() (uint32, *Error) {
	v, err := p.hex4()
	if err != nil {
		return 0, err
	}
	if v >= 0xD800 && v <= 0xDBFF && p.pos+1 < len(p.buf) && p.buf[p.pos] == '\\' && p.buf[p.pos+1] == 'u' {
		p.pos += 2
		lo, err := p.hex4()
		if err != nil {
			return 0, err
		}
		if lo >= 0xDC00 && lo <= 0xDFFF {
			return 0x10000 + (v-0xD800)<<10 + (lo - 0xDC00), nil
		}
		return 0xFFFD, nil
	}
	if v >= 0xD800 && v <= 0xDFFF {
		return 0xFFFD, nil
	}
	return v, nil
}

func (p *jsonParser) hex4() (uint32, *Error) {
	if p.pos+4 > len(p.buf) {
		return 0, fail(CatInvalidFieldSyntax, StageSchema, "truncated unicode escape")
	}
	var v uint32
	for i := 0; i < 4; i++ {
		c := p.buf[p.pos+i]
		var d uint32
		switch {
		case c >= '0' && c <= '9':
			d = uint32(c - '0')
		case c >= 'a' && c <= 'f':
			d = uint32(c-'a') + 10
		case c >= 'A' && c <= 'F':
			d = uint32(c-'A') + 10
		default:
			return 0, fail(CatInvalidFieldSyntax, StageSchema, "invalid unicode escape digit")
		}
		v = v<<4 | d
	}
	p.pos += 4
	return v, nil
}

func utf16Append(out []byte, r uint32) []byte {
	if r < 0x10000 {
		return appendRune(out, rune(r))
	}
	return appendRune(out, rune(r))
}

func appendRune(out []byte, r rune) []byte {
	var tmp [4]byte
	n := encodeRune(tmp[:], r)
	return append(out, tmp[:n]...)
}

func encodeRune(b []byte, r rune) int {
	switch {
	case r < 0x80:
		b[0] = byte(r)
		return 1
	case r < 0x800:
		b[0] = 0xC0 | byte(r>>6)
		b[1] = 0x80 | byte(r)&0x3F
		return 2
	case r < 0x10000:
		b[0] = 0xE0 | byte(r>>12)
		b[1] = 0x80 | byte(r>>6)&0x3F
		b[2] = 0x80 | byte(r)&0x3F
		return 3
	default:
		b[0] = 0xF0 | byte(r>>18)
		b[1] = 0x80 | byte(r>>12)&0x3F
		b[2] = 0x80 | byte(r>>6)&0x3F
		b[3] = 0x80 | byte(r)&0x3F
		return 4
	}
}
