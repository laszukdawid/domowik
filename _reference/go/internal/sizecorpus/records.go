package sizecorpus

import (
	"bytes"
	"encoding/json"
	"strings"
)

// Generator and record schema versions. The generator version pins the
// construction + measurement behavior for Tier-W reproduction claims; the
// schema versions let consumers detect record-format evolution.
const (
	GeneratorVersion    = "0035-part-a.1"
	BudgetsSchema       = "0035.budget-records.v1"
	PreviewSchema       = "0035.preview-inventory.v1"
	CandidatesSchema    = "0035.encoding-candidates.v1"
	SelfCheckVersion    = "0035-selfcheck.v1"
	FormatVersionV01    = "0.1"
	reproductionCommand = `DOMOWIK_WRITE_0035_RECORDS=1 GOTOOLCHAIN=local go test ./internal/sizecorpus -run TestGatedRecordGeneration -count=1`
)

// BudgetRecord is one scenario's measured-budget record: every acceptance
// field of issue 0035 AC-1 plus the reviewer-mandated additions
// (schemaVersion, definition digest, package digest, full entry inventory,
// snapshot/history separation, external byte classes, self-check result).
type BudgetRecord struct {
	SchemaVersion    string          `json:"schemaVersion"`
	Scenario         string          `json:"scenario"`
	DefinitionDigest string          `json:"scenarioDefinitionDigest"`
	SpecBasis        SpecBasis       `json:"specBasis"`
	Generator        GeneratorInfo   `json:"generator"`
	SelfCheck        SelfCheckResult `json:"selfCheck"`
	Package          PackageRecord   `json:"package"`
	Budget           BudgetVerdict   `json:"budget"`
	ClassBreakdown   ClassBreakdown  `json:"classBreakdown"`
	ExternalTotals   ExternalTotals  `json:"externalTotalsExcludedFromHeadline"`
	Preview          PreviewSummary  `json:"preview"`
	Versions         VersionBlock    `json:"versions"`
	Environment      Environment     `json:"environment"`
	RawOutputs       []string        `json:"rawOutputs"`
}

type SpecBasis struct {
	AssetStrategy string `json:"assetStrategy"`
	DecisionRef   string `json:"decisionReference"`
	Provisional   bool   `json:"provisionalPending0021Amendment"`
}

type GeneratorInfo struct {
	Version string `json:"version"`
}

type SelfCheckResult struct {
	Version string `json:"version"`
	Result  string `json:"result"` // "pass" only; failures abort before records exist
}

type PackageRecord struct {
	Path      string `json:"committedPath"`
	Profile   string `json:"profile"`
	SHA256    string `json:"sha256"`
	ByteCount int    `json:"byteCount"`
}

type BudgetVerdict struct {
	TargetKiB        int    `json:"targetKiB"`
	TargetBytes      int64  `json:"targetBytes"`
	Comparison       string `json:"comparison"`
	HeadlineBytes    int    `json:"headlineBytes"`
	PreviewIncluded  bool   `json:"previewIncluded"`
	Pass             bool   `json:"pass"`
	RevisionRequired bool   `json:"budgetRevisionRequired"`
	RevisionNote     string `json:"revisionNote,omitempty"`
}

type classTotals struct {
	LogicalBytes  int `json:"logicalBytes"`
	PhysicalBytes int `json:"physicalBytes"`
}

type ClassBreakdown struct {
	ContainerOverheadDefinition string                 `json:"containerOverheadDefinition"`
	ContainerOverheadBytes      int                    `json:"containerOverheadBytes"`
	Classes                     map[string]classTotals `json:"classes"`
	Entries                     []EntryMeasure         `json:"entries"`
	AccountingIdentity          string                 `json:"accountingIdentity"`
}

type ExternalTotals struct {
	UniverseSize         int    `json:"referencedUniverseSize"`
	LogicalExternalBytes int64  `json:"logicalExternalBytes"`
	ExternalCacheBytes   int64  `json:"externalCacheBytes"`
	ExternalRemoteBytes  int64  `json:"externalRemoteBytes"`
	Note                 string `json:"note"`
}

type PreviewSummary struct {
	InventoryPath string `json:"inventoryPath"`
	EmbeddedCount int    `json:"embeddedCount"`
}

type VersionBlock struct {
	FormatVersion      string `json:"formatVersion"`
	WriterSeam         string `json:"writerSeam"`
	ContainerProfile   string `json:"containerProfile"`
	CompressionLibrary string `json:"compressionLibrary"`
	PreviewEncoder     string `json:"previewEncoder"`
	Oracle             string `json:"oracle"`
}

// PreviewInventory is one scenario's preview-record inventory completing the
// AC-6 style fields: one row per preview namespace entry, with the
// thumbnail's digest, byte count, and dimensions attached to its actual
// preview/data entry (the index entry is recorded separately with its own
// digest and size).
type PreviewInventory struct {
	SchemaVersion string          `json:"schemaVersion"`
	Scenario      string          `json:"scenario"`
	Entries       []PreviewRecord `json:"entries"`
}

type PreviewRecord struct {
	Namespace  string        `json:"namespaceEntry"`
	Kind       string        `json:"kind"` // "thumbnail" | "index"
	Digest     string        `json:"digest"`
	ByteLength int           `json:"byteLength"`
	Dimensions *Dimensions   `json:"dimensions,omitempty"`
	SourceKey  string        `json:"sourceKey,omitempty"`
	Encoder    *EncoderIdent `json:"encoder,omitempty"`
}

type Dimensions struct {
	WidthPx  int `json:"widthPx"`
	HeightPx int `json:"heightPx"`
}

type EncoderIdent struct {
	Name               string `json:"name"`
	Implementation     string `json:"implementation"`
	ZlibRuntimeVersion string `json:"zlibRuntimeVersion"`
	VectorBound        string `json:"vectorBinding"`
	LadderReductions   int    `json:"ladderReductionsApplied"`
}

// CandidateStatus is one promotion-gate encoding candidate row.
type CandidateStatus struct {
	Candidate          string `json:"candidate"`
	Status             string `json:"status"`
	Reason             string `json:"reason,omitempty"`
	PatentRiskReview   string `json:"patentRiskReview"`
	FutureRequirements string `json:"futureRecordRequirements,omitempty"`
}

// EncodingCandidates records that no candidate was measured this session;
// per AC-3 no candidate becomes a format requirement without a complete
// benchmark record, so absence of records keeps every row a candidate.
type EncodingCandidates struct {
	SchemaVersion string            `json:"schemaVersion"`
	Session       string            `json:"session"`
	Candidates    []CandidateStatus `json:"candidates"`
}

// BuildBudgetRecord assembles the record from a measured package.
func BuildBudgetRecord(def Definition, m *Measurement, env Environment,
	pkgPath string, zlibVer string) *BudgetRecord {
	headline := m.Package.ByteCount
	target := int64(def.TargetKiB) * 1024
	pass := int64(headline) < target
	rec := &BudgetRecord{
		SchemaVersion:    BudgetsSchema,
		Scenario:         def.Name,
		DefinitionDigest: def.DefinitionDigest(),
		SpecBasis: SpecBasis{
			AssetStrategy: "ASSET-STRATEGY §10.9",
			DecisionRef:   def.EDReference,
			Provisional:   true,
		},
		Generator: GeneratorInfo{Version: GeneratorVersion},
		SelfCheck: SelfCheckResult{Version: SelfCheckVersion, Result: "pass"},
		Package: PackageRecord{
			Path: pkgPath, Profile: m.Package.Profile,
			SHA256: m.Package.SHA256, ByteCount: headline,
		},
		Budget: BudgetVerdict{
			TargetKiB: def.TargetKiB, TargetBytes: target,
			Comparison:    "strict less-than (measured < KiB * 1024)",
			HeadlineBytes: headline, PreviewIncluded: true,
			Pass: pass, RevisionRequired: !pass,
			RevisionNote: revisionNote(pass),
		},
		ClassBreakdown: ClassBreakdown{
			ContainerOverheadDefinition: "bytes outside entry local spans: local headers of the manifest are inside spans; overhead covers EOCD(22) + central directory",
			ContainerOverheadBytes:      m.Overhead,
			Classes:                     collapseClasses(m),
			Entries:                     m.Entries,
			AccountingIdentity:          "sum(entries.physicalBytes) + containerOverheadBytes == package.byteCount",
		},
		ExternalTotals: ExternalTotals{
			UniverseSize:         m.UniverseSize,
			LogicalExternalBytes: 0,
			ExternalCacheBytes:   0,
			ExternalRemoteBytes:  0,
			Note:                 "thin profile embeds only preview-class blobs; U(P) is empty because the scenarios define zero descriptors and zero instances; external/remote totals are excluded from the headline per ASSET-STRATEGY §10.9",
		},
		Preview: PreviewSummary{InventoryPath: "preview-inventory-" + def.Name + ".json", EmbeddedCount: 2},
		Versions: VersionBlock{
			FormatVersion:      FormatVersionV01,
			WriterSeam:         "internal/persist.BuildPackage (issue 0026 writer seam), branch issue/0035-size-benchmarks",
			ContainerProfile:   "zip container per FORMAT-SPEC §7.7; observed per-entry methods in this package: " + observedMethodSummary(m),
			CompressionLibrary: "Go compress/flate (" + env.GoVersion + ") for deflated container entries; python3 stdlib zlib " + zlibVer + " for the reference thumbnail",
			PreviewEncoder:     "ASSET-STRATEGY §10.9 reference thumbnail encoder via python3 stdlib zlib (dev-only, docs/third-party/tools.json)",
			Oracle:             "internal/document NewCreateOp/NewBatch/Deliver authoritative validation + persist.LoadFile round-trip",
		},
		Environment: env,
		RawOutputs: []string{
			"raw/environment.txt",
			"raw/python3-zlib.txt",
			"raw/go-version.txt",
		},
	}
	return rec
}

// observedMethodSummary derives the container-profile summary from the
// measured archive itself so the record can never contradict its own
// per-entry inventory.
func observedMethodSummary(m *Measurement) string {
	order := []string{classManifest, classDescriptors, classSnapshot, classHistory, classPreviewIndex, classPreviewData}
	methods := map[string]map[string]bool{}
	for _, e := range m.Entries {
		if methods[e.Class] == nil {
			methods[e.Class] = map[string]bool{}
		}
		methods[e.Class][e.Method] = true
	}
	parts := []string{}
	for _, c := range order {
		set, ok := methods[c]
		if !ok || len(set) == 0 {
			parts = append(parts, c+"=absent")
			continue
		}
		names := make([]string, 0, len(set))
		for name := range set {
			names = append(names, name)
		}
		sortStrings(names)
		parts = append(parts, c+"="+strings.Join(names, "|"))
	}
	return strings.Join(parts, "; ")
}

func revisionNote(pass bool) string {
	if pass {
		return ""
	}
	return "measured >= target: mandatory recorded budget revision required before publication; never a silent pass"
}

func collapseClasses(m *Measurement) map[string]classTotals {
	order := []string{classManifest, classDescriptors, classSnapshot, classHistory, classPreviewIndex, classPreviewData}
	out := map[string]classTotals{}
	for _, c := range order {
		out[c] = classTotals{LogicalBytes: m.ClassLogical[c], PhysicalBytes: m.ClassBytes[c]}
	}
	return out
}

// BuildPreviewInventory assembles the preview-record inventory of one
// scenario (AC-2): the reference thumbnail row names its actual
// preview/data entry; the index row records the derived-cache index entry's
// own digest and size.
func BuildPreviewInventory(def Definition, png []byte, zlibVer string) *PreviewInventory {
	pngHex := hexString(sha256Of(png))
	index := PreviewIndexJSON(png)
	inv := &PreviewInventory{
		SchemaVersion: PreviewSchema,
		Scenario:      def.Name,
		Entries: []PreviewRecord{
			{
				Namespace:  "preview/data/" + pngHex[:2] + "/" + pngHex[2:],
				Kind:       "thumbnail",
				Digest:     "sha256:" + pngHex,
				ByteLength: len(png),
				Dimensions: &Dimensions{WidthPx: refPNGWidthPx, HeightPx: refPNGHeightPx},
				SourceKey:  PreviewSourceKey,
				Encoder: &EncoderIdent{
					Name:               "ASSET-STRATEGY §10.9 reference thumbnail encoder",
					Implementation:     "python3 stdlib zlib level 9 over 512 scanlines of 00 + 2048 x FF; PNG RGBA8 IHDR/IEND assembled by this generator",
					ZlibRuntimeVersion: zlibVer,
					VectorBound:        "pinned §10.9 evidence vector 1809 B / sha256 " + refPNGSHA256Hex,
					LadderReductions:   0,
				},
			},
			{
				Namespace:  "preview/index.json",
				Kind:       "index",
				Digest:     "sha256:" + hexString(sha256Of(index)),
				ByteLength: len(index),
			},
		},
	}
	return inv
}

// EncodingCandidatesRecord is the fixed not-measured status set agreed with
// review for this session.
func EncodingCandidatesRecord() *EncodingCandidates {
	return &EncodingCandidates{
		SchemaVersion: CandidatesSchema,
		Session:       "2026-08-25 Part A session (branch issue/0035-size-benchmarks)",
		Candidates: []CandidateStatus{
			{
				Candidate: "WebP", Status: "not-measured",
				Reason:             "reference encoder tool (cwebp/libwebp CLI) absent from the measurement environment; no policy-reviewed substitute tool identity selected",
				PatentRiskReview:   "not-performed",
				FutureRequirements: futureRequirements(),
			},
			{
				Candidate: "KTX2/Basis Universal", Status: "not-measured",
				Reason:             "basisu absent AND its mandatory patent-risk review unperformed",
				PatentRiskReview:   "not-performed (mandatory for Basis Universal; blocks any future promotion until performed)",
				FutureRequirements: futureRequirements(),
			},
			{
				Candidate: "glTF/GLB + meshoptimizer", Status: "not-measured",
				Reason:             "gltfpack absent AND no representative model input exists while furnished-cached-v1 stays dormant (no furniture semantics invented)",
				PatentRiskReview:   "not-performed",
				FutureRequirements: futureRequirements(),
			},
			{
				Candidate: "font subsetting", Status: "not-measured",
				Reason:             "fonttools/pyftsubset absent and no font assets exist in v0.1 scope",
				PatentRiskReview:   "not-performed",
				FutureRequirements: futureRequirements(),
			},
			{
				Candidate: "AVIF", Status: "not-measured",
				Reason:             "encoder binaries present in the environment (avifenc 1.4.2) but the pair agreed to record all five candidates not-measured this session to keep Part A scope closed; thresholds must be pre-registered before any future measurement",
				PatentRiskReview:   "not-performed (AV1 patent landscape requires review before any promotion decision regardless)",
				FutureRequirements: futureRequirements(),
			},
		},
	}
}

func futureRequirements() string {
	return "complete record requires: tool name/version/source URL, license identifier + notice obligations, adapted-vs-independent provenance, patent-risk flag, named reference environment (CPU/RAM/OS/build mode), encoded+decoded sizes per fixed quality tier, decode p50/p95 + peak memory, corpus input digests, committed raw outputs, pre-registered failure thresholds"
}

// MarshalRecordJSON renders a record deterministically: indented JSON, LF
// newlines, trailing newline. Struct field order is fixed by declaration.
func MarshalRecordJSON(v interface{}) ([]byte, error) {
	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.SetIndent("", "  ")
	if err := enc.Encode(v); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}
