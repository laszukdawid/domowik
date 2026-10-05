package persist_test

import (
	"os"
	"path/filepath"
	"testing"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

const corpusDir = "../../testdata/corpus"

func corpusFile(t *testing.T, rel string) []byte {
	t.Helper()
	b, err := os.ReadFile(filepath.Join(corpusDir, rel))
	if err != nil {
		t.Fatalf("read corpus fixture %s: %v", rel, err)
	}
	return b
}

func TestLoadGoldenThin(t *testing.T) {
	view, err := persist.Load(corpusFile(t, "golden/packages/thin-valid.domo"))
	if err != nil {
		t.Fatalf("thin-valid: %v", err)
	}
	if view.Profile != "thin" || view.FormatVersion != "0.1" {
		t.Fatalf("unexpected manifest: %+v", view)
	}
	state := view.Projection.State()
	if len(state.Entities) != 1 {
		t.Fatalf("expected 1 entity after replay, got %d", len(state.Entities))
	}
	if state.Entities[0].Kind() != document.KindLevel {
		t.Fatalf("expected level entity, got %s", state.Entities[0].Kind())
	}
}

func TestLoadGoldenPortable(t *testing.T) {
	view, err := persist.Load(corpusFile(t, "golden/packages/portable-valid.domo"))
	if err != nil {
		t.Fatalf("portable-valid: %v", err)
	}
	if len(view.Blobs) != 1 {
		t.Fatalf("expected 1 embedded blob, got %d", len(view.Blobs))
	}
	if len(view.AssetRoot) != 1 {
		t.Fatalf("expected asset root entry")
	}
}

func TestRoundTripGoldenPackages(t *testing.T) {
	for _, rel := range []string{"golden/packages/thin-valid.domo", "golden/packages/portable-valid.domo"} {
		view, err := persist.Load(corpusFile(t, rel))
		if err != nil {
			t.Fatalf("%s: %v", rel, err)
		}
		pkg, werr := persist.BuildPackage(&persist.WriteInput{
			Profile:         view.Profile,
			State:           view.Projection.State(),
			RetainedBatches: view.Retained,
			AssetRoot:       view.AssetRoot,
			DescriptorsRaw:  view.DescriptorsRaw,
			Blobs:           view.Blobs,
			RefusalReasons:  append(append([]string{}, view.ManifestUnknown...), view.SnapshotUnknown...),
		})
		if werr != nil {
			t.Fatalf("%s rebuild: %v", rel, werr)
		}
		reloaded, lerr := persist.Load(pkg)
		if lerr != nil {
			t.Fatalf("%s reload: %v", rel, lerr)
		}

		// Semantic projection assertions (existing).
		a := view.Projection.State()
		b := reloaded.Projection.State()
		if len(a.Entities) != len(b.Entities) || len(a.Frontier) != len(b.Frontier) ||
			len(a.WinnerPositions) != len(b.WinnerPositions) || len(a.Tombstones) != len(b.Tombstones) ||
			len(a.CreatedPositions) != len(b.CreatedPositions) || len(a.BatchPositions) != len(b.BatchPositions) {
			t.Fatalf("%s round trip changed semantic projection", rel)
		}
		for i := range a.Entities {
			if a.Entities[i].EntityID() != b.Entities[i].EntityID() {
				t.Fatalf("%s entity set diverged at %d", rel, i)
			}
		}

		// Opaque-data preservation assertions (new, per r0029 Challenge 1).
		// Assert ManifestUnknown and SnapshotUnknown slices are preserved.
		// Note: golden fixtures are expected to have NO unknown fields
		// (they're conforming v0.1 packages), so these slices should be empty.
		// If a golden fixture ever carries unknown optional fields, this will catch it.
		if len(view.ManifestUnknown) != len(reloaded.ManifestUnknown) {
			t.Fatalf("%s: ManifestUnknown count changed: %d -> %d", rel, len(view.ManifestUnknown), len(reloaded.ManifestUnknown))
		}
		if len(view.SnapshotUnknown) != len(reloaded.SnapshotUnknown) {
			t.Fatalf("%s: SnapshotUnknown count changed: %d -> %d", rel, len(view.SnapshotUnknown), len(reloaded.SnapshotUnknown))
		}

		// Assert retained batches match (count and structure).
		if len(view.Retained) != len(reloaded.Retained) {
			t.Fatalf("%s: retained batch count changed: %d -> %d", rel, len(view.Retained), len(reloaded.Retained))
		}
		for i := range view.Retained {
			if view.Retained[i].ActorID != reloaded.Retained[i].ActorID {
				t.Fatalf("%s: retained batch %d actor changed: %v -> %v", rel, i, view.Retained[i].ActorID, reloaded.Retained[i].ActorID)
			}
			if view.Retained[i].CounterBase != reloaded.Retained[i].CounterBase {
				t.Fatalf("%s: retained batch %d counter base changed: %d -> %d", rel, i,
					view.Retained[i].CounterBase, reloaded.Retained[i].CounterBase)
			}
			if len(view.Retained[i].Operations) != len(reloaded.Retained[i].Operations) {
				t.Fatalf("%s: retained batch %d operation count changed: %d -> %d", rel, i,
					len(view.Retained[i].Operations), len(reloaded.Retained[i].Operations))
			}
		}
	}
}
