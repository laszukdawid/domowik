package persist

import (
	"os"
	"sort"

	"domowik.dev/domowik/internal/document"
)

// reader_helpers.go: pipeline support pieces shared by the Load path.

func openFileReadOnly(path string) (*os.File, error) {
	return os.Open(path) // read-only handle; regular-file check happens in LoadFile
}

// jsonObjectKeys returns the top-level member names of a decoded
// document for unknown-member bookkeeping (U2).
func jsonObjectKeys(root jsonValue) []string {
	if o, ok := root.(*jsonObject); ok {
		return o.keys
	}
	return nil
}

func manifestObjectKeys(root jsonValue) []string { return jsonObjectKeys(root) }

// parseBatchDocFraming wraps raw-batch parsing: torn or partial batch
// documents are pre-replay framing rejects regardless of the underlying
// syntax classification (§7.6/§4.3 row 4).
func parseBatchDocFraming(raw []byte) (*batchDoc, *Error) {
	d, perr := parseBatchDoc(raw)
	if perr != nil {
		if perr.Category == CatLimitNumberToken || perr.Category == CatLimitNestingDepth ||
			perr.Category == CatNonfiniteValue || perr.Category == CatNumericOverflow ||
			perr.Category == CatLimitArrayCount || perr.Category == CatLimitMemberCount {
			return nil, perr // parser-security rows keep their exact tokens
		}
		return nil, fail(CatMalformedFraming, StageFraming,
			"torn or partial batch document: %s", perr.Detail)
	}
	return d, nil
}

// bindNameToContent enforces §7.6 name/content binding: a batch whose
// embedded batchId, actorId, or counterBase contradicts its entry name
// rejects before replay begins.
func bindNameToContent(entryName string, d *batchDoc) *Error {
	body := entryName[len("ops/") : len(entryName)-len(".json")]
	counterText, actorText, _ := stringsCut(body, "-")
	want := padCounter(d.base)
	if counterText != want || actorText != d.actorID {
		return fail(CatMalformedFraming, StageFraming,
			"entry name %q contradicts embedded identity (counterBase %d, actor %s)",
			entryName, d.base, d.actorID)
	}
	return nil
}

func stringsCut(s, sep string) (string, string, bool) {
	for i := 0; i+len(sep) <= len(s); i++ {
		if s[i:i+len(sep)] == sep {
			return s[:i], s[i+len(sep):], true
		}
	}
	return s, "", false
}

// checkReservedRanges enforces B3 structure across one actor's retained
// stream: ranges are disjoint and strictly increasing with no gaps
// between successive batches. A gap before an actor's earliest retained
// batch is the snapshot-covered prefix (implied reservation [1..W]) and
// is not a defect; gaps between retained batches always are.
func checkReservedRanges(rangesByActor map[string][][2]int64) *Error {
	for actor, rs := range rangesByActor {
		sort.Slice(rs, func(i, j int) bool { return rs[i][0] < rs[j][0] })
		for i := 1; i < len(rs); i++ {
			switch {
			case rs[i][0] <= rs[i-1][1]:
				return fail(CatMalformedFraming, StageFraming,
					"actor %s reserves overlapping ranges [%d,%d] and [%d,%d]",
					actor, rs[i-1][0], rs[i-1][1], rs[i][0], rs[i][1])
			case rs[i][0] != rs[i-1][1]+1:
				return fail(CatMalformedFraming, StageFraming,
					"actor %s has a gap between reserved ranges [%d,%d] and [%d,%d]",
					actor, rs[i-1][0], rs[i-1][1], rs[i][0], rs[i][1])
			}
		}
	}
	return nil
}

// framingErr maps document-layer structural failures to their taxonomy
// classes: schema field errors keep their tokens; everything else in the
// framing/replay boundary is malformed-framing (§4.3 row 4, §9.4).
func framingErr(err error) *Error {
	if fe, ok := err.(*document.FieldError); ok {
		return &Error{Category: Category(fe.Code), Stage: StageSchema, Detail: fe.Error()}
	}
	return fail(CatMalformedFraming, StageFraming, "%v", err)
}

func checkSnapshotMetadataCeiling(inv InventoryEntry) *Error {
	if inv.ByteLength > LimitSnapshotMetadataByte {
		return fail(CatLimitSnapshotMeta, StageSchema,
			"snapshot metadata of %d bytes exceeds %d (L23)",
			inv.ByteLength, LimitSnapshotMetadataByte)
	}
	return nil
}

// ParseSnapshotDocument parses one §5.1 snapshot document into its
// S1–S4 typed state (plus S5 recent batch ids). It backs the package
// pipeline and conformance harnesses; unknown-member bookkeeping is the
// caller's concern via Load.
//
// It is PARSE-ONLY: it holds no retained log. A legacy snapshot carrying a
// tombstone therefore fails closed here rather than being converted,
// because §5.1 makes that case recoverable only from the deleted id's
// retained creating batch. Only Load, which has the batches, may attempt
// that recovery. A parser that sometimes converts and sometimes does not
// would be a worse contract than one that never does; a parser that
// guessed the creation side would be silently wrong.
func ParseSnapshotDocument(raw []byte) (document.State, []string, *Error) {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return document.State{}, nil, perr
	}
	doc, serr := parseSnapshot(root)
	if serr != nil {
		return document.State{}, nil, serr
	}
	if doc.Legacy != nil {
		st, cerr := ConvertLegacySnapshot(doc.Legacy, nil)
		if cerr != nil {
			return document.State{}, nil, cerr
		}
		return st, doc.RecentBatchIDs, nil
	}
	return doc.State, doc.RecentBatchIDs, nil
}

// ParseLimitedJSON exposes the limited decoder for parser-security
// conformance probes (L11/L12/L13/L17/L18 classification).
func ParseLimitedJSON(raw []byte) error {
	if _, err := parseLimitedJSON(raw); err != nil {
		return err
	}
	return nil
}

// ValidateTimestamp exposes the L19 RFC 3339 UTC fixed-form check.
func ValidateTimestamp(s string) error {
	if terr := validateTimestamp(s); terr != nil {
		return terr
	}
	return nil
}

// ValidateDigestID exposes the L19 digest-identifier form check.
func ValidateDigestID(s string) error {
	if verr := validateDigestID(s); verr != nil {
		return verr
	}
	return nil
}

// ParseBatchDocument parses one §4.9 wire batch document into a
// validated document.Batch (per-batch framing checks included). It backs
// the package pipeline's ops decoding and serves conformance harnesses.
func ParseBatchDocument(raw []byte) (*document.Batch, *Error) {
	d, perr := parseBatchDocFraming(raw)
	if perr != nil {
		return nil, perr
	}
	ops, oerr := buildBatchOperations(d)
	if oerr != nil {
		return nil, oerr
	}
	b, berr := document.NewBatch(mustUUID(d.batchID), mustUUID(d.actorID), d.base, frontierOf(d), ops...)
	if berr != nil {
		return nil, framingErr(berr)
	}
	return b, nil
}

// descriptor requirements enum (ASSET-STRATEGY §10.2 as cited by §7.4).
var requirementEnum = map[string]bool{
	"required": true,
	"optional": true,
}

// parseDescriptors validates assets/descriptors.json: ONE object keyed
// by digest; each member's digest field must equal its key; L19 bounds
// apply to mediaType and timestamps.
func parseDescriptors(raw []byte) *Error {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return perr
	}
	obj, ok := root.(*jsonObject)
	if !ok {
		return fail(CatInvalidFieldSyntax, StageSchema, "descriptor document must be one object keyed by digest")
	}
	for _, key := range obj.keys {
		if verr := validateDigestID(key); verr != nil {
			return verr
		}
		member, mok := obj.values[key].(*jsonObject)
		if !mok {
			return fail(CatInvalidFieldSyntax, StageSchema, "descriptor member %q must be an object", key)
		}
		if dv, ok := member.get("digest"); ok {
			s, serr := asString(dv)
			if serr != nil || s != key {
				return fail(CatInvalidFieldSyntax, StageSchema,
					"descriptor member digest must equal its key")
			}
		} else {
			return fail(CatInvalidFieldSyntax, StageSchema, "descriptor member lacks digest")
		}
		if mv, ok := member.get("mediaType"); ok {
			s, serr := asString(mv)
			if serr != nil {
				return serr
			}
			if len(s) > MediaTypeMaxBytes {
				return fail(CatInvalidFieldSyntax, StageSchema,
					"mediaType exceeds %d bytes (L19)", MediaTypeMaxBytes)
			}
		}
		if lv, ok := member.get(fieldByteLength); ok {
			n, okn := asInteger(lv)
			if !okn || n < 0 {
				return fail(CatInvalidFieldSyntax, StageSchema, "byteLength must be a non-negative integer")
			}
		}
		if rv, ok := member.get("requirement"); ok {
			s, serr := asString(rv)
			if serr != nil || !requirementEnum[s] {
				return fail(CatInvalidFieldSyntax, StageSchema, "requirement must be required or optional")
			}
		}
		if pv, ok := member.get("provenance"); ok {
			po, pok := pv.(*jsonObject)
			if !pok {
				return fail(CatInvalidFieldSyntax, StageSchema, "provenance must be an object")
			}
			if av, ok := po.get("acquiredAt"); ok {
				s, serr := asString(av)
				if serr != nil {
					return serr
				}
				if terr := validateTimestamp(s); terr != nil {
					return terr
				}
			}
			if dv, ok := po.get("displayName"); ok {
				if _, serr := asString(dv); serr != nil {
					return serr
				}
			}
		}
	}
	return nil
}
