package document

import (
	"encoding/json"
	"errors"
	"math"
)

// This file holds the shared per-field validators behind the §2.3 schemas.
// Every magnitude bound is expressed through the named provisional
// parameters of params.go; no bound literal appears here (ED-0016).

func wrapField(field string, err error) error {
	if fe, ok := err.(*FieldError); ok {
		if fe.Field == "" {
			fe.Field = field
		}
		return fe
	}
	return &FieldError{Field: field, Code: codeInvalidFieldSyntax, Detail: err.Error()}
}

func reqString(obj map[string]json.RawMessage, name string) (string, error) {
	raw, ok := obj[name]
	if !ok {
		return "", fieldErr(name, codeMissingRequired, "required field absent")
	}
	if jsonNull(raw) {
		return "", fieldErr(name, codeInvalidFieldSyntax, "present-as-null is not a valid string")
	}
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return "", fieldErr(name, codeInvalidFieldSyntax, "must be a string")
	}
	return s, nil
}

func reqDisplayString(obj map[string]json.RawMessage, name string) (string, error) {
	s, err := reqString(obj, name)
	if err != nil {
		return "", err
	}
	if len(s) > DisplayStringMaxBytes {
		return "", fieldErr(name, codeInvalidFieldSyntax, "display string exceeds %d bytes", DisplayStringMaxBytes)
	}
	return s, nil
}

func optDisplayString(obj map[string]json.RawMessage, name string) (string, bool, error) {
	raw, ok := obj[name]
	if !ok {
		return "", false, nil
	}
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return "", false, fieldErr(name, codeInvalidFieldSyntax, "must be a string or null")
	}
	if string(raw) == "null" {
		return "", false, fieldErr(name, codeInvalidFieldSyntax, "present-as-null is not a serialized value; omit the field")
	}
	if len(s) > DisplayStringMaxBytes {
		return "", false, fieldErr(name, codeInvalidFieldSyntax, "display string exceeds %d bytes", DisplayStringMaxBytes)
	}
	return s, true, nil
}

func optStyleToken(obj map[string]json.RawMessage, name string) (string, bool, error) {
	raw, ok := obj[name]
	if !ok {
		return "", false, nil
	}
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return "", false, fieldErr(name, codeInvalidFieldSyntax, "must be a string")
	}
	if s == "" {
		return "", false, fieldErr(name, codeEmptyStyleToken, "style token must be non-empty")
	}
	return s, true, nil
}

// finiteNumber decodes one JSON number as a finite binary64 value with the
// -0 → +0 canonicalization of FORMAT-SPEC §3.2. encoding/json already
// rejects NaN/Infinity literals and magnitudes beyond binary64 range
// (L13); the finiteness check defends against non-JSON in-memory paths.
func finiteNumber(raw json.RawMessage, name string) (float64, error) {
	if jsonNull(raw) {
		return 0, fieldErr(name, codeInvalidFieldSyntax, "present-as-null is not a number")
	}
	var v float64
	if err := json.Unmarshal(raw, &v); err != nil {
		// FS §9.4: WRONG TYPES reject invalid-field-syntax; only genuine
		// magnitude problems are numeric-overflow.
		var typeErr *json.UnmarshalTypeError
		if errors.As(err, &typeErr) {
			return 0, fieldErr(name, codeInvalidFieldSyntax, "field must be a number")
		}
		return 0, fieldErr(name, codeNumericOverflow, "%v", err)
	}
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return 0, fieldErr(name, codeNonfiniteValue, "value must be finite")
	}
	return canonicalizeFloat(v), nil
}

// reqCoord validates an authoritative plan coordinate component:
// finite with absolute value at most CoordBoundMm.
func reqCoord(obj map[string]json.RawMessage, name string) (float64, error) {
	raw, ok := obj[name]
	if !ok {
		return 0, fieldErr(name, codeMissingRequired, "required field absent")
	}
	v, err := finiteNumber(raw, name)
	if err != nil {
		return 0, err
	}
	if v < -CoordBoundMm || v > CoordBoundMm {
		return 0, fieldErr(name, codeCoordOutOfRange, "|%v| exceeds coordBoundMm", v)
	}
	return v, nil
}

func reqCoordPair(obj map[string]json.RawMessage, name string) ([2]float64, error) {
	raw, ok := obj[name]
	if !ok {
		return [2]float64{}, fieldErr(name, codeMissingRequired, "required field absent")
	}
	var pair []float64
	if err := json.Unmarshal(raw, &pair); err != nil {
		return [2]float64{}, fieldErr(name, codeInvalidFieldSyntax, "must be a two-component coordinate array")
	}
	if len(pair) != 2 {
		return [2]float64{}, fieldErr(name, codeInvalidFieldSyntax, "coordinate must have exactly two components, got %d", len(pair))
	}
	out := [2]float64{}
	for i, c := range pair {
		f, err := finiteNumber(mustRaw(c), name)
		if err != nil {
			return out, err
		}
		if f < -CoordBoundMm || f > CoordBoundMm {
			return out, fieldErr(name, codeCoordOutOfRange, "component %d: |%v| exceeds coordBoundMm", i, f)
		}
		out[i] = f
	}
	return out, nil
}

func mustRaw(v float64) json.RawMessage {
	b, _ := json.Marshal(v)
	return b
}

// reqDimPositive validates fields in (0, dimensionCapMm].
func reqDimPositive(obj map[string]json.RawMessage, name string) (float64, error) {
	raw, ok := obj[name]
	if !ok {
		return 0, fieldErr(name, codeMissingRequired, "required field absent")
	}
	v, err := finiteNumber(raw, name)
	if err != nil {
		return 0, err
	}
	if v <= DimensionMin || v > DimensionCapMm {
		return 0, fieldErr(name, codeDimOutOfRange, "%v outside (0, dimensionCapMm]", v)
	}
	return v, nil
}

// reqDimNonNeg validates fields in [0, dimensionCapMm].
func reqDimNonNeg(obj map[string]json.RawMessage, name string) (float64, error) {
	raw, ok := obj[name]
	if !ok {
		return 0, fieldErr(name, codeMissingRequired, "required field absent")
	}
	v, err := finiteNumber(raw, name)
	if err != nil {
		return 0, err
	}
	if v < DimensionMin || v > DimensionCapMm {
		return 0, fieldErr(name, codeDimOutOfRange, "%v outside [0, dimensionCapMm]", v)
	}
	return v, nil
}

func reqUnitInterval(obj map[string]json.RawMessage, name string) (float64, error) {
	raw, ok := obj[name]
	if !ok {
		return 0, fieldErr(name, codeMissingRequired, "required field absent")
	}
	v, err := finiteNumber(raw, name)
	if err != nil {
		return 0, err
	}
	if v < OpeningUMin || v > OpeningUMax {
		return 0, fieldErr(name, codeInvalidFieldSyntax, "u must lie in [%v,%v], got %v", OpeningUMin, OpeningUMax, v)
	}
	return v, nil
}

func reqSwingDeg(obj map[string]json.RawMessage, name string) (float64, error) {
	raw, ok := obj[name]
	if !ok {
		return 0, fieldErr(name, codeMissingRequired, "required field absent")
	}
	v, err := finiteNumber(raw, name)
	if err != nil {
		return 0, err
	}
	if v < SwingDegMin || v > SwingDegMax {
		return 0, fieldErr(name, codeInvalidFieldSyntax, "swingDeg must lie in [%v,%v], got %v", SwingDegMin, SwingDegMax, v)
	}
	return v, nil
}

func reqUUIDRef(obj map[string]json.RawMessage, name string) (UUID, error) {
	raw, ok := obj[name]
	if !ok {
		return UUID{}, fieldErr(name, codeMissingRequired, "required field absent")
	}
	if jsonNull(raw) {
		return UUID{}, fieldErr(name, codeInvalidFieldSyntax, "present-as-null is not a reference")
	}
	var s string
	if err := json.Unmarshal(raw, &s); err != nil {
		return UUID{}, fieldErr(name, codeInvalidFieldSyntax, "reference must be a UUID string")
	}
	id, err := ParseUUID(s)
	if err != nil {
		return UUID{}, fieldErr(name, codeInvalidFieldSyntax, "%q", s)
	}
	return id, nil
}

func reqOpeningKind(obj map[string]json.RawMessage, name string) (OpeningKind, error) {
	s, err := reqString(obj, name)
	if err != nil {
		return "", err
	}
	switch OpeningKind(s) {
	case OpeningDoor, OpeningWindow:
		return OpeningKind(s), nil
	default:
		return "", fieldErr(name, codeInvalidOpeningKind, "%q not in {door, window}", s)
	}
}

func reqSide(obj map[string]json.RawMessage, name string) (Side, error) {
	s, err := reqString(obj, name)
	if err != nil {
		return "", err
	}
	switch Side(s) {
	case SideLeft, SideRight:
		return Side(s), nil
	default:
		return "", fieldErr(name, codeInvalidSide, "%q not in {Left, Right}", s)
	}
}

func reqHinge(obj map[string]json.RawMessage, name string) (Hinge, error) {
	s, err := reqString(obj, name)
	if err != nil {
		return "", err
	}
	switch Hinge(s) {
	case HingeNearStart, HingeNearEnd:
		return Hinge(s), nil
	default:
		return "", fieldErr(name, codeInvalidHinge, "%q not in {nearStart, nearEnd}", s)
	}
}

// digestSyntax enforces L19: exactly `sha256:` plus 64 lowercase hex
// characters, 71 bytes total (FIPS 180-4).
func digestSyntax(s string) bool {
	if len(s) != 71 || s[:7] != "sha256:" {
		return false
	}
	for i := 7; i < len(s); i++ {
		c := s[i]
		if !(c >= '0' && c <= '9') && !(c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}

func reqDigest(obj map[string]json.RawMessage, name string) (string, error) {
	s, err := reqString(obj, name)
	if err != nil {
		return "", err
	}
	if !digestSyntax(s) {
		return "", fieldErr(name, codeInvalidDigest, "digest must be sha256:<64 lowercase hex>")
	}
	return s, nil
}
