package cli

import (
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
	"sort"

	"domowik.dev/domowik/internal/assets"
	"domowik.dev/domowik/internal/persist"
)

// pack.go implements `domowik pack` (issue 0026): thin/portable package
// conversion over the same logical plan (FORMAT-SPEC §7.9, ASSET-STRATEGY
// §10.4). Packing computes the complete referenced-blob universe U(P),
// reports embedded-vs-external byte totals BEFORE any output exists,
// runs a full preflight resolution pass, and only then streams a
// candidate through the §9.7 save machine. Portable packing embeds every
// U(P) member or fails with a COMPLETE omission report and no output
// file; thin packs embed only preview-class blobs and preserve them.

const packUsage = `pack a Domowik package in the thin or portable profile

Usage:
  domowik pack (--thin|--portable) INPUT.domo OUTPUT.domo [--cache DIR]
               [--confirm-overwrite]

Thin embeds only preview-class blobs; portable embeds every blob of the
referenced-blob universe U(P), digest-verified, or aborts before writing
anything with a complete omission report.
`

func runPack(args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("pack", flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	thin := fs.Bool("thin", false, "produce a thin package")
	portable := fs.Bool("portable", false, "produce a portable package")
	confirm := fs.Bool("confirm-overwrite", false, "overwrite an existing output despite external-modification refusal")
	cacheDir := fs.String("cache", "", "cache root directory override")
	if err := fs.Parse(args); err != nil {
		fmt.Fprint(stderr, packUsage)
		return 2
	}
	switch {
	case *thin == *portable:
		fmt.Fprint(stderr, "domowik: exactly one of --thin or --portable is required\n\n")
		fmt.Fprint(stderr, packUsage)
		return 2
	case fs.NArg() != 2:
		fmt.Fprint(stderr, packUsage)
		return 2
	}
	inputPath, outputPath := fs.Arg(0), fs.Arg(1)

	store, code := openAssetStore(*cacheDir, stderr)
	if code != 0 {
		return code
	}
	profile := persist.ProfilePortable()
	if *thin {
		profile = persist.ProfileThin()
	}

	view, err := persist.LoadFile(inputPath)
	if err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return 1
	}
	defer view.Close()

	universe, uerr := assets.ReferencedUniverse(view)
	if uerr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", uerr)
		return 1
	}

	plan, perr := planPacking(view, store, universe, *thin)
	if perr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", perr)
		return 1
	}
	if len(plan.omissions) > 0 {
		fmt.Fprintf(stdout, "packing aborted; no output was written\n")
		fmt.Fprintf(stderr, "omission report (%d of %d U(P) members unresolved):\n",
			len(plan.omissions), len(universe.Digests))
		for _, o := range plan.omissions {
			fmt.Fprintf(stderr, "omitted-digest: %s\treason: %s\tdetail: %s\n", o.digest, o.reason, o.detail)
		}
		return 1
	}

	fmt.Fprintf(stdout, "profile: %s\n", profile)
	fmt.Fprintf(stdout, "universe-size: %d\n", len(universe.Digests))
	fmt.Fprintf(stdout, "resolved-embedded-blobs: %d\n", plan.embeddedCount)
	fmt.Fprintf(stdout, "logical-embedded-bytes: %d\n", plan.embeddedBytes)
	fmt.Fprintf(stdout, "resolved-external-blobs: %d\n", plan.externalCount)
	fmt.Fprintf(stdout, "logical-external-bytes: %d\n", plan.externalBytes)
	for _, d := range plan.diagnostics {
		fmt.Fprintf(stdout, "diagnostic: %s\n", d)
	}
	fmt.Fprintf(stdout, "previews-preserved: %d\n", len(view.Preview))

	in := &persist.WriteInput{
		Profile:           profile,
		State:             view.Projection.State(),
		RetainedBatches:   view.Retained,
		AssetRoot:         universe.Digests,
		DescriptorsRaw:    view.DescriptorsRaw,
		BlobSources:       map[string]persist.BlobSource{},
		PreviewEntries:    view.Preview,
		BaselineConfirmed: *confirm,
	}
	for name, list := range map[string][]string{
		"unknown manifest member": view.ManifestUnknown,
		"unknown snapshot member": view.SnapshotUnknown,
	} {
		for _, m := range list {
			in.RefusalReasons = append(in.RefusalReasons, name+" "+m)
		}
	}
	for batchName, members := range view.BatchUnknown {
		for _, m := range members {
			in.RefusalReasons = append(in.RefusalReasons, "unknown batch member in "+batchName+": "+m)
		}
	}
	sort.Strings(in.RefusalReasons)

	if !*thin {
		for hex, src := range plan.sources {
			in.BlobSources[hex] = src
		}
	}

	if _, serr := persist.SaveDocument(outputPath, 0o644, in, nil); serr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", serr)
		os.Remove(outputPath + ".tmp-0") // never leave partial artifacts behind on our path
		return 1
	}
	fmt.Fprintf(stdout, "packed: %s\n", outputPath)
	// Physical class of the OUTPUT container, distinct from the logical
	// blob classes reported above (§10.4/§10.9 size-class separation).
	if fi, statErr := os.Stat(outputPath); statErr == nil {
		fmt.Fprintf(stdout, "package-bytes-physical: %d\n", fi.Size())
	}
	return 0
}

type omission struct {
	digest string // full sha256: identifier
	reason string
	detail string
}

type packPlan struct {
	sources   map[string]persist.BlobSource // hex digest -> streaming source (portable embeds only)
	omissions []omission                    // portable only; complete, sorted by digest
	// diagnostics carry thin-profile findings: externalized members and
	// missing/unverifiable references (BEHAVIOR-SPEC §9.6 degraded
	// classes). Thin never aborts on them; the package stays valid
	// because the root table and descriptors persist unchanged.
	diagnostics   []string
	embeddedCount int
	embeddedBytes uint64 // logical blob bytes resolved from the input
	externalCount int
	externalBytes uint64 // logical blob bytes referenced outside the output
}

// planPacking resolves every U(P) member through the §10.6 lookup order
// (embedded → cache; remote retrieval stays consent-gated and disabled
// in CLI scope).
//
// Portable: each member is digest-verified BEFORE the caller writes
// anything; every failing member lands in omissions — the report is
// complete, never first-failure — per BEHAVIOR-SPEC §9.6.
//
// Thin: no embedding happens, so unresolved members cannot make the
// OUTPUT invalid or incomplete; they surface as deterministic
// diagnostics while the root table and descriptors preserve the
// references unchanged. Only previews are embedded in a thin package.
func planPacking(view *persist.PackageView, store *assets.Store, universe *assets.Universe, thin bool) (*packPlan, error) {
	plan := &packPlan{sources: map[string]persist.BlobSource{}}
	for _, id := range universe.Digests {
		hex := trimDigestPrefix(id)
		blobName := persist.BlobEntryName(hex)
		inv, embedded := view.EntryInventory(blobName)
		if thin {
			switch {
			case embedded:
				plan.embeddedCount++
				plan.embeddedBytes += inv.ByteLength
				plan.diagnostics = append(plan.diagnostics, fmt.Sprintf(
					"externalized: %s (%d B embedded in the input is not carried into the thin package)",
					id, inv.ByteLength))
			case fileExists(store.BlobPath(hex)):
				if fi, serr := os.Stat(store.BlobPath(hex)); serr == nil {
					plan.externalCount++
					plan.externalBytes += uint64(fi.Size())
				}
			default:
				plan.externalCount++
				plan.diagnostics = append(plan.diagnostics,
					"missing: "+id+" resolves nowhere; reference persists per BEHAVIOR-SPEC §9.6")
			}
			continue
		}
		switch {
		case embedded:
			st, serr := view.StreamEntry(blobName)
			if serr != nil {
				plan.omissions = append(plan.omissions, omission{id, "unreadable", serr.Error()})
				continue
			}
			got, derr := streamHash(st)
			if derr != nil || st.Verified() != nil || got != hex {
				reason, detail := "corrupt", "embedded blob failed verification"
				if derr != nil {
					detail = derr.Error()
				}
				plan.omissions = append(plan.omissions, omission{id, reason, detail})
				continue
			}
			plan.sources[hex] = &assets.EmbeddedSource{View: view, Entry: blobName, Length: inv.ByteLength}
			plan.embeddedCount++
			plan.embeddedBytes += inv.ByteLength
		default:
			length, verr := store.Verify(hex)
			if verr != nil {
				reason := "missing"
				if cat, ok := persist.CategoryOf(verr); ok && cat == persist.CatChecksumError &&
					fileExists(store.BlobPath(hex)) {
					reason = "corrupt"
				}
				plan.omissions = append(plan.omissions, omission{id, reason, verr.Error()})
				continue
			}
			plan.sources[hex] = &assets.CacheFileSource{Path: store.BlobPath(hex), Length: length}
			plan.externalCount++
			plan.externalBytes += length
		}
	}
	sortOmissions(plan.omissions)
	sort.Strings(plan.diagnostics)
	return plan, nil
}

// streamHash drains r into SHA-256 with no retention of payload bytes.
func streamHash(r io.Reader) (string, error) {
	h := sha256.New()
	buf := make([]byte, 32<<10)
	for {
		nr, rerr := r.Read(buf)
		if nr > 0 {
			if _, herr := h.Write(buf[:nr]); herr != nil {
				return "", herr
			}
		}
		if rerr == io.EOF {
			return hex.EncodeToString(h.Sum(nil)), nil
		}
		if rerr != nil {
			return "", rerr
		}
	}
}

func sortOmissions(o []omission) {
	sort.Slice(o, func(i, j int) bool { return o[i].digest < o[j].digest })
}

func trimDigestPrefix(id string) string {
	const prefix = "sha256:"
	if len(id) > len(prefix) && id[:len(prefix)] == prefix {
		return id[len(prefix):]
	}
	return id
}

func fileExists(path string) bool {
	fi, err := os.Stat(path)
	return err == nil && fi.Mode().IsRegular()
}
