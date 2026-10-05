package persist_test

import (
	"bytes"
	"fmt"
	"testing"

	"domowik.dev/domowik/internal/persist"
)

// TestLoadReservedRangeDiagnostic is the ED-0032 regression: overlapping
// or gapped actor ranges must reject at the structural check with
// CatMalformedFraming/StageFraming, not incidentally downstream in replay.
// The defect (reader.go:468 discarding checkReservedRanges return) allowed
// these to pass the structural check and fail later with the wrong diagnostic.
//
// This test proves:
// 1. Overlapping ranges reject with CatMalformedFraming at StageFraming
// 2. Gapped ranges reject with CatMalformedFraming at StageFraming
// 3. The diagnostic comes from the structural check, not replay
func TestLoadReservedRangeDiagnostic(t *testing.T) {
	const actorID = "01949100-0000-7000-8000-000000000001"

	batchAt := func(batchID string, base int64, count int) ([]byte, string) {
		var buf bytes.Buffer
		// F4 requires a batch above its actor's first counter to declare its
		// OWN PRIOR END. Issue 0011's validateOwnPriorEnds enforces this, so an
		// empty frontier at counterBase 42 rejects on F4 before the
		// reserved-range check under test can fire — which would make this
		// fixture prove something other than what it names.
		frontier := "{}"
		if base > 1 {
			frontier = fmt.Sprintf(`{%q:%d}`, actorID, base-1)
		}
		fmt.Fprintf(&buf, `{"batchId":%q,"actorId":%q,"counterBase":%d,"frontier":%s,"operations":[`, batchID, actorID, base, frontier)
		for i := 0; i < count; i++ {
			if i > 0 {
				buf.WriteByte(',')
			}
			fmt.Fprintf(&buf, `{"operationId":"01949100-0000-7000-8000-%012d","kind":"deleteEntity","entityId":"01949101-0000-7000-8000-000000000001"}`, base+int64(i))
		}
		buf.WriteString("]}")
		name := fmt.Sprintf("ops/%019d-%s.json", base, actorID)
		return buf.Bytes(), name
	}

	assemble := func(defs ...[3]any) []byte {
		t.Helper()
		raws := map[string][]byte{}
		for _, d := range defs {
			raw, name := batchAt(d[0].(string), d[1].(int64), d[2].(int))
			raws[name] = raw
		}
		pkg, werr := persist.AssembleRawPackage(raws)
		if werr != nil {
			t.Fatal(werr)
		}
		return pkg
	}

	t.Run("overlapping ranges reject with structural diagnostic", func(t *testing.T) {
		// Two batches with overlapping counter ranges: [42,43] and [43,43]
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000a", int64(42), 2},
			[3]any{"01949102-0000-7000-8000-00000000000b", int64(43), 1},
		)
		_, err := persist.Load(pkg)
		if err == nil {
			t.Fatal("overlapping ranges must reject")
		}
		perr, ok := err.(*persist.Error)
		if !ok {
			t.Fatalf("expected *persist.Error, got %T", err)
		}
		if perr.Category != persist.CatMalformedFraming {
			t.Errorf("expected CatMalformedFraming, got %v (detail: %s)", perr.Category, perr.Detail)
		}
		if perr.Stage != persist.StageFraming {
			t.Errorf("expected StageFraming, got %v (detail: %s)", perr.Stage, perr.Detail)
		}
		// Discriminator: diagnostic must mention "overlapping" AND the specific
		// actor ID. This distinguishes checkReservedRanges output from other
		// framing checks. Category/Stage alone have zero discriminating power
		// because the input already rejects at framing from a different check.
		if !bytes.Contains([]byte(perr.Detail), []byte("overlapping")) {
			t.Errorf("diagnostic must mention 'overlapping', got: %s", perr.Detail)
		}
		if !bytes.Contains([]byte(perr.Detail), []byte(actorID)) {
			t.Errorf("diagnostic must mention actor %s, got: %s", actorID, perr.Detail)
		}
	})

	t.Run("gapped ranges reject with structural diagnostic", func(t *testing.T) {
		// Two batches with a gap: [42,42] and [44,44] (missing 43)
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000c", int64(42), 1},
			[3]any{"01949102-0000-7000-8000-00000000000d", int64(44), 1},
		)
		_, err := persist.Load(pkg)
		if err == nil {
			t.Fatal("gapped ranges must reject")
		}
		perr, ok := err.(*persist.Error)
		if !ok {
			t.Fatalf("expected *persist.Error, got %T", err)
		}
		if perr.Category != persist.CatMalformedFraming {
			t.Errorf("expected CatMalformedFraming, got %v (detail: %s)", perr.Category, perr.Detail)
		}
		if perr.Stage != persist.StageFraming {
			t.Errorf("expected StageFraming, got %v (detail: %s)", perr.Stage, perr.Detail)
		}
		// Discriminator: diagnostic must mention "gap" AND the specific actor ID.
		// This distinguishes checkReservedRanges output from other framing checks.
		// Category/Stage alone have zero discriminating power because the input
		// already rejects at framing from a different check.
		if !bytes.Contains([]byte(perr.Detail), []byte("gap")) {
			t.Errorf("diagnostic must mention 'gap', got: %s", perr.Detail)
		}
		if !bytes.Contains([]byte(perr.Detail), []byte(actorID)) {
			t.Errorf("diagnostic must mention actor %s, got: %s", actorID, perr.Detail)
		}
	})

	t.Run("contiguous ranges load successfully", func(t *testing.T) {
		// Two contiguous batches: [42,42] and [43,43]
		pkg := assemble(
			[3]any{"01949102-0000-7000-8000-00000000000e", int64(42), 1},
			[3]any{"01949102-0000-7000-8000-00000000000f", int64(43), 1},
		)
		_, err := persist.Load(pkg)
		if err != nil {
			t.Fatalf("contiguous ranges must load, got %v", err)
		}
	})
}
