package release

import (
	"debug/buildinfo"
	"fmt"
	"runtime/debug"
	"sort"
	"strings"
)

// ComponentsFromBuildInfo extracts the authoritative component set of a built
// binary: its main module, every recorded dependency module, and the Go
// toolchain that produced it. Replacements are resolved to the effective
// path/version so identity is never lost. Conflicting duplicate identities
// fail closed. The returned list is sorted by key.
func ComponentsFromBuildInfo(bi *debug.BuildInfo) ([]Component, error) {
	if bi == nil || bi.Main.Path == "" {
		return nil, fmt.Errorf("build info has no main module")
	}
	// Policy: a replaced main module is unsupported and rejected explicitly.
	// Dependency replacements resolve to their effective path/version; the
	// main module defines the artifact's first-party identity and must match
	// the reviewed registry row as-is.
	if bi.Main.Replace != nil {
		return nil, fmt.Errorf("main module %q carries a replacement to %q; replaced main modules are unsupported", bi.Main.Path, bi.Main.Replace.Path)
	}
	var comps []Component
	comps = append(comps, Component{Key: bi.Main.Path, Kind: KindModule, Version: bi.Main.Version})
	for _, d := range bi.Deps {
		key, ver := d.Path, d.Version
		if d.Replace != nil {
			key = d.Replace.Path
			ver = d.Replace.Version
		}
		comps = append(comps, Component{Key: key, Kind: KindModule, Version: ver})
	}
	goVersion := strings.TrimPrefix(bi.GoVersion, "go")
	if goVersion == "" {
		return nil, fmt.Errorf("build info has no Go version")
	}
	comps = append(comps, Component{Key: ToolchainKey, Kind: KindToolchain, Version: "go" + goVersion})
	out, err := dedupeComponents(comps)
	if err != nil {
		return nil, err
	}
	return out, nil
}

// ReadBuildInfo reads build metadata from a compiled binary.
func ReadBuildInfo(path string) (*debug.BuildInfo, error) {
	bi, err := buildinfo.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read build info from %s: %w", path, err)
	}
	return bi, nil
}

// dedupeComponents sorts by key and fails closed when one key appears with
// conflicting versions, which would make component identity ambiguous.
func dedupeComponents(comps []Component) ([]Component, error) {
	seen := map[string]Component{}
	out := make([]Component, 0, len(comps))
	for _, c := range comps {
		prev, ok := seen[c.Key]
		if !ok {
			seen[c.Key] = c
			out = append(out, c)
			continue
		}
		if prev.Kind != c.Kind || prev.Version != c.Version {
			return nil, fmt.Errorf("conflicting build-info identities for %q (%s@%s vs %s@%s)",
				c.Key, prev.Kind, prev.Version, c.Kind, c.Version)
		}
	}
	sortComponents(out)
	return out, nil
}

func sortComponents(comps []Component) {
	sort.SliceStable(comps, func(i, j int) bool { return comps[i].Key < comps[j].Key })
}
