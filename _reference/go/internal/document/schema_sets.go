package document

import (
	"encoding/json"
	"fmt"
	"math"
)

// jsonNull reports whether raw is the JSON null literal.
func jsonNull(raw json.RawMessage) bool {
	return string(raw) == "null"
}

// nonFinite reports whether v is NaN or an infinity; such values reject
// everywhere (FORMAT-SPEC §3, L13).
func nonFinite(v float64) bool {
	return math.IsNaN(v) || math.IsInf(v, 0)
}

// Canonical-order set encodings of FORMAT-SPEC §2.2 C3: validators treat
// permutations as equivalent, sort into the declared canonical order, and
// reject duplicate elements deterministically because each sort key covers
// every serialized field of the element.

func decodeStyleRefs(raw json.RawMessage) ([]string, error) {
	var toks []string
	if err := json.Unmarshal(raw, &toks); err != nil {
		return nil, fieldErr("", codeInvalidFieldSyntax, "styleRefs must be an array of tokens")
	}
	out := append([]string(nil), toks...)
	ok := sortSetElements(out, compareByteString)
	if !ok {
		return nil, fieldErr("", codeDuplicateElement, "duplicate style reference token")
	}
	for _, t := range out {
		if t == "" {
			return nil, fieldErr("", codeEmptyStyleToken, "style token must be non-empty")
		}
	}
	return out, nil
}

// compareMembers orders by ascending member UUID (E5), ties by role token
// byte order (C3).
func compareMembers(a, b WallMember) int {
	if c := a.WallID.Compare(b.WallID); c != 0 {
		return c
	}
	return compareByteString(string(a.Role), string(b.Role))
}

// exactObject decodes raw as a JSON object whose member set is EXACTLY the
// given keys: missing required members and unknown nested shape both fail
// closed. Structured sub-objects of known fields are not top-level unknown
// data under §8; unrecognized members inside them are unrecognized
// required data and refuse safely.
func exactObject(raw json.RawMessage, where string, keys ...string) (map[string]json.RawMessage, error) {
	var obj map[string]json.RawMessage
	if err := json.Unmarshal(raw, &obj); err != nil {
		return nil, fieldErr(where, codeInvalidFieldSyntax, "%s must be an object", where)
	}
	allowed := make(map[string]struct{}, len(keys))
	for _, k := range keys {
		allowed[k] = struct{}{}
		if _, ok := obj[k]; !ok {
			return nil, fieldErr(where+"."+k, codeMissingRequired, "required member absent")
		}
	}
	names := make([]string, 0, len(obj))
	for k := range obj {
		if _, ok := allowed[k]; !ok {
			names = append(names, k)
		}
	}
	if len(names) > 0 {
		sortStrings(names)
		return nil, fieldErr(where+"."+names[0], codeUnknownField,
			"unrecognized member %q in %s refuses safely", names[0], where)
	}
	return obj, nil
}

func sortStrings(s []string) {
	for i := 1; i < len(s); i++ {
		for j := i; j > 0 && s[j] < s[j-1]; j-- {
			s[j], s[j-1] = s[j-1], s[j]
		}
	}
}

func reqMembers(obj map[string]json.RawMessage, name string) ([]WallMember, error) {
	raw, ok := obj[name]
	if !ok {
		return nil, fieldErr(name, codeMissingRequired, "required field absent")
	}
	var entries []json.RawMessage
	if err := json.Unmarshal(raw, &entries); err != nil {
		return nil, fieldErr(name, codeInvalidFieldSyntax, "members must be an array of {wallId, role}")
	}
	members := make([]WallMember, 0, len(entries))
	for i, entry := range entries {
		mo, err := exactObject(entry, "members", "wallId", "role")
		if err != nil {
			return nil, wrapField(name, prefixIndex(err, i))
		}
		var idText string
		if err := json.Unmarshal(mo["wallId"], &idText); err != nil {
			return nil, fieldErr(name, codeInvalidFieldSyntax, "entry %d: wallId must be a string", i)
		}
		id, err := ParseUUID(idText)
		if err != nil {
			return nil, fieldErr(name, codeInvalidFieldSyntax, "entry %d: wallId %q", i, idText)
		}
		var role string
		if err := json.Unmarshal(mo["role"], &role); err != nil {
			return nil, fieldErr(name, codeInvalidFieldSyntax, "entry %d: role must be a string", i)
		}
		switch WallRole(role) {
		case RoleStart, RoleEnd:
		default:
			return nil, fieldErr(name, codeInvalidMembership, "entry %d: role %q not in {Start, End}", i, role)
		}
		members = append(members, WallMember{WallID: id, Role: WallRole(role)})
	}
	if len(members) < 2 {
		return nil, fieldErr(name, codeInvalidMembership, "membership needs at least 2 unique entries, got %d", len(members))
	}
	if !sortSetElements(members, compareMembers) {
		return nil, fieldErr(name, codeDuplicateElement, "duplicate membership entry")
	}
	for i := 1; i < len(members); i++ {
		if members[i].WallID == members[i-1].WallID {
			return nil, fieldErr(name, codeInvalidMembership,
				"both endpoints of one wall prohibited (wall %s)", members[i].WallID.String())
		}
	}
	return members, nil
}

func prefixIndex(err error, i int) error {
	if fe, ok := err.(*FieldError); ok {
		fe.Detail = fmt.Sprintf("entry %d: %s", i, fe.Detail)
		return fe
	}
	return err
}

// compareBackgroundImages is the C3 canonical order for elements without a
// member UUID: lexicographic tuple (digest, stackOrdinal, x, y,
// scaleUniform, rotationDeg); digest ascending as a byte string, numeric
// components under the IEEE-754 total order for binary64 after negative-
// zero canonicalization (which decodeBackgroundImages applies).
func compareBackgroundImages(a, b BackgroundImage) int {
	if c := compareByteString(a.Digest, b.Digest); c != 0 {
		return c
	}
	if a.StackOrdinal != b.StackOrdinal {
		if a.StackOrdinal < b.StackOrdinal {
			return -1
		}
		return 1
	}
	if c := compareFloatTotal(a.Anchor.TranslationMm[0], b.Anchor.TranslationMm[0]); c != 0 {
		return c
	}
	if c := compareFloatTotal(a.Anchor.TranslationMm[1], b.Anchor.TranslationMm[1]); c != 0 {
		return c
	}
	if c := compareFloatTotal(a.Anchor.ScaleUniform, b.Anchor.ScaleUniform); c != 0 {
		return c
	}
	return compareFloatTotal(a.Anchor.RotationDeg, b.Anchor.RotationDeg)
}

func decodeBackgroundImages(raw json.RawMessage) ([]BackgroundImage, error) {
	var entries []json.RawMessage
	if err := json.Unmarshal(raw, &entries); err != nil {
		return nil, fieldErr("", codeInvalidFieldSyntax, "backgroundImages must be an array")
	}
	imgs := make([]BackgroundImage, 0, len(entries))
	for i, entry := range entries {
		eo, err := exactObject(entry, "backgroundImages", "digest", "anchor", "stackOrdinal")
		if err != nil {
			return nil, wrapField(FieldBackgroundImgs, prefixIndex(err, i))
		}
		var digest string
		if err := json.Unmarshal(eo["digest"], &digest); err != nil {
			return nil, fieldErr("digest", codeInvalidDigest, "entry %d: must be a string", i)
		}
		if !digestSyntax(digest) {
			return nil, fieldErr("digest", codeInvalidDigest, "entry %d: %q", i, digest)
		}
		var ordinal int64
		if err := json.Unmarshal(eo["stackOrdinal"], &ordinal); err != nil {
			return nil, fieldErr("stackOrdinal", codeInvalidFieldSyntax,
				"entry %d: stackOrdinal must be an integer fitting signed 64-bit", i)
		}
		if ordinal < 0 {
			return nil, fieldErr("stackOrdinal", codeInvalidFieldSyntax, "entry %d: stackOrdinal must be >= 0", i)
		}
		ao, err := exactObject(eo["anchor"], "anchor",
			"translationMm", "scaleUniform", "rotationDeg")
		if err != nil {
			return nil, wrapField(FieldBackgroundImgs+".anchor", prefixIndex(err, i))
		}
		tr, err := reqCoordPair(ao, "translationMm")
		if err != nil {
			return nil, wrapField(FieldBackgroundImgs+".anchor.translationMm", prefixIndex(err, i))
		}
		scale, err := finiteNumber(ao["scaleUniform"], "scaleUniform")
		if err != nil {
			return nil, wrapField(FieldBackgroundImgs+".anchor.scaleUniform", prefixIndex(err, i))
		}
		if scale <= ScaleUniformMin {
			return nil, fieldErr(FieldBackgroundImgs+".anchor.scaleUniform", codeDimOutOfRange,
				"entry %d: scaleUniform must be > 0 and finite", i)
		}
		rot, err := finiteNumber(ao["rotationDeg"], "rotationDeg")
		if err != nil {
			return nil, wrapField(FieldBackgroundImgs+".anchor.rotationDeg", prefixIndex(err, i))
		}
		imgs = append(imgs, BackgroundImage{
			Digest:       digest,
			Anchor:       Anchor{TranslationMm: tr, ScaleUniform: scale, RotationDeg: rot},
			StackOrdinal: ordinal,
		})
	}
	if !sortSetElements(imgs, compareBackgroundImages) {
		return nil, fieldErr("", codeDuplicateElement, "duplicate backgroundImages entry")
	}
	return imgs, nil
}

func decodeTransform(raw json.RawMessage) (Transform, error) {
	var t Transform
	to, err := exactObject(raw, "transform", "translationMm", "scaleUniform", "rotationDeg")
	if err != nil {
		return t, err
	}
	tr, err := reqCoordPair(to, "translationMm")
	if err != nil {
		return t, wrapField("transform.translationMm", err)
	}
	scale, err := finiteNumber(to["scaleUniform"], "scaleUniform")
	if err != nil {
		return t, wrapField("transform.scaleUniform", err)
	}
	if scale <= ScaleUniformMin {
		return t, fieldErr("transform.scaleUniform", codeDimOutOfRange, "scaleUniform must be > 0 and finite")
	}
	rot, err := finiteNumber(to["rotationDeg"], "rotationDeg")
	if err != nil {
		return t, wrapField("transform.rotationDeg", err)
	}
	t = Transform{TranslationMm: tr, ScaleUniform: scale, RotationDeg: rot}
	return t, nil
}
