package release

import (
	"fmt"
	"os"
	"regexp"
	"time"
)

// ToolsSchema is the only tools-record schema version accepted.
const ToolsSchema = 1

// Tool statuses.
const (
	ToolStatusDevOnly     = "dev-only"
	ToolStatusDistributed = "distributed"
)

// PinnedTool is one recorded development-time or distributed tool under the
// third-party policy.
type PinnedTool struct {
	Name       string `json:"name"`
	Version    string `json:"version"`
	CommitSHA  string `json:"commitSha,omitempty"`
	SourceURL  string `json:"sourceUrl"`
	License    string `json:"license"`
	Status     string `json:"status"`
	UsedIn     string `json:"usedIn"`
	Provenance string `json:"provenance,omitempty"`
	Added      string `json:"added"`
}

// ToolsRecord is the reviewed record of pinned tools (CI actions, generators,
// and other non-module components).
type ToolsRecord struct {
	Description string       `json:"description,omitempty"`
	ToolsSchema int          `json:"toolsSchema"`
	Tools       []PinnedTool `json:"tools"`
	// DistributedTools and PackagingTools are validated explicit statements:
	// the fields must be PRESENT in the JSON (possibly empty arrays); absent
	// fields fail closed. Any nonempty entry fails validation: distribution
	// of CI/packaging tooling is an unsupported future state requiring a
	// schema change plus full third-party review.
	DistributedTools *[]string `json:"distributedTools"`
	PackagingTools   *[]string `json:"packagingTools"`
	DeclarationNote  string    `json:"declarationNote,omitempty"`
	RevisitTriggers  []string  `json:"revisitTriggers"`
}

var commitSHARe = regexp.MustCompile(`^[0-9a-f]{40}$`)

// SPDXLicense returns the row's license identifier.
func (t PinnedTool) SPDXLicense() string { return t.License }

// LoadToolsRecord reads and validates the pinned-tools record.
// LoadToolsRecord reads and validates the pinned-tools record from an
// arbitrary path (tests/tooling); production loading goes through
// Generator.LoadInputs with root containment.
func LoadToolsRecord(path string) (*ToolsRecord, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read tools record: %w", err)
	}
	t, err := parseToolsRecord(data)
	if err != nil {
		return nil, fmt.Errorf("tools record %s: %w", path, err)
	}
	return t, nil
}

// parseToolsRecord validates raw tools-record bytes.
func parseToolsRecord(data []byte) (*ToolsRecord, error) {
	var r ToolsRecord
	if err := strictUnmarshal(data, &r); err != nil {
		return nil, fmt.Errorf("parse tools record: %w", err)
	}
	if err := r.validate(); err != nil {
		return nil, fmt.Errorf("invalid tools record: %w", err)
	}
	return &r, nil
}

func (r *ToolsRecord) validate() error {
	if r.ToolsSchema != ToolsSchema {
		return fmt.Errorf("unsupported toolsSchema %d (want %d)", r.ToolsSchema, ToolsSchema)
	}
	if len(r.Tools) == 0 {
		return fmt.Errorf("tools must not be empty")
	}
	if len(r.RevisitTriggers) == 0 {
		return fmt.Errorf("revisitTriggers must not be empty")
	}
	if r.DistributedTools == nil {
		return fmt.Errorf("distributedTools must be present as an explicit declaration (use [] for none)")
	}
	if r.PackagingTools == nil {
		return fmt.Errorf("packagingTools must be present as an explicit declaration (use [] for none)")
	}
	// Deliberate future gate: these lists are unsupported by design; even a
	// reviewed entry cannot pass until the schema itself changes.
	for _, name := range append(append([]string{}, *r.DistributedTools...), *r.PackagingTools...) {
		return fmt.Errorf("tool %q is declared distributed/packaging; this pipeline deliberately does not support distributed tooling — change the schema only after full third-party review", name)
	}
	seen := map[string]bool{}
	for i, t := range r.Tools {
		switch {
		case t.Name == "":
			return fmt.Errorf("tool[%d]: name is required", i)
		case seen[t.Name]:
			return fmt.Errorf("tool[%d]: duplicate name %q", i, t.Name)
		case t.Version == "":
			return fmt.Errorf("tool %q: version is required", t.Name)
		case !spdxIDRE.MatchString(t.License):
			return fmt.Errorf("tool %q: license %q is not a valid SPDX identifier", t.Name, t.License)
		case t.Status != ToolStatusDevOnly && t.Status != ToolStatusDistributed:
			return fmt.Errorf("tool %q: status must be %q or %q", t.Name, ToolStatusDevOnly, ToolStatusDistributed)
		case !isHTTPSURL(t.SourceURL):
			return fmt.Errorf("tool %q: sourceUrl must be an exact https URL, got %q", t.Name, t.SourceURL)
		case t.CommitSHA != "" && !commitSHARe.MatchString(t.CommitSHA):
			return fmt.Errorf("tool %q: commitSha must be a 40-character lowercase hex SHA", t.Name)
		case t.UsedIn == "":
			return fmt.Errorf("tool %q: usedIn is required", t.Name)
		}
		if _, err := time.Parse("2006-01-02", t.Added); err != nil {
			return fmt.Errorf("tool %q: added must be YYYY-MM-DD", t.Name)
		}
		if t.Status == ToolStatusDistributed && t.Provenance == "" {
			return fmt.Errorf("tool %q: distributed tools require an explicit provenance record", t.Name)
		}
		seen[t.Name] = true
	}
	return nil
}
