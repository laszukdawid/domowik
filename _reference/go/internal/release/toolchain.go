package release

import (
	"fmt"
	"net/url"
	"os"
	"regexp"
	"strings"
	"time"
)

// ToolchainSchema is the only toolchain-record schema version this generator
// accepts.
const ToolchainSchema = 1

// ToolchainArtifact is one exact downloadable artifact of the Go toolchain,
// bound to the record by filename, URL, and hash together.
type ToolchainArtifact struct {
	Filename  string `json:"filename"`
	Kind      string `json:"kind"`
	OS        string `json:"os"`
	Arch      string `json:"arch"`
	URL       string `json:"url"`
	SHA256    string `json:"sha256"`
	SizeBytes int64  `json:"sizeBytes"`
}

// ToolchainLicense records the toolchain license identity.
type ToolchainLicense struct {
	SPDXID string `json:"spdxId"`
	Name   string `json:"name"`
	URL    string `json:"url"`
}

// ToolchainRecord captures exact, verifiable data for the Go toolchain used
// to produce distributed binaries: version, canonical download artifacts with
// hashes, license, supported platforms, and revisit triggers.
type ToolchainRecord struct {
	RecordSchema     int                 `json:"recordSchema"`
	Component        string              `json:"component"`
	Version          string              `json:"version"`
	License          ToolchainLicense    `json:"license"`
	Artifacts        []ToolchainArtifact `json:"artifacts"`
	SupportedTargets []string            `json:"supportedTargets"`
	Added            string              `json:"added"`
	Reviewed         bool                `json:"reviewed"`
	RevisitTriggers  []string            `json:"revisitTriggers"`
	Notes            string              `json:"notes,omitempty"`
}

var sha256RE = regexp.MustCompile(`^[0-9a-f]{64}$`)

// LoadToolchain reads and validates the pinned toolchain record. Validation
// rejects mismatched metadata: hashes that are not lowercase SHA-256 hex,
// filenames that do not carry the recorded version, URLs that do not resolve
// to the recorded filename on the canonical host, non-positive sizes, and
// declared targets without a matching archive artifact.
// LoadToolchain reads and validates the pinned toolchain record from an
// arbitrary path (tests/tooling); production loading goes through
// Generator.LoadInputs with root containment.
func LoadToolchain(path string) (*ToolchainRecord, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read toolchain record: %w", err)
	}
	t, err := parseToolchain(data)
	if err != nil {
		return nil, fmt.Errorf("toolchain record %s: %w", path, err)
	}
	return t, nil
}

// parseToolchain validates raw toolchain-record bytes.
func parseToolchain(data []byte) (*ToolchainRecord, error) {
	var t ToolchainRecord
	if err := strictUnmarshal(data, &t); err != nil {
		return nil, fmt.Errorf("parse toolchain record: %w", err)
	}
	if err := t.validate(); err != nil {
		return nil, fmt.Errorf("invalid toolchain record: %w", err)
	}
	return &t, nil
}

func (t *ToolchainRecord) validate() error {
	if t.RecordSchema != ToolchainSchema {
		return fmt.Errorf("unsupported recordSchema %d (want %d)", t.RecordSchema, ToolchainSchema)
	}
	if t.Component == "" {
		return fmt.Errorf("component is required")
	}
	if t.Version == "" {
		return fmt.Errorf("version is required")
	}
	if !t.Reviewed {
		return fmt.Errorf("record must be reviewed")
	}
	if t.License.SPDXID == "" || t.License.Name == "" {
		return fmt.Errorf("license spdxId and name are required")
	}
	if !isHTTPSURL(t.License.URL) {
		return fmt.Errorf("license url must be an exact https URL, got %q", t.License.URL)
	}
	if len(t.Artifacts) == 0 {
		return fmt.Errorf("artifacts must not be empty")
	}
	hasSource := false
	archives := map[string]bool{}
	sourceCount := 0
	for i, a := range t.Artifacts {
		switch {
		case a.Filename == "":
			return fmt.Errorf("artifact[%d]: filename is required", i)
		case !strings.HasPrefix(a.Filename, t.Version+"."):
			return fmt.Errorf("artifact[%d]: filename %q does not carry the recorded version %q; hash is bound to this record's version",
				i, a.Filename, t.Version)
		case !sha256RE.MatchString(a.SHA256):
			return fmt.Errorf("artifact %q: sha256 must be 64 lowercase hex characters binding the exact artifact to this record", a.Filename)
		case a.SizeBytes <= 0:
			return fmt.Errorf("artifact %q: sizeBytes must be positive", a.Filename)
		case !strings.HasSuffix(a.URL, "/"+a.Filename):
			return fmt.Errorf("artifact %q: url %q does not end in its own filename", a.Filename, a.URL)
		}
		u, err := url.Parse(a.URL)
		if err != nil {
			return fmt.Errorf("artifact %q: bad url: %v", a.Filename, err)
		}
		// Exact canonical binding: scheme, host, path == /dl/<filename>,
		// and no query or fragment may accompany the pinned artifact.
		if u.Scheme != "https" || u.Host != "go.dev" || u.Path != "/dl/"+a.Filename ||
			u.RawQuery != "" || u.Fragment != "" {
			return fmt.Errorf("artifact %q: url must be exactly https://go.dev/dl/%s, got %q", a.Filename, a.Filename, a.URL)
		}
		switch a.Kind {
		case "source":
			hasSource = true
			sourceCount++
			if sourceCount > 1 {
				return fmt.Errorf("duplicate source artifact row %q; exactly one source hash may be pinned", a.Filename)
			}
		case "archive":
			if a.OS == "" || a.Arch == "" {
				return fmt.Errorf("archive artifact %q: os and arch are required", a.Filename)
			}
			// Platform filename grammar for Go archives: the filename must
			// carry its own os-arch token so a mutated row cannot bind one
			// platform's hash to another's metadata.
			if !strings.Contains(a.Filename, "."+a.OS+"-"+a.Arch+".") {
				return fmt.Errorf("artifact %q: filename does not carry its recorded os-arch token %q", a.Filename, a.OS+"-"+a.Arch)
			}
			key := a.OS + "/" + a.Arch
			if archives[key] {
				return fmt.Errorf("duplicate archive artifact for target %q", key)
			}
			archives[key] = true
		default:
			return fmt.Errorf("artifact %q: unknown kind %q", a.Filename, a.Kind)
		}
	}
	if !hasSource {
		return fmt.Errorf("the source artifact (and its hash) must be part of the record")
	}
	if len(t.SupportedTargets) == 0 {
		return fmt.Errorf("supportedTargets must not be empty")
	}
	seenTargets := map[string]bool{}
	for _, tgt := range t.SupportedTargets {
		if seenTargets[tgt] {
			return fmt.Errorf("duplicate supportedTargets entry %q", tgt)
		}
		seenTargets[tgt] = true
		if !archives[tgt] {
			return fmt.Errorf("supported target %q has no matching archive artifact in the record", tgt)
		}
	}
	for tgt := range archives {
		if !seenTargets[tgt] {
			return fmt.Errorf("archive artifact for %q exists but is not listed in supportedTargets; keep the platform binding unambiguous", tgt)
		}
	}
	if len(t.RevisitTriggers) == 0 {
		return fmt.Errorf("revisitTriggers must not be empty; the policy requires recorded revisit conditions")
	}
	if _, err := time.Parse("2006-01-02", t.Added); err != nil {
		return fmt.Errorf("added must be YYYY-MM-DD: %v", t.Added)
	}
	return nil
}

// Artifact returns the archive artifact for a GOOS/GOARCH target.
func (t *ToolchainRecord) Artifact(goos, goarch string) (ToolchainArtifact, bool) {
	for _, a := range t.Artifacts {
		if a.Kind == "archive" && a.OS == goos && a.Arch == goarch {
			return a, true
		}
	}
	return ToolchainArtifact{}, false
}
