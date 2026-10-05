package persist

import "fmt"

// Category is an exact FORMAT-SPEC §9.3/§9.4 rejection token (or the
// recovery-state vocabulary of §9.7). Diagnostics carry categories, never
// raw library errors.
type Category string

// Rejection taxonomy tokens (exact ratified strings).
const (
	CatMalformedContainer   Category = "malformed-container"
	CatMalformedLength      Category = "malformed-length"
	CatUnsupportedMethod    Category = "unsupported-method"
	CatUnsupportedExtension Category = "unsupported-extension"
	CatUnsupportedVersion   Category = "unsupported-version"
	CatEncryptedEntry       Category = "encrypted-entry"
	CatChecksumError        Category = "checksum-error"
	CatTrailingData         Category = "trailing-data"
	CatOverlappingRegions   Category = "overlapping-regions"
	CatDuplicateLogicalName Category = "duplicate-logical-name"
	CatAmbiguousManifest    Category = "ambiguous-manifest"
	CatMissingManifest      Category = "missing-manifest"
	CatInvalidEntryName     Category = "invalid-entry-name"
	CatPathTraversal        Category = "path-traversal"
	CatMalformedFraming     Category = "malformed-framing"
	CatInvalidFieldSyntax   Category = "invalid-field-syntax"
	CatNonfiniteValue       Category = "nonfinite-value"
	CatNumericOverflow      Category = "numeric-overflow"

	CatLimitTotalExpanded   Category = "limit-total-expanded"
	CatLimitTotalPhysical   Category = "limit-total-physical"
	CatLimitEntryExpanded   Category = "limit-entry-expanded"
	CatLimitEntryPhysical   Category = "limit-entry-physical"
	CatLimitBlobBytes       Category = "limit-blob-bytes"
	CatLimitEntryCount      Category = "limit-entry-count"
	CatLimitManifestReserve Category = "limit-manifest-reserve"
	CatLimitNameLength      Category = "limit-name-length"
	CatLimitNameComponent   Category = "limit-name-component"
	CatLimitNestingDepth    Category = "limit-nesting-depth"
	CatLimitNumberToken     Category = "limit-number-token"
	CatLimitArrayCount      Category = "limit-array-count"
	CatLimitMemberCount     Category = "limit-member-count"
	CatLimitExpansionRatio  Category = "limit-expansion-ratio"
	CatLimitAggregateExp    Category = "limit-aggregate-expansion"
	CatLimitOperationCount  Category = "limit-operation-count"
	CatLimitSnapshotMeta    Category = "limit-snapshot-metadata"

	// Safe-write refusal (U2/L32 write-back duty; §9.6 outcome class).
	CatSafeWriteRefusal Category = "safe-write-refusal"
)

// Recovery-state machine-readable categories (FORMAT-SPEC §9.7 table;
// diagnostic tokens deliberately outside the §9.4 taxonomy per the
// corpus record field semantics).
const (
	StateValidTarget       Category = "state-valid-target"
	StateStaleCandidate    Category = "state-stale-candidate"
	StateInvalidCandidate  Category = "state-invalid-candidate"
	StateCorruptTarget     Category = "state-corrupt-target"
	StateNoRecovery        Category = "state-no-recovery"
	StateAmbiguousRecovery Category = "state-ambiguous-recovery"
)

// Stage names an enforcement stage for diagnostics (§9.5: the outermost
// failing stage wins and diagnostics name that stage).
type Stage string

const (
	StageContainer Stage = "container"
	StageDirectory Stage = "central-directory"
	StageNames     Stage = "entry-names"
	StageManifest  Stage = "manifest"
	StageEntry     Stage = "entry-stream"
	StageDigest    Stage = "digest"
	StageSchema    Stage = "schema"
	StageFraming   Stage = "framing"
	StageSave      Stage = "save"
	StageRecovery  Stage = "recovery"
)

// Error is the package's typed failure. Category is always an exact
// taxonomy token; Detail is actionable and renders untrusted strings
// inert at the caller.
type Error struct {
	Category Category
	Stage    Stage
	Detail   string
}

func (e *Error) Error() string {
	if e.Detail == "" {
		return fmt.Sprintf("%s (%s)", string(e.Category), e.Stage)
	}
	return fmt.Sprintf("%s (%s): %s", string(e.Category), e.Stage, e.Detail)
}

func fail(cat Category, stage Stage, format string, args ...any) *Error {
	return &Error{Category: cat, Stage: stage, Detail: fmt.Sprintf(format, args...)}
}

// categoryOf extracts the Category of err when it is a *Error.
func categoryOf(err error) (Category, bool) {
	if e, ok := err.(*Error); ok {
		return e.Category, true
	}
	return "", false
}

// CategoryOf reports the exact taxonomy token carried by an error from
// this package, for callers and conformance harnesses.
func CategoryOf(err error) (Category, bool) {
	return categoryOf(err)
}
