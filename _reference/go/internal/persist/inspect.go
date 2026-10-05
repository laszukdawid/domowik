package persist

import (
	"io"
	"sort"
	"strings"
)

// inspect.go: the parse-only persistence inspection seam authorized by
// ED-0023 for issue 0027. It reuses the exact Load pipeline prefix
// (scanArchive) — container, names/namespace, manifest schema and V3
// gate, §9.3 limits, digest arbitration, snapshot-metadata ceiling, and
// structural batch framing — plus read-only DECODE of the same stages'
// stored metadata: the snapshot section's S1–S5 shapes and
// per-operation structural records from framed batch documents, all
// through the limited JSON decoder alone.
//
// NON-EXECUTION CONTRACT (ED-0023): Inspect never constructs document
// operations or batches (no buildBatchOperations/NewBatch), never
// replays a snapshot, never calls Deliver, and mutates nothing.
// Decoding stored metadata for reporting is not execution; both readers
// are the same parsers Load uses, preserving ED-0023's single-parser
// property. The differential proofs in inspect_test.go fail if replay,
// application, or operation construction ever enters this path.

// OpSummary is one operation's structural record: wire identity exactly
// as written. Absent identity members record as empty strings — their
// absence is a construction-stage defect the seam records, not rejects.
type OpSummary struct {
	OperationID string
	Kind        string
	EntityID    string
	HasPayload  bool
}

// BatchSummary is the structural framing record of one retained batch:
// envelope identity exactly as written on the wire. Actor identifiers in
// Frontier are raw text keys; the seam performs no UUID-type promotion
// beyond what framing validation already performed.
type BatchSummary struct {
	EntryName   string
	BatchID     string
	ActorID     string
	CounterBase int64
	EndCounter  int64 // CounterBase + OperationCount - 1
	Frontier    map[string]int64
	Operations  []OpSummary

	// UnknownMembers restates U2 bookkeeping for this batch document.
	UnknownMembers []string
}

// opKeySummary is one pre-ED-0037 (operationCounter, actorId) pair in wire
// text form. It survives only as the input to ED-0038's reading
// convention; nothing orders by it.
type opKeySummary struct {
	Counter int64
	Actor   string
}

// PositionSummary is one stored write position in wire-text form: the
// referenced §7.6 batch stem, its two parsed halves, and the operation
// index. `causalPastSize` is deliberately absent — it is the sum of the
// referenced frontier in BatchPositions, and restating it here would be a
// second source of truth (§5.1 S3).
type PositionSummary struct {
	Batch       string // entry stem, verbatim
	CounterBase int64
	Actor       string
	OpIndex     int64
}

// KeyedPositionSummary pairs an entity/field coordinate with its position.
// Field is empty for creation positions and tombstones.
type KeyedPositionSummary struct {
	Entity   string
	Field    string
	Position PositionSummary
}

// BatchFrontierSummary is one entry of the §5.1 S3 shared table.
type BatchFrontierSummary struct {
	Batch    string
	Frontier map[string]int64
}

// SnapshotSummary restates the snapshot section's S1–S5 metadata as
// raw, structurally-read wire text. It carries NO entity content: only
// the top-level member count of the entities map.
type SnapshotSummary struct {
	Present     bool
	EntityCount int
	Frontier    map[string]int64
	// Legacy reports that the document carried the pre-ED-0037
	// `winnerKeys`/`createdKeys`/`deletingKey` members and that the
	// positions below were produced by ED-0038's reading CONVENTION rather
	// than read from the file. V5 exactness is not claimed for them.
	Legacy           bool
	BatchPositions   []BatchFrontierSummary // ascending by stem
	WinnerPositions  []KeyedPositionSummary // ascending by raw coordinate text
	CreatedPositions []KeyedPositionSummary // ascending by raw coordinate text
	Tombstones       []KeyedPositionSummary // ascending by raw coordinate text
	RecentBatchIDs   []string               // verbatim stored order is not relied on; sorted

	// UnknownMembers restates U2 bookkeeping for the snapshot document.
	UnknownMembers []string
}

// Inspection is the outcome of a successful parse-only inspection. It
// deliberately carries no Projection and no Retained batches: only
// manifest identity, snapshot metadata, and batch framing summaries.
type Inspection struct {
	FormatVersion string
	Profile       string
	AssetRoot     []string

	Snapshot SnapshotSummary
	Batches  []BatchSummary // ascending by entry name, as Load orders them

	ManifestUnknown []string
	MissingAssets   []string
	Diagnostics     []string
}

// Inspect parses one package from an in-memory image without executing
// anything it contains.
func Inspect(data []byte) (*Inspection, error) {
	return inspectArchive(newByteReaderAt(data), int64(len(data)))
}

// InspectFile inspects one package from a path without executing
// anything it contains. Input handling is identical to LoadFile.
func InspectFile(path string) (*Inspection, error) {
	f, err := openFileReadOnly(path)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	st, err := f.Stat()
	if err != nil {
		return nil, fail(CatMalformedContainer, StageContainer, "unstatable input: %v", err)
	}
	if !st.Mode().IsRegular() {
		return nil, fail(CatMalformedContainer, StageContainer, "target is not a regular file")
	}
	return inspectArchive(f, st.Size())
}

func inspectArchive(src io.ReaderAt, size int64) (*Inspection, error) {
	sc, serr := scanArchive(src, size)
	if serr != nil {
		return nil, serr
	}
	view := sc.view

	// Snapshot metadata ceiling (L23): a byte-length cap on the section,
	// enforced without decoding its contents.
	if capErr := checkSnapshotMetadataCeiling(sc.snapshotInv); capErr != nil {
		return nil, capErr
	}

	insp := &Inspection{
		FormatVersion:   view.FormatVersion,
		Profile:         view.Profile,
		AssetRoot:       view.AssetRoot,
		ManifestUnknown: view.ManifestUnknown,
		MissingAssets:   view.MissingAssets,
		Diagnostics:     view.Diagnostics,
	}

	// Snapshot-metadata stage: read the section's S1–S5 shapes
	// structurally. No state is constructed or retained beyond wire-text
	// summaries; entity payloads are never touched at all.
	snap, serr2 := decodeSnapshotSummary(sc.snapshotRaw)
	if serr2 != nil {
		return nil, serr2
	}
	insp.Snapshot = *snap

	// Framing enumeration WITHOUT operation construction (§7.6):
	// envelopes, name/content binding, duplicate ids, reserved-range
	// structure. Operation payloads are recorded structurally (identity
	// members and payload PRESENCE), never decoded into operations.
	if len(sc.batchOrder) > LimitBatchesPerPackage {
		return nil, fail(CatLimitOperationCount, StageFraming,
			"batch count exceeds L23")
	}
	totalOps := 0
	seenBatchIDs := map[string]string{}
	rangesByActor := map[string][][2]int64{}
	for _, name := range sc.batchOrder {
		d := sc.batchDocs[name]
		if derr := bindNameToContent(name, d); derr != nil {
			return nil, derr
		}
		if prev := seenBatchIDs[d.batchID]; prev != "" {
			return nil, fail(CatMalformedFraming, StageFraming,
				"duplicate batch id %s in %q and %q", d.batchID, prev, name)
		}
		seenBatchIDs[d.batchID] = name
		ops, cerr := framedOperationRecords(d.raw)
		if cerr != nil {
			return nil, cerr
		}
		totalOps += len(ops)
		if totalOps > LimitTotalRetainedOps {
			return nil, fail(CatLimitOperationCount, StageFraming,
				"total retained operations exceed L23")
		}
		actorEnd := d.base + int64(len(ops)) - 1
		rangesByActor[d.actorID] = append(rangesByActor[d.actorID], [2]int64{d.base, actorEnd})
		frontier := make(map[string]int64, len(d.frontier))
		for actor, n := range d.frontier {
			frontier[actor] = n
		}
		s := BatchSummary{
			EntryName:   name,
			BatchID:     d.batchID,
			ActorID:     d.actorID,
			CounterBase: d.base,
			EndCounter:  actorEnd,
			Frontier:    frontier,
			Operations:  ops,
		}
		if unk, ok := d.unknownMembers(); ok {
			s.UnknownMembers = unk
		}
		insp.Batches = append(insp.Batches, s)
	}
	// Reserved-range structure is inside the seam's framing scope
	// (§7.6/ED-0023): overlapping or gapped actor ranges fail here with
	// the same malformed-framing token the replaying path produces.
	if rerr := checkReservedRanges(rangesByActor); rerr != nil {
		return nil, rerr
	}
	return insp, nil
}

// framedOperationRecords returns one OpSummary per entry of a framed
// batch document's operations array under the per-batch L23 cap. The
// array-level shape classification (missing/non-array member) mirrors
// buildBatchOperations exactly so both paths stay token-identical
// through framing; per-operation identity members are recorded as-is,
// with absent members recorded as empty strings because their required
// presence is enforced only at construction, which the seam never enters.
func framedOperationRecords(raw []byte) ([]OpSummary, *Error) {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return nil, perr // unreachable after parseBatchDocFraming; keep classification
	}
	obj, _ := root.(*jsonObject)
	opsv, _ := obj.get("operations")
	arr, ferr := asArray(opsv)
	if ferr != nil {
		return nil, ferr
	}
	if int64(len(arr)) > LimitOperationsPerBatch {
		return nil, fail(CatLimitOperationCount, StageFraming,
			"operations per batch exceeds L23")
	}
	out := make([]OpSummary, 0, len(arr))
	for _, item := range arr {
		o, oerr := asObject(item)
		if oerr != nil {
			return nil, oerr
		}
		s := OpSummary{}
		if v, ok := o.get("operationId"); ok {
			s.OperationID, _ = asString(v)
		}
		if v, ok := o.get("kind"); ok {
			s.Kind, _ = asString(v)
		}
		if v, ok := o.get("entityId"); ok {
			s.EntityID, _ = asString(v)
		}
		_, s.HasPayload = o.get("fields")
		out = append(out, s)
	}
	return out, nil
}

// decodeSnapshotSummary performs a STRICTLY STRUCTURAL read of one
// snapshot document's S1–S5 metadata shapes through the limited decoder
// alone: member presence, container shapes, integers, and raw strings.
// It never invokes parseSnapshot or DecodeEntity, never builds a
// document.State, and never validates semantic forms (UUIDs, key
// coordinates stay raw wire text). Values whose container shape makes
// faithful recording impossible reject with structural tokens; absent
// leaf members record as zero values because their REQUIRED-member
// enforcement belongs to stages the seam never enters.
func decodeSnapshotSummary(raw []byte) (*SnapshotSummary, *Error) {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return nil, perr
	}
	obj, oerr := asObject(root)
	if oerr != nil {
		return nil, oerr
	}
	s := &SnapshotSummary{Present: true, Frontier: map[string]int64{}}
	for _, k := range obj.keys {
		if !strings.Contains(snapshotClosedMembers, "\x00"+k) {
			s.UnknownMembers = append(s.UnknownMembers, k)
		}
	}
	if ev, present := obj.get("entities"); present {
		eo, ferr := asObject(ev)
		if ferr != nil {
			return nil, ferr
		}
		// Entity PAYLOADS are never touched: only the member count.
		s.EntityCount = len(eo.keys)
	}
	if fv, present := obj.get("frontier"); present {
		fo, ferr := asObject(fv)
		if ferr != nil {
			return nil, ferr
		}
		for _, actor := range fo.keys {
			n, okn := asInteger(mustGet(fo, actor))
			if !okn {
				return nil, fail(CatInvalidFieldSyntax, StageSchema,
					"frontier counter for %s is not an integer", actor)
			}
			// F1 zero prohibition (issue 0004): zero-valued frontier member is forbidden.
			if n == 0 {
				return nil, fail(CatInvalidFieldSyntax, StageSchema,
					"frontier counter for %s is zero (forbidden by FS §4.4 F1)", actor)
			}
			s.Frontier[actor] = n
		}
	}
	// Both encodings are read structurally. §5.1 makes them EXCLUSIVE, and
	// a document carrying members of both never reaches the semantic
	// parser; this summary is a raw structural read, so it reports whatever
	// members are present and marks the legacy case rather than judging it.
	readPositionMap := func(name string) ([]KeyedPositionSummary, *Error) {
		mv, present := obj.get(name)
		if !present {
			return nil, nil
		}
		mo, ferr := asObject(mv)
		if ferr != nil {
			return nil, ferr
		}
		out := make([]KeyedPositionSummary, 0, len(mo.keys))
		for _, coord := range mo.keys {
			w, werr := structReadPosition(mustGet(mo, coord))
			if werr != nil {
				return nil, werr
			}
			out = append(out, KeyedPositionSummary{
				Entity:   coord, // raw wire coordinate; never semantically split
				Position: w,
			})
		}
		sortKeyedPositions(out)
		return out, nil
	}
	readLegacyKeyMap := func(name string) ([]KeyedPositionSummary, *Error) {
		mv, present := obj.get(name)
		if !present {
			return nil, nil
		}
		s.Legacy = true
		mo, ferr := asObject(mv)
		if ferr != nil {
			return nil, ferr
		}
		out := make([]KeyedPositionSummary, 0, len(mo.keys))
		for _, coord := range mo.keys {
			k, kerr := structReadOpKey(mustGet(mo, coord))
			if kerr != nil {
				return nil, kerr
			}
			out = append(out, KeyedPositionSummary{Entity: coord, Position: conventionPosition(k)})
		}
		sortKeyedPositions(out)
		return out, nil
	}
	// Legacy PRESENCE is recorded before anything is read, and never
	// conditionally on the new family being absent. This summary is
	// structural: a snapshot carrying members of both families is rejected
	// by the semantic parser, and its job here is to REPORT what the bytes
	// contain, not to decide. Reading the legacy family only when the new
	// one is missing made a present `winnerPositions: {}` hide a populated
	// `winnerKeys` entirely — the summary silently denying the ambiguity
	// instead of surfacing it.
	for _, legacyMember := range [...]string{"winnerKeys", "createdKeys"} {
		if _, present := obj.get(legacyMember); present {
			s.Legacy = true
		}
	}
	var kerr *Error
	var legacyWinners, legacyCreated []KeyedPositionSummary
	if s.WinnerPositions, kerr = readPositionMap("winnerPositions"); kerr != nil {
		return nil, kerr
	}
	if legacyWinners, kerr = readLegacyKeyMap("winnerKeys"); kerr != nil {
		return nil, kerr
	}
	if s.WinnerPositions == nil {
		s.WinnerPositions = legacyWinners
	}
	if s.CreatedPositions, kerr = readPositionMap("createdPositions"); kerr != nil {
		return nil, kerr
	}
	if legacyCreated, kerr = readLegacyKeyMap("createdKeys"); kerr != nil {
		return nil, kerr
	}
	if s.CreatedPositions == nil {
		s.CreatedPositions = legacyCreated
	}
	if bpv, present := obj.get("batchPositions"); present {
		bo, ferr := asObject(bpv)
		if ferr != nil {
			return nil, ferr
		}
		s.BatchPositions = make([]BatchFrontierSummary, 0, len(bo.keys))
		for _, stem := range bo.keys {
			fo, oerr := asObject(mustGet(bo, stem))
			if oerr != nil {
				return nil, oerr
			}
			fr := map[string]int64{}
			for _, actor := range fo.keys {
				n, okn := asInteger(mustGet(fo, actor))
				if !okn {
					return nil, fail(CatInvalidFieldSyntax, StageSchema,
						"batchPositions %q frontier counter for %s is not an integer", stem, actor)
				}
				fr[actor] = n
			}
			s.BatchPositions = append(s.BatchPositions, BatchFrontierSummary{Batch: stem, Frontier: fr})
		}
		sort.Slice(s.BatchPositions, func(i, j int) bool {
			return s.BatchPositions[i].Batch < s.BatchPositions[j].Batch
		})
	}
	if tv, present := obj.get("tombstones"); present {
		to, ferr := asObject(tv)
		if ferr != nil {
			return nil, ferr
		}
		s.Tombstones = make([]KeyedPositionSummary, 0, len(to.keys))
		for _, entity := range to.keys {
			mo, mok := mustGet(to, entity).(*jsonObject)
			if !mok {
				return nil, fail(CatInvalidFieldSyntax, StageSchema,
					"tombstone %s is not an object", entity)
			}
			// Both inner shapes are probed, not one-then-the-other: a
			// tombstone carrying BOTH is exactly the ambiguity worth
			// surfacing, and returning on the first hit hid the second.
			dp, hasPos := mo.get("deletingPosition")
			dk, hasKey := mo.get("deletingKey")
			if hasKey {
				s.Legacy = true
			}
			if hasPos {
				w, werr := structReadPosition(dp)
				if werr != nil {
					return nil, werr
				}
				s.Tombstones = append(s.Tombstones, KeyedPositionSummary{Entity: entity, Position: w})
				continue
			}
			if !hasKey {
				return nil, fail(CatInvalidFieldSyntax, StageSchema,
					"tombstone %s carries neither deletingPosition nor deletingKey", entity)
			}
			k, kerr2 := structReadOpKey(dk)
			if kerr2 != nil {
				return nil, kerr2
			}
			s.Tombstones = append(s.Tombstones, KeyedPositionSummary{Entity: entity, Position: conventionPosition(k)})
		}
		sortKeyedPositions(s.Tombstones)
	}
	if bmv, present := obj.get("batchMetadata"); present {
		bmo, ferr := asObject(bmv)
		if ferr != nil {
			return nil, ferr
		}
		if idv, ok := bmo.get("recentBatchIds"); ok {
			arr, ferr := asArray(idv)
			if ferr != nil {
				return nil, ferr
			}
			s.RecentBatchIDs = make([]string, 0, len(arr))
			for _, item := range arr {
				text, serr2 := asString(item)
				if serr2 != nil {
					return nil, serr2
				}
				s.RecentBatchIDs = append(s.RecentBatchIDs, text)
			}
			sortStrings(s.RecentBatchIDs)
		}
	}
	return s, nil
}

// structReadOpKey reads one stored operation key's counter/actor pair
// as raw structural fields: an integer and a verbatim string. Absent
// members record as zero values; wrong-typed containers reject.
func structReadOpKey(v jsonValue) (opKeySummary, *Error) {
	o, ferr := asObject(v)
	if ferr != nil {
		return opKeySummary{}, ferr
	}
	k := opKeySummary{}
	if cv, ok := o.get("operationCounter"); ok {
		n, okn := asInteger(cv)
		if !okn {
			return opKeySummary{}, fail(CatInvalidFieldSyntax, StageSchema,
				"operationCounter is not an integer")
		}
		k.Counter = n
	}
	if av, ok := o.get("actorId"); ok {
		text, serr2 := asString(av)
		if serr2 != nil {
			return opKeySummary{}, serr2
		}
		k.Actor = text
	}
	return k, nil
}

// structReadPosition reads one stored position as raw structural fields:
// the batch stem verbatim plus an integer operation index. The stem's two
// halves are split for reporting only — a malformed stem is a semantic
// reject the parse pipeline raises, not this summary's business.
func structReadPosition(v jsonValue) (PositionSummary, *Error) {
	o, ferr := asObject(v)
	if ferr != nil {
		return PositionSummary{}, ferr
	}
	w := PositionSummary{}
	if bv, ok := o.get("batch"); ok {
		text, serr := asString(bv)
		if serr != nil {
			return PositionSummary{}, serr
		}
		w.Batch = text
		w.CounterBase, w.Actor = splitStemForReport(text)
	}
	if iv, ok := o.get("operationIndex"); ok {
		n, okn := asInteger(iv)
		if !okn {
			return PositionSummary{}, fail(CatInvalidFieldSyntax, StageSchema,
				"operationIndex is not an integer")
		}
		w.OpIndex = n
	}
	return w, nil
}

// conventionPosition renders a pre-ED-0037 key under ED-0038's documented
// reading convention: each retained key is its own one-operation batch, so
// counterBase = operationCounter and operationIndex = 0. It is a
// CONVENTION and the summary marks it as such via SnapshotSummary.Legacy;
// V5 exactness is not claimed for it.
func conventionPosition(k opKeySummary) PositionSummary {
	return PositionSummary{
		Batch:       stemText(k.Counter, k.Actor),
		CounterBase: k.Counter,
		Actor:       k.Actor,
	}
}

func stemText(counterBase int64, actor string) string {
	digits := []byte("0000000000000000000")
	n := counterBase
	for i := len(digits) - 1; i >= 0 && n > 0; i-- {
		digits[i] = byte('0' + n%10)
		n /= 10
	}
	return string(digits) + "-" + actor
}

// splitStemForReport recovers the two halves of a well-formed stem. A stem
// that does not have the fixed §7.6 shape reports zero and the whole text,
// because this summary never rejects on semantics.
func splitStemForReport(stem string) (int64, string) {
	const digits = 19
	if len(stem) < digits+1 || stem[digits] != '-' {
		return 0, stem
	}
	var base int64
	for i := 0; i < digits; i++ {
		c := stem[i]
		if c < '0' || c > '9' {
			return 0, stem
		}
		base = base*10 + int64(c-'0')
	}
	return base, stem[digits+1:]
}

// sortKeyedPositions orders summaries by their raw coordinate text so
// reporting is deterministic regardless of map iteration order.
func sortKeyedPositions(list []KeyedPositionSummary) {
	for i := 1; i < len(list); i++ {
		for j := i; j > 0 && keyedLess(list[j], list[j-1]); j-- {
			list[j], list[j-1] = list[j-1], list[j]
		}
	}
}

func keyedLess(a, b KeyedPositionSummary) bool {
	if a.Entity != b.Entity {
		return a.Entity < b.Entity
	}
	return a.Field < b.Field
}
