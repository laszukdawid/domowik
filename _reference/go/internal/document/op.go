package document

import (
	"encoding/json"
	"math"
	"sort"
)

// OpKind enumerates the v0.1 operation kinds (FORMAT-SPEC §4.2 O2).
// Unknown kinds refuse safely: a batch carrying one is a pre-replay
// framing reject (§4.3 row 4, §4.5 I5).
type OpKind string

const (
	OpCreateEntity OpKind = "createEntity"
	OpSetFields    OpKind = "setFields"
	OpDeleteEntity OpKind = "deleteEntity"
)

// Operation is one logical document change (FORMAT-SPEC §4.2 O1). The
// operationId is an EXPLICIT identity field: identifiers are never derived
// from serialized JSON bytes (§3.3 consequence 1).
type Operation struct {
	OperationID UUID
	Kind        OpKind
	EntityID    UUID
	// Payload holds the complete entity object for createEntity, the
	// top-level field→value mapping for setFields, and nil for
	// deleteEntity. Callers must treat it as read-only.
	Payload json.RawMessage
}

// NewCreateOp builds a createEntity operation. The payload is validated
// against the §2.3 schema at apply time (per-op invariant boundary).
func NewCreateOp(operationID UUID, entityObject json.RawMessage) (*Operation, error) {
	ent, err := decodeEntity(entityObject)
	if err != nil {
		return nil, err
	}
	return &Operation{
		OperationID: operationID,
		Kind:        OpCreateEntity,
		EntityID:    ent.EntityID(),
		Payload:     append(json.RawMessage(nil), entityObject...),
	}, nil
}

// NewSetFieldsOp builds a setFields operation addressing TOP-LEVEL entity
// fields by serialized name (O3). Field-level validation happens at apply
// time; unknown field names and preserved opaque fields reject there.
func NewSetFieldsOp(operationID UUID, entityID UUID, fields map[string]json.RawMessage) (*Operation, error) {
	if len(fields) == 0 {
		return nil, &FieldError{Code: codeInvalidFieldSyntax, Detail: "setFields payload must address at least one field"}
	}
	names := make([]string, 0, len(fields))
	for n := range fields {
		names = append(names, n)
	}
	sort.Strings(names)
	buf := make([]byte, 0, 256)
	buf = append(buf, '{')
	for i, n := range names {
		if i > 0 {
			buf = append(buf, ',')
		}
		key, err := json.Marshal(n)
		if err != nil {
			return nil, err
		}
		buf = append(buf, key...)
		buf = append(buf, ':')
		buf = append(buf, fields[n]...)
	}
	buf = append(buf, '}')
	return &Operation{
		OperationID: operationID,
		Kind:        OpSetFields,
		EntityID:    entityID,
		Payload:     json.RawMessage(buf),
	}, nil
}

// NewDeleteOp builds a deleteEntity operation; it carries no payload (O2).
func NewDeleteOp(operationID UUID, entityID UUID) (*Operation, error) {
	if operationID == (UUID{}) {
		return nil, ErrInvalidUUID
	}
	return &Operation{
		OperationID: operationID,
		Kind:        OpDeleteEntity,
		EntityID:    entityID,
	}, nil
}

// Batch is the unit of atomic change (FORMAT-SPEC §4.1 B1). Its declared
// counter range [CounterBase, CounterBase+len(Operations)-1] reserves a
// contiguous, disjoint, strictly increasing slot in the actor's stream
// (B3). An empty batch is malformed (no frontier position).
type Batch struct {
	BatchID     UUID
	ActorID     UUID
	CounterBase int64
	// Frontier maps observed actorId → highest observed counter at
	// authoring time (§4.4 F1). It drives causal ordering only; the
	// projection's own frontier advances solely through applying the
	// owning actor's batches (B5).
	Frontier   map[UUID]int64
	Operations []*Operation
}

// NewBatch performs the per-batch syntactic framing checks that precede
// replay (§4.3 row 4): identifier validity, counterBase ≥ 1, at least one
// operation, known operation kinds, payload shape per kind, and unique
// operation ids within the batch.
func NewBatch(batchID, actorID UUID, counterBase int64, frontier map[UUID]int64, ops ...*Operation) (*Batch, error) {
	cp := make(map[UUID]int64, len(frontier))
	for a, n := range frontier {
		if a == actorID && n >= counterBase {
			return nil, wrapFraming("frontier claims observation of the batch's own reserved range")
		}
		cp[a] = n
	}
	b := &Batch{
		BatchID:     batchID,
		ActorID:     actorID,
		CounterBase: counterBase,
		Frontier:    cp,
		Operations:  ops,
	}
	if err := validateFraming(b); err != nil {
		return nil, err
	}
	return b, nil
}

// validateFraming performs the per-batch syntactic framing checks that
// precede replay (§4.3 row 4). It runs on constructor output AND on every
// batch entering Deliver: Batch fields are exported, so manually
// constructed values are untrusted input and must fail closed identically.
func validateFraming(b *Batch) error {
	if b.BatchID == (UUID{}) {
		return wrapFraming("batchId must be a valid RFC 9562 UUID")
	}
	if b.ActorID == (UUID{}) {
		return wrapFraming("actorId must be a valid RFC 9562 UUID")
	}
	if b.CounterBase < CounterBaseMin {
		return wrapFraming("counterBase must be >= %d (counters start at 1)", CounterBaseMin)
	}
	if len(b.Operations) == 0 {
		return wrapFraming("batch contains no operations; empty batches are malformed-framing rejects")
	}
	if uint64(b.CounterBase)+uint64(len(b.Operations))-1 > math.MaxInt64 {
		return wrapFraming("reserved counter range ending at %d overflows signed 64-bit (L13)",
			uint64(b.CounterBase)+uint64(len(b.Operations))-1)
	}
	for a, n := range b.Frontier {
		if a == (UUID{}) {
			return wrapFraming("frontier names a zero actorId")
		}
		if n < 0 {
			return wrapFraming("frontier counter for %s is negative", a.String())
		}
	}
	seenOps := make(map[UUID]struct{}, len(b.Operations))
	for i, op := range b.Operations {
		switch {
		case op == nil:
			return wrapFraming("operation %d is absent", i)
		case op.OperationID == (UUID{}):
			return wrapFraming("operation %d: operationId must be a valid RFC 9562 UUID (O1)", i)
		case op.EntityID == (UUID{}):
			return wrapFraming("operation %d: entityId must be a valid RFC 9562 UUID", i)
		}
		if _, dup := seenOps[op.OperationID]; dup {
			return wrapFraming("duplicate operationId %s", op.OperationID.String())
		}
		seenOps[op.OperationID] = struct{}{}
		switch op.Kind {
		case OpCreateEntity:
			if len(op.Payload) == 0 {
				return wrapFraming("operation %d: createEntity requires a complete entity payload", i)
			}
		case OpSetFields:
			if len(op.Payload) == 0 {
				return wrapFraming("operation %d: setFields requires a field payload", i)
			}
		case OpDeleteEntity:
			if len(op.Payload) != 0 {
				return wrapFraming("operation %d: deleteEntity carries no payload", i)
			}
		default:
			return wrapFraming("operation %d: unknown kind %q refuses safely before replay", i, string(op.Kind))
		}
	}
	return nil
}

// EndCounter returns the last counter of the batch's reserved range (B3).
func (b *Batch) EndCounter() int64 {
	return b.CounterBase + int64(len(b.Operations)) - 1
}

// equalContent reports whether two batches carry identical logical
// content, used to distinguish silent redelivery (I2/V1) from a duplicate
// batch-id framing anomaly (§4.3 row 4).
func (b *Batch) equalContent(other *Batch) bool {
	if b.BatchID != other.BatchID || b.ActorID != other.ActorID ||
		b.CounterBase != other.CounterBase || b.EndCounter() != other.EndCounter() {
		return false
	}
	if len(b.Frontier) != len(other.Frontier) {
		return false
	}
	for a, n := range b.Frontier {
		if other.Frontier[a] != n {
			return false
		}
	}
	if len(b.Operations) != len(other.Operations) {
		return false
	}
	for i := range b.Operations {
		x, y := b.Operations[i], other.Operations[i]
		if x.OperationID != y.OperationID || x.Kind != y.Kind || x.EntityID != y.EntityID ||
			string(x.Payload) != string(y.Payload) {
			return false
		}
	}
	return true
}
