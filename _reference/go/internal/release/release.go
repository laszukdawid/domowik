// Package release implements the release inventory pipeline for Domowik:
// dynamic component discovery, fail-closed license auditing against the
// reviewed registry, and deterministic SPDX 2.3 SBOM and NOTICE generation.
//
// Declarative inputs are split by role: the distribution manifest lives
// under release/; reviewed policy and provenance inputs (license registry,
// toolchain record, pinned tools, verbatim notice texts) live under
// docs/third-party/. Discovery is performed at generation time from the
// live Go metadata, never assumed. Every generation and verification step
// fails closed on missing, unknown, stale, or incompatible components.
package release

import (
	"fmt"
	"sort"
)

// Finding is a single fail-closed audit result. A non-empty findings list
// aborts generation and verification.
type Finding struct {
	// Component is the discovered or registered component key.
	Component string
	// Kind classifies the component (module, toolchain, bundled).
	Kind string
	// Rule identifies the violated audit rule.
	Rule string
	// Detail explains the violation.
	Detail string
}

func (f Finding) Error() string {
	return fmt.Sprintf("%s %q: %s: %s", f.Kind, f.Component, f.Rule, f.Detail)
}

// SortFindings orders findings deterministically for stable reports.
func SortFindings(fs []Finding) {
	sort.Slice(fs, func(i, j int) bool {
		if fs[i].Kind != fs[j].Kind {
			return fs[i].Kind < fs[j].Kind
		}
		if fs[i].Component != fs[j].Component {
			return fs[i].Component < fs[j].Component
		}
		return fs[i].Rule < fs[j].Rule
	})
}

// Component is a dynamically discovered inventory item.
type Component struct {
	// Key is the registry lookup identifier (module path or reserved key).
	Key string
	// Kind is the component class matched against registry entries.
	Kind string
	// Version is the exact resolved version string, if any.
	Version string
}

const (
	// KindModule is a resolved Go module (main or dependency).
	KindModule = "module"
	// KindToolchain is the Go compiler/runtime/stdlib set.
	KindToolchain = "toolchain"
	// KindBundled is a non-module file distributed with an artifact.
	KindBundled = "bundled"

	// ToolchainKey is the reserved registry key for the Go toolchain.
	ToolchainKey = "golang.org/toolchain"
)
