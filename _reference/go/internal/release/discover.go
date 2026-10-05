package release

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strings"
)

// Module is a resolved Go module from the live module graph.
type Module struct {
	// Path and Version are the original (pre-replacement) module identity.
	Path    string
	Version string
	Main    bool
	// ReplacedBy is non-empty when the module is served by a replacement.
	ReplacedBy string
	// ReplacedVersion is the effective version supplied by that replacement.
	ReplacedVersion string
}

// EffectivePath returns the registry key for the module.
func (m Module) EffectivePath() string {
	if m.ReplacedBy != "" {
		return m.ReplacedBy
	}
	return m.Path
}

// EffectiveVersion returns the version actually compiled into artifacts.
func (m Module) EffectiveVersion() string {
	if m.ReplacedBy != "" {
		return m.ReplacedVersion
	}
	return m.Version
}

// goListModule mirrors the subset of `go list -m -json all` output used here.
type goListModule struct {
	Path    string
	Version string
	Main    bool
	Replace *goListModule
}

// ListModules discovers the resolved module set dynamically via
// `go list -m -json all` run in dir. The result is sorted by effective path,
// and duplicate effective paths fail closed instead of silently collapsing
// component identity. Module replacements are surfaced explicitly so the
// registry can review the effective source of every path.
func ListModules(ctx context.Context, dir string) ([]Module, error) {
	out, err := runGo(ctx, dir, nil, "list", "-m", "-json", "all")
	if err != nil {
		return nil, fmt.Errorf("go list -m -json all: %w", err)
	}
	var modules []Module
	seen := map[string]bool{}
	dec := json.NewDecoder(bytes.NewReader(out))
	for {
		var raw goListModule
		if err := dec.Decode(&raw); err != nil {
			if err == io.EOF {
				break
			}
			return nil, fmt.Errorf("parse `go list -m -json all` output: %w", err)
		}
		m := Module{Path: raw.Path, Version: raw.Version, Main: raw.Main}
		if raw.Replace != nil {
			m.ReplacedBy = raw.Replace.Path
			m.ReplacedVersion = raw.Replace.Version
		}
		key := m.EffectivePath()
		if seen[key] {
			return nil, fmt.Errorf("module %q appears more than once in the resolved graph; refusing to lose component identity", key)
		}
		seen[key] = true
		modules = append(modules, m)
	}
	sort.Slice(modules, func(i, j int) bool { return modules[i].EffectivePath() < modules[j].EffectivePath() })
	return modules, nil
}

// ListModuleLines returns the plain `go list -m all` lines for cross-checking
// the JSON view against the human-consumed module list.
func ListModuleLines(ctx context.Context, dir string) ([]string, error) {
	out, err := runGo(ctx, dir, nil, "list", "-m", "all")
	if err != nil {
		return nil, fmt.Errorf("go list -m all: %w", err)
	}
	var lines []string
	for _, l := range strings.Split(strings.TrimSpace(string(out)), "\n") {
		if l != "" {
			lines = append(lines, l)
		}
	}
	return lines, nil
}

// CrossCheckModules fails if the JSON and line forms of the module graph
// disagree. Replacement lines reproduce the `orig v => eff effV` form that
// `go list -m all` prints, preserving both the original and effective
// identities.
func CrossCheckModules(modules []Module, lines []string) error {
	jsonLines := make([]string, len(modules))
	for i, m := range modules {
		line := m.Path
		if m.Version != "" && !m.Main {
			line += " " + m.Version
		}
		if m.ReplacedBy != "" {
			line += " => " + m.ReplacedBy
			if m.ReplacedVersion != "" {
				line += " " + m.ReplacedVersion
			}
		}
		jsonLines[i] = line
	}
	sort.Strings(jsonLines)
	live := append([]string(nil), lines...)
	sort.Strings(live)
	if strings.Join(jsonLines, "\n") != strings.Join(live, "\n") {
		return fmt.Errorf("module-set cross-check failed: `go list -m -json all` and `go list -m all` disagree")
	}
	return nil
}

// goListPackage mirrors the subset of `go list -json` package output used to
// inspect per-target embedded files.
type goListPackage struct {
	ImportPath    string
	Dir           string
	EmbedPatterns []string
	EmbedFiles    []string
}

// EmbeddedFile is one bundled file discovered through Go build metadata.
type EmbeddedFile struct {
	// Path is the file path relative to the repository root, slash-separated.
	Path string
	// ImportPath is the package whose source declares the embed.
	ImportPath string
	// Patterns lists the go:embed patterns that pulled the file in.
	Patterns []string
}

// CollectEmbeds inspects embedded files per target by running
// `go list -json` over the given package patterns with the target's
// GOOS/GOARCH/CGO environment so build-tag-dependent embeds are seen as the
// target sees them. Per the go list metadata contract, EmbedFiles are
// relative to each package's Dir, so they are joined with Dir before
// validation. Packages from external modules (Dir outside the repository
// root) that declare embedded files are rejected fail-closed below:
// external bundled content ships inside the binary but is invisible to this
// repository's file-level license review, so it requires review first.
// Every in-root path is checked with SafeEmbedPath; results are deduplicated
// and sorted by path.
func CollectEmbeds(ctx context.Context, root string, targetEnv []string, patterns ...string) ([]EmbeddedFile, error) {
	if len(patterns) == 0 {
		patterns = []string{"./..."}
	}
	args := append([]string{"list", "-json", "-deps"}, patterns...)
	out, err := runGo(ctx, root, targetEnv, args...)
	if err != nil {
		return nil, fmt.Errorf("go list -json %v %v: %w", patterns, targetEnv, err)
	}
	pkgs := map[string]*goListPackage{}
	dec := json.NewDecoder(bytes.NewReader(out))
	for {
		var p goListPackage
		if err := dec.Decode(&p); err != nil {
			if err == io.EOF {
				break
			}
			return nil, fmt.Errorf("parse `go list -json` output: %w", err)
		}
		pkgs[p.ImportPath] = &p
	}
	var files []EmbeddedFile
	seen := map[string]bool{}
	for _, imp := range sortedKeys(pkgs) {
		p := pkgs[imp]
		if len(p.EmbedFiles) == 0 {
			continue
		}
		if p.Dir == "" {
			return nil, fmt.Errorf("package %s reports embed files but no Dir", imp)
		}
		patSet := map[string]bool{}
		for _, pat := range p.EmbedPatterns {
			patSet[pat] = true
		}
		// External-module embedded content is distributed inside the binary
		// but is invisible to this repository's file-level license review;
		// fail closed instead of silently filtering it out.
		inRoot, err := dirInRoot(root, p.Dir)
		if err != nil {
			return nil, fmt.Errorf("package %s: %w", imp, err)
		}
		if !inRoot && len(p.EmbedFiles) > 0 {
			return nil, fmt.Errorf("package %s (module outside this repository) declares embedded files %v; external bundled content requires review before distribution", imp, p.EmbedFiles)
		}
		if !inRoot {
			continue
		}
		relDir, err := relUnderRoot(root, p.Dir)
		if err != nil {
			return nil, fmt.Errorf("package %s: %w", imp, err)
		}
		for _, f := range p.EmbedFiles {
			fullRel := filepath.Join(filepath.FromSlash(relDir), filepath.FromSlash(f))
			rel, err := SafeEmbedPath(root, fullRel)
			if err != nil {
				return nil, fmt.Errorf("embed in %s: %w", imp, err)
			}
			if seen[rel] {
				continue
			}
			seen[rel] = true
			files = append(files, EmbeddedFile{
				Path:       rel,
				ImportPath: imp,
				Patterns:   sortedKeys(patSet),
			})
		}
	}
	sort.Slice(files, func(i, j int) bool { return files[i].Path < files[j].Path })
	return files, nil
}

// SafeEmbedPath validates that an embed path stays safely inside the
// repository. Policy: the final entry itself must not be a symlink (checked
// with Lstat on the unresolved path before any resolution); the resolved
// location, including through any parent directories, must stay under the
// symlink-resolved root; and the target must be a regular file.
func SafeEmbedPath(root, p string) (string, error) {
	if p == "" {
		return "", fmt.Errorf("empty embed path")
	}
	if filepath.IsAbs(p) {
		return "", fmt.Errorf("absolute embed path %q", p)
	}
	clean := filepath.Clean(p)
	if err := rejectUpwardElements(p); err != nil {
		return "", err
	}
	for _, elem := range strings.Split(clean, string(filepath.Separator)) {
		switch elem {
		case ".":
			return "", fmt.Errorf("redundant element in embed path: %q", p)
		}
	}
	full := filepath.Join(root, clean)
	if fi, err := os.Lstat(full); err != nil {
		return "", fmt.Errorf("stat embed path: %w", err)
	} else if fi.Mode()&os.ModeSymlink != 0 {
		return "", fmt.Errorf("embed path %q is a symlink entry; symlinked embedded files are rejected", p)
	}
	realRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return "", fmt.Errorf("resolve root: %w", err)
	}
	realFull, err := filepath.EvalSymlinks(full)
	if err != nil {
		return "", fmt.Errorf("resolve embed path: %w", err)
	}
	if realFull != realRoot && !strings.HasPrefix(realFull, realRoot+string(filepath.Separator)) {
		return "", fmt.Errorf("embed path %q resolves outside the repository root", p)
	}
	fi, err := os.Stat(realFull)
	if err != nil {
		return "", fmt.Errorf("stat embed path: %w", err)
	}
	if !fi.Mode().IsRegular() {
		return "", fmt.Errorf("embed path %q is not a regular file", p)
	}
	rel, err := filepath.Rel(realRoot, realFull)
	if err != nil {
		return "", fmt.Errorf("relativize embed path: %w", err)
	}
	return filepath.ToSlash(rel), nil
}

// relUnderRoot converts an absolute path reported by Go tooling (such as a
// package Dir) into a repository-relative slash path, failing closed if it is
// absolute-but-outside, escapes upward, or resolves through symlinks outside
// the symlink-resolved root.
func relUnderRoot(root, abs string) (string, error) {
	if !filepath.IsAbs(abs) {
		return "", fmt.Errorf("expected an absolute path, got %q", abs)
	}
	realRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return "", fmt.Errorf("resolve root: %w", err)
	}
	realAbs, err := filepath.EvalSymlinks(abs)
	if err != nil {
		return "", fmt.Errorf("resolve %q: %w", abs, err)
	}
	if realAbs != realRoot && !strings.HasPrefix(realAbs, realRoot+string(filepath.Separator)) {
		return "", fmt.Errorf("path %q resolves outside the repository root", abs)
	}
	rel, err := filepath.Rel(realRoot, realAbs)
	if err != nil {
		return "", fmt.Errorf("relativize %q: %w", abs, err)
	}
	if rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return "", fmt.Errorf("path %q escapes the repository root", abs)
	}
	return filepath.ToSlash(rel), nil
}

// discoveryEnv returns the exact environment applied to every Go invocation
// made during discovery and generation. Overrides replace any inherited value
// rather than being shadowed by it, making the pipeline offline and
// read-only by construction:
//
//   - GOPROXY=off fails closed on any attempted module fetch;
//   - GOFLAGS=-mod=readonly forbids go.mod/go.sum mutation;
//   - GOWORK=off isolates the run from surrounding workspace state;
//   - GOTOOLCHAIN=local pins the already-installed toolchain.
func discoveryEnv(extra []string) []string {
	overrides := map[string]string{
		"GOTOOLCHAIN": "local",
		"GOPROXY":     "off",
		"GOFLAGS":     "-mod=readonly",
		"GOWORK":      "off",
	}
	for _, kv := range extra {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			continue
		}
		overrides[k] = v
	}
	env := make([]string, 0, len(os.Environ())+len(overrides)+len(extra))
	for _, kv := range os.Environ() {
		k, _, _ := strings.Cut(kv, "=")
		if _, shadowed := overrides[k]; shadowed {
			continue
		}
		env = append(env, kv)
	}
	for _, k := range sortedKeys(overrides) {
		env = append(env, k+"="+overrides[k])
	}
	return env
}

// BuildGoEnv exposes the composed environment for tests and diagnostics.
func BuildGoEnv(extra []string) []string { return discoveryEnv(extra) }

func runGo(ctx context.Context, dir string, extraEnv []string, args ...string) ([]byte, error) {
	cmd := exec.CommandContext(ctx, "go", args...)
	cmd.Dir = dir
	cmd.Env = discoveryEnv(extraEnv)
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("%v: %v\n%s", cmd.String(), err, stderr.String())
	}
	return out, nil
}

// dirInRoot reports whether an absolute tool-reported directory lies within
// the symlink-resolved repository root.
func dirInRoot(root, abs string) (bool, error) {
	realRoot, err := filepath.EvalSymlinks(root)
	if err != nil {
		return false, fmt.Errorf("resolve root: %w", err)
	}
	realAbs, err := filepath.EvalSymlinks(abs)
	if err != nil {
		return false, nil // missing/unresolvable directory: treat as external
	}
	return realAbs == realRoot || strings.HasPrefix(realAbs, realRoot+string(filepath.Separator)), nil
}

func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
