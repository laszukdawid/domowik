// Command domowik-sbom is the first-party release inventory tool. It is a
// build-time utility and is not distributed with Domowik artifacts; see
// docs/RELEASE-INVENTORY.md for its provenance record and pinning policy.
//
// Usage:
//
//	domowik-sbom generate [--allow-dirty]
//	domowik-sbom verify
//	domowik-sbom plan
//	domowik-sbom check-manifest
//
// Exit codes: 0 success, 1 verification or generation failure, 2 usage error.
package main

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"
	"time"

	"domowik.dev/domowik/internal/release"
)

func main() {
	os.Exit(run(os.Args[1:]))
}

func run(args []string) int {
	if len(args) == 0 {
		usageErr("no command given")
	}
	root, err := repoRoot()
	if err != nil {
		fmt.Fprintf(os.Stderr, "domowik-sbom: %v\n", err)
		return 1
	}
	g := &release.Generator{Root: root}
	ctx := context.Background()

	switch args[0] {
	case "generate":
		allowDirty := false
		for _, a := range args[1:] {
			if a != "--allow-dirty" {
				usageErr("generate: unknown argument %q", a)
			}
			allowDirty = true
		}
		inv, err := g.Build(ctx, release.BuildOptions{AllowDirty: allowDirty})
		if err != nil {
			return fail(err)
		}
		if err := g.WriteAll(inv); err != nil {
			return fail(err)
		}
		for _, p := range sortedOutputs(inv) {
			fmt.Printf("wrote %s\n", p)
		}
		return 0
	case "verify":
		res, err := g.Verify(ctx, release.EnvGetenv, func() (int64, error) { return gitHeadTime(root) })
		if err != nil {
			return fail(err)
		}
		for _, p := range res.CheckedPaths {
			fmt.Printf("ok %s\n", p)
		}
		return 0
	case "plan":
		return plan(ctx, g)
	case "check-manifest":
		return checkConsumptionManifest(ctx, g)
	default:
		usageErr("unknown command %q", args[0])
	}
	return 2
}

// plan reports the same full dynamic inventory Build would produce,
// including per-target embedded files and every audit finding, so a
// successful plan implies generate can succeed.
func plan(ctx context.Context, g *release.Generator) int {
	inv, err := g.Plan(ctx)
	if err != nil {
		return fail(err)
	}
	fmt.Printf("epoch: %s (source: %s)\n", inv.Epoch.UTC().Format(time.RFC3339), inv.EpochSource)
	fmt.Printf("discovered %d component(s):\n", len(inv.Components))
	for _, c := range inv.Components {
		fmt.Printf("  component %s (%s) %s\n", c.Key, c.Kind, c.Version)
	}
	ids := make([]string, 0, len(inv.Embeds))
	for id := range inv.Embeds {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		embeds := inv.Embeds[id]
		fmt.Printf("artifact %s: %d embedded file(s)\n", id, len(embeds))
		for _, ef := range embeds {
			fmt.Printf("  embed %s (via %s in %s)\n", ef.Path, strings.Join(ef.Patterns, ","), ef.ImportPath)
		}
	}
	for _, p := range sortedOutputs(inv) {
		fmt.Printf("would write %s (%d bytes)\n", p, len(inv.Outputs[p]))
	}
	return 0
}

func sortedOutputs(inv *release.Inventory) []string {
	out := make([]string, 0, len(inv.Outputs))
	for p := range inv.Outputs {
		out = append(out, p)
	}
	for i := 1; i < len(out); i++ {
		for j := i; j > 0 && out[j] < out[j-1]; j-- {
			out[j], out[j-1] = out[j-1], out[j]
		}
	}
	return out
}

func fail(err error) int {
	fmt.Fprintf(os.Stderr, "domowik-sbom: %v\n", err)
	return 1
}

func usageErr(format string, args ...any) {
	fmt.Fprintf(os.Stderr, "domowik-sbom: "+format+"\n", args...)
	fmt.Fprint(os.Stderr, "usage: domowik-sbom {generate [--allow-dirty]|verify|check-manifest|plan}\n")
	os.Exit(2)
}

// checkConsumptionManifest re-validates the generated consumption manifest
// against the files on disk using the exact same hashing contract as
// generation: SPDX documents are hashed with their creation timestamp
// stripped, everything else as raw bytes.
func checkConsumptionManifest(ctx context.Context, g *release.Generator) int {
	res, err := g.CheckConsumptionManifest(ctx)
	if err != nil {
		return fail(err)
	}
	for _, path := range res {
		fmt.Printf("ok %s\n", path)
	}
	return 0
}
