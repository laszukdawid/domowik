package assets

import (
	"encoding/json"
	"io"
	"os"

	"domowik.dev/domowik/internal/document"
	"domowik.dev/domowik/internal/persist"
)

// universe.go: the referenced-blob universe U(P) of ASSET-STRATEGY
// §10.5 — the union of {descriptor digests}, {package asset-root table
// entries}, and {instance/reference digests} — plus the two streaming
// BlobSource adapters packing consumes. Previews are deliberately NOT
// members: they are class-4 derived caches, never gate portability, and
// never enter omission reports.

// Universe is one computed U(P).
type Universe struct {
	// Digests is the sorted, deduplicated union in full `sha256:` form.
	Digests []string
	// HexDigests mirrors Digests in bare hex form.
	HexDigests []string
	// Provenance lists, per source class, which digests contributed.
	FromDescriptors []string
	FromRootTable   []string
	FromInstances   []string
}

// ReferencedUniverse computes U(P) for a loaded package view. Descriptor
// keys and root-table entries were validated by the reader; instance and
// background-image digests are validated here so malformed reference
// syntax rejects as hostile input per §10.6 rather than silently
// narrowing completeness.
func ReferencedUniverse(view *persist.PackageView) (*Universe, error) {
	u := &Universe{}
	add := func(dst *[]string, id string) error {
		hex, err := parseHexDigest(id)
		if err != nil {
			return err
		}
		*dst = append(*dst, hex)
		return nil
	}

	if len(view.DescriptorsRaw) > 0 {
		var keyed map[string]json.RawMessage
		if err := json.Unmarshal(view.DescriptorsRaw, &keyed); err != nil {
			return nil, &persist.Error{Category: persist.CatInvalidFieldSyntax, Stage: persist.StageSchema,
				Detail: "descriptor document is not a digest-keyed object"}
		}
		for key := range keyed {
			if err := add(&u.FromDescriptors, key); err != nil {
				return nil, err
			}
		}
	}
	for _, id := range view.AssetRoot {
		if err := add(&u.FromRootTable, id); err != nil {
			return nil, err
		}
	}
	if view.Projection != nil {
		for _, ent := range view.Projection.State().Entities {
			switch e := ent.(type) {
			case *document.AssetInstance:
				if err := add(&u.FromInstances, e.Digest); err != nil {
					return nil, err
				}
			case *document.Level:
				for _, bg := range e.BackgroundImages {
					if err := add(&u.FromInstances, bg.Digest); err != nil {
						return nil, err
					}
				}
			}
		}
	}

	seen := map[string]bool{}
	for _, group := range [][]string{u.FromDescriptors, u.FromRootTable, u.FromInstances} {
		for _, hex := range group {
			if !seen[hex] {
				seen[hex] = true
				u.HexDigests = append(u.HexDigests, hex)
				u.Digests = append(u.Digests, digestPrefix+hex)
			}
		}
	}
	sortHex(u.HexDigests)
	sortFull(u.Digests)
	return u, nil
}

func sortHex(v []string) {
	for i := 1; i < len(v); i++ {
		for j := i; j > 0 && v[j] < v[j-1]; j-- {
			v[j], v[j-1] = v[j-1], v[j]
		}
	}
}

func sortFull(v []string) { sortHex(v) }

// EmbeddedSource streams one blob already embedded in a loaded package
// through its guarded StreamEntry surface (C1/C2/C6): CRC-32 and the
// manifest inventory SHA-256 verify incrementally at end of stream, so a
// corrupted embedded blob aborts emission instead of being copied
// blindly. The declared length comes from the manifest inventory.
type EmbeddedSource struct {
	View  *persist.PackageView
	Entry string // logical blob entry name
	// Length must be the inventory byteLength of Entry.
	Length uint64
}

// DeclaredLength implements persist.BlobSource.
func (e *EmbeddedSource) DeclaredLength() uint64 { return e.Length }

// Open implements persist.BlobSource. Each call returns an independent
// guarded stream over the entry.
func (e *EmbeddedSource) Open() (io.Reader, error) {
	st, err := e.View.StreamEntry(e.Entry)
	if err != nil {
		return nil, err
	}
	return st, nil
}

// CacheFileSource streams one admitted cache blob from disk.
type CacheFileSource struct {
	Path   string
	Length uint64
}

// DeclaredLength implements persist.BlobSource.
func (f *CacheFileSource) DeclaredLength() uint64 { return f.Length }

// Open implements persist.BlobSource.
func (f *CacheFileSource) Open() (io.Reader, error) {
	fi, err := os.Stat(f.Path)
	if err != nil {
		return nil, missingErr("cached blob vanished before embedding: %v", err)
	}
	if uint64(fi.Size()) != f.Length {
		return nil, &persist.Error{Category: persist.CatMalformedLength, Stage: persist.StageDigest,
			Detail: sprintf("cached blob size %d contradicts its recorded length %d (C3)", fi.Size(), f.Length)}
	}
	return os.Open(f.Path)
}
