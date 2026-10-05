package cli

import (
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"domowik.dev/domowik/internal/assets"
	"domowik.dev/domowik/internal/persist"
)

// asset.go implements `domowik asset` (issue 0026): streaming import,
// listing with size-class totals, digest verification, removal guarded
// by pins, the manual-pin surface of ASSET-STRATEGY §10.7, and garbage
// collection that is fail-closed under metadata anomalies.
//
// Exit codes follow the CLI contract: 0 success, 1 operational failure,
// 2 usage error. Success output goes only to stdout; diagnostics go
// only to stderr.

const assetUsage = `asset management (content-addressed store)

Usage:
  domowik asset add [--media-type TYPE/SUBTYPE] [--purpose PURPOSE]
                 [--requirement required|optional] [--display-name NAME]
                 [--author AUTHOR] [--source-description TEXT]
                 [--cache DIR] FILE

Flags precede positional arguments.
  domowik asset list
  domowik asset verify sha256:DIGEST
  domowik asset remove sha256:DIGEST
  domowik asset pin sha256:DIGEST --owner ID [--root-kind KIND]
  domowik asset unpin sha256:DIGEST --owner ID
  domowik asset gc [--repair]

The cache root defaults to $DOMOWIK_CACHE, then the user cache directory
plus domowik/assets; override per invocation with --cache DIR.
`

func runAsset(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	sub := args[0]
	if sub == "--help" || sub == "help" {
		fmt.Fprint(stdout, assetUsage)
		return 0
	}
	rest, cacheDir := extractCacheArg(args[1:])
	store, code := openAssetStore(cacheDir, stderr)
	if code != 0 {
		return code
	}

	switch sub {
	case "add":
		return assetAdd(store, rest, stdout, stderr)
	case "list":
		return assetList(store, rest, stdout, stderr)
	case "verify":
		return assetVerify(store, rest, stdout, stderr)
	case "remove":
		return assetRemove(store, rest, stdout, stderr)
	case "pin", "unpin":
		return assetPin(store, sub, rest, stdout, stderr)
	case "gc":
		return assetGC(store, rest, stdout, stderr)
	default:
		fmt.Fprintf(stderr, "domowik: unknown asset subcommand %q\n\n", sub)
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
}

// extractCacheArg pulls --cache DIR / --cache=DIR out of the argument
// list so each subcommand's flag set owns only its own flags.
func extractCacheArg(args []string) ([]string, string) {
	var rest []string
	cache := ""
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch {
		case a == "--cache" && i+1 < len(args):
			cache = args[i+1]
			i++
		case strings.HasPrefix(a, "--cache="):
			cache = strings.TrimPrefix(a, "--cache=")
		default:
			rest = append(rest, a)
		}
	}
	return rest, cache
}

func openAssetStore(cacheDir string, stderr io.Writer) (*assets.Store, int) {
	dir := cacheDir
	if dir == "" {
		dir = os.Getenv("DOMOWIK_CACHE")
	}
	if dir == "" {
		base, err := os.UserCacheDir()
		if err != nil || base == "" {
			fmt.Fprintf(stderr, "domowik: no cache directory available: %v; pass --cache DIR\n", err)
			return nil, 1
		}
		dir = filepath.Join(base, "domowik", "assets")
	}
	store, err := assets.Open(dir)
	if err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return nil, 1
	}
	return store, 0
}

func assetAdd(store *assets.Store, args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("asset add", flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	mediaType := fs.String("media-type", "", "required type/subtype media type")
	purpose := fs.String("purpose", "", "required role token")
	requirement := fs.String("requirement", "", "required|optional (default required)")
	displayName := fs.String("display-name", "", "attribution display name")
	author := fs.String("author", "", "attribution author")
	sourceDesc := fs.String("source-description", "", "attribution source description")
	if err := fs.Parse(args); err != nil {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	path := fs.Arg(0)
	if path == "" || *mediaType == "" || *purpose == "" {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	src, err := os.Open(path)
	if err != nil {
		fmt.Fprintf(stderr, "domowik: cannot open %q: %v\n", path, err)
		return 1
	}
	defer src.Close()
	res, ierr := store.Import(assets.ImportInput{
		Reader:            src,
		MediaType:         *mediaType,
		Purpose:           *purpose,
		Requirement:       *requirement,
		DisplayName:       *displayName,
		Author:            *author,
		SourceDescription: *sourceDesc,
	})
	if ierr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", ierr)
		return 1
	}
	fmt.Fprintf(stdout, "digest: %s\n", res.Descriptor.Digest)
	fmt.Fprintf(stdout, "byteLength: %d\n", res.Descriptor.ByteLength)
	fmt.Fprintf(stdout, "deduplicated: %t\n", res.Deduplicated)
	return 0
}

func assetList(store *assets.Store, _ []string, stdout, stderr io.Writer) int {
	list, totals, err := store.List()
	if err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return 1
	}
	for _, a := range list {
		fmt.Fprintf(stdout, "%s\t%d\t%s\t%s\t%s\tpinned=%t\tblob=%t\n",
			a.Digest, a.ByteLength, a.MediaType, a.Purpose, a.Requirement, a.Pinned, a.BlobPresent)
	}
	fmt.Fprintf(stdout, "cache-blob-count: %d\n", totals.CacheBlobCount)
	fmt.Fprintf(stdout, "cache-bytes-physical: %d\n", totals.CacheBlobBytes)
	fmt.Fprintf(stdout, "remote-bytes-logical: %d (%s)\n", totals.RemoteBytes, totals.RemoteNote)
	return 0
}

func assetVerify(store *assets.Store, args []string, stdout, stderr io.Writer) int {
	if len(args) != 1 {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	hex, err := requireDigest(args[0])
	if err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return 2
	}
	n, verr := store.Verify(hex)
	if verr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", verr)
		return 1
	}
	fmt.Fprintf(stdout, "verified: sha256:%s\nbyteLength: %d\n", hex, n)
	return 0
}

func assetRemove(store *assets.Store, args []string, stdout, stderr io.Writer) int {
	if len(args) != 1 {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	hex, err := requireDigest(args[0])
	if err != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", err)
		return 2
	}
	if rerr := store.Remove(hex); rerr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", rerr)
		return 1
	}
	fmt.Fprintf(stdout, "removed: sha256:%s\n", hex)
	return 0
}

func assetPin(store *assets.Store, verb string, args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("asset "+verb, flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	owner := fs.String("owner", "", "owning identity for the pin")
	rootKind := fs.String("root-kind", "manual", "snapshot|operation-frontier|root-table|manual")
	if err := fs.Parse(args); err != nil {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	if fs.NArg() != 1 || *owner == "" {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	hex, derr := requireDigest(fs.Arg(0))
	if derr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", derr)
		return 2
	}
	if verb == "pin" {
		if perr := store.AddPin(hex, *owner, *rootKind, ""); perr != nil {
			fmt.Fprintf(stderr, "domowik: %v\n", perr)
			return 1
		}
		fmt.Fprintf(stdout, "pinned: sha256:%s owner=%s rootKind=%s\n", hex, *owner, *rootKind)
		return 0
	}
	if rerr := store.ReleasePin(hex, *owner); rerr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", rerr)
		return 1
	}
	fmt.Fprintf(stdout, "unpinned: sha256:%s owner=%s\n", hex, *owner)
	return 0
}

func assetGC(store *assets.Store, args []string, stdout, stderr io.Writer) int {
	fs := flag.NewFlagSet("asset gc", flag.ContinueOnError)
	fs.SetOutput(io.Discard)
	repair := fs.Bool("repair", false, "rebuild the pin store to a clean validated state")
	if err := fs.Parse(args); err != nil {
		fmt.Fprint(stderr, assetUsage)
		return 2
	}
	if *repair {
		res, rerr := store.Repair()
		if rerr != nil {
			fmt.Fprintf(stderr, "domowik: %v\n", rerr)
			return 1
		}
		fmt.Fprintf(stdout, "repair: anomalous=%t kept-records=%d conservative-pins=%d resumed=%t\n",
			res.WasAnomalous, res.KeptRecords, res.ConservativePins, res.Resumed)
		return 0
	}
	res, cerr := store.Collect()
	if cerr != nil {
		fmt.Fprintf(stderr, "domowik: %v\n", cerr)
		return 1
	}
	if res.FailedClosed {
		fmt.Fprintf(stderr, "domowik: garbage collection failed closed (%s): %s\nnothing was collected; run 'domowik asset gc --repair' after review\n",
			res.AnomalyKind, res.AnomalyText)
		return 1
	}
	fmt.Fprintf(stdout, "collected: %d\nprotected-pins: %d\n", len(res.Collected), res.Protected)
	for _, d := range res.Collected {
		fmt.Fprintf(stdout, "collected-digest: %s\n", d)
	}
	return 0
}

func requireDigest(arg string) (string, error) {
	const prefix = "sha256:"
	if !strings.HasPrefix(arg, prefix) {
		return "", fmt.Errorf("digest argument must carry the sha256: prefix")
	}
	hex := strings.TrimPrefix(arg, prefix)
	if err := persist.ValidateDigestID(arg); err != nil {
		return "", err
	}
	return hex, nil
}
