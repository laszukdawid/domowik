package persist_test

import (
	"encoding/json"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// batchFromFixtureRaw parses a wire batch document into a document.Batch
// using the production framing path (persist's own parser), so the
// harness exercises the same code the package pipeline uses.
func buildBatchForTest(raw []byte) (*document.Batch, *persist.Error) {
	b, err := persist.ParseBatchDocument(raw)
	if err != nil {
		return nil, err
	}
	return b, nil
}

// fieldOf extracts a top-level serialized field from an entity.
func fieldOf(e document.Entity, field string) float64 {
	raw, err := document.EncodeEntity(e)
	if err != nil {
		return 0
	}
	var m map[string]any
	if json.Unmarshal(raw, &m) != nil {
		return 0
	}
	switch v := m[field].(type) {
	case float64:
		return v
	default:
		return 0
	}
}
