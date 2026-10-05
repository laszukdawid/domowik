package assets

import (
	"encoding/json"
	"fmt"
	"math"
	"net"
	"net/url"
	"strconv"
	"strings"
	"unicode/utf8"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// descriptor.go: the v0.1 descriptor schema of ASSET-STRATEGY §10.2.
// Required members: digest, byteLength, mediaType, purpose, encoding,
// requirement, provenance. Optional: license, placeholderDimensions,
// hints. Attribution metadata is never functional (§10.2); hint
// validation follows §10.5/§108's normative field semantics.

const (
	// EncodingIdentity is the v0.1 stored-form profile: the stored bytes
	// equal the logical blob bytes B verbatim (container compression
	// applies outside the hash domain, §10.1).
	EncodingIdentity = "identity"

	RequirementRequired = "required"
	RequirementOptional = "optional"

	// purposeMaxBytes bounds the purpose token. This is a v0.1 store
	// implementation bound (the spec fixes no magnitude for purpose);
	// it mirrors the L19 mediaType envelope and lives only here.
	purposeMaxBytes = 127

	displayMaxBytes = persist.DisplayStringMaxBytes // L19 display strings

	hintMaxEntries = 64 // L27 category scale; store-side metadata sanity cap
)

// Descriptor is one validated §10.2 document.
type Descriptor struct {
	Digest                string // full `sha256:` identifier
	HexDigest             string // bare 64-hex form
	ByteLength            uint64 // len(B)
	MediaType             string // type/subtype token
	Purpose               string // role token
	Encoding              string // exact-comparison string ("identity" in v0.1)
	Requirement           string // required | optional
	ProvenanceDisplayName string
	ProvenanceSource      string
	ProvenanceAuthor      string
	AcquiredAt            string // RFC 3339 UTC fixed form
	LicenseSPDX           string // optional; SPDX expression or URI form below
	LicenseURI            string
	LicenseCopyright      string
	PlaceholderWidthMm    float64 // optional; > 0 when present
	PlaceholderHeightMm   float64
	PlaceholderDepthMm    float64
	HasPlaceholder        bool
	Hints                 []Hint
}

// Hint is one advisory retrieval pointer (§10.5/§10.8). Timestamps are
// advisory freshness metadata only.
type Hint struct {
	URL            string
	AddedAt        string
	LastVerifiedAt string // optional
	ExpiresAt      string // optional
}

// BuildDescriptorJSON serializes a validated descriptor deterministically.
func BuildDescriptorJSON(d *Descriptor) ([]byte, error) {
	if err := validateDescriptor(d); err != nil {
		return nil, err
	}
	var b strings.Builder
	b.WriteString(`{"digest":`)
	writeJSONString(&b, d.Digest)
	fmt.Fprintf(&b, `,"byteLength":%d`, d.ByteLength)
	b.WriteString(`,"mediaType":`)
	writeJSONString(&b, d.MediaType)
	b.WriteString(`,"purpose":`)
	writeJSONString(&b, d.Purpose)
	b.WriteString(`,"encoding":`)
	writeJSONString(&b, d.Encoding)
	b.WriteString(`,"requirement":`)
	writeJSONString(&b, d.Requirement)
	b.WriteString(`,"provenance":{"displayName":`)
	writeJSONString(&b, d.ProvenanceDisplayName)
	b.WriteString(`,"sourceDescription":`)
	writeJSONString(&b, d.ProvenanceSource)
	b.WriteString(`,"author":`)
	writeJSONString(&b, d.ProvenanceAuthor)
	b.WriteString(`,"acquiredAt":`)
	writeJSONString(&b, d.AcquiredAt)
	if d.LicenseSPDX != "" || d.LicenseURI != "" || d.LicenseCopyright != "" {
		b.WriteString(`},"license":{`)
		first := true
		if d.LicenseSPDX != "" {
			b.WriteString(`"spdx":`)
			writeJSONString(&b, d.LicenseSPDX)
			first = false
		}
		if d.LicenseURI != "" {
			if !first {
				b.WriteByte(',')
			}
			b.WriteString(`"uri":`)
			writeJSONString(&b, d.LicenseURI)
			first = false
		}
		if d.LicenseCopyright != "" {
			if !first {
				b.WriteByte(',')
			}
			b.WriteString(`"copyright":`)
			writeJSONString(&b, d.LicenseCopyright)
		}
		b.WriteByte('}')
	} else {
		b.WriteByte('}')
	}
	if d.HasPlaceholder {
		fmt.Fprintf(&b, `,"placeholderDimensions":{"widthMm":%s,"heightMm":%s,"depthMm":%s}`,
			formatMm(d.PlaceholderWidthMm), formatMm(d.PlaceholderHeightMm), formatMm(d.PlaceholderDepthMm))
	}
	if len(d.Hints) > 0 {
		b.WriteString(`,"hints":[`)
		for i, h := range d.Hints {
			if i > 0 {
				b.WriteByte(',')
			}
			b.WriteString(`{"url":`)
			writeJSONString(&b, h.URL)
			b.WriteString(`,"addedAt":`)
			writeJSONString(&b, h.AddedAt)
			if h.LastVerifiedAt != "" {
				b.WriteString(`,"lastVerifiedAt":`)
				writeJSONString(&b, h.LastVerifiedAt)
			}
			if h.ExpiresAt != "" {
				b.WriteString(`,"expiresAt":`)
				writeJSONString(&b, h.ExpiresAt)
			}
			b.WriteByte('}')
		}
		b.WriteString(`]}`)
	}
	b.WriteByte('}')
	return []byte(b.String()), nil
}

// ParseDescriptor validates one §10.2 descriptor document.
func ParseDescriptor(raw []byte) (*Descriptor, error) {
	var top map[string]json.RawMessage
	dec := json.NewDecoder(strings.NewReader(string(raw)))
	dec.UseNumber()
	if err := dec.Decode(&top); err != nil {
		return nil, &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageSchema,
			Detail: "descriptor is not a JSON object: " + err.Error()}
	}
	d := &Descriptor{}
	var err error
	if d.Digest, err = requiredString(top, "digest"); err != nil {
		return nil, err
	}
	hex, herr := parseHexDigest(d.Digest)
	if herr != nil {
		return nil, herr
	}
	d.HexDigest = hex
	n, err := requiredNumber(top, "byteLength")
	if err != nil {
		return nil, err
	}
	ni, ierr := n.Int64()
	if ierr != nil || ni < 0 {
		return nil, invalidField("byteLength must be a non-negative integer")
	}
	d.ByteLength = uint64(ni)
	if d.MediaType, err = requiredString(top, "mediaType"); err != nil {
		return nil, err
	}
	if err := validateMediaType(d.MediaType); err != nil {
		return nil, err
	}
	if d.Purpose, err = requiredString(top, "purpose"); err != nil {
		return nil, err
	}
	if err := validateToken(d.Purpose); err != nil {
		return nil, err
	}
	if len(d.Purpose) > purposeMaxBytes {
		return nil, invalidField("purpose exceeds %d bytes", purposeMaxBytes)
	}
	if d.Encoding, err = requiredString(top, "encoding"); err != nil {
		return nil, err
	}
	if d.Encoding != EncodingIdentity {
		return nil, invalidField("unsupported encoding/profile %q; v0.1 admits %q only", d.Encoding, EncodingIdentity)
	}
	if d.Requirement, err = requiredString(top, "requirement"); err != nil {
		return nil, err
	}
	if d.Requirement != RequirementRequired && d.Requirement != RequirementOptional {
		return nil, invalidField("requirement must be required or optional")
	}
	provRaw, ok := top["provenance"]
	if !ok {
		return nil, invalidField("provenance block absent (required by §10.2)")
	}
	var prov map[string]json.RawMessage
	if jerr := json.Unmarshal(provRaw, &prov); jerr != nil || prov == nil {
		return nil, invalidField("provenance must be an object")
	}
	if d.ProvenanceDisplayName, err = boundedString(prov, "displayName"); err != nil {
		return nil, err
	}
	if d.ProvenanceSource, err = boundedString(prov, "sourceDescription"); err != nil {
		return nil, err
	}
	if d.ProvenanceAuthor, err = boundedString(prov, "author"); err != nil {
		return nil, err
	}
	if d.AcquiredAt, err = requiredTimestamp(prov, "acquiredAt"); err != nil {
		return nil, err
	}
	if licRaw, ok := top["license"]; ok {
		var lic map[string]json.RawMessage
		if jerr := json.Unmarshal(licRaw, &lic); jerr != nil || lic == nil {
			return nil, invalidField("license must be an object")
		}
		if d.LicenseSPDX, err = boundedString(lic, "spdx"); err != nil {
			return nil, err
		}
		if d.LicenseURI, err = boundedString(lic, "uri"); err != nil {
			return nil, err
		}
		if d.LicenseCopyright, err = boundedString(lic, "copyright"); err != nil {
			return nil, err
		}
		if (d.LicenseSPDX == "") == (d.LicenseURI == "") {
			return nil, invalidField("license carries exactly one of spdx or uri")
		}
		if len(d.LicenseURI) > persist.URIMaxBytes {
			return nil, invalidField("license uri exceeds %d bytes (L19)", persist.URIMaxBytes)
		}
	}
	if phRaw, ok := top["placeholderDimensions"]; ok {
		var ph map[string]json.RawMessage
		if jerr := json.Unmarshal(phRaw, &ph); jerr != nil || ph == nil {
			return nil, invalidField("placeholderDimensions must be an object")
		}
		w, werr := positiveDimension(ph, "widthMm")
		hgt, herr2 := positiveDimension(ph, "heightMm")
		dep, derr := positiveDimension(ph, "depthMm")
		if werr != nil || herr2 != nil || derr != nil {
			return nil, invalidField("placeholderDimensions values must be positive mm within dimensionCapMm")
		}
		d.PlaceholderWidthMm, d.PlaceholderHeightMm, d.PlaceholderDepthMm = w, hgt, dep
		d.HasPlaceholder = true
	}
	if hintsRaw, ok := top["hints"]; ok && string(hintsRaw) != "null" {
		var arr []json.RawMessage
		if jerr := json.Unmarshal(hintsRaw, &arr); jerr != nil {
			return nil, invalidField("hints must be an array")
		}
		if len(arr) > hintMaxEntries {
			return nil, invalidField("hints exceed %d entries", hintMaxEntries)
		}
		seen := map[string]bool{}
		for _, item := range arr {
			h, herr := parseHint(item)
			if herr != nil {
				return nil, herr
			}
			if seen[h.URL] {
				return nil, invalidField("duplicate hint URL %q (set semantics)", h.URL)
			}
			seen[h.URL] = true
			d.Hints = append(d.Hints, *h)
		}
	}
	return d, nil
}

func parseHint(item json.RawMessage) (*Hint, error) {
	var m map[string]json.RawMessage
	if jerr := json.Unmarshal(item, &m); jerr != nil || m == nil {
		return nil, invalidField("hint entries must be objects")
	}
	rawURL, err := requiredString(m, "url")
	if err != nil {
		return nil, err
	}
	u, perr := url.Parse(rawURL)
	if perr != nil || !u.IsAbs() {
		return nil, invalidField("hint URL must be an absolute URI")
	}
	if u.User != nil {
		return nil, invalidField("hint URL carries userinfo, which is invalid")
	}
	scheme := strings.ToLower(u.Scheme)
	for _, r := range scheme {
		if !(r >= 'a' && r <= 'z') && !(r >= '0' && r <= '9') && r != '+' && r != '-' && r != '.' {
			return nil, invalidField("hint URL scheme is not ASCII-normalizable")
		}
	}
	if scheme != "https" { // default allowlist ["https"] until configuration exists
		return nil, invalidField("hint URL scheme %q is outside the allowlist", u.Scheme)
	}
	host := strings.ToLower(strings.TrimSuffix(u.Hostname(), "."))
	if host == "" {
		return nil, invalidField("hint URL lacks a host")
	}
	if net.ParseIP(host) != nil {
		return nil, invalidField("hint host may not be an IP literal in v0.1")
	}
	h := &Hint{URL: rawURL}
	if h.AddedAt, err = requiredTimestamp(m, "addedAt"); err != nil {
		return nil, err
	}
	if _, ok := m["lastVerifiedAt"]; ok {
		if h.LastVerifiedAt, err = requiredTimestamp(m, "lastVerifiedAt"); err != nil {
			return nil, err
		}
	}
	if _, ok := m["expiresAt"]; ok {
		if h.ExpiresAt, err = requiredTimestamp(m, "expiresAt"); err != nil {
			return nil, err
		}
	}
	return h, nil
}

func validateDescriptor(d *Descriptor) error {
	if _, err := parseHexDigest(d.Digest); err != nil {
		return err
	}
	if err := validateMediaType(d.MediaType); err != nil {
		return err
	}
	if err := validateToken(d.Purpose); err != nil {
		return err
	}
	if len(d.Purpose) > purposeMaxBytes {
		return invalidField("purpose exceeds %d bytes", purposeMaxBytes)
	}
	if d.Encoding != EncodingIdentity {
		return invalidField("unsupported encoding/profile %q", d.Encoding)
	}
	if d.Requirement != RequirementRequired && d.Requirement != RequirementOptional {
		return invalidField("requirement must be required or optional")
	}
	for _, s := range []string{d.ProvenanceDisplayName, d.ProvenanceSource, d.ProvenanceAuthor} {
		if len(s) > displayMaxBytes {
			return invalidField("provenance string exceeds %d bytes (L19)", displayMaxBytes)
		}
	}
	if terr := persist.ValidateTimestamp(d.AcquiredAt); terr != nil {
		return terr
	}
	if d.HasPlaceholder {
		for _, v := range []float64{d.PlaceholderWidthMm, d.PlaceholderHeightMm, d.PlaceholderDepthMm} {
			if !(v > 0) || math.IsInf(v, 0) || v > document.DimensionCapMm {
				return invalidField("placeholder dimensions must be positive mm within dimensionCapMm")
			}
		}
	}
	return nil
}

func validateMediaType(s string) error {
	if len(s) > persist.MediaTypeMaxBytes {
		return invalidField("mediaType exceeds %d bytes (L19)", persist.MediaTypeMaxBytes)
	}
	major, minor, ok := strings.Cut(s, "/")
	if !ok || major == "" || minor == "" {
		return invalidField("mediaType must be a type/subtype token")
	}
	if err := validateToken(major); err != nil {
		return err
	}
	return validateToken(minor)
}

// validateToken enforces the RFC 2045 token character set over a UTF-8
// string without control characters.
func validateToken(s string) error {
	if !utf8.ValidString(s) {
		return invalidField("token is not valid UTF-8")
	}
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case c >= 'a' && c <= 'z', c >= 'A' && c <= 'Z', c >= '0' && c <= '9':
		case strings.IndexByte("!#$%&'*+-.^_`|~", c) >= 0:
		default:
			return invalidField("token contains character %q outside the token set", rune(c))
		}
	}
	return nil
}

func requiredString(top map[string]json.RawMessage, name string) (string, error) {
	v, ok := top[name]
	if !ok {
		return "", invalidField("%s absent", name)
	}
	var s string
	if jerr := json.Unmarshal(v, &s); jerr != nil {
		return "", invalidField("%s must be a string", name)
	}
	return s, nil
}

func boundedString(top map[string]json.RawMessage, name string) (string, error) {
	s, err := requiredString(top, name)
	if err != nil {
		return "", err
	}
	if len(s) > displayMaxBytes {
		return "", invalidField("%s exceeds %d bytes (L19)", name, displayMaxBytes)
	}
	return s, nil
}

func requiredTimestamp(top map[string]json.RawMessage, name string) (string, error) {
	s, err := requiredString(top, name)
	if err != nil {
		return "", err
	}
	if terr := persist.ValidateTimestamp(s); terr != nil {
		return "", terr
	}
	return s, nil
}

func requiredNumber(top map[string]json.RawMessage, name string) (json.Number, error) {
	v, ok := top[name]
	if !ok {
		return "", invalidField("%s absent", name)
	}
	dec := json.NewDecoder(strings.NewReader(string(v)))
	dec.UseNumber()
	var n json.Number
	if jerr := dec.Decode(&n); jerr != nil {
		return "", invalidField("%s must be a number", name)
	}
	return n, nil
}

func positiveDimension(top map[string]json.RawMessage, name string) (float64, error) {
	n, err := requiredNumber(top, name)
	if err != nil {
		return 0, err
	}
	f, ferr := n.Float64()
	if ferr != nil || !(f > 0) || math.IsInf(f, 0) || f > document.DimensionCapMm {
		return 0, invalidField("%s must be a positive mm value within dimensionCapMm", name)
	}
	return f, nil
}

func invalidField(format string, args ...any) error {
	return &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageSchema,
		Detail: fmt.Sprintf(format, args...)}
}

// formatMm renders a placeholder dimension with a deterministic decimal
// form suitable for re-parsing.
func formatMm(v float64) string {
	return strconv.FormatFloat(v, 'g', -1, 64)
}

// writeJSONString serializes one string member deterministically.
func writeJSONString(b *strings.Builder, s string) {
	const hexdigits = "0123456789abcdef"
	b.WriteByte('"')
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case c == '"':
			b.WriteString(`\"`)
		case c == '\\':
			b.WriteString(`\\`)
		case c == '\n':
			b.WriteString(`\n`)
		case c == '\r':
			b.WriteString(`\r`)
		case c == '\t':
			b.WriteString(`\t`)
		case c < 0x20:
			b.WriteString(`\u00`)
			b.WriteByte(hexdigits[c>>4])
			b.WriteByte(hexdigits[c&0xf])
		default:
			b.WriteByte(c)
		}
	}
	b.WriteByte('"')
}
