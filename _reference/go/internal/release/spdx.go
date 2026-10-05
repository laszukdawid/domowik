package release

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"regexp"
	"sort"
	"strings"
	"time"
)

// SPDX constants pinning the emitted document format.
const (
	spdxVersion   = "SPDX-2.3"
	dataLicense   = "CC0-1.0"
	generatorName = "domowik-sbom"
	// GeneratorVersion is pinned: any change to generation logic that can
	// alter output must bump it and is recorded in docs/RELEASE-INVENTORY.md.
	GeneratorVersion = "1.1.0"
	namespaceBase    = "https://domowik.dev/spdx/"
)

// SPDXChecksum is a file checksum.
type SPDXChecksum struct {
	Algorithm string `json:"algorithm"`
	Value     string `json:"checksumValue"`
}

// SPDXCreationInfo records when and by which tool version a document was
// created. Created is the only field allowed to vary between a committed
// document and its regeneration; verification normalizes it explicitly.
type SPDXCreationInfo struct {
	Created            string   `json:"created"`
	Creators           []string `json:"creators"`
	LicenseListVersion string   `json:"licenseListVersion"`
}

// SPDXPackage is one component in an SBOM.
type SPDXPackage struct {
	Name                 string         `json:"name"`
	SPDXID               string         `json:"SPDXID"`
	VersionInfo          string         `json:"versionInfo,omitempty"`
	DownloadLocation     string         `json:"downloadLocation"`
	FilesAnalyzed        bool           `json:"filesAnalyzed"`
	LicenseConcluded     string         `json:"licenseConcluded"`
	LicenseDeclared      string         `json:"licenseDeclared"`
	CopyrightText        string         `json:"copyrightText"`
	PrimaryPackagePurpos string         `json:"primaryPackagePurpose,omitempty"`
	Comment              string         `json:"comment,omitempty"`
	Checksums            []SPDXChecksum `json:"checksums,omitempty"`
}

// SPDXFile is one file entry in an SBOM.
type SPDXFile struct {
	FileName         string         `json:"fileName"`
	SPDXID           string         `json:"SPDXID"`
	Checksums        []SPDXChecksum `json:"checksums"`
	LicenseConcluded string         `json:"licenseConcluded"`
	CopyrightText    string         `json:"copyrightText"`
	Comment          string         `json:"comment,omitempty"`
}

// SPDXRelationship is one relationship edge in an SBOM.
type SPDXRelationship struct {
	SpdxElementID      string `json:"spdxElementId"`
	RelationshipType   string `json:"relationshipType"`
	RelatedSpdxElement string `json:"relatedSpdxElement"`
	Comment            string `json:"comment,omitempty"`
}

// SPDXDocument is the rendered document root.
type SPDXDocument struct {
	SPDXVersion       string             `json:"spdxVersion"`
	DataLicense       string             `json:"dataLicense"`
	SPDXID            string             `json:"SPDXID"`
	Name              string             `json:"name"`
	DocumentNamespace string             `json:"documentNamespace"`
	CreationInfo      SPDXCreationInfo   `json:"creationInfo"`
	Packages          []SPDXPackage      `json:"packages"`
	Files             []SPDXFile         `json:"files,omitempty"`
	Relationships     []SPDXRelationship `json:"relationships"`
	Comment           string             `json:"comment,omitempty"`
}

// SBOMInput describes everything needed to render one deterministic SBOM.
type SBOMInput struct {
	// Name is the human-readable document name; it also seeds the namespace.
	Name string
	// Created is the resolved deterministic epoch.
	Created time.Time
	// Packages are emitted sorted by SPDXID.
	Packages []SPDXPackage
	// Files are emitted sorted by FileName.
	Files []SPDXFile
	// Relationships are emitted sorted by (from, type, to).
	Relationships []SPDXRelationship
}

// RenderSBOM renders a byte-for-byte deterministic SPDX 2.3 JSON document.
// The document namespace carries a content hash over every input except the
// creation timestamp, so equal inventories share a namespace across epochs
// while remaining distinguishable from any other inventory.
func RenderSBOM(in SBOMInput) ([]byte, error) {
	if in.Name == "" {
		return nil, fmt.Errorf("sbom name is required")
	}
	// Fail closed on SPDXID collisions: sanitized keys such as "a/b" versus
	// "a-b" would otherwise yield duplicate identifiers and ambiguous
	// relationships once more components exist.
	seen := map[string]string{}
	for _, pkg := range in.Packages {
		if prevName, dup := seen[pkg.SPDXID]; dup {
			return nil, fmt.Errorf("spdx id collision between %q and %q (both %s); rename one key", prevName, pkg.Name, pkg.SPDXID)
		}
		seen[pkg.SPDXID] = pkg.Name
	}
	for _, f := range in.Files {
		if prevName, dup := seen[f.SPDXID]; dup {
			return nil, fmt.Errorf("spdx id collision between file %q and %q (both %s)", f.FileName, prevName, f.SPDXID)
		}
		seen[f.SPDXID] = f.FileName
	}
	sort.Slice(in.Packages, func(i, j int) bool { return in.Packages[i].SPDXID < in.Packages[j].SPDXID })
	sort.Slice(in.Files, func(i, j int) bool { return in.Files[i].FileName < in.Files[j].FileName })
	sort.Slice(in.Relationships, func(i, j int) bool {
		a, b := in.Relationships[i], in.Relationships[j]
		if a.SpdxElementID != b.SpdxElementID {
			return a.SpdxElementID < b.SpdxElementID
		}
		if a.RelationshipType != b.RelationshipType {
			return a.RelationshipType < b.RelationshipType
		}
		return a.RelatedSpdxElement < b.RelatedSpdxElement
	})

	doc := SPDXDocument{
		SPDXVersion: spdxVersion,
		DataLicense: dataLicense,
		SPDXID:      "SPDXRef-DOCUMENT",
		Name:        in.Name,
		CreationInfo: SPDXCreationInfo{
			Created: in.Created.UTC().Format(time.RFC3339),
			Creators: []string{
				fmt.Sprintf("Tool: %s-%s", generatorName, GeneratorVersion),
				"Organization: domowik.dev",
			},
			LicenseListVersion: "3.25",
		},
		Packages:      in.Packages,
		Files:         in.Files,
		Relationships: in.Relationships,
	}
	doc.DocumentNamespace = fmt.Sprintf("%s%s-%s", namespaceBase, sanitizeName(in.Name), contentFingerprint(in))
	out, err := json.MarshalIndent(doc, "", "  ")
	if err != nil {
		return nil, fmt.Errorf("encode sbom: %w", err)
	}
	return append(out, '\n'), nil
}

// createdRe matches exactly the emitted creationInfo.created value form and
// captures the timestamp for validity checks.
var createdRe = regexp.MustCompile(`"created": "([^"]*)"`)

// validSBOMTimestamp requires the canonical RFC3339 form the generator emits
// (UTC second precision); garbage or exotic-but-parseable forms fail closed.
func validSBOMTimestamp(v string) bool {
	t, err := time.Parse(time.RFC3339, v)
	if err != nil {
		return false
	}
	return t.Format("2006-01-02T15:04:05Z") == v || t.UTC().Format(time.RFC3339) == v
}

// NormalizeSBOM validates and blanks ONLY the creationInfo.created value,
// leaving every other byte untouched. Unlike a JSON round-trip, this accepts
// no whitespace, key-order, number-formatting drift, or malformed timestamps:
// any other difference between two documents remains visible.
func NormalizeSBOM(data []byte) ([]byte, error) {
	matches := createdRe.FindAllSubmatchIndex(data, -1)
	if len(matches) != 1 {
		return nil, fmt.Errorf("expected exactly one created field, found %d", len(matches))
	}
	value := string(data[matches[0][2]:matches[0][3]])
	if !validSBOMTimestamp(value) {
		return nil, fmt.Errorf("creationInfo.created %q is not the canonical RFC3339 form", value)
	}
	loc := matches[0]
	out := make([]byte, len(data))
	copy(out, data)
	return append(out[:loc[0]:loc[0]], append([]byte(`"created": ""`), out[loc[1]:]...)...), nil
}

func sanitizeName(name string) string {
	var b strings.Builder
	for _, r := range strings.ToLower(name) {
		switch {
		case r >= 'a' && r <= 'z', r >= '0' && r <= '9', r == '-', r == '.':
			b.WriteRune(r)
		default:
			b.WriteRune('-')
		}
	}
	return b.String()
}

// checkEmbeddedFileName enforces the same safe relative-path grammar as the
// generator: a "./" prefix, exactly one canonical form, no empty, ".", "..",
// or backslash elements anywhere.
func checkEmbeddedFileName(raw, name string) error {
	switch {
	case !strings.HasPrefix(raw, "./"):
		return fmt.Errorf("sbom file entry %q must start with ./", raw)
	case name == "":
		return fmt.Errorf("sbom file entry %q has an empty path", raw)
	case strings.ContainsAny(raw, "\\") || raw != "./"+name:
		return fmt.Errorf("sbom file entry %q is not a single ./ prefixed element chain", raw)
	}
	for _, elem := range strings.Split(name, "/") {
		switch elem {
		case "", ".", "..":
			return fmt.Errorf("sbom file entry %q contains an unsafe path element %q", raw, elem)
		}
	}
	return nil
}

// SPDXApplicationChecksum extracts the described application package's ID and
// SHA-256 checksum from an emitted per-binary document.
func SPDXApplicationChecksum(doc []byte) (string, string, error) { return spdxApplicationChecksum(doc) }

func spdxApplicationChecksum(doc []byte) (string, string, error) {
	var parsed struct {
		Packages []struct {
			SPDXID    string         `json:"SPDXID"`
			Name      string         `json:"name"`
			Checksums []SPDXChecksum `json:"checksums"`
		} `json:"packages"`
		Relationships []SPDXRelationship `json:"relationships"`
	}
	if err := json.Unmarshal(doc, &parsed); err != nil {
		return "", "", fmt.Errorf("parse sbom: %w", err)
	}
	var describes []string
	for _, r := range parsed.Relationships {
		if r.SpdxElementID == "SPDXRef-DOCUMENT" && r.RelationshipType == "DESCRIBES" {
			describes = append(describes, r.RelatedSpdxElement)
		}
	}
	if len(describes) != 1 {
		return "", "", fmt.Errorf("expected exactly one DESCRIBES relationship, found %d", len(describes))
	}
	for _, pkg := range parsed.Packages {
		if pkg.SPDXID != describes[0] {
			continue
		}
		for _, c := range pkg.Checksums {
			if c.Algorithm == "SHA256" {
				return c.Value, pkg.SPDXID, nil
			}
		}
		return "", "", fmt.Errorf("application package %q has no SHA-256 checksum", pkg.Name)
	}
	return "", "", fmt.Errorf("described package %q not found in sbom", describes[0])
}

// SPDXEmbeddedFilePaths returns the sorted paths of exactly those files the
// document relates to the described application via CONTAINS — i.e. the
// authoritative per-binary embedded set. It fails closed on malformed,
// duplicate, or unsafe file names and on missing/duplicate DESCRIBES edges.
func SPDXEmbeddedFilePaths(doc []byte) ([]string, error) {
	var parsed struct {
		Packages []struct {
			SPDXID string `json:"SPDXID"`
		} `json:"packages"`
		Files []struct {
			FileName string `json:"fileName"`
			SPDXID   string `json:"SPDXID"`
		} `json:"files"`
		Relationships []SPDXRelationship `json:"relationships"`
	}
	if err := json.Unmarshal(doc, &parsed); err != nil {
		return nil, fmt.Errorf("parse sbom: %w", err)
	}
	seenPkg := map[string]bool{}
	for _, p := range parsed.Packages {
		if seenPkg[p.SPDXID] {
			return nil, fmt.Errorf("duplicate package id %q", p.SPDXID)
		}
		seenPkg[p.SPDXID] = true
	}
	seenFile := map[string]bool{}
	for _, f := range parsed.Files {
		if seenFile[f.SPDXID] {
			return nil, fmt.Errorf("duplicate file id %q", f.SPDXID)
		}
		seenFile[f.SPDXID] = true
	}
	var describes []string
	for _, r := range parsed.Relationships {
		if r.SpdxElementID == "SPDXRef-DOCUMENT" && r.RelationshipType == "DESCRIBES" {
			describes = append(describes, r.RelatedSpdxElement)
		}
	}
	if len(describes) != 1 {
		return nil, fmt.Errorf("expected exactly one DESCRIBES relationship, found %d", len(describes))
	}
	appID := describes[0]
	appKnown := false
	for _, p := range parsed.Packages {
		if p.SPDXID == appID {
			appKnown = true
			break
		}
	}
	if !appKnown {
		return nil, fmt.Errorf("described package %q not found in sbom", appID)
	}
	contained := map[string]bool{}
	count := 0
	for _, r := range parsed.Relationships {
		if r.SpdxElementID == appID && r.RelationshipType == "CONTAINS" {
			contained[r.RelatedSpdxElement] = true
			count++
		}
	}
	seen := map[string]bool{}
	var out []string
	for _, f := range parsed.Files {
		if !contained[f.SPDXID] {
			return nil, fmt.Errorf("file entry %q (%s) is not CONTAINED by the application; a per-binary sbom must bind every file", f.FileName, f.SPDXID)
		}
		name := strings.TrimPrefix(f.FileName, "./")
		if err := checkEmbeddedFileName(f.FileName, name); err != nil {
			return nil, err
		}
		if seen[name] {
			return nil, fmt.Errorf("sbom file path %q appears twice", name)
		}
		seen[name] = true
		out = append(out, name)
	}
	if count != len(out) {
		return nil, fmt.Errorf("%d CONTAINS relationships reference unknown or unsafe file ids", count-len(out))
	}
	sort.Strings(out)
	return out, nil
}

func contentFingerprint(in SBOMInput) string {
	type fpDoc struct {
		Name          string             `json:"name"`
		Packages      []SPDXPackage      `json:"packages"`
		Files         []SPDXFile         `json:"files"`
		Relationships []SPDXRelationship `json:"relationships"`
	}
	payload, err := json.Marshal(fpDoc{in.Name, in.Packages, in.Files, in.Relationships})
	if err != nil {
		// Marshal of plain data structures cannot fail; be explicit anyway.
		panic(fmt.Sprintf("fingerprint marshal: %v", err))
	}
	sum := sha256.Sum256(payload)
	return hex.EncodeToString(sum[:])
}
