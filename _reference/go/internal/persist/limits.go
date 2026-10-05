package persist

// Named FORMAT-SPEC §9.3 quantities (Ratified for v0.1). Every constant
// restates exactly one row of the consolidated limits table; no other
// file in this package may inline a limit magnitude (ED-0021 discipline,
// mirroring the corpus generator's ED-0019 note). "> N rejected; == N
// succeeds" boundaries follow each row's stated semantics.
const (
	// L1: total expanded (logical output) bytes, cumulative per open.
	LimitTotalExpandedBytes = 1 << 31 // row L1, 2 GiB

	// L2: total physical input bytes per open.
	LimitTotalPhysicalBytes = 1 << 31 // row L2, 2 GiB

	// L3: per-entry expanded bytes.
	LimitEntryExpandedBytes = 1 << 29 // row L3, 512 MiB

	// L4: per-entry physical (stored) bytes.
	LimitEntryPhysicalBytes = 1 << 29 // row L4, 512 MiB

	// L5: per-blob logical bytes len(B), asset admission hash domain.
	LimitBlobBytes = 1 << 29 // row L5, 512 MiB

	// L6: entry/member count per package (> 65535 rejected; == 65535
	// succeeds).
	LimitEntryCount = 65535 // row L6

	// L7: manifest/asset-root table combined allocation cap, enforced
	// against computed minimum structure size before proportional
	// allocation.
	LimitManifestReserveBytes = 64 << 20 // row L7, 64 MiB

	// L8/L9: logical name length and per-component length.
	LimitNameLengthBytes       = 1024 // row L8
	LimitNameComponentMaxBytes = 255  // row L9

	// L11: JSON serialization nesting depth (depth == 128 parses).
	LimitJSONDepth = 128 // row L11

	// L12: number literal token length.
	LimitNumberTokenChars = 64 // row L12

	// L15: decompression ratio, per entry. Reject when ratio > 1000;
	// inputs with physical size below L15SmallInputFloorBytes use the
	// absolute expanded bound instead.
	LimitExpansionRatio        = 1000   // row L15
	L15SmallInputFloorBytes    = 64     // row L15 small-input floor
	L15SmallInputExpandedBound = 65_536 // row L15 absolute bound under floor

	// L16: aggregate ratio with absolute floor; both conditions required.
	L16AggregateRatio      = 100      // row L16 (totalExpanded > 100 x totalPhysical)
	L16AggregateFloorBytes = 64 << 20 // row L16 floor (64 MiB)

	// L17/L18: generic parser guards.
	LimitArrayElementCount = 65_536 // row L17
	LimitObjectMemberCount = 65_536 // row L18

	// L19: field-level string forms.
	DigestIdentifierLength = 71   // row L19: "sha256:" + 64 lowercase hex
	UUIDTextualLength      = 36   // row L19 / RFC 9562 canonical form
	MediaTypeMaxBytes      = 127  // row L19
	URIMaxBytes            = 2048 // row L19
	DisplayStringMaxBytes  = 1024 // row L19

	// L23: operation/batch envelope, reader-side resource caps only.
	LimitBatchesPerPackage    = 65_535   // row L23
	LimitOperationsPerBatch   = 65_535   // row L23
	LimitTotalRetainedOps     = 1 << 20  // row L23, 2^20
	LimitSnapshotMetadataByte = 16 << 20 // row L23, 16 MiB
)

// Named provisional writer-policy parameters. Values are PROVISIONAL and
// owned by issue 0029's persistence policy (rows L30/L31 are Deferred);
// they are never serialized as normative constraints and readers never
// validate them (FORMAT-SPEC §7.10 writer-threshold separation). No other
// file may inline these magnitudes.
const (
	// TempArtifactSeqDigits restates the §7.6 counterBase padding width:
	// artifact sequence numbers serialize zero-padded to 19 digits so
	// byte-order equals numeric order within the signed 64-bit ceiling
	// (L13).
	TempArtifactSeqDigits = 19

	// RecoveryArtifactCountMax is the named provisional L30 quantity: at
	// most this many recovery candidates are admitted per target;
	// discovery beyond it excludes the oldest by sequence (never
	// auto-deletes).
	RecoveryArtifactCountMax = 8

	// RetainedOpsWriterTarget is the named provisional L31 quantity: a
	// save whose retained-operation count would exceed this target
	// performs full compaction (snapshot folds the complete log); if
	// compaction cannot bound the count (impossible below L23), the save
	// refuses rather than emit an out-of-envelope package.
	RetainedOpsWriterTarget = 1 << 19
)
