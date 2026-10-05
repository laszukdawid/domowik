package release

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"net/url"
	"os"
	"regexp"
	"sort"
	"strings"
	"time"
)

// isHTTPSURL requires a parseable canonical https URL with a host; it
// rejects bare hosts, paths with prose, and other schemes so recorded
// provenance always points at an exact retrievable location.
func isHTTPSURL(raw string) bool {
	u, err := url.Parse(raw)
	return err == nil && u.Scheme == "https" && u.Host != "" &&
		u.Path != "" && u.Path != "/" && u.RawQuery == "" && u.Fragment == "" &&
		!strings.ContainsAny(u.Path, " ()")
}

// spdxIDRE constrains license fields to SPDX identifier syntax; expression
// operators are intentionally not accepted so every entry is one exact id.
var spdxIDRE = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9.-]*$`)

// bundledPinRE pins bundled rows to exact content hashes.
var bundledPinRE = regexp.MustCompile(`^sha256:[0-9a-f]{64}$`)

// RegistrySchema is the only license-registry schema version this generator
// accepts.
const RegistrySchema = 1

// NoticeFile is one verbatim third-party license text included in the
// generated NOTICE. The recorded hash binds the committed text to the
// registry entry; a mismatched or missing file fails closed.
type NoticeFile struct {
	Path   string `json:"path"`
	SHA256 string `json:"sha256"`
}

// Entry is one reviewed registry row binding a component key to its approved
// license and notice obligations.
type Entry struct {
	Key         string `json:"key"`
	Kind        string `json:"kind"`
	SPDXLicense string `json:"spdxLicense"`
	Status      string `json:"status"`
	// VersionPin records the exact reviewed component version. The main
	// module pins "(devel)" (moving source tree); dependency rows pin their
	// resolved pseudo-version; bundled rows pin a content hash prefix.
	VersionPin string `json:"versionPin"`
	// ReviewedAt is the date the row was last human-reviewed.
	ReviewedAt      string       `json:"reviewedAt"`
	RevisitTriggers []string     `json:"revisitTriggers"`
	NoticeRequired  bool         `json:"noticeRequired"`
	NoticeFiles     []NoticeFile `json:"noticeFiles,omitempty"`
	SourceURL       string       `json:"sourceUrl"`
	Added           string       `json:"added"`
	Notes           string       `json:"notes,omitempty"`
}

// Registry is the fail-closed reviewed license registry. Discovery output is
// always checked against it; the registry never silently accepts a component.
type Registry struct {
	RegistrySchema  int      `json:"registrySchema"`
	AllowedLicenses []string `json:"allowedLicenses"`
	// LicenseDecisions records the reviewed decision document for every
	// allowed license outside the immutable permissive baseline. Copyleft or
	// proprietary admission without a decision record fails validation.
	LicenseDecisions []LicenseDecision `json:"licenseDecisions,omitempty"`
	Entries          []Entry           `json:"entries"`
}

// BaselineLicenses is the immutable permissive baseline; anything else in
// allowedLicenses requires a linked decision record.
var BaselineLicenses = map[string]bool{
	"Apache-2.0": true, "BSD-2-Clause": true, "BSD-3-Clause": true,
	"CC0-1.0": true, "ISC": true, "MIT": true, "Zlib": true,
}

// LicenseDecision links one non-baseline license to its decision document.
type LicenseDecision struct {
	SPDXID      string `json:"spdxId"`
	DecisionRef string `json:"decisionRef"`
}

// LoadRegistry reads and validates the reviewed license registry.
// LoadRegistry reads and validates the reviewed license registry from an
// arbitrary path (tests/tooling); production loading goes through
// Generator.LoadInputs with root containment.
func LoadRegistry(path string) (*Registry, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read registry: %w", err)
	}
	r, err := parseRegistry(data)
	if err != nil {
		return nil, fmt.Errorf("registry %s: %w", path, err)
	}
	return r, nil
}

// parseRegistry validates raw registry bytes.
func parseRegistry(data []byte) (*Registry, error) {
	var r Registry
	if err := strictUnmarshal(data, &r); err != nil {
		return nil, fmt.Errorf("parse registry: %w", err)
	}
	if err := r.validate(); err != nil {
		return nil, fmt.Errorf("invalid registry: %w", err)
	}
	sort.Slice(r.Entries, func(i, j int) bool { return r.Entries[i].Key < r.Entries[j].Key })
	return &r, nil
}

func (r *Registry) validate() error {
	if r.RegistrySchema != RegistrySchema {
		return fmt.Errorf("unsupported registrySchema %d (want %d)", r.RegistrySchema, RegistrySchema)
	}
	if len(r.AllowedLicenses) == 0 {
		return fmt.Errorf("allowedLicenses must not be empty")
	}
	allowed := map[string]bool{}
	for _, l := range r.AllowedLicenses {
		if !spdxIDRE.MatchString(l) {
			return fmt.Errorf("allowedLicenses contains invalid SPDX id %q", l)
		}
		allowed[l] = true
	}
	decisions := map[string]string{}
	for _, d := range r.LicenseDecisions {
		if !spdxIDRE.MatchString(d.SPDXID) {
			return fmt.Errorf("licenseDecisions contains invalid SPDX id %q", d.SPDXID)
		}
		if !allowed[d.SPDXID] {
			return fmt.Errorf("decision record admits license %q which is not in allowedLicenses", d.SPDXID)
		}
		if BaselineLicenses[d.SPDXID] {
			return fmt.Errorf("decision record for %q is redundant; baseline licenses need no admission", d.SPDXID)
		}
		if err := safeRelPath(d.DecisionRef); err != nil {
			return fmt.Errorf("license decision for %q: %w", d.SPDXID, err)
		}
		decisions[d.SPDXID] = d.DecisionRef
	}
	for _, l := range r.AllowedLicenses {
		if !spdxIDRE.MatchString(l) {
			return fmt.Errorf("allowedLicenses contains invalid SPDX id %q", l)
		}
		if !BaselineLicenses[l] && decisions[l] == "" {
			return fmt.Errorf("license %q is outside the permissive baseline; admission requires a decision record in licenseDecisions under docs/THIRD-PARTY-POLICY.md", l)
		}
		allowed[l] = true
	}
	seen := map[string]bool{}
	for i, e := range r.Entries {
		switch {
		case e.Key == "":
			return fmt.Errorf("entry[%d]: key is required", i)
		case seen[e.Key]:
			return fmt.Errorf("entry[%d]: duplicate key %q", i, e.Key)
		case e.Kind != KindModule && e.Kind != KindToolchain && e.Kind != KindBundled:
			return fmt.Errorf("entry %q: unknown kind %q", e.Key, e.Kind)
		case !allowed[e.SPDXLicense]:
			return fmt.Errorf("entry %q: license %q is not in allowedLicenses; extension requires review", e.Key, e.SPDXLicense)
		case e.Status != "reviewed":
			return fmt.Errorf("entry %q: status must be \"reviewed\", got %q", e.Key, e.Status)
		case !isHTTPSURL(e.SourceURL):
			return fmt.Errorf("entry %q: sourceUrl must be an exact https URL, got %q", e.Key, e.SourceURL)
		case !spdxIDRE.MatchString(e.SPDXLicense):
			return fmt.Errorf("entry %q: spdxLicense %q is not a valid SPDX identifier", e.Key, e.SPDXLicense)
		case e.VersionPin == "":
			return fmt.Errorf("entry %q: versionPin is required provenance", e.Key)
		case e.Kind == KindBundled && !bundledPinRE.MatchString(e.VersionPin):
			return fmt.Errorf("entry %q: bundled versionPin must be a sha256 content pin, got %q", e.Key, e.VersionPin)
		case len(e.RevisitTriggers) == 0:
			return fmt.Errorf("entry %q: revisitTriggers must not be empty", e.Key)
		}
		if _, err := time.Parse("2006-01-02", e.ReviewedAt); err != nil {
			return fmt.Errorf("entry %q: reviewedAt must be YYYY-MM-DD: %v", e.Key, err)
		}
		if _, err := time.Parse("2006-01-02", e.Added); err != nil {
			return fmt.Errorf("entry %q: added must be YYYY-MM-DD: %v", e.Key, err)
		}
		if e.NoticeRequired && len(e.NoticeFiles) == 0 {
			return fmt.Errorf("entry %q: noticeRequired is true but no verbatim notice file is recorded; a URL alone does not satisfy the obligation", e.Key)
		}
		for _, nf := range e.NoticeFiles {
			if err := safeRelPath(nf.Path); err != nil {
				return fmt.Errorf("entry %q: notice file: %w", e.Key, err)
			}
			if !sha256RE.MatchString(nf.SHA256) {
				return fmt.Errorf("entry %q: notice file %q: sha256 must be 64 lowercase hex characters", e.Key, nf.Path)
			}
		}
		seen[e.Key] = true
	}
	sort.Slice(r.Entries, func(i, j int) bool { return r.Entries[i].Key < r.Entries[j].Key })
	return nil
}

// Entry returns the registry entry for key, if present.
func (r *Registry) Entry(key string) (Entry, bool) {
	for _, e := range r.Entries {
		if e.Key == key {
			return e, true
		}
	}
	return Entry{}, false
}

// Audit compares discovered components against the registry and returns every
// violation, including stale registry rows. Use it once per generation over
// the FULL union of discovered components (all targets); use AuditEntries
// for narrower views such as a single target's inventory.
//
// Rules enforced:
//
//   - discovered: each component needs a reviewed entry (missing/unreviewed)
//   - incompatible: entry license is outside allowedLicenses (defensive; the
//     loader already rejects this, kept as an invariant check on audit paths)
//   - stale: registry entries that no longer match any discovered component,
//     so removed dependencies cannot accumulate unreviewed residue
func (r *Registry) Audit(components []Component) []Finding {
	var findings []Finding
	discovered := map[string]bool{}
	for _, c := range components {
		discovered[c.Key] = true
		e, ok := r.Entry(c.Key)
		if !ok {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "discovered",
				Detail: "no reviewed registry entry; add one under the third-party policy before adoption",
			})
			continue
		}
		if e.Kind != c.Kind {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "kind-mismatch",
				Detail: fmt.Sprintf("registry kind %q does not match discovered kind %q", e.Kind, c.Kind),
			})
		}
		if !r.allowed(e.SPDXLicense) {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "incompatible",
				Detail: fmt.Sprintf("license %q is not in allowedLicenses", e.SPDXLicense),
			})
		}
		if e.Status != "reviewed" {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "unreviewed",
				Detail: fmt.Sprintf("registry status is %q", e.Status),
			})
		}
		appendVersionPinFinding(&findings, c, e)
	}
	for _, e := range r.Entries {
		if !discovered[e.Key] {
			findings = append(findings, Finding{
				Component: e.Key, Kind: e.Kind, Rule: "stale",
				Detail: "registry entry has no matching discovered component; remove it or fix discovery",
			})
		}
	}
	SortFindings(findings)
	return findings
}

// VerifyDecisionRefs root-safely confirms every non-baseline license
// admission points at an existing reviewed decision document and that no
// duplicate or baseline-redundant rows exist.
func (r *Registry) VerifyDecisionRefs(root string) error {
	seen := map[string]string{}
	for _, d := range r.LicenseDecisions {
		if prev := seen[d.SPDXID]; prev != "" {
			return fmt.Errorf("duplicate license decision for %q (%s and %s)", d.SPDXID, prev, d.DecisionRef)
		}
		seen[d.SPDXID] = d.DecisionRef
		if BaselineLicenses[d.SPDXID] {
			return fmt.Errorf("decision record for %q is redundant; baseline licenses need no admission", d.SPDXID)
		}
		if _, err := readFileUnderRoot(root, d.DecisionRef); err != nil {
			return fmt.Errorf("license decision for %q: %w", d.SPDXID, err)
		}
	}
	allowed := map[string]bool{}
	for _, l := range r.AllowedLicenses {
		allowed[l] = true
	}
	for spdx, ref := range seen {
		if !allowed[spdx] {
			return fmt.Errorf("decision record %q admits license %q which is not in allowedLicenses", ref, spdx)
		}
	}
	return nil
}

// VerifyNoticeFiles checks that every required verbatim notice file exists
// and matches its recorded hash, failing closed on absence or mismatch.
func (r *Registry) VerifyNoticeFiles(root string) error {
	for _, e := range r.Entries {
		for _, nf := range e.NoticeFiles {
			data, err := readFileUnderRoot(root, nf.Path)
			if err != nil {
				return fmt.Errorf("entry %q: notice file %s unreadable: %w", e.Key, nf.Path, err)
			}
			sum := sha256.Sum256(data)
			if got := hex.EncodeToString(sum[:]); got != nf.SHA256 {
				return fmt.Errorf("entry %q: notice file %s content does not match its recorded hash; review the upstream text and update the registry deliberately", e.Key, nf.Path)
			}
		}
	}
	return nil
}

// AuditEntries applies every discovered-direction rule (presence, review
// status, kind match, license compatibility) but never reports stale rows,
// so a subset view such as one target's embed set cannot flag entries that
// belong to other targets or to the declarative baseline.
func (r *Registry) AuditEntries(components []Component) []Finding {
	var findings []Finding
	for _, c := range components {
		e, ok := r.Entry(c.Key)
		if !ok {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "discovered",
				Detail: "no reviewed registry entry; add one under the third-party policy before adoption",
			})
			continue
		}
		if e.Kind != c.Kind {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "kind-mismatch",
				Detail: fmt.Sprintf("registry kind %q does not match discovered kind %q", e.Kind, c.Kind),
			})
		}
		if !r.allowed(e.SPDXLicense) {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "incompatible",
				Detail: fmt.Sprintf("license %q is not in allowedLicenses", e.SPDXLicense),
			})
		}
		if e.Status != "reviewed" {
			findings = append(findings, Finding{
				Component: c.Key, Kind: c.Kind, Rule: "unreviewed",
				Detail: fmt.Sprintf("registry status is %q", e.Status),
			})
		}
		appendVersionPinFinding(&findings, c, e)
	}
	SortFindings(findings)
	return findings
}

// appendVersionPinFinding compares a resolved component version against its
// reviewed pin. Empty/(devel) placeholders compare equal so the moving
// source tree and development builds stay auditable without false alarms.
func appendVersionPinFinding(findings *[]Finding, c Component, e Entry) {
	got := comparableVersion(c.Version)
	want := comparableVersion(e.VersionPin)
	// Only "(devel)" and empty compare equal to each other; an unpinned or
	// unresolvable component version against a concrete pin is a mismatch,
	// never a silent pass.
	if got != want {
		*findings = append(*findings, Finding{
			Component: c.Key, Kind: c.Kind, Rule: "version-pin-mismatch",
			Detail: fmt.Sprintf("resolved version %q does not match reviewed pin %q; re-review before adoption", got, want),
		})
	}
}

func (r *Registry) allowed(license string) bool {
	for _, l := range r.AllowedLicenses {
		if l == license {
			return true
		}
	}
	return false
}
