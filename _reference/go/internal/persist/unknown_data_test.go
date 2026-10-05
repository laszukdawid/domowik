package persist_test

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// unknown_data_test.go: U2/L32 preservation duties — unknown OPTIONAL
// fields survive a save/load round trip with identical value semantics;
// unpreservable constructs force safe-write refusal; unknown REQUIRED
// constructs refuse via the V3 gate.

func TestUnknownOptionalFieldPreservedAcrossRoundTrip(t *testing.T) {
	fixtureRaw := corpusFile(t, "adversarial/unknown-data/unknown-optional-entity-field-preserve.json")
	var fixture map[string]any
	if err := json.Unmarshal(fixtureRaw, &fixture); err != nil {
		t.Fatal(err)
	}
	entityRaw, merr := json.Marshal(fixture["entity"])
	if merr != nil {
		t.Fatal(merr)
	}
	ent, err := document.DecodeEntity(entityRaw)
	if err != nil {
		t.Fatalf("read with unknown optional field must succeed: %v", err)
	}
	wantNoteRaw, err := json.Marshal(fixture["entity"].(map[string]any)["futureNote"])
	if err != nil || len(wantNoteRaw) == 0 {
		t.Fatal("fixture lacks its futureNote probe field")
	}

	levelRaw := corpusFile(t, "golden/snapshots/snapshot-full-s1-s5.json")
	seedState, _, serr := persist.ParseSnapshotDocument(levelRaw)
	if serr != nil {
		t.Fatal(serr)
	}
	input := validWriteInput(t)
	input.State.Entities = nil
	for _, e := range seedState.Entities {
		if e.Kind() == document.KindLevel {
			input.State.Entities = append(input.State.Entities, e)
		}
	}
	input.State.Entities = append(input.State.Entities, ent)
	target := filepath.Join(t.TempDir(), "u2.domo")
	if _, serr := persist.SaveDocument(target, 0o644, input, nil); serr != nil {
		t.Fatalf("save with preserved unknown fields must succeed: %v", serr)
	}
	view, lerr := persist.LoadFile(target)
	if lerr != nil {
		t.Fatalf("reload: %v", lerr)
	}
	var found bool
	for _, e := range view.Projection.State().Entities {
		if e.EntityID() != ent.EntityID() {
			continue
		}
		reenc, eerr := document.EncodeEntity(e)
		if eerr != nil {
			t.Fatal(eerr)
		}
		var after map[string]any
		if json.Unmarshal(reenc, &after) != nil {
			t.Fatal("re-encoded entity unparsable")
		}
		gotNoteRaw, noteErr := json.Marshal(after["futureNote"])
		if noteErr != nil || string(gotNoteRaw) != string(wantNoteRaw) {
			t.Fatalf("U2 violation: futureNote stripped or altered (%s vs %s)", gotNoteRaw, wantNoteRaw)
		}
		found = true
	}
	if !found {
		t.Fatal("entity lost across round trip")
	}
}

func TestUnpreservableConstructsForceSafeWriteRefusal(t *testing.T) {
	input := validWriteInput(t)
	input.RefusalReasons = []string{"unknown manifest member \"vendorExtension\" cannot be re-serialized"}
	_, werr := persist.BuildPackage(input)
	if werr == nil || catOf(t, werr) != persist.CatSafeWriteRefusal {
		t.Fatalf("U2/L32: expected safe-write-refusal, got %v", werr)
	}
}

func TestUnknownRequiredRefusesViaVersionGate(t *testing.T) {
	raw := corpusFile(t, "adversarial/unknown-data/unknown-required-entity-field-refuse.json")
	var fixture map[string]any
	if err := json.Unmarshal(raw, &fixture); err != nil {
		t.Fatal(err)
	}
	futureVersion, _ := fixture["manifestFormatVersion"].(string)
	if futureVersion == "0.1" {
		t.Fatal("fixture must carry a newer schema label for the U1/V3 path")
	}
	// The gate itself is pinned by TestMigrationGateFixtures; here we pin
	// the package-level observable: loading a package stamped with the
	// newer label refuses unsupported-version before any effect.
	patched := relabelManifest(t, nil, futureVersion)
	_, lerr := persist.Load(patched)
	if lerr == nil || catOf(t, lerr) != persist.CatUnsupportedVersion {
		t.Fatalf("V3 gate: expected unsupported-version, got %v", lerr)
	}
}

// relabelManifest assembles a structurally valid container whose manifest
// carries a foreign schema label (raw bytes; no foreign-schema WRITER
// exists by design — the assembler is byte-level harness tooling).
func relabelManifest(t *testing.T, _ []byte, version string) []byte {
	t.Helper()
	type inv struct {
		SHA256     string `json:"sha256"`
		ByteLength int    `json:"byteLength"`
	}
	snapshot := []byte(`{"entities":{},"frontier":{},"winnerKeys":{},"tombstones":{}}`)
	manifest, merr := json.Marshal(map[string]any{
		"formatVersion": version,
		"profile":       "thin",
		"assetRoot":     nil,
		"entries": map[string]inv{
			"snapshot/state.json": {"sha256:" + sha256HexOf(snapshot), len(snapshot)},
		},
	})
	if merr != nil {
		t.Fatal(merr)
	}
	pkg, werr := persist.AssembleRawPackage(map[string][]byte{
		"domowik.json":        manifest,
		"snapshot/state.json": snapshot,
	})
	if werr != nil {
		t.Fatal(werr)
	}
	return pkg
}
