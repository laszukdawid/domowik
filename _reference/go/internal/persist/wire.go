package persist

import (
	"bytes"
	"encoding/json"
	"math"
	"sort"
	"strings"

	"domowik.dev/domowik/internal/document"
)

// wire.go: typed decoding/encoding of the v0.1 wire documents — the
// §7.4 manifest, the §7.5 snapshot section (S1–S5 member shapes owned by
// §5.1), and the §7.6 operation-batch documents. Field syntax follows
// row L19; unknown REQUIRED constructs refuse safely (U1); the batch
// document tolerates documentation-only extra members because fixtures
// pin them as non-semantic annotations.

// InventoryEntry is one manifest entries value.
type InventoryEntry struct {
	SHA256     string // digest identifier, exactly 71 characters
	ByteLength uint64
}

// Manifest is the decoded domowik.json.
type Manifest struct {
	FormatVersion string
	Profile       string
	AssetRoot     []string // digest identifiers, ascending, unique
	Entries       map[string]InventoryEntry
}

const (
	profileThin     = "thin"
	profilePortable = "portable"

	formatVersionV01 = "0.1" // sole accepted schema label (§8.2 V3)

	fieldFormatVersion = "formatVersion"
	fieldProfile       = "profile"
	fieldAssetRoot     = "assetRoot"
	fieldEntries       = "entries"
	fieldDigest        = "digest"
	fieldSHA256        = "sha256"
	fieldByteLength    = "byteLength"
)

func parseManifest(root jsonValue) (*Manifest, *Error) {
	obj, err := asObject(root)
	if err != nil {
		return nil, err
	}
	m := &Manifest{Entries: map[string]InventoryEntry{}}
	fv, ok := obj.get(fieldFormatVersion)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageManifest, "manifest %s absent", fieldFormatVersion)
	}
	s, ferr := asString(fv)
	if ferr != nil {
		return nil, ferr
	}
	if !isSchemaLabel(s) {
		return nil, fail(CatUnsupportedVersion, StageManifest,
			"schema label %q is not a decimal MAJOR.MINOR form", s)
	}
	m.FormatVersion = s

	pv, ok := obj.get(fieldProfile)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageManifest, "manifest %s absent", fieldProfile)
	}
	s, ferr = asString(pv)
	if ferr != nil {
		return nil, ferr
	}
	if s != profileThin && s != profilePortable {
		return nil, fail(CatInvalidFieldSyntax, StageManifest, "profile must be thin or portable")
	}
	m.Profile = s

	arv, ok := obj.get(fieldAssetRoot)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageManifest, "manifest %s absent", fieldAssetRoot)
	}
	// The wire shape permits an explicitly empty root table; generators
	// may serialize an absent table as null (§7.4 example shows a
	// non-empty one). Only a wrong-typed non-null value rejects.
	if _, isNull := arv.(jsonNull); !isNull {
		arr, ferr := asArray(arv)
		if ferr != nil {
			return nil, ferr
		}
		prev := ""
		for i, item := range arr {
			o, ferr := asObject(item)
			if ferr != nil {
				return nil, ferr
			}
			dv, ok := o.get(fieldDigest)
			if !ok {
				return nil, fail(CatInvalidFieldSyntax, StageManifest, "assetRoot[%d] digest absent", i)
			}
			d, ferr := asString(dv)
			if ferr != nil {
				return nil, ferr
			}
			if derr := validateDigestID(d); derr != nil {
				return nil, derr
			}
			if prev != "" && d <= prev {
				return nil, fail(CatInvalidFieldSyntax, StageManifest,
					"assetRoot must be ascending by digest with no duplicates")
			}
			prev = d
			m.AssetRoot = append(m.AssetRoot, d)
		}
	}

	ev, ok := obj.get(fieldEntries)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageManifest, "manifest %s absent", fieldEntries)
	}
	eo, ferr := asObject(ev)
	if ferr != nil {
		return nil, ferr
	}
	for _, name := range eo.keys {
		io2, _ := eo.values[name].(*jsonObject)
		if io2 == nil {
			return nil, fail(CatInvalidFieldSyntax, StageManifest, "entries[%q] must be an object", name)
		}
		inv := InventoryEntry{}
		dv, ok := io2.get(fieldSHA256)
		if !ok {
			return nil, fail(CatInvalidFieldSyntax, StageManifest, "entries[%q] sha256 absent", name)
		}
		inv.SHA256, ferr = asString(dv)
		if ferr != nil {
			return nil, ferr
		}
		if derr := validateDigestID(inv.SHA256); derr != nil {
			return nil, derr
		}
		lv, ok := io2.get(fieldByteLength)
		if !ok {
			return nil, fail(CatInvalidFieldSyntax, StageManifest, "entries[%q] byteLength absent", name)
		}
		n, okn := asInteger(lv)
		if !okn || n < 0 {
			return nil, fail(CatInvalidFieldSyntax, StageManifest, "entries[%q] byteLength must be a non-negative integer", name)
		}
		inv.ByteLength = uint64(n)
		m.Entries[name] = inv
	}
	return m, nil
}

// isSchemaLabel validates the V1 decimal MAJOR.MINOR form without
// leading zeros. Acceptance of a specific label is the reader gate V3.
func isSchemaLabel(s string) bool {
	major, minor, found := strings.Cut(s, ".")
	if !found || len(major) == 0 || len(minor) == 0 {
		return false
	}
	if !allDigits(major) || !allDigits(minor) {
		return false
	}
	if len(major) > 1 && major[0] == '0' {
		return false
	}
	if len(minor) > 1 && minor[0] == '0' {
		return false
	}
	return true
}

func validateDigestID(s string) *Error {
	if len(s) != DigestIdentifierLength || !strings.HasPrefix(s, "sha256:") {
		return fail(CatInvalidFieldSyntax, StageSchema,
			"digest identifier must be exactly %d characters of sha256: + lowercase hex (L19)", DigestIdentifierLength)
	}
	if !isLowerHex(s[7:], 64) {
		return fail(CatInvalidFieldSyntax, StageSchema, "digest hex must be 64 lowercase hex characters")
	}
	return nil
}

// ---------------------------------------------------------------------------
// Snapshot document (S1–S5).

// SnapshotDoc is one parsed §5.1 snapshot document. Exactly one of State
// and Legacy is meaningful: the two encodings are EXCLUSIVE, and a
// document carrying members of both never reaches here — it rejects as
// ambiguous required data.
type SnapshotDoc struct {
	State          document.State
	RecentBatchIDs []string // S5 batch/dedup metadata
	// Legacy is non-nil when the document carries the pre-ED-0037
	// `winnerKeys`, `createdKeys`, or `deletingKey` members. Conversion
	// needs the retained log for tombstones, which this layer does not
	// have, so it is deliberately left unconverted here.
	Legacy *LegacySnapshot
}

// legacyKey is the pre-ED-0037 (operationCounter, actorId) pair. It is not
// an ordering key any more: it cannot place a write against a later
// concurrent one, which is exactly why ED-0037 replaced it.
type legacyKey struct {
	Counter int64
	Actor   document.UUID
}

// LegacySnapshot is the unconverted pre-ED-0037 snapshot. It is carried
// rather than converted because §5.1's tombstone rule needs evidence this
// layer does not hold: a legacy snapshot records a `deletingKey` but, since
// creation metadata was live-entity-only, no creation key for a deleted id,
// so the live interval can be reconstructed only from a retained creating
// batch. Guessing the creation side would apply a late batch positioned
// before the true creation where full replay discards it whole — a silently
// wrong projection, which is worse than a refusal.
type LegacySnapshot struct {
	Entities    []document.Entity
	Frontier    map[document.UUID]int64
	WinnerKeys  map[document.FieldKey]legacyKey
	CreatedKeys map[document.UUID]legacyKey
	Tombstones  map[document.UUID]legacyKey // deletingKey
}

// HasTombstones reports whether conversion needs retained-log evidence.
func (l *LegacySnapshot) HasTombstones() bool { return len(l.Tombstones) > 0 }

// actorUniverse is §5.1's selection predicate for the legacy reading mode:
// the UNION of the actors named in the frontier and those named by the
// retained winner, creation, and tombstone keys. The frontier alone does
// not determine the set — a key actor need not appear in it — and reading
// the frontier alone would route a snapshot whose keys span two actors into
// the single-actor convention (ED-0040 queue item 9, closed in 3cca9ee).
func (l *LegacySnapshot) actorUniverse() map[document.UUID]bool {
	set := map[document.UUID]bool{}
	for a := range l.Frontier {
		set[a] = true
	}
	for _, k := range l.WinnerKeys {
		set[k.Actor] = true
	}
	for _, k := range l.CreatedKeys {
		set[k.Actor] = true
	}
	for _, k := range l.Tombstones {
		set[k.Actor] = true
	}
	return set
}

// IsEmptyProjection reports §7.5's brand-new document: empty entities,
// empty frontier, and no winner, creation, or tombstone entries. It has
// nothing to place, so it reads as the empty projection and no convention
// applies.
func (l *LegacySnapshot) IsEmptyProjection() bool {
	return len(l.Entities) == 0 && len(l.Frontier) == 0 &&
		len(l.WinnerKeys) == 0 && len(l.CreatedKeys) == 0 && len(l.Tombstones) == 0
}

type opKeyWire struct {
	counter int64
	actor   string
}

// positionWire is the §5.1 S3 reference shape: a batch stem plus an
// operation index. `counterBase` and `actorId` are read from the stem and
// `causalPastSize` is the sum of the referenced frontier, so no member
// carries an integer outside L13.
type positionWire struct {
	stem    string
	opIndex int64
}

func parseSnapshot(root jsonValue) (*SnapshotDoc, *Error) {
	obj, err := asObject(root)
	if err != nil {
		return nil, err
	}
	doc := &SnapshotDoc{}

	// Encoding selection is by member PRESENCE, never by entry count. An
	// empty object is still a present member, and a `len(map) > 0` test
	// would read `"winnerPositions": {}` alongside `"createdKeys": {}` as
	// carrying neither family — the empty-collection vacuity ED-0041
	// records as its third instance.
	_, hasWinnerPositions := obj.get("winnerPositions")
	_, hasCreatedPositions := obj.get("createdPositions")
	_, hasBatchPositions := obj.get("batchPositions")
	_, hasWinnerKeys := obj.get("winnerKeys")
	_, hasCreatedKeys := obj.get("createdKeys")

	entities, err := parseSnapshotEntities(obj)
	if err != nil {
		return nil, err
	}
	frontier, err := parseSnapshotFrontier(obj)
	if err != nil {
		return nil, err
	}
	tombRaw, hasTombstones, err := parseSnapshotTombstoneShapes(obj)
	if err != nil {
		return nil, err
	}
	_ = hasTombstones

	newShape := hasWinnerPositions || hasCreatedPositions || hasBatchPositions || tombRaw.hasPositions
	legacyShape := hasWinnerKeys || hasCreatedKeys || tombRaw.hasKeys
	if newShape && legacyShape {
		// §5.1: "A snapshot carrying members of both — any position-bearing
		// member alongside any pre-ED-0037 member — is neither a conforming
		// snapshot nor a legacy one, and REJECTS: it is ambiguous required
		// data ... and no reader may convert it partly."
		return nil, fail(CatInvalidFieldSyntax, StageSchema,
			"snapshot carries both position-bearing and pre-ED-0037 members: ambiguous required data (FS §5.1)")
	}

	if !legacyShape {
		// §5.1: "A snapshot contains ALL of" S1–S5. Absence is absent
		// required data, not an empty default, and the check is by member
		// PRESENCE so an explicitly empty object still satisfies it.
		//
		// This is a REQUIRED-MEMBER assertion rather than a note, because
		// the shape it rejects — `{}` parsing into a silently empty
		// projection — is ED-0041's third instance recurring in new code
		// written after that record existed. A per-entry rule iterating an
		// absent map is indistinguishable from one iterating an empty map,
		// and neither says anything.
		//
		// The LEGACY path is deliberately not held to this list: §5.1
		// blesses the empty legacy snapshot ("empty entities, empty
		// frontier, and no winner, creation, or tombstone entries") in
		// terms, and ED-0038's convention governs what a pre-ED-0037
		// artifact carries. Requiring the new member set of an old file
		// would reject the very packages the convention exists to read.
		for _, name := range [...]string{
			"entities", "frontier", "batchPositions",
			"winnerPositions", "createdPositions", "tombstones", "batchMetadata",
		} {
			if _, present := obj.get(name); !present {
				return nil, fail(CatInvalidFieldSyntax, StageSchema,
					"snapshot omits required member %q: FS §5.1 says a snapshot contains ALL of S1-S5", name)
			}
		}
	}

	if serr := parseSnapshotBatchMetadata(obj, doc); serr != nil {
		return nil, serr
	}

	if legacyShape {
		l := &LegacySnapshot{
			Entities:    entities,
			Frontier:    frontier,
			WinnerKeys:  map[document.FieldKey]legacyKey{},
			CreatedKeys: map[document.UUID]legacyKey{},
			Tombstones:  tombRaw.keys,
		}
		if wkv, present := obj.get("winnerKeys"); present {
			if err := parseOpKeyMap(wkv, ".", func(entity document.UUID, field string, k opKeyWire) *Error {
				a, uerr := parseUUIDText(k.actor)
				if uerr != nil {
					return uerr
				}
				l.WinnerKeys[document.NewFieldKey(entity, field)] = legacyKey{Counter: k.counter, Actor: a}
				return nil
			}); err != nil {
				return nil, err
			}
		}
		if ckv, present := obj.get("createdKeys"); present {
			if err := parseOpKeyMap(ckv, ".", func(entity document.UUID, field string, k opKeyWire) *Error {
				if field != "" {
					return fail(CatInvalidFieldSyntax, StageSchema, "createdKeys values use bare entity keys")
				}
				a, uerr := parseUUIDText(k.actor)
				if uerr != nil {
					return uerr
				}
				l.CreatedKeys[entity] = legacyKey{Counter: k.counter, Actor: a}
				return nil
			}); err != nil {
				return nil, err
			}
		}
		doc.Legacy = l
		return doc, nil
	}

	st := document.State{
		Entities:         entities,
		Frontier:         frontier,
		BatchPositions:   map[document.BatchRef]map[document.UUID]int64{},
		WinnerPositions:  map[document.FieldKey]document.Position{},
		CreatedPositions: map[document.UUID]document.Position{},
		Tombstones:       map[document.UUID]document.Position{},
	}
	if bpv, present := obj.get("batchPositions"); present {
		bp, ferr := asObject(bpv)
		if ferr != nil {
			return nil, ferr
		}
		for _, stem := range sortedKeys(bp) {
			ref, rerr := parseBatchStem(stem)
			if rerr != nil {
				return nil, rerr
			}
			fo, oerr := asObject(mustGet(bp, stem))
			if oerr != nil {
				return nil, oerr
			}
			fr := map[document.UUID]int64{}
			for _, actorText := range sortedKeys(fo) {
				actor, uerr := parseUUIDText(actorText)
				if uerr != nil {
					return nil, uerr
				}
				n, okn := asInteger(mustGet(fo, actorText))
				if !okn || n < 0 {
					return nil, fail(CatInvalidFieldSyntax, StageSchema,
						"batchPositions %q frontier counter for %s must be a non-negative integer", stem, actorText)
				}
				fr[actor] = n
			}
			st.BatchPositions[ref] = fr
		}
	}
	if wpv, present := obj.get("winnerPositions"); present {
		if err := parsePositionMap(wpv, ".", func(entity document.UUID, field string, w positionWire) *Error {
			pos, perr := makePosition(w)
			if perr != nil {
				return perr
			}
			st.WinnerPositions[document.NewFieldKey(entity, field)] = pos
			return nil
		}); err != nil {
			return nil, err
		}
	}
	if cpv, present := obj.get("createdPositions"); present {
		// Keyed by ENTITY, bare (§5.1 S1) — unlike winnerPositions, whose
		// key is "<entityId>.<fieldName>".
		co, ferr := asObject(cpv)
		if ferr != nil {
			return nil, ferr
		}
		for _, idText := range sortedKeys(co) {
			id, uerr := parseUUIDText(idText)
			if uerr != nil {
				return nil, uerr
			}
			w, werr := parsePositionStruct(mustGet(co, idText))
			if werr != nil {
				return nil, werr
			}
			pos, perr := makePosition(w)
			if perr != nil {
				return nil, perr
			}
			st.CreatedPositions[id] = pos
		}
	}
	for id, w := range tombRaw.positions {
		pos, perr := makePosition(w)
		if perr != nil {
			return nil, perr
		}
		st.Tombstones[id] = pos
	}
	doc.State = st
	return doc, nil
}

func parseSnapshotEntities(obj *jsonObject) ([]document.Entity, *Error) {
	ev, present := obj.get("entities")
	var entObj *jsonObject
	if present {
		var err *Error
		if entObj, err = asObject(ev); err != nil {
			return nil, err
		}
	}
	var out []document.Entity
	for _, idText := range sortedKeys(entObj) {
		raw := entObj.raws[idText]
		ent, derr := document.DecodeEntity(raw)
		if derr != nil {
			return nil, schemaErr(derr)
		}
		id, uerr := parseUUIDText(idText)
		if uerr != nil {
			return nil, uerr
		}
		if id != ent.EntityID() {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"snapshot entity key %s does not match entity id", idText)
		}
		out = append(out, ent)
	}
	return out, nil
}

func parseSnapshotFrontier(obj *jsonObject) (map[document.UUID]int64, *Error) {
	out := map[document.UUID]int64{}
	fv, present := obj.get("frontier")
	var fObj *jsonObject
	if present {
		var err *Error
		if fObj, err = asObject(fv); err != nil {
			return nil, err
		}
	}
	for _, actorText := range sortedKeys(fObj) {
		actor, uerr := parseUUIDText(actorText)
		if uerr != nil {
			return nil, uerr
		}
		n, okn := asInteger(mustGet(fObj, actorText))
		if !okn || n < 0 {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"frontier counter for %s must be a non-negative integer", actorText)
		}
		// F1 zero prohibition (issue 0004): zero-valued frontier member is forbidden.
		// Absence (actor omitted) is the sole representation of "nothing observed".
		if n == 0 {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"frontier counter for %s is zero (forbidden by FS §4.4 F1)", actorText)
		}
		out[actor] = n
	}
	return out, nil
}

// tombstoneShapes separates the two tombstone encodings. `tombstones` is
// the one member name both shapes share, so the discriminator is the INNER
// member — `deletingKey` (pre-ED-0037) versus `deletingPosition` — and a
// single tombstone carrying both is itself ambiguous.
type tombstoneShapes struct {
	keys         map[document.UUID]legacyKey
	positions    map[document.UUID]positionWire
	hasKeys      bool
	hasPositions bool
}

func parseSnapshotTombstoneShapes(obj *jsonObject) (tombstoneShapes, bool, *Error) {
	out := tombstoneShapes{
		keys:      map[document.UUID]legacyKey{},
		positions: map[document.UUID]positionWire{},
	}
	tbv, present := obj.get("tombstones")
	if !present {
		return out, false, nil
	}
	tbObj, ferr := asObject(tbv)
	if ferr != nil {
		return out, true, ferr
	}
	for _, idText := range sortedKeys(tbObj) {
		id, uerr := parseUUIDText(idText)
		if uerr != nil {
			return out, true, uerr
		}
		o, oerr := asObject(mustGet(tbObj, idText))
		if oerr != nil {
			return out, true, oerr
		}
		dkRaw, hasKey := o.get("deletingKey")
		dpRaw, hasPos := o.get("deletingPosition")
		switch {
		case hasKey && hasPos:
			return out, true, fail(CatInvalidFieldSyntax, StageSchema,
				"tombstone %s carries both deletingKey and deletingPosition: ambiguous required data (FS §5.1)", idText)
		case hasKey:
			out.hasKeys = true
			k, kerr := parseOpKeyStruct(dkRaw)
			if kerr != nil {
				return out, true, kerr
			}
			a, aerr := parseUUIDText(k.actor)
			if aerr != nil {
				return out, true, aerr
			}
			out.keys[id] = legacyKey{Counter: k.counter, Actor: a}
		case hasPos:
			out.hasPositions = true
			w, werr := parsePositionStruct(dpRaw)
			if werr != nil {
				return out, true, werr
			}
			out.positions[id] = w
		default:
			return out, true, fail(CatInvalidFieldSyntax, StageSchema,
				"tombstone %s lacks deletingPosition", idText)
		}
	}
	return out, true, nil
}

func parseSnapshotBatchMetadata(obj *jsonObject, doc *SnapshotDoc) *Error {
	bmv, present := obj.get("batchMetadata")
	if !present {
		return nil
	}
	bm, ferr := asObject(bmv)
	if ferr != nil {
		return ferr
	}
	idv, ok := bm.get("recentBatchIds")
	if !ok {
		return fail(CatInvalidFieldSyntax, StageSchema, "batchMetadata lacks recentBatchIds")
	}
	arr, aerr := asArray(idv)
	if aerr != nil {
		return aerr
	}
	for _, item := range arr {
		str, serr := asString(item)
		if serr != nil {
			return serr
		}
		if _, uerr := parseUUIDText(str); uerr != nil {
			return uerr
		}
		doc.RecentBatchIDs = append(doc.RecentBatchIDs, str)
	}
	sort.Strings(doc.RecentBatchIDs)
	return nil
}

// parseBatchStem reads a §7.6 entry stem
// `<counterBase padded to 19 digits>-<actorId>`. The padding is what makes
// byte order equal ascending (counterBase, actorId), so a short or unpadded
// form is not the same name and is rejected rather than normalized.
func parseBatchStem(stem string) (document.BatchRef, *Error) {
	const digits = 19
	if len(stem) != digits+1+36 || stem[digits] != '-' {
		return document.BatchRef{}, fail(CatInvalidFieldSyntax, StageSchema,
			"batch stem %q is not <19-digit counterBase>-<actorId>", stem)
	}
	var base int64
	for i := 0; i < digits; i++ {
		c := stem[i]
		if c < '0' || c > '9' {
			return document.BatchRef{}, fail(CatInvalidFieldSyntax, StageSchema,
				"batch stem %q counterBase is not decimal", stem)
		}
		d := int64(c - '0')
		if base > (math.MaxInt64-d)/10 {
			return document.BatchRef{}, fail(CatNumericOverflow, StageSchema,
				"batch stem %q counterBase overflows signed 64-bit (L13)", stem)
		}
		base = base*10 + d
	}
	if base < 1 {
		return document.BatchRef{}, fail(CatInvalidFieldSyntax, StageSchema,
			"batch stem %q counterBase must be >= 1 (B3)", stem)
	}
	actor, uerr := parseUUIDText(stem[digits+1:])
	if uerr != nil {
		return document.BatchRef{}, uerr
	}
	return document.NewBatchRef(base, actor), nil
}

func makePosition(w positionWire) (document.Position, *Error) {
	ref, rerr := parseBatchStem(w.stem)
	if rerr != nil {
		return document.Position{}, rerr
	}
	pos, err := document.NewPosition(ref, w.opIndex)
	if err != nil {
		return document.Position{}, fail(CatInvalidFieldSyntax, StageSchema, "%v", err)
	}
	return pos, nil
}

func parsePositionMap(v jsonValue, sep string, emit func(document.UUID, string, positionWire) *Error) *Error {
	o, err := asObject(v)
	if err != nil {
		return err
	}
	for _, key := range sortedKeys(o) {
		entityText, field, found := strings.Cut(key, sep)
		if !found {
			return fail(CatInvalidFieldSyntax, StageSchema, "position map entry %q malformed", key)
		}
		entity, uerr := parseUUIDText(entityText)
		if uerr != nil {
			return uerr
		}
		w, werr := parsePositionStruct(mustGet(o, key))
		if werr != nil {
			return werr
		}
		if eerr := emit(entity, field, w); eerr != nil {
			return eerr
		}
	}
	return nil
}

func parsePositionStruct(v jsonValue) (positionWire, *Error) {
	o, err := asObject(v)
	if err != nil {
		return positionWire{}, err
	}
	bv, ok := o.get("batch")
	if !ok {
		return positionWire{}, fail(CatInvalidFieldSyntax, StageSchema, "position lacks batch")
	}
	stem, serr := asString(bv)
	if serr != nil {
		return positionWire{}, serr
	}
	iv, ok := o.get("operationIndex")
	if !ok {
		return positionWire{}, fail(CatInvalidFieldSyntax, StageSchema, "position lacks operationIndex")
	}
	idx, okn := asInteger(iv)
	if !okn {
		return positionWire{}, fail(CatInvalidFieldSyntax, StageSchema,
			"operationIndex must be an integer")
	}
	return positionWire{stem: stem, opIndex: idx}, nil
}

func parseOpKeyMap(v jsonValue, sep string, emit func(document.UUID, string, opKeyWire) *Error) *Error {
	o, err := asObject(v)
	if err != nil {
		return err
	}
	for _, key := range sortedKeys(o) {
		entityText, field, found := strings.Cut(key, sep)
		if !found {
			return fail(CatInvalidFieldSyntax, StageSchema, "op-key map entry %q malformed", key)
		}
		entity, uerr := parseUUIDText(entityText)
		if uerr != nil {
			return uerr
		}
		k, kerr := parseOpKeyStruct(mustGet(o, key))
		if kerr != nil {
			return kerr
		}
		if eerr := emit(entity, field, k); eerr != nil {
			return eerr
		}
	}
	return nil
}

func parseOpKeyStruct(v jsonValue) (opKeyWire, *Error) {
	o, err := asObject(v)
	if err != nil {
		return opKeyWire{}, err
	}
	cv, ok := o.get("operationCounter")
	if !ok {
		return opKeyWire{}, fail(CatInvalidFieldSyntax, StageSchema, "op key lacks operationCounter")
	}
	counter, okn := asInteger(cv)
	if !okn || counter < 1 {
		return opKeyWire{}, fail(CatInvalidFieldSyntax, StageSchema, "operationCounter must be a positive integer")
	}
	av, ok := o.get("actorId")
	if !ok {
		return opKeyWire{}, fail(CatInvalidFieldSyntax, StageSchema, "op key lacks actorId")
	}
	actor, ferr := asString(av)
	if ferr != nil {
		return opKeyWire{}, ferr
	}
	return opKeyWire{counter: counter, actor: actor}, nil
}

// ---------------------------------------------------------------------------
// Batch documents (§4.9 shape).

type batchDoc struct {
	raw      []byte
	batchID  string
	actorID  string
	base     int64
	frontier map[string]int64
	// unknown records documentation-only extra members (U2 bookkeeping:
	// they are tolerated on read and force write refusal rather than
	// silent stripping, because batch entries are re-serialized).
	unknown []string
}

func (d *batchDoc) unknownMembers() ([]string, bool) { return d.unknown, len(d.unknown) > 0 }

func parseBatchDoc(raw []byte) (*batchDoc, *Error) {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return nil, perr
	}
	obj, err := asObject(root)
	if err != nil {
		return nil, err
	}
	d := &batchDoc{raw: raw, frontier: map[string]int64{}}
	for _, k := range obj.keys {
		switch k {
		case "batchId", "actorId", "counterBase", "frontier", "operations":
		default:
			d.unknown = append(d.unknown, k)
		}
	}
	get := func(name string) (jsonValue, bool) { return obj.get(name) }
	v, ok := get("batchId")
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageFraming, "batchId absent")
	}
	if d.batchID, err = asString(v); err != nil {
		return nil, err
	}
	v, ok = get("actorId")
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageFraming, "actorId absent")
	}
	if d.actorID, err = asString(v); err != nil {
		return nil, err
	}
	v, ok = get("counterBase")
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageFraming, "counterBase absent")
	}
	n, okn := asInteger(v)
	if !okn {
		return nil, fail(CatInvalidFieldSyntax, StageFraming, "counterBase must be an integer")
	}
	d.base = n
	fv, _ := get("frontier")
	if fv != nil {
		fo, ferr := asObject(fv)
		if ferr != nil {
			return nil, ferr
		}
		for _, actor := range fo.keys {
			cn, okc := asInteger(mustGet(fo, actor))
			if !okc || cn < 0 {
				return nil, fail(CatInvalidFieldSyntax, StageFraming,
					"frontier counter for %s invalid", actor)
			}
			d.frontier[actor] = cn
		}
	}
	return d, nil
}

// buildBatchOperations converts the operations array into document
// operations. Unknown kinds refuse safely here (U1/O2): the containing
// batch rejects before any effect.
func buildBatchOperations(d *batchDoc) ([]*document.Operation, *Error) {
	root, perr := parseLimitedJSON(d.raw)
	if perr != nil {
		return nil, perr
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
	ops := make([]*document.Operation, 0, len(arr))
	for i, item := range arr {
		o, oerr := asObject(item)
		if oerr != nil {
			return nil, oerr
		}
		opIDv, ok := o.get("operationId")
		if !ok {
			return nil, fail(CatInvalidFieldSyntax, StageFraming, "operation %d lacks operationId", i)
		}
		opID, ferr := asString(opIDv)
		if ferr != nil {
			return nil, ferr
		}
		kindv, ok := o.get("kind")
		if !ok {
			return nil, fail(CatInvalidFieldSyntax, StageFraming, "operation %d lacks kind", i)
		}
		kind, ferr := asString(kindv)
		if ferr != nil {
			return nil, ferr
		}
		entv, ok := o.get("entityId")
		if !ok {
			return nil, fail(CatInvalidFieldSyntax, StageFraming, "operation %d lacks entityId", i)
		}
		entID, ferr := asString(entv)
		if ferr != nil {
			return nil, ferr
		}
		fieldsRaw := []byte(nil)
		if fv, hasFields := o.get("fields"); hasFields {
			fieldsRaw = o.raws["fields"]
			_ = fv
		}
		opIDu, uerr := parseUUIDText(opID)
		if uerr != nil {
			return nil, uerr
		}
		entIDu, uerr := parseUUIDText(entID)
		if uerr != nil {
			return nil, uerr
		}
		switch document.OpKind(kind) {
		case document.OpCreateEntity:
			if fieldsRaw == nil {
				return nil, fail(CatMalformedFraming, StageFraming,
					"createEntity operation %d lacks its entity payload", i)
			}
			op, cerr := document.NewCreateOp(opIDu, fieldsRaw)
			if cerr != nil {
				return nil, createOrSchemaErr(cerr, i)
			}
			ops = append(ops, op)
		case document.OpSetFields:
			if fieldsRaw == nil {
				return nil, fail(CatMalformedFraming, StageFraming,
					"setFields operation %d lacks its field payload", i)
			}
			fields, perr := decodeFieldsPayload(fieldsRaw)
			if perr != nil {
				return nil, perr
			}
			op, cerr := document.NewSetFieldsOp(opIDu, entIDu, fields)
			if cerr != nil {
				return nil, createOrSchemaErr(cerr, i)
			}
			ops = append(ops, op)
		case document.OpDeleteEntity:
			if fieldsRaw != nil {
				return nil, fail(CatMalformedFraming, StageFraming,
					"deleteEntity operation %d carries a payload", i)
			}
			op, cerr := document.NewDeleteOp(opIDu, entIDu)
			if cerr != nil {
				return nil, createOrSchemaErr(cerr, i)
			}
			ops = append(ops, op)
		default:
			return nil, fail(CatMalformedFraming, StageFraming,
				"unknown operation kind %q refuses safely before replay (O2/U1)", kind)
		}
	}
	return ops, nil
}

// decodeFieldsPayload converts a raw fields object into the map form
// NewSetFieldsOp expects, preserving each field's exact bytes.
func decodeFieldsPayload(raw []byte) (map[string]json.RawMessage, *Error) {
	root, perr := parseLimitedJSON(raw)
	if perr != nil {
		return nil, perr
	}
	obj, ok := root.(*jsonObject)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageFraming, "fields payload must be an object")
	}
	out := make(map[string]json.RawMessage, len(obj.keys))
	for _, k := range obj.keys {
		out[k] = append(json.RawMessage(nil), obj.raws[k]...)
	}
	return out, nil
}

// buildBatchJSON serializes a delivered batch into its canonical wire
// form with deterministic member ordering.
func buildBatchJSON(b *document.Batch) ([]byte, *Error) {
	var buf bytes.Buffer
	buf.WriteString(`{"batchId":`)
	writeJSONString(&buf, b.BatchID.String())
	buf.WriteString(`,"actorId":`)
	writeJSONString(&buf, b.ActorID.String())
	buf.WriteString(`,"counterBase":`)
	buf.WriteString(itoa64(b.CounterBase))
	buf.WriteString(`,"frontier":{}`)
	frontiers := make([]string, 0, len(b.Frontier))
	for a := range b.Frontier {
		frontiers = append(frontiers, a.String())
	}
	sort.Strings(frontiers)
	if len(frontiers) > 0 {
		buf.Truncate(buf.Len() - 2) // drop the empty-object placeholder
		buf.WriteByte('{')
		for i, a := range frontiers {
			if i > 0 {
				buf.WriteByte(',')
			}
			u, _ := document.ParseUUID(a)
			writeJSONString(&buf, a)
			buf.WriteByte(':')
			buf.WriteString(itoa64(b.Frontier[u]))
		}
		buf.WriteByte('}')
	}
	buf.WriteString(`,"operations":[`)
	for i, op := range b.Operations {
		if i > 0 {
			buf.WriteByte(',')
		}
		buf.WriteString(`{"operationId":`)
		writeJSONString(&buf, op.OperationID.String())
		buf.WriteString(`,"kind":`)
		writeJSONString(&buf, string(op.Kind))
		buf.WriteString(`,"entityId":`)
		writeJSONString(&buf, op.EntityID.String())
		if op.Kind == document.OpCreateEntity || op.Kind == document.OpSetFields {
			buf.WriteString(`,"fields":`)
			buf.Write(op.Payload)
		}
		buf.WriteByte('}')
	}
	buf.WriteString("]}")
	return buf.Bytes(), nil
}

// buildSnapshotJSON serializes S1–S5 deterministically: entities and all
// maps in ascending UUID order, winner/creation keys in ascending
// (entity, field) order, recent batch ids ascending.
func buildSnapshotJSON(state document.State, recentBatchIDs []string) ([]byte, *Error) {
	var buf bytes.Buffer
	buf.WriteString(`{"entities":{`)
	for i, e := range state.Entities {
		raw, err := document.EncodeEntity(e)
		if err != nil {
			return nil, schemaErr(err)
		}
		if i > 0 {
			buf.WriteByte(',')
		}
		writeJSONString(&buf, e.EntityID().String())
		buf.WriteByte(':')
		buf.Write(raw)
	}
	buf.WriteString(`},"frontier":{`)
	frontierActors := make([]string, 0, len(state.Frontier))
	for a := range state.Frontier {
		frontierActors = append(frontierActors, a.String())
	}
	sort.Strings(frontierActors)
	for i, a := range frontierActors {
		u, _ := document.ParseUUID(a)
		if i > 0 {
			buf.WriteByte(',')
		}
		writeJSONString(&buf, a)
		buf.WriteByte(':')
		buf.WriteString(itoa64(state.Frontier[u]))
	}
	buf.WriteString(`}`)
	// §5.1 S3's shared table, in ascending stem order. The stems are
	// zero-padded to 19 digits, so byte order equals ascending
	// (counterBase, actorId) — the same total-order-preserving form §7.6
	// fixes for batch entry names.
	//
	// The table must contain EXACTLY the batches the position maps
	// reference. document.Projection.State() already prunes to that set;
	// this validates it again rather than trusting the caller, because a
	// hand-built State reaching the writer must not be able to emit a
	// dangling reference (absent required data) or an unreferenced entry.
	referenced := map[document.BatchRef]bool{}
	for _, w := range state.WinnerPositions {
		referenced[document.BatchOf(w)] = true
	}
	for _, w := range state.CreatedPositions {
		referenced[document.BatchOf(w)] = true
	}
	for _, w := range state.Tombstones {
		referenced[document.BatchOf(w)] = true
	}
	for ref := range referenced {
		if _, ok := state.BatchPositions[ref]; !ok {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"snapshot references batch %s, which batchPositions does not carry (FS §5.1 S3)",
				document.StemOf(ref))
		}
	}
	for ref := range state.BatchPositions {
		if !referenced[ref] {
			return nil, fail(CatInvalidFieldSyntax, StageSchema,
				"snapshot batchPositions carries unreferenced batch %s (FS §5.1 S3)",
				document.StemOf(ref))
		}
	}
	stems := make([]string, 0, len(state.BatchPositions))
	byStem := make(map[string]document.BatchRef, len(state.BatchPositions))
	for ref := range state.BatchPositions {
		t := document.StemOf(ref)
		stems = append(stems, t)
		byStem[t] = ref
	}
	sort.Strings(stems)
	buf.WriteString(`,"batchPositions":{`)
	for i, stem := range stems {
		if i > 0 {
			buf.WriteByte(',')
		}
		writeJSONString(&buf, stem)
		buf.WriteString(`:{`)
		fr := state.BatchPositions[byStem[stem]]
		actors := make([]string, 0, len(fr))
		for a := range fr {
			actors = append(actors, a.String())
		}
		sort.Strings(actors)
		for j, a := range actors {
			if j > 0 {
				buf.WriteByte(',')
			}
			u, _ := document.ParseUUID(a)
			writeJSONString(&buf, a)
			buf.WriteByte(':')
			buf.WriteString(itoa64(fr[u]))
		}
		buf.WriteString(`}`)
	}
	buf.WriteString(`}`)

	writePosition := func(w document.Position) {
		buf.WriteString(`{"batch":`)
		writeJSONString(&buf, document.StemOf(document.BatchOf(w)))
		buf.WriteString(`,"operationIndex":`)
		buf.WriteString(itoa64(document.OperationIndexOf(w)))
		buf.WriteByte('}')
	}
	writePositionMap := func(name string, m map[document.FieldKey]document.Position) {
		keys := make([]string, 0, len(m))
		texts := make(map[string]document.FieldKey, len(m))
		for f := range m {
			t := f.Entity.String() + "." + f.Field
			keys = append(keys, t)
			texts[t] = f
		}
		sort.Strings(keys)
		buf.WriteString(`,"` + name + `":{`)
		for i, k := range keys {
			if i > 0 {
				buf.WriteByte(',')
			}
			writeJSONString(&buf, k)
			buf.WriteByte(':')
			writePosition(m[texts[k]])
		}
		buf.WriteString(`}`)
	}
	writePositionMap("winnerPositions", state.WinnerPositions)

	// createdPositions is keyed by entity, bare (§5.1 S1).
	createdIDs := make([]string, 0, len(state.CreatedPositions))
	for id := range state.CreatedPositions {
		createdIDs = append(createdIDs, id.String())
	}
	sort.Strings(createdIDs)
	buf.WriteString(`,"createdPositions":{`)
	for i, id := range createdIDs {
		if i > 0 {
			buf.WriteByte(',')
		}
		u, _ := document.ParseUUID(id)
		writeJSONString(&buf, id)
		buf.WriteByte(':')
		writePosition(state.CreatedPositions[u])
	}
	buf.WriteString(`}`)

	// S4: the tombstone carries the DELETING position only. Its creation
	// position lives in createdPositions, which S1 retains for tombstoned
	// ids as well as live ones, so one datum has exactly one home.
	buf.WriteString(`,"tombstones":{`)
	tombIDs := make([]string, 0, len(state.Tombstones))
	for id := range state.Tombstones {
		tombIDs = append(tombIDs, id.String())
	}
	sort.Strings(tombIDs)
	for i, t := range tombIDs {
		u, _ := document.ParseUUID(t)
		if i > 0 {
			buf.WriteByte(',')
		}
		writeJSONString(&buf, t)
		buf.WriteString(`:{"deletingPosition":`)
		writePosition(state.Tombstones[u])
		buf.WriteByte('}')
	}
	buf.WriteString(`}`)

	sorted := append([]string(nil), recentBatchIDs...)
	sort.Strings(sorted)
	buf.WriteString(`,"batchMetadata":{"recentBatchIds":[`)
	for i, id := range sorted {
		if i > 0 {
			buf.WriteByte(',')
		}
		writeJSONString(&buf, id)
	}
	buf.WriteString(`]}}`)
	return buf.Bytes(), nil
}

func writeJSONString(buf *bytes.Buffer, s string) {
	const hexdigits = "0123456789abcdef"
	buf.WriteByte('"')
	for i := 0; i < len(s); i++ {
		c := s[i]
		switch {
		case c == '"':
			buf.WriteString(`\"`)
		case c == '\\':
			buf.WriteString(`\\`)
		case c == '\n':
			buf.WriteString(`\n`)
		case c == '\r':
			buf.WriteString(`\r`)
		case c == '\t':
			buf.WriteString(`\t`)
		case c < 0x20:
			buf.WriteString(`\u00`)
			buf.WriteByte(hexdigits[c>>4])
			buf.WriteByte(hexdigits[c&0xf])
		default:
			buf.WriteByte(c)
		}
	}
	buf.WriteByte('"')
}

// ---------------------------------------------------------------------------
// jsonValue access helpers.

func asObject(v jsonValue) (*jsonObject, *Error) {
	o, ok := v.(*jsonObject)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageSchema, "expected a JSON object")
	}
	return o, nil
}

func asArray(v jsonValue) (jsonArray, *Error) {
	a, ok := v.(jsonArray)
	if !ok {
		return nil, fail(CatInvalidFieldSyntax, StageSchema, "expected a JSON array")
	}
	return a, nil
}

func asString(v jsonValue) (string, *Error) {
	s, ok := v.(jsonString)
	if !ok {
		return "", fail(CatInvalidFieldSyntax, StageSchema, "expected a JSON string")
	}
	return string(s), nil
}

func asInteger(v jsonValue) (int64, bool) {
	switch n := v.(type) {
	case jsonInt:
		return int64(n), true
	default:
		return 0, false
	}
}

func mustGet(o *jsonObject, key string) jsonValue {
	return o.values[key]
}

func sortedKeys(o *jsonObject) []string {
	if o == nil {
		return nil
	}
	out := append([]string(nil), o.keys...)
	sort.Strings(out)
	return out
}

func parseUUIDText(s string) (document.UUID, *Error) {
	if !isCanonicalUUID(s) {
		return document.UUID{}, fail(CatInvalidFieldSyntax, StageSchema,
			"UUID %q is not the canonical 36-character RFC 9562 form", s)
	}
	u, err := document.ParseUUID(s)
	if err != nil {
		return document.UUID{}, fail(CatInvalidFieldSyntax, StageSchema, "%q: %v", s, err)
	}
	return u, nil
}

// schemaErr converts a document.FieldError into the taxonomy token it
// already carries (invalid-field-syntax, nonfinite-value, ...).
func schemaErr(err error) *Error {
	if fe, ok := err.(*document.FieldError); ok {
		return &Error{Category: Category(fe.Code), Stage: StageSchema, Detail: fe.Error()}
	}
	return fail(CatMalformedFraming, StageSchema, "%v", err)
}

// createOrSchemaErr maps constructor failures on one operation to the
// right framing class: payload schema violations keep their field-level
// tokens, structural anomalies are pre-replay framing rejects (§4.3).
func createOrSchemaErr(err error, index int) *Error {
	if fe, ok := err.(*document.FieldError); ok {
		return &Error{Category: Category(fe.Code), Stage: StageSchema, Detail: fe.Error()}
	}
	return fail(CatMalformedFraming, StageFraming, "operation %d: %v", index, err)
}
